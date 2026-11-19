//! The lookup behind the Graphics page's setting pictures, and the manifest the
//! shooter reads to take them. Pure data and path logic; no file is decoded here.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::error::SettingPicturesError;

/// The reasons a setting may have no picture, as keys the app turns into text.
pub const REASON_KEYS: [&str; 4] = [
    "performance-only",
    "same-in-still",
    "needs-item",
    "not-on-local-server",
];

/// The reason a `same_as` value carries when the manifest names none.
const DEFAULT_SAME_REASON: &str = "same-in-still";

fn pause_default() -> bool {
    true
}

/// The committed manifest: how every graphics setting is shot and which values share a look.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
#[non_exhaustive]
pub struct PictureManifest {
    /// The manifest format version.
    pub version: u32,
    /// The mastercomfig release the values were taken from.
    pub mastercomfig: String,
    /// The picture size in pixels, width then height.
    pub size: [u32; 2],
    /// The map every shot is taken on unless a setting names its own.
    pub map: String,
    /// The value each module holds while another is being shot.
    pub base: BTreeMap<String, String>,
    /// One entry per module.
    pub settings: Vec<SettingShot>,
}

/// How one module is shot.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
#[non_exhaustive]
pub struct SettingShot {
    /// The mastercomfig module name.
    pub module: String,
    /// A map for this setting only.
    #[serde(default)]
    pub map: Option<String>,
    /// The camera position.
    #[serde(default)]
    pub setpos: Option<[f32; 3]>,
    /// The camera angles.
    #[serde(default)]
    pub setang: Option<[f32; 3]>,
    /// Console commands run once before the values are shot.
    #[serde(default)]
    pub setup: Vec<String>,
    /// Console commands run before each shot, with a wait after each.
    #[serde(default)]
    pub steps: Vec<ShotStep>,
    /// Whether the game is paused for the shot.
    #[serde(default = "pause_default")]
    pub pause: bool,
    /// What the game must reload after a value changes.
    #[serde(default)]
    pub reload: Reload,
    /// Why this setting has no pictures, as one of [`REASON_KEYS`].
    #[serde(default)]
    pub no_picture: Option<String>,
    /// The values, in shooting order.
    pub values: Vec<ValueShot>,
}

/// One console command and how long to wait after it.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
#[non_exhaustive]
pub struct ShotStep {
    /// The console command.
    pub run: String,
    /// The wait after it, in milliseconds.
    pub wait_ms: u64,
}

/// What the game must reload for a changed value to show.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "lowercase")]
#[non_exhaustive]
pub enum Reload {
    /// Nothing; the change shows at once.
    #[default]
    None,
    /// Reload the materials.
    Materials,
    /// Reload the map.
    Map,
    /// Restart the game.
    Restart,
}

/// One value of a setting.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
#[non_exhaustive]
pub struct ValueShot {
    /// The value name.
    pub value: String,
    /// Another value whose picture this one reuses.
    #[serde(default)]
    pub same_as: Option<String>,
    /// Why the look is shared, as one of [`REASON_KEYS`].
    #[serde(default)]
    pub reason: Option<String>,
}

impl PictureManifest {
    /// Reads a manifest from its JSON bytes; anything malformed is `Format`.
    pub fn parse(bytes: &[u8]) -> Result<Self, SettingPicturesError> {
        serde_json::from_slice(bytes).map_err(|e| SettingPicturesError::Format {
            reason: e.to_string(),
        })
    }

    /// The shooting entry for a module, if the manifest has one.
    pub fn setting(&self, module: &str) -> Option<&SettingShot> {
        self.settings.iter().find(|s| s.module == module)
    }
}

/// What the Graphics page shows for one value.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum SettingPicture {
    /// The value's own picture.
    Picture {
        /// The picture file.
        path: PathBuf,
    },
    /// Another value's picture, because the two look the same.
    SameAs {
        /// The picture file of the other value.
        path: PathBuf,
        /// The other value.
        value: String,
        /// Why they look the same, as a reason key.
        reason: String,
    },
    /// The setting has no pictures.
    NoPicture {
        /// Why, as a reason key.
        reason: String,
    },
    /// No picture exists yet.
    Placeholder {
        /// The placeholder file.
        path: PathBuf,
    },
}

/// A folder of setting pictures with its manifest.
#[derive(Debug, Clone)]
pub struct SettingPictures {
    dir: PathBuf,
    manifest: PictureManifest,
}

impl SettingPictures {
    /// Opens a pictures folder by reading `manifest.json` in it.
    pub fn open(dir: &Path) -> Result<Self, SettingPicturesError> {
        let path = dir.join("manifest.json");
        let bytes =
            fs::read(&path).map_err(|source| SettingPicturesError::Read { path, source })?;
        Ok(Self {
            dir: dir.to_owned(),
            manifest: PictureManifest::parse(&bytes)?,
        })
    }

    /// The parsed manifest.
    pub fn manifest(&self) -> &PictureManifest {
        &self.manifest
    }

    /// Where a value's picture lives, whether or not the file exists.
    pub fn file_for(&self, module: &str, value: &str) -> PathBuf {
        self.dir.join(module).join(format!("{value}.webp"))
    }

    /// The stand-in picture for values that have none yet.
    pub fn placeholder(&self) -> PathBuf {
        self.dir.join("placeholder.webp")
    }

    /// What to show for a value. A missing file always falls back to the placeholder.
    pub fn picture(&self, module: &str, value: &str) -> SettingPicture {
        let placeholder = || SettingPicture::Placeholder {
            path: self.placeholder(),
        };
        let Some(setting) = self.manifest.setting(module) else {
            return placeholder();
        };
        let Some(shot) = setting.values.iter().find(|v| v.value == value) else {
            return placeholder();
        };
        if let Some(reason) = &setting.no_picture {
            return SettingPicture::NoPicture {
                reason: reason.clone(),
            };
        }
        // A shared look shows the other value's file, but only once that file exists.
        let (target, same) = match &shot.same_as {
            Some(other) => (other.as_str(), Some(other)),
            None => (value, None),
        };
        let path = self.file_for(module, target);
        if !path.is_file() {
            return placeholder();
        }
        match same {
            Some(other) => SettingPicture::SameAs {
                path,
                value: other.clone(),
                reason: shot
                    .reason
                    .clone()
                    .unwrap_or_else(|| DEFAULT_SAME_REASON.to_owned()),
            },
            None => SettingPicture::Picture { path },
        }
    }
}
