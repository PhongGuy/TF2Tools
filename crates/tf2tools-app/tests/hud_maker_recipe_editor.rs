//! The HUD Maker editor's recipe previews, without GPUI.

use crate::support;

use support::hud_stock::stock_files;
use tf2tools_app::pages::hud_maker::editor::{Handle, HudEditor};
use tf2tools_core::{
    Change, HudAssets, HudModel, OptionValue, PartKey, Platform, Recipe, Screen, SetKey,
};

const HV: &str = "HudPlayerStatus/HudPlayerHealth/PlayerStatusHealthValue";

fn editor() -> HudEditor {
    HudEditor::new(
        HudModel::load(&stock_files(), Platform::Linux).unwrap(),
        Vec::new(),
    )
}

fn red_recipe() -> Recipe {
    let set = SetKey::new(
        "resource/ui/hudplayerhealth.res",
        &["Resource/UI/HudPlayerHealth.res", "PlayerStatusHealthValue"],
        "fgcolor",
        None,
        None,
        "255 0 0 255",
    );
    Recipe::new("Red", &[Change::set(0, set)], &HudAssets::new())
}

fn colour(e: &HudEditor) -> Option<String> {
    e.model()
        .part(&PartKey::new(HV))?
        .value("fgcolor")
        .map(|l| l.text().to_owned())
}

#[test]
fn a_preview_is_not_a_change() {
    let mut e = editor();
    assert_eq!(e.preview_recipe(&red_recipe()), 0);
    assert!(e.previewing());
    assert_eq!(colour(&e).as_deref(), Some("255 0 0 255"));
    assert!(e.changes().is_empty());
    assert!(!e.is_dirty() && !e.can_undo());
}

#[test]
fn apply_is_one_undo_step() {
    let mut e = editor();
    e.preview_recipe(&red_recipe());
    assert!(e.apply_preview());
    assert!(!e.previewing());
    assert_eq!(e.changes().len(), 1);
    assert!(e.undo());
    assert_eq!(colour(&e).as_deref(), Some("TanDark"));
    assert!(!e.can_undo());
    assert!(!e.apply_preview());
}

#[test]
fn cancel_shows_the_hud_again() {
    let mut e = editor();
    e.preview_recipe(&red_recipe());
    e.cancel_preview();
    assert!(!e.previewing());
    assert_eq!(colour(&e).as_deref(), Some("TanDark"));
}

#[test]
fn an_edit_cancels_the_preview() {
    let mut e = editor();
    e.preview_recipe(&red_recipe());
    e.select(Some(PartKey::new(HV)));
    assert!(!e.previewing());
    e.set_property_selected("labelText", "HP").unwrap();
    assert_eq!(colour(&e).as_deref(), Some("TanDark"));
    assert_eq!(e.changes().len(), 1);
}

#[test]
fn undo_during_a_preview_drops_it_and_steps_back() {
    let mut e = editor();
    e.select(Some(PartKey::new(HV)));
    e.set_property_selected("labelText", "HP").unwrap();
    e.select(None);
    e.preview_recipe(&red_recipe());
    assert!(e.undo());
    assert!(!e.previewing());
    assert!(e.changes().is_empty());
}

#[test]
fn canvas_clicks_drags_and_options_cancel_the_preview() {
    let mut e = editor();
    e.preview_recipe(&red_recipe());
    e.select_at(0, 0);
    assert!(!e.previewing());

    e.select(Some(PartKey::new(HV)));
    e.preview_recipe(&red_recipe());
    let _ = e.drag(Handle::Move, 10, 0);
    assert!(!e.previewing());
    e.end_drag();
    assert_eq!(colour(&e).as_deref(), Some("TanDark"));

    e.select(None);
    e.preview_recipe(&red_recipe());
    e.set_option("item_normal", OptionValue::Rgba([1, 2, 3, 255]))
        .unwrap();
    assert!(!e.previewing());
    assert_eq!(colour(&e).as_deref(), Some("TanDark"));

    e.preview_recipe(&red_recipe());
    e.set_screen(Screen::new(1280, 1024));
    assert!(!e.previewing());
}

#[test]
fn a_second_preview_replaces_the_first() {
    let mut e = editor();
    e.preview_recipe(&red_recipe());
    let empty = Recipe::new("Empty", &[], &HudAssets::new());
    e.preview_recipe(&empty);
    assert_eq!(colour(&e).as_deref(), Some("TanDark"));
    assert!(e.apply_preview());
    assert!(e.changes().is_empty());
    assert!(!e.can_undo(), "an empty apply records no step");
}

