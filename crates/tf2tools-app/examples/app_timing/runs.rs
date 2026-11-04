//! Synthetic seeds and the v6 startup and frame runs.

use std::collections::VecDeque;
use std::io::{self, BufRead, BufReader, Write};
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use tf2tools_app::timing::Case;
use tf2tools_core::{
    CrosshairRef, DataDir, Database, DraftCrosshair, DraftSlot, Library, ProfileDraft, Provenance,
    TfClass, weapons,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SeedCounts {
    pub huds: usize,
    pub crosshairs: usize,
    pub sounds: usize,
}

/// `<tpl>/data/com.tf2tools.app`.
pub fn template_data_dir(tpl: &Path) -> PathBuf {
    tpl.join("data").join("com.tf2tools.app")
}

/// The data dir an app run under `root` sees: `template_data_dir(root)` with its image cache
/// under `<root>/cache`, the `XDG_CACHE_HOME` that `run_env` sets. Templates share the layout.
pub fn home_data(root: &Path) -> DataDir {
    DataDir::at(template_data_dir(root)).with_cache_home(root.join("cache"))
}

/// True when the template's image cache holds files, as `seed-store --warm` leaves it.
pub fn template_is_warm(tpl: &Path) -> bool {
    std::fs::read_dir(home_data(tpl).image_cache_dir()).is_ok_and(|mut d| d.next().is_some())
}

/// ` warm=<b> fake_requests=<n> fake_image_requests=<n> valid=<b>` for one case run against
/// the fake. A warm run that fetched an image, a warm image grid that decoded nothing, or a cold
/// image grid that fetched none, is invalid.
pub fn fake_fields(
    case: Case,
    warm: bool,
    requests: usize,
    images: usize,
    image_bytes: Option<u64>,
) -> (bool, String) {
    let valid = if warm {
        images == 0 && (!case.shows_images() || image_bytes.is_some_and(|b| b > 0))
    } else {
        !case.shows_images() || images > 0
    };
    let line =
        format!(" warm={warm} fake_requests={requests} fake_image_requests={images} valid={valid}");
    (valid, line)
}

/// The `image_bytes=<n>` note in a `frames` line, or None when absent or not a number.
pub fn image_bytes_in(line: &str) -> Option<u64> {
    line.split(' ')
        .find_map(|word| word.strip_prefix("image_bytes="))
        .and_then(|n| n.parse().ok())
}

/// `<tpl>/fake-tf2/Team Fortress 2`.
pub fn fake_game(tpl: &Path) -> PathBuf {
    tpl.join("fake-tf2").join("Team Fortress 2")
}

/// The five store keys whose `{key}_last_synced` the seed stamps, as v5 and core name them.
pub const STORE_KEYS: [&str; 5] = ["curated", "huds_gb", "sounds", "crosshairs", "addons"];

/// Stamps every store's `{key}_last_synced` as `now`, so no app sees a stale store and syncs,
/// and marks the sound walk as v6's newest-first, so no run mistakes the data for a v5 cache.
pub fn stamp_synced(conn: &rusqlite::Connection, now: &str) -> rusqlite::Result<()> {
    let upsert = "INSERT OR REPLACE INTO store_cache_meta (key, value) VALUES (?1, ?2)";
    for key in STORE_KEYS {
        conn.execute(upsert, [format!("{key}_last_synced").as_str(), now])?;
    }
    conn.execute(upsert, ["sounds_sync_order", "newest"])?;
    Ok(())
}

/// A fresh template with `huds` generated HUDs and `crosshairs` one-colour PNG crosshairs,
/// `tf2_path` at `fake_game`, and every store stamped as synced now (RFC 3339).
pub fn seed_synthetic(tpl: &Path, huds: usize, crosshairs: usize) -> Result<SeedCounts, String> {
    let game = fake_game(tpl);
    std::fs::create_dir_all(game.join("tf/custom")).map_err(ctx("fake tf2"))?;
    let src = tpl.join("seed-src");
    std::fs::create_dir_all(&src).map_err(ctx("seed sources"))?;

    // The library API does the imports so the files and rows are exactly what the app writes.
    let dir = DataDir::at(template_data_dir(tpl));
    let db = Database::open(&dir).map_err(ctx("open template db"))?;
    db.set_tf2_path(Some(&game)).map_err(ctx("set tf2_path"))?;
    let library = Library::new(db, dir);
    for i in 0..huds {
        let archive = hud_zip(&src, &format!("hud{i:03}")).map_err(ctx("hud zip"))?;
        library
            .import_hud(&archive, Provenance::Local)
            .map_err(ctx("import hud"))?;
    }
    let png = src.join("probe.png");
    std::fs::write(&png, crosshair_png()?).map_err(ctx("probe png"))?;
    for i in 0..crosshairs {
        library
            .import_crosshair(&png, &format!("Probe {i:03}"), Provenance::Local)
            .map_err(ctx("import crosshair"))?;
    }
    drop(library);
    std::fs::remove_dir_all(&src).map_err(ctx("remove seed sources"))?;

    // Stamped on the closed database, so neither app sees a stale store and starts a sync.
    let conn = rusqlite::Connection::open(template_data_dir(tpl).join("tf2tools.db"))
        .map_err(ctx("reopen template db"))?;
    let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Micros, false);
    stamp_synced(&conn, &now).map_err(ctx("stamp stores"))?;
    let count = |table: &str| -> Result<usize, String> {
        conn.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| {
            r.get::<_, i64>(0)
        })
        .map(|n| n as usize)
        .map_err(ctx("count"))
    };
    Ok(SeedCounts {
        huds: count("library_huds")?,
        crosshairs: count("library_crosshairs")?,
        sounds: count("library_sounds")?,
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct M4Counts {
    pub profiles: usize,
    pub logs: usize,
}

/// Adds the profile and log cases' data to a seeded template: profiles "Timing A" (the first library HUD,
/// the first library crosshair on every Scout weapon) and "Timing B" (the last HUD, the same
/// crosshair on every Soldier weapon), and `logs` info rows sharing one timestamp.
pub fn seed_m4(tpl: &Path, logs: usize) -> Result<M4Counts, String> {
    let dir = DataDir::at(template_data_dir(tpl));
    let library = Library::new(Database::open(&dir).map_err(ctx("open template db"))?, dir);
    let huds = library.list_huds().map_err(ctx("list huds"))?;
    let crosshairs = library.list_crosshairs().map_err(ctx("list crosshairs"))?;
    let (Some(first), Some(last), Some(crosshair)) =
        (huds.first(), huds.last(), crosshairs.first())
    else {
        return Err("seed-synthetic --logs needs --huds and --crosshairs of at least 1".into());
    };
    for (name, hud, class) in [
        ("Timing A", first, TfClass::Scout),
        ("Timing B", last, TfClass::Soldier),
    ] {
        let assigned = weapons()
            .iter()
            .filter(|w| w.class() == class)
            .map(|w| DraftCrosshair::Set {
                weapon_script: w.script_name().into(),
                crosshair: CrosshairRef::Library(crosshair.id().clone()),
            })
            .collect();
        let draft = ProfileDraft::new(name)
            .with_hud(DraftSlot::Item(hud.id().clone()))
            .with_crosshairs(assigned);
        library
            .create_profile(&draft)
            .map_err(ctx("create profile"))?;
    }
    drop(library);

    // One transaction, so a 10,000-row seed takes a moment rather than a minute.
    let mut conn = rusqlite::Connection::open(template_data_dir(tpl).join("tf2tools.db"))
        .map_err(ctx("reopen template db"))?;
    let tx = conn.transaction().map_err(ctx("begin logs"))?;
    let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Micros, false);
    for i in 1..=logs {
        tx.execute(
            "INSERT INTO logs (timestamp, level, category, action, message, details)
             VALUES (?1, 'info', 'hud', 'install', ?2, NULL)",
            (&now, format!("row {i}")),
        )
        .map_err(ctx("insert log row"))?;
    }
    tx.commit().map_err(ctx("commit logs"))?;
    Ok(M4Counts { profiles: 2, logs })
}

/// Prefixes an error with what was being done.
fn ctx<E: std::fmt::Display>(what: &'static str) -> impl FnOnce(E) -> String {
    move |e| format!("{what}: {e}")
}

/// A 16x16 opaque green PNG, as the app tests' `crosshair_png` writes it.
fn crosshair_png() -> Result<Vec<u8>, String> {
    let img = image::RgbaImage::from_pixel(16, 16, image::Rgba([0, 255, 0, 255]));
    let mut out = io::Cursor::new(Vec::new());
    img.write_to(&mut out, image::ImageFormat::Png)
        .map_err(|e| format!("encode png: {e}"))?;
    Ok(out.into_inner())
}

/// `<dir>/<name>.zip` holding a minimal HUD folder `name` (info.vdf and hudlayout.res).
pub fn hud_zip(dir: &Path, name: &str) -> io::Result<PathBuf> {
    let path = dir.join(format!("{name}.zip"));
    let mut zip = zip::ZipWriter::new(std::fs::File::create(&path)?);
    let options = zip::write::SimpleFileOptions::default();
    zip.start_file(format!("{name}/info.vdf"), options)
        .map_err(io::Error::other)?;
    write!(zip, "\"{name}\"\n{{\n\t\"ui_version\"\t\"3\"\n}}\n")?;
    zip.start_file(format!("{name}/resource/ui/hudlayout.res"), options)
        .map_err(io::Error::other)?;
    write!(zip, "\"Resource/HudLayout.res\"\n{{\n}}\n")?;
    zip.finish().map_err(io::Error::other)?;
    Ok(path)
}

/// Copies every file under `from` into `to`, creating directories as needed.
pub fn copy_tree(from: &Path, to: &Path) -> io::Result<()> {
    std::fs::create_dir_all(to)?;
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        let target = to.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_tree(&entry.path(), &target)?;
        } else {
            std::fs::copy(entry.path(), target)?;
        }
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TimingLine {
    Mark(String, u128),
    FirstFrame(u128),
    Frames(String),
    Idle(u128, u128),
}

/// One `TF2TOOLS_TIMING` stderr line from the app; anything else (logs, warnings) is None.
pub fn parse_timing_line(line: &str) -> Option<TimingLine> {
    if line.starts_with("frames ") {
        return Some(TimingLine::Frames(line.to_string()));
    }
    let words: Vec<&str> = line.split(' ').collect();
    let ns = |s: &str| s.parse::<u128>().ok();
    match words.as_slice() {
        ["mark", name, at] => Some(TimingLine::Mark(name.to_string(), ns(at)?)),
        ["first-frame", at] => Some(TimingLine::FirstFrame(ns(at)?)),
        ["idle", from, to] => Some(TimingLine::Idle(ns(from)?, ns(to)?)),
        _ => None,
    }
}

/// What the app may see of the session's env; everything else, DISPLAY included, is dropped.
const PASS_THROUGH: [&str; 5] = [
    "WAYLAND_DISPLAY",
    "XDG_RUNTIME_DIR",
    "DBUS_SESSION_BUS_ADDRESS",
    "PATH",
    "LANG",
];

/// The app's env: only WAYLAND_DISPLAY, XDG_RUNTIME_DIR, DBUS_SESSION_BUS_ADDRESS, PATH, LANG
/// pass from `inherited`; HOME and XDG_{DATA,CONFIG,CACHE}_HOME under `run_dir`;
/// TF2TOOLS_ASSETS=`assets`; TF2TOOLS_TIMING=`mode`; TF2TOOLS_LOOPBACK_BASE=`loopback` when set.
pub fn run_env(
    run_dir: &Path,
    assets: &Path,
    mode: &str,
    loopback: Option<&str>,
    inherited: &[(String, String)],
) -> Vec<(String, String)> {
    let mut env: Vec<(String, String)> = inherited
        .iter()
        .filter(|(k, _)| PASS_THROUGH.contains(&k.as_str()))
        .cloned()
        .collect();
    for (key, sub) in home_dirs() {
        env.push((key.into(), run_dir.join(sub).display().to_string()));
    }
    env.push(("TF2TOOLS_ASSETS".into(), assets.display().to_string()));
    env.push(("TF2TOOLS_TIMING".into(), mode.into()));
    if let Some(base) = loopback {
        env.push(("TF2TOOLS_LOOPBACK_BASE".into(), base.into()));
    }
    env
}

/// Each home-like variable and its subdir of the run dir.
fn home_dirs() -> [(&'static str, &'static str); 4] {
    [
        ("HOME", "home"),
        ("XDG_DATA_HOME", "data"),
        ("XDG_CONFIG_HOME", "config"),
        ("XDG_CACHE_HOME", "cache"),
    ]
}

fn ms_since(from_ns: u128, at_ns: u128) -> f64 {
    at_ns.saturating_sub(from_ns) as f64 / 1e6
}

/// `run=<i> pid=<p> spawned_unix_ms=<ms> ended_unix_ms=<ms> first_frame_ms=<.1>` then
/// `<mark>_ms=<.1>` per mark, all measured from the spawn.
pub fn startup_line(
    run: usize,
    pid: u32,
    spawned_ns: u128,
    ended_ns: u128,
    first_frame_ns: u128,
    marks: &[(String, u128)],
) -> String {
    let mut line = format!(
        "run={run} pid={pid} spawned_unix_ms={} ended_unix_ms={} first_frame_ms={:.1}",
        spawned_ns / 1_000_000,
        ended_ns / 1_000_000,
        ms_since(spawned_ns, first_frame_ns)
    );
    for (name, at) in marks {
        line.push_str(&format!(" {name}_ms={:.1}", ms_since(spawned_ns, *at)));
    }
    line
}

/// The median: the middle value, or the mean of the middle two.
pub fn median(samples: &[f64]) -> f64 {
    let mut sorted = samples.to_vec();
    sorted.sort_by(f64::total_cmp);
    let mid = sorted.len() / 2;
    if sorted.len().is_multiple_of(2) {
        (sorted[mid - 1] + sorted[mid]) / 2.0
    } else {
        sorted[mid]
    }
}

/// `<name> median_ms=<.1> min_ms=<.1> max_ms=<.1> runs=<n>`.
pub fn summary_line(name: &str, samples: &[f64]) -> String {
    let min = samples.iter().copied().fold(f64::INFINITY, f64::min);
    let max = samples.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    format!(
        "{name} median_ms={:.1} min_ms={min:.1} max_ms={max:.1} runs={}",
        median(samples),
        samples.len()
    )
}

/// Where a startup or frame run gets its app and data from.
pub struct RunSetup<'a> {
    pub bin: &'a Path,
    pub assets: &'a Path,
    pub template: &'a Path,
    pub scratch: &'a Path,
    /// The loopback fake the app's stores reach; `None` boots them on a closed port.
    pub loopback: Option<&'a str>,
    /// The fake itself, whose request counts each frame case reports.
    pub fake: Option<&'a crate::fake_gb::FakeGb>,
}

