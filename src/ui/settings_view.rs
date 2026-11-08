//! Settings: whose games are yours, where the replays are, which Dolphin
//! plays them, and the rest.

use crate::app::Data;
use crate::model::normalize_code;
use crate::settings::{Launcher, Settings};
use crate::ui::theme::{self, Ion};
use crate::ui::widgets;
use eframe::egui::{self, Margin, RichText};
use std::path::PathBuf;

#[derive(Default)]
pub struct State {
    new_code: String,
    /// What the last folders added came to ("already covered by …").
    folder_notes: Vec<String>,
}

impl State {
    /// Returns whether codes or folders changed (the app re-reads them).
    pub fn show(&mut self, ui: &mut egui::Ui, settings: &mut Settings, data: &Data) -> bool {
        let mut changed = false;
        egui::ScrollArea::vertical()
            .id_salt("settings")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                egui::Frame::new()
                    .inner_margin(Margin::symmetric(24, 18))
                    .show(ui, |ui| {
                        ui.set_max_width(720.0);
                        changed |= self.codes(ui, settings);
                        ui.add_space(theme::space::XL);
                        changed |= self.folders(ui, settings, data);
                        ui.add_space(theme::space::XL);
                        playback(ui, settings);
                        ui.add_space(theme::space::XL);
                        behaviour(ui, settings);
                        ui.add_space(theme::space::XL);
                        about(ui);
                    });
            });
        changed
    }

    fn codes(&mut self, ui: &mut egui::Ui, settings: &mut Settings) -> bool {
        let mut changed = false;
        ui.label(theme::heavy("Your connect codes", theme::text::HEADING));
        ui.label(theme::muted(
            "Games played with any of these count as yours: add alts and old codes.",
        ));
        ui.add_space(theme::space::S);
        let mut remove = None;
        for (i, code) in settings.codes.iter().enumerate() {
            ui.horizontal(|ui| {
                ui.label(theme::heavy(code, theme::text::LIST));
                if ui
                    .add(egui::Button::new(Ion::CLOSE.rich(14.0).color(theme::MUTED)).frame(false))
                    .on_hover_text("Remove")
                    .clicked()
                {
                    remove = Some(i);
                }
            });
        }
        if let Some(i) = remove {
            settings.codes.remove(i);
            changed = true;
        }
        ui.horizontal(|ui| {
            let response = ui.add(
                egui::TextEdit::singleline(&mut self.new_code)
                    .hint_text("ABCD#123")
                    .desired_width(160.0),
            );
            let code = normalize_code(&self.new_code);
            let valid = code.contains('#') && code.len() >= 3;
            let enter = response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            if (ui.add_enabled(valid, egui::Button::new("Add")).clicked() || (enter && valid))
                && !settings.codes.contains(&code)
            {
                settings.codes.push(code);
                self.new_code.clear();
                changed = true;
            }
        });
        changed
    }
}

/// Adds `picked` folders; returns what to tell the user about them.
fn add_folders(settings: &mut Settings, picked: Vec<PathBuf>) -> (bool, Vec<String>) {
    let mut changed = false;
    let mut notes = Vec::new();
    for folder in picked {
        match crate::folders::add(&mut settings.folders, &folder) {
            crate::folders::Added::Added { replaced } => {
                changed = true;
                if !replaced.is_empty() {
                    notes.push(format!(
                        "{} covers {} you had, so it took their place.",
                        folder.display(),
                        if replaced.len() == 1 {
                            replaced[0].display().to_string()
                        } else {
                            format!("{} folders", replaced.len())
                        },
                    ));
                }
            }
            crate::folders::Added::AlreadyCovered(outer) => notes.push(format!(
                "{} is already read as part of {}.",
                folder.display(),
                outer.display()
            )),
        }
    }
    (changed, notes)
}

