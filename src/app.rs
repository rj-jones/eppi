//! The window: header, navigation, the current view, the details pane and
//! the status bar; the library and rank workers' events; keyboard and
//! actions.

use crate::dolphin::{self, Clip};
use crate::library::db::{self, RankRecord};
use crate::library::{self, Library, Phase, Progress};
use crate::model::{self, Filter, Game, Me, Outcome, Row};
use crate::ranks::RankService;
use crate::settings::Settings;
use crate::ui::details;
use crate::ui::theme::{self, Ion};
use crate::ui::widgets;
use crate::ui::{filter_bar, me, opponents, replay_list, settings_view, stats_page, tonight};
use eframe::egui::{self, Key as K, Margin, RichText, vec2};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum View {
    #[default]
    Tonight,
    Replays,
    Opponents,
    Me,
    Highlights,
    Settings,
}

impl View {
    pub const NAV: [View; 5] = [
        View::Tonight,
        View::Replays,
        View::Opponents,
        View::Me,
        View::Highlights,
    ];

    pub fn label(self) -> &'static str {
        match self {
            View::Tonight => "Tonight",
            View::Replays => "Replays",
            View::Opponents => "Opponents",
            View::Me => "Me",
            View::Highlights => "Highlights",
            View::Settings => "Settings",
        }
    }

    pub fn icon(self) -> Ion {
        match self {
            View::Tonight => Ion::MOON,
            View::Replays => Ion::FILM,
            View::Opponents => Ion::PEOPLE,
            View::Me => Ion::PERSON_CIRCLE,
            View::Highlights => Ion::FLASH,
            View::Settings => Ion::SETTINGS,
        }
    }

    fn uses_filter(self) -> bool {
        matches!(
            self,
            View::Replays | View::Opponents | View::Me | View::Highlights
        )
    }
}

/// What the window remembers between runs.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct Prefs {
    pub view: View,
    pub filter: Filter,
    pub opponent_sort: opponents::Sort,
}

/// Everything the views read.
pub struct Data {
    /// Newest first.
    pub games: Vec<Game>,
    pub index: HashMap<i64, usize>,
    /// Bumped whenever `games` changes.
    pub revision: u64,
    pub me: Me,
    pub my_codes: Vec<String>,
    pub ranks: HashMap<String, RankRecord>,
    pub notes: HashMap<String, String>,
    /// Games being played right now.
    pub live: Vec<Game>,
    pub now: i64,
    pub progress: Progress,
    /// Each folder read and how many replay files it holds.
    pub folder_counts: Vec<(PathBuf, usize)>,
    /// The ranked seasons, newest first, with how I finished each.
    pub seasons: Vec<model::Season>,
    /// Bumped whenever a rank arrives.
    pub ranks_revision: u64,
    /// Each player's rank (its name) in each season, by season id, as
    /// fetched, and when; for the rank filter.
    pub histories: HashMap<String, (HashMap<String, String>, i64)>,
    /// Rank lookups asked for and not answered yet.
    pub ranks_pending: usize,
    /// When each player's latest game was (a lookup stays fresh longer for
    /// someone not played in a while).
    pub last_played: HashMap<String, i64>,
}

impl Data {
    pub fn game(&self, id: i64) -> Option<&Game> {
        self.index.get(&id).map(|&i| &self.games[i])
    }

    pub fn rank(&self, code: &str) -> Option<&RankRecord> {
        self.ranks.get(code)
    }

    /// `code`'s rank tier when `at` was: their finish in the season that ran
    /// then (their rank now in the current season); `None` when unknown
    /// (not looked up yet, no account, or before ranked existed).
    pub fn tier_at(&self, code: &str, at: i64) -> Option<model::RankTier> {
        let season = self
            .seasons
            .iter()
            .find(|s| at >= s.start && s.end.is_none_or(|end| at < end))?;
        if season.is_current()
            && let Some(record) = self.ranks.get(code)
        {
            return record
                .found
                .then(|| model::RankTier::of(record.rank.as_deref().unwrap_or("")));
        }
        let (ranks, _) = self.histories.get(code)?;
        if ranks.is_empty() {
            return None; // no account
        }
        Some(
            ranks
                .get(&season.id)
                .map_or(model::RankTier::Unranked, |rank| model::RankTier::of(rank)),
        )
    }

    /// How `code` finished the past season that was running at `at`:
    /// (the season's name, their rank), when their seasons were fetched.
    pub fn past_season_rank(&self, code: &str, at: i64) -> Option<(&str, String)> {
        let season = self
            .seasons
            .iter()
            .find(|s| !s.is_current() && at >= s.start && s.end.is_some_and(|end| at < end))?;
        let (ranks, _) = self.histories.get(code)?;
        if ranks.is_empty() {
            return None;
        }
        let rank = ranks
            .get(&season.id)
            .map_or("Unranked".to_owned(), |r| display_rank(r));
        Some((&season.name, rank))
    }

    fn rebuild_index(&mut self) {
        self.games
            .sort_by(|a, b| b.start_at.cmp(&a.start_at).then(b.id.cmp(&a.id)));
        self.index = self
            .games
            .iter()
            .enumerate()
            .map(|(i, g)| (g.id, i))
            .collect();
        // Newest first, so the first game seen with a player is the latest.
        self.last_played.clear();
        for game in &self.games {
            for player in &game.players {
                if !player.code.is_empty() {
                    self.last_played
                        .entry(player.code.clone())
                        .or_insert(game.start_at);
                }
            }
        }
        self.revision += 1;
    }
}

