//! Sound library: importing a WAV or a VPK sound pack with its provenance, renaming,
//! deleting, reading the bytes to play, and installing them as the hitsound or killsound.

use std::path::{Path, PathBuf};

use rusqlite::{OptionalExtension, params};

use super::{
    IoFailure, Library, Provenance, SoundId, check_vpk_magic, create_dir_all, folder_name,
    game_custom_dir, gb_id_column, import_db, install_db, io_at, is_plain_name, library_db, new_id,
    now, read_setting, remove_dir_if_present, remove_file_if_present, save_setting,
};
use crate::error::{ImportError, InstallError, LibraryError};
use crate::pak::first_wav_in_vpk;
use crate::paths::LibraryKind;
use crate::tf2::Tf2Install;
use crate::wav::inspect_wav;

/// v5's managed folder under `tf/custom` that holds installed WAV sounds.
const SOUNDS_DIR: &str = "tf2tools-sounds";

/// Which game sound a library sound replaces.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum SoundKind {
    /// The sound played when a shot hits.
    Hitsound,
    /// The sound played on a kill.
    Killsound,
}

impl SoundKind {
    const ALL: [Self; 2] = [Self::Hitsound, Self::Killsound];

    /// The kind's name as v5 writes it in file names and settings keys.
    pub fn name(self) -> &'static str {
        match self {
            Self::Hitsound => "hitsound",
            Self::Killsound => "killsound",
        }
    }

    fn setting_key(self) -> String {
        format!("installed_{}_folder", self.name())
    }

    /// `tf/custom/tf2tools-sounds/sound/ui/<kind>.wav`.
    fn wav_target(self, custom: &Path) -> PathBuf {
        custom
            .join(SOUNDS_DIR)
            .join("sound/ui")
            .join(format!("{}.wav", self.name()))
    }

    /// `tf/custom/tf2tools-<kind>.vpk`.
    fn vpk_target(self, custom: &Path) -> PathBuf {
        custom.join(format!("tf2tools-{}.vpk", self.name()))
    }
}

/// What `tf/custom` holds for the hitsound and killsound, plus other folders' sounds.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct InstalledSounds {
    hitsound: InstalledSound,
    killsound: InstalledSound,
    conflicts: Vec<SoundConflict>,
}

impl InstalledSounds {
    /// The installed state of one kind.
    pub fn slot(&self, kind: SoundKind) -> &InstalledSound {
        match kind {
            SoundKind::Hitsound => &self.hitsound,
            SoundKind::Killsound => &self.killsound,
        }
    }

    /// Other `tf/custom` folders that ship a hitsound or killsound and may override ours,
    /// sorted by folder, then hitsound before killsound.
    pub fn conflicts(&self) -> &[SoundConflict] {
        &self.conflicts
    }
}

/// One kind's installed files and the library sound they came from.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct InstalledSound {
    wav: bool,
    vpk: bool,
    managed: Option<SoundId>,
}

impl InstalledSound {
    /// The form TF2 plays: the VPK wins when both exist, as in v5. `None` when empty.
    pub fn active(&self) -> Option<SoundFormat> {
        if self.vpk {
            Some(SoundFormat::Vpk)
        } else if self.wav {
            Some(SoundFormat::Wav)
        } else {
            None
        }
    }

    /// The library sound recorded as installed, when the slot is not empty.
    pub fn managed(&self) -> Option<&SoundId> {
        self.managed.as_ref()
    }
}

/// A folder in `tf/custom` other than ours that ships `sound/ui/<kind>.wav`.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct SoundConflict {
    folder_name: String,
    kind: SoundKind,
}

impl SoundConflict {
    /// The conflicting folder's name.
    pub fn folder_name(&self) -> &str {
        &self.folder_name
    }

    /// Which sound it would override.
    pub fn kind(&self) -> SoundKind {
        self.kind
    }
}

/// How a library sound is stored, v5's `sound_type`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum SoundFormat {
    /// A single TF2-ready WAV file.
    Wav,
    /// A VPK sound pack.
    Vpk,
}

