//! Player rows: code-set columns, the name's remaining width and row spacing.

mod common;

use ColumnKind::*;
use common::hud_stock::{scoreboard_with, stock_files};
use tf2tools_core::{
    ColumnKind, HudFiles, HudModel, PartKey, Platform, PlayerListLayout, Rgba, Scene, Screen, Team,
};

const HD: Screen = Screen::new(1920, 1080);
const BLUE: &str = "scores/BluePlayerList";
const WIDTHS: &str = "    medal_column_width 18\n    avatar_width 28\n    spacer 5\n    name_width 118\n    nemesis_width 15\n    class_width 15\n    score_width 25\n    ping_width 15\n    killstreak_width 18\n    killstreak_image_width 15\n";

fn list(files: &HudFiles) -> PlayerListLayout {
    HudModel::load_scene(files, Platform::Linux, Scene::Scoreboard)
        .unwrap()
        .player_list(&PartKey::new(BLUE), HD, 20)
        .unwrap()
}

fn columns(l: &PlayerListLayout) -> Vec<(ColumnKind, i32, i32)> {
    l.columns()
        .iter()
        .map(|c| (c.kind(), c.x(), c.wide()))
        .collect()
}

#[test]
fn fixture_columns_give_the_name_what_is_left() {
    let l = list(&stock_files());
    assert_eq!(l.team(), Team::Blue);
    assert_eq!(
        columns(&l),
        [
            (Medal, 0, 40),
            (Avatar, 40, 63),
            (Spacer, 103, 11),
            (Name, 114, 299),
            (Killstreak, 413, 40),
            (KillstreakImage, 453, 33),
            (Dominating, 486, 33),
            (Nemesis, 519, 33),
            (Score, 552, 56),
            (Class, 608, 33),
            (Ping, 641, 33),
        ]
    );
    assert_eq!((l.row_x(), l.row_wide(), l.header()), (5, 687, (5, 27)));
    let rows = l.rows(12);
    assert_eq!(rows.len(), 12);
    assert_eq!(
        (rows[0], rows[1], rows[11]),
        ((32, 40), (83, 40), (593, 40))
    );
    // The list is 630 px tall: the last row runs past it, and the canvas clips it as the game does.
    assert!(rows[11].0 + rows[11].1 > 630);
    assert_eq!(PlayerListLayout::ROW_BACKGROUND, Rgba::new(0, 0, 0, 80));
    assert_eq!(PlayerListLayout::HEADER_COLOUR, Rgba::WHITE);
    assert_eq!(PlayerListLayout::ROW_FONT, "Default");
}

#[test]
fn name_width_cancels_and_missing_widths_take_the_sdk_defaults() {
    let wider = list(&scoreboard_with(
        "    name_width 118\n",
        "    name_width 500\n",
    ));
    assert_eq!(columns(&wider), columns(&list(&stock_files())));
    assert_eq!(
        columns(&list(&scoreboard_with(WIDTHS, ""))),
        [
            (Medal, 0, 33),
            (Avatar, 33, 40),
            (Spacer, 73, 11),
            (Name, 84, 203),
            (Killstreak, 287, 45),
            (KillstreakImage, 332, 45),
            (Dominating, 377, 45),
            (Nemesis, 422, 45),
            (Score, 467, 78),
            (Class, 545, 78),
            (Ping, 623, 51),
        ]
    );
    let narrow = list(&scoreboard_with("wide 310", "wide 100"));
    let cols = columns(&narrow);
    assert_eq!((cols[3], cols[4]), ((Name, 114, 0), (Killstreak, 114, 40)));
}

#[test]
fn row_spacing_reads_linespacing_and_linegap() {
    let plain = list(&scoreboard_with(
        "linespacing 18 linegap 5 fgcolor blue",
        "fgcolor blue",
    ));
    assert_eq!(plain.rows(2), [(32, 45), (77, 45)]);
    // budhud's lists: linespacing 16, linegap 0.
    let bud = list(&scoreboard_with(
        "linespacing 18 linegap 5 fgcolor blue",
        "linespacing 16 linegap 0 fgcolor blue",
    ));
    assert_eq!(bud.rows(2), [(32, 36), (68, 36)]);
}

#[test]
fn only_the_two_section_lists_have_rows() {
    let m = HudModel::load_scene(&stock_files(), Platform::Linux, Scene::Scoreboard).unwrap();
    let red = m
        .player_list(&PartKey::new("scores/RedPlayerList"), HD, 20)
        .unwrap();
    assert_eq!(red.team(), Team::Red);
    assert!(
        m.player_list(&PartKey::new("scores/BlueTeamScore"), HD, 20)
            .is_none()
    );
    let hud = HudModel::load(&stock_files(), Platform::Linux).unwrap();
    assert!(hud.player_list(&PartKey::new(BLUE), HD, 20).is_none());
    let panel = scoreboard_with(
        "BluePlayerList { ControlName SectionedListPanel",
        "BluePlayerList { ControlName EditablePanel",
    );
    let m = HudModel::load_scene(&panel, Platform::Linux, Scene::Scoreboard).unwrap();
    assert!(m.player_list(&PartKey::new(BLUE), HD, 20).is_none());
}

#[test]
fn huge_spacing_and_widths_saturate_instead_of_panicking() {
    let huge = list(&scoreboard_with(
        "linespacing 18 linegap 5 fgcolor blue",
        "linespacing 2147483647 linegap 2147483647 sectiongap 2147483647 fgcolor blue",
    ));
    let rows = huge.rows(3);
    assert_eq!(rows[0].1, i32::MAX);
    assert_eq!(rows[2].0, i32::MAX);
    let wide = list(&scoreboard_with("wide 310", "wide 2147483647"));
    assert_eq!(wide.row_wide(), i32::MAX - 10);
    let spacer = list(&scoreboard_with("spacer 5", "spacer 2147483647"));
    assert!(columns(&spacer).iter().all(|&(_, x, w)| x >= 0 && w >= 0));
    let m = HudModel::load_scene(&stock_files(), Platform::Linux, Scene::Scoreboard).unwrap();
    let tall_font = m.player_list(&PartKey::new(BLUE), HD, i32::MAX).unwrap();
    assert_eq!(tall_font.header(), (5, i32::MAX));
    assert_eq!(tall_font.rows(1)[0].0, i32::MAX);
}
