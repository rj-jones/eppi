//! The replay list: sets fold their games under one row (open them with a
//! click on the arrow, → or Enter), every row is drawn with the painter at
//! a fixed height, and only rows on screen are drawn (blabbur's virtual
//! list), so 8,000+ games scroll as smoothly as 80.

use crate::app::{Action, Data, Derived, display_rank};
use crate::dolphin::Clip;
use crate::melee;
use crate::model::{Game, GameSet, Outcome, Row, SetKind, set_score};
use crate::ui::details::Selection;
use crate::ui::theme::{self, Ion};
use crate::ui::virtual_list::{ScrollAlign, VirtualList};
use crate::ui::widgets::{self, paint_text_clipped};
use eframe::egui::{self, Color32, CornerRadius, FontId, Rect, Sense, pos2, vec2};
use std::collections::HashSet;

pub const SET_HEIGHT: f32 = 58.0;
pub const GAME_HEIGHT: f32 = 48.0;
pub const CHILD_HEIGHT: f32 = 40.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Key {
    Set(u64),
    Game(i64),
}

#[derive(Debug, Clone, Copy)]
pub enum Item {
    /// A set's header; `row` indexes the rows.
    Set { row: usize },
    /// A game; `index` into `Data::games`; `child` when under a set.
    Game { index: usize, child: bool },
}

/// Keyboard movement in a list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Nav {
    Up,
    Down,
    PageUp,
    PageDown,
    Home,
    End,
    Left,
    Right,
    Enter,
    Space,
}

impl Nav {
    pub fn from_input(ctx: &egui::Context) -> Option<Nav> {
        ctx.input(|i| {
            let pressed = |key| i.key_pressed(key);
            if pressed(egui::Key::ArrowUp) || pressed(egui::Key::K) {
                Some(Nav::Up)
            } else if pressed(egui::Key::ArrowDown) || pressed(egui::Key::J) {
                Some(Nav::Down)
            } else if pressed(egui::Key::PageUp) {
                Some(Nav::PageUp)
            } else if pressed(egui::Key::PageDown) {
                Some(Nav::PageDown)
            } else if pressed(egui::Key::Home) {
                Some(Nav::Home)
            } else if pressed(egui::Key::End) {
                Some(Nav::End)
            } else if pressed(egui::Key::ArrowLeft) {
                Some(Nav::Left)
            } else if pressed(egui::Key::ArrowRight) {
                Some(Nav::Right)
            } else if pressed(egui::Key::Enter) {
                Some(Nav::Enter)
            } else if pressed(egui::Key::Space) {
                Some(Nav::Space)
            } else {
                None
            }
        })
    }
}

/// A list of rows with sets that open and close, and a selection.
#[derive(Default)]
pub struct ListState {
    list: VirtualList<Key>,
    items: Vec<(Key, Item)>,
    built_for: Option<(u64, u64)>,
    expanded: HashSet<u64>,
    expanded_revision: u64,
    pub selected: Option<Key>,
    visible: (usize, usize),
    last_derived: u64,
}

impl ListState {
    fn rebuild(&mut self, data: &Data, rows: &[Row], revision: u64) {
        let key = (revision, self.expanded_revision);
        if self.built_for == Some(key) {
            return;
        }
        self.built_for = Some(key);
        self.items.clear();
        for (row_index, row) in rows.iter().enumerate() {
            match row {
                Row::Set(set) => {
                    self.items
                        .push((Key::Set(set.key), Item::Set { row: row_index }));
                    if self.expanded.contains(&set.key) {
                        for &index in &set.games {
                            self.items.push((
                                Key::Game(data.games[index].id),
                                Item::Game { index, child: true },
                            ));
                        }
                    }
                }
                Row::Game(index) => self.items.push((
                    Key::Game(data.games[*index].id),
                    Item::Game {
                        index: *index,
                        child: false,
                    },
                )),
            }
        }
        if let Some(selected) = self.selected
            && !self.items.iter().any(|(k, _)| *k == selected)
        {
            self.selected = None;
        }
    }

    fn toggle(&mut self, key: u64) {
        if !self.expanded.remove(&key) {
            self.expanded.insert(key);
        }
        self.expanded_revision += 1;
    }

    fn position(&self) -> Option<usize> {
        let selected = self.selected?;
        self.items.iter().position(|(k, _)| *k == selected)
    }

