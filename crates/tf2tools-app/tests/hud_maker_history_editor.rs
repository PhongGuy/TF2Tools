//! The HUD Maker editor's history: restoring a snapshot and laying one out to compare, without
//! GPUI.

use crate::support;

use support::hud_stock::stock_files;
use tf2tools_app::pages::hud_maker::editor::HudEditor;
use tf2tools_core::{
    BoardMode, Change, HudAssets, HudModel, PartKey, Platform, Recipe, Scene, Screen, SetKey,
};

const HV: &str = "HudPlayerStatus/HudPlayerHealth/PlayerStatusHealthValue";
const LABEL: &str = "scores/ServerLabel";
const FONT_SOURCE: &str = ".tf2tools/fonts/big_numbers.ttf";

fn editor() -> HudEditor {
    HudEditor::new(
        HudModel::load(&stock_files(), Platform::Linux).unwrap(),
        Vec::new(),
    )
}

fn colour_change(group: u32, colour: &str) -> Change {
    let set = SetKey::new(
        "resource/ui/hudplayerhealth.res",
        &["Resource/UI/HudPlayerHealth.res", "PlayerStatusHealthValue"],
        "fgcolor",
        None,
        None,
        colour,
    );
    Change::set(group, set)
}

fn colour_in(model: &HudModel) -> Option<String> {
    model
        .part(&PartKey::new(HV))?
        .value("fgcolor")
        .map(|l| l.text().to_owned())
}

#[test]
fn restore_is_one_undo_step() {
    let mut e = editor();
    e.select(Some(PartKey::new(HV)));
    e.recolour_selected("1 1 1 255").unwrap();
    let mine = e.changes().to_vec();
    let then = vec![colour_change(0, "255 0 0 255")];
    assert!(e.restore(then.clone(), &HudAssets::new()));
    assert_eq!(e.changes(), then);
    assert_eq!(colour_in(e.model()).as_deref(), Some("255 0 0 255"));
    assert!(e.is_dirty());
    assert!(e.undo());
    assert_eq!(e.changes(), mine);
    assert!(e.redo());
    assert_eq!(e.changes(), then);
}

#[test]
fn restoring_the_changes_as_they_are_records_nothing() {
    let mut e = editor();
    assert!(!e.restore(Vec::new(), &HudAssets::new()));
    assert!(!e.can_undo());
}

#[test]
fn restore_ends_a_preview_and_a_comparison() {
    let mut e = editor();
    let red = vec![colour_change(0, "255 0 0 255")];
    e.preview_recipe(&Recipe::new("Red", &red, &HudAssets::new()));
    e.compare_with(red);
    assert!(e.restore(vec![colour_change(0, "0 0 255 255")], &HudAssets::new()));
    assert!(!e.previewing());
    assert!(e.compared().is_none());
    assert_eq!(colour_in(e.model()).as_deref(), Some("0 0 255 255"));
}

#[test]
fn restore_takes_font_bytes_the_editor_lacks_and_keeps_its_own() {
    let bytes = support::fonts::sfnt_bytes(&[(1, "Big Numbers")]);
    let mut maker = editor();
    maker.select(Some(PartKey::new(HV)));
    maker
        .add_font_file_selected("font", "Big Numbers.ttf", bytes.clone())
        .unwrap();
    let with_font = maker.changes().to_vec();
    let mut kept = HudAssets::new();
    kept.insert(FONT_SOURCE, bytes.clone());

    let mut e = editor();
    assert!(e.restore(with_font.clone(), &kept));
    assert_eq!(e.assets().get(FONT_SOURCE), Some(bytes.as_slice()));

    // Bytes the editor already holds for the file win over the snapshot's kept copy.
    let other = support::fonts::sfnt_bytes(&[(1, "Big Numbers"), (2, "Regular")]);
    let mut stale = HudAssets::new();
    stale.insert(FONT_SOURCE, other);
    assert!(maker.restore(Vec::new(), &HudAssets::new()));
    assert!(maker.restore(with_font, &stale));
    assert_eq!(maker.assets().get(FONT_SOURCE), Some(bytes.as_slice()));
}

