//! Times the v6 app's startup and frames inside a private KWin session.
//!
//! `session --scratch <dir> --out <file> -- <command> [args...]` runs one command in a private
//! KWin session and writes its window damage log. `seed-synthetic` writes a data template and
//! `seed-store` adds store rows to it; `v6-startup` and `v6-frames` run the app from one, the
//! latter optionally against a loopback fake GameBanana (see USAGE). Linux only: elsewhere `main`
//! just says so, since the session, `/proc` reads and runs are all Linux desktop specifics.

#[cfg(target_os = "linux")]
mod fake_gb;
#[cfg(target_os = "linux")]
mod guard;
#[cfg(target_os = "linux")]
mod report;
#[cfg(target_os = "linux")]
mod runs;
#[cfg(target_os = "linux")]
mod session;
#[cfg(target_os = "linux")]
mod store_seed;

#[cfg(target_os = "linux")]
use std::collections::HashMap;
#[cfg(target_os = "linux")]
use std::ffi::OsString;
#[cfg(target_os = "linux")]
use std::path::{Path, PathBuf};
use std::process::ExitCode;
#[cfg(target_os = "linux")]
use std::time::Duration;

#[cfg(target_os = "linux")]
use tf2tools_app::timing::Case;

#[cfg(target_os = "linux")]
const USAGE: &str = "usage: app_timing session --scratch <dir> --out <file> -- <command> [args...]
       app_timing seed-synthetic --huds <n> --crosshairs <n> --out <tpl> [--logs <n>]
       app_timing seed-store --template <tpl> [--warm]
       app_timing v6-startup --bin <tf2tools> --assets <dir> --template <tpl> --scratch <dir> \
           [--runs 10] [--hold-ms 5000]
       app_timing v6-frames --bin <tf2tools> --assets <dir> --template <tpl> --scratch <dir> \
           [--cases all|<name>,<name>] [--fake [--image-delay-ms 150]]
       app_timing report --runs <file> --damage <file> [--quiet-ms 1000]";

#[cfg(target_os = "linux")]
fn main() -> ExitCode {
    let args: Vec<OsString> = std::env::args_os().skip(1).collect();
    match run(&args) {
        Ok(code) => ExitCode::from(u8::try_from(code).unwrap_or(1)),
        Err(e) => {
            eprintln!("app_timing: {e}\n{USAGE}");
            ExitCode::from(2)
        }
    }
}

#[cfg(not(target_os = "linux"))]
fn main() -> ExitCode {
    eprintln!("app_timing measures the Linux desktop session only");
    ExitCode::FAILURE
}

#[cfg(target_os = "linux")]
fn run(args: &[OsString]) -> Result<i32, String> {
    match args.first().and_then(|a| a.to_str()) {
        Some("session") => session_cmd(&args[1..]),
        Some("seed-synthetic") => seed_cmd(&args[1..]),
        Some("seed-store") => seed_store_cmd(&args[1..]),
        Some("v6-startup") => startup_cmd(&args[1..]),
        Some("v6-frames") => frames_cmd(&args[1..]),
        Some("report") => report_cmd(&args[1..]),
        _ => Err("unknown or missing subcommand".into()),
    }
}

#[cfg(target_os = "linux")]
fn session_cmd(args: &[OsString]) -> Result<i32, String> {
    // Options come before `--`; everything after it is the command, passed through untouched.
    let split = args
        .iter()
        .position(|a| a == "--")
        .ok_or("missing `--` before the command")?;
    let (opts, command) = (&args[..split], &args[split + 1..]);
    if command.is_empty() {
        return Err("missing command after `--`".into());
    }
    let (mut scratch, mut out) = (None, None);
    let mut it = opts.iter();
    while let Some(opt) = it.next() {
        let value = it
            .next()
            .map(PathBuf::from)
            .ok_or_else(|| format!("{} needs a value", opt.to_string_lossy()))?;
        match opt.to_str() {
            Some("--scratch") => scratch = Some(value),
            Some("--out") => out = Some(value),
            _ => return Err(format!("unknown option {}", opt.to_string_lossy())),
        }
    }
    let scratch = scratch.ok_or("missing --scratch")?;
    let out = out.ok_or("missing --out")?;
    // Resolved here, in the caller's own env, before the session clears it.
    let real = guard::real_data_dir()?;
    session::run_session(&scratch, &out, command, &real)
}

/// `--name value` pairs, each name one of `known`.
#[cfg(target_os = "linux")]
fn options(args: &[OsString], known: &[&str]) -> Result<HashMap<String, String>, String> {
    let mut out = HashMap::new();
    let mut it = args.iter();
    while let Some(opt) = it.next() {
        let opt = opt.to_string_lossy().into_owned();
        if !known.contains(&opt.as_str()) {
            return Err(format!("unknown option {opt}"));
        }
        let value = it
            .next()
            .and_then(|v| v.to_str())
            .ok_or_else(|| format!("{opt} needs a UTF-8 value"))?;
        out.insert(opt, value.to_string());
    }
    Ok(out)
}