    /// Selects the game `id`, opening its set, and scrolls to it.
    pub fn reveal_game(&mut self, id: i64, data: &Data, derived: &Derived) {
        if let Some(&index) = data.index.get(&id) {
            for row in &derived.rows {
                if let Row::Set(set) = row
                    && set.games.contains(&index)
                    && self.expanded.insert(set.key)
                {
                    self.expanded_revision += 1;
                }
            }
        }
        self.selected = Some(Key::Game(id));
        self.list
            .scroll_to(Key::Game(id), ScrollAlign::Center, true);
    }

    pub fn navigate(
        &mut self,
        nav: Option<Nav>,
        data: &Data,
        derived: &Derived,
        actions: &mut Vec<Action>,
    ) {
        self.rebuild(data, &derived.rows, derived.revision);
        self.navigate_items(nav, data, &derived.rows, actions);
    }

    /// Moves the selection through `rows` (already rebuilt).
    pub fn navigate_items(
        &mut self,
        nav: Option<Nav>,
        data: &Data,
        rows: &[Row],
        actions: &mut Vec<Action>,
    ) {
        let Some(nav) = nav else { return };
        if self.items.is_empty() {
            return;
        }
        let page = (self.visible.1.saturating_sub(self.visible.0)).max(1);
        let current = self.position();
        let last = self.items.len() - 1;
        let target = match (nav, current) {
            (Nav::Up, Some(i)) => Some(i.saturating_sub(1)),
            (Nav::Down, Some(i)) => Some((i + 1).min(last)),
            (Nav::Up | Nav::Down | Nav::Home, None) => Some(0),
            (Nav::PageUp, i) => Some(i.unwrap_or(0).saturating_sub(page)),
            (Nav::PageDown, i) => Some((i.unwrap_or(0) + page).min(last)),
            (Nav::Home, _) => Some(0),
            (Nav::End, _) => Some(last),
            (Nav::Left, Some(i)) => match self.items[i] {
                (Key::Set(key), _) if self.expanded.contains(&key) => {
                    self.toggle(key);
                    None
                }
                (_, Item::Game { child: true, .. }) => (0..i)
                    .rev()
                    .find(|&j| matches!(self.items[j].1, Item::Set { .. })),
                _ => None,
            },
            (Nav::Right, Some(i)) => {
                if let (Key::Set(key), _) = self.items[i]
                    && !self.expanded.contains(&key)
                {
                    self.toggle(key);
                }
                None
            }
            (Nav::Enter, Some(i)) => {
                match self.items[i] {
                    (Key::Set(key), _) => self.toggle(key),
                    (Key::Game(id), _) => actions.push(Action::OpenStats(id)),
                }
                None
            }
            (Nav::Space, Some(_)) => {
                let clips: Vec<Clip> = self
                    .selected_games_in(data, rows)
                    .iter()
                    .rev()
                    .filter_map(|id| data.game(*id))
                    .map(|g| Clip::whole(&g.path))
                    .collect();
                if !clips.is_empty() {
                    actions.push(Action::Play(clips));
                }
                None
            }
            _ => None,
        };
        if let Some(target) = target {
            let key = self.items[target].0;
            self.selected = Some(key);
            self.list.scroll_to(key, ScrollAlign::Nearest, false);
        }
    }

    pub fn selected_games(&self, data: &Data, derived: &Derived) -> Vec<i64> {
        self.selected_games_in(data, &derived.rows)
    }

    fn selected_games_in(&self, data: &Data, rows: &[Row]) -> Vec<i64> {
        match self.selected {
            Some(Key::Game(id)) => vec![id],
            Some(Key::Set(key)) => rows
                .iter()
                .find_map(|r| match r {
                    Row::Set(set) if set.key == key => {
                        Some(set.games.iter().map(|&i| data.games[i].id).collect())
                    }
                    _ => None,
                })
                .unwrap_or_default(),
            None => Vec::new(),
        }
    }

    pub fn selection<'a>(&self, data: &'a Data, derived: &'a Derived) -> Selection<'a> {
        self.selection_in(data, &derived.rows)
    }

