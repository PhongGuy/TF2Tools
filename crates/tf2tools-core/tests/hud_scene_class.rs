//! The class select scene: its file, its root and the stock-shaped layout.

mod common;

use common::hud_stock::{stock_files, stock_files_with};
use tf2tools_core::{
    HideTechnique, HudFiles, HudModel, PanelRule, PartId, PartKey, Platform, Scene, SceneRule,
    Screen,
};

const FILE: &str = "resource/ui/classselection.res";
const SD: Screen = Screen::new(640, 480);

fn menu(files: &HudFiles) -> HudModel {
    HudModel::load_scene(files, Platform::Linux, Scene::ClassSelect).unwrap()
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
fn the_scene_reads_one_file_and_hides_the_hud() {
    let rule = SceneRule::get(Scene::ClassSelect);
    assert_eq!(rule.label(), "Class select");
    assert_eq!(rule.files(), [FILE]);
    assert!(rule.read_only_files().is_empty() && rule.nested().is_empty());
    assert_eq!(rule.root(), Some("class"));
    assert!(rule.hides_hud() && !rule.toggles_any_depth());
    let [panel] = rule.rules() else {
        panic!("one root")
    };
    assert_eq!(panel.id(), PartId::ClassSelect);
    assert_eq!((panel.panel(), panel.children_file()), ("class", FILE));
    assert!(panel.geometry().is_empty() && !panel.clickable() && panel.has_children());
    assert_eq!(panel.hide(), HideTechnique::NotOffered);
    assert_eq!(PanelRule::get(PartId::ClassSelect).panel(), "class");
    assert_eq!(Scene::for_file(FILE), Some(Scene::ClassSelect));
}

#[test]
fn the_stock_file_builds_the_menu_tree_at_the_boxes_the_game_gives_it() {
    let m = menu(&stock_files());
    assert_eq!(m.scene(), Scene::ClassSelect);
    assert!(m.problems().is_empty() && m.stub_files().is_empty());
    let root = m.part(&k("class")).unwrap();
    assert!(root.is_root() && root.is_readable());
    assert_eq!(root.owner(), Some(PartId::ClassSelect));
    let keys: Vec<&str> = m.parts().iter().map(|p| p.key().as_str()).collect();
    for key in [
        "class/soldier",
        "class/soldier/SubImage",
        "class/numSoldier",
        "class/countImage10",
        "class/ClassTipsPanel",
        "class/TFPlayerModel",
        "class/MenuBG",
        "class/StartExplanation/TitleLabel",
    ] {
        assert!(keys.contains(&key), "{key} in {keys:?}");
    }
    // The root fills the screen; every control sits against the screen's centre or edges.
    assert_eq!(at(&m, "class"), (0, 0, 640, 480));
    assert_eq!(at(&m, "class/soldier"), (70, -5, 50, 100));
    assert_eq!(at(&m, "class/soldier/SubImage"), (70, -5, 50, 100));
    assert_eq!(at(&m, "class/numSoldier"), (70, 10, 50, 12));
    assert_eq!(at(&m, "class/random"), (570, -5, 50, 100));
    assert_eq!(at(&m, "class/countImage0"), (340, 345, 30, 60));
    assert_eq!(at(&m, "class/localPlayerImage"), (290, 330, 45, 90));
    assert_eq!(at(&m, "class/ClassTipsPanel"), (348, 156, 235, 165));
    assert_eq!(at(&m, "class/TFPlayerModel"), (0, 0, 480, 480));
    assert_eq!(at(&m, "class/CancelButton"), (25, 442, 100, 25));
    assert_eq!(at(&m, "class/EditLoadoutButton"), (460, 442, 150, 25));
    assert!(m.hidden_by_hud().is_none());
}

#[test]
fn a_missing_or_unparsable_file_is_a_class_select_problem_only() {
    for (text, why) in [(None, "missing"), (Some("}"), "unparsable")] {
        let files = stock_files_with(&[(FILE, text)]);
        let m = menu(&files);
        assert_eq!(m.problems(), [FILE], "{why}");
        assert!(!m.part(&k("class")).unwrap().is_readable(), "{why}");
        for scene in [
            Scene::Hud,
            Scene::Scoreboard,
            Scene::MainMenu,
            Scene::FreezeCam,
            Scene::WinPanel,
        ] {
            assert!(m.with_scene(scene).problems().is_empty(), "{why} {scene:?}");
        }
    }
}

#[test]
fn the_class_buttons_paint_over_the_model_that_shares_their_zpos() {
    // The model and the buttons all set zpos 6, the model listed last; the game puts a later
    // sibling of equal zpos beneath, so every button and its picture paint over the model.
    let l = menu(&stock_files()).layout(SD);
    let model = l.paint_rank(&k("class/TFPlayerModel")).unwrap();
    for button in ["scout", "soldier", "heavyweapons", "random"] {
        for key in [
            format!("class/{button}"),
            format!("class/{button}/SubImage"),
        ] {
            assert!(l.paint_rank(&k(&key)).unwrap() > model, "{key}");
        }
    }
    // A HUD that raises the model's zpos above the buttons' puts it on top, as the game does.
    let raised = common::hud_stock::class_select().replace(
        "TFPlayerModel { ControlName CTFPlayerModelPanel xpos 0 ypos 0 zpos 6",
        "TFPlayerModel { ControlName CTFPlayerModelPanel xpos 0 ypos 0 zpos 8",
    );
    let l = menu(&stock_files_with(&[(FILE, Some(&raised))])).layout(SD);
    let model = l.paint_rank(&k("class/TFPlayerModel")).unwrap();
    assert!(l.paint_rank(&k("class/soldier/SubImage")).unwrap() < model);
}
