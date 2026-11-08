//! The details pane: the selected game or set at a glance, with Play and
//! the way to its stats.

use crate::app::{Action, Data, display_rank, outcome_word};
use crate::dolphin::Clip;
use crate::melee;
use crate::model::{Game, Outcome, Player, SetKind, set_score};
use crate::stats_view;
use crate::ui::theme::{self, Ion};
use crate::ui::widgets;
use eframe::egui::{self, Color32, CornerRadius, Margin, Rect, RichText, Sense, pos2, vec2};

pub enum Selection<'a> {
    None,
    Game(&'a Game),
    /// A set's games, newest first.
    Set(SetKind, Vec<&'a Game>),
}

pub fn show(
    ui: &mut egui::Ui,
    selection: Selection<'_>,
    data: &Data,
    conn: Option<&rusqlite::Connection>,
    actions: &mut Vec<Action>,
) {
    egui::ScrollArea::vertical()
        .id_salt("details")
        .auto_shrink([false, false])
        .show(ui, |ui| match selection {
            Selection::None => nothing_selected(ui),
            Selection::Game(game) => game_details(ui, game, data, conn, actions),
            Selection::Set(kind, games) => set_details(ui, kind, &games, data, conn, actions),
        });
}

fn nothing_selected(ui: &mut egui::Ui) {
    ui.add_space(120.0);
    ui.vertical_centered(|ui| {
        ui.label(Ion::GAME_CONTROLLER.rich(36.0).color(theme::FAINT));
        ui.add_space(theme::space::S);
        ui.label(theme::muted("Pick a game or a set to see it here."));
        ui.label(theme::muted("Enter opens its stats, Space plays it.").color(theme::FAINT));
    });
}

/// The stage picture across the top, darkened under its text.
fn banner(ui: &mut egui::Ui, stage: u16, title: &str, subtitle: &str) {
    let width = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(vec2(width, 132.0), Sense::hover());
    if let Some(source) = widgets::stage_source(stage) {
        // The square picture, cropped to the banner.
        let aspect = rect.height() / rect.width();
        let v0 = (0.5 - aspect / 2.0).max(0.0);
        egui::Image::new(source)
            .uv(Rect::from_min_max(
                pos2(0.0, v0),
                pos2(1.0, (v0 + aspect).min(1.0)),
            ))
            .tint(Color32::from_gray(150))
            .paint_at(ui, rect);
    } else {
        ui.painter().rect_filled(rect, 0, theme::STRIP);
    }
    let painter = ui.painter();
    // Fade to the pane's colour at the bottom.
    let mut mesh = egui::Mesh::default();
    let fade = Rect::from_min_max(pos2(rect.min.x, rect.center().y - 10.0), rect.max);
    mesh.colored_vertex(fade.left_top(), Color32::TRANSPARENT);
    mesh.colored_vertex(fade.right_top(), Color32::TRANSPARENT);
    mesh.colored_vertex(fade.right_bottom(), theme::SIDEBAR);
    mesh.colored_vertex(fade.left_bottom(), theme::SIDEBAR);
    mesh.add_triangle(0, 1, 2);
    mesh.add_triangle(0, 2, 3);
    painter.add(mesh);
    painter.text(
        pos2(rect.min.x + 16.0, rect.max.y - 34.0),
        egui::Align2::LEFT_BOTTOM,
        title,
        theme::heavy_font(theme::text::HEADING + 2.0),
        theme::SELECTED_TEXT,
    );
    painter.text(
        pos2(rect.min.x + 16.0, rect.max.y - 14.0),
        egui::Align2::LEFT_BOTTOM,
        subtitle,
        egui::FontId::proportional(theme::text::SMALL),
        theme::TEXT,
    );
}

fn section(ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui)) {
    egui::Frame::new()
        .inner_margin(Margin::symmetric(16, 8))
        .show(ui, add);
}

fn field(ui: &mut egui::Ui, label: &str, value: impl Into<egui::WidgetText>) {
    ui.label(theme::caption(label));
    ui.label(value);
    ui.add_space(2.0);
}

