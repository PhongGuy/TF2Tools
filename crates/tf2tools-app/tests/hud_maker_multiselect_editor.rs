//! Selecting several parts in the HUD Maker's editor state, moving and lining them up, snapping
//! a resize and stepping a part's layer, without GPUI.

mod support;

use tf2tools_app::pages::hud_maker::editor::{Align, Handle, HudEditor, Spread};
use tf2tools_core::{GuideAxis, PartKey};

const HV: &str = "HudPlayerStatus/HudPlayerHealth/PlayerStatusHealthValue";
const HEALTH: &str = "HudPlayerStatus/HudPlayerHealth";
const MEDIC: &str = "HudMedicCharge";
const AMMO: &str = "HudWeaponAmmo";

fn key(k: &str) -> PartKey {
    PartKey::new(k)
}

fn editor() -> HudEditor {
    HudEditor::new(support::stock_model(), Vec::new())
}

/// An editor holding `parts` selected, added one by one as Shift-clicks would.
fn holding(parts: &[&str]) -> HudEditor {
    let mut e = editor();
    for part in parts {
        e.toggle_select(key(part));
    }
    e
}

fn rect(e: &HudEditor, k: &str) -> (i32, i32, i32, i32) {
    let b = e.layout().get(&key(k)).unwrap();
    (b.x(), b.y(), b.wide(), b.tall())
}

fn selection(e: &HudEditor) -> Vec<&str> {
    e.selection().iter().map(PartKey::as_str).collect()
}

#[test]
fn toggling_adds_and_removes_parts_and_one_left_is_the_plain_selection() {
    let mut e = holding(&[HV]);
    assert_eq!(e.selected(), Some(&key(HV)));
    e.toggle_select(key(MEDIC));
    assert_eq!(selection(&e), [HV, MEDIC]);
    assert_eq!(
        e.selected(),
        None,
        "several parts have no single part's rows"
    );
    assert!(e.several());
    e.toggle_select(key(HV));
    assert_eq!(selection(&e), [MEDIC]);
    assert_eq!(e.selected(), Some(&key(MEDIC)));
    e.select(None);
    assert!(e.selection().is_empty());
}

#[test]
fn a_part_inside_a_selected_group_takes_the_group_out() {
    let mut e = holding(&[HEALTH, MEDIC]);
    e.toggle_select(key(HV));
    assert_eq!(
        selection(&e),
        [MEDIC, HV],
        "a part and its group never move twice"
    );
    e.toggle_select(key(HEALTH));
    assert_eq!(selection(&e), [MEDIC, HEALTH]);
}

#[test]
fn several_parts_drag_together_as_one_named_step() {
    let mut e = holding(&[HEALTH, MEDIC]);
    let (hv, medic) = (rect(&e, HV), rect(&e, MEDIC));
    e.drag(Handle::Move, 45, 0).unwrap();
    e.drag(Handle::Move, 90, 0).unwrap();
    e.end_drag();
    // Each part lands on the nearest whole file unit, within a pixel of the pointer's delta.
    assert!((rect(&e, HV).0 - (hv.0 + 90)).abs() <= 1);
    assert!((rect(&e, MEDIC).0 - (medic.0 + 90)).abs() <= 1);
    assert_eq!(e.undo_name(), Some("move 2 parts"));
    assert!(e.undo());
    assert_eq!((rect(&e, HV), rect(&e, MEDIC)), (hv, medic));
    assert!(!e.can_undo(), "the whole drag was one step");
    assert_eq!(selection(&e), [HEALTH, MEDIC], "undo keeps the selection");
}

#[test]
fn several_parts_never_take_a_resize_handle() {
    let mut e = holding(&[HV, MEDIC]);
    assert!(e.drag(Handle::Right, 40, 0).is_err());
    assert!(!e.is_dragging());
}

#[test]
fn several_parts_snap_as_one_box() {
    let e = holding(&[HV, MEDIC]);
    let medic = rect(&e, MEDIC);
    let right = medic.0 + medic.2;
    // The pair's box ends at `right`; a move bringing that edge within 6 px of the screen centre
    // lands it on the nearest line there: the centre, or a centred panel's middle a pixel off.
    let s = e.snap_move(960 - right - 4, 0, 6).unwrap();
    let across = (s.guides().iter()).find(|g| g.axis() == GuideAxis::Vertical);
    let at = across.expect("a vertical guide").at();
    assert!((959..=960).contains(&at), "{at}");
    assert_eq!(
        s.dx(),
        at - right,
        "the box's right edge lands on the guide"
    );
}

#[test]
fn align_lines_the_parts_up_on_the_boxes_edges_as_one_step() {
    let mut e = holding(&[HV, MEDIC]);
    let (hv, medic) = (rect(&e, HV), rect(&e, MEDIC));
    // A file unit is 2.25 px at 1080p, so a part lands within one unit of the line.
    let near = |a: i32, b: i32| (a - b).abs() <= 2;
    e.align_selected(Align::Left).unwrap();
    assert!(near(rect(&e, MEDIC).0, hv.0));
    assert_eq!(rect(&e, HV).0, hv.0, "the part already on the line stays");
    assert_eq!(e.undo_name(), Some("align 2 parts"));
    assert!(e.undo());
    assert_eq!((rect(&e, HV), rect(&e, MEDIC)), (hv, medic));

    e.align_selected(Align::Top).unwrap();
    assert!(near(rect(&e, HV).1, medic.1.min(hv.1)));
    assert!(near(rect(&e, MEDIC).1, medic.1.min(hv.1)));
    assert!(e.undo());

    e.align_selected(Align::Bottom).unwrap();
    let bottom = |r: (i32, i32, i32, i32)| r.1 + r.3;
    let want = bottom(hv).max(bottom(medic));
    assert!(near(bottom(rect(&e, HV)), want));
    assert!(near(bottom(rect(&e, MEDIC)), want));
}

