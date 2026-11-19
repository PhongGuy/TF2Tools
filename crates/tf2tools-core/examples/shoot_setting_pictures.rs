//! Shoots the Graphics page's setting pictures in a private, windowed TF2 session on Linux.
//! It backs up the player's cfg files, launches TF2 through Steam with a scratch custom
//! folder whose cfg loop runs one step file at a time, and restores the backup on every exit.
//!
//! Usage: `shoot_setting_pictures [--only <module>] [--window-at X,Y] [--assets <dir>] [--dry-run]`.
//! `--dry-run` launches nothing and prints the step files the run would write.

use std::collections::HashSet;
use std::fs;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, OnceLock};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use tf2tools_core::shoot::{self, SAME_LOOK_RMSE, ShotAction};
use tf2tools_core::{PictureManifest, Reload, SettingShot, Tf2Install, tf2_running};
use walkdir::WalkDir;

const USAGE: &str = "usage: shoot_setting_pictures [--only <module>] [--window-at X,Y] [--assets <dir>] [--dry-run]";
/// The scratch custom folder, relative to `tf/`; removing it undoes everything the run added.
const SCRATCH: &str = "custom/tf2tools-shots";
/// The name a step file is written under before it is renamed into place.
const STEP_TEMP: &str = "step.tmp";
const TF2_PROCESS: &str = "tf_linux64";
const STEAM_START: Duration = Duration::from_secs(60);
/// Steam may process shaders before the game starts, which took over two minutes.
const MAP_READY: Duration = Duration::from_secs(420);
/// The window waits on the same shader processing as the map, so it gets the same time.
const WINDOW_APPEARS: Duration = Duration::from_secs(420);
const STEP_DONE: Duration = Duration::from_secs(30);
const SHOT_SAVED: Duration = Duration::from_secs(10);
const GAME_QUITS: Duration = Duration::from_secs(60);
const POLL: Duration = Duration::from_millis(200);
/// The least time between two step sends, so the server's command quota is never hit.
const SEND_GAP: Duration = Duration::from_millis(300);
const GUARD_POLL: Duration = Duration::from_millis(500);
/// The pauses around the two join sends after a map loads, in milliseconds.
const JOIN_SLEEPS: [u64; 3] = [5000, 4000, 2000];

#[derive(Debug)]
struct Args {
    only: Option<String>,
    window_at: (i32, i32),
    assets: PathBuf,
    dry_run: bool,
}

/// Why the run stopped: bad arguments exit 2, everything else exits 1.
enum Failure {
    Usage(String),
    Run(String),
}

impl From<String> for Failure {
    fn from(message: String) -> Self {
        Self::Run(message)
    }
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(Failure::Usage(message)) => {
            eprintln!("{message}\n{USAGE}");
            ExitCode::from(2)
        }
        Err(Failure::Run(message)) => {
            eprintln!("error: {message}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), Failure> {
    let args = parse_args(std::env::args().skip(1)).map_err(Failure::Usage)?;
    let manifest = read_manifest(&args.assets)?;
    let settings = select(&manifest, args.only.as_deref()).map_err(Failure::Usage)?;
    if args.dry_run {
        print!("{}", dry_run(&manifest, &settings));
        return Ok(());
    }
    if !cfg!(target_os = "linux") {
        return Err("the shooter runs on Linux only".to_owned().into());
    }
    if tf2_running() {
        return Err("TF2 is running; quit it first".to_owned().into());
    }
    let install = Tf2Install::detect()
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "TF2 was not found".to_owned())?;
    let game = install.game_dir().to_owned();
    let steam = steam_root(&game)
        .filter(|root| root.join("userdata").is_dir())
        .ok_or_else(|| format!("no Steam userdata folder above {}", game.display()))?;
    let backup = Backup::take(&backup_roots(&game, &steam), &backup_dir()?)?;
    println!("backup of the cfg files: {}", backup.dir.display());
    for (name, root) in &backup.roots {
        println!("  {name} -> {}", root.display());
    }
    println!("if interrupted: quit TF2, then copy the backup back");

    // Every path from here on ends in `finish`, so the player's files come back.
    let mut session = Session::new(install.tf_dir(), args.window_at);
    let shot = shoot_all(&mut session, &manifest, &settings, &args.assets);
    let finished = finish(&mut session, &backup);
    match (shot, finished) {
        (Ok(()), Ok(())) => Ok(()),
        (Err(e), Ok(())) | (Ok(()), Err(e)) => Err(e.into()),
        (Err(shot), Err(restore)) => Err(format!("{shot}; then {restore}").into()),
    }
}

/// Launches TF2 once and shoots every selected setting, then prints the timings and the
/// look-alike pairs, also when the run stopped part way.
fn shoot_all(
    session: &mut Session,
    manifest: &PictureManifest,
    settings: &[&SettingShot],
    assets: &Path,
) -> Result<(), String> {
    let mut report = Report::new();
    let shot = session
        .launch(manifest, &manifest.map, None)
        .and_then(|()| {
            println!("TF2 is ready; {} settings selected", settings.len());
            shoot_order(settings).into_iter().try_for_each(|setting| {
                shoot_setting(session, manifest, setting, assets, &mut report)
            })
        });
    report.print();
    shot
}

/// Settings that change live go first and the ones needing a restart last, each group in
/// manifest order, so the game restarts as late and as rarely as possible.
fn shoot_order<'m>(settings: &[&'m SettingShot]) -> Vec<&'m SettingShot> {
    let (live, restart): (Vec<&SettingShot>, Vec<&SettingShot>) =
        settings.iter().partition(|s| s.reload != Reload::Restart);
    live.into_iter().chain(restart).collect()
}

