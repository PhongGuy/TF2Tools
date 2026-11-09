//! The local library: items imported under `<data>/library/<kind>/`, their rows in the
//! v5 database, and installing them into `tf/custom`. Files are written first and the
//! database last, so a failed copy leaves the library state unchanged.

mod addons;
mod borrow;
mod crosshairs;
mod hud_cfg;
mod hud_copy;
mod hud_folder;
mod hud_history;
mod hud_presets;
mod hud_print;
mod hud_thumbnail;
mod huds;
mod live_link;
mod made_huds;
mod mastercomfig;
mod sounds;
mod tree_copy;

use std::fmt;
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};

use rusqlite::OptionalExtension;
use walkdir::WalkDir;

use crate::archive::VPK_MAGIC;
use crate::db::Database;
use crate::error::{
    ApplyProfileError, HistoryError, HudLoadError, ImportError, InstallError,
    InstallMastercomfigError, KeepHudError, LibraryError, MakeHudError, PaintError, PresetError,
    ProfileError, ReinstallError, RemoveMastercomfigError,
};
use crate::fonts::Platform;
use crate::hud_maker::{HudFiles, HudOptions, HudSchemas, MakerBase};
use crate::paths::DataDir;
use crate::tf2::Tf2Install;

pub use addons::{AddonEntry, AddonName};
pub(crate) use addons::{display_info, is_override};
pub use crosshairs::{
    ApplyReport, Assignment, CrosshairEntry, CrosshairFormat, CrosshairRef, InstalledCrosshairs,
    PaintTarget, PaintedSource, SkipReason, SkippedAssignment, crosshair_scripts_need_repair,
};
pub use hud_copy::{CopyUpdate, CopyUpdated, DownloadUpdate, TreeHash};
pub(crate) use hud_folder::is_safe_folder;
pub use hud_history::{HudSnapshot, MAX_SNAPSHOTS, SnapshotEntry};
pub use hud_presets::{PresetEntry, read_recipe_file};
pub use huds::{ExternalHud, HudEntry, HudInstall, HudStatus};
pub use live_link::{Recovery, Reinstall};
pub use made_huds::{HudAssets, MadeHud, MakeTarget, SavedHud, pictures_fit};
pub use mastercomfig::{MastercomfigStatus, Preset};
pub(crate) use mastercomfig::{record_install, write_setup_hook};
pub use profiles::{
    ApplyProfileReport, ApplyStep, DraftCrosshair, DraftSlot, MissingItem, MissingKind, Profile,
    ProfileCrosshair, ProfileCrosshairRef, ProfileDraft, ProfileId, ProfileItem, ProfileUpdate,
};
pub use sounds::{
    InstalledSound, InstalledSounds, SoundConflict, SoundEntry, SoundFormat, SoundKind,
};

/// v5's per-item metadata folder; it belongs to the app and is never installed.
const METADATA_DIR: &str = ".tf2tools";

/// Handle to the local library. Cheap to clone; clones share the database connections.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct Library {
    db: Database,
    dir: DataDir,
    /// Free bytes on the disk holding a path; replaced in tests to simulate a full disk.
    free_space: fn(&Path) -> io::Result<u64>,
    /// The HUD option schemas a copy's options are read with, matched by its base's repo.
    hud_schemas: Arc<HudSchemas>,
    /// Held for the whole of a made HUD's save and a copy's update, so a save that checks the
    /// copy's base never interleaves with an update rewriting it.
    writes: Arc<Mutex<()>>,
}

impl Library {
    /// A library over an open database and the data directory it lives in. Touches no files.
    pub fn new(db: Database, dir: DataDir) -> Self {
        Self {
            db,
            dir,
            free_space: crate::disk::free_bytes,
            hud_schemas: Arc::new(HudSchemas::bundled()),
            writes: Arc::new(Mutex::new(())),
        }
    }

    /// Takes the library write lock; a writer that panicked leaves nothing it guards half done
    /// that the next writer relies on, so a poisoned lock is taken as is.
    pub(crate) fn lock_writes(&self) -> MutexGuard<'_, ()> {
        self.writes.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Replaces the schemas copies are read with; the bundled set is the default.
    pub fn set_hud_schemas(&mut self, schemas: HudSchemas) {
        self.hud_schemas = Arc::new(schemas);
    }

