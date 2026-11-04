//! The casual sample the scoreboard preview draws: players, ping bars, colours and labels.

use std::collections::HashSet;
use tf2tools_core::{PingLevel, SampleSet, Scene, SceneRule, Team};

#[test]
fn twelve_casual_players_a_team_sorted_by_score() {
    let s = SampleSet::Casual;
    let mut names = HashSet::new();
    for team in [Team::Red, Team::Blue] {
        let players = s.players(team);
        assert_eq!(players.len(), 12, "{team:?}");
        assert!(
            players.windows(2).all(|w| w[0].score() >= w[1].score()),
            "{team:?}"
        );
        for p in players {
            assert_eq!(p.team(), team);
            assert!(names.insert(p.name()), "{}", p.name());
        }
    }
    let red = s.players(Team::Red);
    assert_eq!(
        (red[0].name(), red[0].class(), red[0].score()),
        ("You", "soldier", 24)
    );
    assert!(red[0].local() && red[0].alive());
    let locals = [Team::Red, Team::Blue]
        .iter()
        .flat_map(|t| s.players(*t))
        .filter(|p| p.local())
        .count();
    assert_eq!(locals, 1);
    assert_eq!(s.selected(), Some((Team::Red, 0)));
    assert!(SampleSet::Empty.players(Team::Red).is_empty());
    assert_eq!(SampleSet::Empty.selected(), None);
    assert_eq!(
        SceneRule::get(Scene::Scoreboard).sample(),
        SampleSet::Casual
    );
}

#[test]
fn ping_shows_as_bars_by_the_game_thresholds() {
    use PingLevel::*;
    for (ms, want) in [
        (0, None),
        (1, Some(Low)),
        (124, Some(Low)),
        (125, Some(Medium)),
        (199, Some(Medium)),
        (200, Some(High)),
        (274, Some(High)),
        (275, Some(VeryHigh)),
        (999, Some(VeryHigh)),
    ] {
        assert_eq!(PingLevel::of(ms), want, "{ms}");
    }
    let red = SampleSet::Casual.players(Team::Red);
    assert_eq!(red[0].ping_image(), Some("../hud/scoreboard_ping_low"));
    assert_eq!(red[7].ping_image(), Some("../hud/scoreboard_ping_med_d"));
    assert_eq!(
        red[10].ping_image(),
        Some("../hud/scoreboard_ping_very_high_d")
    );
}

#[test]
fn row_colours_follow_team_life_and_the_local_player() {
    let c = |team: Team, alive, local| team.row_colour(alive, local).to_array();
    assert_eq!(c(Team::Red, true, false), [255, 64, 64, 255]);
    assert_eq!(c(Team::Red, true, true), [255, 64, 64, 255]);
    assert_eq!(c(Team::Blue, true, false), [153, 204, 255, 255]);
    assert_eq!(c(Team::Blue, true, true), [153, 204, 255, 255]);
    assert_eq!(c(Team::Red, false, false), [135, 83, 83, 255]);
    assert_eq!(c(Team::Red, false, true), [182, 75, 75, 255]);
    assert_eq!(c(Team::Blue, false, false), [81, 97, 129, 255]);
    assert_eq!(c(Team::Blue, false, true), [123, 153, 187, 255]);
    let red = SampleSet::Casual.players(Team::Red);
    assert_eq!(red[7].row_colour().to_array(), [135, 83, 83, 255]);
}

#[test]
fn dialog_variables_fill_the_labels() {
    let s = SampleSet::Casual;
    for (name, want) in [
        ("blueteamscore", "2"),
        ("redteamscore", "1"),
        ("BlueTeamPlayerCount", "12 players"),
        ("redteamplayercount", "12 players"),
        ("blueteamname", "BLU"),
        ("redteamname", "RED"),
        ("server", "TF2Tools Preview"),
        ("mapname", "2Fort"),
        ("servertimeleft", "25:00"),
        ("servertime", "25:00"),
        ("playername", "You"),
        ("playerscore", "24"),
        ("kills", "12"),
        ("deaths", "4"),
        ("score", "0"),
        ("Revenge", "0"),
        ("damage", "0"),
        ("spectators", ""),
    ] {
        assert_eq!(s.variable(name), Some(want), "{name}");
    }
    assert_eq!(s.variable("nosuchvar"), None);
    assert_eq!(SampleSet::Empty.variable("server"), None);
    assert_eq!(
        (s.team_score(Team::Blue), s.team_score(Team::Red)),
        (Some(2), Some(1))
    );
    assert_eq!(SampleSet::Empty.team_score(Team::Red), None);
}

#[test]
fn class_icons_show_for_your_own_team_only() {
    let red = SampleSet::Casual.players(Team::Red);
    let blue = SampleSet::Casual.players(Team::Blue);
    assert_eq!(
        red[0].class_image(Team::Red),
        Some("../hud/leaderboard_class_soldier")
    );
    assert_eq!(red[2].class(), "demoman");
    assert_eq!(
        red[2].class_image(Team::Red),
        Some("../hud/leaderboard_class_demo")
    );
    assert_eq!((red[7].name(), red[7].alive()), ("Long Range", false));
    assert_eq!(
        red[7].class_image(Team::Red),
        Some("../hud/leaderboard_class_sniper_d")
    );
    assert!(blue.iter().all(|p| p.class_image(Team::Red).is_none()));
    assert_eq!(
        blue[0].class_image(Team::Blue),
        Some("../hud/leaderboard_class_soldier")
    );
}

#[test]
fn each_class_kills_with_its_stock_primary() {
    let weapons: Vec<(&str, &str)> = [Team::Red, Team::Blue]
        .iter()
        .flat_map(|t| SampleSet::Casual.players(*t))
        .map(|p| (p.class(), p.kill_weapon()))
        .collect();
    for (class, weapon) in [
        ("scout", "scattergun"),
        ("soldier", "tf_projectile_rocket"),
        ("pyro", "flamethrower"),
        ("demoman", "tf_projectile_pipe"),
        ("heavy", "minigun"),
        ("engineer", "obj_sentrygun"),
        ("medic", "syringegun_medic"),
        ("sniper", "sniperrifle"),
        ("spy", "knife"),
    ] {
        assert!(weapons.contains(&(class, weapon)), "{class}");
    }
}
