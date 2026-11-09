//! Me: how you're doing under the filters. Your record and rank, your
//! numbers next to your opponents', how you do by matchup, stage and
//! character, a week-by-week table to see whether you're improving, and a
//! report on one matchup (what kills you, what you kill with, which of your
//! openings pay off).

use crate::app::{Data, Derived, display_rank};
use crate::melee;
use crate::model::{Game, Numbers, Outcome, Row, set_score};
use crate::ui::theme::{self, Ion};
use crate::ui::widgets;
use eframe::egui::{self, Margin, RichText, vec2};
use std::collections::HashMap;

/// Sums of `Numbers` over many games.
#[derive(Debug, Clone, Default)]
pub struct Totals {
    pub games: u32,
    pub kills: u32,
    pub damage: f64,
    pub openings_per_kill: (f64, f64),
    pub damage_per_opening: (f64, f64),
    pub neutral_wins: (f64, f64),
    pub counter_hits: (f64, f64),
    pub inputs_per_minute: (f64, f64),
    pub l_cancels: (u32, u32),
    pub techs: (u32, u32),
    pub grabs: (u32, u32),
    pub wavedashes: u32,
    pub dash_dances: u32,
}

impl Totals {
    pub fn add(&mut self, n: &Numbers) {
        fn add2(a: &mut (f64, f64), b: (f64, f64)) {
            a.0 += b.0;
            a.1 += b.1;
        }
        fn add2u(a: &mut (u32, u32), b: (u32, u32)) {
            a.0 += b.0;
            a.1 += b.1;
        }
        self.games += 1;
        self.kills += n.kills;
        self.damage += n.damage;
        add2(&mut self.openings_per_kill, n.openings_per_kill);
        add2(&mut self.damage_per_opening, n.damage_per_opening);
        add2(&mut self.neutral_wins, n.neutral_wins);
        add2(&mut self.counter_hits, n.counter_hits);
        add2(&mut self.inputs_per_minute, n.inputs_per_minute);
        add2u(&mut self.l_cancels, n.l_cancels);
        add2u(&mut self.techs, n.techs);
        add2u(&mut self.grabs, n.grabs);
        self.wavedashes += n.wavedashes;
        self.dash_dances += n.dash_dances;
    }
}

fn ratio(pair: (f64, f64)) -> Option<f64> {
    (pair.1 > 0.0).then(|| pair.0 / pair.1)
}

fn ratio_u(pair: (u32, u32)) -> Option<f64> {
    (pair.1 > 0).then(|| pair.0 as f64 / pair.1 as f64)
}

/// One stat line in the comparison and trend tables.
struct Stat {
    label: &'static str,
    tip: &'static str,
    /// `None`: a style number, neither better nor worse.
    higher_is_better: Option<bool>,
    value: fn(&Totals) -> Option<f64>,
    show: fn(f64) -> String,
}

