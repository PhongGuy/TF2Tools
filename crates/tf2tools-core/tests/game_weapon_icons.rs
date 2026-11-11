//! Weapon icons from the game: the items_game item each weapon script shows, its backpack
//! image through prefabs, and the cached PNGs built from tf2_textures_dir.vpk.

use std::collections::BTreeMap;

use tf2tools_core::{Weapon, weapon_inventory_images, weapons};

// Checked against the live items_game.txt on 2026-11-11: each name occurs once and its
// backpack `_large` VTF is in tf2_textures_dir.vpk.
const ITEMS: [(&str, &str); 64] = [
    ("tf_weapon_pep_brawler_blaster", "Baby Face's Blaster"),
    ("tf_weapon_lunchbox_drink", "Bonk! Atomic Punch"),
    ("tf_weapon_bat_fish", "The Holy Mackerel"),
    ("tf_weapon_jar_milk", "Mad Milk"),
    ("tf_weapon_bat", "TF_WEAPON_BAT"),
    ("tf_weapon_bat_wood", "The Sandman"),
    ("tf_weapon_scattergun", "TF_WEAPON_SCATTERGUN"),
    ("tf_weapon_handgun_scout_primary", "The Shortstop"),
    ("tf_weapon_soda_popper", "The Soda Popper"),
    ("tf_weapon_pistol_scout", "TF_WEAPON_PISTOL_SCOUT"),
    ("tf_weapon_handgun_scout_secondary", "The Winger"),
    ("tf_weapon_bat_giftwrap", "The Wrap Assassin"),
    ("tf_weapon_cleaver", "The Flying Guillotine"),
    ("tf_weapon_rocketlauncher_airstrike", "The Air Strike"),
    ("tf_weapon_buff_item", "The Buff Banner"),
    ("tf_weapon_particle_cannon", "The Cow Mangler 5000"),
    ("tf_weapon_rocketlauncher_directhit", "The Direct Hit"),
    ("tf_weapon_shovel", "TF_WEAPON_SHOVEL"),
    ("tf_weapon_raygun", "The Righteous Bison"),
    ("tf_weapon_rocketlauncher", "TF_WEAPON_ROCKETLAUNCHER"),
    ("tf_weapon_shotgun_soldier", "TF_WEAPON_SHOTGUN_SOLDIER"),
    ("tf_weapon_rocketlauncher_fireball", "The Dragon's Fury"),
    ("tf_weapon_flamethrower", "TF_WEAPON_FLAMETHROWER"),
    ("tf_weapon_flaregun", "The Flare Gun"),
    ("tf_weapon_slap", "The Hot Hand"),
    ("tf_weapon_flaregun_revenge", "The Manmelter"),
    ("tf_weapon_fireaxe", "TF_WEAPON_FIREAXE"),
    ("tf_weapon_shotgun_pyro", "TF_WEAPON_SHOTGUN_PYRO"),
    ("tf_weapon_rocketpack", "The Thermal Thruster"),
    ("tf_weapon_grenadelauncher", "TF_WEAPON_GRENADELAUNCHER"),
    ("tf_weapon_cannon", "The Loose Cannon"),
    ("tf_weapon_bottle", "TF_WEAPON_BOTTLE"),
    ("tf_weapon_katana", "The Half-Zatoichi"),
    ("tf_weapon_pipebomblauncher", "TF_WEAPON_PIPEBOMBLAUNCHER"),
    ("tf_weapon_sword", "The Eyelander"),
    ("tf_weapon_stickbomb", "The Ullapool Caber"),
    ("tf_weapon_fists", "TF_WEAPON_FISTS"),
    ("tf_weapon_minigun", "TF_WEAPON_MINIGUN"),
    ("tf_weapon_lunchbox", "The Sandvich"),
    ("tf_weapon_shotgun_hwg", "TF_WEAPON_SHOTGUN_HWG"),
    ("tf_weapon_robot_arm", "The Gunslinger"),
    ("tf_weapon_wrench", "TF_WEAPON_WRENCH"),
    ("tf_weapon_pistol", "TF_WEAPON_PISTOL"),
    ("tf_weapon_shotgun_building_rescue", "The Rescue Ranger"),
    ("tf_weapon_mechanical_arm", "The Short Circuit"),
    ("tf_weapon_shotgun_primary", "TF_WEAPON_SHOTGUN_PRIMARY"),
    ("tf_weapon_sentry_revenge", "The Frontier Justice"),
    ("tf_weapon_laser_pointer", "The Wrangler"),
    ("tf_weapon_crossbow", "The Crusader's Crossbow"),
    ("tf_weapon_medigun", "TF_WEAPON_MEDIGUN"),
    ("tf_weapon_bonesaw", "TF_WEAPON_BONESAW"),
    ("tf_weapon_syringegun_medic", "TF_WEAPON_SYRINGEGUN_MEDIC"),
    ("tf_weapon_compound_bow", "The Huntsman"),
    ("tf_weapon_sniperrifle_classic", "The Classic"),
    ("tf_weapon_charged_smg", "The Cleaner's Carbine"),
    ("tf_weapon_jar", "Jarate"),
    ("tf_weapon_club", "TF_WEAPON_CLUB"),
    ("tf_weapon_sniperrifle", "TF_WEAPON_SNIPERRIFLE"),
    ("tf_weapon_smg", "TF_WEAPON_SMG"),
    ("tf_weapon_sniperrifle_decap", "The Bazaar Bargain"),
    ("tf_weapon_sapper", "The Red-Tape Recorder"),
    ("tf_weapon_knife", "TF_WEAPON_KNIFE"),
    ("tf_weapon_revolver", "TF_WEAPON_REVOLVER"),
    ("tf_weapon_builder", "TF_WEAPON_BUILDER_SPY"),
];

