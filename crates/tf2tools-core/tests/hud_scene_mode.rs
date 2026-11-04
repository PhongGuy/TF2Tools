//! Scoreboard modes: one rule per mode and which mode owns a file.

use tf2tools_core::{BoardMode, Scene, SceneRule};

#[test]
fn the_scoreboard_has_a_rule_for_each_mode_and_no_other_scene_does() {
    assert_eq!(
        BoardMode::ALL,
        [BoardMode::Casual, BoardMode::Competitive, BoardMode::Mvm]
    );
    assert_eq!(BoardMode::default(), BoardMode::Casual);
    assert_eq!(
        BoardMode::ALL.map(BoardMode::label),
        ["Casual", "Competitive", "MvM"]
    );
    let casual = SceneRule::for_mode(Scene::Scoreboard, BoardMode::Casual);
    assert!(std::ptr::eq(casual, SceneRule::get(Scene::Scoreboard)));
    for mode in BoardMode::ALL {
        let rule = SceneRule::for_mode(Scene::Scoreboard, mode);
        assert_eq!(
            (rule.scene(), rule.mode(), rule.root()),
            (Scene::Scoreboard, mode, Some("scores"))
        );
        assert!(rule.hides_hud(), "{mode:?}");
        assert_eq!(rule.rules().len(), 1, "{mode:?}");
    }
    for scene in Scene::ALL.into_iter().filter(|&s| s != Scene::Scoreboard) {
        for mode in BoardMode::ALL {
            assert!(
                std::ptr::eq(SceneRule::for_mode(scene, mode), SceneRule::get(scene)),
                "{scene:?}"
            );
        }
        assert_eq!(SceneRule::get(scene).mode(), BoardMode::Casual, "{scene:?}");
    }
}

#[test]
fn each_mode_reads_its_files_and_turns_if_mvm_on_only_in_mvm() {
    let rule = |m| SceneRule::for_mode(Scene::Scoreboard, m);
    let board = "resource/ui/scoreboard.res";
    let mvm = "resource/ui/mvmscoreboard.res";
    assert_eq!(rule(BoardMode::Casual).files(), [board]);
    assert_eq!(rule(BoardMode::Competitive).files(), [board]);
    assert_eq!(rule(BoardMode::Mvm).files(), [board, mvm]);
    assert_eq!(rule(BoardMode::Mvm).nested(), [("MvMScoreboard", mvm)]);
    assert!(rule(BoardMode::Casual).nested().is_empty());
    assert!(rule(BoardMode::Competitive).nested().is_empty());
    assert!(rule(BoardMode::Casual).conditions().is_empty());
    assert!(rule(BoardMode::Competitive).conditions().is_empty());
    assert_eq!(rule(BoardMode::Mvm).conditions(), ["if_mvm"]);
    assert!(rule(BoardMode::Mvm).toggles_any_depth());
    assert!(!rule(BoardMode::Casual).toggles_any_depth());
}

#[test]
fn every_lists_each_rule_once_and_a_file_finds_its_scene_and_mode() {
    let every = SceneRule::every();
    assert_eq!(every.len(), Scene::ALL.len() + 2);
    for scene in Scene::ALL {
        assert!(
            every
                .iter()
                .any(|r| std::ptr::eq(*r, SceneRule::get(scene))),
            "{scene:?}"
        );
    }
    assert_eq!(
        Scene::for_file("Resource/UI/MvMScoreboard.res"),
        Some(Scene::Scoreboard)
    );
    assert_eq!(
        BoardMode::for_file("Resource/UI/MvMScoreboard.res"),
        Some(BoardMode::Mvm)
    );
    assert_eq!(BoardMode::for_file("resource/ui/scoreboard.res"), None);
    assert_eq!(BoardMode::for_file("scripts/hudlayout.res"), None);
}