/// A player's card: stock icon, name, code, rank.
fn player_card(
    ui: &mut egui::Ui,
    player: &Player,
    data: &Data,
    outcome: Option<Outcome>,
    stocks_left: Option<u8>,
    actions: &mut Vec<Action>,
) {
    egui::Frame::new()
        .fill(theme::SURFACE)
        .corner_radius(CornerRadius::same(8))
        .inner_margin(Margin::same(10))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                widgets::stock_icon(ui, player.character, player.costume, 34.0);
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = 1.0;
                    ui.horizontal(|ui| {
                        ui.label(theme::heavy(player.display_name(), theme::text::LIST));
                        if let Some(outcome) = outcome {
                            theme::pill(ui, outcome_word(outcome), widgets::outcome_color(outcome));
                        }
                    });
                    let mut line = melee::character_name(player.character).to_owned();
                    if !player.code.is_empty() && player.code != player.display_name() {
                        line = format!("{} · {}", player.code, line);
                    }
                    ui.label(theme::muted(line).size(theme::text::SMALL));
                });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if let Some(left) = stocks_left {
                        ui.label(theme::muted(format!("{left} left")).size(theme::text::SMALL));
                    }
                });
            });
            if !player.code.is_empty() && !data.me.is_me(&player.code) {
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    let record = data.rank(&player.code);
                    rank_line(ui, record, data);
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui
                            .small_button("Opponent")
                            .on_hover_text("Your record against them, their rank and your notes")
                            .clicked()
                        {
                            actions.push(Action::GoToOpponent(player.code.clone()));
                        }
                    });
                });
                if let Some(note) = data.notes.get(&player.code) {
                    ui.add_space(4.0);
                    ui.label(
                        RichText::new(note)
                            .size(theme::text::SMALL)
                            .color(theme::WARNING)
                            .italics(),
                    );
                }
            }
        });
}

pub fn rank_line(ui: &mut egui::Ui, record: Option<&crate::library::db::RankRecord>, data: &Data) {
    match record {
        Some(record) => {
            let rank = record.rank.as_deref().map(display_rank);
            match rank {
                Some(rank) => {
                    if let Some(source) = widgets::rank_source(&rank) {
                        ui.add(egui::Image::new(source).fit_to_exact_size(vec2(20.0, 20.0)));
                    }
                    ui.label(RichText::new(&rank).color(widgets::rank_color(&rank)));
                    if let Some(rating) = record.rating.filter(|r| *r > 0.0) {
                        ui.label(theme::muted(format!("{rating:.0}")).size(theme::text::SMALL));
                    }
                }
                None if record.found => {
                    ui.label(theme::muted("No ranked profile"));
                }
                None => {
                    ui.label(theme::muted("No Slippi account with this code"));
                }
            }
            ui.label(
                theme::muted(format!(
                    "· {}",
                    crate::time::ago(record.fetched_at, data.now)
                ))
                .size(theme::text::SMALL)
                .color(theme::FAINT),
            );
        }
        None => {
            ui.label(theme::muted("Rank not looked up yet"));
        }
    }
}