/// The filtered games and what is built from them, recomputed only when
/// the games, the filter or my codes change.
#[derive(Default)]
pub struct Derived {
    key: Option<(u64, u64, u64)>,
    /// Indexes into `Data::games`, newest first.
    pub order: Vec<usize>,
    pub rows: Vec<Row>,
    pub opponents: Vec<opponents::Summary>,
    /// Bumped with every recompute (for views that cache on it).
    pub revision: u64,
}

impl Derived {
    fn update(&mut self, data: &Data, filter: &Filter) {
        let key = (
            // New ranks only matter while the filter goes by rank.
            data.revision
                ^ if filter.their_ranks.is_empty() {
                    0
                } else {
                    data.ranks_revision << 40
                },
            model::hash_str(&format!("{filter:?}")),
            model::hash_str(&format!("{:?}", data.my_codes)),
        );
        if self.key == Some(key) {
            return;
        }
        self.key = Some(key);
        self.revision += 1;
        let now = data.now;
        self.order = data
            .games
            .iter()
            .enumerate()
            .filter(|(_, g)| filter.matches(g, &data.me, now, |code, at| data.tier_at(code, at)))
            .map(|(i, _)| i)
            .collect();
        self.rows = model::group_rows(&data.games, &self.order);
        self.opponents = opponents::summarize(&data.games, &self.order, &data.me);
    }
}

/// Whether the Filters menu is open, and where its button and the menu
/// were last drawn (a click outside both closes it).
#[derive(Default)]
struct FiltersMenu {
    open: bool,
    button: Option<egui::Rect>,
    area: Option<egui::Rect>,
}

/// What a view asks the app to do.
#[derive(Debug, Clone)]
pub enum Action {
    Play(Vec<Clip>),
    OpenStats(i64),
    CloseStats,
    ShowFile(PathBuf),
    CopyText(String),
    /// Show the opponent in the Opponents view.
    GoToOpponent(String),
    /// Show the game in the Replays view.
    GoToGame(i64),
    FetchRank {
        code: String,
        force: bool,
    },
    SaveNote {
        code: String,
        text: String,
    },
    SetView(View),
    SettingsChanged,
    Rescan,
    /// The screenshot tour: show a game as if it had just started (or
    /// stop showing it).
    DemoLive(Option<Box<Game>>),
    /// The screenshot tour: type `String` into the search bar.
    DemoSearch(String),
    /// The screenshot tour: show this tab of the open stats page.
    DemoStatsTab(&'static str),
    /// The screenshot tour: open or close the Filters menu.
    DemoFiltersMenu(bool),
}

struct Toast {
    text: String,
    error: bool,
    since: Instant,
}

pub struct EppiApp {
    pub settings: Settings,
    pub prefs: Prefs,
    library: Library,
    /// The UI's own connection, for stats, notes and rank history.
    conn: Option<rusqlite::Connection>,
    pub data: Data,
    derived: Derived,
    ranks: RankService,
    dolphin: dolphin::Player,
    replays: replay_list::ListState,
    tonight: tonight::State,
    opponents: opponents::State,
    me: me::State,
    settings_view: settings_view::State,
    stats: Option<stats_page::Page>,
    highlights: crate::ui::highlights::State,
    toasts: Vec<Toast>,
    search: crate::ui::search::Search,
    /// Screenshot mode: (folder, views still to capture, frames to wait).
    screenshots: Option<crate::screenshot::Tour>,
    /// The Filters button's menu.
    filters_menu: FiltersMenu,
    /// Demo mode (`--showcase`): other players made anonymous.
    demo: bool,
    /// What the rank filter's last look-up plan was made from, so it's
    /// only made again when that changes.
    rank_plan_key: Option<(u64, u64, u64, usize)>,
}

/// The navigation column's width.
const NAV_WIDTH: f32 = 196.0;

const SETTINGS_KEY: &str = "settings";
const PREFS_KEY: &str = "prefs";

impl EppiApp {
    pub fn new(cc: &eframe::CreationContext<'_>, tour: Option<crate::screenshot::Tour>) -> Self {
        theme::install(&cc.egui_ctx);
        let demo = tour.as_ref().is_some_and(|t| t.demo);
        if let Some(tour) = &tour {
            cc.egui_ctx.set_zoom_factor(tour.scale);
        }
        egui_extras::install_image_loaders(&cc.egui_ctx);

        let mut settings: Settings = cc
            .storage
            .and_then(|s| eframe::get_value(s, SETTINGS_KEY))
            .unwrap_or_default();
        settings.seed_from_launcher();
        settings.folders = crate::folders::tidy(&settings.folders);
        let mut prefs: Prefs = cc
            .storage
            .and_then(|s| eframe::get_value(s, PREFS_KEY))
            .unwrap_or_default();
        prefs.filter.search.clear();

        let db_path = crate::settings::data_dir().join("eppi.sqlite");
        // Open (and migrate) the index before the worker opens it too.
        let conn = db::open(&db_path)
            .map_err(|e| log::error!("open {}: {e}", db_path.display()))
            .ok();
        let ctx = cc.egui_ctx.clone();
        let library = Library::start(db_path.clone(), settings.folders.clone(), move || {
            ctx.request_repaint()
        });
        let ranks_known = conn
            .as_ref()
            .and_then(|c| db::latest_ranks(c).ok())
            .unwrap_or_default();
        let notes = conn
            .as_ref()
            .and_then(|c| db::load_notes(c).ok())
            .unwrap_or_default();
        let ctx = cc.egui_ctx.clone();
        let ranks = RankService::start(move || ctx.request_repaint());

        let my_codes: Vec<String> = settings
            .codes
            .iter()
            .map(|c| model::normalize_code(c))
            .collect();
        let mut app = Self {
            data: Data {
                games: Vec::new(),
                index: HashMap::new(),
                revision: 0,
                me: Me::new(&my_codes),
                my_codes,
                ranks: ranks_known,
                notes,
                live: Vec::new(),
                now: crate::time::now(),
                progress: Progress::default(),
                folder_counts: Vec::new(),
                seasons: Vec::new(),
                ranks_revision: 0,
                ranks_pending: 0,
                histories: HashMap::new(),
                last_played: HashMap::new(),
            },
            settings,
            prefs,
            library,
            conn,
            derived: Derived::default(),
            ranks,
            dolphin: dolphin::Player::default(),
            replays: replay_list::ListState::default(),
            tonight: tonight::State::default(),
            opponents: opponents::State::default(),
            me: me::State::default(),
            settings_view: settings_view::State::default(),
            stats: None,
            highlights: Default::default(),
            toasts: Vec::new(),
            search: Default::default(),
            screenshots: tour,
            demo,
            filters_menu: FiltersMenu::default(),
            rank_plan_key: None,
        };
        app.load_seasons();
        app.load_histories();
        if app.demo {
            // Nobody else's name, code or notes in the pictures, and no
            // lookups under made-up codes.
            app.settings.fetch_ranks = false;
            app.settings.notify_live = false;
            app.data.notes.clear();
            let me = app.data.me.clone();
            app.data.ranks = crate::demo::ranks(std::mem::take(&mut app.data.ranks), &me);
            app.data.histories = crate::demo::keys(std::mem::take(&mut app.data.histories), &me);
        }
        if app.settings.fetch_ranks {
            let codes = app.data.my_codes.clone();
            app.ranks.want(
                codes.iter().map(String::as_str),
                &app.data.ranks,
                &app.data.last_played,
                app.data.now,
                false,
            );
        }
        app
    }

