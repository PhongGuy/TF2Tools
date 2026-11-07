//! The HUD Maker's editor state, without GPUI.

mod support;

use tf2tools_app::pages::hud_maker::editor::{Handle, HudEditor, UNDO_LIMIT};
use tf2tools_core::{EditRefused, PartKey, Scene, Screen};

const HV: &str = "HudPlayerStatus/HudPlayerHealth/PlayerStatusHealthValue";

fn hv() -> PartKey {
    PartKey::new(HV)
}

fn editor() -> HudEditor {
    HudEditor::new(support::stock_model(), Vec::new())
}

fn selected() -> HudEditor {
    let mut e = editor();
    e.select(Some(hv()));
    e
}

fn rect(e: &HudEditor) -> (i32, i32, i32, i32) {
    let b = e.layout().get(&hv()).unwrap();
    (b.x(), b.y(), b.wide(), b.tall())
}

#[test]
fn a_fresh_editor_shows_the_default_screen_with_nothing_selected() {
    let e = editor();
    assert_eq!(e.screen(), Screen::new(1920, 1080));
    assert_eq!(rect(&e), (171, 927, 99, 40));
    assert_eq!(e.selected(), None);
    assert!(!e.is_dirty() && !e.can_undo() && !e.can_redo());
}

#[test]
fn a_click_picks_the_deepest_part_and_a_miss_clears() {
    let mut e = editor();
    assert_eq!(e.select_at(220, 947).map(PartKey::as_str), Some(HV));
    assert_eq!(e.selected(), Some(&hv()));
    assert_eq!(e.select_at(1900, 20), None);
    assert_eq!(e.selected(), None);
    e.select(Some(PartKey::new("NoSuchPart")));
    assert_eq!(e.selected(), None);
}

#[test]
fn an_edit_relays_out_and_is_one_undo_step() {
    let mut e = selected();
    e.move_selected(45, 0).unwrap();
    assert_eq!(rect(&e).0, 216);
    assert!(e.is_dirty() && e.can_undo());
    assert!(e.undo());
    assert_eq!(rect(&e).0, 171);
    assert!(!e.is_dirty() && e.can_redo());
    assert!(e.redo());
    assert_eq!(rect(&e).0, 216);
}

#[test]
fn edits_take_new_groups_and_an_empty_edit_records_nothing() {
    let mut e = selected();
    e.move_selected(1, 0).unwrap();
    assert!(!e.can_undo(), "one pixel is under one unit at 1080p");
    e.move_selected(45, 0).unwrap();
    e.recolour_selected("255 0 0 255").unwrap();
    let groups: Vec<u32> = e.changes().iter().map(|c| c.group()).collect();
    assert_eq!(groups.len(), 2);
    assert!(groups[1] > groups[0]);
    e.undo();
    assert_eq!(e.changes().len(), 1);
    assert_eq!(rect(&e).0, 216);
}

#[test]
fn a_refused_edit_or_no_selection_changes_nothing() {
    let mut e = editor();
    assert_eq!(e.move_selected(10, 0), Err(EditRefused::UnknownPart));
    e.select(Some(hv()));
    assert_eq!(
        e.recolour_selected("not a colour"),
        Err(EditRefused::BadValue)
    );
    assert!(!e.can_undo() && e.changes().is_empty() && !e.is_dirty());
}

#[test]
fn reset_drops_only_that_propertys_changes_as_an_undoable_step() {
    let mut e = selected();
    e.move_selected(45, 0).unwrap();
    e.recolour_selected("255 0 0 255").unwrap();
    assert!(e.changed(&["xpos", "ypos"]) && !e.changed(&["zpos"]));
    assert!(e.reset(&["xpos", "ypos"]));
    assert_eq!(rect(&e).0, 171);
    assert_eq!(e.changes().len(), 1);
    assert!(!e.reset(&["xpos", "ypos"]), "nothing left to reset");
    assert!(e.undo());
    assert_eq!(rect(&e).0, 216);
}