fn game_details(
    ui: &mut egui::Ui,
    game: &Game,
    data: &Data,
    conn: Option<&rusqlite::Connection>,
    actions: &mut Vec<Action>,
) {
    let outcome = game.outcome(&data.me);
    let title = match (game.sides(&data.me), outcome) {
        (Some(_), Outcome::Win) => "Won".to_owned(),
        (Some(_), Outcome::Loss) => "Lost".to_owned(),
        _ => melee::stage_name(game.stage).to_owned(),
    };
    let subtitle = format!(
        "{} · {} · {}",
        melee::stage_name(game.stage),
        crate::time::duration(game.seconds()),
        crate::time::short(game.start_at, data.now)
    );
    banner(ui, game.stage, &title, &subtitle);

    section(ui, |ui| {
        ui.horizontal(|ui| {
            if ui
                .add(theme::primary_icon_button(Ion::PLAY, "Play"))
                .on_hover_text("Watch in Slippi Dolphin (Space)")
                .clicked()
            {
                actions.push(Action::Play(vec![Clip::whole(&game.path)]));
            }
            if ui
                .add(theme::icon_button(Ion::STATS_CHART, "Stats"))
                .on_hover_text("Kills, punishes and tech (Enter)")
                .clicked()
            {
                actions.push(Action::OpenStats(game.id));
            }
            if ui
                .add(egui::Button::new(Ion::FOLDER_OPEN.rich(16.0)))
                .on_hover_text("Show in Explorer")
                .clicked()
            {
                actions.push(Action::ShowFile(game.path.clone()));
            }
        });
    });

    let ordered = ordered_players(game, data);
    section(ui, |ui| {
        for player in &ordered {
            let outcome = if game.winners.is_empty() {
                None
            } else if game.winners.contains(&player.port) {
                Some(Outcome::Win)
            } else {
                Some(Outcome::Loss)
            };
            player_card(
                ui,
                player,
                data,
                outcome.filter(|_| game.is_singles()),
                game.stocks_left(player.port),
                actions,
            );
            ui.add_space(4.0);
        }
    });

    if let Some(stats) = stats_view::cached(ui, conn, game) {
        section(ui, |ui| {
            ui.label(theme::caption("At a glance"));
            ui.add_space(4.0);
            stats_view::headline_table(ui, game, &stats, &ordered);
        });
    } else if game.stats_version == 0 {
        section(ui, |ui| {
            ui.label(
                theme::muted("Stats are on their way (they're worked out in the background).")
                    .size(theme::text::SMALL),
            );
        });
    }

    section(ui, |ui| {
        field(ui, "Mode", game.mode.label());
        field(ui, "Started", crate::time::long(game.start_at));
        if !game.match_id.is_empty() {
            let game_line = if game.tiebreaker > 0 {
                format!("Game {} (tiebreak {})", game.game_number, game.tiebreaker)
            } else {
                format!("Game {}", game.game_number)
            };
            field(ui, "In the match", game_line);
        }
        if let Some(quitter) = game.lras.and_then(|p| game.player(p)) {
            field(
                ui,
                "Ended",
                format!("{} quit (L+R+A+Start)", quitter.display_name()),
            );
        }
        ui.label(theme::caption("File"));
        let file = game.path.display().to_string();
        if ui
            .add(
                egui::Label::new(
                    RichText::new(&file)
                        .size(theme::text::SMALL)
                        .color(theme::MUTED),
                )
                .sense(Sense::click()),
            )
            .on_hover_text("Click to copy")
            .clicked()
        {
            actions.push(Action::CopyText(file));
        }
    });
}

/// Me first, then the opponent (or the ports' order).
fn ordered_players<'a>(game: &'a Game, data: &Data) -> Vec<&'a Player> {
    match game.sides(&data.me) {
        Some(s) => vec![&game.players[s.me], &game.players[s.opponent]],
        None => game.players.iter().collect(),
    }
}