#[cfg(target_os = "linux")]
fn required<'a>(opts: &'a HashMap<String, String>, name: &str) -> Result<&'a str, String> {
    opts.get(name)
        .map(String::as_str)
        .ok_or_else(|| format!("missing {name}"))
}

#[cfg(target_os = "linux")]
fn number(opts: &HashMap<String, String>, name: &str, default: u64) -> Result<u64, String> {
    opts.get(name).map_or(Ok(default), |v| {
        v.parse()
            .map_err(|_| format!("{name} must be a number: {v}"))
    })
}

/// Every scratch-like path is checked apart from the real data; `fresh` ones must also be empty.
#[cfg(target_os = "linux")]
fn guard_paths(scratch: &[&Path], fresh: &[&Path]) -> Result<(), String> {
    let real = guard::real_data_dir()?;
    for path in scratch.iter().chain(fresh) {
        guard::assert_scratch(path, &real)?;
    }
    fresh.iter().try_for_each(|path| guard::assert_fresh(path))
}

#[cfg(target_os = "linux")]
fn seed_cmd(args: &[OsString]) -> Result<i32, String> {
    let opts = options(args, &["--huds", "--crosshairs", "--out", "--logs"])?;
    let out = PathBuf::from(required(&opts, "--out")?);
    guard_paths(&[], &[&out])?;
    let huds = number(&opts, "--huds", 0)? as usize;
    let crosshairs = number(&opts, "--crosshairs", 0)? as usize;
    let counts = runs::seed_synthetic(&out, huds, crosshairs)?;
    println!(
        "seed huds={} crosshairs={} sounds={}",
        counts.huds, counts.crosshairs, counts.sounds
    );
    // The seeded cases (logs-scroll, profile-apply) need profiles and log rows on top.
    if opts.contains_key("--logs") {
        let m4 = runs::seed_m4(&out, number(&opts, "--logs", 0)? as usize)?;
        println!("profiles={} logs={}", m4.profiles, m4.logs);
    }
    Ok(0)
}

/// `args` without the bare `flag`, and whether it was there.
#[cfg(target_os = "linux")]
fn take_flag(args: &[OsString], flag: &str) -> (Vec<OsString>, bool) {
    let rest: Vec<OsString> = args.iter().filter(|a| *a != flag).cloned().collect();
    let found = rest.len() != args.len();
    (rest, found)
}

#[cfg(target_os = "linux")]
fn seed_store_cmd(args: &[OsString]) -> Result<i32, String> {
    let (args, warm) = take_flag(args, "--warm");
    let opts = options(&args, &["--template"])?;
    let template = PathBuf::from(required(&opts, "--template")?);
    guard_paths(&[&template], &[])?;
    let seed = store_seed::StoreSeed {
        warm_images: warm,
        ..store_seed::StoreSeed::REAL
    };
    let base = store_seed::fake_base(store_seed::FAKE_PORT);
    let urls = store_seed::seed_store(&runs::home_data(&template), &seed, &base)?;
    println!(
        "seed-store sounds={} gb_huds={} curated={} crosshairs={} addons={} images={} warm={warm}",
        seed.sounds,
        seed.gb_huds,
        seed.curated,
        seed.crosshairs,
        seed.addons,
        urls.len()
    );
    Ok(0)
}

#[cfg(target_os = "linux")]
const RUN_OPTIONS: [&str; 4] = ["--bin", "--assets", "--template", "--scratch"];

/// The four paths every app run needs, guarded before anything is spawned.
#[cfg(target_os = "linux")]
fn run_paths(opts: &HashMap<String, String>) -> Result<[PathBuf; 4], String> {
    let [bin, assets, template, scratch] =
        RUN_OPTIONS.map(|name| required(opts, name).map(PathBuf::from));
    let (template, scratch) = (template?, scratch?);
    guard_paths(&[&template], &[&scratch])?;
    Ok([bin?, assets?, template, scratch])
}

#[cfg(target_os = "linux")]
fn startup_cmd(args: &[OsString]) -> Result<i32, String> {
    let known = [&RUN_OPTIONS[..], &["--runs", "--hold-ms"]].concat();
    let opts = options(args, &known)?;
    // The summary's median needs at least one sample.
    let runs = number(&opts, "--runs", 10)? as usize;
    if runs == 0 {
        return Err("--runs must be at least 1".into());
    }
    let [bin, assets, template, scratch] = run_paths(&opts)?;
    let hold = Duration::from_millis(number(&opts, "--hold-ms", 5000)?);
    let setup = runs::RunSetup {
        bin: &bin,
        assets: &assets,
        template: &template,
        scratch: &scratch,
        loopback: None,
        fake: None,
    };
    runs::v6_startup(&setup, runs, hold)?;
    Ok(0)
}

