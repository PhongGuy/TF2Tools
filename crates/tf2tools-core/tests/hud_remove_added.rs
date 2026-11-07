//! Only parts the player added can be deleted, and deleting keeps the player's other edits.

use crate::common;

use common::hud_anims::{VALUE, anim_model};
use common::hud_stock::stock_files;
use tf2tools_core::{
    AnimationChange, Change, ChangeTarget, Fired, HudModel, NewPart, PartKey, PinCorner, Platform,
    RemoveRefused, Rgba, Screen,
};

fn model() -> HudModel {
    HudModel::load(&stock_files(), Platform::Linux).unwrap()
}

fn apply(base: &HudModel, changes: &[Change]) -> HudModel {
    base.with_changes(changes).into_model()
}

fn body(m: &HudModel, event: &str) -> String {
    m.animations().event(event).unwrap().body()
}

#[test]
fn deleting_a_shadow_drops_its_block_and_its_edits_only() {
    let base = model();
    let value = PartKey::new(VALUE);
    let (shadow, mut changes) = base.add_part(NewPart::Shadow, &value, 1).unwrap();
    changes.extend(apply(&base, &changes).set_layer(&shadow, 9, 2).unwrap());
    let recolour = apply(&base, &changes)
        .recolour_part(&value, "255 0 0 255", 3)
        .unwrap();
    changes.extend(recolour.clone());
    let m = apply(&base, &changes);
    assert!(m.is_added(&changes, &shadow));
    assert!(!m.is_added(&changes, &value));
    let left = m.remove_added(&changes, &shadow, 4).unwrap();
    assert_eq!(left, recolour);
    assert!(apply(&base, &left).part(&shadow).is_none());
}

#[test]
fn deleting_keeps_a_colour_edit_made_in_the_same_event() {
    let base = anim_model(&[]);
    let value = PartKey::new(VALUE);
    let (shadow, mut changes) = base.add_part(NewPart::Shadow, &value, 1).unwrap();
    let kept = "Animate \"PlayerStatusHealthValue\" \"FgColor\" \"255 255 0 255\" Linear 0 0.1\nAnimate \"PlayerStatusHealthValue\" \"FgColor\" \"TanDark\" Accel 0.1 0.1\nRunEvent \"HudHealthDyingPulseLoop\" 0.25";
    let edited =
        format!("{kept}\nAnimate \"PlayerStatusHealthValueShadow\" \"Alpha\" \"0\" Linear 0 0.1");
    changes.push(Change::animation(
        2,
        AnimationChange::new("HudHealthDyingPulse", &edited),
    ));
    let m = apply(&base, &changes);
    let next = m.remove_added(&changes, &shadow, 3).unwrap();
    assert!(
        next.contains(&changes[1]),
        "the player's whole-event edit is kept"
    );
    let after = apply(&base, &next);
    assert_eq!(body(&after, "HudHealthDyingPulse"), kept);
    assert!(after.part(&shadow).is_none());
    assert!(
        next.iter()
            .filter(|c| c.group() == 3)
            .all(|c| matches!(c.target(), ChangeTarget::Animation(_)))
    );
    let undone = apply(&base, &changes);
    assert_eq!(body(&undone, "HudHealthDyingPulse"), edited);
    assert!(undone.part(&shadow).is_some());
}

#[test]
fn deleting_a_crosshair_drops_its_hit_flash_lines() {
    let base = anim_model(&[]);
    let (cross, mut changes) = base
        .add_part(NewPart::Crosshair, &PartKey::new(VALUE), 1)
        .unwrap();
    changes.extend(
        apply(&base, &changes)
            .hit_flash(&cross, "255 0 0 255", 2)
            .unwrap(),
    );
    let next = apply(&base, &changes)
        .remove_added(&changes, &cross, 3)
        .unwrap();
    assert!(
        !next
            .iter()
            .any(|c| matches!(c.target(), ChangeTarget::AddBlock(_)))
    );
    let after = apply(&base, &next);
    assert!(
        after.animations().event("TF2ToolsHitMarker").is_none(),
        "its appended lines went with it"
    );
    assert_eq!(
        body(&after, "DamagedPlayer"),
        "",
        "its trigger lines went with it"
    );
}

#[test]
fn stock_parts_are_never_deleted() {
    let m = model();
    assert_eq!(
        m.remove_added(&[], &PartKey::new(VALUE), 1),
        Err(RemoveRefused::NotAdded)
    );
    assert_eq!(
        m.remove_added(&[], &PartKey::new("Nope"), 1),
        Err(RemoveRefused::UnknownPart)
    );
}

#[test]
fn an_anchor_something_is_attached_to_stays_until_detached() {
    let base = model();
    let value = PartKey::new(VALUE);
    let (anchor, mut changes) = base.add_part(NewPart::Anchor, &value, 1).unwrap();
    let pin = apply(&base, &changes)
        .pin_part(&value, &anchor, PinCorner::TopLeft, PinCorner::TopLeft, 2)
        .unwrap();
    changes.extend(pin);
    let m = apply(&base, &changes);
    assert_eq!(
        m.remove_added(&changes, &anchor, 3),
        Err(RemoveRefused::Attached)
    );
    changes.extend(m.unpin_part(&value, 3).unwrap());
    let m = apply(&base, &changes);
    assert!(m.remove_added(&changes, &anchor, 4).is_ok());
}

#[test]
fn deleting_a_crosshair_keeps_another_label_s_flash_firing() {
    let base = anim_model(&[]);
    let value = PartKey::new(VALUE);
    let (cross, mut changes) = base.add_part(NewPart::Crosshair, &value, 1).unwrap();
    changes.extend(
        apply(&base, &changes)
            .hit_flash(&cross, "255 0 0 255", 2)
            .unwrap(),
    );
    changes.extend(
        apply(&base, &changes)
            .hit_flash(&value, "0 0 255 255", 3)
            .unwrap(),
    );
    let next = apply(&base, &changes)
        .remove_added(&changes, &cross, 4)
        .unwrap();
    let after = apply(&base, &next);
    assert_eq!(
        body(&after, "DamagedPlayer"),
        "StopEvent \"TF2ToolsHitMarker\" 0\nRunEvent \"TF2ToolsHitMarker\" 0.00001"
    );
    let layout = after.layout(Screen::new(1920, 1080));
    let hit = [Fired::new("DamagedPlayer", None)];
    let frame = after.animations().frame(&after, &layout, &hit, 0.05);
    assert_eq!(
        frame.get(&value).unwrap().fg(),
        Some(Rgba::new(0, 0, 255, 255))
    );
}
