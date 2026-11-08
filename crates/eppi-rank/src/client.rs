//! The blocking GraphQL client: query building, HTTP, response parsing.

use std::collections::HashMap;
use std::fmt;
use std::time::Duration;

use serde::Deserialize;

use crate::{CharacterUsage, Lookup, Rank, RankedProfile};

/// Slippi's GraphQL endpoint. Unofficial for third parties, but it is the URL
/// baked into the official launcher and Dolphin builds, and needs no auth for
/// `getUser` by connect code.
pub const ENDPOINT: &str = "https://internal.slippi.gg/graphql";

/// Sent with every request so Slippi can tell who is calling.
pub const USER_AGENT: &str = "Eppi/1.0 (+https://github.com/rj-jones/eppi)";

/// Codes per HTTP request. The server took ~0.24 s for 4 codes, ~1.3 s for
/// 60 and ~3.2 s for 150, so 25 keeps each request well under a second.
pub const BATCH_SIZE: usize = 25;

/// Whole-request timeout (connect, send and read).
pub const TIMEOUT: Duration = Duration::from_secs(15);

/// Fields asked for each player. Deliberately leaves out profile `id`, which
/// embeds the account's Firebase UID.
const USER_FIELDS: &str = "displayName connectCode { code } status \
    rankedNetplayProfile { ratingOrdinal ratingUpdateCount wins losses \
    dailyGlobalPlacement dailyRegionalPlacement continent \
    characters { character gameCount } season { name } }";

/// Why a lookup failed.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum Error {
    /// Couldn't reach the server or read its reply: no connection, DNS, TLS,
    /// timeout, or a broken response body.
    Network(String),
    /// The server answered with a non-success status and no GraphQL errors.
    /// `429` means rate limited; `retry_after` is the `Retry-After` header in
    /// seconds, when present.
    Http {
        status: u16,
        retry_after: Option<Duration>,
    },
    /// The server reported GraphQL errors: a rejected query (HTTP 400,
    /// e.g. a field Slippi removed) or a failure while resolving (HTTP 200
    /// with `errors`). Any error fails the whole batch.
    GraphQl { status: u16, messages: Vec<String> },
    /// A success status but a body this crate can't make sense of.
    InvalidResponse(String),
}

impl Error {
    /// True for HTTP 429.
    pub fn is_rate_limited(&self) -> bool {
        matches!(self, Error::Http { status: 429, .. })
    }

    /// How long the server asked us to wait, if it said.
    pub fn retry_after(&self) -> Option<Duration> {
        match self {
            Error::Http { retry_after, .. } => *retry_after,
            _ => None,
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Network(msg) => write!(f, "couldn't reach Slippi: {msg}"),
            Error::Http {
                status: 429,
                retry_after,
            } => match retry_after {
                Some(d) => write!(f, "Slippi is rate limiting us; retry in {}s", d.as_secs()),
                None => f.write_str("Slippi is rate limiting us"),
            },
            Error::Http { status, .. } => write!(f, "Slippi answered HTTP {status}"),
            Error::GraphQl { messages, .. } => {
                write!(f, "Slippi rejected the query: {}", messages.join("; "))
            }
            Error::InvalidResponse(msg) => write!(f, "unexpected reply from Slippi: {msg}"),
        }
    }
}

impl std::error::Error for Error {}

/// Looks up each connect code and returns one [`Lookup`] per input, in the
/// same order.
///
/// Codes are normalized first (see [`normalize_code`]); blank ones come back
/// as [`Lookup::NotFound`] without a request. Codes go out [`BATCH_SIZE`] per
/// request, one request after another, and the first failing request fails
/// the call. Blocks for up to [`TIMEOUT`] per request.
pub fn fetch(codes: &[&str]) -> Result<Vec<Lookup>, Error> {
    let normalized: Vec<String> = codes.iter().map(|c| normalize_code(c)).collect();
    let mut results: Vec<Option<Lookup>> = vec![None; normalized.len()];

    let wanted: Vec<usize> = (0..normalized.len())
        .filter(|&i| !normalized[i].is_empty())
        .collect();

    if !wanted.is_empty() {
        let agent = agent();
        for chunk in wanted.chunks(BATCH_SIZE) {
            let chunk_codes: Vec<String> = chunk.iter().map(|&i| normalized[i].clone()).collect();
            let body = post(&agent, &build_request(&chunk_codes))?;
            let lookups = parse_response(&chunk_codes, &body)?;
            for (&i, lookup) in chunk.iter().zip(lookups) {
                results[i] = Some(lookup);
            }
        }
    }

    Ok(results
        .into_iter()
        .zip(normalized)
        .map(|(lookup, code)| lookup.unwrap_or(Lookup::NotFound(code)))
        .collect())
}