#[cfg(target_os = "linux")]
fn frames_cmd(args: &[OsString]) -> Result<i32, String> {
    let (args, fake) = take_flag(args, "--fake");
    let known = [&RUN_OPTIONS[..], &["--cases", "--image-delay-ms"]].concat();
    let opts = options(&args, &known)?;
    if !fake && opts.contains_key("--image-delay-ms") {
        return Err("--image-delay-ms needs --fake".into());
    }
    let cases: Vec<Case> = match opts.get("--cases").map_or("all", String::as_str) {
        "all" => all_cases(),
        list => list
            .split(',')
            .map(|name| Case::from_name(name).ok_or_else(|| format!("unknown case {name}")))
            .collect::<Result<Vec<_>, _>>()?,
    };
    // Seeded store images live on the fake's port, so a store case without it would time an
    // imageless grid and still report numbers.
    if let Some(case) = cases.iter().find(|c| c.needs_store())
        && !fake
    {
        return Err(format!("case {} needs --fake", case.name()));
    }
    let [bin, assets, template, scratch] = run_paths(&opts)?;
    // The fake lives until every case has run; without it the apps' stores hit a closed port.
    let delay = Duration::from_millis(number(&opts, "--image-delay-ms", 150)?);
    let fake = fake
        .then(|| {
            let total = store_seed::StoreSeed::REAL.sounds as u32;
            fake_gb::FakeGb::start(store_seed::FAKE_PORT, total, delay)
        })
        .transpose()?;
    let base = fake.as_ref().map(fake_gb::FakeGb::base);
    let setup = runs::RunSetup {
        bin: &bin,
        assets: &assets,
        template: &template,
        scratch: &scratch,
        loopback: base.as_deref(),
        fake: fake.as_ref(),
    };
    let ok = runs::v6_frames(&setup, &cases)?;
    // A fake that saw no requests means the cases never reached it, so their numbers are suspect.
    if let Some(fake) = &fake {
        println!("fake requests={}", fake.requests());
    }
    Ok(if ok { 0 } else { 1 })
}

/// `--cases all`: every case that runs on a plain template, without a seeded store or library.
#[cfg(target_os = "linux")]
fn all_cases() -> Vec<Case> {
    Case::ALL
        .into_iter()
        .filter(|c| !c.needs_store() && !c.needs_seed())
        .collect()
}

#[cfg(target_os = "linux")]
fn report_cmd(args: &[OsString]) -> Result<i32, String> {
    let opts = options(args, &["--runs", "--damage", "--quiet-ms"])?;
    let read = |name: &str| {
        let path = required(&opts, name)?;
        std::fs::read_to_string(path).map_err(|e| format!("{path}: {e}"))
    };
    let quiet_ms = number(&opts, "--quiet-ms", 1000)?;
    let report = report::report(&read("--runs")?, &read("--damage")?, quiet_ms)?;
    let shown = |ms: Option<f64>| ms.map_or("none".into(), |ms| format!("{ms:.0}"));
    for r in &report.runs {
        println!(
            "run={} first_damage_ms={} settled_ms={} damages={} other_damages={}",
            r.run,
            shown(r.first_ms),
            shown(r.settled_ms),
            r.damages,
            r.other_damages
        );
    }
    // Summaries cover the runs that have the value; `median` needs at least one sample.
    let firsts: Vec<f64> = report.runs.iter().filter_map(|r| r.first_ms).collect();
    if !firsts.is_empty() {
        println!("{}", runs::summary_line("first_damage", &firsts));
    }
    let settled: Vec<f64> = report.runs.iter().filter_map(|r| r.settled_ms).collect();
    let unsettled = report.runs.len() - settled.len();
    if !settled.is_empty() {
        let summary = runs::summary_line("settled", &settled);
        println!("{summary} unsettled={unsettled}");
    } else if unsettled > 0 {
        println!("settled none unsettled={unsettled}");
    }
    for idle in &report.idle {
        println!("idle pid={} frames={}", idle.pid, idle.frames);
    }
    Ok(0)
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;

    #[test]
    fn startup_refuses_zero_runs() {
        let args: Vec<OsString> = ["v6-startup", "--runs", "0"]
            .iter()
            .map(OsString::from)
            .collect();
        let err = run(&args).unwrap_err();
        assert!(err.contains("--runs"), "{err}");
    }

    #[test]
    fn a_store_case_needs_the_fake() {
        let args: Vec<OsString> = ["v6-frames", "--cases", "sound-store-scroll"]
            .iter()
            .map(OsString::from)
            .collect();
        let err = run(&args).unwrap_err();
        assert!(err.contains("needs --fake"), "{err}");
    }

    #[test]
    fn all_skips_store_and_seeded_cases() {
        let all = all_cases();
        assert!(all.contains(&Case::ProfilesNav) && all.contains(&Case::Calibrate));
        assert!(!all.contains(&Case::LogsScroll));
        assert!(!all.contains(&Case::SoundStoreNav));
        assert!(!all.contains(&Case::ProfileApply));
    }
}
