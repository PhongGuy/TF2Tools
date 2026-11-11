//! Weapon icons from the game: the items_game item each weapon script shows, its backpack
//! image through prefabs, and the cached PNGs built from tf2_textures_dir.vpk.

use std::collections::BTreeMap;
use std::path::PathBuf;

use crate::common;
use tf2tools_core::{
    GameWeaponIcons, RgbaImage, Weapon, WeaponIconsError, read_png, rgba_to_vtf,
    weapon_inventory_images, weapons,
};

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

const SCATTERGUN_LARGE: &str = "materials/backpack/weapons/c_models/c_scattergun_large.vtf";
// Stored in mixed case: the lookup ignores case.
const SHOTGUN_LARGE: &str = "materials/Backpack/weapons/w_models/W_Shotgun_large.vtf";
// The sapper has only its plain texture, the fallback when `_large` is missing.
const SAPPER_PLAIN: &str = "materials/backpack/weapons/w_models/w_sapper.vtf";

fn vtf(w: u32, h: u32, rgba: [u8; 4]) -> Vec<u8> {
    let pixels = rgba.repeat((w * h) as usize);
    rgba_to_vtf(&RgbaImage::from_pixels(w, h, pixels).unwrap()).unwrap()
}

/// A temp folder holding `tf/scripts/items/items_game.txt` (ITEMS_GAME) and
/// `tf/tf2_textures_dir.vpk` holding `entries`, plus an empty `cache` path; returns (tmp, tf, cache).
fn game(entries: &[(&str, &[u8])]) -> (tempfile::TempDir, PathBuf, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let tf = dir.path().join("tf");
    common::write_file(
        &tf.join("scripts/items/items_game.txt"),
        ITEMS_GAME.as_bytes(),
    );
    common::write_file(
        &tf.join("tf2_textures_dir.vpk"),
        &common::vpk_bytes(entries),
    );
    let cache = dir.path().join("cache");
    (dir, tf, cache)
}

fn standard_entries() -> Vec<(&'static str, Vec<u8>)> {
    vec![
        (SCATTERGUN_LARGE, vtf(256, 256, [200, 30, 30, 255])),
        (SHOTGUN_LARGE, vtf(64, 32, [30, 30, 200, 255])),
        (SAPPER_PLAIN, vtf(16, 16, [30, 200, 30, 255])),
    ]
}

fn borrowed<'a>(entries: &'a [(&'static str, Vec<u8>)]) -> Vec<(&'static str, &'a [u8])> {
    entries.iter().map(|(p, b)| (*p, b.as_slice())).collect()
}

fn png_size(path: &std::path::Path) -> (u32, u32) {
    let png = read_png(path).unwrap();
    (png.width(), png.height())
}

// The resize filter may round a flat colour by a step, so compare the first pixel loosely.
fn assert_first_pixel_near(path: &std::path::Path, want: [u8; 4]) {
    let png = read_png(path).unwrap();
    let got = &png.pixels()[..4];
    for (g, w) in got.iter().zip(want) {
        assert!(g.abs_diff(w) <= 2, "{got:?} is not near {want:?}");
    }
}

/// Dates the file or folder `path` `secs` seconds from now (negative is the past).
fn touch(path: &std::path::Path, secs: i64) {
    let now = std::time::SystemTime::now();
    let delta = std::time::Duration::from_secs(secs.unsigned_abs());
    let when = if secs < 0 { now - delta } else { now + delta };
    open_for_dating(path).unwrap().set_modified(when).unwrap();
}

// Unix sets a folder's times through a read-only handle; Windows opens folders only with
// backup semantics.
#[cfg(unix)]
fn open_for_dating(path: &std::path::Path) -> std::io::Result<std::fs::File> {
    if path.is_dir() {
        std::fs::File::open(path)
    } else {
        std::fs::File::options().write(true).open(path)
    }
}

#[cfg(windows)]
fn open_for_dating(path: &std::path::Path) -> std::io::Result<std::fs::File> {
    use std::os::windows::fs::OpenOptionsExt as _;
    const FILE_FLAG_BACKUP_SEMANTICS: u32 = 0x0200_0000;
    std::fs::File::options()
        .write(true)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS)
        .open(path)
}

