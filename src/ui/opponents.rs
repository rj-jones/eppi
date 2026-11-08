//! Opponents: everyone you've played (under the filters), your record
//! against them and their rank; the pane shows the matchup history, how
//! you do on each stage and against each of their characters, and notes.

use crate::app::{Action, Data, Derived, display_rank};
use crate::melee;
use crate::model::{Game, Me, Outcome, Row, set_score};
use crate::ui::replay_list::Nav;
use crate::ui::theme::{self, Ion};
use crate::ui::virtual_list::{ScrollAlign, VirtualList};
use crate::ui::widgets::{self, paint_text_clipped};
use eframe::egui::{
    self, Color32, CornerRadius, FontId, Margin, Rect, RichText, Sense, pos2, vec2,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq)]
pub struct Summary {
    pub code: String,
    pub name: String,
    pub games: u32,
    pub wins: u32,
    pub losses: u32,
    pub last_played: i64,
    pub first_played: i64,
    /// (character, costume, games), most played first.
    pub characters: Vec<(u8, u8, u32)>,
}

/// Opponents in `order` (indexes into `games`), last played first.
pub fn summarize(games: &[Game], order: &[usize], me: &Me) -> Vec<Summary> {
    let mut by_code: HashMap<&str, Summary> = HashMap::new();
    let mut characters: HashMap<&str, HashMap<u8, (u8, u32)>> = HashMap::new();
    for &i in order {
        let game = &games[i];
        let Some(opponent) = game.opponent(me) else {
            continue;
        };
        if opponent.code.is_empty() {
            continue;
        }
        let entry = by_code.entry(&opponent.code).or_insert_with(|| Summary {
            code: opponent.code.clone(),
            name: opponent.display_name(),
            games: 0,
            wins: 0,
            losses: 0,
            last_played: game.start_at,
            first_played: game.start_at,
            characters: Vec::new(),
        });
        entry.games += 1;
        match game.outcome(me) {
            Outcome::Win => entry.wins += 1,
            Outcome::Loss => entry.losses += 1,
            Outcome::Unknown => {}
        }
        entry.last_played = entry.last_played.max(game.start_at);
        entry.first_played = entry.first_played.min(game.start_at);
        let used = characters
            .entry(&opponent.code)
            .or_default()
            .entry(opponent.character)
            .or_insert((opponent.costume, 0));
        used.1 += 1;
    }
    let mut out: Vec<Summary> = by_code
        .into_iter()
        .map(|(code, mut summary)| {
            let mut used: Vec<(u8, u8, u32)> = characters
                .remove(code)
                .unwrap_or_default()
                .into_iter()
                .map(|(c, (costume, n))| (c, costume, n))
                .collect();
            used.sort_by_key(|u| std::cmp::Reverse(u.2));
            summary.characters = used;
            summary
        })
        .collect();
    out.sort_by(|a, b| b.last_played.cmp(&a.last_played).then(a.code.cmp(&b.code)));
    out
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum Sort {
    #[default]
    LastPlayed,
    MostGames,
    Toughest,
    Rating,
}

impl Sort {
    const ALL: [Sort; 4] = [
        Sort::LastPlayed,
        Sort::MostGames,
        Sort::Toughest,
        Sort::Rating,
    ];

    fn label(self) -> &'static str {
        match self {
            Sort::LastPlayed => "Last played",
            Sort::MostGames => "Most games",
            Sort::Toughest => "Toughest (lowest win rate)",
            Sort::Rating => "Highest rated",
        }
    }
}

#[derive(Default)]
pub struct State {
    list: VirtualList<u64>,
    /// Positions into `Derived::opponents`, sorted and searched.
    shown: Vec<usize>,
    shown_for: Option<(u64, Sort, usize)>,
    selected: Option<String>,
    reveal: Option<String>,
    visible: (usize, usize),
    note: Option<(String, String)>,
}

impl State {
    pub fn select(&mut self, code: &str) {
        self.selected = Some(code.to_owned());
        self.reveal = Some(code.to_owned());
    }

