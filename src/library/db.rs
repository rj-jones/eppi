//! The SQLite index: every replay's summary (so a start doesn't parse 8,000
//! files again), each game's stats, opponents' ranks over time, and notes.

use crate::model::{Game, Mode, Player};
use rusqlite::{Connection, OptionalExtension, params};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

const SCHEMA_VERSION: i32 = 6;

pub fn open(path: &Path) -> rusqlite::Result<Connection> {
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let conn = Connection::open(path)?;
    conn.busy_timeout(std::time::Duration::from_secs(10))?;
    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.pragma_update(None, "synchronous", "NORMAL")?;
    conn.pragma_update(None, "foreign_keys", "ON")?;
    migrate(&conn)?;
    Ok(conn)
}

/// Brings the schema up to date in one write transaction, so two
/// connections (the window's and the worker's) can't both run a step.
fn migrate(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch("BEGIN IMMEDIATE")?;
    match migrate_steps(conn) {
        Ok(()) => conn.execute_batch("COMMIT"),
        Err(e) => {
            let _ = conn.execute_batch("ROLLBACK");
            Err(e)
        }
    }
}

fn migrate_steps(conn: &Connection) -> rusqlite::Result<()> {
    let version: i32 = conn.pragma_query_value(None, "user_version", |row| row.get(0))?;
    if version < 1 {
        conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS games (
                id INTEGER PRIMARY KEY,
                path TEXT NOT NULL UNIQUE,
                size INTEGER NOT NULL,
                mtime INTEGER NOT NULL,
                error TEXT,
                start_at INTEGER NOT NULL DEFAULT 0,
                last_frame INTEGER NOT NULL DEFAULT 0,
                stage INTEGER NOT NULL DEFAULT 0,
                mode INTEGER NOT NULL DEFAULT 5,
                match_id TEXT NOT NULL DEFAULT '',
                game_number INTEGER NOT NULL DEFAULT 0,
                tiebreaker INTEGER NOT NULL DEFAULT 0,
                is_teams INTEGER NOT NULL DEFAULT 0,
                players TEXT NOT NULL DEFAULT '[]',
                winners TEXT NOT NULL DEFAULT '[]',
                lras INTEGER,
                stocks_left TEXT NOT NULL DEFAULT '[]',
                stats_version INTEGER NOT NULL DEFAULT 0
            );
            CREATE INDEX IF NOT EXISTS games_start ON games(start_at);
            CREATE TABLE IF NOT EXISTS game_stats (
                game_id INTEGER PRIMARY KEY REFERENCES games(id) ON DELETE CASCADE,
                version INTEGER NOT NULL,
                json TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS ranks (
                code TEXT NOT NULL,
                fetched_at INTEGER NOT NULL,
                found INTEGER NOT NULL,
                display_name TEXT,
                rating REAL,
                rank TEXT,
                sets INTEGER,
                global_placement INTEGER,
                regional_placement INTEGER,
                extra TEXT,
                PRIMARY KEY (code, fetched_at)
            );
            CREATE TABLE IF NOT EXISTS notes (
                code TEXT PRIMARY KEY,
                text TEXT NOT NULL,
                updated_at INTEGER NOT NULL
            );
            ",
        )?;
    }
    if version < 2 {
        // The stats pass's compact numbers, and every punish and kill as
        // rows (Highlights and the matchup report query them).
        conn.execute_batch(
            "
            ALTER TABLE games ADD COLUMN numbers TEXT NOT NULL DEFAULT '[]';
            CREATE TABLE IF NOT EXISTS punishes (
                game_id INTEGER NOT NULL REFERENCES games(id) ON DELETE CASCADE,
                attacker INTEGER,
                victim INTEGER NOT NULL,
                start_frame INTEGER NOT NULL,
                end_frame INTEGER,
                start_percent REAL NOT NULL,
                end_percent REAL NOT NULL,
                hits INTEGER NOT NULL,
                did_kill INTEGER NOT NULL,
                opening TEXT NOT NULL,
                first_move INTEGER,
                last_move INTEGER
            );
            CREATE INDEX IF NOT EXISTS punishes_game ON punishes(game_id);
            CREATE INDEX IF NOT EXISTS punishes_damage ON punishes(end_percent - start_percent);
            CREATE TABLE IF NOT EXISTS kills (
                game_id INTEGER NOT NULL REFERENCES games(id) ON DELETE CASCADE,
                victim INTEGER NOT NULL,
                killer INTEGER,
                stock INTEGER NOT NULL,
                frame INTEGER,
                percent REAL,
                move_id INTEGER
            );
            CREATE INDEX IF NOT EXISTS kills_game ON kills(game_id);
            UPDATE games SET stats_version = 0;
            ",
        )?;
    }
    if version < 3 {
        // Before 3, stats without a winner wiped the winner the game end
        // event named (quit-outs): read every file again.
        conn.execute_batch("UPDATE games SET mtime = -1;")?;
    }
    if version < 4 {
        // Files peppi rejected are read from a repaired copy since 4.
        conn.execute_batch("UPDATE games SET mtime = -1 WHERE error IS NOT NULL;")?;
    }
    if version < 5 {
        // A game's identity apart from its path, so copies of a replay in
        // several folders count once: read every file again to fill it in.
        conn.execute_batch(
            "ALTER TABLE games ADD COLUMN fingerprint INTEGER;
             CREATE INDEX IF NOT EXISTS games_fingerprint ON games(fingerprint);
             UPDATE games SET mtime = -1;",
        )?;
    }
    if version < 6 {
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS seasons (
                code TEXT NOT NULL,
                id TEXT NOT NULL,
                fetched_at INTEGER NOT NULL,
                json TEXT NOT NULL,
                PRIMARY KEY (code, id)
            );",
        )?;
    }
    if version < SCHEMA_VERSION {
        conn.pragma_update(None, "user_version", SCHEMA_VERSION)?;
    }
    Ok(())
}

