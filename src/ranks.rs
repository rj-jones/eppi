//! Looking up ranks in the background: the UI says which codes it shows,
//! and a worker fetches the ones that aren't fresh (see [`fresh_for`]), a
//! batch at a time and at most one request a second, backing off when
//! Slippi says to slow down. The rows on screen come first: when you
//! scroll past rows faster than they're looked up, the oldest asks are
//! dropped. It also fetches the ranked seasons' calendar (with how you
//! finished each one), and for the rank filter, opponents' past seasons.

use crate::library::db::RankRecord;
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::time::{Duration, Instant};

/// How long a lookup counts as fresh, for you and anyone you've played in
/// the last month.
pub const FRESH_FOR: i64 = 24 * 3_600;
/// For someone you haven't played in a month: their rank is shown less and
/// matters less, so it's looked up once a week.
const FRESH_FOR_OLD_OPPONENT: i64 = 7 * 24 * 3_600;
/// For a code no account has: it stays that way, so once a month.
const FRESH_FOR_NO_ACCOUNT: i64 = 30 * 24 * 3_600;
/// "Recently played" for [`fresh_for`].
const RECENT: i64 = 30 * 24 * 3_600;
/// At most this many codes wait in the on-screen queue; older ones are
/// dropped (and asked for again if they're on screen again).
const MAX_QUEUED: usize = 100;
/// Pause between requests.
const PACE: Duration = Duration::from_millis(1_100);

enum Request {
    Fetch(Vec<String>),
    Seasons(String),
    /// Every season of these players (for the rank filter).
    Histories(Vec<String>),
    Shutdown,
}

/// A player's current rank and their seasons (`None`: no account has the
/// code).
pub type PlayerSeasons = (RankRecord, Option<Vec<crate::model::Season>>);

/// What the worker found out.
pub enum Reply {
    Ranks(Result<Vec<RankRecord>, String>),
    /// Codes dropped from the queue before they were looked up.
    Dropped(Vec<String>),
    /// The seasons `code` played in (all of them since their account was
    /// made), newest first.
    Seasons {
        code: String,
        result: Result<Vec<crate::model::Season>, String>,
    },
    /// Players' seasons (`None`: no account has the code), each with their
    /// current rank, which the same request brings.
    Histories(Result<Vec<PlayerSeasons>, String>),
}

pub struct RankService {
    requests: Sender<Request>,
    pub results: Receiver<Reply>,
    /// Codes asked for and not answered yet.
    pending: HashSet<String>,
    /// Codes whose seasons were asked for and not answered yet.
    pending_histories: HashSet<String>,
    /// When each code was last tried this run (failed lookups wait too).
    tried: HashMap<String, Instant>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl RankService {
    pub fn start(repaint: impl Fn() + Send + 'static) -> Self {
        let (request_tx, request_rx) = mpsc::channel();
        let (result_tx, result_rx) = mpsc::channel();
        let thread = std::thread::Builder::new()
            .name("eppi-ranks".into())
            .spawn(move || run(request_rx, result_tx, repaint))
            .expect("start the rank thread");
        Self {
            requests: request_tx,
            results: result_rx,
            pending: HashSet::new(),
            pending_histories: HashSet::new(),
            tried: HashMap::new(),
            thread: Some(thread),
        }
    }

    /// Asks for `codes` that aren't fresh in `known` (or all of them with
    /// `force`). `last_played` (when you last played each code) sets how
    /// long a lookup stays fresh.
    pub fn want<'a>(
        &mut self,
        codes: impl IntoIterator<Item = &'a str>,
        known: &HashMap<String, RankRecord>,
        last_played: &HashMap<String, i64>,
        now: i64,
        force: bool,
    ) {
        let mut batch = Vec::new();
        for code in codes {
            if code.is_empty() || !code.contains('#') || self.pending.contains(code) {
                continue;
            }
            if !force {
                if known.get(code).is_some_and(|r| {
                    now - r.fetched_at < fresh_for(r, last_played.get(code).copied(), now)
                }) {
                    continue;
                }
                if self
                    .tried
                    .get(code)
                    .is_some_and(|t| t.elapsed() < Duration::from_secs(600))
                {
                    continue;
                }
            }
            self.pending.insert(code.to_owned());
            self.tried.insert(code.to_owned(), Instant::now());
            batch.push(code.to_owned());
        }
        if !batch.is_empty() {
            let _ = self.requests.send(Request::Fetch(batch));
        }
    }

    /// Asks for the seasons' calendar and how `code` did in each.
    pub fn want_seasons(&mut self, code: &str) {
        let _ = self.requests.send(Request::Seasons(code.to_owned()));
    }

    pub fn pending_count(&self) -> usize {
        self.pending.len() + self.pending_histories.len()
    }

