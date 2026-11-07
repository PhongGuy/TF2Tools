//! Made HUDs: the HUD Maker's changes saved as a sparse library HUD. The folder holds
//! `info.vdf`, each touched file as edited and `.tf2tools/maker.json`; it is built under the
//! library tmp dir and renamed into place, and the database row is written last.

use std::collections::{BTreeMap, BTreeSet};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use rusqlite::{OptionalExtension, TransactionBehavior, params};
use sha2::{Digest, Sha256};
use walkdir::WalkDir;

use super::hud_copy::read_maker;
use super::huds::{HUD_COLUMNS, hud_from_row};
use super::{
    HudEntry, HudId, IoFailure, Library, METADATA_DIR, create_dir_all, folder_name, io_at,
    is_plain_name, library_db, new_id, now, remove_dir_if_present, walk_failure,
};
use crate::error::{HudLoadError, LibraryError, MakeHudError};
use crate::fonts::Platform;
use crate::hud_maker::{
    ANIM_MANIFEST, AnimationSave, AssetFile, BORROWED_DIR, Change, ChangeTarget, Edited, FONT_DIR,
    HudFiles, HudModel, HudOptions, IMAGES_DIR, KEPT_DIR, MAX_IMAGE_BYTES, MakerBase, MakerFile,
    OWN_SCRIPT, animation_files, check_hud_path, is_picture_path, within_caps,
};
use crate::paths::LibraryKind;

/// The made HUD's record of its changes, inside `.tf2tools/`.
pub(super) const MAKER_FILE: &str = "maker.json";

/// A copy's kept base bytes of each file the Maker has written over, inside `.tf2tools/`.
pub(super) const ORIGINAL_DIR: &str = "original";

/// The paths a copy save is adding over game files, listed under `.tf2tools/` until its
/// `maker.json` names them.
pub(super) const ADDING_FILE: &str = "adding.txt";

/// The note that names the authors of borrowed parts, beside `info.vdf`.
pub(super) const CREDITS_FILE: &str = "TF2Tools-credits.txt";

/// Name chars kept in a new folder's slug: at four bytes each, the slug, id suffix and `.old`
/// stay under the 255-byte file name limit.
pub(super) const MAX_SLUG_CHARS: usize = 48;

/// The bytes of the files a made HUD adds, by their `source` (`.tf2tools/...`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct HudAssets {
    bytes: BTreeMap<String, Arc<[u8]>>,
}

impl HudAssets {
    /// No files.
    pub fn new() -> HudAssets {
        HudAssets::default()
    }

    /// Keeps `bytes` as the file at `source`, replacing any earlier bytes for it.
    pub fn insert(&mut self, source: &str, bytes: Vec<u8>) {
        self.bytes.insert(source.to_owned(), bytes.into());
    }

    /// The bytes kept for `source`.
    pub fn get(&self, source: &str) -> Option<&[u8]> {
        self.bytes.get(source).map(|b| &**b)
    }

    /// How many files are kept.
    pub fn len(&self) -> usize {
        self.bytes.len()
    }

    /// True when no file is kept.
    pub fn is_empty(&self) -> bool {
        self.bytes.is_empty()
    }
}

/// Where a save goes: a new library HUD, or over a HUD the maker made before.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum MakeTarget {
    /// A new library HUD.
    New,
    /// The library HUD with this id, which must hold a `maker.json`.
    Existing(HudId),
    /// A new library HUD made from a whole copy of the HUD copy with this id.
    CopyOf(HudId),
}

/// A saved made HUD: its library row and how many changes did not resolve.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct SavedHud {
    entry: HudEntry,
    dropped: usize,
}

impl SavedHud {
    /// The library row as written.
    pub fn entry(&self) -> &HudEntry {
        &self.entry
    }

    /// How many changes were left out because they did not resolve.
    pub fn dropped(&self) -> usize {
        self.dropped
    }
}

/// A made HUD reopened for editing: its row, the game's pristine files and the saved changes
/// that still resolve over them.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct MadeHud {
    entry: HudEntry,
    base: MakerBase,
    options: Arc<HudOptions>,
    files: HudFiles,
    pristine: HudModel,
    changes: Vec<Change>,
    dropped: usize,
    tf2_updated: bool,
    assets: HudAssets,
    pictures_left_out: usize,
}

impl MadeHud {
    /// The library row.
    pub fn entry(&self) -> &HudEntry {
        &self.entry
    }

    /// The game's files as read now, with the HUD's own choices switched in; replay
    /// `changes()` over it with `with_changes`.
    pub fn pristine(&self) -> &HudModel {
        &self.pristine
    }

    /// What the HUD was made over, as its `maker.json` recorded it when reopened; a copy's
    /// changes when it is updated.
    pub fn base(&self) -> &MakerBase {
        &self.base
    }

    /// The copy's own options, read from its pristine files; empty for a HUD that is no copy.
    pub fn options(&self) -> &Arc<HudOptions> {
        &self.options
    }

    /// The files `pristine()` was loaded from: the given files with the switch layer on top.
    pub fn files(&self) -> &HudFiles {
        &self.files
    }

    /// The saved changes that still resolve, in the order they were made.
    pub fn changes(&self) -> &[Change] {
        &self.changes
    }

    /// How many saved changes were left out: refused by the path rule or no longer resolving.
    pub fn dropped(&self) -> usize {
        self.dropped
    }

    /// True when a file the changes touch differs from the bytes they were saved against.
    pub fn tf2_updated(&self) -> bool {
        self.tf2_updated
    }

    /// How many borrowed pictures were left out on reopen because the saved file listed more
    /// than a HUD may hold.
    pub fn pictures_left_out(&self) -> usize {
        self.pictures_left_out
    }

    /// A plain sentence for the player when pictures were left out, else `None`.
    pub fn notice(&self) -> Option<&'static str> {
        (self.pictures_left_out > 0).then_some(
            "Some borrowed pictures were left out because this HUD held more than it can keep.",
        )
    }

    /// The kept copy of every applied asset, read from the HUD folder on reopen; an asset
    /// whose copy cannot be read is dropped with its group (counted in `dropped`).
    pub fn assets(&self) -> &HudAssets {
        &self.assets
    }
}

