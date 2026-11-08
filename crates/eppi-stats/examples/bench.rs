//! Throughput of read + compute over many replays, single-threaded and on rayon.
//!
//! Usage: `bench <list-file> [limit]` (one .slp path per line). Replays are only read.

use std::path::Path;
use std::time::Instant;

use rayon::prelude::*;

fn run(path: &str) -> (u64, bool) {
    let bytes = std::fs::metadata(path).map_or(0, |m| m.len());
    match eppi_stats::read_replay(Path::new(path)) {
        Ok(game) => {
            let stats = eppi_stats::compute(&game);
            std::hint::black_box(&stats);
            (bytes, true)
        }
        Err(_) => (bytes, false),
    }
}

fn report(label: &str, start: Instant, results: &[(u64, bool)]) {
    let secs = start.elapsed().as_secs_f64();
    let n = results.len();
    let bytes: u64 = results.iter().map(|r| r.0).sum();
    let failed = results.iter().filter(|r| !r.1).count();
    println!(
        "{label}: {n} replays ({:.1} GB) in {secs:.1} s = {:.2} ms/replay wall, {:.0} MB/s, {failed} failed",
        bytes as f64 / 1e9,
        secs * 1000.0 / n.max(1) as f64,
        bytes as f64 / 1e6 / secs,
    );
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let list = std::fs::read_to_string(args.get(1).expect("usage: bench <list-file> [limit]"))
        .expect("read list");
    let limit = args
        .get(2)
        .and_then(|s| s.parse().ok())
        .unwrap_or(usize::MAX);
    let paths: Vec<&str> = list
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .take(limit)
        .collect();

    let start = Instant::now();
    let results: Vec<_> = paths.iter().map(|p| run(p)).collect();
    report("1 thread", start, &results);

    let start = Instant::now();
    let results: Vec<_> = paths.par_iter().map(|p| run(p)).collect();
    report(
        &format!("rayon ({} threads)", rayon::current_num_threads()),
        start,
        &results,
    );
}
