//! Attaching a part to a sibling: keys, place kept, cycles refused.

use crate::common;

use common::hud_anims::{HEALTH_KEY, HEALTH_RES, VALUE};
use common::hud_stock::{HEALTH, label, stock_files_with, target_id_files, with_blocks};
use tf2tools_core::{HudModel, PartKey, PinCorner, PinRefused, Platform, Scene, Screen};

const HD: Screen = Screen::new(1920, 1080);

fn kids(blocks: &str) -> HudModel {
    let health = with_blocks(HEALTH, blocks);
    HudModel::load(
        &stock_files_with(&[(HEALTH_RES, Some(&health))]),
        Platform::Linux,
    )
    .unwrap()
}

fn key(name: &str) -> PartKey {
    PartKey::new(format!("{HEALTH_KEY}/{name}"))
}

fn place(m: &HudModel, name: &str) -> (i32, i32) {
    let b = m.layout(HD);
    let b = b.get(&key(name)).unwrap();
    (b.x(), b.y())
}

fn text(m: &HudModel, name: &str, k: &str) -> Option<String> {
    m.part(&key(name))?.value(k).map(|l| l.text().to_owned())
}

#[test]
fn corner_names_round_trip() {
    for corner in PinCorner::ALL {
        assert_eq!(PinCorner::parse(Some(corner.key_text())), corner);
    }
    assert_eq!(PinCorner::CenterLeft.key_text(), "PIN_CENTER_LEFT");
}

#[test]
fn a_pin_writes_its_keys_and_the_part_stays_put() {
    let m = kids(&(label("A", "10", "10", &[]) + &label("B", "50", "40", &[])));
    let before = place(&m, "A");
    let changes = m
        .pin_part(
            &key("A"),
            &key("B"),
            PinCorner::TopLeft,
            PinCorner::TopRight,
            4,
        )
        .unwrap();
    assert!(changes.iter().all(|c| c.group() == 4));
    let after = m.with_changes(&changes).into_model();
    assert_eq!(text(&after, "A", "pin_to_sibling").as_deref(), Some("B"));
    assert_eq!(
        text(&after, "A", "pin_corner_to_sibling").as_deref(),
        Some("PIN_TOPLEFT")
    );
    assert_eq!(
        text(&after, "A", "pin_to_sibling_corner").as_deref(),
        Some("PIN_TOPRIGHT")
    );
    assert!(after.layout(HD).get(&key("A")).unwrap().pinned());
    let now = place(&after, "A");
    assert!(
        (now.0 - before.0).abs() <= 2 && (now.1 - before.1).abs() <= 2,
        "{before:?} -> {now:?}"
    );
}

#[test]
fn a_pin_that_closes_a_loop_is_refused() {
    let m = kids(
        &(label("A", "0", "0", &[])
            + &label("B", "0", "0", &[("pin_to_sibling", "C")])
            + &label("C", "0", "0", &[("pin_to_sibling", "a")])),
    );
    let corner = PinCorner::TopLeft;
    assert_eq!(
        m.pin_part(&key("A"), &key("B"), corner, corner, 1),
        Err(PinRefused::Cycle)
    );
    assert_eq!(
        m.pin_part(&key("A"), &key("A"), corner, corner, 1),
        Err(PinRefused::Cycle)
    );
    let targets = m.pin_targets(&key("A"));
    assert!(
        !targets.contains(&key("A"))
            && !targets.contains(&key("B"))
            && !targets.contains(&key("C"))
    );
    assert!(targets.contains(&PartKey::new(VALUE)));
    for t in &targets {
        assert!(
            m.pin_part(&key("A"), t, corner, corner, 1).is_ok(),
            "{}",
            t.as_str()
        );
    }
}

#[test]
fn only_siblings_and_known_parts_pin() {
    let m = kids(&label("A", "0", "0", &[]));
    let corner = PinCorner::TopLeft;
    assert_eq!(
        m.pin_part(
            &key("A"),
            &PartKey::new("HudWeaponAmmo/AmmoInClip"),
            corner,
            corner,
            1
        ),
        Err(PinRefused::NotSibling)
    );
    assert_eq!(
        m.pin_part(&key("Nope"), &key("A"), corner, corner, 1),
        Err(PinRefused::UnknownPart)
    );
}

#[test]
fn unreadable_and_code_owned_places_are_refused() {
    let m = kids(&(label("Bad", "c1e30", "0", &[]) + &label("B", "0", "0", &[])));
    let corner = PinCorner::TopLeft;
    assert_eq!(
        m.pin_part(&key("Bad"), &key("B"), corner, corner, 1),
        Err(PinRefused::ReadOnly)
    );
    let spec = HudModel::load_scene(&target_id_files(), Platform::Linux, Scene::Spectator).unwrap();
    assert_eq!(
        spec.pin_part(
            &PartKey::new("CSpectatorTargetID/TargetNameLabel"),
            &PartKey::new("CSpectatorTargetID/AmmoIcon"),
            corner,
            corner,
            1
        ),
        Err(PinRefused::NotOffered)
    );
}

#[test]
fn detaching_keeps_the_place_and_drops_the_keys() {
    let pinned = [
        ("pin_to_sibling", "B"),
        ("pin_corner_to_sibling", "PIN_TOPLEFT"),
        ("pin_to_sibling_corner", "PIN_TOPRIGHT"),
    ];
    let m = kids(&(label("A", "5", "0", &pinned) + &label("B", "50", "40", &[])));
    let before = place(&m, "A");
    let changes = m.unpin_part(&key("A"), 2).unwrap();
    let after = m.with_changes(&changes).into_model();
    for k in [
        "pin_to_sibling",
        "pin_corner_to_sibling",
        "pin_to_sibling_corner",
    ] {
        assert_eq!(text(&after, "A", k), None, "{k}");
    }
    let now = place(&after, "A");
    assert!(
        (now.0 - before.0).abs() <= 2 && (now.1 - before.1).abs() <= 2,
        "{before:?} -> {now:?}"
    );
    assert!(m.unpin_part(&key("B"), 2).unwrap().is_empty());
}