/// Ages every file in `cache` by an hour, so pruning sees it as left from an earlier build.
fn age(cache: &std::path::Path) {
    for entry in std::fs::read_dir(cache).unwrap() {
        let path = entry.unwrap().path();
        if path.is_file() {
            touch(&path, -3600);
        }
    }
}

#[test]
fn each_pictured_weapon_gets_a_png_no_larger_than_128() {
    let entries = standard_entries();
    let (_tmp, tf, cache) = game(&borrowed(&entries));
    let icons = GameWeaponIcons::build(&tf, &cache).unwrap();

    let scattergun = icons.get("tf_weapon_scattergun").unwrap();
    assert!(scattergun.starts_with(&cache));
    assert_eq!(png_size(scattergun), (128, 128));
    assert_first_pixel_near(scattergun, [200, 30, 30, 255]);
    // Smaller pictures keep their size and shape.
    assert_eq!(
        png_size(icons.get("tf_weapon_shotgun_primary").unwrap()),
        (64, 32)
    );
    assert_eq!(png_size(icons.get("tf_weapon_builder").unwrap()), (16, 16));
    // The soldier shotgun's own image is not in the pak; the bat's prefabs loop.
    assert_eq!(icons.get("tf_weapon_shotgun_soldier"), None);
    assert_eq!(icons.get("tf_weapon_bat"), None);
    assert_eq!((icons.len(), icons.decoded()), (3, 3));
}

#[test]
fn a_second_build_reuses_the_cached_pngs() {
    let entries = standard_entries();
    let (_tmp, tf, cache) = game(&borrowed(&entries));
    let first = GameWeaponIcons::build(&tf, &cache).unwrap();
    let second = GameWeaponIcons::build(&tf, &cache).unwrap();
    assert_eq!(second.decoded(), 0);
    assert_eq!(second.len(), 3);
    for script in [
        "tf_weapon_scattergun",
        "tf_weapon_shotgun_primary",
        "tf_weapon_builder",
    ] {
        assert_eq!(first.get(script), second.get(script), "{script}");
    }
}

#[test]
fn a_changed_texture_is_rebuilt_and_stale_pngs_are_removed() {
    let entries = standard_entries();
    let (_tmp, tf, cache) = game(&borrowed(&entries));
    let first = GameWeaponIcons::build(&tf, &cache).unwrap();
    let old = first.get("tf_weapon_scattergun").unwrap().to_path_buf();
    common::write_file(&cache.join("junk.png"), b"not ours");
    common::write_file(&cache.join("notes.txt"), b"left alone");

    // A game update redraws the scattergun at another size.
    let mut updated = standard_entries();
    updated[0].1 = vtf(128, 128, [255, 255, 255, 255]);
    common::write_file(
        &tf.join("tf2_textures_dir.vpk"),
        &common::vpk_bytes(&borrowed(&updated)),
    );
    age(&cache);
    let second = GameWeaponIcons::build(&tf, &cache).unwrap();

    let new = second.get("tf_weapon_scattergun").unwrap();
    assert_ne!(new, old.as_path());
    assert_first_pixel_near(new, [255, 255, 255, 255]);
    assert_eq!(second.decoded(), 1);
    assert!(!old.exists(), "the old scattergun png is pruned");
    assert!(!cache.join("junk.png").exists());
    assert!(cache.join("notes.txt").exists());
    assert_eq!(
        second.get("tf_weapon_builder"),
        first.get("tf_weapon_builder")
    );
}

#[test]
fn an_undecodable_texture_leaves_out_only_its_weapon() {
    let mut entries = standard_entries();
    entries[0].1 = b"VTF\0 but nothing after".to_vec();
    let (_tmp, tf, cache) = game(&borrowed(&entries));
    let icons = GameWeaponIcons::build(&tf, &cache).unwrap();
    assert_eq!(icons.get("tf_weapon_scattergun"), None);
    assert!(icons.get("tf_weapon_shotgun_primary").is_some());
    assert_eq!(icons.len(), 2);
}

