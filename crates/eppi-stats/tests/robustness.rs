//! Feeds damaged copies of a real replay through `read_replay_bytes` + `compute`.
//!
//! Set `EPPI_STATS_TEST_REPLAY` to a .slp path to run it (replays aren't checked in); without
//! it the test passes trivially. The replay is only read.

use std::panic;
use std::sync::{Arc, Mutex};

#[test]
fn damaged_replays_never_panic_in_compute() {
    let Ok(path) = std::env::var("EPPI_STATS_TEST_REPLAY") else {
        eprintln!("EPPI_STATS_TEST_REPLAY not set; skipping");
        return;
    };
    let data = std::fs::read(path).expect("read replay");
    let full = eppi_stats::read_replay_bytes(&data).expect("intact replay parses");
    let full_stats = eppi_stats::compute(&full);
    assert!(full_stats.game_complete);

    // peppi panics are caught by read_replay_bytes and compute catches its own, so record where
    // panics happen: none may come from this crate.
    let ours: Arc<Mutex<Vec<String>>> = Arc::default();
    let sink = Arc::clone(&ours);
    panic::set_hook(Box::new(move |info| {
        if let Some(loc) = info.location()
            && loc.file().replace('\\', "/").contains("eppi-stats/src")
        {
            sink.lock().unwrap().push(format!("{loc}: {info}"));
        }
    }));
    let mut parsed_truncations = 0;
    let step = (data.len() / 300).max(1);
    for cut in (0..data.len()).step_by(step) {
        if let Ok(game) = eppi_stats::read_replay_bytes(&data[..cut]) {
            parsed_truncations += 1;
            let stats = eppi_stats::compute(&game);
            assert!(stats.last_frame <= full_stats.last_frame);
        }
    }
    // Byte flips inside the frame data.
    let mut flipped_parsed = 0;
    for i in 0..200 {
        let mut copy = data.clone();
        let at = 1000 + (i * 7919 * 13) % (data.len() - 2000);
        copy[at] ^= 0xa5;
        if let Ok(game) = eppi_stats::read_replay_bytes(&copy) {
            flipped_parsed += 1;
            let _ = eppi_stats::compute(&game);
        }
    }
    let _ = panic::take_hook();
    let ours = ours.lock().unwrap();
    assert!(ours.is_empty(), "eppi-stats panicked: {ours:#?}");
    eprintln!("truncations parsed: {parsed_truncations}, byte flips parsed: {flipped_parsed}");
    // Most cut points land after the game start, so the truncation repair should recover them.
    assert!(parsed_truncations > 250);
}