impl SoundFormat {
    fn as_str(self) -> &'static str {
        match self {
            Self::Wav => "wav",
            Self::Vpk => "vpk",
        }
    }

    /// v5 defaults the column to `'wav'`, so any other text reads as a WAV.
    fn from_column(text: &str) -> Self {
        if text == "vpk" { Self::Vpk } else { Self::Wav }
    }
}

/// A sound in the library, read from `library_sounds`.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct SoundEntry {
    id: SoundId,
    folder_name: String,
    name: String,
    original_filename: String,
    format: SoundFormat,
    provenance: Provenance,
    created_at: String,
}

impl SoundEntry {
    /// The library id.
    pub fn id(&self) -> &SoundId {
        &self.id
    }

    /// The folder under `library/sounds/`.
    pub fn folder_name(&self) -> &str {
        &self.folder_name
    }

    /// The display name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The imported file's name, kept inside the folder.
    pub fn original_filename(&self) -> &str {
        &self.original_filename
    }

    /// Whether the sound is a WAV or a VPK pack.
    pub fn format(&self) -> SoundFormat {
        self.format
    }

    /// Where the sound came from; a GameBanana author always reads back empty.
    pub fn provenance(&self) -> &Provenance {
        &self.provenance
    }

    /// When the sound was imported, as v5's RFC 3339 text.
    pub fn created_at(&self) -> &str {
        &self.created_at
    }
}

fn sound_from_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<SoundEntry> {
    Ok(SoundEntry {
        id: SoundId::new(r.get::<_, String>("id")?),
        folder_name: r.get("folder_name")?,
        name: r.get("name")?,
        original_filename: r.get("original_filename")?,
        format: SoundFormat::from_column(&r.get::<_, String>("sound_type")?),
        provenance: Provenance::from_columns(None, r.get("store_gb_id")?, ""),
        created_at: r.get("created_at")?,
    })
}

impl Library {
    /// Every library sound, ordered by display name.
    pub fn list_sounds(&self) -> Result<Vec<SoundEntry>, LibraryError> {
        self.db
            .read(|conn| {
                let mut stmt = conn.prepare(
                    "SELECT id, folder_name, name, original_filename, store_gb_id, sound_type,
                         created_at
                     FROM library_sounds ORDER BY name",
                )?;
                stmt.query_map([], sound_from_row)?.collect()
            })
            .map_err(library_db)
    }

    /// Imports a `.wav` (which must be TF2-playable) or a `.vpk` (which must carry the VPK
    /// magic) into `library/sounds/<slug>-<id8>/` and records it. Curated provenance is
    /// refused, since only HUDs come from the curated store.
    pub fn import_sound(
        &self,
        path: &Path,
        name: &str,
        provenance: Provenance,
    ) -> Result<SoundEntry, ImportError> {
        let store_gb_id = match &provenance {
            Provenance::Local => None,
            Provenance::GameBanana { gb_id, .. } => Some(gb_id_column(*gb_id)?),
            Provenance::Curated { .. } => return Err(ImportError::UnsupportedProvenance),
        };
        let unsupported = || ImportError::UnsupportedFile {
            path: path.to_path_buf(),
        };
        let ext = path
            .extension()
            .map(|e| e.to_string_lossy().to_lowercase())
            .unwrap_or_default();
        let format = match ext.as_str() {
            "vpk" => {
                check_vpk_magic(path)?;
                SoundFormat::Vpk
            }
            "wav" => {
                inspect_wav(&std::fs::read(path).map_err(io_at(path))?)?;
                SoundFormat::Wav
            }
            _ => return Err(unsupported()),
        };
        let original_filename = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .filter(|n| is_plain_name(n))
            .ok_or_else(unsupported)?;

        let id = new_id();
        let folder = folder_name(name, &id);
        let dest = self.dir.library_dir(LibraryKind::Sounds).join(&folder);
        let copied = create_dir_all(&dest).and_then(|()| {
            let target = dest.join(&original_filename);
            std::fs::copy(path, &target)
                .map(drop)
                .map_err(io_at(&target))
        });
        if let Err(failure) = copied {
            let _ = remove_dir_if_present(&dest);
            return Err(failure.into());
        }

        let entry = SoundEntry {
            id: SoundId::new(id),
            folder_name: folder,
            name: name.to_owned(),
            original_filename,
            format,
            // Sounds keep no author column, so the entry matches what a list reads back.
            provenance: Provenance::from_columns(None, store_gb_id, ""),
            created_at: now(),
        };
        let source = if store_gb_id.is_some() {
            "store"
        } else {
            "local"
        };
        let recorded = self.db.write(|conn| {
            conn.execute(
                "INSERT INTO library_sounds (id, folder_name, name, original_filename, source,
                     store_gb_id, sound_type, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![
                    entry.id.as_str(),
                    entry.folder_name,
                    entry.name,
                    entry.original_filename,
                    source,
                    store_gb_id,
                    format.as_str(),
                    entry.created_at
                ],
            )
        });
        if let Err(e) = recorded {
            let _ = remove_dir_if_present(&dest);
            return Err(import_db(e));
        }
        Ok(entry)
    }

