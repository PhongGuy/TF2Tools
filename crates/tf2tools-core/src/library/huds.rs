//! HUD library: importing a HUD archive with its provenance, listing and deleting, and
//! installing one HUD at a time into `tf/custom`.

use std::collections::{HashMap, HashSet};
use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};

use rusqlite::{OptionalExtension, params};
use walkdir::WalkDir;

use super::{
    HudId, IoFailure, Library, METADATA_DIR, Provenance, copy_dir_excluding_metadata,
    create_dir_all, first_free_number, game_custom_dir, gb_id_column, hud_folder, hud_print,
    import_db, install_db, io_at, is_plain_name, library_db, live_link, made_huds, new_id, now,
    read_setting, remove_dir_if_present, save_setting, walk_failure,
};
use crate::archive::extract_archive;
use crate::error::{ImportError, InstallError, KeepHudError, LibraryError};
use crate::hud_maker::{MakerBase, MakerFile};
use crate::paths::LibraryKind;
use crate::tf2::Tf2Install;

/// The setting beside `installed_hud_folder` that fingerprints what the install put there.
const PRINT_KEY: &str = "installed_hud_fingerprint";

/// How a HUD folder in `tf/custom` stands against the library.
enum Standing {
    /// The library can put it back, so removing it loses nothing.
    PutBack,
    /// A library HUD's folder holding files the library does not have.
    Changed,
    /// The library has nothing for it.
    Foreign,
}

/// Where `Library::install_hud` put a HUD in `tf/custom`.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct HudInstall {
    folder: String,
    blocked_name: Option<String>,
}

impl HudInstall {
    /// The folder under `tf/custom/` the HUD now lives in.
    pub fn folder(&self) -> &str {
        &self.folder
    }

    /// The name the HUD expected, when a folder or file of the player's held it and the
    /// HUD went under its library folder name instead; parts of it may then not show.
    pub fn blocked_name(&self) -> Option<&str> {
        self.blocked_name.as_deref()
    }
}

/// What `tf/custom` holds in the way of HUDs: folders directly under it with an `info.vdf`.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum HudStatus {
    /// No HUD is installed.
    NotInstalled,
    /// The one installed HUD is a library HUD.
    Managed {
        /// The folder under `tf/custom/`.
        folder_name: String,
        /// The library HUD with that folder name.
        id: HudId,
    },
    /// The one installed HUD is not in the library.
    Unmanaged {
        /// The folder under `tf/custom/`.
        folder_name: String,
    },
    /// More than one HUD is installed, so none of them is reliably active.
    Multiple {
        /// The HUD folders, sorted.
        folder_names: Vec<String>,
    },
}

/// A HUD folder in `tf/custom` the library cannot put back, so removing it loses it.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct ExternalHud {
    folder: OsString,
    folder_name: String,
    changed: bool,
}

impl ExternalHud {
    /// The folder's name under `tf/custom/`, for showing to the player.
    pub fn folder_name(&self) -> &str {
        &self.folder_name
    }

    /// Whether this is a library HUD's folder whose files differ from what the library can
    /// put back, so what goes is the player's changes.
    pub fn has_changes(&self) -> bool {
        self.changed
    }
}

/// A HUD in the library, read from `library_huds`.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct HudEntry {
    id: HudId,
    folder_name: String,
    name: String,
    author: String,
    provenance: Provenance,
    has_thumbnail: bool,
    has_gallery: bool,
    created_at: String,
    updated_at: String,
    wanted_name: Option<String>,
}

impl HudEntry {
    /// The library id.
    pub fn id(&self) -> &HudId {
        &self.id
    }

    /// The folder under `library/huds/`; `install_hud` returns the `tf/custom` folder,
    /// which may differ.
    pub fn folder_name(&self) -> &str {
        &self.folder_name
    }

    /// The display name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The author; empty when unknown.
    pub fn author(&self) -> &str {
        &self.author
    }

    /// Where the HUD came from.
    pub fn provenance(&self) -> &Provenance {
        &self.provenance
    }

    /// Whether `.tf2tools/` holds a thumbnail.
    pub fn has_thumbnail(&self) -> bool {
        self.has_thumbnail
    }

    /// Whether `.tf2tools/` holds gallery images.
    pub fn has_gallery(&self) -> bool {
        self.has_gallery
    }

    /// When the HUD was imported, as v5's RFC 3339 text.
    pub fn created_at(&self) -> &str {
        &self.created_at
    }

    /// When the HUD was last changed, as v5's RFC 3339 text.
    pub fn updated_at(&self) -> &str {
        &self.updated_at
    }

    /// The name an import wanted when another library HUD already had it, so this one
    /// took a numbered name instead. Only the entry `import_hud` returns carries it.
    pub fn wanted_name(&self) -> Option<&str> {
        self.wanted_name.as_deref()
    }