    fn refresh(&mut self, data: &Data, derived: &Derived, sort: Sort) {
        let key = (derived.revision, sort, data.ranks.len());
        if self.shown_for == Some(key) {
            return;
        }
        self.shown_for = Some(key);
        let mut shown: Vec<usize> = (0..derived.opponents.len()).collect();
        let all = &derived.opponents;
        match sort {
            Sort::LastPlayed => {}
            Sort::MostGames => shown.sort_by(|a, b| all[*b].games.cmp(&all[*a].games)),
            Sort::Toughest => shown.sort_by(|a, b| {
                let rate = |s: &Summary| {
                    let total = s.wins + s.losses;
                    // Few games say little: they go after the rest.
                    if total < 3 {
                        2.0
                    } else {
                        s.wins as f64 / total as f64
                    }
                };
                rate(&all[*a]).total_cmp(&rate(&all[*b]))
            }),
            Sort::Rating => shown.sort_by(|a, b| {
                let rating =
                    |s: &Summary| data.rank(&s.code).and_then(|r| r.rating).unwrap_or(-1.0);
                rating(&all[*b]).total_cmp(&rating(&all[*a]))
            }),
        }
        self.shown = shown;
    }

    pub fn navigate(
        &mut self,
        nav: Option<Nav>,
        data: &Data,
        derived: &Derived,
        actions: &mut Vec<Action>,
    ) {
        let Some(nav) = nav else { return };
        if self.shown.is_empty() {
            return;
        }
        let current = self.selected.as_ref().and_then(|code| {
            self.shown
                .iter()
                .position(|&i| derived.opponents[i].code == *code)
        });
        let last = self.shown.len() - 1;
        let page = (self.visible.1 - self.visible.0.min(self.visible.1)).max(1);
        let target = match (nav, current) {
            (Nav::Up, Some(i)) => i.saturating_sub(1),
            (Nav::Down, Some(i)) => (i + 1).min(last),
            (Nav::PageUp, i) => i.unwrap_or(0).saturating_sub(page),
            (Nav::PageDown, i) => (i.unwrap_or(0) + page).min(last),
            (Nav::End, _) => last,
            (Nav::Up | Nav::Down | Nav::Home, _) => 0,
            (Nav::Enter, Some(i)) => {
                // Their sets, in Replays.
                let code = &derived.opponents[self.shown[i]].code;
                if let Some(id) = derived.rows.iter().find_map(|row| {
                    let first = match row {
                        Row::Set(set) => set.games[0],
                        Row::Game(g) => *g,
                    };
                    let game = &data.games[first];
                    game.opponent(&data.me)
                        .is_some_and(|p| p.code == *code)
                        .then_some(game.id)
                }) {
                    actions.push(Action::GoToGame(id));
                }
                return;
            }
            _ => return,
        };
        let code = derived.opponents[self.shown[target]].code.clone();
        self.list
            .scroll_to(crate::model::hash_str(&code), ScrollAlign::Nearest, false);
        self.selected = Some(code);
    }

