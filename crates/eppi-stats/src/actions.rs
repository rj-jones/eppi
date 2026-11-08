//! Action counts (slippi-js `actions.ts`).

use crate::frames::Frames;
use crate::melee::{aerial_landing_lag, internal, state::*};
use crate::types::ActionCounts;

/// slippi-js wavedash lookback window.
const WAVEDASH_LOOKBACK: usize = 15;

struct PlayerState {
    counts: ActionCounts,
    animations: Vec<u16>,
    action_frame_counters: Vec<Option<f32>>,
    positions_y: Vec<f32>,
    last_l_cancel_status: u8,
    last_handled_air_dodge_end_index: Option<usize>,
}

fn is_aerial_landing(a: Option<u16>) -> bool {
    a.is_some_and(|a| (AERIAL_LANDING_START..=AERIAL_LANDING_END).contains(&a))
}

fn is_grabbing(a: Option<u16>) -> bool {
    matches!(a, Some(GRAB) | Some(DASH_GRAB))
}

/// Grab pull, wait, pummel and throws.
fn is_grab_action(a: u16) -> bool {
    a > GRAB && a <= THROW_DOWN && a != DASH_GRAB
}

/// Computes action counts for singles permutations (one entry per permutation).
pub(crate) fn compute(frames: &Frames, perms: &[(usize, usize)]) -> Vec<ActionCounts> {
    let mut states: Vec<PlayerState> = perms
        .iter()
        .map(|&(p, _)| PlayerState {
            counts: ActionCounts {
                player_index: frames.players[p].player_index,
                ..Default::default()
            },
            animations: Vec::with_capacity(frames.len),
            action_frame_counters: Vec::with_capacity(frames.len),
            positions_y: Vec::with_capacity(frames.len),
            last_l_cancel_status: 0,
            last_handled_air_dodge_end_index: None,
        })
        .collect();

    for k in 0..frames.len {
        for (st, &(p, o)) in states.iter_mut().zip(perms) {
            let pf = &frames.players[p].post[k];
            let of = &frames.players[o].post[k];

            let current = pf.state;
            st.animations.push(current);
            st.action_frame_counters.push(pf.state_age);
            st.positions_y.push(pf.y);

            let n = st.animations.len();
            let prev = n.checked_sub(2).map(|i| st.animations[i]);
            let prev_counter = n.checked_sub(2).and_then(|i| st.action_frame_counters[i]);

            // New action if the animation changed or its frame counter went back down.
            let counter_went_down =
                matches!((prev_counter, pf.state_age), (Some(a), Some(b)) if a > b);
            if Some(current) == prev && !counter_went_down {
                continue;
            }
            let c = &mut st.counts;

            if n >= 3 && st.animations[n - 3..] == [DASH, TURN, DASH] {
                c.dash_dance_count += 1;
            }
            if current == ROLL_BACKWARD || current == ROLL_FORWARD {
                c.roll_count += 1;
            }
            if current == SPOT_DODGE {
                c.spot_dodge_count += 1;
            }
            if current == AIR_DODGE {
                c.air_dodge_count += 1;
            }
            if current == CLIFF_CATCH {
                c.ledgegrab_count += 1;
            }
            if current == TAUNT_LEFT || current == TAUNT_RIGHT {
                c.taunt_count += 1;
            }

            // Grabs.
            if is_grabbing(prev) {
                if is_grab_action(current) {
                    c.grab_count.success += 1;
                } else {
                    c.grab_count.fail += 1;
                }
            }
            if current == DASH_GRAB && prev == Some(ATTACK_DASH) {
                c.attack_count.dash -= 1; // boost grab, not a dash attack
            }

            // Attacks.
            let a = &mut c.attack_count;
            match current {
                ATTACK_JAB1 => a.jab1 += 1,
                ATTACK_JAB2 => a.jab2 += 1,
                ATTACK_JAB3 => a.jab3 += 1,
                ATTACK_JABM => a.jabm += 1,
                ATTACK_DASH => a.dash += 1,
                ATTACK_FTILT_START..=ATTACK_FTILT_END => a.ftilt += 1,
                ATTACK_UTILT => a.utilt += 1,
                ATTACK_DTILT => a.dtilt += 1,
                ATTACK_FSMASH_START..=ATTACK_FSMASH_END => a.fsmash += 1,
                ATTACK_USMASH => a.usmash += 1,
                ATTACK_DSMASH => a.dsmash += 1,
                AERIAL_NAIR => a.nair += 1,
                AERIAL_FAIR => a.fair += 1,
                AERIAL_BAIR => a.bair += 1,
                AERIAL_UAIR => a.uair += 1,
                AERIAL_DAIR => a.dair += 1,
                _ => {}
            }
            if pf.character == internal::GAME_AND_WATCH {
                match current {
                    GNW_JAB1 => a.jab1 += 1,
                    GNW_JABM => a.jabm += 1,
                    GNW_DTILT => a.dtilt += 1,
                    GNW_FSMASH => a.fsmash += 1,
                    GNW_NAIR => a.nair += 1,
                    GNW_BAIR => a.bair += 1,
                    GNW_UAIR => a.uair += 1,
                    _ => {}
                }
            }
            if pf.character == internal::PEACH
                && matches!(current, PEACH_FSMASH1 | PEACH_FSMASH2 | PEACH_FSMASH3)
            {
                a.fsmash += 1;
            }

            // Throws.
            match current {
                THROW_UP => c.throw_count.up += 1,
                THROW_FORWARD => c.throw_count.forward += 1,
                THROW_DOWN => c.throw_count.down += 1,
                THROW_BACK => c.throw_count.back += 1,
                _ => {}
            }

            // Techs ("in"/"away" relative to the opponent).
            let opponent_dir = if pf.x > of.x { -1.0 } else { 1.0 };
            let facing_opponent = f64::from(pf.direction) == opponent_dir;
            match current {
                TECH_MISS_UP | TECH_MISS_DOWN => c.ground_tech_count.fail += 1,
                FORWARD_TECH if facing_opponent => c.ground_tech_count.in_ += 1,
                FORWARD_TECH => c.ground_tech_count.away += 1,
                BACKWARD_TECH if facing_opponent => c.ground_tech_count.away += 1,
                BACKWARD_TECH => c.ground_tech_count.in_ += 1,
                NEUTRAL_TECH => c.ground_tech_count.neutral += 1,
                WALL_TECH => c.wall_tech_count.success += 1,
                MISSED_WALL_TECH => c.wall_tech_count.fail += 1,
                _ => {}
            }

            if is_aerial_landing(Some(current)) {
                match pf.l_cancel {
                    Some(1) => c.l_cancel_count.success += 1,
                    Some(2) => c.l_cancel_count.fail += 1,
                    _ => {}
                }
            }

            // Edge-cancelled aerials.
            if is_aerial_landing(prev) && (current == FALL || current == TEETER) {
                let landing_frames = prev_animation_length(&st.animations);
                // slippi-js throws here for characters without frame data; we skip instead.
                if let Some((landing_lag, l_cancelled_lag)) =
                    prev.and_then(|prev| aerial_landing_lag(pf.character, prev))
                {
                    let (landing_lag, l_cancelled_lag) =
                        (u32::from(landing_lag), u32::from(l_cancelled_lag));
                    if landing_frames < l_cancelled_lag {
                        c.edge_cancel_count.success += 1;
                    }
                    if landing_frames >= l_cancelled_lag && landing_frames < landing_lag {
                        c.edge_cancel_count.slow += 1;
                    }
                    // Edge cancels don't count as failed L-cancels.
                    if landing_frames <= l_cancelled_lag && st.last_l_cancel_status == 2 {
                        c.l_cancel_count.fail -= 1;
                    }
                }
            }

            handle_wavedash(st);

            if let Some(l) = pf.l_cancel.filter(|&l| l > 0) {
                st.last_l_cancel_status = l;
            }
        }
    }
    states.into_iter().map(|s| s.counts).collect()
}

