//! Library HUDs built from the stock test files, for the copy tests.

use std::io::Write as _;
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};

use tf2tools_app::services::Services;
use tf2tools_core::{GbId, HudEntry, Provenance};

/// A store item's provenance: only the same store item arriving again replaces its library
/// HUD, so a base updated by a re-import needs one.
pub fn store_item() -> Provenance {
    Provenance::GameBanana {
        gb_id: GbId::new(7),
        author: "someone".into(),
    }
}

static ARCHIVES: AtomicUsize = AtomicUsize::new(0);

/// Imports a HUD rooted at `root`: `info.vdf`, the stock files, then `extra` (path, text)
/// replacing or adding files. Each call writes a fresh archive, so a root can be re-imported.
pub fn import_stock_hud(
    services: &Services,
    dir: &Path,
    root: &str,
    extra: &[(&str, &str)],
    provenance: Provenance,
) -> HudEntry {
    let mut files = super::hud_stock::stock();
    for (rel, text) in extra {
        files.retain(|(p, _)| p != rel);
        files.push(((*rel).to_owned(), (*text).to_owned()));
    }
    let n = ARCHIVES.fetch_add(1, Ordering::Relaxed);
    let path = dir.join(format!("{root}-{n}.zip"));
    let mut zip = zip::ZipWriter::new(std::fs::File::create(&path).unwrap());
    let options = zip::write::SimpleFileOptions::default();
    zip.start_file(format!("{root}/info.vdf"), options).unwrap();
    write!(zip, "\"{root}\" {{ ui_version 3 }}").unwrap();
    for (rel, text) in files {
        zip.start_file(format!("{root}/{rel}"), options).unwrap();
        zip.write_all(text.as_bytes()).unwrap();
    }
    zip.finish().unwrap();
    services.library.import_hud(&path, provenance).unwrap()
}
