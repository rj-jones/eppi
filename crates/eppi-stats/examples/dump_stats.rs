//! Writes eppi-stats JSON for replays, for comparison with slippi-js (see tools/verify-stats).
//!
//! Usage: `dump_stats <list-file> <out-dir>`, where the list file has one .slp path per line.
//! Writes `<out-dir>/<basename>.json` per replay and prints timing to stdout.

use std::path::Path;
use std::time::{Duration, Instant};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 3 {
        eprintln!("usage: dump_stats <list-file> <out-dir>");
        std::process::exit(2);
    }
    let list = std::fs::read_to_string(&args[1]).expect("read list file");
    let out_dir = Path::new(&args[2]);
    std::fs::create_dir_all(out_dir).expect("create out dir");

    let (mut read_time, mut compute_time) = (Duration::ZERO, Duration::ZERO);
    let (mut ok, mut failed) = (0usize, 0usize);
    for line in list.lines().map(str::trim).filter(|l| !l.is_empty()) {
        let path = Path::new(line);
        let t0 = Instant::now();
        let game = eppi_stats::read_replay(path);
        let t1 = Instant::now();
        read_time += t1 - t0;
        let json = match game {
            Ok(game) => {
                let stats = eppi_stats::compute(&game);
                compute_time += t1.elapsed();
                ok += 1;
                serde_json::json!({ "file": line, "stats": stats })
            }
            Err(e) => {
                failed += 1;
                serde_json::json!({ "file": line, "error": e.to_string() })
            }
        };
        let name = path
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned()
            + ".json";
        std::fs::write(
            out_dir.join(name),
            serde_json::to_vec(&json).expect("serialize"),
        )
        .expect("write output");
    }
    let n = (ok + failed).max(1) as f64;
    println!(
        "{ok} ok, {failed} failed; read {:.2} ms/file, compute {:.2} ms/file",
        read_time.as_secs_f64() * 1000.0 / n,
        compute_time.as_secs_f64() * 1000.0 / ok.max(1) as f64,
    );
}
