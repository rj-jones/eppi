//! The stats pass: a full replay read and run through `eppi-stats`, then
//! cut into what the app stores (the stats JSON for the stats page, compact
//! numbers per player for the aggregate views, and punishes and kills as
//! rows for Highlights and the matchup report).

use super::db::{KillRow, PunishRow, StatsUpdate};
use crate::model::Numbers;
use eppi_stats::GameStats;
use std::path::Path;

/// The stats version stored with each game: the stats crate's, plus ours
/// for what we cut from it.
pub const VERSION: u32 = eppi_stats::STATS_VERSION * 100 + 1;

pub fn compute(id: i64, path: &Path) -> StatsUpdate {
    let failed = || StatsUpdate {
        id,
        version: VERSION,
        winners: Vec::new(),
        stocks_left: Vec::new(),
        numbers: Vec::new(),
        punishes: Vec::new(),
        kills: Vec::new(),
        json: None,
    };
    let stats = std::panic::catch_unwind(|| {
        let game = eppi_stats::read_replay(path).ok()?;
        Some(eppi_stats::compute(&game))
    });
    let Ok(Some(stats)) = stats else {
        log::info!("no stats for {}", path.display());
        return failed();
    };
    StatsUpdate {
        id,
        version: VERSION,
        winners: stats.winners.iter().map(|w| w.player_index).collect(),
        stocks_left: stats
            .players
            .iter()
            .map(|p| (p.player_index, p.final_stocks))
            .collect(),
        numbers: numbers(&stats),
        punishes: stats
            .conversions
            .iter()
            .map(|c| PunishRow {
                attacker: c.last_hit_by,
                victim: c.player_index,
                start_frame: c.start_frame,
                end_frame: c.end_frame,
                start_percent: c.start_percent,
                end_percent: c.end_percent.unwrap_or(c.current_percent),
                hits: c.moves.len() as u32,
                did_kill: c.did_kill,
                opening: format!("{:?}", c.opening_type),
                first_move: c.moves.first().map(|m| m.move_id),
                last_move: c.moves.last().map(|m| m.move_id),
            })
            .collect(),
        kills: stats
            .stocks
            .iter()
            .filter(|s| s.end_frame.is_some())
            .map(|s| KillRow {
                victim: s.player_index,
                killer: s.killed_by,
                stock: s.count,
                frame: s.end_frame,
                percent: s.end_percent,
                move_id: s.kill_move_id,
            })
            .collect(),
        json: serde_json::to_string(&stats).ok(),
    }
}

fn pair(r: &eppi_stats::Ratio) -> (f64, f64) {
    (r.count, r.total)
}

fn numbers(stats: &GameStats) -> Vec<Numbers> {
    stats
        .overall
        .iter()
        .map(|o| {
            let actions = stats
                .action_counts
                .iter()
                .find(|a| a.player_index == o.player_index);
            let mut n = Numbers {
                port: o.player_index,
                kills: o.kill_count,
                damage: o.total_damage,
                conversions: o.conversion_count,
                openings_per_kill: pair(&o.openings_per_kill),
                damage_per_opening: pair(&o.damage_per_opening),
                neutral_wins: pair(&o.neutral_win_ratio),
                counter_hits: pair(&o.counter_hit_ratio),
                trades: pair(&o.beneficial_trade_ratio),
                inputs_per_minute: pair(&o.inputs_per_minute),
                ..Default::default()
            };
            if let Some(a) = actions {
                let l = &a.l_cancel_count;
                n.l_cancels = (l.success.max(0) as u32, (l.success + l.fail).max(0) as u32);
                let t = &a.ground_tech_count;
                let teched = (t.away + t.in_ + t.neutral).max(0) as u32;
                n.techs = (teched, teched + t.fail.max(0) as u32);
                n.wavedashes = a.wavedash_count.max(0) as u32;
                n.dash_dances = a.dash_dance_count.max(0) as u32;
                n.ledgegrabs = a.ledgegrab_count.max(0) as u32;
                n.rolls = a.roll_count.max(0) as u32;
                n.spot_dodges = a.spot_dodge_count.max(0) as u32;
                n.air_dodges = a.air_dodge_count.max(0) as u32;
                let g = &a.grab_count;
                n.grabs = (g.success.max(0) as u32, (g.success + g.fail).max(0) as u32);
            }
            n
        })
        .collect()
}