/// What the index knows about a file on disk.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FileState {
    pub id: i64,
    pub size: u64,
    pub mtime: i64,
}

pub fn file_states(conn: &Connection) -> rusqlite::Result<HashMap<PathBuf, FileState>> {
    let mut statement = conn.prepare("SELECT id, path, size, mtime FROM games")?;
    let rows = statement.query_map([], |row| {
        Ok((
            PathBuf::from(row.get::<_, String>(1)?),
            FileState {
                id: row.get(0)?,
                size: row.get::<_, i64>(2)? as u64,
                mtime: row.get(3)?,
            },
        ))
    })?;
    rows.collect()
}

pub fn file_state(conn: &Connection, path: &Path) -> rusqlite::Result<Option<FileState>> {
    conn.query_row(
        "SELECT id, size, mtime FROM games WHERE path = ?1",
        [path.to_string_lossy()],
        |row| {
            Ok(FileState {
                id: row.get(0)?,
                size: row.get::<_, i64>(1)? as u64,
                mtime: row.get(2)?,
            })
        },
    )
    .optional()
}

const GAME_COLUMNS: &str = "id, path, start_at, last_frame, stage, mode, match_id, game_number, tiebreaker, is_teams, players, winners, lras, stocks_left, stats_version, numbers, fingerprint";

/// Rows that are the copy of their game the app shows: copies of one
/// replay in several places share a fingerprint, and the first indexed
/// stands for them all.
const PRIMARY: &str =
    "id IN (SELECT MIN(id) FROM games WHERE error IS NULL GROUP BY COALESCE(fingerprint, -id))";

