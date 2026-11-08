//! Ranked seasons: when each ran, and how a player finished it.

use serde::Deserialize;

use crate::Rank;
use crate::client::{Error, agent, graphql_errors, normalize_code, post};

/// A player's code and their seasons (`None`: no account has the code).
pub type History = (String, Option<Vec<Season>>);

/// One ranked season and a player's profile in it.
#[derive(Debug, Clone, PartialEq)]
pub struct Season {
    /// `season-6`, `beta-season-2`.
    pub id: String,
    /// `Season 6`, `Beta Season 2`.
    pub name: String,
    /// Still running.
    pub active: bool,
    /// When it started and ended, RFC 3339 (`2026-10-19T15:49:50.886Z`);
    /// no end while it runs.
    pub started_at: String,
    pub ended_at: Option<String>,
    /// The player's rating, rank and sets in it (their final ones for a
    /// finished season).
    pub rating: f64,
    pub rank: Rank,
    pub sets_played: u32,
    pub wins: Option<u32>,
    pub losses: Option<u32>,
}

/// What is asked of each player.
const USER_FIELDS: &str = "rankedNetplayProfile { ...p } rankedNetplayProfileHistory { ...p }";
const FRAGMENT: &str = "fragment p on NetplayProfile { ratingOrdinal ratingUpdateCount wins losses \
    dailyGlobalPlacement dailyRegionalPlacement season { id name status startedAt endedAt } }";

/// Every ranked season `code` has a profile in (every season since they
/// made their account), newest first. The dates are the same for everyone,
/// so one player's list serves as the season calendar. Empty when no
/// account has the code.
pub fn fetch_seasons(code: &str) -> Result<Vec<Season>, Error> {
    let request = serde_json::json!({
        "query": format!("query EppiSeasons($c: String) {{ getUser(connectCode: $c) {{ {USER_FIELDS} }} }} {FRAGMENT}"),
        "variables": { "c": normalize_code(code) },
    });
    let body = post(&agent(), &request)?;
    parse(&body)
}

/// Every season of each player in `codes`: one result per code, in order,
/// `None` when no account has the code. Up to [`crate::BATCH_SIZE`] codes per
/// request (with GraphQL aliases), one request after another.
pub fn fetch_histories(codes: &[&str]) -> Result<Vec<History>, Error> {
    let agent = agent();
    let mut out = Vec::with_capacity(codes.len());
    for chunk in codes.chunks(crate::BATCH_SIZE) {
        let codes: Vec<String> = chunk.iter().map(|c| normalize_code(c)).collect();
        let params: Vec<String> = (0..codes.len()).map(|i| format!("$c{i}: String")).collect();
        let fields: Vec<String> = (0..codes.len())
            .map(|i| format!("u{i}: getUser(connectCode: $c{i}) {{ {USER_FIELDS} }}"))
            .collect();
        let variables: serde_json::Map<String, serde_json::Value> = codes
            .iter()
            .enumerate()
            .map(|(i, code)| (format!("c{i}"), code.clone().into()))
            .collect();
        let request = serde_json::json!({
            "query": format!("query EppiHistories({}) {{ {} }} {FRAGMENT}", params.join(", "), fields.join(" ")),
            "variables": variables,
        });
        let body = post(&agent, &request)?;
        out.extend(parse_batch(&codes, &body)?);
    }
    Ok(out)
}

#[derive(Deserialize)]
struct BatchEnvelope {
    data: Option<std::collections::HashMap<String, Option<User>>>,
}

fn parse_batch(codes: &[String], body: &str) -> Result<Vec<History>, Error> {
    if let Some(messages) = graphql_errors(body) {
        return Err(Error::GraphQl {
            status: 200,
            messages,
        });
    }
    let envelope: BatchEnvelope =
        serde_json::from_str(body).map_err(|e| Error::InvalidResponse(e.to_string()))?;
    let mut users = envelope.data.unwrap_or_default();
    Ok(codes
        .iter()
        .enumerate()
        .map(|(i, code)| {
            let seasons = users.remove(&format!("u{i}")).flatten().map(seasons_of);
            (code.clone(), seasons)
        })
        .collect())
}

