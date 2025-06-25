/// Fetch a player's rank from the Slippi GraphQL API.
///
/// This was previously defined in `peppi.rs`, but all HTTP / web
/// functionality now lives inside `web.rs`.
///
/// Returns the rank as a `String` on success or an error on failure.
pub async fn fetch_player_rank(
    player_tag: &str,
) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    log::info!("🌐 Fetching rank for player: {player_tag} via Slippi GraphQL API");

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

    log::debug!("📡 GraphQL Status: {}", response.status());

    let response_text = response.text().await?;
    log::debug!("📄 Response length: {} characters", response_text.len());

    // Parse JSON response
    let json_response: serde_json::Value = serde_json::from_str(&response_text)?;

    log::debug!("🔍 Parsing GraphQL response...");
    log::debug!("Full JSON response: {json_response}");

    // Extract player data from the response
    if let Some(user_data) = json_response.get("data").and_then(|d| d.get("getUser")) {
        if let Some(ranked_profile) = user_data.get("rankedNetplayProfile") {
            if let Some(rating_ordinal) =
                ranked_profile.get("ratingOrdinal").and_then(|r| r.as_f64())
            {
                let regional_placement = ranked_profile
                    .get("dailyRegionalPlacement")
                    .and_then(|p| p.as_i64())
                    .unwrap_or(i64::MAX) as i32;
                let global_placement = ranked_profile
                    .get("dailyGlobalPlacement")
                    .and_then(|p| p.as_i64())
                    .unwrap_or(i64::MAX) as i32;

                let rank = elo_to_rank(rating_ordinal as i32, regional_placement, global_placement);
                log::info!("✅ Found rank: {rank} (ELO: {rating_ordinal}, Regional: {regional_placement}, Global: {global_placement})");
                return Ok(rank);
            } else {
                // Player has a ranked profile but no ratingOrdinal (e.g., unranked season)
                log::warn!("⚠️  Player has ranked profile but no ratingOrdinal.");
                if let Some(display_name) = user_data.get("displayName").and_then(|n| n.as_str()) {
                    return Ok(format!("{display_name} (Unranked Season)"));
                }
            }
        }

        // Check if player exists but has no ranked data (not even a profile)
        if let Some(display_name) = user_data.get("displayName").and_then(|n| n.as_str()) {
            log::warn!(
                "⚠️  Player '{display_name}' found but has no ranked netplay profile (or no ratingOrdinal)."
            );
            return Ok("Unranked".to_string());
        }
    }

    // Check for errors in the response (e.g., player not found)
    if let Some(errors) = json_response.get("errors") {
        log::error!("❌ GraphQL errors: {errors}");
        return Err(format!("GraphQL API returned errors: {errors}").into());
    }

    log::error!("❌ Player not found or no ranking data available in response: {json_response}");
    Err("Player not found or no ranking data available".into())
}

/// Convert an ELO value into the human-readable rank string used by Slippi.
fn elo_to_rank(rating: i32, regional_placement: i32, global_placement: i32) -> String {
    match rating {
        r if r < 766 => "Bronze 1".to_string(),
        r if r < 914 => "Bronze 2".to_string(),
        r if r < 1055 => "Bronze 3".to_string(),
        r if r < 1189 => "Silver 1".to_string(),
        r if r < 1316 => "Silver 2".to_string(),
        r if r < 1436 => "Silver 3".to_string(),
        r if r < 1549 => "Gold 1".to_string(),
        r if r < 1654 => "Gold 2".to_string(),
        r if r < 1752 => "Gold 3".to_string(),
        r if r < 1843 => "Platinum 1".to_string(),
        r if r < 1928 => "Platinum 2".to_string(),
        r if r < 2004 => "Platinum 3".to_string(),
        r if r < 2074 => "Diamond 1".to_string(),
        r if r < 2137 => "Diamond 2".to_string(),
        r if r < 2192 => "Diamond 3".to_string(),
        r if r >= 2192 && (regional_placement <= 100 || global_placement <= 300) => {
            "Grandmaster".to_string()
        }
        r if r < 2275 => "Master 1".to_string(),
        r if r < 2350 => "Master 2".to_string(),
        r if r >= 2350 => "Master 3".to_string(),
        _ => "Unranked".to_string(),
    }
}