fn game_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Game> {
    let players: String = row.get(10)?;
    let winners: String = row.get(11)?;
    let stocks_left: String = row.get(13)?;
    Ok(Game {
        id: row.get(0)?,
        path: PathBuf::from(row.get::<_, String>(1)?),
        start_at: row.get(2)?,
        last_frame: row.get(3)?,
        stage: row.get::<_, i64>(4)? as u16,
        mode: Mode::from_i64(row.get(5)?),
        match_id: row.get(6)?,
        game_number: row.get::<_, i64>(7)? as u32,
        tiebreaker: row.get::<_, i64>(8)? as u32,
        is_teams: row.get(9)?,
        players: serde_json::from_str::<Vec<Player>>(&players).unwrap_or_default(),
        winners: serde_json::from_str(&winners).unwrap_or_default(),
        lras: row.get::<_, Option<i64>>(12)?.map(|p| p as u8),
        stocks_left: serde_json::from_str(&stocks_left).unwrap_or_default(),
        stats_version: row.get::<_, i64>(14)? as u32,
        numbers: serde_json::from_str(&row.get::<_, String>(15)?).unwrap_or_default(),
        fingerprint: row.get::<_, Option<i64>>(16)?.unwrap_or_default(),
    })
}

/// Every readable game (one copy of each), newest first.
pub fn load_games(conn: &Connection) -> rusqlite::Result<Vec<Game>> {
    let mut statement = conn.prepare(&format!(
        "SELECT {GAME_COLUMNS} FROM games WHERE error IS NULL AND {PRIMARY} ORDER BY start_at DESC, id DESC"
    ))?;
    let rows = statement.query_map([], game_from_row)?;
    rows.collect()
}

pub fn load_game(conn: &Connection, id: i64) -> rusqlite::Result<Option<Game>> {
    conn.query_row(
        &format!("SELECT {GAME_COLUMNS} FROM games WHERE id = ?1"),
        [id],
        game_from_row,
    )
    .optional()
}

/// Inserts or replaces a game read from `path`; returns its id. The same
/// game read again (its fingerprint unchanged) keeps its stats.
pub fn upsert_game(conn: &Connection, game: &Game, size: u64, mtime: i64) -> rusqlite::Result<i64> {
    let path = game.path.to_string_lossy();
    conn.execute(
        "INSERT INTO games (path, size, mtime, error, start_at, last_frame, stage, mode, match_id,
             game_number, tiebreaker, is_teams, players, winners, lras, stocks_left, stats_version,
             fingerprint)
         VALUES (?1, ?2, ?3, NULL, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, '[]', 0, ?15)
         ON CONFLICT(path) DO UPDATE SET size = excluded.size, mtime = excluded.mtime, error = NULL,
             start_at = excluded.start_at, last_frame = excluded.last_frame, stage = excluded.stage,
             mode = excluded.mode, match_id = excluded.match_id, game_number = excluded.game_number,
             tiebreaker = excluded.tiebreaker, is_teams = excluded.is_teams, players = excluded.players,
             lras = excluded.lras,
             winners = CASE WHEN fingerprint IS excluded.fingerprint THEN winners ELSE excluded.winners END,
             stocks_left = CASE WHEN fingerprint IS excluded.fingerprint THEN stocks_left ELSE '[]' END,
             stats_version = CASE WHEN fingerprint IS excluded.fingerprint THEN stats_version ELSE 0 END,
             numbers = CASE WHEN fingerprint IS excluded.fingerprint THEN numbers ELSE '[]' END,
             fingerprint = excluded.fingerprint",
        params![
            path,
            size as i64,
            mtime,
            game.start_at,
            game.last_frame,
            game.stage as i64,
            game.mode.to_i64(),
            game.match_id,
            game.game_number as i64,
            game.tiebreaker as i64,
            game.is_teams,
            serde_json::to_string(&game.players).unwrap_or_else(|_| "[]".into()),
            serde_json::to_string(&game.winners).unwrap_or_else(|_| "[]".into()),
            game.lras.map(|p| p as i64),
            game.fingerprint,
        ],
    )?;
    conn.query_row("SELECT id FROM games WHERE path = ?1", [path], |row| {
        row.get(0)
    })
}

/// Remembers a file that couldn't be read, so it isn't read again until it
/// changes.
pub fn mark_unreadable(
    conn: &Connection,
    path: &Path,
    size: u64,
    mtime: i64,
    error: &str,
) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO games (path, size, mtime, error) VALUES (?1, ?2, ?3, ?4)
         ON CONFLICT(path) DO UPDATE SET size = excluded.size, mtime = excluded.mtime, error = excluded.error",
        params![path.to_string_lossy(), size as i64, mtime, error],
    )?;
    Ok(())
}