    pub fn visible_codes(&self, derived: &Derived) -> Vec<String> {
        let (first, last) = self.visible;
        self.shown
            .iter()
            .take(last + 1)
            .skip(first)
            .map(|&i| derived.opponents[i].code.clone())
            .chain(self.selected.clone())
            .collect()
    }

    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        data: &Data,
        derived: &Derived,
        sort: &mut Sort,
        _actions: &mut Vec<Action>,
    ) {
        // The header's search box is part of the filter, so the opponents
        // are searched already.
        egui::Frame::new()
            .inner_margin(Margin::symmetric(16, 10))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(theme::heavy(
                        format!("{} opponents", widgets::thousands(self.shown.len())),
                        theme::text::TITLE,
                    ));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        egui::ComboBox::from_id_salt("opponent-sort")
                            .selected_text(sort.label())
                            .width(210.0)
                            .show_ui(ui, |ui| {
                                for s in Sort::ALL {
                                    ui.selectable_value(sort, s, s.label());
                                }
                            });
                        ui.label(theme::muted("Sort"));
                    });
                });
            });
        self.refresh(data, derived, *sort);
        if self.shown.is_empty() {
            ui.add_space(60.0);
            ui.vertical_centered(|ui| {
                ui.label(Ion::PEOPLE.rich(40.0).color(theme::FAINT));
                ui.label(theme::muted(if data.me.is_empty() {
                    "Add your connect code in Settings to see who you've played."
                } else {
                    "No opponents under these filters."
                }));
            });
            return;
        }
        if let Some(code) = self.reveal.take() {
            self.list
                .scroll_to(crate::model::hash_str(&code), ScrollAlign::Center, true);
        }
        let width = ui.available_width();
        let shown = &self.shown;
        let all = &derived.opponents;
        self.list.sync(
            self.shown_for
                .map_or(0, |k| crate::model::hash_str(&format!("{k:?}"))),
            width,
            shown
                .iter()
                .map(|&i| (crate::model::hash_str(&all[i].code), 0, 54.0)),
        );
        let selected = self.selected.clone();
        let mut clicked = None;
        let output = self.list.show(ui, "opponents", |ui, index, _| {
            let summary = &all[shown[index]];
            let response = row(
                ui,
                summary,
                data,
                selected.as_deref() == Some(&summary.code),
            );
            if response.clicked() {
                clicked = Some(summary.code.clone());
            }
        });
        self.visible = (output.first_visible, output.last_visible);
        if let Some(code) = clicked {
            self.selected = Some(code);
        }
    }

    pub fn details(
        &mut self,
        ui: &mut egui::Ui,
        data: &Data,
        derived: &Derived,
        actions: &mut Vec<Action>,
    ) {
        let Some(code) = self.selected.clone() else {
            ui.add_space(120.0);
            ui.vertical_centered(|ui| {
                ui.label(Ion::PERSON.rich(36.0).color(theme::FAINT));
                ui.label(theme::muted(
                    "Pick an opponent to see your history with them.",
                ));
            });
            return;
        };
        let Some(summary) = derived.opponents.iter().find(|s| s.code == code) else {
            ui.add_space(40.0);
            ui.vertical_centered(|ui| {
                ui.label(theme::muted("No games with them under these filters."))
            });
            return;
        };
        egui::ScrollArea::vertical()
            .id_salt("opponent-details")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                egui::Frame::new()
                    .inner_margin(Margin::same(16))
                    .show(ui, |ui| {
                        opponent_header(ui, summary, data, actions);
                        ui.add_space(theme::space::M);
                        self.note_editor(ui, &summary.code, data, actions);
                        ui.add_space(theme::space::M);
                        breakdowns(ui, summary, data, derived);
                        ui.add_space(theme::space::M);
                        history(ui, summary, data, derived, actions);
                    });
            });
    }

    fn note_editor(
        &mut self,
        ui: &mut egui::Ui,
        code: &str,
        data: &Data,
        actions: &mut Vec<Action>,
    ) {
        if self.note.as_ref().is_none_or(|(c, _)| c != code) {
            self.note = Some((
                code.to_owned(),
                data.notes.get(code).cloned().unwrap_or_default(),
            ));
        }
        let Some((_, text)) = &mut self.note else {
            return;
        };
        ui.label(theme::caption("Notes"));
        let response = ui.add(
            egui::TextEdit::multiline(text)
                .hint_text("What do they do? (\"Ledge-hops a lot, shield-grabs everything\")")
                .desired_rows(3)
                .desired_width(f32::INFINITY),
        );
        if response.changed() {
            actions.push(Action::SaveNote {
                code: code.to_owned(),
                text: text.clone(),
            });
        }
    }
}

