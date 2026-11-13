//! Readable weapon names for logs.tf's log names, from items_game.txt and tf_english.txt.

use tf2tools_core::{TfClass, WeaponNames, WeaponNamesError};

fn utf16le(text: &str) -> Vec<u8> {
    let mut out = vec![0xFF, 0xFE];
    for unit in text.encode_utf16() {
        out.extend_from_slice(&unit.to_le_bytes());
    }
    out
}

// Shapes copied from the live items_game.txt (items 18, 205, 513, 649, 1151, prefab weapon_quadball);
// no Valve bytes. 205 is listed before 18 so file order is not defindex order.
const ITEMS_GAME: &str = r##""items_game"
{
	"prefabs"
	{
		"weapon_rocketlauncher" { "item_class" "tf_weapon_rocketlauncher" "item_name" "#TF_Weapon_RocketLauncher" }
		"weapon_quadball"
		{
			"item_class"		"tf_weapon_grenadelauncher"
			"item_logname"	"iron_bomber"
			"item_name"		"#TF_Weapon_Iron_bomber"
			"propername"		"1"
		}
		"paintkit_base" { "image_inventory" "backpack/paintkit" }
		"weapon_spy_knife_item2" { "item_class" "tf_weapon_knife" }
		"loop_a" { "prefab" "loop_b" }
		"loop_b" { "prefab" "loop_a" }
	}
	"items"
	{
		"default" { "item_class" "tf_weapon_bat" "item_name" "#TF_Weapon_Bat" }
		"205" { "name" "Upgradeable TF_WEAPON_ROCKETLAUNCHER" "prefab" "weapon_rocketlauncher" "item_name" "#TF_Weapon_RocketLauncher_Upgradeable" }
		"18" { "name" "TF_WEAPON_ROCKETLAUNCHER" "prefab" "weapon_rocketlauncher" }
		"2" { "item_class" "tf_weapon_knife" "item_logname" "kunai" "item_name" "#TF_Kunai" }
		"4" { "item_class" "tf_weapon_knife" "item_name" "#TF_Weapon_Knife" }
		"513"
		{
			"name"	"The Original"
			"item_class"	"tf_weapon_rocketlauncher"
			"item_logname"	"quake_rl"
			"item_name"	"#TF_TheOriginal"
			"propername"	"1"
		}
		"649"
		{
			"name"	"The Spy-cicle"
			"prefab"	"weapon_spy_knife_item2"
			"item_name"	"#TF_SpyCicle"
			"propername"	"1"
			"item_logname"	"spy_cicle"
		}
		"1151" { "name" "The Iron Bomber" "prefab" "Weapon_QuadBall paintkit_base" }
		"1152" { "prefab" "weapon_quadball" "item_logname" "plain_bomber" "propername" "0" }
		"21" { "item_class" "tf_weapon_flamethrower" "item_name" "#TF_Weapon_FlameThrower" }
		"39" { "item_class" "tf_weapon_flamethrower" "item_logname" "backburner" "item_name" "#TF_Nowhere" }
		"40" { "item_class" "tf_weapon_flamethrower" "item_logname" "backburner" "item_name" "#TF_Unique_Backburner" "propername" "1" }
		"1146" { "item_class" "tf_weapon_flamethrower" "item_logname" "backburner" "item_name" "#TF_Weapon_FestiveBackburner" }
		"700" { "item_logname" "ghost" "item_name" "#TF_Nowhere" }
		"701" { "item_logname" "bare" "item_name" "Bare Name" }
		"800" { "prefab" "loop_a" "item_logname" "looped" }
	}
}"##;

