//! The session's game weapon names for logs.tf: the found TF2's English names, read once per
//! `tf` folder on the background pool.

use std::path::Path;
use std::sync::Arc;

use gpui_kit::{App, Entity};
use tf2tools_core::WeaponNames;

use crate::folder_load::{FolderLoad, FolderLoader};
use crate::services::Services;

/// The session's weapon names, loaded once per found `tf` folder.
pub type LoadedWeaponNames = FolderLoad<WeaponNames>;
/// Reads a `tf` folder's weapon names on the background pool.
pub type NamesLoader = FolderLoader<WeaponNames>;

// The production loader: core's read; a failed read leaves every log name raw.
fn read_names(_: &Services, tf_dir: &Path) -> WeaponNames {
    WeaponNames::load(tf_dir).unwrap_or_else(|err| {
        tracing::warn!(%err, "could not read the game's weapon names");
        WeaponNames::default()
    })
}

impl FolderLoad<WeaponNames> {
    /// The session's names on the production loader, created on first use. Needs `Services`.
    pub fn global(cx: &mut App) -> Entity<Self> {
        Self::global_or(cx, || Arc::new(read_names))
    }

    /// The names shown now: empty before a load lands or without a game.
    pub fn names(&self) -> &WeaponNames {
        self.value()
    }
}