    /// Asks for every season of `codes` (those not asked for already).
    pub fn want_histories<'a>(&mut self, codes: impl IntoIterator<Item = &'a str>) {
        let batch: Vec<String> = codes
            .into_iter()
            .filter(|c| c.contains('#') && self.pending_histories.insert((*c).to_owned()))
            .map(str::to_owned)
            .collect();
        if !batch.is_empty() {
            let _ = self.requests.send(Request::Histories(batch));
        }
    }

    pub fn histories_answered(&mut self, codes: impl IntoIterator<Item = String>) {
        for code in codes {
            self.pending_histories.remove(&code);
        }
    }

    /// After a failed history request, those codes can be asked for again
    /// (in a while: the app asks every frame, so it waits on `tried`).
    pub fn histories_failed(&mut self) {
        for code in self.pending_histories.drain() {
            self.tried.insert(format!("history:{code}"), Instant::now());
        }
    }

    pub fn history_recently_failed(&self, code: &str) -> bool {
        self.tried
            .get(&format!("history:{code}"))
            .is_some_and(|t| t.elapsed() < Duration::from_secs(600))
    }

    pub fn answered(&mut self, records: &[RankRecord]) {
        for record in records {
            self.pending.remove(&record.code);
        }
    }

    /// The worker dropped these before looking them up: they can be asked
    /// for again as soon as they're on screen.
    pub fn dropped(&mut self, codes: &[String]) {
        for code in codes {
            self.pending.remove(code);
            self.tried.remove(code);
        }
    }

    /// After a failed request, nothing is pending any more (it's retried
    /// after the wait in [`Self::want`]).
    pub fn failed(&mut self) {
        self.pending.clear();
    }
}

