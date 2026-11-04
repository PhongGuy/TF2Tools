//! Scenes: which files each reads, its rule table and the parts code shows or hides.

use tf2tools_core::{HideTechnique, MeterKind, PanelRule, PartId, SampleSet, Scene, SceneRule};

#[test]
fn every_scene_has_one_rule() {
    assert_eq!(
        Scene::ALL,
        [
            Scene::Hud,
            Scene::Scoreboard,
            Scene::MainMenu,
            Scene::FreezeCam,
            Scene::WinPanel,
            Scene::ClassSelect,
            Scene::Spectator
        ]
    );
    for scene in Scene::ALL {
        assert_eq!(SceneRule::get(scene).scene(), scene);
    }
    assert_eq!(SceneRule::get(Scene::Hud).label(), "HUD");
    assert_eq!(SceneRule::get(Scene::Scoreboard).label(), "Scoreboard");
    assert_eq!(SceneRule::get(Scene::MainMenu).label(), "Main menu");
    assert_eq!(
        SceneRule::get(Scene::FreezeCam).label(),
        "Death and freeze cam"
    );
    assert_eq!(SceneRule::get(Scene::WinPanel).label(), "Win panel");
    assert_eq!(SceneRule::get(Scene::ClassSelect).label(), "Class select");
    assert_eq!(SceneRule::get(Scene::Spectator).label(), "Spectator");
}

#[test]
fn the_hud_scene_is_stage_1_unchanged() {
    let hud = SceneRule::get(Scene::Hud);
    let ids: Vec<PartId> = hud.rules().iter().map(PanelRule::id).collect();
    let all: Vec<PartId> = PanelRule::all().iter().map(PanelRule::id).collect();
    assert_eq!(ids, all);
    let mut want: Vec<&str> = vec!["scripts/hudlayout.res"];
    for rule in PanelRule::all() {
        let paths = rule.geometry().iter().map(|(p, _)| *p);
        for path in paths.chain([rule.children_file()]) {
            if !want.contains(&path) {
                want.push(path);
            }
        }
        assert!(!rule.centres_y(), "{:?}", rule.id());
    }
    want.extend(MeterKind::ALL[1..].iter().map(|k| k.file()));
    assert_eq!(hud.files(), want.as_slice());
    assert!(hud.read_only_files().is_empty());
    assert_eq!(
        (hud.root(), hud.hides_hud(), hud.sample()),
        (None, false, SampleSet::Empty)
    );
    assert_eq!(hud.code_toggled().len(), 55);
}

#[test]
fn the_scoreboard_scene_reads_one_file_and_hides_the_hud() {
    let board = SceneRule::get(Scene::Scoreboard);
    assert_eq!(board.files(), ["resource/ui/scoreboard.res"]);
    assert!(board.read_only_files().is_empty());
    assert_eq!(board.root(), Some("scores"));
    assert!(board.hides_hud());
    assert_eq!(board.sample(), SampleSet::Casual);
    let [rule] = board.rules() else {
        panic!("one rule")
    };
    assert_eq!(rule.id(), PartId::Scoreboard);
    assert_eq!(
        (rule.panel(), rule.parent(), rule.children_file()),
        ("scores", None, "resource/ui/scoreboard.res")
    );
    assert_eq!(rule.label(), "Scoreboard");
    assert!(rule.shows_when().starts_with("Shows "));
    assert!(rule.geometry().is_empty());
    assert!(rule.conditions().is_empty(), "if_mvm stays inactive");
    for key in ["xpos", "ypos", "visible", "XPOS"] {
        assert!(rule.ignores(key), "{key}");
    }
    for key in ["wide", "tall", "alpha", "zpos"] {
        assert!(!rule.ignores(key), "{key}");
    }
    assert!(rule.centres_x() && rule.centres_y());
    assert_eq!(rule.hide(), HideTechnique::SizeZero);
    assert_eq!(PanelRule::get(PartId::Scoreboard).panel(), "scores");
    assert_eq!(
        PanelRule::get(PartId::RoundTimer).panel(),
        "ObjectiveStatusTimePanel"
    );
}

#[test]
fn code_toggled_parts_and_their_preview_defaults() {
    let board = SceneRule::get(Scene::Scoreboard);
    let table: Vec<(&str, bool)> = board
        .code_toggled()
        .iter()
        .map(|t| (t.panel(), t.shown_in_sample()))
        .collect();
    assert_eq!(
        table,
        [
            ("scores", true),
            ("BlueTeamImage", true),
            ("RedTeamImage", true),
            ("BlueLeaderAvatar", false),
            ("BlueLeaderAvatarBG", false),
            ("RedLeaderAvatar", false),
            ("RedLeaderAvatarBG", false),
            ("BlueTeamLabel", false),
            ("RedTeamLabel", false),
            ("ClassImage", false),
            ("classmodelpanel", true),
            ("LocalPlayerStatsPanel", true),
            ("LocalPlayerDuelStatsPanel", false),
            ("BluePlayerList", true),
            ("RedPlayerList", true),
        ]
    );
    for t in board.code_toggled() {
        assert_eq!(t.hide(), HideTechnique::SizeZero, "{}", t.panel());
        assert!(t.shows_when().starts_with("Shows "), "{}", t.panel());
    }
    let shows = |panel| board.toggled(panel).unwrap().shows_when();
    assert!(shows("RedTeamImage").contains("Mann vs. Machine"));
    assert_eq!(shows("scores"), board.rules()[0].shows_when());
    assert_eq!(
        board.toggled("bluETEAMlabel").map(|t| t.panel()),
        Some("BlueTeamLabel")
    );
    assert!(board.toggled("Kills").is_none());
}

#[test]
fn a_file_belongs_to_the_scene_that_reads_it() {
    assert_eq!(
        Scene::for_file("resource/ui/scoreboard.res"),
        Some(Scene::Scoreboard)
    );
    assert_eq!(
        Scene::for_file("Resource/UI/ScoreBoard.res"),
        Some(Scene::Scoreboard)
    );
    assert_eq!(Scene::for_file("scripts/hudlayout.res"), Some(Scene::Hud));
    assert_eq!(
        Scene::for_file("resource/ui/hudplayerhealth.res"),
        Some(Scene::Hud)
    );
    assert_eq!(
        Scene::for_file("resource/ui/winpanel.res"),
        Some(Scene::WinPanel)
    );
    assert_eq!(
        Scene::for_file("Resource/UI/ClassSelection.res"),
        Some(Scene::ClassSelect)
    );
    assert_eq!(
        Scene::for_file("Resource/UI/Spectator.res"),
        Some(Scene::Spectator)
    );
}