    /// The entry with its thumbnail flag set to `made`.
    pub(super) fn with_thumbnail(mut self, made: bool) -> HudEntry {
        self.has_thumbnail = made;
        self
    }
}

pub(super) const HUD_COLUMNS: &str = "id, folder_name, name, author, store_hud_id, store_gb_id, \
     has_thumbnail, has_gallery, created_at, updated_at";

pub(super) fn hud_from_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<HudEntry> {
    let author: String = r.get("author")?;
    Ok(HudEntry {
        id: HudId::new(r.get::<_, String>("id")?),
        folder_name: r.get("folder_name")?,
        name: r.get("name")?,
        provenance: Provenance::from_columns(
            r.get("store_hud_id")?,
            r.get("store_gb_id")?,
            &author,
        ),
        author,
        has_thumbnail: r.get::<_, i64>("has_thumbnail")? != 0,
        has_gallery: r.get::<_, i64>("has_gallery")? != 0,
        created_at: r.get("created_at")?,
        updated_at: r.get("updated_at")?,
        wanted_name: None,
    })
}

/// The numbered name a HUD takes when the one it wanted is in use.
struct Rename {
    name: String,
    wanted: String,
}

/// The display name and author a HUD in `folder_name` takes from the store row its
/// provenance points at; without a store row the folder name stands in for the name.
fn hud_display(
    conn: &rusqlite::Connection,
    folder_name: &str,
    provenance: &Provenance,
    store_gb_id: Option<i64>,
) -> rusqlite::Result<(String, String)> {
    let fallback = || (folder_name.to_owned(), String::new());
    Ok(match provenance {
        Provenance::Local => fallback(),
        Provenance::Curated { hud_id } => conn
            .query_row(
                "SELECT name, author FROM store_huds WHERE hud_id = ?1",
                [hud_id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?
            .unwrap_or_else(fallback),
        Provenance::GameBanana { author, .. } => {
            let name = conn
                .query_row(
                    "SELECT name FROM store_huds_gb WHERE gb_id = ?1",
                    [store_gb_id],
                    |r| r.get(0),
                )
                .optional()?
                .unwrap_or_else(|| folder_name.to_owned());
            (name, author.clone())
        }
    })
}

impl Library {
    /// Every library HUD, ordered by display name.
    pub fn list_huds(&self) -> Result<Vec<HudEntry>, LibraryError> {
        self.db
            .read(|conn| {
                let mut stmt = conn.prepare(&format!(
                    "SELECT {HUD_COLUMNS} FROM library_huds ORDER BY name"
                ))?;
                stmt.query_map([], hud_from_row)?.collect()
            })
            .map_err(library_db)
    }

    /// Imports the HUD inside `archive` (any format `extract_archive` reads) into
    /// `library/huds/<folder>` and records it with `provenance`. The same store item in that
    /// folder is replaced; any other HUD holding the folder or the name stays, and this one
    /// takes a numbered name (see `HudEntry::wanted_name`). Fails with `NotAHud` when no
    /// `info.vdf` is inside.
    pub fn import_hud(
        &self,
        archive: &Path,
        provenance: Provenance,
    ) -> Result<HudEntry, ImportError> {
        // The scratch dir is removed when the guard drops, on every return path.
        let tmp = self.dir.tmp_dir();
        create_dir_all(&tmp)?;
        let scratch = tempfile::Builder::new()
            .tempdir_in(&tmp)
            .map_err(io_at(&tmp))?;
        let extracted = extract_archive(archive, scratch.path())?;
        if extracted == archive {
            return Err(ImportError::NotAHud {
                path: archive.to_path_buf(),
            });
        }

        let root = find_hud_root(scratch.path())?.ok_or_else(|| ImportError::NotAHud {
            path: archive.to_path_buf(),
        })?;
        // A HUD at the archive root has no folder of its own, so it takes the archive's name.
        let folder_os = if root == scratch.path() {
            archive.file_stem()
        } else {
            root.file_name()
        };
        let folder_name = folder_os
            .map(|n| n.to_string_lossy().into_owned())
            .filter(|n| is_plain_name(n))
            .ok_or_else(|| ImportError::NotAHud {
                path: archive.to_path_buf(),
            })?;

        let (folder_name, rename) = self.place_hud(&folder_name, &provenance)?;
        self.add_hud_folder(&root, &folder_name, provenance, rename)
    }

    /// Where a HUD arriving as `folder` with `provenance` goes. The same store item already
    /// in that folder is an update and goes back in place. Any other HUD holding the folder
    /// or the display name, or a folder already on disk, keeps it, and the newcomer takes
    /// the first free number: `<folder>-<n>` named `<name> (<n>)`.
    fn place_hud(
        &self,
        folder: &str,
        provenance: &Provenance,
    ) -> Result<(String, Option<Rename>), ImportError> {
        let gb_column = match provenance {
            Provenance::GameBanana { gb_id, .. } => Some(gb_id_column(*gb_id)?),
            Provenance::Local | Provenance::Curated { .. } => None,
        };
        let (name, rows) = self
            .db
            .read(|conn| {
                let (name, _) = hud_display(conn, folder, provenance, gb_column)?;
                let mut stmt = conn.prepare(
                    "SELECT folder_name, name, store_hud_id, store_gb_id FROM library_huds",
                )?;
                let rows = stmt
                    .query_map([], |r| {
                        Ok((
                            r.get::<_, String>(0)?,
                            r.get::<_, String>(1)?,
                            r.get::<_, Option<String>>(2)?,
                            r.get::<_, Option<i64>>(3)?,
                        ))
                    })?
                    .collect::<rusqlite::Result<Vec<_>>>()?;
                Ok((name, rows))
            })
            .map_err(import_db)?;

        // Rows of this very store item never stand in the way; a manual import has no
        // identity, so it is never the same HUD.
        let same_item = |hud_id: Option<&str>, gb_id: Option<i64>| match provenance {
            Provenance::Local => false,
            Provenance::Curated { hud_id: wanted } => hud_id == Some(wanted.as_str()),
            Provenance::GameBanana { .. } => gb_id.is_some() && gb_id == gb_column,
        };
        let mut folders = HashSet::new();
        let mut names = HashSet::new();
        for (row_folder, row_name, hud_id, gb_id) in &rows {
            if same_item(hud_id.as_deref(), *gb_id) {
                if row_folder == folder {
                    return Ok((folder.to_owned(), None));
                }
                continue;
            }
            folders.insert(row_folder.as_str());
            names.insert(row_name.as_str());
        }

        let library = self.dir.library_dir(LibraryKind::Huds);
        let free = |f: &str, n: &str| {
            !folders.contains(f)
                && !names.contains(n)
                && library.join(f).symlink_metadata().is_err()
        };
        if free(folder, &name) {
            return Ok((folder.to_owned(), None));
        }
        let number =
            first_free_number(|n| free(&format!("{folder}-{n}"), &format!("{name} ({n})")));
        let rename = Rename {
            name: format!("{name} ({number})"),
            wanted: name,
        };
        Ok((format!("{folder}-{number}"), Some(rename)))
    }

    /// Copies the HUD at `root` to `library/huds/<folder_name>`, replacing what is there, and
    /// records it, under `rename`'s name when given. A failure leaves no folder behind.
    fn add_hud_folder(
        &self,
        root: &Path,
        folder_name: &str,
        provenance: Provenance,
        rename: Option<Rename>,
    ) -> Result<HudEntry, ImportError> {
        let dest = self.dir.library_dir(LibraryKind::Huds).join(folder_name);
        let copied = remove_dir_if_present(&dest)
            .and_then(|()| copy_dir_excluding_metadata(root, &dest))
            .and_then(|()| create_dir_all(&dest.join(METADATA_DIR)));
        if let Err(failure) = copied {
            let _ = remove_dir_if_present(&dest);
            return Err(failure.into());
        }

        let recorded = self.record_hud(folder_name, provenance, rename);
        if recorded.is_err() {
            let _ = remove_dir_if_present(&dest);
        }
        recorded.map(|entry| self.refresh_hud_thumbnail(entry))
    }

    /// Writes the `library_huds` row for a freshly copied folder, taking the display name
    /// (unless `rename` gives one) and author from the store row its provenance points at.
    fn record_hud(
        &self,
        folder_name: &str,
        provenance: Provenance,
        rename: Option<Rename>,
    ) -> Result<HudEntry, ImportError> {
        let (source, store_hud_id, store_gb_id) = match &provenance {
            Provenance::Local => ("local", None, None),
            Provenance::Curated { hud_id } => ("store", Some(hud_id.clone()), None),
            Provenance::GameBanana { gb_id, .. } => ("store_gb", None, Some(gb_id_column(*gb_id)?)),
        };
        let id = new_id();
        let timestamp = now();
        self.db
            .write(|conn| {
                let tx = conn.transaction()?;
                let (mut name, author) = hud_display(&tx, folder_name, &provenance, store_gb_id)?;
                let wanted_name = rename.map(|rename| {
                    name = rename.name;
                    rename.wanted
                });
                tx.execute(
                    "INSERT OR REPLACE INTO library_huds (id, folder_name, name, author, source,
                         store_hud_id, store_gb_id, created_at, updated_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?8)",
                    params![
                        id,
                        folder_name,
                        name,
                        author,
                        source,
                        store_hud_id,
                        store_gb_id,
                        timestamp
                    ],
                )?;
                tx.commit()?;
                Ok(HudEntry {
                    id: HudId::new(id),
                    folder_name: folder_name.to_owned(),
                    name,
                    author,
                    provenance,
                    has_thumbnail: false,
                    has_gallery: false,
                    created_at: timestamp.clone(),
                    updated_at: timestamp,
                    wanted_name,
                })
            })
            .map_err(import_db)
    }

    /// Removes a HUD's library folder and its row; `tf/custom` is not touched. A made HUD's
    /// previous save stranded as `<folder>.old` by an interrupted re-save goes too. A row
    /// whose folder name is not a plain name has no files, so only the row goes.
    pub fn delete_hud(&self, id: &HudId) -> Result<(), LibraryError> {
        let folder: Option<String> = self
            .db
            .read(|conn| {
                conn.query_row(
                    "SELECT folder_name FROM library_huds WHERE id = ?1",
                    [id.as_str()],
                    |r| r.get(0),
                )
                .optional()
            })
            .map_err(library_db)?;
        let folder = folder.ok_or_else(|| LibraryError::NotFound {
            what: format!("HUD {id}"),
        })?;
        // The row goes first, so a failed removal leaves stray files, never a dangling row.
        self.db
            .write(|conn| conn.execute("DELETE FROM library_huds WHERE id = ?1", [id.as_str()]))
            .map_err(library_db)?;
        if is_plain_name(&folder) {
            let live = self.dir.library_dir(LibraryKind::Huds).join(&folder);
            remove_dir_if_present(&live)?;
            if let Some(old) = made_huds::stranded_save(&live) {
                remove_dir_if_present(&old)?;
            }
        }
        Ok(())
    }
}

impl Library {
    /// Installs a library HUD into `tf/custom/<folder>`, first removing every installed HUD
    /// (only one can be active), records it as v5 does and returns where it went. `<folder>`
    /// is the name the HUD's files reference, else a store HUD's repo name, else its library
    /// folder; when the player's own folder or file holds that name, the library folder
    /// (numbered if that is held too), with the held name reported. The library copy is checked before anything is removed,
    /// so an unknown or vanished HUD leaves the installed one. Fails with `ExternalHuds` and
    /// changes nothing when a HUD the library cannot put back is in `tf/custom`.
    pub fn install_hud(&self, id: &HudId, tf2: &Tf2Install) -> Result<HudInstall, InstallError> {
        self.install_hud_discarding(id, tf2, &[])
    }

    /// `install_hud`, except that it may remove the HUDs in `discard`. It fails with
    /// `ExternalHuds` and changes nothing when any other HUD the library cannot put back is
    /// in `tf/custom`.
    pub fn install_hud_discarding(
        &self,
        id: &HudId,
        tf2: &Tf2Install,
        discard: &[ExternalHud],
    ) -> Result<HudInstall, InstallError> {
        let _swap = live_link::hud_swap_lock();
        let not_in_library = || InstallError::NotInLibrary {
            what: format!("HUD {id}"),
        };
        let row: Option<(String, Option<String>, Option<i64>)> = self
            .db
            .read(|conn| {
                conn.query_row(
                    "SELECT folder_name, store_hud_id, store_gb_id FROM library_huds WHERE id = ?1",
                    [id.as_str()],
                    |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
                )
                .optional()
            })
            .map_err(install_db)?;
        let (folder, store_hud_id, store_gb_id) = row
            .filter(|(f, _, _)| is_plain_name(f))
            .ok_or_else(not_in_library)?;
        // A made HUD caught mid re-save is read from its stranded `.old`, never renamed back:
        // only a save moves library folders.
        let live = self.dir.library_dir(LibraryKind::Huds).join(&folder);
        let src = if live.is_dir() {
            live
        } else {
            made_huds::stranded_save(&live).ok_or_else(not_in_library)?
        };

        let custom = game_custom_dir(tf2)?;
        self.refuse_unnamed_huds(&custom, discard, None)?;
        // Scan before touching tf/custom, so a failed scan leaves the old install in place.
        let mut target = self.install_name(&src, store_hud_id, store_gb_id, &folder)?;
        // Blanked before anything moves, so a failed removal or copy never leaves the
        // previous HUD recorded as installed; unset keys stay unset.
        self.db
            .write(|conn| {
                conn.execute(
                    "UPDATE settings SET value = '' \
                     WHERE key IN ('installed_hud_folder', 'installed_hud_id', \
                     'installed_hud_fingerprint')",
                    [],
                )
            })
            .map_err(install_db)?;
        // A stale reinstall's staging folder goes too, so its set-aside copy never returns.
        live_link::clear_staging(&custom)?;
        self.revert_hud_cfg(tf2)?;
        remove_installed_huds(&custom)?;
        // Removal frees names held by HUDs; anything still there is the user's own, so the
        // HUD takes its library folder name, numbered if need be, rather than merging into it.
        let mut blocked_name = None;
        if custom.join(&target).symlink_metadata().is_ok() {
            let free = free_custom_folder(&custom, &folder);
            blocked_name = Some(std::mem::replace(&mut target, free));
        }
        // The target was free, so a half copy is ours to clear; left behind it would hold the
        // name against the retry.
        let dest = custom.join(&target);
        if let Err(failure) = copy_dir_excluding_metadata(&src, &dest) {
            let _ = remove_dir_if_present(&dest);
            return Err(failure.into());
        }
        self.write_hud_cfg(&src, tf2)?;
        // Without a fingerprint the folder is judged against the library copy, so a failed
        // read costs a question at worst.
        let print = hud_print::fingerprint(&dest).unwrap_or_default();

        self.db
            .write(|conn| {
                let tx = conn.transaction()?;
                save_setting(&tx, "installed_hud_folder", &target)?;
                save_setting(&tx, "installed_hud_id", id.as_str())?;
                save_setting(&tx, PRINT_KEY, &print)?;
                save_setting(&tx, "active_profile_id", "")?;
                tx.commit()
            })
            .map_err(install_db)?;
        Ok(HudInstall {
            folder: target,
            blocked_name,
        })
    }

    /// The `tf/custom` folder a HUD's files at `src` want: the name they reference most,
    /// else a curated HUD's repo name, else a copy's recorded install folder, else its
    /// library `folder`.
    pub(super) fn install_name(
        &self,
        src: &Path,
        store_hud_id: Option<String>,
        store_gb_id: Option<i64>,
        folder: &str,
    ) -> Result<String, InstallError> {
        if let Some(name) = hud_folder::referenced_folder(src)? {
            return Ok(name);
        }
        if let Provenance::Curated { hud_id } =
            Provenance::from_columns(store_hud_id, store_gb_id, "")
            && let Some(name) = self.curated_folder(&hud_id, folder)?
        {
            return Ok(name);
        }
        Ok(recorded_install_folder(src).unwrap_or_else(|| folder.to_owned()))
    }

    /// A curated HUD's fallback install folder: its store repo's name, else its library
    /// folder without the commit hash, else `None`.
    fn curated_folder(&self, hud_id: &str, folder: &str) -> Result<Option<String>, InstallError> {
        let repo: Option<String> = self
            .db
            .read(|conn| {
                conn.query_row(
                    "SELECT repo FROM store_huds WHERE hud_id = ?1",
                    [hud_id],
                    |r| r.get(0),
                )
                .optional()
            })
            .map_err(install_db)?;
        Ok(repo
            .as_deref()
            .and_then(hud_folder::repo_folder)
            .or_else(|| hud_folder::strip_commit_hash(folder)))
    }

    /// Removes every installed HUD from `tf/custom` and clears v5's install settings. Fails
    /// with `ExternalHuds` and changes nothing when one is a HUD the library cannot put back.
    pub fn uninstall_hud(&self, tf2: &Tf2Install) -> Result<(), InstallError> {
        self.uninstall_hud_discarding(tf2, &[])
    }

    /// `uninstall_hud`, except that it may remove the HUDs in `discard`. It fails with
    /// `ExternalHuds` and changes nothing when any other HUD the library cannot put back is
    /// in `tf/custom`.
    pub fn uninstall_hud_discarding(
        &self,
        tf2: &Tf2Install,
        discard: &[ExternalHud],
    ) -> Result<(), InstallError> {
        let _swap = live_link::hud_swap_lock();
        let custom = game_custom_dir(tf2)?;
        self.refuse_unnamed_huds(&custom, discard, None)?;
        live_link::clear_staging(&custom)?;
        self.revert_hud_cfg(tf2)?;
        remove_installed_huds(&custom)?;
        self.forget_install()
    }

    /// Removes the one HUD called `folder` from `tf/custom`, as `hud_status` names it, and
    /// leaves every other HUD alone. Install settings are cleared only when it is the
    /// recorded install. A name that is no HUD folder there changes nothing. Fails with
    /// `ExternalHuds` and changes nothing when the library cannot put this HUD back and
    /// `discard` does not name it.
    pub fn uninstall_one_hud(
        &self,
        tf2: &Tf2Install,
        folder: &str,
        discard: &[ExternalHud],
    ) -> Result<(), InstallError> {
        let _swap = live_link::hud_swap_lock();
        let custom = game_custom_dir(tf2)?;
        // Only what the scan found can go, so no name reaches outside tf/custom.
        let found = scan_hud_folders(&custom)?;
        let target = found.iter().find_map(|path| {
            let name = path.file_name()?;
            (name.to_string_lossy() == folder).then_some((path, name))
        });
        let Some((path, name)) = target else {
            return Ok(());
        };
        self.refuse_unnamed_huds(&custom, discard, Some(name))?;
        let recorded = self
            .db
            .read(|conn| read_setting(conn, "installed_hud_folder"))
            .map_err(install_db)?
            .is_some_and(|installed| installed == folder);
        // Staging only ever holds the recorded install, and another HUD's removal must not
        // cost that install its way back after a crash.
        if recorded {
            live_link::clear_staging(&custom)?;
            self.revert_hud_cfg(tf2)?;
        }
        remove_dir_if_present(path)?;
        if recorded {
            self.forget_install()?;
        }
        Ok(())
    }

    /// Clears the record of what is installed, along with the profile that put it there.
    fn forget_install(&self) -> Result<(), InstallError> {
        self.db
            .write(|conn| {
                let tx = conn.transaction()?;
                save_setting(&tx, "installed_hud_folder", "")?;
                save_setting(&tx, "installed_hud_id", "")?;
                save_setting(&tx, PRINT_KEY, "")?;
                save_setting(&tx, "active_profile_id", "")?;
                tx.commit()
            })
            .map_err(install_db)
    }

    /// The HUD folders in `tf/custom` that installing or uninstalling would lose for good,
    /// because the library cannot put them back; sorted.
    pub fn external_huds(&self, tf2: &Tf2Install) -> Result<Vec<ExternalHud>, LibraryError> {
        self.external_in(&tf2.custom_dir())
    }

    /// Adds each of `huds` still in `tf/custom` to the library as a local HUD, leaving
    /// `tf/custom` as it is, and returns the new entries. A folder name the library already
    /// uses gets a numbered one, so the installing call must still name `huds`. A failure
    /// stops at that HUD; what was added before it stays. `.tf2tools` is not copied.
    pub fn keep_external_huds(
        &self,
        huds: &[ExternalHud],
        tf2: &Tf2Install,
    ) -> Result<Vec<HudEntry>, KeepHudError> {
        let _swap = live_link::hud_swap_lock();
        let custom = tf2.custom_dir();
        let present = self.external_in(&custom).map_err(keep_library)?;
        let mut kept = Vec::new();
        for hud in huds.iter().filter(|hud| present.contains(hud)) {
            let src = custom.join(&hud.folder);
            refuse_uncopyable(&src)?;
            let folder = self.free_hud_folder(hud.folder_name())?;
            let entry = self.add_hud_folder(&src, &folder, Provenance::Local, None);
            kept.push(entry.map_err(keep_import)?);
        }
        Ok(kept)
    }

    /// The HUDs in `custom` the library cannot put back: a folder that is no library HUD's,
    /// whose library files are gone, or that no longer matches its install (or, with no
    /// install on record, its library copy).
    pub(super) fn external_in(&self, custom: &Path) -> Result<Vec<ExternalHud>, LibraryError> {
        let folders = scan_hud_folders(custom)?;
        if folders.is_empty() {
            return Ok(Vec::new());
        }
        // Each folder in tf/custom a library HUD answers for, with that HUD's library folder,
        // and the recorded install's folder with its fingerprint.
        let (claims, print) = self
            .db
            .read(|conn| {
                let mut stmt = conn.prepare("SELECT folder_name FROM library_huds")?;
                let mut claims = stmt
                    .query_map([], |r| {
                        let folder: String = r.get(0)?;
                        Ok((folder.clone(), folder))
                    })?
                    .collect::<rusqlite::Result<HashMap<_, _>>>()?;
                let installed = read_setting(conn, "installed_hud_folder")?;
                if let Some(installed) = &installed
                    && let Some(id) = read_setting(conn, "installed_hud_id")?
                    && let Some(folder) = conn
                        .query_row(
                            "SELECT folder_name FROM library_huds WHERE id = ?1",
                            [&id],
                            |r| r.get(0),
                        )
                        .optional()?
                {
                    claims.insert(installed.clone(), folder);
                }
                let print = read_setting(conn, PRINT_KEY)?.filter(|print| !print.is_empty());
                Ok((claims, installed.zip(print)))
            })
            .map_err(library_db)?;
        let mut external = Vec::new();
        for path in &folders {
            let Some(folder) = path.file_name() else {
                continue;
            };
            let changed = match self.standing(path, folder.to_str(), &claims, print.as_ref())? {
                Standing::PutBack => continue,
                Standing::Changed => true,
                Standing::Foreign => false,
            };
            external.push(ExternalHud {
                folder: folder.to_os_string(),
                folder_name: folder.to_string_lossy().into_owned(),
                changed,
            });
        }
        Ok(external)
    }

    /// How the HUD folder at `path`, called `name` when that is valid text, stands against
    /// `claims` and the recorded install `print`.
    fn standing(
        &self,
        path: &Path,
        name: Option<&str>,
        claims: &HashMap<String, String>,
        print: Option<&(String, String)>,
    ) -> Result<Standing, IoFailure> {
        let Some(library_folder) = name
            .and_then(|name| claims.get(name))
            .filter(|folder| is_plain_name(folder))
        else {
            return Ok(Standing::Foreign);
        };
        let live = self.dir.library_dir(LibraryKind::Huds).join(library_folder);
        let src = if live.is_dir() {
            live
        } else {
            match made_huds::stranded_save(&live) {
                Some(old) => old,
                None => return Ok(Standing::Foreign),
            }
        };
        // Only the install's own record proves a match once a save has moved the library on.
        // A mismatch may just be moved times, and identical bytes can safely be put back.
        let same = match print {
            Some((installed, print))
                if name == Some(installed.as_str()) && hud_print::fingerprint(path)? == *print =>
            {
                true
            }
            _ => hud_print::same_files(path, &src)?,
        };
        Ok(if same {
            Standing::PutBack
        } else {
            Standing::Changed
        })
    }

    /// Records what a live update just put at `tf/custom/<folder>` as the install's own.
    pub(super) fn record_install_print(&self, custom: &Path, folder: &str) -> rusqlite::Result<()> {
        let print = hud_print::fingerprint(&custom.join(folder)).unwrap_or_default();
        self.db.write(|conn| save_setting(conn, PRINT_KEY, &print))
    }

    /// The HUDs in `custom` the library cannot put back that `discard` does not name.
    pub(super) fn unnamed_huds(
        &self,
        custom: &Path,
        discard: &[ExternalHud],
    ) -> Result<Vec<ExternalHud>, LibraryError> {
        let mut found = self.external_in(custom)?;
        found.retain(|hud| !discard.contains(hud));
        Ok(found)
    }

    // The one gate in front of every removal, so no caller can lose a HUD by forgetting to ask.
    // `only` narrows it to the one folder a removal touches.
    fn refuse_unnamed_huds(
        &self,
        custom: &Path,
        discard: &[ExternalHud],
        only: Option<&OsStr>,
    ) -> Result<(), InstallError> {
        let mut unnamed = self.unnamed_huds(custom, discard).map_err(|e| match e {
            LibraryError::Io { path, source } => InstallError::Io { path, source },
            other @ LibraryError::NotFound { .. }
            | other @ LibraryError::InvalidWeaponScript { .. }
            | other @ LibraryError::Vpk(_)
            | other @ LibraryError::Vtf(_)
            | other @ LibraryError::Database { .. } => InstallError::Database {
                message: other.to_string(),
            },
        })?;
        unnamed.retain(|hud| only.is_none_or(|folder| hud.folder == folder));
        if unnamed.is_empty() {
            Ok(())
        } else {
            Err(InstallError::ExternalHuds(unnamed))
        }
    }

    /// `wanted`, or the first numbered name after it that no library row or folder uses.
    fn free_hud_folder(&self, wanted: &str) -> Result<String, KeepHudError> {
        let used: HashSet<String> = self
            .db
            .read(|conn| {
                let mut stmt = conn.prepare("SELECT folder_name FROM library_huds")?;
                stmt.query_map([], |r| r.get(0))?.collect()
            })
            .map_err(|e: rusqlite::Error| KeepHudError::Database {
                message: e.to_string(),
            })?;
        let library = self.dir.library_dir(LibraryKind::Huds);
        let taken =
            |name: &str| used.contains(name) || library.join(name).symlink_metadata().is_ok();
        let mut name = wanted.to_owned();
        let mut number = 1u32;
        while taken(&name) {
            number = number.saturating_add(1);
            name = format!("{wanted}-{number}");
        }
        Ok(name)
    }

    /// Scans `tf/custom` for HUDs and tells whether the one installed HUD is from the library.
    pub fn hud_status(&self, tf2: &Tf2Install) -> Result<HudStatus, LibraryError> {
        let mut folders: Vec<String> = scan_hud_folders(&tf2.custom_dir())?
            .iter()
            .filter_map(|path| path.file_name())
            .map(|name| name.to_string_lossy().into_owned())
            .collect();
        if folders.len() > 1 {
            return Ok(HudStatus::Multiple {
                folder_names: folders,
            });
        }
        let Some(folder_name) = folders.pop() else {
            return Ok(HudStatus::NotInstalled);
        };
        // The last install's recorded id claims the folder only while it is still the
        // recorded folder and in the library; else v5's lookup by library folder name,
        // which keeps installs made under the library name Managed.
        let id: Option<String> = self
            .db
            .read(|conn| {
                if read_setting(conn, "installed_hud_folder")?.as_deref() == Some(&folder_name)
                    && let Some(id) = read_setting(conn, "installed_hud_id")?
                    && let Some(id) = conn
                        .query_row("SELECT id FROM library_huds WHERE id = ?1", [&id], |r| {
                            r.get(0)
                        })
                        .optional()?
                {
                    return Ok(Some(id));
                }
                conn.query_row(
                    "SELECT id FROM library_huds WHERE folder_name = ?1 ORDER BY id LIMIT 1",
                    [&folder_name],
                    |r| r.get(0),
                )
                .optional()
            })
            .map_err(library_db)?;
        Ok(match id {
            Some(id) => HudStatus::Managed {
                folder_name,
                id: HudId::new(id),
            },
            None => HudStatus::Unmanaged { folder_name },
        })
    }
}

/// The sorted paths of the folders directly under `custom` that hold an `info.vdf`, ported
/// from v5's `scan_hud_folders`. A missing `custom` holds none.
fn scan_hud_folders(custom: &Path) -> Result<Vec<PathBuf>, IoFailure> {
    let entries = match std::fs::read_dir(custom) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(io_at(custom)(e)),
    };
    let mut folders = Vec::new();
    for entry in entries {
        let path = entry.map_err(io_at(custom))?.path();
        if path.is_dir() && path.join("info.vdf").is_file() {
            folders.push(path);
        }
    }
    folders.sort();
    Ok(folders)
}

