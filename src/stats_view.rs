//! Reading a game's stats back from the index (cached for the frames that
//! show them) and the side-by-side tables the details pane draws.

use crate::model::{Game, Player};
use crate::ui::theme;
use eframe::egui::{self, Color32, RichText};
use eppi_stats::{GameStats, Ratio};
use std::sync::Arc;

/// The stats of `game`, read from the index once per game and stats
/// version and kept in egui's memory while it's shown.
pub fn cached(
    ui: &egui::Ui,
    conn: Option<&rusqlite::Connection>,
    game: &Game,
) -> Option<Arc<GameStats>> {
    if game.stats_version == 0 {
        return None;
    }
    let id = egui::Id::unique(("game-stats", game.id, game.stats_version));
    if let Some(hit) = ui.data(|d| d.get_temp::<Option<Arc<GameStats>>>(id)) {
        return hit;
    }
    let loaded = conn
        .and_then(|c| {
            crate::library::db::load_stats_json(c, game.id)
                .ok()
                .flatten()
        })
        .and_then(|json| serde_json::from_str::<GameStats>(&json).ok())
        .map(Arc::new);
    ui.data_mut(|d| d.insert_temp(id, loaded.clone()));
    loaded
}

pub fn overall(stats: &GameStats, port: u8) -> Option<&eppi_stats::Overall> {
    stats.overall.iter().find(|o| o.player_index == port)
}

pub fn actions(stats: &GameStats, port: u8) -> Option<&eppi_stats::ActionCounts> {
    stats.action_counts.iter().find(|a| a.player_index == port)
}

/// One line of a comparison: how to read it from the stats, how to show
/// it, and whether more is better.
struct Line {
    label: &'static str,
    tip: &'static str,
    higher_is_better: bool,
    value: fn(&GameStats, u8) -> Option<(f64, f64)>,
    show: fn(f64) -> String,
}

fn ratio(r: &Ratio) -> Option<(f64, f64)> {
    (r.total > 0.0).then_some((r.count, r.total))
}

const LINES: &[Line] = &[
    Line {
        label: "Kills",
        tip: "Stocks taken",
        higher_is_better: true,
        value: |s, p| overall(s, p).map(|o| (o.kill_count as f64, 1.0)),
        show: |v| format!("{v:.0}"),
    },
    Line {
        label: "Damage done",
        tip: "Percent dealt over the game",
        higher_is_better: true,
        value: |s, p| overall(s, p).map(|o| (o.total_damage, 1.0)),
        show: |v| format!("{v:.0}%"),
    },
    Line {
        label: "Openings / kill",
        tip: "How many openings it took per stock taken (fewer is better)",
        higher_is_better: false,
        value: |s, p| overall(s, p).and_then(|o| ratio(&o.openings_per_kill)),
        show: |v| format!("{v:.1}"),
    },
    Line {
        label: "Damage / opening",
        tip: "Average percent dealt per opening",
        higher_is_better: true,
        value: |s, p| overall(s, p).and_then(|o| ratio(&o.damage_per_opening)),
        show: |v| format!("{v:.1}"),
    },
    Line {
        label: "Neutral wins",
        tip: "Share of neutral exchanges won",
        higher_is_better: true,
        value: |s, p| overall(s, p).and_then(|o| ratio(&o.neutral_win_ratio)),
        show: |v| format!("{:.0}%", v * 100.0),
    },
    Line {
        label: "Counter hits",
        tip: "Share of counter hits won",
        higher_is_better: true,
        value: |s, p| overall(s, p).and_then(|o| ratio(&o.counter_hit_ratio)),
        show: |v| format!("{:.0}%", v * 100.0),
    },
    Line {
        label: "Inputs / min",
        tip: "Button, stick and trigger changes per minute",
        higher_is_better: true,
        value: |s, p| overall(s, p).and_then(|o| ratio(&o.inputs_per_minute)),
        show: |v| format!("{v:.0}"),
    },
    Line {
        label: "L-cancels",
        tip: "Successful L-cancels / all aerials landed",
        higher_is_better: true,
        value: |s, p| {
            actions(s, p).and_then(|a| {
                let total = (a.l_cancel_count.success + a.l_cancel_count.fail) as f64;
                (total > 0.0).then_some((a.l_cancel_count.success as f64, total))
            })
        },
        show: |v| format!("{:.0}%", v * 100.0),
    },
];

fn compare_color(a: Option<f64>, b: Option<f64>, higher_is_better: bool) -> (Color32, Color32) {
    match (a, b) {
        (Some(a), Some(b)) if (a - b).abs() > 1e-9 => {
            let a_better = (a > b) == higher_is_better;
            if a_better {
                (theme::WIN, theme::TEXT)
            } else {
                (theme::TEXT, theme::WIN)
            }
        }
        _ => (theme::TEXT, theme::TEXT),
    }
}

/// Me (first) against them, line by line.
pub fn headline_table(ui: &mut egui::Ui, _game: &Game, stats: &GameStats, players: &[&Player]) {
    let summed: Vec<(&GameStats, Vec<u8>)> =
        vec![(stats, players.iter().map(|p| p.port).collect())];
    comparison(ui, &summed, players);
}

/// The same lines summed over a set's games (`games` with their stats).
pub fn set_table(ui: &mut egui::Ui, games: &[(&Game, Arc<GameStats>)], players: &[&Player]) {
    // Ports can change between games of a set; follow players by code.
    let mut rows = Vec::new();
    for (game, stats) in games {
        let ports: Vec<u8> = players
            .iter()
            .map(|p| {
                game.players
                    .iter()
                    .find(|q| !p.code.is_empty() && q.code == p.code)
                    .map_or(p.port, |q| q.port)
            })
            .collect();
        rows.push((stats.as_ref(), ports));
    }
    comparison(ui, &rows, players);
}

fn comparison(ui: &mut egui::Ui, games: &[(&GameStats, Vec<u8>)], players: &[&Player]) {
    if players.len() != 2 {
        return;
    }
    egui::Grid::new(ui.next_auto_id())
        .num_columns(3)
        .spacing([18.0, 6.0])
        .striped(false)
        .show(ui, |ui| {
            ui.label("");
            for player in players {
                ui.label(
                    theme::muted(crate::melee::character_short(player.character))
                        .size(theme::text::SMALL),
                );
            }
            ui.end_row();
            for line in LINES {
                let mut sums = [(0.0, 0.0, false); 2];
                for (stats, ports) in games {
                    for side in 0..2 {
                        if let Some((count, total)) = (line.value)(stats, ports[side]) {
                            sums[side].0 += count;
                            sums[side].1 += total;
                            sums[side].2 = true;
                        }
                    }
                }
                let value = |s: (f64, f64, bool)| (s.2 && s.1 > 0.0).then(|| s.0 / s.1);
                let (a, b) = (value(sums[0]), value(sums[1]));
                let (ca, cb) = compare_color(a, b, line.higher_is_better);
                ui.label(theme::muted(line.label)).on_hover_text(line.tip);
                ui.label(
                    RichText::new(a.map_or("–".into(), line.show))
                        .color(ca)
                        .strong(),
                );
                ui.label(
                    RichText::new(b.map_or("–".into(), line.show))
                        .color(cb)
                        .strong(),
                );
                ui.end_row();
            }
        });
}

#[cfg(test)]
mod tests {
    #[test]
    #[ignore]
    fn stored_stats_read_back() {
        let json = std::fs::read_to_string("target/sample-stats.json").unwrap();
        let stats: eppi_stats::GameStats = serde_json::from_str(&json).unwrap();
        assert!(!stats.overall.is_empty());
    }
}
