//! The Spectator scene's second root: the watched player's target ID.

use crate::common;

use common::hud_stock::{stock_files_with, target_id_files};
use tf2tools_core::{
    ChangeTarget, EditRefused, HideTechnique, HudFiles, HudModel, PanelRule, PartId, PartKey,
    Platform, Scene, SceneRule, Screen,
};

const ROOT: &str = "CSpectatorTargetID";
const HEALTH: &str = "CSpectatorTargetID/SpectatorGUIHealth";
const SD: Screen = Screen::new(640, 480);

fn bar(files: &HudFiles) -> HudModel {
    HudModel::load_scene(files, Platform::Linux, Scene::Spectator).unwrap()
}

fn k(text: &str) -> PartKey {
    PartKey::new(text)
}

fn at(m: &HudModel, key: &str) -> (i32, i32, i32, i32) {
    let l = m.layout(SD);
    let b = l.get(&k(key)).unwrap_or_else(|| panic!("{key}"));
    (b.x(), b.y(), b.wide(), b.tall())
}

#[test]
fn the_scene_has_a_second_root_for_the_target_id() {
    let rule = SceneRule::get(Scene::Spectator);
    let ids: Vec<PartId> = rule.rules().iter().map(PanelRule::id).collect();
    assert_eq!(ids, [PartId::Spectator, PartId::SpectatorTargetId]);
    assert_eq!(rule.extras().len(), 3);
    assert_eq!(rule.problem_files(), ["resource/ui/spectator.res"]);
    let id = PanelRule::get(PartId::SpectatorTargetId);
    assert_eq!(
        (id.panel(), id.children_file()),
        (ROOT, "resource/ui/targetid.res")
    );
    assert_eq!(id.geometry(), [("scripts/hudlayout.res", ROOT)]);
    for key in ["xpos", "wide", "visible"] {
        assert!(id.ignores(key), "{key}");
    }
    assert!(!id.ignores("ypos") && !id.ignores("tall"));
    assert!(id.centres_x() && id.clickable() && id.has_children() && !id.fills_screen());
    assert_eq!(id.hide(), HideTechnique::OffScreenY);
    // The HUD's target ID is a different rule and a different panel.
    assert_eq!(PanelRule::get(PartId::TargetId).panel(), "CMainTargetID");
}

#[test]
fn the_second_root_builds_the_target_id_tree_with_the_health_panel_nested() {
    let m = bar(&target_id_files());
    assert!(m.problems().is_empty(), "{:?}", m.problems());
    let root = m.part(&k(ROOT)).unwrap();
    assert!(root.is_root() && root.is_readable());
    assert_eq!(root.owner(), Some(PartId::SpectatorTargetId));
    let keys: Vec<&str> = m.parts().iter().map(|p| p.key().as_str()).collect();
    for key in [
        "Spectator/TipLabel",
        "CSpectatorTargetID/TargetNameLabel",
        "CSpectatorTargetID/TargetDataLabel",
        "CSpectatorTargetID/TargetIDBG_Spec_Red",
        HEALTH,
        "CSpectatorTargetID/SpectatorGUIHealth/PlayerStatusHealthValue",
    ] {
        assert!(keys.contains(&key), "{key} in {keys:?}");
    }
    assert!(!keys.contains(&"CMainTargetID"));
    // The game centres the panel on x and keeps the file's y; the health panel sits inside it.
    assert_eq!(at(&m, ROOT), (194, 250, 252, 35));
    assert_eq!(at(&m, HEALTH), (197, 252, 32, 32));
    assert_eq!(
        at(
            &m,
            "CSpectatorTargetID/SpectatorGUIHealth/PlayerStatusHealthValue"
        ),
        (203, 263, 20, 10)
    );
}

#[test]
fn a_hud_without_the_target_id_files_still_draws_the_bars() {
    // tf2-recording-hud ships neither file; the bars must not read "Can't show".
    let files = stock_files_with(&[
        ("resource/ui/targetid.res", None),
        ("resource/ui/spectatorguihealth.res", None),
    ]);
    let m = bar(&files);
    assert!(m.problems().is_empty(), "{:?}", m.problems());
    assert!(m.part(&k("Spectator/TipLabel")).unwrap().is_readable());
    assert!(!m.part(&k(ROOT)).unwrap().is_readable());
}

const NAME: &str = "CSpectatorTargetID/TargetNameLabel";
const DATA: &str = "CSpectatorTargetID/TargetDataLabel";

fn shown(m: &HudModel, key: &str) -> bool {
    m.layout(SD).get(&k(key)).unwrap().hidden().is_none()
}

#[test]
fn the_labels_sit_where_the_code_puts_them() {
    let m = bar(&target_id_files());
    // x is XRES(8) + the health panel's wide (32); y is the file's.
    assert_eq!(at(&m, NAME), (234, 255, 280, 24));
    assert_eq!(at(&m, DATA), (234, 267, 280, 15));
    // XRES follows the screen width: 8 * 1280 / 640 = 16, and the health panel is 48 wide.
    let l = m.layout(Screen::new(1280, 720));
    let (r, n) = (l.get(&k(ROOT)).unwrap(), l.get(&k(NAME)).unwrap());
    assert_eq!((r.x(), r.y()), (451, 375));
    assert_eq!((n.x(), n.y()), (451 + 16 + 48, 375 + 7));
}

