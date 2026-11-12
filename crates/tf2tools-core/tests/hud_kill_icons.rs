//! Killfeed icons from the game's HUD icon tables: a prefix per file key, texel regions, the
//! `dneg_` fallback to `d_`, and `mod_textures.txt` searched before `hud_textures.txt`.

use tf2tools_core::{HudFiles, Platform};

const MOD: &str = r#""sprites/640_hud"
{
  TextureFileRefs { "dfile" { "prefix" "d_" } "dnegfile" { "prefix" "dneg_" } }
  TextureData
  {
    "plus_sign" { "font" "Icons" "character" "+" }
    "tf_projectile_rocket" { "dfile" "HUD/d_images" "dnegfile" "HUD/dneg_images" "x" "0" "y" "224" "width" "96" "height" "32" }
    "obj_sentrygun" { "dfile" "HUD/d_images" "x" "96" "y" "160" "y" "192" "width" "64" "height" "32" }
    "empty" { "dfile" "HUD/d_images" "x" "0" "y" "0" "width" "0" "height" "32" }
    "shared" { "file" "sprites/mod_shared" "x" "1" "y" "2" "width" "3" "height" "4" }
  }
}"#;

const HUD: &str = r#""sprites/640_hud"
{
  TextureData
  {
    "shared" { "file" "sprites/hud_shared" "x" "9" "y" "9" "width" "9" "height" "9" }
    "arrow" { "file" "sprites/arrow" "x" "0" "y" "0" "width" "32" "height" "32" }
  }
}"#;

fn files() -> HudFiles {
    HudFiles::from_memory(vec![vec![
        (
            "scripts/mod_textures.txt".to_owned(),
            MOD.as_bytes().to_vec(),
        ),
        (
            "scripts/hud_textures.txt".to_owned(),
            HUD.as_bytes().to_vec(),
        ),
    ]])
}

fn icon(name: &str) -> Option<(String, u32, u32, u32, u32)> {
    let icon = files().kill_icon(name, Platform::Linux).unwrap()?;
    Some((
        icon.image().to_owned(),
        icon.x(),
        icon.y(),
        icon.wide(),
        icon.tall(),
    ))
}

#[test]
fn a_prefixed_name_reads_its_file_keys_region() {
    assert_eq!(
        icon("d_tf_projectile_rocket"),
        Some(("../HUD/d_images".to_owned(), 0, 224, 96, 32))
    );
    assert_eq!(
        icon("DNEG_tf_projectile_rocket"),
        Some(("../HUD/dneg_images".to_owned(), 0, 224, 96, 32))
    );
}

#[test]
fn an_inverted_icon_the_table_lacks_falls_back_on_the_plain_one() {
    assert_eq!(
        icon("dneg_obj_sentrygun"),
        Some(("../HUD/d_images".to_owned(), 96, 160, 64, 32)),
        "the first of two y keys wins"
    );
}

#[test]
fn mod_textures_wins_and_hud_textures_fills_in() {
    assert_eq!(
        icon("shared"),
        Some(("../sprites/mod_shared".to_owned(), 1, 2, 3, 4))
    );
    assert_eq!(
        icon("arrow"),
        Some(("../sprites/arrow".to_owned(), 0, 0, 32, 32))
    );
}

#[test]
fn font_glyphs_empty_regions_and_unknown_names_are_none() {
    assert_eq!(icon("plus_sign"), None);
    assert_eq!(icon("d_empty"), None);
    assert_eq!(icon("d_nothing"), None);
    assert_eq!(
        icon("tf_projectile_rocket"),
        None,
        "the bare name has no file key"
    );
    let none = HudFiles::from_memory(vec![vec![]]);
    assert_eq!(none.kill_icon("d_x", Platform::Linux).unwrap(), None);
}

#[test]
fn the_tables_are_read_once_and_answer_every_name_after() {
    // The parsed tables own their entries, so lookups after the files are gone read nothing.
    let icons = files().kill_icons(Platform::Linux).unwrap();
    let region = |name: &str| {
        icons
            .get(name)
            .map(|i| (i.image().to_owned(), i.x(), i.y()))
    };
    assert_eq!(
        region("dneg_tf_projectile_rocket"),
        Some(("../HUD/dneg_images".to_owned(), 0, 224))
    );
    assert_eq!(
        region("dneg_obj_sentrygun"),
        Some(("../HUD/d_images".to_owned(), 96, 160))
    );
    assert_eq!(region("arrow"), Some(("../sprites/arrow".to_owned(), 0, 0)));
    assert_eq!(region("d_nothing"), None);
}

#[test]
fn names_lists_every_drawn_icon_once_with_its_prefix() {
    let icons = files().kill_icons(Platform::Linux).unwrap();
    assert_eq!(
        icons.names(),
        [
            "d_tf_projectile_rocket",
            "dneg_tf_projectile_rocket",
            "d_obj_sentrygun",
            "shared",
            "arrow",
        ],
        "glyphs and empty regions are left out; hud_textures' shared is shadowed"
    );
    for name in icons.names() {
        assert!(icons.get(&name).is_some(), "{name} answers get");
    }
    let none = HudFiles::from_memory(vec![vec![]]);
    assert!(none.kill_icons(Platform::Linux).unwrap().names().is_empty());
}
