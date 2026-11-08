//! `eppi --screenshots <folder>`: opens each view once the library has
//! loaded, saves a PNG of the window for each, and quits. For the README
//! and for checking the UI without clicking through it.

use crate::app::{Action, Data, Prefs, View};
use eframe::egui;
use std::path::PathBuf;
use std::time::{Duration, Instant};

pub struct Tour {
    /// Other players made anonymous (`--showcase`).
    pub demo: bool,
    /// The zoom the pictures are drawn at (2 for sharp showcase pictures).
    pub scale: f32,
    folder: PathBuf,
    steps: Vec<Step>,
    /// Frames drawn since the current step started.
    frames: u32,
    started: Option<Instant>,
    requested: bool,
    seen_busy: bool,
    /// The window was given its size.
    sized: bool,
    quiet_since: Option<Instant>,
    /// Where egui's screenshot callback leaves the picture.
    shot: std::sync::Arc<std::sync::Mutex<Option<std::sync::Arc<egui::ColorImage>>>>,
}

#[derive(Clone, Copy, Debug)]
enum Step {
    View(View, &'static str),
    /// A view in a window tall enough to show all of it.
    Tall(View, &'static str),
    /// Replays against Gold players, once every opponent's rank is known.
    RankFilter(&'static str),
    /// The search bar with its suggestions open.
    Search(&'static str),
    /// A tab of the stats page opened by the step before.
    StatsTab(&'static str, &'static str),
    Stats(&'static str),
}

impl Tour {
    /// The window, in the units the OS gives eframe: the view at `scale`.
    pub fn window_size(&self) -> [f32; 2] {
        let (w, h) = if self.demo {
            (1280.0, 800.0)
        } else {
            (1360.0, 860.0)
        };
        [w * self.scale, h * self.scale]
    }

    /// Pictures for the README: anonymous players, drawn at twice the size.
    pub fn showcase(folder: PathBuf) -> Self {
        let mut tour = Self::new(folder);
        tour.demo = true;
        // As large as fits this PC's screen (Windows shrinks a bigger
        // window to fit).
        tour.scale = 1.25;
        tour.steps = vec![
            Step::View(View::Replays, "replays"),
            Step::View(View::Tonight, "tonight"),
            Step::Stats("stats"),
            Step::StatsTab("punishes", "punishes"),
            Step::View(View::Opponents, "opponents"),
            Step::View(View::Me, "me"),
            Step::View(View::Highlights, "highlights"),
            Step::Search("search"),
            Step::RankFilter("filters"),
        ];
        tour
    }

    pub fn new(folder: PathBuf) -> Self {
        let _ = std::fs::create_dir_all(&folder);
        Self {
            demo: false,
            scale: 1.0,
            folder,
            steps: vec![
                Step::View(View::Tonight, "1-tonight"),
                Step::View(View::Replays, "2-replays"),
                Step::Stats("3-stats"),
                Step::View(View::Opponents, "4-opponents"),
                Step::View(View::Me, "5-me"),
                Step::Tall(View::Me, "5b-me-full"),
                Step::View(View::Highlights, "6-highlights"),
                Step::View(View::Settings, "7-settings"),
                Step::RankFilter("8-rank-filter"),
                Step::Search("9-search"),
            ],
            frames: 0,
            started: None,
            requested: false,
            seen_busy: false,
            sized: false,
            quiet_since: None,
            shot: Default::default(),
        }
    }

    /// Runs one frame of the tour; returns true when it's done.
    pub fn step(
        &mut self,
        ctx: &egui::Context,
        _frame: &mut eframe::Frame,
        prefs: &mut Prefs,
        data: &Data,
        actions: &mut Vec<Action>,
    ) -> bool {
        ctx.request_repaint();
        // Wait for the library to read the folders and work out the stats
        // (it starts idle, so wait to see it busy first, or 10 s of quiet),
        // and for the ranks of the people on screen.
        let started = *self.started.get_or_insert_with(Instant::now);
        let idle = data.progress.phase == crate::library::Phase::Idle;
        if !idle {
            self.seen_busy = true;
        }
        // A machine with no replays (a build check) settles too, once the
        // library has stayed idle a while.
        let settled = idle
            && (self.seen_busy || started.elapsed() > Duration::from_secs(10))
            && (!data.games.is_empty() || started.elapsed() > Duration::from_secs(20));
        if !settled && started.elapsed() < Duration::from_secs(300) {
            self.quiet_since = None;
            return false;
        }
        let quiet = *self.quiet_since.get_or_insert_with(Instant::now);
        if quiet.elapsed() < Duration::from_secs(8) {
            return false;
        }
        // The screenshot callback leaves the picture here.
        let image = self.shot.lock().ok().and_then(|mut s| s.take());
        if let Some(image) = image {
            let name = match self.steps.first() {
                Some(
                    Step::View(_, name)
                    | Step::Tall(_, name)
                    | Step::Stats(name)
                    | Step::RankFilter(name)
                    | Step::Search(name)
                    | Step::StatsTab(_, name),
                ) => *name,
                None => "done",
            };
            let path = self.folder.join(format!("{name}.png"));
            let [w, h] = image.size;
            let bytes: Vec<u8> = image.pixels.iter().flat_map(|p| p.to_array()).collect();
            if let Err(e) = image::save_buffer(
                &path,
                &bytes,
                w as u32,
                h as u32,
                image::ExtendedColorType::Rgba8,
            ) {
                log::error!("save {}: {e}", path.display());
            } else {
                log::info!("saved {}", path.display());
            }
            if matches!(self.steps.first(), Some(Step::Tall(..))) {
                ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(1360.0, 860.0)));
            }
            self.steps.remove(0);
            self.frames = 0;
            self.requested = false;
            return self.steps.is_empty();
        }
        let Some(step) = self.steps.first().copied() else {
            return true;
        };
        if self.frames == 0 && !self.sized {
            // Windows can open the window maximized or clamped; ask for the
            // size the pictures are meant to have.
            self.sized = true;
            ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(false));
            let [w, h] = self.window_size();
            let zoom = ctx.zoom_factor();
            ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(
                w / zoom,
                h / zoom,
            )));
        }
        if self.frames == 0 {
            match step {
                Step::View(view, _) => {
                    actions.push(Action::CloseStats);
                    actions.push(Action::DemoLive(None));
                    prefs.view = view;
                    if self.demo {
                        // From the defaults, not the owner's saved filters.
                        prefs.filter = crate::model::Filter::default();
                        prefs.show_filters = false;
                    }
                    // Show the details pane with something in it.
                    let latest = data.games.iter().find(|g| g.sides(&data.me).is_some());
                    match view {
                        View::Replays => {
                            if let Some(game) = featured_game(data) {
                                actions.push(Action::GoToGame(game.id));
                            }
                        }
                        View::Tonight => {
                            if let Some(game) = latest {
                                let mut live = game.clone();
                                live.path = "live.slp".into();
                                actions.push(Action::DemoLive(Some(Box::new(live))));
                            }
                        }
                        View::Opponents => {
                            if let Some(code) = rival(data) {
                                actions.push(Action::GoToOpponent(code));
                            }
                        }
                        _ => {}
                    }
                }
                Step::Tall(view, _) => {
                    prefs.view = view;
                    ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(
                        1360.0, 2300.0,
                    )));
                }
                Step::Search(_) => {
                    prefs.filter.their_ranks.clear();
                    prefs.show_filters = false;
                    prefs.view = View::Replays;
                    actions.push(Action::DemoSearch(
                        if self.demo { "co" } else { "pin" }.into(),
                    ));
                }
                Step::RankFilter(_) => {
                    actions.push(Action::CloseStats);
                    actions.push(Action::DemoLive(None));
                    // Clears the search the step before typed, too.
                    prefs.filter = crate::model::Filter::default();
                    prefs.view = View::Replays;
                    prefs.show_filters = true;
                    prefs.filter.their_ranks = vec![crate::model::RankTier::Gold];
                }
                Step::StatsTab(tab, _) => {
                    actions.push(Action::DemoLive(None));
                    actions.push(Action::DemoStatsTab(tab));
                }
                Step::Stats(_) => {
                    prefs.view = View::Replays;
                    actions.push(Action::DemoLive(None));
                    if let Some(game) = featured_game(data) {
                        actions.push(Action::OpenStats(game.id));
                    }
                }
            }
            // No game to open (a machine without replays): no stats
            // pictures, rather than copies of the replay list.
            if matches!(step, Step::Stats(_) | Step::StatsTab(..)) && featured_game(data).is_none()
            {
                log::info!("no game with stats: skipped {step:?}");
                self.steps.remove(0);
                return self.steps.is_empty();
            }
        }
        self.frames += 1;
        // Let images load and layouts settle before the capture.
        let waiting_for_ranks =
            matches!(step, Step::RankFilter(_)) && data.ranks_pending > 0 && self.frames < 60 * 240;
        if self.frames > 45 && !self.requested && !waiting_for_ranks {
            self.requested = true;
            let shot = self.shot.clone();
            let ctx2 = ctx.clone();
            ctx.request_screenshot(move |image| {
                if let Ok(mut s) = shot.lock() {
                    *s = Some(image);
                }
                ctx2.request_repaint();
            });
        }
        false
    }
}

/// The newest game I won that has stats: what the pictures show open.
fn featured_game(data: &Data) -> Option<&crate::model::Game> {
    data.games
        .iter()
        .find(|g| g.stats_version > 0 && g.outcome(&data.me) == crate::model::Outcome::Win)
}

/// The ranked opponent I've played most among my last 400 games.
fn rival(data: &Data) -> Option<String> {
    let mut counts: std::collections::HashMap<&str, usize> = std::collections::HashMap::new();
    let ranked = |code: &str| {
        data.rank(code)
            .and_then(|r| r.rank.as_deref())
            .is_some_and(|r| crate::model::RankTier::of(r) != crate::model::RankTier::Unranked)
    };
    for game in data
        .games
        .iter()
        .filter(|g| g.sides(&data.me).is_some())
        .take(400)
    {
        if let Some(opponent) = game
            .opponent(&data.me)
            .filter(|p| !p.code.is_empty() && ranked(&p.code))
        {
            *counts.entry(&opponent.code).or_default() += 1;
        }
    }
    counts
        .into_iter()
        .max_by_key(|(_, n)| *n)
        .map(|(code, _)| code.to_owned())
}
