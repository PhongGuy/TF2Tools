//! A part moved out of its parent's box, which the game cuts off there, and growing the parent
//! to hold it again, in the HUD Maker's editor state without GPUI.

use crate::support;

use tf2tools_app::pages::hud_maker::editor::{Align, HudEditor, outside_parent};
use tf2tools_core::PartKey;

const ROUND: &str = "HudMatchStatus/ObjectiveStatusTimePanel";
const TIME: &str = "HudMatchStatus/ObjectiveStatusTimePanel/TimePanelValue";
const MEDIC: &str = "HudMedicCharge";
const HV: &str = "HudPlayerStatus/HudPlayerHealth/PlayerStatusHealthValue";

fn key(k: &str) -> PartKey {
    PartKey::new(k)
}

fn rect(e: &HudEditor, k: &str) -> (i32, i32, i32, i32) {
    let b = e.layout().get(&key(k)).unwrap();
    (b.x(), b.y(), b.wide(), b.tall())
}

fn outside(e: &HudEditor, k: &str) -> Option<String> {
    outside_parent(e.model(), e.layout(), &key(k)).map(|p| p.as_str().to_owned())
}

/// The round timer's text lined up with the bottom of the medic meter, as the report did.
fn timer_aligned_to_the_bottom() -> HudEditor {
    let mut e = HudEditor::new(support::stock_model(), Vec::new());
    e.toggle_select(key(TIME));
    e.toggle_select(key(MEDIC));
    e.align_selected(Align::Bottom).unwrap();
    e.select(Some(key(TIME)));
    e
}

#[test]
fn a_part_inside_its_parent_is_not_marked() {
    let e = HudEditor::new(support::stock_model(), Vec::new());
    assert_eq!(outside(&e, TIME), None);
    assert_eq!(outside(&e, HV), None);
    assert_eq!(
        outside(&e, ROUND),
        None,
        "a part on the screen's own panel never is"
    );
}

#[test]
fn aligning_the_timer_text_down_moves_it_out_of_the_round_timer_which_cuts_it_off() {
    let e = timer_aligned_to_the_bottom();
    assert_eq!(outside(&e, TIME).as_deref(), Some(ROUND));
    assert_eq!(e.selection_outside_parent(), Some(&key(ROUND)));
    assert_eq!(
        e.layout().shown(&key(TIME)),
        None,
        "the game shows nothing of the text, as the report saw"
    );
}

#[test]
fn growing_the_round_timer_shows_the_text_where_it_sits_as_one_named_step() {
    let mut e = timer_aligned_to_the_bottom();
    let (time, round) = (rect(&e, TIME), rect(&e, ROUND));
    e.grow_parent_to_fit().unwrap();
    assert_eq!(
        rect(&e, TIME),
        time,
        "the text stays where the player put it"
    );
    assert_eq!(outside(&e, TIME), None);
    assert_eq!(e.layout().shown(&key(TIME)), Some(time), "all of it shows");
    let grown = rect(&e, ROUND);
    assert_eq!(
        (grown.0, grown.1),
        (round.0, round.1),
        "only the bottom edge moves"
    );
    assert!(grown.1 + grown.3 >= time.1 + time.3);
    // Grown by whole file units (2.25 px at 1080p), so at most one unit past the text.
    assert!(
        grown.1 + grown.3 - (time.1 + time.3) <= 3,
        "{grown:?} {time:?}"
    );
    assert_eq!(e.undo_name(), Some("grow Round timer to fit"));
    assert_eq!(e.selected(), Some(&key(TIME)), "the selection stays");
    assert!(e.undo());
    assert_eq!((rect(&e, TIME), rect(&e, ROUND)), (time, round));
    assert_eq!(outside(&e, TIME).as_deref(), Some(ROUND));
}

#[test]
fn growing_left_and_up_moves_the_parent_and_keeps_its_parts_in_place() {
    let mut e = HudEditor::new(support::stock_model(), Vec::new());
    e.select(Some(key(TIME)));
    // Left and up past the round timer's corner, in file units.
    e.nudge_selected(-60, -20).unwrap();
    let time = rect(&e, TIME);
    assert_eq!(outside(&e, TIME).as_deref(), Some(ROUND));
    e.grow_parent_to_fit().unwrap();
    assert_eq!(
        rect(&e, TIME),
        time,
        "the text stays on screen where it was"
    );
    assert_eq!(outside(&e, TIME), None);
    let round = rect(&e, ROUND);
    assert!(round.0 <= time.0 && round.1 <= time.1, "{round:?} {time:?}");
    assert!(
        time.0 - round.0 <= 3 && time.1 - round.1 <= 3,
        "grown no more than needed"
    );
}

#[test]
fn a_part_that_fits_offers_no_grow() {
    let mut e = HudEditor::new(support::stock_model(), Vec::new());
    e.select(Some(key(TIME)));
    assert!(e.grow_parent_to_fit().is_err());
    assert!(!e.can_undo());
}
