//! Where core keeps its files, laid out exactly as v5 did so a v5 library carries over.

use std::path::{Path, PathBuf};

use crate::error::DataDirError;

/// Root of everything core writes outside TF2: `tf2tools.db`, `library/<kind>/`, `tmp/`.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct DataDir {
    root: PathBuf,
    image_cache: PathBuf,
}

impl DataDir {
    /// The per-user data directory joined with v5's Tauri identifier `com.tf2tools.app`.
    ///
    /// Fails when the platform reports no data directory.
    pub fn platform_default() -> Result<Self, DataDirError> {
        let base = dirs::data_dir().ok_or(DataDirError::NoPlatformDataDir)?;
        let dir = Self::at(base.join("com.tf2tools.app"));
        // Images are disposable, so they go to the user cache dir when the platform has one.
        Ok(match dirs::cache_dir() {
            Some(cache) => dir.with_cache_home(cache),
            None => dir,
        })
    }

    /// This data directory with its image cache under `cache_home` (an `XDG_CACHE_HOME`), at
    /// `<cache_home>/tf2tools/images`, as `platform_default` lays it out. Touches no files.
    pub fn with_cache_home(mut self, cache_home: impl AsRef<Path>) -> Self {
        self.image_cache = cache_home.as_ref().join("tf2tools").join("images");
        self
    }

    /// A data directory rooted at `root`. Touches no files.
    pub fn at(root: impl Into<PathBuf>) -> Self {
        let root = root.into();
        let image_cache = root.join("cache").join("images");
        Self { root, image_cache }
    }

    /// The root directory.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// The SQLite database file, `<root>/tf2tools.db`.
    pub fn db_path(&self) -> PathBuf {
        self.root.join("tf2tools.db")
    }

    /// The library directory for one kind, `<root>/library/<kind>`.
    pub fn library_dir(&self, kind: LibraryKind) -> PathBuf {
        let name = match kind {
            LibraryKind::Huds => "huds",
            LibraryKind::Sounds => "sounds",
            LibraryKind::Crosshairs => "crosshairs",
            LibraryKind::Addons => "addons",
        };
        self.root.join("library").join(name)
    }

    /// Saved HUD Maker presets, `<root>/hud_presets`. Touches no files.
    pub fn hud_presets_dir(&self) -> PathBuf {
        self.root.join("hud_presets")
    }

    /// mastercomfig's module catalogs, `<root>/mastercomfig`, one folder per release tag.
    /// Touches no files.
    pub fn mastercomfig_dir(&self) -> PathBuf {
        self.root.join("mastercomfig")
    }

    /// GameBanana add-on copies, `<root>/library/gb-addons`. Touches no files.
    pub fn gb_addon_dir(&self) -> PathBuf {
        self.root.join("library").join("gb-addons")
    }

    /// Scratch space for downloads and extraction, `<root>/tmp`.
    pub fn tmp_dir(&self) -> PathBuf {
        self.root.join("tmp")
    }

    /// The store image cache: `$XDG_CACHE_HOME/tf2tools/images` from `platform_default`,
    /// `<root>/cache/images` from `at`.
    pub fn image_cache_dir(&self) -> PathBuf {
        self.image_cache.clone()
    }
}

/// A kind of installable item with its own library directory.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum LibraryKind {
    /// HUD packs.
    Huds,
    /// Hitsound and killsound packs.
    Sounds,
    /// Crosshair packs.
    Crosshairs,
    /// Mastercomfig addons.
    Addons,
}

impl LibraryKind {
    /// Every kind, in a stable order.
    pub const ALL: [LibraryKind; 4] = [Self::Huds, Self::Sounds, Self::Crosshairs, Self::Addons];
}
