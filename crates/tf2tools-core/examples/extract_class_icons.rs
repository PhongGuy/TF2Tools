//! Extracts the nine TF2 class leaderboard icons from the player's own game files into PNGs.
//!
//! Usage: `extract_class_icons <tf dir> <out dir>`. Reads `<tf dir>/tf2_textures_dir.vpk`,
//! writes `<out dir>/<stem>.png` per class, prints each path and size, and exits non-zero
//! naming every class it could not find, read in full, or decode.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use tf2tools_core::{TfClass, decode_vtf, leaderboard_icon_stem, write_png};

fn main() -> ExitCode {
    let args: Vec<PathBuf> = std::env::args_os().skip(1).map(PathBuf::from).collect();
    let [tf_dir, out_dir] = args.as_slice() else {
        eprintln!("usage: extract_class_icons <tf dir> <out dir>");
        return ExitCode::FAILURE;
    };
    let vpk_path = tf_dir.join("tf2_textures_dir.vpk");
    let pak = match vpk::from_path(&vpk_path) {
        Ok(pak) => pak,
        Err(e) => {
            eprintln!("{}: {e}", vpk_path.display());
            return ExitCode::FAILURE;
        }
    };
    if let Err(e) = std::fs::create_dir_all(out_dir) {
        eprintln!("{}: {e}", out_dir.display());
        return ExitCode::FAILURE;
    }

    // Try every class before failing, so one run names all the problems.
    let mut failed = false;
    for class in TfClass::ALL {
        let stem = leaderboard_icon_stem(class);
        let name = format!("materials/hud/leaderboard_class_{stem}.vtf");
        match extract(&pak, &name, &out_dir.join(format!("{stem}.png"))) {
            Ok(line) => println!("{line}"),
            Err(why) => {
                eprintln!("{}: {why}", class.name());
                failed = true;
            }
        }
    }
    if failed {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}

// One icon: the entry read in full (the vpk crate returns short data on a truncated archive
// without an error), decoded and written; the result is the line to print.
fn extract(pak: &vpk::VPK, name: &str, out: &Path) -> Result<String, String> {
    let entry = pak
        .tree
        .get(name)
        .ok_or_else(|| format!("{name} is not in the VPK"))?;
    let data = entry.get().map_err(|e| format!("{name}: {e}"))?;
    let expected =
        u64::from(entry.dir_entry.preload_length) + u64::from(entry.dir_entry.file_length);
    if data.len() as u64 != expected {
        return Err(format!(
            "{name} holds {} of its {expected} bytes",
            data.len()
        ));
    }
    let image = decode_vtf(&data).map_err(|e| format!("{name}: {e}"))?;
    write_png(out, &image).map_err(|e| format!("{}: {e}", out.display()))?;
    Ok(format!(
        "{} {}x{}",
        out.display(),
        image.width(),
        image.height()
    ))
}