/// A spawned app: its stderr arrives line by line on `lines`, and the last 20 are kept.
struct Running {
    child: Child,
    spawned_ns: u128,
    lines: Receiver<String>,
    tail: VecDeque<String>,
}

impl Running {
    /// The next stderr line before `deadline`, remembered in the tail.
    fn next_line(&mut self, deadline: Instant) -> Result<String, RecvTimeoutError> {
        let wait = deadline.saturating_duration_since(Instant::now());
        let line = self.lines.recv_timeout(wait)?;
        if self.tail.len() == 20 {
            self.tail.pop_front();
        }
        self.tail.push_back(line.clone());
        Ok(line)
    }

    fn tail(&self) -> String {
        self.tail.iter().cloned().collect::<Vec<_>>().join("\n")
    }
}

fn unix_ns() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos()
}

/// Makes a fresh `run_dir` with its home dirs, a copy of the template's data dir and, when the
/// template has one, of its image cache, both where `home_data(run_dir)` finds them.
pub fn stage_run(template: &Path, run_dir: &Path) -> Result<(), String> {
    if run_dir.exists() {
        return Err(format!("{} already exists", run_dir.display()));
    }
    for (_, sub) in home_dirs() {
        std::fs::create_dir_all(run_dir.join(sub)).map_err(|e| e.to_string())?;
    }
    // The fake TF2 tree stays in the template and is only read.
    let (from, to) = (home_data(template), home_data(run_dir));
    copy_tree(from.root(), to.root()).map_err(|e| format!("copy template: {e}"))?;
    if from.image_cache_dir().is_dir() {
        copy_tree(&from.image_cache_dir(), &to.image_cache_dir())
            .map_err(|e| format!("copy image cache: {e}"))?;
    }
    Ok(())
}

