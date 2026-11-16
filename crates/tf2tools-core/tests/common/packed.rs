//! Library HUDs in either form: a loose tree, or `.tf2tools/files.zip` beside loose metadata.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use tf2tools_core::{ConvertReport, DataDir, HudEntry, LibraryKind, Provenance};

use super::{LibraryEnv, tree, write_file, zip_bytes};

/// The HUD's library folder.
pub fn hud_dir(env: &LibraryEnv, hud: &HudEntry) -> PathBuf {
    hud_dir_in(&env.dir, hud)
}

/// The HUD's library folder under the data dir `dir`.
fn hud_dir_in(dir: &DataDir, hud: &HudEntry) -> PathBuf {
    dir.library_dir(LibraryKind::Huds).join(hud.folder_name())
}

/// Where the HUD's zip is when it is packed.
pub fn zip_path(env: &LibraryEnv, hud: &HudEntry) -> PathBuf {
    hud_dir(env, hud).join(".tf2tools/files.zip")
}

/// The loose files outside `.tf2tools/`, sorted.
pub fn loose_files(env: &LibraryEnv, hud: &HudEntry) -> Vec<String> {
    let mut files = tree(&hud_dir(env, hud));
    files.retain(|rel| !rel.starts_with(".tf2tools/"));
    files
}

/// The zip's entries with their bytes, sorted by name.
pub fn packed_files(env: &LibraryEnv, hud: &HudEntry) -> Vec<(String, Vec<u8>)> {
    zip_files(&zip_path(env, hud))
}

fn zip_files(zip: &Path) -> Vec<(String, Vec<u8>)> {
    let file = std::fs::File::open(zip).unwrap();
    let mut zip = zip::ZipArchive::new(file).unwrap();
    let mut out = Vec::new();
    for i in 0..zip.len() {
        let mut entry = zip.by_index(i).unwrap();
        let mut bytes = Vec::new();
        entry.read_to_end(&mut bytes).unwrap();
        out.push((entry.name().unwrap().into_owned(), bytes));
    }
    out.sort();
    out
}

/// The HUD's files outside `.tf2tools/` with their bytes, read from whichever form it has.
pub fn library_files(env: &LibraryEnv, hud: &HudEntry) -> Vec<(String, Vec<u8>)> {
    library_files_in(&env.dir, hud)
}

/// `library_files` for a HUD in the library under the data dir `dir`.
pub fn library_files_in(dir: &DataDir, hud: &HudEntry) -> Vec<(String, Vec<u8>)> {
    let folder = hud_dir_in(dir, hud);
    let zip = folder.join(".tf2tools/files.zip");
    if zip.is_file() {
        return zip_files(&zip);
    }
    let mut files = tree(&folder);
    files.retain(|rel| !rel.starts_with(".tf2tools/"));
    files
        .into_iter()
        .map(|rel| {
            let bytes = std::fs::read(folder.join(&rel)).unwrap();
            (rel, bytes)
        })
        .collect()
}

/// The names `library_files` lists, in order.
pub fn library_names(env: &LibraryEnv, hud: &HudEntry) -> Vec<String> {
    library_files(env, hud)
        .into_iter()
        .map(|(rel, _)| rel)
        .collect()
}

static ARCHIVES: AtomicUsize = AtomicUsize::new(0);

/// Imports a HUD `folder` holding `info.vdf` and `files`, then leaves it loose, as a library
/// from before packing holds it.
pub fn loose_hud(
    env: &LibraryEnv,
    folder: &str,
    files: &[(&str, &[u8])],
    provenance: Provenance,
) -> HudEntry {
    let mut named = vec![(
        format!("{folder}/info.vdf"),
        format!("\"{folder}\" {{}}").into_bytes(),
    )];
    named.extend(
        files
            .iter()
            .map(|(rel, b)| (format!("{folder}/{rel}"), b.to_vec())),
    );
    let refs: Vec<(&str, &[u8])> = named
        .iter()
        .map(|(p, b)| (p.as_str(), b.as_slice()))
        .collect();
    let n = ARCHIVES.fetch_add(1, Ordering::Relaxed);
    let archive = write_file(
        &env.tmp.path().join(format!("in/packed-{folder}-{n}.zip")),
        &zip_bytes(&refs),
    );
    let hud = env.lib.import_hud(&archive, provenance).unwrap();
    env.lib.unpack_hud_for_tests(hud.id()).unwrap();
    hud
}

/// One conversion pass.
pub fn convert(env: &LibraryEnv) -> ConvertReport {
    env.lib.convert_library()
}
