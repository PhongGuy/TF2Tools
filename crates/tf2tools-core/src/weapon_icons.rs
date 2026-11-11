//! Which item pictures each weapon script: the items_game.txt item whose backpack image
//! the game ships, read from the player's own TF2 install.

use std::collections::{BTreeMap, HashMap};

use crate::fonts::Platform;
use crate::fonts::keyvalues::{self, KvPair, KvValue};
use crate::{Weapon, weapons};

impl Weapon {
    /// The items_game.txt item (its `name` value) whose backpack picture shows this weapon
    /// script, such as `"TF_WEAPON_SCATTERGUN"`. None for a script with no picture.
    pub fn icon_item(&self) -> Option<&'static str> {
        // Named by hand: looking items up by item_class picks the wrong item for shared
        // classes (builder, shotguns, Scout's pistol). Rows covering several items show the
        // stock or first-listed one.
        Some(match self.script_name() {
            "tf_weapon_pep_brawler_blaster" => "Baby Face's Blaster",
            "tf_weapon_lunchbox_drink" => "Bonk! Atomic Punch",
            "tf_weapon_bat_fish" => "The Holy Mackerel",
            "tf_weapon_jar_milk" => "Mad Milk",
            "tf_weapon_bat" => "TF_WEAPON_BAT",
            "tf_weapon_bat_wood" => "The Sandman",
            "tf_weapon_scattergun" => "TF_WEAPON_SCATTERGUN",
            "tf_weapon_handgun_scout_primary" => "The Shortstop",
            "tf_weapon_soda_popper" => "The Soda Popper",
            "tf_weapon_pistol_scout" => "TF_WEAPON_PISTOL_SCOUT",
            "tf_weapon_handgun_scout_secondary" => "The Winger",
            "tf_weapon_bat_giftwrap" => "The Wrap Assassin",
            "tf_weapon_cleaver" => "The Flying Guillotine",
            "tf_weapon_rocketlauncher_airstrike" => "The Air Strike",
            "tf_weapon_buff_item" => "The Buff Banner",
            "tf_weapon_particle_cannon" => "The Cow Mangler 5000",
            "tf_weapon_rocketlauncher_directhit" => "The Direct Hit",
            "tf_weapon_shovel" => "TF_WEAPON_SHOVEL",
            "tf_weapon_raygun" => "The Righteous Bison",
            "tf_weapon_rocketlauncher" => "TF_WEAPON_ROCKETLAUNCHER",
            "tf_weapon_shotgun_soldier" => "TF_WEAPON_SHOTGUN_SOLDIER",
            "tf_weapon_rocketlauncher_fireball" => "The Dragon's Fury",
            "tf_weapon_flamethrower" => "TF_WEAPON_FLAMETHROWER",
            "tf_weapon_flaregun" => "The Flare Gun",
            "tf_weapon_slap" => "The Hot Hand",
            "tf_weapon_flaregun_revenge" => "The Manmelter",
            "tf_weapon_fireaxe" => "TF_WEAPON_FIREAXE",
            "tf_weapon_shotgun_pyro" => "TF_WEAPON_SHOTGUN_PYRO",
            "tf_weapon_rocketpack" => "The Thermal Thruster",
            "tf_weapon_grenadelauncher" => "TF_WEAPON_GRENADELAUNCHER",
            "tf_weapon_cannon" => "The Loose Cannon",
            "tf_weapon_bottle" => "TF_WEAPON_BOTTLE",
            "tf_weapon_katana" => "The Half-Zatoichi",
            "tf_weapon_pipebomblauncher" => "TF_WEAPON_PIPEBOMBLAUNCHER",
            "tf_weapon_sword" => "The Eyelander",
            "tf_weapon_stickbomb" => "The Ullapool Caber",
            "tf_weapon_fists" => "TF_WEAPON_FISTS",
            "tf_weapon_minigun" => "TF_WEAPON_MINIGUN",
            "tf_weapon_lunchbox" => "The Sandvich",
            "tf_weapon_shotgun_hwg" => "TF_WEAPON_SHOTGUN_HWG",
            "tf_weapon_robot_arm" => "The Gunslinger",
            "tf_weapon_wrench" => "TF_WEAPON_WRENCH",
            "tf_weapon_pistol" => "TF_WEAPON_PISTOL",
            "tf_weapon_shotgun_building_rescue" => "The Rescue Ranger",
            "tf_weapon_mechanical_arm" => "The Short Circuit",
            "tf_weapon_shotgun_primary" => "TF_WEAPON_SHOTGUN_PRIMARY",
            "tf_weapon_sentry_revenge" => "The Frontier Justice",
            "tf_weapon_laser_pointer" => "The Wrangler",
            "tf_weapon_crossbow" => "The Crusader's Crossbow",
            "tf_weapon_medigun" => "TF_WEAPON_MEDIGUN",
            "tf_weapon_bonesaw" => "TF_WEAPON_BONESAW",
            "tf_weapon_syringegun_medic" => "TF_WEAPON_SYRINGEGUN_MEDIC",
            "tf_weapon_compound_bow" => "The Huntsman",
            "tf_weapon_sniperrifle_classic" => "The Classic",
            "tf_weapon_charged_smg" => "The Cleaner's Carbine",
            "tf_weapon_jar" => "Jarate",
            "tf_weapon_club" => "TF_WEAPON_CLUB",
            "tf_weapon_sniperrifle" => "TF_WEAPON_SNIPERRIFLE",
            "tf_weapon_smg" => "TF_WEAPON_SMG",
            "tf_weapon_sniperrifle_decap" => "The Bazaar Bargain",
            "tf_weapon_sapper" => "The Red-Tape Recorder",
            "tf_weapon_knife" => "TF_WEAPON_KNIFE",
            "tf_weapon_revolver" => "TF_WEAPON_REVOLVER",
            "tf_weapon_builder" => "TF_WEAPON_BUILDER_SPY",
            _ => return None,
        })
    }
}

/// How many prefab hops an image lookup follows before giving up, which also ends cycles.
const PREFAB_DEPTH: usize = 8;

/// Each weapon script's `image_inventory` path (as items_game spells it, no extension) from
/// items_game.txt text, through the item's prefabs. Scripts whose item or image is absent are
/// left out.
pub fn weapon_inventory_images(items_game: &str) -> BTreeMap<&'static str, String> {
    // Parse without `keyvalues::load`: its file size cap sits just above the live file.
    let file = keyvalues::parse(items_game, Platform::current().unwrap_or(Platform::Windows));
    let Some(root) = block_named(&file.pairs, "items_game") else {
        return BTreeMap::new();
    };

    // Every `prefabs` block feeds one case-blind lookup; the first definition of a name wins.
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
            if let Some(image) = inventory_image(item, &prefabs, 0) {
                images.insert(script, image.to_owned());
            }
        }
    }
    images
}

/// An item's (or prefab's) own `image_inventory`, else the first found through its `prefab`
/// list, left to right and depth-first.
fn inventory_image<'a>(
    pairs: &'a [KvPair],
    prefabs: &HashMap<String, &'a [KvPair]>,
    depth: usize,
) -> Option<&'a str> {
    if let Some(image) = text(pairs, "image_inventory") {
        return Some(image);
    }
    if depth >= PREFAB_DEPTH {
        return None;
    }
    text(pairs, "prefab")?
        .split_whitespace()
        .filter_map(|name| prefabs.get(&name.to_ascii_lowercase()))
        .find_map(|prefab| inventory_image(prefab, prefabs, depth + 1))
}

/// The children of a block value.
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
