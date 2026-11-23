//! Prints the snapshot the Demo Viewer would show for a demo: its tick, the caption time, the
//! team counts and how long the parse took.
//! Usage: cargo run --release -p tf2tools-core --example demo_snapshot_probe -- <file.dem>

use std::io::Read as _;
use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Instant;

use tf2tools_core::{Team, read_demo_header, read_demo_snapshot};

fn main() -> ExitCode {
    let Some(path) = std::env::args_os().nth(1).map(PathBuf::from) else {
        eprintln!("usage: demo_snapshot_probe <file.dem>");
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
    let snapshot = read_demo_snapshot(&path);
    let elapsed = started.elapsed().as_secs_f64();
    // The page's rule: duration over ticks, or 0.015 s for an unfinalised demo.
    let per_tick = match header.ticks() {
        0 => 0.015,
        ticks => header.duration_seconds() / ticks as f32,
    };
    let snapshot = match snapshot {
        Ok(snapshot) => snapshot,
        Err(error) => {
            println!("map={} error={error} parse={elapsed:.3}s", header.map());
            return ExitCode::FAILURE;
        }
    };
    let seconds = (snapshot.tick() as f32 * per_tick).floor() as u32;
    let red = snapshot
        .players()
        .iter()
        .filter(|p| p.team() == Team::Red)
        .count();
    println!(
        "map={} tick={} time={}:{:02} red={red} blu={} parse={elapsed:.3}s",
        header.map(),
        snapshot.tick(),
        seconds / 60,
        seconds % 60,
        snapshot.players().len() - red
    );
    for p in snapshot.players() {
        println!(
            "{:?} {:?} feet={:?} yaw={:.1} pitch={:.1} eye={:.1} ducked={}",
            p.team(),
            p.class(),
            p.feet(),
            p.yaw(),
            p.pitch(),
            p.eye_height(),
            p.ducked()
        );
    }
    ExitCode::SUCCESS
}
