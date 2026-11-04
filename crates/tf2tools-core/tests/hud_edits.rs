//! Player actions as changes: forms, every line of a key, code-owned keys and refusals.

mod common;

use common::hud_stock::{HEALTH, label, stock_files, stock_files_with, with_blocks};
use tf2tools_core::{
    Change, ChangeTarget, EditRefused, HudFiles, HudModel, PartKey, Platform, Screen,
};

const HD: Screen = Screen::new(1920, 1080);
const HV: &str = "HudPlayerStatus/HudPlayerHealth/PlayerStatusHealthValue";

fn model_of(files: &HudFiles) -> HudModel {
    HudModel::load(files, Platform::Linux).unwrap()
}

fn k(text: &str) -> PartKey {
    PartKey::new(text)
}

/// (key, cond, old, new, block depth) of one Set.
type Row = (String, Option<String>, Option<String>, String, usize);

/// The rows of each Set, checking the group.
fn sets(changes: &[Change], group: u32) -> Vec<Row> {
    changes
        .iter()
        .map(|c| {
            assert_eq!(c.group(), group);
            let ChangeTarget::Set(s) = c.target() else {
                panic!("not a set")
            };
            (
                s.key().to_owned(),
                s.cond().map(str::to_owned),
                s.old().map(str::to_owned),
                s.value().to_owned(),
                s.block().len(),
            )
        })
        .collect()
}

fn row(key: &str, cond: Option<&str>, old: Option<&str>, new: &str, depth: usize) -> Row {
    (
        key.to_owned(),
        cond.map(str::to_owned),
        old.map(str::to_owned),
        new.to_owned(),
        depth,
    )
}

#[test]
fn a_move_writes_every_line_in_whole_units_and_lands_where_dragged() {
    let m = model_of(&stock_files());
    let moved = m.move_part(&k(HV), 45, -45, HD, 3).unwrap();
    assert_eq!(
        sets(&moved, 3),
        [
            row("xpos", None, Some("76"), "96", 2),
            row("ypos", None, Some("52"), "32", 2)
        ]
    );
    let after = m.with_changes(&moved).into_model().layout(HD);
    assert_eq!(
        (
            after.get(&k(HV)).unwrap().x(),
            after.get(&k(HV)).unwrap().y()
        ),
        (216, 882)
    );
    let health = m
        .move_part(&k("HudPlayerStatus/HudPlayerHealth"), 45, 0, HD, 1)
        .unwrap();
    assert_eq!(
        sets(&health, 1),
        [
            row("xpos", Some("$WIN32"), Some("0"), "20", 2),
            row("xpos", Some("$X360"), Some("32"), "52", 2)
        ]
    );
    let timer = m
        .move_part(&k("HudMatchStatus/ObjectiveStatusTimePanel"), 45, 0, HD, 1)
        .unwrap();
    assert_eq!(
        sets(&timer, 1),
        [
            row("xpos", None, Some("c-55"), "c-35", 2),
            row("xpos", None, Some("cs-0.5"), "cs-0.5+20", 3)
        ]
    );
    assert!(
        m.move_part(&k(HV), 1, 0, HD, 1).unwrap().is_empty(),
        "under one unit"
    );
}

#[test]
fn code_owned_axes_are_skipped_or_refused() {
    let m = model_of(&stock_files());
    let target = m.move_part(&k("CMainTargetID"), 45, 45, HD, 2).unwrap();
    assert_eq!(
        sets(&target, 2),
        [
            row("ypos", None, Some("355"), "375", 2),
            row("ypos", None, Some("290"), "310", 3)
        ]
    );
    let resized = m
        .resize_part(&k("CMainTargetID"), Some(600), Some(90), HD, 2)
        .unwrap();
    assert_eq!(sets(&resized, 2), [row("tall", None, Some("35"), "40", 2)]);
    assert_eq!(
        m.move_part(&k("CDamageAccountPanel"), 45, 0, HD, 1),
        Err(EditRefused::NotOffered)
    );
    assert_eq!(
        m.hide_part(&k("CDamageAccountPanel"), 1),
        Err(EditRefused::NotOffered)
    );
    assert_eq!(
        m.move_part(&k("Nope"), 45, 0, HD, 1),
        Err(EditRefused::UnknownPart)
    );
    let caps = m.capabilities(&k("CMainTargetID")).unwrap();
    assert!(
        !caps.move_x() && caps.move_y() && !caps.resize_wide() && caps.resize_tall() && caps.hide()
    );
    let value = m.capabilities(&k(HV)).unwrap();
    assert!(value.move_x() && value.resize_wide() && value.hide() && value.recolour());
}

