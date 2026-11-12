//! The freeze cam's code toggles, matched by name at any depth.

use crate::common;

use common::hud_stock::stock_files;
use tf2tools_core::{HiddenBy, HudModel, PartKey, Platform, Scene, SceneRule, Screen};

const SD: Screen = Screen::new(640, 480);

fn freeze() -> HudModel {
    HudModel::load_scene(&stock_files(), Platform::Linux, Scene::FreezeCam).unwrap()
}

fn k(text: &str) -> PartKey {
    PartKey::new(text)
}

#[test]
fn the_game_shows_the_screenshot_hint_and_hides_the_rest() {
    let rule = SceneRule::get(Scene::FreezeCam);
    assert!(rule.toggles_any_depth());
    for scene in [Scene::Hud, Scene::Scoreboard, Scene::MainMenu] {
        assert!(!SceneRule::get(scene).toggles_any_depth(), "{scene:?}");
    }
    let table: Vec<(&str, bool)> = (rule.code_toggled().iter())
        .map(|t| (t.panel(), t.shown_in_sample()))
        .collect();
    assert_eq!(
        table,
        [
            ("ScreenshotPanel", true),
            ("itempanel", false),
            ("NemesisSubPanel", false),
            ("PlayerStatusHealthBonusImage", false),
            ("BuildingStatusHealthImageBG", false),
            ("ChatInputLine", false),
            ("ChatFiltersButton", false),
            ("TimePanelValue", false),
        ]
    );
    for t in rule.code_toggled() {
        assert!(t.shows_when().starts_with("Shows "), "{}", t.panel());
        assert!(t.shows_for().is_none(), "{}", t.panel());
    }
    let m = freeze();
    let l = m.layout(SD);
    let hidden = |key: &str| l.get(&k(key)).unwrap().hidden();
    // The file hides the hint (`visible 0`); the preview shows it as the game does.
    assert_eq!(hidden("FreezePanel/ScreenshotPanel"), None);
    for key in [
        "FreezePanel/itempanel",
        "FreezePanel/FreezePanelBase/NemesisSubPanel",
    ] {
        assert_eq!(hidden(key), Some(HiddenBy::Game), "{key}");
    }
    // A child inside a hidden panel is hidden through its parent, not toggled itself.
    assert!(
        m.part(&k(
            "FreezePanel/FreezePanelBase/NemesisSubPanel/NemesisLabel"
        ))
        .unwrap()
        .code_toggled()
        .is_none()
    );
    assert_eq!(
        hidden("FreezePanel/FreezePanelBase/FreezeLabelKiller"),
        None
    );
    // The other scenes keep matching the root and its direct children only.
    let board = HudModel::load_scene(&stock_files(), Platform::Linux, Scene::Scoreboard).unwrap();
    assert!(
        board
            .part(&k("scores/LocalPlayerStatsPanel/Kills"))
            .unwrap()
            .code_toggled()
            .is_none()
    );
}
