//! Reading what the list needs from a replay without its frames.

use crate::model::{Game, Mode, Player};
use peppi::game::{EndMethod, PlayerType};
use std::fs::File;
use std::io::{BufReader, Read, Seek};
use std::path::Path;

/// Players, stage, match and result of a finished replay, skipping its
/// frames (a seek to the end), so thousands read in seconds.
pub fn read_summary(path: &Path) -> Result<Game, String> {
    let file = File::open(path).map_err(|e| e.to_string())?;
    let mut reader = BufReader::with_capacity(16 * 1024, file);
    let opts = peppi::io::slippi::de::Opts {
        skip_frames: true,
        ..Default::default()
    };
    match guarded(|| peppi::io::slippi::read(&mut reader, Some(&opts))) {
        Ok(game) => Ok(summarize(
            path,
            &game.start,
            game.end.as_ref(),
            game.metadata.as_ref(),
        )),
        // peppi rejects some real replays (cut off mid-frame, or an end
        // method it doesn't know); eppi-stats reads those from a repaired
        // copy, frames and all.
        Err(fast_error) => {
            let game = eppi_stats::read_replay(path).map_err(|_| fast_error)?;
            let mut summary =
                summarize(path, &game.start, game.end.as_ref(), game.metadata.as_ref());
            if game
                .metadata
                .as_ref()
                .is_none_or(|m| m.get("lastFrame").is_none())
                && let Some(last) = game.frames.id.values().last()
            {
                summary.last_frame = *last;
            }
            Ok(summary)
        }
    }
}

/// What a replay that is still being recorded says about its players: the
/// game start event, which Slippi writes as the game begins.
pub fn read_live(path: &Path) -> Result<Game, String> {
    let file = File::open(path).map_err(|e| e.to_string())?;
    let mut reader = BufReader::new(file);
    let start = guarded(|| -> Result<peppi::game::Start, peppi::io::Error> {
        peppi::io::slippi::de::parse_header(&mut reader, None)?;
        let state = peppi::io::slippi::de::parse_start(&mut reader, None)?;
        Ok(peppi::game::Game::start(&state).clone())
    })?;
    let mut game = summarize(path, &start, None, None);
    game.last_frame = i32::MAX;
    Ok(game)
}

/// Runs a peppi read, turning a panic on a corrupt file into an error.
fn guarded<T, E: std::fmt::Display>(read: impl FnOnce() -> Result<T, E>) -> Result<T, String> {
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(read)) {
        Ok(Ok(value)) => Ok(value),
        Ok(Err(error)) => Err(error.to_string()),
        Err(_) => Err("the replay is corrupt (the parser gave up)".to_owned()),
    }
}

/// Whether a replay file is finished: Slippi writes the raw block's length
/// into the header only when the game ends (0 while recording).
pub fn is_finished(path: &Path) -> bool {
    let Ok(mut file) = File::open(path) else {
        return false;
    };
    let mut header = [0u8; 15];
    if file.read_exact(&mut header).is_err() {
        return false;
    }
    let _ = file.rewind();
    let len = u32::from_be_bytes([header[11], header[12], header[13], header[14]]);
    len > 0
}

fn summarize(
    path: &Path,
    start: &peppi::game::Start,
    end: Option<&peppi::game::End>,
    metadata: Option<&serde_json::Map<String, serde_json::Value>>,
) -> Game {
    let meta_players = metadata
        .and_then(|m| m.get("players"))
        .and_then(|p| p.as_object());
    let players: Vec<Player> = start
        .players
        .iter()
        .map(|p| {
            let port = u8::from(p.port);
            let names = meta_players
                .and_then(|m| m.get(&port.to_string()))
                .and_then(|p| p.get("names"));
            let from_meta = |key: &str| {
                names
                    .and_then(|n| n.get(key))
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_owned()
            };
            let (name, code) = match &p.netplay {
                Some(netplay) => (netplay.name.to_normalized(), netplay.code.to_normalized()),
                None => (from_meta("netplay"), from_meta("code")),
            };
            Player {
                port,
                character: p.character,
                costume: p.costume,
                team: if start.is_teams {
                    p.team.map(|t| t.color)
                } else {
                    None
                },
                cpu: matches!(p.r#type, PlayerType::Cpu),
                code: crate::model::normalize_code(&code),
                name: name.trim().to_owned(),
            }
        })
        .collect();

    let netplay = players.iter().any(|p| !p.code.is_empty());
    let (match_id, game_number, tiebreaker) =
        start.r#match.as_ref().map_or((String::new(), 0, 0), |m| {
            (m.id.clone(), m.game, m.tiebreaker)
        });
    let mut mode = Mode::from_match_id(&match_id, netplay);
    if start.is_teams && mode == Mode::Other {
        mode = Mode::Teams;
    }

    let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
    let start_at = metadata
        .and_then(|m| m.get("startAt"))
        .and_then(|v| v.as_str())
        .and_then(crate::time::parse_start_at)
        .or_else(|| crate::time::from_file_name(file_name))
        .or_else(|| {
            std::fs::metadata(path)
                .and_then(|m| m.modified())
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_secs() as i64)
        })
        .unwrap_or(0);
    let last_frame = metadata
        .and_then(|m| m.get("lastFrame"))
        .and_then(|v| v.as_i64())
        .unwrap_or(-123) as i32;

    let lras = end.and_then(|e| e.lras_initiator).flatten().map(u8::from);
    let winners = end.map_or_else(Vec::new, |end| winners(start, end, &players, lras));

    Game {
        id: 0,
        path: path.to_owned(),
        start_at,
        last_frame,
        stage: start.stage,
        mode,
        match_id,
        game_number,
        tiebreaker,
        is_teams: start.is_teams,
        players,
        winners,
        lras,
        stocks_left: Vec::new(),
        stats_version: 0,
        numbers: Vec::new(),
    }
}

/// Winners from the game end event alone (the stats pass refines this from
/// the frames for old replays and timeouts): the side that didn't quit, else
/// first place and their team.
fn winners(
    start: &peppi::game::Start,
    end: &peppi::game::End,
    players: &[Player],
    lras: Option<u8>,
) -> Vec<u8> {
    if let Some(quitter) = lras {
        if players.len() == 2 {
            return players
                .iter()
                .map(|p| p.port)
                .filter(|p| *p != quitter)
                .collect();
        }
        return Vec::new();
    }
    if matches!(end.method, EndMethod::NoContest | EndMethod::Unresolved) {
        return Vec::new();
    }
    let Some(placements) = &end.players else {
        return Vec::new();
    };
    let Some(first) = placements.iter().find(|p| p.placement == 0) else {
        return Vec::new();
    };
    let first_port = u8::from(first.port);
    if start.is_teams {
        let team = players
            .iter()
            .find(|p| p.port == first_port)
            .and_then(|p| p.team);
        players
            .iter()
            .filter(|p| p.team.is_some() && p.team == team)
            .map(|p| p.port)
            .collect()
    } else {
        vec![first_port]
    }
}
