use peppi::game::immutable::Game;
use peppi::game::Port;
use peppi::io::slippi;
use rayon::prelude::*;
use rayon::slice::ParallelSliceMut;
use std::collections::HashMap;
use std::fs;
use std::io;
use std::panic;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::SystemTime;
use walkdir::WalkDir;

// Re-export web-related helpers so existing code (e.g. in `app.rs`) keeps compiling
pub use crate::web::fetch_player_rank;

#[derive(Debug, Clone)]
pub struct ReplayInfo {
    pub player1: PlayerInfo,
    pub player2: PlayerInfo,
    pub result: GameResult,
    pub stage_name: String,
    pub duration: Option<i32>,
    pub date: Option<SystemTime>,
    pub opponent_rank: Option<String>,
}

#[derive(Debug, Clone)]
pub struct PlayerInfo {
    pub name: String,
}

#[derive(Debug, Clone)]
pub enum GameResult {
    Player1Won,
    Player2Won,
    Unknown,
}

pub struct ReplayAnalyzer {
    pub replays: Vec<ReplayInfo>,
    pub rank_cache: HashMap<String, String>, // Cache for player tag -> rank
}

impl ReplayAnalyzer {
    pub fn new() -> Self {
        Self {
            replays: Vec::new(),
            rank_cache: HashMap::new(),
        }
    }

    pub fn scan_directory(&mut self, dir_path: &str) -> io::Result<()> {
        // Cache directory inside OS data dir (e.g. %APPDATA%/eppi)
        let cache_dir = dirs::data_local_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("eppi");
        let cache_path = cache_dir.join("bad_replays.txt");

        // Load bad-file cache if it exists
        let mut bad_cache: std::collections::HashSet<String> =
            if let Ok(contents) = fs::read_to_string(&cache_path) {
                contents
                    .lines()
                    .map(|l| l.trim())
                    .filter(|l| !l.is_empty())
                    .map(|l| l.to_owned())
                    .collect()
            } else {
                std::collections::HashSet::new()
            };

        // Install a silent panic hook once to suppress per-file panic prints
        static HOOK_SET: std::sync::Once = std::sync::Once::new();
        HOOK_SET.call_once(|| {
            let _ = panic::take_hook(); // drop the default that prints
            panic::set_hook(Box::new(|_| {}));
        });

        // First, collect all .slp files, skipping those known to be bad
        let slp_files: Vec<_> = WalkDir::new(dir_path)
            .into_iter()
            .filter_map(|e| {
                if let Ok(entry) = e {
                    if entry.path().is_file()
                        && entry.path().extension().and_then(|s| s.to_str()) == Some("slp")
                        && !bad_cache.contains(entry.path().to_string_lossy().as_ref())
                    {
                        Some(entry.path().to_path_buf())
                    } else {
                        None
                    }
                } else {
                    None
                }
            })
            .collect();

        log::info!("Found {} .slp files to process", slp_files.len());

        // Build a rayon pool with physical core count to avoid hyper-thread oversubscription
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(num_cpus::get_physical())
            .build()
            .map_err(|e| io::Error::other(format!("Thread-pool error: {e}")))?;

        let new_bad: Mutex<Vec<String>> = Mutex::new(Vec::new());

        let mut replays: Vec<ReplayInfo> = pool.install(|| {
            slp_files
                .into_par_iter()
                .filter_map(|path| {
                    let file_path = path.to_str()?.to_string();

                    // Use catch_unwind to handle panics from corrupt replay files
                    let result = panic::catch_unwind(|| parse_replay(&file_path));

                    match result {
                        Ok(Ok(replay_info)) => Some(replay_info),
                        _ => {
                            if let Ok(mut vec) = new_bad.lock() {
                                vec.push(file_path.clone());
                            }
                            None
                        }
                    }
                })
                .collect()
        });

        let skipped_count = new_bad.lock().map(|v| v.len()).unwrap_or(0);
        log::info!(
            "Successfully parsed {} replays (skipped {skipped_count})",
            replays.len()
        );

        // Sort by date (newest first) in parallel
        replays.par_sort_unstable_by(|a, b| {
            match (a.date, b.date) {
                (Some(date_a), Some(date_b)) => date_b.cmp(&date_a), // Newer first
                (Some(_), None) => std::cmp::Ordering::Less,         // Files with dates come first
                (None, Some(_)) => std::cmp::Ordering::Greater, // Files without dates come last
                (None, None) => std::cmp::Ordering::Equal,      // Equal if both have no date
            }
        });

        self.replays = replays;

        let new_bad_vec = new_bad.into_inner().unwrap_or_default();

        if !new_bad_vec.is_empty() {
            // Ensure cache dir exists
            if let Err(e) = fs::create_dir_all(&cache_dir) {
                log::error!("Failed to create cache directory {cache_dir:?}: {e}");
            }
            for p in new_bad_vec {
                bad_cache.insert(p);
            }
            if let Some(parent) = cache_path.parent() {
                if !parent.exists() {
                    log::warn!("Parent directory {parent:?} does NOT exist – creating it");
                    if let Err(e) = fs::create_dir_all(parent) {
                        log::error!("Failed to create parent directory {parent:?}: {e}");
                    }
                }
            }
            let data = bad_cache.into_iter().collect::<Vec<_>>().join("\n");
            log::info!("Caching {skipped_count} bad replay paths to {cache_path:?}");
            if let Err(e) = fs::write(&cache_path, data) {
                log::error!("Failed to update bad replay cache at {cache_path:?}: {e}");
            }
        }

        Ok(())
    }

