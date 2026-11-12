//! The game's killfeed icons for logs.tf weapon names: every `d_` icon the HUD tables draw,
//! cut from its sheet once per found `tf` folder on the background pool.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use std::time::Instant;

use gpui_kit::{App, Entity, RenderImage};
use tf2tools_core::{HudFiles, Platform, RgbaImage};

use crate::folder_load::{FolderLoad, FolderLoader};
use crate::hud_textures::{crop, texture_pixels};
use crate::previews::to_render_image;
use crate::services::Services;

/// Kill icons keyed by the lowercase weapon name behind `d_`.
#[derive(Default, Clone)]
pub struct KillIconImages(HashMap<String, Arc<RenderImage>>);

impl KillIconImages {
    /// Wraps `map`, lowercasing its keys.
    pub fn from_map(map: HashMap<String, Arc<RenderImage>>) -> Self {
        Self(
            map.into_iter()
                .map(|(name, image)| (name.to_lowercase(), image))
                .collect(),
        )
    }

    /// The icon for a logs.tf weapon name, ignoring case.
    pub fn get(&self, log_name: &str) -> Option<&Arc<RenderImage>> {
        self.0.get(&log_name.to_lowercase())
    }

    /// How many icons there are.
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Whether there are none.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

/// The session's kill icons, loaded once per found `tf` folder.
pub type LoadedKillIcons = FolderLoad<KillIconImages>;
/// Reads a `tf` folder's kill icons on the background pool.
pub type IconsLoader = FolderLoader<KillIconImages>;

impl FolderLoad<KillIconImages> {
    /// The session's icons on the production loader, created on first use. Needs `Services`.
    pub fn global(cx: &mut App) -> Entity<Self> {
        Self::global_or(cx, || Arc::new(read_kill_icons))
    }

    /// The icons shown now: none before a load lands or without a game.
    pub fn icons(&self) -> &KillIconImages {
        self.value()
    }
}

// The production loader. Each sheet decodes once and serves every icon on it; an unreadable
// game or sheet leaves those names without icons. Icons dropped on a folder change are not
// freed from the GPU atlas, which is rare enough to accept.
fn read_kill_icons(_: &Services, tf_dir: &Path) -> KillIconImages {
    let started = Instant::now();
    let platform = Platform::current().unwrap_or(Platform::Linux);
    let opened =
        HudFiles::open_game(tf_dir).and_then(|files| Ok((files.kill_icons(platform)?, files)));
    let (table, files) = match opened {
        Ok(opened) => opened,
        Err(err) => {
            tracing::warn!(%err, "could not read the game's kill icons");
            return KillIconImages::default();
        }
    };
    let mut sheets: HashMap<String, Option<Arc<RgbaImage>>> = HashMap::new();
    let mut map = HashMap::new();
    for name in table.names() {
        let weapon = name
            .get(..2)
            .filter(|p| p.eq_ignore_ascii_case("d_"))
            .and(name.get(2..));
        let (Some(weapon), Some(icon)) = (weapon, table.get(&name)) else {
            continue;
        };
        let sheet = sheets
            .entry(icon.image().to_lowercase())
            .or_insert_with(|| {
                let pixels = texture_pixels(&files, icon.image(), platform);
                if pixels.is_none() {
                    tracing::warn!(sheet = icon.image(), "could not decode a kill icon sheet");
                }
                pixels
            });
        let image = sheet
            .as_deref()
            .and_then(|sheet| crop(sheet, &icon))
            .and_then(|cut| to_render_image(cut.width(), cut.height(), cut.into_pixels()));
        if let Some(image) = image {
            map.insert(weapon.to_lowercase(), image);
        }
    }
    tracing::debug!(
        icons = map.len(),
        elapsed_ms = started.elapsed().as_millis(),
        "read the game's kill icons"
    );
    KillIconImages(map)
}