    /// Everyone's stored seasons, for the rank filter.
    fn load_histories(&mut self) {
        let Some(conn) = &self.conn else { return };
        self.data.histories = db::load_histories(conn)
            .unwrap_or_default()
            .into_iter()
            .map(|(code, (seasons, at))| (code, (tiers(&seasons), at)))
            .collect();
    }

    /// The stored seasons for my code, and a fresh look once a day.
    fn load_seasons(&mut self) {
        let Some(code) = self.data.my_codes.first().cloned() else {
            self.data.seasons.clear();
            return;
        };
        let (seasons, fetched_at) = self
            .conn
            .as_ref()
            .and_then(|c| db::load_seasons(c, &code).ok())
            .unwrap_or_default();
        self.data.seasons = seasons;
        if self.settings.fetch_ranks && self.data.now - fetched_at > crate::ranks::FRESH_FOR {
            self.ranks.want_seasons(&code);
        }
    }

    fn toast(&mut self, text: impl Into<String>, error: bool) {
        self.toasts.push(Toast {
            text: text.into(),
            error,
            since: Instant::now(),
        });
    }

    fn handle_library_events(&mut self, ctx: &egui::Context) {
        let mut changed = false;
        while let Ok(event) = self.library.events.try_recv() {
            match event {
                library::Event::Loaded(mut games) => {
                    if self.demo {
                        games
                            .iter_mut()
                            .for_each(|g| crate::demo::game(g, &self.data.me));
                    }
                    self.data.games = games;
                    changed = true;
                }
                library::Event::Upserted(games) => {
                    for mut game in games {
                        if self.demo {
                            crate::demo::game(&mut game, &self.data.me);
                        }
                        match self.data.index.get(&game.id) {
                            Some(&i) => self.data.games[i] = game,
                            None => {
                                self.data.index.insert(game.id, self.data.games.len());
                                self.data.games.push(game);
                            }
                        }
                    }
                    changed = true;
                }
                library::Event::Removed(ids) => {
                    let ids: HashSet<i64> = ids.into_iter().collect();
                    self.data.games.retain(|g| !ids.contains(&g.id));
                    changed = true;
                }
                library::Event::Stats(updates) => {
                    for update in updates {
                        if let Some(&i) = self.data.index.get(&update.id) {
                            let game = &mut self.data.games[i];
                            if !update.winners.is_empty() || game.winners.is_empty() {
                                game.winners = update.winners;
                            }
                            game.stocks_left = update.stocks_left;
                            game.numbers = update.numbers;
                            game.stats_version = update.version;
                        }
                    }
                    // Winners can change: the order doesn't, so the index
                    // stays; the derived rows must be rebuilt.
                    self.data.revision += 1;
                }
                library::Event::Progress(progress) => self.data.progress = progress,
                library::Event::LiveStarted(mut game) => {
                    if self.demo {
                        crate::demo::game(&mut game, &self.data.me);
                    }
                    let opponent = game.opponent(&self.data.me).cloned();
                    if let Some(opponent) = &opponent
                        && self.settings.fetch_ranks
                    {
                        self.ranks.want(
                            [opponent.code.as_str()],
                            &self.data.ranks,
                            &self.data.last_played,
                            self.data.now,
                            false,
                        );
                    }
                    let focused = ctx.input(|i| i.viewport().focused.unwrap_or(true));
                    if !focused && self.settings.notify_live {
                        ctx.send_viewport_cmd(egui::ViewportCommand::RequestUserAttention(
                            egui::UserAttentionType::Informational,
                        ));
                        if let Some(opponent) = &opponent {
                            crate::notify::game_started(&game, opponent, &self.data);
                        }
                    }
                    self.data.live.retain(|g| g.path != game.path);
                    self.data.live.push(game);
                }
                library::Event::Folders(counts) => self.data.folder_counts = counts,
                library::Event::LiveEnded(path) => {
                    self.data.live.retain(|g| g.path != path);
                }
                library::Event::Error(error) => self.toast(error, true),
            }
        }
        if changed {
            self.data.rebuild_index();
        }
    }

