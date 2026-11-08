//! Turns peppi's columnar frames into the frame sequence slippi-js's stat computers see.
//!
//! slippi-js only feeds *finalized* frames to its stats, keyed by frame number, keeping the
//! latest copy of a frame when rollbacks resend it. It then walks frames from -123 upward and
//! stops at the first frame that is missing or lacks post-frame data for any player. This module
//! reproduces that, reading every column with bounds checks so a malformed replay can't panic.

use peppi::frame::immutable::{Data, Frame};
use peppi::game::immutable::Game;
use peppi::io::slippi::Version;

/// First frame number of every replay.
pub const FIRST_FRAME: i32 = -123;
/// First frame where players can act.
pub const FIRST_PLAYABLE_FRAME: i32 = -39;
/// slippi-js `MAX_ROLLBACK_FRAMES`: without a usable "latest finalized frame", frames this far
/// behind the newest one are treated as final.
const MAX_ROLLBACK_FRAMES: i32 = 7;
/// Scene major for Slippi online games (slippi-js `GameMode.ONLINE`).
const SCENE_MAJOR_ONLINE: u8 = 8;

/// Post-frame values the stats use.
#[derive(Debug, Clone, Copy, Default)]
pub struct Post {
    pub state: u16,
    pub percent: f32,
    pub stocks: u8,
    /// Action state frame counter; absent before Slippi 0.2.
    pub state_age: Option<f32>,
    pub last_attack_landed: u8,
    pub x: f32,
    pub y: f32,
    pub direction: f32,
    pub character: u8,
    /// Absent before Slippi 2.0.
    pub l_cancel: Option<u8>,
}

/// Pre-frame values the input counter uses.
#[derive(Debug, Clone, Copy, Default)]
pub struct Pre {
    pub buttons_physical: u16,
    pub joystick_x: f32,
    pub joystick_y: f32,
    pub cstick_x: f32,
    pub cstick_y: f32,
    pub trigger_l: f32,
    pub trigger_r: f32,
}

/// One player's data across the processed frames.
#[derive(Debug, Default)]
pub struct PlayerFrames {
    pub player_index: u8,
    /// One entry per processed frame.
    pub post: Vec<Post>,
    /// One entry per processed frame; `None` if pre-frame data is missing.
    pub pre: Vec<Option<Pre>>,
}

/// The frames slippi-js's stat computers would process, plus game-level frame facts.
#[derive(Debug, Default)]
pub struct Frames {
    /// slippi-js `getLatestFrameNumber()`.
    pub last_frame: i32,
    /// Number of processed frames; processed frame `k` is frame number `FIRST_FRAME + k`.
    pub len: usize,
    /// Per settings player, in settings order.
    pub players: Vec<PlayerFrames>,
}

impl Frames {
    /// Frame number of processed frame `k`.
    pub fn frame_number(k: usize) -> i32 {
        FIRST_FRAME + k as i32
    }
}

fn get<T: Copy>(values: &[T], i: usize) -> Option<T> {
    values.get(i).copied()
}

fn valid(data: &Data, i: usize) -> bool {
    match &data.validity {
        None => true,
        Some(bits) => i < bits.len() && bits.get_bit(i),
    }
}

/// Reads one character's post-frame values at row `i`, or `None` if absent or truncated.
pub fn read_post(data: &Data, i: usize) -> Option<Post> {
    if !valid(data, i) {
        return None;
    }
    let p = &data.post;
    if let Some(bits) = &p.validity
        && !(i < bits.len() && bits.get_bit(i))
    {
        return None;
    }
    Some(Post {
        state: get(p.state.values(), i)?,
        percent: get(p.percent.values(), i)?,
        stocks: get(p.stocks.values(), i)?,
        state_age: p.state_age.as_ref().and_then(|a| get(a.values(), i)),
        last_attack_landed: get(p.last_attack_landed.values(), i)?,
        x: get(p.position.x.values(), i)?,
        y: get(p.position.y.values(), i)?,
        direction: get(p.direction.values(), i)?,
        character: get(p.character.values(), i)?,
        l_cancel: p.l_cancel.as_ref().and_then(|a| get(a.values(), i)),
    })
}

