//! Highlights: every punish in the index that fits (big damage, kills,
//! zero-to-deaths; yours or theirs) across the filtered games, best first,
//! ready to watch one by one or all in a row in Dolphin.

use crate::app::{Action, Data, Derived};
use crate::dolphin::Clip;
use crate::melee;
use crate::ui::replay_list::Nav;
use crate::ui::theme::{self, Ion};
use crate::ui::virtual_list::{ScrollAlign, VirtualList};
use crate::ui::widgets::{self, paint_text_clipped};
use eframe::egui::{self, Color32, FontId, Margin, Rect, Sense, pos2, vec2};
use std::collections::HashSet;

/// The most punishes listed.
const MAX_HITS: usize = 2_000;

/// How a punish started, in words ("NeutralWin" → "neutral win").
pub fn opening_label(opening: &str) -> &'static str {
    match opening {
        "NeutralWin" => "neutral win",
        "CounterAttack" => "counter hit",
        "Trade" => "trade",
        _ => "opening",
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Whose {
    Mine,
    Theirs,
    Anyone,
}

#[derive(Debug, Clone, PartialEq)]
struct Query {
    whose: Whose,
    min_damage: f32,
    kills_only: bool,
    zero_to_death: bool,
}

impl Default for Query {
    fn default() -> Self {
        Self {
            whose: Whose::Mine,
            min_damage: 60.0,
            kills_only: false,
            zero_to_death: false,
        }
    }
}

#[derive(Debug, Clone)]
struct Hit {
    game: i64,
    attacker: Option<u8>,
    start_frame: i32,
    end_frame: Option<i32>,
    start_percent: f64,
    end_percent: f64,
    hits: u32,
    did_kill: bool,
    opening: String,
}

#[derive(Default)]
pub struct State {
    query: Query,
    hits: Vec<Hit>,
    built_for: Option<(u64, String)>,
    list: VirtualList<usize>,
    selected: Option<usize>,
    visible: (usize, usize),
    /// Space was pressed on this row: play it on the next frame (the clip
    /// needs the data the keyboard handler doesn't get).
    play_index: Option<usize>,
}

impl State {
    fn rebuild(&mut self, data: &Data, derived: &Derived, conn: Option<&rusqlite::Connection>) {
        let key = (derived.revision, format!("{:?}", self.query));
        if self.built_for.as_ref() == Some(&key) {
            return;
        }
        self.built_for = Some(key);
        self.selected = None;
        self.hits.clear();
        let Some(conn) = conn else { return };
        let allowed: HashSet<i64> = derived.order.iter().map(|&i| data.games[i].id).collect();
        let q = &self.query;
        let min = if q.zero_to_death {
            0.0
        } else {
            q.min_damage as f64
        };
        let mut sql = String::from(
            "SELECT game_id, attacker, start_frame, end_frame, start_percent, end_percent, hits, did_kill, opening
             FROM punishes WHERE end_percent - start_percent >= ?1",
        );
        if q.kills_only || q.zero_to_death {
            sql.push_str(" AND did_kill = 1");
        }
        if q.zero_to_death {
            sql.push_str(" AND start_percent < 1");
        }
        sql.push_str(" ORDER BY end_percent - start_percent DESC LIMIT 20000");
        let Ok(mut statement) = conn.prepare(&sql) else {
            return;
        };
        let rows = statement.query_map([min], |r| {
            Ok(Hit {
                game: r.get(0)?,
                attacker: r.get::<_, Option<i64>>(1)?.map(|p| p as u8),
                start_frame: r.get(2)?,
                end_frame: r.get(3)?,
                start_percent: r.get(4)?,
                end_percent: r.get(5)?,
                hits: r.get::<_, i64>(6)? as u32,
                did_kill: r.get(7)?,
                opening: r.get(8)?,
            })
        });
        let Ok(rows) = rows else { return };
        for hit in rows.flatten() {
            if !allowed.contains(&hit.game) {
                continue;
            }
            let Some(game) = data.game(hit.game) else {
                continue;
            };
            let mine = game.mine(&data.me).map(|p| p.port);
            let keep = match q.whose {
                Whose::Anyone => true,
                Whose::Mine => mine.is_some() && hit.attacker == mine,
                Whose::Theirs => mine.is_some() && hit.attacker.is_some() && hit.attacker != mine,
            };
            if keep {
                self.hits.push(hit);
            }
            if self.hits.len() >= MAX_HITS {
                break;
            }
        }
    }

    fn clip(data: &Data, hit: &Hit) -> Option<Clip> {
        let game = data.game(hit.game)?;
        Some(Clip {
            path: game.path.clone(),
            start_frame: Some((hit.start_frame - 90).max(melee::FIRST_FRAME)),
            end_frame: hit.end_frame.map(|f| f + 90),
        })
    }

    pub fn navigate(&mut self, nav: Option<Nav>, actions: &mut Vec<Action>) {
        let Some(nav) = nav else { return };
        if self.hits.is_empty() {
            return;
        }
        let last = self.hits.len() - 1;
        let page = (self.visible.1.saturating_sub(self.visible.0)).max(1);
        let target = match (nav, self.selected) {
            (Nav::Up, Some(i)) => i.saturating_sub(1),
            (Nav::Down, Some(i)) => (i + 1).min(last),
            (Nav::PageUp, i) => i.unwrap_or(0).saturating_sub(page),
            (Nav::PageDown, i) => (i.unwrap_or(0) + page).min(last),
            (Nav::End, _) => last,
            (Nav::Up | Nav::Down | Nav::Home, _) => 0,
            (Nav::Enter, Some(i)) => {
                actions.push(Action::OpenStats(self.hits[i].game));
                return;
            }
            (Nav::Space, Some(i)) => {
                self.play_index = Some(i);
                return;
            }
            _ => return,
        };
        self.selected = Some(target);
        self.list.scroll_to(target, ScrollAlign::Nearest, false);
    }

    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        data: &Data,
        derived: &Derived,
        conn: Option<&rusqlite::Connection>,
        actions: &mut Vec<Action>,
    ) {
        if let Some(i) = self.play_index.take()
            && let Some(clip) = self.hits.get(i).and_then(|h| Self::clip(data, h))
        {
            actions.push(Action::Play(vec![clip]));
        }
        egui::Frame::new()
            .inner_margin(Margin::symmetric(16, 10))
            .show(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing.x = 10.0;
                    for (whose, label) in [
                        (Whose::Mine, "Mine"),
                        (Whose::Theirs, "Against me"),
                        (Whose::Anyone, "Anyone's"),
                    ] {
                        if ui
                            .add(egui::Button::new(label).selected(self.query.whose == whose))
                            .clicked()
                        {
                            self.query.whose = whose;
                        }
                    }
                    ui.separator();
                    ui.add_enabled_ui(!self.query.zero_to_death, |ui| {
                        ui.label(theme::muted("At least"));
                        ui.add(
                            egui::Slider::new(&mut self.query.min_damage, 20.0..=150.0)
                                .step_by(5.0)
                                .suffix("%"),
                        );
                    });
                    ui.checkbox(&mut self.query.kills_only, "Ended in a kill");
                    ui.checkbox(&mut self.query.zero_to_death, "Zero-to-deaths");
                });
                ui.add_space(6.0);
                self.rebuild(data, derived, conn);
                ui.horizontal(|ui| {
                    let count = widgets::thousands(self.hits.len());
                    let title = if self.hits.len() >= MAX_HITS {
                        format!("The top {count} punishes")
                    } else {
                        format!("{count} punishes")
                    };
                    ui.label(theme::heavy(title, theme::text::TITLE));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let count = self.hits.len().min(50);
                        if count > 0
                            && ui
                                .add(theme::primary_icon_button(
                                    Ion::PLAY_FORWARD,
                                    &format!("Play top {count}"),
                                ))
                                .on_hover_text("Watch them back to back in Dolphin")
                                .clicked()
                        {
                            let clips = self
                                .hits
                                .iter()
                                .take(count)
                                .filter_map(|h| Self::clip(data, h))
                                .collect();
                            actions.push(Action::Play(clips));
                        }
                    });
                });
            });
        if self.hits.is_empty() {
            ui.add_space(50.0);
            ui.vertical_centered(|ui| {
                ui.label(Ion::FLASH.rich(40.0).color(theme::FAINT));
                let waiting = data.progress.phase == crate::library::Phase::Stats;
                ui.label(theme::muted(if waiting {
                    "Punishes show up here as your games' stats are worked out."
                } else {
                    "Nothing fits. Lower the damage, or loosen the filters."
                }));
            });
            return;
        }
        let width = ui.available_width();
        let revision = crate::model::hash_str(&format!("{:?}", self.built_for));
        self.list
            .sync(revision, width, (0..self.hits.len()).map(|i| (i, 0, 50.0)));
        let hits = &self.hits;
        let selected = self.selected;
        let mut clicked = None;
        let mut play = None;
        let output = self.list.show(ui, "highlights", |ui, index, _| {
            let (response, play_clicked) =
                row(ui, &hits[index], data, selected == Some(index), index + 1);
            if response.clicked() {
                clicked = Some(index);
            }
            if response.double_clicked() || play_clicked {
                play = Some(index);
            }
            response.context_menu(|ui| {
                if ui
                    .add(theme::icon_button(
                        Ion::STATS_CHART,
                        "Open the game's stats",
                    ))
                    .clicked()
                {
                    actions.push(Action::OpenStats(hits[index].game));
                    ui.close();
                }
                if ui
                    .add(theme::icon_button(Ion::FILM, "Show in Replays"))
                    .clicked()
                {
                    actions.push(Action::GoToGame(hits[index].game));
                    ui.close();
                }
            });
        });
        self.visible = (output.first_visible, output.last_visible);
        if let Some(i) = clicked {
            self.selected = Some(i);
        }
        if let Some(i) = play
            && let Some(clip) = Self::clip(data, &self.hits[i])
        {
            self.selected = Some(i);
            actions.push(Action::Play(vec![clip]));
        }
    }
}