/// The values that get their own picture; a `same_as` value borrows another's.
fn values_to_shoot(setting: &SettingShot) -> Vec<&str> {
    setting
        .values
        .iter()
        .filter(|v| v.same_as.is_none())
        .map(|v| v.value.as_str())
        .collect()
}

/// Shoots each of the setting's values, puts the module back on its base value and prints
/// how far apart every pair of the new pictures is.
fn shoot_setting(
    session: &mut Session,
    manifest: &PictureManifest,
    setting: &SettingShot,
    assets: &Path,
    report: &mut Report,
) -> Result<(), String> {
    let module = setting.module.as_str();
    println!("===== {module} =====");
    let shot = Shot {
        manifest,
        setting,
        value: None,
        assets,
    };
    shot.execute(session, &shoot::setting_actions(manifest, setting))
        .map_err(|e| format!("{module} (placing the camera): {e}"))?;
    let mut recovered = false;
    for value in values_to_shoot(setting) {
        let started = Instant::now();
        let shot = Shot {
            value: Some(value),
            ..shot
        };
        shoot_value(session, &shot, value, &mut recovered)?;
        let took = started.elapsed();
        println!("  {value}: {:.1} s", took.as_secs_f64());
        report.shots.push(took);
    }
    shot.execute(session, &shoot::reset_actions(manifest, setting))
        .map_err(|e| format!("{module} (back to base): {e}"))?;
    let distances = pair_distances(assets, setting)?;
    let flagged = flag_same_looks(setting, &distances);
    println!("  rmse between pictures (same look at or under {SAME_LOOK_RMSE}):");
    for ((a, b), d) in &distances {
        let mark = if flagged.contains(&(a.clone(), b.clone())) {
            "  LOOKS THE SAME"
        } else {
            ""
        };
        println!("    {a:<16} {b:<16} {d:>8.2}{mark}");
    }
    report.flagged.extend(
        flagged
            .into_iter()
            .map(|(a, b)| format!("{module}: {a} ~ {b}")),
    );
    Ok(())
}

/// Shoots one value. If the server kicked the player meanwhile, it reloads the map, rejoins,
/// places the camera again and retries the value; a second kick in the setting stops the run.
fn shoot_value(
    session: &mut Session,
    shot: &Shot,
    value: &str,
    recovered: &mut bool,
) -> Result<(), String> {
    let (manifest, setting) = (shot.manifest, shot.setting);
    let module = setting.module.as_str();
    loop {
        let since = log_len(&session.log());
        let result = shot.execute(session, &shoot::value_actions(manifest, setting, value));
        if !shoot::kick_seen(&log_since(&session.log(), since)) {
            return result.map_err(|e| format!("{module} = {value}: {e}"));
        }
        if *recovered {
            return Err(format!(
                "{module} = {value}: kicked from the server a second time"
            ));
        }
        *recovered = true;
        println!("  {value}: kicked from the server; reloading the map and retrying");
        let map = setting.map.as_deref().unwrap_or(&manifest.map);
        let mut rejoin = vec![
            ShotAction::Run(vec![format!("map {map}")]),
            ShotAction::Rejoin,
        ];
        rejoin.extend(shoot::setting_actions(manifest, setting));
        shot.execute(session, &rejoin)
            .map_err(|e| format!("{module} = {value} (rejoining after a kick): {e}"))?;
    }
}

/// Two values of a setting and the RMSE between their pictures.
type Distance = ((String, String), f64);

/// The RMSE of every pair of the setting's new pictures, decoded to RGB; a pair whose sizes
/// differ is left out with a note.
fn pair_distances(assets: &Path, setting: &SettingShot) -> Result<Vec<Distance>, String> {
    let values = values_to_shoot(setting);
    let mut pixels = Vec::new();
    for value in &values {
        let path = assets.join(&setting.module).join(format!("{value}.webp"));
        let image = image::open(&path).map_err(|e| format!("decoding {}: {e}", path.display()))?;
        pixels.push(image.to_rgb8().into_raw());
    }
    let mut distances = Vec::new();
    for (i, a) in values.iter().enumerate() {
        for (j, b) in values.iter().enumerate().skip(i + 1) {
            match shoot::rmse(&pixels[i], &pixels[j]) {
                Some(d) => distances.push((((*a).to_owned(), (*b).to_owned()), d)),
                None => println!("    {a} and {b} differ in size; not compared"),
            }
        }
    }
    Ok(distances)
}

/// The pairs at or under `SAME_LOOK_RMSE` that the manifest does not already join by `same_as`.
fn flag_same_looks(setting: &SettingShot, distances: &[Distance]) -> Vec<(String, String)> {
    let same_as = |value: &str, other: &str| {
        setting
            .values
            .iter()
            .any(|v| v.value == value && v.same_as.as_deref() == Some(other))
    };
    distances
        .iter()
        .filter(|((a, b), d)| *d <= SAME_LOOK_RMSE && !same_as(a, b) && !same_as(b, a))
        .map(|(pair, _)| pair.clone())
        .collect()
}

/// The `magick` arguments that turn a screenshot into a picture of exactly `size`.
fn convert_args(jpg: &Path, webp: &Path, [width, height]: [u32; 2]) -> Vec<String> {
    vec![
        jpg.display().to_string(),
        "-resize".to_owned(),
        format!("{width}x{height}!"),
        "-quality".to_owned(),
        "80".to_owned(),
        webp.display().to_string(),
    ]
}