fn set_details(
    ui: &mut egui::Ui,
    kind: SetKind,
    games: &[&Game],
    data: &Data,
    conn: Option<&rusqlite::Connection>,
    actions: &mut Vec<Action>,
) {
    let Some(first) = games.first() else { return };
    let indexes: Vec<usize> = games
        .iter()
        .filter_map(|g| data.index.get(&g.id).copied())
        .collect();
    let (mine, theirs) = set_score(&data.games, &indexes, &data.me);
    let opponent = first.opponent(&data.me);
    let title = match (opponent, mine.cmp(&theirs)) {
        (Some(_), std::cmp::Ordering::Greater) => format!("Won {mine}–{theirs}"),
        (Some(_), std::cmp::Ordering::Less) => format!("Lost {mine}–{theirs}"),
        (Some(_), _) => format!("{mine}–{theirs}"),
        (None, _) => widgets::count(games.len(), "game"),
    };
    let label = match kind {
        SetKind::Ranked => "Ranked set",
        SetKind::Session => "Session",
    };
    let seconds: f64 = games.iter().map(|g| g.seconds()).sum();
    let subtitle = format!(
        "{label} · {} · {} · {}",
        widgets::count(games.len(), "game"),
        crate::time::duration(seconds),
        crate::time::short(first.start_at, data.now)
    );
    // The last game's stage, or the first's when the set was one game.
    banner(
        ui,
        games.last().map_or(first.stage, |g| g.stage),
        &title,
        &subtitle,
    );

    section(ui, |ui| {
        ui.horizontal(|ui| {
            if ui
                .add(theme::primary_icon_button(Ion::PLAY, "Play set"))
                .on_hover_text("Watch every game in Dolphin, in order (Space)")
                .clicked()
            {
                actions.push(Action::Play(
                    games.iter().rev().map(|g| Clip::whole(&g.path)).collect(),
                ));
            }
            if let Some(opponent) = opponent.filter(|p| !p.code.is_empty())
                && ui
                    .add(theme::icon_button(Ion::PERSON, "Opponent"))
                    .clicked()
            {
                actions.push(Action::GoToOpponent(opponent.code.clone()));
            }
        });
    });

    let ordered = ordered_players(first, data);
    section(ui, |ui| {
        for player in &ordered {
            let outcome = opponent.map(|_| {
                let mine_side = data.me.is_me(&player.code);
                match (mine.cmp(&theirs), mine_side) {
                    (std::cmp::Ordering::Greater, true) | (std::cmp::Ordering::Less, false) => {
                        Outcome::Win
                    }
                    (std::cmp::Ordering::Equal, _) => Outcome::Unknown,
                    _ => Outcome::Loss,
                }
            });
            player_card(
                ui,
                player,
                data,
                outcome.filter(|o| *o != Outcome::Unknown),
                None,
                actions,
            );
            ui.add_space(4.0);
        }
    });

    section(ui, |ui| {
        ui.label(theme::caption("Games"));
        ui.add_space(4.0);
        for game in games.iter().rev() {
            let outcome = game.outcome(&data.me);
            let (rect, response) =
                ui.allocate_exact_size(vec2(ui.available_width(), 34.0), Sense::click());
            if response.hovered() {
                ui.painter()
                    .rect_filled(rect, CornerRadius::same(6), theme::HOVERED_ROW);
            }
            let painter = ui.painter();
            let cy = rect.center().y;
            let mark = Rect::from_center_size(pos2(rect.min.x + 14.0, cy), vec2(20.0, 20.0));
            if opponent.is_some() {
                widgets::paint_outcome(painter, mark, outcome);
            }
            let mut x = rect.min.x + 32.0;
            for player in &ordered {
                let r = Rect::from_center_size(pos2(x + 10.0, cy), vec2(20.0, 20.0));
                widgets::paint_stock(ui, r, player.character, player.costume, Color32::WHITE);
                x += 24.0;
            }
            painter.text(
                pos2(x + 6.0, cy),
                egui::Align2::LEFT_CENTER,
                melee::stage_name(game.stage),
                egui::FontId::proportional(theme::text::BODY),
                theme::TEXT,
            );
            painter.text(
                pos2(rect.max.x - 8.0, cy),
                egui::Align2::RIGHT_CENTER,
                crate::time::duration(game.seconds()),
                egui::FontId::proportional(theme::text::SMALL),
                theme::MUTED,
            );
            let response = response
                .on_hover_text("Click for its stats · right-click to play")
                .on_hover_cursor(egui::CursorIcon::PointingHand);
            if response.clicked() {
                actions.push(Action::OpenStats(game.id));
            }
            if response.secondary_clicked() {
                actions.push(Action::Play(vec![Clip::whole(&game.path)]));
            }
        }
    });

    // The set's numbers, summed over its games.
    let all: Vec<_> = games
        .iter()
        .filter_map(|g| stats_view::cached(ui, conn, g).map(|s| (*g, s)))
        .collect();
    if all.len() == games.len() && !all.is_empty() {
        section(ui, |ui| {
            ui.label(theme::caption("Across the set"));
            ui.add_space(4.0);
            stats_view::set_table(ui, &all, &ordered);
        });
    }
}