    pub fn get_stats_for_player(&self, player_tag: &str) -> (usize, usize) {
        let mut wins = 0;
        let mut losses = 0;

        for replay in &self.replays {
            if replay.player1.name == player_tag {
                match replay.result {
                    GameResult::Player1Won => wins += 1,
                    GameResult::Player2Won => losses += 1,
                    GameResult::Unknown => {}
                }
            } else if replay.player2.name == player_tag {
                match replay.result {
                    GameResult::Player1Won => losses += 1,
                    GameResult::Player2Won => wins += 1,
                    GameResult::Unknown => {}
                }
            }
        }

        (wins, losses)
    }

    pub fn get_cached_rank(&self, player_tag: &str) -> Option<&String> {
        self.rank_cache.get(player_tag)
    }
}

impl Default for ReplayAnalyzer {
    fn default() -> Self {
        Self::new()
    }
}

pub fn parse_replay(file_path: &str) -> io::Result<ReplayInfo> {
    let mut r = io::BufReader::new(fs::File::open(file_path)?);
    let game = slippi::read(&mut r, None).map_err(|e| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("Failed to parse replay: {e}"),
        )
    })?;

    let (player1, player2) = extract_player_info(&game)?;
    let result = determine_game_result(&game)?;
    let stage = game.start.stage;
    let stage_name = stage_id_to_name(stage);

    // Extract duration from frame data
    let duration = extract_game_duration(&game);

    // Get file modification date
    let date = fs::metadata(file_path)
        .ok()
        .and_then(|metadata| metadata.modified().ok());

    Ok(ReplayInfo {
        player1,
        player2,
        result,
        stage_name,
        duration,
        date,
        opponent_rank: None, // Will be filled in later by rank lookup
    })
}

fn extract_game_duration(game: &Game) -> Option<i32> {
    // Get the last frame ID which represents the game duration in frames
    if let Some(last_frame) = game.frames.id.iter().enumerate().next_back() {
        if let Some(frame_id) = last_frame.1 {
            return Some(*frame_id);
        }
    }
    None
}

fn stage_id_to_name(stage_id: u16) -> String {
    match stage_id {
        2 => "Fountain of Dreams".to_string(),
        3 => "Pokémon Stadium".to_string(),
        4 => "Princess Peach's Castle".to_string(),
        5 => "Kongo Jungle".to_string(),
        6 => "Brinstar".to_string(),
        7 => "Corneria".to_string(),
        8 => "Yoshi's Story".to_string(),
        9 => "Onett".to_string(),
        10 => "Mute City".to_string(),
        11 => "Rainbow Cruise".to_string(),
        12 => "Jungle Japes".to_string(),
        13 => "Great Bay".to_string(),
        14 => "Hyrule Temple".to_string(),
        15 => "Brinstar Depths".to_string(),
        16 => "Yoshi's Island".to_string(),
        17 => "Green Greens".to_string(),
        18 => "Fourside".to_string(),
        19 => "Mushroom Kingdom I".to_string(),
        20 => "Mushroom Kingdom II".to_string(),
        22 => "Venom".to_string(),
        23 => "Poké Floats".to_string(),
        24 => "Big Blue".to_string(),
        25 => "Icicle Mountain".to_string(),
        26 => "Icetop".to_string(),
        27 => "Flat Zone".to_string(),
        28 => "Dream Land N64".to_string(),
        29 => "Yoshi's Island N64".to_string(),
        30 => "Kongo Jungle N64".to_string(),
        31 => "Battlefield".to_string(),
        32 => "Final Destination".to_string(),
        _ => format!("Unknown Stage ({stage_id})"),
    }
}

fn extract_player_info(game: &Game) -> io::Result<(PlayerInfo, PlayerInfo)> {
    // Handle both cases: with and without metadata
    let (player1_name, player2_name) = if let Some(metadata) = &game.metadata {
        extract_names_from_metadata(metadata)
    } else {
        ("Unknown".to_string(), "Unknown".to_string())
    };

    // Get character and team info from start data
    let mut players_info = Vec::new();

    for (i, _player) in game.start.players.iter().enumerate() {
        let name = if i == 0 { &player1_name } else { &player2_name };

        players_info.push(PlayerInfo { name: name.clone() });
    }

    if players_info.len() >= 2 {
        Ok((players_info[0].clone(), players_info[1].clone()))
    } else {
        Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Not enough players found in replay",
        ))
    }
}

fn extract_names_from_metadata(
    metadata: &serde_json::Map<String, serde_json::Value>,
) -> (String, String) {
    if let Some(players) = metadata.get("players").and_then(|p| p.as_object()) {
        let player1_name = players
            .get("0")
            .and_then(|p| p.as_object())
            .and_then(|p| p.get("names"))
            .and_then(|n| n.as_object())
            .and_then(|n| n.get("code"))
            .and_then(|c| c.as_str())
            .unwrap_or("Unknown")
            .to_string();

        let player2_name = players
            .get("1")
            .and_then(|p| p.as_object())
            .and_then(|p| p.get("names"))
            .and_then(|n| n.as_object())
            .and_then(|n| n.get("code"))
            .and_then(|c| c.as_str())
            .unwrap_or("Unknown")
            .to_string();

        (player1_name, player2_name)
    } else {
        ("Unknown".to_string(), "Unknown".to_string())
    }
}

fn determine_game_result(game: &Game) -> io::Result<GameResult> {
    if let Some(end) = &game.end {
        if let Some(players) = &end.players {
            // Find the winner (placement == 0)
            for player in players {
                if player.placement == 0 {
                    return Ok(match player.port {
                        Port::P1 | Port::P3 => GameResult::Player1Won, // Assuming P1/P3 are team 1
                        Port::P2 | Port::P4 => GameResult::Player2Won, // Assuming P2/P4 are team 2
                    });
                }
            }
        }
    }

    Ok(GameResult::Unknown)
}
