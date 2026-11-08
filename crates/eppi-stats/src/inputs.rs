//! Input counts (slippi-js `inputs.ts`).

use crate::frames::{FIRST_PLAYABLE_FRAME, Frames};
use crate::types::InputCounts;

/// Joystick region (slippi-js `JoystickRegion`); 0 is the deadzone.
fn joystick_region(x: f32, y: f32) -> u8 {
    // slippi-js compares the f32 values as doubles.
    const T: f64 = 0.2875;
    let (x, y) = (f64::from(x), f64::from(y));
    if x >= T && y >= T {
        1 // NE
    } else if x >= T && y <= -T {
        2 // SE
    } else if x <= -T && y <= -T {
        3 // SW
    } else if x <= -T && y >= T {
        4 // NW
    } else if y >= T {
        5 // N
    } else if x >= T {
        6 // E
    } else if y <= -T {
        7 // S
    } else if x <= -T {
        8 // W
    } else {
        0
    }
}

/// Input counts per singles permutation, as `(player_index, counts)`.
pub(crate) fn compute(frames: &Frames, perms: &[(usize, usize)]) -> Vec<(u8, InputCounts)> {
    const TRIGGER: f64 = 0.3;
    perms
        .iter()
        .map(|&(p, _)| {
            let player = &frames.players[p];
            let mut c = InputCounts::default();
            for k in 1..frames.len {
                if Frames::frame_number(k) < FIRST_PLAYABLE_FRAME {
                    continue;
                }
                let (Some(cur), Some(prev)) = (player.pre[k], player.pre[k - 1]) else {
                    continue;
                };

                let pressed = (!prev.buttons_physical & cur.buttons_physical & 0xfff).count_ones();
                c.buttons += pressed;
                c.total += pressed;

                let (prev_region, region) = (
                    joystick_region(prev.joystick_x, prev.joystick_y),
                    joystick_region(cur.joystick_x, cur.joystick_y),
                );
                if prev_region != region && region != 0 {
                    c.joystick += 1;
                    c.total += 1;
                }
                let (prev_region, region) = (
                    joystick_region(prev.cstick_x, prev.cstick_y),
                    joystick_region(cur.cstick_x, cur.cstick_y),
                );
                if prev_region != region && region != 0 {
                    c.cstick += 1;
                    c.total += 1;
                }

                for (prev_t, cur_t) in [
                    (prev.trigger_l, cur.trigger_l),
                    (prev.trigger_r, cur.trigger_r),
                ] {
                    if f64::from(prev_t) < TRIGGER && f64::from(cur_t) >= TRIGGER {
                        c.triggers += 1;
                        c.total += 1;
                    }
                }
            }
            (player.player_index, c)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::joystick_region;

    #[test]
    fn regions() {
        assert_eq!(joystick_region(0.0, 0.0), 0);
        assert_eq!(joystick_region(0.3, 0.3), 1);
        assert_eq!(joystick_region(0.5, -0.5), 2);
        assert_eq!(joystick_region(0.0, -1.0), 7);
        assert_eq!(joystick_region(-1.0, 0.1), 8);
        assert_eq!(joystick_region(0.28, 0.0), 0);
    }
}
