//! GameBanana add-ons installed from the Addon Store: their rows in `library_gb_addons`,
//! their copies under `library/gb-addons/<gb_id>` and their files in `tf/custom`.

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::OptionalExtension;
use walkdir::WalkDir;

use super::{
    GbId, IoFailure, Library, create_dir_all, game_custom_dir, io_at, is_plain_name, library_db,
    remove_dir_if_present, remove_file_if_present, slug, walk_failure,
};
use crate::error::{GbAddonInstallError, LibraryError};
use crate::payloads::{Payload, PayloadKind};
use crate::store::GbAddonDownload;
use crate::tf2::Tf2Install;

/// The longest slug an installed name carries, so names stay well inside path limits.
const MAX_SLUG: usize = 40;

const COLUMNS: &str = "gb_id, section, name, author, file_date, latest_file_date, checked_at,
    payloads, files, installed, gone";

/// A recorded GameBanana add-on, as the library lists it.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct GbAddonEntry {
    gb_id: GbId,
    section: String,
    name: String,
    author: String,
    file_date: i64,
    latest_file_date: i64,
    checked_at: i64,
    payloads: Vec<String>,
    files: Vec<String>,
    installed: bool,
    gone: bool,
    present: bool,
}

impl GbAddonEntry {
    /// The GameBanana item id.
    pub fn gb_id(&self) -> GbId {
        self.gb_id
    }

    /// The `AddonSection` key the add-on was installed from.
    pub fn section(&self) -> &str {
        &self.section
    }

    /// The item's name on GameBanana.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The item's submitter.
    pub fn author(&self) -> &str {
        &self.author
    }

    /// The upload date of the installed file, in Unix seconds.
    pub fn file_date(&self) -> i64 {
        self.file_date
    }

    /// The upload date of the newest file the last check found, in Unix seconds.
    pub fn latest_file_date(&self) -> i64 {
        self.latest_file_date
    }

    /// When GameBanana was last asked about the item, in Unix seconds.
    pub fn checked_at(&self) -> i64 {
        self.checked_at
    }

    /// The names of the payloads chosen at install.
    pub fn payloads(&self) -> &[String] {
        &self.payloads
    }

    /// The installed files and folders, relative to `tf/` and `/`-separated.
    pub fn files(&self) -> &[String] {
        &self.files
    }

    /// The last install completed and every recorded file is in the game.
    pub fn is_installed(&self) -> bool {
        self.installed && self.present
    }

    /// GameBanana has a newer file than the installed one.
    pub fn has_update(&self) -> bool {
        self.latest_file_date > self.file_date
    }

    /// The item is no longer on GameBanana.
    pub fn is_gone(&self) -> bool {
        self.gone
    }
}

/// Reads one JSON array column, naming the column when it is malformed.
fn json_list(row: &rusqlite::Row<'_>, column: &str) -> rusqlite::Result<Vec<String>> {
    let text: String = row.get(column)?;
    serde_json::from_str(&text).map_err(|e| malformed(row, column, &e))
}

fn malformed(row: &rusqlite::Row<'_>, column: &str, e: &dyn std::fmt::Display) -> rusqlite::Error {
    let index = row.as_ref().column_index(column).unwrap_or(0);
    rusqlite::Error::FromSqlConversionFailure(
        index,
        rusqlite::types::Type::Text,
        format!("library_gb_addons.{column}: {e}").into(),
    )
}

fn entry_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<GbAddonEntry> {
    let gb_id: i64 = row.get("gb_id")?;
    let gb_id = u64::try_from(gb_id).map_err(|e| malformed(row, "gb_id", &e))?;
    Ok(GbAddonEntry {
        gb_id: GbId::new(gb_id),
        section: row.get("section")?,
        name: row.get("name")?,
        author: row.get("author")?,
        file_date: row.get("file_date")?,
        latest_file_date: row.get("latest_file_date")?,
        checked_at: row.get("checked_at")?,
        payloads: json_list(row, "payloads")?,
        files: json_list(row, "files")?,
        installed: row.get("installed")?,
        gone: row.get("gone")?,
        present: false,
    })
}

