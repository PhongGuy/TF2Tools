//! The crosshair weapons' backpack icons, decoded once from the game's texture pak and kept
//! as small PNGs named by the texture's crc and length, so a game update makes new files.

use std::collections::{BTreeMap, HashSet};
use std::io::Read as _;
use std::path::{Path, PathBuf};

use crate::error::WeaponIconsError;
use crate::pak::{open_vpk, read_entry};
use crate::texture::{RgbaImage, as_buffer, decode_vtf, stretch, write_png};
use crate::weapon_inventory_images;

const ITEMS_GAME_MAX_BYTES: u64 = 64 * 1024 * 1024;
const ICON_MAX_SIDE: u32 = 128;

/// The game's backpack icons for the crosshair weapons, as PNGs in a cache folder.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct GameWeaponIcons {
    icons: BTreeMap<&'static str, PathBuf>,
    decoded: usize,
}

impl GameWeaponIcons {
    /// Reads `<tf_dir>/scripts/items/items_game.txt` and `<tf_dir>/tf2_textures_dir.vpk` and
    /// makes sure `cache_dir` holds a PNG, at most 128 px a side, for every weapon the game
    /// pictures; a PNG already there for the same texture is reused. Removes every other
    /// `.png` in `cache_dir`. Blocking.
    ///
    /// Fails with `ItemsGame` or `Textures` when either game file cannot be read, and with
    /// `Cache` when the folder or a PNG cannot be written. A weapon whose item or texture is
    /// missing or undecodable is left out instead.
    pub fn build(tf_dir: &Path, cache_dir: &Path) -> Result<GameWeaponIcons, WeaponIconsError> {
        let images = weapon_inventory_images(&read_items_game(
            &tf_dir.join("scripts/items/items_game.txt"),
        )?);
        let pak_path = tf_dir.join("tf2_textures_dir.vpk");
        let pak = open_vpk(&pak_path).map_err(|source| WeaponIconsError::Textures { source })?;
        std::fs::create_dir_all(cache_dir).map_err(|e| cache_error(cache_dir, &e))?;

        let mut icons = BTreeMap::new();
        let mut decoded = 0;
        for (script, image) in images {
            let Some((name, entry)) = find_texture(&pak, &image) else {
                continue;
            };
            // The crc and length stand in for the texture's content, so an unchanged
            // texture is reused without reading it from the pak.
            let dir = &entry.dir_entry;
            let length = u64::from(dir.preload_length) + u64::from(dir.file_length);
            let png = cache_dir.join(format!("{script}-{:08x}-{length:x}.png", dir.crc32));
            if !png.is_file() {
                let Some(icon) = read_entry(&pak_path, name, entry)
                    .ok()
                    .and_then(|bytes| decode_vtf(&bytes).ok())
                else {
                    continue;
                };
                write_atomically(&png, &fit(icon))?;
                decoded += 1;
            }
            icons.insert(script, png);
        }
        prune(cache_dir, &icons)?;
        Ok(GameWeaponIcons { icons, decoded })
    }

    /// The cached PNG for weapon script `script`, None when the game has no picture for it.
    pub fn get(&self, script: &str) -> Option<&Path> {
        self.icons.get(script).map(PathBuf::as_path)
    }

    /// How many weapons have a PNG.
    pub fn len(&self) -> usize {
        self.icons.len()
    }

    /// Whether no weapon has a PNG.
    pub fn is_empty(&self) -> bool {
        self.icons.is_empty()
    }

    /// How many PNGs this build decoded and wrote, as opposed to reused.
    pub fn decoded(&self) -> usize {
        self.decoded
    }
}

fn read_items_game(path: &Path) -> Result<String, WeaponIconsError> {
    let fail = |source| WeaponIconsError::ItemsGame {
        path: path.to_path_buf(),
        source,
    };
    if std::fs::metadata(path).map_err(fail)?.len() > ITEMS_GAME_MAX_BYTES {
        return Err(fail(std::io::Error::other("the file is over 64 MiB")));
    }
    // The take guards a file that grew after the size check.
    let mut bytes = Vec::new();
    std::fs::File::open(path)
        .and_then(|f| f.take(ITEMS_GAME_MAX_BYTES).read_to_end(&mut bytes))
        .map_err(fail)?;
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

/// The `_large` backpack texture for `image`, else its plain one, matched ignoring case: an
/// exact lowercase key first, then a scan for paks that store mixed case.
fn find_texture<'a>(pak: &'a vpk::VPK, image: &str) -> Option<(&'a str, &'a vpk::entry::VPKEntry)> {
    [
        format!("materials/{image}_large.vtf"),
        format!("materials/{image}.vtf"),
    ]
    .into_iter()
    .find_map(|key| {
        let key = key.to_ascii_lowercase();
        pak.tree.get_key_value(&key).or_else(|| {
            pak.tree
                .iter()
                .find(|(name, _)| name.eq_ignore_ascii_case(&key))
        })
    })
    .map(|(name, entry)| (name.as_str(), entry))
}

/// `icon` shrunk to fit inside 128x128 keeping its shape; a smaller picture is kept as is.
fn fit(icon: RgbaImage) -> RgbaImage {
    let (w, h) = (icon.width(), icon.height());
    let longer = w.max(h);
    if longer <= ICON_MAX_SIDE {
        return icon;
    }
    let scale = |side: u32| {
        let scaled = u64::from(side) * u64::from(ICON_MAX_SIDE) / u64::from(longer);
        u32::try_from(scaled).unwrap_or(ICON_MAX_SIDE).max(1)
    };
    stretch(&as_buffer(&icon), scale(w), scale(h))
}

/// Writes under a process-unique temporary name and renames, so a crash or a second app
/// never leaves half a PNG under the final name.
fn write_atomically(png: &Path, icon: &RgbaImage) -> Result<(), WeaponIconsError> {
    let mut tmp = png.as_os_str().to_owned();
    tmp.push(format!(".{}.tmp", std::process::id()));
    let tmp = PathBuf::from(tmp);
    write_png(&tmp, icon).map_err(|e| WeaponIconsError::Cache {
        path: tmp.clone(),
        message: e.to_string(),
    })?;
    std::fs::rename(&tmp, png).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        cache_error(png, &e)
    })
}

/// Removes every `.png` in `cache_dir` that this build did not keep.
fn prune(
    cache_dir: &Path,
    icons: &BTreeMap<&'static str, PathBuf>,
) -> Result<(), WeaponIconsError> {
    let keep: HashSet<&Path> = icons.values().map(PathBuf::as_path).collect();
    let entries = std::fs::read_dir(cache_dir).map_err(|e| cache_error(cache_dir, &e))?;
    for entry in entries {
        let path = entry.map_err(|e| cache_error(cache_dir, &e))?.path();
        let is_png = path
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("png"));
        if !is_png || keep.contains(path.as_path()) {
            continue;
        }
        // Another app pruning the same folder may have removed it first.
        match std::fs::remove_file(&path) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => {
                return Err(cache_error(&path, &e));
            }
            _ => {}
        }
    }
    Ok(())
}

fn cache_error(path: &Path, error: &std::io::Error) -> WeaponIconsError {
    WeaponIconsError::Cache {
        path: path.to_path_buf(),
        message: error.to_string(),
    }
}