impl Library {
    /// Reopens the made HUD `id` over `files`, replaying its `maker.json`.
    ///
    /// # Errors
    /// `NotFound` for no such row, `NotMade` when the folder has no `maker.json`, `Damaged` or
    /// `Newer` from the file itself, `Files` when the game's files cannot be read, `Io` or
    /// `Database` when reading fails.
    pub fn open_made_hud(
        &self,
        id: &HudId,
        files: &HudFiles,
        platform: Platform,
    ) -> Result<MadeHud, HudLoadError> {
        let not_found = || HudLoadError::NotFound {
            id: id.as_str().to_owned(),
        };
        let entry = self
            .db
            .read(|conn| read_row(conn, id.as_str()).optional())
            .map_err(load_db)?
            .filter(|e| is_plain_name(e.folder_name()))
            .ok_or_else(not_found)?;
        let live = self
            .dir
            .library_dir(LibraryKind::Huds)
            .join(entry.folder_name());
        let folder = readable_folder(live);
        let path = folder.join(METADATA_DIR).join(MAKER_FILE);
        if !path.is_file() {
            return Err(HudLoadError::NotMade {
                id: id.as_str().to_owned(),
            });
        }
        let (maker, refused) = MakerFile::parse(&read_capped(&path)?)?;

        // The HUD's own choices switch the files first; the model loads over the switched view.
        let options = Arc::new(self.hud_options(files, Some(maker.base()), platform));
        let switched = options.switch(files, maker.changes())?;
        let stored = switched.kept(maker.changes());
        let refused = refused + switched.dropped();
        let switched = switched.into_files();
        let pristine = HudModel::load(&switched, platform)?;
        // Read each applied asset's kept copy; a copy that cannot be read takes its group out.
        let mut assets = HudAssets::new();
        let (mut edited, mut removed) =
            replay_with_kept_copies(&pristine, &stored, &folder, &mut assets);
        // A hand-edited `maker.json` must not get past the picture caps the save enforces: the
        // borrowed pictures past them are left out and the player is told.
        let over = groups_over_caps(edited.applied(), &assets);
        let mut pictures_left_out = 0;
        if !over.is_empty() {
            pictures_left_out = (edited.applied().iter())
                .filter(|c| over.contains(&c.group()))
                .filter(|c| matches!(c.target(), ChangeTarget::Asset(_)))
                .count();
            let kept: Vec<Change> = (stored.iter())
                .filter(|c| !over.contains(&c.group()))
                .cloned()
                .collect();
            let (again, more) = replay_with_kept_copies(&pristine, &kept, &folder, &mut assets);
            (edited, removed) = (again, stored.len() - kept.len() + more);
        }
        // Bytes read for a group that a later missing copy dropped are not the HUD's any more;
        // an applied pick that a later one overrides keeps its bytes for the next save.
        let named: BTreeSet<String> = (edited.applied().iter())
            .filter_map(|c| match c.target() {
                ChangeTarget::Asset(asset) => asset_source(asset),
                ChangeTarget::Set(_)
                | ChangeTarget::Remove(_)
                | ChangeTarget::AddBlock(_)
                | ChangeTarget::Option(_)
                | ChangeTarget::Credit(_)
                | ChangeTarget::Animation(_)
                | ChangeTarget::AnimationAppend(_) => None,
            })
            .collect();
        assets.bytes.retain(|source, _| named.contains(source));
        // Sources hash base bytes: a path the switch layer writes or takes is read from the
        // unswitched files, any other path's document is the same in the switched model, and a
        // path only a switch adds has no base bytes and is skipped.
        let (added, taken) = switched.switched_paths();
        // A file the game no longer ships counts as changed, like one with different bytes.
        // The manifest is no part document, so its bytes are read from the files.
        let tf2_updated = maker.sources().iter().any(|(path, hash)| {
            let key = path.to_ascii_lowercase();
            if added.contains(&key) && !files.in_copy(path) {
                return false;
            }
            let switched_here = added.contains(&key) || taken.contains(&key);
            let now = match pristine.document(path).filter(|_| !switched_here) {
                Some(doc) => Some(sha256_hex(doc.as_bytes())),
                None => (files.read(path).ok().flatten()).map(|f| sha256_hex(f.bytes())),
            };
            now.as_ref() != Some(hash)
        });
        Ok(MadeHud {
            entry,
            base: maker.base().clone(),
            options,
            files: switched,
            changes: edited.applied().to_vec(),
            dropped: refused + removed + edited.dropped(),
            tf2_updated,
            pristine,
            assets,
            pictures_left_out,
        })
    }

    /// True when the HUD's folder, or its `.old` mid re-save, holds `.tf2tools/maker.json`;
    /// false for an unknown id. Never writes, so it is safe beside a running save.
    ///
    /// # Errors
    /// `Database` when the lookup fails.
    pub fn is_made_hud(&self, id: &HudId) -> Result<bool, LibraryError> {
        let Some(folder) = self.hud_folder(id).map_err(library_db)? else {
            return Ok(false);
        };
        let live = self.dir.library_dir(LibraryKind::Huds).join(folder);
        let maker = |dir: &Path| dir.join(METADATA_DIR).join(MAKER_FILE).is_file();
        Ok(maker(&live) || maker(&old_folder(&live)))
    }

    /// The folder of HUD `id`, when the row exists and its folder name is a plain name.
    pub(super) fn hud_folder(&self, id: &HudId) -> rusqlite::Result<Option<String>> {
        let folder: Option<String> = self.db.read(|conn| {
            conn.query_row(
                "SELECT folder_name FROM library_huds WHERE id = ?1",
                [id.as_str()],
                |r| r.get(0),
            )
            .optional()
        })?;
        Ok(folder.filter(|f| is_plain_name(f)))
    }

    /// Saves `changes` replayed over `files` as a made HUD called `name` (trimmed). Only the
    /// changes that resolve are written; nothing is written for a refused name or target.
    ///
    /// # Errors
    /// `EmptyName`, `NameTaken` (any case), `NotFound` or `NotMade` for the target, `Files`
    /// when the game's files cannot be read, `Io` or `Database` when writing fails.
    pub fn save_made_hud(
        &self,
        target: &MakeTarget,
        name: &str,
        files: &HudFiles,
        platform: Platform,
        changes: &[Change],
    ) -> Result<SavedHud, MakeHudError> {
        let assets = HudAssets::new();
        self.save_made_hud_with_assets(target, name, files, platform, changes, &assets)
    }

    /// As `save_made_hud`, also writing each applied asset's bytes from `assets` at its `path`
    /// and its `source`. An applied asset without bytes is left out with every change of its
    /// group, each counted in `dropped`, before the HUD is replayed and written.
    ///
    /// # Errors
    /// As `save_made_hud`.
    pub fn save_made_hud_with_assets(
        &self,
        target: &MakeTarget,
        name: &str,
        files: &HudFiles,
        platform: Platform,
        changes: &[Change],
        assets: &HudAssets,
    ) -> Result<SavedHud, MakeHudError> {
        let _writes = self.lock_writes();
        self.save_made_hud_locked(target, name, files, platform, changes, assets)
            .map(|saved| self.with_fresh_thumbnail(saved))
    }

