//! Result types. Field names serialize in camelCase to line up with slippi-js's `getStats()`.
//!
//! Percents and damage are `f64` computed from the replay's `f32` values exactly the way
//! slippi-js (JavaScript doubles) computes them, so sums match bit for bit.

use serde::{Deserialize, Serialize};

/// Everything computed for one game: slippi-js `getStats()` plus `getWinners()` and a few extras.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GameStats {
    /// Number of the last frame in the replay (frames start at -123; 0 is "GO!").
    /// -124 if the replay has no frames.
    pub last_frame: i32,
    /// Frames since the first playable frame (-39). Divide by 60 for seconds.
    pub playable_frame_count: i32,
    /// Stocks, in the order they started. Singles (exactly two players) only, as in slippi-js.
    pub stocks: Vec<Stock>,
    /// Conversions ("punishes"): every opening and its follow-up until the victim escapes for
    /// 45 frames or dies. Singles only.
    pub conversions: Vec<Conversion>,
    /// Combos: like conversions but they end as soon as the victim is out of hitstun / tech /
    /// down states for 45 frames. Singles only.
    pub combos: Vec<Combo>,
    /// Action counts (wavedashes, L-cancels, techs, ...), one per player. Singles only.
    pub action_counts: Vec<ActionCounts>,
    /// Overall stats, one per player (all game types; conversion-based stats are empty
    /// outside singles).
    pub overall: Vec<Overall>,
    /// Whether the replay has a Game End event (it wasn't cut off).
    pub game_complete: bool,
    /// Winners as slippi-js `getWinners()` reports them (empty for draws, quit-outs where the
    /// quitter is unknown, and undecidable games).
    pub winners: Vec<Placement>,
    /// Per-player end-of-game state, in settings (port) order.
    pub players: Vec<PlayerSummary>,
}

/// One stock.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Stock {
    /// Player index (port, 0-based) of the player who owns this stock.
    pub player_index: u8,
    /// Frame the stock started (player no longer in a dying state).
    pub start_frame: i32,
    /// Frame the stock was lost, `None` if the game ended first.
    pub end_frame: Option<i32>,
    /// Percent at the start (always 0, as in slippi-js).
    pub start_percent: f64,
    /// Latest percent while the stock was alive.
    pub current_percent: f64,
    /// Percent on the frame before the stock was lost, `None` if not lost.
    pub end_percent: Option<f64>,
    /// Stocks remaining when this stock started (e.g. 4 for the first stock).
    pub count: u8,
    /// Action state on the frame the stock was lost (see [`crate::melee::death_direction`]).
    pub death_animation: Option<u16>,
    /// Extra: player index of whoever took the stock: the attacker of the conversion that ended
    /// with this death. `None` for a self-destruct (no conversion was in progress).
    pub killed_by: Option<u8>,
    /// Extra: move id of the last move of that conversion (see [`crate::melee::move_name`]).
    pub kill_move_id: Option<u8>,
}

/// One move landed during a combo or conversion.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MoveLanded {
    /// Attacker's player index.
    pub player_index: u8,
    /// Frame the move first hit.
    pub frame: i32,
    /// Move id (see [`crate::melee::move_name`]).
    pub move_id: u8,
    /// Number of hits (multi-hit moves count once with several hits).
    pub hit_count: u32,
    /// Total damage dealt by this move.
    pub damage: f64,
}

/// A combo (slippi-js `ComboType`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Combo {
    /// Victim's player index (the player being comboed).
    pub player_index: u8,
    /// Frame the combo started.
    pub start_frame: i32,
    /// Frame it ended, `None` if the game ended first.
    pub end_frame: Option<i32>,
    /// Victim's percent before the first hit.
    pub start_percent: f64,
    /// Victim's latest percent during the combo.
    pub current_percent: f64,
    /// Victim's percent on the frame before the combo ended.
    pub end_percent: Option<f64>,
    /// Moves landed, in order.
    pub moves: Vec<MoveLanded>,
    /// Whether the combo ended with the victim losing a stock.
    pub did_kill: bool,
    /// Attacker's player index.
    pub last_hit_by: Option<u8>,
}

/// How a conversion started.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum OpeningType {
    /// Not classified (never present in finished output).
    Unknown,
    /// Both players started a conversion on the same frame.
    Trade,
    /// Started while the opponent's own conversion on this player was still going.
    CounterAttack,
    /// Started from neutral.
    NeutralWin,
}

/// A conversion / punish (slippi-js `ConversionType`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Conversion {
    /// Victim's player index.
    pub player_index: u8,
    /// Attacker's player index.
    pub last_hit_by: Option<u8>,
    /// Frame the conversion started.
    pub start_frame: i32,
    /// Frame it ended, `None` if the game ended first.
    pub end_frame: Option<i32>,
    /// Victim's percent before the first hit.
    pub start_percent: f64,
    /// Victim's latest percent during the conversion.
    pub current_percent: f64,
    /// Victim's percent on the frame before the conversion ended.
    pub end_percent: Option<f64>,
    /// Moves landed, in order. Can be empty (e.g. a grab that was broken immediately).
    pub moves: Vec<MoveLanded>,
    /// Whether the conversion ended with the victim losing a stock.
    pub did_kill: bool,
    /// How the conversion started.
    pub opening_type: OpeningType,
}

