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
    /// Each pictureless setting's reason key, by module.
    no_picture: HashMap<String, String>,
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
        let mut no_picture = HashMap::new();
        // A no-picture setting keeps only its reason; otherwise only real files count.
        for setting in &manifest.settings {
            if let Some(reason) = &setting.no_picture {
                no_picture.insert(setting.module.clone(), reason.clone());
                continue;
            }
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
            no_picture,
        }
    }

    /// Picture or SameAs only; NoPicture, Placeholder and unknown ids are None.
    pub fn get(&self, module: &str, value: &str) -> Option<&ValuePicture> {
        self.modules.get(module)?.get(value)
    }

    /// Why `module` has no pictures, as a core reason key; None for a module with pictures
    /// or outside the manifest.
    pub fn no_picture(&self, module: &str) -> Option<&str> {
        self.no_picture.get(module).map(String::as_str)
    }

    pub fn has_pictures(&self, module: &str) -> bool {
        self.modules.contains_key(module)
    }

    /// The image key URL for a bundled picture, versioned by the manifest.
    pub fn url(&self, path: &Path) -> String {
        local_image_url(path, &self.version)
    }
}

/// The player-facing line for a no-picture reason key; None for a key the app doesn't know.
pub fn no_picture_note(reason: &str) -> Option<SharedString> {
    Some(match reason {
        "performance-only" => tr!("graphics.pictures.no_picture.performance_only"),
        "same-in-still" => tr!("graphics.pictures.no_picture.same_in_still"),
        "needs-item" => tr!("graphics.pictures.no_picture.needs_item"),
        "not-on-local-server" => tr!("graphics.pictures.no_picture.not_on_local_server"),
        "needs-motion" => tr!("graphics.pictures.no_picture.needs_motion"),
        _ => return None,
    })
}

/// Whose choices an editor shows, which names the compare view's other side: the Graphics
/// page shows the player's own, the Profiles editor a profile's.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ChoicesOwner {
    #[default]
    Player,
    Profile,
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

impl Compare {
    /// The pictures drawn, preset's side first.
    pub fn pictures(&self) -> Vec<PathBuf> {
        match self {
            Compare::None => Vec::new(),
            Compare::Same { picture, .. } | Compare::Single { picture, .. } => {
                vec![picture.clone()]
            }
            Compare::Wipe { preset, yours, .. } => vec![preset.clone(), yours.clone()],
        }
    }
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
    /// The zoom toggle shows: the compared pair can crop (see `pair_has_zoom`).
    pub has_zoom: bool,
}

/// Whether the compare view can zoom: there is a side with a picture and every such side
/// has a crop, so a crop never wipes against a whole shot.
pub fn pair_has_zoom(preset: Option<&ValuePicture>, yours: Option<&ValuePicture>) -> bool {
    let mut sides = [preset, yours].into_iter().flatten().peekable();
    sides.peek().is_some() && sides.all(|p| p.zoom.is_some())
}

/// The preset's and the player's compare pictures, cropped under `zoom` when `pair_has_zoom`.
pub fn compared_pair(
    preset: Option<&ValuePicture>,
    yours: Option<&ValuePicture>,
    zoom: bool,
) -> (Option<PathBuf>, Option<PathBuf>) {
    let cropped = zoom && pair_has_zoom(preset, yours);
    let pick = |p: &ValuePicture| match (&p.zoom, cropped) {
        (Some(crop), true) => crop.clone(),
        _ => p.full.clone(),
    };
    (preset.map(pick), yours.map(pick))
}

/// None when no value of the row's module has a picture. Thumb and strip use full pictures;
/// `zoom` swaps the compare pictures for crops when every compared side has one (see `compared_pair`).
pub fn row_pictures(
    index: &PictureIndex,
    row: &RowView,
    zoom: bool,
    owner: ChoicesOwner,
) -> Option<RowPictures> {
    if !index.has_pictures(&row.module) {
        return None;
    }
    let picture = |choice: &ModuleChoice| match choice {
        ModuleChoice::Value(value) => index.get(&row.module, value),
        ModuleChoice::NotSet | _ => None,
    };
    let label = |choice: &ModuleChoice| {
        row.options
            .iter()
            .find(|o| o.choice == *choice)
            .map(|o| o.label.clone())
            .unwrap_or_default()
    };
    let yours_pill = || match owner {
        ChoicesOwner::Player => tr!("graphics.pictures.yours", value = label(&row.shown)),
        ChoicesOwner::Profile => tr!("graphics.pictures.this_profile", value = label(&row.shown)),
    };
    // The same value on both sides shows once; otherwise the player's pick wipes over the
    // preset's, or whichever side has a picture stands alone.
    let (preset, yours) = (picture(&row.preset_value), picture(&row.shown));
    let compare = match compared_pair(preset, yours, zoom) {
        (Some(picture), Some(_)) if row.shown == row.preset_value => Compare::Same {
            picture,
            pill: tr!(
                "graphics.pictures.same",
                value = label(&row.shown),
                preset = row.preset_name
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
        has_zoom: pair_has_zoom(preset, yours),
    })
}