const LANG: &str = "\"lang\"\r\n{\r\n\"Language\" \"English\"\r\n\"Tokens\"\r\n{\r\n\
\"TF_Weapon_RocketLauncher\"\t\"Rocket Launcher\"\r\n\
\"TF_Weapon_RocketLauncher_Upgradeable\"\t\"Upgradeable Rocket Launcher\"\r\n\
\"TF_Weapon_Knife\"\t\t\"Knife\"\r\n\
\"TF_Kunai\"\t\"Conniver's Kunai\"\r\n\
\"TF_TheOriginal\"\t\t\t\"Original\" // ADD THE\r\n\
\"TF_SpyCicle\"\t\t\t\t\t\"Spy-Cicle\" // ADD THE\r\n\
\"TF_Weapon_Iron_bomber\"\t\t\t\"Iron Bomber\" // ADD THE\r\n\
\"TF_Weapon_FlameThrower\"\t\"Flame Thrower\"\r\n\
\"TF_Unique_Backburner\"\t\"Backburner\"\r\n\
\"TF_Weapon_FestiveBackburner\"\t\"Festive Backburner\"\r\n\
\"TF_Weapon_Bat\"\t\"Bat\"\r\n\
}\r\n}\r\n";

fn names() -> WeaponNames {
    WeaponNames::from_files(ITEMS_GAME, &utf16le(LANG))
}

#[test]
fn unlocks_read_their_item_name_with_the_when_proper() {
    let n = names();
    assert_eq!(n.get("quake_rl"), Some("The Original"));
    assert_eq!(n.get("spy_cicle"), Some("The Spy-Cicle"));
    assert_eq!(n.get("kunai"), Some("Conniver's Kunai"));
    // 39 does not localise, so 40 names it; the festive 1146 comes later.
    assert_eq!(n.get("backburner"), Some("The Backburner"));
}

#[test]
fn a_logname_name_and_propername_follow_a_mixed_case_prefab_list() {
    let n = names();
    assert_eq!(n.get("iron_bomber"), Some("The Iron Bomber"));
    assert_eq!(
        n.get("plain_bomber"),
        Some("Iron Bomber"),
        "own propername 0 wins"
    );
    assert_eq!(
        n.get("grenadelauncher"),
        None,
        "a logname item is no stock item"
    );
}

#[test]
fn stock_names_come_from_the_lowest_defindex_without_a_logname() {
    let n = names();
    assert_eq!(n.get("rocketlauncher"), Some("Rocket Launcher"));
    assert_eq!(n.get("knife"), Some("Knife"));
    assert_eq!(n.get("flamethrower"), Some("Flame Thrower"));
}

#[test]
fn lookup_ignores_ascii_case() {
    let n = names();
    assert_eq!(n.get("QUAKE_RL"), Some("The Original"));
    assert_eq!(n.get("RocketLauncher"), Some("Rocket Launcher"));
}

#[test]
fn unknown_unlocalised_and_cyclic_names_have_no_entry() {
    let n = names();
    for raw in [
        "ghost",
        "bare",
        "looped",
        "bat",
        "tf_projectile_rocket",
        "world",
    ] {
        assert_eq!(n.get(raw), None, "{raw}");
    }
}

#[test]
fn an_unlock_beats_a_stock_weapon_of_the_same_name() {
    let items = r##""items_game" { "items" {
        "4" { "item_class" "tf_weapon_knife" "item_name" "#TF_Weapon_Knife" }
        "900" { "item_logname" "knife" "item_name" "#TF_Kunai" }
    } }"##;
    let n = WeaponNames::from_files(items, &utf16le(LANG));
    assert_eq!(n.get("knife"), Some("Conniver's Kunai"));
}

#[test]
fn text_that_is_no_items_game_gives_no_names() {
    let lang = utf16le(LANG);
    assert_eq!(WeaponNames::from_files("", &lang), WeaponNames::default());
    assert_eq!(
        WeaponNames::from_files(r#""items_game" { "items" "oops" }"#, &lang),
        WeaponNames::default()
    );
    assert_eq!(
        names(),
        WeaponNames::from_files(ITEMS_GAME, LANG.as_bytes()),
        "UTF-8 lang too"
    );
    assert_eq!(WeaponNames::default().get("quake_rl"), None);
}

fn tf_folder(items: Option<&str>, english: Option<&[u8]>) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    if let Some(items) = items {
        std::fs::create_dir_all(dir.path().join("scripts/items")).unwrap();
        std::fs::write(dir.path().join("scripts/items/items_game.txt"), items).unwrap();
    }
    if let Some(english) = english {
        std::fs::create_dir_all(dir.path().join("resource")).unwrap();
        std::fs::write(dir.path().join("resource/tf_english.txt"), english).unwrap();
    }
    dir
}

