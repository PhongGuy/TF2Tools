//! Alignment tools: nudges, Match size, Mirror and snapping.

use crate::common;

use common::hud_stock::{HEALTH, label, stock_files, stock_files_with, with_blocks};
use tf2tools_core::{
    Change, ChangeTarget, EditRefused, HudFiles, HudModel, PartKey, Platform, Screen,
};

const HD: Screen = Screen::new(1920, 1080);
const HV: &str = "HudPlayerStatus/HudPlayerHealth/PlayerStatusHealthValue";
const IMAGE: &str = "HudPlayerStatus/HudPlayerHealth/PlayerStatusHealthImage";
const PIN_A: &str = "HudPlayerStatus/HudPlayerHealth/PinA";

fn model_of(files: &HudFiles) -> HudModel {
    HudModel::load(files, Platform::Linux).unwrap()
}

fn stock() -> HudModel {
    model_of(&stock_files())
}

fn k(text: &str) -> PartKey {
    PartKey::new(text)
}

fn health_with(text: &str) -> HudModel {
    model_of(&stock_files_with(&[(
        "resource/ui/hudplayerhealth.res",
        Some(text),
    )]))
}

/// The stock HUD with PinA pinned to the health value.
fn pinned() -> HudModel {
    let pin = label(
        "PinA",
        "4",
        "-2",
        &[("pin_to_sibling", "PlayerStatusHealthValue")],
    );
    health_with(&with_blocks(HEALTH, &pin))
}

/// The stock HUD with the health image at width 0, so it is hidden.
fn flat_image() -> HudModel {
    health_with(&HEALTH.replacen("wide 51", "wide 0", 1))
}

fn applied(m: &HudModel, changes: &[Change]) -> HudModel {
    m.with_changes(changes).into_model()
}

fn rect(m: &HudModel, key: &str) -> (i32, i32, i32, i32) {
    let b = m.layout(HD).get(&k(key)).unwrap().clone();
    (b.x(), b.y(), b.wide(), b.tall())
}

/// (key, cond, old, new, block depth) of one Set.
type Row = (String, Option<String>, Option<String>, String, usize);