    /// As `save_made_hud_with_assets`, refusing with `BaseChanged` before writing anything when
    /// `target` is an existing copy whose recorded base is no longer `opened`. Any other target
    /// ignores `opened`.
    ///
    /// # Errors
    /// As `save_made_hud`, and `BaseChanged`.
    #[expect(
        clippy::too_many_arguments,
        reason = "the save's own arguments plus the base the page opened"
    )]
    pub fn save_made_hud_opened(
        &self,
        target: &MakeTarget,
        name: &str,
        files: &HudFiles,
        platform: Platform,
        changes: &[Change],
        assets: &HudAssets,
        opened: &MakerBase,
    ) -> Result<SavedHud, MakeHudError> {
        let _writes = self.lock_writes();
        if let MakeTarget::Existing(id) = target {
            let folder = self.made_folder(id)?;
            let huds = self.dir.library_dir(LibraryKind::Huds);
            let now = read_maker(&huds.join(folder)).map_err(make_load)?;
            if let Some(now) = now
                && matches!(now.base(), MakerBase::Copy(_))
                && now.base() != opened
            {
                return Err(MakeHudError::BaseChanged);
            }
        }
        self.save_made_hud_locked(target, name, files, platform, changes, assets)
            .map(|saved| self.with_fresh_thumbnail(saved))
    }

    // A saved HUD's card picture follows its menu background, which the save may have changed.
    fn with_fresh_thumbnail(&self, saved: SavedHud) -> SavedHud {
        SavedHud {
            entry: self.refresh_hud_thumbnail(saved.entry),
            ..saved
        }
    }

    // The save itself; the caller holds the library write lock, which is not reentrant.
    fn save_made_hud_locked(
        &self,
        target: &MakeTarget,
        name: &str,
        files: &HudFiles,
        platform: Platform,
        changes: &[Change],
        assets: &HudAssets,
    ) -> Result<SavedHud, MakeHudError> {
        let name = name.trim();
        if name.is_empty() {
            return Err(MakeHudError::EmptyName);
        }
        let (id, copy_of) = match target {
            MakeTarget::New => (None, None),
            MakeTarget::Existing(id) => (Some(id), None),
            MakeTarget::CopyOf(from) => (None, Some(from)),
        };
        if self
            .db
            .read(|conn| name_taken(conn, name, id))
            .map_err(make_db)?
        {
            return Err(name_taken_error(name));
        }
        // Every refusal about the target comes before anything is built or moved.
        let existing = match id {
            Some(id) => Some(self.made_folder(id)?),
            None => None,
        };
        let source = match copy_of {
            Some(from) => Some(self.copy_source(from)?),
            None => None,
        };

        let huds = self.dir.library_dir(LibraryKind::Huds);
        // A copy records its base in `maker.json` and is saved in place over its own files.
        let previous = match (&existing, &source) {
            (Some(folder), _) => read_maker(&huds.join(folder)).map_err(make_load)?,
            (None, Some((_, maker))) => Some(maker.clone()),
            (None, None) => None,
        };
        // The HUD's own choices switch the files first; everything below reads the switched
        // view, whose pristine reads still give the base bytes.
        let options = self.hud_options(files, previous.as_ref().map(MakerFile::base), platform);
        let switched = options.switch(files, changes)?;
        let switch_refused = switched.dropped();
        let changes = switched.kept(changes);
        let switched = switched.into_files();
        let files = &switched;
        let model = HudModel::load(files, platform)?;
        // A HUD with no manifest in any layer has nowhere to list our script.
        let (changes, unlisted) = without_animations_unless_listed(files, &changes)?;
        let changes = changes.as_slice();
        let has_bytes = |asset: &AssetFile| assets.get(&asset_source(asset)?).map(|_| ());
        let (mut edited, mut removed) = without_missing_assets(&model, changes, has_bytes);
        // A group of borrowed pictures past a cap is left out whole and counted as dropped.
        let over = groups_over_caps(edited.applied(), assets);
        if !over.is_empty() {
            let kept: Vec<Change> = (changes.iter())
                .filter(|c| !over.contains(&c.group()))
                .cloned()
                .collect();
            let (again, more) = without_missing_assets(&model, &kept, has_bytes);
            (edited, removed) = (again, changes.len() - kept.len() + more);
        }
        removed += unlisted + switch_refused;

        // The scratch dir is removed when the guard drops, on every return path.
        let tmp = self.dir.tmp_dir();
        create_dir_all(&tmp)?;
        let scratch = tempfile::Builder::new()
            .tempdir_in(&tmp)
            .map_err(io_at(&tmp))?;
        // `folder_name` slugs every separator away, and `made_folder` passed `is_plain_name`.
        let new_id = new_id();
        let folder = existing.unwrap_or_else(|| {
            let short: String = name.chars().take(MAX_SLUG_CHARS).collect();
            folder_name(&short, &new_id)
        });
        create_dir_all(&huds)?;
        let live = huds.join(&folder);
        if let Some((from_dir, maker)) = &source {
            // Save as new from a copy: the whole folder, originals and history included,
            // copied anew and then saved in place like the copy itself.
            let copied = scratch.path().join("copy").join(&folder);
            let skip = |rel: &Path| rel == Path::new(METADATA_DIR).join(MAKER_FILE);
            super::tree_copy::copy_tree(from_dir, &copied, &skip)?;
            let build = scratch.path().join(&folder);
            removed += write_build(
                &build,
                &folder,
                &model,
                &edited,
                assets,
                files,
                platform,
                maker.base(),
            )?;
            write_copy_in_place(&copied, &build, &scratch.path().join("stage"), files, maker)?;
            prune_pictures(&copied, edited.applied())?;
            rename(&copied, &live)?;
            let inserted = self.insert_made_row(&new_id, &folder, name);
            if !matches!(inserted, Ok(Some(_))) {
                let _ = remove_dir_if_present(&live);
            }
            return Ok(SavedHud {
                entry: inserted?.ok_or_else(|| name_taken_error(name))?,
                dropped: removed + edited.dropped(),
            });
        }
        let base = previous
            .as_ref()
            .map_or(MakerBase::Default, |m| m.base().clone());
        let build = scratch.path().join(&folder);
        removed += write_build(
            &build, &folder, &model, &edited, assets, files, platform, &base,
        )?;
        if let (Some(id), Some(previous)) = (id, &previous)
            && matches!(base, MakerBase::Copy(_))
        {
            write_copy_in_place(
                &live,
                &build,
                &scratch.path().join("stage"),
                files,
                previous,
            )?;
            prune_pictures(&live, edited.applied())?;
            let entry = self
                .update_made_row(id, name)?
                .ok_or_else(|| name_taken_error(name))?;
            return Ok(SavedHud {
                entry,
                dropped: removed + edited.dropped(),
            });
        }
        let entry = match id {
            None => {
                prune_pictures(&build, edited.applied())?;
                rename(&build, &live)?;
                let inserted = self.insert_made_row(&new_id, &folder, name);
                if !matches!(inserted, Ok(Some(_))) {
                    let _ = remove_dir_if_present(&live);
                }
                inserted?.ok_or_else(|| name_taken_error(name))?
            }
            Some(id) => {
                // Carry the live metadata over first so pruning keeps its snapshots' pictures.
                carry_metadata(&live.join(METADATA_DIR), &build.join(METADATA_DIR))?;
                prune_pictures(&build, edited.applied())?;
                // Swap through `<folder>.old` so the previous save is never half replaced.
                let old = old_folder(&live);
                rename(&live, &old)?;
                let swapped = rename(&build, &live).and_then(|()| {
                    self.update_made_row(id, name)?
                        .ok_or_else(|| name_taken_error(name))
                });
                match swapped {
                    Ok(updated) => {
                        // The save is committed; a `.old` left here is removed by the next
                        // save's recovery rather than reported as a failed save.
                        let _ = remove_dir_if_present(&old);
                        updated
                    }
                    Err(e) => {
                        roll_back(&live, &old, &scratch.path().join("failed"));
                        return Err(e);
                    }
                }
            }
        };
        Ok(SavedHud {
            entry,
            dropped: removed + edited.dropped(),
        })
    }

    /// The folder of the made HUD `id`, refusing a missing row or a HUD with no `maker.json`.
    pub(super) fn made_folder(&self, id: &HudId) -> Result<String, MakeHudError> {
        let folder =
            self.hud_folder(id)
                .map_err(make_db)?
                .ok_or_else(|| MakeHudError::NotFound {
                    id: id.as_str().to_owned(),
                })?;
        let live = self.dir.library_dir(LibraryKind::Huds).join(&folder);
        recover_swap(&live)?;
        let maker = live.join(METADATA_DIR).join(MAKER_FILE);
        if !maker.is_file() {
            return Err(MakeHudError::NotMade {
                id: id.as_str().to_owned(),
            });
        }
        Ok(folder)
    }

    /// The folder and `maker.json` of the copy `from`, for a save as new; refused before
    /// anything is written when it is no copy or the disk lacks room for its whole folder.
    fn copy_source(&self, from: &HudId) -> Result<(PathBuf, MakerFile), MakeHudError> {
        let folder = self.made_folder(from)?;
        let dir = self.dir.library_dir(LibraryKind::Huds).join(folder);
        let maker = read_maker(&dir)
            .map_err(make_load)?
            .filter(|m| matches!(m.base(), MakerBase::Copy(_)))
            .ok_or_else(|| MakeHudError::NotMade {
                id: from.as_str().to_owned(),
            })?;
        let skip = |rel: &Path| rel == Path::new(METADATA_DIR).join(MAKER_FILE);
        let bytes = super::tree_copy::tree_bytes(&dir, &skip)?;
        self.check_space(bytes)
            .map_err(|(needed, free)| MakeHudError::NoSpace { needed, free })?;
        Ok((dir, maker))
    }

    /// Inserts a local row for a new made HUD; `None` when the name was taken meanwhile.
    fn insert_made_row(
        &self,
        id: &str,
        folder: &str,
        name: &str,
    ) -> Result<Option<HudEntry>, MakeHudError> {
        let timestamp = now();
        self.db
            .write(|conn| {
                let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
                if name_taken(&tx, name, None)? {
                    return Ok(None);
                }
                tx.execute(
                    "INSERT INTO library_huds (id, folder_name, name, author, source,
                         created_at, updated_at)
                     VALUES (?1, ?2, ?3, '', 'local', ?4, ?4)",
                    params![id, folder, name, timestamp],
                )?;
                let entry = read_row(&tx, id)?;
                tx.commit()?;
                Ok(Some(entry))
            })
            .map_err(make_db)
    }

    /// Renames a made HUD's row; `None` when the name was taken meanwhile.
    pub(super) fn update_made_row(
        &self,
        id: &HudId,
        name: &str,
    ) -> Result<Option<HudEntry>, MakeHudError> {
        let timestamp = now();
        self.db
            .write(|conn| {
                let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
                if name_taken(&tx, name, Some(id))? {
                    return Ok(None);
                }
                tx.execute(
                    "UPDATE library_huds SET name = ?2, updated_at = ?3 WHERE id = ?1",
                    params![id.as_str(), name, timestamp],
                )?;
                let entry = read_row(&tx, id.as_str())?;
                tx.commit()?;
                Ok(Some(entry))
            })
            .map_err(make_db)
    }
}

