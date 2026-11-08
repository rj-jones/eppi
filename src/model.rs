//! Games as the app keeps them in memory, whose they are, how they ended,
//! and how they fold into sets.

use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::PathBuf;

/// The kind of game, from the Slippi match id (`mode.ranked-…`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum Mode {
    Ranked,
    Unranked,
    Direct,
    Teams,
    /// Offline (a console or local Dolphin): no match id, no netplay codes.
    Local,
    #[default]
    Other,
}

impl Mode {
    pub const ALL: [Mode; 5] = [
        Mode::Ranked,
        Mode::Unranked,
        Mode::Direct,
        Mode::Teams,
        Mode::Local,
    ];

    pub fn from_match_id(id: &str, netplay: bool) -> Self {
        let kind = id
            .strip_prefix("mode.")
            .and_then(|rest| rest.split('-').next())
            .unwrap_or("");
        match kind {
            "ranked" => Mode::Ranked,
            "unranked" => Mode::Unranked,
            "direct" => Mode::Direct,
            "teams" => Mode::Teams,
            "" if !netplay => Mode::Local,
            _ => Mode::Other,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Mode::Ranked => "Ranked",
            Mode::Unranked => "Unranked",
            Mode::Direct => "Direct",
            Mode::Teams => "Teams",
            Mode::Local => "Local",
            Mode::Other => "Other",
        }
    }

    pub fn to_i64(self) -> i64 {
        match self {
            Mode::Ranked => 0,
            Mode::Unranked => 1,
            Mode::Direct => 2,
            Mode::Teams => 3,
            Mode::Local => 4,
            Mode::Other => 5,
        }
    }

    pub fn from_i64(value: i64) -> Self {
        match value {
            0 => Mode::Ranked,
            1 => Mode::Unranked,
            2 => Mode::Direct,
            3 => Mode::Teams,
            4 => Mode::Local,
            _ => Mode::Other,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct Player {
    /// 0–3 (P1–P4).
    pub port: u8,
    /// External character id.
    pub character: u8,
    pub costume: u8,
    pub team: Option<u8>,
    pub cpu: bool,
    /// Connect code (`BEAN#888`), empty offline.
    pub code: String,
    /// Netplay display name, empty offline.
    pub name: String,
}

impl Player {
    /// The name to show: the display name, else the code, else the port.
    pub fn display_name(&self) -> String {
        if !self.name.is_empty() {
            self.name.clone()
        } else if !self.code.is_empty() {
            self.code.clone()
        } else if self.cpu {
            "CPU".to_owned()
        } else {
            format!("P{}", self.port + 1)
        }
    }
}

/// One replay file.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct Game {
    pub id: i64,
    pub path: PathBuf,
    /// When the game started, Unix seconds (UTC).
    pub start_at: i64,
    /// Slippi's last frame number (the game lasted `last_frame + 123` frames).
    pub last_frame: i32,
    pub stage: u16,
    pub mode: Mode,
    pub match_id: String,
    pub game_number: u32,
    pub tiebreaker: u32,
    pub is_teams: bool,
    pub players: Vec<Player>,
    /// Ports of the winners (one in 1v1), empty when unknown or a no contest.
    pub winners: Vec<u8>,
    /// Who quit with L+R+A+Start, if someone did.
    pub lras: Option<u8>,
    /// Stocks each port had left at the end (from the stats pass).
    pub stocks_left: Vec<(u8, u8)>,
    /// The stats version the cached stats were computed with (0: none yet).
    pub stats_version: u32,
    /// Per-player numbers from the stats pass (empty before it).
    pub numbers: Vec<Numbers>,
}

/// One player's numbers in one game: what the aggregate views add up.
/// Ratios are kept as (count, total) so they sum across games correctly.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Numbers {
    pub port: u8,
    pub kills: u32,
    pub damage: f64,
    pub conversions: u32,
    pub openings_per_kill: (f64, f64),
    pub damage_per_opening: (f64, f64),
    pub neutral_wins: (f64, f64),
    pub counter_hits: (f64, f64),
    pub trades: (f64, f64),
    pub inputs_per_minute: (f64, f64),
    /// (successful, all) L-cancels.
    pub l_cancels: (u32, u32),
    /// (techs, techs + missed techs) on the ground.
    pub techs: (u32, u32),
    pub wavedashes: u32,
    pub dash_dances: u32,
    pub ledgegrabs: u32,
    pub rolls: u32,
    pub spot_dodges: u32,
    pub air_dodges: u32,
    /// (successful, all) grabs.
    pub grabs: (u32, u32),
}

impl Game {
    pub fn numbers(&self, port: u8) -> Option<&Numbers> {
        self.numbers.iter().find(|n| n.port == port)
    }
}

impl Game {
    pub fn seconds(&self) -> f64 {
        crate::melee::game_seconds(self.last_frame)
    }