/// Successful / failed counter.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SuccessFail {
    /// Successes.
    pub success: i32,
    /// Failures.
    pub fail: i32,
}

/// Edge cancel counter.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct EdgeCancelCount {
    /// Edge cancels faster than an L-cancel would have been.
    pub success: i32,
    /// Edge cancels no faster than an L-cancel (but faster than no cancel).
    pub slow: i32,
}

/// Attack usage counts (move starts, not hits).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[allow(missing_docs)]
pub struct AttackCount {
    pub jab1: i32,
    pub jab2: i32,
    pub jab3: i32,
    pub jabm: i32,
    pub dash: i32,
    pub ftilt: i32,
    pub utilt: i32,
    pub dtilt: i32,
    pub fsmash: i32,
    pub usmash: i32,
    pub dsmash: i32,
    pub nair: i32,
    pub fair: i32,
    pub bair: i32,
    pub uair: i32,
    pub dair: i32,
}

/// Throw counts.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[allow(missing_docs)]
pub struct ThrowCount {
    pub up: i32,
    pub forward: i32,
    pub back: i32,
    pub down: i32,
}

/// Ground tech counts. "In"/"away" are relative to the opponent, not the stage.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[allow(missing_docs)]
pub struct GroundTechCount {
    pub away: i32,
    #[serde(rename = "in")]
    pub in_: i32,
    pub neutral: i32,
    pub fail: i32,
}

/// Per-player action counts (slippi-js `ActionCountsType`).
///
/// Counts are signed because slippi-js decrements some of them (air dodges used for
/// wavedashes, dash attacks turned into boost grabs, edge cancels that were failed L-cancels).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[allow(missing_docs)]
pub struct ActionCounts {
    pub player_index: u8,
    pub wavedash_count: i32,
    pub waveland_count: i32,
    pub air_dodge_count: i32,
    pub dash_dance_count: i32,
    pub spot_dodge_count: i32,
    pub ledgegrab_count: i32,
    pub roll_count: i32,
    pub taunt_count: i32,
    pub edge_cancel_count: EdgeCancelCount,
    pub l_cancel_count: SuccessFail,
    pub attack_count: AttackCount,
    pub grab_count: SuccessFail,
    pub throw_count: ThrowCount,
    pub ground_tech_count: GroundTechCount,
    pub wall_tech_count: SuccessFail,
}

/// Input counts (slippi-js `InputCountsType`). Singles only; zero otherwise.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct InputCounts {
    /// Physical button presses (0 to 1 transitions).
    pub buttons: u32,
    /// Analog trigger presses past 0.3.
    pub triggers: u32,
    /// Joystick moves into a new non-deadzone region.
    pub joystick: u32,
    /// C-stick moves into a new non-deadzone region.
    pub cstick: u32,
    /// Sum of the above.
    pub total: u32,
}

/// `count / total`, with `ratio` `None` when `total` is 0.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct Ratio {
    /// Numerator.
    pub count: f64,
    /// Denominator.
    pub total: f64,
    /// `count / total`, `None` if `total` is 0.
    pub ratio: Option<f64>,
}

impl Ratio {
    pub(crate) fn new(count: f64, total: f64) -> Self {
        Self {
            count,
            total,
            ratio: (total != 0.0 && !total.is_nan()).then(|| count / total),
        }
    }
}

/// Per-player overall stats (slippi-js `OverallType`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Overall {
    /// Player index.
    pub player_index: u8,
    /// Input counts.
    pub input_counts: InputCounts,
    /// Number of conversions on this player's opponents (slippi-js counts every conversion
    /// whose victim is not this player).
    pub conversion_count: u32,
    /// Damage this player dealt during conversions.
    pub total_damage: f64,
    /// Kills credited to this player.
    pub kill_count: u32,
    /// Conversions with more than one move started by this player / `conversion_count`.
    pub successful_conversions: Ratio,
    /// Inputs per minute of playable time.
    pub inputs_per_minute: Ratio,
    /// Button presses per minute of playable time.
    pub digital_inputs_per_minute: Ratio,
    /// Openings per kill.
    pub openings_per_kill: Ratio,
    /// Damage per opening.
    pub damage_per_opening: Ratio,
    /// Neutral wins / all neutral wins.
    pub neutral_win_ratio: Ratio,
    /// Counter hits / all counter hits.
    pub counter_hit_ratio: Ratio,
    /// Trades that went in this player's favor / this player's trades.
    pub beneficial_trade_ratio: Ratio,
}

/// A winner (slippi-js `PlacementType`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Placement {
    /// Player index.
    pub player_index: u8,
    /// 0-based placement (`None` if the replay didn't record one).
    pub position: Option<i8>,
}

/// End-of-game state for one player.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlayerSummary {
    /// Player index (port, 0-based).
    pub player_index: u8,
    /// Stocks remaining on the player's last frame of data.
    pub final_stocks: u8,
    /// Percent on the player's last frame of data.
    pub final_percent: f64,
}