/// `wanted`, or the first numbered name after it that nothing under `custom` holds.
fn free_custom_folder(custom: &Path, wanted: &str) -> String {
    let mut name = wanted.to_owned();
    let mut number = 1u32;
    while custom.join(&name).symlink_metadata().is_ok() {
        number = number.saturating_add(1);
        name = format!("{wanted}-{number}");
    }
    name
}

/// Fails on the first entry a copy would silently leave out, so a HUD is never reported
/// kept with parts missing.
fn refuse_uncopyable(src: &Path) -> Result<(), KeepHudError> {
    let walk = WalkDir::new(src)
        .min_depth(1)
        .into_iter()
        .filter_entry(|e| e.file_name() != METADATA_DIR);
    for entry in walk {
        let entry = entry.map_err(|e| walk_failure(src, e))?;
        let kind = entry.file_type();
        if !kind.is_dir() && !kind.is_file() {
            return Err(KeepHudError::NotCopyable {
                path: entry.path().to_path_buf(),
            });
        }
    }
    Ok(())
}

fn keep_library(e: LibraryError) -> KeepHudError {
    match e {
        LibraryError::Io { path, source } => KeepHudError::Io { path, source },
        other @ LibraryError::NotFound { .. }
        | other @ LibraryError::InvalidWeaponScript { .. }
        | other @ LibraryError::Vpk(_)
        | other @ LibraryError::Vtf(_)
        | other @ LibraryError::Database { .. } => KeepHudError::Database {
            message: other.to_string(),
        },
    }
}