    pub fn selection_in<'a>(&self, data: &'a Data, rows: &'a [Row]) -> Selection<'a> {
        match self.selected {
            Some(Key::Game(id)) => data.game(id).map_or(Selection::None, Selection::Game),
            Some(Key::Set(key)) => rows
                .iter()
                .find_map(|r| match r {
                    Row::Set(set) if set.key == key => Some(Selection::Set(
                        set.kind,
                        set.games.iter().map(|&i| &data.games[i]).collect(),
                    )),
                    _ => None,
                })
                .unwrap_or(Selection::None),
            None => Selection::None,
        }
    }

    /// Opponents' codes on screen (to look up their ranks).
    pub fn visible_codes(&self, data: &Data, derived: &Derived) -> Vec<String> {
        self.visible_codes_in(data, &derived.rows)
    }

    pub fn visible_codes_in(&self, data: &Data, rows: &[Row]) -> Vec<String> {
        let (first, last) = self.visible;
        let mut codes = Vec::new();
        for (_, item) in self.items.iter().take(last + 1).skip(first) {
            let index = match item {
                Item::Set { row } => match rows.get(*row) {
                    Some(Row::Set(set)) => set.games.first().copied(),
                    _ => None,
                },
                Item::Game { index, .. } => Some(*index),
            };
            if let Some(game) = index.and_then(|i| data.games.get(i))
                && let Some(opponent) = game.opponent(&data.me)
                && !opponent.code.is_empty()
            {
                codes.push(opponent.code.clone());
            }
        }
        codes
    }

    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        data: &Data,
        derived: &Derived,
        actions: &mut Vec<Action>,
    ) {
        if derived.revision != self.last_derived {
            // A new filter or search starts at the top.
            let filter_changed = self.last_derived != 0 && self.items.len() != derived.order.len();
            self.last_derived = derived.revision;
            if filter_changed && self.selected.is_none() {
                self.list.scroll_to_top();
            }
        }
        if derived.order.is_empty() {
            empty_state(ui, data);
            return;
        }
        self.show_rows(
            ui,
            data,
            &derived.rows,
            derived.revision,
            "replays",
            actions,
        );
    }

    /// Draws `rows` (rebuilt for `revision`).
    pub fn show_rows(
        &mut self,
        ui: &mut egui::Ui,
        data: &Data,
        rows: &[Row],
        revision: u64,
        id: &str,
        actions: &mut Vec<Action>,
    ) {
        self.rebuild(data, rows, revision);
        let width = ui.available_width();
        let items = &self.items;
        self.list.sync(
            revision ^ (self.expanded_revision << 32),
            width,
            items.iter().map(|(key, item)| {
                let height = match item {
                    Item::Set { .. } => SET_HEIGHT,
                    Item::Game { child: false, .. } => GAME_HEIGHT,
                    Item::Game { child: true, .. } => CHILD_HEIGHT,
                };
                (*key, 0, height)
            }),
        );
        let mut clicked: Option<(Key, bool)> = None;
        let mut toggle: Option<u64> = None;
        let selected = self.selected;
        let flash = self.list.highlighted();
        let list = &mut self.list;
        let output = list.show(ui, id, |ui, index, key| {
            let item = items[index].1;
            let is_selected = selected == Some(key);

            let response = match item {
                Item::Set { row } => {
                    let Some(Row::Set(set)) = rows.get(row) else {
                        return;
                    };
                    let (response, arrow) =
                        set_row(ui, set, data, is_selected, self_expanded(items, index));
                    if arrow {
                        toggle = Some(set.key);
                    }
                    response
                }
                Item::Game { index: g, child } => {
                    game_row(ui, &data.games[g], data, is_selected, child)
                }
            };
            if let Some((flashed, strength)) = flash
                && flashed == key
            {
                // The row a jump landed on glows for a moment.
                ui.painter().rect_filled(
                    response.rect.shrink2(vec2(6.0, 2.0)),
                    CornerRadius::same(6),
                    theme::ACCENT.gamma_multiply(0.30 * strength),
                );
            }
            if response.clicked() {
                clicked = Some((key, false));
            }
            if response.double_clicked() {
                clicked = Some((key, true));
            }
            response.context_menu(|ui| context_menu(ui, key, item, rows, data, actions));
        });
        self.visible = (output.first_visible, output.last_visible);
        if let Some(key) = toggle {
            self.toggle(key);
        }
        if let Some((key, double)) = clicked {
            self.selected = Some(key);
            if double {
                match key {
                    Key::Game(id) => actions.push(Action::OpenStats(id)),
                    Key::Set(k) => self.toggle(k),
                }
            }
        }
    }
}

