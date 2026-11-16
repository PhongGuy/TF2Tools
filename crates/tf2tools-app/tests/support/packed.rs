//! Library HUD files in either stored form, as core's `tests/common/packed.rs` reads them.

use std::io::Read as _;
use std::path::Path;

/// The files of the library HUD at `dir` outside `.tf2tools/`, with their bytes, sorted by
/// name: from `.tf2tools/files.zip` when it is packed, else from the loose tree.
pub fn library_files(dir: &Path) -> Vec<(String, Vec<u8>)> {
    let zip = dir.join(".tf2tools/files.zip");
    if zip.is_file() {
        let mut archive = zip::ZipArchive::new(std::fs::File::open(zip).unwrap()).unwrap();
        let mut out = Vec::new();
        for i in 0..archive.len() {
            let mut entry = archive.by_index(i).unwrap();
            let mut bytes = Vec::new();
            entry.read_to_end(&mut bytes).unwrap();
            out.push((entry.name().unwrap().into_owned(), bytes));
        }
        out.sort();
        return out;
    }
    let mut out = Vec::new();
    walk(dir, "", &mut out);
    out.retain(|(rel, _)| !rel.starts_with(".tf2tools/"));
    out.sort();
    out
}

fn walk(dir: &Path, prefix: &str, out: &mut Vec<(String, Vec<u8>)>) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let entry = entry.unwrap();
        let name = entry.file_name().to_string_lossy().into_owned();
        let rel = if prefix.is_empty() {
            name
        } else {
            format!("{prefix}/{name}")
        };
        if entry.file_type().unwrap().is_dir() {
            walk(&entry.path(), &rel, out);
        } else {
            out.push((rel, std::fs::read(entry.path()).unwrap()));
        }
    }
}
