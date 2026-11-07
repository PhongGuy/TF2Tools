//! The win panel's code toggles, matched by name at any depth.

use crate::common;

use common::hud_stock::{WIN_PANEL, stock_files, stock_files_with};
use tf2tools_core::{HiddenBy, HudModel, PartKey, Platform, Scene, SceneRule, Screen};

const SD: Screen = Screen::new(640, 480);

fn win() -> HudModel {
    HudModel::load_scene(&stock_files(), Platform::Linux, Scene::WinPanel).unwrap()
}

fn k(text: &str) -> PartKey {
    PartKey::new(text)
}

#[test]
fn the_table_has_the_game_s_27_cases_and_none_is_class_or_weapon_driven() {
    let rule = SceneRule::get(Scene::WinPanel);
    assert_eq!(rule.code_toggled().len(), 27);
    for t in rule.code_toggled() {
        assert!(t.shows_when().starts_with("Shows "), "{}", t.panel());
        assert!(t.shows_for().is_none(), "{}", t.panel());
    }
    let shown = |name: &str| rule.toggled(name).map(|t| t.shown_in_sample());
    for name in [
        "TeamScoresPanel",
        "BlueTeamLabel",
        "RedTeamLabel",
        "Player2Name",
        "Player3Score",
        "KillStreakPlayer1Avatar",
    ] {
        assert_eq!(shown(name), Some(true), "{name}");
    }
    for name in [
        "BlueLeaderAvatar",
        "RedLeaderAvatarBG",
        "Player1Badge",
        "Player3Badge",
        "KillStreakPlayer1Badge",
    ] {
        assert_eq!(shown(name), Some(false), "{name}");
    }
    // Not toggled: the lines the code fills through variables stay as the file draws them.
    for name in ["WinningTeamLabel", "AdvancingTeamLabel", "WinPanelBGBorder"] {
        assert!(rule.toggled(name).is_none(), "{name}");
    }
}

#[test]
fn the_game_hides_the_leader_avatars_and_the_badges_and_shows_the_rest() {
    let m = win();
    let l = m.layout(SD);
    let hidden = |key: &str| l.get(&k(key)).unwrap().hidden();
    for key in [
        "WinPanel/TeamScoresPanel/BlueLeaderAvatar",
        "WinPanel/TeamScoresPanel/RedLeaderAvatarBG",
        "WinPanel/Player2Badge",
        "WinPanel/KillStreakPlayer1Badge",
    ] {
        assert_eq!(hidden(key), Some(HiddenBy::Game), "{key}");
    }
    for key in [
        "WinPanel/TeamScoresPanel",
        "WinPanel/TeamScoresPanel/BlueTeamLabel",
        "WinPanel/Player1Name",
        "WinPanel/KillStreakPlayer1Score",
    ] {
        assert_eq!(hidden(key), None, "{key}");
    }
    // A match toggle reaches a child one level down, not only the root's direct children.
    let nested = m
        .part(&k("WinPanel/TeamScoresPanel/BlueLeaderAvatarBG"))
        .unwrap();
    assert!(nested.code_toggled().is_some());
}

#[test]
fn a_hud_that_shows_a_badge_the_game_hides_still_gets_the_game_s_hide() {
    // The file says `visible 1`; the game decides, so the preview hides it.
    let shown = WIN_PANEL.replace(
        "Player1Badge { ControlName CTFBadgePanel xpos 11 ypos 152 zpos 3 wide 25 tall 25 visible 0 }",
        "Player1Badge { ControlName CTFBadgePanel xpos 11 ypos 152 zpos 3 wide 25 tall 25 visible 1 }",
    );
    assert_ne!(shown, WIN_PANEL);
    let files = stock_files_with(&[("resource/ui/winpanel.res", Some(&shown))]);
    let m = HudModel::load_scene(&files, Platform::Linux, Scene::WinPanel).unwrap();
    assert_eq!(
        m.layout(SD)
            .get(&k("WinPanel/Player1Badge"))
            .unwrap()
            .hidden(),
        Some(HiddenBy::Game)
    );
}