#[test]
fn unhide_removes_the_hide_and_keeps_the_rest() {
    let mut e = selected();
    e.move_selected(45, 0).unwrap();
    e.hide_selected().unwrap();
    assert!(e.layout().get(&hv()).unwrap().hidden().is_some());
    assert!(e.hidden_by_player());
    assert!(e.unhide_selected());
    assert!(e.layout().get(&hv()).unwrap().hidden().is_none());
    assert!(!e.hidden_by_player());
    assert_eq!(rect(&e).0, 216, "the move stays");
}

#[test]
fn a_part_hides_and_shows_by_key_as_one_step_each_and_keeps_the_selection() {
    let mut e = editor();
    let ammo = PartKey::new("HudWeaponAmmo");
    e.select(Some(ammo.clone()));
    e.set_part_hidden(&hv(), true).unwrap();
    assert!(e.layout().get(&hv()).unwrap().hidden().is_some());
    assert!(e.part_hidden_by_player(&hv()));
    assert_eq!(e.selected(), Some(&ammo));
    // The same change the "Hide this part" switch writes, so selecting HV finds it hidden.
    e.select(Some(hv()));
    assert!(e.hidden_by_player());
    e.set_part_hidden(&hv(), false).unwrap();
    assert!(!e.part_hidden_by_player(&hv()));
    assert!(e.changes().is_empty());
    assert!(e.undo());
    assert!(e.part_hidden_by_player(&hv()));
    assert!(e.undo());
    assert!(!e.part_hidden_by_player(&hv()) && !e.can_undo());
}

#[test]
fn a_part_counts_as_changed_only_while_it_differs_from_the_starting_hud() {
    let mut e = selected();
    let health = PartKey::new("HudPlayerStatus/HudPlayerHealth");
    assert!(!e.part_changed(&hv()));
    e.move_selected(45, 0).unwrap();
    assert!(e.part_changed(&hv()));
    assert!(!e.part_changed(&health), "the parent did not change");
    assert!(e.undo());
    assert!(!e.part_changed(&hv()), "undone is the starting HUD again");
    e.set_part_hidden(&health, true).unwrap();
    assert!(e.part_changed(&health));
    assert!(e.undo());
    assert!(!e.part_changed(&health));
}

#[test]
fn a_new_screen_relays_out_and_keeps_the_selection() {
    let mut e = selected();
    e.set_screen(Screen::new(1280, 1024));
    assert_eq!(e.layout().screen(), Screen::new(1280, 1024));
    assert_eq!(e.selected(), Some(&hv()));
    assert!(!e.is_dirty());
}

#[test]
fn opened_changes_count_as_saved_until_the_next_edit() {
    let m = support::stock_model();
    let changes = m
        .move_part(&hv(), 45, 0, Screen::new(1920, 1080), 7)
        .unwrap();
    let mut e = HudEditor::new(m, changes);
    assert_eq!(rect(&e).0, 216);
    assert!(!e.is_dirty());
    e.select(Some(hv()));
    e.recolour_selected("255 0 0 255").unwrap();
    assert!(e.changes()[1].group() > 7);
    assert!(e.is_dirty());
    e.mark_saved();
    assert!(!e.is_dirty());
}

#[test]
fn undo_keeps_the_last_hundred_steps() {
    let mut e = selected();
    for i in 0..=UNDO_LIMIT {
        e.recolour_selected(&format!("{} 0 0 255", i % 256))
            .unwrap();
    }
    let mut undone = 0;
    while e.undo() {
        undone += 1;
    }
    assert_eq!(undone, UNDO_LIMIT);
}

#[test]
fn the_seeded_game_holds_the_stock_hud() {
    let game = tempfile::tempdir().unwrap();
    support::seed_stock_game(game.path());
    let files = tf2tools_core::HudFiles::open_game(&game.path().join("tf")).unwrap();
    let model = tf2tools_core::HudModel::load(&files, tf2tools_core::Platform::Linux).unwrap();
    assert_eq!(model.parts().len(), support::stock_model().parts().len());
    assert!(model.part(&hv()).is_some());
}