const FONT_SOURCE: &str = ".tf2tools/fonts/big_numbers.ttf";

// An editor on the stock HUD with the health value selected, ready to take a font file.
fn editor_on_health() -> HudEditor {
    let mut e = editor();
    e.select(Some(PartKey::new(HV)));
    e
}

// A recipe that adds "Big Numbers.ttf" with `bytes` to the health value.
fn font_recipe(bytes: Vec<u8>) -> Recipe {
    let mut maker = editor_on_health();
    maker
        .add_font_file_selected("font", "Big Numbers.ttf", bytes)
        .unwrap();
    Recipe::new("Fonts", maker.changes(), maker.assets())
}

#[test]
fn a_cancelled_preview_leaves_no_font_bytes() {
    let theirs = support::fonts::sfnt_bytes(&[(1, "Big Numbers"), (2, "Regular")]);
    let mut e = editor();
    e.preview_recipe(&font_recipe(theirs.clone()));
    assert_eq!(e.shown_assets().get(FONT_SOURCE), Some(theirs.as_slice()));
    assert_eq!(
        e.assets().get(FONT_SOURCE),
        None,
        "kept apart until applied"
    );
    e.cancel_preview();
    assert_eq!(e.assets().get(FONT_SOURCE), None);
    assert_eq!(e.shown_assets().get(FONT_SOURCE), None);
}

#[test]
fn an_applied_preview_keeps_its_font_bytes() {
    let theirs = support::fonts::sfnt_bytes(&[(1, "Big Numbers"), (2, "Regular")]);
    let mut e = editor();
    e.preview_recipe(&font_recipe(theirs.clone()));
    assert!(e.apply_preview());
    assert_eq!(e.assets().get(FONT_SOURCE), Some(theirs.as_slice()));
}

#[test]
fn a_cancelled_preview_never_replaces_an_undone_fonts_bytes() {
    let mine = support::fonts::sfnt_bytes(&[(1, "Big Numbers")]);
    let theirs = support::fonts::sfnt_bytes(&[(1, "Big Numbers"), (2, "Regular")]);
    let mut e = editor_on_health();
    e.add_font_file_selected("font", "Big Numbers.ttf", mine.clone())
        .unwrap();
    assert!(e.undo());
    e.preview_recipe(&font_recipe(theirs));
    e.cancel_preview();
    assert!(e.redo());
    assert_eq!(e.assets().get(FONT_SOURCE), Some(mine.as_slice()));
}

#[test]
fn a_font_the_hud_cant_use_is_counted_apart_from_the_changes() {
    let theirs = support::fonts::sfnt_bytes(&[(1, "Big Numbers"), (2, "Regular")]);
    // The same recipe read back with bytes that are no font: apply refuses the font.
    let mut json: serde_json::Value =
        serde_json::from_slice(&font_recipe(theirs).to_json()).unwrap();
    json["assets"][FONT_SOURCE] = "AAAA".into();
    let (junk, _) = Recipe::parse(&serde_json::to_vec(&json).unwrap()).unwrap();
    let mut e = editor();
    let skipped = e.preview_recipe(&junk);
    assert_eq!(e.preview_fonts_left_out(), 1);
    assert!(skipped >= 1, "the font is in the skipped count");
    e.cancel_preview();
    assert_eq!(e.preview_fonts_left_out(), 0, "nothing previewing");
}

#[test]
fn a_previewed_borrow_counts_the_changes_that_did_not_apply() {
    let mut e = editor();
    let fits = SetKey::new(
        "resource/ui/hudplayerhealth.res",
        &["Resource/UI/HudPlayerHealth.res", "PlayerStatusHealthValue"],
        "fgcolor",
        None,
        None,
        "255 0 0 255",
    );
    let lost = SetKey::new(
        "resource/ui/hudplayerhealth.res",
        &["Resource/UI/HudPlayerHealth.res", "NoSuchBlock"],
        "xpos",
        None,
        None,
        "5",
    );
    let added = vec![Change::set(0, fits), Change::set(0, lost)];
    assert_eq!(e.preview_changes(added, &HudAssets::new()), 1);
    assert!(e.previewing());
}