#[test]
fn a_missing_items_game_or_texture_pak_fails_the_build() {
    let entries = standard_entries();
    let (_tmp, tf, cache) = game(&borrowed(&entries));
    std::fs::remove_file(tf.join("tf2_textures_dir.vpk")).unwrap();
    let err = GameWeaponIcons::build(&tf, &cache).unwrap_err();
    assert!(matches!(err, WeaponIconsError::Textures { .. }), "{err:?}");

    std::fs::remove_file(tf.join("scripts/items/items_game.txt")).unwrap();
    let err = GameWeaponIcons::build(&tf, &cache).unwrap_err();
    assert!(matches!(err, WeaponIconsError::ItemsGame { .. }), "{err:?}");
}

#[test]
fn an_unwritable_cache_fails_the_build() {
    let entries = standard_entries();
    let (_tmp, tf, cache) = game(&borrowed(&entries));
    common::write_file(&cache, b"a file where the folder should be");
    let err = GameWeaponIcons::build(&tf, &cache).unwrap_err();
    assert!(matches!(err, WeaponIconsError::Cache { .. }), "{err:?}");
}

#[test]
fn cache_names_carry_the_icon_format_version() {
    let entries = standard_entries();
    let (_tmp, tf, cache) = game(&borrowed(&entries));
    let icons = GameWeaponIcons::build(&tf, &cache).unwrap();
    let name = icons
        .get("tf_weapon_scattergun")
        .unwrap()
        .file_name()
        .unwrap();
    assert!(
        name.to_string_lossy()
            .starts_with("tf_weapon_scattergun-v1-"),
        "{name:?}"
    );
}

#[test]
fn pruning_removes_another_processes_leftover_tmp_files() {
    let entries = standard_entries();
    let (_tmp, tf, cache) = game(&borrowed(&entries));
    // Pid 1 is the init process, never this test.
    let theirs = cache.join("tf_weapon_bat-v1-0-0.png.1.7.tmp");
    let ours = cache.join(format!(
        "tf_weapon_bat-v1-0-0.png.{}.7.tmp",
        std::process::id()
    ));
    common::write_file(&theirs, b"half a png");
    common::write_file(&ours, b"being written");
    age(&cache);
    GameWeaponIcons::build(&tf, &cache).unwrap();
    assert!(!theirs.exists());
    assert!(ours.exists());
}

#[test]
fn a_stale_png_that_cannot_be_removed_does_not_fail_the_build() {
    let entries = standard_entries();
    let (_tmp, tf, cache) = game(&borrowed(&entries));
    // A folder named like a PNG cannot be removed as a file.
    common::write_file(&cache.join("stuck.png/inside"), b"x");
    touch(&cache.join("stuck.png"), -3600);
    let icons = GameWeaponIcons::build(&tf, &cache).unwrap();
    assert_eq!(icons.len(), 3);
}

#[test]
fn pruning_spares_files_written_while_the_build_ran() {
    let entries = standard_entries();
    let (_tmp, tf, cache) = game(&borrowed(&entries));
    // Another build (TF2 moved, or a second app) writes these while this one runs.
    let png = common::write_file(&cache.join("tf_weapon_bat-v1-1-2.png"), b"theirs");
    let tmp = common::write_file(&cache.join("tf_weapon_bat-v1-1-2.png.1.tmp"), b"theirs");
    touch(&png, 60);
    touch(&tmp, 60);
    GameWeaponIcons::build(&tf, &cache).unwrap();
    assert!(png.exists());
    assert!(tmp.exists());
}

#[test]
#[ignore = "needs a TF2 game folder in TF2TOOLS_TF2_GAME"]
fn the_real_game_pictures_all_64_weapons() {
    let game = PathBuf::from(std::env::var("TF2TOOLS_TF2_GAME").expect("TF2TOOLS_TF2_GAME"));
    let cache = tempfile::tempdir().unwrap();
    let icons = GameWeaponIcons::build(&game.join("tf"), cache.path()).unwrap();
    for w in weapons() {
        let path = icons
            .get(w.script_name())
            .unwrap_or_else(|| panic!("{} has no icon", w.script_name()));
        assert_eq!(png_size(path), (128, 128), "{}", w.script_name());
    }
}