    /// Changes a sound's display name; its folder keeps the name it was imported under.
    pub fn rename_sound(&self, id: &SoundId, name: &str) -> Result<(), LibraryError> {
        let changed = self
            .db
            .write(|conn| {
                conn.execute(
                    "UPDATE library_sounds SET name = ?1 WHERE id = ?2",
                    [name, id.as_str()],
                )
            })
            .map_err(library_db)?;
        if changed == 0 {
            return Err(sound_not_found(id));
        }
        Ok(())
    }

    /// Removes a sound's library folder and row. With a TF2 install, a sound that is the
    /// installed hitsound or killsound also leaves the game: its installed file and that
    /// install setting go, and the active profile is cleared.
    pub fn delete_sound(&self, id: &SoundId, tf2: Option<&Tf2Install>) -> Result<(), LibraryError> {
        let (folder, format) = self.sound_folder(id)?;
        let installed_kinds: Vec<SoundKind> = match tf2 {
            Some(_) => self
                .db
                .read(|conn| {
                    let mut kinds = Vec::new();
                    for kind in SoundKind::ALL {
                        if read_setting(conn, &kind.setting_key())?.as_deref() == Some(&folder) {
                            kinds.push(kind);
                        }
                    }
                    Ok(kinds)
                })
                .map_err(library_db)?,
            None => Vec::new(),
        };
        if let Some(tf2) = tf2 {
            let custom = tf2.custom_dir();
            for kind in &installed_kinds {
                remove_file_if_present(&match format {
                    SoundFormat::Wav => kind.wav_target(&custom),
                    SoundFormat::Vpk => kind.vpk_target(&custom),
                })?;
            }
        }
        // The row goes before the library folder, so a failed removal never leaves a row
        // pointing at missing files. A folder name that is not plain has no files.
        self.db
            .write(|conn| {
                let tx = conn.transaction()?;
                for kind in &installed_kinds {
                    save_setting(&tx, &kind.setting_key(), "")?;
                    save_setting(&tx, "active_profile_id", "")?;
                }
                tx.execute("DELETE FROM library_sounds WHERE id = ?1", [id.as_str()])?;
                tx.commit()
            })
            .map_err(library_db)?;
        if is_plain_name(&folder) {
            remove_dir_if_present(&self.dir.library_dir(LibraryKind::Sounds).join(&folder))?;
        }
        Ok(())
    }

