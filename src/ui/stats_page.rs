//! A game's stats, as Slippi Launcher shows them: Overview, Kills,
//! Punishes and Actions, with "play from here" on every kill and punish.

use crate::app::{Action, Data, display_rank};
use crate::dolphin::Clip;
use crate::melee;
use crate::model::{Game, Outcome, Player};
use crate::ui::theme::{self, Ion};
use crate::ui::widgets;
use eframe::egui::{self, Margin, RichText, vec2};
use eppi_stats::{GameStats, Ratio};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Tab {
    Overview,
    Kills,
    Punishes,
    Actions,
}

pub struct Page {
    pub game: Game,
    stats: Option<GameStats>,
    tab: Tab,
    /// Punishes: only the ones that killed, or over this much damage.
    kills_only: bool,
}

/// Seconds into the game ("GO!" is frame 0) as m:ss.
fn clock(frame: i32) -> String {
    crate::time::duration((frame.max(0)) as f64 / 60.0)
}

/// Starts a little before `frame`, so the moment has context.
fn play_from(game: &Game, frame: i32, lead: i32) -> Action {
    Action::Play(vec![Clip {
        path: game.path.clone(),
        start_frame: Some((frame - lead).max(melee::FIRST_FRAME)),
        end_frame: None,
    }])
}

impl Page {
    pub fn new(game: Game, json: Option<String>) -> Self {
        let stats = json.and_then(|j| serde_json::from_str(&j).ok());
        Self {
            game,
            stats,
            tab: Tab::Overview,
            kills_only: false,
        }
    }

    fn players<'a>(&'a self, data: &Data) -> Vec<&'a Player> {
        match self.game.sides(&data.me) {
            Some(s) => vec![&self.game.players[s.me], &self.game.players[s.opponent]],
            None => self.game.players.iter().collect(),
        }
    }

    pub fn show(&mut self, ui: &mut egui::Ui, data: &Data, actions: &mut Vec<Action>) {
        // Keep the latest copy of the game (its winners arrive with stats).
        if let Some(game) = data.game(self.game.id)
            && game.stats_version != self.game.stats_version
        {
            self.game = game.clone();
        }
        egui::Frame::new()
            .fill(theme::SIDEBAR)
            .inner_margin(Margin::symmetric(16, 10))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                self.header(ui, data, actions);
            });
        egui::Frame::new()
            .inner_margin(Margin::symmetric(16, 8))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    for (tab, label) in [
                        (Tab::Overview, "Overview"),
                        (Tab::Kills, "Kills"),
                        (Tab::Punishes, "Punishes"),
                        (Tab::Actions, "Actions"),
                    ] {
                        if ui
                            .add(egui::Button::new(label).selected(self.tab == tab))
                            .clicked()
                        {
                            self.tab = tab;
                        }
                    }
                });
            });
        let Some(stats) = self.stats.as_ref() else {
            ui.add_space(60.0);
            ui.vertical_centered(|ui| {
                ui.label(Ion::HOURGLASS.rich(36.0).color(theme::FAINT));
                ui.label(theme::muted(if self.game.stats_version == 0 {
                    "This game's stats are still being worked out. Come back in a moment."
                } else {
                    "Eppi couldn't read this replay's frames, so it has no stats."
                }));
            });
            return;
        };
        let players: Vec<Player> = self.players(data).into_iter().cloned().collect();
        let game = self.game.clone();
        egui::ScrollArea::vertical()
            .id_salt("stats-page")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                egui::Frame::new()
                    .inner_margin(Margin::symmetric(16, 4))
                    .show(ui, |ui| match self.tab {
                        Tab::Overview => overview(ui, stats, &players),
                        Tab::Kills => kills(ui, &game, stats, &players, actions),
                        Tab::Punishes => {
                            punishes(ui, &game, stats, &players, &mut self.kills_only, actions)
                        }
                        Tab::Actions => action_counts(ui, stats, &players),
                    });
            });
    }

    fn header(&self, ui: &mut egui::Ui, data: &Data, actions: &mut Vec<Action>) {
        ui.horizontal(|ui| {
            if ui
                .add(theme::icon_button(Ion::ARROW_BACK, "Back"))
                .on_hover_text("Esc")
                .clicked()
            {
                actions.push(Action::CloseStats);
            }
            ui.add_space(theme::space::M);
            let players = self.players(data);
            for (i, player) in players.iter().enumerate() {
                if i > 0 {
                    ui.label(theme::muted("vs"));
                }
                widgets::stock_icon(ui, player.character, player.costume, 30.0);
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = 0.0;
                    ui.label(theme::heavy(player.display_name(), theme::text::LIST));
                    let rank = data
                        .rank(&player.code)
                        .and_then(|r| r.rank.as_deref())
                        .map(display_rank)
                        .unwrap_or_default();
                    let line = if rank.is_empty() {
                        player.code.clone()
                    } else {
                        format!("{} · {rank}", player.code)
                    };
                    ui.label(theme::muted(line).size(theme::text::SMALL));
                });
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .add(theme::primary_icon_button(Ion::PLAY, "Play"))
                    .clicked()
                {
                    actions.push(Action::Play(vec![Clip::whole(&self.game.path)]));
                }
                let outcome = self.game.outcome(&data.me);
                if outcome != Outcome::Unknown {
                    theme::pill(
                        ui,
                        crate::app::outcome_word(outcome),
                        widgets::outcome_color(outcome),
                    );
                }
                ui.label(theme::muted(format!(
                    "{} · {} · {}",
                    melee::stage_name(self.game.stage),
                    crate::time::duration(self.game.seconds()),
                    crate::time::long(self.game.start_at)
                )));
            });
        });
    }
}

