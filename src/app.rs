use eframe::egui;
use egui::TextureHandle;
use egui_file::FileDialog;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::mpsc;

pub use crate::peppi::*;
use crate::ui::helpers::{format_date, format_duration};

#[derive(PartialEq, serde::Deserialize, serde::Serialize)]
pub(crate) enum DemoType {
    Manual,
    ReplayData,
    ManyHomogeneous,
    ManyHeterogenous,
}

/// We derive Deserialize/Serialize so we can persist app state on shutdown.
#[derive(serde::Deserialize, serde::Serialize)]
#[serde(default)] // if we add new fields, give them default values when deserializing old state
pub struct Eppi {
    connect_code: String,
    replay_dir: String,

    // Table demo fields
    demo: DemoType,
    striped: bool,
    overline: bool,
    resizable: bool,
    clickable: bool,
    num_rows: usize,
    scroll_to_row_slider: usize,
    scroll_to_row: Option<usize>,
    selection: std::collections::HashSet<usize>,
    checked: bool,
    reversed: bool,

    #[serde(skip)]
    opened_file: Option<PathBuf>,
    #[serde(skip)]
    open_file_dialog: Option<FileDialog>,
    #[serde(skip)]
    open_dir_dialog: Option<FileDialog>,
    #[serde(skip)]
    replay_analyzer: ReplayAnalyzer,
    #[serde(skip)]
    is_scanning: bool,
    #[serde(skip)]
    scan_status: String,
    #[serde(skip)]
    is_fetching_rank: bool,
    #[serde(skip)]
    rank_receiver: Option<mpsc::Receiver<(String, Result<String, String>)>>,
    #[serde(skip)]
    scan_receiver: Option<mpsc::Receiver<Result<ReplayAnalyzer, String>>>,
    #[serde(skip)]
    rank_icons: HashMap<String, TextureHandle>,
}

impl Default for Eppi {
    fn default() -> Self {
        Self {
            connect_code: "".to_owned(),
            replay_dir: "".to_owned(),
            demo: DemoType::ReplayData,
            striped: true,
            overline: false,
            resizable: true,
            clickable: true,
            num_rows: 10,
            scroll_to_row_slider: 0,
            scroll_to_row: None,
            selection: std::collections::HashSet::new(),
            checked: false,
            reversed: false,
            opened_file: None,
            open_file_dialog: None,
            open_dir_dialog: None,
            replay_analyzer: ReplayAnalyzer::new(),
            is_scanning: false,
            scan_status: "Ready".to_string(),
            is_fetching_rank: false,
            rank_receiver: None,
            scan_receiver: None,
            rank_icons: HashMap::new(),
        }
    }
}

impl Eppi {
    /// Called once before the first frame.
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        // This is also where you can customize the look and feel of egui using
        // `cc.egui_ctx.set_visuals` and `cc.egui_ctx.set_fonts`.

        // Load previous app state (if any).
        // Note that you must enable the `persistence` feature for this to work.
        let mut app = if let Some(storage) = cc.storage {
            eframe::get_value(storage, eframe::APP_KEY).unwrap_or_default()
        } else {
            Self::default()
        };

        // Always start in replay data mode
        app.demo = DemoType::ReplayData;

        // Load rank icons
        app.load_rank_icons(&cc.egui_ctx);