    /// The WAV bytes to play: the sound's own WAV, or the first WAV inside its VPK.
    pub fn sound_bytes(&self, id: &SoundId) -> Result<Vec<u8>, LibraryError> {
        let (folder, format) = self.sound_folder(id)?;
        let file = self
            .sound_file(&folder, format)?
            .ok_or_else(|| LibraryError::NotFound {
                what: format!("the {} file of sound {id}", format.as_str()),
            })?;
        match format {
            SoundFormat::Wav => Ok(std::fs::read(&file).map_err(io_at(&file))?),
            SoundFormat::Vpk => Ok(first_wav_in_vpk(&file)?),
        }
    }

    /// Installs a library sound as the hitsound or killsound, ported from v5: a WAV goes to
    /// `tf2tools-sounds/sound/ui/<kind>.wav`, a VPK to `tf2tools-<kind>.vpk`, the other form
    /// of that kind is removed, and `sound.cache` is deleted so TF2 rebuilds it.
    pub fn install_sound(
        &self,
        id: &SoundId,
        kind: SoundKind,
        tf2: &Tf2Install,
    ) -> Result<(), InstallError> {
        let not_in_library = || InstallError::NotInLibrary {
            what: format!("sound {id}"),
        };
        let (folder, format) = self.sound_folder(id).map_err(|e| match e {
            LibraryError::Database { message } => InstallError::Database { message },
            LibraryError::Io { .. }
            | LibraryError::NotFound { .. }
            | LibraryError::InvalidWeaponScript { .. }
            | LibraryError::Vpk(_)
            | LibraryError::Vtf(_) => not_in_library(),
        })?;
        let src = self
            .sound_file(&folder, format)?
            .ok_or_else(not_in_library)?;

        let custom = game_custom_dir(tf2)?;
        let (target, other) = match format {
            SoundFormat::Wav => (kind.wav_target(&custom), kind.vpk_target(&custom)),
            SoundFormat::Vpk => (kind.vpk_target(&custom), kind.wav_target(&custom)),
        };
        remove_file_if_present(&other)?;
        if let Some(parent) = target.parent() {
            create_dir_all(parent)?;
        }
        std::fs::copy(&src, &target).map_err(io_at(&target))?;
        remove_file_if_present(&custom.join(SOUNDS_DIR).join("sound.cache"))?;

        self.db
            .write(|conn| {
                let tx = conn.transaction()?;
                save_setting(&tx, &kind.setting_key(), &folder)?;
                save_setting(&tx, "active_profile_id", "")?;
                tx.commit()
            })
            .map_err(install_db)
    }

    /// Removes both installed forms of one kind and clears its install settings.
    pub fn uninstall_sound(&self, kind: SoundKind, tf2: &Tf2Install) -> Result<(), InstallError> {
        let custom = game_custom_dir(tf2)?;
        remove_file_if_present(&kind.wav_target(&custom))?;
        remove_file_if_present(&kind.vpk_target(&custom))?;
        self.db
            .write(|conn| {
                let tx = conn.transaction()?;
                save_setting(&tx, &kind.setting_key(), "")?;
                save_setting(&tx, "active_profile_id", "")?;
                tx.commit()
            })
            .map_err(install_db)
    }

    /// Scans `tf/custom` for the installed hitsound and killsound, which library sound
    /// each is, and other folders whose sounds may override them.
    pub fn installed_sounds(&self, tf2: &Tf2Install) -> Result<InstalledSounds, LibraryError> {
        let custom = tf2.custom_dir();
        let slot = |kind: SoundKind| -> Result<InstalledSound, LibraryError> {
            let wav = kind.wav_target(&custom).is_file();
            let vpk = kind.vpk_target(&custom).is_file();
            let managed = if wav || vpk {
                self.managed_sound(kind)?
            } else {
                None
            };
            Ok(InstalledSound { wav, vpk, managed })
        };
        Ok(InstalledSounds {
            hitsound: slot(SoundKind::Hitsound)?,
            killsound: slot(SoundKind::Killsound)?,
            conflicts: scan_sound_conflicts(&custom)?,
        })
    }

