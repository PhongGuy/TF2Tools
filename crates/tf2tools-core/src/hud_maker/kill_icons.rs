//! Killfeed weapon icons from the game's HUD icon tables.

use crate::error::HudFilesError;
use crate::fonts::Platform;
use crate::fonts::keyvalues::{KvPair, KvValue};

use super::files::HudFiles;

// Searched in this order; only the first holds killfeed icons in the stock game.
const TABLES: [&str; 2] = ["scripts/mod_textures.txt", "scripts/hud_textures.txt"];

/// A region of a texture sheet that the HUD icon table names.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct KillIcon {
    image: String,
    x: u32,
    y: u32,
    wide: u32,
    tall: u32,
}

impl KillIcon {
    /// The sheet as an image value, `../<file>`, for [`HudFiles::image`].
    pub fn image(&self) -> &str {
        &self.image
    }

    /// The region's left edge in texels.
    pub fn x(&self) -> u32 {
        self.x
    }

    /// The region's top edge in texels.
    pub fn y(&self) -> u32 {
        self.y
    }

    /// The region's width in texels.
    pub fn wide(&self) -> u32 {
        self.wide
    }

    /// The region's height in texels.
    pub fn tall(&self) -> u32 {
        self.tall
    }
}

/// The HUD icon tables of one load, read and parsed once; looking up names reads nothing more.
#[non_exhaustive]
pub struct KillIcons {
    tables: Vec<Vec<KvPair>>,
}

impl std::fmt::Debug for KillIcons {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("KillIcons")
            .field("tables", &self.tables.len())
            .finish_non_exhaustive()
    }
}

impl KillIcons {
    /// The icon the tables give `name` (ignoring case), such as `d_scattergun`: each entry
    /// answers to its name behind the prefix of each file key it sets, and a `dneg_` name the
    /// tables lack falls back to its `d_` icon. `None` for an unknown name, a font glyph or an
    /// empty region.
    pub fn get(&self, name: &str) -> Option<KillIcon> {
        let find = |name: &str| self.tables.iter().find_map(|pairs| icon_in(pairs, name));
        let fallback = || {
            let rest = name
                .get(..5)?
                .eq_ignore_ascii_case("dneg_")
                .then(|| &name[5..])?;
            find(&format!("d_{rest}"))
        };
        find(name).or_else(fallback)
    }
}

impl HudFiles {
    /// Reads and parses the HUD icon tables once, for any number of [`KillIcons::get`] lookups;
    /// an absent table counts as empty.
    pub fn kill_icons(&self, platform: Platform) -> Result<KillIcons, HudFilesError> {
        let mut tables = Vec::with_capacity(TABLES.len());
        for path in TABLES {
            tables.push(self.load_kv(path, platform)?.0);
        }
        Ok(KillIcons { tables })
    }

    /// One name's icon, as [`KillIcons::get`] answers it; this reads the tables on each call,
    /// so a run of names takes [`HudFiles::kill_icons`] once instead.
    pub fn kill_icon(
        &self,
        name: &str,
        platform: Platform,
    ) -> Result<Option<KillIcon>, HudFilesError> {
        Ok(self.kill_icons(platform)?.get(name))
    }
}

/// `name` in one table: the first entry whose prefixed name matches and that draws a region.
fn icon_in(pairs: &[KvPair], name: &str) -> Option<KillIcon> {
    let root = pairs.iter().find_map(block)?;
    // A file key's prefix: plain `file` has none, the rest are declared in TextureFileRefs.
    let mut prefixes = vec![("file", "")];
    for file_ref in block_named(root, "TextureFileRefs").unwrap_or_default() {
        if let Some(prefix) = block(file_ref).and_then(|b| text(b, "prefix")) {
            prefixes.push((file_ref.key.as_str(), prefix));
        }
    }
    let entries = block_named(root, "TextureData").unwrap_or_default();
    entries.iter().find_map(|entry| {
        let keys = block(entry)?;
        let file = prefixes.iter().find_map(|&(file_key, prefix)| {
            let rest = name.get(prefix.len()..)?;
            let named = name[..prefix.len()].eq_ignore_ascii_case(prefix)
                && rest.eq_ignore_ascii_case(&entry.key);
            named.then(|| text(keys, file_key)).flatten()
        })?;
        let number = |key: &str| text(keys, key).and_then(|t| t.trim().parse().ok());
        let icon = KillIcon {
            image: format!("../{file}"),
            x: number("x").unwrap_or(0),
            y: number("y").unwrap_or(0),
            wide: number("width").unwrap_or(0),
            tall: number("height").unwrap_or(0),
        };
        let glyph = text(keys, "font").is_some() || text(keys, "character").is_some();
        (!glyph && icon.wide > 0 && icon.tall > 0).then_some(icon)
    })
}

fn block(pair: &KvPair) -> Option<&[KvPair]> {
    match &pair.value {
        KvValue::Block(children) => Some(children),
        KvValue::Text(_) => None,
    }
}

/// The first block named `name`, ignoring case.
fn block_named<'a>(pairs: &'a [KvPair], name: &str) -> Option<&'a [KvPair]> {
    pairs
        .iter()
        .find(|pair| pair.key.eq_ignore_ascii_case(name))
        .and_then(block)
}

/// The first text value named `name`, ignoring case.
fn text<'a>(pairs: &'a [KvPair], name: &str) -> Option<&'a str> {
    pairs.iter().find_map(|pair| match &pair.value {
        KvValue::Text(t) if pair.key.eq_ignore_ascii_case(name) => Some(t.as_str()),
        KvValue::Text(_) | KvValue::Block(_) => None,
    })
}
