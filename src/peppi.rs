use peppi::game::immutable::Game;
use peppi::game::Port;
use peppi::io::slippi;
use rayon::prelude::*;
use std::collections::HashMap;
use std::fs;
use std::io;
use std::time::SystemTime;
use walkdir::WalkDir;

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
        let mut replays: Vec<ReplayInfo> = WalkDir::new(dir_path)
            .into_iter()
            .filter_map(|e| {
                if let Ok(entry) = e {
                    if entry.path().is_file()
                        && entry.path().extension().and_then(|s| s.to_str()) == Some("slp")
                    {
                        Some(entry)
                    } else {
                        None
                    }
                } else {
                    None
                }
            })
            .par_bridge()
            .filter_map(|entry| {
                let path = entry.path();
                let file_path = path.to_str().unwrap().to_string();

                parse_replay(&file_path).ok()
            })
            .collect();

        // Sort by date (newest first)
        replays.sort_by(|a, b| {
            match (a.date, b.date) {
                (Some(date_a), Some(date_b)) => date_b.cmp(&date_a), // Newer first
                (Some(_), None) => std::cmp::Ordering::Less,         // Files with dates come first
                (None, Some(_)) => std::cmp::Ordering::Greater, // Files without dates come last
                (None, None) => std::cmp::Ordering::Equal,      // Equal if both have no date
            }
        });

        self.replays = replays;
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

pub async fn fetch_player_rank(player_tag: &str) -> Result<String, Box<dyn std::error::Error>> {
    println!("🌐 Fetching rank for player: {player_tag} via Slippi GraphQL API");

    let client = reqwest::Client::builder()
        .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/125.0.0.0 Safari/537.36")
        .build()?;

    // GraphQL query to get user profile by connect code
    let query = r#"
      query UserProfilePageQuery($cc: String, $uid: String) {
        getUser(fbUid: $uid, connectCode: $cc) {
          displayName
          connectCode {
            code
          }
          rankedNetplayProfile {
            ratingOrdinal
            dailyGlobalPlacement
            dailyRegionalPlacement
          }
        }
      }
    "#;

    let json_data = serde_json::json!({
        "query": query,
        "variables": {
            "cc": player_tag,
            "uid": serde_json::Value::Null // Explicitly set uid to null as per example
        }
    });

    let response = client
        .post("https://internal.slippi.gg/graphql")
        .header("content-type", "application/json")
        .json(&json_data)
        .send()
        .await?;

    println!("📡 GraphQL Status: {}", response.status());

    let response_text = response.text().await?;
    println!("📄 Response length: {} characters", response_text.len());

    // Parse JSON response
    let json_response: serde_json::Value = serde_json::from_str(&response_text)?;

    println!("🔍 Parsing GraphQL response...");
    println!("Full JSON response: {json_response}"); // Debugging: print full JSON

    // Extract player data from the response
    if let Some(user_data) = json_response.get("data").and_then(|d| d.get("getUser")) {
        if let Some(ranked_profile) = user_data.get("rankedNetplayProfile") {
            if let Some(rating_ordinal) =
                ranked_profile.get("ratingOrdinal").and_then(|r| r.as_f64())
            {
                let rank = elo_to_rank(rating_ordinal as i32);
                println!("✅ Found rank: {rank} (ELO: {rating_ordinal})");
                return Ok(rank);
            } else {
                // Player has a ranked profile but no ratingOrdinal (e.g., unranked season)
                println!("⚠️  Player has ranked profile but no ratingOrdinal.");
                if let Some(display_name) = user_data.get("displayName").and_then(|n| n.as_str()) {
                    return Ok(format!("{display_name} (Unranked Season)"));
                }
            }
        }

        // Check if player exists but has no ranked data (not even a profile)
        if let Some(display_name) = user_data.get("displayName").and_then(|n| n.as_str()) {
            println!(
                "⚠️  Player '{display_name}' found but has no ranked netplay profile (or no ratingOrdinal)."
            );
            return Ok("Unranked".to_string());
        }
    }

    // Check for errors in the response (e.g., player not found)
    if let Some(errors) = json_response.get("errors") {
        println!("❌ GraphQL errors: {errors}");
        return Err(format!("GraphQL API returned errors: {errors}").into());
    }

    println!("❌ Player not found or no ranking data available in response: {json_response}");
    Err("Player not found or no ranking data available".into())
}

fn elo_to_rank(elo: i32) -> String {
    match elo {
        0..=765 => "Bronze 1".to_string(),
        766..=913 => "Bronze 2".to_string(),
        914..=1054 => "Bronze 3".to_string(),
        1055..=1188 => "Silver 1".to_string(),
        1189..=1315 => "Silver 2".to_string(),
        1316..=1436 => "Silver 3".to_string(),
        1437..=1546 => "Gold 1".to_string(),
        1547..=1654 => "Gold 2".to_string(),
        1655..=1751 => "Gold 3".to_string(),
        1752..=1842 => "Platinum 1".to_string(),
        1843..=1927 => "Platinum 2".to_string(),
        1928..=2003 => "Platinum 3".to_string(),
        2004..=2074 => "Diamond 1".to_string(),
        2075..=2136 => "Diamond 2".to_string(),
        2137..=2191 => "Diamond 3".to_string(),
        2192..=2274 => "Master 1".to_string(),
        2275..=2350 => "Master 2".to_string(),
        2351..=2999 => "Master 3".to_string(),
        _ => "Grandmaster".to_string(),
    }
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