#[test]
fn load_reads_items_game_and_the_english_lang_file() {
    let lang = utf16le(LANG);
    let tf = tf_folder(Some(ITEMS_GAME), Some(&lang));
    assert_eq!(WeaponNames::load(tf.path()).unwrap(), names());
}

#[test]
fn a_missing_items_game_or_lang_file_fails_the_load() {
    let lang = utf16le(LANG);
    let err = WeaponNames::load(tf_folder(None, Some(&lang)).path()).unwrap_err();
    assert!(matches!(err, WeaponNamesError::ItemsGame { .. }), "{err:?}");
    let err = WeaponNames::load(tf_folder(Some(ITEMS_GAME), None).path()).unwrap_err();
    assert!(matches!(err, WeaponNamesError::English { .. }), "{err:?}");
}

#[test]
#[ignore = "needs a TF2 game folder in TF2TOOLS_TF2_GAME"]
fn the_real_game_names_the_fixture_weapons() {
    let game =
        std::path::PathBuf::from(std::env::var("TF2TOOLS_TF2_GAME").expect("TF2TOOLS_TF2_GAME"));
    let started = std::time::Instant::now();
    let n = WeaponNames::load(&game.join("tf")).unwrap();
    eprintln!("loaded in {:?}", started.elapsed());
    assert_eq!(n.get("quake_rl"), Some("The Original"));
    assert_eq!(n.get("spy_cicle"), Some("The Spy-Cicle"));
    assert_eq!(n.get("iron_bomber"), Some("The Iron Bomber"));
    assert_eq!(n.get("scattergun"), Some("Scattergun"));
    assert_eq!(n.get("rocketlauncher"), Some("Rocket Launcher"));
}

// Kill-icon shapes from the live items_game.txt: an unlock with no item_iconname (775), one whose
// icon is named apart from its log name (141), one whose named icon the game lacks (939), and the
// stock Soldier shotgun whose entity name differs from its item_class.
const ICON_ITEMS: &str = r##""items_game" { "items" {
    "10" { "name" "TF_WEAPON_SHOTGUN_SOLDIER" "item_class" "tf_weapon_shotgun" "item_name" "#TF_Weapon_Shotgun" }
    "141" { "item_class" "tf_weapon_sentry_revenge" "item_logname" "frontier_justice" "item_iconname" "frontier_kill" }
    "775" { "name" "The Escape Plan" "item_class" "tf_weapon_shovel" "item_logname" "unique_pickaxe_escape" "item_name" "#TF_Nowhere" }
    "939" { "item_class" "saxxy" "item_logname" "skullbat" "item_iconname" "skullbat" }
} }"##;

fn icon_names() -> WeaponNames {
    let lang = utf16le("\"lang\" { \"Tokens\" { \"TF_Weapon_Shotgun\" \"Shotgun\" } }");
    WeaponNames::from_files(ICON_ITEMS, &lang)
}

#[test]
fn the_escape_plan_draws_the_pickaxe_kill_icon() {
    let n = icon_names();
    assert_eq!(n.kill_icons("unique_pickaxe_escape", None)[0], "pickaxe");
    assert_eq!(n.kill_icons("UNIQUE_PICKAXE_ESCAPE", None)[0], "pickaxe");
}

