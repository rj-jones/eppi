//! Looking up ranks in the background: the UI says which codes it shows,
//! and a worker fetches the ones not looked up in the last day, a batch at
//! a time and at most one request a second, backing off when Slippi says
//! to slow down. It also fetches the ranked seasons' calendar (with how
//! you finished each one).

use crate::library::db::RankRecord;
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::time::{Duration, Instant};

/// How long a lookup counts as fresh.
pub const FRESH_FOR: i64 = 24 * 3_600;
/// Pause between requests.
const PACE: Duration = Duration::from_millis(1_100);

enum Request {
    Fetch(Vec<String>),
    Seasons(String),
    Shutdown,
}

/// What the worker found out.
pub enum Reply {
    Ranks(Result<Vec<RankRecord>, String>),
    /// The seasons `code` played in (all of them since their account was
    /// made), newest first.
    Seasons {
        code: String,
        result: Result<Vec<crate::model::Season>, String>,
    },
}

pub struct RankService {
    requests: Sender<Request>,
    pub results: Receiver<Reply>,
    /// Codes asked for and not answered yet.
    pending: HashSet<String>,
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
            tried: HashMap::new(),
            thread: Some(thread),
        }
    }

    /// Asks for `codes` that aren't fresh in `known` (or all of them with
    /// `force`).
    pub fn want<'a>(
        &mut self,
        codes: impl IntoIterator<Item = &'a str>,
        known: &HashMap<String, RankRecord>,
        now: i64,
        force: bool,
    ) {
        let mut batch = Vec::new();
        for code in codes {
            if code.is_empty() || !code.contains('#') || self.pending.contains(code) {
                continue;
            }
            if !force {
                if known
                    .get(code)
                    .is_some_and(|r| now - r.fetched_at < FRESH_FOR)
                {
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

    pub fn answered(&mut self, records: &[RankRecord]) {
        for record in records {
            self.pending.remove(&record.code);
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
    let mut seasons: Option<String> = None;
    let mut next_request = Instant::now();
    loop {
        let wait = if queue.is_empty() && seasons.is_none() {
            Duration::from_secs(3_600)
        } else {
            next_request.saturating_duration_since(Instant::now())
        };
        match requests.recv_timeout(wait) {
            Ok(Request::Fetch(codes)) => {
                for code in codes {
                    if !queue.contains(&code) {
                        queue.push_back(code);
                    }
                }
                continue;
            }
            Ok(Request::Seasons(code)) => {
                seasons = Some(code);
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
