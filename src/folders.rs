//! Replay folders as a set: one spelling per folder, and no folder inside
//! another (its replays are read with the outer one already).

use std::path::{Path, PathBuf};

/// `path` absolute and resolved (junctions, `..`), without Windows' `\\?\`
/// prefix; as given when it doesn't exist (an unplugged drive).
pub fn normalize(path: &Path) -> PathBuf {
    match std::fs::canonicalize(path) {
        Ok(real) => {
            let text = real.to_string_lossy();
            match text.strip_prefix(r"\\?\") {
                Some(rest) if !rest.starts_with("UNC\\") => PathBuf::from(rest),
                Some(rest) => PathBuf::from(format!(r"\\{}", &rest[4..])),
                None => real,
            }
        }
        Err(_) => path.to_path_buf(),
    }
}

/// How a path compares: one separator, no trailing one, and on Windows
/// case-insensitive.
fn key(path: &Path) -> String {
    let mut text = path.to_string_lossy().replace('\\', "/");
    while text.len() > 1 && text.ends_with('/') {
        text.pop();
    }
    if cfg!(windows) {
        text = text.to_lowercase();
    }
    text
}

/// Whether `inner` is `outer` or inside it.
pub fn covers(outer: &Path, inner: &Path) -> bool {
    let (outer, inner) = (key(outer), key(inner));
    inner == outer || inner.starts_with(&format!("{outer}/"))
}

/// The folders to read: each once, and none that another one covers (the
/// outer folder stays). Keeps the order they were given in.
pub fn tidy(folders: &[PathBuf]) -> Vec<PathBuf> {
    let folders: Vec<PathBuf> = folders.iter().map(|f| normalize(f)).collect();
    let mut out: Vec<PathBuf> = Vec::new();
    for (i, folder) in folders.iter().enumerate() {
        let covered = folders.iter().enumerate().any(|(j, other)| {
            j != i && covers(other, folder) && (key(other) != key(folder) || j < i)
        });
        if !covered {
            out.push(folder.clone());
        }
    }
    out
}

/// What adding a folder did.
#[derive(Debug, PartialEq)]
pub enum Added {
    /// Added; these folders inside it were taken out (it covers them now).
    Added { replaced: Vec<PathBuf> },
    /// Not added: this folder already covers it.
    AlreadyCovered(PathBuf),
}

pub fn add(folders: &mut Vec<PathBuf>, folder: &Path) -> Added {
    let folder = normalize(folder);
    if let Some(outer) = folders.iter().find(|f| covers(f, &folder)) {
        return Added::AlreadyCovered(outer.clone());
    }
    let (replaced, kept): (Vec<PathBuf>, Vec<PathBuf>) =
        folders.drain(..).partition(|f| covers(&folder, f));
    *folders = kept;
    folders.push(folder);
    Added::Added { replaced }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(s: &str) -> PathBuf {
        PathBuf::from(s)
    }

    #[test]
    fn a_folder_covers_itself_and_what_is_inside_it() {
        assert!(covers(&p("C:/Slippi"), &p("C:/Slippi")));
        assert!(covers(&p("C:/Slippi"), &p("C:/Slippi/2025-05")));
        assert!(covers(&p("C:/Slippi/"), &p(r"C:\Slippi\Spectate")));
        assert!(
            !covers(&p("C:/Slippi"), &p("C:/Slippi Old")),
            "a prefix isn't a parent"
        );
        assert!(!covers(&p("C:/Slippi/2025"), &p("C:/Slippi")));
        if cfg!(windows) {
            assert!(covers(&p(r"c:\slippi"), &p(r"C:\Slippi\x")));
        }
    }

    #[test]
    fn tidying_drops_repeats_and_nested_folders() {
        let folders = [
            p("X:/none/Replays/2025"),
            p("X:/none/Replays"),
            p("X:/none/Backup"),
            p("X:/none/Replays/"),
        ];
        assert_eq!(
            tidy(&folders),
            vec![p("X:/none/Replays"), p("X:/none/Backup")]
        );
    }

    #[test]
    fn adding_a_parent_replaces_its_children_and_a_child_is_refused() {
        let mut folders = vec![p("X:/none/a/1"), p("X:/none/b"), p("X:/none/a/2")];
        assert_eq!(
            add(&mut folders, &p("X:/none/a")),
            Added::Added {
                replaced: vec![p("X:/none/a/1"), p("X:/none/a/2")]
            }
        );
        assert_eq!(folders, vec![p("X:/none/b"), p("X:/none/a")]);
        assert_eq!(
            add(&mut folders, &p("X:/none/a/3")),
            Added::AlreadyCovered(p("X:/none/a"))
        );
        assert_eq!(folders.len(), 2);
    }
}
