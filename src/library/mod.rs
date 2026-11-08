//! The replay library: a worker thread that keeps the SQLite index in step
//! with the replay folders and tells the UI what changed.
//!
//! - On start it sends every indexed game (no parsing), then walks the
//!   folders and reads only new or changed files (frames skipped), a few
//!   hundred at a time on rayon.
//! - A second pass computes full stats for games that have none (or older
//!   ones), newest first, in small chunks so new work can cut in.
//! - A folder watcher picks up new replays. A file Slippi is still writing is
//!   a live game: its players are read from the game start event right away,
//!   and the game is indexed (and its stats computed) when it ends.

pub mod db;
pub mod read;
pub mod stats;

use crate::model::Game;
use db::StatsUpdate;
use rayon::prelude::*;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::time::{Duration, Instant};

/// What the UI asks of the worker.
#[derive(Debug)]
pub enum Command {
    /// Index these folders (the settings changed, or Refresh).
    Scan(Vec<PathBuf>),
    /// Something changed on disk.
    Fs(notify::Event),
    Shutdown,
}

/// What the worker tells the UI.
#[derive(Debug)]
pub enum Event {
    /// Every indexed game, newest first (once, at start).
    Loaded(Vec<Game>),
    /// Games read or read again.
    Upserted(Vec<Game>),
    Removed(Vec<i64>),
    /// Results of the stats pass.
    Stats(Vec<StatsUpdate>),
    Progress(Progress),
    /// A game started (its replay is being written).
    LiveStarted(Game),
    /// That game ended (or its file went away).
    LiveEnded(PathBuf),
    /// The folders read, each with how many replay files it holds.
    Folders(Vec<(PathBuf, usize)>),
    Error(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Phase {
    #[default]
    Idle,
    Listing,
    Reading,
    Stats,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Progress {
    pub phase: Phase,
    pub done: usize,
    pub total: usize,
    /// Files that couldn't be read in the last scan.
    pub unreadable: usize,
    /// Files that are a copy of a game read from another file.
    pub duplicates: usize,
}

pub struct Library {
    pub commands: Sender<Command>,
    pub events: Receiver<Event>,
    _watcher: Option<notify::RecommendedWatcher>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl Library {
    pub fn start(
        db_path: PathBuf,
        folders: Vec<PathBuf>,
        repaint: impl Fn() + Send + 'static,
    ) -> Self {
        let folders = crate::folders::tidy(&folders);
        let (command_tx, command_rx) = mpsc::channel();
        let (event_tx, event_rx) = mpsc::channel();
        let watcher = watch(&folders, command_tx.clone());
        let thread = std::thread::Builder::new()
            .name("eppi-library".into())
            .spawn(move || {
                let worker = Worker {
                    db_path,
                    commands: command_rx,
                    events: event_tx,
                    repaint: Box::new(repaint),
                };
                worker.run(folders);
            })
            .expect("start the library thread");
        Self {
            commands: command_tx,
            events: event_rx,
            _watcher: watcher,
            thread: Some(thread),
        }
    }

    /// Watches `folders` instead of the ones before, and indexes them.
    pub fn set_folders(&mut self, folders: Vec<PathBuf>) {
        let folders = crate::folders::tidy(&folders);
        self._watcher = watch(&folders, self.commands.clone());
        let _ = self.commands.send(Command::Scan(folders));
    }
}

impl Drop for Library {
    fn drop(&mut self) {
        let _ = self.commands.send(Command::Shutdown);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn watch(folders: &[PathBuf], commands: Sender<Command>) -> Option<notify::RecommendedWatcher> {
    use notify::Watcher;
    let mut watcher = notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
        if let Ok(event) = event {
            let _ = commands.send(Command::Fs(event));
        }
    })
    .map_err(|e| log::warn!("no folder watcher: {e}"))
    .ok()?;
    for folder in folders {
        if let Err(e) = watcher.watch(folder, notify::RecursiveMode::Recursive) {
            log::warn!("can't watch {}: {e}", folder.display());
        }
    }
    Some(watcher)
}

/// A file Slippi is writing.
struct Live {
    announced: bool,
    since: Instant,
}

struct Worker {
    db_path: PathBuf,
    commands: Receiver<Command>,
    events: Sender<Event>,
    repaint: Box<dyn Fn() + Send>,
}

/// Games read per batch (sent to the UI as they finish).
const READ_BATCH: usize = 400;
/// Games per stats chunk (new work can cut in between chunks).
const STATS_BATCH: usize = 48;

impl Worker {
    fn send(&self, event: Event) {
        let _ = self.events.send(event);
        (self.repaint)();
    }

    fn run(self, folders: Vec<PathBuf>) {
        let conn = match db::open(&self.db_path) {
            Ok(conn) => conn,
            Err(e) => {
                self.send(Event::Error(format!(
                    "Couldn't open the index at {}: {e}",
                    self.db_path.display()
                )));
                return;
            }
        };
        match db::load_games(&conn) {
            Ok(games) => self.send(Event::Loaded(games)),
            Err(e) => self.send(Event::Error(format!("Couldn't read the index: {e}"))),
        }

        let threads = std::thread::available_parallelism()
            .map_or(4, |n| n.get())
            .saturating_sub(1)
            .max(1);
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(threads)
            .thread_name(|i| format!("eppi-read-{i}"))
            .build()
            .expect("build the reading pool");

        let mut folders = folders;
        let mut scan_wanted = true;
        let mut stats_queue: Vec<(i64, PathBuf)> = Vec::new();
        let mut stats_total = 0;
        let mut stats_done = 0;
        let mut counts = Progress::default();
        let mut live: HashMap<PathBuf, Live> = HashMap::new();
        let mut touched: HashSet<PathBuf> = HashSet::new();
        let mut last_tick = Instant::now();

        loop {
            let busy = scan_wanted || !stats_queue.is_empty();
            let command = if busy {
                match self.commands.try_recv() {
                    Ok(c) => Some(c),
                    Err(mpsc::TryRecvError::Empty) => None,
                    Err(mpsc::TryRecvError::Disconnected) => return,
                }
            } else {
                match self.commands.recv_timeout(Duration::from_millis(700)) {
                    Ok(c) => Some(c),
                    Err(RecvTimeoutError::Timeout) => None,
                    Err(RecvTimeoutError::Disconnected) => return,
                }
            };
            let had_command = command.is_some();
            match command {
                Some(Command::Shutdown) => return,
                Some(Command::Scan(new_folders)) => {
                    folders = crate::folders::tidy(&new_folders);
                    scan_wanted = true;
                }
                Some(Command::Fs(event)) => {
                    for path in event.paths {
                        if is_replay(&path) {
                            touched.insert(path);
                        }
                    }
                }
                None => {}
            }
            // Drain what else is waiting before doing work.
            if had_command {
                continue;
            }

            if !touched.is_empty() || last_tick.elapsed() >= Duration::from_millis(700) {
                last_tick = Instant::now();
                let paths: Vec<PathBuf> = touched.drain().collect();
                self.check_files(&conn, paths, &mut live, &mut stats_queue);
            }

            if scan_wanted {
                scan_wanted = false;
                counts = self.scan(&conn, &pool, &folders);
                stats_queue = db::games_needing_stats(&conn, stats::VERSION).unwrap_or_default();
                stats_queue.reverse(); // pop() takes the newest first
                stats_total = stats_queue.len();
                stats_done = 0;
                continue;
            }

            if !stats_queue.is_empty() {
                let take = STATS_BATCH.min(stats_queue.len());
                let chunk: Vec<(i64, PathBuf)> = stats_queue.split_off(stats_queue.len() - take);
                let updates: Vec<StatsUpdate> = pool.install(|| {
                    chunk
                        .par_iter()
                        .map(|(id, path)| stats::compute(*id, path))
                        .collect()
                });
                if let Err(e) = db::save_stats(&conn, &updates) {
                    self.send(Event::Error(format!("Couldn't save stats: {e}")));
                }
                stats_done += updates.len();
                self.send(Event::Stats(updates));
                let phase = if stats_queue.is_empty() {
                    Phase::Idle
                } else {
                    Phase::Stats
                };
                self.send(Event::Progress(Progress {
                    phase,
                    done: stats_done,
                    total: stats_total,
                    ..counts
                }));
            }
        }
    }

    /// Indexes new and changed replays under `folders` and forgets ones that
    /// went away; returns the files it couldn't read and the duplicates.
    fn scan(
        &self,
        conn: &rusqlite::Connection,
        pool: &rayon::ThreadPool,
        folders: &[PathBuf],
    ) -> Progress {
        self.send(Event::Progress(Progress {
            phase: Phase::Listing,
            ..Default::default()
        }));
        let mut on_disk = Vec::new();
        let mut seen = HashSet::new();
        let mut per_folder = Vec::new();
        for folder in folders {
            let mut files = Vec::new();
            list_replays(folder, &mut files);
            per_folder.push((folder.clone(), files.len()));
            // A file reached twice (a link inside a folder) is read once.
            on_disk.extend(files.into_iter().filter(|(p, _, _)| seen.insert(p.clone())));
        }
        self.send(Event::Folders(per_folder));
        let known = db::file_states(conn).unwrap_or_default();
        let on_disk_set: HashSet<&PathBuf> = on_disk.iter().map(|(p, _, _)| p).collect();
        // Forget files that went away, and files of folders no longer in the
        // settings, but not those of a folder that is only missing for now
        // (an unplugged drive): they come back with it.
        let gone: Vec<i64> = known
            .iter()
            .filter(|(path, _)| !on_disk_set.contains(path))
            .filter(
                |(path, _)| match folders.iter().find(|f| path.starts_with(f)) {
                    Some(folder) => folder.is_dir(),
                    None => true,
                },
            )
            .map(|(_, state)| state.id)
            .collect();
        if !gone.is_empty() {
            let fingerprints = db::fingerprints(conn, &gone).unwrap_or_default();
            if let Err(e) = db::remove_games(conn, &gone) {
                self.send(Event::Error(format!("Couldn't update the index: {e}")));
            }
            self.send(Event::Removed(gone));
            // A game whose shown copy went away is shown from another copy.
            let promoted: Vec<Game> = fingerprints
                .into_iter()
                .filter_map(|f| db::primary_id(conn, f).ok().flatten())
                .filter_map(|id| db::load_game(conn, id).ok().flatten())
                .collect();
            if !promoted.is_empty() {
                self.send(Event::Upserted(promoted));
            }
        }
        let todo: Vec<(PathBuf, u64, i64)> = on_disk
            .into_iter()
            .filter(|(path, size, mtime)| {
                known
                    .get(path)
                    .is_none_or(|state| state.size != *size || state.mtime != *mtime)
            })
            .filter(|(path, _, _)| read::is_finished(path))
            .collect();
        let total = todo.len();
        let mut done = 0;
        let mut unreadable = 0;
        for batch in todo.chunks(READ_BATCH) {
            let results: Vec<(&PathBuf, u64, i64, Result<Game, String>)> = pool.install(|| {
                batch
                    .par_iter()
                    .map(|(path, size, mtime)| (path, *size, *mtime, read::read_summary(path)))
                    .collect()
            });
            let mut games = Vec::with_capacity(results.len());
            let tx = conn.unchecked_transaction().ok();
            for (path, size, mtime, result) in results {
                match result {
                    Ok(game) => match db::upsert_game(conn, &game, size, mtime) {
                        Ok(id) => {
                            if let Some(game) = shown_copy(conn, id, game.fingerprint) {
                                games.push(game);
                            }
                        }
                        Err(e) => log::warn!("index {}: {e}", path.display()),
                    },
                    Err(error) => {
                        unreadable += 1;
                        log::info!("unreadable {}: {error}", path.display());
                        let _ = db::mark_unreadable(conn, path, size, mtime, &error);
                    }
                }
            }
            if let Some(tx) = tx {
                let _ = tx.commit();
            }
            done += batch.len();
            self.send(Event::Upserted(games));
            self.send(Event::Progress(Progress {
                phase: Phase::Reading,
                done,
                total,
                unreadable,
                duplicates: 0,
            }));
            // Leave the loop if the app is closing.
            if let Ok(Command::Shutdown) = self.commands.try_recv() {
                return Progress::default();
            }
        }
        let counts = Progress {
            phase: Phase::Idle,
            done,
            total,
            unreadable: db::unreadable_count(conn).unwrap_or(unreadable),
            duplicates: db::duplicate_count(conn).unwrap_or_default(),
        };
        self.send(Event::Progress(counts));
        counts
    }

    /// Follows files the watcher saw: live games starting and ending, and
    /// finished replays copied in.
    fn check_files(
        &self,
        conn: &rusqlite::Connection,
        touched: Vec<PathBuf>,
        live: &mut HashMap<PathBuf, Live>,
        stats_queue: &mut Vec<(i64, PathBuf)>,
    ) {
        for path in touched {
            live.entry(path).or_insert(Live {
                announced: false,
                since: Instant::now(),
            });
        }
        let paths: Vec<PathBuf> = live.keys().cloned().collect();
        for path in paths {
            let Ok(meta) = std::fs::metadata(&path) else {
                if live.remove(&path).is_some_and(|l| l.announced) {
                    self.send(Event::LiveEnded(path));
                }
                continue;
            };
            if read::is_finished(&path) {
                let entry = live.remove(&path);
                let (size, mtime) = size_mtime(&meta);
                let unchanged = db::file_state(conn, &path)
                    .ok()
                    .flatten()
                    .is_some_and(|known| known.size == size && known.mtime == mtime);
                if unchanged {
                    if entry.is_some_and(|l| l.announced) {
                        self.send(Event::LiveEnded(path));
                    }
                    continue;
                }
                match read::read_summary(&path) {
                    Ok(game) => {
                        let (size, mtime) = size_mtime(&meta);
                        if let Ok(id) = db::upsert_game(conn, &game, size, mtime)
                            && let Some(game) = shown_copy(conn, id, game.fingerprint)
                        {
                            if game.stats_version != stats::VERSION {
                                stats_queue.push((id, path.clone()));
                            }
                            self.send(Event::Upserted(vec![game]));
                        }
                    }
                    Err(error) => {
                        let (size, mtime) = size_mtime(&meta);
                        let _ = db::mark_unreadable(conn, &path, size, mtime, &error);
                    }
                }
                if entry.is_some_and(|l| l.announced) {
                    self.send(Event::LiveEnded(path));
                }
                continue;
            }
            let Some(state) = live.get_mut(&path) else {
                continue;
            };
            if !state.announced {
                match read::read_live(&path) {
                    Ok(game) => {
                        state.announced = true;
                        self.send(Event::LiveStarted(game));
                    }
                    Err(_) if state.since.elapsed() > Duration::from_secs(30) => {
                        live.remove(&path);
                    }
                    Err(_) => {}
                }
            } else if state.since.elapsed() > Duration::from_secs(60 * 30) {
                // Dolphin closed mid-game: the file never finishes.
                live.remove(&path);
                self.send(Event::LiveEnded(path));
            }
        }
    }
}

/// The game just stored as `id`, as the app shows it, unless it is a copy
/// of a game already shown from another file.
fn shown_copy(conn: &rusqlite::Connection, id: i64, fingerprint: i64) -> Option<Game> {
    if db::primary_id(conn, fingerprint).ok().flatten() != Some(id) {
        return None;
    }
    db::load_game(conn, id).ok().flatten()
}

fn is_replay(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("slp"))
}

fn size_mtime(meta: &std::fs::Metadata) -> (u64, i64) {
    let mtime = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map_or(0, |d| d.as_secs() as i64);
    (meta.len(), mtime)
}

/// Every `.slp` under `folder` with its size and modified time.
fn list_replays(folder: &Path, out: &mut Vec<(PathBuf, u64, i64)>) {
    let Ok(entries) = std::fs::read_dir(folder) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if file_type.is_dir() {
            list_replays(&path, out);
        } else if is_replay(&path)
            && let Ok(meta) = entry.metadata()
        {
            let (size, mtime) = size_mtime(&meta);
            out.push((path, size, mtime));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A real replay to copy from (the owner's folder), if this machine
    /// has one; the test is skipped where it doesn't (CI).
    fn sample_replay() -> Option<PathBuf> {
        let folder = dirs::document_dir()?.join("Slippi");
        let mut files = Vec::new();
        list_replays(&folder, &mut files);
        files.sort_by_key(|(p, _, _)| std::cmp::Reverse(p.clone()));
        files
            .into_iter()
            .map(|(p, size, _)| (p, size))
            .find(|(p, size)| {
                *size > 200_000 && read::read_summary(p).is_ok_and(|g| g.is_singles())
            })
            .map(|(p, _)| p)
    }

    fn wait_for(events: &Receiver<Event>, what: &str, mut want: impl FnMut(&Event) -> bool) {
        let deadline = Instant::now() + Duration::from_secs(20);
        while Instant::now() < deadline {
            if let Ok(event) = events.recv_timeout(Duration::from_millis(200))
                && want(&event)
            {
                return;
            }
        }
        panic!("no {what} within 20 s");
    }

    #[test]
    fn a_game_being_recorded_is_live_until_it_ends_then_indexed() {
        let Some(source) = sample_replay() else {
            eprintln!("skipped: no replays on this machine");
            return;
        };
        let bytes = std::fs::read(&source).unwrap();
        let dir = tempfile::tempdir().unwrap();
        let folder = dir.path().join("replays");
        std::fs::create_dir_all(&folder).unwrap();
        let library = Library::start(dir.path().join("eppi.sqlite"), vec![folder.clone()], || {});
        wait_for(
            &library.events,
            "first scan",
            |e| matches!(e, Event::Progress(p) if p.phase == Phase::Idle),
        );

        // Slippi writes the raw block's length only when the game ends.
        let target = folder.join("Game_20991231T235959.slp");
        let mut partial = bytes[..60_000].to_vec();
        partial[11..15].copy_from_slice(&[0, 0, 0, 0]);
        std::fs::write(&target, &partial).unwrap();
        wait_for(
            &library.events,
            "the live game",
            |e| matches!(e, Event::LiveStarted(g) if g.path == target && g.players.len() == 2),
        );

        std::fs::write(&target, &bytes).unwrap();
        let mut ended = false;
        let mut indexed = false;
        wait_for(&library.events, "the finished game", |e| {
            match e {
                Event::LiveEnded(p) if *p == target => ended = true,
                Event::Upserted(games) if games.iter().any(|g| g.path == target) => indexed = true,
                _ => {}
            }
            ended && indexed
        });
        wait_for(
            &library.events,
            "its stats",
            |e| matches!(e, Event::Stats(s) if !s.is_empty()),
        );
    }

    fn games_after_scan(library: &Library) -> (usize, Progress) {
        let mut shown: HashSet<i64> = HashSet::new();
        let deadline = Instant::now() + Duration::from_secs(30);
        while Instant::now() < deadline {
            match library.events.recv_timeout(Duration::from_millis(200)) {
                Ok(Event::Loaded(games) | Event::Upserted(games)) => {
                    shown.extend(games.iter().map(|g| g.id))
                }
                Ok(Event::Removed(ids)) => ids.iter().for_each(|id| {
                    shown.remove(id);
                }),
                Ok(Event::Progress(p)) if p.phase == Phase::Idle && p.total > 0 => {
                    return (shown.len(), p);
                }
                _ => {}
            }
        }
        panic!("the scan didn't finish");
    }

    #[test]
    fn copies_in_several_folders_and_nested_folders_count_once() {
        let Some(source) = sample_replay() else {
            eprintln!("skipped: no replays on this machine");
            return;
        };
        let dir = tempfile::tempdir().unwrap();
        let (a, b) = (dir.path().join("a"), dir.path().join("b"));
        std::fs::create_dir_all(a.join("sub")).unwrap();
        std::fs::create_dir_all(&b).unwrap();
        std::fs::copy(&source, a.join("sub").join("Game_1.slp")).unwrap();
        std::fs::copy(&source, b.join("Game_1 (copy).slp")).unwrap();
        let folders = vec![a.join("sub"), a.clone(), b.clone()];
        let db_path = dir.path().join("eppi.sqlite");
        {
            let library = Library::start(db_path.clone(), folders.clone(), || {});
            let (shown, progress) = games_after_scan(&library);
            assert_eq!(shown, 1, "two copies, one game");
            assert_eq!(progress.duplicates, 1);
        }
        // The shown copy goes away: the other one takes its place.
        std::fs::remove_file(a.join("sub").join("Game_1.slp")).unwrap();
        let library = Library::start(db_path, folders, || {});
        let (shown, progress) = games_after_scan(&library);
        assert_eq!(shown, 1);
        assert_eq!(progress.duplicates, 0);
    }
}
