mod common;

use common::hud_borrow::source;
use common::hud_stock::stock_files;
use tf2tools_core::{BorrowRefused, Brings, ChangeTarget, HudModel, PartKey, Platform, Screen};

const ROOT: &str = "HudPlayerStatus/HudPlayerHealth";
const VALUE: &str = "HudPlayerStatus/HudPlayerHealth/PlayerStatusHealthValue";
const IMAGE: &str = "HudPlayerStatus/HudPlayerHealth/PlayerStatusHealthImage";
const BADGE: &str = "HudPlayerStatus/HudPlayerHealth/MineBadge";
const HD: Screen = Screen::new(1920, 1080);
const HEALTH_RES: &str = "resource/ui/hudplayerhealth.res";
const MINE: &str = r#""Resource/UI/HudPlayerHealth.res"
{
  HudPlayerHealth { ControlName EditablePanel xpos 4 ypos r100 zpos 2 wide 200 tall 100 visible 1 }
  PlayerStatusHealthValue { ControlName CExLabel xpos 90 ypos 40 zpos 5 wide 60 tall 30 labelText %Health% }
  MineBadge { ControlName ImagePanel xpos 10 ypos 10 wide 20 tall 20 }
}
"#;

fn k(t: &str) -> PartKey {
    PartKey::new(t)
}
fn target() -> HudModel {
    HudModel::load(&stock_files(), Platform::Linux).unwrap()
}
fn text(m: &HudModel, part: &str, key: &str) -> Option<String> {
    m.part(&k(part))
        .and_then(|p| p.value(key))
        .map(|l| l.text().to_owned())
}

#[test]
fn a_root_replaces_resolved_keys_removes_keys_only_the_target_sets_and_credits_the_author() {
    let t = target();
    let b = t
        .borrow_part(&source(&[(HEALTH_RES, MINE)]), &stock_files(), &k(ROOT), 7)
        .unwrap();
    assert!(b.changes().iter().all(|c| c.group() == 7));
    assert!(
        b.changes()
            .iter()
            .any(|c| matches!(c.target(), ChangeTarget::Credit(n) if n.author() == "Mia"))
    );
    let e = t.with_changes(b.changes());
    assert_eq!(e.dropped(), 0);
    let m = e.model();
    assert_eq!(text(m, ROOT, "xpos").as_deref(), Some("4"));
    assert_eq!(text(m, ROOT, "wide").as_deref(), Some("200"));
    assert_eq!(
        text(m, ROOT, "xpos_minmode"),
        None,
        "the target set it, the source did not"
    );
}

#[test]
fn children_are_replaced_added_and_the_missing_ones_hidden() {
    let t = target();
    let b = t
        .borrow_part(&source(&[(HEALTH_RES, MINE)]), &stock_files(), &k(ROOT), 1)
        .unwrap();
    let e = t.with_changes(b.changes());
    let m = e.model();
    assert_eq!(text(m, VALUE, "xpos").as_deref(), Some("90"));
    assert_eq!(text(m, BADGE, "ControlName").as_deref(), Some("ImagePanel"));
    assert!(
        m.layout(HD).get(&k(IMAGE)).unwrap().hidden().is_some(),
        "the source has no health image"
    );
    assert!(b.brings().contains(&Brings::Child("MineBadge".into())));
}

#[test]
fn one_child_added_under_the_same_parent_leaves_its_siblings_alone() {
    let t = target();
    let b = t
        .borrow_part(&source(&[(HEALTH_RES, MINE)]), &stock_files(), &k(BADGE), 1)
        .unwrap();
    let m = t.with_changes(b.changes()).into_model();
    assert_eq!(text(&m, BADGE, "xpos").as_deref(), Some("10"));
    assert_eq!(
        text(&m, VALUE, "xpos").as_deref(),
        Some("76"),
        "the stock value label is unchanged"
    );
}

#[test]
fn a_pin_to_a_part_outside_the_borrow_becomes_its_resolved_position() {
    let pinned = MINE.replace(
        "MineBadge { ControlName ImagePanel xpos 10 ypos 10 wide 20 tall 20 }",
        "Anchor { ControlName ImagePanel xpos 30 ypos 20 wide 5 tall 5 }\n  MineBadge { ControlName ImagePanel xpos 10 ypos 10 wide 20 tall 20 pin_to_sibling Anchor pin_corner_to_sibling 0 pin_to_sibling_corner 0 }",
    );
    let src = source(&[(HEALTH_RES, &pinned)]);
    let t = target();
    let b = t.borrow_part(&src, &stock_files(), &k(BADGE), 1).unwrap();
    let m = t.with_changes(b.changes()).into_model();
    assert_eq!(
        text(&m, BADGE, "pin_to_sibling"),
        None,
        "the target has no Anchor sibling"
    );
    // Boxes are absolute and the two roots sit at different x, so compare offsets from the root.
    let offset = |m: &HudModel| {
        let layout = m.layout(HD);
        layout.get(&k(BADGE)).unwrap().x() - layout.get(&k(ROOT)).unwrap().x()
    };
    let (want, got) = (offset(src.model()), offset(&m));
    assert!(want.abs_diff(got) <= 3, "{want} vs {got}");
}

#[test]
fn refusals_name_the_reason() {
    let t = target();
    assert_eq!(
        t.borrow_part(&source(&[]), &stock_files(), &k("Nope"), 1)
            .unwrap_err(),
        BorrowRefused::UnknownPart
    );
    let broken = source(&[(HEALTH_RES, "")]);
    assert_eq!(
        t.borrow_part(&broken, &stock_files(), &k(ROOT), 1)
            .unwrap_err(),
        BorrowRefused::Unreadable
    );
}
