//! The search bar in the header: what you type filters the current view's
//! list (players, characters, stages), and a dropdown suggests the players
//! it matches (by netplay name, connect code or in-game tag) across every
//! game, to open in Opponents.

use crate::app::{Action, Data, View, display_rank};
use crate::model::{Outcome, Player};
use crate::ui::theme::{self, Ion};
use crate::ui::widgets::{self, paint_text_clipped};
use eframe::egui::{self, Color32, CornerRadius, FontId, Margin, Rect, Sense, pos2, vec2};
use std::collections::HashMap;

/// Someone you've played (or watched, in games that aren't yours).
#[derive(Debug, Clone)]
struct Person {
    code: String,
    name: String,
    tag: String,
    character: u8,
    costume: u8,
    games: u32,
    wins: u32,
    losses: u32,
    last: i64,
}

impl Person {
    fn title(&self) -> &str {
        [&self.name, &self.code, &self.tag]
            .into_iter()
            .find(|s| !s.is_empty())
            .map_or("?", |s| s.as_str())
    }
}

const MAX_SUGGESTIONS: usize = 8;
const ROW_HEIGHT: f32 = 44.0;
/// The bar's height inside its frame.
const BAR_HEIGHT: f32 = 32.0;

#[derive(Default)]
pub struct Search {
    people: Vec<Person>,
    built_for: Option<u64>,
    /// The suggestion the arrow keys are on.
    highlight: usize,
    open: bool,
    /// Where the dropdown was drawn last frame (clicks there keep it open).
    popup: Option<Rect>,
    /// Focus the field on the next frame (`/`, Ctrl+K).
    pub focus: bool,
}

impl Search {
    fn rebuild(&mut self, data: &Data) {
        if self.built_for == Some(data.revision) {
            return;
        }
        self.built_for = Some(data.revision);
        let mut people: HashMap<String, Person> = HashMap::new();
        for game in &data.games {
            let outcome = game.outcome(&data.me);
            let others: Vec<&Player> = match game.opponent(&data.me) {
                Some(opponent) => vec![opponent],
                None => game
                    .players
                    .iter()
                    .filter(|p| !p.cpu && !data.me.is_me(&p.code))
                    .collect(),
            };
            for player in others {
                let key = if !player.code.is_empty() {
                    player.code.clone()
                } else if !player.tag.is_empty() {
                    format!("tag:{}", player.tag.to_lowercase())
                } else {
                    continue;
                };
                let person = people.entry(key).or_insert_with(|| Person {
                    code: player.code.clone(),
                    name: player.name.clone(),
                    tag: player.tag.clone(),
                    character: player.character,
                    costume: player.costume,
                    games: 0,
                    wins: 0,
                    losses: 0,
                    last: game.start_at,
                });
                person.games += 1;
                match outcome {
                    Outcome::Win => person.wins += 1,
                    Outcome::Loss => person.losses += 1,
                    Outcome::Unknown => {}
                }
                // Games come newest first: the first seen is the latest.
                if person.tag.is_empty() {
                    person.tag = player.tag.clone();
                }
            }
        }
        let mut people: Vec<Person> = people.into_values().collect();
        people.sort_by_key(|p| std::cmp::Reverse((p.games, p.last)));
        self.people = people;
    }

    /// Players matching `query`: those whose name, code or tag starts with
    /// it first, then the rest that contain it, most played first.
    fn matches(&self, query: &str) -> Vec<Person> {
        let query = query.trim().to_lowercase();
        if query.is_empty() {
            return Vec::new();
        }
        let fields = |p: &Person| {
            [
                p.name.to_lowercase(),
                p.code.to_lowercase(),
                p.tag.to_lowercase(),
            ]
        };
        let mut starts = Vec::new();
        let mut contains = Vec::new();
        for person in &self.people {
            let fields = fields(person);
            if fields.iter().any(|f| f.starts_with(&query)) {
                starts.push(person);
            } else if fields.iter().any(|f| f.contains(&query)) {
                contains.push(person);
            }
            if starts.len() >= MAX_SUGGESTIONS {
                break;
            }
        }
        starts.extend(contains);
        starts.truncate(MAX_SUGGESTIONS);
        starts.into_iter().cloned().collect()
    }

