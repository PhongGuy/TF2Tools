//! The made-up spectator view and what the game's code writes on it.

mod common;

use common::hud_stock::stock_files;
use tf2tools_core::{HudModel, PartKey, Platform, Rgba, SampleSet, Scene, SceneRule, Team};

#[test]
fn the_scene_previews_one_dead_player_watching_a_teammate() {
    assert_eq!(
        SceneRule::get(Scene::Spectator).sample(),
        SampleSet::Spectating
    );
    for team in [Team::Red, Team::Blue] {
        assert!(SampleSet::Spectating.players(team).is_empty());
        assert_eq!(SampleSet::Spectating.team_score(team), None);
    }
    assert!(SampleSet::Spectating.killer().is_none() && SampleSet::Spectating.winner().is_none());
    assert_eq!(SampleSet::Spectating.selected(), None);
}

#[test]
fn the_variables_fill_the_tip_and_the_key_names() {
    let fill = |text| SampleSet::Spectating.fill(text);
    assert!(fill("%tip%").starts_with("Tip: "));
    assert_eq!(fill("[%jump%]"), "[SPACE]");
    assert_eq!(fill("[%JUMP%]"), "[SPACE]");
    assert_eq!(fill("[%attack%] [%attack2%]"), "[MOUSE1] [MOUSE2]");
    assert_eq!(fill("Press [ %changeclass% ]"), "Press [ N ]");
    assert_eq!(fill("%changeteam%"), "");
    // The other samples do not know these tokens.
    assert_eq!(SampleSet::Casual.fill("[%jump%]"), "[]");
    assert_eq!(SampleSet::Empty.fill("%tip%"), "");
}

#[test]
fn the_code_writes_six_labels_and_only_for_this_sample() {
    let text = |part: &str| SampleSet::Spectating.code_text(part);
    assert_eq!(text("ReinforcementsLabel"), Some("Respawn in: 7 seconds"));
    assert_eq!(text("mapLABEL"), Some("Map: 2Fort"));
    assert_eq!(text("ClassOrTeamLabel"), Some("#TF_Spectator_ChangeClass"));
    assert_eq!(
        text("SwitchCamModeKeyLabel"),
        Some("#TF_Spectator_SwitchCamModeKey")
    );
    assert_eq!(
        text("CycleTargetFwdKeyLabel"),
        Some("#TF_Spectator_CycleTargetFwdKey")
    );
    assert_eq!(
        text("CycleTargetRevKeyLabel"),
        Some("#TF_Spectator_CycleTargetRevKey")
    );
    // The tip is a dialog variable, the other camera labels keep the file's text.
    for part in [
        "TipLabel",
        "SwitchCamModeLabel",
        "CycleTargetFwdLabel",
        "BuyBackLabel",
        "Player1Name",
    ] {
        assert_eq!(text(part), None, "{part}");
    }
    for other in [
        SampleSet::Empty,
        SampleSet::Casual,
        SampleSet::Won,
        SampleSet::ClassPick,
    ] {
        assert_eq!(other.code_text("ReinforcementsLabel"), None);
    }
    // Nothing else the other hooks give.
    assert!(SampleSet::Spectating.code_label("Player1Name").is_none());
    assert!(
        SampleSet::Spectating
            .code_image("SubImage", Some("soldier"))
            .is_none()
    );
    assert!(
        SampleSet::Spectating
            .game_filled("ClassTipsPanel")
            .is_none()
    );
    assert!(!SampleSet::Spectating.is_class_button("soldier"));
}

#[test]
fn the_bars_are_filled_with_the_game_s_near_black() {
    let fill = |part: &str| SampleSet::Spectating.code_fill(part).map(Rgba::to_array);
    assert_eq!(fill("topbar"), Some([52, 48, 45, 255]));
    assert_eq!(fill("BottomBarBlank"), Some([52, 48, 45, 255]));
    assert_eq!(fill("BottomBar"), None);
    assert_eq!(SampleSet::Casual.code_fill("topbar"), None);
    // The class menu's room is unchanged, and its file's colour still beats the fallback.
    assert!(SampleSet::ClassPick.code_fill("MenuBG").is_some());
}

#[test]
fn only_the_spectator_bars_beat_the_file_s_colour() {
    for part in ["topbar", "BottomBarBlank"] {
        assert!(SampleSet::Spectating.code_fill_wins(part), "{part}");
    }
    assert!(!SampleSet::Spectating.code_fill_wins("BottomBar"));
    assert!(!SampleSet::ClassPick.code_fill_wins("MenuBG"));
    assert!(!SampleSet::Casual.code_fill_wins("topbar"));
}

#[test]
fn the_file_s_own_text_stays_the_file_s_in_the_model() {
    // The model carries the file's labelText; only the canvas swaps in the code's.
    let m = HudModel::load_scene(&stock_files(), Platform::Linux, Scene::Spectator).unwrap();
    let part = m.part(&PartKey::new("Spectator/MapLabel")).unwrap();
    assert_eq!(
        part.value("labelText").map(|l| l.text()),
        Some("from the file")
    );
}
