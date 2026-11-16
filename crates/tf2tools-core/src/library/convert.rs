//! Converting loose library HUDs to packed zips, one item at a time under the copies write lock.

use std::io;
use std::path::Path;

use super::made_huds::{MAKER_FILE, old_folder};
use super::packed::{HUD_ZIP, ItemSource, pack};
use super::{
    HudId, IoFailure, Library, METADATA_DIR, io_at, library_db, remove_dir_if_present,
    remove_file_if_present,
};
use crate::error::LibraryError;
use crate::paths::LibraryKind;

/// What one conversion pass did, per library kind.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct ConvertReport {
    huds: KindConverted,
}

impl ConvertReport {
    /// The HUDs' tally.
    pub fn huds(&self) -> &KindConverted {
        &self.huds
    }
}

/// One kind's tally: items packed, items that failed and stayed loose, packed items whose
/// leftover loose files were removed, and the bytes of the packed items before and after.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct KindConverted {
    packed: usize,
    failed: usize,
    cleaned: usize,
    bytes_before: u64,
    bytes_after: u64,
}

impl KindConverted {
    /// Items packed by this pass.
    pub fn packed(&self) -> usize {
        self.packed
    }

    /// Items that could not be packed and stay loose.
    pub fn failed(&self) -> usize {
        self.failed
    }

    /// Already packed items whose leftover loose files were removed.
    pub fn cleaned(&self) -> usize {
        self.cleaned
    }

    /// The loose bytes of the items packed by this pass.
    pub fn bytes_before(&self) -> u64 {
        self.bytes_before
    }

    /// The zip bytes of the items packed by this pass.
    pub fn bytes_after(&self) -> u64 {
        self.bytes_after
    }
}

/// What converting one HUD folder came to.
enum Converted {
    Untouched,
    Packed { before: u64, after: u64 },
    Cleaned,
}

impl Library {
    /// Packs every loose, non-made library HUD into `.tf2tools/files.zip` and clears leftovers
    /// beside existing zips. Each item is its own step; a failure leaves that item loose, is
    /// logged and counted, and the pass moves on. A failed HUD listing returns an empty report.
    pub fn convert_library(&self) -> ConvertReport {
        let mut report = ConvertReport::default();
        let huds = match self.list_huds() {
            Ok(huds) => huds,
            Err(error) => {
                tracing::warn!(%error, "library conversion could not list HUDs");
                return report;
            }
        };
        let root = self.dir.library_dir(LibraryKind::Huds);
        for hud in huds {
            let live = root.join(hud.folder_name());
            let tally = &mut report.huds;
            match self.convert_hud(&live) {
                Ok(Converted::Untouched) => {}
                Ok(Converted::Packed { before, after }) => {
                    tally.packed += 1;
                    tally.bytes_before = tally.bytes_before.saturating_add(before);
                    tally.bytes_after = tally.bytes_after.saturating_add(after);
                }
                Ok(Converted::Cleaned) => tally.cleaned += 1,
                Err(IoFailure { path, source }) => {
                    tally.failed += 1;
                    tracing::warn!(
                        folder = hud.folder_name(),
                        path = %path.display(),
                        error = %source,
                        "HUD left loose: it could not be packed"
                    );
                }
            }
        }
        report
    }

    /// Extracts the packed HUD `id` back into its folder and removes its zip, so a test can
    /// edit its tree; a loose HUD is left as it is.
    ///
    /// # Errors
    /// `NotFound` for no such row, `Database` when the lookup fails, `Io` when a file fails.
    #[doc(hidden)]
    pub fn unpack_hud_for_tests(&self, id: &HudId) -> Result<(), LibraryError> {
        let folder =
            self.hud_folder(id)
                .map_err(library_db)?
                .ok_or_else(|| LibraryError::NotFound {
                    what: format!("HUD {}", id.as_str()),
                })?;
        let live = self.dir.library_dir(LibraryKind::Huds).join(folder);
        let _copies = self.copies_write();
        let source = ItemSource::open(&live, HUD_ZIP)?;
        if source.is_packed() {
            source.extract_into(&live, &|_| false)?;
            remove_file_if_present(&live.join(HUD_ZIP))?;
        }
        Ok(())
    }

    /// Converts one HUD folder under the write guard. A folder that is gone (a save moved it)
    /// or made by the Maker stays as it is; made HUDs are edited in place, so they stay loose.
    fn convert_hud(&self, live: &Path) -> Result<Converted, IoFailure> {
        let _copies = self.copies_write();
        let is_dir = |dir: &Path| std::fs::symlink_metadata(dir).is_ok_and(|m| m.is_dir());
        let made = |dir: &Path| dir.join(METADATA_DIR).join(MAKER_FILE).is_file();
        if !is_dir(live) || made(live) || made(&old_folder(live)) {
            return Ok(Converted::Untouched);
        }
        let zip = live.join(HUD_ZIP);
        let mut tmp = zip.as_os_str().to_owned();
        tmp.push(".tmp");
        remove_file_if_present(Path::new(&tmp))?;
        if std::fs::symlink_metadata(&zip).is_ok() {
            // A crash between placing the zip and clearing the loose tree leaves both; the
            // loose files go only once the zip reads back as the item, never for a link.
            require_packed(live)?;
            let removed = remove_loose(live)?;
            return Ok(if removed {
                Converted::Cleaned
            } else {
                Converted::Untouched
            });
        }
        let before = ItemSource::Loose(live.to_path_buf()).unpacked_bytes()?;
        let top_metadata = |rel: &Path| rel == Path::new(METADATA_DIR);
        let after = pack(live, &top_metadata, &zip)?;
        require_packed(live)?;
        remove_loose(live)?;
        Ok(Converted::Packed { before, after })
    }
}

/// Fails unless readers would take `live` as packed: its zip a regular file whose central
/// directory parses with every name accepted. Guards every removal of loose files.
fn require_packed(live: &Path) -> Result<(), IoFailure> {
    if ItemSource::open(live, HUD_ZIP)?.is_packed() {
        return Ok(());
    }
    let path = live.join(HUD_ZIP);
    let source = io::Error::new(io::ErrorKind::InvalidData, "the zip is not a regular file");
    Err(IoFailure { path, source })
}

/// Removes everything at the top of `dir` but `.tf2tools`, links included and never followed;
/// true when anything went.
fn remove_loose(dir: &Path) -> Result<bool, IoFailure> {
    let mut removed = false;
    for entry in std::fs::read_dir(dir).map_err(io_at(dir))? {
        let entry = entry.map_err(io_at(dir))?;
        if entry.file_name() == METADATA_DIR {
            continue;
        }
        let path = entry.path();
        let kind = entry.file_type().map_err(io_at(&path))?;
        if kind.is_dir() {
            remove_dir_if_present(&path)?;
        } else {
            remove_file_if_present(&path)?;
        }
        removed = true;
    }
    Ok(removed)
}