/// `<folder>.old`, the previous save while a re-save swaps folders.
pub(super) fn old_folder(live: &Path) -> PathBuf {
    let mut name = live.file_name().unwrap_or_default().to_os_string();
    name.push(".old");
    live.with_file_name(name)
}

/// `<folder>.old` when it is a made HUD's previous save stranded by an interrupted re-save,
/// recognised by its `.tf2tools/maker.json`; an imported folder merely named `x.old` has none.
/// A symlink is never a save, so a planted link cannot pull files from outside the library.
pub(super) fn stranded_save(live: &Path) -> Option<PathBuf> {
    let old = old_folder(live);
    let real_dir = std::fs::symlink_metadata(&old).is_ok_and(|meta| meta.is_dir());
    (real_dir && old.join(METADATA_DIR).join(MAKER_FILE).is_file()).then_some(old)
}

/// Where a made HUD's files are now: the live folder, or `<folder>.old` when a re-save is
/// caught between its renames. Only a save recovers the swap; readers never write.
pub(super) fn readable_folder(live: PathBuf) -> PathBuf {
    if live.exists() {
        live
    } else {
        old_folder(&live)
    }
}

/// Finishes a re-save a crash interrupted. A lone `.old` is the previous save caught between
/// the renames; with both present the live folder is the newer complete build.
pub(super) fn recover_swap(live: &Path) -> Result<(), IoFailure> {
    let old = old_folder(live);
    if !old.exists() {
        return Ok(());
    }
    if live.exists() {
        remove_dir_if_present(&old)
    } else {
        std::fs::rename(&old, live).map_err(io_at(live))
    }
}

