//! The game's English names and killfeed icons for the weapon names logs.tf records, read from
//! items_game.txt and tf_english.txt.

use std::collections::{HashMap, HashSet};
use std::path::Path;

use crate::fonts::Platform;
use crate::fonts::keyvalues::{self, KvPair, block, block_named, text};
use crate::fonts::read_regular_capped;
use crate::game_icons::ITEMS_GAME_MAX_BYTES;
use crate::weapon_icons::{PREFAB_DEPTH, inherited, prefab_map};
use crate::{GameText, TfClass, WeaponNamesError};

/// Display names and killfeed icon names keyed by logs.tf weapon log name, looked up without
/// regard to ASCII case.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct WeaponNames {
    names: HashMap<String, String>,
    // An item's `item_iconname` by its log name, where the two differ.
    icons: HashMap<String, String>,
    // Every item log and icon name: the weapon keys an item can put in a log.
    items: HashSet<String>,
    // The classes that can equip each item, by log, icon and stock entity name, as bits in
    // `TfClass::ALL` order. Empty when items_game lists no classes at all.
    classes: HashMap<String, u16>,
}

// Who logs the kill names that are no item. A stock projectile is its launcher's, and a Pyro
// logs it too when reflecting it. Bleeding comes from Scout, Engineer and Sniper weapons.
const NON_ITEM_CLASSES: [(&str, &[TfClass]); 9] = [
    ("tf_projectile_rocket", &[TfClass::Soldier, TfClass::Pyro]),
    ("tf_projectile_pipe", &[TfClass::Demoman, TfClass::Pyro]),
    (
        "tf_projectile_pipe_remote",
        &[TfClass::Demoman, TfClass::Pyro],
    ),
    ("tf_projectile_arrow", &[TfClass::Sniper, TfClass::Pyro]),
    ("obj_sentrygun", &[TfClass::Engineer]),
    ("obj_sentrygun2", &[TfClass::Engineer]),
    ("obj_sentrygun3", &[TfClass::Engineer]),
    ("obj_minisentry", &[TfClass::Engineer]),
    (
        "bleed_kill",
        &[TfClass::Scout, TfClass::Engineer, TfClass::Sniper],
    ),
];

// Stock projectiles by the launcher that fires them, and the `deflect_` kill they become when a
// Pyro reflects them, which names the kill and picks its killfeed icon.
const PROJECTILE_NAMES: [(&str, &str, &str); 4] = [
    ("tf_projectile_rocket", "Rocket Launcher", "deflect_rocket"),
    ("tf_projectile_pipe", "Grenade Launcher", "deflect_promode"),
    (
        "tf_projectile_pipe_remote",
        "Stickybomb Launcher",
        "deflect_sticky",
    ),
    ("tf_projectile_arrow", "Huntsman", "deflect_arrow"),
];

// The `deflect_` kill names, all Pyro's, by what was reflected; promode is a grenade.
const DEFLECT_NAMES: [(&str, &str); 8] = [
    ("deflect_rocket", "Reflected rocket"),
    ("deflect_promode", "Reflected pipe"),
    ("deflect_sticky", "Reflected sticky"),
    ("deflect_arrow", "Reflected arrow"),
    ("deflect_huntsman_flyingburn", "Reflected arrow"),
    ("deflect_flare", "Reflected flare"),
    ("deflect_flare_detonator", "Reflected flare"),
    ("deflect_ball", "Reflected ball"),
];

// Log names with no `d_` icon in mod_textures.txt, each with the entry the game's table draws
// for that kill instead: items, the Huntsman's arrow (it has no log name) and pumpkin bombs.
const ICON_ALIASES: [(&str, &str); 8] = [
    ("tf_projectile_arrow", "huntsman"),
    ("tf_pumpkin_bomb", "pumpkindeath"),
    ("unique_pickaxe", "pickaxe"),
    ("unique_pickaxe_escape", "pickaxe"),
    ("awper_hand", "sniperrifle"),
    ("stickybomb_defender", "sticky_resistance"),
    ("rocketpack", "rocketpack_stomp"),
    ("laser_pointer", "wrangler_kill"),
];

