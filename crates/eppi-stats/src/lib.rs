//! Slippi replay statistics: a port of slippi-js's `getStats()` and `getWinners()` on top of
//! [peppi].
//!
//! Licensed under the GNU LGPL v3.0 or later, as is slippi-js
//! (<https://github.com/project-slippi/slippi-js>, by Project Slippi), which this ports. See
//! `COPYING.LESSER` and `COPYING` next to this crate's `Cargo.toml`.
//!
//! ```no_run
//! let game = eppi_stats::read_replay("Game_20250515T114102.slp".as_ref()).unwrap();
//! let stats = eppi_stats::compute(&game);
//! println!("{}", serde_json::to_string(&stats).unwrap());
//! ```
//!
//! Definitions follow slippi-js 9.1.3 exactly (including its quirks), so numbers match the
//! Slippi Launcher's stats page. Stocks, combos, conversions, action counts and input counts are
//! only computed for singles (exactly two players), as in slippi-js. Serialized field names are
//! camelCase and line up with slippi-js's JSON; the extras ([`Stock::killed_by`],
//! [`Stock::kill_move_id`], [`GameStats::winners`], [`GameStats::players`]) are additions.
//!
//! Results are meant to be cached; bump [`STATS_VERSION`] whenever output changes.

#![warn(missing_docs)]

mod actions;
mod frames;
mod inputs;
pub mod melee;
mod overall;
mod punishes;
mod repair;
mod stocks;
mod types;
mod winners;

use std::io::Cursor;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::Path;

pub use peppi::game::immutable::Game;
pub use types::*;

use frames::{FIRST_PLAYABLE_FRAME, Frames, read_post};

/// Version of the stats output. Bump whenever [`compute`]'s output changes so cached stats
/// get recomputed.
pub const STATS_VERSION: u32 = 1;

/// Error from [`read_replay`].
#[derive(Debug)]
pub enum ReadError {
    /// The file couldn't be opened.
    Io(std::io::Error),
    /// peppi rejected the replay (truncated, corrupt, or an unsupported version).
    Parse(String),
    /// peppi panicked while parsing (it asserts on some malformed input).
    Panic(String),
}

impl std::fmt::Display for ReadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ReadError::Io(e) => write!(f, "can't open replay: {e}"),
            ReadError::Parse(e) => write!(f, "can't parse replay: {e}"),
            ReadError::Panic(e) => write!(f, "replay parser crashed: {e}"),
        }
    }
}

impl std::error::Error for ReadError {}

fn panic_message(payload: Box<dyn std::any::Any + Send>) -> String {
    payload
        .downcast_ref::<&str>()
        .map(|s| s.to_string())
        .or_else(|| payload.downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "unknown panic".to_owned())
}

fn parse(bytes: &[u8]) -> Result<Game, ReadError> {
    match catch_unwind(AssertUnwindSafe(|| {
        peppi::io::slippi::read(Cursor::new(bytes), None)
    })) {
        Ok(Ok(game)) => Ok(game),
        Ok(Err(e)) => Err(ReadError::Parse(e.to_string())),
        Err(payload) => Err(ReadError::Panic(panic_message(payload))),
    }
}

/// Reads a replay file (read-only) with all frame data.
///
/// Beyond `peppi::io::slippi::read`, this:
/// - catches peppi panics (it `assert!`s on some malformed input) as [`ReadError::Panic`]
///   (the default panic hook still prints them to stderr; install a quiet hook if that matters);
/// - reads replays that were cut off mid-write (Dolphin crashed or the file wasn't finished),
///   keeping every complete frame but dropping the metadata block;
/// - reads replays whose Game End method peppi 2.1 doesn't know (newer Slippi writes 9). In that
///   case `game.end.method` is a stand-in (`Resolved`) but `game.end.bytes` keeps the original
///   byte, which is what [`compute`] uses.
pub fn read_replay(path: &Path) -> Result<Game, ReadError> {
    let data = std::fs::read(path).map_err(ReadError::Io)?;
    read_replay_bytes(&data)
}

/// [`read_replay`] for a replay already in memory.
pub fn read_replay_bytes(data: &[u8]) -> Result<Game, ReadError> {
    let err = match parse(data) {
        Ok(game) => return Ok(game),
        Err(e) => e,
    };
    let Some(fixed) = repair::repair(data) else {
        return Err(err);
    };
    let mut game = parse(&fixed.bytes).map_err(|_| err)?;
    if let (Some(method), Some(end)) = (fixed.original_end_method, game.end.as_mut())
        && let Some(b) = end.bytes.0.first_mut()
    {
        *b = method;
    }
    Ok(game)
}

/// Computes all stats for a parsed game. Never panics: on malformed data it returns whatever
/// could be computed (in the worst case, defaults with no frames processed).
pub fn compute(game: &Game) -> GameStats {
    catch_unwind(AssertUnwindSafe(|| compute_inner(game))).unwrap_or_default()
}

fn compute_inner(game: &Game) -> GameStats {
    let frames = frames::extract(game);
    let player_count = frames.players.len();
    // Singles permutations: (player, opponent) indices into `frames.players`.
    let perms: Vec<(usize, usize)> = if player_count == 2 {
        vec![(0, 1), (1, 0)]
    } else {
        Vec::new()
    };

    let last_frame = frames.last_frame;
    let playable_frame_count = if game.frames.id.len() == 0 || last_frame < FIRST_PLAYABLE_FRAME {
        0
    } else {
        last_frame - FIRST_PLAYABLE_FRAME
    };

    let mut stocks = stocks::compute(&frames, &perms);
    let conversions = punishes::conversions(&frames, &perms);
    let combos = punishes::combos(&frames, &perms);
    let action_counts = actions::compute(&frames, &perms);
    let inputs = inputs::compute(&frames, &perms);
    stocks::attribute_kills(&mut stocks, &conversions);

    let settings_players: Vec<overall::SettingsPlayer> = game
        .start
        .players
        .iter()
        .map(|p| overall::SettingsPlayer {
            index: u8::from(p.port),
            team: p.team.map(|t| t.color),
        })
        .collect();
    let overall = overall::compute(
        &settings_players,
        game.start.is_teams,
        &inputs,
        &conversions,
        playable_frame_count,
    );

    GameStats {
        last_frame,
        playable_frame_count,
        stocks,
        conversions,
        combos,
        action_counts,
        overall,
        game_complete: game.end.is_some(),
        winners: winners::compute(game),
        players: player_summaries(game, &frames),
    }
}

/// Each player's stocks and percent on their last frame with data (latest copy of each frame).
fn player_summaries(game: &Game, frames: &Frames) -> Vec<PlayerSummary> {
    let rows = frames::latest_rows(&game.frames);
    let cols = frames::port_columns(game);
    frames
        .players
        .iter()
        .zip(&cols)
        .map(|(pf, col)| {
            let last = col.and_then(|c| {
                let data = &game.frames.ports[c].leader;
                rows.iter()
                    .rev()
                    .flatten()
                    .find_map(|&row| read_post(data, row))
            });
            PlayerSummary {
                player_index: pf.player_index,
                final_stocks: last.map_or(0, |p| p.stocks),
                final_percent: last.map_or(0.0, |p| f64::from(p.percent)),
            }
        })
        .collect()
}