/// Makes a fresh `run_dir` with its own copy of the template's data and starts the app in it.
fn spawn_app(setup: &RunSetup, run_dir: &Path, mode: &str) -> Result<Running, String> {
    stage_run(setup.template, run_dir)?;
    let mut log = std::fs::File::create(run_dir.join("app.log")).map_err(|e| e.to_string())?;
    let inherited: Vec<(String, String)> = std::env::vars().collect();
    let mut cmd = Command::new(setup.bin);
    cmd.env_clear()
        .envs(run_env(
            run_dir,
            setup.assets,
            mode,
            setup.loopback,
            &inherited,
        ))
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .process_group(0);
    let spawned_ns = unix_ns();
    let mut child = cmd
        .spawn()
        .map_err(|e| format!("spawn {}: {e}", setup.bin.display()))?;

    // The reader keeps the pipe drained for the whole run and keeps a full log beside it.
    let stderr = child.stderr.take().ok_or("no stderr pipe")?;
    let (tx, lines) = mpsc::channel();
    std::thread::spawn(move || {
        for line in BufReader::new(stderr).lines().map_while(Result::ok) {
            let _ = writeln!(log, "{line}");
            if tx.send(line).is_err() {
                break;
            }
        }
    });
    Ok(Running {
        child,
        spawned_ns,
        lines,
        tail: VecDeque::new(),
    })
}

