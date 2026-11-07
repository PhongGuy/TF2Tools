//! The HUD Maker editor's Stage 2 edits, without GPUI.

use crate::support;

use support::hud_stock::{HEALTH, stock_files_with};
use tf2tools_app::pages::hud_maker::editor::{FontAddRefused, HudEditor};
use tf2tools_core::{ChangeTarget, EditRefused, FontEdit, HudAssets, HudModel, PartKey, Platform};

const HV: &str = "HudPlayerStatus/HudPlayerHealth/PlayerStatusHealthValue";
const METER: &str = "HudItemEffectMeter";

pub const FONTS: &str = r#""Scheme" { "Colors" { "TanDark" "117 107 94 255" "TanLight" "235 226 202 255" }
  "Fonts" { "Default" { "1" { "name" "Verdana" "tall" "12" } }
            "HudFontSmall" { "1" { "name" "TF2 Build" "tall" "14" } } } }"#;

fn editor_on(key: &str) -> HudEditor {
    // The health value keeps no `font` line, so its font is the row's default.
    let health = HEALTH.replace("    font HudClassHealth\n", "");
    let files = stock_files_with(&[
        ("resource/clientscheme.res", Some(FONTS)),
        ("resource/ui/hudplayerhealth.res", Some(health.as_str())),
    ]);
    let mut e = HudEditor::new(HudModel::load(&files, Platform::Linux).unwrap(), Vec::new());
    e.select(Some(PartKey::new(key)));
    e
}

fn value(e: &HudEditor, part: &str, key: &str) -> Option<String> {
    e.model()
        .part(&PartKey::new(part))?
        .value(key)
        .map(|l| l.text().to_owned())
}

#[test]
fn a_stage_2_row_edit_is_one_undo_step() {
    let mut e = editor_on(HV);
    e.set_property_selected("labelText", "HP %Health%").unwrap();
    assert_eq!(value(&e, HV, "labelText").as_deref(), Some("HP %Health%"));
    assert!(e.undo());
    assert_eq!(value(&e, HV, "labelText").as_deref(), Some("%Health%"));
    assert!(e.can_redo());
    assert_eq!(
        e.set_property_selected("allcaps", "maybe"),
        Err(EditRefused::BadValue)
    );
}

#[test]
fn a_repeated_property_edit_replaces_the_last_step() {
    let mut e = editor_on(HV);
    e.set_property_selected("fgcolor", "10 10 10 255").unwrap();
    e.set_property_selected_again("fgcolor", "20 20 20 255")
        .unwrap();
    e.set_property_selected_again("fgcolor", "30 30 30 255")
        .unwrap();
    assert_eq!(value(&e, HV, "fgcolor").as_deref(), Some("30 30 30 255"));
    assert!(e.undo());
    assert_eq!(value(&e, HV, "fgcolor").as_deref(), Some("TanDark"));
    assert!(!e.can_undo());
}

#[test]
fn any_setting_edits_as_text_and_removes() {
    let mut e = editor_on(METER);
    e.set_key_text_selected("tall", "60").unwrap();
    assert_eq!(value(&e, METER, "tall").as_deref(), Some("60"));
    e.remove_key_selected("xpos").unwrap();
    assert_eq!(value(&e, METER, "xpos"), None);
    assert!(e.undo());
    assert_eq!(value(&e, METER, "xpos").as_deref(), Some("r174"));
    assert_eq!(
        e.remove_key_selected("visible"),
        Err(EditRefused::NotOffered)
    );
}

#[test]
fn a_font_edit_clones_the_parts_font() {
    let mut e = editor_on(HV);
    e.edit_font_selected("font", FontEdit::Outline(true))
        .unwrap();
    assert_eq!(value(&e, HV, "font").as_deref(), Some("TF2Tools_1"));
    let font = e
        .model()
        .scheme()
        .font("TF2Tools_1", e.screen().tall())
        .unwrap();
    assert!(font.outline());
    assert!(e.undo());
    assert!(e.model().scheme().font("TF2Tools_1", 1080).is_none());
}

