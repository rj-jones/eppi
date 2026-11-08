//! Settings: whose games are yours, where the replays are, which Dolphin
//! plays them, and the rest.

use crate::app::Data;
use crate::model::normalize_code;
use crate::settings::{Launcher, Settings};
use crate::ui::theme::{self, Ion};
use eframe::egui::{self, Margin, RichText};
use std::path::PathBuf;

#[derive(Default)]
pub struct State {
    new_code: String,
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
                        changed |= folders(ui, settings, data);
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

fn folders(ui: &mut egui::Ui, settings: &mut Settings, data: &Data) -> bool {
    let mut changed = false;
    ui.label(theme::heavy("Replay folders", theme::text::HEADING));
    ui.label(theme::muted("Eppi reads every .slp in these folders and their subfolders, and watches them for new games."));
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
                ui.label(theme::muted("(not found)"));
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
        changed = true;
    }
    ui.horizontal(|ui| {
        if ui
            .add(theme::icon_button(Ion::ADD, "Add folder…"))
            .clicked()
            && let Some(folder) = rfd::FileDialog::new()
                .set_title("A folder with replays")
                .pick_folder()
            && !settings.folders.contains(&folder)
        {
            settings.folders.push(folder);
            changed = true;
        }
        let launcher = Launcher::detect();
        let missing: Vec<PathBuf> = launcher
            .replay_folders
            .into_iter()
            .filter(|f| !settings.folders.contains(f))
            .collect();
        if !missing.is_empty()
            && ui
                .button("Add Slippi Launcher's folders")
                .on_hover_text(
                    missing
                        .iter()
                        .map(|p| p.display().to_string())
                        .collect::<Vec<_>>()
                        .join("\n"),
                )
                .clicked()
        {
            settings.folders.extend(missing);
            changed = true;
        }
    });
    ui.label(
        theme::muted(format!(
            "{} games indexed{}",
            crate::ui::widgets::thousands(data.games.len()),
            if data.progress.unreadable > 0 {
                format!(", {} files couldn't be read", data.progress.unreadable)
            } else {
                String::new()
            }
        ))
        .size(theme::text::SMALL),
    );
    changed
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