    /// A singles game between two humans or a human and a CPU.
    pub fn is_singles(&self) -> bool {
        self.players.len() == 2 && !self.is_teams
    }

    pub fn has_cpu(&self) -> bool {
        self.players.iter().any(|p| p.cpu)
    }

    pub fn player(&self, port: u8) -> Option<&Player> {
        self.players.iter().find(|p| p.port == port)
    }

    pub fn stocks_left(&self, port: u8) -> Option<u8> {
        self.stocks_left
            .iter()
            .find(|(p, _)| *p == port)
            .map(|(_, s)| *s)
    }

    /// Which player is "me" and which the opponent, in singles.
    pub fn sides(&self, me: &Me) -> Option<Sides> {
        if !self.is_singles() {
            return None;
        }
        let mine = self.players.iter().position(|p| me.is_me(&p.code))?;
        Some(Sides {
            me: mine,
            opponent: 1 - mine,
        })
    }

    /// My result in this game, when it's a singles game of mine.
    pub fn outcome(&self, me: &Me) -> Outcome {
        let Some(sides) = self.sides(me) else {
            return Outcome::Unknown;
        };
        let my_port = self.players[sides.me].port;
        if self.winners.is_empty() {
            Outcome::Unknown
        } else if self.winners.contains(&my_port) {
            Outcome::Win
        } else {
            Outcome::Loss
        }
    }

    pub fn opponent<'a>(&'a self, me: &Me) -> Option<&'a Player> {
        self.sides(me).map(|s| &self.players[s.opponent])
    }

    pub fn mine<'a>(&'a self, me: &Me) -> Option<&'a Player> {
        self.sides(me).map(|s| &self.players[s.me])
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Sides {
    pub me: usize,
    pub opponent: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Outcome {
    Win,
    Loss,
    Unknown,
}

/// The connect codes that count as me.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Me {
    codes: HashSet<String>,
}

impl Me {
    pub fn new<'a>(codes: impl IntoIterator<Item = &'a String>) -> Self {
        Self {
            codes: codes
                .into_iter()
                .map(|c| normalize_code(c))
                .filter(|c| !c.is_empty())
                .collect(),
        }
    }

    pub fn is_me(&self, code: &str) -> bool {
        !code.is_empty() && self.codes.contains(&normalize_code(code))
    }

    pub fn is_empty(&self) -> bool {
        self.codes.is_empty()
    }
}

/// `bean#888` and full-width `＃` → `BEAN#888`.
pub fn normalize_code(code: &str) -> String {
    code.trim().replace('＃', "#").to_uppercase()
}

/// Date presets for the filter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum Period {
    #[default]
    All,
    Today,
    Week,
    Month,
    Year,
}

impl Period {
    pub const ALL: [Period; 5] = [
        Period::All,
        Period::Today,
        Period::Week,
        Period::Month,
        Period::Year,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Period::All => "All time",
            Period::Today => "Today",
            Period::Week => "7 days",
            Period::Month => "30 days",
            Period::Year => "12 months",
        }
    }

    /// The earliest start time it lets through, `now` in Unix seconds.
    pub fn since(self, now: i64) -> Option<i64> {
        const DAY: i64 = 86_400;
        match self {
            Period::All => None,
            Period::Today => Some(crate::time::local_day_start(now)),
            Period::Week => Some(now - 7 * DAY),
            Period::Month => Some(now - 30 * DAY),
            Period::Year => Some(now - 365 * DAY),
        }
    }
}

/// What the filter bar asks for.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Filter {
    /// Hide games shorter than [`SHORT_GAME_SECONDS`].
    pub hide_short: bool,
    pub hide_cpu: bool,
    /// Modes shown; empty shows all.
    pub modes: Vec<Mode>,
    pub my_character: Option<u8>,
    pub their_character: Option<u8>,
    pub stage: Option<u16>,
    pub period: Period,
    /// Only games of mine.
    pub only_mine: bool,
    /// Matches an opponent's code or name (or either player's when the game
    /// isn't mine).
    pub search: String,
}

pub const SHORT_GAME_SECONDS: f64 = 30.0;