fn read_pre(data: &Data, i: usize) -> Option<Pre> {
    if !valid(data, i) {
        return None;
    }
    let p = &data.pre;
    Some(Pre {
        buttons_physical: get(p.buttons_physical.values(), i)?,
        joystick_x: get(p.joystick.x.values(), i)?,
        joystick_y: get(p.joystick.y.values(), i)?,
        cstick_x: get(p.cstick.x.values(), i)?,
        cstick_y: get(p.cstick.y.values(), i)?,
        trigger_l: get(p.triggers_physical.l.values(), i)?,
        trigger_r: get(p.triggers_physical.r.values(), i)?,
    })
}

/// Index into `frames.ports` for each settings player (by port), `None` if peppi has no column.
pub fn port_columns(game: &Game) -> Vec<Option<usize>> {
    game.start
        .players
        .iter()
        .map(|p| game.frames.ports.iter().position(|pd| pd.port == p.port))
        .collect()
}

/// For each frame number from `FIRST_FRAME`, the row holding its latest copy.
/// Frame numbers that could never be reached contiguously from `FIRST_FRAME` are ignored.
pub fn latest_rows(frames: &Frame) -> Vec<Option<usize>> {
    let ids = frames.id.values();
    let mut rows: Vec<Option<usize>> = Vec::new();
    for (row, &id) in ids.iter().enumerate() {
        let Some(k) = id.checked_sub(FIRST_FRAME) else {
            continue;
        };
        let Ok(k) = usize::try_from(k) else { continue };
        if k >= ids.len() {
            continue;
        }
        if k >= rows.len() {
            rows.resize(k + 1, None);
        }
        rows[k] = Some(row);
    }
    rows
}

/// Highest frame number slippi-js would have finalized.
fn finalized_limit(game: &Game, last_frame: i32) -> i32 {
    if game.end.is_some() {
        // Game End finalizes everything received.
        return last_frame;
    }
    if game.start.slippi.version <= Version(2, 2, 0) {
        // No frame bookends (<= 2.2.0): each frame update finalizes the frame before it.
        return last_frame.saturating_sub(1);
    }
    // Frame bookends: online games trust the bookend's latest finalized frame (3.7+); otherwise
    // frames MAX_ROLLBACK_FRAMES behind are final. Finalization only ever moves forward.
    let online = game
        .start
        .scene
        .is_some_and(|s| s.major == SCENE_MAJOR_ONLINE);
    let ids = game.frames.id.values();
    let lff = game
        .frames
        .end
        .as_ref()
        .and_then(|e| e.latest_finalized_frame.as_ref())
        .map(|a| a.values());
    let mut limit = i32::MIN;
    for (row, &id) in ids.iter().enumerate() {
        let target = match lff.and_then(|v| v.get(row).copied()) {
            Some(f) if online && f >= FIRST_FRAME => f,
            _ => id.saturating_sub(MAX_ROLLBACK_FRAMES),
        };
        limit = limit.max(target);
    }
    limit
}

/// Builds the processed frame sequence.
pub fn extract(game: &Game) -> Frames {
    let ids = game.frames.id.values();
    let last_frame = ids.last().copied().unwrap_or(FIRST_FRAME - 1);
    let cols = port_columns(game);
    let mut players: Vec<PlayerFrames> = game
        .start
        .players
        .iter()
        .map(|p| PlayerFrames {
            player_index: u8::from(p.port),
            ..Default::default()
        })
        .collect();

    let limit = finalized_limit(game, last_frame);
    let rows = latest_rows(&game.frames);
    let mut len = 0;
    'frames: for (k, row) in rows.iter().enumerate() {
        let Some(row) = *row else { break };
        if Frames::frame_number(k) > limit {
            break;
        }
        // A frame is only processed once every player has post-frame data.
        let mut posts = Vec::with_capacity(players.len());
        for col in &cols {
            let Some(post) = col.and_then(|c| read_post(&game.frames.ports[c].leader, row)) else {
                break 'frames;
            };
            posts.push(post);
        }
        for ((pf, post), col) in players.iter_mut().zip(posts).zip(&cols) {
            pf.post.push(post);
            pf.pre
                .push(col.and_then(|c| read_pre(&game.frames.ports[c].leader, row)));
        }
        len += 1;
    }
    if players.is_empty() {
        len = 0;
    }

    Frames {
        last_frame,
        len,
        players,
    }
}
