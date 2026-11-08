//! `eppi --screenshots <folder>`: opens each view once the library has
//! loaded, saves a PNG of the window for each, and quits. For the README
//! and for checking the UI without clicking through it.

use crate::app::{Action, Data, Prefs, View};
use eframe::egui;
use std::path::PathBuf;
use std::time::{Duration, Instant};

pub struct Tour {
    folder: PathBuf,
    steps: Vec<Step>,
    /// Frames drawn since the current step started.
    frames: u32,
    started: Option<Instant>,
    requested: bool,
    seen_busy: bool,
    quiet_since: Option<Instant>,
    /// Where egui's screenshot callback leaves the picture.
    shot: std::sync::Arc<std::sync::Mutex<Option<std::sync::Arc<egui::ColorImage>>>>,
}

#[derive(Clone, Copy, Debug)]
enum Step {
    View(View, &'static str),
    /// A view in a window tall enough to show all of it.
    Tall(View, &'static str),
    Stats(&'static str),
}

impl Tour {
    pub fn new(folder: PathBuf) -> Self {
        let _ = std::fs::create_dir_all(&folder);
        Self {
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
            ],
            frames: 0,
            started: None,
            requested: false,
            seen_busy: false,
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
        let settled = idle
            && !data.games.is_empty()
            && (self.seen_busy || started.elapsed() > Duration::from_secs(10));
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
                Some(Step::View(_, name) | Step::Tall(_, name) | Step::Stats(name)) => *name,
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
        if self.frames == 0 {
            match step {
                Step::View(view, _) => {
                    actions.push(Action::CloseStats);
                    actions.push(Action::DemoLive(None));
                    prefs.view = view;
                    // Show the details pane with something in it.
                    let latest = data.games.iter().find(|g| g.sides(&data.me).is_some());
                    match view {
                        View::Replays => {
                            if let Some(game) = latest {
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
                            if let Some(opponent) = latest.and_then(|g| g.opponent(&data.me)) {
                                actions.push(Action::GoToOpponent(opponent.code.clone()));
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
                Step::Stats(_) => {
                    prefs.view = View::Replays;
                    if let Some(game) = data
                        .games
                        .iter()
                        .find(|g| g.stats_version > 0 && g.sides(&data.me).is_some())
                    {
                        actions.push(Action::OpenStats(game.id));
                    }
                }
            }
        }
        self.frames += 1;
        // Let images load and layouts settle before the capture.
        if self.frames > 45 && !self.requested {
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