impl Library {
    /// Every recorded add-on, by section key then name (case-insensitive). Without `tf2`
    /// none counts as installed.
    pub fn list_gb_addons(
        &self,
        tf2: Option<&Tf2Install>,
    ) -> Result<Vec<GbAddonEntry>, LibraryError> {
        let mut entries = self
            .db
            .read(|conn| {
                let mut stmt = conn.prepare(&format!(
                    "SELECT {COLUMNS} FROM library_gb_addons
                     ORDER BY section COLLATE NOCASE, name COLLATE NOCASE"
                ))?;
                let rows = stmt.query_map([], entry_from_row)?;
                rows.collect::<Result<Vec<_>, _>>()
            })
            .map_err(library_db)?;
        // An empty record never counts as present: there is nothing in the game to find.
        if let Some(tf2) = tf2 {
            let tf = tf2.tf_dir();
            for entry in &mut entries {
                entry.present = !entry.files.is_empty()
                    && entry
                        .files
                        .iter()
                        .all(|f| recorded_path(&tf, entry.gb_id, f).is_some_and(|p| p.exists()));
            }
        }
        Ok(entries)
    }

    /// Installs `chosen` (payload names of `download`) as add-on `download.gb_id()`,
    /// replacing any earlier install of it, and returns its row. `tf2_running` is asked
    /// once, before anything changes, when a VPK would be written or deleted.
    ///
    /// # Errors
    /// [`GbAddonInstallError`]: a refusal before anything changed, or an I/O or database
    /// failure; a failure after the old install was cleared leaves a row not installed.
    pub fn install_gb_addon(
        &self,
        download: GbAddonDownload,
        chosen: &[String],
        tf2: &Tf2Install,
        tf2_running: &dyn Fn() -> bool,
    ) -> Result<GbAddonEntry, GbAddonInstallError> {
        let _writes = self.lock_writes();
        if chosen.is_empty() {
            return Err(GbAddonInstallError::NothingChosen);
        }
        if let Some(name) = chosen
            .iter()
            .find(|c| !download.payloads.iter().any(|p| p.name() == c.as_str()))
        {
            return Err(GbAddonInstallError::UnknownPayload { name: name.clone() });
        }
        let picked: Vec<&Payload> = download
            .payloads
            .iter()
            .filter(|p| chosen.iter().any(|c| c == p.name()))
            .collect();
        let custom = game_custom_dir(tf2)?;
        let tf = tf2.tf_dir();
        let gb_id = download.gb_id;
        let previous = self.gb_addon_row(gb_id)?;

        // TF2 holds its VPKs open, so writing a new one or deleting an old one waits for it
        // to close; a folder-only change goes ahead.
        let vpk_changes = picked.iter().any(|p| p.kind() == PayloadKind::Vpk)
            || previous.iter().flat_map(|e| &e.files).any(|f| is_vpk(f));
        if vpk_changes && tf2_running() {
            return Err(GbAddonInstallError::Tf2Running);
        }

        // The library copy is staged whole before anything that exists is touched, so a
        // failure here leaves the old install as it was.
        let root = self.dir.gb_addon_dir();
        let staging = root.join(format!(".staging-{gb_id}"));
        let base = installed_base(gb_id, &download.name);
        let staged = remove_dir_if_present(&staging)
            .and_then(|()| stage(&staging, &picked, &base))
            .inspect_err(|_| {
                let _ = remove_dir_if_present(&staging);
            })?;
        if previous.is_some()
            && let Err(e) = self.mark_not_installed(gb_id)
        {
            let _ = remove_dir_if_present(&staging);
            return Err(e);
        }

        let mut entry = GbAddonEntry {
            gb_id,
            section: download.section.to_owned(),
            name: download.name.clone(),
            author: download.author.clone(),
            file_date: download.file_date,
            latest_file_date: download.file_date,
            checked_at: unix_now(),
            payloads: picked.iter().map(|p| p.name().to_owned()).collect(),
            files: staged
                .iter()
                .flat_map(|(_, names)| names.iter().map(|n| format!("custom/{n}")))
                .collect(),
            installed: false,
            gone: false,
            present: false,
        };

        // From the clear on, a failure keeps the new row with installed = 0 so the page
        // offers to install it again from the library copy.
        let library_copy = root.join(gb_id.to_string());
        let mut copied = Vec::new();
        let swapped = (|| {
            for file in previous.iter().flat_map(|e| &e.files) {
                if let Some(path) = recorded_path(&tf, gb_id, file) {
                    remove_entry(&path)?;
                }
            }
            remove_dir_if_present(&library_copy)?;
            std::fs::rename(&staging, &library_copy).map_err(io_at(&library_copy))?;
            create_dir_all(&custom)?;
            for (n, names) in &staged {
                for name in names {
                    let target = custom.join(name);
                    remove_entry(&target)?;
                    copied.push(target.clone());
                    copy_entry(&library_copy.join(n.to_string()).join(name), &target)?;
                }
            }
            Ok::<_, IoFailure>(())
        })();
        if let Err(e) = swapped {
            for path in &copied {
                let _ = remove_entry(path);
            }
            // A clear or rename that failed leaves the new files only in staging; they
            // replace the library copy so it always matches the row the page reinstalls from.
            if staging.exists() {
                let _ = remove_dir_if_present(&library_copy).and_then(|()| {
                    std::fs::rename(&staging, &library_copy).map_err(io_at(&staging))
                });
            }
            // The I/O failure is the one to report; a row that also failed to save keeps
            // the old row's installed = 0.
            let _ = self.save_gb_addon(&entry, download.file_id);
            return Err(e.into());
        }
        entry.installed = true;
        entry.present = true;
        self.save_gb_addon(&entry, download.file_id)?;
        Ok(entry)
    }

