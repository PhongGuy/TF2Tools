//! Measures the camera milestone's unknowns: the eye heights a demo's tracks hold, how a
//! Gaussian local linear fit treats flicks and rest jitter, and what a sparse ray grid costs
//! on the stock maps with the most triangles.
//! Usage: cargo run --release -p tf2tools-core --example demo_cameras_probe -- eyes <file.dem>
//!        cargo run --release -p tf2tools-core --example demo_cameras_probe -- grid <map.bsp>...

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::atomic::AtomicBool;
use std::time::Instant;

use tf2tools_core::{MapMesh, load_map_file, read_demo_tracks};

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("eyes") if args.len() == 2 => eyes(&PathBuf::from(&args[1])),
        Some("grid") if args.len() >= 2 => grid(&args[1..]),
        _ => {
            eprintln!("usage: demo_cameras_probe eyes <file.dem> | grid <map.bsp>...");
            ExitCode::from(2)
        }
    }
}

// One player followed tick to tick by team, class and the nearest feet, since the public
// snapshot carries no id yet.
struct Chain {
    key: (u8, u8),
    feet: [f32; 3],
    last_tick: u32,
    yaw: Vec<f64>,
    pitch: Vec<f64>,
}

fn eyes(path: &Path) -> ExitCode {
    let tracks = match read_demo_tracks(path, &AtomicBool::new(false)) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("tracks failed: {e}");
            return ExitCode::FAILURE;
        }
    };
    let spt = f64::from(tracks.seconds_per_tick());
    println!(
        "ticks={}..{} samples={} players={} spt={spt}",
        tracks.first_tick(),
        tracks.last_tick(),
        tracks.sample_count(),
        tracks.player_count()
    );
    let mut heights: BTreeMap<(u32, bool), u64> = BTreeMap::new();
    let mut open: Vec<Chain> = Vec::new();
    let mut done: Vec<Chain> = Vec::new();
    for tick in tracks.first_tick()..=tracks.last_tick() {
        let snap = tracks.at(f64::from(tick));
        for p in snap.players() {
            *heights
                .entry((p.eye_height().round() as u32, p.ducked()))
                .or_default() += 1;
            let key = (p.team() as u8, p.class() as u8);
            let feet = p.feet();
            let near = open
                .iter_mut()
                .filter(|c| c.key == key && c.last_tick + 1 == tick)
                .map(|c| {
                    let d: f32 = (0..3).map(|i| (c.feet[i] - feet[i]).powi(2)).sum();
                    (d, c)
                })
                .filter(|(d, _)| *d < 40. * 40.)
                .min_by(|a, b| a.0.total_cmp(&b.0));
            let yaw = f64::from(p.yaw());
            match near {
                Some((_, c)) => {
                    let prev = c.yaw.last().copied().unwrap_or(yaw);
                    c.yaw
                        .push(prev + (yaw - prev + 180.).rem_euclid(360.) - 180.);
                    c.pitch.push(f64::from(p.pitch()));
                    c.feet = feet;
                    c.last_tick = tick;
                }
                None => open.push(Chain {
                    key,
                    feet,
                    last_tick: tick,
                    yaw: vec![yaw],
                    pitch: vec![f64::from(p.pitch())],
                }),
            }
        }
        let (keep, stale): (Vec<_>, Vec<_>) = open.into_iter().partition(|c| c.last_tick == tick);
        open = keep;
        done.extend(stale);
    }
    done.extend(open);
    done.retain(|c| c.yaw.len() >= 66);
    println!("eye heights (height, ducked) -> rows: {heights:?}");
    println!(
        "chains={} chain_ticks={}",
        done.len(),
        done.iter().map(|c| c.yaw.len()).sum::<usize>()
    );
    for sigma in [1.0, 2.0, 3.0] {
        flicks(&done, sigma, spt);
    }
    ExitCode::SUCCESS
}

// A Gaussian-weighted line fit through the samples within three sigma of `i`, at `i`.
fn smooth(y: &[f64], i: usize, sigma: f64) -> f64 {
    let reach = (3. * sigma).ceil() as usize;
    let (lo, hi) = (i.saturating_sub(reach), (i + reach).min(y.len() - 1));
    let (mut sw, mut sx, mut sy, mut sxx, mut sxy) = (0., 0., 0., 0., 0.);
    for (j, &v) in y.iter().enumerate().take(hi + 1).skip(lo) {
        let x = j as f64 - i as f64;
        let w = (-x * x / (2. * sigma * sigma)).exp();
        sw += w;
        sx += w * x;
        sy += w * v;
        sxx += w * x * x;
        sxy += w * x * v;
    }
    let det = sw * sxx - sx * sx;
    if det.abs() < 1e-9 {
        return sy / sw;
    }
    // The fitted line's value at x = 0.
    (sy * sxx - sx * sxy) / det
}