/// What the run prints at the end: every shot's duration and the look-alike pairs.
struct Report {
    started: Instant,
    shots: Vec<Duration>,
    flagged: Vec<String>,
}

impl Report {
    fn new() -> Self {
        Self {
            started: Instant::now(),
            shots: Vec::new(),
            flagged: Vec::new(),
        }
    }

    fn print(&self) {
        let seconds: Vec<f64> = self.shots.iter().map(Duration::as_secs_f64).collect();
        let mean = seconds.iter().sum::<f64>() / seconds.len().max(1) as f64;
        let max = seconds.iter().copied().fold(0.0, f64::max);
        println!("===== summary =====");
        println!("shots:          {}", seconds.len());
        println!("per shot:       mean {mean:.1} s, max {max:.1} s");
        println!(
            "total:          {:.1} s",
            self.started.elapsed().as_secs_f64()
        );
        println!("look-alike pairs (rmse at or under {SAME_LOOK_RMSE}, not marked same_as):");
        if self.flagged.is_empty() {
            println!("  none");
        }
        for pair in &self.flagged {
            println!("  {pair}");
        }
    }
}

/// The context one action list runs in: the setting, and the value when one is being shot.
#[derive(Clone, Copy)]
struct Shot<'a> {
    manifest: &'a PictureManifest,
    setting: &'a SettingShot,
    value: Option<&'a str>,
    assets: &'a Path,
}

impl Shot<'_> {
    /// Runs the actions in order against the live game; the first failure stops the list.
    fn execute(&self, session: &mut Session, actions: &[ShotAction]) -> Result<(), String> {
        for action in actions {
            match action {
                ShotAction::Run(commands) => session.send(commands)?,
                ShotAction::Sleep(ms) => thread::sleep(Duration::from_millis(*ms)),
                ShotAction::Rejoin => {
                    let since = session.last_send;
                    session.join_when_ready(since)?;
                }
                ShotAction::Restart => {
                    let value = self.value()?;
                    session.quit_game()?;
                    let map = self.setting.map.as_deref().unwrap_or(&self.manifest.map);
                    session.launch(self.manifest, map, Some((&self.setting.module, value)))?;
                }
                ShotAction::Jpeg(name) => {
                    let webp = self
                        .assets
                        .join(&self.setting.module)
                        .join(format!("{}.webp", self.value()?));
                    session.take_picture(name, &webp, self.manifest.size)?;
                }
                other => return Err(format!("the shooter cannot run {other:?}")),
            }
        }
        Ok(())
    }

    fn value(&self) -> Result<&str, String> {
        self.value
            .ok_or_else(|| "a restart or screenshot outside a value".to_owned())
    }
}

fn parse_args(mut args: impl Iterator<Item = String>) -> Result<Args, String> {
    let mut parsed = Args {
        only: None,
        window_at: (0, 0),
        assets: Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/setting-pictures"),
        dry_run: false,
    };
    while let Some(flag) = args.next() {
        let mut value = || args.next().ok_or_else(|| format!("{flag} needs a value"));
        match flag.as_str() {
            "--dry-run" => parsed.dry_run = true,
            "--only" => parsed.only = Some(value()?),
            "--assets" => parsed.assets = PathBuf::from(value()?),
            "--window-at" => {
                let point = value()?;
                parsed.window_at = parse_point(&point)
                    .ok_or_else(|| format!("--window-at wants X,Y, not {point}"))?;
            }
            other => return Err(format!("unknown argument {other}")),
        }
    }
    Ok(parsed)
}

fn parse_point(text: &str) -> Option<(i32, i32)> {
    let (x, y) = text.split_once(',')?;
    Some((x.trim().parse().ok()?, y.trim().parse().ok()?))
}

/// The Steam root holding `steamapps/common/<game>`.
fn steam_root(game: &Path) -> Option<PathBuf> {
    game.parent()?.parent()?.parent().map(Path::to_path_buf)
}

/// The window position from `xdotool getwindowgeometry --shell` output.
fn parse_geometry(out: &str) -> Option<(i32, i32)> {
    let field = |key: &str| {
        out.lines()
            .find_map(|line| line.strip_prefix(key)?.trim().parse().ok())
    };
    Some((field("X=")?, field("Y=")?))
}

fn read_manifest(assets: &Path) -> Result<PictureManifest, String> {
    let path = assets.join("manifest.json");
    let bytes = fs::read(&path).map_err(|e| format!("reading {}: {e}", path.display()))?;
    PictureManifest::parse(&bytes).map_err(|e| e.to_string())
}

/// The settings with pictures, or the one `--only` names.
fn select<'m>(
    manifest: &'m PictureManifest,
    only: Option<&str>,
) -> Result<Vec<&'m SettingShot>, String> {
    if let Some(module) = only {
        let setting = manifest
            .setting(module)
            .ok_or_else(|| format!("no setting named {module} in the manifest"))?;
        if setting.no_picture.is_some() {
            return Err(format!("{module} has no pictures"));
        }
    }
    Ok(manifest
        .settings
        .iter()
        .filter(|s| s.no_picture.is_none() && only.is_none_or(|m| s.module == m))
        .collect())
}