/// The copy of the game `fingerprint` the app shows.
pub fn primary_id(conn: &Connection, fingerprint: i64) -> rusqlite::Result<Option<i64>> {
    conn.query_row(
        "SELECT MIN(id) FROM games WHERE error IS NULL AND fingerprint = ?1",
        [fingerprint],
        |row| row.get(0),
    )
}

/// Indexed files that couldn't be read.
pub fn unreadable_count(conn: &Connection) -> rusqlite::Result<usize> {
    conn.query_row(
        "SELECT COUNT(*) FROM games WHERE error IS NOT NULL",
        [],
        |row| row.get::<_, i64>(0).map(|n| n as usize),
    )
}

/// Indexed files that are a copy of a game indexed from another file.
pub fn duplicate_count(conn: &Connection) -> rusqlite::Result<usize> {
    conn.query_row(
        &format!("SELECT COUNT(*) FROM games WHERE error IS NULL AND NOT {PRIMARY}"),
        [],
        |row| row.get::<_, i64>(0).map(|n| n as usize),
    )
}

/// The fingerprints of games `ids` (those that have one).
pub fn fingerprints(conn: &Connection, ids: &[i64]) -> rusqlite::Result<Vec<i64>> {
    let mut statement =
        conn.prepare("SELECT fingerprint FROM games WHERE id = ?1 AND fingerprint IS NOT NULL")?;
    let mut out = Vec::new();
    for id in ids {
        if let Some(f) = statement.query_row([id], |row| row.get(0)).optional()? {
            out.push(f);
        }
    }
    Ok(out)
}

pub fn remove_games(conn: &Connection, ids: &[i64]) -> rusqlite::Result<()> {
    let mut statement = conn.prepare("DELETE FROM games WHERE id = ?1")?;
    for id in ids {
        statement.execute([id])?;
    }
    Ok(())
}

/// Games whose stats are missing or from an older version, newest first.
pub fn games_needing_stats(
    conn: &Connection,
    version: u32,
) -> rusqlite::Result<Vec<(i64, PathBuf)>> {
    let mut statement = conn.prepare(&format!(
        "SELECT id, path FROM games WHERE error IS NULL AND stats_version != ?1 AND {PRIMARY}
             ORDER BY start_at DESC"
    ))?;
    let rows = statement.query_map([version as i64], |row| {
        Ok((row.get(0)?, PathBuf::from(row.get::<_, String>(1)?)))
    })?;
    rows.collect()
}

/// What the stats pass learnt about a game.
#[derive(Debug, Clone, PartialEq)]
pub struct StatsUpdate {
    pub id: i64,
    pub version: u32,
    pub winners: Vec<u8>,
    pub stocks_left: Vec<(u8, u8)>,
    pub numbers: Vec<crate::model::Numbers>,
    pub punishes: Vec<PunishRow>,
    pub kills: Vec<KillRow>,
    /// The stats as JSON; `None` when the frames couldn't be read (the game
    /// isn't tried again until the stats version changes).
    pub json: Option<String>,
}

/// A conversion, as Highlights queries it.
#[derive(Debug, Clone, PartialEq)]
pub struct PunishRow {
    pub attacker: Option<u8>,
    pub victim: u8,
    pub start_frame: i32,
    pub end_frame: Option<i32>,
    pub start_percent: f64,
    pub end_percent: f64,
    pub hits: u32,
    pub did_kill: bool,
    pub opening: String,
    pub first_move: Option<u8>,
    pub last_move: Option<u8>,
}

/// A stock lost.
#[derive(Debug, Clone, PartialEq)]
pub struct KillRow {
    pub victim: u8,
    pub killer: Option<u8>,
    pub stock: u8,
    pub frame: Option<i32>,
    pub percent: Option<f64>,
    pub move_id: Option<u8>,
}

