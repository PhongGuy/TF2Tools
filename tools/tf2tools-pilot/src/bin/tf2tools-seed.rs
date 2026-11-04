//! `tf2tools-seed <data_dir> <game_dir>`: writes the pilot's seeded library and fake TF2 there,
//! leaving the store `TF2TOOLS_PILOT_EMPTY_STORE` names empty.

use std::path::PathBuf;
use std::process::ExitCode;

use tf2tools_app::assets::AssetRoot;

fn main() -> ExitCode {
    let args: Vec<PathBuf> = std::env::args_os().skip(1).map(PathBuf::from).collect();
    let [data, game] = args.as_slice() else {
        eprintln!("usage: tf2tools-seed <data_dir> <game_dir>");
        return ExitCode::from(2);
    };
    let seeded = AssetRoot::resolve()
        .map_err(anyhow::Error::from)
        .and_then(|assets| {
            let empty = tf2tools_pilot::empty_store();
            tf2tools_pilot::seed::seed_with(data, game, assets.path(), empty.as_deref())
        });
    match seeded {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("tf2tools-seed: {err:#}");
            ExitCode::FAILURE
        }
    }
}