/// The run's plan as text: every step file it writes, with waits, joins, restarts and
/// screenshots as `//` comment lines. Step numbers start again at 1 after a restart.
fn dry_run(manifest: &PictureManifest, settings: &[&SettingShot]) -> String {
    let mut out = format!(
        "// launch: steam {}\n",
        shoot::launch_args(&manifest.map).join(" ")
    );
    let mut step = 1;
    render_join(&mut out, &mut step);
    for setting in shoot_order(settings) {
        out.push_str(&format!("// ===== {} =====\n", setting.module));
        render(
            &mut out,
            &mut step,
            &shoot::setting_actions(manifest, setting),
            None,
        );
        for value in values_to_shoot(setting) {
            out.push_str(&format!("// ----- {} = {value} -----\n", setting.module));
            let actions = shoot::value_actions(manifest, setting, value);
            render(
                &mut out,
                &mut step,
                &actions,
                Some((&setting.module, value)),
            );
        }
        render(
            &mut out,
            &mut step,
            &shoot::reset_actions(manifest, setting),
            None,
        );
    }
    out
}

fn render(out: &mut String, step: &mut u32, actions: &[ShotAction], value: Option<(&str, &str)>) {
    for action in actions {
        match action {
            ShotAction::Run(commands) => render_step(out, step, commands),
            ShotAction::Sleep(ms) => out.push_str(&format!("// sleep {ms} ms\n")),
            ShotAction::Rejoin => render_join(out, step),
            ShotAction::Restart => {
                let (module, shown) = value.unwrap_or(("?", "?"));
                out.push_str(&format!(
                    "// restart: quit TF2, relaunch with modules.cfg {module}={shown}\n"
                ));
                *step = 1;
                render_join(out, step);
            }
            ShotAction::Jpeg(name) => out.push_str(&format!("// screenshot {name}\n")),
            other => out.push_str(&format!("// {other:?}\n")),
        }
    }
}

fn render_step(out: &mut String, step: &mut u32, commands: &[String]) {
    out.push_str(&format!("// step_{step}.cfg\n"));
    out.push_str(&shoot::step_cfg(*step, commands));
    *step += 1;
}

/// The wait for the map and the two join sends that follow every map load.
fn render_join(out: &mut String, step: &mut u32) {
    let [settle, between, after] = JOIN_SLEEPS;
    out.push_str(&format!(
        "// wait for SV_ActivateServer, sleep {settle} ms\n"
    ));
    render_step(out, step, &shoot::join_commands());
    out.push_str(&format!("// sleep {between} ms\n"));
    render_step(out, step, &shoot::join_commands());
    out.push_str(&format!("// sleep {after} ms\n"));
}

/// The folders whose files the run backs up, named for the backup's subfolders:
/// the game's cfg folder and every Steam account's TF2 folder.
fn backup_roots(game: &Path, steam: &Path) -> Vec<(String, PathBuf)> {
    let mut roots = vec![("tf-cfg".to_owned(), game.join("tf").join("cfg"))];
    if let Ok(accounts) = fs::read_dir(steam.join("userdata")) {
        for account in accounts.flatten() {
            let tf2 = account.path().join("440");
            if tf2.is_dir() {
                let id = account.file_name().to_string_lossy().into_owned();
                roots.push((format!("userdata-{id}-440"), tf2));
            }
        }
    }
    roots
}

/// A fresh folder that stays on disk after the process ends.
fn backup_dir() -> Result<PathBuf, String> {
    tempfile::Builder::new()
        .prefix("tf2tools-shots-backup-")
        .tempdir()
        .map(tempfile::TempDir::keep)
        .map_err(|e| format!("creating the backup folder: {e}"))
}

/// The player's files as they were before the run.
struct Backup {
    dir: PathBuf,
    roots: Vec<(String, PathBuf)>,
    /// Each original file with its copy.
    files: Vec<(PathBuf, PathBuf)>,
}

/// What a restore did; `failures` makes the run exit 1.
struct Restored {
    count: usize,
    extras: Vec<PathBuf>,
    failures: Vec<String>,
}

impl Backup {
    /// Copies every file under each root into `dir/<name>/`, keeping relative paths.
    fn take(roots: &[(String, PathBuf)], dir: &Path) -> Result<Self, String> {
        let mut files = Vec::new();
        for (name, root) in roots.iter().filter(|(_, root)| root.is_dir()) {
            for entry in WalkDir::new(root) {
                let entry = entry.map_err(|e| format!("reading {}: {e}", root.display()))?;
                if !entry.file_type().is_file() {
                    continue;
                }
                let original = entry.path().to_owned();
                let relative = original.strip_prefix(root).map_err(|e| e.to_string())?;
                let copy = dir.join(name).join(relative);
                if let Some(parent) = copy.parent() {
                    fs::create_dir_all(parent).map_err(|e| e.to_string())?;
                }
                fs::copy(&original, &copy)
                    .map_err(|e| format!("backing up {}: {e}", original.display()))?;
                files.push((original, copy));
            }
        }
        Ok(Self {
            dir: dir.to_owned(),
            roots: roots.to_vec(),
            files,
        })
    }

    /// Copies every file back and checks its bytes; one failure never stops the rest.
    /// Files the run left behind are listed, never deleted.
    fn restore(&self) -> Restored {
        let mut failures = Vec::new();
        for (original, copy) in &self.files {
            let put_back = original
                .parent()
                .map_or(Ok(()), fs::create_dir_all)
                .and_then(|()| fs::copy(copy, original));
            let same = put_back.is_ok()
                && matches!((fs::read(original), fs::read(copy)), (Ok(a), Ok(b)) if a == b);
            if !same {
                failures.push(format!("{} was not restored", original.display()));
            }
        }
        let known: HashSet<&Path> = self.files.iter().map(|(o, _)| o.as_path()).collect();
        let extras = self
            .roots
            .iter()
            .flat_map(|(_, root)| WalkDir::new(root).into_iter().flatten())
            .filter(|e| e.file_type().is_file() && !known.contains(e.path()))
            .map(|e| e.path().to_owned())
            .collect();
        Restored {
            count: self.files.len() - failures.len(),
            extras,
            failures,
        }
    }
}

