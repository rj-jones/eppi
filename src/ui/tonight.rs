//! Tonight: the session you're in (or the last one) at a glance, the
//! opponent you're playing right now, and tonight's sets.
//!
//! A session is the newest games back to the first gap of more than an
//! hour and a half between two games.

use crate::app::{Action, Data, display_rank};
use crate::library::db;
use crate::melee;
use crate::model::{Game, Outcome, Row, SHORT_GAME_SECONDS, group_rows, set_score};
use crate::ui::details::Selection;
use crate::ui::replay_list::{ListState, Nav};
use crate::ui::theme::{self, Ion};
use crate::ui::widgets;
use eframe::egui::{self, Margin, RichText, vec2};

/// The longest break inside one session.
const SESSION_GAP: i64 = 90 * 60;
/// A session counts as tonight's while its last game is this recent.
const TONIGHT: i64 = 10 * 3_600;

#[derive(Default)]
pub struct State {
    list: ListState,
    built_for: u64,
    /// Indexes into `Data::games`, newest first.
    session: Vec<usize>,
    rows: Vec<Row>,
    /// My rating before the session and now.
    rating: Option<(Option<f64>, Option<f64>)>,
    rating_for: (u64, usize),
}

impl State {
    fn rebuild(&mut self, data: &Data) {
        if self.built_for == data.revision {
            return;
        }
        self.built_for = data.revision;
        self.session.clear();
        let mut previous_start: Option<i64> = None;
        for (i, game) in data.games.iter().enumerate() {
            if game.seconds() < SHORT_GAME_SECONDS || game.has_cpu() {
                continue;
            }
            if !data.me.is_empty() && game.sides(&data.me).is_none() {
                continue;
            }
            if let Some(newer) = previous_start {
                let ended = game.start_at + game.seconds() as i64;
                if newer - ended > SESSION_GAP {
                    break;
                }
            }
            previous_start = Some(game.start_at);
            self.session.push(i);
        }
        self.rows = group_rows(&data.games, &self.session);
    }

    fn is_tonight(&self, data: &Data) -> bool {
        self.session
            .first()
            .is_some_and(|&i| data.now - data.games[i].start_at < TONIGHT)
    }

    pub fn navigate(&mut self, nav: Option<Nav>, data: &Data, actions: &mut Vec<Action>) {
        self.rebuild(data);
        self.list.navigate_items(nav, data, &self.rows, actions);
    }

    pub fn selected_games(&self, data: &Data) -> Vec<i64> {
        match self.list.selection_in(data, &self.rows) {
            Selection::Game(g) => vec![g.id],
            Selection::Set(_, games) => games.iter().map(|g| g.id).collect(),
            Selection::None => Vec::new(),
        }
    }

    pub fn selection<'a>(&'a self, data: &'a Data) -> Selection<'a> {
        self.list.selection_in(data, &self.rows)
    }

    pub fn visible_codes(&self, data: &Data) -> Vec<String> {
        self.list.visible_codes_in(data, &self.rows)
    }

