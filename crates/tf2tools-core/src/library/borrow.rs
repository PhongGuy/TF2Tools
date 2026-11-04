//! Listing and opening the library HUDs a player can borrow a part from.

use std::path::Path;

use rusqlite::OptionalExtension;
use walkdir::WalkDir;

use super::made_huds::{read_row, readable_folder};
use super::{HudEntry, HudId, Library, is_plain_name};
use crate::error::{BorrowSourceError, LibraryError};
use crate::hud_maker::HudFiles;
use crate::paths::LibraryKind;

/// Whether `folder` holds a `.res` file under `resource/` or `scripts/`; symlinks are not followed.
fn has_res_files(folder: &Path) -> bool {
    ["resource", "scripts"].iter().any(|top| {
        WalkDir::new(folder.join(top))
            .follow_links(false)
            .into_iter()
            .filter_map(Result::ok)
            .any(|e| {
                e.file_type().is_file()
                    && e.path()
                        .extension()
                        .is_some_and(|x| x.eq_ignore_ascii_case("res"))
            })
    })
}

impl Library {
    /// The library HUDs with a readable folder holding `.res` files, except `except` (the HUD
    /// being edited), ordered by name.
    pub fn list_borrowable(&self, except: Option<&HudId>) -> Result<Vec<HudEntry>, LibraryError> {
        let huds_dir = self.dir.library_dir(LibraryKind::Huds);
        Ok(self
            .list_huds()?
            .into_iter()
            .filter(|h| except != Some(h.id()))
            .filter(|h| is_plain_name(h.folder_name()))
            .map(|h| {
                let folder = readable_folder(huds_dir.join(h.folder_name()));
                (h, folder)
            })
            .filter(|(_, folder)| {
                let linked = std::fs::symlink_metadata(folder).is_ok_and(|m| m.is_symlink());
                !linked && has_res_files(folder)
            })
            .map(|(h, _)| h)
            .collect())
    }

    /// The HUD `id` with its folder read above the game's files at `tf_dir`. Nothing is written.
    pub fn borrow_files(
        &self,
        id: &HudId,
        tf_dir: &Path,
    ) -> Result<(HudEntry, HudFiles), BorrowSourceError> {
        let entry = self
            .db
            .read(|conn| read_row(conn, id.as_str()).optional())
            .map_err(|_| BorrowSourceError::Unreadable)?
            .filter(|e| is_plain_name(e.folder_name()))
            .ok_or_else(|| BorrowSourceError::NotFound {
                id: id.as_str().to_owned(),
            })?;
        let live = self
            .dir
            .library_dir(LibraryKind::Huds)
            .join(entry.folder_name());
        let folder = readable_folder(live);
        if !folder.is_dir() {
            return Err(BorrowSourceError::Unreadable);
        }
        let files = HudFiles::open_hud(tf_dir, &folder)?;
        Ok((entry, files))
    }
}
