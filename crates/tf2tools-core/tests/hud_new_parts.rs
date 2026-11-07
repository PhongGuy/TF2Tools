//! Adding a shadow, a background box or an anchor beside a part.

use crate::common;

use common::hud_anims::{HEALTH_KEY, HEALTH_RES, VALUE};
use common::hud_stock::{HEALTH, SCHEME, label, stock_files, stock_files_with, with_blocks};
use tf2tools_core::{ChangeTarget, EditRefused, HudModel, NewPart, PartKey, Platform, Screen};

const HD: Screen = Screen::new(1920, 1080);

fn model() -> HudModel {
    HudModel::load(&stock_files(), Platform::Linux).unwrap()
}

fn rect(m: &HudModel, key: &PartKey) -> (i32, i32, i32, i32) {
    let l = m.layout(HD);
    let b = l.get(key).unwrap();
    (b.x(), b.y(), b.wide(), b.tall())
}

fn added(m: &HudModel, kind: NewPart, near: &str) -> (PartKey, HudModel) {
    let (key, changes) = m.add_part(kind, &PartKey::new(near), 1).unwrap();
    assert_eq!(changes.len(), 1);
    assert!(matches!(changes[0].target(), ChangeTarget::AddBlock(_)));
    (key, m.with_changes(&changes).into_model())
}

fn text(m: &HudModel, key: &PartKey, k: &str) -> Option<String> {
    m.part(key)?.value(k).map(|l| l.text().to_owned())
}

#[test]
fn a_shadow_copies_the_label_and_sits_under_it() {
    let m = model();
    let (key, after) = added(&m, NewPart::Shadow, VALUE);
    assert_eq!(key.as_str(), format!("{VALUE}Shadow"));
    let ChangeTarget::AddBlock(add) = m
        .add_part(NewPart::Shadow, &PartKey::new(VALUE), 1)
        .unwrap()
        .1[0]
        .target()
        .clone()
    else {
        unreachable!()
    };
    assert_eq!(
        (add.file(), add.parent()),
        (
            HEALTH_RES,
            &["Resource/UI/HudPlayerHealth.res".to_owned()][..]
        )
    );
    assert_eq!(text(&after, &key, "labelText").as_deref(), Some("%Health%"));
    assert_eq!(
        text(&after, &key, "font").as_deref(),
        Some("HudClassHealth")
    );
    assert_eq!(
        text(&after, &key, "pin_to_sibling").as_deref(),
        Some("PlayerStatusHealthValue")
    );
    assert_eq!(text(&after, &key, "zpos").as_deref(), Some("4"));
    assert_eq!(text(&after, &key, "fgcolor").as_deref(), Some("0 0 0 255"));
    let (vx, vy, vw, vt) = rect(&after, &PartKey::new(VALUE));
    assert_eq!(rect(&after, &key), (vx + 2, vy + 2, vw, vt));
}

#[test]
fn a_shadow_uses_the_scheme_s_shadow_colour_when_it_has_one() {
    let scheme = SCHEME.replacen(
        "    TanDark \"117 107 94 255\"\n",
        "    TanDark \"117 107 94 255\"\n    TanDarkShadow \"70 64 56 255\"\n",
        1,
    );
    assert_ne!(scheme, SCHEME);
    let m = HudModel::load(
        &stock_files_with(&[("resource/clientscheme.res", Some(&scheme))]),
        Platform::Linux,
    )
    .unwrap();
    let (key, after) = added(&m, NewPart::Shadow, VALUE);
    assert_eq!(
        text(&after, &key, "fgcolor").as_deref(),
        Some("TanDarkShadow")
    );
}

#[test]
fn names_never_clash_and_only_labels_get_shadows() {
    let m = model();
    let (_, once) = added(&m, NewPart::Shadow, VALUE);
    let (key, _) = once
        .add_part(NewPart::Shadow, &PartKey::new(VALUE), 2)
        .unwrap();
    assert_eq!(key.as_str(), format!("{VALUE}Shadow_2"));
    let image = format!("{HEALTH_KEY}/PlayerStatusHealthImage");
    assert_eq!(
        m.add_part(NewPart::Shadow, &PartKey::new(image), 1).err(),
        Some(EditRefused::NotOffered)
    );
    assert_eq!(
        m.add_part(NewPart::Box, &PartKey::new("Nope"), 1).err(),
        Some(EditRefused::UnknownPart)
    );
}

