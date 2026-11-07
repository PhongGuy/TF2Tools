//! HUD card pictures: the HUD's own widescreen main-menu background, shrunk to a 16:9 JPEG
//! at `.tf2tools/thumbnail.jpg`, the size a store card decodes its screenshot to.

use std::path::{Path, PathBuf};

use rusqlite::params;

use super::{
    HudEntry, Library, METADATA_DIR, is_plain_name, library_db, read_setting, save_setting,
};
use crate::error::LibraryError;
use crate::fonts::Platform;
use crate::hud_maker::{HudFiles, HudImage};
use crate::paths::LibraryKind;
use crate::texture::{RgbaImage, decode_vtf, thumbnail_jpeg, vtf_size};

/// The picture's file under the HUD's `.tf2tools/`.
const THUMBNAIL_FILE: &str = "thumbnail.jpg";
/// A store card's picture box.
const SIZE: (u32, u32) = (480, 270);
/// The setting that records the one pass over a library from before thumbnails.
const BACKFILL_KEY: &str = "hud_thumbnails_made";
/// The most pixels a background may have before it is left undecoded (4096 x 4096).
const MAX_PIXELS: u64 = 16_777_216;

impl Library {
    /// Where `hud`'s card picture is, when it has one.
    pub fn hud_thumbnail(&self, hud: &HudEntry) -> Option<PathBuf> {
        let folder = hud.folder_name();
        (hud.has_thumbnail() && is_plain_name(folder)).then(|| {
            let huds = self.dir.library_dir(LibraryKind::Huds);
            huds.join(folder).join(METADATA_DIR).join(THUMBNAIL_FILE)
        })
    }

    /// Makes the card picture of every library HUD without one, the first time it runs on a
    /// library; later calls do nothing. Returns how many HUDs got a picture. Blocking: a HUD's
    /// background is decoded, so callers run it off the UI thread.
    ///
    /// # Errors
    /// `Database` when the HUDs or the setting cannot be read or written.
    pub fn make_missing_hud_thumbnails(&self) -> Result<usize, LibraryError> {
        let (done, rows) = self
            .db
            .read(|conn| {
                let done = read_setting(conn, BACKFILL_KEY)?.is_some();
                let mut stmt = conn
                    .prepare("SELECT id, folder_name FROM library_huds WHERE has_thumbnail = 0")?;
                let rows = stmt
                    .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?
                    .collect::<rusqlite::Result<Vec<_>>>()?;
                Ok((done, rows))
            })
            .map_err(library_db)?;
        if done {
            return Ok(0);
        }
        let huds = self.dir.library_dir(LibraryKind::Huds);
        let mut made = 0;
        for (id, folder) in rows.iter().filter(|(_, f)| is_plain_name(f)) {
            // A made HUD's save swaps its folder under this lock, so the picture never lands
            // in a folder about to be replaced.
            let _writes = self.lock_writes();
            let dir = huds.join(folder);
            if dir.is_dir() && write_thumbnail(&dir) {
                self.set_thumbnail_flag(id, true)?;
                made += 1;
            }
        }
        self.db
            .write(|conn| save_setting(conn, BACKFILL_KEY, "1"))
            .map_err(library_db)?;
        Ok(made)
    }

    /// Makes (or clears) `entry`'s card picture from its folder as it now is, and returns the
    /// entry with the flag the database holds. A failure only leaves the old flag.
    pub(super) fn refresh_hud_thumbnail(&self, entry: HudEntry) -> HudEntry {
        let folder = entry.folder_name();
        let made = is_plain_name(folder)
            && write_thumbnail(&self.dir.library_dir(LibraryKind::Huds).join(folder));
        if made == entry.has_thumbnail() {
            return entry;
        }
        match self.set_thumbnail_flag(entry.id().as_str(), made) {
            Ok(()) => entry.with_thumbnail(made),
            Err(_) => entry,
        }
    }

    fn set_thumbnail_flag(&self, id: &str, made: bool) -> Result<(), LibraryError> {
        self.db
            .write(|conn| {
                conn.execute(
                    "UPDATE library_huds SET has_thumbnail = ?2 WHERE id = ?1",
                    params![id, i64::from(made)],
                )
            })
            .map(|_| ())
            .map_err(library_db)
    }
}

/// Writes `<hud_dir>/.tf2tools/thumbnail.jpg` from the HUD's widescreen menu background and
/// says whether it did; with no usable background any old picture is removed. A linked
/// `.tf2tools` is never written through.
fn write_thumbnail(hud_dir: &Path) -> bool {
    let meta = hud_dir.join(METADATA_DIR);
    let path = meta.join(THUMBNAIL_FILE);
    let jpeg = HudFiles::open_folder(hud_dir)
        .ok()
        .and_then(|files| menu_picture(&files))
        .and_then(|image| thumbnail_jpeg(&image, SIZE.0, SIZE.1));
    let meta_ok = match std::fs::symlink_metadata(&meta) {
        Ok(m) => m.is_dir(),
        Err(_) => std::fs::create_dir_all(&meta).is_ok(),
    };
    let Some(jpeg) = jpeg.filter(|_| meta_ok) else {
        if meta_ok {
            let _ = std::fs::remove_file(&path);
        }
        return false;
    };
    // Written beside and renamed over, so a card never reads half a picture.
    let partial = meta.join("thumbnail.jpg.part");
    let written =
        std::fs::write(&partial, &jpeg).is_ok() && std::fs::rename(&partial, &path).is_ok();
    if !written {
        let _ = std::fs::remove_file(&partial);
    }
    written
}

/// The first widescreen menu background that decodes: the chapter list's own first, then
/// every `background*_widescreen` the HUD ships, by name.
fn menu_picture(files: &HudFiles) -> Option<RgbaImage> {
    let listed = (files.menu_background(false, Platform::Linux).ok().flatten())
        .and_then(|value| Some(value.strip_prefix("../console/")?.to_ascii_lowercase()));
    let shipped = files
        .list("materials/console/")
        .into_iter()
        .filter_map(|key| {
            let name = key.strip_prefix("materials/console/")?;
            let stem = (name.strip_suffix(".vmt")).or_else(|| name.strip_suffix(".vtf"))?;
            let base = stem.strip_suffix("_widescreen")?;
            (base.starts_with("background") && !base.contains('/')).then(|| base.to_owned())
        });
    listed
        .into_iter()
        .chain(shipped)
        .find_map(|name| widescreen(files, &name))
}

/// `<name>_widescreen` decoded through its material when the HUD has one, else straight from
/// the texture of that name the game's own material would load.
fn widescreen(files: &HudFiles, name: &str) -> Option<RgbaImage> {
    let wide = format!("{name}_widescreen");
    if let Ok(HudImage::Found(texture)) =
        files.image(&format!("../console/{wide}"), Platform::Linux)
    {
        return Some(texture.image().clone());
    }
    let file = files
        .read(&format!("materials/console/{wide}.vtf"))
        .ok()??;
    let (w, h) = vtf_size(file.bytes())?;
    if u64::from(w) * u64::from(h) > MAX_PIXELS {
        return None;
    }
    decode_vtf(file.bytes()).ok()
}