#[test]
fn a_resize_that_rounds_to_nothing_is_refused_and_keeps_redo() {
    let mut e = selected();
    e.move_selected(45, 0).unwrap();
    e.undo();
    assert_eq!(e.resize_selected(Some(1), None), Err(EditRefused::BadValue));
    assert_eq!(e.resize_selected(None, Some(1)), Err(EditRefused::BadValue));
    assert!(e.changes().is_empty() && !e.can_undo() && e.can_redo());
    e.resize_selected(Some(3), None).unwrap();
    assert!(rect(&e).2 >= 1, "one file unit stays visible");
}

#[test]
fn a_new_edit_clears_redo() {
    let mut e = selected();
    e.move_selected(45, 0).unwrap();
    e.undo();
    assert!(e.can_redo());
    e.recolour_selected("255 0 0 255").unwrap();
    assert!(!e.can_redo() && !e.redo());
}

#[test]
fn redo_keeps_the_last_hundred_steps() {
    let mut e = selected();
    for i in 0..=UNDO_LIMIT {
        e.recolour_selected(&format!("{} 0 0 255", i % 256))
            .unwrap();
    }
    while e.undo() {}
    let mut redone = 0;
    while e.redo() {
        redone += 1;
    }
    assert_eq!(redone, UNDO_LIMIT);
}

#[test]
fn a_drag_previews_then_commits_as_one_step() {
    let mut e = selected();
    e.drag(Handle::Move, 20, 0).unwrap();
    e.drag(Handle::Move, 45, 0).unwrap();
    assert!(e.is_dragging());
    assert_eq!(rect(&e).0, 216);
    assert!(!e.can_undo());
    e.end_drag();
    assert!(!e.is_dragging());
    assert_eq!(rect(&e).0, 216);
    assert!(e.undo());
    assert_eq!(rect(&e).0, 171);
    assert!(!e.can_undo());
}

#[test]
fn the_right_handle_resizes_and_the_left_one_also_moves() {
    let mut e = selected();
    e.drag(Handle::Right, 36, 0).unwrap();
    e.end_drag();
    assert_eq!(rect(&e), (171, 927, 135, 40));
    e.drag(Handle::Left, -45, 0).unwrap();
    e.end_drag();
    assert_eq!(rect(&e), (126, 927, 180, 40));
    let last: Vec<u32> = e
        .changes()
        .iter()
        .rev()
        .take(2)
        .map(|c| c.group())
        .collect();
    assert_eq!(last[0], last[1], "one group for the move and the resize");
    assert!(e.undo());
    assert_eq!(rect(&e), (171, 927, 135, 40));
}

#[test]
fn cancel_and_undo_drop_a_running_preview() {
    let mut e = selected();
    e.drag(Handle::Move, 45, 0).unwrap();
    e.cancel_drag();
    assert_eq!(rect(&e).0, 171);
    assert!(!e.is_dirty());
    e.move_selected(45, 0).unwrap();
    e.drag(Handle::Bottom, 0, 45).unwrap();
    assert!(e.undo());
    assert!(!e.is_dragging());
    assert_eq!(rect(&e), (216, 927, 99, 40), "only the drag is undone");
    assert!(e.can_undo());
}

#[test]
fn a_resize_never_goes_below_one_unit() {
    let mut e = selected();
    e.drag(Handle::Right, -500, 0).unwrap();
    e.end_drag();
    let b = e.layout().get(&hv()).unwrap();
    assert!(b.wide() > 0 && b.hidden().is_none(), "{b:?}");
}

#[test]
fn only_the_snapshot_taken_at_save_counts_as_saved() {
    let mut e = selected();
    e.move_selected(45, 0).unwrap();
    let snapshot = e.changes().to_vec();
    e.recolour_selected("255 0 0 255").unwrap();
    e.mark_saved_as(snapshot, e.assets().clone());
    assert!(e.is_dirty(), "the edit made during the save is unsaved");
    e.undo();
    assert!(!e.is_dirty());
}

// Screens whose scale is not a whole number of pixels per unit, where rounding shows.
const ODD_SCREENS: [Screen; 3] = [
    Screen::new(1280, 1024),
    Screen::new(1920, 1200),
    Screen::new(1366, 768),
];

