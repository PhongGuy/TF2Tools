//! The HUD Maker editor's picture and menu background steps, without GPUI.

use crate::support;

use support::hud_stock::{HEALTH, block, stock_files_with, with_blocks};
use tf2tools_app::pages::hud_maker::editor::{HudEditor, PictureRefused};
use tf2tools_core::{
    HudModel, PartKey, PictureTarget, Platform, RgbaImage, hud_texture_vtf, picture_id,
};

const UNDER: &str = "HudPlayerStatus/HudPlayerHealth/";

fn k(name: &str) -> PartKey {
    PartKey::new(format!("{UNDER}{name}"))
}

fn editor() -> HudEditor {
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
    HudEditor::new(HudModel::load(&files, Platform::Linux).unwrap(), Vec::new())
}

/// A `side` x `side` texture of one grey `level`, so different levels give different ids.
fn vtf(side: u32, level: u8) -> Vec<u8> {
    let pixels = vec![level; (side * side * 4) as usize];
    hud_texture_vtf(&RgbaImage::from_pixels(side, side, pixels).unwrap()).unwrap()
}

fn image_of(e: &HudEditor, name: &str) -> Option<String> {
    e.model()
        .part(&k(name))?
        .value("image")
        .map(|l| l.text().to_owned())
}

fn shown(bytes: &[u8]) -> String {
    format!("replay/thumbnails/tf2tools/{}", picture_id(bytes))
}

#[test]
fn the_selection_s_target_comes_from_core() {
    let mut e = editor();
    assert_eq!(e.picture_target_selected(), None);
    e.select(Some(k("PlayerStatusHealthImage")));
    assert!(matches!(
        e.picture_target_selected(),
        Some(PictureTarget::Code { .. })
    ));
    e.select(Some(k("PlayerStatusHealthValue")));
    assert_eq!(e.picture_target_selected(), None);
}

#[test]
fn a_pick_is_one_undo_step_and_its_bytes_outlive_the_undo() {
    let mut e = editor();
    e.select(Some(k("Badge")));
    let bytes = vtf(4, 120);
    e.replace_picture_selected(bytes.clone()).unwrap();
    assert_eq!(image_of(&e, "Badge"), Some(shown(&bytes)));
    let kept = format!(
        ".tf2tools/images/{0}/materials/vgui/replay/thumbnails/tf2tools/{0}.vtf",
        picture_id(&bytes)
    );
    assert_eq!(e.assets().get(&kept), Some(bytes.as_slice()));
    assert!(e.undo());
    assert_eq!(
        image_of(&e, "Badge").as_deref(),
        Some("replay/thumbnails/old")
    );
    assert!(!e.can_undo());
    assert!(e.redo());
    assert_eq!(image_of(&e, "Badge"), Some(shown(&bytes)));
    assert_eq!(e.assets().get(&kept), Some(bytes.as_slice()));
}

#[test]
fn each_picture_is_named_by_its_bytes() {
    let mut e = editor();
    e.select(Some(k("Badge")));
    e.replace_picture_selected(vtf(4, 10)).unwrap();
    e.replace_picture_selected(vtf(4, 20)).unwrap();
    assert_eq!(image_of(&e, "Badge"), Some(shown(&vtf(4, 20))));
    assert!(e.undo());
    assert_eq!(image_of(&e, "Badge"), Some(shown(&vtf(4, 10))));
    e.replace_picture_selected(vtf(4, 10)).unwrap();
    assert_eq!(
        image_of(&e, "Badge"),
        Some(shown(&vtf(4, 10))),
        "the same bytes, the same name"
    );
}

#[test]
fn a_label_offers_no_picture_and_nothing_changes() {
    let mut e = editor();
    e.select(Some(k("PlayerStatusHealthValue")));
    assert_eq!(
        e.replace_picture_selected(vtf(4, 1)),
        Err(PictureRefused::NotOffered)
    );
    assert!(e.changes().is_empty() && !e.can_undo());
}

#[test]
fn the_picture_caps_refuse_the_33rd_pick() {
    let mut e = editor();
    e.select(Some(k("PlayerStatusHealthImage")));
    for level in 0..32 {
        e.replace_picture_selected(vtf(2, level)).unwrap();
    }
    let before = e.changes().len();
    assert_eq!(
        e.replace_picture_selected(vtf(2, 99)),
        Err(PictureRefused::HudFull)
    );
    assert_eq!(e.changes().len(), before);
}

#[test]
fn reset_drops_the_part_s_picks_in_one_step() {
    let mut e = editor();
    e.select(Some(k("PlayerStatusHealthImage")));
    e.replace_picture_selected(vtf(2, 1)).unwrap();
    e.replace_picture_selected(vtf(2, 2)).unwrap();
    assert!(e.reset_picture_selected());
    assert!(e.changes().is_empty());
    assert!(!e.reset_picture_selected(), "nothing left to reset");
    assert!(e.undo());
    assert_eq!(e.changes().len(), 4);
}

#[test]
fn the_menu_background_is_one_step_and_resets() {
    let mut e = editor();
    assert!(!e.has_menu_background());
    e.set_menu_background(vtf(4, 1), vtf(4, 2)).unwrap();
    assert!(e.has_menu_background());
    assert!(
        e.model()
            .assets()
            .iter()
            .any(|(p, _)| p == "scripts/chapterbackgrounds.txt")
    );
    assert!(e.reset_menu_background());
    assert!(!e.has_menu_background());
    assert!(e.undo());
    assert!(e.has_menu_background());
}

#[test]
fn a_restored_snapshot_keeps_the_bytes_of_an_overridden_pick() {
    let mut first = editor();
    first.select(Some(k("PlayerStatusHealthImage")));
    first.replace_picture_selected(vtf(2, 1)).unwrap();
    first.replace_picture_selected(vtf(2, 2)).unwrap();
    let (changes, held) = (first.changes().to_vec(), first.assets().clone());
    let mut fresh = editor();
    assert!(fresh.restore(changes, &held));
    assert!(fresh.undo());
    for level in [1, 2] {
        let source = format!(
            ".tf2tools/images/{}/materials/hud/health_color.vmt",
            picture_id(&vtf(2, level))
        );
        assert!(fresh.assets().get(&source).is_some(), "{source}");
    }
}
