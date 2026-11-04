//! Nudges, Mirror, Match size and snapping in the HUD Maker's editor state, without GPUI.

mod support;

use tf2tools_app::pages::hud_maker::editor::{Handle, HudEditor};
use tf2tools_core::{EditRefused, PartKey};

const HV: &str = "HudPlayerStatus/HudPlayerHealth/PlayerStatusHealthValue";
const IMAGE: &str = "HudPlayerStatus/HudPlayerHealth/PlayerStatusHealthImage";

fn editor() -> HudEditor {
    HudEditor::new(support::stock_model(), Vec::new())
}

fn selected() -> HudEditor {
    let mut e = editor();
    e.select(Some(PartKey::new(HV)));
    e
}

fn rect(e: &HudEditor) -> (i32, i32, i32, i32) {
    let b = e.layout().get(&PartKey::new(HV)).unwrap();
    (b.x(), b.y(), b.wide(), b.tall())
}

#[test]
fn a_nudge_is_one_undo_step_of_whole_units() {
    let mut e = selected();
    e.nudge_selected(1, 0).unwrap();
    assert_eq!(rect(&e).0, 173);
    e.nudge_selected(0, -10).unwrap();
    assert_eq!(rect(&e).1, 904, "42 units below the panel's top at 810");
    assert!(e.undo());
    assert_eq!(rect(&e), (173, 927, 99, 40));
    assert!(e.undo());
    assert_eq!(rect(&e).0, 171);
    assert!(!e.can_undo());
}

#[test]
fn without_a_selection_nothing_changes() {
    let mut e = editor();
    assert_eq!(e.nudge_selected(1, 0), Err(EditRefused::UnknownPart));
    assert_eq!(e.mirror_selected(), Err(EditRefused::UnknownPart));
    assert_eq!(
        e.match_size_selected(&PartKey::new(IMAGE)),
        Err(EditRefused::UnknownPart)
    );
    assert!(!e.can_undo());
}

const MEDIC: &str = "HudMedicCharge";

fn medic(e: &HudEditor) -> (i32, i32, i32, i32) {
    let b = e.layout().get(&PartKey::new(MEDIC)).unwrap();
    (b.x(), b.y(), b.wide(), b.tall())
}

#[test]
fn mirror_and_match_size_are_one_undo_step_each() {
    let mut e = editor();
    e.select(Some(PartKey::new(MEDIC)));
    e.mirror_selected().unwrap();
    assert_eq!(medic(&e), (821, 832, 225, 90), "c-38 becomes c-62");
    e.match_size_selected(&PartKey::new(IMAGE)).unwrap();
    assert_eq!(
        medic(&e),
        (821, 832, 114, 114),
        "c-62 keeps the left edge as it grows"
    );
    assert_eq!(e.selected(), Some(&PartKey::new(MEDIC)));
    assert!(e.undo());
    assert_eq!(medic(&e), (821, 832, 225, 90));
    assert!(e.undo());
    assert_eq!(medic(&e), (875, 832, 225, 90));
}

#[test]
fn a_part_measured_against_the_screen_in_a_narrow_parent_is_not_mirrored() {
    let mut e = selected();
    assert_eq!(e.mirror_selected(), Err(EditRefused::NotOffered));
    assert!(!e.can_undo());
}

#[test]
fn a_snap_measures_from_where_the_drag_began() {
    let mut e = selected();
    let s = e.snap_move(45, 0, 6).unwrap();
    assert_eq!((s.dx(), s.dy()), (50, 1));
    e.drag(Handle::Move, 45, 0).unwrap();
    let s = e.snap_move(45, 0, 6).unwrap();
    assert_eq!(
        (s.dx(), s.dy()),
        (50, 1),
        "the preview's own box is not the start"
    );
    e.end_drag();
    assert!(
        editor().snap_move(45, 7, 6).is_none(),
        "nothing selected snaps nothing"
    );
}