/// Puts a connect code in the form the API expects: trimmed, upper case, with
/// an ASCII `#`. Lookups are case-sensitive (`bean#888` finds nothing), and
/// replay files store the full-width `＃`.
pub fn normalize_code(code: &str) -> String {
    code.trim().replace('＃', "#").to_uppercase()
}

fn agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_global(Some(TIMEOUT))
        .user_agent(USER_AGENT)
        .http_status_as_error(false)
        .build()
        .into()
}

fn post(agent: &ureq::Agent, request: &serde_json::Value) -> Result<String, Error> {
    let mut response = agent
        .post(ENDPOINT)
        .send_json(request)
        .map_err(|e| Error::Network(e.to_string()))?;
    let status = response.status().as_u16();
    let retry_after = response
        .headers()
        .get("retry-after")
        .and_then(|v| v.to_str().ok())
        .and_then(parse_retry_after);
    let body = response
        .body_mut()
        .read_to_string()
        .map_err(|e| Error::Network(e.to_string()))?;
    log::debug!("rank lookup: HTTP {status}, {} bytes", body.len());
    check_status(status, retry_after, body)
}

/// Turns a non-success reply into the right [`Error`]; passes success bodies
/// through.
fn check_status(status: u16, retry_after: Option<Duration>, body: String) -> Result<String, Error> {
    if (200..300).contains(&status) {
        return Ok(body);
    }
    if status != 429
        && let Some(messages) = graphql_errors(&body)
    {
        return Err(Error::GraphQl { status, messages });
    }
    Err(Error::Http {
        status,
        retry_after,
    })
}

/// `Retry-After` in seconds. The HTTP-date form is ignored.
fn parse_retry_after(value: &str) -> Option<Duration> {
    value.trim().parse::<u64>().ok().map(Duration::from_secs)
}

/// Builds one query that looks up every code under its own alias:
/// `u0: getUser(connectCode: $c0) { ... } u1: ...`.
fn build_request(codes: &[String]) -> serde_json::Value {
    let params: Vec<String> = (0..codes.len()).map(|i| format!("$c{i}: String")).collect();
    let fields: Vec<String> = (0..codes.len())
        .map(|i| format!("u{i}: getUser(connectCode: $c{i}) {{ {USER_FIELDS} }}"))
        .collect();
    let query = format!(
        "query EppiRankLookup({}) {{ {} }}",
        params.join(", "),
        fields.join(" ")
    );
    let variables: serde_json::Map<String, serde_json::Value> = codes
        .iter()
        .enumerate()
        .map(|(i, code)| (format!("c{i}"), code.clone().into()))
        .collect();
    serde_json::json!({ "query": query, "variables": variables })
}

#[derive(Deserialize)]
struct Envelope {
    data: Option<HashMap<String, Option<UserJson>>>,
    #[serde(default)]
    errors: Vec<ErrorJson>,
}