    fn gb_addon_row(&self, gb_id: GbId) -> Result<Option<GbAddonEntry>, GbAddonInstallError> {
        let id = gb_id_column(gb_id)?;
        self.db
            .read(|conn| {
                conn.query_row(
                    &format!("SELECT {COLUMNS} FROM library_gb_addons WHERE gb_id = ?1"),
                    [id],
                    entry_from_row,
                )
                .optional()
            })
            .map_err(gb_db)
    }

    fn mark_not_installed(&self, gb_id: GbId) -> Result<(), GbAddonInstallError> {
        let id = gb_id_column(gb_id)?;
        self.db
            .write(|conn| {
                conn.execute(
                    "UPDATE library_gb_addons SET installed = 0 WHERE gb_id = ?1",
                    [id],
                )
            })
            .map(drop)
            .map_err(gb_db)
    }

    /// Writes `entry` as the add-on's whole row, replacing any earlier one.
    fn save_gb_addon(&self, entry: &GbAddonEntry, file_id: u64) -> Result<(), GbAddonInstallError> {
        let id = gb_id_column(entry.gb_id)?;
        let file_id = i64::try_from(file_id).map_err(|e| GbAddonInstallError::Database {
            message: format!("file id {file_id} does not fit the database: {e}"),
        })?;
        let payloads = serde_json::to_string(&entry.payloads).unwrap_or_else(|_| "[]".into());
        let files = serde_json::to_string(&entry.files).unwrap_or_else(|_| "[]".into());
        self.db
            .write(|conn| {
                conn.execute(
                    "INSERT OR REPLACE INTO library_gb_addons (gb_id, section, name, author,
                     file_id, file_date, latest_file_date, checked_at, payloads, files,
                     installed, gone) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
                    rusqlite::params![
                        id,
                        entry.section,
                        entry.name,
                        entry.author,
                        file_id,
                        entry.file_date,
                        entry.latest_file_date,
                        entry.checked_at,
                        payloads,
                        files,
                        entry.installed,
                        entry.gone,
                    ],
                )
            })
            .map(drop)
            .map_err(gb_db)
    }
}

/// `tf/custom/<name>` for a recorded `custom/<name>` that is a plain name of this add-on's
/// own (`tf2tools-gb-<id>-...`, no `:` that Windows reads as a drive). Any other shape, such
/// as a tampered row's `../x`, an absolute path or another add-on's folder, is never followed.
fn recorded_path(tf: &Path, gb_id: GbId, file: &str) -> Option<PathBuf> {
    let name = file.strip_prefix("custom/")?;
    let own = name.starts_with(&format!("tf2tools-gb-{gb_id}-"));
    (own && is_plain_name(name) && !name.contains(':')).then(|| tf.join("custom").join(name))
}

