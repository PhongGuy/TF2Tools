//! Prints what playback would hold for a demo: its players, runs and samples, the heap they
//! take, the share of repeated samples, the tick length, the round starts and any cut.
//! Usage: cargo run --release -p tf2tools-core --example demo_tracks_probe -- <file.dem>

use std::io::Read as _;
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::atomic::AtomicBool;
use std::time::Instant;

use tf2tools_core::{read_demo_header, read_demo_tracks};

fn main() -> ExitCode {
    let Some(path) = std::env::args_os().nth(1).map(PathBuf::from) else {
        eprintln!("usage: demo_tracks_probe <file.dem>");
        return ExitCode::from(2);
    };
    let mut head = Vec::new();
    if let Err(error) =
        std::fs::File::open(&path).and_then(|file| file.take(1072).read_to_end(&mut head))
    {
        eprintln!("cannot read {}: {error}", path.display());
        return ExitCode::FAILURE;
    }
    let header = match read_demo_header(&head) {
        Ok(header) => header,
        Err(error) => {
            eprintln!("not a usable demo: {error}");
            return ExitCode::FAILURE;
        }
    };
    let started = Instant::now();
    let tracks = read_demo_tracks(&path, &AtomicBool::new(false));
    let elapsed = started.elapsed().as_secs_f64();
    let tracks = match tracks {
        Ok(tracks) => tracks,
        Err(error) => {
            println!("map={} error={error} build={elapsed:.3}s", header.map());
            return ExitCode::FAILURE;
        }
    };
    let repeats = 100. * tracks.repeated_sample_count() as f64 / tracks.sample_count() as f64;
    println!(
        "map={} header_ticks={} first={} last={} players={} runs={} samples={} repeats={repeats:.1}% \
         heap_bytes={} build={elapsed:.3}s spt={} rounds={:?} cut={:?}",
        header.map(),
        header.ticks(),
        tracks.first_tick(),
        tracks.last_tick(),
        tracks.player_count(),
        tracks.run_count(),
        tracks.sample_count(),
        tracks.heap_bytes(),
        tracks.seconds_per_tick(),
        tracks.round_starts(),
        tracks.cut()
    );
    ExitCode::SUCCESS
}