#[test]
fn an_item_icon_comes_before_its_log_name_and_the_skull_comes_last() {
    let n = icon_names();
    assert_eq!(
        n.kill_icons("frontier_justice", None),
        ["frontier_kill", "frontier_justice", "skull_tf"]
    );
    assert_eq!(n.kill_icons("skullbat", None), ["skullbat", "skull_tf"]);
    assert_eq!(
        n.kill_icons("tf_projectile_rocket", None),
        ["tf_projectile_rocket"]
    );
    assert_eq!(n.kill_icons("world", None), ["world"]);
}

#[test]
fn every_item_log_and_icon_name_is_a_key() {
    let names = icon_names();
    let mut keys: Vec<&str> = names.log_names().collect();
    keys.sort_unstable();
    assert_eq!(
        keys,
        [
            "frontier_justice",
            "frontier_kill",
            "skullbat",
            "unique_pickaxe_escape"
        ]
    );
}

#[test]
fn a_stock_weapon_is_named_by_its_entity_name_too() {
    let n = icon_names();
    assert_eq!(n.get("shotgun_soldier"), Some("Shotgun"));
    assert_eq!(n.get("shotgun"), Some("Shotgun"));
}

#[test]
fn each_log_name_without_its_own_icon_draws_the_games_entry_for_it() {
    let n = WeaponNames::default();
    for (log, icon) in [
        ("tf_projectile_arrow", "huntsman"),
        ("tf_pumpkin_bomb", "pumpkindeath"),
        ("unique_pickaxe", "pickaxe"),
        ("unique_pickaxe_escape", "pickaxe"),
        ("awper_hand", "sniperrifle"),
        ("stickybomb_defender", "sticky_resistance"),
        ("rocketpack", "rocketpack_stomp"),
        ("laser_pointer", "wrangler_kill"),
    ] {
        assert_eq!(n.kill_icons(log, None)[0], icon, "{log}");
    }
}

#[test]
fn a_pyros_stock_projectile_kill_draws_the_reflect_icon_first() {
    let n = WeaponNames::default();
    let pyro = Some(TfClass::Pyro);
    assert_eq!(
        n.kill_icons("tf_projectile_rocket", pyro),
        ["deflect_rocket", "tf_projectile_rocket"]
    );
    assert_eq!(
        n.kill_icons("TF_PROJECTILE_PIPE", pyro)[0],
        "deflect_promode"
    );
    assert_eq!(
        n.kill_icons("tf_projectile_pipe_remote", pyro)[0],
        "deflect_sticky"
    );
    assert_eq!(
        n.kill_icons("tf_projectile_arrow", pyro),
        ["deflect_arrow", "huntsman", "tf_projectile_arrow"]
    );
}

#[test]
fn another_class_keeps_the_launchers_projectile_icon() {
    let n = WeaponNames::default();
    for class in [Some(TfClass::Soldier), None] {
        assert_eq!(
            n.kill_icons("tf_projectile_rocket", class),
            ["tf_projectile_rocket"]
        );
        assert_eq!(n.kill_icons("tf_projectile_arrow", class)[0], "huntsman");
    }
    assert_eq!(
        n.kill_icons("deflect_rocket", Some(TfClass::Pyro)),
        ["deflect_rocket"]
    );
}

// Kill names logs.tf records that are no item: sentries, reflects, stock projectiles, class
// taunts, bleeding and pumpkin bombs. Each draws a `d_` icon of the game's.
const NON_ITEM_KEYS: [&str; 23] = [
    "obj_sentrygun",
    "obj_sentrygun2",
    "obj_sentrygun3",
    "obj_minisentry",
    "deflect_rocket",
    "deflect_promode",
    "deflect_sticky",
    "deflect_arrow",
    "deflect_flare",
    "tf_projectile_rocket",
    "tf_projectile_pipe",
    "tf_projectile_pipe_remote",
    "tf_projectile_arrow",
    "tf_pumpkin_bomb",
    "taunt_scout",
    "taunt_soldier",
    "taunt_pyro",
    "taunt_demoman",
    "taunt_heavy",
    "taunt_medic",
    "taunt_sniper",
    "taunt_spy",
    "bleed_kill",
];