/// Reads `maker.json` up to one byte past the cap, so `MakerFile::parse` refuses an oversized
/// file without it ever being read whole.
pub(super) fn read_capped(path: &Path) -> Result<Vec<u8>, IoFailure> {
    let file = std::fs::File::open(path).map_err(io_at(path))?;
    let mut bytes = Vec::new();
    file.take(MakerFile::MAX_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(io_at(path))?;
    Ok(bytes)
}

/// Puts the previous save back after a failed swap. The new folder is moved into the scratch
/// dir, never deleted in place: a half-deleted folder beside `.old` would win the next save's
/// recovery. When the move fails both folders stay, and recovery keeps the complete new build.
pub(super) fn roll_back(live: &Path, old: &Path, discard: &Path) {
    let moved = !live.exists() || std::fs::rename(live, discard).is_ok();
    if moved {
        let _ = std::fs::rename(old, live);
    }
}

/// Copies the previous save's `.tf2tools/` into the build, except any file the build has
/// written afresh (its `maker.json`, a kept font picked again under the same name). Symlinks
/// are skipped, as everywhere in the library.
fn carry_metadata(from: &Path, to: &Path) -> Result<(), MakeHudError> {
    for entry in WalkDir::new(from).min_depth(1) {
        let entry = entry.map_err(|e| walk_failure(from, e))?;
        let Ok(relative) = entry.path().strip_prefix(from) else {
            continue;
        };
        if relative == Path::new(MAKER_FILE) {
            continue;
        }
        let target = to.join(relative);
        if entry.file_type().is_dir() {
            create_dir_all(&target)?;
        } else if entry.file_type().is_file() && !target.exists() {
            std::fs::copy(entry.path(), &target).map_err(io_at(&target))?;
        }
    }
    Ok(())
}

/// Replays `changes` over `model`, then again without every change group holding an applied
/// asset that `found` refuses; returns the replay and how many changes those groups held.
fn without_missing_assets(
    model: &HudModel,
    changes: &[Change],
    mut found: impl FnMut(&AssetFile) -> Option<()>,
) -> (Edited, usize) {
    let edited = model.with_changes(changes);
    let missing: BTreeSet<u32> = (edited.applied().iter())
        .filter(|c| matches!(c.target(), ChangeTarget::Asset(a) if found(a).is_none()))
        .map(Change::group)
        .collect();
    if missing.is_empty() {
        return (edited, 0);
    }
    let kept: Vec<Change> = (changes.iter())
        .filter(|c| !missing.contains(&c.group()))
        .cloned()
        .collect();
    let removed = changes.len() - kept.len();
    (model.with_changes(&kept), removed)
}

/// Replays `changes`, reading each applied asset's kept copy into `assets`; a copy that cannot
/// be read takes its group out. Returns the replay and how many changes were removed.
pub(super) fn replay_with_kept_copies(
    pristine: &HudModel,
    changes: &[Change],
    folder: &Path,
    assets: &mut HudAssets,
) -> (Edited, usize) {
    without_missing_assets(pristine, changes, |asset| {
        let source = asset_source(asset)?;
        if assets.get(&source).is_none() {
            assets.insert(&source, read_kept_copy(folder, &source)?);
        }
        Some(())
    })
}

/// The asset's normalised `source` when it is a file the maker added: a font (its `path` under
/// `resource/fonts/tf2tools/`, kept under `.tf2tools/fonts/`) or a picture (a `.vmt` or `.vtf`
/// under `materials/`, or the chapter list, kept at the same path under `.tf2tools/borrowed/`
/// or `.tf2tools/images/<16 hex>/`). Any other asset, such as one from a hand-edited
/// `maker.json`, has no bytes the library will read.
pub(super) fn asset_source(asset: &AssetFile) -> Option<String> {
    let path = check_hud_path(asset.path())?;
    let source = check_hud_path(asset.source())?;
    let inside = |p: &str, dir: &str| p.strip_prefix(dir).is_some_and(|r| r.starts_with('/'));
    let font = inside(&path, FONT_DIR) && inside(&source, KEPT_DIR);
    let picture_path = is_picture_path(&path);
    let borrowed = source
        .strip_prefix(BORROWED_DIR)
        .and_then(|r| r.strip_prefix('/'))
        == Some(&path);
    // A picked picture's folder is its content id: exactly 16 lowercase hex digits.
    let picked = (source.strip_prefix(IMAGES_DIR))
        .and_then(|r| r.strip_prefix('/'))
        .and_then(|r| r.split_once('/'))
        .is_some_and(|(id, rest)| {
            id.len() == 16
                && id.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
                && rest == path
        });
    (font || (picture_path && (borrowed || picked))).then_some(source)
}

/// Whether a kept copy is a picture, borrowed or picked.
pub(super) fn is_picture(source: &str) -> bool {
    source.starts_with(&format!("{BORROWED_DIR}/")) || source.starts_with(&format!("{IMAGES_DIR}/"))
}

/// True when no change group's pictures, borrowed and picked together, pass the picture caps.
pub fn pictures_fit(changes: &[Change], assets: &HudAssets) -> bool {
    groups_over_caps(changes, assets).is_empty()
}

/// The change groups whose picture files do not fit the picture caps. Groups are taken in the
/// order they first appear, each kept whole or dropped whole.
fn groups_over_caps(applied: &[Change], assets: &HudAssets) -> BTreeSet<u32> {
    let mut order: Vec<u32> = Vec::new();
    let mut sizes_by_group: BTreeMap<u32, Vec<u64>> = BTreeMap::new();
    for change in applied {
        let ChangeTarget::Asset(asset) = change.target() else {
            continue;
        };
        let Some(source) = asset_source(asset).filter(|s| is_picture(s)) else {
            continue;
        };
        let size = assets.get(&source).map_or(0, |b| b.len() as u64);
        if !order.contains(&change.group()) {
            order.push(change.group());
        }
        sizes_by_group.entry(change.group()).or_default().push(size);
    }
    let (mut kept, mut over) = (Vec::new(), BTreeSet::new());
    for group in order {
        let mut tried = kept.clone();
        tried.extend(&sizes_by_group[&group]);
        if within_caps(&tried) {
            kept = tried;
        } else {
            over.insert(group);
        }
    }
    over
}

/// Reads the kept copy at `source` under `folder`, or `None` when any part of the way is a
/// symlink, the file is not a regular file, is over the cap or cannot be read.
pub(super) fn read_kept_copy(folder: &Path, source: &str) -> Option<Vec<u8>> {
    let mut path = folder.to_path_buf();
    for segment in source.split('/') {
        path.push(segment);
        if std::fs::symlink_metadata(&path).ok()?.is_symlink() {
            return None;
        }
    }
    // A picture has a tighter cap than a font.
    let cap = if is_picture(source) {
        MAX_IMAGE_BYTES
    } else {
        crate::fonts::names::MAX_FONT_BYTES
    };
    crate::fonts::read_regular_capped(&path, cap).ok().flatten()
}

/// Writes `bytes` as the kept picture copy at `source` under `folder` unless a regular file is
/// already there. Refuses a link anywhere on the way, a step that is not a folder, and bytes
/// over the picture cap; the copy is written beside its name and renamed in.
pub(super) fn write_kept_copy(folder: &Path, source: &str, bytes: &[u8]) -> Result<(), IoFailure> {
    let refused = |path: &Path, why: &str| io_at(path)(std::io::Error::other(why.to_owned()));
    if bytes.len() as u64 > MAX_IMAGE_BYTES {
        return Err(refused(&folder.join(source), "over the picture size cap"));
    }
    let mut path = folder.to_path_buf();
    let segments: Vec<&str> = source.split('/').collect();
    for (n, segment) in segments.iter().enumerate() {
        path.push(segment);
        let last = n + 1 == segments.len();
        match std::fs::symlink_metadata(&path) {
            Ok(meta) if meta.is_symlink() => return Err(refused(&path, "a link is in the way")),
            Ok(meta) if last && meta.is_file() => return Ok(()),
            Ok(meta) if !last && meta.is_dir() => {}
            Ok(_) => return Err(refused(&path, "something else is in the way")),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                if !last {
                    std::fs::create_dir(&path).map_err(io_at(&path))?;
                }
            }
            Err(e) => return Err(io_at(&path)(e)),
        }
    }
    let tmp = temp_beside(&path);
    std::fs::write(&tmp, bytes).map_err(io_at(&tmp))?;
    std::fs::rename(&tmp, &path).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        io_at(&path)(e)
    })
}

/// The name a file is written under before it is renamed into place at `path`: its whole
/// name plus a suffix, so a pick's texture and material never share one.
fn temp_beside(path: &Path) -> PathBuf {
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(".tf2tools-tmp");
    path.with_file_name(name)
}