const STATS: &[Stat] = &[
    Stat {
        label: "Openings / kill",
        tip: "Openings it takes you to take a stock (fewer is better)",
        higher_is_better: Some(false),
        value: |t| ratio(t.openings_per_kill),
        show: |v| format!("{v:.2}"),
    },
    Stat {
        label: "Damage / opening",
        tip: "Average percent per opening",
        higher_is_better: Some(true),
        value: |t| ratio(t.damage_per_opening),
        show: |v| format!("{v:.1}"),
    },
    Stat {
        label: "Neutral wins",
        tip: "Share of neutral exchanges won",
        higher_is_better: Some(true),
        value: |t| ratio(t.neutral_wins),
        show: |v| format!("{:.1}%", v * 100.0),
    },
    Stat {
        label: "Counter hits",
        tip: "Share of counter hits won",
        higher_is_better: Some(true),
        value: |t| ratio(t.counter_hits),
        show: |v| format!("{:.1}%", v * 100.0),
    },
    Stat {
        label: "Inputs / min",
        tip: "Button, stick and trigger changes per minute",
        higher_is_better: Some(true),
        value: |t| ratio(t.inputs_per_minute),
        show: |v| format!("{v:.0}"),
    },
    Stat {
        label: "L-cancels",
        tip: "Successful L-cancels",
        higher_is_better: Some(true),
        value: |t| ratio_u(t.l_cancels),
        show: |v| format!("{:.1}%", v * 100.0),
    },
    Stat {
        label: "Techs",
        tip: "Ground techs out of all knockdowns you could tech",
        higher_is_better: Some(true),
        value: |t| ratio_u(t.techs),
        show: |v| format!("{:.0}%", v * 100.0),
    },
    Stat {
        label: "Grabs",
        tip: "Grabs that connected",
        higher_is_better: Some(true),
        value: |t| ratio_u(t.grabs),
        show: |v| format!("{:.0}%", v * 100.0),
    },
    Stat {
        label: "Kills / game",
        tip: "Stocks taken per game",
        higher_is_better: Some(true),
        value: |t| (t.games > 0).then(|| t.kills as f64 / t.games as f64),
        show: |v| format!("{v:.2}"),
    },
    Stat {
        label: "Damage / game",
        tip: "Percent dealt per game",
        higher_is_better: Some(true),
        value: |t| (t.games > 0).then(|| t.damage / t.games as f64),
        show: |v| format!("{v:.0}%"),
    },
    Stat {
        label: "Wavedashes / game",
        tip: "",
        higher_is_better: None,
        value: |t| (t.games > 0).then(|| t.wavedashes as f64 / t.games as f64),
        show: |v| format!("{v:.1}"),
    },
];

/// Wins and losses.
#[derive(Debug, Clone, Copy, Default)]
struct Record {
    wins: u32,
    losses: u32,
}

impl Record {
    fn add(&mut self, outcome: Outcome) {
        match outcome {
            Outcome::Win => self.wins += 1,
            Outcome::Loss => self.losses += 1,
            Outcome::Unknown => {}
        }
    }
    fn total(&self) -> u32 {
        self.wins + self.losses
    }
}

/// What the view computes from the filtered games (once per change).
#[derive(Default)]
struct Summary {
    games: Record,
    sets: Record,
    seconds: f64,
    opponents: usize,
    mine: Totals,
    theirs: Totals,
    by_matchup: Vec<((u8, u8), Record)>,
    by_stage: Vec<(u16, Record)>,
    by_character: Vec<(u8, Record)>,
    /// (week start, record, my totals), newest first.
    weeks: Vec<(i64, Record, Totals)>,
}

#[derive(Default)]
pub struct State {
    summary: Summary,
    built_for: u64,
    history: Vec<crate::library::db::RankRecord>,
    history_for: (u64, usize),
    matchup: Option<(u8, u8)>,
    report: Option<Report>,
    report_for: Option<(u64, (u8, u8))>,
}