pub fn save_stats(conn: &Connection, updates: &[StatsUpdate]) -> rusqlite::Result<()> {
    let tx = conn.unchecked_transaction()?;
    {
        let mut game = tx.prepare(
            // Stats that can't name a winner (quit-outs slippi-js leaves
            // open) keep the one the game end event named.
            "UPDATE games SET winners = CASE WHEN ?2 = '[]' THEN winners ELSE ?2 END,
                 stocks_left = ?3, stats_version = ?4, numbers = ?5 WHERE id = ?1",
        )?;
        let mut clear_punishes = tx.prepare("DELETE FROM punishes WHERE game_id = ?1")?;
        let mut clear_kills = tx.prepare("DELETE FROM kills WHERE game_id = ?1")?;
        let mut punish = tx.prepare(
            "INSERT INTO punishes (game_id, attacker, victim, start_frame, end_frame, start_percent,
                 end_percent, hits, did_kill, opening, first_move, last_move)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
        )?;
        let mut kill = tx.prepare(
            "INSERT INTO kills (game_id, victim, killer, stock, frame, percent, move_id)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        )?;
        let mut stats = tx.prepare(
            "INSERT INTO game_stats (game_id, version, json) VALUES (?1, ?2, ?3)
             ON CONFLICT(game_id) DO UPDATE SET version = excluded.version, json = excluded.json",
        )?;
        for update in updates {
            game.execute(params![
                update.id,
                serde_json::to_string(&update.winners).unwrap_or_else(|_| "[]".into()),
                serde_json::to_string(&update.stocks_left).unwrap_or_else(|_| "[]".into()),
                update.version as i64,
                serde_json::to_string(&update.numbers).unwrap_or_else(|_| "[]".into()),
            ])?;
            clear_punishes.execute([update.id])?;
            clear_kills.execute([update.id])?;
            for p in &update.punishes {
                punish.execute(params![
                    update.id,
                    p.attacker.map(i64::from),
                    p.victim as i64,
                    p.start_frame,
                    p.end_frame,
                    p.start_percent,
                    p.end_percent,
                    p.hits as i64,
                    p.did_kill,
                    p.opening,
                    p.first_move.map(i64::from),
                    p.last_move.map(i64::from),
                ])?;
            }
            for k in &update.kills {
                kill.execute(params![
                    update.id,
                    k.victim as i64,
                    k.killer.map(i64::from),
                    k.stock as i64,
                    k.frame,
                    k.percent,
                    k.move_id.map(i64::from),
                ])?;
            }
            if let Some(json) = &update.json {
                stats.execute(params![update.id, update.version as i64, json])?;
            }
        }
    }
    tx.commit()
}

pub fn load_stats_json(conn: &Connection, id: i64) -> rusqlite::Result<Option<String>> {
    conn.query_row(
        "SELECT json FROM game_stats WHERE game_id = ?1",
        [id],
        |row| row.get(0),
    )
    .optional()
}

/// One rank lookup, as stored.
#[derive(Debug, Clone, PartialEq)]
pub struct RankRecord {
    pub code: String,
    pub fetched_at: i64,
    pub found: bool,
    pub display_name: Option<String>,
    pub rating: Option<f64>,
    /// The rank's name ("Diamond 2"), or `None` without a ranked profile.
    pub rank: Option<String>,
    pub sets: Option<i64>,
    pub global_placement: Option<i64>,
    pub regional_placement: Option<i64>,
    /// Anything else the lookup returned, as JSON.
    pub extra: Option<String>,
}

pub fn save_rank(conn: &Connection, record: &RankRecord) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT OR REPLACE INTO ranks (code, fetched_at, found, display_name, rating, rank, sets,
             global_placement, regional_placement, extra)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
        params![
            record.code,
            record.fetched_at,
            record.found,
            record.display_name,
            record.rating,
            record.rank,
            record.sets,
            record.global_placement,
            record.regional_placement,
            record.extra,
        ],
    )?;
    Ok(())
}

