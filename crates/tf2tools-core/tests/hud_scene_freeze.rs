//! The death and freeze cam scene: its files, two roots and the stock-shaped layout.

use crate::common;

use common::hud_stock::{stock_files, stock_files_with};
use tf2tools_core::{
    HideTechnique, HudModel, PanelRule, PartId, PartKey, Platform, Scene, SceneRule, Screen,
};

const BASIC: &str = "resource/ui/freezepanel_basic.res";
const HEALTH: &str = "resource/ui/freezepanelkillerhealth.res";
const SD: Screen = Screen::new(640, 480);
const BASE: &str = "FreezePanel/FreezePanelBase";

fn freeze() -> HudModel {
    HudModel::load_scene(&stock_files(), Platform::Linux, Scene::FreezeCam).unwrap()
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
fn the_scene_reads_two_files_and_one_root_and_hides_the_hud() {
    let rule = SceneRule::get(Scene::FreezeCam);
    assert_eq!(rule.label(), "Death and freeze cam");
    assert_eq!(rule.files(), ["scripts/hudlayout.res", BASIC, HEALTH]);
    assert!(rule.read_only_files().is_empty());
    assert_eq!(rule.root(), Some("FreezePanel"));
    assert!(rule.hides_hud());
    let [panel] = rule.rules() else {
        panic!("one root")
    };
    assert_eq!(panel.id(), PartId::FreezeCam);
    assert_eq!(
        (panel.panel(), panel.children_file(), panel.geometry()),
        (
            "FreezePanel",
            BASIC,
            [("scripts/hudlayout.res", "FreezePanel")].as_slice()
        )
    );
    assert!(!panel.clickable() && panel.has_children());
    assert_eq!(panel.hide(), HideTechnique::NotOffered);
    assert_eq!(PanelRule::get(PartId::FreezeCam).panel(), "FreezePanel");
    for file in [BASIC, HEALTH] {
        assert_eq!(Scene::for_file(file), Some(Scene::FreezeCam), "{file}");
    }
    // The layout file stays the HUD's.
    assert_eq!(Scene::for_file("scripts/hudlayout.res"), Some(Scene::Hud));
}

#[test]
fn the_win32_block_builds_the_panel_tree_and_the_x360_block_is_ignored() {
    let m = freeze();
    assert_eq!(m.scene(), Scene::FreezeCam);
    assert!(m.problems().is_empty());
    let root = m.part(&k("FreezePanel")).unwrap();
    assert!(root.is_root() && root.is_readable());
    assert_eq!(root.owner(), Some(PartId::FreezeCam));
    assert_eq!(root.class(), Some("CTFFreezePanel"));
    let keys: Vec<&str> = m.parts().iter().map(|p| p.key().as_str()).collect();
    for key in [
        "FreezePanel/FreezePanelBase",
        "FreezePanel/FreezePanelBase/FreezePanelBG",
        "FreezePanel/FreezePanelBase/FreezeLabel",
        "FreezePanel/FreezePanelBase/FreezeLabelKiller",
        "FreezePanel/FreezePanelBase/AvatarImage",
        "FreezePanel/FreezePanelBase/NemesisSubPanel/NemesisLabel",
        "FreezePanel/itempanel",
        "FreezePanel/ScreenshotPanel/ScreenshotLabel",
    ] {
        assert!(keys.contains(&key), "{key} in {keys:?}");
    }
    // One FreezePanelBase part, with the WIN32 block's box.
    assert_eq!(
        keys.iter()
            .filter(|k| k.ends_with("/FreezePanelBase"))
            .count(),
        1
    );
    assert_eq!(at(&m, "FreezePanel"), (0, 0, 640, 480));
    assert_eq!(at(&m, BASE), (370, 93, 267, 200));
    assert_eq!(
        at(&m, "FreezePanel/FreezePanelBase/FreezePanelBG"),
        (378, 253, 256, 33)
    );
    assert_eq!(
        at(&m, "FreezePanel/FreezePanelBase/FreezeLabel"),
        (410, 255, 252, 12)
    );
    assert_eq!(
        at(&m, "FreezePanel/FreezePanelBase/FreezeLabelKiller"),
        (431, 266, 252, 18)
    );
    assert_eq!(
        at(&m, "FreezePanel/FreezePanelBase/AvatarImage"),
        (419, 267, 14, 14)
    );
    assert_eq!(
        at(&m, "FreezePanel/FreezePanelBase/FreezePanelHealth"),
        (378, 253, 31, 31)
    );
    assert_eq!(at(&m, "FreezePanel/ScreenshotPanel"), (237, 50, 166, 40));
    assert!(m.hidden_by_hud().is_none());
}

#[test]
fn a_missing_stub_or_unparsable_file_is_a_freeze_cam_problem_only() {
    // A `#base`-only file has no block: ten corpus HUDs ship one, and the scene says so.
    let stub = "#base \"freezepanel_basic_base.res\"";
    for (text, why) in [
        (None, "missing"),
        (Some(stub), "stub"),
        (Some("}"), "unparsable"),
    ] {
        let files = stock_files_with(&[(BASIC, text)]);
        let m = HudModel::load_scene(&files, Platform::Linux, Scene::FreezeCam).unwrap();
        assert_eq!(m.problems(), [BASIC], "{why}");
        assert!(!m.part(&k("FreezePanel")).unwrap().is_readable(), "{why}");
        for scene in [Scene::Hud, Scene::Scoreboard, Scene::MainMenu] {
            assert!(m.with_scene(scene).problems().is_empty(), "{why} {scene:?}");
        }
    }
}

#[test]
fn a_hud_that_zeroes_the_layout_box_hides_the_scene() {
    let layout = common::hud_stock::HUDLAYOUT.replace(
        "FreezePanel { visible 1 xpos 0 ypos 0 wide f0 tall 480 }",
        "FreezePanel { visible 1 xpos 0 ypos 0 wide 0 tall 0 }",
    );
    let files = stock_files_with(&[("scripts/hudlayout.res", Some(&layout))]);
    let m = HudModel::load_scene(&files, Platform::Linux, Scene::FreezeCam).unwrap();
    assert!(m.hidden_by_hud().is_some());
}
