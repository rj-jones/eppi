//! Eppi: a fast desktop app for your Slippi replays.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod demo;
mod dolphin;
mod folders;
mod library;
mod melee;
mod model;
mod notify;
mod ranks;
mod screenshot;
mod settings;
mod stats_view;
mod time;
mod ui;

fn main() -> eframe::Result {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        // peppi logs every replay's metadata at info.
        .filter_module("peppi", log::LevelFilter::Warn)
        .init();

    // peppi asserts on some malformed replays; the library catches those
    // and marks the file unreadable, so its worker threads' panics go to
    // the log instead of stderr.
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let worker = std::thread::current()
            .name()
            .is_some_and(|n| n.starts_with("eppi-read") || n == "eppi-library");
        if worker {
            log::debug!("replay parser gave up: {info}");
        } else {
            default_hook(info);
        }
    }));

    let mut args = std::env::args().skip(1);
    let mut tour = None;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--screenshots" => {
                let folder = args.next().unwrap_or_else(|| "screenshots".into());
                tour = Some(screenshot::Tour::new(folder.into()));
            }
            "--showcase" => {
                let folder = args.next().unwrap_or_else(|| "docs/screenshots".into());
                tour = Some(screenshot::Tour::showcase(folder.into()));
            }
            "--version" => {
                println!("eppi {}", env!("CARGO_PKG_VERSION"));
                return Ok(());
            }
            other => eprintln!("eppi: ignoring unknown argument {other}"),
        }
    }

    let icon = eframe::icon_data::from_png_bytes(include_bytes!("../assets/icon-256.png")).ok();
    let mut viewport = eframe::egui::ViewportBuilder::default()
        .with_title("Eppi")
        .with_app_id("eppi")
        .with_inner_size(tour.as_ref().map_or([1360.0, 860.0], |t| t.window_size()))
        .with_min_inner_size([960.0, 600.0]);
    if let Some(icon) = icon {
        viewport = viewport.with_icon(icon);
    }
    let options = eframe::NativeOptions {
        viewport,
        persist_window: tour.is_none(),
        ..Default::default()
    };
    eframe::run_native(
        "Eppi",
        options,
        Box::new(move |cc| Ok(Box::new(app::EppiApp::new(cc, tour)))),
    )
}