/// Whether the set header at `index` is followed by its games (it's open).
fn self_expanded(items: &[(Key, Item)], index: usize) -> bool {
    matches!(
        items.get(index + 1),
        Some((_, Item::Game { child: true, .. }))
    )
}

fn empty_state(ui: &mut egui::Ui, data: &Data) {
    ui.add_space(80.0);
    ui.vertical_centered(|ui| {
        ui.label(Ion::FILM.rich(40.0).color(theme::FAINT));
        ui.add_space(theme::space::S);
        let (title, body) = if data.games.is_empty() {
            match data.progress.phase {
                crate::library::Phase::Idle => (
                    "No replays yet",
                    "Add your replay folder in Settings. Slippi saves them in Documents\\Slippi.",
                ),
                _ => (
                    "Reading your replays…",
                    "They show up here as they're read.",
                ),
            }
        } else {
            (
                "Nothing matches",
                "Try another search, or loosen the filters.",
            )
        };
        ui.label(theme::heavy(title, theme::text::TITLE));
        ui.label(theme::muted(body));
    });
}

fn context_menu(
    ui: &mut egui::Ui,
    key: Key,
    item: Item,
    rows: &[Row],
    data: &Data,
    actions: &mut Vec<Action>,
) {
    let games: Vec<&Game> = match item {
        Item::Set { row } => match rows.get(row) {
            Some(Row::Set(set)) => set.games.iter().rev().map(|&i| &data.games[i]).collect(),
            _ => Vec::new(),
        },
        Item::Game { index, .. } => vec![&data.games[index]],
    };
    let Some(first) = games.first() else { return };
    let label = if games.len() > 1 { "Play set" } else { "Play" };
    if ui.add(theme::icon_button(Ion::PLAY, label)).clicked() {
        actions.push(Action::Play(
            games.iter().map(|g| Clip::whole(&g.path)).collect(),
        ));
        ui.close();
    }
    if let Key::Game(id) = key
        && ui
            .add(theme::icon_button(Ion::STATS_CHART, "Open stats"))
            .clicked()
    {
        actions.push(Action::OpenStats(id));
        ui.close();
    }
    if let Some(opponent) = first.opponent(&data.me).filter(|p| !p.code.is_empty()) {
        if ui
            .add(theme::icon_button(
                Ion::PERSON,
                &format!("Go to {}", opponent.display_name()),
            ))
            .clicked()
        {
            actions.push(Action::GoToOpponent(opponent.code.clone()));
            ui.close();
        }
        if ui
            .add(theme::icon_button(Ion::COPY, "Copy their code"))
            .clicked()
        {
            actions.push(Action::CopyText(opponent.code.clone()));
            ui.close();
        }
    }
    ui.separator();
    if ui
        .add(theme::icon_button(Ion::FOLDER_OPEN, "Show in Explorer"))
        .clicked()
    {
        actions.push(Action::ShowFile(first.path.clone()));
        ui.close();
    }
    if ui
        .add(theme::icon_button(Ion::COPY, "Copy file path"))
        .clicked()
    {
        actions.push(Action::CopyText(first.path.display().to_string()));
        ui.close();
    }
}

/// Where the columns of a row go at the list's width.
struct Columns {
    left: f32,
    name: f32,
    name_width: f32,
    rank: Option<f32>,
    mode: Option<f32>,
    result: f32,
    stage: Option<f32>,
    time_right: f32,
}