impl Default for Filter {
    fn default() -> Self {
        Self {
            hide_short: true,
            hide_cpu: true,
            modes: Vec::new(),
            my_character: None,
            their_character: None,
            stage: None,
            period: Period::All,
            only_mine: false,
            search: String::new(),
        }
    }
}

impl Filter {
    /// How many settings differ from the default (for the filter button).
    pub fn active_count(&self) -> usize {
        let default = Filter::default();
        [
            self.hide_short != default.hide_short,
            self.hide_cpu != default.hide_cpu,
            !self.modes.is_empty(),
            self.my_character.is_some(),
            self.their_character.is_some(),
            self.stage.is_some(),
            self.period != Period::All,
            self.only_mine,
        ]
        .into_iter()
        .filter(|on| *on)
        .count()
    }

    pub fn matches(&self, game: &Game, me: &Me, now: i64) -> bool {
        if self.hide_short && game.seconds() < SHORT_GAME_SECONDS {
            return false;
        }
        if self.hide_cpu && game.has_cpu() {
            return false;
        }
        if !self.modes.is_empty() && !self.modes.contains(&game.mode) {
            return false;
        }
        if let Some(stage) = self.stage
            && game.stage != stage
        {
            return false;
        }
        if let Some(since) = self.period.since(now)
            && game.start_at < since
        {
            return false;
        }
        let sides = game.sides(me);
        if self.only_mine && sides.is_none() {
            return false;
        }
        if let Some(character) = self.my_character {
            match sides {
                Some(s) if game.players[s.me].character == character => {}
                _ => return false,
            }
        }
        if let Some(character) = self.their_character {
            match sides {
                Some(s) if game.players[s.opponent].character == character => {}
                None if game.players.iter().any(|p| p.character == character) => {}
                _ => return false,
            }
        }
        let needle = self.search.trim();
        if !needle.is_empty() {
            let needle = needle.to_lowercase();
            let hit = |p: &Player| {
                p.code.to_lowercase().contains(&needle) || p.name.to_lowercase().contains(&needle)
            };
            let found = match sides {
                Some(s) => hit(&game.players[s.opponent]),
                None => game.players.iter().any(hit),
            };
            let stage_hit = crate::melee::stage_name(game.stage)
                .to_lowercase()
                .contains(&needle);
            let character_hit = game.players.iter().any(|p| {
                crate::melee::character_name(p.character)
                    .to_lowercase()
                    .contains(&needle)
            });
            if !found && !stage_hit && !character_hit {
                return false;
            }
        }
        true
    }
}

/// A row of the replay list: a set (or session) of games, or one game.
#[derive(Debug, Clone, PartialEq)]
pub enum Row {
    Set(GameSet),
    Game(usize),
}