fn overall(stats: &GameStats, port: u8) -> Option<&eppi_stats::Overall> {
    stats.overall.iter().find(|o| o.player_index == port)
}

fn ratio_text(r: &Ratio, percent: bool, decimals: usize) -> String {
    match r.ratio {
        Some(v) if percent => format!("{:.*}%", decimals, v * 100.0),
        Some(v) => format!("{v:.decimals$}"),
        None => "–".to_owned(),
    }
}

/// A two- (or more-) column table: a label, then a value per player, the
/// better one in green.
/// A table row: its label, each player's (text, value), and whether more
/// is better (`None`: don't colour it).
type TableRow<'a> = (&'a str, Vec<(String, Option<f64>)>, Option<bool>);

fn table(ui: &mut egui::Ui, id: &str, players: &[Player], rows: Vec<TableRow<'_>>) {
    theme::card().show(ui, |ui| {
        ui.set_width(ui.available_width());
        egui::Grid::new(id)
            .num_columns(players.len() + 1)
            .spacing([40.0, 8.0])
            .striped(true)
            .show(ui, |ui| {
                ui.label("");
                for p in players {
                    ui.horizontal(|ui| {
                        widgets::stock_icon(ui, p.character, p.costume, 18.0);
                        ui.label(theme::caption(&p.display_name()));
                    });
                }
                ui.end_row();
                for (label, values, higher_is_better) in rows {
                    ui.label(theme::muted(label));
                    let best = higher_is_better.and_then(|higher| {
                        let numbers: Vec<f64> = values.iter().filter_map(|(_, v)| *v).collect();
                        if numbers.len() < 2
                            || numbers.windows(2).all(|w| (w[0] - w[1]).abs() < 1e-9)
                        {
                            return None;
                        }
                        Some(if higher {
                            numbers.iter().cloned().fold(f64::MIN, f64::max)
                        } else {
                            numbers.iter().cloned().fold(f64::MAX, f64::min)
                        })
                    });
                    for (text, value) in values {
                        let is_best =
                            best.is_some_and(|b| value.is_some_and(|v| (v - b).abs() < 1e-9));
                        ui.label(RichText::new(text).strong().color(if is_best {
                            theme::WIN
                        } else {
                            theme::TEXT
                        }));
                    }
                    ui.end_row();
                }
            });
    });
}

