//! The local library: items imported under `<data>/library/<kind>/`, their rows in the
//! v5 database, and installing them into `tf/custom`. Files are written first and the
//! database last, so a failed copy leaves the library state unchanged.

mod addons;
mod borrow;
mod convert;
mod crosshairs;
mod gb_addons;
mod gb_huds;
mod gb_maps;
mod hud_cfg;
mod hud_copy;
mod hud_folder;
mod hud_history;
mod hud_lowercase;
mod hud_presets;
mod hud_print;
mod hud_thumbnail;
mod huds;
mod live_link;
mod made_huds;
mod mastercomfig;
mod mastercomfig_modules;
mod movement;
mod packed;
mod sounds;
mod space;
mod tree_copy;

use std::fmt;
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError, RwLock};

use rusqlite::OptionalExtension;
use walkdir::WalkDir;

use crate::archive::VPK_MAGIC;
use crate::db::Database;
use crate::error::{
    ApplyModulesError, ApplyPresetError, ApplyProfileError, ArchiveError, GbAddonFetchError,
    GbAddonInstallError, HistoryError, HudFilesError, HudLoadError, ImportError, InstallError,
    InstallMastercomfigError, KeepHudError, LibraryError, MakeHudError, PaintError, PresetError,
    ProfileError, ReinstallError, RemoveMastercomfigError, RestoreMastercomfigError,
};
use crate::fonts::Platform;
use crate::hud_maker::{HudFiles, HudOptions, HudSchemas, MakerBase};
use crate::paths::DataDir;
use crate::tf2::Tf2Install;

pub use addons::{AddonEntry, AddonName};
pub(crate) use addons::{display_info, is_override};
pub use convert::{ConvertReport, KindConverted};
pub use crosshairs::{
    ApplyReport, Assignment, CrosshairEntry, CrosshairFormat, CrosshairRef, InstalledCrosshairs,
    PaintTarget, PaintedSource, SkipReason, SkippedAssignment, crosshair_scripts_need_repair,
};
pub use gb_addons::{GbAddonCheck, GbAddonEntry};
pub use hud_copy::{CopyUpdate, CopyUpdated, DownloadUpdate, TreeHash};
pub(crate) use hud_folder::is_safe_folder;
pub use hud_history::{HudSnapshot, MAX_SNAPSHOTS, SnapshotEntry};
pub use hud_presets::{PresetEntry, read_recipe_file};
pub use huds::{ExternalHud, HudEntry, HudInstall, HudStatus};
pub use live_link::{Recovery, Reinstall};
pub use made_huds::{HudAssets, MadeHud, MakeTarget, SavedHud, pictures_fit};
pub use mastercomfig::{MastercomfigStatus, Preset};
pub(crate) use mastercomfig::{
    back_up, modules_cfg_path, read_choices, record_install, save_modules, write_modules_cfg,
    write_setup_hook, written_by_us,
};
pub use mastercomfig_modules::{
    Choices, ComfigModule, ModuleCatalog, ModuleCategory, ModuleChoice, ModuleValue, readable_name,
};
pub(crate) use movement::NULL_MOVEMENT;
#[cfg(test)]
pub(crate) use movement::movement_keys;
pub use movement::{MoveDirection, MovementKeys};
pub(crate) use packed::{HUD_ZIP, ItemSource, read_packed_entry};
pub use profiles::{
    ApplyProfileReport, ApplyStep, DraftCrosshair, DraftSlot, MissingItem, MissingKind, Profile,
    ProfileCrosshair, ProfileCrosshairRef, ProfileDraft, ProfileId, ProfileItem, ProfileUpdate,
};
pub use sounds::{
    InstalledSound, InstalledSounds, SoundConflict, SoundEntry, SoundFormat, SoundKind,
};
pub use space::LibrarySpace;

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
    /// Moves a written zip into place; replaced in tests to damage it after the move.
    pack_rename: fn(&Path, &Path) -> io::Result<()>,
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
            pack_rename: |from, to| std::fs::rename(from, to),
            hud_schemas: Arc::new(HudSchemas::bundled()),
            writes: Arc::new(Mutex::new(())),
        }
    }

    /// Takes the library write lock; a writer that panicked leaves nothing it guards half done
    /// that the next writer relies on, so a poisoned lock is taken as is.
    pub(crate) fn lock_writes(&self) -> MutexGuard<'_, ()> {
        assert_copies_free();
        self.writes.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Takes the copies lock for reading. Not reentrant: code under it calls the `_locked` forms.
    pub(crate) fn copies_read(&self) -> CopiesGuard {
        assert_copies_free_to_take();
        let held = CopiesHeld::Read(COPIES.read().unwrap_or_else(PoisonError::into_inner));
        COPIES_HELD.with(|flag| flag.set(true));
        CopiesGuard { _held: held }
    }

    /// Takes the copies lock for writing, waiting out every reader. Not reentrant.
    pub(crate) fn copies_write(&self) -> CopiesGuard {
        assert_copies_free_to_take();
        let held = CopiesHeld::Write(COPIES.write().unwrap_or_else(PoisonError::into_inner));
        COPIES_HELD.with(|flag| flag.set(true));
        CopiesGuard { _held: held }
    }

    /// Runs `f` holding the copies lock for reading, so a test can make a writer wait.
    #[doc(hidden)]
    pub fn with_copies_read<R>(&self, f: impl FnOnce() -> R) -> R {
        let _copies = self.copies_read();
        f()
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

/// True when `name` is a single path component that cannot climb out of its parent; a `:`
/// is refused since Windows reads `C:` as a drive. Folder names read from a v5 database pass
/// this before they are joined into a path.
pub(crate) fn is_plain_name(name: &str) -> bool {
    !name.is_empty() && name != "." && name != ".." && !name.contains(['/', '\\', '\0', ':'])
}

/// v5's `<slug>-<first 8 chars of id>` folder name for sounds and crosshairs.
pub(crate) fn folder_name(name: &str, id: &str) -> String {
    let short = id.get(..8).unwrap_or(id);
    format!("{}-{short}", slug(name))
}

/// The lowercased name with every char but alphanumerics, `-` and `_` turned into `-`,
/// so no display name can form a path.
pub(crate) fn slug(name: &str) -> String {
    name.to_lowercase()
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '-'
            }
        })
        .collect()
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