impl Columns {
    /// Lays the columns out from the right: the time and the result always,
    /// then the rank, the mode and the stages while the name keeps room for
    /// itself (they drop out in the reverse order as the list narrows).
    fn new(rect: Rect, indent: f32) -> Self {
        const NAME_MIN: f32 = 170.0;
        const TIME: f32 = 100.0;
        const RESULT: f32 = 84.0;
        const RANK: f32 = 136.0;
        const MODE: f32 = 86.0;
        const STAGE: f32 = 170.0;
        const GAP: f32 = 12.0;
        let left = rect.min.x + 12.0 + indent;
        let name = left + 116.0;
        let time_right = rect.max.x - 14.0;
        let mut room = time_right - TIME - GAP - RESULT - GAP - name - NAME_MIN;
        let mut take = |width: f32| {
            let fits = room >= width + GAP;
            if fits {
                room -= width + GAP;
            }
            fits
        };
        let (has_rank, has_mode, has_stage) = (take(RANK), take(MODE), take(STAGE));
        let mut x = time_right - TIME - GAP;
        let stage = has_stage.then(|| {
            x -= STAGE;
            let at = x;
            x -= GAP;
            at
        });
        x -= RESULT;
        let result = x;
        x -= GAP;
        let mode = has_mode.then(|| {
            x -= MODE;
            let at = x;
            x -= GAP;
            at
        });
        let rank = has_rank.then(|| {
            x -= RANK;
            let at = x;
            x -= GAP;
            at
        });
        Self {
            left,
            name,
            name_width: (x - name).max(60.0),
            rank,
            mode,
            result,
            stage,
            time_right,
        }
    }
}

fn row_background(ui: &egui::Ui, rect: Rect, response: &egui::Response, selected: bool) {
    let painter = ui.painter();
    let inner = rect.shrink2(vec2(6.0, 2.0));
    if selected {
        painter.rect_filled(inner, CornerRadius::same(6), theme::SELECTED_ROW);
    } else if response.hovered() {
        painter.rect_filled(inner, CornerRadius::same(6), theme::HOVERED_ROW);
    }
}

/// "me vs them" stock icons; returns the x after them.
fn paint_matchup(ui: &egui::Ui, game: &Game, data: &Data, x: f32, cy: f32, size: f32) -> f32 {
    let painter = ui.painter();
    let (a, b) = match game.sides(&data.me) {
        Some(s) => (&game.players[s.me], &game.players[s.opponent]),
        None if game.players.len() >= 2 => (&game.players[0], &game.players[1]),
        None => return x,
    };
    let r1 = Rect::from_center_size(pos2(x + size / 2.0, cy), vec2(size, size));
    widgets::paint_stock(ui, r1, a.character, a.costume, Color32::WHITE);
    painter.text(
        pos2(x + size + 13.0, cy),
        egui::Align2::CENTER_CENTER,
        "vs",
        FontId::proportional(theme::text::TINY),
        theme::FAINT,
    );
    let r2 = Rect::from_center_size(pos2(x + size + 26.0 + size / 2.0, cy), vec2(size, size));
    widgets::paint_stock(ui, r2, b.character, b.costume, Color32::WHITE);
    if game.players.len() > 2 {
        painter.text(
            pos2(r2.max.x + 4.0, cy + 8.0),
            egui::Align2::LEFT_CENTER,
            format!("+{}", game.players.len() - 2),
            FontId::proportional(theme::text::TINY),
            theme::MUTED,
        );
    }
    r2.max.x
}

/// Name over code, for the opponent (or both players when the game isn't
/// mine).
fn paint_who(ui: &egui::Ui, game: &Game, data: &Data, columns: &Columns, cy: f32) {
    let painter = ui.painter();
    let (title, subtitle) = match game.opponent(&data.me) {
        Some(opponent) => {
            let name = opponent.display_name();
            let code = if opponent.code.is_empty() || opponent.code == name {
                melee::character_name(opponent.character).to_owned()
            } else {
                format!(
                    "{} · {}",
                    opponent.code,
                    melee::character_short(opponent.character)
                )
            };
            (name, code)
        }
        None => {
            let names: Vec<String> = game.players.iter().map(|p| p.display_name()).collect();
            let chars: Vec<&str> = game
                .players
                .iter()
                .map(|p| melee::character_short(p.character))
                .collect();
            (names.join(" vs "), chars.join(" vs "))
        }
    };
    paint_text_clipped(
        painter,
        pos2(columns.name, cy - 9.0),
        &title,
        theme::heavy_font(theme::text::LIST),
        theme::TEXT,
        columns.name_width,
    );
    paint_text_clipped(
        painter,
        pos2(columns.name, cy + 10.0),
        &subtitle,
        FontId::proportional(theme::text::SMALL),
        theme::MUTED,
        columns.name_width,
    );
}