impl State {
    fn folders(&mut self, ui: &mut egui::Ui, settings: &mut Settings, data: &Data) -> bool {
        let mut changed = false;
        ui.label(theme::heavy("Replay folders", theme::text::HEADING));
        ui.label(theme::muted(
            "Eppi reads every .slp in these folders and their subfolders, and watches them for new games. A replay copied into more than one place counts once.",
        ));
        ui.add_space(theme::space::S);
        let mut remove = None;
        for (i, folder) in settings.folders.iter().enumerate() {
            ui.horizontal(|ui| {
                ui.label(Ion::FOLDER.rich(16.0).color(theme::MUTED));
                let exists = folder.is_dir();
                ui.label(
                    RichText::new(folder.display().to_string()).color(if exists {
                        theme::TEXT
                    } else {
                        theme::WARNING
                    }),
                );
                if !exists {
                    ui.label(theme::muted("(not found: its games stay until it's back)"));
                } else if let Some((_, count)) = data.folder_counts.iter().find(|(f, _)| {
                    crate::folders::covers(f, folder) && crate::folders::covers(folder, f)
                }) {
                    ui.label(
                        theme::muted(format!("{} replays", widgets::thousands(*count)))
                            .size(theme::text::SMALL),
                    );
                }
                if ui
                    .add(egui::Button::new(Ion::CLOSE.rich(14.0).color(theme::MUTED)).frame(false))
                    .on_hover_text("Stop reading this folder")
                    .clicked()
                {
                    remove = Some(i);
                }
            });
        }
        if let Some(i) = remove {
            settings.folders.remove(i);
            self.folder_notes.clear();
            changed = true;
        }
        ui.horizontal(|ui| {
            if ui
                .add(theme::icon_button(Ion::ADD, "Add folders…"))
                .on_hover_text("Pick one or more folders (hold Ctrl to pick several)")
                .clicked()
                && let Some(picked) = rfd::FileDialog::new()
                    .set_title("Folders with replays")
                    .pick_folders()
            {
                let (added, notes) = add_folders(settings, picked);
                changed |= added;
                self.folder_notes = notes;
            }
            let launcher = Launcher::detect();
            let missing: Vec<PathBuf> = launcher
                .replay_folders
                .into_iter()
                .filter(|f| {
                    !settings
                        .folders
                        .iter()
                        .any(|mine| crate::folders::covers(mine, f))
                })
                .collect();
            if !missing.is_empty()
                && ui
                    .button("Add Slippi Launcher's folders")
                    .on_hover_text(
                        missing
                            .iter()
                            .map(|p| p.display().to_string())
                            .collect::<Vec<_>>()
                            .join(
                                "
",
                            ),
                    )
                    .clicked()
            {
                let (added, notes) = add_folders(settings, missing);
                changed |= added;
                self.folder_notes = notes;
            }
        });
        for note in &self.folder_notes {
            ui.label(
                RichText::new(note)
                    .color(theme::WARNING)
                    .size(theme::text::SMALL),
            );
        }
        let p = data.progress;
        let mut line = format!("{} games indexed", widgets::thousands(data.games.len()));
        if p.duplicates > 0 {
            line.push_str(&format!(
                " · {} duplicate cop{} skipped",
                widgets::thousands(p.duplicates),
                if p.duplicates == 1 { "y" } else { "ies" }
            ));
        }
        if p.unreadable > 0 {
            line.push_str(&format!(" · {} files couldn't be read", p.unreadable));
        }
        ui.label(theme::muted(line).size(theme::text::SMALL));
        changed
    }
}

fn path_row(
    ui: &mut egui::Ui,
    label: &str,
    value: &mut Option<PathBuf>,
    detected: Option<PathBuf>,
    filter: (&str, &[&str]),
) {
    ui.label(theme::caption(label));
    ui.horizontal(|ui| {
        let shown = value.clone().or(detected.clone());
        match &shown {
            Some(path) => {
                ui.label(path.display().to_string());
                if value.is_none() {
                    ui.label(theme::muted("(from Slippi Launcher)"));
                }
            }
            None => {
                ui.label(RichText::new("Not found").color(theme::WARNING));
            }
        }
    });
    ui.horizontal(|ui| {
        if ui.button("Choose…").clicked()
            && let Some(file) = rfd::FileDialog::new()
                .add_filter(filter.0, filter.1)
                .pick_file()
        {
            *value = Some(file);
        }
        if value.is_some() && ui.button("Use Slippi Launcher's").clicked() {
            *value = None;
        }
    });
    ui.add_space(theme::space::S);
}

fn playback(ui: &mut egui::Ui, settings: &mut Settings) {
    ui.label(theme::heavy("Playback", theme::text::HEADING));
    ui.label(theme::muted(
        "Play opens replays in Slippi's playback Dolphin with your Melee ISO.",
    ));
    ui.add_space(theme::space::S);
    let launcher = Launcher::detect();
    let exe: &[&str] = if cfg!(windows) { &["exe"] } else { &["*"] };
    path_row(
        ui,
        "Playback Dolphin",
        &mut settings.dolphin,
        launcher.playback_dolphin,
        ("Dolphin", exe),
    );
    path_row(
        ui,
        "Melee ISO",
        &mut settings.iso,
        launcher.iso,
        ("Melee ISO", &["iso", "gcm", "ciso", "rvz", "gcz"]),
    );
}

fn behaviour(ui: &mut egui::Ui, settings: &mut Settings) {
    ui.label(theme::heavy(
        "Ranks and notifications",
        theme::text::HEADING,
    ));
    ui.add_space(theme::space::S);
    ui.checkbox(&mut settings.fetch_ranks, "Look up opponents' ranks")
        .on_hover_text("From Slippi's servers, at most once a second and once a day per player");
    ui.checkbox(
        &mut settings.notify_live,
        "Tell me who I'm playing when a game starts and Eppi is in the background",
    );
}

fn about(ui: &mut egui::Ui) {
    ui.label(theme::heavy("About", theme::text::HEADING));
    ui.label(theme::muted(format!("Eppi {}", env!("CARGO_PKG_VERSION"))));
    let data = crate::settings::data_dir();
    ui.horizontal(|ui| {
        ui.label(theme::muted(format!(
            "Index and settings: {}",
            data.display()
        )));
        if ui.small_button("Open").clicked() {
            let _ = open::that(&data);
        }
    });
    ui.label(
        theme::muted("Ranks come from Slippi's servers; replays are read with peppi; art is Slippi Launcher's.")
            .size(theme::text::SMALL),
    );
}
