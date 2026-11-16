//! Converting loose library items to packed zips, one item at a time under the copies write lock.

use std::io;
use std::path::{Path, PathBuf};

use super::gb_addons::slots_survive_packing;
use super::made_huds::{MAKER_FILE, old_folder};
use super::packed::{GB_ZIP, HUD_ZIP, ItemSource, pack_with};
use super::{
    GbId, HudId, IoFailure, Library, METADATA_DIR, io_at, library_db, remove_dir_if_present,
    remove_file_if_present,
};
use crate::error::LibraryError;
use crate::paths::LibraryKind;

/// What one conversion pass did, per library kind.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct ConvertReport {
    huds: KindConverted,
    gb_addons: KindConverted,
}

impl ConvertReport {
    /// The HUDs' tally.
    pub fn huds(&self) -> &KindConverted {
        &self.huds
    }

    /// The GameBanana add-on copies' tally.
    pub fn gb_addons(&self) -> &KindConverted {
        &self.gb_addons
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
    /// Counts one item's outcome; a failure is returned for the caller to log.
    fn add(&mut self, result: Result<Converted, IoFailure>) -> Option<IoFailure> {
        match result {
            Ok(Converted::Untouched) => {}
            Ok(Converted::Packed { before, after }) => {
                self.packed += 1;
                self.bytes_before = self.bytes_before.saturating_add(before);
                self.bytes_after = self.bytes_after.saturating_add(after);
            }
            Ok(Converted::Cleaned) => self.cleaned += 1,
            Err(failure) => {
                self.failed += 1;
                return Some(failure);
            }
        }
        None
    }

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

/// What converting one item folder came to.
enum Converted {
    Untouched,
    Packed { before: u64, after: u64 },
    Cleaned,
}

impl Library {
    /// Packs every loose, non-made library HUD into `.tf2tools/files.zip` and every GameBanana
    /// add-on copy into `copy.zip`, and clears leftovers beside existing zips. Each item is its
    /// own step; a failure leaves that item loose, is logged and counted, and the pass moves on.
    /// A failed listing of one kind is logged and skips only that kind.
    pub fn convert_library(&self) -> ConvertReport {
        let mut report = ConvertReport::default();
        match self.list_huds() {
            Ok(huds) => {
                let root = self.dir.library_dir(LibraryKind::Huds);
                // Made HUDs are edited in place, so they stay loose; so does one mid-swap.
                let made = |dir: &Path| dir.join(METADATA_DIR).join(MAKER_FILE).is_file();
                let keep_made = |live: &Path| made(live) || made(&old_folder(live));
                for hud in huds {
                    let live = root.join(hud.folder_name());
                    let result = self.convert_item(&live, HUD_ZIP, &keep_made);
                    if let Some(IoFailure { path, source }) = report.huds.add(result) {
                        tracing::warn!(
                            folder = hud.folder_name(),
                            path = %path.display(),
                            error = %source,
                            "HUD left loose: it could not be packed"
                        );
                    }
                }
            }
            Err(error) => tracing::warn!(%error, "library conversion could not list HUDs"),
        }
        match self.list_gb_addons(None) {
            Ok(addons) => {
                let root = self.dir.gb_addon_dir();
                // A copy whose slot entries a zip cannot keep (an empty folder) stays loose.
                let keep = |dir: &Path| !dir.join(GB_ZIP).exists() && !slots_survive_packing(dir);
                for addon in addons {
                    let gb_id = addon.gb_id();
                    let live = root.join(gb_id.to_string());
                    let result = self.convert_item(&live, GB_ZIP, &keep);
                    if let Some(IoFailure { path, source }) = report.gb_addons.add(result) {
                        tracing::warn!(
                            %gb_id,
                            path = %path.display(),
                            error = %source,
                            "add-on copy left loose: it could not be packed"
                        );
                    }
                }
            }
            Err(error) => tracing::warn!(%error, "library conversion could not list add-ons"),
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

    /// Extracts add-on `gb_id`'s packed copy back into its folder and removes `copy.zip`, so a
    /// test can edit it; a loose or missing copy is left as it is.
    ///
    /// # Errors
    /// `Io` when the zip does not open or a file fails.
    #[doc(hidden)]
    pub fn unpack_gb_addon_for_tests(&self, gb_id: GbId) -> Result<(), LibraryError> {
        let live = self.dir.gb_addon_dir().join(gb_id.to_string());
        let _copies = self.copies_write();
        let source = ItemSource::open(&live, GB_ZIP)?;
        if source.is_packed() {
            source.extract_into(&live, &|_| false)?;
            remove_file_if_present(&live.join(GB_ZIP))?;
        }
        Ok(())
    }

    /// Replaces how a written zip is moved into place; tests use it to make the read-back fail.
    #[doc(hidden)]
    pub fn set_pack_rename(&mut self, rename: fn(&Path, &Path) -> io::Result<()>) {
        self.pack_rename = rename;
    }

    /// Packs the loose item at `live` into `live/zip_rel`, leaving out `zip_rel`'s first
    /// component and its `.tmp`, then clears the loose files; returns the zip's length. A zip
    /// that does not read back is removed (best effort), leaving the loose tree.
    pub(super) fn pack_folder(&self, live: &Path, zip_rel: &str) -> Result<u64, IoFailure> {
        let top = top_of(zip_rel);
        let tmp = tmp_of(Path::new(zip_rel));
        let left_out = |rel: &Path| rel == top || rel == tmp;
        let zip = live.join(zip_rel);
        let mut rename = self.pack_rename;
        let after = pack_with(live, &left_out, &zip, &mut rename)?;
        if let Err(failure) = require_packed(live, zip_rel) {
            let _ = remove_file_if_present(&zip);
            return Err(failure);
        }
        remove_loose(live, top)?;
        Ok(after)
    }

    /// Converts one item folder under the write guard; `keep_loose(live)` is asked where the
    /// made-HUD check is today. A folder that is gone (a save moved it) stays as it is.
    fn convert_item(
        &self,
        live: &Path,
        zip_rel: &str,
        keep_loose: &dyn Fn(&Path) -> bool,
    ) -> Result<Converted, IoFailure> {
        let _copies = self.copies_write();
        let is_dir = |dir: &Path| std::fs::symlink_metadata(dir).is_ok_and(|m| m.is_dir());
        if !is_dir(live) || keep_loose(live) {
            return Ok(Converted::Untouched);
        }
        let zip = live.join(zip_rel);
        remove_file_if_present(&tmp_of(&zip))?;
        if std::fs::symlink_metadata(&zip).is_ok() {
            // A crash between placing the zip and clearing the loose tree leaves both.
            let removed = drop_loose_tree(live, zip_rel)?;
            return Ok(if removed {
                Converted::Cleaned
            } else {
                Converted::Untouched
            });
        }
        let before = ItemSource::Loose(live.to_path_buf()).unpacked_bytes()?;
        let after = self.pack_folder(live, zip_rel)?;
        Ok(Converted::Packed { before, after })
    }
}

/// The top-level entry of an item folder that holds its zip and is never the item's own.
fn top_of(zip_rel: &str) -> &Path {
    Path::new(zip_rel)
        .components()
        .next()
        .map_or(Path::new(zip_rel), |c| Path::new(c.as_os_str()))
}

/// The `.tmp` path `pack_with` writes beside `zip`.
fn tmp_of(zip: &Path) -> PathBuf {
    let mut tmp = zip.as_os_str().to_owned();
    tmp.push(".tmp");
    PathBuf::from(tmp)
}

/// Removes the loose files beside `live`'s zip once the zip reads back as the item, never for
/// a link or a damaged zip; true when anything went.
fn drop_loose_tree(live: &Path, zip_rel: &str) -> Result<bool, IoFailure> {
    require_packed(live, zip_rel)?;
    remove_loose(live, top_of(zip_rel))
}

/// Fails unless readers would take `live` as packed: its zip a regular file whose central
/// directory parses with every name accepted. Guards every removal of loose files.
fn require_packed(live: &Path, zip_rel: &str) -> Result<(), IoFailure> {
    if ItemSource::open(live, zip_rel)?.is_packed() {
        return Ok(());
    }
    let path = live.join(zip_rel);
    let source = io::Error::new(io::ErrorKind::InvalidData, "the zip is not a regular file");
    Err(IoFailure { path, source })
}

/// Removes everything at the top of `dir` but `keep` (the zip's top entry), links included
/// and never followed; true when anything went.
fn remove_loose(dir: &Path, keep: &Path) -> Result<bool, IoFailure> {
    let mut removed = false;
    for entry in std::fs::read_dir(dir).map_err(io_at(dir))? {
        let entry = entry.map_err(io_at(dir))?;
        if entry.file_name() == keep.as_os_str() {
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