    fn handle_rank_results(&mut self) {
        while let Ok(reply) = self.ranks.results.try_recv() {
            let result = match reply {
                crate::ranks::Reply::Ranks(result) => result,
                crate::ranks::Reply::Dropped(codes) => {
                    self.ranks.dropped(&codes);
                    continue;
                }
                crate::ranks::Reply::Histories(result) => {
                    match result {
                        Ok(found) => {
                            let now = self.data.now;
                            self.ranks
                                .histories_answered(found.iter().map(|(r, _)| r.code.clone()));
                            for (record, seasons) in found {
                                let code = record.code.clone();
                                if let Some(conn) = &self.conn {
                                    let _ = match &seasons {
                                        Some(list) if !list.is_empty() => {
                                            db::save_seasons(conn, &code, list, now)
                                        }
                                        _ => db::save_no_account(conn, &code, now),
                                    };
                                    // The same request brought their rank now.
                                    if let Err(e) = db::save_rank(conn, &record) {
                                        log::warn!("save rank: {e}");
                                    }
                                }
                                self.ranks.answered(std::slice::from_ref(&record));
                                self.data.ranks.insert(code.clone(), record);
                                self.data.histories.insert(
                                    code,
                                    (tiers(seasons.as_deref().unwrap_or_default()), now),
                                );
                            }
                            self.data.ranks_revision += 1;
                        }
                        Err(error) => {
                            self.ranks.histories_failed();
                            log::warn!("seasons of opponents: {error}");
                        }
                    }
                    continue;
                }
                crate::ranks::Reply::Seasons { code, result } => {
                    match result {
                        Ok(seasons) if !seasons.is_empty() => {
                            if let Some(conn) = &self.conn {
                                let _ = db::save_seasons(conn, &code, &seasons, self.data.now);
                            }
                            if self.data.my_codes.first() == Some(&code) {
                                self.data.seasons = seasons;
                            }
                        }
                        Ok(_) => {}
                        Err(error) => log::warn!("seasons: {error}"),
                    }
                    continue;
                }
            };
            match result {
                Ok(records) => {
                    self.ranks.answered(&records);
                    for record in records {
                        if let Some(conn) = &self.conn
                            && let Err(e) = db::save_rank(conn, &record)
                        {
                            log::warn!("save rank: {e}");
                        }
                        self.data.ranks.insert(record.code.clone(), record);
                    }
                    self.data.ranks_revision += 1;
                }
                Err(error) => {
                    self.ranks.failed();
                    log::warn!("rank lookup: {error}");
                }
            }
        }
    }

    fn apply(&mut self, ctx: &egui::Context, actions: Vec<Action>) {
        for action in actions {
            match action {
                Action::Play(clips) => {
                    let result = self.dolphin.play(
                        self.settings.dolphin_path(),
                        self.settings.iso_path(),
                        &clips,
                    );
                    match result {
                        Ok(()) if clips.len() > 1 => {
                            self.toast(format!("Playing {} replays in Dolphin", clips.len()), false)
                        }
                        Ok(()) => self.toast("Playing in Dolphin", false),
                        Err(error) => self.toast(error, true),
                    }
                }
                Action::OpenStats(id) => {
                    if let Some(game) = self.data.game(id).cloned() {
                        let json = self
                            .conn
                            .as_ref()
                            .and_then(|c| db::load_stats_json(c, id).ok().flatten());
                        self.stats = Some(stats_page::Page::new(game, json));
                    }
                }
                Action::CloseStats => self.stats = None,
                Action::ShowFile(path) => show_in_folder(&path),
                Action::CopyText(text) => {
                    ctx.copy_text(text);
                    self.toast("Copied", false);
                }
                Action::GoToOpponent(code) => {
                    self.stats = None;
                    self.prefs.view = View::Opponents;
                    self.opponents.select(&code);
                }
                Action::GoToGame(id) => {
                    self.stats = None;
                    self.prefs.view = View::Replays;
                    self.replays.reveal_game(id, &self.data, &self.derived);
                }
                Action::FetchRank { code, force } => {
                    self.ranks.want(
                        [code.as_str()],
                        &self.data.ranks,
                        &self.data.last_played,
                        self.data.now,
                        force,
                    );
                }
                Action::SaveNote { code, text } => {
                    if let Some(conn) = &self.conn {
                        let _ = db::save_note(conn, &code, &text, self.data.now);
                    }
                    if text.trim().is_empty() {
                        self.data.notes.remove(&code);
                    } else {
                        self.data.notes.insert(code, text);
                    }
                }
                Action::SetView(view) => {
                    self.stats = None;
                    self.prefs.view = view;
                }
                Action::SettingsChanged => {
                    self.data.my_codes = self
                        .settings
                        .codes
                        .iter()
                        .map(|c| model::normalize_code(c))
                        .collect();
                    self.data.me = Me::new(&self.data.my_codes);
                    self.data.revision += 1;
                    self.load_seasons();
                    self.library.set_folders(self.settings.folders.clone());
                }
                Action::DemoLive(game) => {
                    self.data.live = game.map(|g| vec![*g]).unwrap_or_default()
                }
                Action::DemoFiltersMenu(open) => self.filters_menu.open = open,
                Action::DemoStatsTab(tab) => {
                    if let Some(page) = &mut self.stats {
                        page.show_tab(tab);
                    }
                }
                Action::DemoSearch(text) => {
                    self.prefs.filter.search = text;
                    self.search.focus = true;
                }
                Action::Rescan => {
                    let _ = self
                        .library
                        .commands
                        .send(library::Command::Scan(self.settings.folders.clone()));
                }
            }
        }
    }