fn rank_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<RankRecord> {
    Ok(RankRecord {
        code: row.get(0)?,
        fetched_at: row.get(1)?,
        found: row.get(2)?,
        display_name: row.get(3)?,
        rating: row.get(4)?,
        rank: row.get(5)?,
        sets: row.get(6)?,
        global_placement: row.get(7)?,
        regional_placement: row.get(8)?,
        extra: row.get(9)?,
    })
}

const RANK_COLUMNS: &str = "code, fetched_at, found, display_name, rating, rank, sets, global_placement, regional_placement, extra";

/// The latest lookup of every code.
pub fn latest_ranks(conn: &Connection) -> rusqlite::Result<HashMap<String, RankRecord>> {
    let mut statement = conn.prepare(&format!(
        "SELECT {RANK_COLUMNS} FROM ranks r WHERE fetched_at = (SELECT MAX(fetched_at) FROM ranks WHERE code = r.code)"
    ))?;
    let rows = statement.query_map([], rank_from_row)?;
    rows.map(|r| r.map(|record| (record.code.clone(), record)))
        .collect()
}

/// Every lookup of `code`, newest first.
pub fn rank_history(conn: &Connection, code: &str) -> rusqlite::Result<Vec<RankRecord>> {
    let mut statement = conn.prepare(&format!(
        "SELECT {RANK_COLUMNS} FROM ranks WHERE code = ?1 ORDER BY fetched_at DESC"
    ))?;
    let rows = statement.query_map([code], rank_from_row)?;
    rows.collect()
}

/// Stores the seasons `code` played in (as fetched at `now`).
pub fn save_seasons(
    conn: &Connection,
    code: &str,
    seasons: &[crate::model::Season],
    now: i64,
) -> rusqlite::Result<()> {
    let tx = conn.unchecked_transaction()?;
    {
        let mut statement = tx.prepare(
            "INSERT OR REPLACE INTO seasons (code, id, fetched_at, json) VALUES (?1, ?2, ?3, ?4)",
        )?;
        for season in seasons {
            let json = serde_json::to_string(season).unwrap_or_default();
            statement.execute(params![code, season.id, now, json])?;
        }
    }
    tx.commit()
}

/// Every player's stored seasons and when they were fetched (a player
/// with no account has none).
pub fn load_histories(
    conn: &Connection,
) -> rusqlite::Result<HashMap<String, (Vec<crate::model::Season>, i64)>> {
    let mut statement = conn.prepare("SELECT code, json, fetched_at FROM seasons")?;
    let rows = statement.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, i64>(2)?,
        ))
    })?;
    let mut out: HashMap<String, (Vec<crate::model::Season>, i64)> = HashMap::new();
    for (code, json, at) in rows.flatten() {
        let entry = out.entry(code).or_default();
        entry.1 = entry.1.max(at);
        if let Ok(season) = serde_json::from_str::<crate::model::Season>(&json)
            && !season.id.is_empty()
        {
            entry.0.push(season);
        }
    }
    Ok(out)
}

/// Remembers that no account has `code` (so its seasons aren't asked for
/// again until the next season).
pub fn save_no_account(conn: &Connection, code: &str, now: i64) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT OR REPLACE INTO seasons (code, id, fetched_at, json) VALUES (?1, '', ?2, '{}')",
        params![code, now],
    )?;
    Ok(())
}

/// The seasons stored for `code`, newest first, and when they were fetched.
pub fn load_seasons(
    conn: &Connection,
    code: &str,
) -> rusqlite::Result<(Vec<crate::model::Season>, i64)> {
    let mut statement = conn.prepare("SELECT json, fetched_at FROM seasons WHERE code = ?1")?;
    let rows = statement.query_map([code], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
    })?;
    let mut seasons = Vec::new();
    let mut fetched = 0;
    for (json, at) in rows.flatten() {
        fetched = fetched.max(at);
        if let Ok(season) = serde_json::from_str::<crate::model::Season>(&json)
            && !season.id.is_empty()
        {
            seasons.push(season);
        }
    }
    seasons.sort_by_key(|s| std::cmp::Reverse(s.start));
    Ok((seasons, fetched))
}

