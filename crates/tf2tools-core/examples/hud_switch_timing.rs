//! Times a HUD switch onto a large library HUD: the archive and a tiny two-file HUD are
//! imported into a fresh library, then each run installs the tiny HUD untimed and the large
//! one timed, so every timed install removes one HUD and writes the other.
//!
//! Usage: `hud_switch_timing --archive <hud.zip> --data <empty dir> --game <empty dir>`.
//! Refuses a `--data` that holds `tf2tools.db` and a `--game` that is not empty. Uses only
//! APIs that predate the packed library, so the same file times both forms.

use std::error::Error;
use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::{Duration, Instant};

use tf2tools_core::{DataDir, Database, Library, LibraryKind, Provenance, Tf2Install};
use walkdir::WalkDir;

const USAGE: &str = "usage: hud_switch_timing --archive <hud.zip> --data <dir> --game <dir>";
/// Timed installs; the first is a warm-up and stays out of the median.
const RUNS: usize = 6;
/// Where a packed HUD keeps its files, relative to its library folder.
const HUD_ZIP: &str = ".tf2tools/files.zip";

struct Args {
    archive: PathBuf,
    data: PathBuf,
    game: PathBuf,
}

fn parse_args(mut args: impl Iterator<Item = String>) -> Result<Args, String> {
    let (mut archive, mut data, mut game) = (None, None, None);
    while let Some(flag) = args.next() {
        let slot = match flag.as_str() {
            "--archive" => &mut archive,
            "--data" => &mut data,
            "--game" => &mut game,
            other => return Err(format!("unknown argument {other}")),
        };
        *slot = Some(PathBuf::from(
            args.next().ok_or(format!("{flag} needs a value"))?,
        ));
    }
    Ok(Args {
        archive: archive.ok_or("--archive is required")?,
        data: data.ok_or("--data is required")?,
        game: game.ok_or("--game is required")?,
    })
}

/// The median of every run but the first, which pays for a cold cache.
fn median_after_warm_up(runs: &[Duration]) -> Duration {
    let mut warm = runs.get(1..).unwrap_or_default().to_vec();
    warm.sort();
    warm.get(warm.len() / 2).copied().unwrap_or_default()
}

/// Errors unless `data` is fresh and `game` is missing or empty, so a run never touches a
/// real library or game folder.
fn refuse_used_dirs(args: &Args) -> Result<(), String> {
    let db = DataDir::at(&args.data).db_path();
    if db.exists() {
        return Err(format!(
            "{} already exists; --data must be fresh",
            db.display()
        ));
    }
    let used = std::fs::read_dir(&args.game).is_ok_and(|mut d| d.next().is_some());
    if used {
        return Err(format!("{} is not empty", args.game.display()));
    }
    Ok(())
}

/// Writes a two-file HUD zip under `dir`, the HUD each run switches away from.
fn small_hud(dir: &Path) -> Result<PathBuf, Box<dyn Error>> {
    let path = dir.join("smallhud.zip");
    let mut zip = zip::ZipWriter::new(File::create(&path)?);
    let options = zip::write::SimpleFileOptions::default();
    zip.start_file("smallhud/info.vdf", options)?;
    zip.write_all(b"\"smallhud\"\n{\n\t\"ui_version\"\t\"3\"\n}\n")?;
    zip.start_file("smallhud/resource/ui/hudplayerhealth.res", options)?;
    zip.write_all(b"\"Resource/UI/HudPlayerHealth.res\"\n{\n}\n")?;
    zip.finish()?;
    Ok(path)
}

/// File count and unpacked bytes of a library HUD in whichever form it is stored, plus the
/// zip's size when it is packed.
fn hud_size(folder: &Path) -> Result<(usize, u64, Option<u64>), Box<dyn Error>> {
    let zip_path = folder.join(HUD_ZIP);
    if zip_path.is_file() {
        let mut zip = zip::ZipArchive::new(File::open(&zip_path)?)?;
        let mut bytes = 0;
        for i in 0..zip.len() {
            bytes += zip.by_index_raw(i)?.size();
        }
        return Ok((zip.len(), bytes, Some(zip_path.metadata()?.len())));
    }
    let (mut files, mut bytes) = (0, 0);
    let entries = WalkDir::new(folder).into_iter();
    for entry in entries.filter_entry(|e| e.file_name() != ".tf2tools") {
        let entry = entry?;
        if entry.file_type().is_file() {
            files += 1;
            bytes += entry.metadata()?.len();
        }
    }
    Ok((files, bytes, None))
}

fn run(args: &Args) -> Result<(), Box<dyn Error>> {
    refuse_used_dirs(args)?;
    let dir = DataDir::at(&args.data);
    std::fs::create_dir_all(dir.root())?;
    let library = Library::new(Database::open(&dir)?, dir.clone());
    let scratch = tempfile::tempdir_in(dir.root())?;
    let small = library.import_hud(&small_hud(scratch.path())?, Provenance::Local)?;
    let big = library.import_hud(&args.archive, Provenance::Local)?;

    std::fs::create_dir_all(args.game.join("tf").join("custom"))?;
    let tf2 = Tf2Install::from_path(&args.game)?;

    let folder = dir.library_dir(LibraryKind::Huds).join(big.folder_name());
    let (files, bytes, packed) = hud_size(&folder)?;
    println!("library folder: {}", folder.display());
    println!("files: {files}, unpacked bytes: {bytes}");
    match packed {
        Some(size) => println!("{HUD_ZIP}: yes, {size} bytes"),
        None => println!("{HUD_ZIP}: no"),
    }

    let mut runs = Vec::with_capacity(RUNS);
    for n in 1..=RUNS {
        library.install_hud(small.id(), &tf2)?;
        let start = Instant::now();
        library.install_hud(big.id(), &tf2)?;
        let took = start.elapsed();
        println!("run {n}: {} ms", took.as_millis());
        runs.push(took);
    }
    println!(
        "median of runs 2 to {RUNS}: {} ms",
        median_after_warm_up(&runs).as_millis()
    );
    Ok(())
}

fn main() -> ExitCode {
    let args = match parse_args(std::env::args().skip(1)) {
        Ok(args) => args,
        Err(e) => {
            eprintln!("{e}\n{USAGE}");
            return ExitCode::FAILURE;
        }
    };
    match run(&args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("hud_switch_timing: {e}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_median_drops_the_warm_up() {
        let runs = [900, 50, 40, 60, 45, 55].map(Duration::from_millis);
        assert_eq!(median_after_warm_up(&runs), Duration::from_millis(50));
    }

    #[test]
    fn arguments_need_all_three_paths() {
        let args = |v: &[&str]| parse_args(v.iter().map(|s| s.to_string()));
        assert!(args(&["--archive", "a.zip", "--data", "d"]).is_err());
        let ok = args(&["--archive", "a.zip", "--data", "d", "--game", "g"]).unwrap();
        assert_eq!(
            (ok.archive, ok.data, ok.game),
            ("a.zip".into(), "d".into(), "g".into())
        );
    }
}