/// Readies the scratch folder for a launch: no stale step files, a fresh `start.cfg`, the
/// `modules.cfg` mastercomfig reads at startup with `value` in place, and no old screenshots.
fn prepare_scratch(
    tf: &Path,
    manifest: &PictureManifest,
    value: Option<(&str, &str)>,
) -> Result<(), String> {
    let tfs = tfs_dir(tf);
    fs::create_dir_all(&tfs).map_err(|e| e.to_string())?;
    for entry in fs::read_dir(&tfs).map_err(|e| e.to_string())?.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if (name.starts_with("step_") && name.ends_with(".cfg")) || name == STEP_TEMP {
            fs::remove_file(entry.path()).map_err(|e| e.to_string())?;
        }
    }
    fs::write(tfs.join("start.cfg"), shoot::start_cfg()).map_err(|e| e.to_string())?;
    let overrides = tf.join(SCRATCH).join("cfg").join("overrides");
    fs::create_dir_all(&overrides).map_err(|e| e.to_string())?;
    fs::write(
        overrides.join("modules.cfg"),
        shoot::modules_cfg(manifest, value),
    )
    .map_err(|e| e.to_string())?;
    clear_screenshots(tf)
}

fn tfs_dir(tf: &Path) -> PathBuf {
    tf.join(SCRATCH).join("cfg").join("tfs")
}

/// Removes the run's `tfs_*` screenshots and leaves the player's own.
fn clear_screenshots(tf: &Path) -> Result<(), String> {
    let Ok(entries) = fs::read_dir(tf.join("screenshots")) else {
        return Ok(());
    };
    for entry in entries.flatten() {
        if entry.file_name().to_string_lossy().starts_with("tfs_") {
            fs::remove_file(entry.path()).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

/// Quits TF2, restores the backup and removes the scratch folder and screenshots.
fn finish(session: &mut Session, backup: &Backup) -> Result<(), String> {
    let mut problems = Vec::new();
    if let Err(e) = session.quit_game() {
        problems.push(e);
    }
    let restored = backup.restore();
    let scratch = session.tf.join(SCRATCH);
    if scratch.exists()
        && let Err(e) = fs::remove_dir_all(&scratch)
    {
        problems.push(format!("removing {}: {e}", scratch.display()));
    }
    if let Err(e) = clear_screenshots(&session.tf) {
        problems.push(e);
    }
    for extra in &restored.extras {
        println!("left in place: {}", extra.display());
    }
    println!(
        "restored {} files, {} extra files left",
        restored.count,
        restored.extras.len()
    );
    problems.extend(restored.failures);
    if problems.is_empty() {
        Ok(())
    } else {
        Err(problems.join("; "))
    }
}

/// The window guard: a thread that keeps the TF2 window at the target position.
struct Guard {
    stop: Arc<AtomicBool>,
    failure: Arc<OnceLock<String>>,
    handle: JoinHandle<()>,
}

impl Guard {
    /// Every half second, finds the TF2 window and moves it back to `target` if it drifted.
    /// It only moves the window; it never focuses it or sends input.
    fn start(target: (i32, i32)) -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let failure = Arc::new(OnceLock::new());
        let (stop_flag, failed) = (Arc::clone(&stop), Arc::clone(&failure));
        let handle = thread::spawn(move || {
            let started = Instant::now();
            let mut found = false;
            while !stop_flag.load(Ordering::Relaxed) {
                match pid_of(TF2_PROCESS).and_then(|pid| window_of(&pid)) {
                    Some(window) => {
                        found = true;
                        if let Err(e) = place_window(&window, target) {
                            let _ = failed.set(e);
                            return;
                        }
                    }
                    None if !found && started.elapsed() > WINDOW_APPEARS => {
                        let _ = failed.set(format!(
                            "no TF2 window within {} s of launch",
                            WINDOW_APPEARS.as_secs()
                        ));
                        return;
                    }
                    None => {}
                }
                thread::sleep(GUARD_POLL);
            }
        });
        Self {
            stop,
            failure,
            handle,
        }
    }

    fn stop(self) {
        self.stop.store(true, Ordering::Relaxed);
        let _ = self.handle.join();
    }
}

fn output_of(program: &str, args: &[&str]) -> Option<String> {
    let out = Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).into_owned())
}

fn pid_of(process: &str) -> Option<String> {
    let out = output_of("pgrep", &["-x", process])?;
    out.lines().next().map(|pid| pid.trim().to_owned())
}

fn window_of(pid: &str) -> Option<String> {
    let out = output_of("xdotool", &["search", "--pid", pid])?;
    out.lines()
        .rfind(|line| !line.trim().is_empty())
        .map(|id| id.trim().to_owned())
}

fn window_position(window: &str) -> Option<(i32, i32)> {
    parse_geometry(&output_of(
        "xdotool",
        &["getwindowgeometry", "--shell", window],
    )?)
}

fn place_window(window: &str, target: (i32, i32)) -> Result<(), String> {
    match window_position(window) {
        None => Ok(()),
        Some(at) if at == target => Ok(()),
        Some(_) => {
            let (x, y) = (target.0.to_string(), target.1.to_string());
            let _ = output_of("xdotool", &["windowmove", window, &x, &y]);
            match window_position(window) {
                Some(at) if at != target => {
                    Err(format!("the TF2 window stays at {at:?}, not {target:?}"))
                }
                _ => Ok(()),
            }
        }
    }
}