fn keep_import(e: ImportError) -> KeepHudError {
    match e {
        ImportError::Io { path, source } => KeepHudError::Io { path, source },
        other @ ImportError::Archive(_)
        | other @ ImportError::NotAHud { .. }
        | other @ ImportError::UnsupportedFile { .. }
        | other @ ImportError::Wav(_)
        | other @ ImportError::NotAVpk { .. }
        | other @ ImportError::Vtf(_)
        | other @ ImportError::UnsupportedProvenance
        | other @ ImportError::Database { .. } => KeepHudError::Database {
            message: other.to_string(),
        },
    }
}

fn remove_installed_huds(custom: &Path) -> Result<(), IoFailure> {
    for folder in scan_hud_folders(custom)? {
        remove_dir_if_present(&folder)?;
    }
    Ok(())
}

/// The directory holding the shallowest `info.vdf` under `scratch`, ties broken by path.
fn find_hud_root(scratch: &Path) -> Result<Option<PathBuf>, ImportError> {
    let mut best: Option<(usize, PathBuf)> = None;
    for entry in WalkDir::new(scratch).min_depth(1) {
        let entry = entry.map_err(|e| walk_failure(scratch, e))?;
        if !entry.file_type().is_file() || entry.file_name() != "info.vdf" {
            continue;
        }
        let candidate = (entry.depth(), entry.path().to_path_buf());
        if best.as_ref().is_none_or(|b| candidate < *b) {
            best = Some(candidate);
        }
    }
    Ok(best.and_then(|(_, path)| path.parent().map(Path::to_path_buf)))
}

/// The install folder a copy's `maker.json` records; an unreadable or damaged file, or an
/// unsafe name (refused by the parser), is `None`.
fn recorded_install_folder(src: &Path) -> Option<String> {
    let path = src.join(METADATA_DIR).join("maker.json");
    let meta = std::fs::symlink_metadata(&path).ok()?;
    if !meta.is_file() || meta.len() > MakerFile::MAX_BYTES as u64 {
        return None;
    }
    let (file, _) = MakerFile::parse(&std::fs::read(&path).ok()?).ok()?;
    match file.base() {
        MakerBase::Copy(base) => base.install_folder().map(str::to_owned),
        MakerBase::Default => None,
    }
}