    /// The options of a HUD made over `base`: a copy's own switches with the schema for its
    /// repo, if one matches; none for a HUD made over the default HUD.
    pub(crate) fn hud_options(
        &self,
        files: &HudFiles,
        base: Option<&MakerBase>,
        platform: Platform,
    ) -> HudOptions {
        let Some(MakerBase::Copy(copy)) = base else {
            return HudOptions::default();
        };
        HudOptions::read(files, self.hud_schemas.for_repo(copy.repo()), platform)
    }
}

/// One library item of any kind, as a store download returns it.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum LibraryEntry {
    /// A HUD.
    Hud(HudEntry),
    /// A hitsound or killsound.
    Sound(SoundEntry),
    /// A crosshair.
    Crosshair(CrosshairEntry),
    /// A mastercomfig addon.
    Addon(AddonEntry),
}

/// Where a library item came from, as recorded in v5's `source` and store id columns.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Provenance {
    /// Imported from a file on disk.
    Local,
    /// Downloaded from the curated HUD store (hud-db); HUDs only.
    Curated {
        /// The store's HUD id.
        hud_id: String,
    },
    /// Downloaded from GameBanana.
    GameBanana {
        /// The GameBanana item id.
        gb_id: GbId,
        /// The submitter's name; empty for kinds whose rows keep no author.
        author: String,
    },
}

impl Provenance {
    /// Reads provenance back from the store id columns, whatever the `source` text says:
    /// a GameBanana id wins, then a curated id, else the item is local.
    fn from_columns(store_hud_id: Option<String>, store_gb_id: Option<i64>, author: &str) -> Self {
        let gb_id = store_gb_id.and_then(|id| u64::try_from(id).ok());
        match (gb_id, store_hud_id) {
            (Some(gb_id), _) => Self::GameBanana {
                gb_id: GbId::new(gb_id),
                author: author.to_owned(),
            },
            (None, Some(hud_id)) => Self::Curated { hud_id },
            (None, None) => Self::Local,
        }
    }
}

macro_rules! text_id {
    ($(#[$doc:meta])* $name:ident) => {
        $(#[$doc])*
        #[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
        #[non_exhaustive]
        pub struct $name(String);

        impl $name {
            /// Wraps an id as stored in the database.
            pub fn new(id: impl Into<String>) -> Self {
                Self(id.into())
            }

            /// The id text.
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(&self.0)
            }
        }
    };
}

// Declared below `text_id!`, which a module can only use after the macro's definition.
mod profiles;

text_id!(
    /// Id of a library HUD, v5's `library_huds.id`.
    HudId
);
text_id!(
    /// Id of a library sound, v5's `library_sounds.id`.
    SoundId
);
text_id!(
    /// Id of a library crosshair, v5's `library_crosshairs.id`.
    CrosshairId
);

/// A GameBanana item id.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[non_exhaustive]
pub struct GbId(u64);

impl GbId {
    /// Wraps a GameBanana id.
    pub fn new(id: u64) -> Self {
        Self(id)
    }

    /// The numeric id.
    pub fn get(self) -> u64 {
        self.0
    }
}

impl fmt::Display for GbId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// True when `name` is a single path component that cannot climb out of its parent.
/// Folder names read from a v5 database pass this before they are joined into a path.
pub(crate) fn is_plain_name(name: &str) -> bool {
    !name.is_empty() && name != "." && name != ".." && !name.contains(['/', '\\', '\0'])
}

/// v5's `<slug>-<first 8 chars of id>` folder name for sounds and crosshairs. The slug is
/// the lowercased name with every char but alphanumerics, `-` and `_` turned into `-`,
/// so no display name can form a path.
pub(crate) fn folder_name(name: &str, id: &str) -> String {
    let slug: String = name
        .to_lowercase()
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '-'
            }
        })
        .collect();
    let short = id.get(..8).unwrap_or(id);
    format!("{slug}-{short}")
}

/// A GameBanana id as v5's signed `store_gb_id` column.
pub(crate) fn gb_id_column(gb_id: GbId) -> Result<i64, ImportError> {
    i64::try_from(gb_id.get()).map_err(|e| ImportError::Database {
        message: format!("GameBanana id {gb_id} does not fit the database: {e}"),
    })
}