/// Starts a program that outlives this one, with no terminal attached.
fn spawn_detached(program: &str, args: &[String]) -> Result<(), String> {
    let mut child = Command::new(program)
        .args(args)
        .env("SDL_VIDEODRIVER", "x11")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("starting {program}: {e}"))?;
    thread::spawn(move || child.wait());
    Ok(())
}

/// Starts Steam if it is not running and waits for it; `setsid` keeps a Ctrl-C here from
/// reaching it.
fn ensure_steam() -> Result<(), String> {
    if pid_of("steam").is_some() {
        return Ok(());
    }
    spawn_detached("setsid", &["steam".to_owned(), "-silent".to_owned()])?;
    let deadline = Instant::now() + STEAM_START;
    while pid_of("steam").is_none() {
        if Instant::now() >= deadline {
            return Err("Steam did not start within 60 s".to_owned());
        }
        thread::sleep(POLL);
    }
    Ok(())
}

fn log_len(log: &Path) -> u64 {
    fs::metadata(log).map_or(0, |m| m.len())
}

/// The console log's text after byte `since`, or all of it if the log was cut shorter.
fn log_since(log: &Path, since: u64) -> String {
    let Ok(mut file) = fs::File::open(log) else {
        return String::new();
    };
    let start = if log_len(log) < since { 0 } else { since };
    let mut bytes = Vec::new();
    if file.seek(SeekFrom::Start(start)).is_err() || file.read_to_end(&mut bytes).is_err() {
        return String::new();
    }
    String::from_utf8_lossy(&bytes).into_owned()
}

/// One TF2 launch driven through step files and read back through `console.log`.
struct Session {
    tf: PathBuf,
    window_at: (i32, i32),
    /// The number the next step file gets.
    step: u32,
    /// The log length just before the last step file was written.
    last_send: u64,
    /// When the last step file was written, to pace the next one.
    last_sent_at: Option<Instant>,
    /// Whether TF2 has been seen running since the last launch.
    tf2_seen: bool,
    guard: Option<Guard>,
}

impl Session {
    fn new(tf: PathBuf, window_at: (i32, i32)) -> Self {
        Self {
            tf,
            window_at,
            step: 1,
            last_send: 0,
            last_sent_at: None,
            tf2_seen: false,
            guard: None,
        }
    }

    fn log(&self) -> PathBuf {
        self.tf.join("console.log")
    }

    /// Launches TF2 on `map` with `value` (module, value) in modules.cfg, then waits for the
    /// map and joins. Used for the first launch and for every restart.
    fn launch(
        &mut self,
        manifest: &PictureManifest,
        map: &str,
        value: Option<(&str, &str)>,
    ) -> Result<(), String> {
        self.stop_guard();
        prepare_scratch(&self.tf, manifest, value)?;
        self.step = 1;
        self.tf2_seen = false;
        ensure_steam()?;
        let since = log_len(&self.log());
        self.last_send = since;
        spawn_detached("steam", &shoot::launch_args(map))?;
        self.guard = Some(Guard::start(self.window_at));
        self.join_when_ready(since)
    }

    /// Waits for a map that started loading after log byte `since`, then joins BLU twice,
    /// because the first join can land while the team menu is still opening.
    fn join_when_ready(&mut self, since: u64) -> Result<(), String> {
        self.wait_log(since, MAP_READY, "the map to load", |log| {
            log.contains("SV_ActivateServer")
        })?;
        let [settle, between, after] = JOIN_SLEEPS.map(Duration::from_millis);
        thread::sleep(settle);
        self.send(&shoot::join_commands())?;
        thread::sleep(between);
        self.send(&shoot::join_commands())?;
        thread::sleep(after);
        Ok(())
    }

    /// Writes the next step file and waits for its marker in the console log.
    fn send(&mut self, commands: &[String]) -> Result<(), String> {
        let n = self.write_step(commands)?;
        let since = self.last_send;
        self.wait_log(since, STEP_DONE, &format!("step {n}"), |log| {
            shoot::marker_seen(log, n)
        })
    }

    /// Writes step `n` through a temp file and rename so the game never reads half a file,
    /// at least `SEND_GAP` after the previous step.
    fn write_step(&mut self, commands: &[String]) -> Result<u32, String> {
        if let Some(last) = self.last_sent_at {
            thread::sleep(send_gap(last, Instant::now()));
        }
        let n = self.step;
        let tfs = tfs_dir(&self.tf);
        let temp = tfs.join(STEP_TEMP);
        self.last_send = log_len(&self.log());
        fs::write(&temp, shoot::step_cfg(n, commands))
            .and_then(|()| fs::rename(&temp, tfs.join(format!("step_{n}.cfg"))))
            .map_err(|e| format!("writing step {n}: {e}"))?;
        self.step += 1;
        self.last_sent_at = Some(Instant::now());
        Ok(n)
    }

    /// Polls the log after `since` until `found` holds; fails early when the window guard
    /// failed or TF2 exited.
    fn wait_log(
        &mut self,
        since: u64,
        timeout: Duration,
        what: &str,
        found: impl Fn(&str) -> bool,
    ) -> Result<(), String> {
        let deadline = Instant::now() + timeout;
        loop {
            if found(&log_since(&self.log(), since)) {
                return Ok(());
            }
            self.check_alive()?;
            if Instant::now() >= deadline {
                return Err(format!(
                    "timed out after {} s waiting for {what}",
                    timeout.as_secs()
                ));
            }
            thread::sleep(POLL);
        }
    }

    fn check_alive(&mut self) -> Result<(), String> {
        if let Some(failure) = self.guard.as_ref().and_then(|g| g.failure.get()) {
            return Err(format!("window guard: {failure}"));
        }
        if tf2_running() {
            self.tf2_seen = true;
        } else if self.tf2_seen {
            return Err("TF2 exited".to_owned());
        }
        Ok(())
    }

