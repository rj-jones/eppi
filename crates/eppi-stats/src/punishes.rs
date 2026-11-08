//! Combos and conversions (slippi-js `combos.ts` and `conversions.ts`).
//!
//! Both computers share the same hit/move tracking; they differ only in when the punish
//! ends (the "reset counter").

use crate::frames::Frames;
use crate::melee::state::{
    is_command_grabbed, is_damaged, is_dead, is_down, is_grabbed, is_in_control, is_teching,
};
use crate::stocks::did_lose_stock;
use crate::types::{Combo, Conversion, MoveLanded, OpeningType};

/// slippi-js `Timers.PUNISH_RESET_FRAMES` / `COMBO_STRING_RESET_FRAMES`.
const RESET_FRAMES: u32 = 45;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Combo,
    Conversion,
}

/// Fields shared by combos and conversions.
struct Punish {
    victim: u8,
    attacker: u8,
    start_frame: i32,
    end_frame: Option<i32>,
    start_percent: f64,
    current_percent: f64,
    end_percent: Option<f64>,
    moves: Vec<MoveLanded>,
    did_kill: bool,
}

#[derive(Default)]
struct State {
    /// Index of the punish in progress.
    current: Option<usize>,
    /// Index (into the current punish's moves) of the move being counted.
    move_index: Option<usize>,
    reset_counter: u32,
    last_hit_animation: Option<u16>,
}

fn compute(frames: &Frames, perms: &[(usize, usize)], kind: Kind) -> Vec<Punish> {
    let mut punishes: Vec<Punish> = Vec::new();
    let mut states: Vec<State> = perms.iter().map(|_| State::default()).collect();
    for k in 0..frames.len {
        let frame_number = Frames::frame_number(k);
        for (state, &(p, o)) in states.iter_mut().zip(perms) {
            let player = &frames.players[p];
            let opponent = &frames.players[o];
            let pf = &player.post[k];
            let of = &opponent.post[k];
            let prev = k
                .checked_sub(1)
                .map(|j| (&player.post[j], &opponent.post[j]));

            let opp_state = of.state;
            let opp_damaged = is_damaged(opp_state);
            let opp_grabbed = is_grabbed(opp_state);
            let opp_command_grabbed = is_command_grabbed(opp_state);
            let damage_taken =
                prev.map_or(0.0, |(_, po)| f64::from(of.percent) - f64::from(po.percent));

            // A hit only counts as a new move once the attacker's animation changes or restarts
            // (so multi-hit moves count once, but repeated jabs count each time).
            let action_changed_since_hit = Some(pf.state) != state.last_hit_animation;
            let prev_action_counter = match prev {
                Some((pp, _)) => pp.state_age,
                None => Some(0.0),
            };
            let action_counter_reset = matches!(
                (pf.state_age, prev_action_counter),
                (Some(cur), Some(prev)) if cur < prev
            );
            if action_changed_since_hit || action_counter_reset {
                state.last_hit_animation = None;
            }

            let opp_in_stun = opp_damaged || opp_grabbed || opp_command_grabbed;
            if opp_in_stun {
                if state.current.is_none() {
                    state.current = Some(punishes.len());
                    punishes.push(Punish {
                        victim: opponent.player_index,
                        attacker: player.player_index,
                        start_frame: frame_number,
                        end_frame: None,
                        start_percent: prev.map_or(0.0, |(_, po)| f64::from(po.percent)),
                        current_percent: f64::from(of.percent),
                        end_percent: None,
                        moves: Vec::new(),
                        did_kill: false,
                    });
                }
                let punish = &mut punishes[state.current.unwrap_or_default()];

                // JS truthiness: any non-zero, non-NaN damage (including negative) counts.
                if damage_taken != 0.0 && !damage_taken.is_nan() {
                    if state.last_hit_animation.is_none() {
                        state.move_index = Some(punish.moves.len());
                        punish.moves.push(MoveLanded {
                            player_index: player.player_index,
                            frame: frame_number,
                            move_id: pf.last_attack_landed,
                            hit_count: 0,
                            damage: 0.0,
                        });
                    }
                    if let Some(m) = state.move_index.and_then(|i| punish.moves.get_mut(i)) {
                        m.hit_count += 1;
                        m.damage += damage_taken;
                    }
                    // The previous frame's animation is the one that connected (handles trades).
                    state.last_hit_animation = prev.map(|(pp, _)| pp.state);
                }
            }

            let Some(current) = state.current else {
                continue;
            };
            let punish = &mut punishes[current];

            let opp_lost_stock = prev.is_some_and(|(_, po)| did_lose_stock(of, po));
            if !opp_lost_stock {
                punish.current_percent = f64::from(of.percent);
            }

            match kind {
                Kind::Conversion => {
                    if opp_in_stun {
                        state.reset_counter = 0;
                    }
                    let start = state.reset_counter == 0 && is_in_control(opp_state);
                    if start || state.reset_counter > 0 {
                        state.reset_counter += 1;
                    }
                }
                Kind::Combo => {
                    if opp_in_stun
                        || is_teching(opp_state)
                        || is_down(opp_state)
                        || is_dead(opp_state)
                    {
                        state.reset_counter = 0;
                    } else {
                        state.reset_counter += 1;
                    }
                }
            }

            let mut terminate = false;
            if opp_lost_stock {
                punish.did_kill = true;
                terminate = true;
            }
            if state.reset_counter > RESET_FRAMES {
                terminate = true;
            }
            if terminate {
                punish.end_frame = Some(frame_number);
                punish.end_percent = Some(prev.map_or(0.0, |(_, po)| f64::from(po.percent)));
                state.current = None;
                state.move_index = None;
            }
        }
    }
    punishes
}

