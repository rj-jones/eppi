//! Overall stats (slippi-js `overall.ts`).

use crate::types::{Conversion, InputCounts, OpeningType, Overall, Ratio};

/// A settings player: index and team id (only meaningful in teams games).
pub(crate) struct SettingsPlayer {
    pub index: u8,
    pub team: Option<u8>,
}

/// Opening player for grouping: the first move's attacker, `None` (slippi-js -1) without moves.
fn opener(c: &Conversion) -> Option<u8> {
    c.moves.first().map(|m| m.player_index)
}

fn openings<'a>(
    conversions: &'a [Conversion],
    player: u8,
    kind: OpeningType,
) -> impl Iterator<Item = &'a Conversion> + 'a {
    conversions
        .iter()
        .filter(move |c| opener(c) == Some(player) && c.opening_type == kind)
}

pub(crate) fn compute(
    players: &[SettingsPlayer],
    is_teams: bool,
    inputs: &[(u8, InputCounts)],
    conversions: &[Conversion],
    playable_frame_count: i32,
) -> Vec<Overall> {
    let game_minutes = f64::from(playable_frame_count) / 3600.0;
    players
        .iter()
        .map(|player| {
            let me = player.index;
            let input_counts = inputs
                .iter()
                .find(|(i, _)| *i == me)
                .map(|(_, c)| *c)
                .unwrap_or_default();

            let opponents: Vec<u8> = players
                .iter()
                .filter(|o| o.index != me && (!is_teams || o.team != player.team))
                .map(|o| o.index)
                .collect();

            let mut conversion_count = 0u32;
            let mut successful = 0u32;
            let mut total_damage = 0.0f64;
            let mut kill_count = 0u32;
            for c in conversions.iter().filter(|c| c.player_index != me) {
                conversion_count += 1;
                if c.did_kill && c.last_hit_by == Some(me) {
                    kill_count += 1;
                }
                if c.moves.len() > 1 && c.moves[0].player_index == me {
                    successful += 1;
                }
                for m in c.moves.iter().filter(|m| m.player_index == me) {
                    total_damage += m.damage;
                }
            }

            let opening_ratio = |kind: OpeningType| {
                let mine = openings(conversions, me, kind).count();
                let theirs: usize = opponents
                    .iter()
                    .map(|&o| openings(conversions, o, kind).count())
                    .sum();
                Ratio::new(mine as f64, (mine + theirs) as f64)
            };

            Overall {
                player_index: me,
                input_counts,
                conversion_count,
                total_damage,
                kill_count,
                successful_conversions: Ratio::new(successful.into(), conversion_count.into()),
                inputs_per_minute: Ratio::new(input_counts.total.into(), game_minutes),
                digital_inputs_per_minute: Ratio::new(input_counts.buttons.into(), game_minutes),
                openings_per_kill: Ratio::new(conversion_count.into(), kill_count.into()),
                damage_per_opening: Ratio::new(total_damage, conversion_count.into()),
                neutral_win_ratio: opening_ratio(OpeningType::NeutralWin),
                counter_hit_ratio: opening_ratio(OpeningType::CounterAttack),
                beneficial_trade_ratio: beneficial_trade_ratio(conversions, me, &opponents),
            }
        })
        .collect()
}

/// Pairs this player's trades with the opponents' trades in order and counts the ones where
/// this player killed (and the opponent didn't) or did more damage.
fn beneficial_trade_ratio(conversions: &[Conversion], me: u8, opponents: &[u8]) -> Ratio {
    let mine: Vec<&Conversion> = openings(conversions, me, OpeningType::Trade).collect();
    let theirs: Vec<&Conversion> = opponents
        .iter()
        .flat_map(|&o| openings(conversions, o, OpeningType::Trade))
        .collect();
    let benefits = mine
        .iter()
        .zip(&theirs)
        .filter(|(p, o)| {
            let player_damage = p.current_percent - p.start_percent;
            let opponent_damage = o.current_percent - o.start_percent;
            (p.did_kill && !o.did_kill) || player_damage > opponent_damage
        })
        .count();
    Ratio::new(benefits as f64, mine.len() as f64)
}