fn row(ui: &mut egui::Ui, summary: &Summary, data: &Data, selected: bool) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(vec2(ui.available_width(), 54.0), Sense::click());
    if !ui.is_rect_visible(rect) {
        return response;
    }
    let inner = rect.shrink2(vec2(6.0, 2.0));
    let painter = ui.painter();
    if selected {
        painter.rect_filled(inner, CornerRadius::same(6), theme::SELECTED_ROW);
    } else if response.hovered() {
        painter.rect_filled(inner, CornerRadius::same(6), theme::HOVERED_ROW);
    }
    let cy = rect.center().y;
    let mut x = rect.min.x + 16.0;
    for (character, costume, _) in summary.characters.iter().take(3) {
        let r = Rect::from_center_size(pos2(x + 13.0, cy), vec2(26.0, 26.0));
        widgets::paint_stock(ui, r, *character, *costume, Color32::WHITE);
        x += 28.0;
    }
    let name_x = rect.min.x + 110.0;
    let right = rect.max.x - 16.0;
    let wide = rect.width() > 700.0;
    let record_x = right - if wide { 330.0 } else { 200.0 };
    let rank_x = record_x - 160.0;
    paint_text_clipped(
        painter,
        pos2(name_x, cy - 9.0),
        &summary.name,
        theme::heavy_font(theme::text::LIST),
        theme::TEXT,
        rank_x - name_x - 12.0,
    );
    paint_text_clipped(
        painter,
        pos2(name_x, cy + 10.0),
        &summary.code,
        FontId::proportional(theme::text::SMALL),
        theme::MUTED,
        rank_x - name_x - 12.0,
    );
    if let Some(record) = data.rank(&summary.code)
        && let Some(rank) = record.rank.as_deref().map(display_rank)
    {
        let icon = Rect::from_center_size(pos2(rank_x + 11.0, cy), vec2(22.0, 22.0));
        widgets::paint_rank(ui, icon, &rank);
        painter.text(
            pos2(rank_x + 28.0, cy),
            egui::Align2::LEFT_CENTER,
            &rank,
            FontId::proportional(theme::text::SMALL),
            widgets::rank_color(&rank),
        );
    }
    let decided = summary.wins + summary.losses;
    painter.text(
        pos2(record_x, cy - 9.0),
        egui::Align2::LEFT_CENTER,
        format!("{}–{}", summary.wins, summary.losses),
        theme::heavy_font(theme::text::LIST),
        widgets::win_rate_color(summary.wins, decided),
    );
    painter.text(
        pos2(record_x, cy + 10.0),
        egui::Align2::LEFT_CENTER,
        format!(
            "{} · {} games",
            widgets::percent(summary.wins, decided),
            summary.games
        ),
        FontId::proportional(theme::text::SMALL),
        theme::MUTED,
    );
    if wide {
        let bar = Rect::from_min_size(pos2(record_x + 110.0, cy - 3.0), vec2(90.0, 6.0));
        painter.rect_filled(bar, CornerRadius::same(3), theme::LOSS.gamma_multiply(0.5));
        if decided > 0 {
            let won = bar.width() * summary.wins as f32 / decided as f32;
            painter.rect_filled(
                Rect::from_min_size(bar.min, vec2(won, bar.height())),
                CornerRadius::same(3),
                theme::WIN,
            );
        }
    }
    painter.text(
        pos2(right, cy),
        egui::Align2::RIGHT_CENTER,
        crate::time::short(summary.last_played, data.now),
        FontId::proportional(theme::text::SMALL),
        theme::MUTED,
    );
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}

fn opponent_header(ui: &mut egui::Ui, summary: &Summary, data: &Data, actions: &mut Vec<Action>) {
    ui.horizontal(|ui| {
        if let Some((c, costume, _)) = summary.characters.first() {
            widgets::stock_icon(ui, *c, *costume, 44.0);
        }
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing.y = 2.0;
            ui.label(theme::heavy(&summary.name, theme::text::HEADING + 2.0));
            ui.horizontal(|ui| {
                ui.label(theme::muted(&summary.code));
                if ui
                    .add(egui::Button::new(Ion::COPY.rich(13.0).color(theme::MUTED)).frame(false))
                    .on_hover_text("Copy their code")
                    .clicked()
                {
                    actions.push(Action::CopyText(summary.code.clone()));
                }
            });
        });
    });
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        crate::ui::details::rank_line(ui, data.rank(&summary.code), data);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui
                .add(egui::Button::new(Ion::REFRESH.rich(14.0)).frame(false))
                .on_hover_text("Look up their rank again")
                .clicked()
            {
                actions.push(Action::FetchRank {
                    code: summary.code.clone(),
                    force: true,
                });
            }
        });
    });
    if let Some(record) = data.rank(&summary.code)
        && let Some((w, l)) = crate::ranks::season_record(record)
    {
        ui.label(
            theme::muted(format!("Ranked this season: {w}–{l} in sets")).size(theme::text::SMALL),
        );
    }
    ui.add_space(theme::space::M);
    let decided = summary.wins + summary.losses;
    theme::card().show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal(|ui| {
            widgets::stat_tile(
                ui,
                format!("{}–{}", summary.wins, summary.losses),
                "Games",
                widgets::win_rate_color(summary.wins, decided),
            );
            ui.add_space(theme::space::L);
            widgets::stat_tile(
                ui,
                widgets::percent(summary.wins, decided),
                "Win rate",
                theme::TEXT,
            );
            ui.add_space(theme::space::L);
            widgets::stat_tile(
                ui,
                crate::time::day_label(summary.first_played),
                "First played",
                theme::MUTED,
            );
        });
    });
}