/// Frames spent in the previous animation (the last element is the current action's first frame).
fn prev_animation_length(animations: &[u16]) -> u32 {
    let Some(start) = animations.len().checked_sub(2) else {
        return 0;
    };
    let anim = animations[start];
    animations[..=start]
        .iter()
        .rev()
        .take_while(|&&a| a == anim)
        .count() as u32
}

fn is_wavedash_initiation(a: u16) -> bool {
    a == AIR_DODGE || (CONTROLLED_JUMP_START..=CONTROLLED_JUMP_END).contains(&a)
}

/// slippi-js `handleActionWavedash`: classifies a special landing after a jump or air dodge.
fn handle_wavedash(st: &mut PlayerState) {
    let anims = &st.animations;
    let n = anims.len();
    if n < 2 || anims[n - 1] != LANDING_FALL_SPECIAL || !is_wavedash_initiation(anims[n - 2]) {
        return;
    }

    let recent_start = n.saturating_sub(WAVEDASH_LOOKBACK);
    let recent = &anims[recent_start..];
    let has_air_dodge = recent.contains(&AIR_DODGE);
    // Most recent air dodge frame within the window.
    let air_dodge_end = recent
        .iter()
        .rposition(|&a| a == AIR_DODGE)
        .map(|i| recent_start + i);

    let mut distinct: Vec<u16> = Vec::with_capacity(4);
    for &a in recent {
        if !distinct.contains(&a) {
            distinct.push(a);
        }
    }
    if distinct.len() == 2 && has_air_dodge {
        // Only air dodge and the landing: a very late air dodge, not a wavedash.
        if air_dodge_end.is_some() {
            st.last_handled_air_dodge_end_index = air_dodge_end;
        }
        return;
    }

    if has_air_dodge
        && let Some(end) = air_dodge_end
        && st.last_handled_air_dodge_end_index.is_none_or(|h| end > h)
    {
        // Air dodges used to wavedash/waveland don't count as air dodges (once per air dodge).
        st.counts.air_dodge_count -= 1;
        st.last_handled_air_dodge_end_index = Some(end);
    }

    if let Some(knee_bend) = recent.iter().rposition(|&a| a == ACTION_KNEE_BEND) {
        // Airborne jump frames (excluding knee bend itself).
        let jump_frames = recent
            .iter()
            .filter(|&&a| a > CONTROLLED_JUMP_START && a <= CONTROLLED_JUMP_END)
            .count();
        let ys = &st.positions_y[recent_start..];
        let y_difference = f64::from(ys[ys.len() - 1]) - f64::from(ys[knee_bend]);
        let changed_y = y_difference.abs() > 0.1;
        // Five or more airborne frames with a height change: traveled through the air.
        if jump_frames >= 5 && changed_y {
            st.counts.waveland_count += 1;
        } else {
            st.counts.wavedash_count += 1;
        }
    } else {
        st.counts.waveland_count += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prev_animation_length_counts_run_before_current() {
        assert_eq!(prev_animation_length(&[1, 2, 2, 2, 3]), 3);
        assert_eq!(prev_animation_length(&[2, 2, 3]), 2);
        assert_eq!(prev_animation_length(&[3]), 0);
    }

    fn state_with(anims: &[u16], ys: &[f32]) -> PlayerState {
        PlayerState {
            counts: ActionCounts::default(),
            animations: anims.to_vec(),
            action_frame_counters: vec![None; anims.len()],
            positions_y: ys.to_vec(),
            last_l_cancel_status: 0,
            last_handled_air_dodge_end_index: None,
        }
    }

    #[test]
    fn wavedash_vs_waveland() {
        // Knee bend, one jump frame, air dodge, landing: wavedash; the air dodge is refunded.
        let anims = [
            ACTION_WAIT,
            ACTION_KNEE_BEND,
            JUMP_FORWARD,
            AIR_DODGE,
            LANDING_FALL_SPECIAL,
        ];
        let mut st = state_with(&anims, &[0.0; 5]);
        handle_wavedash(&mut st);
        assert_eq!(st.counts.wavedash_count, 1);
        assert_eq!(st.counts.air_dodge_count, -1);

        // A second landing inside the window doesn't refund the same air dodge again.
        st.animations.extend([JUMP_FORWARD, LANDING_FALL_SPECIAL]);
        st.positions_y.extend([0.0, 0.0]);
        handle_wavedash(&mut st);
        assert_eq!(st.counts.air_dodge_count, -1);
        assert_eq!(st.counts.wavedash_count, 2);

        // No knee bend in the window: waveland.
        let anims = [FALL, FALL, AIR_DODGE, LANDING_FALL_SPECIAL];
        let mut st = state_with(&anims, &[5.0, 4.0, 3.0, 0.0]);
        handle_wavedash(&mut st);
        assert_eq!(st.counts.waveland_count, 1);

        // Many airborne jump frames with a height change: waveland, even with a knee bend.
        let mut anims = vec![ACTION_KNEE_BEND];
        anims.extend([JUMP_FORWARD; 6]);
        anims.extend([AIR_DODGE, LANDING_FALL_SPECIAL]);
        let mut ys = vec![0.0; anims.len()];
        *ys.last_mut().unwrap() = 20.0;
        let mut st = state_with(&anims, &ys);
        handle_wavedash(&mut st);
        assert_eq!((st.counts.wavedash_count, st.counts.waveland_count), (0, 1));
    }
}