    /// Draws the bar; `text` is the filter's search.
    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        text: &mut String,
        data: &Data,
        actions: &mut Vec<Action>,
    ) {
        self.rebuild(data);
        let id = egui::Id::unique("search");
        let focused = ui.memory(|m| m.has_focus(id));
        let width = ui.available_width().min(380.0);
        let frame = egui::Frame::new()
            .fill(theme::BUTTON)
            .corner_radius(CornerRadius::same(7))
            .inner_margin(Margin::symmetric(10, 0))
            .stroke(egui::Stroke::new(
                1.0,
                if focused {
                    theme::OUTLINE_ACTIVE
                } else {
                    theme::SEPARATOR
                },
            ));
        let mut picked: Option<usize> = None;
        let inner = frame.show(ui, |ui| {
            ui.set_width(width - 20.0);
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 6.0;
                ui.label(Ion::SEARCH.rich(15.0).color(if focused {
                    theme::TEXT
                } else {
                    theme::MUTED
                }));
                let clear_room = if text.is_empty() { 0.0 } else { 24.0 };
                let edit = egui::TextEdit::singleline(text)
                    .id(id)
                    .frame(egui::Frame::NONE)
                    .hint_text("Search names, codes, tags…   /")
                    .desired_width(ui.available_width() - clear_room)
                    // Text and hint centred in the bar, level with the icon.
                    .min_size(vec2(0.0, BAR_HEIGHT))
                    .vertical_align(egui::Align::Center)
                    .margin(Margin::ZERO);
                let response = ui.add(edit);
                if std::mem::take(&mut self.focus) {
                    response.request_focus();
                }
                if !text.is_empty()
                    && ui
                        .add(
                            egui::Button::new(Ion::CLOSE_CIRCLE.rich(15.0).color(theme::MUTED))
                                .frame(false),
                        )
                        .on_hover_text("Clear (Esc)")
                        .clicked()
                {
                    text.clear();
                    self.open = false;
                }
                response
            })
            .inner
        });
        let response = inner.inner;
        let bar = inner.response.rect;

        // The dropdown: open while typing in the field, closed by Esc, an
        // emptied field, or a click somewhere else.
        if response.changed() {
            self.open = !text.trim().is_empty();
            self.highlight = 0;
        }
        if response.gained_focus() && !text.trim().is_empty() {
            self.open = true;
        }
        let escape = ui.input(|i| i.key_pressed(egui::Key::Escape));
        if escape {
            self.open = false;
        }
        let clicked_outside = ui.input(|i| i.pointer.any_pressed())
            && !ui
                .input(|i| i.pointer.interact_pos())
                .is_some_and(|p| bar.contains(p) || self.popup.is_some_and(|r| r.contains(p)));
        if clicked_outside {
            self.open = false;
        }
        let suggestions = self.matches(text);
        if suggestions.is_empty() {
            self.open = false;
        }
        if self.open && focused {
            ui.input_mut(|i| {
                if i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowDown) {
                    self.highlight = (self.highlight + 1).min(suggestions.len().saturating_sub(1));
                }
                if i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowUp) {
                    self.highlight = self.highlight.saturating_sub(1);
                }
            });
        }
        if self.open && response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
            picked = Some(self.highlight);
        }

        self.popup = None;
        if self.open {
            let area = egui::Area::new(id.with("suggestions"))
                .order(egui::Order::Foreground)
                .fixed_pos(bar.left_bottom() + vec2(0.0, 4.0))
                .show(ui.ctx(), |ui| {
                    egui::Frame::new()
                        .fill(theme::SIDEBAR)
                        .corner_radius(CornerRadius::same(8))
                        .stroke(egui::Stroke::new(1.0, theme::SEPARATOR))
                        .shadow(ui.visuals().popup_shadow)
                        .inner_margin(Margin::same(4))
                        .show(ui, |ui| {
                            ui.set_width(bar.width().max(380.0) - 8.0);
                            ui.label(theme::caption("Players").size(theme::text::TINY));
                            for (i, person) in suggestions.iter().enumerate() {
                                let row = suggestion(ui, person, data, i == self.highlight);
                                if row.hovered() {
                                    self.highlight = i;
                                }
                                if row.clicked() {
                                    picked = Some(i);
                                }
                            }
                            ui.label(
                                theme::muted("Enter opens them in Opponents · the list below is searched too")
                                    .size(theme::text::TINY)
                                    .color(theme::FAINT),
                            );
                        });
                });
            self.popup = Some(area.response.rect);
        }

        if let Some(person) = picked.and_then(|i| suggestions.get(i)) {
            if person.code.is_empty() {
                // Offline players have no code: show their games.
                *text = person.tag.clone();
                actions.push(Action::SetView(View::Replays));
            } else {
                actions.push(Action::GoToOpponent(person.code.clone()));
                text.clear();
            }
            self.open = false;
            ui.memory_mut(|m| m.surrender_focus(id));
        }
    }
}