    fn stop_guard(&mut self) {
        if let Some(guard) = self.guard.take() {
            guard.stop();
        }
    }

    /// Takes screenshot `name`, waits for the game to finish writing it, converts it to
    /// `webp` at `size` with magick and removes the screenshot.
    fn take_picture(&mut self, name: &str, webp: &Path, size: [u32; 2]) -> Result<(), String> {
        let jpg = self.tf.join("screenshots").join(format!("{name}.jpg"));
        if jpg.exists() {
            fs::remove_file(&jpg).map_err(|e| format!("removing {}: {e}", jpg.display()))?;
        }
        self.send(&[format!("jpeg {name} 95")])?;
        self.wait_saved(&jpg)?;
        if let Some(dir) = webp.parent() {
            fs::create_dir_all(dir).map_err(|e| format!("creating {}: {e}", dir.display()))?;
        }
        let out = Command::new("magick")
            .args(convert_args(&jpg, webp, size))
            .stdin(Stdio::null())
            .output()
            .map_err(|e| format!("starting magick: {e}"))?;
        if !out.status.success() {
            return Err(format!(
                "magick could not convert {}: {}",
                jpg.display(),
                String::from_utf8_lossy(&out.stderr).trim()
            ));
        }
        fs::remove_file(&jpg).map_err(|e| format!("removing {}: {e}", jpg.display()))
    }

    /// Waits until the screenshot exists and its size holds across two polls.
    fn wait_saved(&mut self, jpg: &Path) -> Result<(), String> {
        let deadline = Instant::now() + SHOT_SAVED;
        let mut last = None;
        loop {
            let size = fs::metadata(jpg).ok().map(|m| m.len()).filter(|&n| n > 0);
            if size.is_some() && size == last {
                return Ok(());
            }
            last = size;
            self.check_alive()?;
            if Instant::now() >= deadline {
                return Err(format!(
                    "{} was not saved within {} s",
                    jpg.display(),
                    SHOT_SAVED.as_secs()
                ));
            }
            thread::sleep(POLL);
        }
    }

    /// Quits TF2 after re-reading the player's config, so the config TF2 writes on quit is
    /// theirs; kills it if it has not exited within a minute.
    fn quit_game(&mut self) -> Result<(), String> {
        self.stop_guard();
        if !tf2_running() {
            return Ok(());
        }
        let quit = ["exec config.cfg".to_owned(), "quit".to_owned()];
        let _ = self.write_step(&quit);
        if wait_for_exit(GAME_QUITS) {
            return Ok(());
        }
        let _ = output_of("pkill", &["-x", TF2_PROCESS]);
        if wait_for_exit(Duration::from_secs(10)) {
            Ok(())
        } else {
            Err("TF2 is still running".to_owned())
        }
    }
}

/// How long to wait before the next send so it lands `SEND_GAP` after the one at `last`.
fn send_gap(last: Instant, now: Instant) -> Duration {
    SEND_GAP.saturating_sub(now.saturating_duration_since(last))
}

