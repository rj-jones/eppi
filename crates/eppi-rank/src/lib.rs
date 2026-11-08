//! Slippi ranked profile lookups by connect code.
//!
//! [`fetch`] asks Slippi's GraphQL API (the same endpoint the official
//! launcher and Dolphin use) for one or more players, batching up to
//! [`BATCH_SIZE`] codes per HTTP request. It blocks, so call it from a worker
//! thread. See `RESEARCH.md` for how the API behaves.
//!
//! ```no_run
//! for lookup in eppi_rank::fetch(&["BEAN#888"])? {
//!     if let eppi_rank::Lookup::Found(p) = lookup {
//!         println!("{} is {} ({:.1})", p.connect_code, p.rank, p.rating);
//!     }
//! }
//! # Ok::<(), eppi_rank::Error>(())
//! ```

mod client;
mod rank;
mod seasons;

pub use client::{BATCH_SIZE, ENDPOINT, Error, TIMEOUT, USER_AGENT, fetch, normalize_code};
pub use rank::{GRANDMASTER_MIN_RATING, Rank, SETS_REQUIRED_FOR_RANK, UNRANKED_COLOR};
pub use seasons::{History, Season, fetch_histories, fetch_seasons};

/// The result for one connect code.
#[derive(Debug, Clone, PartialEq)]
pub enum Lookup {
    /// The player exists and has a ranked profile for the current season.
    ///
    /// In practice every existing account gets a profile (rating 1100, zero
    /// sets, so [`Rank::None`]) even if it has never played ranked.
    Found(RankedProfile),
    /// The account exists but the API returned no ranked profile.
    NoRankedProfile { code: String, display_name: String },
    /// No account has this code. Malformed codes land here too.
    NotFound(String),
}

impl Lookup {
    /// The connect code this result is for (normalized, e.g. `BEAN#888`).
    pub fn code(&self) -> &str {
        match self {
            Lookup::Found(p) => &p.connect_code,
            Lookup::NoRankedProfile { code, .. } | Lookup::NotFound(code) => code,
        }
    }

    /// The profile, if there is one.
    pub fn profile(&self) -> Option<&RankedProfile> {
        match self {
            Lookup::Found(p) => Some(p),
            _ => None,
        }
    }
}

/// A player's ranked singles profile for the current season.
#[derive(Debug, Clone, PartialEq)]
pub struct RankedProfile {
    /// Connect code as the server spells it, e.g. `BEAN#888`.
    pub connect_code: String,
    pub display_name: String,
    /// Rating ("ratingOrdinal"); 0 if the server sent none, like the launcher.
    pub rating: f64,
    /// Tier, computed the way the launcher does. [`Rank::Banned`] if the
    /// account status is `BANNED` (this status value is inferred, not seen).
    pub rank: Rank,
    /// Ranked sets played this season ("ratingUpdateCount").
    pub sets_played: u32,
    /// Daily global leaderboard placement, if any.
    pub daily_global_placement: Option<u32>,
    /// Daily regional leaderboard placement, if any.
    pub daily_regional_placement: Option<u32>,
    /// Set wins this season.
    pub wins: Option<u32>,
    /// Set losses this season.
    pub losses: Option<u32>,
    /// Continent the player is ranked in, e.g. `NORTH_AMERICA`, `EUROPE`.
    /// `None` until they play a ranked set.
    pub continent: Option<String>,
    /// Characters played in ranked this season, most games first.
    pub characters: Vec<CharacterUsage>,
    /// Season name, e.g. `Season 6`.
    pub season: Option<String>,
    /// Account status, e.g. `ACTIVE`.
    pub status: Option<String>,
}

impl RankedProfile {
    /// The character with the most ranked games this season.
    pub fn main_character(&self) -> Option<&CharacterUsage> {
        self.characters.first()
    }
}

/// How many ranked games a player has on one character.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CharacterUsage {
    /// The API's character id, e.g. `FALCO`, `CAPTAIN_FALCON`, `ICE_CLIMBERS`.
    pub character: String,
    pub game_count: u32,
}
