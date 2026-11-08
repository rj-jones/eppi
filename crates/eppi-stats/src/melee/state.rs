//! Action-state ids and predicates, as defined by slippi-js (`stats/common.ts`).
//!
//! Melee reuses some ids between characters (e.g. `GNW_UAIR` and `PEACH_FSMASH1` are both
//! `0x15d`), so character-specific ids only mean something together with the character.

#![allow(missing_docs)]

// Ranges.
pub const DAMAGE_START: u16 = 0x4b;
pub const DAMAGE_END: u16 = 0x5b;
pub const CAPTURE_START: u16 = 0xdf;
pub const CAPTURE_END: u16 = 0xe8;
pub const GUARD_START: u16 = 0xb2;
pub const GUARD_END: u16 = 0xb6;
pub const GROUNDED_CONTROL_START: u16 = 0xe;
pub const GROUNDED_CONTROL_END: u16 = 0x18;
pub const SQUAT_START: u16 = 0x27;
pub const SQUAT_END: u16 = 0x29;
pub const DOWN_START: u16 = 0xb7;
pub const DOWN_END: u16 = 0xc6;
pub const TECH_START: u16 = 0xc7;
pub const TECH_END: u16 = 0xcc;
pub const DYING_START: u16 = 0x0;
pub const DYING_END: u16 = 0xa;
pub const CONTROLLED_JUMP_START: u16 = 0x18;
pub const CONTROLLED_JUMP_END: u16 = 0x22;
pub const GROUND_ATTACK_START: u16 = 0x2c;
pub const GROUND_ATTACK_END: u16 = 0x40;
pub const AERIAL_ATTACK_START: u16 = 0x41;
pub const AERIAL_LANDING_START: u16 = 0x46;
pub const AERIAL_LANDING_END: u16 = 0x4a;
pub const ATTACK_FTILT_START: u16 = 0x33;
pub const ATTACK_FTILT_END: u16 = 0x37;
pub const ATTACK_FSMASH_START: u16 = 0x3a;
pub const ATTACK_FSMASH_END: u16 = 0x3e;

// Specific states.
pub const ROLL_FORWARD: u16 = 0xe9;
pub const ROLL_BACKWARD: u16 = 0xea;
pub const SPOT_DODGE: u16 = 0xeb;
pub const AIR_DODGE: u16 = 0xec;
pub const ACTION_WAIT: u16 = 0xe;
pub const ACTION_DASH: u16 = 0x14;
pub const ACTION_KNEE_BEND: u16 = 0x18;
pub const GUARD_ON: u16 = 0xb2;
pub const TECH_MISS_UP: u16 = 0xb7;
pub const JAB_RESET_UP: u16 = 0xb9;
pub const TECH_MISS_DOWN: u16 = 0xbf;
pub const JAB_RESET_DOWN: u16 = 0xc1;
pub const NEUTRAL_TECH: u16 = 0xc7;
pub const FORWARD_TECH: u16 = 0xc8;
pub const BACKWARD_TECH: u16 = 0xc9;
pub const WALL_TECH: u16 = 0xca;
pub const MISSED_WALL_TECH: u16 = 0xf7;
pub const DASH: u16 = 0x14;
pub const TURN: u16 = 0x12;
pub const LANDING_FALL_SPECIAL: u16 = 0x2b;
pub const JUMP_FORWARD: u16 = 0x19;
pub const JUMP_BACKWARD: u16 = 0x1a;
pub const FALL: u16 = 0x1d;
pub const FALL_FORWARD: u16 = 0x1e;
pub const FALL_BACKWARD: u16 = 0x1f;
pub const GRAB: u16 = 0xd4;
pub const DASH_GRAB: u16 = 0xd6;
pub const GRAB_WAIT: u16 = 0xd8;
pub const PUMMEL: u16 = 0xd9;
pub const CLIFF_CATCH: u16 = 0xfc;
pub const THROW_UP: u16 = 0xdd;
pub const THROW_FORWARD: u16 = 0xdb;
pub const THROW_DOWN: u16 = 0xde;
pub const THROW_BACK: u16 = 0xdc;
pub const DAMAGE_FALL: u16 = 0x26;
pub const ATTACK_JAB1: u16 = 0x2c;
pub const ATTACK_JAB2: u16 = 0x2d;
pub const ATTACK_JAB3: u16 = 0x2e;
pub const ATTACK_JABM: u16 = 0x2f;
pub const ATTACK_DASH: u16 = 0x32;
pub const ATTACK_UTILT: u16 = 0x38;
pub const ATTACK_DTILT: u16 = 0x39;
pub const ATTACK_USMASH: u16 = 0x3f;
pub const ATTACK_DSMASH: u16 = 0x40;
pub const AERIAL_NAIR: u16 = 0x41;
pub const AERIAL_FAIR: u16 = 0x42;
pub const AERIAL_BAIR: u16 = 0x43;
pub const AERIAL_UAIR: u16 = 0x44;
pub const AERIAL_DAIR: u16 = 0x45;
pub const AERIAL_NAIR_LANDING: u16 = 0x46;
pub const AERIAL_FAIR_LANDING: u16 = 0x47;
pub const AERIAL_BAIR_LANDING: u16 = 0x48;
pub const AERIAL_UAIR_LANDING: u16 = 0x49;
pub const AERIAL_DAIR_LANDING: u16 = 0x4a;
pub const TEETER: u16 = 0xf5;

