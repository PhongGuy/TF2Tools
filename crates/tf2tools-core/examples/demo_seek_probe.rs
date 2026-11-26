//! Times how long the tracks take to place every player at one tick: random seeks over the
//! whole demo, then a 60 fps sweep from first to last, each against the 16.7 ms frame.
//! Usage: cargo run --release -p tf2tools-core --example demo_seek_probe -- <file.dem>

use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::atomic::AtomicBool;
use std::time::Instant;

use tf2tools_core::{DemoTracks, read_demo_tracks};

const SEEKS: usize = 10_000;
const FRAME_US: f64 = 1_000_000. / 60.;

fn main() -> ExitCode {
    let Some(path) = std::env::args_os().nth(1).map(PathBuf::from) else {
        eprintln!("usage: demo_seek_probe <file.dem>");
        return ExitCode::from(2);
    };
    let tracks = match read_demo_tracks(&path, &AtomicBool::new(false)) {
        Ok(tracks) => tracks,
        Err(error) => {
            eprintln!("no tracks: {error}");
            return ExitCode::FAILURE;
        }
    };
    let (first, last) = (tracks.first_tick(), tracks.last_tick());
    println!(
        "first={first} last={last} players={} samples={} spt={}",
        tracks.player_count(),
        tracks.sample_count(),
        tracks.seconds_per_tick()
    );

    // A fixed-seed LCG keeps the random ticks the same from run to run.
    let span = u64::from(last - first) + 1;
    let mut state = 0x2545_f491_4f6c_dd1d_u64;
    let random: Vec<f64> = (0..SEEKS)
        .map(|_| {
            state = state
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            f64::from(first) + ((state >> 33) % span) as f64
        })
        .collect();
    report("random", &tracks, &random);

    let step = 1. / 60. / f64::from(tracks.seconds_per_tick());
    let sweep: Vec<f64> = (0..)
        .map(|i| f64::from(first) + f64::from(i) * step)
        .take_while(|&tick| tick <= f64::from(last))
        .collect();
    report("sweep60", &tracks, &sweep);
    ExitCode::SUCCESS
}

// Times `at()` once per tick and prints the count, p50, p99 and max in microseconds, and the
// share of a 60 fps frame the p99 takes.
fn report(name: &str, tracks: &DemoTracks, ticks: &[f64]) {
    let mut drawn = 0usize;
    let mut micros: Vec<f64> = ticks
        .iter()
        .map(|&tick| {
            let started = Instant::now();
            drawn += std::hint::black_box(tracks.at(tick)).players().len();
            started.elapsed().as_secs_f64() * 1e6
        })
        .collect();
    micros.sort_by(f64::total_cmp);
    let pick = |q: f64| micros[((micros.len() - 1) as f64 * q).round() as usize];
    let (p50, p99) = (pick(0.5), pick(0.99));
    let max = micros.last().copied().unwrap_or(0.);
    println!(
        "{name}: count={} drawn={drawn} p50={p50:.2}us p99={p99:.2}us max={max:.2}us \
         p99_frame_share={:.4}%",
        micros.len(),
        100. * p99 / FRAME_US
    );
}