#[test]
fn adding_a_font_file_sets_the_family_in_one_step() {
    let mut e = editor_on(HV);
    let bytes = support::fonts::sfnt_bytes(&[(1, "Big Numbers")]);
    let family = e
        .add_font_file_selected("font", "Big Numbers.ttf", bytes.clone())
        .unwrap();
    assert_eq!(family, "Big Numbers");
    assert_eq!(
        e.assets().get(".tf2tools/fonts/big_numbers.ttf"),
        Some(bytes.as_slice())
    );
    let font = value(&e, HV, "font").unwrap();
    assert_eq!(
        e.model().scheme().font(&font, 1080).unwrap().family(),
        "Big Numbers"
    );
    assert!(
        e.changes()
            .iter()
            .any(|c| matches!(c.target(), ChangeTarget::Asset(_)))
    );
    assert_eq!(e.font_families(), ["Big Numbers", "TF2 Build", "Verdana"]);
    assert!(e.undo());
    assert!(e.changes().is_empty(), "one step");
    assert_eq!(
        e.font_families(),
        ["TF2 Build", "Verdana"],
        "an undone font is not offered"
    );
}

#[test]
fn an_added_font_offers_every_family_it_carries() {
    let mut e = editor_on(HV);
    let bytes = support::fonts::sfnt_bytes(&[(1, "Team Fortress"), (1, "Counter-Strike")]);
    let family = e.add_font_file_selected("font", "tf.ttf", bytes).unwrap();
    assert_eq!(family, "Team Fortress");
    assert_eq!(
        e.font_families(),
        ["Counter-Strike", "Team Fortress", "TF2 Build", "Verdana"]
    );
}

#[test]
fn a_bad_font_file_changes_nothing() {
    let mut e = editor_on(HV);
    assert_eq!(
        e.add_font_file_selected("font", "x.ttf", b"nope".to_vec()),
        Err(FontAddRefused::NotAFont)
    );
    let good = support::fonts::sfnt_bytes(&[(1, "Big Numbers")]);
    assert_eq!(
        e.add_font_file_selected("font", "notes.txt", good),
        Err(FontAddRefused::NotAFont)
    );
    assert!(e.changes().is_empty() && e.assets().is_empty() && !e.can_undo());
}

#[test]
fn a_good_font_file_is_refused_for_the_part_not_the_file() {
    // The part's font has no block in the HUD's own scheme, so it has nothing to clone.
    let files = stock_files_with(&[(
        "resource/clientscheme.res",
        Some(r#""Scheme" { "Colors" { "TanDark" "117 107 94 255" } }"#),
    )]);
    let mut e = HudEditor::new(HudModel::load(&files, Platform::Linux).unwrap(), Vec::new());
    e.select(Some(PartKey::new(HV)));
    let bytes = support::fonts::sfnt_bytes(&[(1, "Big Numbers")]);
    assert_eq!(
        e.add_font_file_selected("font", "Big Numbers.ttf", bytes),
        Err(FontAddRefused::PartFont)
    );
    assert!(e.changes().is_empty() && e.assets().is_empty() && !e.can_undo());
}

#[test]
fn a_font_whose_name_belongs_to_the_system_is_refused_for_its_name() {
    let mut e = editor_on(HV);
    let bytes = support::fonts::sfnt_bytes(&[(1, "Marlett")]);
    assert_eq!(
        e.add_font_file_selected("font", "marlett.ttf", bytes),
        Err(FontAddRefused::BadName)
    );
    assert!(e.changes().is_empty());
}

#[test]
fn a_reopened_hud_keeps_its_assets() {
    let mut assets = HudAssets::new();
    assets.insert(
        ".tf2tools/fonts/a.ttf",
        support::fonts::sfnt_bytes(&[(1, "A Font")]),
    );
    let e = editor_on(HV).with_assets(assets.clone());
    assert_eq!(e.assets(), &assets);
    assert_eq!(
        e.font_families(),
        ["TF2 Build", "Verdana"],
        "bytes alone, with no applied asset, add no family"
    );
}

#[test]
fn a_different_font_file_of_the_same_name_after_save_is_unsaved() {
    let mut e = editor_on(HV);
    let first = support::fonts::sfnt_bytes(&[(1, "Big Numbers")]);
    let second = support::fonts::sfnt_bytes(&[(1, "Big Numbers"), (2, "Regular")]);
    assert_ne!(first, second);
    e.add_font_file_selected("font", "Big Numbers.ttf", first)
        .unwrap();
    e.mark_saved_as(e.changes().to_vec(), e.assets().clone());
    assert!(!e.is_dirty());
    assert!(e.undo());
    e.add_font_file_selected("font", "Big Numbers.ttf", second)
        .unwrap();
    assert!(
        e.is_dirty(),
        "the HUD now holds bytes that were never saved"
    );
}
