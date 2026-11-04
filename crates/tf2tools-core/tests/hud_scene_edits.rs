//! Scoreboard edits: code-owned root position, code-toggled hides, lists and the HUD's hide.

mod common;

use common::hud_stock::{scoreboard_with, stock_files};
use tf2tools_core::{
    Change, ChangeTarget, EditRefused, HudFiles, HudModel, PartKey, Platform, Scene, Screen,
};

const BOARD: &str = "resource/ui/scoreboard.res";
const HD: Screen = Screen::new(1920, 1080);
const ZEROED: (&str, &str) = ("    wide 640\n    tall 448\n", "    wide 0\n    tall 0\n");

fn board_of(files: &HudFiles) -> HudModel {
    HudModel::load_scene(files, Platform::Linux, Scene::Scoreboard).unwrap()
}

fn k(text: &str) -> PartKey {
    PartKey::new(text)
}

type Row = (String, String, String, Option<String>, String);

/// (file, block name, key, old, new) of each Set.
fn sets(changes: &[Change]) -> Vec<Row> {
    changes
        .iter()
        .map(|c| {
            let ChangeTarget::Set(s) = c.target() else {
                panic!("not a set")
            };
            let block = s.block().last().cloned().unwrap_or_default();
            (
                s.file().to_owned(),
                block,
                s.key().to_owned(),
                s.old().map(str::to_owned),
                s.value().to_owned(),
            )
        })
        .collect()
}

fn set(block: &str, key: &str, old: Option<&str>, new: &str) -> Row {
    (
        BOARD.to_owned(),
        block.to_owned(),
        key.to_owned(),
        old.map(str::to_owned),
        new.to_owned(),
    )
}

#[test]
fn the_root_resizes_and_hides_but_code_owns_its_position() {
    let m = board_of(&stock_files());
    let caps = m.capabilities(&k("scores")).unwrap();
    assert_eq!(
        (
            caps.move_x(),
            caps.move_y(),
            caps.resize_wide(),
            caps.resize_tall(),
            caps.hide()
        ),
        (false, false, true, true, true)
    );
    assert_eq!(
        m.move_part(&k("scores"), 10, 10, HD, 1),
        Err(EditRefused::NotOffered)
    );
    assert_eq!(
        sets(&m.hide_part(&k("scores"), 1).unwrap()),
        [
            set("scores", "wide", Some("640"), "0"),
            set("scores", "tall", Some("448"), "0")
        ]
    );
}

#[test]
fn code_toggled_children_hide_by_size_zero_even_with_visible_set() {
    let m = board_of(&stock_files());
    let hide = |key: &str| sets(&m.hide_part(&k(key), 2).unwrap());
    assert_eq!(
        hide("scores/BlueTeamImage"),
        [
            set("BlueTeamImage", "wide", Some("60"), "0"),
            set("BlueTeamImage", "tall", Some("60"), "0")
        ]
    );
    assert_eq!(
        hide("scores/BlueTeamLabel"),
        [
            set("BlueTeamLabel", "wide", Some("120"), "0"),
            set("BlueTeamLabel", "tall", Some("20"), "0")
        ]
    );
}

#[test]
fn player_lists_offer_no_recolour_though_they_set_fgcolor() {
    let m = board_of(&stock_files());
    let list = k("scores/BluePlayerList");
    let caps = m.capabilities(&list).unwrap();
    assert!(!caps.recolour());
    assert!(
        caps.move_x() && caps.move_y() && caps.resize_wide() && caps.resize_tall() && caps.hide()
    );
    assert_eq!(
        m.recolour_part(&list, "255 0 0 255", 3),
        Err(EditRefused::NotOffered)
    );
    assert!(
        m.capabilities(&k("scores/BlueTeamScore"))
            .unwrap()
            .recolour()
    );
}

#[test]
fn a_scoreboard_the_hud_hides_is_read_only() {
    let m = board_of(&scoreboard_with(ZEROED.0, ZEROED.1));
    for key in [
        "scores",
        "scores/BlueTeamScore",
        "scores/BluePlayerList",
        "scores/LocalPlayerStatsPanel/Kills",
    ] {
        let part = k(key);
        let c = m.capabilities(&part).unwrap();
        let any = c.move_x()
            || c.move_y()
            || c.resize_wide()
            || c.resize_tall()
            || c.hide()
            || c.recolour();
        assert!(!any, "{key}");
        assert!(!m.part(&part).unwrap().is_readable(), "{key}");
        assert_eq!(
            m.set_layer(&part, 3, 4),
            Err(EditRefused::ReadOnly),
            "{key}"
        );
        assert_eq!(m.hide_part(&part, 4), Err(EditRefused::ReadOnly), "{key}");
    }
    assert_eq!(
        m.move_part(&k("scores/BlueTeamScore"), 5, 0, HD, 4),
        Err(EditRefused::ReadOnly)
    );
    let shown = board_of(&scoreboard_with("    visible 1\n", "    visible 0\n"));
    assert!(
        shown
            .part(&k("scores/BlueTeamScore"))
            .unwrap()
            .is_readable()
    );
    let hud = HudModel::load(&scoreboard_with(ZEROED.0, ZEROED.1), Platform::Linux).unwrap();
    assert!(hud.parts().iter().all(|p| p.is_readable()));
}

#[test]
fn the_players_own_hide_of_the_scoreboard_does_not_lock_it() {
    let m = board_of(&stock_files());
    let root = k("scores");
    let changes = m.hide_part(&root, 1).unwrap();
    let hidden = m.with_changes(&changes).into_model();
    assert!(hidden.layout(HD).get(&root).unwrap().hidden().is_some());
    assert_eq!(hidden.hidden_by_hud(), None);
    assert!(hidden.parts().iter().all(|p| p.is_readable()));
    assert_eq!(hidden.hide_groups(&root, &changes), [1]);
    let again = hidden.with_scene(Scene::Scoreboard);
    assert!(again.parts().iter().all(|p| p.is_readable()));
    // Unhiding replays the player's list without the hide group, other edits kept.
    let score = k("scores/BlueTeamScore");
    let mut mixed = m.set_layer(&score, 3, 2).unwrap();
    mixed.extend(changes);
    let hidden = m.with_changes(&mixed).into_model();
    assert!(hidden.layout(HD).get(&root).unwrap().hidden().is_some());
    let kept: Vec<Change> = mixed.into_iter().filter(|c| c.group() != 1).collect();
    assert_eq!(kept.len(), 1);
    let unhidden = hidden.with_changes(&kept).into_model();
    assert!(unhidden.layout(HD).get(&root).unwrap().hidden().is_none());
    let zpos = unhidden.part(&score).unwrap().value("zpos").unwrap();
    assert_eq!(zpos.text(), "3");

    let zeroed = board_of(&scoreboard_with(ZEROED.0, ZEROED.1));
    let replayed = zeroed.with_changes(&[]).into_model();
    assert!(replayed.hidden_by_hud().is_some());
    assert!(replayed.parts().iter().all(|p| !p.is_readable()));
    assert!(
        replayed
            .with_scene(Scene::Scoreboard)
            .parts()
            .iter()
            .all(|p| !p.is_readable())
    );
}
