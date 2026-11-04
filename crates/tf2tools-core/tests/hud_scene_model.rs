//! The scene parameter: which parts a model resolves, shared documents and scoped problems.

mod common;

use common::hud_stock::{scoreboard_with, stock_files, stock_files_with};
use tf2tools_core::{Change, HudModel, PartId, PartKey, Platform, Scene, SetKey};

const BOARD: &str = "resource/ui/scoreboard.res";

fn board() -> HudModel {
    HudModel::load_scene(&stock_files(), Platform::Linux, Scene::Scoreboard).unwrap()
}

fn keys(m: &HudModel) -> Vec<&str> {
    m.parts().iter().map(|p| p.key().as_str()).collect()
}

fn k(text: &str) -> PartKey {
    PartKey::new(text)
}

#[test]
fn the_hud_scene_is_the_default_and_unchanged() {
    let hud = HudModel::load(&stock_files(), Platform::Linux).unwrap();
    assert_eq!(hud.scene(), Scene::Hud);
    let explicit = HudModel::load_scene(&stock_files(), Platform::Linux, Scene::Hud).unwrap();
    assert_eq!(keys(&hud), keys(&explicit));
    assert!(keys(&hud).iter().all(|k| !k.starts_with("scores")));
    assert!(
        hud.part(&k("HudMedicCharge"))
            .unwrap()
            .code_toggled()
            .is_some()
    );
    assert!(
        hud.part(&k(
            "HudPlayerStatus/HudPlayerHealth/PlayerStatusHealthValue"
        ))
        .unwrap()
        .code_toggled()
        .is_none()
    );
    assert!(hud.problems().is_empty());
}

#[test]
fn the_scoreboard_scene_resolves_the_scores_tree() {
    let m = board();
    assert_eq!(m.scene(), Scene::Scoreboard);
    assert_eq!(
        keys(&m),
        [
            "scores",
            "scores/BlueTeamImage",
            "scores/BlueLeaderAvatar",
            "scores/BlueTeamLabel",
            "scores/BlueTeamScore",
            "scores/BluePlayerList",
            "scores/RedPlayerList",
            "scores/ServerLabel",
            "scores/ClassImage",
            "scores/classmodelpanel",
            "scores/LocalPlayerStatsPanel",
            "scores/LocalPlayerStatsPanel/Kills",
            "scores/LocalPlayerDuelStatsPanel",
            "scores/MvMScoreboard",
        ]
    );
    let root = m.part(&k("scores")).unwrap();
    assert!(root.is_root() && root.is_readable());
    assert_eq!(root.owner(), Some(PartId::Scoreboard));
    assert_eq!(root.class(), Some("CTFClientScoreBoardDialog"));
    assert_eq!(root.value("avatar_width").unwrap().text(), "28");
    let list = m.part(&k("scores/BluePlayerList")).unwrap();
    assert_eq!(list.class(), Some("SectionedListPanel"));
    let block = [
        "Resource/UI/Scoreboard.res".to_owned(),
        "BluePlayerList".to_owned(),
    ];
    assert_eq!(list.own_block(), (BOARD, block.as_slice()));
    assert!(m.problems().is_empty());
}

#[test]
fn if_mvm_blocks_stay_inactive() {
    let m = board();
    let server = m.part(&k("scores/ServerLabel")).unwrap();
    assert_eq!(server.value("ypos").unwrap().text(), "350");
    assert!(
        m.part(&k("scores/RedPlayerList"))
            .unwrap()
            .value("visible")
            .is_none()
    );
}

#[test]
fn code_toggled_marks_the_root_and_its_named_children_only() {
    let m = board();
    let toggled = |m: &HudModel, key: &str| {
        let part = m.part(&k(key)).unwrap();
        part.code_toggled()
            .map(|t| (t.panel(), t.shown_in_sample()))
    };
    assert_eq!(toggled(&m, "scores"), Some(("scores", true)));
    assert_eq!(
        toggled(&m, "scores/BlueTeamLabel"),
        Some(("BlueTeamLabel", false))
    );
    assert_eq!(
        toggled(&m, "scores/classmodelpanel"),
        Some(("classmodelpanel", true))
    );
    assert_eq!(
        toggled(&m, "scores/BluePlayerList"),
        Some(("BluePlayerList", true))
    );
    assert_eq!(toggled(&m, "scores/ServerLabel"), None);
    assert_eq!(toggled(&m, "scores/LocalPlayerStatsPanel/Kills"), None);
    // A nested block named like a toggled child is not the child code toggles.
    let nested = scoreboard_with(
        "Kills {",
        "ClassImage { ControlName ImagePanel wide 5 tall 5 }\n    Kills {",
    );
    let m = HudModel::load_scene(&nested, Platform::Linux, Scene::Scoreboard).unwrap();
    assert_eq!(toggled(&m, "scores/LocalPlayerStatsPanel/ClassImage"), None);
}

#[test]
fn a_missing_or_broken_scoreboard_leaves_the_hud_scene_alone() {
    let stock_hud = HudModel::load(&stock_files(), Platform::Linux).unwrap();
    // Nesting past 64 levels is the parser's refusal (as in hud_model.rs); a comment has no root.
    let deep = format!("\"R\"\n{}{}", "{\n".repeat(65), "}\n".repeat(65));
    for text in [None, Some(deep.as_str()), Some("// only a comment\n")] {
        let files = stock_files_with(&[(BOARD, text)]);
        let m = HudModel::load_scene(&files, Platform::Linux, Scene::Scoreboard).unwrap();
        assert_eq!(m.problems(), [BOARD], "{text:?}");
        assert!(!m.part(&k("scores")).unwrap().is_readable(), "{text:?}");
        let hud = HudModel::load(&files, Platform::Linux).unwrap();
        assert!(hud.problems().is_empty(), "{text:?}");
        assert_eq!(keys(&hud), keys(&stock_hud), "{text:?}");
    }
    let files = stock_files_with(&[("resource/ui/hudmediccharge.res", None)]);
    let m = HudModel::load_scene(&files, Platform::Linux, Scene::Scoreboard).unwrap();
    assert!(m.problems().is_empty());
}

#[test]
fn one_change_list_serves_every_scene() {
    let hud = HudModel::load(&stock_files(), Platform::Linux).unwrap();
    let change = Change::set(
        1,
        SetKey::new(
            BOARD,
            &["Resource/UI/Scoreboard.res", "BlueTeamScore"],
            "zpos",
            None,
            None,
            "4",
        ),
    );
    let edited = hud.with_changes(std::slice::from_ref(&change));
    assert_eq!(
        (edited.applied(), edited.dropped()),
        ([change].as_slice(), 0)
    );
    assert_eq!(edited.touched(), [BOARD]);
    assert_eq!(edited.model().scene(), Scene::Hud);
    let shown = edited.model().with_scene(Scene::Scoreboard);
    assert_eq!(shown.scene(), Scene::Scoreboard);
    let score = shown.part(&k("scores/BlueTeamScore")).unwrap();
    assert_eq!(score.value("zpos").unwrap().text(), "4");
    assert_eq!(keys(&shown), keys(&board()));
    assert_eq!(keys(&shown.with_scene(Scene::Hud)), keys(&hud));
    assert_eq!(board().with_changes(&[]).model().scene(), Scene::Scoreboard);
    let replayed = shown.with_changes(&[]).into_model();
    let score = replayed.part(&k("scores/BlueTeamScore")).unwrap();
    assert_ne!(score.value("zpos").map(|v| v.text()), Some("4"));
}