fn second_diff(y: &[f64]) -> f64 {
    let n = y.len().saturating_sub(2).max(1);
    y.windows(3)
        .map(|w| (w[2] - 2. * w[1] + w[0]).abs())
        .sum::<f64>()
        / n as f64
}

// Flicks are sweeps of 90 degrees or more inside 6 ticks; each is measured at its end tick:
// the share of the sweep the smoothed yaw reached, the peak raw-smoothed gap, and how long
// until the smoothed yaw stays within one degree of the raw, as p50, p95 and p99.
fn flicks(chains: &[Chain], sigma: f64, spt: f64) {
    let (mut shares, mut gaps, mut settles) = (Vec::new(), Vec::new(), Vec::new());
    let (mut raw_jit, mut sm_jit, mut raw_pj, mut sm_pj) = (0., 0., 0., 0.);
    let (mut rest_steps, mut raw_rest_steps) = (Vec::new(), Vec::new());
    for c in chains {
        let s: Vec<f64> = (0..c.yaw.len()).map(|i| smooth(&c.yaw, i, sigma)).collect();
        let sp: Vec<f64> = (0..c.pitch.len())
            .map(|i| smooth(&c.pitch, i, sigma))
            .collect();
        raw_jit += second_diff(&c.yaw);
        sm_jit += second_diff(&s);
        raw_pj += second_diff(&c.pitch);
        sm_pj += second_diff(&sp);
        // Rest: a 13-tick window whose raw yaw spans at most 1.5 degrees.
        for i in 6..c.yaw.len().saturating_sub(6) {
            let w = &c.yaw[i - 6..=i + 6];
            let span = w.iter().cloned().fold(f64::MIN, f64::max)
                - w.iter().cloned().fold(f64::MAX, f64::min);
            if span <= 1.5 {
                rest_steps.push((s[i] - s[i - 1]).abs());
                raw_rest_steps.push((c.yaw[i] - c.yaw[i - 1]).abs());
            }
        }
        let mut i = 0;
        while i + 6 < c.yaw.len() {
            let sweep = c.yaw[i + 6] - c.yaw[i];
            // A step of 150 degrees or more in one tick is a respawn or a mismatch, not a turn.
            let jump = c.yaw[i..=i + 6]
                .windows(2)
                .any(|w| (w[1] - w[0]).abs() >= 150.);
            if sweep.abs() < 90. || jump {
                i += 1;
                continue;
            }
            // The end is the first tick, at most 30 past the window, where the turn has stopped.
            let mut e = i + 6;
            while e + 1 < c.yaw.len().min(i + 36) && (c.yaw[e + 1] - c.yaw[e]).abs() > 0.5 {
                e += 1;
            }
            let total = c.yaw[e] - c.yaw[i];
            if total.abs() >= 45. {
                shares.push((s[e] - c.yaw[i]) / total);
                let gap = (i..=e).map(|k| (s[k] - c.yaw[k]).abs()).fold(0., f64::max);
                gaps.push(gap);
                let horizon = c.yaw.len().min(e + 60);
                let last_far = (e..horizon).rev().find(|&k| (s[k] - c.yaw[k]).abs() > 1.);
                settles.push(last_far.map_or(0., |k| (k + 1 - e) as f64 * spt * 1000.));
            }
            i = e + 1;
        }
    }
    let n = chains.len().max(1) as f64;
    println!(
        "sigma={sigma} flicks={} reached p50/p5/p1={} gap_deg p50/p95/p99={} settle_ms p50/p95/p99={} \
         yaw_d2 raw={:.3} smoothed={:.3} pitch_d2 raw={:.3} smoothed={:.3} \
         rest_step_deg raw p50/p95/p99={} smoothed={}",
        shares.len(),
        low(&mut shares),
        pct(&mut gaps),
        pct(&mut settles),
        raw_jit / n,
        sm_jit / n,
        raw_pj / n,
        sm_pj / n,
        pct(&mut raw_rest_steps),
        pct(&mut rest_steps),
    );
}

