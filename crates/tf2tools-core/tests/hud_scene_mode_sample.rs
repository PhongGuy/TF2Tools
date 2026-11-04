//! The Competitive and MvM samples.

use std::collections::HashSet;
use tf2tools_core::{BoardMode, SampleSet, Scene, SceneRule, Team};

#[test]
fn competitive_is_six_a_side_with_team_names() {
    let s = SampleSet::Competitive;
    let mut names = HashSet::new();
    for team in [Team::Red, Team::Blue] {
        let players = s.players(team);
        assert_eq!(players.len(), 6, "{team:?}");
        assert!(
            players.windows(2).all(|w| w[0].score() >= w[1].score()),
            "{team:?}"
        );
        assert!(
            players
                .iter()
                .all(|p| p.team() == team && p.mvm().is_none() && names.insert(p.name()))
        );
    }
    assert_eq!(s.selected(), Some((Team::Red, 0)));
    assert_eq!(
        (s.team_score(Team::Red), s.team_score(Team::Blue)),
        (Some(2), Some(3))
    );
    assert_eq!(s.variable("redteamname"), Some("Waffle Squad"));
    assert_eq!(s.variable("blueteamname"), Some("Pancake Club"));
    assert_eq!(s.variable("redteamplayercount"), Some("6 players"));
    for name in [
        "server",
        "mapname",
        "servertimeleft",
        "playername",
        "kills",
        "blueteamscore",
    ] {
        assert!(s.variable(name).is_some(), "{name}");
    }
    assert_eq!(
        s.fill("%redteamname% vs %blueteamname%"),
        "Waffle Squad vs Pancake Club"
    );
}

#[test]
fn mvm_is_six_defenders_on_red_with_a_stat_row_each() {
    let s = SampleSet::Mvm;
    let red = s.players(Team::Red);
    assert_eq!(red.len(), 6);
    assert!(s.players(Team::Blue).is_empty());
    assert_eq!(
        (s.team_score(Team::Red), s.team_score(Team::Blue)),
        (None, None)
    );
    assert!(red.windows(2).all(|w| w[0].score() >= w[1].score()));
    assert_eq!(red.iter().filter(|p| p.local()).count(), 1);
    assert_eq!(s.selected(), Some((Team::Red, 0)));
    let first = red[0].mvm().expect("stats");
    assert_eq!(
        (red[0].name(), red[0].class(), red[0].score()),
        ("You", "soldier", 4210)
    );
    assert_eq!(
        (
            first.tour(),
            first.damage(),
            first.tank(),
            first.healing(),
            first.support(),
            first.credits(),
            first.squad_surplus()
        ),
        (12, 3890, 1200, 0, 640, 2150, false)
    );
    assert!(red.iter().all(|p| p.mvm().is_some()));
    assert!(red.iter().any(|p| !p.alive()));
    assert!(
        red.iter()
            .any(|p| p.mvm().is_some_and(|m| m.squad_surplus()))
    );
    assert!(
        red.iter()
            .any(|p| p.mvm().is_some_and(|m| m.healing() > 5000))
    );
    assert_eq!(s.variable("popfile"), Some("Operation Pancake Panic"));
    assert_eq!(s.variable("difficultyvalue"), Some("Intermediate"));
    assert_eq!(s.variable("respecstatus"), Some(""));
}

#[test]
fn casual_rows_carry_no_stats_and_the_modes_pick_their_samples() {
    assert!(
        SampleSet::Casual
            .players(Team::Red)
            .iter()
            .all(|p| p.mvm().is_none())
    );
    let sample = |m| SceneRule::for_mode(Scene::Scoreboard, m).sample();
    assert_eq!(sample(BoardMode::Casual), SampleSet::Casual);
    assert_eq!(sample(BoardMode::Competitive), SampleSet::Competitive);
    assert_eq!(sample(BoardMode::Mvm), SampleSet::Mvm);
}

#[test]
fn the_mvm_sample_notes_what_the_game_draws_itself() {
    let s = SampleSet::Mvm;
    assert_eq!(
        s.game_filled("WaveStatusPanel"),
        Some("The wave bar draws in game")
    );
    for part in [
        "PreviousWaveCreditInfoPanel",
        "TotalGameCreditInfoPanel",
        "PreviousWaveCreditSpendPanel",
        "TotalGameCreditSpendPanel",
    ] {
        assert_eq!(
            s.game_filled(part),
            Some("The credit tables draw in game"),
            "{part}"
        );
    }
    assert_eq!(SampleSet::Competitive.game_filled("WaveStatusPanel"), None);
    assert_eq!(
        SampleSet::ClassPick.game_filled("ClassTipsPanel"),
        Some("Class tips draw in game")
    );
}