fn paint_rank(ui: &egui::Ui, game: &Game, data: &Data, x: f32, cy: f32) -> Option<Rect> {
    let opponent = game.opponent(&data.me)?;
    if opponent.code.is_empty() {
        return None;
    }
    let painter = ui.painter();
    match data.rank(&opponent.code) {
        Some(record) => {
            let rank = match (&record.rank, record.found) {
                (Some(rank), _) => display_rank(rank),
                (None, true) => "Unranked".to_owned(),
                (None, false) => return None,
            };
            let icon = Rect::from_center_size(pos2(x + 11.0, cy), vec2(22.0, 22.0));
            widgets::paint_rank(ui, icon, &rank);
            paint_text_clipped(
                painter,
                pos2(x + 28.0, cy),
                &rank,
                FontId::proportional(theme::text::SMALL),
                widgets::rank_color(&rank),
                110.0,
            );
            Some(Rect::from_min_max(
                pos2(x, cy - 11.0),
                pos2(x + 140.0, cy + 11.0),
            ))
        }
        None => {
            painter.text(
                pos2(x + 2.0, cy),
                egui::Align2::LEFT_CENTER,
                "…",
                FontId::proportional(theme::text::SMALL),
                theme::FAINT,
            );
            None
        }
    }
}

fn paint_pill(painter: &egui::Painter, x: f32, cy: f32, text: &str, color: Color32) -> f32 {
    let galley = painter.layout_no_wrap(
        text.to_owned(),
        FontId::proportional(theme::text::TINY),
        color,
    );
    let size = galley.size() + vec2(12.0, 4.0);
    let rect = Rect::from_min_size(pos2(x, cy - size.y / 2.0), size);
    painter.rect_filled(rect, CornerRadius::same(7), color.gamma_multiply(0.18));
    painter.galley(rect.min + vec2(6.0, 2.0), galley, color);
    size.x
}

pub fn mode_color(mode: crate::model::Mode) -> Color32 {
    use crate::model::Mode;
    match mode {
        Mode::Ranked => theme::ACCENT_TEXT,
        Mode::Unranked => theme::MUTED,
        Mode::Direct => theme::hex(0x9FB6C9),
        Mode::Teams => theme::hex(0xB7A3CC),
        Mode::Local | Mode::Other => theme::FAINT,
    }
}

fn paint_time(painter: &egui::Painter, columns: &Columns, cy: f32, top: String, bottom: String) {
    painter.text(
        pos2(columns.time_right, cy - 9.0),
        egui::Align2::RIGHT_CENTER,
        top,
        FontId::proportional(theme::text::SMALL),
        theme::TEXT,
    );
    painter.text(
        pos2(columns.time_right, cy + 10.0),
        egui::Align2::RIGHT_CENTER,
        bottom,
        FontId::proportional(theme::text::SMALL),
        theme::FAINT,
    );
}