/// The rows of each Set, checking the group.
fn sets(changes: &[Change], group: u32) -> Vec<Row> {
    changes
        .iter()
        .map(|c| {
            assert_eq!(c.group(), group, "every change should be in group {group}");
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
fn a_nudge_moves_whole_units_on_every_line() {
    let m = stock();
    assert_eq!(
        sets(&m.nudge_part(&k(HV), 1, -1, 3).unwrap(), 3),
        [
            row("xpos", None, Some("76"), "77", 2),
            row("ypos", None, Some("52"), "51", 2)
        ]
    );
    assert_eq!(
        sets(&m.nudge_part(&k("HudWeaponAmmo"), -1, 0, 4).unwrap(), 4),
        [
            row("xpos", Some("$WIN32"), Some("r95"), "r96", 2),
            row("xpos", Some("$X360"), Some("r131"), "r132", 2)
        ]
    );
    let moved = m.nudge_part(&k(HV), 10, 0, 5).unwrap();
    assert_eq!(rect(&applied(&m, &moved), HV).0, 193, "86 units at 1080p");
}

#[test]
fn a_nudge_skips_code_owned_axes_and_refuses_what_it_cannot_move() {
    let m = stock();
    assert_eq!(
        sets(&m.nudge_part(&k("CMainTargetID"), 1, 1, 2).unwrap(), 2),
        [
            row("ypos", None, Some("355"), "356", 2),
            row("ypos", None, Some("290"), "291", 3)
        ]
    );
    assert_eq!(
        m.nudge_part(&k("CDamageAccountPanel"), 1, 0, 1),
        Err(EditRefused::NotOffered)
    );
    assert_eq!(
        m.nudge_part(&k("Nope"), 1, 0, 1),
        Err(EditRefused::UnknownPart)
    );
    assert_eq!(m.nudge_part(&k(HV), 0, 0, 1), Ok(Vec::new()));
}

#[test]
fn a_pinned_part_nudges_by_the_pin_sign() {
    assert_eq!(
        sets(&pinned().nudge_part(&k(PIN_A), 1, 0, 5).unwrap(), 5),
        [row("xpos", None, Some("4"), "5", 2)]
    );
}

#[test]
fn match_size_copies_the_other_parts_size_as_plain_units() {
    let m = stock();
    let matched = m.match_size(&k(HV), &k(IMAGE), HD, 6).unwrap();
    assert_eq!(
        sets(&matched, 6),
        [
            row("wide", None, Some("44"), "51", 2),
            row("tall", None, Some("18"), "51", 2)
        ]
    );
    assert_eq!(rect(&applied(&m, &matched), HV), (171, 927, 114, 114));
    let full = m
        .match_size(&k(HV), &k("HudPlayerStatus/HudPlayerClass"), HD, 7)
        .unwrap();
    assert_eq!(
        sets(&full, 7),
        [
            row("wide", None, Some("44"), "854", 2),
            row("tall", None, Some("18"), "480", 2)
        ],
        "f0 becomes the fewest units that reach 1920 pixels"
    );
}

#[test]
fn match_size_skips_equal_and_code_owned_axes() {
    let m = stock();
    assert_eq!(
        sets(
            &m.match_size(
                &k("HudMedicCharge/ChargeLabel"),
                &k("HudMedicCharge"),
                HD,
                1
            )
            .unwrap(),
            1
        ),
        [row("tall", None, Some("20"), "40", 2)]
    );
    assert_eq!(
        sets(
            &m.match_size(&k("CMainTargetID"), &k(HV), HD, 2).unwrap(),
            2
        ),
        [row("tall", None, Some("35"), "18", 2)]
    );
    assert_eq!(m.match_size(&k(HV), &k(HV), HD, 3), Ok(Vec::new()));
}

#[test]
fn match_size_refuses_an_unknown_or_flat_source() {
    assert_eq!(
        stock().match_size(&k(HV), &k("Nope"), HD, 1),
        Err(EditRefused::UnknownPart)
    );
    assert_eq!(
        stock().match_size(&k("Nope"), &k(HV), HD, 1),
        Err(EditRefused::UnknownPart)
    );
    assert_eq!(
        flat_image().match_size(&k(HV), &k(IMAGE), HD, 1),
        Err(EditRefused::BadValue)
    );
}

use common::hud_stock::{block, stock as stock_texts};
use tf2tools_core::NoMirror;

const CLASS_FILE: &str = "resource/ui/hudplayerclass.res";
const MEDIC: &str = "HudMedicCharge";
const HEALTH_PANEL: &str = "HudPlayerStatus/HudPlayerHealth";
const WIDE: &str = "HudPlayerStatus/HudPlayerHealth/Wide";
const HALF: &str = "HudPlayerStatus/HudPlayerHealth/Half";
const FAR: &str = "HudPlayerStatus/HudPlayerHealth/Far";
const FRAC: &str = "HudPlayerStatus/HudPlayerHealth/Frac";
const SCREEN_WIDE: &str = "HudPlayerStatus/HudPlayerClass/ScreenWide";
const SCREEN_HALF: &str = "HudPlayerStatus/HudPlayerClass/ScreenHalf";

fn label_at(name: &str, xpos: &str, wide: &str, to_parent: bool) -> String {
    let mut keys = vec![
        ("xpos", xpos),
        ("ypos", "0"),
        ("wide", wide),
        ("tall", "10"),
    ];
    if to_parent {
        keys.push(("proportionaltoparent", "1"));
    }
    block(name, "CExLabel", &keys)
}

/// The stock HUD plus labels. In the 250-unit health panel, with `proportionaltoparent`:
/// `Wide` (`f200` at `xpos 5`), `Half` (`p0.5`), `Far` (`f200` at `r10`) and `Frac` (a `p0.1`
/// position). In the full-screen class panel, without it: `ScreenWide` (`f200` at `xpos 5`)
/// and `ScreenHalf` (`p0.5`).
fn widths() -> HudModel {
    let health = [
        label_at("Wide", "5", "f200", true),
        label_at("Half", "10", "p0.5", true),
        label_at("Far", "r10", "f200", true),
        label("Frac", "p0.1", "0", &[("proportionaltoparent", "1")]),
    ]
    .concat();
    let class = [
        label_at("ScreenWide", "5", "f200", false),
        label_at("ScreenHalf", "10", "p0.5", false),
    ]
    .concat();
    let class_text = stock_texts()
        .into_iter()
        .find(|(p, _)| p == CLASS_FILE)
        .unwrap()
        .1;
    model_of(&stock_files_with(&[
        (
            "resource/ui/hudplayerhealth.res",
            Some(with_blocks(HEALTH, &health).as_str()),
        ),
        (CLASS_FILE, Some(with_blocks(&class_text, &class).as_str())),
    ]))
}

#[test]
fn mirror_writes_the_opposite_anchor_against_the_screen() {
    let m = stock();
    let medic = m.mirror_part(&k(MEDIC), 8).unwrap();
    assert_eq!(
        sets(&medic, 8),
        [row("xpos", None, Some("c-38"), "c-62", 2)]
    );
    assert_eq!(rect(&applied(&m, &medic), MEDIC), (821, 832, 225, 90));
    assert_eq!(
        sets(&m.mirror_part(&k("HudWeaponAmmo"), 1).unwrap(), 1),
        [
            row("xpos", Some("$WIN32"), Some("r95"), "1", 2),
            row("xpos", Some("$X360"), Some("r131"), "37", 2)
        ]
    );
    assert_eq!(
        sets(&m.mirror_part(&k("HudItemEffectMeter"), 1).unwrap(), 1),
        [row("xpos", None, Some("r174"), "74", 2)],
        "the width comes from hudlayout when the position's block has none"
    );
    let panel = m.mirror_part(&k(HEALTH_PANEL), 1).unwrap();
    assert_eq!(
        sets(&panel, 1),
        [
            row("xpos", Some("$WIN32"), Some("0"), "r250", 2),
            row("xpos", Some("$X360"), Some("32"), "r282", 2)
        ],
        "a child of a full-screen panel at x 0 mirrors across the screen"
    );
    assert_eq!(
        rect(&applied(&m, &panel), HEALTH_PANEL),
        (1358, 810, 562, 270)
    );
    let w = widths();
    let screen_wide = w.mirror_part(&k(SCREEN_WIDE), 1).unwrap();
    assert_eq!(
        sets(&screen_wide, 1),
        [row("xpos", None, Some("5"), "195", 2)]
    );
    assert_eq!(
        rect(&applied(&w, &screen_wide), SCREEN_WIDE),
        (438, 0, 1470, 22)
    );
}

#[test]
fn proportionaltoparent_mirrors_inside_the_parent() {
    let w = widths();
    let wide = w.mirror_part(&k(WIDE), 1).unwrap();
    assert_eq!(sets(&wide, 1), [row("xpos", None, Some("5"), "195", 2)]);
    assert_eq!(rect(&w, WIDE), (11, 810, 112, 22));
    assert_eq!(
        rect(&applied(&w, &wide), WIDE),
        (438, 810, 112, 22),
        "562 - 438 - 112 = 12"
    );
}

#[test]
fn centred_and_full_width_parts_mirror_onto_themselves() {
    let m = stock();
    assert_eq!(
        m.mirror_part(&k("HudMatchStatus/ObjectiveStatusTimePanel"), 1),
        Ok(Vec::new())
    );
    assert_eq!(
        m.mirror_part(&k("HudPlayerStatus/HudPlayerClass"), 1),
        Ok(Vec::new())
    );
}

#[test]
fn mirror_holds_on_every_screen_shape() {
    // (part, measured inside its parent): plain widths under each anchor, f widths both ways.
    let cases = [
        ("HudWeaponAmmo", false),
        (MEDIC, false),
        ("HudItemEffectMeter", false),
        (HEALTH_PANEL, false),
        (SCREEN_WIDE, false),
        (WIDE, true),
    ];
    let m = widths();
    for (key, inside) in cases {
        let mirrored = applied(&m, &m.mirror_part(&k(key), 1).unwrap());
        for screen in Screen::PRESETS {
            let (before, after) = (m.layout(screen), mirrored.layout(screen));
            let (px, pw) = match inside {
                true => {
                    let parent = m.part(&k(key)).unwrap().parent().unwrap();
                    let b = before.get(parent).unwrap();
                    (b.x(), b.wide())
                }
                false => (0, screen.wide() as i32),
            };
            let (b, a) = (before.get(&k(key)).unwrap(), after.get(&k(key)).unwrap());
            let want = px + pw - (b.x() - px) - b.wide();
            assert!(
                (a.x() - want).abs() <= 1,
                "{key} at {screen:?}: {} vs {want}",
                a.x()
            );
            assert_eq!(a.wide(), b.wide(), "{key} at {screen:?}");
        }
    }
}

#[test]
fn mirroring_twice_puts_the_text_back() {
    let m = stock();
    let mid = applied(&m, &m.mirror_part(&k(MEDIC), 1).unwrap());
    assert_eq!(
        sets(&mid.mirror_part(&k(MEDIC), 2).unwrap(), 2),
        [row("xpos", None, Some("c-62"), "c-38", 2)]
    );
}

#[test]
fn mirror_refuses_what_it_cannot_keep_on_every_screen() {
    let m = widths();
    for key in [HALF, FAR, SCREEN_HALF] {
        assert_eq!(
            m.mirror_part(&k(key), 1),
            Err(EditRefused::NotOffered),
            "{key}"
        );
        assert_eq!(m.why_not_mirrored(&k(key)), Some(NoMirror::Width), "{key}");
    }
    // HV and TimePanelValue measure against the screen inside a narrower parent; Frac has a
    // `p` position; the target ID's place is the game's.
    let value = "HudMatchStatus/ObjectiveStatusTimePanel/TimePanelValue";
    for key in [HV, value, FRAC, "CMainTargetID"] {
        assert_eq!(
            m.mirror_part(&k(key), 1),
            Err(EditRefused::NotOffered),
            "{key}"
        );
        assert_eq!(m.why_not_mirrored(&k(key)), Some(NoMirror::Place), "{key}");
    }
    assert_eq!(pinned().why_not_mirrored(&k(PIN_A)), Some(NoMirror::Place));
    assert_eq!(m.mirror_part(&k("Nope"), 1), Err(EditRefused::UnknownPart));
    assert_eq!(
        (
            m.why_not_mirrored(&k(WIDE)),
            m.why_not_mirrored(&k(SCREEN_WIDE))
        ),
        (None, None)
    );
    let offered = |m: &HudModel, key: &str| m.capabilities(&k(key)).unwrap().mirror();
    assert!(offered(&m, MEDIC) && offered(&m, WIDE) && offered(&m, SCREEN_WIDE));
    assert!(!offered(&m, HV) && !offered(&m, HALF) && !offered(&m, FRAC));
    assert!(!offered(&m, "CMainTargetID") && !offered(&pinned(), PIN_A));
}

use tf2tools_core::GuideAxis;

fn snap(
    m: &HudModel,
    key: &str,
    dx: i32,
    dy: i32,
    threshold: i32,
) -> (i32, i32, Vec<(GuideAxis, i32)>) {
    let s = m.layout(HD).snap_move(&k(key), dx, dy, threshold);
    (
        s.dx(),
        s.dy(),
        s.guides().iter().map(|g| (g.axis(), g.at())).collect(),
    )
}

#[test]
fn a_move_snaps_to_the_nearest_edge_or_centre_within_the_threshold() {
    // HV moved 45 px right: its left edge (216) is 5 px from the health image's centre (221)
    // and its bottom (967) 1 px from the ammo clip's top (968).
    assert_eq!(
        snap(&stock(), HV, 45, 0, 6),
        (
            50,
            1,
            vec![(GuideAxis::Vertical, 221), (GuideAxis::Horizontal, 968)]
        )
    );
    assert_eq!(
        snap(&stock(), HV, 45, 0, 4),
        (45, 1, vec![(GuideAxis::Horizontal, 968)])
    );
    assert_eq!(snap(&stock(), HV, 45, 0, 0), (45, 0, vec![]));
}

#[test]
fn the_screen_centre_is_a_target() {
    // The medic meter's centre lands at 962 after the move, 2 px from the screen's 960.
    let (dx, _, guides) = snap(&stock(), "HudMedicCharge", -25, 0, 6);
    assert_eq!(dx, -27);
    assert!(guides.contains(&(GuideAxis::Vertical, 960)), "{guides:?}");
}

#[test]
fn the_parts_own_children_and_hidden_parts_are_not_targets() {
    // Moved 3 px left, the health panel's centre (278) would meet its own image's right edge;
    // its left edge meets HudPlayerStatus's left edge at x 0 instead (the screen itself offers
    // only its centre).
    let (dx, _, guides) = snap(&stock(), "HudPlayerStatus/HudPlayerHealth", -3, 0, 6);
    assert_eq!((dx, guides.first()), (0, Some(&(GuideAxis::Vertical, 0))));
    // With the image hidden nothing is within 6 px of HV's moved left, centre or right.
    let (dx, _, guides) = snap(&flat_image(), HV, 45, 0, 6);
    assert_eq!(dx, 45);
    assert!(
        guides.iter().all(|g| g.0 != GuideAxis::Vertical),
        "{guides:?}"
    );
}

#[test]
fn an_unknown_part_does_not_snap() {
    assert_eq!(snap(&stock(), "Nope", 45, 7, 6), (45, 7, vec![]));
}

/// The health panel plus, with `proportionaltoparent`: `NoWide` (an `rs1-10` position and no
/// `wide`), `Huge` (a position at the edge of i32) and `Bad` (a malformed `c1e30` position).
fn edge_cases() -> HudModel {
    let to_parent = ("proportionaltoparent", "1");
    let blocks = [
        block(
            "NoWide",
            "CExLabel",
            &[("xpos", "rs1-10"), ("ypos", "0"), ("tall", "10"), to_parent],
        ),
        label_at("Huge", "2147483647", "44", true),
        label("Bad", "c1e30", "0", &[to_parent]),
    ]
    .concat();
    health_with(&with_blocks(HEALTH, &blocks))
}

#[test]
fn mirror_takes_an_own_size_position_without_a_width() {
    let m = edge_cases();
    let key = "HudPlayerStatus/HudPlayerHealth/NoWide";
    assert_eq!(m.why_not_mirrored(&k(key)), None);
    assert_eq!(
        sets(&m.mirror_part(&k(key), 1).unwrap(), 1),
        [row("xpos", None, Some("rs1-10"), "10", 2)]
    );
}

#[test]
fn mirror_refuses_a_place_it_cannot_write_or_read() {
    let m = edge_cases();
    let huge = "HudPlayerStatus/HudPlayerHealth/Huge";
    assert_eq!(m.mirror_part(&k(huge), 1), Err(EditRefused::NotOffered));
    assert_eq!(m.why_not_mirrored(&k(huge)), Some(NoMirror::Place));
    let bad = "HudPlayerStatus/HudPlayerHealth/Bad";
    assert_eq!(m.mirror_part(&k(bad), 1), Err(EditRefused::ReadOnly));
    assert_eq!(m.why_not_mirrored(&k(bad)), Some(NoMirror::Place));
}

/// The stock HUD plus two labels in the 562 px health panel: `Out` (540..585) sticks out past
/// its right edge and `Gone` (585..630) lies wholly outside it, so the game shows none of it.
fn clipped() -> HudModel {
    let blocks = [
        label("Out", "240", "0", &[]),
        label("Gone", "260", "0", &[]),
    ]
    .concat();
    health_with(&with_blocks(HEALTH, &blocks))
}

#[test]
fn guides_follow_the_edges_the_game_shows() {
    let m = clipped();
    // HV's right edge (270) moved 313 px lands 2 px short of Out's cut-off right edge at 585.
    let (dx, _, guides) = snap(&m, HV, 313, 0, 3);
    assert_eq!(dx, 313, "{guides:?}");
    // Moved 358 px it lands 2 px short of Gone's right edge at 630, which the game never draws.
    let (dx, _, guides) = snap(&m, HV, 358, 0, 3);
    assert_eq!(dx, 358, "{guides:?}");
    // Out's visible right edge is the panel's own at 562: still a target.
    let (dx, _, guides) = snap(&m, HV, 290, 0, 3);
    assert_eq!(
        (dx, guides.first()),
        (292, Some(&(GuideAxis::Vertical, 562)))
    );
}