// The killfeed's own icon for a weapon its table has no entry for.
const FALLBACK_ICON: &str = "skull_tf";

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
    /// named by its `item_logname`, a stock weapon by its `tf_weapon_` class and entity `name`;
    /// the lowest defindex whose name localises wins, and an unlock beats a stock weapon of the
    /// same name.
    /// Every item's log and icon names are kept for [`WeaponNames::kill_icons`] whether or not
    /// they localise. Text that is no items_game gives no names.
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
        let mut icons = HashMap::new();
        let mut keys = HashSet::new();
        let mut classes: HashMap<String, u16> = HashMap::new();
        for (_, item) in items {
            let logname = inherited(item, &prefabs, "item_logname").map(str::to_ascii_lowercase);
            let iconname = inherited(item, &prefabs, "item_iconname").map(str::to_ascii_lowercase);
            let used_by = used_by_classes(item, &prefabs, 0);
            if used_by != 0 {
                // A stock weapon logs as its entity, under its item_class or its own `name`.
                let stock = logname.is_none().then(|| {
                    [inherited(item, &prefabs, "item_class"), text(item, "name")]
                        .into_iter()
                        .flatten()
                        .filter_map(|n| {
                            n.to_ascii_lowercase()
                                .strip_prefix("tf_weapon_")
                                .map(str::to_owned)
                        })
                });
                for key in logname
                    .iter()
                    .chain(&iconname)
                    .cloned()
                    .chain(stock.into_iter().flatten())
                {
                    *classes.entry(key).or_default() |= used_by;
                }
            }
            if let (Some(log), Some(icon)) = (&logname, &iconname)
                && log != icon
            {
                icons.entry(log.clone()).or_insert_with(|| icon.clone());
            }
            keys.extend(logname.into_iter().chain(iconname));
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
            } else {
                // A stock weapon logs as its entity, which its `name` spells when the shared
                // item_class does not, as TF_WEAPON_SHOTGUN_SOLDIER does for tf_weapon_shotgun.
                let class = inherited(item, &prefabs, "item_class");
                for name in [class, text(item, "name")].into_iter().flatten() {
                    let name = name.to_ascii_lowercase();
                    if let Some(weapon) = name.strip_prefix("tf_weapon_") {
                        stock
                            .entry(weapon.to_owned())
                            .or_insert_with(|| display.clone());
                    }
                }
            }
        }
        stock.extend(unlocks);
        WeaponNames {
            names: stock,
            icons,
            items: keys,
            classes,
        }
    }

    /// The classes that can equip the weapon a logs.tf log name stands for, in
    /// [`TfClass::ALL`] order; empty when unknown or when items_game listed no classes.
    pub fn classes(&self, log_name: &str) -> Vec<TfClass> {
        let mask = self.class_mask(&log_name.to_ascii_lowercase()).unwrap_or(0);
        TfClass::ALL
            .into_iter()
            .enumerate()
            .filter(|(bit, _)| mask & (1 << bit) != 0)
            .map(|(_, class)| class)
            .collect()
    }

    /// Whether `class` can have logged `log_name`: true for its own weapons and for any name
    /// whose classes are unknown, such as world, so no data is hidden on a guess.
    pub fn usable_by(&self, log_name: &str, class: TfClass) -> bool {
        self.class_mask(&log_name.to_ascii_lowercase())
            .is_none_or(|mask| mask & class_bit(class) != 0)
    }

    /// The display name for `log_name` logged on `class`: a stock projectile as its launcher,
    /// or as a reflect on Pyro, else [`WeaponNames::get`].
    pub fn class_name(&self, log_name: &str, class: TfClass) -> Option<&str> {
        let log = log_name.to_ascii_lowercase();
        let deflect_name = |key: &str| {
            DEFLECT_NAMES
                .iter()
                .find(|(k, _)| *k == key)
                .map(|&(_, n)| n)
        };
        let projectile = PROJECTILE_NAMES
            .iter()
            .find(|(key, ..)| *key == log)
            .and_then(|&(_, own, deflect)| {
                if class == TfClass::Pyro {
                    deflect_name(deflect)
                } else {
                    Some(own)
                }
            });
        deflect_name(&log).or(projectile).or_else(|| self.get(&log))
    }

    // The class bits for a lowercase log name, from items_game then the hand rules; None when
    // unknown, and always None when items_game gave no class lists to judge by.
    fn class_mask(&self, log: &str) -> Option<u16> {
        if self.classes.is_empty() {
            return None;
        }
        if let Some(&mask) = self.classes.get(log) {
            return Some(mask);
        }
        if log.starts_with("deflect_") {
            return Some(class_bit(TfClass::Pyro));
        }
        if let Some(class) = log.strip_prefix("taunt_").and_then(|c| {
            TfClass::ALL
                .into_iter()
                .find(|t| t.name().eq_ignore_ascii_case(c))
        }) {
            return Some(class_bit(class));
        }
        NON_ITEM_CLASSES
            .iter()
            .find(|(key, _)| *key == log)
            .map(|(_, owners)| owners.iter().fold(0, |mask, &c| mask | class_bit(c)))
    }

    /// The killfeed icon names to try for a logs.tf weapon log name, best first and without the
    /// `d_` prefix: the item's icon name or the entry the game draws for it, the log name
    /// itself, then the killfeed's skull for an item the table lacks. On Pyro a stock projectile
    /// is a reflect, so its `deflect_` icon leads; `None` for an unknown class.
    pub fn kill_icons<'a>(&'a self, log_name: &'a str, class: Option<TfClass>) -> Vec<&'a str> {
        let log = log_name.to_ascii_lowercase();
        let icon = self.icons.get(&log).map(String::as_str);
        let alias = |name: &str| {
            ICON_ALIASES
                .iter()
                .find(|(from, _)| *from == name)
                .map(|&(_, to)| to)
        };
        let item = self.items.contains(&log);
        let reflect = PROJECTILE_NAMES
            .iter()
            .find(|(key, ..)| *key == log && class == Some(TfClass::Pyro))
            .map(|&(.., deflect)| deflect);
        let mut found: Vec<&str> = Vec::with_capacity(5);
        for name in [
            reflect,
            icon.and_then(alias),
            icon,
            alias(&log),
            Some(log_name),
            item.then_some(FALLBACK_ICON),
        ]
        .into_iter()
        .flatten()
        {
            if !found.iter().any(|f| f.eq_ignore_ascii_case(name)) {
                found.push(name);
            }
        }
        found
    }

    /// Every weapon key an item can put in a log: each item's log name and icon name, lowercase.
    pub fn log_names(&self) -> impl Iterator<Item = &str> {
        self.items.iter().map(String::as_str)
    }

    /// The display name for a logs.tf weapon log name, ignoring ASCII case.
    pub fn get(&self, log_name: &str) -> Option<&str> {
        self.names
            .get(&log_name.to_ascii_lowercase())
            .map(String::as_str)
    }
}