impl From<IoFailure> for HudFilesError {
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

impl From<IoFailure> for ApplyPresetError {
    fn from(IoFailure { path, source }: IoFailure) -> Self {
        Self::Io { path, source }
    }
}

impl From<IoFailure> for RestoreMastercomfigError {
    fn from(IoFailure { path, source }: IoFailure) -> Self {
        Self::Io { path, source }
    }
}

impl From<IoFailure> for InstallMastercomfigError {
    fn from(IoFailure { path, source }: IoFailure) -> Self {
        Self::Io { path, source }
    }
}

impl From<IoFailure> for ApplyModulesError {
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

impl From<GameMissing> for ApplyPresetError {
    fn from(GameMissing { path }: GameMissing) -> Self {
        Self::GameMissing { path }
    }
}

impl From<GameMissing> for InstallMastercomfigError {
    fn from(GameMissing { path }: GameMissing) -> Self {
        Self::GameMissing { path }
    }
}

impl From<GameMissing> for ApplyModulesError {
    fn from(GameMissing { path }: GameMissing) -> Self {
        Self::GameMissing { path }
    }
}

impl From<GameMissing> for GbAddonInstallError {
    fn from(GameMissing { path }: GameMissing) -> Self {
        Self::GameMissing { path }
    }
}

impl From<IoFailure> for ArchiveError {
    fn from(IoFailure { path, source }: IoFailure) -> Self {
        Self::Io { path, source }
    }
}

impl From<IoFailure> for GbAddonFetchError {
    fn from(IoFailure { path, source }: IoFailure) -> Self {
        Self::Io { path, source }
    }
}

impl From<IoFailure> for GbAddonInstallError {
    fn from(IoFailure { path, source }: IoFailure) -> Self {
        Self::Io { path, source }
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

/// The copies lock, process-wide like the swap lock, so every `Library` over the data dir
/// (the app's and each download's) excludes the others. Read by every entry point that reads a
/// HUD copy, written by any that replaces one; taken after `writes` and the swap lock.
static COPIES: RwLock<()> = RwLock::new(());

thread_local! {
    /// Whether this thread holds the copies lock; debug builds check nesting and order with it.
    static COPIES_HELD: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// A held copies lock, read or write. Dropping it frees the thread for the next guard.
pub(crate) struct CopiesGuard {
    _held: CopiesHeld,
}

/// The std guard behind a `CopiesGuard`; held only for its drop.
enum CopiesHeld {
    Read(
        #[expect(dead_code, reason = "held for its drop")] std::sync::RwLockReadGuard<'static, ()>,
    ),
    Write(
        #[expect(dead_code, reason = "held for its drop")] std::sync::RwLockWriteGuard<'static, ()>,
    ),
}

impl Drop for CopiesGuard {
    fn drop(&mut self) {
        COPIES_HELD.with(|held| held.set(false));
    }
}

/// Checked before blocking, so a nested guard panics instead of deadlocking; the flag itself
/// is set only once the lock is held, so a failed take never leaves the thread marked.
fn assert_copies_free_to_take() {
    debug_assert!(
        !COPIES_HELD.with(std::cell::Cell::get),
        "the library copies lock is not reentrant; call the _locked form"
    );
}

/// `lock_writes` and `hud_swap_lock` come before the copies lock, never after it.
pub(crate) fn assert_copies_free() {
    debug_assert!(
        !COPIES_HELD.with(std::cell::Cell::get),
        "lock order: lock_writes, hud_swap_lock, then copies"
    );
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

#[cfg(all(test, debug_assertions))]
mod copies_lock_tests {
    use super::*;
    use crate::db::Database;

    fn library() -> (tempfile::TempDir, Library) {
        let tmp = tempfile::tempdir().unwrap();
        let dir = DataDir::at(tmp.path().join("data"));
        (tmp, Library::new(Database::open(&dir).unwrap(), dir))
    }

    #[test]
    #[should_panic(expected = "not reentrant")]
    fn a_nested_copies_guard_panics() {
        let (_tmp, lib) = library();
        let _outer = lib.copies_read();
        let _inner = lib.copies_read();
    }

    #[test]
    #[should_panic(expected = "lock order")]
    fn the_swap_lock_under_the_copies_lock_panics() {
        let (_tmp, lib) = library();
        let _copies = lib.copies_write();
        let _swap = live_link::hud_swap_lock();
    }

    #[test]
    #[should_panic(expected = "lock order")]
    fn the_write_lock_under_the_copies_lock_panics() {
        let (_tmp, lib) = library();
        let _copies = lib.copies_read();
        let _writes = lib.lock_writes();
    }

    #[test]
    fn a_write_guard_on_one_library_blocks_a_reader_on_another() {
        let (tmp, writer) = library();
        let dir = DataDir::at(tmp.path().join("data"));
        let reader = Library::new(Database::open(&dir).unwrap(), dir);
        let held = writer.copies_write();
        let (tx, rx) = std::sync::mpsc::channel();
        let waiter = std::thread::spawn(move || {
            let _copies = reader.copies_read();
            tx.send(()).unwrap();
        });
        let waiting = std::time::Duration::from_millis(300);
        assert!(
            rx.recv_timeout(waiting).is_err(),
            "the reader got in under a writer"
        );
        drop(held);
        rx.recv_timeout(std::time::Duration::from_secs(10))
            .expect("the reader runs once the writer is gone");
        waiter.join().unwrap();
    }

    #[test]
    fn a_dropped_guard_frees_the_thread() {
        let (_tmp, lib) = library();
        drop(lib.copies_read());
        drop(lib.copies_write());
        let _swap = live_link::hud_swap_lock();
    }
}

#[cfg(test)]
mod tests {
    use super::is_plain_name;

    #[test]
    fn a_name_with_a_colon_is_not_plain() {
        assert!(is_plain_name("rayshud"));
        assert!(!is_plain_name("C:"));
        assert!(!is_plain_name("a:b"));
    }
}
