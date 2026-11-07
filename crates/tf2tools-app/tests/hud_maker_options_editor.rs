//! The HUD Maker editor's option edits, without GPUI.

use crate::support;

use support::hud_stock::{HUDLAYOUT, stock_files_with, with_blocks};
use tf2tools_app::pages::hud_maker::editor::HudEditor;
use tf2tools_core::{EditRefused, HudModel, OptionValue, PartKey, Platform};

const SCHEME: &str = r#""Scheme" { Colors { TanDark "117 107 94 255" TanLight "235 226 202 255"
  QualityColorNormal "178 178 178 255" QualityColorUnique "255 215 0 255" } }"#;
const BASECHAT: &str = r#""Resource/UI/BaseChat.res" { HudChat { xpos 10 ypos 275 } }"#;
const MAINMENU: &str = r#""Resource/UI/MainMenuOverride.res" { FriendsContainer { visible 1 } }"#;

fn editor() -> HudEditor {
    let layout = with_blocks(
        HUDLAYOUT,
        "  HudDeathNotice { fieldName HudDeathNotice xpos r640 ypos 18 wide 628 tall 468 MaxDeathNotices 4 }\n",
    );
    let files = stock_files_with(&[
        ("resource/clientscheme.res", Some(SCHEME)),
        ("scripts/hudlayout.res", Some(layout.as_str())),
        ("resource/ui/basechat.res", Some(BASECHAT)),
        ("resource/ui/mainmenuoverride.res", Some(MAINMENU)),
    ]);
    HudEditor::new(HudModel::load(&files, Platform::Linux).unwrap(), Vec::new())
}

fn rows(e: &HudEditor) -> Option<OptionValue> {
    e.option_value("killfeed_rows")
}

#[test]
fn an_option_is_one_undo_step() {
    let mut e = editor();
    e.set_option("killfeed_rows", OptionValue::Int(6)).unwrap();
    assert_eq!(rows(&e), Some(OptionValue::Int(6)));
    assert!(e.is_dirty());
    assert!(e.undo());
    assert_eq!(rows(&e), Some(OptionValue::Int(4)));
    assert!(e.redo());
    assert_eq!(rows(&e), Some(OptionValue::Int(6)));
}

#[test]
fn the_same_choice_twice_records_one_step() {
    let mut e = editor();
    e.set_option("chat_top", OptionValue::Bool(true)).unwrap();
    e.set_option("chat_top", OptionValue::Bool(true)).unwrap();
    assert!(e.undo());
    assert!(!e.can_undo());
}

#[test]
fn switching_back_leaves_nothing_stored() {
    let mut e = editor();
    e.set_option("chat_top", OptionValue::Bool(true)).unwrap();
    e.set_option("chat_top", OptionValue::Bool(false)).unwrap();
    assert!(e.changes().is_empty());
    assert!(e.can_undo());
}

#[test]
fn a_previewed_option_commits_once() {
    let mut e = editor();
    e.select(Some(PartKey::new("HudWeaponAmmo")));
    e.preview_option("killfeed_rows", OptionValue::Int(6))
        .unwrap();
    e.preview_option("killfeed_rows", OptionValue::Int(7))
        .unwrap();
    assert!(e.is_dragging());
    assert_eq!(rows(&e), Some(OptionValue::Int(7)));
    assert_eq!(e.selected(), Some(&PartKey::new("HudWeaponAmmo")));
    e.end_drag();
    assert!(e.undo());
    assert_eq!(rows(&e), Some(OptionValue::Int(4)));
    assert!(!e.can_undo());
}

#[test]
fn a_cancelled_preview_leaves_nothing() {
    let mut e = editor();
    e.preview_option("killfeed_rows", OptionValue::Int(9))
        .unwrap();
    e.cancel_drag();
    assert_eq!(rows(&e), Some(OptionValue::Int(4)));
    assert!(e.changes().is_empty() && !e.can_undo());
}

#[test]
fn a_preview_back_to_the_stored_value_records_nothing() {
    let mut e = editor();
    e.set_option("killfeed_rows", OptionValue::Int(6)).unwrap();
    e.mark_saved();
    e.preview_option("killfeed_rows", OptionValue::Int(7))
        .unwrap();
    e.preview_option("killfeed_rows", OptionValue::Int(6))
        .unwrap();
    e.end_drag();
    assert_eq!(rows(&e), Some(OptionValue::Int(6)));
    assert!(!e.is_dirty());
    assert!(e.undo());
    assert!(!e.can_undo());
}

#[test]
fn reset_drops_one_group_as_one_step() {
    let mut e = editor();
    e.set_option("item_normal", OptionValue::Rgba([1, 2, 3, 255]))
        .unwrap();
    e.set_option("killfeed_rows", OptionValue::Int(6)).unwrap();
    assert!(e.option_group_changed("Item colours"));
    assert!(e.reset_option_group("Item colours"));
    assert!(!e.option_group_changed("Item colours"));
    assert_eq!(
        e.option_value("item_normal"),
        Some(OptionValue::Rgba([178, 178, 178, 255]))
    );
    assert_eq!(rows(&e), Some(OptionValue::Int(6)));
    assert!(e.undo());
    assert_eq!(
        e.option_value("item_normal"),
        Some(OptionValue::Rgba([1, 2, 3, 255]))
    );
    assert!(!e.reset_option_group("Main menu"));
}

#[test]
fn a_refused_option_records_nothing() {
    let mut e = editor();
    assert_eq!(
        e.set_option("nope", OptionValue::Bool(true)),
        Err(EditRefused::NotOffered)
    );
    assert_eq!(
        e.preview_option("killfeed_rows", OptionValue::Int(99)),
        Err(EditRefused::BadValue)
    );
    assert!(!e.can_undo() && !e.is_dragging());
}