impl Drop for RankService {
    fn drop(&mut self) {
        let _ = self.requests.send(Request::Shutdown);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn run(requests: Receiver<Request>, results: Sender<Reply>, repaint: impl Fn()) {
    let mut queue: VecDeque<String> = VecDeque::new();
    let mut histories: VecDeque<String> = VecDeque::new();
    let mut seasons: Option<String> = None;
    let mut next_request = Instant::now();
    loop {
        let wait = if queue.is_empty() && histories.is_empty() && seasons.is_none() {
            Duration::from_secs(3_600)
        } else {
            next_request.saturating_duration_since(Instant::now())
        };
        match requests.recv_timeout(wait) {
            Ok(Request::Fetch(codes)) => {
                // The newest ask goes first: it's what's on screen now.
                for code in codes.into_iter().rev() {
                    queue.retain(|c| *c != code);
                    queue.push_front(code);
                }
                if queue.len() > MAX_QUEUED {
                    let dropped: Vec<String> = queue.drain(MAX_QUEUED..).collect();
                    let _ = results.send(Reply::Dropped(dropped));
                }
                continue;
            }
            Ok(Request::Seasons(code)) => {
                seasons = Some(code);
                continue;
            }
            Ok(Request::Histories(codes)) => {
                histories.extend(codes);
                continue;
            }
            Ok(Request::Shutdown) | Err(RecvTimeoutError::Disconnected) => return,
            Err(RecvTimeoutError::Timeout) => {}
        }
        if Instant::now() < next_request {
            continue;
        }
        if let Some(code) = seasons.take() {
            next_request = Instant::now() + PACE;
            let result = eppi_rank::fetch_seasons(&code)
                .map(|list| list.iter().map(season).collect())
                .map_err(|e| e.to_string());
            let _ = results.send(Reply::Seasons { code, result });
            repaint();
            continue;
        }
        if queue.is_empty() {
            // Ranks on screen first; histories when nothing else waits.
            if !histories.is_empty() {
                let take = eppi_rank::BATCH_SIZE.min(histories.len());
                let batch: Vec<String> = histories.drain(..take).collect();
                let refs: Vec<&str> = batch.iter().map(String::as_str).collect();
                let now = crate::time::now();
                next_request = Instant::now() + PACE;
                let reply = match eppi_rank::fetch_histories(&refs) {
                    Ok(found) => Ok(found
                        .into_iter()
                        .zip(&batch)
                        .map(|((lookup, seasons), asked)| {
                            let mut record = record(&lookup, now);
                            // Keyed by the code as the app asked for it.
                            record.code = crate::model::normalize_code(asked);
                            (
                                record,
                                seasons.map(|list| list.iter().map(season).collect()),
                            )
                        })
                        .collect()),
                    Err(error) => {
                        if let Some(wait) = error.retry_after() {
                            next_request = Instant::now() + wait;
                        } else if error.is_rate_limited() {
                            next_request = Instant::now() + Duration::from_secs(60);
                        }
                        histories.clear();
                        Err(error.to_string())
                    }
                };
                let _ = results.send(Reply::Histories(reply));
                repaint();
            }
            continue;
        }
        let take = eppi_rank::BATCH_SIZE.min(queue.len());
        let batch: Vec<String> = queue.drain(..take).collect();
        let refs: Vec<&str> = batch.iter().map(String::as_str).collect();
        let now = crate::time::now();
        next_request = Instant::now() + PACE;
        match eppi_rank::fetch(&refs) {
            Ok(lookups) => {
                let records = lookups.iter().map(|l| record(l, now)).collect();
                let _ = results.send(Reply::Ranks(Ok(records)));
            }
            Err(error) => {
                if let Some(wait) = error.retry_after() {
                    next_request = Instant::now() + wait;
                } else if error.is_rate_limited() {
                    next_request = Instant::now() + Duration::from_secs(60);
                }
                let _ = results.send(Reply::Ranks(Err(error.to_string())));
            }
        }
        repaint();
    }
}

/// How long `record` stays fresh: a day for anyone played in the last month
/// (or with no games: you, or a player searched for), a week for older
/// opponents, a month for a code no account has.
pub fn fresh_for(record: &RankRecord, last_played: Option<i64>, now: i64) -> i64 {
    if !record.found {
        FRESH_FOR_NO_ACCOUNT
    } else if last_played.is_some_and(|at| now - at > RECENT) {
        FRESH_FOR_OLD_OPPONENT
    } else {
        FRESH_FOR
    }
}

fn season(s: &eppi_rank::Season) -> crate::model::Season {
    crate::model::Season {
        id: s.id.clone(),
        name: s.name.clone(),
        start: crate::time::parse_start_at(&s.started_at).unwrap_or(0),
        end: s.ended_at.as_deref().and_then(crate::time::parse_start_at),
        rating: s.rating,
        rank: s.rank.name().to_owned(),
        sets: s.sets_played,
        wins: s.wins,
        losses: s.losses,
    }
}

fn record(lookup: &eppi_rank::Lookup, now: i64) -> RankRecord {
    match lookup {
        eppi_rank::Lookup::Found(profile) => {
            let extra = serde_json::json!({
                "wins": profile.wins,
                "losses": profile.losses,
                "continent": profile.continent,
                "season": profile.season,
                "status": profile.status,
                "characters": profile
                    .characters
                    .iter()
                    .map(|c| serde_json::json!({"character": c.character, "games": c.game_count}))
                    .collect::<Vec<_>>(),
            });
            RankRecord {
                code: crate::model::normalize_code(&profile.connect_code),
                fetched_at: now,
                found: true,
                display_name: Some(profile.display_name.clone()),
                rating: Some(profile.rating),
                rank: Some(profile.rank.name().to_owned()),
                sets: Some(profile.sets_played as i64),
                global_placement: profile.daily_global_placement.map(i64::from),
                regional_placement: profile.daily_regional_placement.map(i64::from),
                extra: Some(extra.to_string()),
            }
        }
        eppi_rank::Lookup::NoRankedProfile { code, display_name } => RankRecord {
            code: crate::model::normalize_code(code),
            fetched_at: now,
            found: true,
            display_name: Some(display_name.clone()),
            rating: None,
            rank: None,
            sets: None,
            global_placement: None,
            regional_placement: None,
            extra: None,
        },
        eppi_rank::Lookup::NotFound(code) => RankRecord {
            code: crate::model::normalize_code(code),
            fetched_at: now,
            found: false,
            display_name: None,
            rating: None,
            rank: None,
            sets: None,
            global_placement: None,
            regional_placement: None,
            extra: None,
        },
    }
}

/// The extra fields a lookup stored: set wins and losses this season.
pub fn season_record(record: &RankRecord) -> Option<(u32, u32)> {
    let extra: serde_json::Value = serde_json::from_str(record.extra.as_deref()?).ok()?;
    Some((
        extra["wins"].as_u64()? as u32,
        extra["losses"].as_u64()? as u32,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    const DAY: i64 = 24 * 3_600;

    fn found(found: bool) -> RankRecord {
        RankRecord {
            code: "ABC#123".into(),
            fetched_at: 0,
            found,
            ..Default::default()
        }
    }

    #[test]
    fn freshness_depends_on_when_you_last_played_them() {
        let now = 400 * DAY;
        // Played this month, or never (you, or someone searched for): a day.
        assert_eq!(fresh_for(&found(true), Some(now - 3 * DAY), now), DAY);
        assert_eq!(fresh_for(&found(true), None, now), DAY);
        // Not played for over a month: a week.
        assert_eq!(fresh_for(&found(true), Some(now - 90 * DAY), now), 7 * DAY);
        // No account has the code: a month, however recent.
        assert_eq!(fresh_for(&found(false), Some(now), now), 30 * DAY);
    }
}