fn class_bit(class: TfClass) -> u16 {
    let index = TfClass::ALL.iter().position(|&c| c == class).unwrap_or(0);
    1 << index
}

// The class bits of every `used_by_classes` block on an item and down its prefab list, merged as
// the game merges prefabs.
fn used_by_classes(pairs: &[KvPair], prefabs: &HashMap<String, &[KvPair]>, depth: usize) -> u16 {
    let own = pairs
        .iter()
        .filter(|p| p.key.eq_ignore_ascii_case("used_by_classes"))
        .flat_map(|p| block(p).unwrap_or_default())
        .filter_map(|p| {
            TfClass::ALL
                .into_iter()
                .find(|c| c.name().eq_ignore_ascii_case(&p.key))
        })
        .fold(0, |mask, class| mask | class_bit(class));
    if depth >= PREFAB_DEPTH {
        return own;
    }
    text(pairs, "prefab")
        .unwrap_or_default()
        .split_whitespace()
        .filter_map(|name| prefabs.get(&name.to_ascii_lowercase()))
        .fold(own, |mask, prefab| {
            mask | used_by_classes(prefab, prefabs, depth + 1)
        })
}

/// A whole regular file of at most `cap` bytes; anything else is an I/O error.
fn read_capped(path: &Path, cap: u64) -> std::io::Result<Vec<u8>> {
    read_regular_capped(path, cap)?.ok_or_else(|| {
        std::io::Error::other(format!("not a file or over {} MiB", cap / (1024 * 1024)))
    })
}