        app
    }

    fn scan_replays(&mut self, ctx: &egui::Context) {
        if !self.replay_dir.is_empty() && !self.is_scanning {
            self.is_scanning = true;
            self.scan_status = "Scanning replays...".to_string();

            // Create channel for async communication
            let (tx, rx) = mpsc::channel();
            self.scan_receiver = Some(rx);

            // Spawn async task for scanning
            let replay_dir = self.replay_dir.clone();
            let ctx_clone = ctx.clone();

            tokio::spawn(async move {
                // Adding a small delay to make the spinner visible for testing
                // tokio::time::sleep(tokio::time::Duration::from_millis(1000)).await;

                let mut analyzer = ReplayAnalyzer::new();
                let result = match analyzer.scan_directory(&replay_dir) {
                    Ok(_) => Ok(analyzer),
                    Err(e) => Err(format!("Error: {e}")),
                };

                // Send result through channel
                if tx.send(result).is_ok() {
                    // Request repaint to update UI with the result
                    ctx_clone.request_repaint();
                }
            });
        }
    }

    fn lookup_opponent_rank(&mut self, ctx: &egui::Context) {
        if !self.connect_code.is_empty()
            && !self.is_fetching_rank
            && !self.replay_analyzer.replays.is_empty()
        {
            self.is_fetching_rank = true;
            self.scan_status = "Looking up opponent rank...".to_string();

            // Get the opponent from the most recent replay
            let most_recent_replay = &self.replay_analyzer.replays[0];
            let opponent_tag = if most_recent_replay.player1.name == self.connect_code {
                most_recent_replay.player2.name.clone()
            } else {
                most_recent_replay.player1.name.clone()
            };

            // Check if we already have this opponent's rank cached
            let cached_rank = self.replay_analyzer.get_cached_rank(&opponent_tag).cloned();
            if let Some(cached_rank) = cached_rank {
                // Update the most recent replay with cached rank
                if let Some(first_replay) = self.replay_analyzer.replays.get_mut(0) {
                    first_replay.opponent_rank = Some(cached_rank.clone());
                }
                self.scan_status = format!("Found cached rank for {opponent_tag}: {cached_rank}");
                self.is_fetching_rank = false;
                return;
            }

            // Create channel for async communication
            let (tx, rx) = mpsc::channel();
            self.rank_receiver = Some(rx);

            // Spawn async task for web scraping
            let ctx_clone = ctx.clone();
            let opponent_tag_clone = opponent_tag.clone();

            tokio::spawn(async move {
                let result = match crate::peppi::fetch_player_rank(&opponent_tag_clone).await {
                    Ok(rank) => Ok(rank),
                    Err(e) => Err(format!("Failed to fetch rank: {e}")),
                };

                // Send result through channel
                if tx.send((opponent_tag_clone, result)).is_ok() {
                    // Request repaint to update UI with the result
                    ctx_clone.request_repaint();
                }
            });

            self.scan_status = format!("Looking up rank for {opponent_tag}...");
        }
    }

    fn rank_to_icon_path(rank: &str) -> Option<String> {
        // Map rank strings to icon file names
        let icon_name = match rank {
            // Handle various rank formats
            rank if rank.starts_with("Bronze") => rank.replace("Bronze", "BRONZE"),
            rank if rank.starts_with("Silver") => rank.replace("Silver", "SILVER"),
            rank if rank.starts_with("Gold") => rank.replace("Gold", "GOLD"),
            rank if rank.starts_with("Platinum") => rank.replace("Platinum", "PLATINUM"),
            rank if rank.starts_with("Diamond") => rank.replace("Diamond", "DIAMOND"),
            rank if rank.starts_with("Master") => rank.replace("Master", "MASTER"),
            "Grandmaster" => "GRANDMASTER".to_string(),
            "Unranked" => "UNRANKED".to_string(),
            "Unknown" => "undefined".to_string(),
            _ => return None,
        };

        Some(format!("assets/rank-icons/{icon_name}.svg"))
    }

    fn load_rank_icons(&mut self, ctx: &egui::Context) {
        // List of all rank names that might appear
        let ranks = vec![
            "Bronze 1",
            "Bronze 2",
            "Bronze 3",
            "Silver 1",
            "Silver 2",
            "Silver 3",
            "Gold 1",
            "Gold 2",
            "Gold 3",
            "Platinum 1",
            "Platinum 2",
            "Platinum 3",
            "Diamond 1",
            "Diamond 2",
            "Diamond 3",
            "Master 1",
            "Master 2",
            "Master 3",
            "Grandmaster",
            "Unranked",
            "Unknown",
        ];

        for rank in ranks {
            if let Some(icon_path) = Self::rank_to_icon_path(rank) {
                // Try to load the SVG file
                if let Ok(svg_bytes) = std::fs::read(&icon_path) {
                    // Load SVG as an image
                    let image = egui_extras::image::load_svg_bytes(&svg_bytes);

                    match image {
                        Ok(color_image) => {
                            let texture = ctx.load_texture(
                                format!("rank_{}", rank.replace(' ', "_")),
                                color_image,
                                egui::TextureOptions::LINEAR,
                            );
                            self.rank_icons.insert(rank.to_string(), texture);
                        }
                        Err(e) => {
                            log::error!("Failed to load rank icon {icon_path}: {e}");
                        }
                    }
                } else {
                    log::error!("Failed to read rank icon file: {icon_path}");
                }
            }
        }
    }
}