    fn keyboard(&mut self, ctx: &egui::Context, actions: &mut Vec<Action>) {
        let typing = ctx.egui_wants_keyboard_input();
        ctx.input_mut(|input| {
            if input.consume_key(egui::Modifiers::COMMAND, K::K)
                || (!typing && input.consume_key(egui::Modifiers::NONE, K::Slash))
            {
                self.search.focus = true;
            }
            if input.consume_key(egui::Modifiers::COMMAND, K::R)
                || input.consume_key(egui::Modifiers::NONE, K::F5)
            {
                actions.push(Action::Rescan);
            }
            for (i, view) in View::NAV.iter().enumerate() {
                let key = [K::Num1, K::Num2, K::Num3, K::Num4, K::Num5][i];
                if input.consume_key(egui::Modifiers::COMMAND, key) {
                    actions.push(Action::SetView(*view));
                }
            }
            if input.consume_key(egui::Modifiers::COMMAND, K::Comma) {
                actions.push(Action::SetView(View::Settings));
            }
        });
        if typing {
            if ctx.input(|i| i.key_pressed(K::Escape)) {
                ctx.memory_mut(|m| m.surrender_focus(egui::Id::unique("search")));
            }
            return;
        }
        if self.stats.is_some() {
            if ctx.input(|i| i.key_pressed(K::Escape) || i.key_pressed(K::Backspace)) {
                actions.push(Action::CloseStats);
            }
            return;
        }
        if ctx.input(|i| i.key_pressed(K::Escape)) && !self.prefs.filter.search.is_empty() {
            self.prefs.filter.search.clear();
        }
        let nav = replay_list::Nav::from_input(ctx);
        if nav.is_none() {
            return;
        }
        match self.prefs.view {
            View::Replays => self
                .replays
                .navigate(nav, &self.data, &self.derived, actions),
            View::Tonight => self.tonight.navigate(nav, &self.data, actions),
            View::Opponents => self
                .opponents
                .navigate(nav, &self.data, &self.derived, actions),
            View::Highlights => self.highlights.navigate(nav, actions),
            _ => {}
        }
    }