// Kills logs.tf records with no weapon behind them; the game's table has no icon for them.
const BLANK_KEYS: [&str; 5] = [
    "world",
    "player",
    "telefrag",
    "trigger_hurt",
    "env_explosion",
];

// Every weapon key the game can log (each item's log and icon name, plus the documented non-item
// kills) must resolve to a killfeed icon the game's own table draws, and only the two items the
// table truly lacks may fall back to the skull.
#[test]
#[ignore = "needs a TF2 game folder in TF2TOOLS_TF2_GAME"]
fn every_real_weapon_key_has_a_kill_icon() {
    let game =
        std::path::PathBuf::from(std::env::var("TF2TOOLS_TF2_GAME").expect("TF2TOOLS_TF2_GAME"));
    let tf = game.join("tf");
    let names = WeaponNames::load(&tf).unwrap();
    let platform = tf2tools_core::Platform::current().unwrap();
    let icons = tf2tools_core::HudFiles::open_game(&tf)
        .unwrap()
        .kill_icons(platform)
        .unwrap();
    let drawn = |key: &str| {
        names
            .kill_icons(key, None)
            .into_iter()
            .find(|icon| icons.get(&format!("d_{icon}")).is_some())
            .map(str::to_owned)
    };
    let items: Vec<&str> = names.log_names().collect();
    assert!(items.len() > 100, "{} keys", items.len());
    let keys: Vec<&str> = items.iter().copied().chain(NON_ITEM_KEYS).collect();
    let missing: Vec<&str> = keys
        .iter()
        .copied()
        .filter(|k| drawn(k).is_none())
        .collect();
    let mut skulls: Vec<&str> = keys
        .iter()
        .copied()
        .filter(|k| drawn(k).as_deref() == Some("skull_tf"))
        .collect();
    skulls.sort_unstable();
    eprintln!(
        "{} keys, {} without an icon, skull for {skulls:?}",
        keys.len(),
        missing.len()
    );
    assert!(missing.is_empty(), "no kill icon for {missing:?}");
    assert_eq!(skulls, ["grappling_hook", "skullbat"]);
    for key in BLANK_KEYS {
        assert!(!items.contains(&key), "{key} is no item");
        assert_eq!(drawn(key), None, "{key}");
    }
    assert_eq!(drawn("unique_pickaxe_escape").as_deref(), Some("pickaxe"));
    assert_eq!(names.get("shotgun_soldier"), Some("Shotgun"));
    // A Pyro's stock projectile kill leads with a reflect icon the game really draws.
    for key in &NON_ITEM_KEYS[9..13] {
        let first = names.kill_icons(key, Some(TfClass::Pyro))[0];
        assert!(first.starts_with("deflect_"), "{key}: {first}");
        assert!(icons.get(&format!("d_{first}")).is_some(), "{key}: {first}");
    }
}

// Class shapes from the live items_game.txt: a class list on a prefab (18), on an unlock (513),
// shared by two classes (154), the four stock shotguns on one item_class, and an icon name (141).
const CLASS_ITEMS: &str = r##""items_game" {
    "prefabs" {
        "weapon_rocketlauncher" { "item_class" "tf_weapon_rocketlauncher" "used_by_classes" { "soldier" "1" } }
        "weapon_shotgun" { "item_class" "tf_weapon_shotgun" }
    }
    "items" {
        "18" { "name" "TF_WEAPON_ROCKETLAUNCHER" "prefab" "weapon_rocketlauncher" }
        "513" { "item_class" "tf_weapon_rocketlauncher" "item_logname" "quake_rl" "used_by_classes" { "soldier" "1" } }
        "154" { "item_class" "tf_weapon_shovel" "item_logname" "paintrain" "used_by_classes" { "Soldier" "1" "demoman" "1" } }
        "9" { "name" "TF_WEAPON_SHOTGUN_PRIMARY" "prefab" "weapon_shotgun" "used_by_classes" { "engineer" "1" } }
        "10" { "name" "TF_WEAPON_SHOTGUN_SOLDIER" "prefab" "weapon_shotgun" "used_by_classes" { "soldier" "1" } }
        "11" { "name" "TF_WEAPON_SHOTGUN_HWG" "prefab" "weapon_shotgun" "used_by_classes" { "heavy" "1" } }
        "12" { "name" "TF_WEAPON_SHOTGUN_PYRO" "prefab" "weapon_shotgun" "used_by_classes" { "pyro" "1" } }
        "141" { "item_class" "tf_weapon_sentry_revenge" "item_logname" "frontier_justice" "item_iconname" "frontier_kill" "used_by_classes" { "engineer" "1" } }
        "1000" { "item_logname" "classless" }
    }
}"##;