fn wait_for_exit(timeout: Duration) -> bool {
    let deadline = Instant::now() + timeout;
    while tf2_running() {
        if Instant::now() >= deadline {
            return false;
        }
        thread::sleep(POLL);
    }
    true
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    fn args(list: &[&str]) -> Result<Args, String> {
        parse_args(list.iter().map(|s| (*s).to_owned()))
    }

    #[test]
    fn defaults_shoot_everything_at_the_top_left() {
        let a = args(&[]).unwrap();
        assert_eq!(a.only, None);
        assert_eq!(a.window_at, (0, 0));
        assert!(!a.dry_run);
        assert!(a.assets.ends_with("assets/setting-pictures"));
    }

    #[test]
    fn options_are_read() {
        let a = args(&[
            "--only",
            "shadows",
            "--window-at",
            "10,20",
            "--dry-run",
            "--assets",
            "/x",
        ])
        .unwrap();
        assert_eq!(a.only.as_deref(), Some("shadows"));
        assert_eq!(a.window_at, (10, 20));
        assert!(a.dry_run);
        assert_eq!(a.assets, std::path::PathBuf::from("/x"));
    }

    #[test]
    fn bad_options_are_refused() {
        assert!(args(&["--window-at", "10"]).is_err());
        assert!(args(&["--only"]).is_err());
        assert!(args(&["--frobnicate"]).is_err());
    }

    #[test]
    fn the_steam_root_is_three_levels_above_the_game() {
        let game = std::path::Path::new("/s/steamapps/common/Team Fortress 2");
        assert_eq!(steam_root(game), Some(std::path::PathBuf::from("/s")));
    }

    #[test]
    fn window_geometry_is_read_from_xdotool_shell_output() {
        let out = "WINDOW=44040239\nX=1440\nY=0\nWIDTH=1440\nHEIGHT=810\nSCREEN=0\n";
        assert_eq!(parse_geometry(out), Some((1440, 0)));
        assert_eq!(parse_geometry("WINDOW=1\n"), None);
    }

    fn tiny_manifest() -> PictureManifest {
        PictureManifest::parse(br#"{"version":1,"mastercomfig":"9.100.1","size":[960,540],"map":"ctf_2fort","base":{"shadows":"high"},
            "settings":[{"module":"shadows","values":[{"value":"low"}]}]}"#).unwrap()
    }

    #[test]
    fn a_restore_puts_every_file_back_and_lists_new_ones() {
        let game = tempfile::tempdir().unwrap();
        let store = tempfile::tempdir().unwrap();
        let cfg = game.path().join("cfg");
        fs::create_dir_all(cfg.join("sub")).unwrap();
        fs::write(cfg.join("config.cfg"), "theirs").unwrap();
        fs::write(cfg.join("sub/autoexec.cfg"), "mine").unwrap();
        let backup = Backup::take(&[("tf-cfg".to_owned(), cfg.clone())], store.path()).unwrap();

        fs::write(cfg.join("config.cfg"), "changed by the run").unwrap();
        fs::remove_file(cfg.join("sub/autoexec.cfg")).unwrap();
        fs::write(cfg.join("new.cfg"), "added").unwrap();
        let restored = backup.restore();

        assert_eq!(
            fs::read_to_string(cfg.join("config.cfg")).unwrap(),
            "theirs"
        );
        assert_eq!(
            fs::read_to_string(cfg.join("sub/autoexec.cfg")).unwrap(),
            "mine"
        );
        assert_eq!(restored.count, 2);
        assert!(restored.failures.is_empty(), "{:?}", restored.failures);
        assert_eq!(restored.extras, vec![cfg.join("new.cfg")]);
    }

    #[test]
    fn a_launch_starts_from_a_clean_scratch_folder() {
        let tf = tempfile::tempdir().unwrap();
        let tfs = tfs_dir(tf.path());
        fs::create_dir_all(&tfs).unwrap();
        fs::write(tfs.join("step_12.cfg"), "stale").unwrap();
        fs::write(tfs.join(STEP_TEMP), "half").unwrap();
        let shots = tf.path().join("screenshots");
        fs::create_dir_all(&shots).unwrap();
        fs::write(shots.join("tfs_shadows_low.jpg"), "old").unwrap();
        fs::write(shots.join("theirs.jpg"), "keep").unwrap();
        let manifest = tiny_manifest();

        prepare_scratch(tf.path(), &manifest, Some(("shadows", "low"))).unwrap();

        let mut left: Vec<_> = fs::read_dir(&tfs)
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        left.sort();
        assert_eq!(left, vec!["start.cfg"]);
        assert_eq!(
            fs::read_to_string(tfs.join("start.cfg")).unwrap(),
            shoot::start_cfg()
        );
        let modules = tf.path().join(SCRATCH).join("cfg/overrides/modules.cfg");
        assert_eq!(fs::read_to_string(modules).unwrap(), "shadows=low\n");
        assert!(
            !shots.join("tfs_shadows_low.jpg").exists(),
            "old shots are removed"
        );
        assert!(shots.join("theirs.jpg").exists(), "the player's shots stay");
    }

    #[test]
    fn flagged_pairs_skip_values_already_marked_same() {
        let m = tf2tools_core::PictureManifest::parse(br#"{"version":1,"mastercomfig":"9.100.1","size":[960,540],"map":"ctf_2fort","base":{},
            "settings":[{"module":"gibs","values":[{"value":"off"},{"value":"low"},{"value":"high","same_as":"low","reason":"same-in-still"}]}]}"#).unwrap();
        let gibs = m.setting("gibs").unwrap();
        let distances = vec![
            (("off".to_owned(), "low".to_owned()), 12.0),
            (("low".to_owned(), "off".to_owned()), 0.1),
        ];
        assert_eq!(
            flag_same_looks(gibs, &distances),
            vec![("low".to_owned(), "off".to_owned())]
        );
    }

    #[test]
    fn the_shot_values_leave_out_same_as_values() {
        let m = tf2tools_core::PictureManifest::parse(br#"{"version":1,"mastercomfig":"9.100.1","size":[960,540],"map":"ctf_2fort","base":{},
            "settings":[{"module":"gibs","values":[{"value":"off"},{"value":"high","same_as":"off","reason":"same-in-still"}]}]}"#).unwrap();
        assert_eq!(values_to_shoot(m.setting("gibs").unwrap()), vec!["off"]);
    }

    #[test]
    fn magick_resizes_to_the_manifest_size() {
        let cmd = convert_args(
            std::path::Path::new("/a/x.jpg"),
            std::path::Path::new("/b/y.webp"),
            [960, 540],
        );
        assert_eq!(
            cmd,
            [
                "/a/x.jpg",
                "-resize",
                "960x540!",
                "-quality",
                "80",
                "/b/y.webp"
            ]
        );
    }

    #[test]
    fn sends_are_spaced_at_least_300_ms_apart() {
        let last = Instant::now();
        assert_eq!(send_gap(last, last), Duration::from_millis(300));
        assert_eq!(
            send_gap(last, last + Duration::from_millis(100)),
            Duration::from_millis(200)
        );
        assert_eq!(
            send_gap(last, last + Duration::from_secs(2)),
            Duration::ZERO
        );
    }

    #[test]
    fn restart_settings_are_shot_last_in_manifest_order() {
        let m = PictureManifest::parse(br#"{"version":1,"mastercomfig":"9.100.1","size":[960,540],"map":"ctf_2fort","base":{},
            "settings":[{"module":"a","reload":"restart","values":[{"value":"x"}]},{"module":"b","values":[{"value":"x"}]},
            {"module":"c","reload":"restart","values":[{"value":"x"}]},{"module":"d","reload":"map","values":[{"value":"x"}]}]}"#).unwrap();
        let all: Vec<_> = m.settings.iter().collect();
        let order: Vec<_> = shoot_order(&all)
            .iter()
            .map(|s| s.module.as_str())
            .collect();
        assert_eq!(order, ["b", "d", "a", "c"]);
    }
}
