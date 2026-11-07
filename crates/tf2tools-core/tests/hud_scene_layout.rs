//! Scoreboard layout: code centring, code-toggled parts and a scoreboard the HUD hides.

use crate::common;

use common::hud_stock::{scoreboard_with, stock_files};
use tf2tools_core::{HiddenBy, HudFiles, HudLayout, HudModel, PartKey, Platform, Scene, Screen};

const HD: Screen = Screen::new(1920, 1080);

fn board_of(files: &HudFiles) -> HudModel {
    HudModel::load_scene(files, Platform::Linux, Scene::Scoreboard).unwrap()
}

fn rect(l: &HudLayout, key: &str) -> (i32, i32, i32, i32) {
    let b = l.get(&PartKey::new(key)).unwrap_or_else(|| panic!("{key}"));
    (b.x(), b.y(), b.wide(), b.tall())
}

fn hidden(l: &HudLayout, key: &str) -> Option<HiddenBy> {
    l.get(&PartKey::new(key))
        .unwrap_or_else(|| panic!("{key}"))
        .hidden()
}

#[test]
fn code_centres_the_scoreboard_on_every_screen() {
    let m = board_of(&stock_files());
    assert_eq!(rect(&m.layout(HD), "scores"), (240, 36, 1440, 1008));
    assert_eq!(
        rect(&m.layout(Screen::new(3840, 1080)), "scores"),
        (1200, 36, 1440, 1008)
    );
    assert_eq!(
        rect(&m.layout(Screen::new(1280, 1024)), "scores"),
        (-42, 34, 1365, 955)
    );
    let moved = board_of(&scoreboard_with(
        "    xpos cs-0.5\n    ypos 31\n",
        "    xpos 100\n    ypos 9999\n",
    ));
    assert_eq!(rect(&moved.layout(HD), "scores"), (240, 36, 1440, 1008));
    assert_eq!(
        rect(&m.layout(HD), "scores/BluePlayerList"),
        (251, 186, 697, 630)
    );
}

#[test]
fn code_toggled_parts_show_as_the_casual_preview_does() {
    let l = board_of(&stock_files()).layout(HD);
    for key in [
        "scores",
        "scores/BlueTeamImage",
        "scores/BluePlayerList",
        "scores/RedPlayerList",
        "scores/classmodelpanel",
        "scores/LocalPlayerStatsPanel",
        "scores/ServerLabel",
    ] {
        assert_eq!(hidden(&l, key), None, "{key}");
    }
    for key in [
        "scores/BlueLeaderAvatar",
        "scores/BlueTeamLabel",
        "scores/ClassImage",
        "scores/LocalPlayerDuelStatsPanel",
    ] {
        assert_eq!(hidden(&l, key), Some(HiddenBy::Game), "{key}");
    }
    assert_eq!(
        hidden(&l, "scores/LocalPlayerStatsPanel/Kills"),
        Some(HiddenBy::Visible)
    );
    assert_eq!(hidden(&l, "scores/MvMScoreboard"), Some(HiddenBy::Visible));
    // BlueLeaderAvatar sits on BlueTeamImage, later in order, but the game hides it.
    assert_eq!(
        l.hit(260, 61).map(PartKey::as_str),
        Some("scores/BlueTeamImage")
    );
    let faded = board_of(&scoreboard_with(
        "BlueTeamImage { ControlName ImagePanel",
        "BlueTeamImage { alpha 0 ControlName ImagePanel",
    ));
    assert_eq!(
        hidden(&faded.layout(HD), "scores/BlueTeamImage"),
        Some(HiddenBy::Alpha)
    );
}

#[test]
fn the_hud_hides_the_scoreboard_by_size_or_alpha_never_by_visible_or_position() {
    assert_eq!(board_of(&stock_files()).hidden_by_hud(), None);
    let hud = HudModel::load(&stock_files(), Platform::Linux).unwrap();
    assert_eq!(hud.hidden_by_hud(), None);
    let root = "    xpos cs-0.5\n    ypos 31\n    wide 640\n    tall 448\n    visible 1\n";
    let cases = [
        (
            "    wide 640\n    tall 448\n",
            "    wide 0\n    tall 0\n",
            Some(HiddenBy::SizeZero),
        ),
        (
            "    visible 1\n",
            "    visible 1\n    alpha 0\n",
            Some(HiddenBy::Alpha),
        ),
        ("    visible 1\n", "    visible 0\n", None),
        ("    visible 1\n", "    visible 1\n    enabled 0\n", None),
        (
            "    xpos cs-0.5\n    ypos 31\n",
            "    xpos 9999\n    ypos 9999\n",
            None,
        ),
        // tf2-recording-hud's root sets every hide at once; alpha is checked first.
        (
            root,
            "    xpos 9999\n    ypos 9999\n    wide 0\n    tall 0\n    visible 0\n    enabled 0\n    alpha 0\n",
            Some(HiddenBy::Alpha),
        ),
    ];
    for (from, to, want) in cases {
        assert_eq!(
            board_of(&scoreboard_with(from, to)).hidden_by_hud(),
            want,
            "{to:?}"
        );
    }
}
