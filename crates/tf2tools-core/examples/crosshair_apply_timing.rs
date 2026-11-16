//! Times applying library crosshairs: 44 store-sized crosshair VTFs (512x512, about 1 MB, a
//! transparent canvas with an opaque cross) are imported into a fresh library and assigned
//! one per weapon script of a scratch game, then `apply_crosshairs` runs once to warm up and
//! five times timed.
//!
//! Usage: `crosshair_apply_timing --data <empty dir> --game <empty dir>`. Refuses a `--data`
//! that holds `tf2tools.db` and a `--game` that is not empty. Uses only APIs that predate the
//! packed crosshair library, so the same file times both forms.

use std::error::Error;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::{Duration, Instant};

use tf2tools_core::{
    BuiltinCrosshairs, CrosshairRef, DataDir, Database, Library, LibraryKind, Provenance,
    RgbaImage, Tf2Install, rgba_to_vtf, weapons,
};
use walkdir::WalkDir;

const USAGE: &str = "usage: crosshair_apply_timing --data <dir> --game <dir>";
/// Timed applies; the first is a warm-up and stays out of the median.
const RUNS: usize = 6;
/// Library crosshairs imported and weapon scripts assigned, one each.
const CROSSHAIRS: usize = 44;
/// Side of each synthetic crosshair, the size store crosshairs commonly ship at.
const SIDE: u32 = 512;
/// Where a packed library item keeps its files, relative to its folder.
const ITEM_ZIP: &str = "copy.zip";
/// A stock weapon script as TF2 ships it in `tf2_misc_dir.vpk`, ICE-encrypted.
const STOCK_WEAPON_CTX: &[u8] = include_bytes!("../tests/fixtures/crosshair/stock_weapon.ctx");

struct Args {
    data: PathBuf,
    game: PathBuf,
}