    fn header(&mut self, ui: &mut egui::Ui, actions: &mut Vec<Action>) {
        egui::Panel::top("header")
            .exact_size(56.0)
            .show_separator_line(false)
            .frame(
                egui::Frame::new()
                    .fill(theme::STRIP)
                    .inner_margin(Margin::symmetric(theme::space::L as i8, 0)),
            )
            .show(ui, |ui| {
                ui.horizontal_centered(|ui| {
                    ui.label(
                        theme::heavy("Eppi", theme::text::HEADING + 2.0).color(theme::ACCENT_TEXT),
                    );
                    ui.add_space(theme::space::M);
                    let view = self.prefs.view;
                    let title = if self.stats.is_some() {
                        "Game stats"
                    } else {
                        view.label()
                    };
                    ui.label(theme::heavy(title, theme::text::TITLE));
                    // The search bar lines up with the content column under
                    // it (past the nav, at the views' own margin).
                    let content_left = NAV_WIDTH + theme::space::L;
                    ui.add_space((content_left - ui.cursor().min.x).max(theme::space::L));

                    if view != View::Settings {
                        self.search
                            .show(ui, &mut self.prefs.filter.search, &self.data, actions);
                        if view.uses_filter() {
                            let count = self.prefs.filter.active_count();
                            let label = if count > 0 {
                                format!("Filters · {count}")
                            } else {
                                "Filters".to_owned()
                            };
                            let button = egui::Button::new(theme::icon_label(
                                Ion::FUNNEL,
                                &label,
                                // On the orange of an open menu, white reads;
                                // closed, orange says filters are on.
                                if self.filters_menu.open {
                                    egui::Color32::WHITE
                                } else if count > 0 {
                                    theme::ACCENT_TEXT
                                } else {
                                    theme::TEXT
                                },
                            ))
                            .selected(self.filters_menu.open);
                            let response = ui
                                .add(button)
                                .on_hover_text("Modes, opponents' ranks and which games to hide");
                            if response.clicked() {
                                self.filters_menu.open = !self.filters_menu.open;
                            }
                            self.filters_menu.button = Some(response.rect);
                        }
                    }

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui
                            .add(egui::Button::new(Ion::REFRESH.rich(17.0)).frame(false))
                            .on_hover_text("Look for new replays (F5)")
                            .clicked()
                        {
                            actions.push(Action::Rescan);
                        }
                        self.progress_label(ui);
                        ui.add_space(theme::space::S);
                        self.my_rank(ui, actions);
                    });
                });
            });
    }

    fn progress_label(&self, ui: &mut egui::Ui) {
        let p = self.data.progress;
        let text = match p.phase {
            Phase::Idle => None,
            Phase::Listing => Some("Looking for replays…".to_owned()),
            Phase::Reading => Some(format!(
                "Reading {} of {}",
                widgets::thousands(p.done),
                widgets::thousands(p.total)
            )),
            Phase::Stats => Some(format!(
                "Stats {}%",
                (100 * p.done / p.total.max(1)).min(99)
            )),
        };
        if let Some(text) = text {
            ui.label(theme::muted(text).size(theme::text::SMALL));
            ui.add(egui::Spinner::new().size(14.0).color(theme::MUTED));
        }
    }

    fn my_rank(&mut self, ui: &mut egui::Ui, actions: &mut Vec<Action>) {
        let Some(code) = self.data.my_codes.first().cloned() else {
            if ui.link("Set your connect code").clicked() {
                actions.push(Action::SetView(View::Settings));
            }
            return;
        };
        let record = self.data.rank(&code).cloned();
        let response = ui
            .scope(|ui| {
                ui.spacing_mut().item_spacing.x = 6.0;
                if let Some(rating) = record.as_ref().and_then(|r| r.rating) {
                    ui.label(theme::muted(format!("{rating:.0}")).size(theme::text::SMALL));
                }
                let rank = record
                    .as_ref()
                    .and_then(|r| r.rank.clone())
                    .unwrap_or_default();
                let rank = display_rank(&rank);
                ui.label(RichText::new(&rank).color(widgets::rank_color(&rank)));
                if let Some(source) = widgets::rank_source(&rank) {
                    ui.add(egui::Image::new(source).fit_to_exact_size(vec2(24.0, 24.0)));
                }
                ui.label(theme::heavy(&code, theme::text::BODY));
            })
            .response;
        let tip = match &record {
            Some(r) => format!(
                "Your ranked profile, looked up {}. Click to look again.",
                crate::time::ago(r.fetched_at, self.data.now)
            ),
            None => "Your rank: click to look it up.".to_owned(),
        };
        if response
            .interact(egui::Sense::click())
            .on_hover_text(tip)
            .clicked()
        {
            actions.push(Action::FetchRank { code, force: true });
        }
    }

    fn nav(&mut self, ui: &mut egui::Ui, actions: &mut Vec<Action>) {
        egui::Panel::left("nav")
            .exact_size(NAV_WIDTH)
            .show_separator_line(false)
            .frame(
                egui::Frame::new()
                    .fill(theme::STRIP)
                    .inner_margin(Margin::symmetric(
                        theme::space::S as i8,
                        theme::space::S as i8,
                    )),
            )
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 2.0;
                for (i, view) in View::NAV.iter().enumerate() {
                    let count = match view {
                        View::Replays => Some(widgets::thousands(self.derived.order.len())),
                        View::Opponents => Some(widgets::thousands(self.derived.opponents.len())),
                        View::Tonight if !self.data.live.is_empty() => Some("LIVE".to_owned()),
                        _ => None,
                    };
                    let selected = self.prefs.view == *view && self.stats.is_none();
                    let response =
                        widgets::nav_item(ui, view.icon(), view.label(), count, selected)
                            .on_hover_text(format!("Ctrl+{}", i + 1));
                    if response.clicked() {
                        actions.push(Action::SetView(*view));
                    }
                }
                ui.with_layout(egui::Layout::bottom_up(egui::Align::Min), |ui| {
                    let selected = self.prefs.view == View::Settings && self.stats.is_none();
                    if widgets::nav_item(ui, Ion::SETTINGS, "Settings", None, selected).clicked() {
                        actions.push(Action::SetView(View::Settings));
                    }
                    ui.add_space(theme::space::S);
                    if let Some(game) = self.data.live.last() {
                        tonight::live_mini(ui, game, &self.data, actions);
                    }
                });
            });
    }

    fn status_bar(&mut self, ui: &mut egui::Ui, actions: &mut Vec<Action>) {
        egui::Panel::bottom("status")
            .exact_size(40.0)
            .show_separator_line(false)
            .frame(
                egui::Frame::new()
                    .fill(theme::STRIP)
                    .inner_margin(Margin::symmetric(theme::space::L as i8, 0)),
            )
            .show(ui, |ui| {
                ui.horizontal_centered(|ui| {
                    let selection = self.current_games();
                    let text = match selection.len() {
                        0 => {
                            let p = self.data.progress;
                            if p.unreadable > 0 && p.phase == Phase::Idle {
                                format!(
                                    "{} games · {} files couldn't be read",
                                    widgets::thousands(self.data.games.len()),
                                    p.unreadable
                                )
                            } else {
                                format!("{} games", widgets::thousands(self.data.games.len()))
                            }
                        }
                        1 => "1 game selected".to_owned(),
                        n => format!("{n} games selected"),
                    };
                    ui.label(theme::muted(text));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if !selection.is_empty() {
                            let label = if selection.len() > 1 {
                                "Play all"
                            } else {
                                "Play"
                            };
                            if ui
                                .add(theme::primary_icon_button(Ion::PLAY, label))
                                .on_hover_text("Watch in Slippi Dolphin (Space)")
                                .clicked()
                            {
                                let clips = selection
                                    .iter()
                                    .rev()
                                    .filter_map(|id| self.data.game(*id))
                                    .map(|g| Clip::whole(&g.path))
                                    .collect();
                                actions.push(Action::Play(clips));
                            }
                        }
                        ui.label(
                            theme::muted(
                                "Arrows move  ·  Enter opens  ·  Space plays  ·  / searches",
                            )
                            .size(theme::text::SMALL)
                            .color(theme::FAINT),
                        );
                    });
                });
            });
    }

    /// The games the status bar's Play acts on, newest first.
    fn current_games(&self) -> Vec<i64> {
        if let Some(page) = &self.stats {
            return vec![page.game.id];
        }
        match self.prefs.view {
            View::Replays => self.replays.selected_games(&self.data, &self.derived),
            View::Tonight => self.tonight.selected_games(&self.data),
            _ => Vec::new(),
        }
    }

    fn details(&mut self, ui: &mut egui::Ui, actions: &mut Vec<Action>) {
        let show = self.stats.is_none()
            && matches!(
                self.prefs.view,
                View::Replays | View::Tonight | View::Opponents
            );
        if !show {
            return;
        }
        egui::Panel::right("details")
            .resizable(true)
            .default_size(380.0)
            .size_range(300.0..=560.0)
            .frame(
                egui::Frame::new()
                    .fill(theme::SIDEBAR)
                    .inner_margin(Margin::same(0)),
            )
            .show(ui, |ui| match self.prefs.view {
                View::Opponents => self
                    .opponents
                    .details(ui, &self.data, &self.derived, actions),
                View::Tonight => {
                    let selection = self.tonight.selection(&self.data);
                    details::show(ui, selection, &self.data, self.conn.as_ref(), actions)
                }
                _ => {
                    let selection = self.replays.selection(&self.data, &self.derived);
                    details::show(ui, selection, &self.data, self.conn.as_ref(), actions)
                }
            });
    }

    /// The Filters button's menu, under it, while open: closed by the
    /// button, Esc, or a click anywhere else.
    fn filters_menu(&mut self, ctx: &egui::Context) {
        let menu = &mut self.filters_menu;
        if !menu.open || !self.prefs.view.uses_filter() || self.stats.is_some() {
            menu.open = false;
            return;
        }
        let Some(button) = menu.button else { return };
        let escape = ctx.input(|i| i.key_pressed(egui::Key::Escape));
        let clicked_away = ctx.input(|i| i.pointer.any_pressed())
            && !ctx
                .input(|i| i.pointer.interact_pos())
                .is_some_and(|p| button.contains(p) || menu.area.is_some_and(|r| r.contains(p)));
        if escape || clicked_away {
            menu.open = false;
            return;
        }
        let area = egui::Area::new(egui::Id::unique("filters-menu"))
            .order(egui::Order::Foreground)
            .fixed_pos(button.left_bottom() + vec2(0.0, 6.0))
            .show(ctx, |ui| {
                egui::Frame::new()
                    .fill(theme::SIDEBAR)
                    .corner_radius(8)
                    .stroke(egui::Stroke::new(1.0, theme::SEPARATOR))
                    .shadow(ui.visuals().popup_shadow)
                    .inner_margin(Margin::same(14))
                    .show(ui, |ui| {
                        ui.set_max_width(460.0);
                        filter_bar::menu(ui, &mut self.prefs.filter, &self.data);
                    });
            });
        self.filters_menu.area = Some(area.response.rect);
    }

    fn toasts(&mut self, ctx: &egui::Context) {
        self.toasts
            .retain(|t| t.since.elapsed() < Duration::from_secs(if t.error { 8 } else { 3 }));
        if self.toasts.is_empty() {
            return;
        }
        ctx.request_repaint_after(Duration::from_millis(250));
        egui::Area::new(egui::Id::unique("toasts"))
            .anchor(egui::Align2::RIGHT_BOTTOM, vec2(-16.0, -52.0))
            .order(egui::Order::Foreground)
            .show(ctx, |ui| {
                for toast in &self.toasts {
                    egui::Frame::new()
                        .fill(if toast.error {
                            theme::NOTICE
                        } else {
                            theme::BUTTON
                        })
                        .corner_radius(8)
                        .inner_margin(Margin::symmetric(12, 8))
                        .stroke(egui::Stroke::new(1.0, theme::SEPARATOR))
                        .show(ui, |ui| {
                            ui.set_max_width(360.0);
                            let color = if toast.error {
                                theme::WARNING
                            } else {
                                theme::TEXT
                            };
                            ui.label(RichText::new(&toast.text).color(color));
                        });
                    ui.add_space(6.0);
                }
            });
    }

    /// What the rank filter needs looked up: for each opponent in the
    /// games the other filters keep, their rank in the season of each game.
    /// Games from before ranked began need nothing. A finished season needs
    /// their history fetched after it ended (it can't change after that,
    /// so it's fetched once); the season running now needs their current
    /// rank, of any age. The same request brings both, so both go in one
    /// queue, the most recently played opponents first.
    fn plan_rank_filter(&mut self) {
        let mut others = self.prefs.filter.clone();
        others.their_ranks.clear();
        let key = (
            self.data.revision,
            self.data.ranks_revision,
            model::hash_str(&format!("{others:?}")),
            self.data.seasons.len(),
        );
        if self.rank_plan_key == Some(key) {
            return;
        }
        self.rank_plan_key = Some(key);
        let data = &self.data;
        let mut seen = HashSet::new();
        let mut needed = Vec::new();
        for game in &data.games {
            if !others.matches(game, &data.me, data.now, |_, _| None) {
                continue;
            }
            let Some(code) = game.opponent(&data.me).map(|p| p.code.as_str()) else {
                continue;
            };
            if !code.contains('#') || seen.contains(code) {
                continue;
            }
            let Some(season) = data
                .seasons
                .iter()
                .find(|s| game.start_at >= s.start && s.end.is_none_or(|end| game.start_at < end))
            else {
                continue; // before ranked
            };
            let missing = match season.end {
                None => !data.ranks.contains_key(code) && !data.histories.contains_key(code),
                Some(end) => data.histories.get(code).is_none_or(|(_, at)| *at < end),
            };
            if missing && !self.ranks.history_recently_failed(code) {
                seen.insert(code);
                needed.push(code);
            }
        }
        self.ranks.want_histories(needed);
    }

    fn want_ranks(&mut self) {
        if !self.settings.fetch_ranks {
            return;
        }
        let codes: Vec<String> = match self.prefs.view {
            View::Replays => self.replays.visible_codes(&self.data, &self.derived),
            View::Tonight => self.tonight.visible_codes(&self.data),
            View::Opponents => self.opponents.visible_codes(&self.derived),
            _ => Vec::new(),
        };
        let live: Vec<String> = self
            .data
            .live
            .iter()
            .filter_map(|g| g.opponent(&self.data.me).map(|p| p.code.clone()))
            .collect();
        self.ranks.want(
            codes.iter().chain(live.iter()).map(String::as_str),
            &self.data.ranks,
            &self.data.last_played,
            self.data.now,
            false,
        );
        if !self.prefs.filter.their_ranks.is_empty() {
            self.plan_rank_filter();
        }
        self.data.ranks_pending = self.ranks.pending_count();
    }
}