pub fn load_notes(conn: &Connection) -> rusqlite::Result<HashMap<String, String>> {
    let mut statement = conn.prepare("SELECT code, text FROM notes")?;
    let rows = statement.query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?;
    rows.collect()
}

pub fn save_note(conn: &Connection, code: &str, text: &str, now: i64) -> rusqlite::Result<()> {
    if text.trim().is_empty() {
        conn.execute("DELETE FROM notes WHERE code = ?1", [code])?;
    } else {
        conn.execute(
            "INSERT INTO notes (code, text, updated_at) VALUES (?1, ?2, ?3)
             ON CONFLICT(code) DO UPDATE SET text = excluded.text, updated_at = excluded.updated_at",
            params![code, text, now],
        )?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_db() -> (tempfile::TempDir, Connection) {
        let dir = tempfile::tempdir().unwrap();
        let conn = open(&dir.path().join("eppi.sqlite")).unwrap();
        (dir, conn)
    }

    #[test]
    fn games_round_trip_and_stats_update_them() {
        let (_dir, conn) = temp_db();
        let game = Game {
            path: PathBuf::from("C:/replays/Game_1.slp"),
            start_at: 100,
            last_frame: 5000,
            stage: 31,
            mode: Mode::Ranked,
            match_id: "mode.ranked-x".into(),
            players: vec![Player {
                port: 0,
                character: 2,
                code: "BEAN#888".into(),
                ..Default::default()
            }],
            winners: vec![0],
            ..Default::default()
        };
        let id = upsert_game(&conn, &game, 10, 20).unwrap();
        let again = upsert_game(&conn, &game, 11, 21).unwrap();
        assert_eq!(id, again, "the same path keeps its id");
        let loaded = load_games(&conn).unwrap();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].players, game.players);
        assert_eq!(loaded[0].mode, Mode::Ranked);
        assert_eq!(games_needing_stats(&conn, 3).unwrap().len(), 1);
        save_stats(
            &conn,
            &[StatsUpdate {
                id,
                version: 3,
                winners: vec![1],
                stocks_left: vec![(1, 2)],
                numbers: Vec::new(),
                punishes: Vec::new(),
                kills: Vec::new(),
                json: Some("{}".into()),
            }],
        )
        .unwrap();
        let loaded = load_game(&conn, id).unwrap().unwrap();
        assert_eq!(loaded.winners, vec![1]);
        assert_eq!(loaded.stocks_left, vec![(1, 2)]);
        assert!(games_needing_stats(&conn, 3).unwrap().is_empty());
        assert_eq!(load_stats_json(&conn, id).unwrap().as_deref(), Some("{}"));
        mark_unreadable(&conn, Path::new("C:/replays/bad.slp"), 1, 1, "corrupt").unwrap();
        assert_eq!(
            load_games(&conn).unwrap().len(),
            1,
            "unreadable files aren't games"
        );
        assert_eq!(file_states(&conn).unwrap().len(), 2);
    }

    #[test]
    fn ranks_keep_their_history() {
        let (_dir, conn) = temp_db();
        for (at, rating) in [(1, 1500.0), (2, 1600.0)] {
            save_rank(
                &conn,
                &RankRecord {
                    code: "ABC#123".into(),
                    fetched_at: at,
                    found: true,
                    display_name: Some("abc".into()),
                    rating: Some(rating),
                    rank: Some("Gold 2".into()),
                    sets: Some(10),
                    global_placement: None,
                    regional_placement: None,
                    extra: None,
                },
            )
            .unwrap();
        }
        assert_eq!(latest_ranks(&conn).unwrap()["ABC#123"].rating, Some(1600.0));
        assert_eq!(rank_history(&conn, "ABC#123").unwrap().len(), 2);
        save_note(&conn, "ABC#123", "camps platforms", 5).unwrap();
        assert_eq!(load_notes(&conn).unwrap()["ABC#123"], "camps platforms");
        save_note(&conn, "ABC#123", "  ", 6).unwrap();
        assert!(load_notes(&conn).unwrap().is_empty());
    }
}
