//! The freeze cam's killer health file as children of FreezePanelHealth.

mod common;

use common::hud_stock::{stock_files, stock_files_with};
use tf2tools_core::{
    ChangeTarget, HiddenBy, HudModel, PartId, PartKey, Platform, Scene, SceneRule, Screen, Team,
    drawn_image,
};

const HEALTH: &str = "resource/ui/freezepanelkillerhealth.res";
const SD: Screen = Screen::new(640, 480);
const PANEL: &str = "FreezePanel/FreezePanelBase/FreezePanelHealth";

fn freeze() -> HudModel {
    HudModel::load_scene(&stock_files(), Platform::Linux, Scene::FreezeCam).unwrap()
}

fn k(text: &str) -> PartKey {
    PartKey::new(text)
}

#[test]
fn the_killer_health_file_fills_the_health_panel() {
    let rule = SceneRule::get(Scene::FreezeCam);
    assert_eq!(rule.nested(), [("FreezePanelHealth", HEALTH)]);
    let m = freeze();
    assert!(m.problems().is_empty());
    let l = m.layout(SD);
    let at = |name: &str| {
        let b = l
            .get(&k(&format!("{PANEL}/{name}")))
            .unwrap_or_else(|| panic!("{name}"));
        (b.x(), b.y(), b.wide(), b.tall())
    };
    // The panel sits at (378, 253); the file's boxes hang off it.
    assert_eq!(at("PlayerStatusHealthImage"), (385, 260, 18, 18));
    assert_eq!(at("PlayerStatusHealthImageBG"), (383, 258, 22, 22));
    assert_eq!(at("PlayerStatusHealthValue"), (384, 264, 20, 10));
    let value = m
        .part(&k(&format!("{PANEL}/PlayerStatusHealthValue")))
        .unwrap();
    assert!(value.is_readable());
    assert_eq!(value.owner(), None, "only a rule's root has an owner");
    // The code paints the health cross; the file names no image for it.
    let cross = m
        .part(&k(&format!("{PANEL}/PlayerStatusHealthImage")))
        .unwrap();
    assert_eq!(drawn_image(cross, Team::Red), Some("../hud/health_color"));
    // Nothing nested leaks into the other scenes.
    assert!(
        m.with_scene(Scene::Hud)
            .part(&k(&format!("{PANEL}/PlayerStatusHealthValue")))
            .is_none()
    );
}

#[test]
fn a_missing_killer_health_file_leaves_the_panel_childless_and_a_problem() {
    let files = stock_files_with(&[(HEALTH, None)]);
    let m = HudModel::load_scene(&files, Platform::Linux, Scene::FreezeCam).unwrap();
    assert_eq!(m.problems(), [HEALTH]);
    assert!(m.part(&k(PANEL)).is_some());
    assert!(
        m.part(&k(&format!("{PANEL}/PlayerStatusHealthValue")))
            .is_none()
    );
}

#[test]
fn the_overheal_and_building_images_are_hidden_by_the_game() {
    let m = freeze();
    let l = m.layout(SD);
    for name in [
        "PlayerStatusHealthBonusImage",
        "BuildingStatusHealthImageBG",
    ] {
        let part = m.part(&k(&format!("{PANEL}/{name}"))).unwrap();
        assert!(part.code_toggled().is_some(), "{name}");
        assert_eq!(
            l.get(part.key()).unwrap().hidden(),
            Some(HiddenBy::Game),
            "{name}"
        );
    }
}

#[test]
fn nested_parts_edit_their_own_file_and_hide_by_size_zero() {
    let m = freeze();
    let value = k(&format!("{PANEL}/PlayerStatusHealthValue"));
    let changes = m.move_part(&value, 10, 12, SD, 1).unwrap();
    assert!(!changes.is_empty());
    for c in &changes {
        let ChangeTarget::Set(s) = c.target() else {
            panic!("not a set")
        };
        assert_eq!(s.file(), HEALTH);
    }
    // Not code-toggled and a file control rather than a hudlayout element, so
    // `unclassified_hide(false)` applies: size zero.
    let hide = m
        .hide_part(&k(&format!("{PANEL}/PlayerStatusHealthImage")), 2)
        .unwrap();
    let keys: Vec<String> = hide
        .iter()
        .map(|c| match c.target() {
            ChangeTarget::Set(s) => format!("{}:{}", s.file(), s.key()),
            _ => String::new(),
        })
        .collect();
    assert_eq!(keys, [format!("{HEALTH}:wide"), format!("{HEALTH}:tall")]);
    // A rule root keeps its owner; only PartId::FreezeCam owns FreezePanel.
    assert_eq!(
        m.part(&k("FreezePanel")).unwrap().owner(),
        Some(PartId::FreezeCam)
    );
}