#[test]
fn centre_and_middle_line_up_the_parts_centres() {
    let mut e = holding(&[HV, MEDIC]);
    e.align_selected(Align::Centre).unwrap();
    let centre = |r: (i32, i32, i32, i32)| r.0 + r.2 / 2;
    let (hv, medic) = (rect(&e, HV), rect(&e, MEDIC));
    assert!((centre(hv) - centre(medic)).abs() <= 2, "{hv:?} {medic:?}");
    e.align_selected(Align::Middle).unwrap();
    let middle = |r: (i32, i32, i32, i32)| r.1 + r.3 / 2;
    let (hv, medic) = (rect(&e, HV), rect(&e, MEDIC));
    assert!((middle(hv) - middle(medic)).abs() <= 2, "{hv:?} {medic:?}");
}

#[test]
fn spreading_three_parts_leaves_equal_gaps_between_them() {
    // The meter starts well off the even spot, so spacing out moves it.
    let mut e = holding(&[MEDIC]);
    e.nudge_selected(-60, 0).unwrap();
    e.select(None);
    for part in [HV, MEDIC, AMMO] {
        e.toggle_select(key(part));
    }
    let before: Vec<_> = [HV, MEDIC, AMMO].map(|k| rect(&e, k)).to_vec();
    e.distribute_selected(Spread::Across).unwrap();
    let mut after: Vec<_> = [HV, MEDIC, AMMO].map(|k| rect(&e, k)).to_vec();
    after.sort_by_key(|r| r.0);
    let gap = |a: (i32, i32, i32, i32), b: (i32, i32, i32, i32)| b.0 - (a.0 + a.2);
    assert!(
        (gap(after[0], after[1]) - gap(after[1], after[2])).abs() <= 2,
        "{after:?}"
    );
    let (first, last) = (
        before.iter().map(|r| r.0).min().unwrap(),
        before.iter().map(|r| r.0 + r.2).max().unwrap(),
    );
    assert_eq!(after[0].0, first, "the outer parts stay");
    assert_eq!(after[2].0 + after[2].2, last);
    assert_eq!(e.undo_name(), Some("space out 3 parts"));
}

#[test]
fn spreading_needs_three_parts_and_aligning_two() {
    let mut e = holding(&[HV, MEDIC]);
    assert!(!e.can_distribute());
    assert!(e.can_align());
    assert!(e.distribute_selected(Spread::Down).is_err());
    let mut one = holding(&[HV]);
    assert!(!one.can_align());
    assert!(one.align_selected(Align::Left).is_err());
}

#[test]
fn hiding_several_parts_is_one_step() {
    let mut e = holding(&[HV, MEDIC]);
    e.hide_selected().unwrap();
    assert!(e.part_hidden_by_player(&key(HV)));
    assert!(e.part_hidden_by_player(&key(MEDIC)));
    assert_eq!(e.undo_name(), Some("hide 2 parts"));
    assert!(e.undo());
    assert!(!e.part_hidden_by_player(&key(HV)));
    assert!(!e.part_hidden_by_player(&key(MEDIC)));
}

#[test]
fn arrow_keys_nudge_every_selected_part() {
    let mut e = holding(&[HV, MEDIC]);
    let (hv, medic) = (rect(&e, HV), rect(&e, MEDIC));
    e.nudge_selected(0, 10).unwrap();
    assert!(rect(&e, HV).1 > hv.1 && rect(&e, MEDIC).1 > medic.1);
    assert!(e.undo());
    assert!(!e.can_undo());
}

#[test]
fn a_resize_snaps_its_moving_edge_and_shows_a_guide() {
    let mut e = holding(&[HV]);
    let hv = rect(&e, HV);
    // The right edge comes 4 px short of the screen centre and snaps onto the line there.
    let right = hv.0 + hv.2;
    let grow = 960 - right - 4;
    let s = e.snap_resize(Handle::Right, grow, 0, 6).unwrap();
    assert_eq!(s.guides().len(), 1);
    let at = s.guides()[0].at();
    assert!((959..=960).contains(&at), "{at}");
    assert_eq!(s.dx(), at - right);
    assert_eq!(s.dy(), 0, "an axis the handle leaves never snaps");
    // Far from every line, the resize goes where the pointer is.
    let s = e.snap_resize(Handle::Bottom, 0, 300, 0).unwrap();
    assert_eq!((s.dx(), s.dy()), (0, 300));
    assert!(s.guides().is_empty());
    e.select(None);
    assert!(e.snap_resize(Handle::Right, grow, 0, 6).is_none());
}

#[test]
fn bring_forward_and_send_back_step_past_the_next_sibling() {
    let mut e = holding(&[HV]);
    let rank = |e: &HudEditor| e.layout().paint_rank(&key(HV)).unwrap();
    let start = rank(&e);
    assert!(
        e.can_layer(false),
        "the health image draws under the number"
    );
    e.layer_selected(false).unwrap();
    assert!(rank(&e) < start, "Send back draws it under a sibling");
    assert_eq!(e.undo_name(), Some("change the layer of Health value"));
    let back = rank(&e);
    assert!(e.can_layer(true));
    e.layer_selected(true).unwrap();
    assert!(
        rank(&e) > back,
        "Bring forward draws it over that sibling again"
    );
    assert_eq!(e.selection(), [key(HV)]);
}
