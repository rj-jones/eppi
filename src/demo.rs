//! Demo mode (`--showcase`): other players' names, codes and tags become
//! made-up ones, the same for a player everywhere, so screenshots can be
//! shared without anyone else's identity in them. Your own codes and every
//! number stay as they are.

use crate::library::db::RankRecord;
use crate::model::{Game, Me, Player, hash_str};
use std::collections::HashMap;

const FIRST: [&str; 24] = [
    "Pixel", "Ledge", "Static", "Mellow", "Turbo", "Lucky", "Quiet", "Neon", "Velvet", "Frosty",
    "Rusty", "Swift", "Hazel", "Cobalt", "Maple", "Ember", "Juniper", "Nimbus", "Sable", "Comet",
    "Indigo", "Clover", "Tundra", "Saffron",
];
const SECOND: [&str; 20] = [
    "Fox", "Rook", "Wren", "Otter", "Sprout", "Pilot", "Raven", "Moth", "Pike", "Finch", "Badger",
    "Koi", "Lynx", "Heron", "Mole", "Yak", "Gecko", "Crane", "Newt", "Marten",
];

/// The made-up name for `seed` ("Cobalt Heron").
fn name_for(seed: &str) -> String {
    let h = hash_str(seed);
    format!(
        "{} {}",
        FIRST[(h % FIRST.len() as u64) as usize],
        SECOND[((h >> 16) % SECOND.len() as u64) as usize]
    )
}

/// The made-up code for a real one ("COHE#412").
pub fn code(real: &str) -> String {
    if real.is_empty() {
        return String::new();
    }
    let name = name_for(real);
    let letters: String = name
        .split(' ')
        .flat_map(|w| w.chars().take(2))
        .collect::<String>()
        .to_uppercase();
    format!("{letters}#{}", 100 + hash_str(&format!("#{real}")) % 900)
}

fn player(p: &mut Player, me: &Me) {
    if me.is_me(&p.code) {
        return;
    }
    let seed = if !p.code.is_empty() {
        p.code.clone()
    } else if !p.name.is_empty() {
        p.name.clone()
    } else {
        p.tag.clone()
    };
    if seed.is_empty() {
        return;
    }
    if !p.name.is_empty() {
        p.name = name_for(&seed);
    }
    if !p.tag.is_empty() {
        p.tag = name_for(&seed)
            .split(' ')
            .next()
            .unwrap_or("")
            .to_uppercase();
    }
    p.code = code(&p.code);
}

pub fn game(game: &mut Game, me: &Me) {
    for p in &mut game.players {
        player(p, me);
    }
}

/// Ranks under the made-up codes (mine kept).
pub fn ranks(ranks: HashMap<String, RankRecord>, me: &Me) -> HashMap<String, RankRecord> {
    ranks
        .into_iter()
        .map(|(real, mut record)| {
            if !me.is_me(&real) {
                record.code = code(&real);
                record.display_name = record.display_name.map(|_| name_for(&real));
            }
            (record.code.clone(), record)
        })
        .collect()
}

/// Any map keyed by code, under the made-up codes (mine kept).
pub fn keys<V>(map: HashMap<String, V>, me: &Me) -> HashMap<String, V> {
    map.into_iter()
        .map(|(real, v)| (if me.is_me(&real) { real } else { code(&real) }, v))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_player_gets_the_same_made_up_identity_everywhere() {
        assert_eq!(code("ABC#123"), code("ABC#123"));
        assert_ne!(code("ABC#123"), "ABC#123");
        assert!(code("ABC#123").contains('#'));
        let me = Me::new(&["BEAN#888".to_owned()]);
        let mut mine = Player {
            code: "BEAN#888".into(),
            name: "BEAN".into(),
            ..Default::default()
        };
        player(&mut mine, &me);
        assert_eq!(mine.code, "BEAN#888", "my own code stays");
        let mut them = Player {
            code: "ABC#123".into(),
            name: "abc".into(),
            ..Default::default()
        };
        player(&mut them, &me);
        assert_eq!(them.code, code("ABC#123"));
        assert_eq!(them.name, name_for("ABC#123"));
    }
}