impl State {
    fn rebuild(&mut self, data: &Data, derived: &Derived) {
        if self.built_for == derived.revision {
            return;
        }
        self.built_for = derived.revision;
        let mut s = Summary::default();
        let mut matchups: HashMap<(u8, u8), Record> = HashMap::new();
        let mut stages: HashMap<u16, Record> = HashMap::new();
        let mut characters: HashMap<u8, Record> = HashMap::new();
        let mut weeks: HashMap<i64, (Record, Totals)> = HashMap::new();
        let mut opponents = std::collections::HashSet::new();
        for &i in &derived.order {
            let game = &data.games[i];
            let Some(sides) = game.sides(&data.me) else {
                continue;
            };
            let me = &game.players[sides.me];
            let them = &game.players[sides.opponent];
            let outcome = game.outcome(&data.me);
            s.games.add(outcome);
            s.seconds += game.seconds();
            opponents.insert(them.code.clone());
            matchups
                .entry((me.character, them.character))
                .or_default()
                .add(outcome);
            stages.entry(game.stage).or_default().add(outcome);
            characters.entry(me.character).or_default().add(outcome);
            let week = weeks
                .entry(crate::time::week_start(game.start_at))
                .or_default();
            week.0.add(outcome);
            if let Some(n) = game.numbers(me.port) {
                s.mine.add(n);
                week.1.add(n);
            }
            if let Some(n) = game.numbers(them.port) {
                s.theirs.add(n);
            }
        }
        for row in &derived.rows {
            if let Row::Set(set) = row
                && data.games[set.games[0]].sides(&data.me).is_some()
            {
                let (a, b) = set_score(&data.games, &set.games, &data.me);
                if a > b {
                    s.sets.wins += 1;
                } else if b > a {
                    s.sets.losses += 1;
                }
            }
        }
        s.opponents = opponents.len();
        s.by_matchup = sorted(matchups);
        s.by_stage = sorted(stages);
        s.by_character = sorted(characters);
        let mut weeks: Vec<(i64, Record, Totals)> =
            weeks.into_iter().map(|(w, (r, t))| (w, r, t)).collect();
        weeks.sort_by_key(|w| std::cmp::Reverse(w.0));
        weeks.truncate(16);
        s.weeks = weeks;
        if self
            .matchup
            .is_none_or(|m| !s.by_matchup.iter().any(|(k, _)| *k == m))
        {
            self.matchup = s.by_matchup.first().map(|(k, _)| *k);
        }
        self.summary = s;
    }

    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        data: &Data,
        derived: &Derived,
        conn: Option<&rusqlite::Connection>,
    ) {
        self.rebuild(data, derived);
        if data.me.is_empty() {
            ui.add_space(80.0);
            ui.vertical_centered(|ui| {
                ui.label(Ion::PERSON_CIRCLE.rich(40.0).color(theme::FAINT));
                ui.label(theme::muted(
                    "Add your connect code in Settings to see your numbers.",
                ));
            });
            return;
        }
        egui::ScrollArea::vertical()
            .id_salt("me")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                egui::Frame::new()
                    .inner_margin(Margin::symmetric(20, 14))
                    .show(ui, |ui| {
                        // The filters above hold the period (and season).
                        self.tiles(ui);
                        ui.add_space(theme::space::L);
                        let wide = ui.available_width() > 900.0;
                        if wide {
                            ui.columns(2, |columns| {
                                self.numbers(&mut columns[0]);
                                columns[0].add_space(theme::space::L);
                                self.breakdown(&mut columns[0], "By matchup", Breakdown::Matchup);
                                self.rank(&mut columns[1], data, conn);
                                columns[1].add_space(theme::space::L);
                                self.breakdown(&mut columns[1], "By stage", Breakdown::Stage);
                                columns[1].add_space(theme::space::L);
                                self.breakdown(
                                    &mut columns[1],
                                    "By your character",
                                    Breakdown::Character,
                                );
                            });
                        } else {
                            self.numbers(ui);
                            ui.add_space(theme::space::L);
                            self.rank(ui, data, conn);
                            ui.add_space(theme::space::L);
                            self.breakdown(ui, "By matchup", Breakdown::Matchup);
                            self.breakdown(ui, "By stage", Breakdown::Stage);
                            self.breakdown(ui, "By your character", Breakdown::Character);
                        }
                        ui.add_space(theme::space::L);
                        self.trends(ui);
                        ui.add_space(theme::space::L);
                        self.matchup_report(ui, data, derived, conn);
                    });
            });
    }

    fn tiles(&self, ui: &mut egui::Ui) {
        let s = &self.summary;
        theme::card().show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing.x = 34.0;
                widgets::stat_tile(
                    ui,
                    format!(
                        "{}–{}",
                        widgets::thousands(s.games.wins as usize),
                        widgets::thousands(s.games.losses as usize)
                    ),
                    "Games",
                    widgets::win_rate_color(s.games.wins, s.games.total()),
                );
                widgets::stat_tile(
                    ui,
                    widgets::percent(s.games.wins, s.games.total()),
                    "Win rate",
                    theme::TEXT,
                );
                widgets::stat_tile(
                    ui,
                    format!(
                        "{}–{}",
                        widgets::thousands(s.sets.wins as usize),
                        widgets::thousands(s.sets.losses as usize)
                    ),
                    "Sets",
                    widgets::win_rate_color(s.sets.wins, s.sets.total()),
                );
                widgets::stat_tile(
                    ui,
                    widgets::thousands(s.opponents),
                    "Opponents",
                    theme::TEXT,
                );
                widgets::stat_tile(
                    ui,
                    format!("{:.0} h", s.seconds / 3_600.0),
                    "Played",
                    theme::MUTED,
                );
            });
        });
    }

    fn numbers(&self, ui: &mut egui::Ui) {
        let s = &self.summary;
        ui.label(theme::heavy("Your numbers", theme::text::TITLE));
        ui.label(
            theme::muted("Yours against your opponents', over the same games.")
                .size(theme::text::SMALL),
        );
        ui.add_space(4.0);
        if s.mine.games == 0 {
            ui.label(theme::muted(
                "They appear once the stats of these games are worked out.",
            ));
            return;
        }
        theme::card().show(ui, |ui| {
            ui.set_width(ui.available_width());
            egui::Grid::new("me-numbers")
                .num_columns(3)
                .spacing([24.0, 6.0])
                .show(ui, |ui| {
                    ui.label("");
                    ui.label(theme::caption("You"));
                    ui.label(theme::caption("Opponents"));
                    ui.end_row();
                    for stat in STATS {
                        let a = (stat.value)(&s.mine);
                        let b = (stat.value)(&s.theirs);
                        let better = match (a, b) {
                            (Some(a), Some(b)) if (a - b).abs() > 1e-9 => {
                                stat.higher_is_better.map(|h| (a > b) == h)
                            }
                            _ => None,
                        };
                        let label = ui.label(theme::muted(stat.label));
                        if !stat.tip.is_empty() {
                            label.on_hover_text(stat.tip);
                        }
                        ui.label(
                            RichText::new(a.map_or("–".into(), stat.show))
                                .strong()
                                .color(match better {
                                    Some(true) => theme::WIN,
                                    Some(false) => theme::LOSS,
                                    None => theme::TEXT,
                                }),
                        );
                        ui.label(theme::muted(b.map_or("–".into(), stat.show)));
                        ui.end_row();
                    }
                });
            ui.label(
                theme::muted(format!(
                    "From {} games with stats",
                    widgets::thousands(s.mine.games as usize)
                ))
                .size(theme::text::SMALL)
                .color(theme::FAINT),
            );
        });
    }

    fn rank(&mut self, ui: &mut egui::Ui, data: &Data, conn: Option<&rusqlite::Connection>) {
        ui.label(theme::heavy("Rank", theme::text::TITLE));
        ui.add_space(4.0);
        let Some(code) = data.my_codes.first() else {
            return;
        };
        let key = (
            data.revision,
            data.ranks.get(code).map_or(0, |r| r.fetched_at as usize),
        );
        if self.history_for != key {
            self.history_for = key;
            self.history = conn
                .and_then(|c| crate::library::db::rank_history(c, code).ok())
                .unwrap_or_default();
        }
        theme::card().show(ui, |ui| {
            ui.set_width(ui.available_width());
            for code in &data.my_codes {
                ui.horizontal(|ui| {
                    ui.label(theme::heavy(code, theme::text::BODY));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if let Some(record) = data.rank(code) {
                            if let Some((w, l)) = crate::ranks::season_record(record) {
                                ui.label(theme::muted(format!("{w}–{l} sets this season")));
                            }
                            if let Some(rating) = record.rating {
                                ui.label(theme::muted(format!("{rating:.1}")));
                            }
                            let rank = record.rank.as_deref().map(display_rank).unwrap_or_default();
                            ui.label(RichText::new(&rank).color(widgets::rank_color(&rank)).strong());
                            if let Some(source) = widgets::rank_source(&rank) {
                                ui.add(egui::Image::new(source).fit_to_exact_size(vec2(24.0, 24.0)));
                            }
                        } else {
                            ui.label(theme::muted("Not looked up yet"));
                        }
                    });
                });
            }
            if !data.seasons.is_empty() {
                ui.add_space(8.0);
                ui.label(theme::caption("Seasons"));
                for season in &data.seasons {
                    seasons_row(ui, season);
                }
            }
            if self.history.len() > 1 {
                ui.add_space(6.0);
                ui.label(theme::caption("Rating over time"));
                let mut previous: Option<f64> = None;
                let mut lines = Vec::new();
                for record in self.history.iter().rev() {
                    let Some(rating) = record.rating else { continue };
                    if previous.is_some_and(|p| (p - rating).abs() < 0.05) {
                        continue;
                    }
                    lines.push((record.fetched_at, rating, previous.map(|p| rating - p)));
                    previous = Some(rating);
                }
                for (at, rating, delta) in lines.iter().rev().take(12) {
                    ui.horizontal(|ui| {
                        ui.label(theme::muted(crate::time::short(*at, data.now)).size(theme::text::SMALL));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if let Some(delta) = delta {
                                let color = if *delta >= 0.0 { theme::WIN } else { theme::LOSS };
                                ui.label(RichText::new(format!("{delta:+.1}")).color(color).size(theme::text::SMALL));
                            }
                            ui.label(RichText::new(format!("{rating:.1}")).size(theme::text::SMALL));
                        });
                    });
                }
            } else {
                ui.label(
                    theme::muted("Eppi keeps every rank it looks up, so your rating's history builds up from here.")
                        .size(theme::text::SMALL),
                );
            }
        });
    }

    fn breakdown(&self, ui: &mut egui::Ui, title: &str, which: Breakdown) {
        ui.label(theme::heavy(title, theme::text::TITLE));
        ui.add_space(4.0);
        theme::card().show(ui, |ui| {
            ui.set_width(ui.available_width());
            let rows: Vec<(RowLabel, Record)> = match which {
                Breakdown::Matchup => self
                    .summary
                    .by_matchup
                    .iter()
                    .take(12)
                    .map(|((a, b), r)| {
                        let (a, b) = (*a, *b);
                        (
                            Box::new(move |ui: &mut egui::Ui| {
                                widgets::stock_icon(ui, a, 0, 20.0);
                                ui.label(theme::muted("vs"));
                                widgets::stock_icon(ui, b, 0, 20.0);
                                ui.label(melee::character_short(b));
                            }) as RowLabel,
                            *r,
                        )
                    })
                    .collect(),
                Breakdown::Stage => self
                    .summary
                    .by_stage
                    .iter()
                    .take(10)
                    .map(|(s, r)| {
                        let s = *s;
                        (
                            Box::new(move |ui: &mut egui::Ui| {
                                ui.label(melee::stage_name(s));
                            }) as RowLabel,
                            *r,
                        )
                    })
                    .collect(),
                Breakdown::Character => self
                    .summary
                    .by_character
                    .iter()
                    .take(8)
                    .map(|(c, r)| {
                        let c = *c;
                        (
                            Box::new(move |ui: &mut egui::Ui| {
                                widgets::stock_icon(ui, c, 0, 20.0);
                                ui.label(melee::character_name(c));
                            }) as RowLabel,
                            *r,
                        )
                    })
                    .collect(),
            };
            if rows.is_empty() {
                ui.label(theme::muted("No games."));
            }
            for (label, record) in rows {
                ui.horizontal(|ui| {
                    label(ui);
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        win_bar(ui, record);
                        ui.label(
                            theme::muted(widgets::count(record.total() as usize, "game"))
                                .size(theme::text::SMALL),
                        );
                    });
                });
            }
        });
    }

    fn trends(&self, ui: &mut egui::Ui) {
        ui.label(theme::heavy("Week by week", theme::text::TITLE));
        ui.label(
            theme::muted("Are you getting better? Newest week first.").size(theme::text::SMALL),
        );
        ui.add_space(4.0);
        if self.summary.weeks.is_empty() {
            return;
        }
        let shown = [0usize, 1, 2, 4, 5];
        theme::card().show(ui, |ui| {
            ui.set_width(ui.available_width());
            egui::ScrollArea::horizontal()
                .id_salt("weeks")
                .show(ui, |ui| {
                    egui::Grid::new("me-weeks")
                        .num_columns(3 + shown.len())
                        .spacing([22.0, 6.0])
                        .striped(true)
                        .show(ui, |ui| {
                            ui.label(theme::caption("Week of"));
                            ui.label(theme::caption("Games"));
                            ui.label(theme::caption("Win rate"));
                            for i in shown {
                                ui.label(theme::caption(STATS[i].label))
                                    .on_hover_text(STATS[i].tip);
                            }
                            ui.end_row();
                            let weeks = &self.summary.weeks;
                            for (n, (week, record, totals)) in weeks.iter().enumerate() {
                                let older = weeks.get(n + 1);
                                ui.label(crate::time::day_label(*week));
                                ui.label(record.total().to_string());
                                ui.label(
                                    RichText::new(widgets::percent(record.wins, record.total()))
                                        .color(widgets::win_rate_color(
                                            record.wins,
                                            record.total(),
                                        )),
                                );
                                for i in shown {
                                    let stat = &STATS[i];
                                    let value = (stat.value)(totals);
                                    let before = older.and_then(|(_, _, t)| (stat.value)(t));
                                    let arrow = match (value, before) {
                                        (Some(v), Some(b)) if (v - b).abs() > 1e-6 => {
                                            let up = v > b;
                                            let color = match stat.higher_is_better {
                                                Some(h) if h == up => theme::WIN,
                                                Some(_) => theme::LOSS,
                                                None => theme::MUTED,
                                            };
                                            Some((if up { "+" } else { "−" }, color))
                                        }
                                        _ => None,
                                    };
                                    ui.horizontal(|ui| {
                                        ui.spacing_mut().item_spacing.x = 3.0;
                                        ui.label(value.map_or("–".into(), stat.show));
                                        if let Some((a, c)) = arrow {
                                            ui.label(
                                                RichText::new(a).size(theme::text::TINY).color(c),
                                            );
                                        }
                                    });
                                }
                                ui.end_row();
                            }
                        });
                });
        });
    }

    fn matchup_report(
        &mut self,
        ui: &mut egui::Ui,
        data: &Data,
        derived: &Derived,
        conn: Option<&rusqlite::Connection>,
    ) {
        ui.label(theme::heavy("Matchup report", theme::text::TITLE));
        ui.label(
            theme::muted("What kills you, what you kill with, and which openings pay off.")
                .size(theme::text::SMALL),
        );
        ui.add_space(4.0);
        let Some(current) = self.matchup else {
            ui.label(theme::muted("No singles games under these filters."));
            return;
        };
        let label = |(a, b): (u8, u8)| {
            format!(
                "{} vs {}",
                melee::character_name(a),
                melee::character_name(b)
            )
        };
        egui::ComboBox::from_id_salt("matchup")
            .selected_text(label(current))
            .width(280.0)
            .height(400.0)
            .show_ui(ui, |ui| {
                for (key, record) in &self.summary.by_matchup {
                    ui.selectable_value(
                        &mut self.matchup,
                        Some(*key),
                        format!(
                            "{}  ({})",
                            label(*key),
                            widgets::count(record.total() as usize, "game")
                        ),
                    );
                }
            });
        let Some(current) = self.matchup else { return };
        if self.report_for != Some((derived.revision, current)) {
            self.report_for = Some((derived.revision, current));
            self.report = conn.map(|c| Report::build(c, data, derived, current));
        }
        let Some(report) = &self.report else { return };
        if report.games == 0 {
            ui.label(theme::muted("None of these games has stats yet."));
            return;
        }
        ui.add_space(6.0);
        ui.columns(3, |columns| {
            report_list(
                &mut columns[0],
                "What kills you",
                &report.killed_by,
                "kills",
                |(_, n, pct)| format!("{n} · {pct:.0}% avg"),
            );
            report_list(
                &mut columns[1],
                "What you kill with",
                &report.kills_with,
                "kills",
                |(_, n, pct)| format!("{n} · {pct:.0}% avg"),
            );
            report_list(
                &mut columns[2],
                "Your best openers",
                &report.openers,
                "",
                |(_, n, dmg)| format!("{dmg:.0}% avg · {n}×"),
            );
        });
        ui.label(
            theme::muted(format!(
                "From {} games with stats. You die at {:.0}% on average and kill at {:.0}%.",
                report.games, report.death_percent, report.kill_percent
            ))
            .size(theme::text::SMALL),
        );
    }
}