impl eframe::App for Eppi {
    /// Called by the frame work to save state before shutdown.
    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        eframe::set_value(storage, eframe::APP_KEY, self);
    }

    /// Called each time the UI needs repainting, which may be many times per second.
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Check for scan results from async tasks
        if let Some(receiver) = &self.scan_receiver {
            if let Ok(result) = receiver.try_recv() {
                match result {
                    Ok(replay_analyzer) => {
                        // Replace our analyzer with the one from the async task
                        self.replay_analyzer = replay_analyzer;
                        self.scan_status =
                            format!("Found {} replays", self.replay_analyzer.replays.len());
                    }
                    Err(error_msg) => {
                        self.scan_status = error_msg;
                    }
                }
                self.is_scanning = false;
                self.scan_receiver = None; // Clear the receiver
            }
        }

        // Check for rank lookup results from async tasks
        if let Some(receiver) = &self.rank_receiver {
            if let Ok((opponent_tag, result)) = receiver.try_recv() {
                match result {
                    Ok(rank) => {
                        // Update cache and most recent replay
                        self.replay_analyzer
                            .rank_cache
                            .insert(opponent_tag.clone(), rank.clone());
                        if let Some(first_replay) = self.replay_analyzer.replays.get_mut(0) {
                            first_replay.opponent_rank = Some(rank.clone());
                        }
                        self.scan_status = format!("Found rank for {opponent_tag}: {rank}");
                    }
                    Err(error_msg) => {
                        // Cache the error to avoid retrying
                        self.replay_analyzer
                            .rank_cache
                            .insert(opponent_tag.clone(), "Unranked".to_string());
                        self.scan_status =
                            format!("Failed to lookup rank for {opponent_tag}: {error_msg}");
                    }
                }
                self.is_fetching_rank = false;
                self.rank_receiver = None; // Clear the receiver
            }
        }
        // Put your widgets into a `SidePanel`, `TopBottomPanel`, `CentralPanel`, `Window` or `Area`.
        // For inspiration and more examples, go to https://emilk.github.io/egui

        egui::TopBottomPanel::top("top_panel").show(ctx, |ui| {
            // The top panel is often a good place for a menu bar:

            egui::menu::bar(ui, |ui| {
                // NOTE: no File->Quit on web pages!
                let is_web = cfg!(target_arch = "wasm32");
                if !is_web {
                    ui.menu_button("File", |ui| {
                        if ui.button("Quit").clicked() {
                            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                        }
                    });
                    ui.add_space(16.0);
                }

                egui::widgets::global_theme_preference_buttons(ui);
            });
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            // The central panel the region left after adding TopPanel's and SidePanel's
            ui.horizontal(|ui| {
                ui.label("My Connect Code:");
                ui.text_edit_singleline(&mut self.connect_code);
            });

            ui.horizontal(|ui| {
                ui.label("Replays Directory:");
                ui.text_edit_singleline(&mut self.replay_dir);
                if ui.button("Browse...").clicked() {
                    let initial_path = if self.replay_dir.is_empty() {
                        None
                    } else {
                        Some(self.replay_dir.clone().into())
                    };
                    let mut dialog = FileDialog::select_folder(initial_path);
                    dialog.open();
                    self.open_dir_dialog = Some(dialog);
                }

                ui.add_enabled_ui(!self.is_scanning && !self.replay_dir.is_empty(), |ui| {
                    if ui.button("Scan Replays").clicked() {
                        self.scan_replays(ctx);
                    }
                });

                // Show a loading spinner while scanning replays, similar to the opponent-rank lookup flow
                if self.is_scanning {
                    ui.spinner();
                }
            });

            ui.horizontal(|ui| {
                ui.label("Status:");
                if self.is_scanning {
                    ui.spinner();
                }
                ui.label(&self.scan_status);

                ui.separator();

                ui.add_enabled_ui(
                    !self.is_fetching_rank
                        && !self.connect_code.is_empty()
                        && !self.replay_analyzer.replays.is_empty(),
                    |ui| {
                        if ui.button("Lookup Opponent Rank").clicked() {
                            self.lookup_opponent_rank(ctx);
                        }
                    },
                );

                if self.is_fetching_rank {
                    ui.spinner();
                }
            });

            if let Some(dialog) = &mut self.open_dir_dialog {
                if dialog.show(ctx).selected() {
                    if let Some(path) = dialog.path() {
                        self.replay_dir = path.to_string_lossy().to_string();
                    }
                }
            }

            ui.separator();

            self.replays_table(ui);

            egui::warn_if_debug_build(ui);
        });
    }
}