/// Their characters and the stages: how you do on each.
fn breakdowns(ui: &mut egui::Ui, summary: &Summary, data: &Data, derived: &Derived) {
    let mut by_character: HashMap<(u8, u8), (u32, u32)> = HashMap::new();
    let mut by_stage: HashMap<u16, (u32, u32)> = HashMap::new();
    for &i in &derived.order {
        let game = &data.games[i];
        let Some(opponent) = game.opponent(&data.me) else {
            continue;
        };
        if opponent.code != summary.code {
            continue;
        }
        let outcome = game.outcome(&data.me);
        let bump = |entry: &mut (u32, u32)| match outcome {
            Outcome::Win => entry.0 += 1,
            Outcome::Loss => entry.1 += 1,
            Outcome::Unknown => {}
        };
        bump(
            by_character
                .entry((opponent.character, opponent.costume))
                .or_default(),
        );
        bump(by_stage.entry(game.stage).or_default());
    }
    ui.label(theme::caption("Against their characters"));
    let mut characters: Vec<_> = by_character.into_iter().collect();
    characters.sort_by_key(|c| std::cmp::Reverse(c.1.0 + c.1.1));
    for ((character, costume), (w, l)) in characters.into_iter().filter(|(_, (w, l))| w + l > 0) {
        ui.horizontal(|ui| {
            widgets::stock_icon(ui, character, costume, 20.0);
            ui.label(melee::character_name(character));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(theme::muted(widgets::percent(w, w + l)));
                ui.label(
                    RichText::new(format!("{w}–{l}"))
                        .color(widgets::win_rate_color(w, w + l))
                        .strong(),
                );
            });
        });
    }
    ui.add_space(theme::space::S);
    ui.label(theme::caption("On each stage"));
    let mut stages: Vec<_> = by_stage.into_iter().collect();
    stages.sort_by_key(|s| std::cmp::Reverse(s.1.0 + s.1.1));
    for (stage, (w, l)) in stages.into_iter().filter(|(_, (w, l))| w + l > 0) {
        ui.horizontal(|ui| {
            ui.label(melee::stage_name(stage));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(theme::muted(widgets::percent(w, w + l)));
                ui.label(
                    RichText::new(format!("{w}–{l}"))
                        .color(widgets::win_rate_color(w, w + l))
                        .strong(),
                );
            });
        });
    }
}

/// Every set and game with them, newest first.
fn history(
    ui: &mut egui::Ui,
    summary: &Summary,
    data: &Data,
    derived: &Derived,
    actions: &mut Vec<Action>,
) {
    ui.label(theme::caption("History"));
    ui.add_space(4.0);
    let mut shown = 0;
    for row in &derived.rows {
        let (games, is_set): (Vec<usize>, bool) = match row {
            Row::Set(set) => (set.games.clone(), true),
            Row::Game(i) => (vec![*i], false),
        };
        let first = &data.games[games[0]];
        if first
            .opponent(&data.me)
            .is_none_or(|p| p.code != summary.code)
        {
            continue;
        }
        shown += 1;
        if shown > 200 {
            ui.label(theme::muted("…and more in Replays (search their code)."));
            break;
        }
        let (rect, response) =
            ui.allocate_exact_size(vec2(ui.available_width(), 32.0), Sense::click());
        if response.hovered() {
            ui.painter()
                .rect_filled(rect, CornerRadius::same(6), theme::HOVERED_ROW);
        }
        let painter = ui.painter();
        let cy = rect.center().y;
        let (mine, theirs) = set_score(&data.games, &games, &data.me);
        let outcome = match mine.cmp(&theirs) {
            std::cmp::Ordering::Greater => Outcome::Win,
            std::cmp::Ordering::Less => Outcome::Loss,
            std::cmp::Ordering::Equal => Outcome::Unknown,
        };
        widgets::paint_outcome(
            painter,
            Rect::from_center_size(pos2(rect.min.x + 12.0, cy), vec2(18.0, 18.0)),
            outcome,
        );
        let label = if is_set {
            format!("{} {mine}–{theirs}", first.mode.label())
        } else {
            format!(
                "{} · {}",
                first.mode.label(),
                melee::stage_short(first.stage)
            )
        };
        painter.text(
            pos2(rect.min.x + 30.0, cy),
            egui::Align2::LEFT_CENTER,
            label,
            FontId::proportional(theme::text::BODY),
            theme::TEXT,
        );
        painter.text(
            pos2(rect.max.x - 6.0, cy),
            egui::Align2::RIGHT_CENTER,
            crate::time::short(first.start_at, data.now),
            FontId::proportional(theme::text::SMALL),
            theme::MUTED,
        );
        if response
            .on_hover_cursor(egui::CursorIcon::PointingHand)
            .on_hover_text("Show in Replays")
            .clicked()
        {
            actions.push(Action::GoToGame(first.id));
        }
    }
}