#[derive(Deserialize)]
struct Envelope {
    data: Option<Data>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Data {
    get_user: Option<User>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct User {
    ranked_netplay_profile: Option<Profile>,
    #[serde(default)]
    ranked_netplay_profile_history: Vec<Profile>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Profile {
    rating_ordinal: Option<f64>,
    rating_update_count: Option<u32>,
    wins: Option<u32>,
    losses: Option<u32>,
    daily_global_placement: Option<u32>,
    daily_regional_placement: Option<u32>,
    season: Option<SeasonJson>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SeasonJson {
    id: String,
    name: Option<String>,
    status: Option<String>,
    started_at: Option<String>,
    ended_at: Option<String>,
}

fn parse(body: &str) -> Result<Vec<Season>, Error> {
    if let Some(messages) = graphql_errors(body) {
        return Err(Error::GraphQl {
            status: 200,
            messages,
        });
    }
    let envelope: Envelope =
        serde_json::from_str(body).map_err(|e| Error::InvalidResponse(e.to_string()))?;
    Ok(envelope
        .data
        .and_then(|d| d.get_user)
        .map(seasons_of)
        .unwrap_or_default())
}

/// A player's seasons, newest first.
fn seasons_of(user: User) -> Vec<Season> {
    let mut seasons: Vec<Season> = user
        .ranked_netplay_profile
        .into_iter()
        .chain(user.ranked_netplay_profile_history)
        .filter_map(|p| {
            let season = p.season?;
            let started_at = season.started_at?;
            let rating = p.rating_ordinal.unwrap_or(0.0);
            let sets_played = p.rating_update_count.unwrap_or(0);
            // As the launcher: a placement of 0 is no placement.
            let placed = p.daily_global_placement.is_some_and(|n| n > 0)
                || p.daily_regional_placement.is_some_and(|n| n > 0);
            Some(Season {
                name: season.name.unwrap_or_else(|| season.id.clone()),
                id: season.id,
                active: season.status.as_deref() == Some("ACTIVE") || season.ended_at.is_none(),
                started_at,
                ended_at: season.ended_at,
                rating,
                rank: Rank::from_profile(rating, sets_played, placed),
                sets_played,
                wins: p.wins,
                losses: p.losses,
            })
        })
        .collect();
    seasons.sort_by(|a, b| b.started_at.cmp(&a.started_at));
    seasons.dedup_by(|a, b| a.id == b.id);
    seasons
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seasons_come_newest_first_with_their_dates_and_finish() {
        let body = r#"{"data":{"getUser":{
            "rankedNetplayProfile":{"ratingOrdinal":1704.89,"ratingUpdateCount":23,"wins":10,"losses":13,
              "season":{"id":"season-6","name":"Season 6","status":"ACTIVE","startedAt":"2026-10-19T15:49:50.886Z","endedAt":null}},
            "rankedNetplayProfileHistory":[
              {"ratingOrdinal":1654.5,"ratingUpdateCount":272,"wins":134,"losses":138,
               "season":{"id":"beta-season","name":"Beta Season","status":"COMPLETED","startedAt":"2022-12-12T17:09:03.62Z","endedAt":"2023-07-11T20:20:52.73Z"}},
              {"ratingOrdinal":1764.66,"ratingUpdateCount":45,"wins":23,"losses":22,
               "season":{"id":"season-5","name":"Season 5","status":"COMPLETED","startedAt":"2026-04-20T17:07:46.817Z","endedAt":"2026-10-19T15:49:50.886Z"}}
            ]}}}"#;
        let seasons = parse(body).unwrap();
        let names: Vec<&str> = seasons.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, ["Season 6", "Season 5", "Beta Season"]);
        assert!(seasons[0].active && seasons[0].ended_at.is_none());
        assert_eq!(seasons[1].rank, Rank::Platinum1);
        assert_eq!((seasons[1].wins, seasons[1].losses), (Some(23), Some(22)));
    }

    #[test]
    fn a_batch_answers_each_code_in_order() {
        let body = r#"{"data":{
            "u0":{"rankedNetplayProfile":{"ratingOrdinal":1500,"ratingUpdateCount":9,
              "season":{"id":"season-6","name":"Season 6","status":"ACTIVE","startedAt":"2026-10-19T15:49:50.886Z","endedAt":null}},
              "rankedNetplayProfileHistory":[]},
            "u1":null}}"#;
        let codes = ["A#1".to_owned(), "B#2".to_owned()];
        let out = parse_batch(&codes, body).unwrap();
        assert_eq!(out[0].0, "A#1");
        assert_eq!(out[0].1.as_ref().unwrap()[0].rank, Rank::Gold1);
        assert_eq!(out[1], ("B#2".to_owned(), None));
    }

    #[test]
    fn an_unknown_code_has_no_seasons() {
        assert_eq!(parse(r#"{"data":{"getUser":null}}"#).unwrap(), Vec::new());
    }
}