// Taunts.
pub const TAUNT_LEFT: u16 = 0x108;
pub const TAUNT_RIGHT: u16 = 0x109;

// Mr. Game & Watch's unique ids.
pub const GNW_JAB1: u16 = 0x155;
pub const GNW_JABM: u16 = 0x156;
pub const GNW_DTILT: u16 = 0x159;
pub const GNW_FSMASH: u16 = 0x15a;
pub const GNW_NAIR: u16 = 0x15b;
pub const GNW_BAIR: u16 = 0x15c;
pub const GNW_UAIR: u16 = 0x15d;

// Peach's forward smashes: golf club, frying pan, tennis racket.
pub const PEACH_FSMASH1: u16 = 0x15d;
pub const PEACH_FSMASH2: u16 = 0x15e;
pub const PEACH_FSMASH3: u16 = 0x15f;

// Command grabs.
pub const BARREL_WAIT: u16 = 0x125;
pub const COMMAND_GRAB_RANGE1_START: u16 = 0x10a;
pub const COMMAND_GRAB_RANGE1_END: u16 = 0x130;
pub const COMMAND_GRAB_RANGE2_START: u16 = 0x147;
pub const COMMAND_GRAB_RANGE2_END: u16 = 0x152;

/// Actionable: standing/walking/dashing/turning, crouching, a ground attack (except jab 1), or grab.
pub fn is_in_control(state: u16) -> bool {
    let ground = (GROUNDED_CONTROL_START..=GROUNDED_CONTROL_END).contains(&state);
    let squat = (SQUAT_START..=SQUAT_END).contains(&state);
    let ground_attack = state > GROUND_ATTACK_START && state <= GROUND_ATTACK_END;
    ground || squat || ground_attack || state == GRAB
}

/// Teching (neutral/forward/backward/wall tech states).
pub fn is_teching(state: u16) -> bool {
    (TECH_START..=TECH_END).contains(&state)
}

/// Lying on the ground (missed tech, jab reset, getup states).
pub fn is_down(state: u16) -> bool {
    (DOWN_START..=DOWN_END).contains(&state)
}

/// In hitstun/damage states, tumble, or jab reset.
pub fn is_damaged(state: u16) -> bool {
    (DAMAGE_START..=DAMAGE_END).contains(&state)
        || state == DAMAGE_FALL
        || state == JAB_RESET_UP
        || state == JAB_RESET_DOWN
}

/// Held by a normal grab.
pub fn is_grabbed(state: u16) -> bool {
    (CAPTURE_START..=CAPTURE_END).contains(&state)
}

/// Held by a command grab (e.g. Bowser's side-B), excluding DK's barrel wait.
pub fn is_command_grabbed(state: u16) -> bool {
    ((COMMAND_GRAB_RANGE1_START..=COMMAND_GRAB_RANGE1_END).contains(&state)
        || (COMMAND_GRAB_RANGE2_START..=COMMAND_GRAB_RANGE2_END).contains(&state))
        && state != BARREL_WAIT
}

/// In a dying state (blast-zone KO animations).
pub fn is_dead(state: u16) -> bool {
    state <= DYING_END
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn predicates() {
        assert!(is_dead(0) && is_dead(0xa) && !is_dead(0xb));
        assert!(is_damaged(0x4b) && is_damaged(DAMAGE_FALL) && is_damaged(JAB_RESET_DOWN));
        assert!(!is_damaged(0x5c));
        // Jab 1 is a ground attack but not "in control"; jab 2 is.
        assert!(!is_in_control(ATTACK_JAB1) && is_in_control(ATTACK_JAB2));
        assert!(is_in_control(GRAB) && !is_in_control(DASH_GRAB));
        assert!(is_command_grabbed(0x10a) && !is_command_grabbed(BARREL_WAIT));
        assert!(is_grabbed(CAPTURE_START) && !is_grabbed(CAPTURE_END + 1));
    }
}