/// A set's row; returns its response and whether its arrow was clicked.
pub fn set_row(
    ui: &mut egui::Ui,
    set: &GameSet,
    data: &Data,
    selected: bool,
    open: bool,
) -> (egui::Response, bool) {
    let (rect, response) =
        ui.allocate_exact_size(vec2(ui.available_width(), SET_HEIGHT), Sense::click());
    if !ui.is_rect_visible(rect) {
        return (response, false);
    }
    row_background(ui, rect, &response, selected);
    let first = &data.games[set.games[0]];
    let columns = Columns::new(rect, 0.0);
    let cy = rect.center().y;

    // The arrow that opens the set.
    let arrow_rect = Rect::from_center_size(pos2(columns.left + 8.0, cy), vec2(22.0, 30.0));
    let arrow = ui.interact(arrow_rect, response.id.with("arrow"), Sense::click());
    let icon = if open {
        Ion::CHEVRON_DOWN
    } else {
        Ion::CHEVRON_FORWARD
    };
    icon.paint(
        ui.painter(),
        arrow_rect,
        14.0,
        if arrow.hovered() {
            theme::TEXT
        } else {
            theme::FAINT
        },
    );

    let x = paint_matchup(ui, first, data, columns.left + 24.0, cy, 28.0);
    let _ = x;
    paint_who(ui, first, data, &columns, cy);
    let rank_rect = columns
        .rank
        .and_then(|x| paint_rank(ui, first, data, x, cy));
    let painter = ui.painter();
    if let Some(x) = columns.mode {
        let label = match set.kind {
            SetKind::Ranked => "Ranked",
            SetKind::Session => first.mode.label(),
        };
        paint_pill(painter, x, cy, label, mode_color(first.mode));
    }

    // The score: 2–1 from my side.
    let (mine, theirs) = set_score(&data.games, &set.games, &data.me);
    let mine_known = first.sides(&data.me).is_some();
    let outcome = if !mine_known || mine == theirs {
        Outcome::Unknown
    } else if mine > theirs {
        Outcome::Win
    } else {
        Outcome::Loss
    };
    let mark = Rect::from_center_size(pos2(columns.result + 11.0, cy), vec2(22.0, 22.0));
    if mine_known {
        widgets::paint_outcome(painter, mark, outcome);
        painter.text(
            pos2(mark.max.x + 8.0, cy),
            egui::Align2::LEFT_CENTER,
            format!("{mine}–{theirs}"),
            theme::heavy_font(theme::text::TITLE),
            widgets::outcome_color(outcome),
        );
    } else {
        painter.text(
            pos2(columns.result, cy),
            egui::Align2::LEFT_CENTER,
            widgets::count(set.games.len(), "game"),
            FontId::proportional(theme::text::SMALL),
            theme::MUTED,
        );
    }

    if let Some(x) = columns.stage {
        // One dot per game, its stage on hover is in the details.
        let mut stages: Vec<&str> = set
            .games
            .iter()
            .rev()
            .map(|&i| melee::stage_short(data.games[i].stage))
            .collect();
        stages.dedup();
        paint_text_clipped(
            painter,
            pos2(x, cy),
            &stages.join(" · "),
            FontId::proportional(theme::text::SMALL),
            theme::MUTED,
            columns.time_right - x - 70.0,
        );
    }
    let seconds: f64 = set.games.iter().map(|&i| data.games[i].seconds()).sum();
    paint_time(
        painter,
        &columns,
        cy,
        crate::time::short(first.start_at, data.now),
        format!(
            "{} · {}",
            widgets::count(set.games.len(), "game"),
            crate::time::duration(seconds)
        ),
    );
    if let (Some(rect), Some(opponent)) = (rank_rect, first.opponent(&data.me)) {
        rank_tooltip(ui, rect, response.id, &opponent.code, first.start_at, data);
    }
    let response = response.on_hover_cursor(egui::CursorIcon::PointingHand);
    (response, arrow.clicked())
}

/// Their rank now, and how they finished the season the game was in.
fn rank_tooltip(ui: &egui::Ui, rect: Rect, id: egui::Id, code: &str, at: i64, data: &Data) {
    let hover = ui.interact(rect, id.with("rank"), Sense::hover());
    if let Some(record) = data.rank(code) {
        let mut text = String::from("Their rank now");
        if let Some((season, rank)) = data.past_season_rank(code, at) {
            text.push_str(&format!(
                "\nThis game was in {season}: they finished {rank}"
            ));
        }
        text.push('\n');
        if let Some(rating) = record.rating {
            text.push_str(&format!("Rating {rating:.1}"));
        }
        if let Some((w, l)) = crate::ranks::season_record(record) {
            text.push_str(&format!("\nThis season: {w}–{l} in sets"));
        }
        text.push_str(&format!(
            "\nLooked up {}",
            crate::time::ago(record.fetched_at, data.now)
        ));
        hover.on_hover_text(text.trim());
    }
}