/// Removes every picked-picture copy under `<build>/.tf2tools/images/` that neither an applied
/// change nor a snapshot in the build's history names, then the folders left empty. Keeps
/// everything when the history cannot be read whole; links are left in place.
pub(super) fn prune_pictures(build: &Path, applied: &[Change]) -> Result<(), MakeHudError> {
    let Some(mut needed) = super::hud_history::snapshot_sources(build) else {
        return Ok(());
    };
    needed.extend(applied.iter().filter_map(|c| match c.target() {
        ChangeTarget::Asset(asset) => asset_source(asset),
        ChangeTarget::Set(_)
        | ChangeTarget::Remove(_)
        | ChangeTarget::AddBlock(_)
        | ChangeTarget::Option(_)
        | ChangeTarget::Credit(_)
        | ChangeTarget::Animation(_)
        | ChangeTarget::AnimationAppend(_) => None,
    }));
    let images = build.join(IMAGES_DIR);
    if !std::fs::symlink_metadata(&images).is_ok_and(|m| m.is_dir()) {
        return Ok(());
    }
    // Contents come before their folder, so a folder emptied here is seen empty afterwards.
    for entry in WalkDir::new(&images).min_depth(1).contents_first(true) {
        let entry = entry.map_err(|e| walk_failure(&images, e))?;
        let path = entry.path();
        if entry.file_type().is_dir() {
            // Fails, as wanted, while anything is left inside.
            let _ = std::fs::remove_dir(path);
            continue;
        }
        if !entry.file_type().is_file() {
            continue;
        }
        let Some(relative) = path.strip_prefix(build).ok().and_then(Path::to_str) else {
            continue;
        };
        let source = relative.replace(std::path::MAIN_SEPARATOR, "/");
        if !needed.contains(&source) {
            std::fs::remove_file(path).map_err(io_at(path))?;
        }
    }
    Ok(())
}

/// Writes the made HUD's folder at `build`: `info.vdf`, each touched file's edited bytes, each
/// applied asset's bytes at its path and kept copy, and `maker.json` with the SHA-256 of each
/// touched file's pristine bytes. Returns how many animation changes were left out.
#[allow(clippy::too_many_arguments)]
pub(super) fn write_build(
    build: &Path,
    folder: &str,
    pristine: &HudModel,
    edited: &Edited,
    assets: &HudAssets,
    files: &HudFiles,
    platform: Platform,
    base: &MakerBase,
) -> Result<usize, MakeHudError> {
    // A copy keeps its base's own `info.vdf`; only a stock-based HUD gets one written.
    if matches!(base, MakerBase::Default) {
        let info = format!("\"{folder}\"\n{{\n\t\"ui_version\"\t\"3\"\n}}\n");
        write_file(&build.join("info.vdf"), info.as_bytes())?;
    }
    let mut sources = BTreeMap::new();
    // A source records base bytes: under a switch, the copy's own file beneath the layer; a
    // path only a switch adds records its own bytes, as it has no base.
    let (switched, _) = files.switched_paths();
    let base_hash = |rel: &str, bytes: &[u8]| -> Result<String, MakeHudError> {
        if switched.iter().any(|s| s == rel)
            && files.in_copy(rel)
            && let Some(file) = files.read_pristine(rel)?
        {
            return Ok(sha256_hex(file.bytes()));
        }
        Ok(sha256_hex(bytes))
    };
    // Switched files are written whole first, so a model edit to the same path wins.
    for rel in &switched {
        let Some(file) = files.read(rel)? else {
            continue;
        };
        write_file(&build.join(rel), file.bytes())?;
        sources.insert(rel.clone(), base_hash(rel, file.bytes())?);
    }
    for path in edited.touched() {
        // Only path-rule paths are joined, so nothing lands outside the build.
        let (Some(rel), Some(before), Some(after)) = (
            check_hud_path(path),
            pristine.document(path),
            edited.model().document(path),
        ) else {
            continue;
        };
        write_file(&build.join(&rel), after.as_bytes())?;
        let hash = base_hash(&rel, before.as_bytes())?;
        sources.insert(rel, hash);
    }
    // Save dropped every group whose asset has no bytes, so each one left has them. Every
    // applied asset keeps its copy, so an overridden pick survives; only winners reach the game.
    for change in edited.applied() {
        let ChangeTarget::Asset(asset) = change.target() else {
            continue;
        };
        let Some(source) = asset_source(asset) else {
            continue;
        };
        if let Some(bytes) = assets.get(&source) {
            write_file(&build.join(&source), bytes)?;
        }
    }
    for (path, source) in edited.model().assets() {
        let checked = asset_source(&AssetFile::new(path, source));
        let Some(bytes) = checked.and_then(|source| assets.get(&source)) else {
            continue;
        };
        write_file(&build.join(path), bytes)?;
    }
    // Animation changes that cannot be written safely stay out of maker.json too.
    let is_animation = |c: &Change| {
        matches!(
            c.target(),
            ChangeTarget::Animation(_) | ChangeTarget::AnimationAppend(_)
        )
    };
    let mut listed = edited.applied().to_vec();
    let mut refused = 0;
    match animation_files(files, edited, platform)? {
        AnimationSave::Write(anims) => {
            write_file(&build.join(OWN_SCRIPT), anims.script.as_bytes())?;
            write_file(&build.join(&anims.manifest_path), &anims.manifest)?;
            let hash = base_hash(&anims.manifest_path, &anims.pristine)?;
            sources.insert(anims.manifest_path, hash);
        }
        AnimationSave::Refused => {
            refused = listed.iter().filter(|c| is_animation(c)).count();
            listed.retain(|c| !is_animation(c));
        }
        AnimationSave::Nothing => {}
    }
    if let Some(credits) = credits_text(edited.applied(), base) {
        write_file(&build.join(CREDITS_FILE), credits.as_bytes())?;
    }
    let maker = MakerFile::new(sources, listed).with_base(base.clone());
    write_file(&build.join(METADATA_DIR).join(MAKER_FILE), &maker.to_json())?;
    // A copy always lists its settings files, even none, so an older list never outlives
    // the options that wrote it.
    if matches!(base, MakerBase::Copy(_)) {
        let list = super::hud_cfg::list_json(&files.switched_cfg());
        write_file(
            &build.join(METADATA_DIR).join(super::hud_cfg::CFG_FILE),
            &list,
        )?;
    }
    Ok(refused)
}