/// How a stored rank reads: "None" (no sets yet) as "Unranked".
pub fn display_rank(rank: &str) -> String {
    match rank {
        "" | "None" => "Unranked".to_owned(),
        other => other.to_owned(),
    }
}

impl eframe::App for EppiApp {
    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.data.now = crate::time::now();
        self.handle_library_events(&ctx);
        self.handle_rank_results();
        self.derived.update(&self.data, &self.prefs.filter);

        let mut actions = Vec::new();
        self.keyboard(&ctx, &mut actions);

        self.header(ui, &mut actions);
        self.nav(ui, &mut actions);
        self.status_bar(ui, &mut actions);
        self.details(ui, &mut actions);

        let view = self.prefs.view;
        egui::CentralPanel::no_frame()
            .frame(egui::Frame::new().fill(theme::SURFACE))
            .show(ui, |ui| {
                if let Some(page) = &mut self.stats {
                    page.show(ui, &self.data, &mut actions);
                    return;
                }
                if view.uses_filter() {
                    filter_bar::show(ui, &mut self.prefs.filter, &self.data);
                }
                match view {
                    View::Tonight => {
                        self.tonight
                            .show(ui, &self.data, self.conn.as_ref(), &mut actions)
                    }
                    View::Replays => self
                        .replays
                        .show(ui, &self.data, &self.derived, &mut actions),
                    View::Opponents => self.opponents.show(
                        ui,
                        &self.data,
                        &self.derived,
                        &mut self.prefs.opponent_sort,
                        &mut actions,
                    ),
                    View::Me => self
                        .me
                        .show(ui, &self.data, &self.derived, self.conn.as_ref()),
                    View::Highlights => self.highlights.show(
                        ui,
                        &self.data,
                        &self.derived,
                        self.conn.as_ref(),
                        &mut actions,
                    ),
                    View::Settings => {
                        if self.settings_view.show(ui, &mut self.settings, &self.data) {
                            actions.push(Action::SettingsChanged);
                        }
                    }
                }
            });

