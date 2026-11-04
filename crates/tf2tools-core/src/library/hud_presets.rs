//! HUD Maker presets: recipe files of option choices under `<data>/hud_presets/`, each named
//! by a fresh uuid so a player's preset name never becomes a path.

use std::io::{self, Read};
use std::path::{Component, Path, PathBuf};

use super::{IoFailure, Library, io_at};
use crate::error::PresetError;
use crate::hud_maker::{Change, Recipe};

/// Directory entries a listing looks at; the rest are skipped.
const MAX_LISTED: usize = 1000;

/// One saved preset: its name and its file inside `hud_presets/`.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct PresetEntry {
    name: String,
    file: String,
}

impl PresetEntry {
    /// The preset's name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The file name inside `hud_presets/`.
    pub fn file(&self) -> &str {
        &self.file
    }
}

impl Library {
    /// Presets that read as recipes, sorted by name ignoring case; none when the folder is missing.
    pub fn list_presets(&self) -> Vec<PresetEntry> {
        let Ok(entries) = std::fs::read_dir(self.dir.hud_presets_dir()) else {
            return Vec::new();
        };
        // A file that is unreadable, damaged or not ours is left out rather than failing the list.
        let mut presets: Vec<PresetEntry> = entries
            .filter_map(Result::ok)
            .take(MAX_LISTED)
            .filter_map(|entry| {
                let file = entry.file_name().into_string().ok()?;
                let recipe = self.load_preset(&file).ok()?;
                Some(PresetEntry {
                    name: recipe.name().to_owned(),
                    file,
                })
            })
            .collect();
        presets.sort_by_cached_key(|p| (p.name.to_lowercase(), p.file.clone()));
        presets
    }

    /// Saves the `Option` changes of `changes` as a preset named `name` (trimmed).
    ///
    /// # Errors
    /// `NameBlank` for an empty name, `NoOptions` when `changes` holds no option choice,
    /// `NameTaken` when a preset has the name ignoring case, `Io` when the file cannot be written.
    pub fn save_preset(&self, name: &str, changes: &[Change]) -> Result<PresetEntry, PresetError> {
        if name.chars().all(|c| c.is_control() || c.is_whitespace()) {
            return Err(PresetError::NameBlank);
        }
        let recipe = Recipe::preset(name, changes);
        if recipe.changes().is_empty() {
            return Err(PresetError::NoOptions);
        }
        let wanted = recipe.name().to_lowercase();
        if self
            .list_presets()
            .iter()
            .any(|p| p.name.to_lowercase() == wanted)
        {
            return Err(PresetError::NameTaken {
                name: recipe.name().to_owned(),
            });
        }

        // Written beside its final name and renamed in, so a preset is never seen half written.
        let dir = self.dir.hud_presets_dir();
        std::fs::create_dir_all(&dir).map_err(io_at(&dir))?;
        let file = format!("{}.json", uuid::Uuid::new_v4());
        let path = dir.join(&file);
        let tmp = dir.join(format!("{file}.tmp"));
        std::fs::write(&tmp, recipe.to_json()).map_err(io_at(&tmp))?;
        if let Err(source) = std::fs::rename(&tmp, &path) {
            let _ = std::fs::remove_file(&tmp);
            return Err(PresetError::Io { path, source });
        }
        Ok(PresetEntry {
            name: recipe.name().to_owned(),
            file,
        })
    }

    /// The preset in `file`, a plain file name inside `hud_presets/`.
    ///
    /// # Errors
    /// `NotFound` when `file` is not a plain `.json` name or is not a regular file there (a link
    /// is never followed), `Recipe` when the file is not a readable recipe, `Io` when it
    /// cannot be read.
    pub fn load_preset(&self, file: &str) -> Result<Recipe, PresetError> {
        let path = self.preset_path(file)?;
        let bytes = read_capped(&path)
            .map_err(not_found_or_io)?
            .ok_or(PresetError::NotFound)?;
        let (recipe, _skipped) = Recipe::parse(&bytes)?;
        Ok(recipe)
    }

    /// Deletes the preset in `file`, a plain file name inside `hud_presets/`.
    ///
    /// # Errors
    /// `NotFound` when `file` is not a plain `.json` name or is neither a file nor a link there
    /// (a link is removed, not followed), `Io` when it cannot be removed.
    pub fn delete_preset(&self, file: &str) -> Result<(), PresetError> {
        let path = self.preset_path(file)?;
        // A link is removed itself, never followed; anything else that is not a file stays.
        let meta =
            std::fs::symlink_metadata(&path).map_err(|e| not_found_or_io(io_at(&path)(e)))?;
        if !meta.is_file() && !meta.file_type().is_symlink() {
            return Err(PresetError::NotFound);
        }
        std::fs::remove_file(&path).map_err(|e| not_found_or_io(io_at(&path)(e)))
    }

    /// `file` inside `hud_presets/`, only when it is a single plain component ending `.json`,
    /// so no name can reach outside the folder.
    fn preset_path(&self, file: &str) -> Result<PathBuf, PresetError> {
        let mut parts = Path::new(file).components();
        match (parts.next(), parts.next()) {
            (Some(Component::Normal(_)), None) if file.ends_with(".json") => {
                Ok(self.dir.hud_presets_dir().join(file))
            }
            _ => Err(PresetError::NotFound),
        }
    }
}

/// Reads the recipe in the file at `path`, such as one the player picked, and how many of its
/// changes were skipped.
///
/// # Errors
/// `NotFound` when `path` is not a regular file (a link is never followed and a pipe never
/// waited on), `Recipe` when the file is not a readable recipe, `Io` when it cannot be read.
pub fn read_recipe_file(path: &Path) -> Result<(Recipe, usize), PresetError> {
    let bytes = read_capped(path)
        .map_err(not_found_or_io)?
        .ok_or(PresetError::NotFound)?;
    Ok(Recipe::parse(&bytes)?)
}

/// Reads at most one byte past the recipe cap, so an oversized file is refused, not loaded.
/// `None` when the entry is not a regular file: a link is never followed and a pipe never
/// opened, so a listing cannot be led outside the folder or left waiting.
pub(super) fn read_capped(path: &Path) -> Result<Option<Vec<u8>>, IoFailure> {
    if !std::fs::symlink_metadata(path)
        .map_err(io_at(path))?
        .is_file()
    {
        return Ok(None);
    }
    let file = open_plain(path).map_err(io_at(path))?;
    // Checked again on the open handle, in case the entry was swapped after the first look.
    if !file.metadata().map_err(io_at(path))?.is_file() {
        return Ok(None);
    }
    let mut bytes = Vec::new();
    file.take(Recipe::MAX_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(io_at(path))?;
    Ok(Some(bytes))
}

/// Opens `path` for reading without following a final link or waiting on a pipe.
#[cfg(unix)]
fn open_plain(path: &Path) -> io::Result<std::fs::File> {
    use std::os::unix::fs::OpenOptionsExt;
    std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)
}

#[cfg(not(unix))]
fn open_plain(path: &Path) -> io::Result<std::fs::File> {
    std::fs::File::open(path)
}

/// A missing file is `NotFound`; any other I/O failure keeps its path.
fn not_found_or_io(failure: IoFailure) -> PresetError {
    match PresetError::from(failure) {
        PresetError::Io { source, .. } if source.kind() == io::ErrorKind::NotFound => {
            PresetError::NotFound
        }
        other => other,
    }
}