#[test]
fn malformed_geometry_is_read_only() {
    let text = HEALTH.replace("xpos 76", "xpos 7w");
    let m = model_of(&stock_files_with(&[(
        "resource/ui/hudplayerhealth.res",
        Some(text.as_str()),
    )]));
    assert_eq!(
        m.move_part(&k(HV), 45, 0, HD, 1),
        Err(EditRefused::ReadOnly)
    );
    assert!(!m.capabilities(&k(HV)).unwrap().move_x());
}

#[test]
fn resizes_and_pinned_moves() {
    let m = model_of(&stock_files());
    assert_eq!(
        sets(&m.resize_part(&k(HV), Some(180), None, HD, 4).unwrap(), 4),
        [row("wide", None, Some("44"), "80", 2)]
    );
    let class = m
        .resize_part(
            &k("HudPlayerStatus/HudPlayerClass"),
            Some(1800),
            None,
            HD,
            4,
        )
        .unwrap();
    assert_eq!(sets(&class, 4), [row("wide", None, Some("f0"), "f53", 2)]);
    let pinned = with_blocks(
        HEALTH,
        &label(
            "PinA",
            "4",
            "-2",
            &[("pin_to_sibling", "PlayerStatusHealthValue")],
        ),
    );
    let m = model_of(&stock_files_with(&[(
        "resource/ui/hudplayerhealth.res",
        Some(pinned.as_str()),
    )]));
    let moved = m
        .move_part(&k("HudPlayerStatus/HudPlayerHealth/PinA"), 45, 0, HD, 5)
        .unwrap();
    assert_eq!(
        sets(&moved, 5),
        [row("xpos", None, Some("4"), "24", 2)],
        "sign +1 pending probe"
    );
}

#[test]
fn hide_layer_and_recolour() {
    let m = model_of(&stock_files());
    let hide = |p: &str| sets(&m.hide_part(&k(p), 6).unwrap(), 6);
    assert_eq!(
        hide("HudPlayerStatus/HudPlayerHealth"),
        [row("visible", None, Some("1"), "0", 2)]
    );
    assert_eq!(
        hide("CMainTargetID"),
        [
            row("ypos", None, Some("355"), "9999", 2),
            row("ypos", None, Some("290"), "9999", 3)
        ]
    );
    assert_eq!(
        hide(HV),
        [
            row("wide", None, Some("44"), "0", 2),
            row("tall", None, Some("18"), "0", 2)
        ]
    );
    assert_eq!(
        hide("HudPlayerStatus"),
        [row("xpos", None, Some("0"), "9999", 2)]
    );
    assert_eq!(
        sets(&m.set_layer(&k(HV), 7, 6).unwrap(), 6),
        [row("zpos", None, Some("5"), "7", 2)]
    );
    let red = m.recolour_part(&k(HV), "255 80 80 255", 6).unwrap();
    assert_eq!(
        sets(&red, 6),
        [row("fgcolor", None, Some("TanDark"), "255 80 80 255", 2)]
    );
    let image = m
        .recolour_part(
            &k("HudPlayerStatus/HudPlayerHealth/PlayerStatusHealthImage"),
            "TanLight",
            6,
        )
        .unwrap();
    assert_eq!(
        sets(&image, 6),
        [row("drawcolor", None, None, "TanLight", 2)]
    );
    assert_eq!(
        m.recolour_part(&k("HudPlayerStatus"), "TanLight", 6),
        Err(EditRefused::NotOffered)
    );
    for bad in [
        "a \"b\"",
        "NoSuchColour",
        "256 0 0 255",
        "1 2 3",
        "1 2 3 4 5",
        "-1 0 0 255",
    ] {
        assert_eq!(
            m.recolour_part(&k(HV), bad, 6),
            Err(EditRefused::BadValue),
            "{bad}"
        );
    }
}
