//! The main menu's background from one picture, and pictures left out of shared recipes.

mod common;

use common::hud_stock::{HEALTH, block, stock_files, stock_files_with, with_blocks};
use tf2tools_core::{
    ChangeTarget, EditRefused, HudAssets, HudFiles, HudImage, HudModel, PartKey, Platform, Recipe,
    RgbaImage, hud_texture_vtf, menu_textures,
};

fn textures() -> (Vec<u8>, Vec<u8>) {
    let (narrow, wide) =
        menu_textures(&RgbaImage::from_pixels(16, 9, vec![200; 16 * 9 * 4]).unwrap());
    (
        hud_texture_vtf(&narrow).unwrap(),
        hud_texture_vtf(&wide).unwrap(),
    )
}

fn model() -> HudModel {
    HudModel::load(&stock_files(), Platform::Linux).unwrap()
}

fn game_layer(files: &[(String, Vec<u8>)]) -> HudFiles {
    let layer = files
        .iter()
        .map(|(s, b)| (s.splitn(4, '/').nth(3).unwrap().to_owned(), b.clone()))
        .collect();
    HudFiles::from_memory(vec![layer])
}

#[test]
fn one_pick_writes_the_list_two_materials_and_two_textures() {
    let (narrow, wide) = textures();
    let new = model().set_menu_background(narrow, wide, 9).unwrap();
    let mut paths: Vec<String> = new
        .changes()
        .iter()
        .map(|c| match c.target() {
            ChangeTarget::Asset(a) => a.path().to_owned(),
            other => panic!("not an asset: {other:?}"),
        })
        .collect();
    paths.sort();
    assert_eq!(paths.len(), 5);
    assert_eq!(
        &paths[..2],
        [
            "materials/console/background_upward.vmt",
            "materials/console/background_upward_widescreen.vmt"
        ]
    );
    assert!(
        paths[2].starts_with("materials/vgui/replay/thumbnails/tf2tools/")
            && paths[3].ends_with("_widescreen.vtf")
    );
    assert_eq!(paths[4], "scripts/chapterbackgrounds.txt");
    assert!(new.changes().iter().all(|c| c.group() == 9));
}

#[test]
fn the_written_files_preview_as_the_menu_background() {
    let (narrow, wide) = textures();
    let new = model().set_menu_background(narrow, wide, 9).unwrap();
    let top = game_layer(new.files());
    assert_eq!(
        top.menu_background(false, Platform::Linux)
            .unwrap()
            .as_deref(),
        Some("../console/background_upward")
    );
    assert_eq!(
        top.menu_background(true, Platform::Linux)
            .unwrap()
            .as_deref(),
        Some("../console/background_upward_widescreen")
    );
    let size = |name: &str| match top.image(name, Platform::Linux).unwrap() {
        HudImage::Found(t) => (t.image().width(), t.image().height()),
        other => panic!("{other:?}"),
    };
    assert_eq!(size("../console/background_upward"), (1024, 1024));
    assert_eq!(
        size("../console/background_upward_widescreen"),
        (2048, 1024)
    );
}

#[test]
fn something_that_is_not_a_texture_is_refused() {
    let (narrow, _) = textures();
    assert_eq!(
        model()
            .set_menu_background(narrow, b"nope".to_vec(), 1)
            .err(),
        Some(EditRefused::BadValue)
    );
}

#[test]
fn reset_drops_every_menu_pick_and_keeps_the_rest() {
    let m = model();
    let (narrow, wide) = textures();
    let mut changes = m
        .set_menu_background(narrow.clone(), wide.clone(), 1)
        .unwrap()
        .into_parts()
        .0;
    let key = PartKey::new("HudPlayerStatus/HudPlayerHealth/PlayerStatusHealthValue");
    changes.extend(m.recolour_part(&key, "255 0 0 255", 2).unwrap());
    changes.extend(
        m.set_menu_background(narrow, wide, 3)
            .unwrap()
            .into_parts()
            .0,
    );
    let kept = m.reset_menu_background(&changes);
    assert!(!kept.is_empty() && kept.iter().all(|c| c.group() == 2));
}

#[test]
fn a_shared_recipe_leaves_pictures_out_and_counts_them() {
    let m = model();
    let (narrow, wide) = textures();
    let (mut changes, files) = m.set_menu_background(narrow, wide, 1).unwrap().into_parts();
    let key = PartKey::new("HudPlayerStatus/HudPlayerHealth/PlayerStatusHealthValue");
    changes.extend(m.recolour_part(&key, "255 0 0 255", 2).unwrap());
    let mut assets = HudAssets::new();
    for (source, bytes) in files {
        assets.insert(&source, bytes);
    }
    let recipe = Recipe::new("Menu", &changes, &assets);
    assert_eq!(recipe.pictures_left_out(), 5);
    assert_eq!(
        recipe.fonts_left_out(),
        5,
        "the existing count still counts every asset left out"
    );
    assert!(recipe.changes().iter().all(|c| c.group() == 2));
}

#[test]
fn a_shared_recipe_drops_a_keyed_pick_whole() {
    let badge = block(
        "Badge",
        "ImagePanel",
        &[
            ("wide", "20"),
            ("tall", "20"),
            ("image", "replay/thumbnails/old"),
        ],
    );
    let health = with_blocks(HEALTH, &badge);
    let files = stock_files_with(&[("resource/ui/hudplayerhealth.res", Some(&health))]);
    let m = HudModel::load(&files, Platform::Linux).unwrap();
    let vtf = hud_texture_vtf(&RgbaImage::from_pixels(2, 2, vec![9; 16]).unwrap()).unwrap();
    let key = PartKey::new("HudPlayerStatus/HudPlayerHealth/Badge");
    let (changes, files) = m.replace_picture(&key, vtf, 1).unwrap().into_parts();
    let mut assets = HudAssets::new();
    for (source, bytes) in files {
        assets.insert(&source, bytes);
    }
    let recipe = Recipe::new("Badge", &changes, &assets);
    assert_eq!(recipe.pictures_left_out(), 2);
    assert!(
        !recipe.changes().iter().any(|c| matches!(c.target(), ChangeTarget::Set(s)
            if s.key().eq_ignore_ascii_case("image") || s.key().eq_ignore_ascii_case("scaleImage"))),
        "no dangling picture key survives"
    );
    assert!(recipe.changes().is_empty());
}