fn overview(ui: &mut egui::Ui, stats: &GameStats, players: &[Player]) {
    let get = |port: u8| overall(stats, port);
    let row = |label: &'static str,
               higher: Option<bool>,
               f: &dyn Fn(&eppi_stats::Overall) -> (String, Option<f64>)|
     -> TableRow<'static> {
        (
            label,
            players
                .iter()
                .map(|p| get(p.port).map_or(("–".to_owned(), None), f))
                .collect(),
            higher,
        )
    };
    ui.label(theme::heavy("Offense", theme::text::TITLE));
    table(
        ui,
        "overview-offense",
        players,
        vec![
            row("Kills", Some(true), &|o| {
                (o.kill_count.to_string(), Some(o.kill_count as f64))
            }),
            row("Damage done", Some(true), &|o| {
                (format!("{:.1}%", o.total_damage), Some(o.total_damage))
            }),
            row("Openings / kill", Some(false), &|o| {
                (
                    ratio_text(&o.openings_per_kill, false, 1),
                    o.openings_per_kill.ratio,
                )
            }),
            row("Damage / opening", Some(true), &|o| {
                (
                    ratio_text(&o.damage_per_opening, false, 1),
                    o.damage_per_opening.ratio,
                )
            }),
            row("Punishes", None, &|o| {
                (o.conversion_count.to_string(), None)
            }),
            row("Successful punishes", Some(true), &|o| {
                (
                    ratio_text(&o.successful_conversions, true, 0),
                    o.successful_conversions.ratio,
                )
            }),
        ],
    );
    ui.add_space(theme::space::M);
    ui.label(theme::heavy("Neutral", theme::text::TITLE));
    table(
        ui,
        "overview-neutral",
        players,
        vec![
            row("Neutral wins", Some(true), &|o| {
                (
                    ratio_text(&o.neutral_win_ratio, true, 0),
                    o.neutral_win_ratio.ratio,
                )
            }),
            row("Counter hits", Some(true), &|o| {
                (
                    ratio_text(&o.counter_hit_ratio, true, 0),
                    o.counter_hit_ratio.ratio,
                )
            }),
            row("Beneficial trades", Some(true), &|o| {
                (
                    ratio_text(&o.beneficial_trade_ratio, true, 0),
                    o.beneficial_trade_ratio.ratio,
                )
            }),
        ],
    );
    ui.add_space(theme::space::M);
    ui.label(theme::heavy("Inputs", theme::text::TITLE));
    table(
        ui,
        "overview-inputs",
        players,
        vec![
            row("Inputs / minute", Some(true), &|o| {
                (
                    ratio_text(&o.inputs_per_minute, false, 0),
                    o.inputs_per_minute.ratio,
                )
            }),
            row("Digital inputs / minute", Some(true), &|o| {
                (
                    ratio_text(&o.digital_inputs_per_minute, false, 0),
                    o.digital_inputs_per_minute.ratio,
                )
            }),
            row("Total inputs", None, &|o| {
                (o.input_counts.total.to_string(), None)
            }),
        ],
    );
}

fn who(players: &[Player], port: Option<u8>) -> Option<&Player> {
    port.and_then(|p| players.iter().find(|q| q.port == p))
}