/// Draws a breakdown row's label.
type RowLabel = Box<dyn Fn(&mut egui::Ui)>;

enum Breakdown {
    Matchup,
    Stage,
    Character,
}

/// Most games first; ties by key, so the order doesn't depend on the
/// HashMap's.
fn sorted<K: Ord>(map: HashMap<K, Record>) -> Vec<(K, Record)> {
    let mut v: Vec<(K, Record)> = map.into_iter().collect();
    v.sort_by(|a, b| b.1.total().cmp(&a.1.total()).then_with(|| a.0.cmp(&b.0)));
    v
}

fn win_bar(ui: &mut egui::Ui, record: Record) {
    let (rect, _) = ui.allocate_exact_size(vec2(70.0, 18.0), egui::Sense::hover());
    let bar = egui::Rect::from_center_size(rect.center(), vec2(70.0, 6.0));
    let painter = ui.painter();
    painter.rect_filled(bar, 3, theme::LOSS.gamma_multiply(0.5));
    if record.total() > 0 {
        let won = bar.width() * record.wins as f32 / record.total() as f32;
        painter.rect_filled(
            egui::Rect::from_min_size(bar.min, vec2(won, bar.height())),
            3,
            theme::WIN,
        );
    }
    ui.label(
        RichText::new(widgets::percent(record.wins, record.total()))
            .strong()
            .color(widgets::win_rate_color(record.wins, record.total())),
    );
}