fn class_names() -> WeaponNames {
    // No lang file: ownership does not depend on a weapon's name localising.
    WeaponNames::from_files(CLASS_ITEMS, b"")
}

#[test]
fn used_by_classes_give_an_items_classes() {
    let n = class_names();
    assert_eq!(n.classes("quake_rl"), [TfClass::Soldier]);
    assert_eq!(n.classes("QUAKE_RL"), [TfClass::Soldier]);
    assert_eq!(n.classes("paintrain"), [TfClass::Soldier, TfClass::Demoman]);
    assert_eq!(n.classes("frontier_kill"), [TfClass::Engineer]);
    assert_eq!(
        n.classes("rocketlauncher"),
        [TfClass::Soldier],
        "through the prefab"
    );
}

#[test]
fn each_stock_shotgun_belongs_to_its_entity_class() {
    let n = class_names();
    assert_eq!(n.classes("shotgun_soldier"), [TfClass::Soldier]);
    assert_eq!(n.classes("shotgun_pyro"), [TfClass::Pyro]);
    assert_eq!(n.classes("shotgun_hwg"), [TfClass::Heavy]);
    assert_eq!(n.classes("shotgun_primary"), [TfClass::Engineer]);
}

#[test]
fn reflects_belong_to_pyro_and_stock_projectiles_to_their_launcher_and_pyro() {
    let n = class_names();
    for key in [
        "deflect_rocket",
        "deflect_promode",
        "deflect_sticky",
        "deflect_flare",
    ] {
        assert_eq!(n.classes(key), [TfClass::Pyro], "{key}");
    }
    assert_eq!(
        n.classes("tf_projectile_rocket"),
        [TfClass::Soldier, TfClass::Pyro]
    );
    assert_eq!(
        n.classes("tf_projectile_pipe"),
        [TfClass::Pyro, TfClass::Demoman]
    );
    assert_eq!(
        n.classes("tf_projectile_pipe_remote"),
        [TfClass::Pyro, TfClass::Demoman]
    );
    assert_eq!(
        n.classes("tf_projectile_arrow"),
        [TfClass::Pyro, TfClass::Sniper]
    );
    assert_eq!(n.classes("obj_sentrygun3"), [TfClass::Engineer]);
    assert_eq!(n.classes("taunt_medic"), [TfClass::Medic]);
}

#[test]
fn a_class_uses_only_its_own_weapons_and_anything_unknown() {
    let n = class_names();
    assert!(n.usable_by("quake_rl", TfClass::Soldier));
    assert!(!n.usable_by("quake_rl", TfClass::Pyro));
    assert!(!n.usable_by("quake_rl", TfClass::Medic));
    assert!(n.usable_by("paintrain", TfClass::Demoman));
    assert!(n.usable_by("tf_projectile_rocket", TfClass::Pyro));
    assert!(!n.usable_by("tf_projectile_rocket", TfClass::Medic));
    for key in [
        "mystery_gun",
        "classless",
        "world",
        "player",
        "tf_pumpkin_bomb",
    ] {
        assert!(n.classes(key).is_empty(), "{key}");
        for class in TfClass::ALL {
            assert!(n.usable_by(key, class), "{key} on {class:?}");
        }
    }
}