fn kills(
    ui: &mut egui::Ui,
    game: &Game,
    stats: &GameStats,
    players: &[Player],
    actions: &mut Vec<Action>,
) {
    ui.columns(players.len().max(1), |columns| {
        for (column, player) in columns.iter_mut().zip(players) {
            column.horizontal(|ui| {
                widgets::stock_icon(ui, player.character, player.costume, 22.0);
                ui.label(theme::heavy(
                    format!("{} lost", player.display_name()),
                    theme::text::TITLE,
                ));
            });
            column.add_space(4.0);
            let lost: Vec<_> = stats
                .stocks
                .iter()
                .filter(|s| s.player_index == player.port && s.end_frame.is_some())
                .collect();
            if lost.is_empty() {
                column.label(theme::muted("No stocks lost."));
            }
            for stock in lost {
                theme::card().show(column, |ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal(|ui| {
                        ui.label(theme::heavy(
                            format!("Stock {}", stock.count),
                            theme::text::BODY,
                        ));
                        ui.label(theme::muted(clock(stock.end_frame.unwrap_or(0))));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui
                                .add(egui::Button::new(Ion::PLAY.rich(14.0)))
                                .on_hover_text("Play from just before this kill")
                                .clicked()
                            {
                                actions.push(play_from(game, stock.end_frame.unwrap_or(0), 300));
                            }
                        });
                    });
                    let by = who(players, stock.killed_by).map(|p| p.display_name());
                    let what = stock.kill_move_id.map(eppi_stats::melee::move_name);
                    let line = match (by, what) {
                        (Some(by), Some(what)) => format!("{what} from {by}"),
                        (None, Some(what)) => what.to_owned(),
                        (Some(by), None) => format!("by {by}"),
                        (None, None) => "Self-destruct".to_owned(),
                    };
                    ui.label(line);
                    ui.label(
                        theme::muted(format!(
                            "at {:.0}% · took {:.0}% this stock",
                            stock.end_percent.unwrap_or(stock.current_percent),
                            stock.end_percent.unwrap_or(stock.current_percent)
                                - stock.start_percent
                        ))
                        .size(theme::text::SMALL),
                    );
                });
                column.add_space(4.0);
            }
        }
    });
    let _ = vec2(0.0, 0.0);
}

fn punishes(
    ui: &mut egui::Ui,
    game: &Game,
    stats: &GameStats,
    players: &[Player],
    kills_only: &mut bool,
    actions: &mut Vec<Action>,
) {
    ui.horizontal(|ui| {
        ui.checkbox(kills_only, "Only punishes that killed");
        ui.label(theme::muted(format!(
            "{} punishes",
            stats.conversions.len()
        )));
    });
    ui.add_space(4.0);
    for c in &stats.conversions {
        if *kills_only && !c.did_kill {
            continue;
        }
        let attacker = who(players, c.last_hit_by);
        let end = c.end_percent.unwrap_or(c.current_percent);
        let damage = end - c.start_percent;
        let (rect, response) =
            ui.allocate_exact_size(vec2(ui.available_width(), 46.0), egui::Sense::click());
        if response.hovered() {
            ui.painter().rect_filled(rect, 6, theme::HOVERED_ROW);
        }
        let mut child = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(rect.shrink2(vec2(8.0, 4.0)))
                .layout(egui::Layout::left_to_right(egui::Align::Center)),
        );
        let ui2 = &mut child;
        ui2.label(theme::muted(clock(c.start_frame)).size(theme::text::SMALL));
        ui2.add_space(6.0);
        if let Some(a) = attacker {
            widgets::stock_icon(ui2, a.character, a.costume, 22.0);
        }
        ui2.vertical(|ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            let moves: Vec<&str> = c
                .moves
                .iter()
                .map(|m| eppi_stats::melee::move_short_name(m.move_id))
                .collect();
            ui.label(
                RichText::new(if moves.is_empty() {
                    "–".to_owned()
                } else {
                    moves.join(" › ")
                })
                .size(theme::text::BODY),
            );
            ui.label(
                theme::muted(format!(
                    "{} · {:.0}% to {:.0}% · {} hit{}",
                    crate::ui::highlights::opening_label(&format!("{:?}", c.opening_type)),
                    c.start_percent,
                    end,
                    c.moves.len(),
                    if c.moves.len() == 1 { "" } else { "s" }
                ))
                .size(theme::text::SMALL),
            );
        });
        ui2.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui
                .add(egui::Button::new(Ion::PLAY.rich(14.0)))
                .on_hover_text("Play this punish")
                .clicked()
            {
                actions.push(Action::Play(vec![Clip {
                    path: game.path.clone(),
                    start_frame: Some((c.start_frame - 90).max(melee::FIRST_FRAME)),
                    end_frame: c.end_frame.map(|f| f + 90),
                }]));
            }
            if c.did_kill {
                theme::pill(ui, "Killed", theme::DANGER);
            }
            ui.label(
                theme::heavy(format!("{damage:.0}%"), theme::text::TITLE).color(
                    if damage >= 60.0 {
                        theme::ACCENT_TEXT
                    } else {
                        theme::TEXT
                    },
                ),
            );
        });
    }
}