fn suggestion(
    ui: &mut egui::Ui,
    person: &Person,
    data: &Data,
    highlighted: bool,
) -> egui::Response {
    let (rect, response) =
        ui.allocate_exact_size(vec2(ui.available_width(), ROW_HEIGHT), Sense::click());
    let painter = ui.painter();
    if highlighted {
        painter.rect_filled(rect, CornerRadius::same(6), theme::SELECTED_ROW);
    }
    let cy = rect.center().y;
    widgets::paint_stock(
        ui,
        Rect::from_center_size(pos2(rect.min.x + 20.0, cy), vec2(26.0, 26.0)),
        person.character,
        person.costume,
        Color32::WHITE,
    );
    let x = rect.min.x + 42.0;
    let right = rect.max.x - 10.0;
    let text_width = right - x - 150.0;
    paint_text_clipped(
        painter,
        pos2(x, cy - 8.0),
        person.title(),
        theme::heavy_font(theme::text::BODY),
        theme::TEXT,
        text_width,
    );
    let mut line: Vec<&str> = Vec::new();
    if !person.code.is_empty() && person.code != person.title() {
        line.push(&person.code);
    }
    if !person.tag.is_empty() && person.tag != person.title() {
        line.push(&person.tag);
    }
    let line = if line.is_empty() {
        crate::melee::character_name(person.character).to_owned()
    } else {
        line.join(" · ")
    };
    paint_text_clipped(
        painter,
        pos2(x, cy + 10.0),
        &line,
        FontId::proportional(theme::text::SMALL),
        theme::MUTED,
        text_width,
    );
    let decided = person.wins + person.losses;
    let record = if decided > 0 {
        format!("{}–{}", person.wins, person.losses)
    } else {
        String::new()
    };
    painter.text(
        pos2(right, cy - 8.0),
        egui::Align2::RIGHT_CENTER,
        record,
        theme::heavy_font(theme::text::SMALL),
        widgets::win_rate_color(person.wins, decided),
    );
    painter.text(
        pos2(right, cy + 10.0),
        egui::Align2::RIGHT_CENTER,
        format!(
            "{} game{}",
            person.games,
            if person.games == 1 { "" } else { "s" }
        ),
        FontId::proportional(theme::text::TINY),
        theme::FAINT,
    );
    if let Some(rank) = data
        .rank(&person.code)
        .and_then(|r| r.rank.as_deref())
        .map(display_rank)
    {
        let icon = Rect::from_center_size(pos2(right - 72.0, cy), vec2(20.0, 20.0));
        widgets::paint_rank(ui, icon, &rank);
    }
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}