#[test]
fn without_class_lists_every_class_uses_everything() {
    for n in [WeaponNames::default(), names()] {
        for key in ["quake_rl", "deflect_rocket", "tf_projectile_pipe"] {
            assert!(n.classes(key).is_empty(), "{key}");
            assert!(n.usable_by(key, TfClass::Medic), "{key}");
        }
    }
}

#[test]
fn projectiles_read_as_their_launcher_or_a_reflect_on_pyro() {
    let n = WeaponNames::default();
    for (key, class, shown) in [
        ("tf_projectile_rocket", TfClass::Soldier, "Rocket Launcher"),
        ("tf_projectile_rocket", TfClass::Pyro, "Reflected rocket"),
        ("tf_projectile_pipe", TfClass::Demoman, "Grenade Launcher"),
        ("tf_projectile_pipe", TfClass::Pyro, "Reflected pipe"),
        (
            "tf_projectile_pipe_remote",
            TfClass::Demoman,
            "Stickybomb Launcher",
        ),
        (
            "tf_projectile_pipe_remote",
            TfClass::Pyro,
            "Reflected sticky",
        ),
        ("tf_projectile_arrow", TfClass::Sniper, "Huntsman"),
        ("tf_projectile_arrow", TfClass::Pyro, "Reflected arrow"),
        ("deflect_rocket", TfClass::Pyro, "Reflected rocket"),
        ("deflect_promode", TfClass::Pyro, "Reflected pipe"),
        ("deflect_sticky", TfClass::Pyro, "Reflected sticky"),
        ("deflect_arrow", TfClass::Pyro, "Reflected arrow"),
        ("deflect_flare", TfClass::Pyro, "Reflected flare"),
        ("deflect_ball", TfClass::Pyro, "Reflected ball"),
    ] {
        assert_eq!(n.class_name(key, class), Some(shown), "{key} on {class:?}");
    }
    let n = names();
    assert_eq!(
        n.class_name("quake_rl", TfClass::Soldier),
        Some("The Original")
    );
    assert_eq!(n.class_name("mystery_gun", TfClass::Pyro), None);
}

// Every item key the game can log, and each stock and non-item kill name, belongs to a class.
#[test]
#[ignore = "needs a TF2 game folder in TF2TOOLS_TF2_GAME"]
fn every_real_weapon_key_has_a_class() {
    let game =
        std::path::PathBuf::from(std::env::var("TF2TOOLS_TF2_GAME").expect("TF2TOOLS_TF2_GAME"));
    let names = WeaponNames::load(&game.join("tf")).unwrap();
    let stock = [
        "scattergun",
        "pistol_scout",
        "shotgun_soldier",
        "shotgun_pyro",
        "shotgun_hwg",
        "shotgun_primary",
        "pistol",
        "flamethrower",
        "minigun",
        "sniperrifle",
        "smg",
        "knife",
    ];
    let owned = NON_ITEM_KEYS
        .into_iter()
        .filter(|k| *k != "tf_pumpkin_bomb");
    let mut missing: Vec<&str> = names
        .log_names()
        .chain(stock)
        .chain(owned)
        .filter(|k| names.classes(k).is_empty())
        .collect();
    missing.sort_unstable();
    assert!(missing.is_empty(), "no class for {missing:?}");
    assert_eq!(names.classes("quake_rl"), [TfClass::Soldier]);
    assert_eq!(
        names.classes("paintrain"),
        [TfClass::Soldier, TfClass::Demoman]
    );
    assert!(!names.usable_by("quake_rl", TfClass::Pyro));
    for key in BLANK_KEYS.into_iter().chain(["tf_pumpkin_bomb"]) {
        assert!(names.classes(key).is_empty(), "{key}");
    }
}
