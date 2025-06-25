use serde_json;

/// Fetch a player's rank from the Slippi GraphQL API.
///
/// This was previously defined in `peppi.rs`, but all HTTP / web
/// functionality now lives inside `web.rs`.
///
/// Returns the rank as a `String` on success or an error on failure.
pub async fn fetch_player_rank(
    player_tag: &str,
) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
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

/// Convert an ELO value into the human-readable rank string used by Slippi.
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