/// A game's row (on its own or under its set).
pub fn game_row(
    ui: &mut egui::Ui,
    game: &Game,
    data: &Data,
    selected: bool,
    child: bool,
) -> egui::Response {
    let height = if child { CHILD_HEIGHT } else { GAME_HEIGHT };
    let (rect, response) =
        ui.allocate_exact_size(vec2(ui.available_width(), height), Sense::click());
    if !ui.is_rect_visible(rect) {
        return response;
    }
    row_background(ui, rect, &response, selected);
    let columns = Columns::new(rect, if child { 24.0 } else { 0.0 });
    let cy = rect.center().y;
    let outcome = game.outcome(&data.me);
    let painter = ui.painter();

    if child {
        // A thin rail joins a set's games to it.
        painter.line_segment(
            [
                pos2(columns.left - 16.0, rect.min.y),
                pos2(columns.left - 16.0, rect.max.y),
            ],
            egui::Stroke::new(1.0, theme::SEPARATOR),
        );
        let label = if game.game_number > 0 {
            format!("Game {}", game.game_number)
        } else {
            "Game".to_owned()
        };
        painter.text(
            pos2(columns.left, cy),
            egui::Align2::LEFT_CENTER,
            label,
            FontId::proportional(theme::text::SMALL),
            theme::MUTED,
        );
        // Stocks left, both sides.
        let mut x = columns.left + 64.0;
        if let Some(sides) = game.sides(&data.me).or(if game.players.len() == 2 {
            Some(crate::model::Sides { me: 0, opponent: 1 })
        } else {
            None
        }) {
            for (i, side) in [sides.me, sides.opponent].into_iter().enumerate() {
                let p = &game.players[side];
                let left = game.stocks_left(p.port);
                let total = 4;
                let w = match left {
                    Some(left) => widgets::paint_stocks(
                        ui,
                        pos2(x, cy - 8.0),
                        p.character,
                        p.costume,
                        left,
                        total,
                        16.0,
                    ),
                    None => {
                        let r = Rect::from_center_size(pos2(x + 9.0, cy), vec2(18.0, 18.0));
                        widgets::paint_stock(ui, r, p.character, p.costume, Color32::WHITE);
                        20.0
                    }
                };
                x += w + 4.0;
                if i == 0 {
                    ui.painter().text(
                        pos2(x + 6.0, cy),
                        egui::Align2::CENTER_CENTER,
                        "vs",
                        FontId::proportional(theme::text::TINY),
                        theme::FAINT,
                    );
                    x += 16.0;
                }
            }
        }
    } else {
        paint_matchup(ui, game, data, columns.left + 24.0, cy, 24.0);
        let mark = Rect::from_center_size(pos2(columns.left + 8.0, cy), vec2(18.0, 18.0));
        if game.sides(&data.me).is_some() {
            widgets::paint_outcome(ui.painter(), mark, outcome);
        }
        paint_who(ui, game, data, &columns, cy);
        if let Some(x) = columns.rank
            && let Some(rect) = paint_rank(ui, game, data, x, cy)
            && let Some(opponent) = game.opponent(&data.me)
        {
            rank_tooltip(ui, rect, response.id, &opponent.code, game.start_at, data);
        }
        if let Some(x) = columns.mode {
            paint_pill(
                ui.painter(),
                x,
                cy,
                game.mode.label(),
                mode_color(game.mode),
            );
        }
    }

    let painter = ui.painter();
    if child {
        let mark = Rect::from_center_size(pos2(columns.result + 11.0, cy), vec2(20.0, 20.0));
        widgets::paint_outcome(painter, mark, outcome);
    } else if let Some(sides) = game.sides(&data.me) {
        // Stocks left for the winner: "3 stocks".
        let left = game.stocks_left.iter().map(|(_, s)| *s).max().unwrap_or(0);
        let text = if game.stocks_left.is_empty() {
            String::new()
        } else if outcome == Outcome::Win {
            format!("{} stock{} left", left, if left == 1 { "" } else { "s" })
        } else {
            let theirs = game
                .stocks_left(game.players[sides.opponent].port)
                .unwrap_or(0);
            format!(
                "{} stock{} left",
                theirs,
                if theirs == 1 { "" } else { "s" }
            )
        };
        painter.text(
            pos2(columns.result, cy),
            egui::Align2::LEFT_CENTER,
            text,
            FontId::proportional(theme::text::SMALL),
            widgets::outcome_color(outcome),
        );
    }
    if let Some(x) = columns.stage.or(if child {
        Some(columns.result + 36.0)
    } else {
        None
    }) {
        paint_text_clipped(
            painter,
            pos2(x, cy),
            melee::stage_name(game.stage),
            FontId::proportional(theme::text::SMALL),
            theme::MUTED,
            (columns.time_right - x - 70.0).max(40.0),
        );
    }
    if child {
        painter.text(
            pos2(columns.time_right, cy),
            egui::Align2::RIGHT_CENTER,
            crate::time::duration(game.seconds()),
            FontId::proportional(theme::text::SMALL),
            theme::MUTED,
        );
    } else {
        paint_time(
            painter,
            &columns,
            cy,
            crate::time::short(game.start_at, data.now),
            crate::time::duration(game.seconds()),
        );
    }
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}