// A cut-down items_game.txt: nested and listed prefabs, an item's own key in odd case beating
// its prefab, a prefab cycle, and an item whose image is nowhere.
const ITEMS_GAME: &str = r#""items_game"
{
	"prefabs"
	{
		"weapon_scattergun" { "image_inventory" "backpack/weapons/c_models/c_scattergun" }
		"valve" { "quality" "unique" }
		"weapon_shotgun" { "prefab" "weapon_base_shotgun" }
		"weapon_base_shotgun" { "image_inventory" "backpack/weapons/w_models/w_shotgun" }
		"loop_a" { "prefab" "loop_b" }
		"loop_b" { "prefab" "loop_a" }
	}
	"items"
	{
		"default" { "name" "default" }
		"13" { "name" "TF_WEAPON_SCATTERGUN" "prefab" "weapon_scattergun" }
		"9" { "name" "TF_WEAPON_SHOTGUN_PRIMARY" "prefab" "valve weapon_shotgun" }
		"10" { "NAME" "TF_WEAPON_SHOTGUN_SOLDIER" "prefab" "weapon_shotgun" "Image_Inventory" "backpack/own/soldier_shotgun" }
		"0" { "name" "TF_WEAPON_BAT" "prefab" "loop_a" }
		"735" { "name" "TF_WEAPON_BUILDER_SPY" "image_inventory" "backpack/weapons/w_models/w_sapper" }
		"12" { "name" "TF_WEAPON_SHOTGUN_PYRO" }
	}
}"#;

fn weapon(script: &str) -> &'static Weapon {
    weapons()
        .iter()
        .find(|w| w.script_name() == script)
        .unwrap_or_else(|| panic!("no weapon {script}"))
}

#[test]
fn every_weapon_is_pictured_by_its_checked_item() {
    let got: Vec<(&str, Option<&str>)> = weapons()
        .iter()
        .map(|w| (w.script_name(), w.icon_item()))
        .collect();
    let want: Vec<(&str, Option<&str>)> = ITEMS.iter().map(|&(s, n)| (s, Some(n))).collect();
    assert_eq!(got, want);
}

// Looking these up by item_class picks the Engineer's PDA, the Widowmaker, or nothing.
#[test]
fn shared_classes_are_named_by_hand() {
    assert_eq!(
        weapon("tf_weapon_builder").icon_item(),
        Some("TF_WEAPON_BUILDER_SPY")
    );
    assert_eq!(
        weapon("tf_weapon_shotgun_primary").icon_item(),
        Some("TF_WEAPON_SHOTGUN_PRIMARY")
    );
    assert_eq!(
        weapon("tf_weapon_pistol_scout").icon_item(),
        Some("TF_WEAPON_PISTOL_SCOUT")
    );
}

#[test]
fn backpack_images_follow_prefabs_and_an_items_own_key_wins() {
    let want: BTreeMap<&str, String> = [
        (
            "tf_weapon_scattergun",
            "backpack/weapons/c_models/c_scattergun",
        ),
        (
            "tf_weapon_shotgun_primary",
            "backpack/weapons/w_models/w_shotgun",
        ),
        ("tf_weapon_shotgun_soldier", "backpack/own/soldier_shotgun"),
        ("tf_weapon_builder", "backpack/weapons/w_models/w_sapper"),
    ]
    .into_iter()
    .map(|(s, i)| (s, i.to_owned()))
    .collect();
    assert_eq!(weapon_inventory_images(ITEMS_GAME), want);
}

#[test]
fn a_prefab_cycle_or_a_missing_image_leaves_the_weapon_out() {
    let images = weapon_inventory_images(ITEMS_GAME);
    assert!(!images.contains_key("tf_weapon_bat"));
    assert!(!images.contains_key("tf_weapon_shotgun_pyro"));
}

#[test]
fn text_that_is_no_items_game_gives_no_images() {
    assert!(weapon_inventory_images("").is_empty());
    assert!(weapon_inventory_images(r#""items_game" { "items" "oops" }"#).is_empty());
}
