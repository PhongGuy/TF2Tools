//! Game and HUD folders on disk for the copy, layer and update tests.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use tempfile::TempDir;
use tf2tools_core::{GbId, HudEntry, Provenance};

use super::{LibraryEnv, vpk_bytes, write_file, zip_bytes};

/// A `tf` folder whose gameinfo lists no VPKs, so only the HUD layers and tf/cfg hold files.
pub fn empty_game() -> (TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let tf = dir.path().join("tf");
    write_file(
        &tf.join("gameinfo.txt"),
        br#""GameInfo" { FileSystem { SearchPaths { } } }"#,
    );
    (dir, tf)
}

/// Writes each (relative path, text) under `root` and returns `root`.
pub fn folder_with(root: &Path, files: &[(&str, &str)]) -> PathBuf {
    for (rel, text) in files {
        write_file(&root.join(rel), text.as_bytes());
    }
    root.to_path_buf()
}

/// A store item's provenance: only the same store item arriving again replaces its library
/// HUD, so a base that is updated by a re-import needs one.
pub fn store_item() -> Provenance {
    Provenance::GameBanana {
        gb_id: GbId::new(7),
        author: "someone".into(),
    }
}

static ARCHIVES: AtomicUsize = AtomicUsize::new(0);

/// Imports a HUD rooted at `root`: `info.vdf`, the stock files, then `extra` replacing or adding.
pub fn import_stock_hud(
    env: &LibraryEnv,
    root: &str,
    extra: &[(&str, &str)],
    provenance: Provenance,
) -> HudEntry {
    let mut files: Vec<(String, String)> = super::hud_stock::stock();
    files.push((
        "info.vdf".to_owned(),
        format!("\"{root}\" {{ ui_version 3 }}"),
    ));
    for (rel, text) in extra {
        files.retain(|(p, _)| p != rel);
        files.push(((*rel).to_owned(), (*text).to_owned()));
    }
    let named: Vec<(String, Vec<u8>)> = files
        .into_iter()
        .map(|(p, t)| (format!("{root}/{p}"), t.into_bytes()))
        .collect();
    let refs: Vec<(&str, &[u8])> = named
        .iter()
        .map(|(p, b)| (p.as_str(), b.as_slice()))
        .collect();
    // A fresh archive name per call, so one root can be imported again with other bytes.
    let n = ARCHIVES.fetch_add(1, Ordering::Relaxed);
    let archive = write_file(
        &env.tmp.path().join(format!("in/{root}-{n}.zip")),
        &zip_bytes(&refs),
    );
    env.lib.import_hud(&archive, provenance).unwrap()
}

/// Writes the env's `tf/gameinfo.txt`: no VPKs, or one `tf2_misc` VPK holding `vpk`.
pub fn game_for(env: &LibraryEnv, vpk: &[(&str, &[u8])]) -> PathBuf {
    let tf = env.tf2.tf_dir();
    if vpk.is_empty() {
        write_file(
            &tf.join("gameinfo.txt"),
            br#""GameInfo" { FileSystem { SearchPaths { } } }"#,
        );
    } else {
        write_file(
            &tf.join("gameinfo.txt"),
            br#""GameInfo" { FileSystem { SearchPaths { game+mod+vgui tf/tf2_misc.vpk } } }"#,
        );
        write_file(&tf.join("tf2_misc_dir.vpk"), &vpk_bytes(vpk));
    }
    tf
}

/// Every file under `dir` with its bytes, sorted, for byte-identical comparisons.
pub fn snapshot(dir: &Path) -> Vec<(String, Vec<u8>)> {
    super::tree(dir)
        .into_iter()
        .map(|rel| {
            let bytes = std::fs::read(dir.join(&rel)).unwrap();
            (rel, bytes)
        })
        .collect()
}