        self.filters_menu(&ctx);
        self.toasts(&ctx);
        self.want_ranks();
        let mut tour_done = false;
        if let Some(tour) = &mut self.screenshots {
            tour_done = tour.step(&ctx, frame, &mut self.prefs, &self.data, &mut actions);
        }
        self.apply(&ctx, actions);
        if tour_done {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }

        if !self.data.live.is_empty() || self.data.progress.phase != Phase::Idle {
            ctx.request_repaint_after(Duration::from_secs(1));
        } else {
            // Times like "2 min ago" move on.
            ctx.request_repaint_after(Duration::from_secs(30));
        }
    }

    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        // The screenshot tour changes views and filters: keep the user's.
        if self.screenshots.is_some() {
            return;
        }
        eframe::set_value(storage, SETTINGS_KEY, &self.settings);
        eframe::set_value(storage, PREFS_KEY, &self.prefs);
    }
}

fn show_in_folder(path: &std::path::Path) {
    #[cfg(windows)]
    {
        let _ = std::process::Command::new("explorer")
            .arg(format!("/select,{}", path.display()))
            .spawn();
    }
    #[cfg(target_os = "macos")]
    {
        // Finder, with the file selected.
        let _ = std::process::Command::new("open")
            .arg("-R")
            .arg(path)
            .spawn();
    }
    #[cfg(not(any(windows, target_os = "macos")))]
    {
        if let Some(parent) = path.parent() {
            let _ = open::that(parent);
        }
    }
}

/// The result of `outcome` as a word.
pub fn outcome_word(outcome: Outcome) -> &'static str {
    match outcome {
        Outcome::Win => "Won",
        Outcome::Loss => "Lost",
        Outcome::Unknown => "No result",
    }
}

/// Season id to rank name, from a player's seasons.
fn tiers(seasons: &[model::Season]) -> HashMap<String, String> {
    seasons
        .iter()
        .map(|s| (s.id.clone(), s.rank.clone()))
        .collect()
}
