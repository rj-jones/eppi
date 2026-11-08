//! Playing replays in Slippi's playback Dolphin, the way Slippi Launcher
//! does: a JSON "comm" file says what to play (one replay from a frame, or a
//! queue), and Dolphin starts with `-i <comm file> -b -e <iso>`. While that
//! Dolphin stays open, writing the comm file again (with a new command id)
//! makes it play the new request.

use serde::Serialize;
use std::path::{Path, PathBuf};
use std::process::{Child, Command};

/// One replay to play, from and to a frame.
#[derive(Debug, Clone, PartialEq)]
pub struct Clip {
    pub path: PathBuf,
    pub start_frame: Option<i32>,
    pub end_frame: Option<i32>,
}

impl Clip {
    pub fn whole(path: &Path) -> Self {
        Self {
            path: path.to_owned(),
            start_frame: None,
            end_frame: None,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Comm<'a> {
    mode: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    replay: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    start_frame: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    end_frame: Option<i32>,
    command_id: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    queue: Vec<QueueItem>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct QueueItem {
    path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    start_frame: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    end_frame: Option<i32>,
}

/// The playback Dolphin Eppi started, kept so the next request reuses it.
#[derive(Default)]
pub struct Player {
    child: Option<Child>,
    comm_path: Option<PathBuf>,
}

impl Player {
    /// Plays `clips` (one, or a queue). Errors are for the user to read.
    pub fn play(
        &mut self,
        dolphin: Option<PathBuf>,
        iso: Option<PathBuf>,
        clips: &[Clip],
    ) -> Result<(), String> {
        if clips.is_empty() {
            return Ok(());
        }
        let dolphin = dolphin.ok_or(
            "Eppi couldn't find playback Dolphin. Open Slippi Launcher once to install it, or choose it in Settings.",
        )?;
        let comm = comm(clips, command_id());
        let running = self
            .child
            .as_mut()
            .is_some_and(|child| matches!(child.try_wait(), Ok(None)));
        let comm_path = match (&self.comm_path, running) {
            (Some(path), true) => path.clone(),
            _ => {
                let dir = crate::settings::data_dir().join("temp");
                std::fs::create_dir_all(&dir)
                    .map_err(|e| format!("Couldn't make {}: {e}", dir.display()))?;
                dir.join(format!("slippi-comms-{}.json", command_id()))
            }
        };
        let json = serde_json::to_string_pretty(&comm).map_err(|e| e.to_string())?;
        std::fs::write(&comm_path, json)
            .map_err(|e| format!("Couldn't write {}: {e}", comm_path.display()))?;
        self.comm_path = Some(comm_path.clone());
        if running {
            return Ok(());
        }
        let mut command = Command::new(&dolphin);
        command.arg("-i").arg(&comm_path);
        if let Some(iso) = iso {
            command.arg("-b").arg("-e").arg(iso);
        }
        if let Some(dir) = dolphin.parent() {
            command.current_dir(dir);
        }
        let child = command
            .spawn()
            .map_err(|e| format!("Couldn't start {}: {e}", dolphin.display()))?;
        self.child = Some(child);
        Ok(())
    }
}

impl Drop for Player {
    fn drop(&mut self) {
        // Dolphin stays open after Eppi closes; only the comm file goes.
        if let Some(path) = &self.comm_path
            && self
                .child
                .as_mut()
                .is_none_or(|c| !matches!(c.try_wait(), Ok(None)))
        {
            let _ = std::fs::remove_file(path);
        }
    }
}

/// What to tell Dolphin: one replay ("normal") or a queue.
fn comm(clips: &[Clip], command_id: String) -> Comm<'static> {
    match clips {
        [one] => Comm {
            mode: "normal",
            replay: Some(one.path.to_string_lossy().into_owned()),
            start_frame: one.start_frame,
            end_frame: one.end_frame,
            command_id,
            queue: Vec::new(),
        },
        _ => Comm {
            mode: "queue",
            replay: None,
            start_frame: None,
            end_frame: None,
            command_id,
            queue: clips
                .iter()
                .map(|c| QueueItem {
                    path: c.path.to_string_lossy().into_owned(),
                    start_frame: c.start_frame,
                    end_frame: c.end_frame,
                })
                .collect(),
        },
    }
}

fn command_id() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    format!("{:x}{:x}", nanos, std::process::id())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Opens a real replay in the playback Dolphin Slippi Launcher
    /// installed, checks it keeps running, and closes it again.
    #[test]
    #[ignore]
    fn playback_dolphin_starts_with_a_comm_file() {
        let launcher = crate::settings::Launcher::detect();
        let dolphin = launcher.playback_dolphin.expect("playback Dolphin");
        let replay = std::env::var("EPPI_REPLAY").expect("EPPI_REPLAY");
        let mut player = Player::default();
        player
            .play(
                Some(dolphin),
                launcher.iso,
                &[Clip::whole(Path::new(&replay))],
            )
            .unwrap();
        std::thread::sleep(std::time::Duration::from_secs(6));
        let child = player.child.as_mut().unwrap();
        assert!(matches!(child.try_wait(), Ok(None)), "Dolphin exited early");
        let comm = std::fs::read_to_string(player.comm_path.as_ref().unwrap()).unwrap();
        assert!(comm.contains("\"mode\": \"normal\""));
        child.kill().unwrap();
        let _ = child.wait();
    }

    #[test]
    fn comm_files_read_like_slippi_launchers() {
        let one = Clip {
            path: PathBuf::from("C:/r/a.slp"),
            start_frame: Some(100),
            end_frame: None,
        };
        let json = serde_json::to_value(comm(std::slice::from_ref(&one), "x".into())).unwrap();
        assert_eq!(
            json,
            serde_json::json!({"mode": "normal", "replay": "C:/r/a.slp", "startFrame": 100, "commandId": "x"})
        );
        let two = [one, Clip::whole(Path::new("C:/r/b.slp"))];
        let json = serde_json::to_value(comm(&two, "y".into())).unwrap();
        assert_eq!(
            json,
            serde_json::json!({"mode": "queue", "commandId": "y", "queue": [
                {"path": "C:/r/a.slp", "startFrame": 100},
                {"path": "C:/r/b.slp"}
            ]})
        );
    }
}