fn at(v: &[f64], q: f64) -> f64 {
    v.get(((v.len() as f64 - 1.) * q).round() as usize)
        .copied()
        .unwrap_or(f64::NAN)
}

fn pct(v: &mut [f64]) -> String {
    v.sort_by(f64::total_cmp);
    format!("{:.2}/{:.2}/{:.2}", at(v, 0.5), at(v, 0.95), at(v, 0.99))
}

fn low(v: &mut [f64]) -> String {
    v.sort_by(f64::total_cmp);
    format!("{:.3}/{:.3}/{:.3}", at(v, 0.5), at(v, 0.05), at(v, 0.01))
}

// The sparse grid under test: sorted cell keys, each with a slice of triangle ids.
struct Grid {
    cell: f32,
    keys: Vec<u64>,
    starts: Vec<u32>,
    tris: Vec<u32>,
}

const BIAS: i64 = 1 << 20;

fn key(c: [i64; 3]) -> u64 {
    (((c[0] + BIAS) as u64) << 42) | (((c[1] + BIAS) as u64) << 21) | ((c[2] + BIAS) as u64)
}

fn build(mesh: &MapMesh, cell: f32) -> Grid {
    let p = mesh.positions();
    let mut pairs: Vec<(u64, u32)> = Vec::new();
    for t in 0..p.len() / 3 {
        let tri = &p[t * 3..t * 3 + 3];
        let lo = [0, 1, 2]
            .map(|a| (tri.iter().map(|v| v[a]).fold(f32::MAX, f32::min) / cell).floor() as i64);
        let hi = [0, 1, 2]
            .map(|a| (tri.iter().map(|v| v[a]).fold(f32::MIN, f32::max) / cell).floor() as i64);
        for x in lo[0]..=hi[0] {
            for y in lo[1]..=hi[1] {
                for z in lo[2]..=hi[2] {
                    pairs.push((key([x, y, z]), t as u32));
                }
            }
        }
    }
    pairs.sort_unstable();
    let (mut keys, mut starts, mut tris) =
        (Vec::new(), Vec::new(), Vec::with_capacity(pairs.len()));
    for (k, t) in pairs {
        if keys.last() != Some(&k) {
            keys.push(k);
            starts.push(tris.len() as u32);
        }
        tris.push(t);
    }
    starts.push(tris.len() as u32);
    Grid {
        cell,
        keys,
        starts,
        tris,
    }
}

fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

// Two-sided Moller-Trumbore: the distance along `d` to the triangle, if it is hit ahead.
fn hit(o: [f32; 3], d: [f32; 3], t: &[[f32; 3]]) -> Option<f32> {
    let (e1, e2) = (sub(t[1], t[0]), sub(t[2], t[0]));
    let pv = cross(d, e2);
    let det = dot(e1, pv);
    if det.abs() < 1e-8 {
        return None;
    }
    let tv = sub(o, t[0]);
    let u = dot(tv, pv) / det;
    if !(0. ..=1.).contains(&u) {
        return None;
    }
    let q = cross(tv, e1);
    let v = dot(d, q) / det;
    if v < 0. || u + v > 1. {
        return None;
    }
    let dist = dot(e2, q) / det;
    (dist >= 0.).then_some(dist)
}

// Walks the cells the segment crosses (Amanatides-Woo) and keeps the nearest hit, stopping once
// a hit lies before the current cell's exit.
fn ray(
    g: &Grid,
    p: &[[f32; 3]],
    o: [f32; 3],
    d: [f32; 3],
    max: f32,
    tested: &mut u64,
) -> Option<f32> {
    let mut c = [0, 1, 2].map(|a| (o[a] / g.cell).floor() as i64);
    let step = [0, 1, 2].map(|a| if d[a] >= 0. { 1i64 } else { -1 });
    let delta = [0, 1, 2].map(|a| {
        if d[a] == 0. {
            f32::INFINITY
        } else {
            (g.cell / d[a]).abs()
        }
    });
    let mut next = [0, 1, 2].map(|a| {
        if d[a] == 0. {
            return f32::INFINITY;
        }
        let edge = (c[a] + i64::from(d[a] > 0.)) as f32 * g.cell;
        (edge - o[a]) / d[a]
    });
    let mut best: Option<f32> = None;
    loop {
        if let Ok(k) = g.keys.binary_search(&key(c)) {
            for &t in &g.tris[g.starts[k] as usize..g.starts[k + 1] as usize] {
                *tested += 1;
                let t = t as usize;
                if let Some(h) = hit(o, d, &p[t * 3..t * 3 + 3])
                    && h <= max
                    && best.is_none_or(|b| h < b)
                {
                    best = Some(h);
                }
            }
        }
        let a = if next[0] <= next[1] && next[0] <= next[2] {
            0
        } else if next[1] <= next[2] {
            1
        } else {
            2
        };
        let exit = next[a];
        if best.is_some_and(|b| b <= exit) || exit > max {
            return best;
        }
        c[a] += step[a];
        next[a] += delta[a];
    }
}

