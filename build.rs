//! Embeds the art in `assets/` (stock icons per costume, stage images, rank
//! icons) so the app is one file: generates `assets.rs` with a lookup per
//! kind.

use std::fmt::Write as _;
use std::path::Path;

fn main() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("assets");
    println!("cargo:rerun-if-changed=assets");
    let mut out = String::new();

    // characters/<id>/<costume>/stock.png
    out.push_str("/// The stock icon of `character` in `costume` (PNG).\npub fn stock(character: u8, costume: u8) -> Option<(&'static str, &'static [u8])> {\n    match (character, costume) {\n");
    let mut characters: Vec<_> = read_dir(&root.join("characters"));
    characters.sort();
    for character in characters {
        let Ok(id) = character.parse::<u8>() else {
            continue;
        };
        let mut costumes = read_dir(&root.join("characters").join(&character));
        costumes.sort();
        for costume in costumes {
            let Ok(c) = costume.parse::<u8>() else {
                continue;
            };
            let file = root
                .join("characters")
                .join(&character)
                .join(&costume)
                .join("stock.png");
            if file.is_file() {
                writeln!(
                    out,
                    "        ({id}, {c}) => Some((\"bytes://stock/{id}/{c}.png\", include_bytes!({:?}))),",
                    file.display().to_string()
                )
                .unwrap();
            }
        }
    }
    out.push_str("        _ => None,\n    }\n}\n\n");

    // stages/<id>.png
    out.push_str("/// The picture of stage `id` (PNG).\npub fn stage(id: u16) -> Option<(&'static str, &'static [u8])> {\n    match id {\n");
    let mut stages = read_files(&root.join("stages"));
    stages.sort();
    for stage in stages {
        let Some(id) = stage
            .strip_suffix(".png")
            .and_then(|s| s.parse::<u16>().ok())
        else {
            continue;
        };
        writeln!(
            out,
            "        {id} => Some((\"bytes://stage/{id}.png\", include_bytes!({:?}))),",
            root.join("stages").join(&stage).display().to_string()
        )
        .unwrap();
    }
    out.push_str("        _ => None,\n    }\n}\n\n");

    // rank-icons/<NAME>.svg
    out.push_str("/// The rank icon whose file is `name` (without `.svg`, e.g. `DIAMOND 2`).\npub fn rank_icon(name: &str) -> Option<(&'static str, &'static [u8])> {\n    match name {\n");
    let mut ranks = read_files(&root.join("rank-icons"));
    ranks.sort();
    for rank in ranks {
        let Some(name) = rank.strip_suffix(".svg") else {
            continue;
        };
        writeln!(
            out,
            "        {name:?} => Some(({:?}, include_bytes!({:?}))),",
            format!("bytes://rank/{}.svg", name.replace(' ', "_")),
            root.join("rank-icons").join(&rank).display().to_string()
        )
        .unwrap();
    }
    out.push_str("        _ => None,\n    }\n}\n");

    let dest = Path::new(&std::env::var("OUT_DIR").unwrap()).join("assets.rs");
    std::fs::write(dest, out).unwrap();

    windows_icon();
}

fn read_dir(path: &Path) -> Vec<String> {
    std::fs::read_dir(path)
        .map(|entries| {
            entries
                .flatten()
                .filter(|e| e.path().is_dir())
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .collect()
        })
        .unwrap_or_default()
}

fn read_files(path: &Path) -> Vec<String> {
    std::fs::read_dir(path)
        .map(|entries| {
            entries
                .flatten()
                .filter(|e| e.path().is_file())
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .collect()
        })
        .unwrap_or_default()
}

/// Builds `assets/favicon.ico` into the Windows program, so Explorer, the
/// Start menu and Windows Search show it (the window sets its own icon at
/// run time).
fn windows_icon() {
    println!("cargo:rerun-if-changed=assets/favicon.ico");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let icon = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("assets")
        .join("favicon.ico");
    let out = Path::new(&std::env::var("OUT_DIR").unwrap()).join("icon.rc");
    // rc.exe wants the icon's absolute path, backslashes doubled.
    let path = icon.display().to_string().replace('\\', "\\\\");
    std::fs::write(&out, format!("1 ICON \"{path}\"\n")).unwrap();
    embed_resource::compile(&out, embed_resource::NONE)
        .manifest_optional()
        .expect("compile the icon resource with rc.exe");
}