#[derive(Debug, Clone, PartialEq)]
pub struct GameSet {
    /// Hash of the match id: the row's key.
    pub key: u64,
    pub kind: SetKind,
    /// Indexes into the games, newest first.
    pub games: Vec<usize>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SetKind {
    /// A ranked best of three.
    Ranked,
    /// Two or more unranked or direct games in one match.
    Session,
}

/// Folds `order` (indexes into `games`, newest first) into rows: games that
/// share a match id fold into one row, always when ranked and from two games
/// otherwise.
pub fn group_rows(games: &[Game], order: &[usize]) -> Vec<Row> {
    let mut rows = Vec::new();
    let mut i = 0;
    while i < order.len() {
        let first = &games[order[i]];
        let mut j = i + 1;
        if !first.match_id.is_empty() {
            while j < order.len() && games[order[j]].match_id == first.match_id {
                j += 1;
            }
        }
        let run = &order[i..j];
        let kind = match first.mode {
            Mode::Ranked if !first.match_id.is_empty() => Some(SetKind::Ranked),
            _ if run.len() >= 2 => Some(SetKind::Session),
            _ => None,
        };
        match kind {
            Some(kind) => rows.push(Row::Set(GameSet {
                key: hash_str(&first.match_id),
                kind,
                games: run.to_vec(),
            })),
            None => rows.extend(run.iter().map(|&g| Row::Game(g))),
        }
        i = j;
    }
    rows
}

/// A set's score from my side: (my wins, their wins).
pub fn set_score(games: &[Game], set: &[usize], me: &Me) -> (u32, u32) {
    let mut score = (0, 0);
    for &g in set {
        match games[g].outcome(me) {
            Outcome::Win => score.0 += 1,
            Outcome::Loss => score.1 += 1,
            Outcome::Unknown => {}
        }
    }
    score
}

pub fn hash_str(value: &str) -> u64 {
    use std::hash::{BuildHasher, BuildHasherDefault, DefaultHasher};
    BuildHasherDefault::<DefaultHasher>::default().hash_one(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn player(port: u8, code: &str, character: u8) -> Player {
        Player {
            port,
            character,
            code: code.into(),
            name: code.split('#').next().unwrap_or("").into(),
            ..Default::default()
        }
    }

    fn game(id: i64, match_id: &str, mode: Mode, start_at: i64, winner: u8) -> Game {
        Game {
            id,
            match_id: match_id.into(),
            mode,
            start_at,
            last_frame: 60 * 120,
            players: vec![player(0, "BEAN#888", 2), player(1, "ABC#123", 20)],
            winners: vec![winner],
            ..Default::default()
        }
    }

    #[test]
    fn modes_come_from_the_match_id() {
        assert_eq!(
            Mode::from_match_id("mode.ranked-2026-11-08T01:59:18.73Z-1", true),
            Mode::Ranked
        );
        assert_eq!(
            Mode::from_match_id("mode.unranked-2025-05-18T19:27:16.62-2", true),
            Mode::Unranked
        );
        assert_eq!(Mode::from_match_id("mode.direct-x", true), Mode::Direct);
        assert_eq!(Mode::from_match_id("", false), Mode::Local);
        assert_eq!(Mode::from_match_id("", true), Mode::Other);
    }

    #[test]
    fn ranked_games_always_fold_and_unranked_from_two() {
        let games = vec![
            game(1, "mode.ranked-a", Mode::Ranked, 100, 0),
            game(2, "mode.ranked-a", Mode::Ranked, 90, 1),
            game(3, "mode.ranked-b", Mode::Ranked, 80, 0),
            game(4, "mode.unranked-c", Mode::Unranked, 70, 0),
            game(5, "mode.unranked-d", Mode::Unranked, 60, 0),
            game(6, "mode.unranked-d", Mode::Unranked, 50, 1),
            game(7, "", Mode::Local, 40, 0),
            game(8, "", Mode::Local, 30, 0),
        ];
        let order: Vec<usize> = (0..games.len()).collect();
        let rows = group_rows(&games, &order);
        assert_eq!(rows.len(), 6);
        assert!(matches!(&rows[0], Row::Set(s) if s.kind == SetKind::Ranked && s.games == [0, 1]));
        assert!(matches!(&rows[1], Row::Set(s) if s.kind == SetKind::Ranked && s.games == [2]));
        assert_eq!(rows[2], Row::Game(3));
        assert!(matches!(&rows[3], Row::Set(s) if s.kind == SetKind::Session && s.games == [4, 5]));
        assert_eq!(rows[4], Row::Game(6));
        assert_eq!(rows[5], Row::Game(7), "local games never fold");
    }

    #[test]
    fn outcomes_and_scores_follow_my_codes() {
        let me = Me::new(&["bean#888".to_owned()]);
        let games = vec![
            game(1, "m", Mode::Ranked, 3, 0),
            game(2, "m", Mode::Ranked, 2, 1),
            game(3, "m", Mode::Ranked, 1, 0),
        ];
        assert_eq!(games[0].outcome(&me), Outcome::Win);
        assert_eq!(games[1].outcome(&me), Outcome::Loss);
        assert_eq!(set_score(&games, &[0, 1, 2], &me), (2, 1));
        assert_eq!(games[0].outcome(&Me::default()), Outcome::Unknown);
    }

    #[test]
    fn the_filter_hides_short_and_cpu_games_by_default() {
        let me = Me::new(&["BEAN#888".to_owned()]);
        let filter = Filter::default();
        let mut short = game(1, "", Mode::Unranked, 0, 0);
        short.last_frame = 60 * 10;
        assert!(!filter.matches(&short, &me, 0));
        let mut cpu = game(2, "", Mode::Local, 0, 0);
        cpu.players[1].cpu = true;
        assert!(!filter.matches(&cpu, &me, 0));
        let normal = game(3, "", Mode::Unranked, 0, 0);
        assert!(filter.matches(&normal, &me, 0));
        let search = Filter {
            search: "abc".into(),
            ..Filter::default()
        };
        assert!(search.matches(&normal, &me, 0));
        let falco = Filter {
            their_character: Some(20),
            ..Filter::default()
        };
        assert!(falco.matches(&normal, &me, 0));
        let fox = Filter {
            their_character: Some(2),
            ..Filter::default()
        };
        assert!(!fox.matches(&normal, &me, 0), "fox is mine, not theirs");
    }
}