impl Eppi {
    fn replays_table(&mut self, ui: &mut egui::Ui) {
        // Always use striped rows, resizable columns and clickable rows.
        self.striped = true;
        self.resizable = true;
        self.clickable = false;

        // The demo modes have been removed ‑ we are always in replay-data mode.
        self.demo = DemoType::ReplayData;

        ui.vertical(|ui| {
            // Display W/L stats if a connect code is provided
            ui.horizontal(|ui| {
                if !self.connect_code.is_empty() {
                    let (wins, losses) = self
                        .replay_analyzer
                        .get_stats_for_player(&self.connect_code);
                    let total = wins + losses;
                    let win_rate = if total > 0 {
                        wins as f64 / total as f64 * 100.0
                    } else {
                        0.0
                    };
                    ui.label(format!("W/L: {wins}/{losses} ({win_rate:.1}%)"));
                }
            });
        });

        ui.separator();

        // The table itself
        egui::ScrollArea::horizontal().show(ui, |ui| {
            self.table_ui(ui, /*reset=*/ false);
        });
    }

    fn table_ui(&mut self, ui: &mut egui::Ui, reset: bool) {
        use egui_extras::{Column, TableBuilder};

        let text_height = egui::TextStyle::Body
            .resolve(ui.style())
            .size
            .max(ui.spacing().interact_size.y);

        let available_height = ui.available_height();

        let mut table = TableBuilder::new(ui)
            .striped(self.striped)
            .resizable(self.resizable)
            .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
            .column(Column::auto().at_least(100.0)) // Player 1
            .column(Column::auto().at_least(100.0)) // Player 2
            .column(Column::auto().at_least(60.0)) // Result
            .column(Column::auto().at_least(120.0)) // Stage
            .column(Column::auto().at_least(80.0)) // Date
            .column(Column::auto().at_least(70.0)) // Duration
            .column(Column::auto().at_least(120.0)) // Opponent Rank
            .min_scrolled_height(0.0)
            .max_scroll_height(available_height);

        if self.clickable {
            table = table.sense(egui::Sense::click());
        }

        if let Some(row_index) = self.scroll_to_row.take() {
            table = table.scroll_to_row(row_index, None);
        }

        if reset {
            table.reset();
        }

        table
            .header(20.0, |mut header| {
                header.col(|ui| {
                    ui.strong("Player 1");
                });
                header.col(|ui| {
                    ui.strong("Player 2");
                });
                header.col(|ui| {
                    ui.strong("Result");
                });
                header.col(|ui| {
                    ui.strong("Stage");
                });
                header.col(|ui| {
                    ui.strong("Date");
                });
                header.col(|ui| {
                    ui.strong("Duration");
                });
                header.col(|ui| {
                    ui.strong("Opponent Rank");
                });
            })
            .body(|mut body| {
                let replays = &self.replay_analyzer.replays;
                let connect_code = &self.connect_code;
                let mut rows_to_toggle = Vec::new();

                if replays.is_empty() {
                    // Show helpful message when no replays are loaded
                    body.row(30.0, |mut row| {
                        row.col(|ui| {
                            ui.label("");
                        });
                        row.col(|ui| {
                            ui.label("");
                        });
                        row.col(|ui| {
                            ui.colored_label(egui::Color32::GRAY, "No replays loaded. Browse to your Slippi directory and click 'Scan Replays'");
                        });
                        row.col(|ui| {
                            ui.label("");
                        });
                        row.col(|ui| {
                            ui.label("");
                        });
                        row.col(|ui| {
                            ui.label("");
                        });
                        row.col(|ui| {
                            ui.label("");
                        });
                    });
                }

                for (row_index, replay) in replays.iter().enumerate() {
                    body.row(text_height, |mut row| {
                        row.set_selected(self.selection.contains(&row_index));

                        row.col(|ui| {
                            ui.label(&replay.player1.name);
                        });
                        row.col(|ui| {
                            ui.label(&replay.player2.name);
                        });
                        row.col(|ui| {
                            let (result_text, color) = match &replay.result {
                                GameResult::Player1Won => {
                                    if !connect_code.is_empty()
                                        && replay.player1.name == *connect_code
                                    {
                                        ("WIN", egui::Color32::GREEN)
                                    } else if !connect_code.is_empty()
                                        && replay.player2.name == *connect_code
                                    {
                                        ("LOSS", egui::Color32::RED)
                                    } else {
                                        ("P1 Win", egui::Color32::GRAY)
                                    }
                                }
                                GameResult::Player2Won => {
                                    if !connect_code.is_empty()
                                        && replay.player2.name == *connect_code
                                    {
                                        ("WIN", egui::Color32::GREEN)
                                    } else if !connect_code.is_empty()
                                        && replay.player1.name == *connect_code
                                    {
                                        ("LOSS", egui::Color32::RED)
                                    } else {
                                        ("P2 Win", egui::Color32::GRAY)
                                    }
                                }
                                GameResult::Unknown => ("Unknown", egui::Color32::YELLOW),
                            };
                            ui.colored_label(color, result_text);
                        });
                        row.col(|ui| {
                            ui.label(&replay.stage_name);
                        });
                        row.col(|ui| {
                            let date_text = if let Some(date) = replay.date {
                                format_date(date)
                            } else {
                                "Unknown".to_string()
                            };
                            ui.label(date_text);
                        });
                        row.col(|ui| {
                            let duration_text = if let Some(duration_frames) = replay.duration {
                                format_duration(duration_frames)
                            } else {
                                "Unknown".to_string()
                            };
                            ui.label(duration_text);
                        });
                        row.col(|ui| {
                            // Show opponent rank based on who the user is
                            let opponent_name = if !connect_code.is_empty() {
                                if replay.player1.name == *connect_code {
                                    &replay.player2.name
                                } else if replay.player2.name == *connect_code {
                                    &replay.player1.name
                                } else {
                                    "N/A"
                                }
                            } else {
                                "N/A"
                            };

                            let rank_text = if opponent_name != "N/A" {
                                // Check if this is the most recent replay and if rank lookup was performed
                                if row_index == 0 {
                                    replay.opponent_rank.as_deref().unwrap_or("Unranked")
                                } else {
                                    "Unranked"
                                }
                            } else {
                                "N/A"
                            };

                            // Display icon and rank text horizontally
                            ui.horizontal(|ui| {
                                // Show rank icon if available
                                if let Some(icon_texture) = self.rank_icons.get(rank_text) {
                                    ui.add(egui::Image::from_texture(icon_texture).max_size(egui::Vec2::new(20.0, 20.0)));
                                }
                                ui.label(rank_text);
                            });
                        });

                        if row.response().clicked() {
                            rows_to_toggle.push(row_index);
                        }
                    });
                }

                // Handle row selection after the iteration
                for row_index in rows_to_toggle {
                    if self.selection.contains(&row_index) {
                        self.selection.remove(&row_index);
                    } else {
                        self.selection.insert(row_index);
                    }
                }
            });
    }

    // fn toggle_row_selection(&mut self, row_index: usize, row_response: &egui::Response) {
    //     if row_response.clicked() {
    //         if self.selection.contains(&row_index) {
    //             self.selection.remove(&row_index);
    //         } else {
    //             self.selection.insert(row_index);
    //         }
    //     }
    // }
}