#[test]
fn a_wider_health_panel_pushes_the_labels_right() {
    let text = common::hud_stock::TARGET_ID.replace(
        "SpectatorGUIHealth { ControlName EditablePanel xpos 3 ypos 2 wide 32",
        "SpectatorGUIHealth { ControlName EditablePanel xpos 3 ypos 2 wide 60",
    );
    assert_ne!(text, common::hud_stock::TARGET_ID);
    let m = bar(&stock_files_with_target_id(&text));
    assert_eq!(at(&m, NAME).0, 194 + 8 + 60);
    // The file's own x for the label is ignored.
    let moved = common::hud_stock::TARGET_ID.replace("xpos 8 ypos 5", "xpos 90 ypos 5");
    assert_eq!(at(&bar(&stock_files_with_target_id(&moved)), NAME).0, 234);
}

fn stock_files_with_target_id(text: &str) -> HudFiles {
    let mut over = common::hud_stock::target_id_texts();
    over.retain(|(p, _)| *p != "resource/ui/targetid.res");
    let mut refs: Vec<(&str, Option<&str>)> =
        over.iter().map(|(p, t)| (*p, Some(t.as_str()))).collect();
    refs.push(("resource/ui/targetid.res", Some(text)));
    stock_files_with(&refs)
}

#[test]
fn the_code_hides_what_it_hides_and_shows_what_the_file_hides() {
    let m = bar(&target_id_files());
    for key in [
        ROOT,
        HEALTH,
        NAME,
        DATA,
        "CSpectatorTargetID/TargetIDBG_Spec_Red",
        "CSpectatorTargetID/SpectatorGUIHealth/PlayerStatusHealthValue",
    ] {
        assert!(shown(&m, key), "{key} should show");
    }
    for name in [
        "TargetIDBG",
        "TargetIDBG_Spec_Blue",
        "AvatarImage",
        "AmmoIcon",
        "KillStreakIcon",
        "MoveableSubPanel",
    ] {
        assert!(!shown(&m, &format!("{ROOT}/{name}")), "{name} should hide");
    }
}

#[test]
fn the_labels_keep_font_and_y_but_not_x_or_colour() {
    let m = bar(&target_id_files());
    for key in [NAME, DATA] {
        let part = m.part(&k(key)).unwrap();
        for owned in ["xpos", "fgcolor", "fgcolor_override"] {
            assert!(part.code_toggled().unwrap().owns(owned), "{key} {owned}");
        }
        assert!(
            !part.code_toggled().unwrap().owns("ypos")
                && !part.code_toggled().unwrap().owns("font")
        );
        let caps = m.capabilities(&k(key)).unwrap();
        assert!(!caps.move_x() && caps.move_y() && !caps.recolour() && !caps.mirror());
        assert!(m.set_property(&k(key), "font", "HudFontMedium", 1).is_ok());
        // A nudge moves y and never writes an x.
        let changes = m.nudge_part(&k(key), 5, 3, 1).unwrap();
        assert!(!changes.is_empty());
        for c in &changes {
            let ChangeTarget::Set(s) = c.target() else {
                panic!("not a set")
            };
            assert_eq!(s.key(), "ypos");
        }
    }
    // A label the code does not colour still takes a recolour.
    assert!(m.capabilities(&k("Spectator/TipLabel")).unwrap().recolour());
    // The root's x is the game's; its y is the file's.
    let root = m.capabilities(&k(ROOT)).unwrap();
    assert!(!root.move_x() && root.move_y());
}

#[test]
fn recolouring_an_owned_label_is_refused_on_both_tabs() {
    let m = bar(&target_id_files());
    for key in [NAME, DATA] {
        let refused = m.recolour_part(&k(key), "255 0 0 255", 1);
        assert!(matches!(refused, Err(EditRefused::NotOffered)), "{key}");
    }
    // The game overwrites the HUD tab's labels the same way, so they refuse a recolour too.
    let hud = HudModel::load_scene(&target_id_files(), Platform::Linux, Scene::Hud).unwrap();
    for name in ["TargetNameLabel", "TargetDataLabel"] {
        let label = k(&format!("CMainTargetID/{name}"));
        let part = hud.part(&label).unwrap();
        for owned in ["fgcolor", "fgcolor_override"] {
            assert!(part.code_toggled().unwrap().owns(owned), "{name} {owned}");
        }
        assert!(!hud.capabilities(&label).unwrap().recolour(), "{name}");
        let refused = hud.recolour_part(&label, "255 0 0 255", 1);
        assert!(matches!(refused, Err(EditRefused::NotOffered)), "{name}");
    }
    // A label the code does not colour still takes a recolour.
    let tip = k("Spectator/TipLabel");
    assert!(m.capabilities(&tip).unwrap().recolour());
}
