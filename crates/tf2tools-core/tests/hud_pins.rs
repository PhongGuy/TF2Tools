//! Attaching a part to a sibling: keys, place kept, cycles refused.

use crate::common;

use common::hud_anims::{HEALTH_KEY, HEALTH_RES, VALUE};
use common::hud_stock::{HEALTH, block, label, stock_files_with, target_id_files, with_blocks};
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

/// A 32x16-unit label (72x36 px at 1080p), even so centre pins land on whole pixels.
fn even_label(name: &str, xpos: &str, ypos: &str, extra: &[(&str, &str)]) -> String {
    let mut keys = vec![
        ("xpos", xpos),
        ("ypos", ypos),
        ("wide", "32"),
        ("tall", "16"),
    ];
    keys.extend_from_slice(extra);
    block(name, "CExLabel", &keys)
}

/// A pinned `own` to B's `sib` with `xpos`/`ypos` offsets, returning the gap in pixels from
/// B's pinned corner to A's.
fn pin_gap(own: PinCorner, sib: PinCorner, xpos: &str, ypos: &str) -> (i32, i32) {
    let pin = [
        ("pin_to_sibling", "B"),
        ("pin_corner_to_sibling", own.key_text()),
        ("pin_to_sibling_corner", sib.key_text()),
    ];
    let m = kids(&(even_label("A", xpos, ypos, &pin) + &even_label("B", "100", "100", &[])));
    let l = m.layout(HD);
    let corner = |name: &str, c: PinCorner| {
        let b = l.get(&key(name)).unwrap();
        let p = c.point(b.wide(), b.tall());
        (b.x() + p.x, b.y() + p.y)
    };
    let (a, b) = (corner("A", own), corner("B", sib));
    (a.0 - b.0, a.1 - b.1)
}

/// The engine's offset direction (VPanel::Solve): positive values move away from the
/// sibling's corner, so a left or top sibling corner turns that axis around.
fn engine_sign(sib: PinCorner) -> (i32, i32) {
    use PinCorner::*;
    let x = if matches!(sib, TopLeft | BottomLeft | CenterLeft) {
        -1
    } else {
        1
    };
    let y = if matches!(sib, TopLeft | TopRight | CenterTop) {
        -1
    } else {
        1
    };
    (x, y)
}

#[test]
fn bottom_right_pinned_to_top_left_moves_up_and_left_as_in_game() {
    // Checked in TF2: offsets 12/17 put the label up and left of its panel, out of it.
    let (dx, dy) = pin_gap(PinCorner::BottomRight, PinCorner::TopLeft, "12", "17");
    assert_eq!((dx, dy), (-27, -38));
}

#[test]
fn every_corner_pair_offsets_the_way_the_engine_does() {
    for own in PinCorner::ALL {
        for sib in PinCorner::ALL {
            let (sx, sy) = engine_sign(sib);
            assert_eq!(
                pin_gap(own, sib, "8", "4"),
                (sx * 18, sy * 9),
                "{own:?} to {sib:?}"
            );
        }
    }
}

#[test]
fn zero_offsets_put_the_corners_together() {
    for own in PinCorner::ALL {
        for sib in PinCorner::ALL {
            assert_eq!(pin_gap(own, sib, "0", "0"), (0, 0), "{own:?} to {sib:?}");
        }
    }
}

#[test]
fn pinning_keeps_the_place_for_every_corner_pair() {
    let m = kids(&(even_label("A", "10", "10", &[]) + &even_label("B", "100", "100", &[])));
    let before = place(&m, "A");
    for own in PinCorner::ALL {
        for sib in PinCorner::ALL {
            let changes = m.pin_part(&key("A"), &key("B"), own, sib, 4).unwrap();
            let now = place(&m.with_changes(&changes).into_model(), "A");
            assert!(
                (now.0 - before.0).abs() <= 2 && (now.1 - before.1).abs() <= 2,
                "{own:?} to {sib:?}: {before:?} -> {now:?}"
            );
        }
    }
}

#[test]
fn dragging_a_pinned_part_moves_it_with_the_pointer_for_every_corner_pair() {
    // A drag of 45/90 px writes offsets that the layout reads back as the same move.
    for own in PinCorner::ALL {
        for sib in PinCorner::ALL {
            let pin = [
                ("pin_to_sibling", "B"),
                ("pin_corner_to_sibling", own.key_text()),
                ("pin_to_sibling_corner", sib.key_text()),
            ];
            let m = kids(&(even_label("A", "8", "4", &pin) + &even_label("B", "100", "100", &[])));
            let before = place(&m, "A");
            let moved = m.move_part(&key("A"), 45, -90, HD, 5).unwrap();
            let now = place(&m.with_changes(&moved).into_model(), "A");
            assert_eq!(
                (now.0 - before.0, now.1 - before.1),
                (45, -90),
                "{own:?} to {sib:?}"
            );
            let nudged = m.nudge_part(&key("A"), 2, 0, 6).unwrap();
            let now = place(&m.with_changes(&nudged).into_model(), "A");
            assert!(
                now.0 > before.0,
                "{own:?} to {sib:?}: a right nudge moves right"
            );
        }
    }
}