fn report_list(
    ui: &mut egui::Ui,
    title: &str,
    rows: &[(u8, u32, f64)],
    _unit: &str,
    show: impl Fn(&(u8, u32, f64)) -> String,
) {
    theme::card().show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.label(theme::caption(title));
        if rows.is_empty() {
            ui.label(theme::muted("–"));
        }
        for row in rows.iter().take(6) {
            ui.horizontal(|ui| {
                ui.label(eppi_stats::melee::move_name(row.0));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(theme::muted(show(row)).size(theme::text::SMALL));
                });
            });
        }
    });
}

/// One matchup's kills and punishes, from the index.
struct Report {
    games: usize,
    /// (move, count, average percent) of the moves that killed me.
    killed_by: Vec<(u8, u32, f64)>,
    kills_with: Vec<(u8, u32, f64)>,
    /// (first move, count, average damage) of my punishes, best first.
    openers: Vec<(u8, u32, f64)>,
    death_percent: f64,
    kill_percent: f64,
}

impl Report {
    fn build(
        conn: &rusqlite::Connection,
        data: &Data,
        derived: &Derived,
        (mine, theirs): (u8, u8),
    ) -> Report {
        // The games of this matchup with stats, and my port in each.
        let mut ports: HashMap<i64, (u8, u8)> = HashMap::new();
        for &i in &derived.order {
            let game: &Game = &data.games[i];
            let Some(sides) = game.sides(&data.me) else {
                continue;
            };
            let (me, them) = (&game.players[sides.me], &game.players[sides.opponent]);
            if me.character == mine && them.character == theirs && game.stats_version > 0 {
                ports.insert(game.id, (me.port, them.port));
            }
        }
        let mut killed_by: HashMap<u8, (u32, f64)> = HashMap::new();
        let mut kills_with: HashMap<u8, (u32, f64)> = HashMap::new();
        let mut openers: HashMap<u8, (u32, f64)> = HashMap::new();
        let (mut deaths, mut death_sum, mut kills, mut kill_sum) = (0u32, 0.0, 0u32, 0.0);
        let ids: Vec<i64> = ports.keys().copied().collect();
        for chunk in ids.chunks(400) {
            let list = chunk
                .iter()
                .map(|id| id.to_string())
                .collect::<Vec<_>>()
                .join(",");
            if let Ok(mut statement) = conn.prepare(&format!(
                "SELECT game_id, victim, move_id, percent FROM kills WHERE game_id IN ({list})"
            )) {
                let rows = statement.query_map([], |r| {
                    Ok((
                        r.get::<_, i64>(0)?,
                        r.get::<_, i64>(1)? as u8,
                        r.get::<_, Option<i64>>(2)?,
                        r.get::<_, Option<f64>>(3)?,
                    ))
                });
                for (game, victim, move_id, percent) in rows.into_iter().flatten().flatten() {
                    let Some(&(me, _)) = ports.get(&game) else {
                        continue;
                    };
                    let percent = percent.unwrap_or(0.0);
                    let target = if victim == me {
                        deaths += 1;
                        death_sum += percent;
                        &mut killed_by
                    } else {
                        kills += 1;
                        kill_sum += percent;
                        &mut kills_with
                    };
                    if let Some(m) = move_id {
                        let e = target.entry(m as u8).or_default();
                        e.0 += 1;
                        e.1 += percent;
                    }
                }
            }
            if let Ok(mut statement) = conn.prepare(&format!(
                "SELECT game_id, attacker, first_move, end_percent - start_percent FROM punishes WHERE game_id IN ({list})"
            )) {
                let rows = statement.query_map([], |r| {
                    Ok((r.get::<_, i64>(0)?, r.get::<_, Option<i64>>(1)?, r.get::<_, Option<i64>>(2)?, r.get::<_, f64>(3)?))
                });
                for (game, attacker, first, damage) in rows.into_iter().flatten().flatten() {
                    let Some(&(me, _)) = ports.get(&game) else { continue };
                    if attacker != Some(me as i64) {
                        continue;
                    }
                    if let Some(m) = first {
                        let e = openers.entry(m as u8).or_default();
                        e.0 += 1;
                        e.1 += damage;
                    }
                }
            }
        }
        let average = |map: HashMap<u8, (u32, f64)>, by_damage: bool| {
            let mut v: Vec<(u8, u32, f64)> = map
                .into_iter()
                .map(|(m, (n, sum))| (m, n, sum / n.max(1) as f64))
                .collect();
            if by_damage {
                v.retain(|r| r.1 >= 5);
                v.sort_by(|a, b| b.2.total_cmp(&a.2).then(a.0.cmp(&b.0)));
            } else {
                v.sort_by_key(|r| (std::cmp::Reverse(r.1), r.0));
            }
            v
        };
        Report {
            games: ports.len(),
            killed_by: average(killed_by, false),
            kills_with: average(kills_with, false),
            openers: average(openers, true),
            death_percent: death_sum / deaths.max(1) as f64,
            kill_percent: kill_sum / kills.max(1) as f64,
        }
    }
}