fn brute(p: &[[f32; 3]], o: [f32; 3], d: [f32; 3], max: f32) -> Option<f32> {
    p.as_chunks::<3>()
        .0
        .iter()
        .filter_map(|t| hit(o, d, t))
        .filter(|&h| h <= max)
        .min_by(f32::total_cmp)
}

fn grid(paths: &[String]) -> ExitCode {
    let mut best: Option<(String, MapMesh)> = None;
    for path in paths {
        let started = Instant::now();
        match load_map_file(Path::new(path)) {
            Ok(mesh) => {
                println!(
                    "{path}: triangles={} load={:.3}s",
                    mesh.triangle_count(),
                    started.elapsed().as_secs_f64()
                );
                if best
                    .as_ref()
                    .is_none_or(|(_, b)| mesh.triangle_count() > b.triangle_count())
                {
                    best = Some((path.clone(), mesh));
                }
            }
            Err(e) => println!("{path}: error {e}"),
        }
    }
    let Some((name, mesh)) = best else {
        return ExitCode::FAILURE;
    };
    let p = mesh.positions();
    let mesh_bytes = p.len() * 12 * 2;
    println!(
        "largest={name} triangles={} mesh_bytes={mesh_bytes}",
        mesh.triangle_count()
    );
    let mut seed = 0x9e37_79b9_7f4a_7c15u64;
    let mut rnd = move || {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        (seed >> 11) as f32 / (1u64 << 53) as f32
    };
    // Rays start 32 units off random triangles, along their normal, in random directions.
    let mut rays = Vec::new();
    for _ in 0..20_000 {
        let t = ((rnd() * (p.len() / 3) as f32) as usize).min(p.len() / 3 - 1);
        let tri = &p[t * 3..t * 3 + 3];
        let n = mesh.normals()[t * 3];
        let centre = [0, 1, 2].map(|a| (tri[0][a] + tri[1][a] + tri[2][a]) / 3. + n[a] * 32.);
        let mut d = [rnd() * 2. - 1., rnd() * 2. - 1., rnd() * 2. - 1.];
        let len = dot(d, d).sqrt().max(1e-6);
        d = d.map(|c| c / len);
        rays.push((centre, d));
    }
    for cell in [64., 128., 256.] {
        let started = Instant::now();
        let g = build(&mesh, cell);
        let built = started.elapsed().as_secs_f64();
        let bytes = g.keys.len() * 8 + g.starts.len() * 4 + g.tris.len() * 4;
        let mut tested = 0u64;
        let started = Instant::now();
        let mut hits = 0;
        for &(o, d) in &rays {
            hits += usize::from(ray(&g, p, o, d, 96., &mut tested).is_some());
        }
        let per_ray_us = started.elapsed().as_secs_f64() * 1e6 / rays.len() as f64;
        let mut wrong = 0;
        for &(o, d) in rays.iter().take(200) {
            let a = ray(&g, p, o, d, 96., &mut 0);
            let b = brute(p, o, d, 96.);
            if a.map(|x| (x * 1000.).round()) != b.map(|x| (x * 1000.).round()) {
                wrong += 1;
            }
        }
        println!(
            "cell={cell} build={:.1}ms cells={} filed={} grid_bytes={bytes} ({:.0}% of mesh) \
             tris_per_ray={:.1} per_ray={per_ray_us:.2}us hits={hits}/{} brute_mismatch={wrong}/200",
            built * 1000.,
            g.keys.len(),
            g.tris.len(),
            100. * bytes as f64 / mesh_bytes as f64,
            tested as f64 / rays.len() as f64,
            rays.len()
        );
    }
    ExitCode::SUCCESS
}
