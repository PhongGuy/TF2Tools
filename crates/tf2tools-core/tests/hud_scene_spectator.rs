//! The spectator scene: its file, its root and the stock-shaped layout.

use crate::common;

use common::hud_stock::{stock_files, stock_files_with};
use tf2tools_core::{
    HideTechnique, HudFiles, HudModel, PanelRule, PartId, PartKey, Platform, Scene, SceneRule,
    Screen,
};

const FILE: &str = "resource/ui/spectator.res";
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
fn the_scene_reads_the_bar_and_the_target_id_files_and_hides_the_hud() {
    let rule = SceneRule::get(Scene::Spectator);
    assert_eq!(rule.label(), "Spectator");
    assert_eq!(
        rule.files(),
        [
            FILE,
            "scripts/hudlayout.res",
            "resource/ui/targetid.res",
            "resource/ui/spectatorguihealth.res"
        ]
    );
    assert!(rule.read_only_files().is_empty());
    assert_eq!(
        rule.nested(),
        [("SpectatorGUIHealth", "resource/ui/spectatorguihealth.res")]
    );
    assert_eq!(rule.root(), Some("Spectator"));
    assert!(rule.hides_hud() && !rule.toggles_any_depth());
    let [panel, _id] = rule.rules() else {
        panic!("two roots")
    };
    assert_eq!(panel.id(), PartId::Spectator);
    assert_eq!((panel.panel(), panel.children_file()), ("Spectator", FILE));
    assert!(panel.geometry().is_empty() && !panel.clickable() && panel.has_children());
    for key in ["xpos", "ypos", "wide", "tall", "visible"] {
        assert!(panel.ignores(key), "{key}");
    }
    assert!(panel.fills_screen());
    assert_eq!(panel.hide(), HideTechnique::NotOffered);
    assert_eq!(PanelRule::get(PartId::Spectator).panel(), "Spectator");
    assert_eq!(Scene::for_file(FILE), Some(Scene::Spectator));
    // Only this root is sized by the game; no HUD rule and no other scene's root is.
    assert!(PanelRule::all().iter().all(|r| !r.fills_screen()));
    assert!(!PanelRule::get(PartId::ClassSelect).fills_screen());
}

#[test]
fn the_stock_file_builds_the_bar_tree_at_the_boxes_the_game_gives_it() {
    let m = bar(&stock_files());
    assert_eq!(m.scene(), Scene::Spectator);
    assert!(m.problems().is_empty() && m.stub_files().is_empty());
    let root = m.part(&k("Spectator")).unwrap();
    assert!(root.is_root() && root.is_readable());
    assert_eq!(root.owner(), Some(PartId::Spectator));
    let keys: Vec<&str> = m.parts().iter().map(|p| p.key().as_str()).collect();
    for key in [
        "Spectator/topbar",
        "Spectator/bottombarblank",
        "Spectator/TipLabel",
        "Spectator/itempanel/ItemLabel",
    ] {
        assert!(keys.contains(&key), "{key} in {keys:?}");
    }
    // The file's root has no wide and tall 480; the game makes it the whole screen.
    assert_eq!(at(&m, "Spectator"), (0, 0, 640, 480));
    let wide = m.layout(Screen::new(1280, 720));
    let b = wide.get(&k("Spectator")).unwrap();
    assert_eq!((b.x(), b.y(), b.wide(), b.tall()), (0, 0, 1280, 720));
    assert_eq!(at(&m, "Spectator/topbar"), (0, 0, 640, 84));
    assert_eq!(at(&m, "Spectator/bottombarblank"), (0, 410, 640, 70));
    assert_eq!(at(&m, "Spectator/ReinforcementsLabel"), (20, 50, 600, 18));
    assert_eq!(at(&m, "Spectator/MapLabel"), (380, 16, 240, 20));
    assert_eq!(at(&m, "Spectator/ClassOrTeamLabel"), (235, 68, 170, 15));
    assert_eq!(at(&m, "Spectator/CycleTargetRevKeyLabel"), (5, 30, 60, 20));
    assert_eq!(at(&m, "Spectator/TipLabel"), (15, 413, 145, 64));
    assert!(m.hidden_by_hud().is_none());
}

#[test]
fn a_missing_or_unparsable_file_is_a_spectator_problem_only() {
    for (text, why) in [(None, "missing"), (Some("}"), "unparsable")] {
        let m = bar(&stock_files_with(&[(FILE, text)]));
        assert_eq!(m.problems(), [FILE], "{why}");
        assert!(!m.part(&k("Spectator")).unwrap().is_readable(), "{why}");
        for scene in [
            Scene::Hud,
            Scene::Scoreboard,
            Scene::MainMenu,
            Scene::FreezeCam,
            Scene::WinPanel,
            Scene::ClassSelect,
        ] {
            assert!(m.with_scene(scene).problems().is_empty(), "{why} {scene:?}");
        }
    }
}
