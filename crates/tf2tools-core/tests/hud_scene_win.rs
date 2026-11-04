//! The win panel scene: its files, its root and the stock-shaped layout.

mod common;

use common::hud_stock::{stock_files, stock_files_with};
use tf2tools_core::{
    HideTechnique, HudFiles, HudModel, PanelRule, PartId, PartKey, Platform, Scene, SceneRule,
    Screen,
};

const WIN: &str = "resource/ui/winpanel.res";
const SD: Screen = Screen::new(640, 480);

fn win(files: &HudFiles) -> HudModel {
    HudModel::load_scene(files, Platform::Linux, Scene::WinPanel).unwrap()
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
fn the_scene_reads_hudlayout_and_winpanel_and_hides_the_hud() {
    let rule = SceneRule::get(Scene::WinPanel);
    assert_eq!(rule.label(), "Win panel");
    assert_eq!(rule.files(), ["scripts/hudlayout.res", WIN]);
    assert!(rule.read_only_files().is_empty() && rule.nested().is_empty());
    assert_eq!(rule.root(), Some("WinPanel"));
    assert!(rule.hides_hud() && rule.toggles_any_depth());
    let [panel] = rule.rules() else {
        panic!("one root")
    };
    assert_eq!(panel.id(), PartId::WinPanel);
    assert_eq!(
        (panel.panel(), panel.children_file(), panel.geometry()),
        (
            "WinPanel",
            WIN,
            [("scripts/hudlayout.res", "WinPanel")].as_slice()
        )
    );
    assert!(!panel.clickable() && panel.has_children());
    assert_eq!(panel.hide(), HideTechnique::NotOffered);
    assert_eq!(PanelRule::get(PartId::WinPanel).panel(), "WinPanel");
    assert_eq!(Scene::for_file(WIN), Some(Scene::WinPanel));
    // The layout file stays the HUD's.
    assert_eq!(Scene::for_file("scripts/hudlayout.res"), Some(Scene::Hud));
}

#[test]
fn the_stock_file_builds_the_panel_tree_at_the_boxes_the_game_gives_it() {
    let m = win(&stock_files());
    assert_eq!(m.scene(), Scene::WinPanel);
    assert!(m.problems().is_empty() && m.stub_files().is_empty());
    let root = m.part(&k("WinPanel")).unwrap();
    assert!(root.is_root() && root.is_readable());
    assert_eq!(root.owner(), Some(PartId::WinPanel));
    let keys: Vec<&str> = m.parts().iter().map(|p| p.key().as_str()).collect();
    for key in [
        "WinPanel/TeamScoresPanel",
        "WinPanel/TeamScoresPanel/BlueScoreBG",
        "WinPanel/TeamScoresPanel/RedTeamScore",
        "WinPanel/WinPanelBGBorder",
        "WinPanel/WinningTeamLabel",
        "WinPanel/Player3Score",
        "WinPanel/KillStreakPlayer1Name",
    ] {
        assert!(keys.contains(&key), "{key} in {keys:?}");
    }
    // The root is hudlayout's box; the children hang off it.
    assert_eq!(at(&m, "WinPanel"), (170, 215, 300, 300));
    assert_eq!(at(&m, "WinPanel/TeamScoresPanel"), (129, 228, 480, 60));
    assert_eq!(
        at(&m, "WinPanel/TeamScoresPanel/BlueScoreBG"),
        (179, 243, 135, 40)
    );
    assert_eq!(at(&m, "WinPanel/WinningTeamLabel"), (185, 287, 268, 24));
    assert_eq!(at(&m, "WinPanel/Player1Name"), (226, 369, 125, 20));
    assert_eq!(
        at(&m, "WinPanel/KillStreakPlayer1Name"),
        (226, 453, 125, 20)
    );
    // `cs-0.5` and `p0.94`: centred in the parent at 94% of its width, give or take rounding.
    let (x, y, wide, tall) = at(&m, "WinPanel/WinPanelBGBorder");
    assert!(
        (x - 179).abs() <= 1 && (wide - 282).abs() <= 1,
        "{x} {wide}"
    );
    assert_eq!((y, tall), (285, 185));
    assert!(m.hidden_by_hud().is_none());
}

#[test]
fn a_missing_or_unparsable_file_is_a_win_panel_problem_only() {
    for (text, why) in [(None, "missing"), (Some("}"), "unparsable")] {
        let files = stock_files_with(&[(WIN, text)]);
        let m = win(&files);
        assert_eq!(m.problems(), [WIN], "{why}");
        assert!(!m.part(&k("WinPanel")).unwrap().is_readable(), "{why}");
        for scene in [
            Scene::Hud,
            Scene::Scoreboard,
            Scene::MainMenu,
            Scene::FreezeCam,
        ] {
            assert!(m.with_scene(scene).problems().is_empty(), "{why} {scene:?}");
        }
    }
}
