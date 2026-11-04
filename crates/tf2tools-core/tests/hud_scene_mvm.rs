//! The MvM mode: the nested file, the `if_mvm` condition, edits and the mode's survival.

mod common;

use common::hud_stock::{stock_files, stock_files_with};
use tf2tools_core::{BoardMode, HiddenBy, HudModel, PartKey, Platform, Scene, SceneRule, Screen};

const HD: Screen = Screen::new(1920, 1080);
const MVM: &str = "scores/MvMScoreboard";
const LIST: &str = "scores/MvMScoreboard/MvMPlayerList";
const FILE: &str = "resource/ui/mvmscoreboard.res";

fn k(text: &str) -> PartKey {
    PartKey::new(text)
}

fn casual() -> HudModel {
    HudModel::load_scene(&stock_files(), Platform::Linux, Scene::Scoreboard).unwrap()
}

fn mvm() -> HudModel {
    casual().with_board_mode(BoardMode::Mvm)
}

fn ypos(m: &HudModel, key: &str) -> Option<String> {
    m.part(&k(key))
        .unwrap()
        .value("ypos")
        .map(|l| l.text().to_owned())
}

#[test]
fn the_default_mode_is_casual_and_draws_no_mvm_parts() {
    let m = casual();
    assert_eq!(m.board_mode(), BoardMode::Casual);
    assert!(std::ptr::eq(
        m.scene_rule(),
        SceneRule::get(Scene::Scoreboard)
    ));
    assert!(m.part(&k(LIST)).is_none());
    assert_eq!(
        m.layout(HD).get(&k(MVM)).unwrap().hidden(),
        Some(HiddenBy::Visible)
    );
    let again = m.with_board_mode(BoardMode::Casual);
    assert_eq!(
        m.parts()
            .iter()
            .map(|p| p.key().as_str())
            .collect::<Vec<_>>(),
        again
            .parts()
            .iter()
            .map(|p| p.key().as_str())
            .collect::<Vec<_>>()
    );
}

#[test]
fn mvm_hangs_the_file_under_its_panel_and_turns_if_mvm_on() {
    let (c, m) = (casual(), mvm());
    assert_eq!(m.board_mode(), BoardMode::Mvm);
    for key in [
        LIST,
        "scores/MvMScoreboard/WaveStatusPanel",
        "scores/MvMScoreboard/DifficultyContainer/DifficultyValue",
        "scores/MvMScoreboard/CreditStatsContainer/RespecStatusLabel",
    ] {
        assert!(m.part(&k(key)).is_some_and(|p| p.is_readable()), "{key}");
    }
    assert_eq!(ypos(&c, "scores/ServerLabel").as_deref(), Some("350"));
    assert_eq!(
        ypos(&m, "scores/ServerLabel").as_deref(),
        Some("0"),
        "if_mvm applies"
    );
    assert_eq!(
        m.layout(HD).get(&k(MVM)).unwrap().hidden(),
        None,
        "if_mvm shows the panel"
    );
}

#[test]
fn the_mode_survives_a_scene_switch_and_an_edit_in_the_nested_file() {
    let m = mvm().with_scene(Scene::Hud).with_scene(Scene::Scoreboard);
    assert_eq!(m.board_mode(), BoardMode::Mvm);
    // 45 px at 1080 high is 20 game units.
    let moved = m.move_part(&k(LIST), 45, 0, HD, 1).unwrap();
    assert_eq!(moved.len(), 1);
    let edited = m.with_changes(&moved);
    assert_eq!(edited.model().board_mode(), BoardMode::Mvm);
    assert_eq!(
        ypos(edited.model(), LIST).as_deref(),
        Some("79"),
        "only xpos moved"
    );
    let xpos = edited
        .model()
        .part(&k(LIST))
        .unwrap()
        .value("xpos")
        .map(|l| l.text().to_owned());
    assert_eq!(xpos.as_deref(), Some("55"));
    assert_eq!(edited.touched(), [FILE]);
}

#[test]
fn a_missing_mvm_file_is_a_problem_in_mvm_only() {
    let files = stock_files_with(&[(FILE, None)]);
    let c = HudModel::load_scene(&files, Platform::Linux, Scene::Scoreboard).unwrap();
    assert!(c.problems().is_empty());
    assert_eq!(c.with_board_mode(BoardMode::Mvm).problems(), [FILE]);
}