fn row(
    ui: &mut egui::Ui,
    hit: &Hit,
    data: &Data,
    selected: bool,
    rank: usize,
) -> (egui::Response, bool) {
    let (rect, response) = ui.allocate_exact_size(vec2(ui.available_width(), 50.0), Sense::click());
    if !ui.is_rect_visible(rect) {
        return (response, false);
    }
    let inner = rect.shrink2(vec2(6.0, 2.0));
    if selected {
        ui.painter().rect_filled(inner, 6, theme::SELECTED_ROW);
    } else if response.hovered() {
        ui.painter().rect_filled(inner, 6, theme::HOVERED_ROW);
    }
    let Some(game) = data.game(hit.game) else {
        return (response, false);
    };
    let cy = rect.center().y;
    let painter = ui.painter();
    painter.text(
        pos2(rect.min.x + 30.0, cy),
        egui::Align2::RIGHT_CENTER,
        rank.to_string(),
        FontId::proportional(theme::text::SMALL),
        theme::FAINT,
    );
    let attacker = hit.attacker.and_then(|p| game.player(p));
    let victim = game.players.iter().find(|p| Some(p.port) != hit.attacker);
    let mut x = rect.min.x + 42.0;
    if let Some(a) = attacker {
        widgets::paint_stock(
            ui,
            Rect::from_center_size(pos2(x + 13.0, cy), vec2(26.0, 26.0)),
            a.character,
            a.costume,
            Color32::WHITE,
        );
    }
    x += 30.0;
    ui.painter().text(
        pos2(x + 6.0, cy),
        egui::Align2::CENTER_CENTER,
        "›",
        FontId::proportional(theme::text::BODY),
        theme::FAINT,
    );
    x += 14.0;
    if let Some(v) = victim {
        widgets::paint_stock(
            ui,
            Rect::from_center_size(pos2(x + 11.0, cy), vec2(22.0, 22.0)),
            v.character,
            v.costume,
            Color32::WHITE,
        );
    }
    x += 34.0;
    let painter = ui.painter();
    let damage = hit.end_percent - hit.start_percent;
    painter.text(
        pos2(x, cy - 8.0),
        egui::Align2::LEFT_CENTER,
        format!("{damage:.0}%"),
        theme::heavy_font(theme::text::TITLE),
        if hit.did_kill {
            theme::ACCENT_TEXT
        } else {
            theme::TEXT
        },
    );
    painter.text(
        pos2(x, cy + 11.0),
        egui::Align2::LEFT_CENTER,
        format!(
            "{:.0}% to {:.0}%{}",
            hit.start_percent,
            hit.end_percent,
            if hit.did_kill { " · killed" } else { "" }
        ),
        FontId::proportional(theme::text::SMALL),
        theme::MUTED,
    );
    let name_x = x + 120.0;
    let who = match (attacker, victim) {
        (Some(a), Some(v)) => format!("{} on {}", a.display_name(), v.display_name()),
        _ => String::new(),
    };
    let right = rect.max.x - 50.0;
    paint_text_clipped(
        painter,
        pos2(name_x, cy - 8.0),
        &who,
        theme::heavy_font(theme::text::BODY),
        theme::TEXT,
        right - name_x - 110.0,
    );
    paint_text_clipped(
        painter,
        pos2(name_x, cy + 11.0),
        &format!(
            "{} hits · {} · {}",
            hit.hits,
            opening_label(&hit.opening),
            melee::stage_name(game.stage)
        ),
        FontId::proportional(theme::text::SMALL),
        theme::MUTED,
        right - name_x - 110.0,
    );
    painter.text(
        pos2(right - 8.0, cy),
        egui::Align2::RIGHT_CENTER,
        crate::time::short(game.start_at, data.now),
        FontId::proportional(theme::text::SMALL),
        theme::MUTED,
    );
    let play_rect = Rect::from_center_size(pos2(rect.max.x - 26.0, cy), vec2(30.0, 30.0));
    let play = ui.interact(play_rect, response.id.with("play"), Sense::click());
    if play.hovered() {
        ui.painter()
            .rect_filled(play_rect, 6, theme::BUTTON_HOVERED);
    }
    Ion::PLAY.paint(
        ui.painter(),
        play_rect,
        15.0,
        if play.hovered() {
            theme::ACCENT_TEXT
        } else {
            theme::MUTED
        },
    );
    let clicked = play.on_hover_text("Play this punish").clicked();
    (
        response.on_hover_cursor(egui::CursorIcon::PointingHand),
        clicked,
    )
}
