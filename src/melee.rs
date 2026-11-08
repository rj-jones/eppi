//! Names and colours for Melee's characters and stages, by the external
//! ids Slippi records.

/// (full name, short name) by external character id.
const CHARACTERS: [(&str, &str); 33] = [
    ("Captain Falcon", "Falcon"),
    ("Donkey Kong", "DK"),
    ("Fox", "Fox"),
    ("Mr. Game & Watch", "G&W"),
    ("Kirby", "Kirby"),
    ("Bowser", "Bowser"),
    ("Link", "Link"),
    ("Luigi", "Luigi"),
    ("Mario", "Mario"),
    ("Marth", "Marth"),
    ("Mewtwo", "Mewtwo"),
    ("Ness", "Ness"),
    ("Peach", "Peach"),
    ("Pikachu", "Pikachu"),
    ("Ice Climbers", "ICs"),
    ("Jigglypuff", "Puff"),
    ("Samus", "Samus"),
    ("Yoshi", "Yoshi"),
    ("Zelda", "Zelda"),
    ("Sheik", "Sheik"),
    ("Falco", "Falco"),
    ("Young Link", "YLink"),
    ("Dr. Mario", "Doc"),
    ("Roy", "Roy"),
    ("Pichu", "Pichu"),
    ("Ganondorf", "Ganon"),
    ("Master Hand", "M.Hand"),
    ("Wireframe (male)", "Wire M"),
    ("Wireframe (female)", "Wire F"),
    ("Giga Bowser", "Giga"),
    ("Crazy Hand", "C.Hand"),
    ("Sandbag", "Sandbag"),
    ("Popo", "Popo"),
];

/// The playable characters' ids, in the order the select screen reads.
pub const PLAYABLE: [u8; 26] = [
    22, 8, 7, 5, 12, 17, 1, 0, 25, 20, 2, 11, 14, 4, 16, 18, 19, 6, 21, 24, 13, 15, 10, 3, 9, 23,
];

pub fn character_name(id: u8) -> &'static str {
    CHARACTERS
        .get(id as usize)
        .map_or("Unknown", |(name, _)| name)
}

pub fn character_short(id: u8) -> &'static str {
    CHARACTERS.get(id as usize).map_or("?", |(_, short)| short)
}

pub fn stage_name(id: u16) -> &'static str {
    match id {
        2 => "Fountain of Dreams",
        3 => "Pokémon Stadium",
        4 => "Princess Peach's Castle",
        5 => "Kongo Jungle",
        6 => "Brinstar",
        7 => "Corneria",
        8 => "Yoshi's Story",
        9 => "Onett",
        10 => "Mute City",
        11 => "Rainbow Cruise",
        12 => "Jungle Japes",
        13 => "Great Bay",
        14 => "Hyrule Temple",
        15 => "Brinstar Depths",
        16 => "Yoshi's Island",
        17 => "Green Greens",
        18 => "Fourside",
        19 => "Mushroom Kingdom I",
        20 => "Mushroom Kingdom II",
        22 => "Venom",
        23 => "Poké Floats",
        24 => "Big Blue",
        25 => "Icicle Mountain",
        26 => "Icetop",
        27 => "Flat Zone",
        28 => "Dream Land N64",
        29 => "Yoshi's Island N64",
        30 => "Kongo Jungle N64",
        31 => "Battlefield",
        32 => "Final Destination",
        84 => "Target Test",
        _ => "Unknown stage",
    }
}

pub fn stage_short(id: u16) -> &'static str {
    match id {
        2 => "FoD",
        3 => "Stadium",
        8 => "Yoshi's",
        28 => "Dream Land",
        31 => "Battlefield",
        32 => "FD",
        _ => stage_name(id),
    }
}

/// The tournament-legal stages, in the order the filter lists them.
pub const LEGAL_STAGES: [u16; 6] = [31, 32, 28, 8, 2, 3];

/// Frames per second.
pub const FPS: f64 = 60.0;
/// Slippi numbers frames from -123 (the first frame of the countdown).
pub const FIRST_FRAME: i32 = -123;

/// Seconds a game with `last_frame` lasted, from "GO!".
pub fn game_seconds(last_frame: i32) -> f64 {
    (last_frame.max(-39) + 39) as f64 / FPS
}