/// `tf2tools-gb-<id>-<slug>`: the name's slug capped and trimmed of `-`, or `addon`.
fn installed_base(gb_id: GbId, name: &str) -> String {
    let capped: String = slug(name).chars().take(MAX_SLUG).collect();
    let trimmed = capped.trim_matches('-');
    let slug = if trimmed.is_empty() { "addon" } else { trimmed };
    format!("tf2tools-gb-{gb_id}-{slug}")
}

/// Copies each picked payload into `staging/<n>/` under its installed name, `n` counting
/// from 1, and returns each slot with the names it holds. The first payload takes `base`,
/// the n-th `base-n`; a multi-chunk VPK keeps its `_dir` and `_NNN` suffixes.
fn stage(
    staging: &Path,
    picked: &[&Payload],
    base: &str,
) -> Result<Vec<(usize, Vec<String>)>, IoFailure> {
    let mut staged = Vec::new();
    for (n, payload) in (1..).zip(picked) {
        let name = if n == 1 {
            base.to_owned()
        } else {
            format!("{base}-{n}")
        };
        let slot = staging.join(n.to_string());
        create_dir_all(&slot)?;
        let mut names = Vec::new();
        match payload.kind() {
            PayloadKind::Folder => {
                copy_without_vpks(payload.path(), &slot.join(&name))?;
                names.push(name);
            }
            PayloadKind::Vpk => {
                let single = payload.chunks().is_empty();
                let parts = std::iter::once(payload.path())
                    .chain(payload.chunks().iter().map(PathBuf::as_path));
                for (i, source) in parts.enumerate() {
                    let file = match i {
                        0 if single => format!("{name}.vpk"),
                        0 => format!("{name}_dir.vpk"),
                        _ => format!("{name}_{}.vpk", chunk_number(source)),
                    };
                    let target = slot.join(&file);
                    std::fs::copy(source, &target).map_err(io_at(&target))?;
                    names.push(file);
                }
            }
        }
        staged.push((n, names));
    }
    Ok(staged)
}

/// The `NNN` of a `<base>_NNN.vpk` chunk, which the payload walk guarantees.
fn chunk_number(chunk: &Path) -> String {
    let stem = chunk
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    stem.rsplit_once('_')
        .map_or(stem.clone(), |(_, digits)| digits.to_owned())
}

/// Copies the folder `src` to `dst` without its `.vpk` files or symlinks: a folder
/// payload installs loose, and its nested VPKs are payloads of their own.
fn copy_without_vpks(src: &Path, dst: &Path) -> Result<(), IoFailure> {
    create_dir_all(dst)?;
    for entry in WalkDir::new(src).min_depth(1) {
        let entry = entry.map_err(|e| walk_failure(src, e))?;
        let Ok(relative) = entry.path().strip_prefix(src) else {
            continue;
        };
        let target = dst.join(relative);
        if entry.file_type().is_dir() {
            create_dir_all(&target)?;
        } else if entry.file_type().is_file() && !is_vpk(&relative.to_string_lossy()) {
            std::fs::copy(entry.path(), &target).map_err(io_at(&target))?;
        }
    }
    Ok(())
}

/// Copies a library-copy file or folder into the game.
fn copy_entry(src: &Path, dst: &Path) -> Result<(), IoFailure> {
    if src.is_dir() {
        copy_without_vpks(src, dst)
    } else {
        std::fs::copy(src, dst).map(drop).map_err(io_at(dst))
    }
}

/// Removes a file or folder; one already gone counts as removed. A symlink is removed
/// itself, never followed.
fn remove_entry(path: &Path) -> Result<(), IoFailure> {
    match std::fs::symlink_metadata(path) {
        Ok(meta) if meta.is_dir() => remove_dir_if_present(path),
        Ok(_) => remove_file_if_present(path),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(io_at(path)(e)),
    }
}

fn is_vpk(name: &str) -> bool {
    Path::new(name)
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("vpk"))
}

fn unix_now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_secs()).unwrap_or(i64::MAX))
}

fn gb_id_column(gb_id: GbId) -> Result<i64, GbAddonInstallError> {
    i64::try_from(gb_id.get()).map_err(|e| GbAddonInstallError::Database {
        message: format!("GameBanana id {gb_id} does not fit the database: {e}"),
    })
}

fn gb_db(e: rusqlite::Error) -> GbAddonInstallError {
    GbAddonInstallError::Database {
        message: e.to_string(),
    }
}