#[test]
fn a_shadow_of_a_pinned_label_pins_to_the_label_instead() {
    let pinned = [
        ("pin_to_sibling", "PlayerStatusHealthImage"),
        ("pin_to_sibling_corner", "PIN_BOTTOMLEFT"),
    ];
    let health = with_blocks(HEALTH, &label("Tag", "4", "4", &pinned));
    let m = HudModel::load(
        &stock_files_with(&[(HEALTH_RES, Some(&health))]),
        Platform::Linux,
    )
    .unwrap();
    let (key, after) = added(&m, NewPart::Shadow, &format!("{HEALTH_KEY}/Tag"));
    assert_eq!(text(&after, &key, "pin_to_sibling").as_deref(), Some("Tag"));
    assert_eq!(
        text(&after, &key, "pin_to_sibling_corner").as_deref(),
        Some("PIN_TOPLEFT")
    );
    assert_eq!(
        text(&after, &key, "pin_corner_to_sibling").as_deref(),
        Some("PIN_TOPLEFT")
    );
    assert_eq!(
        (
            text(&after, &key, "xpos").as_deref(),
            text(&after, &key, "ypos").as_deref()
        ),
        (Some("1"), Some("1"))
    );
}

#[test]
fn an_unreadable_part_takes_no_new_parts() {
    let m = HudModel::load(
        &stock_files_with(&[(HEALTH_RES, Some("not keyvalues {"))]),
        Platform::Linux,
    )
    .unwrap();
    assert_eq!(
        m.add_part(NewPart::Box, &PartKey::new(HEALTH_KEY), 1).err(),
        Some(EditRefused::ReadOnly)
    );
}

#[test]
fn a_box_sits_behind_the_part_a_little_larger() {
    let m = model();
    let (key, after) = added(&m, NewPart::Box, VALUE);
    assert_eq!(key.as_str(), format!("{VALUE}Box"));
    assert_eq!(
        text(&after, &key, "bgcolor_override").as_deref(),
        Some("0 0 0 120")
    );
    assert_eq!(text(&after, &key, "zpos").as_deref(), Some("4"));
    let (vx, vy, vw, vt) = rect(&after, &PartKey::new(VALUE));
    let (bx, by, bw, bt) = rect(&after, &key);
    assert!(
        (bx - (vx - 4)).abs() <= 1 && (by - (vy - 4)).abs() <= 1,
        "{bx},{by}"
    );
    assert!(
        (bw - (vw + 9)).abs() <= 2 && (bt - (vt + 9)).abs() <= 2,
        "{bw}x{bt}"
    );
}

#[test]
fn a_box_behind_a_whole_element_fills_it_from_inside() {
    let m = model();
    let (key, after) = added(&m, NewPart::Box, "HudWeaponAmmo");
    assert_eq!(key.as_str(), "HudWeaponAmmo/HudWeaponAmmoBox");
    assert_eq!(
        rect(&after, &key),
        rect(&after, &PartKey::new("HudWeaponAmmo"))
    );
}

#[test]
fn an_anchor_sits_at_its_parent_s_centre() {
    let m = model();
    let (key, after) = added(&m, NewPart::Anchor, VALUE);
    assert_eq!(key.as_str(), format!("{HEALTH_KEY}/TF2ToolsAnchor"));
    let (px, py, pw, pt) = rect(&after, &PartKey::new(HEALTH_KEY));
    let (ax, ay, aw, at) = rect(&after, &key);
    assert!(
        ((ax + aw / 2) - (px + pw / 2)).abs() <= 1 && ((ay + at / 2) - (py + pt / 2)).abs() <= 1
    );
    let (second, _) = after
        .add_part(NewPart::Anchor, &PartKey::new(VALUE), 2)
        .unwrap();
    assert_eq!(second.as_str(), format!("{HEALTH_KEY}/TF2ToolsAnchor_2"));
}
