//! The game's English names for the weapon names logs.tf records, read from items_game.txt
//! and tf_english.txt.

use std::collections::HashMap;
use std::path::Path;

use crate::fonts::Platform;
use crate::fonts::keyvalues::{self, KvPair, block, block_named};
use crate::fonts::read_regular_capped;
use crate::game_icons::ITEMS_GAME_MAX_BYTES;
use crate::weapon_icons::{inherited, prefab_map};
use crate::{GameText, WeaponNamesError};

/// Display names keyed by logs.tf weapon log name, looked up without regard to ASCII case.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct WeaponNames {
    names: HashMap<String, String>,
}

/// tf_english.txt read cap, matching what `GameText::parse` reads.
const ENGLISH_MAX_BYTES: u64 = 16 * 1024 * 1024;

impl WeaponNames {
    /// Names from the game's `tf` folder: `scripts/items/items_game.txt` and
    /// `resource/tf_english.txt`. Blocking; fails when either file cannot be read.
    pub fn load(tf_dir: &Path) -> Result<WeaponNames, WeaponNamesError> {
        let items_path = tf_dir.join("scripts/items/items_game.txt");
        let items = read_capped(&items_path, ITEMS_GAME_MAX_BYTES).map_err(|source| {
            WeaponNamesError::ItemsGame {
                path: items_path,
                source,
            }
        })?;
        let english_path = tf_dir.join("resource/tf_english.txt");
        let english = read_capped(&english_path, ENGLISH_MAX_BYTES).map_err(|source| {
            WeaponNamesError::English {
                path: english_path,
                source,
            }
        })?;
        Ok(WeaponNames::from_files(
            &String::from_utf8_lossy(&items),
            &english,
        ))
    }

    /// Names from items_game.txt text and tf_english.txt bytes (UTF-16 or UTF-8). An unlock is
    /// named by its `item_logname`, a stock weapon by its `tf_weapon_` class; the lowest
    /// defindex whose name localises wins, and an unlock beats a stock weapon of the same name.
    /// Text that is no items_game gives no names.
    pub fn from_files(items_game: &str, english: &[u8]) -> WeaponNames {
        let platform = Platform::current().unwrap_or(Platform::Windows);
        let file = keyvalues::parse(items_game, platform);
        let Some(root) = block_named(&file.pairs, "items_game") else {
            return WeaponNames::default();
        };
        let prefabs = prefab_map(root);
        let lang = GameText::parse(english, platform);

        // Visit items lowest defindex first, so the first entry under a name is the one kept.
        let mut items: Vec<(u32, &[KvPair])> = root
            .iter()
            .filter(|p| p.key.eq_ignore_ascii_case("items"))
            .flat_map(|p| block(p).unwrap_or_default())
            .filter_map(|item| Some((item.key.parse().ok()?, block(item)?)))
            .collect();
        items.sort_by_key(|&(defindex, _)| defindex);

        let mut unlocks = HashMap::new();
        let mut stock = HashMap::new();
        for (_, item) in items {
            let Some(token) = inherited(item, &prefabs, "item_name").filter(|name| {
                name.strip_prefix('#')
                    .is_some_and(|t| lang.get(t).is_some())
            }) else {
                continue;
            };
            let shown = lang.show(token);
            let display = if inherited(item, &prefabs, "propername") == Some("1") {
                format!("The {shown}")
            } else {
                shown.into_owned()
            };
            if let Some(logname) = inherited(item, &prefabs, "item_logname") {
                unlocks
                    .entry(logname.to_ascii_lowercase())
                    .or_insert(display);
            } else if let Some(class) = inherited(item, &prefabs, "item_class")
                .map(str::to_ascii_lowercase)
                .and_then(|class| class.strip_prefix("tf_weapon_").map(str::to_owned))
            {
                stock.entry(class).or_insert(display);
            }
        }
        stock.extend(unlocks);
        WeaponNames { names: stock }
    }

    /// The display name for a logs.tf weapon log name, ignoring ASCII case.
    pub fn get(&self, log_name: &str) -> Option<&str> {
        self.names
            .get(&log_name.to_ascii_lowercase())
            .map(String::as_str)
    }
}

/// A whole regular file of at most `cap` bytes; anything else is an I/O error.
fn read_capped(path: &Path, cap: u64) -> std::io::Result<Vec<u8>> {
    read_regular_capped(path, cap)?.ok_or_else(|| {
        std::io::Error::other(format!("not a file or over {} MiB", cap / (1024 * 1024)))
    })
}
