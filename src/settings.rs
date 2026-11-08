//! What the user sets: whose games are theirs, where the replays are, and
//! which Dolphin and ISO play them. Seeded from Slippi Launcher's own files
//! the first time Eppi starts.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// Connect codes that count as me.
    pub codes: Vec<String>,
    /// Folders scanned for replays (with their subfolders).
    pub folders: Vec<PathBuf>,
    /// Playback Dolphin; `None` uses the one Slippi Launcher installed.
    pub dolphin: Option<PathBuf>,
    /// The Melee ISO; `None` uses the one Slippi Launcher knows.
    pub iso: Option<PathBuf>,
    /// Look up opponents' ranks as they come into view.
    pub fetch_ranks: bool,
    /// Show a desktop notification when a game starts while Eppi is in the
    /// background.
    pub notify_live: bool,
    /// Whether the settings were seeded already.
    pub seeded: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            codes: Vec::new(),
            folders: Vec::new(),
            dolphin: None,
            iso: None,
            fetch_ranks: true,
            notify_live: true,
            seeded: false,
        }
    }
}

impl Settings {
    /// Fills empty settings from Slippi Launcher (once).
    pub fn seed_from_launcher(&mut self) {
        if self.seeded {
            return;
        }
        self.seeded = true;
        let launcher = Launcher::detect();
        if self.codes.is_empty()
            && let Some(code) = launcher.connect_code.clone()
        {
            self.codes.push(code);
        }
        if self.folders.is_empty() {
            self.folders = launcher.replay_folders.clone();
        }
    }

    pub fn dolphin_path(&self) -> Option<PathBuf> {
        self.dolphin
            .clone()
            .filter(|p| p.is_file())
            .or_else(|| Launcher::detect().playback_dolphin)
    }

    pub fn iso_path(&self) -> Option<PathBuf> {
        self.iso
            .clone()
            .filter(|p| p.is_file())
            .or_else(|| Launcher::detect().iso)
    }
}

/// What Slippi Launcher has on this machine.
#[derive(Debug, Clone, Default)]
pub struct Launcher {
    pub connect_code: Option<String>,
    pub replay_folders: Vec<PathBuf>,
    pub playback_dolphin: Option<PathBuf>,
    pub iso: Option<PathBuf>,
}

impl Launcher {
    /// Electron's `userData`: `%APPDATA%\Slippi Launcher` on Windows.
    pub fn data_dir() -> Option<PathBuf> {
        Some(dirs::config_dir()?.join("Slippi Launcher"))
    }

    pub fn detect() -> Self {
        let Some(dir) = Self::data_dir() else {
            return Self::default();
        };
        let settings: serde_json::Value = std::fs::read_to_string(dir.join("Settings"))
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default();
        let settings = &settings["settings"];
        let mut replay_folders = Vec::new();
        match settings["rootSlpPath"].as_str() {
            Some(root) => replay_folders.push(PathBuf::from(root)),
            None => {
                if let Some(documents) = dirs::document_dir() {
                    replay_folders.push(documents.join("Slippi"));
                }
            }
        }
        if let Some(extra) = settings["extraSlpPaths"].as_array() {
            replay_folders.extend(extra.iter().filter_map(|p| p.as_str()).map(PathBuf::from));
        }
        replay_folders.retain(|p| p.is_dir());

        let iso = settings["isoPath"]
            .as_str()
            .map(PathBuf::from)
            .filter(|p| p.is_file());

        let exe = if cfg!(windows) {
            "Slippi Dolphin.exe"
        } else if cfg!(target_os = "macos") {
            "Slippi Dolphin.app/Contents/MacOS/Slippi Dolphin"
        } else {
            "Slippi_Playback-x86_64.AppImage"
        };
        let playback_dolphin = Some(dir.join("playback").join(exe)).filter(|p| p.exists());

        let connect_code = read_connect_code(&dir.join("netplay/User/Slippi/user.json"));

        Self {
            connect_code,
            replay_folders,
            playback_dolphin,
            iso,
        }
    }
}

fn read_connect_code(path: &Path) -> Option<String> {
    let text = std::fs::read_to_string(path).ok()?;
    let value: serde_json::Value = serde_json::from_str(&text).ok()?;
    value["connectCode"]
        .as_str()
        .map(crate::model::normalize_code)
        .filter(|c| !c.is_empty())
}

/// Where Eppi keeps its database.
pub fn data_dir() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("Eppi")
}
