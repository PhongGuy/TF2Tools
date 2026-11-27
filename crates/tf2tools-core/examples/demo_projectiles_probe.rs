//! Prints what the tracks hold of a demo's projectiles and kills: track and sample counts, the
//! views per kind and the most alive at one tick, the heap, and the killfeed's flags and weapons.
//! Usage: cargo run --release -p tf2tools-core --example demo_projectiles_probe -- <file.dem>

use std::collections::{BTreeMap, BTreeSet};
use std::io::Read as _;
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::atomic::AtomicBool;
use std::time::Instant;

use tf2tools_core::{DemoTracks, read_demo_header, read_demo_tracks};

fn main() -> ExitCode {
    let Some(path) = std::env::args_os().nth(1).map(PathBuf::from) else {
        eprintln!("usage: demo_projectiles_probe <file.dem>");
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
    projectiles_line(&tracks, elapsed);
    kills_line(&tracks);
    ExitCode::SUCCESS
}

// Asks for the projectiles at every integer tick, as a viewer stepping through the demo would,
// so the per-kind sums sit near the samples per kind.
fn projectiles_line(tracks: &DemoTracks, elapsed: f64) {
    let mut per_kind: BTreeMap<String, usize> = BTreeMap::new();
    let mut most_alive = (0, tracks.first_tick());
    for tick in tracks.first_tick()..=tracks.last_tick() {
        let views = tracks.projectiles_at(f64::from(tick));
        if views.len() > most_alive.0 {
            most_alive = (views.len(), tick);
        }
        for view in &views {
            *per_kind.entry(format!("{:?}", view.kind())).or_default() += 1;
        }
    }
    println!(
        "projectiles count={} samples={} views={per_kind:?} most_alive={} at_tick={} \
         heap_bytes={} build={elapsed:.3}s",
        tracks.projectile_count(),
        tracks.projectile_sample_count(),
        most_alive.0,
        most_alive.1,
        tracks.heap_bytes()
    );
}

fn kills_line(tracks: &DemoTracks) {
    let kills = tracks.kills();
    let no_killer = kills.iter().filter(|k| k.killer().is_none()).count();
    let feigned = kills.iter().filter(|k| k.feigned()).count();
    let critical = kills.iter().filter(|k| k.critical()).count();
    let weapons: BTreeSet<&str> = kills.iter().filter_map(|k| k.weapon()).collect();
    let longest = weapons
        .iter()
        .max_by_key(|w| w.len())
        .copied()
        .unwrap_or("none");
    println!(
        "kills count={} no_killer={no_killer} feigned={feigned} critical={critical} weapons={} \
         longest_weapon={longest} ({} chars)",
        tracks.kill_count(),
        weapons.len(),
        longest.len()
    );
}