/// A season in the rank card: its name and dates, and how I finished it.
fn seasons_row(ui: &mut egui::Ui, season: &crate::model::Season) {
    ui.horizontal(|ui| {
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            let name = if season.is_current() {
                format!("{} (now)", season.name)
            } else {
                season.name.clone()
            };
            ui.label(RichText::new(name).strong());
            let dates = match season.end {
                Some(end) => format!(
                    "{} – {}",
                    crate::time::month_label(season.start),
                    crate::time::month_label(end)
                ),
                None => format!("since {}", crate::time::day_label(season.start)),
            };
            ui.label(theme::muted(dates).size(theme::text::SMALL));
        });
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if let (Some(w), Some(l)) = (season.wins, season.losses) {
                ui.label(
                    RichText::new(format!("{w}–{l}"))
                        .color(widgets::win_rate_color(w, w + l))
                        .size(theme::text::SMALL),
                )
                .on_hover_text("Sets won and lost");
            }
            if season.sets > 0 {
                ui.label(theme::muted(format!("{:.0}", season.rating)).size(theme::text::SMALL));
            }
            let rank = display_rank(&season.rank);
            ui.label(RichText::new(&rank).color(widgets::rank_color(&rank)));
            if let Some(source) = widgets::rank_source(&rank) {
                ui.add(egui::Image::new(source).fit_to_exact_size(vec2(22.0, 22.0)));
            }
        });
    });
}