    fn rating_change(
        &mut self,
        data: &Data,
        conn: Option<&rusqlite::Connection>,
    ) -> Option<(Option<f64>, Option<f64>)> {
        let code = data.my_codes.first()?;
        let key = (
            data.revision,
            data.ranks.get(code).map_or(0, |r| r.fetched_at as usize),
        );
        if self.rating_for != key {
            self.rating_for = key;
            let start = self.session.last().map(|&i| data.games[i].start_at)?;
            let history = db::rank_history(conn?, code).ok()?;
            let before = history
                .iter()
                .find(|r| r.fetched_at <= start)
                .and_then(|r| r.rating);
            let now = history.first().and_then(|r| r.rating);
            self.rating = Some((before, now));
        }
        self.rating
    }

    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        data: &Data,
        conn: Option<&rusqlite::Connection>,
        actions: &mut Vec<Action>,
    ) {
        self.rebuild(data);
        let rating = self.rating_change(data, conn);
        egui::Frame::new()
            .inner_margin(Margin::symmetric(16, 12))
            .show(ui, |ui| {
                for game in &data.live {
                    live_card(ui, game, data, actions);
                    ui.add_space(theme::space::M);
                }
                if self.session.is_empty() {
                    ui.add_space(40.0);
                    ui.vertical_centered(|ui| {
                        ui.label(Ion::MOON.rich(40.0).color(theme::FAINT));
                        ui.label(theme::heavy("No games yet", theme::text::TITLE));
                        ui.label(theme::muted(
                            "Play some netplay: your session shows up here, game by game, as you play.",
                        ));
                    });
                    return;
                }
                let title = if self.is_tonight(data) {
                    "Tonight".to_owned()
                } else {
                    let last = &data.games[self.session[0]];
                    format!("Last session · {}", crate::time::day_label(last.start_at))
                };
                ui.label(theme::heavy(title, theme::text::HEADING));
                ui.add_space(theme::space::S);
                self.summary(ui, data, rating);
            });
        ui.add_space(4.0);
        let revision = data.revision ^ 0x7031;
        self.list
            .show_rows(ui, data, &self.rows, revision, "tonight", actions);
    }

    fn summary(&self, ui: &mut egui::Ui, data: &Data, rating: Option<(Option<f64>, Option<f64>)>) {
        let mut games = (0, 0);
        let mut sets = (0, 0);
        let mut seconds = 0.0;
        let mut opponents = std::collections::HashSet::new();
        for &i in &self.session {
            let game = &data.games[i];
            seconds += game.seconds();
            match game.outcome(&data.me) {
                Outcome::Win => games.0 += 1,
                Outcome::Loss => games.1 += 1,
                Outcome::Unknown => {}
            }
            if let Some(o) = game.opponent(&data.me) {
                opponents.insert(o.code.clone());
            }
        }
        for row in &self.rows {
            if let Row::Set(set) = row {
                let (mine, theirs) = set_score(&data.games, &set.games, &data.me);
                if mine > theirs {
                    sets.0 += 1;
                } else if theirs > mine {
                    sets.1 += 1;
                }
            }
        }
        theme::card().show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing.x = 34.0;
                widgets::stat_tile(
                    ui,
                    format!("{}–{}", sets.0, sets.1),
                    "Sets",
                    widgets::win_rate_color(sets.0, sets.0 + sets.1),
                );
                widgets::stat_tile(
                    ui,
                    format!("{}–{}", games.0, games.1),
                    "Games",
                    widgets::win_rate_color(games.0, games.0 + games.1),
                );
                widgets::stat_tile(
                    ui,
                    widgets::percent(games.0, games.0 + games.1),
                    "Win rate",
                    theme::TEXT,
                );
                if let Some((before, Some(now))) = rating {
                    let (text, color) = match before {
                        Some(before) => {
                            let delta = now - before;
                            (
                                format!("{}{:.1}", if delta >= 0.0 { "+" } else { "" }, delta),
                                if delta > 0.05 {
                                    theme::WIN
                                } else if delta < -0.05 {
                                    theme::LOSS
                                } else {
                                    theme::TEXT
                                },
                            )
                        }
                        None => (format!("{now:.0}"), theme::TEXT),
                    };
                    widgets::stat_tile(ui, text, "Rating", color);
                }
                widgets::stat_tile(ui, opponents.len().to_string(), "Opponents", theme::TEXT);
                widgets::stat_tile(ui, crate::time::duration(seconds), "Played", theme::MUTED);
            });
        });
    }
}

/// Wins and losses, and the last set's score with when it was.
type HeadToHead = ((u32, u32), Option<(u32, u32, i64)>);

/// Our record against `code`: games (W, L) and the last set's score.
fn head_to_head(code: &str, data: &Data) -> HeadToHead {
    let mut games = (0, 0);
    let mut last_match: Option<&str> = None;
    let mut last_set: Vec<usize> = Vec::new();
    for (i, game) in data.games.iter().enumerate() {
        let Some(opponent) = game.opponent(&data.me) else {
            continue;
        };
        if opponent.code != code {
            continue;
        }
        match game.outcome(&data.me) {
            Outcome::Win => games.0 += 1,
            Outcome::Loss => games.1 += 1,
            Outcome::Unknown => {}
        }
        match last_match {
            None => {
                last_match = Some(&game.match_id);
                last_set.push(i);
            }
            Some(id) if id == game.match_id && !id.is_empty() => last_set.push(i),
            _ => {}
        }
    }
    let last = last_set.first().map(|&i| {
        let (a, b) = set_score(&data.games, &last_set, &data.me);
        (a, b, data.games[i].start_at)
    });
    (games, last)
}