/// Combos for singles permutations.
pub(crate) fn combos(frames: &Frames, perms: &[(usize, usize)]) -> Vec<Combo> {
    compute(frames, perms, Kind::Combo)
        .into_iter()
        .map(|p| Combo {
            player_index: p.victim,
            start_frame: p.start_frame,
            end_frame: p.end_frame,
            start_percent: p.start_percent,
            current_percent: p.current_percent,
            end_percent: p.end_percent,
            moves: p.moves,
            did_kill: p.did_kill,
            last_hit_by: Some(p.attacker),
        })
        .collect()
}

/// Conversions for singles permutations, with opening types filled in.
pub(crate) fn conversions(frames: &Frames, perms: &[(usize, usize)]) -> Vec<Conversion> {
    let mut conversions: Vec<Conversion> = compute(frames, perms, Kind::Conversion)
        .into_iter()
        .map(|p| Conversion {
            player_index: p.victim,
            last_hit_by: Some(p.attacker),
            start_frame: p.start_frame,
            end_frame: p.end_frame,
            start_percent: p.start_percent,
            current_percent: p.current_percent,
            end_percent: p.end_percent,
            moves: p.moves,
            did_kill: p.did_kill,
            opening_type: OpeningType::Unknown,
        })
        .collect();
    populate_opening_types(&mut conversions);
    conversions
}

/// slippi-js `ConversionComputer._populateConversionTypes`.
///
/// Conversions starting on the same frame are trades. Otherwise a conversion is a counter-attack
/// if the latest conversion *on the attacker* (processed so far, in start-frame order) ends after
/// this one starts. Quirks kept from slippi-js: an end frame of 0 or "not ended" never counts, and
/// a conversion without moves compares against its own end frame.
fn populate_opening_types(conversions: &mut [Conversion]) {
    // Groups of indices with equal start frames, ordered by start frame, insertion order inside.
    let mut order: Vec<usize> = (0..conversions.len()).collect();
    order.sort_by_key(|&i| conversions[i].start_frame);
    // Last end frame of a conversion on each player index (victim). JS `undefined` -> None.
    let mut last_end_frame: [Option<Option<i32>>; 256] = [None; 256];
    let mut i = 0;
    while i < order.len() {
        let start = conversions[order[i]].start_frame;
        let mut j = i;
        while j < order.len() && conversions[order[j]].start_frame == start {
            j += 1;
        }
        let is_trade = j - i >= 2;
        for &ci in &order[i..j] {
            let c = &mut conversions[ci];
            last_end_frame[c.player_index as usize] = Some(c.end_frame);
            if is_trade {
                c.opening_type = OpeningType::Trade;
                continue;
            }
            let key = c.moves.last().map_or(c.player_index, |m| m.player_index);
            let opp_end = last_end_frame[key as usize].flatten();
            let is_counter = matches!(opp_end, Some(e) if e != 0 && e > c.start_frame);
            c.opening_type = if is_counter {
                OpeningType::CounterAttack
            } else {
                OpeningType::NeutralWin
            };
        }
        i = j;
    }
}