fn action_counts(ui: &mut egui::Ui, stats: &GameStats, players: &[Player]) {
    let get = |port: u8| stats.action_counts.iter().find(|a| a.player_index == port);
    let row = |label: &'static str,
               f: &dyn Fn(&eppi_stats::ActionCounts) -> String|
     -> TableRow<'static> {
        (
            label,
            players
                .iter()
                .map(|p| (get(p.port).map_or("–".into(), f), None))
                .collect(),
            None,
        )
    };
    let sf = |s: &eppi_stats::SuccessFail| {
        let total = s.success + s.fail;
        if total == 0 {
            "–".to_owned()
        } else {
            format!(
                "{} / {} ({:.0}%)",
                s.success,
                total,
                100.0 * s.success as f64 / total as f64
            )
        }
    };
    ui.label(theme::heavy("Movement", theme::text::TITLE));
    table(
        ui,
        "actions-movement",
        players,
        vec![
            row("Wavedashes", &|a| a.wavedash_count.to_string()),
            row("Wavelands", &|a| a.waveland_count.to_string()),
            row("Dash dances", &|a| a.dash_dance_count.to_string()),
            row("Air dodges", &|a| a.air_dodge_count.to_string()),
            row("Spot dodges", &|a| a.spot_dodge_count.to_string()),
            row("Rolls", &|a| a.roll_count.to_string()),
            row("Ledge grabs", &|a| a.ledgegrab_count.to_string()),
            row("Taunts", &|a| a.taunt_count.to_string()),
        ],
    );
    ui.add_space(theme::space::M);
    ui.label(theme::heavy("Tech", theme::text::TITLE));
    table(
        ui,
        "actions-tech",
        players,
        vec![
            row("L-cancels", &|a| sf(&a.l_cancel_count)),
            row("Grabs", &|a| sf(&a.grab_count)),
            row("Wall techs", &|a| sf(&a.wall_tech_count)),
            row("Ground techs (in / away / neutral / missed)", &|a| {
                let t = &a.ground_tech_count;
                format!("{} / {} / {} / {}", t.in_, t.away, t.neutral, t.fail)
            }),
            row("Throws (up / forward / back / down)", &|a| {
                let t = &a.throw_count;
                format!("{} / {} / {} / {}", t.up, t.forward, t.back, t.down)
            }),
        ],
    );
    ui.add_space(theme::space::M);
    ui.label(theme::heavy("Attacks", theme::text::TITLE));
    table(
        ui,
        "actions-attacks",
        players,
        vec![
            row("Jabs", &|a| {
                (a.attack_count.jab1
                    + a.attack_count.jab2
                    + a.attack_count.jab3
                    + a.attack_count.jabm)
                    .to_string()
            }),
            row("Dash attacks", &|a| a.attack_count.dash.to_string()),
            row("Tilts (f / u / d)", &|a| {
                format!(
                    "{} / {} / {}",
                    a.attack_count.ftilt, a.attack_count.utilt, a.attack_count.dtilt
                )
            }),
            row("Smashes (f / u / d)", &|a| {
                format!(
                    "{} / {} / {}",
                    a.attack_count.fsmash, a.attack_count.usmash, a.attack_count.dsmash
                )
            }),
            row("Aerials (n / f / b / u / d)", &|a| {
                let c = &a.attack_count;
                format!(
                    "{} / {} / {} / {} / {}",
                    c.nair, c.fair, c.bair, c.uair, c.dair
                )
            }),
        ],
    );
}