/// The opponent you're playing now: who they are, their rank, and how it
/// went before.
pub fn live_card(ui: &mut egui::Ui, game: &Game, data: &Data, actions: &mut Vec<Action>) {
    let Some(opponent) = game.opponent(&data.me).cloned() else {
        return;
    };
    let mine = game.mine(&data.me).cloned();
    egui::Frame::new()
        .fill(theme::NOTICE)
        .corner_radius(10)
        .inner_margin(Margin::same(16))
        .stroke(egui::Stroke::new(1.0, theme::ACCENT.gamma_multiply(0.6)))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                theme::pill(ui, "LIVE", theme::DANGER);
                ui.label(theme::muted(format!(
                    "{} · {} · game {}",
                    game.mode.label(),
                    melee::stage_name(game.stage),
                    game.game_number.max(1)
                )));
            });
            ui.add_space(theme::space::S);
            ui.horizontal(|ui| {
                if let Some(mine) = &mine {
                    widgets::stock_icon(ui, mine.character, mine.costume, 40.0);
                    ui.label(theme::muted("vs"));
                }
                widgets::stock_icon(ui, opponent.character, opponent.costume, 52.0);
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = 2.0;
                    ui.label(theme::heavy(opponent.display_name(), theme::text::DISPLAY));
                    ui.label(theme::muted(format!(
                        "{} · {}",
                        opponent.code,
                        melee::character_name(opponent.character)
                    )));
                });
                ui.add_space(theme::space::XL);
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = 2.0;
                    match data.rank(&opponent.code) {
                        Some(record) => {
                            let rank = record
                                .rank
                                .as_deref()
                                .map(display_rank)
                                .unwrap_or("Unranked".into());
                            ui.horizontal(|ui| {
                                if let Some(source) = widgets::rank_source(&rank) {
                                    ui.add(
                                        egui::Image::new(source)
                                            .fit_to_exact_size(vec2(34.0, 34.0)),
                                    );
                                }
                                ui.label(
                                    theme::heavy(&rank, theme::text::HEADING)
                                        .color(widgets::rank_color(&rank)),
                                );
                            });
                            let mut line = String::new();
                            if let Some(rating) = record.rating.filter(|r| *r > 0.0) {
                                line.push_str(&format!("{rating:.0}"));
                            }
                            if let Some((w, l)) = crate::ranks::season_record(record) {
                                line.push_str(&format!("  ·  {w}–{l} in sets this season"));
                            }
                            ui.label(theme::muted(line));
                        }
                        None => {
                            ui.horizontal(|ui| {
                                ui.add(egui::Spinner::new().size(16.0));
                                ui.label(theme::muted("Looking up their rank…"));
                            });
                        }
                    }
                });
            });
            ui.add_space(theme::space::S);
            let ((w, l), last) = head_to_head(&opponent.code, data);
            ui.horizontal(|ui| {
                if w + l == 0 {
                    ui.label(RichText::new("First time playing them").color(theme::TEXT));
                } else {
                    ui.label(
                        RichText::new(format!("You're {w}–{l} in games against them"))
                            .color(widgets::win_rate_color(w, w + l))
                            .strong(),
                    );
                    if let Some((a, b, at)) = last {
                        ui.label(theme::muted(format!(
                            "· last time {a}–{b}, {}",
                            crate::time::ago(at, data.now)
                        )));
                    }
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if w + l > 0 && ui.add(theme::icon_button(Ion::PERSON, "History")).clicked() {
                        actions.push(Action::GoToOpponent(opponent.code.clone()));
                    }
                });
            });
            if let Some(note) = data.notes.get(&opponent.code) {
                ui.add_space(4.0);
                ui.label(
                    RichText::new(format!("Your notes: {note}"))
                        .color(theme::WARNING)
                        .italics(),
                );
            }
        });
}

/// The live game in the nav, under the views.
pub fn live_mini(ui: &mut egui::Ui, game: &Game, data: &Data, actions: &mut Vec<Action>) {
    let Some(opponent) = game.opponent(&data.me) else {
        return;
    };
    let response = egui::Frame::new()
        .fill(theme::NOTICE)
        .corner_radius(8)
        .inner_margin(Margin::same(8))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                widgets::stock_icon(ui, opponent.character, opponent.costume, 26.0);
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = 0.0;
                    ui.label(
                        RichText::new("Playing now")
                            .size(theme::text::TINY)
                            .color(theme::DANGER),
                    );
                    ui.label(theme::heavy(opponent.display_name(), theme::text::BODY));
                    let rank = data
                        .rank(&opponent.code)
                        .and_then(|r| r.rank.as_deref())
                        .map(display_rank)
                        .unwrap_or_default();
                    if !rank.is_empty() {
                        ui.label(
                            RichText::new(rank.clone())
                                .size(theme::text::SMALL)
                                .color(widgets::rank_color(&rank)),
                        );
                    }
                });
            });
        })
        .response
        .interact(egui::Sense::click())
        .on_hover_cursor(egui::CursorIcon::PointingHand);
    if response.clicked() {
        actions.push(Action::SetView(crate::app::View::Tonight));
    }
}