/// A fresh v5-style item id.
pub(crate) fn new_id() -> String {
    uuid::Uuid::new_v4().to_string()
}

/// The current time in the database's fixed-width RFC 3339 format.
pub(crate) fn now() -> String {
    crate::timestamp::now()
}

/// The number a clashing import takes in "<name> (<n>)": the first from 2 that `free`
/// accepts. Shared by HUDs and crosshairs so both number the same way.
pub(super) fn first_free_number(free: impl Fn(u32) -> bool) -> u32 {
    let mut number = 2u32;
    while !free(number) {
        number = number.saturating_add(1);
    }
    number
}

/// An I/O failure and the path it happened on, mapped into each operation's error enum.
/// Crate-private, so the `From` impls below cannot be reached from outside the crate.
#[derive(Debug)]
pub(crate) struct IoFailure {
    path: PathBuf,
    source: io::Error,
}

/// Tags an I/O error with the path it happened on.
pub(crate) fn io_at(path: &Path) -> impl FnOnce(io::Error) -> IoFailure + use<> {
    let path = path.to_path_buf();
    move |source| IoFailure { path, source }
}

impl From<IoFailure> for PresetError {
    fn from(IoFailure { path, source }: IoFailure) -> Self {
        Self::Io { path, source }
    }
}

impl From<IoFailure> for HistoryError {
    fn from(IoFailure { path, source }: IoFailure) -> Self {
        Self::Io { path, source }
    }
}

impl From<IoFailure> for ImportError {
    fn from(IoFailure { path, source }: IoFailure) -> Self {
        Self::Io { path, source }
    }
}

impl From<IoFailure> for InstallError {
    fn from(IoFailure { path, source }: IoFailure) -> Self {
        Self::Io { path, source }
    }
}

impl From<IoFailure> for KeepHudError {
    fn from(IoFailure { path, source }: IoFailure) -> Self {
        Self::Io { path, source }
    }
}

impl From<IoFailure> for PaintError {
    fn from(IoFailure { path, source }: IoFailure) -> Self {
        Self::Io { path, source }
    }
}

impl From<IoFailure> for MakeHudError {
    fn from(IoFailure { path, source }: IoFailure) -> Self {
        Self::Io { path, source }
    }
}

impl From<IoFailure> for HudLoadError {
    fn from(IoFailure { path, source }: IoFailure) -> Self {
        Self::Io { path, source }
    }
}

impl From<IoFailure> for LibraryError {
    fn from(IoFailure { path, source }: IoFailure) -> Self {
        Self::Io { path, source }
    }
}

impl From<IoFailure> for RemoveMastercomfigError {
    fn from(IoFailure { path, source }: IoFailure) -> Self {
        Self::Io { path, source }
    }
}

impl From<IoFailure> for InstallMastercomfigError {
    fn from(IoFailure { path, source }: IoFailure) -> Self {
        Self::Io { path, source }
    }
}

/// The game's `tf` folder has moved away; mapped into each writer's `GameMissing`.
pub(crate) struct GameMissing {
    path: PathBuf,
}

/// `tf/custom` of `tf2` for a writer into the game. Refuses when `tf` itself is gone, so
/// no writer recreates a moved game's tree; a missing `custom` is still the writer's to make.
pub(crate) fn game_custom_dir(tf2: &Tf2Install) -> Result<PathBuf, GameMissing> {
    if tf2.is_present() {
        Ok(tf2.custom_dir())
    } else {
        Err(GameMissing { path: tf2.tf_dir() })
    }
}

impl From<GameMissing> for InstallError {
    fn from(GameMissing { path }: GameMissing) -> Self {
        Self::GameMissing { path }
    }
}

impl From<GameMissing> for ReinstallError {
    fn from(GameMissing { path }: GameMissing) -> Self {
        Self::GameMissing { path }
    }
}

impl From<GameMissing> for ApplyProfileError {
    fn from(GameMissing { path }: GameMissing) -> Self {
        Self::GameMissing { path }
    }
}

