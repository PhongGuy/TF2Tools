//! Player-list spacing: the three Stage 1 rows and the number edit behind them.

use crate::common;

use common::hud_stock::{scoreboard_with, stock_files};
use tf2tools_core::{
    Change, ChangeTarget, ClassSchema, EditRefused, HudFiles, HudModel, PartKey, Platform,
    PropertyKind, Scene,
};

fn board_of(files: &HudFiles) -> HudModel {
    HudModel::load_scene(files, Platform::Linux, Scene::Scoreboard).unwrap()
}

fn k(text: &str) -> PartKey {
    PartKey::new(text)
}

/// (block name, key, old, new) of each Set, checking the group.
fn sets(changes: &[Change], group: u32) -> Vec<(String, String, Option<String>, String)> {
    changes
        .iter()
        .map(|c| {
            assert_eq!(c.group(), group);
            let ChangeTarget::Set(s) = c.target() else {
                panic!("not a set")
            };
            let block = s.block().last().cloned().unwrap_or_default();
            (
                block,
                s.key().to_owned(),
                s.old().map(str::to_owned),
                s.value().to_owned(),
            )
        })
        .collect()
}

#[test]
fn player_lists_offer_three_spacing_rows_in_stage_1() {
    let m = board_of(&stock_files());
    let schema = ClassSchema::for_part(m.part(&k("scores/BluePlayerList")).unwrap());
    assert_eq!(schema.class(), "SectionedListPanel");
    let rows: Vec<(&str, &str, PropertyKind)> = schema
        .up_to(1)
        .map(|p| (p.key(), p.label(), p.kind()))
        .collect();
    assert_eq!(
        rows,
        [
            ("xpos", "Position", PropertyKind::Position),
            ("wide", "Size", PropertyKind::Size),
            ("zpos", "Layer", PropertyKind::Layer),
            ("visible", "Hide", PropertyKind::Hide),
            ("linespacing", "Row height", PropertyKind::Number),
            ("linegap", "Row gap", PropertyKind::Number),
            ("sectiongap", "Section gap", PropertyKind::Number),
        ]
    );
    assert!(schema.get("fgcolor").is_none());
}

#[test]
fn spacing_sets_the_line_or_adds_one() {
    let m = board_of(&stock_files());
    let list = k("scores/BluePlayerList");
    let row = |block: &str, key: &str, old: Option<&str>, new: &str| {
        (
            block.to_owned(),
            key.to_owned(),
            old.map(str::to_owned),
            new.to_owned(),
        )
    };
    assert_eq!(
        sets(&m.set_number(&list, "linespacing", 22, 5).unwrap(), 5),
        [row("BluePlayerList", "linespacing", Some("18"), "22")]
    );
    assert_eq!(
        sets(&m.set_number(&list, "SectionGap", 8, 5).unwrap(), 5),
        [row("BluePlayerList", "sectiongap", None, "8")]
    );
    assert_eq!(
        sets(&m.set_number(&list, "linegap", 0, 6).unwrap(), 6),
        [row("BluePlayerList", "linegap", Some("5"), "0")]
    );
}

#[test]
fn spacing_refusals() {
    let m = board_of(&stock_files());
    let list = k("scores/BluePlayerList");
    assert_eq!(
        m.set_number(&list, "linegap", -1, 1),
        Err(EditRefused::BadValue)
    );
    // Four screen heights (1920 units) is the most a spacing key may be set to.
    assert!(m.set_number(&list, "linespacing", 1920, 1).is_ok());
    for too_big in [1921, i32::MAX] {
        assert_eq!(
            m.set_number(&list, "sectiongap", too_big, 1),
            Err(EditRefused::BadValue)
        );
    }
    assert_eq!(
        m.set_number(&list, "zpos", 3, 1),
        Err(EditRefused::NotOffered)
    );
    assert_eq!(
        m.set_number(&k("scores/BlueTeamScore"), "linespacing", 20, 1),
        Err(EditRefused::NotOffered)
    );
    assert_eq!(
        m.set_number(&k("scores/Nope"), "linespacing", 20, 1),
        Err(EditRefused::UnknownPart)
    );
    let locked = board_of(&scoreboard_with(
        "    wide 640\n    tall 448\n",
        "    wide 0\n    tall 0\n",
    ));
    assert_eq!(
        locked.set_number(&list, "linespacing", 20, 1),
        Err(EditRefused::ReadOnly)
    );
}