/// Moves a copy's freshly built files from `build` into the copy at `live` without touching
/// any other file. Renames run in an order that keeps pristine bytes reachable at every
/// step: (1) originals of base files written for the first time, (2) the edited files and
/// kept copies, (3) base files no longer edited restored from their original and files the
/// Maker added earlier removed, (4) `maker.json`, (5) the originals no longer needed.
/// `stage` (under the library tmp dir) holds bytes before they are renamed in.
pub(super) fn write_copy_in_place(
    live: &Path,
    build: &Path,
    stage: &Path,
    files: &HudFiles,
    previous: &MakerFile,
) -> Result<(), MakeHudError> {
    let original_dir = live.join(METADATA_DIR).join(ORIGINAL_DIR);
    let maker_rel = format!("{METADATA_DIR}/{MAKER_FILE}");
    let built = regular_files(build)?;
    // A base file keeps the copy's own spelling; anything else is written as built.
    let spelling = |rel: &str| -> Result<String, MakeHudError> {
        if files.in_copy(rel)
            && let Some(file) = files.read_pristine(rel)?
        {
            return Ok(file.name().to_owned());
        }
        Ok(rel.to_owned())
    };
    let mut written: Vec<(String, String)> = Vec::new();
    for rel in built.iter().filter(|rel| **rel != maker_rel) {
        let target = if rel.starts_with(&format!("{METADATA_DIR}/")) {
            rel.clone()
        } else {
            spelling(rel)?
        };
        written.push((rel.clone(), target));
    }
    let edited: BTreeSet<String> = (written.iter())
        .filter(|(rel, _)| !rel.starts_with(&format!("{METADATA_DIR}/")))
        .map(|(rel, _)| rel.to_ascii_lowercase())
        .collect();
    // Base files a switch moved away, as the copy spells them.
    let mut taken: Vec<String> = Vec::new();
    for rel in files.switched_paths().1 {
        if files.in_copy(&rel) {
            taken.push(spelling(&rel)?);
        }
    }
    let taken_keys: BTreeSet<String> = taken.iter().map(|t| t.to_ascii_lowercase()).collect();
    let originals = regular_files(&original_dir)?;
    let has_original = |name: &str| {
        let key = name.to_ascii_lowercase();
        originals.iter().any(|o| o.to_ascii_lowercase() == key)
    };

    // 1. Originals: a base file written for the first time keeps its untouched bytes.
    let mut step = 0usize;
    for (rel, target) in &written {
        if rel.starts_with(&format!("{METADATA_DIR}/")) || !files.in_copy(rel) {
            continue;
        }
        if has_original(target) {
            continue;
        }
        let Some(file) = files.read_pristine(rel)? else {
            continue;
        };
        step += 1;
        let staged = stage.join(format!("{step}"));
        write_file(&staged, file.bytes())?;
        place(&staged, &original_dir.join(target))?;
    }
    // A base file a switch moves away keeps its bytes the same way before it goes.
    for target in taken.iter().filter(|t| !has_original(t)) {
        let Some(file) = files.read_pristine(target)? else {
            continue;
        };
        step += 1;
        let staged = stage.join(format!("{step}"));
        write_file(&staged, file.bytes())?;
        place(&staged, &original_dir.join(target))?;
    }
    // Files about to be added over the game's are listed first, so a crash after step 2
    // still reads them as the Maker's, never as part of the base.
    let earlier_adding = read_adding(live);
    let adding: Vec<&str> = (written.iter())
        .filter(|(rel, _)| !rel.starts_with(&format!("{METADATA_DIR}/")) && !files.in_copy(rel))
        .map(|(_, target)| target.as_str())
        .collect();
    let adding_path = live.join(METADATA_DIR).join(ADDING_FILE);
    if !adding.is_empty() {
        step += 1;
        let staged = stage.join(format!("{step}"));
        let list: String = adding.iter().map(|p| format!("{p}\n")).collect();
        write_file(&staged, list.as_bytes())?;
        place(&staged, &adding_path)?;
    }
    // 2. The edited files and kept copies.
    for (rel, target) in &written {
        place(&build.join(rel), &live.join(target))?;
    }
    for target in &taken {
        super::remove_file_if_present(&live.join(target))?;
    }
    // 3. Restore base files no longer edited, and drop files the Maker added before that
    // are no longer written; old `maker.json` still hides those, so either state reopens.
    let stale: Vec<&String> = (originals.iter())
        .filter(|o| {
            let key = o.to_ascii_lowercase();
            !edited.contains(&key) && !taken_keys.contains(&key)
        })
        .collect();
    for name in &stale {
        step += 1;
        let staged = stage.join(format!("{step}"));
        let bytes =
            std::fs::read(original_dir.join(name)).map_err(io_at(&original_dir.join(name)))?;
        write_file(&staged, &bytes)?;
        place(&staged, &live.join(name))?;
    }
    for added in maker_added(previous, &originals)
        .into_iter()
        .chain(earlier_adding)
    {
        if !edited.contains(&added.to_ascii_lowercase()) {
            super::remove_file_if_present(&live.join(&added))?;
        }
    }
    // 4. The record of the changes.
    place(&build.join(&maker_rel), &live.join(&maker_rel))?;
    super::remove_file_if_present(&adding_path)?;
    // 5. Originals of files that are back to their base bytes.
    for name in stale {
        super::remove_file_if_present(&original_dir.join(name))?;
    }
    Ok(())
}

/// The paths an interrupted copy save was adding, from `.tf2tools/adding.txt`; a missing,
/// linked, oversized or unreadable list is empty, and paths failing the path rule are left out.
pub(super) fn read_adding(folder: &Path) -> Vec<String> {
    let path = folder.join(METADATA_DIR).join(ADDING_FILE);
    let meta = std::fs::symlink_metadata(folder.join(METADATA_DIR));
    if !meta.is_ok_and(|m| m.is_dir()) {
        return Vec::new();
    }
    let Ok(Some(bytes)) = crate::fonts::read_regular_capped(&path, 1024 * 1024) else {
        return Vec::new();
    };
    String::from_utf8_lossy(&bytes)
        .lines()
        .filter(|line| check_hud_path(line).is_some())
        .map(str::to_owned)
        .collect()
}

/// The files a previous save added over game files: its sources with no kept original, its
/// asset paths, our animation script and the credits file.
fn maker_added(previous: &MakerFile, originals: &[String]) -> Vec<String> {
    let kept: BTreeSet<String> = originals.iter().map(|o| o.to_ascii_lowercase()).collect();
    let mut added: Vec<String> = (previous.sources().keys())
        .filter(|path| !kept.contains(&path.to_ascii_lowercase()))
        .cloned()
        .collect();
    added.extend(previous.changes().iter().filter_map(|c| match c.target() {
        ChangeTarget::Asset(asset) => check_hud_path(asset.path()),
        ChangeTarget::Set(_)
        | ChangeTarget::Remove(_)
        | ChangeTarget::AddBlock(_)
        | ChangeTarget::Option(_)
        | ChangeTarget::Credit(_)
        | ChangeTarget::Animation(_)
        | ChangeTarget::AnimationAppend(_) => None,
    }));
    added.push(OWN_SCRIPT.to_owned());
    added.push(CREDITS_FILE.to_owned());
    added.retain(|p| check_hud_path(p).is_some());
    added
}

/// Every regular file under `root` as a `/`-separated relative path; links are skipped and
/// a missing root has none.
fn regular_files(root: &Path) -> Result<Vec<String>, MakeHudError> {
    if !std::fs::symlink_metadata(root).is_ok_and(|m| m.is_dir()) {
        return Ok(Vec::new());
    }
    let mut found = Vec::new();
    for entry in WalkDir::new(root).min_depth(1).sort_by_file_name() {
        let entry = entry.map_err(|e| walk_failure(root, e))?;
        if !entry.file_type().is_file() {
            continue;
        }
        let Some(rel) = entry.path().strip_prefix(root).ok().and_then(Path::to_str) else {
            continue;
        };
        found.push(rel.replace(std::path::MAIN_SEPARATOR, "/"));
    }
    Ok(found)
}

/// Renames `from` over `to`, creating `to`'s folders first.
fn place(from: &Path, to: &Path) -> Result<(), MakeHudError> {
    if let Some(parent) = to.parent() {
        create_dir_all(parent)?;
    }
    rename(from, to)
}

/// `changes` without its animation changes when no layer has a manifest, and how many
/// were left out.
fn without_animations_unless_listed(
    files: &HudFiles,
    changes: &[Change],
) -> Result<(Vec<Change>, usize), MakeHudError> {
    if files.read(ANIM_MANIFEST)?.is_some() {
        return Ok((changes.to_vec(), 0));
    }
    let kept: Vec<Change> = (changes.iter())
        .filter(|c| {
            !matches!(
                c.target(),
                ChangeTarget::Animation(_) | ChangeTarget::AnimationAppend(_)
            )
        })
        .cloned()
        .collect();
    let left_out = changes.len() - kept.len();
    Ok((kept, left_out))
}

