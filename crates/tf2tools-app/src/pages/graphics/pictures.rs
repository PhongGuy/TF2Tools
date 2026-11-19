//! The bundled setting pictures, resolved once into an index so drawing never touches the
//! disk, and what one row shows of them: its thumbnail, its compare view and its strip.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use gpui_kit::SharedString;
use tf2tools_core::{ModuleChoice, SettingPicture, SettingPictures};

use super::view::RowView;
use crate::i18n::tr;
use crate::remote_images::local_image_url;

/// One value's bundled picture and, when its setting has a crop, the zoomed crop.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValuePicture {
    pub full: PathBuf,
    pub zoom: Option<PathBuf>,
}

/// Which values of which modules have a real picture on disk, by module then value.
#[derive(Debug, Clone, Default)]
pub struct PictureIndex {
    version: String,
    modules: HashMap<String, HashMap<String, ValuePicture>>,
}

impl PictureIndex {
    /// Reads `dir/manifest.json` and checks every value's files. Blocking. A folder that
    /// cannot be opened gives an empty index and logs one warning.
    pub fn open(dir: &Path) -> Self {
        let pictures = match SettingPictures::open(dir) {
            Ok(pictures) => pictures,
            Err(e) => {
                tracing::warn!("setting pictures in {}: {e}", dir.display());
                return Self::default();
            }
        };
        let manifest = pictures.manifest();
        let mut modules: HashMap<String, HashMap<String, ValuePicture>> = HashMap::new();
        // Only real files count: a placeholder or a performance-only setting has no picture.
        for setting in &manifest.settings {
            for shot in &setting.values {
                let full = match pictures.picture(&setting.module, &shot.value) {
                    SettingPicture::Picture { path } | SettingPicture::SameAs { path, .. } => path,
                    SettingPicture::NoPicture { .. } | SettingPicture::Placeholder { .. } | _ => {
                        continue;
                    }
                };
                let zoom = pictures.zoom_for(&setting.module, &shot.value);
                modules
                    .entry(setting.module.clone())
                    .or_default()
                    .insert(shot.value.clone(), ValuePicture { full, zoom });
            }
        }
        Self {
            version: manifest.version.to_string(),
            modules,
        }
    }

    /// Picture or SameAs only; NoPicture, Placeholder and unknown ids are None.
    pub fn get(&self, module: &str, value: &str) -> Option<&ValuePicture> {
        self.modules.get(module)?.get(value)
    }

    pub fn has_pictures(&self, module: &str) -> bool {
        self.modules.contains_key(module)
    }

    /// Any value of `module` has a zoom file.
    pub fn has_zoom(&self, module: &str) -> bool {
        self.modules
            .get(module)
            .is_some_and(|values| values.values().any(|p| p.zoom.is_some()))
    }

    /// The image key URL for a bundled picture, versioned by the manifest.
    pub fn url(&self, path: &Path) -> String {
        local_image_url(path, &self.version)
    }
}

/// What the open row's compare view shows.
#[derive(Debug, Clone, PartialEq)]
pub enum Compare {
    None,
    Same {
        picture: PathBuf,
        pill: SharedString,
    },
    /// Only one side has a picture; `yours` says which.
    Single {
        picture: PathBuf,
        pill: SharedString,
        yours: bool,
    },
    Wipe {
        preset: PathBuf,
        yours: PathBuf,
        preset_pill: SharedString,
        yours_pill: SharedString,
    },
}

/// One value in the strip that picks a value by picture.
#[derive(Debug, Clone, PartialEq)]
pub struct StripPicture {
    pub value: String,
    pub label: SharedString,
    pub picture: PathBuf,
    pub preset: bool,
    pub chosen: bool,
}

/// Everything a row draws of its pictures.
#[derive(Debug, Clone, PartialEq)]
pub struct RowPictures {
    pub thumb: Option<PathBuf>,
    pub compare: Compare,
    pub strip: Vec<StripPicture>,
    pub has_zoom: bool,
}

/// None when no value of the row's module has a picture. Thumb and strip use full pictures;
/// `zoom` swaps the compare pictures for crops where the value has one. `preset` is the
/// preset's label for the strip tag; the tag's words are drawn by the panel, so it is unused here.
pub fn row_pictures(
    index: &PictureIndex,
    row: &RowView,
    _preset: &SharedString,
    zoom: bool,
) -> Option<RowPictures> {
    if !index.has_pictures(&row.module) {
        return None;
    }
    let picture = |choice: &ModuleChoice| match choice {
        ModuleChoice::Value(value) => index.get(&row.module, value),
        ModuleChoice::NotSet | _ => None,
    };
    let compared = |choice: &ModuleChoice| {
        picture(choice).map(|p| match (&p.zoom, zoom) {
            (Some(crop), true) => crop.clone(),
            _ => p.full.clone(),
        })
    };
    let label = |choice: &ModuleChoice| {
        row.options
            .iter()
            .find(|o| o.choice == *choice)
            .map(|o| o.label.clone())
            .unwrap_or_default()
    };
    let yours_pill = || tr!("graphics.pictures.yours", value = label(&row.shown));
    // The same value on both sides shows once; otherwise the player's pick wipes over the
    // preset's, or whichever side has a picture stands alone.
    let compare = match (compared(&row.preset_value), compared(&row.shown)) {
        (Some(picture), Some(_)) if row.shown == row.preset_value => Compare::Same {
            picture,
            pill: tr!(
                "graphics.pictures.same",
                value = label(&row.shown),
                preset = row.preset_text
            ),
        },
        (Some(preset), Some(yours)) => Compare::Wipe {
            preset,
            yours,
            preset_pill: row.preset_text.clone(),
            yours_pill: yours_pill(),
        },
        (Some(picture), None) => Compare::Single {
            picture,
            pill: row.preset_text.clone(),
            yours: false,
        },
        (None, Some(picture)) => Compare::Single {
            picture,
            pill: yours_pill(),
            yours: true,
        },
        (None, None) => Compare::None,
    };
    let strip = row
        .options
        .iter()
        .filter_map(|option| {
            let ModuleChoice::Value(value) = &option.choice else {
                return None;
            };
            Some(StripPicture {
                value: value.clone(),
                label: option.label.clone(),
                picture: picture(&option.choice)?.full.clone(),
                preset: option.choice == row.preset_value,
                chosen: option.choice == row.shown,
            })
        })
        .collect();
    Some(RowPictures {
        thumb: picture(&row.shown).map(|p| p.full.clone()),
        compare,
        strip,
        has_zoom: index.has_zoom(&row.module),
    })
}