#[derive(Deserialize)]
struct ErrorJson {
    message: String,
    #[serde(default)]
    path: Vec<serde_json::Value>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct UserJson {
    display_name: Option<String>,
    connect_code: Option<CodeJson>,
    status: Option<String>,
    ranked_netplay_profile: Option<ProfileJson>,
}

#[derive(Deserialize)]
struct CodeJson {
    code: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProfileJson {
    rating_ordinal: Option<f64>,
    rating_update_count: Option<u32>,
    wins: Option<u32>,
    losses: Option<u32>,
    daily_global_placement: Option<u32>,
    daily_regional_placement: Option<u32>,
    continent: Option<String>,
    characters: Option<Vec<CharacterJson>>,
    season: Option<SeasonJson>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CharacterJson {
    character: String,
    game_count: u32,
}

#[derive(Deserialize)]
struct SeasonJson {
    name: Option<String>,
}

/// The GraphQL error messages in a body, if it has any.
fn graphql_errors(body: &str) -> Option<Vec<String>> {
    let envelope: Envelope = serde_json::from_str(body).ok()?;
    if envelope.errors.is_empty() {
        None
    } else {
        Some(envelope.errors.iter().map(describe_error).collect())
    }
}

fn describe_error(e: &ErrorJson) -> String {
    if e.path.is_empty() {
        return e.message.clone();
    }
    let path: Vec<String> = e
        .path
        .iter()
        .map(|p| p.as_str().map_or_else(|| p.to_string(), str::to_owned))
        .collect();
    format!("{} (at {})", e.message, path.join("."))
}

/// Matches a successful reply to the codes that were asked for, in order.
fn parse_response(codes: &[String], body: &str) -> Result<Vec<Lookup>, Error> {
    let envelope: Envelope =
        serde_json::from_str(body).map_err(|e| Error::InvalidResponse(e.to_string()))?;
    if !envelope.errors.is_empty() {
        return Err(Error::GraphQl {
            status: 200,
            messages: envelope.errors.iter().map(describe_error).collect(),
        });
    }
    let mut data = envelope
        .data
        .ok_or_else(|| Error::InvalidResponse("no data in reply".into()))?;

    codes
        .iter()
        .enumerate()
        .map(|(i, code)| {
            let alias = format!("u{i}");
            match data.remove(&alias) {
                None => Err(Error::InvalidResponse(format!("reply is missing {alias}"))),
                Some(None) => Ok(Lookup::NotFound(code.clone())),
                Some(Some(user)) => Ok(to_lookup(code, user)),
            }
        })
        .collect()
}

fn to_lookup(queried: &str, user: UserJson) -> Lookup {
    let code = user
        .connect_code
        .map_or_else(|| queried.to_owned(), |c| c.code);
    let display_name = user.display_name.unwrap_or_default();
    let Some(p) = user.ranked_netplay_profile else {
        return Lookup::NoRankedProfile { code, display_name };
    };

    let rating = p.rating_ordinal.unwrap_or(0.0);
    let sets_played = p.rating_update_count.unwrap_or(0);
    // The launcher treats a placement of 0 as no placement.
    let has_placement = p.daily_global_placement.is_some_and(|n| n > 0)
        || p.daily_regional_placement.is_some_and(|n| n > 0);
    let banned = user
        .status
        .as_deref()
        .is_some_and(|s| s.eq_ignore_ascii_case("BANNED"));
    let rank = if banned {
        Rank::Banned
    } else {
        Rank::from_profile(rating, sets_played, has_placement)
    };

    let mut characters: Vec<CharacterUsage> = p
        .characters
        .unwrap_or_default()
        .into_iter()
        .map(|c| CharacterUsage {
            character: c.character,
            game_count: c.game_count,
        })
        .collect();
    characters.sort_by_key(|c| std::cmp::Reverse(c.game_count));

    Lookup::Found(RankedProfile {
        connect_code: code,
        display_name,
        rating,
        rank,
        sets_played,
        daily_global_placement: p.daily_global_placement,
        daily_regional_placement: p.daily_regional_placement,
        wins: p.wins,
        losses: p.losses,
        continent: p.continent,
        characters,
        season: p.season.and_then(|s| s.name),
        status: user.status,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn codes(list: &[&str]) -> Vec<String> {
        list.iter().map(|c| (*c).to_owned()).collect()
    }

    fn fixture(name: &str) -> String {
        let path = format!("{}/tests/fixtures/{name}", env!("CARGO_MANIFEST_DIR"));
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{path}: {e}"))
    }

    #[test]
    fn parses_a_single_profile() {
        let lookups = parse_response(&codes(&["BEAN#888"]), &fixture("single.json")).unwrap();
        let [Lookup::Found(p)] = lookups.as_slice() else {
            panic!("expected one Found, got {lookups:?}");
        };
        assert_eq!(p.connect_code, "BEAN#888");
        assert_eq!(p.display_name, "BEAN");
        assert!((p.rating - 1704.891328288768).abs() < 1e-9);
        assert_eq!(p.rank, Rank::Gold3);
        assert_eq!(p.sets_played, 23);
        assert_eq!((p.wins, p.losses), (Some(10), Some(13)));
        assert_eq!(p.daily_global_placement, None);
        assert_eq!(p.continent.as_deref(), Some("NORTH_AMERICA"));
        assert_eq!(p.season.as_deref(), Some("Season 6"));
        assert_eq!(p.status.as_deref(), Some("ACTIVE"));
        // Sorted by game count even though the reply wasn't.
        let mains: Vec<_> = p
            .characters
            .iter()
            .map(|c| (c.character.as_str(), c.game_count))
            .collect();
        assert_eq!(mains, [("FALCO", 31), ("CAPTAIN_FALCON", 26)]);
        assert_eq!(p.main_character().unwrap().character, "FALCO");
    }

    #[test]
    fn parses_a_batch_in_request_order() {
        let asked = codes(&["BEAN#888", "TOPP#1", "GONE#404", "NEWB#0"]);
        let lookups = parse_response(&asked, &fixture("batched.json")).unwrap();
        assert_eq!(lookups.len(), 4);
        assert_eq!(lookups[0].profile().unwrap().rank, Rank::Gold3);

        let top = lookups[1].profile().unwrap();
        assert_eq!(top.rank, Rank::Grandmaster);
        assert_eq!(top.daily_global_placement, Some(199));
        assert_eq!(top.continent.as_deref(), Some("EUROPE"));

        assert_eq!(lookups[2], Lookup::NotFound("GONE#404".into()));

        let newb = lookups[3].profile().unwrap();
        assert_eq!(newb.rank, Rank::None);
        assert_eq!(newb.sets_played, 0);
        assert!(newb.characters.is_empty());
        assert_eq!(newb.continent, None);

        let order: Vec<&str> = lookups.iter().map(Lookup::code).collect();
        assert_eq!(order, ["BEAN#888", "TOPP#1", "GONE#404", "NEWB#0"]);
    }

    #[test]
    fn parses_not_found() {
        let lookups = parse_response(&codes(&["ZZZZ#999"]), &fixture("not_found.json")).unwrap();
        assert_eq!(lookups, [Lookup::NotFound("ZZZZ#999".into())]);
    }

    #[test]
    fn parses_no_ranked_profile() {
        let lookups =
            parse_response(&codes(&["CASU#42"]), &fixture("no_ranked_profile.json")).unwrap();
        assert_eq!(
            lookups,
            [Lookup::NoRankedProfile {
                code: "CASU#42".into(),
                display_name: "Casual".into()
            }]
        );
    }

    #[test]
    fn rejected_query_is_a_graphql_error() {
        // The server sends validation errors with HTTP 400.
        let err = check_status(400, None, fixture("graphql_error.json")).unwrap_err();
        let Error::GraphQl { status, messages } = &err else {
            panic!("expected GraphQl, got {err:?}");
        };
        assert_eq!(*status, 400);
        assert_eq!(messages.len(), 2);
        assert!(messages[0].contains("Cannot query field \"rating\""));
        assert!(!err.is_rate_limited());
    }

    #[test]
    fn resolver_errors_with_200_fail_the_batch() {
        let err = parse_response(
            &codes(&["BEAN#888", "TOPP#1"]),
            &fixture("partial_error.json"),
        )
        .unwrap_err();
        let Error::GraphQl { status, messages } = err else {
            panic!("expected GraphQl");
        };
        assert_eq!(status, 200);
        assert_eq!(messages, ["Internal server error (at u1)"]);
    }

    #[test]
    fn rate_limit_keeps_retry_after() {
        let err =
            check_status(429, parse_retry_after("30"), "Too Many Requests".into()).unwrap_err();
        assert!(err.is_rate_limited());
        assert_eq!(err.retry_after(), Some(Duration::from_secs(30)));
        assert_eq!(err.to_string(), "Slippi is rate limiting us; retry in 30s");
        // Even a 429 with a GraphQL-shaped body stays an HTTP error.
        let err = check_status(429, None, fixture("graphql_error.json")).unwrap_err();
        assert!(err.is_rate_limited());
        assert_eq!(err.retry_after(), None);
    }

    #[test]
    fn other_statuses_are_http_errors() {
        let err = check_status(503, None, "<html>Service Unavailable</html>".into()).unwrap_err();
        assert_eq!(
            err,
            Error::Http {
                status: 503,
                retry_after: None
            }
        );
        assert_eq!(check_status(200, None, "{}".into()).unwrap(), "{}");
    }

    #[test]
    fn retry_after_parsing() {
        assert_eq!(parse_retry_after(" 5 "), Some(Duration::from_secs(5)));
        assert_eq!(parse_retry_after("Wed, 21 Oct 2026 07:28:00 GMT"), None);
    }

    #[test]
    fn malformed_bodies_are_invalid_responses() {
        let one = codes(&["BEAN#888"]);
        assert!(matches!(
            parse_response(&one, "not json"),
            Err(Error::InvalidResponse(_))
        ));
        assert!(matches!(
            parse_response(&one, r#"{"data":null}"#),
            Err(Error::InvalidResponse(_))
        ));
        assert!(matches!(
            parse_response(&one, r#"{"data":{}}"#),
            Err(Error::InvalidResponse(msg)) if msg.contains("u0")
        ));
    }

    #[test]
    fn banned_status_overrides_rating() {
        let body = r#"{"data":{"u0":{"displayName":"X","connectCode":{"code":"BAD#1"},"status":"BANNED",
            "rankedNetplayProfile":{"ratingOrdinal":2000,"ratingUpdateCount":50}}}}"#;
        let lookups = parse_response(&codes(&["BAD#1"]), body).unwrap();
        assert_eq!(lookups[0].profile().unwrap().rank, Rank::Banned);
    }

    #[test]
    fn zero_placement_does_not_make_grandmaster() {
        let body = r#"{"data":{"u0":{"displayName":"X","connectCode":{"code":"HI#1"},
            "rankedNetplayProfile":{"ratingOrdinal":2400,"ratingUpdateCount":50,"dailyGlobalPlacement":0}}}}"#;
        let lookups = parse_response(&codes(&["HI#1"]), body).unwrap();
        assert_eq!(lookups[0].profile().unwrap().rank, Rank::Master3);
    }

