//! Which item pictures each weapon script: the items_game.txt item whose backpack image
//! the game ships, read from the player's own TF2 install.

use std::collections::{BTreeMap, HashMap};

use crate::fonts::Platform;
use crate::fonts::keyvalues::{self, KvPair, block, block_named, text};
use crate::weapons;

/// How many prefab hops a lookup follows before giving up, which also ends cycles.
pub(crate) const PREFAB_DEPTH: usize = 8;

/// Each weapon script's `image_inventory` path (as items_game spells it, no extension) from
/// items_game.txt text, through the item's prefabs. Scripts whose item or image is absent are
/// left out.
pub fn weapon_inventory_images(items_game: &str) -> BTreeMap<&'static str, String> {
    // Parse without `keyvalues::load`: its file size cap sits just above the live file.
    let file = keyvalues::parse(items_game, Platform::current().unwrap_or(Platform::Windows));
    let Some(root) = block_named(&file.pairs, "items_game") else {
        return BTreeMap::new();
    };

    let prefabs = prefab_map(root);

    let wanted: HashMap<&str, &'static str> = weapons()
        .iter()
        .filter_map(|w| Some((w.icon_item()?, w.script_name())))
        .collect();
    let mut images = BTreeMap::new();
    for pair in root.iter().filter(|p| p.key.eq_ignore_ascii_case("items")) {
        for item in block(pair).unwrap_or_default().iter().filter_map(block) {
            let Some(&script) = text(item, "name").and_then(|name| wanted.get(name)) else {
                continue;
            };
            if images.contains_key(script) {
                continue;
            }
            if let Some(image) = inherited(item, &prefabs, "image_inventory") {
                images.insert(script, image.to_owned());
            }
        }
    }
    images
}

/// Every `prefabs` block of items_game's root as one case-blind lookup; the first definition
/// of a name wins.
pub(crate) fn prefab_map(root: &[KvPair]) -> HashMap<String, &[KvPair]> {
    let mut prefabs: HashMap<String, &[KvPair]> = HashMap::new();
    for pair in root
        .iter()
        .filter(|p| p.key.eq_ignore_ascii_case("prefabs"))
    {
        for prefab in block(pair).unwrap_or_default() {
            if let Some(children) = block(prefab) {
                prefabs
                    .entry(prefab.key.to_ascii_lowercase())
                    .or_insert(children);
            }
        }
    }
    prefabs
}

/// An item's (or prefab's) own text value `key`, else the first found through its `prefab`
/// list, left to right and depth-first.
pub(crate) fn inherited<'a>(
    pairs: &'a [KvPair],
    prefabs: &HashMap<String, &'a [KvPair]>,
    key: &str,
) -> Option<&'a str> {
    inherited_at(pairs, prefabs, key, 0)
}

fn inherited_at<'a>(
    pairs: &'a [KvPair],
    prefabs: &HashMap<String, &'a [KvPair]>,
    key: &str,
    depth: usize,
) -> Option<&'a str> {
    if let Some(value) = text(pairs, key) {
        return Some(value);
    }
    if depth >= PREFAB_DEPTH {
        return None;
    }
    text(pairs, "prefab")?
        .split_whitespace()
        .filter_map(|name| prefabs.get(&name.to_ascii_lowercase()))
        .find_map(|prefab| inherited_at(prefab, prefabs, key, depth + 1))
}
