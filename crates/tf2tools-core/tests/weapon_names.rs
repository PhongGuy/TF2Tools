//! Readable weapon names for logs.tf's log names, from items_game.txt and tf_english.txt.

use tf2tools_core::WeaponNames;

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