    /// The installed kind's WAV bytes to play: the first WAV in its VPK when one is
    /// installed, else its WAV. `NotFound` when the slot is empty.
    pub fn installed_sound_bytes(
        &self,
        kind: SoundKind,
        tf2: &Tf2Install,
    ) -> Result<Vec<u8>, LibraryError> {
        let custom = tf2.custom_dir();
        let vpk = kind.vpk_target(&custom);
        if vpk.is_file() {
            return Ok(first_wav_in_vpk(&vpk)?);
        }
        let wav = kind.wav_target(&custom);
        if wav.is_file() {
            return Ok(std::fs::read(&wav).map_err(io_at(&wav))?);
        }
        Err(LibraryError::NotFound {
            what: format!("an installed {}", kind.name()),
        })
    }

    /// The library sound whose folder the kind's install setting names, if any.
    fn managed_sound(&self, kind: SoundKind) -> Result<Option<SoundId>, LibraryError> {
        self.db
            .read(|conn| {
                let Some(folder) = read_setting(conn, &kind.setting_key())? else {
                    return Ok(None);
                };
                conn.query_row(
                    "SELECT id FROM library_sounds WHERE folder_name = ?1 ORDER BY id LIMIT 1",
                    [folder],
                    |r| r.get::<_, String>(0),
                )
                .optional()
            })
            .map(|id| id.map(SoundId::new))
            .map_err(library_db)
    }

    /// A sound's folder name and format, or `NotFound` for an unknown id.
    fn sound_folder(&self, id: &SoundId) -> Result<(String, SoundFormat), LibraryError> {
        self.db
            .read(|conn| {
                conn.query_row(
                    "SELECT folder_name, sound_type FROM library_sounds WHERE id = ?1",
                    [id.as_str()],
                    |r| Ok((r.get(0)?, SoundFormat::from_column(&r.get::<_, String>(1)?))),
                )
                .optional()
            })
            .map_err(library_db)?
            .ok_or_else(|| sound_not_found(id))
    }

    /// The first file in a sound's library folder with its format's extension, compared
    /// case-insensitively and sorted by name. A missing or unsafe folder holds none.
    fn sound_file(&self, folder: &str, format: SoundFormat) -> Result<Option<PathBuf>, IoFailure> {
        if !is_plain_name(folder) {
            return Ok(None);
        }
        let dir = self.dir.library_dir(LibraryKind::Sounds).join(folder);
        let entries = match std::fs::read_dir(&dir) {
            Ok(entries) => entries,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(io_at(&dir)(e)),
        };
        let mut matches = Vec::new();
        for entry in entries {
            let path = entry.map_err(io_at(&dir))?.path();
            let wanted = path
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case(format.as_str()));
            if wanted && path.is_file() {
                matches.push(path);
            }
        }
        Ok(matches.into_iter().min())
    }
}

fn sound_not_found(id: &SoundId) -> LibraryError {
    LibraryError::NotFound {
        what: format!("sound {id}"),
    }
}

/// Folders directly under `custom`, other than ours, that ship `sound/ui/<kind>.wav`, ported
/// from v5's `scan_sound_conflicts`. A missing `custom` has none.
fn scan_sound_conflicts(custom: &Path) -> Result<Vec<SoundConflict>, IoFailure> {
    let entries = match std::fs::read_dir(custom) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(io_at(custom)(e)),
    };
    let mut folders = Vec::new();
    for entry in entries {
        let path = entry.map_err(io_at(custom))?.path();
        if let Some(name) = path.file_name().map(|n| n.to_string_lossy().into_owned())
            && name != SOUNDS_DIR
            && path.is_dir()
        {
            folders.push((name, path));
        }
    }
    folders.sort();
    let mut conflicts = Vec::new();
    for (folder_name, path) in folders {
        for kind in SoundKind::ALL {
            if path
                .join("sound/ui")
                .join(format!("{}.wav", kind.name()))
                .is_file()
            {
                conflicts.push(SoundConflict {
                    folder_name: folder_name.clone(),
                    kind,
                });
            }
        }
    }
    Ok(conflicts)
}