/// HV's file value for `key` as plain units.
fn units(e: &HudEditor, key: &str) -> i32 {
    let part = e.model().part(&hv()).unwrap();
    let text = part.value(key).unwrap().text();
    text.parse().unwrap_or_else(|_| panic!("{key} is {text:?}"))
}

/// The far edge on each axis, in file units and in pixels.
fn far_edges(e: &HudEditor) -> ((i32, i32), (i32, i32)) {
    let (x, y, w, t) = rect(e);
    (
        (
            units(e, "xpos") + units(e, "wide"),
            units(e, "ypos") + units(e, "tall"),
        ),
        (x + w, y + t),
    )
}

// The far edge stays put in the file; on screen the game truncates position and size apart, so
// it may land one pixel off. A position pushed below zero can land two off, which these drags
// never reach.
fn assert_still(e: &HudEditor, start: ((i32, i32), (i32, i32)), what: &str) {
    let (units_now, px_now) = far_edges(e);
    assert_eq!(units_now, start.0, "file units after {what}");
    let off = ((px_now.0 - start.1.0).abs(), (px_now.1 - start.1.1).abs());
    assert!(
        off.0 <= 1 && off.1 <= 1,
        "pixels {px_now:?} vs {:?} after {what}",
        start.1
    );
}

#[test]
fn left_and_top_drags_keep_the_opposite_edge_still() {
    for screen in ODD_SCREENS {
        for d in [-41, -17, -9, -3, 2, 5, 11, 23] {
            let mut e = selected();
            e.set_screen(screen);
            let start = far_edges(&e);
            for handle in [Handle::Left, Handle::Top, Handle::TopLeft] {
                e.drag(handle, d, d).unwrap();
                e.end_drag();
                assert_still(&e, start, &format!("{handle:?} {d:+} at {screen:?}"));
            }
        }
    }
}

#[test]
fn a_handle_drag_that_goes_nowhere_records_nothing() {
    use Handle::*;
    for screen in ODD_SCREENS {
        for handle in [
            Left,
            Right,
            Top,
            Bottom,
            TopLeft,
            TopRight,
            BottomLeft,
            BottomRight,
        ] {
            let mut e = selected();
            e.set_screen(screen);
            let before = rect(&e);
            let _ = e.drag(handle, 0, 0);
            e.end_drag();
            assert_eq!(rect(&e), before, "{handle:?} at {screen:?}");
            assert!(!e.can_undo(), "{handle:?} at {screen:?} recorded a step");
        }
    }
}

#[test]
fn drags_stay_in_the_typed_ranges() {
    let screen = Screen::new(1920, 1080);
    let mut e = selected();
    e.drag(Handle::BottomRight, 100_000, 100_000).unwrap();
    e.end_drag();
    let (_, _, w, t) = rect(&e);
    assert!(w <= 4 * 1920 && t <= 4 * 1080, "size {w}x{t}");
    let mut e = selected();
    e.drag(Handle::Move, 100_000, -100_000).unwrap();
    e.end_drag();
    let (x, y, _, _) = rect(&e);
    assert!(
        x <= 4 * 1920 && y >= -4 * 1080,
        "position {x},{y} at {screen:?}"
    );
    let mut e = selected();
    e.drag(Handle::Left, 100_000, 0).unwrap();
    e.end_drag();
    let (x, _, w, _) = rect(&e);
    assert!(w >= 1 && x <= 4 * 1920, "left past the right edge: {x} {w}");
}

#[test]
fn a_part_flat_on_one_axis_still_drags_along_the_other() {
    let flat = support::stock_model()
        .resize_part(&hv(), None, Some(0), Screen::new(1920, 1080), 0)
        .unwrap();
    let mut e = HudEditor::new(support::stock_model(), flat);
    e.select(Some(hv()));
    let (x, _, wide, tall) = rect(&e);
    assert_eq!(tall, 0);
    e.drag(Handle::Move, 45, 0).unwrap();
    e.end_drag();
    assert_eq!(rect(&e).0, x + 45);
    e.drag(Handle::Right, 45, 0).unwrap();
    e.end_drag();
    assert!(rect(&e).2 > wide);
    // A drag on the flat axis keeps the minimum size, so it never stays flat.
    e.drag(Handle::Bottom, 0, -1).unwrap();
    assert!(rect(&e).3 >= 1);
}