impl From<GameMissing> for RemoveMastercomfigError {
    fn from(GameMissing { path }: GameMissing) -> Self {
        Self::GameMissing { path }
    }
}

impl From<GameMissing> for InstallMastercomfigError {
    fn from(GameMissing { path }: GameMissing) -> Self {
        Self::GameMissing { path }
    }
}

pub(crate) fn import_db(e: rusqlite::Error) -> ImportError {
    ImportError::Database {
        message: e.to_string(),
    }
}

pub(crate) fn install_db(e: rusqlite::Error) -> InstallError {
    InstallError::Database {
        message: e.to_string(),
    }
}

/// Writes one v5 `settings` key inside the caller's transaction.
pub(crate) fn save_setting(
    conn: &rusqlite::Connection,
    key: &str,
    value: &str,
) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT OR REPLACE INTO settings (key, value) VALUES (?1, ?2)",
        [key, value],
    )?;
    Ok(())
}

/// Reads one v5 `settings` key; an unset key is `None`.
pub(crate) fn read_setting(
    conn: &rusqlite::Connection,
    key: &str,
) -> rusqlite::Result<Option<String>> {
    conn.query_row("SELECT value FROM settings WHERE key = ?1", [key], |r| {
        r.get(0)
    })
    .optional()
}

pub(crate) fn paint_db(e: rusqlite::Error) -> PaintError {
    PaintError::Database {
        message: e.to_string(),
    }
}

pub(crate) fn library_db(e: rusqlite::Error) -> LibraryError {
    LibraryError::Database {
        message: e.to_string(),
    }
}

pub(crate) fn profile_db(e: rusqlite::Error) -> ProfileError {
    ProfileError::Database {
        message: e.to_string(),
    }
}

/// Copies the tree at `src` into `dst`, ported from v5's `copy_dir_excluding_metadata`:
/// anything under a `.tf2tools` folder and every symlink is left behind.
pub(crate) fn copy_dir_excluding_metadata(src: &Path, dst: &Path) -> Result<(), IoFailure> {
    create_dir_all(dst)?;
    for entry in WalkDir::new(src).min_depth(1) {
        let entry = entry.map_err(|e| walk_failure(src, e))?;
        let Ok(relative) = entry.path().strip_prefix(src) else {
            continue;
        };
        if relative.components().any(|c| c.as_os_str() == METADATA_DIR) {
            continue;
        }
        let target = dst.join(relative);
        let kind = entry.file_type();
        if kind.is_dir() {
            create_dir_all(&target)?;
        } else if kind.is_file() {
            if let Some(parent) = target.parent() {
                create_dir_all(parent)?;
            }
            std::fs::copy(entry.path(), &target).map_err(io_at(&target))?;
        }
    }
    Ok(())
}

pub(crate) fn walk_failure(root: &Path, e: walkdir::Error) -> IoFailure {
    let path = e.path().unwrap_or(root).to_path_buf();
    let source = e
        .into_io_error()
        .unwrap_or_else(|| io::Error::other("filesystem loop"));
    IoFailure { path, source }
}

pub(crate) fn create_dir_all(dir: &Path) -> Result<(), IoFailure> {
    std::fs::create_dir_all(dir).map_err(io_at(dir))
}

/// Removes a directory tree; one that is already gone counts as removed.
pub(crate) fn remove_dir_if_present(dir: &Path) -> Result<(), IoFailure> {
    match std::fs::remove_dir_all(dir) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(io_at(dir)(e)),
    }
}

/// v5 checked only the four magic bytes; a file shorter than that is not a VPK either.
pub(crate) fn check_vpk_magic(path: &Path) -> Result<(), ImportError> {
    let mut head = Vec::with_capacity(VPK_MAGIC.len());
    std::fs::File::open(path)
        .and_then(|f| f.take(VPK_MAGIC.len() as u64).read_to_end(&mut head))
        .map_err(io_at(path))?;
    if head != VPK_MAGIC {
        return Err(ImportError::NotAVpk {
            path: path.to_path_buf(),
        });
    }
    Ok(())
}

/// Removes a file; one that is already gone counts as removed.
pub(crate) fn remove_file_if_present(path: &Path) -> Result<(), IoFailure> {
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(io_at(path)(e)),
    }
}