/// Stops the app's process group: TERM, KILL after 3 s, then reaps it.
fn stop(mut child: Child) {
    let group = format!("-{}", child.id());
    let signal = |sig: &str| {
        let _ = Command::new("kill")
            .args(["-s", sig, "--", &group])
            .stderr(Stdio::null())
            .status();
    };
    signal("TERM");
    let deadline = Instant::now() + Duration::from_secs(3);
    while Instant::now() < deadline && matches!(child.try_wait(), Ok(None)) {
        std::thread::sleep(Duration::from_millis(50));
    }
    signal("KILL");
    let _ = child.wait();
}

/// Times `runs` startups to the first frame, holding each window up until `hold` after its
/// spawn so the damage log sees it settle. Prints a `startup_line` per run, then the summary.
pub fn v6_startup(setup: &RunSetup, runs: usize, hold: Duration) -> Result<(), String> {
    let mut samples = Vec::with_capacity(runs);
    for i in 1..=runs {
        let run_dir = setup.scratch.join(format!("run-{i}"));
        let mut app = spawn_app(setup, &run_dir, "1")?;
        let pid = app.child.id();

        // A first load that fails never stamps a frame, so the deadline turns a hang into an error.
        let deadline = Instant::now() + Duration::from_secs(30);
        let mut marks = Vec::new();
        let first_frame = loop {
            match app.next_line(deadline) {
                Ok(line) => match parse_timing_line(&line) {
                    Some(TimingLine::Mark(name, at)) => marks.push((name, at)),
                    Some(TimingLine::FirstFrame(at)) => break at,
                    _ => {}
                },
                Err(e) => {
                    let why = match e {
                        RecvTimeoutError::Timeout => "no first frame within 30 s",
                        RecvTimeoutError::Disconnected => "the app exited before its first frame",
                    };
                    let tail = app.tail();
                    stop(app.child);
                    return Err(format!("run {i} pid {pid}: {why}; stderr tail:\n{tail}"));
                }
            }
        };

        let hold_until = app.spawned_ns + hold.as_nanos();
        let now = unix_ns();
        if now < hold_until {
            std::thread::sleep(Duration::from_nanos((hold_until - now) as u64));
        }
        let ended = unix_ns();
        stop(app.child);
        println!(
            "{}",
            startup_line(i, pid, app.spawned_ns, ended, first_frame, &marks)
        );
        samples.push(ms_since(app.spawned_ns, first_frame));
    }
    println!("{}", summary_line("first_frame", &samples));
    Ok(())
}