const BLUE: &str = "scores/BluePlayerList";

fn board() -> HudEditor {
    let mut e = editor();
    e.set_scene(Scene::Scoreboard);
    e
}

fn spacing(e: &HudEditor) -> Option<String> {
    e.model()
        .part(&PartKey::new(BLUE))?
        .value("linespacing")
        .map(|l| l.text().to_owned())
}

#[test]
fn switching_scene_keeps_the_changes_and_clears_the_selection() {
    let mut e = selected();
    e.move_selected(45, 0).unwrap();
    e.set_scene(Scene::Scoreboard);
    assert_eq!(e.scene(), Scene::Scoreboard);
    assert_eq!(e.selected(), None);
    assert!(e.layout().get(&PartKey::new("scores")).is_some());
    assert!(e.layout().get(&hv()).is_none());
    assert!(e.is_dirty() && e.can_undo());
    assert_eq!(e.dropped(), 0);
    e.set_scene(Scene::Hud);
    assert_eq!(rect(&e).0, 216);
}

#[test]
fn the_same_scene_keeps_the_selection() {
    let mut e = selected();
    e.set_scene(Scene::Hud);
    assert_eq!(e.selected(), Some(&hv()));
}

#[test]
fn switching_scene_cancels_a_running_drag() {
    let mut e = selected();
    e.drag(Handle::Move, 10, 0).unwrap();
    e.set_scene(Scene::Scoreboard);
    assert!(!e.is_dragging() && !e.is_dirty());
    e.set_scene(Scene::Hud);
    assert_eq!(rect(&e).0, 171);
}

#[test]
fn a_number_row_writes_its_key_and_refuses_what_core_refuses() {
    let mut e = board();
    e.select(Some(PartKey::new(BLUE)));
    assert_eq!(spacing(&e).as_deref(), Some("18"));
    assert_eq!(
        e.set_number_selected("linespacing", 1921),
        Err(EditRefused::BadValue)
    );
    assert_eq!(
        e.set_number_selected("linespacing", -1),
        Err(EditRefused::BadValue)
    );
    assert_eq!(
        e.set_number_selected("xpos", 5),
        Err(EditRefused::NotOffered)
    );
    assert!(!e.can_undo());
    e.set_number_selected("linespacing", 24).unwrap();
    assert_eq!(spacing(&e).as_deref(), Some("24"));
    assert_eq!(e.selected(), Some(&PartKey::new(BLUE)));
    assert!(e.undo());
    assert_eq!(spacing(&e).as_deref(), Some("18"));
}

#[test]
fn undo_and_redo_go_to_the_scene_of_the_change() {
    let mut e = board();
    e.select(Some(PartKey::new(BLUE)));
    e.set_number_selected("linespacing", 24).unwrap();
    e.set_scene(Scene::Hud);
    e.select(Some(hv()));
    e.move_selected(45, 0).unwrap();
    e.set_scene(Scene::Scoreboard);
    // The last step moved HV, so undoing it shows the HUD.
    assert!(e.undo());
    assert_eq!(e.scene(), Scene::Hud);
    assert_eq!(rect(&e).0, 171);
    assert!(e.undo());
    assert_eq!(e.scene(), Scene::Scoreboard);
    assert_eq!(e.selected(), None);
    assert_eq!(spacing(&e).as_deref(), Some("18"));
    e.set_scene(Scene::Hud);
    assert!(e.redo());
    assert_eq!(e.scene(), Scene::Scoreboard);
    assert_eq!(spacing(&e).as_deref(), Some("24"));
    assert!(e.redo());
    assert_eq!(e.scene(), Scene::Hud);
    assert_eq!(rect(&e).0, 216);
}