    #[test]
    fn builds_an_aliased_query() {
        let request = build_request(&codes(&["BEAN#888", "TOPP#1"]));
        let query = request["query"].as_str().unwrap();
        assert!(query.starts_with("query EppiRankLookup($c0: String, $c1: String) {"));
        assert!(query.contains("u0: getUser(connectCode: $c0) {"));
        assert!(query.contains("u1: getUser(connectCode: $c1) {"));
        assert!(!query.contains(" id "), "must not ask for the profile id");
        assert_eq!(request["variables"]["c0"], "BEAN#888");
        assert_eq!(request["variables"]["c1"], "TOPP#1");
    }

    #[test]
    fn normalizes_codes() {
        assert_eq!(normalize_code(" bean#888 "), "BEAN#888");
        assert_eq!(normalize_code("bean＃888"), "BEAN#888");
        assert_eq!(normalize_code("   "), "");
    }

    #[test]
    fn blank_codes_skip_the_network() {
        assert_eq!(fetch(&[]).unwrap(), []);
        assert_eq!(
            fetch(&["", "  "]).unwrap(),
            [
                Lookup::NotFound(String::new()),
                Lookup::NotFound(String::new())
            ]
        );
    }

    /// Hits the real API. Run with `cargo test -p eppi-rank -- --ignored`.
    #[test]
    #[ignore = "makes a live request to Slippi"]
    fn live_fetch_bean() {
        let lookups = fetch(&["bean#888", "NOBODY#99999"]).expect("live lookup");
        let Lookup::Found(p) = &lookups[0] else {
            panic!("expected BEAN#888 to be found, got {:?}", lookups[0]);
        };
        assert_eq!(p.connect_code, "BEAN#888");
        assert!(p.rating > 0.0);
        println!("{p:#?}");
        assert_eq!(lookups[1], Lookup::NotFound("NOBODY#99999".into()));
    }
}