/// Runs each case in its own app process and prints its result line; Ok(false) if any failed.
pub fn v6_frames(setup: &RunSetup, cases: &[Case]) -> Result<bool, String> {
    let mut all_ok = true;
    let warm = template_is_warm(setup.template);
    for &case in cases {
        let name = case.name();
        let before = setup.fake.map(|f| (f.requests(), f.image_requests()));
        let mut app = spawn_app(setup, &setup.scratch.join(name), name)?;
        let pid = app.child.id();
        let deadline = Instant::now() + Duration::from_secs(180);
        let mut result = None;
        // The stderr pipe closes when the app exits, which ends the loop.
        let timed_out = loop {
            match app.next_line(deadline) {
                Ok(line) => match parse_timing_line(&line) {
                    Some(TimingLine::Frames(l)) => result = Some(format!("{l} pid={pid}")),
                    Some(TimingLine::Idle(from, to)) => {
                        result = Some(format!(
                            "idle pid={pid} from_unix_ms={} to_unix_ms={}",
                            from / 1_000_000,
                            to / 1_000_000
                        ))
                    }
                    _ => {}
                },
                Err(RecvTimeoutError::Timeout) => break true,
                Err(RecvTimeoutError::Disconnected) => break false,
            }
        };
        let line = if timed_out {
            stop(app.child);
            all_ok = false;
            format!("frames case={name} error=timeout pid={pid}")
        } else {
            let status = app.child.wait().map_err(|e| e.to_string())?;
            all_ok &= status.success() && result.is_some();
            match result {
                Some(line) => line,
                None => format!("frames case={name} error=exit {status} pid={pid}"),
            }
        };
        // Each case's own share of the fake's traffic proves what its run measured.
        let checked = setup.fake.zip(before).map(|(f, (requests, images))| {
            let (requests, images) = (f.requests() - requests, f.image_requests() - images);
            fake_fields(case, warm, requests, images, image_bytes_in(&line))
        });
        let (valid, fields) = checked.unwrap_or((true, String::new()));
        all_ok &= valid;
        println!("{line}{fields}");
    }
    Ok(all_ok)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tf2tools_core::{DataDir, Database};

    #[test]
    fn the_run_env_names_the_loopback_fake_when_given() {
        let base = "http://127.0.0.1:47613";
        let env = run_env(Path::new("/s/r"), Path::new("/a"), "x", Some(base), &[]);
        assert!(env.contains(&("TF2TOOLS_LOOPBACK_BASE".into(), base.into())));
    }

    fn small_seed(warm_images: bool) -> crate::store_seed::StoreSeed {
        crate::store_seed::StoreSeed {
            sounds: 3,
            gb_huds: 2,
            curated: 1,
            crosshairs: 1,
            addons: 0,
            warm_images,
        }
    }

    #[test]
    fn a_warm_seed_lands_in_the_image_cache_each_run_reads() {
        let tmp = tempfile::tempdir().unwrap();
        let tpl = tmp.path().join("tpl");
        let base = crate::store_seed::fake_base(crate::store_seed::FAKE_PORT);
        let urls =
            crate::store_seed::seed_store(&home_data(&tpl), &small_seed(true), &base).unwrap();
        assert!(template_is_warm(&tpl));
        let run = tmp.path().join("run");
        stage_run(&tpl, &run).unwrap();
        // The app resolves its image cache from XDG_CACHE_HOME, as `platform_default` does.
        let env = run_env(&run, Path::new("/a"), "x", None, &[]);
        let (_, cache_home) = env.iter().find(|(k, _)| k == "XDG_CACHE_HOME").unwrap();
        let cache = DataDir::at("/unused")
            .with_cache_home(cache_home)
            .image_cache_dir();
        assert_eq!(std::fs::read_dir(cache).unwrap().count(), urls.len());
        assert!(home_data(&run).db_path().exists());
    }

    #[test]
    fn a_cold_template_is_not_warm_and_still_stages() {
        let tmp = tempfile::tempdir().unwrap();
        let tpl = tmp.path().join("tpl");
        let base = crate::store_seed::fake_base(crate::store_seed::FAKE_PORT);
        crate::store_seed::seed_store(&home_data(&tpl), &small_seed(false), &base).unwrap();
        assert!(!template_is_warm(&tpl));
        stage_run(&tpl, &tmp.path().join("run")).unwrap();
    }

    #[test]
    fn a_warm_run_fetches_no_image_and_a_cold_image_grid_does() {
        assert_eq!(
            fake_fields(Case::GameBananaScroll, true, 3, 0, Some(4096)),
            (
                true,
                " warm=true fake_requests=3 fake_image_requests=0 valid=true".into()
            )
        );
        assert!(!fake_fields(Case::GameBananaScroll, true, 216, 216, None).0);
        assert!(!fake_fields(Case::SoundStoreScroll, true, 1, 1, None).0);
        assert!(fake_fields(Case::CuratedScroll, false, 40, 40, None).0);
        assert!(!fake_fields(Case::CuratedScroll, false, 0, 0, None).0);
        // Sound cards carry no image, so a cold sound scroll fetching none is still valid.
        assert!(fake_fields(Case::SoundStoreScroll, false, 0, 0, None).0);
    }

    #[test]
    fn a_warm_image_grid_that_decoded_nothing_is_invalid() {
        assert!(!fake_fields(Case::GameBananaScroll, true, 3, 0, Some(0)).0);
        assert!(!fake_fields(Case::CuratedScroll, true, 40, 0, None).0);
        assert!(fake_fields(Case::CuratedScroll, true, 40, 0, Some(1)).0);
        // Sound cards carry no image, so a warm sound scroll needs no bytes.
        assert!(fake_fields(Case::SoundStoreScroll, true, 1, 0, None).0);
    }

    #[test]
    fn image_bytes_come_from_the_frames_line() {
        let line = "frames case=curated-scroll intervals=300 median_ms=8.000 max_scroll_px=900 \
                    cards=60 image_bytes=4096 foreground_decodes=0";
        assert_eq!(image_bytes_in(line), Some(4096));
        assert_eq!(
            image_bytes_in("frames case=curated-scroll error=timeout"),
            None
        );
    }

    #[test]
    fn app_lines_parse() {
        assert_eq!(
            parse_timing_line("mark window 1792144800050500000"),
            Some(TimingLine::Mark("window".into(), 1_792_144_800_050_500_000))
        );
        assert_eq!(
            parse_timing_line("first-frame 5"),
            Some(TimingLine::FirstFrame(5))
        );
        assert_eq!(
            parse_timing_line("idle 1000000 6000000"),
            Some(TimingLine::Idle(1_000_000, 6_000_000))
        );
        assert_eq!(
            parse_timing_line("frames case=calibrate intervals=300"),
            Some(TimingLine::Frames(
                "frames case=calibrate intervals=300".into()
            ))
        );
        assert_eq!(parse_timing_line("mark window soon"), None);
        assert_eq!(parse_timing_line("[WARN] gpui: something"), None);
    }

    #[test]
    fn the_run_env_isolates_every_home_and_drops_the_desktop() {
        let run = Path::new("/scratch/run-1");
        let inherited: Vec<(String, String)> = [
            ("DISPLAY", ":0"),
            ("WAYLAND_DISPLAY", "tf2tools-timing"),
            ("XDG_RUNTIME_DIR", "/scratch/sess/rt"),
            ("HOME", "/home/u"),
            ("XDG_DATA_HOME", "/home/u/.local/share"),
        ]
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .to_vec();
        let env = run_env(run, Path::new("/repo/assets"), "1", None, &inherited);
        let get = |k: &str| {
            env.iter()
                .find(|(key, _)| key == k)
                .map(|(_, v)| v.as_str())
        };
        assert_eq!(get("DISPLAY"), None);
        assert_eq!(get("WAYLAND_DISPLAY"), Some("tf2tools-timing"));
        assert_eq!(get("XDG_RUNTIME_DIR"), Some("/scratch/sess/rt"));
        assert_eq!(get("TF2TOOLS_TIMING"), Some("1"));
        assert_eq!(get("TF2TOOLS_ASSETS"), Some("/repo/assets"));
        assert_eq!(get("TF2TOOLS_LOOPBACK_BASE"), None);
        for key in ["HOME", "XDG_DATA_HOME", "XDG_CONFIG_HOME", "XDG_CACHE_HOME"] {
            assert!(get(key).unwrap().starts_with("/scratch/run-1"), "{key}");
        }
    }

    #[test]
    fn a_synthetic_seed_holds_its_library_and_a_fake_tf2() {
        let tmp = tempfile::tempdir().unwrap();
        let tpl = tmp.path().join("tpl");
        let counts = seed_synthetic(&tpl, 3, 2).unwrap();
        assert_eq!(
            counts,
            SeedCounts {
                huds: 3,
                crosshairs: 2,
                sounds: 0
            }
        );
        let db = Database::open(&DataDir::at(template_data_dir(&tpl))).unwrap();
        assert_eq!(
            db.settings().unwrap().tf2_path(),
            Some(fake_game(&tpl).as_path())
        );
        assert!(fake_game(&tpl).join("tf/custom").is_dir());
    }

    #[test]
    fn the_m4_seed_adds_two_profiles_and_the_log_rows() {
        let tmp = tempfile::tempdir().unwrap();
        let tpl = tmp.path().join("tpl");
        seed_synthetic(&tpl, 2, 1).unwrap();
        assert_eq!(
            seed_m4(&tpl, 250).unwrap(),
            M4Counts {
                profiles: 2,
                logs: 250
            }
        );
        let dir = DataDir::at(template_data_dir(&tpl));
        let lib = Library::new(Database::open(&dir).unwrap(), dir);
        let profiles = lib.profiles().unwrap();
        let names: Vec<_> = profiles.iter().map(|p| p.name().to_owned()).collect();
        assert_eq!(names, ["Timing A", "Timing B"]);
        assert!(
            profiles
                .iter()
                .all(|p| p.hud().is_some() && !p.crosshairs().is_empty())
        );
        assert_ne!(profiles[0].hud(), profiles[1].hud());
    }

    #[test]
    fn the_m4_seed_needs_a_hud_and_a_crosshair() {
        let tmp = tempfile::tempdir().unwrap();
        let tpl = tmp.path().join("tpl");
        seed_synthetic(&tpl, 0, 1).unwrap();
        let err = seed_m4(&tpl, 10).unwrap_err();
        assert!(err.contains("--huds and --crosshairs"), "{err}");
    }

    #[test]
    fn stamp_synced_marks_every_store_and_the_newest_sound_walk() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        conn.execute_batch("CREATE TABLE store_cache_meta (key TEXT PRIMARY KEY, value TEXT)")
            .unwrap();
        stamp_synced(&conn, "2026-10-17T12:00:00.000000+00:00").unwrap();
        let get = |key: String| -> String {
            conn.query_row(
                "SELECT value FROM store_cache_meta WHERE key = ?1",
                [key],
                |r| r.get(0),
            )
            .unwrap()
        };
        for key in STORE_KEYS {
            assert_eq!(
                get(format!("{key}_last_synced")),
                "2026-10-17T12:00:00.000000+00:00"
            );
        }
        assert_eq!(get("sounds_sync_order".into()), "newest");
    }

    #[test]
    fn a_seed_marks_every_store_as_synced() {
        let tmp = tempfile::tempdir().unwrap();
        let tpl = tmp.path().join("tpl");
        seed_synthetic(&tpl, 1, 0).unwrap();
        let conn = rusqlite::Connection::open(template_data_dir(&tpl).join("tf2tools.db")).unwrap();
        for key in STORE_KEYS {
            let value: String = conn
                .query_row(
                    "SELECT value FROM store_cache_meta WHERE key = ?1",
                    [format!("{key}_last_synced")],
                    |r| r.get(0),
                )
                .unwrap();
            assert!(chrono_like(&value), "{key}: {value}");
        }
    }

    #[test]
    fn the_seed_marks_the_sound_walk_newest_so_no_run_resets_it() {
        let tpl = tempfile::tempdir().unwrap();
        seed_synthetic(tpl.path(), 0, 0).unwrap();
        let conn =
            rusqlite::Connection::open(template_data_dir(tpl.path()).join("tf2tools.db")).unwrap();
        let order: String = conn
            .query_row(
                "SELECT value FROM store_cache_meta WHERE key = 'sounds_sync_order'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(order, "newest");
    }

    /// An RFC 3339 timestamp as v5 writes it: date, 'T', time, and an offset or 'Z'.
    fn chrono_like(value: &str) -> bool {
        value.len() >= 20
            && value.as_bytes()[10] == b'T'
            && (value.ends_with('Z') || value.contains('+'))
    }

    #[test]
    fn copy_tree_copies_nested_files() {
        let tmp = tempfile::tempdir().unwrap();
        let from = tmp.path().join("from");
        std::fs::create_dir_all(from.join("a/b")).unwrap();
        std::fs::write(from.join("a/b/c.txt"), "hi").unwrap();
        std::fs::write(from.join("top.db"), "db").unwrap();
        let to = tmp.path().join("to");
        copy_tree(&from, &to).unwrap();
        assert_eq!(std::fs::read_to_string(to.join("a/b/c.txt")).unwrap(), "hi");
        assert_eq!(std::fs::read_to_string(to.join("top.db")).unwrap(), "db");
    }

    #[test]
    fn the_startup_line_measures_from_the_spawn() {
        let spawned = 1_792_144_800_000_000_000u128;
        let marks = vec![
            ("main".to_string(), spawned + 3_000_000),
            ("window".to_string(), spawned + 50_500_000),
        ];
        assert_eq!(
            startup_line(
                1,
                77,
                spawned,
                spawned + 5_300_000_000,
                spawned + 250_000_000,
                &marks
            ),
            "run=1 pid=77 spawned_unix_ms=1792144800000 ended_unix_ms=1792144805300 \
             first_frame_ms=250.0 main_ms=3.0 window_ms=50.5"
        );
    }

    #[test]
    fn medians_follow_m1() {
        assert_eq!(median(&[3.0, 1.0, 2.0]), 2.0);
        assert_eq!(median(&[4.0, 1.0, 3.0, 2.0]), 2.5);
        assert_eq!(
            summary_line("first_frame", &[120.0, 100.0, 110.0]),
            "first_frame median_ms=110.0 min_ms=100.0 max_ms=120.0 runs=3"
        );
    }
}
