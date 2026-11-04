//! The MvM player table: the SDK's columns at default widths and its row colours.

mod common;

use ColumnKind::*;
use common::hud_stock::stock_files;
use tf2tools_core::{
    BoardMode, ColumnKind, HudModel, PartKey, Platform, PlayerListLayout, Rgba, Scene, Screen, Team,
};

const HD: Screen = Screen::new(1920, 1080);
const LIST: &str = "scores/MvMScoreboard/MvMPlayerList";

fn model() -> HudModel {
    HudModel::load_scene(&stock_files(), Platform::Linux, Scene::Scoreboard)
        .unwrap()
        .with_board_mode(BoardMode::Mvm)
}

#[test]
fn the_table_has_the_sdk_columns_at_default_widths() {
    let l = model().player_list(&PartKey::new(LIST), HD, 20).unwrap();
    assert_eq!(l.team(), Team::Red);
    let columns: Vec<_> = l
        .columns()
        .iter()
        .map(|c| (c.kind(), c.x(), c.wide()))
        .collect();
    // 2.25 pixels a unit: 20, 4, 2, 94, 25 and 43 units truncate; the avatar's 32 does not scale.
    assert_eq!(
        columns,
        [
            (Medal, 0, 45),
            (MedalSpacer, 45, 9),
            (Avatar, 54, 32),
            (Spacer, 86, 4),
            (Name, 90, 211),
            (Class, 301, 56),
            (Tour, 357, 56),
            (Score, 413, 96),
            (Damage, 509, 96),
            (Tank, 605, 96),
            (Healing, 701, 96),
            (Support, 797, 96),
            (Credits, 893, 96),
            (SquadSurplus, 989, 96),
            (Ping, 1085, 56),
        ]
    );
}

#[test]
fn rows_follow_the_lists_spacing_and_the_header_its_font() {
    let m = model();
    let l = m.player_list(&PartKey::new(LIST), HD, 20).unwrap();
    // The header is 20 + 7; linespacing 22 is 49 px, no gap.
    assert_eq!(l.header(), (5, 27));
    assert_eq!(l.rows(2), [(32, 49), (81, 49)]);
    let wide = m.layout(HD).get(&PartKey::new(LIST)).unwrap().wide();
    assert_eq!(l.row_wide(), wide - 10);
}

#[test]
fn only_the_mvm_list_is_an_mvm_table_and_casual_has_none() {
    let m = model();
    assert!(
        m.player_list(
            &PartKey::new("scores/MvMScoreboard/PlayerListBackground"),
            HD,
            20
        )
        .is_none()
    );
    assert!(
        m.player_list(&PartKey::new("scores/BluePlayerList"), HD, 20)
            .is_some()
    );
    let casual = m.with_board_mode(BoardMode::Casual);
    assert!(casual.player_list(&PartKey::new(LIST), HD, 20).is_none());
}

#[test]
fn dead_defenders_are_grey_on_a_grey_row() {
    assert_eq!(PlayerListLayout::MVM_ROW_BACKGROUND, Rgba::new(0, 0, 0, 0));
    assert_eq!(
        PlayerListLayout::MVM_DEAD_BACKGROUND,
        Rgba::new(78, 78, 78, 150)
    );
    assert_eq!(
        PlayerListLayout::MVM_DEAD_COLOUR,
        Rgba::new(117, 107, 94, 255)
    );
    assert_eq!(PlayerListLayout::MVM_FONT, "ScoreboardVerySmall");
}
