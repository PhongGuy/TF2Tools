//! The made-up won round the win panel previews, and what the game's code sets on its parts.

use crate::common;

use common::hud_stock::stock_files;
use tf2tools_core::{HudModel, PartKey, Platform, SampleSet, Scene, SceneRule, Team};

#[test]
fn the_win_panel_previews_one_made_up_won_round() {
    assert_eq!(SceneRule::get(Scene::WinPanel).sample(), SampleSet::Won);
    assert_eq!(SampleSet::Won.winner(), Some(Team::Blue));
    assert_eq!(SampleSet::Casual.winner(), None);
    assert_eq!(SampleSet::Empty.winner(), None);
    // No scoreboard rows, no killer, no match score of the casual sample's kind.
    for team in [Team::Red, Team::Blue] {
        assert!(SampleSet::Won.players(team).is_empty());
        assert_eq!(SampleSet::Won.team_score(team), None);
    }
    assert!(SampleSet::Won.killer().is_none());
}

#[test]
fn the_variables_fill_the_lines_the_game_s_code_sets() {
    let fill = |text| SampleSet::Won.fill(text);
    assert_eq!(fill("%WinningTeamLabel%"), "BLU TEAM WINS!");
    assert_eq!(fill("%winningteamlabel%"), "BLU TEAM WINS!");
    assert_eq!(fill("%AdvancingTeamLabel%"), "");
    assert_eq!(fill("%WinReasonLabel%"), "BLU captured all control points");
    assert_eq!(
        fill("%DetailsLabel%"),
        "Winning capture: Blue Rocket, Demoknight"
    );
    assert_eq!(fill("%TopPlayersLabel%"), "BLU Team MVPs:");
    assert_eq!(fill("%blueteamname% %blueteamscore%"), "BLU 2");
    assert_eq!(fill("%redteamname% %redteamscore%"), "RED 1");
    // The other samples do not know these tokens.
    assert_eq!(SampleSet::Casual.fill("%WinningTeamLabel%"), "");
    assert_eq!(SampleSet::Empty.fill("%DetailsLabel%"), "");
}

#[test]
fn the_top_players_are_the_blue_sample_s_best_and_draw_in_their_team_colour() {
    let name = |part: &str| SampleSet::Won.code_label(part).map(|l| l.text());
    assert_eq!(name("Player1Name"), Some("Blue Rocket"));
    assert_eq!(name("player2name"), Some("Market Garden"));
    assert_eq!(name("Player3Class"), Some("Demoman"));
    assert_eq!(name("Player1Score"), Some("22"));
    assert_eq!(name("KillStreakPlayer1Name"), Some("Rocket Jumper"));
    assert_eq!(name("KillStreakPlayer1Score"), Some("9"));
    assert_eq!(name("WinningTeamLabel"), None);
    assert_eq!(
        SampleSet::Casual
            .code_label("Player1Name")
            .map(|l| l.text()),
        None
    );
    assert_eq!(
        SampleSet::Empty.code_label("Player1Name").map(|l| l.text()),
        None
    );
    let colour = |part: &str| SampleSet::Won.code_label(part).unwrap().colour();
    assert_eq!(colour("Player2Name"), Team::Blue.row_colour(true, false));
    assert_eq!(colour("Player3Score"), Team::Blue.row_colour(true, false));
    assert_eq!(
        colour("KillStreakPlayer1Class"),
        Team::Red.row_colour(true, false)
    );
    // Every name is one the existing casual sample already has, with the score it has there.
    let known = |team: Team, name: &str, score: i32| {
        SampleSet::Casual
            .players(team)
            .iter()
            .any(|p| p.name() == name && p.score() == score)
    };
    assert!(known(Team::Blue, "Blue Rocket", 22));
    assert!(known(Team::Blue, "Market Garden", 19));
    assert!(known(Team::Blue, "Demoknight", 17));
    assert!(
        SampleSet::Casual
            .players(Team::Red)
            .iter()
            .any(|p| p.name() == "Rocket Jumper" && p.class() == "soldier")
    );
}

#[test]
fn the_game_sets_the_borders_by_team_whatever_the_file_says() {
    let border = |part: &str| SampleSet::Won.code_border(part);
    assert_eq!(border("WinPanelBGBorder"), Some("TFFatLineBorderBlueBG"));
    assert_eq!(border("BlueScoreBG"), Some("TFFatLineBorderBlueBG"));
    assert_eq!(border("RedScoreBG"), Some("TFFatLineBorderRedBG"));
    assert_eq!(border("MainBG"), None);
    assert_eq!(SampleSet::Casual.code_border("BlueScoreBG"), None);
    // Each named part exists in the scene, so the lookup has something to draw on.
    let m = HudModel::load_scene(&stock_files(), Platform::Linux, Scene::WinPanel).unwrap();
    for key in [
        "WinPanel/WinPanelBGBorder",
        "WinPanel/TeamScoresPanel/BlueScoreBG",
        "WinPanel/TeamScoresPanel/RedScoreBG",
    ] {
        let part = m.part(&PartKey::new(key)).unwrap();
        assert!(SampleSet::Won.code_border(part.name()).is_some(), "{key}");
    }
}