fn parse_args(mut args: impl Iterator<Item = String>) -> Result<Args, String> {
    let (mut data, mut game) = (None, None);
    while let Some(flag) = args.next() {
        let slot = match flag.as_str() {
            "--data" => &mut data,
            "--game" => &mut game,
            other => return Err(format!("unknown argument {other}")),
        };
        *slot = Some(PathBuf::from(
            args.next().ok_or(format!("{flag} needs a value"))?,
        ));
    }
    Ok(Args {
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

/// A `SIDE`-square transparent canvas with an opaque cross whose arm width and colour vary
/// with `n`, so every crosshair differs and compresses like a drawn store crosshair.
fn cross_image(n: usize) -> Option<RgbaImage> {
    let half = (4 + (n % 8) as u32) as i64;
    let colour = [255, (n * 37 % 256) as u8, (n * 91 % 256) as u8, 255];
    let centre = i64::from(SIDE / 2);
    let mut pixels = vec![0u8; (SIDE * SIDE * 4) as usize];
    for (i, px) in pixels.as_chunks_mut::<4>().0.iter_mut().enumerate() {
        let (x, y) = ((i as u32 % SIDE) as i64, (i as u32 / SIDE) as i64);
        let on_arm = (x - centre).abs() < half || (y - centre).abs() < half;
        let in_reach = (x - centre).abs() < 160 && (y - centre).abs() < 160;
        if on_arm && in_reach {
            *px = colour;
        }
    }
    RgbaImage::from_pixels(SIDE, SIDE, pixels)
}

/// A version 1 `_dir.vpk` holding `bytes` as `scripts/<name>.ctx` for each of `names`,
/// stored inline after the tree, the layout the core tests' `vpk_bytes` writes.
fn stock_scripts_vpk(names: &[&str], bytes: &[u8]) -> Vec<u8> {
    let (mut tree, mut data) = (Vec::new(), Vec::new());
    for name in names {
        for part in ["ctx", "scripts", name] {
            tree.extend_from_slice(part.as_bytes());
            tree.push(0);
        }
        tree.extend_from_slice(&0u32.to_le_bytes()); // crc, not checked by readers
        tree.extend_from_slice(&0u16.to_le_bytes()); // preload length
        tree.extend_from_slice(&0x7fffu16.to_le_bytes()); // stored in the dir file itself
        tree.extend_from_slice(&(data.len() as u32).to_le_bytes());
        tree.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
        tree.extend_from_slice(&0xffffu16.to_le_bytes());
        tree.extend_from_slice(&[0, 0]); // end of names, end of paths
        data.extend_from_slice(bytes);
    }
    tree.push(0); // end of extensions
    let mut out = Vec::new();
    out.extend_from_slice(&0x55aa_1234u32.to_le_bytes());
    out.extend_from_slice(&1u32.to_le_bytes());
    out.extend_from_slice(&(tree.len() as u32).to_le_bytes());
    out.extend(tree);
    out.extend(data);
    out
}

/// An empty builtin catalog under `dir`, since only library crosshairs are timed.
fn empty_catalog(dir: &Path) -> Result<BuiltinCrosshairs, Box<dyn Error>> {
    std::fs::create_dir_all(dir.join("vtf"))?;
    std::fs::create_dir_all(dir.join("vmt"))?;
    std::fs::write(dir.join("crosshairs.json"), "[]")?;
    Ok(BuiltinCrosshairs::load(dir)?)
}

/// Total bytes of every file under `root`, and whether any item folder holds `ITEM_ZIP`.
fn library_size(root: &Path) -> Result<(u64, bool), Box<dyn Error>> {
    let (mut bytes, mut packed) = (0, false);
    for entry in WalkDir::new(root) {
        let entry = entry?;
        if entry.file_type().is_file() {
            bytes += entry.metadata()?.len();
            packed |= entry.file_name() == ITEM_ZIP;
        }
    }
    Ok((bytes, packed))
}

fn run(args: &Args) -> Result<(), Box<dyn Error>> {
    refuse_used_dirs(args)?;
    let scripts: Vec<&str> = weapons().iter().map(|w| w.script_name()).collect();
    let scripts = scripts.get(..CROSSHAIRS).ok_or(format!(
        "only {} weapon scripts, {CROSSHAIRS} needed",
        scripts.len()
    ))?;

    let dir = DataDir::at(&args.data);
    std::fs::create_dir_all(dir.root())?;
    let library = Library::new(Database::open(&dir)?, dir.clone());
    let scratch = tempfile::tempdir_in(dir.root())?;
    let builtins = empty_catalog(&scratch.path().join("catalog"))?;

    let tf = args.game.join("tf");
    std::fs::create_dir_all(tf.join("custom"))?;
    let vpk = stock_scripts_vpk(scripts, STOCK_WEAPON_CTX);
    std::fs::write(tf.join("tf2_misc_dir.vpk"), vpk)?;
    let tf2 = Tf2Install::from_path(&args.game)?;

    for (n, script) in scripts.iter().enumerate() {
        let vtf = scratch.path().join(format!("cross{n}.vtf"));
        let image = cross_image(n).ok_or("cross pixels do not match the side")?;
        std::fs::write(&vtf, rgba_to_vtf(&image)?)?;
        let entry = library.import_crosshair(&vtf, &format!("cross {n}"), Provenance::Local)?;
        library.assign(script, &CrosshairRef::Library(entry.id().clone()))?;
    }

    let root = dir.library_dir(LibraryKind::Crosshairs);
    let (bytes, packed) = library_size(&root)?;
    println!("library crosshairs: {CROSSHAIRS}, folder bytes: {bytes}");
    println!("{ITEM_ZIP}: {}", if packed { "yes" } else { "no" });

    let mut runs = Vec::with_capacity(RUNS);
    for n in 1..=RUNS {
        let start = Instant::now();
        let report = library.apply_crosshairs(&tf2, &builtins)?;
        let took = start.elapsed();
        if !report.skipped().is_empty() {
            return Err(format!("{} assignments skipped", report.skipped().len()).into());
        }
        println!(
            "run {n}: {} applied, {} ms",
            report.applied().len(),
            took.as_millis()
        );
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
            eprintln!("crosshair_apply_timing: {e}");
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
    fn arguments_need_both_paths() {
        let args = |v: &[&str]| parse_args(v.iter().map(|s| s.to_string()));
        assert!(args(&["--data", "d"]).is_err());
        let ok = args(&["--data", "d", "--game", "g"]).unwrap();
        assert_eq!((ok.data, ok.game), ("d".into(), "g".into()));
    }

    #[test]
    fn a_cross_is_a_megabyte_vtf_that_decodes_with_a_clear_corner() {
        let vtf = rgba_to_vtf(&cross_image(3).unwrap()).unwrap();
        assert!(vtf.len() > 1_000_000, "{} bytes", vtf.len());
        let image = tf2tools_core::decode_vtf(&vtf).unwrap();
        assert_eq!((image.width(), image.height()), (SIDE, SIDE));
        assert_eq!(image.pixels()[3], 0, "the corner is transparent");
        let centre = ((SIDE / 2 * SIDE + SIDE / 2) * 4) as usize;
        assert_eq!(image.pixels()[centre + 3], 255, "the centre is drawn");
    }
}