#[test]
fn a_comparison_is_laid_out_once_and_leaves_the_changes_alone() {
    let mut e = editor();
    e.compare_with(vec![colour_change(0, "255 0 0 255")]);
    assert_eq!(e.compare_builds(), 1);
    let shown = e.compared().unwrap();
    assert_eq!(colour_in(shown.model()).as_deref(), Some("255 0 0 255"));
    assert!(shown.layout().get(&PartKey::new(HV)).is_some());
    assert_eq!(colour_in(e.model()).as_deref(), Some("TanDark"));
    assert!(e.changes().is_empty() && !e.can_undo() && !e.is_dirty());

    // Editing "now" never lays the snapshot out again.
    e.select(Some(PartKey::new(HV)));
    e.recolour_selected("0 255 0 255").unwrap();
    assert!(e.undo());
    assert!(e.redo());
    assert_eq!(e.compare_builds(), 1);
    assert_eq!(
        colour_in(e.compared().unwrap().model()).as_deref(),
        Some("255 0 0 255")
    );
}

#[test]
fn a_new_screen_or_scene_lays_the_comparison_out_again() {
    let mut e = editor();
    e.compare_with(vec![colour_change(0, "255 0 0 255")]);
    let hv_x = |e: &HudEditor| {
        let layout = e.compared().unwrap().layout();
        layout.get(&PartKey::new(HV)).unwrap().x()
    };
    let at_1080 = hv_x(&e);
    e.set_screen(Screen::new(1280, 1024));
    assert_eq!(e.compare_builds(), 2);
    assert_ne!(hv_x(&e), at_1080);
    e.set_scene(Scene::Scoreboard);
    assert_eq!(e.compare_builds(), 3);
    assert_eq!(e.compared().unwrap().model().scene(), Scene::Scoreboard);
    e.stop_comparing();
    assert!(e.compared().is_none());
}

#[test]
fn a_board_mode_change_lays_the_comparison_out_again() {
    let mut e = editor();
    e.set_scene(Scene::Scoreboard);
    e.compare_with(Vec::new());
    assert_eq!(e.compare_builds(), 1);
    e.set_board_mode(BoardMode::Mvm);
    assert_eq!(e.compare_builds(), 2);
    assert_eq!(e.compared().unwrap().model().board_mode(), BoardMode::Mvm);
    // The same mode again is a no-op.
    e.set_board_mode(BoardMode::Mvm);
    assert_eq!(e.compare_builds(), 2);
}

#[test]
fn an_undo_across_scenes_lays_the_comparison_out_again() {
    let mut e = editor();
    e.select(Some(PartKey::new(HV)));
    e.recolour_selected("1 1 1 255").unwrap();
    e.set_scene(Scene::Scoreboard);
    e.compare_with(Vec::new());
    assert_eq!(e.compare_builds(), 1);
    // The step was made on the HUD scene, so undo follows it there.
    assert!(e.undo());
    assert_eq!(e.scene(), Scene::Hud);
    assert_eq!(e.compare_builds(), 2);
    assert_eq!(e.compared().unwrap().model().scene(), Scene::Hud);
    assert!(e.redo());
    assert_eq!(
        e.compare_builds(),
        2,
        "redo of a step made on the HUD scene stays"
    );
}

#[test]
fn undoing_a_restore_follows_a_scoreboard_change_to_its_scene_and_mode() {
    // A change list whose first difference from the empty list is in the Scoreboard's file.
    let mut maker = editor();
    maker.set_scene(Scene::Scoreboard);
    maker.select(Some(PartKey::new(LABEL)));
    maker.move_selected(20, 0).unwrap();
    let board = maker.changes().to_vec();

    let mut e = editor();
    e.set_scene(Scene::Scoreboard);
    e.set_board_mode(BoardMode::Mvm);
    e.set_scene(Scene::Hud);
    assert_eq!(e.board_mode(), BoardMode::Mvm);
    assert!(e.restore(board, &HudAssets::new()));
    assert_eq!(e.scene(), Scene::Hud, "restore itself does not travel");

    // Undo shows the player the step: it enters the Scoreboard in the mode it was made under,
    // and a comparison open at that moment follows.
    e.set_board_mode(BoardMode::Casual);
    e.compare_with(Vec::new());
    let builds = e.compare_builds();
    assert!(e.undo());
    assert_eq!(e.scene(), Scene::Scoreboard);
    assert_eq!(e.board_mode(), BoardMode::Mvm);
    assert_eq!(e.compare_builds(), builds + 1);
    let shown = e.compared().unwrap().model();
    assert_eq!(
        (shown.scene(), shown.board_mode()),
        (Scene::Scoreboard, BoardMode::Mvm)
    );
}
