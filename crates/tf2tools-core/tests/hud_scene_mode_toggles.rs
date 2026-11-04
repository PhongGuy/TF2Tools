//! What the game's code shows in each scoreboard mode.

mod common;

use common::hud_stock::stock_files;
use tf2tools_core::{BoardMode, HiddenBy, HudModel, PartKey, Platform, Scene, SceneRule, Screen};

const HD: Screen = Screen::new(1920, 1080);

fn table(mode: BoardMode) -> Vec<(&'static str, bool)> {
    let rule = SceneRule::for_mode(Scene::Scoreboard, mode);
    rule.code_toggled()
        .iter()
        .map(|t| (t.panel(), t.shown_in_sample()))
        .collect()
}

fn hidden(mode: BoardMode, key: &str) -> Option<HiddenBy> {
    HudModel::load_scene(&stock_files(), Platform::Linux, Scene::Scoreboard)
        .unwrap()
        .with_board_mode(mode)
        .layout(HD)
        .get(&PartKey::new(key))
        .unwrap_or_else(|| panic!("{key}"))
        .hidden()
}

#[test]
fn competitive_differs_from_casual_in_the_two_team_names_only() {
    let (casual, comp) = (table(BoardMode::Casual), table(BoardMode::Competitive));
    assert_eq!(casual.len(), comp.len());
    let changed: Vec<_> = casual
        .iter()
        .zip(&comp)
        .filter(|(a, b)| a != b)
        .map(|(_, b)| *b)
        .collect();
    assert_eq!(changed, [("BlueTeamLabel", true), ("RedTeamLabel", true)]);
    assert_eq!(
        hidden(BoardMode::Casual, "scores/BlueTeamLabel"),
        Some(HiddenBy::Game)
    );
    assert_eq!(hidden(BoardMode::Competitive, "scores/BlueTeamLabel"), None);
}

#[test]
fn mvm_hides_the_casual_panels_and_toggles_its_own() {
    let mvm = table(BoardMode::Mvm);
    for want in [
        ("scores", true),
        ("BlueTeamImage", false),
        ("RedTeamImage", false),
        ("BlueLeaderAvatar", false),
        ("BlueTeamLabel", false),
        ("RedTeamLabel", false),
        ("BluePlayerList", false),
        ("RedPlayerList", false),
        ("DifficultyContainer", true),
        ("RespecStatusLabel", false),
    ] {
        assert!(mvm.contains(&want), "{want:?} in {mvm:?}");
    }
    let rule = SceneRule::for_mode(Scene::Scoreboard, BoardMode::Mvm);
    for t in rule.code_toggled() {
        assert!(t.shows_when().starts_with("Shows "), "{}", t.panel());
    }
    assert!(rule.toggled("respecstatuslabel").is_some());
}

#[test]
fn the_layout_follows_the_mvm_table_at_any_depth() {
    let respec = "scores/MvMScoreboard/CreditStatsContainer/RespecStatusLabel";
    assert_eq!(hidden(BoardMode::Mvm, respec), Some(HiddenBy::Game));
    assert_eq!(
        hidden(BoardMode::Mvm, "scores/MvMScoreboard/DifficultyContainer"),
        None
    );
    assert_eq!(
        hidden(BoardMode::Mvm, "scores/BluePlayerList"),
        Some(HiddenBy::Game)
    );
    assert_eq!(
        hidden(BoardMode::Mvm, "scores/BlueTeamImage"),
        Some(HiddenBy::Game)
    );
    assert_eq!(hidden(BoardMode::Casual, "scores/BluePlayerList"), None);
}