/// The credits file: for a copy, a paragraph naming its base first, then a note for the
/// applied `Credit` changes; `None` when there is neither.
fn credits_text(applied: &[Change], base: &MakerBase) -> Option<String> {
    let lines: Vec<String> = (applied.iter())
        .filter_map(|c| match c.target() {
            ChangeTarget::Credit(n) => Some(n),
            ChangeTarget::Set(_)
            | ChangeTarget::Remove(_)
            | ChangeTarget::AddBlock(_)
            | ChangeTarget::Asset(_)
            | ChangeTarget::Option(_)
            | ChangeTarget::Animation(_)
            | ChangeTarget::AnimationAppend(_) => None,
        })
        .map(|n| {
            let by = if n.author().is_empty() {
                String::new()
            } else {
                format!(" by {}", n.author())
            };
            format!("\"{}\" from {}{} ({})\n", n.part(), n.hud(), by, n.origin())
        })
        .collect();
    let borrowed = (!lines.is_empty()).then(|| {
        format!(
            "These parts were copied from other HUDs, so their authors' terms apply.\n\n{}",
            lines.join("\n")
        )
    });
    let MakerBase::Copy(copy) = base else {
        return borrowed;
    };
    let by = if copy.author().is_empty() {
        String::new()
    } else {
        format!(" by {}", copy.author())
    };
    let intro = format!(
        "This HUD is a copy of \"{}\"{by} ({}), changed with TF2Tools. Its author's terms \
         apply; check the HUD's own page before you share it.\n",
        copy.name(),
        copy.origin()
    );
    Some(match borrowed {
        Some(borrowed) => format!("{intro}\n{borrowed}"),
        None => intro,
    })
}

fn write_file(path: &Path, bytes: &[u8]) -> Result<(), MakeHudError> {
    if let Some(parent) = path.parent() {
        create_dir_all(parent)?;
    }
    std::fs::write(path, bytes).map_err(io_at(path))?;
    Ok(())
}

fn rename(from: &Path, to: &Path) -> Result<(), MakeHudError> {
    std::fs::rename(from, to).map_err(io_at(to))?;
    Ok(())
}

fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

pub(super) fn read_row(conn: &rusqlite::Connection, id: &str) -> rusqlite::Result<HudEntry> {
    conn.query_row(
        &format!("SELECT {HUD_COLUMNS} FROM library_huds WHERE id = ?1"),
        [id],
        hud_from_row,
    )
}

/// True when a HUD row other than `except` has `name` in any case. Folding runs in Rust,
/// since SQLite's `lower()` folds ASCII only.
pub(super) fn name_taken(
    conn: &rusqlite::Connection,
    name: &str,
    except: Option<&HudId>,
) -> rusqlite::Result<bool> {
    let wanted = name.to_lowercase();
    let mut stmt = conn.prepare("SELECT id, name FROM library_huds")?;
    let mut rows = stmt.query([])?;
    while let Some(row) = rows.next()? {
        let id: String = row.get(0)?;
        let other: String = row.get(1)?;
        if except.is_none_or(|e| e.as_str() != id) && other.to_lowercase() == wanted {
            return Ok(true);
        }
    }
    Ok(false)
}

fn name_taken_error(name: &str) -> MakeHudError {
    MakeHudError::NameTaken {
        name: name.to_owned(),
    }
}

fn load_db(e: rusqlite::Error) -> HudLoadError {
    HudLoadError::Database {
        message: e.to_string(),
    }
}

/// A `maker.json` read failure during a save: I/O stays I/O, a file this build cannot read
/// is reported through the database message, as `made_folder` already vetted it.
fn make_load(e: HudLoadError) -> MakeHudError {
    match e {
        HudLoadError::Io { path, source } => MakeHudError::Io { path, source },
        HudLoadError::Files(files) => MakeHudError::Files(files),
        other @ HudLoadError::Damaged { .. }
        | other @ HudLoadError::Newer
        | other @ HudLoadError::NotFound { .. }
        | other @ HudLoadError::NotMade { .. }
        | other @ HudLoadError::Database { .. } => MakeHudError::Database {
            message: other.to_string(),
        },
    }
}

fn make_db(e: rusqlite::Error) -> MakeHudError {
    MakeHudError::Database {
        message: e.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ID: &str = "0123456789abcdef";

    #[test]
    fn a_pick_s_texture_and_material_get_distinct_temp_names() {
        let dir = Path::new("x");
        let vtf = temp_beside(&dir.join(format!("{ID}.vtf")));
        let vmt = temp_beside(&dir.join(format!("{ID}.vmt")));
        assert_ne!(vtf, vmt);
    }

    #[cfg(unix)]
    #[test]
    fn a_link_on_the_way_blocks_a_kept_copy_write() {
        let tmp = tempfile::tempdir().unwrap();
        let (folder, outside) = (tmp.path().join("hud"), tmp.path().join("outside"));
        std::fs::create_dir_all(folder.join(".tf2tools")).unwrap();
        std::fs::create_dir_all(&outside).unwrap();
        std::os::unix::fs::symlink(&outside, folder.join(IMAGES_DIR)).unwrap();
        let source = format!("{IMAGES_DIR}/{ID}/materials/a.vtf");
        assert!(write_kept_copy(&folder, &source, b"bytes").is_err());
        assert!(
            std::fs::read_dir(&outside).unwrap().next().is_none(),
            "nothing written outside"
        );
    }

    #[cfg(unix)]
    #[test]
    fn prune_never_follows_or_deletes_through_a_link() {
        let tmp = tempfile::tempdir().unwrap();
        let (build, outside) = (tmp.path().join("build"), tmp.path().join("outside"));
        let images = build.join(IMAGES_DIR);
        std::fs::create_dir_all(&images).unwrap();
        std::fs::create_dir_all(&outside).unwrap();
        std::fs::write(outside.join("a.vtf"), b"keep me").unwrap();
        std::os::unix::fs::symlink(&outside, images.join(ID)).unwrap();
        std::os::unix::fs::symlink(outside.join("a.vtf"), images.join("file-link")).unwrap();
        prune_pictures(&build, &[]).unwrap();
        assert_eq!(std::fs::read(outside.join("a.vtf")).unwrap(), b"keep me");
        assert!(
            std::fs::symlink_metadata(images.join(ID))
                .unwrap()
                .is_symlink()
        );
        assert!(
            std::fs::symlink_metadata(images.join("file-link"))
                .unwrap()
                .is_symlink()
        );
    }

    #[test]
    fn a_roll_back_that_cannot_move_the_new_folder_leaves_both() {
        let tmp = tempfile::tempdir().unwrap();
        let live = tmp.path().join("hud");
        let old = old_folder(&live);
        std::fs::create_dir_all(&live).unwrap();
        std::fs::write(live.join("new"), b"").unwrap();
        std::fs::create_dir_all(&old).unwrap();
        std::fs::write(old.join("previous"), b"").unwrap();
        // A discard path whose parent is missing makes the move fail.
        roll_back(&live, &old, &tmp.path().join("missing/failed"));
        assert!(live.join("new").is_file() && old.join("previous").is_file());
    }
}
