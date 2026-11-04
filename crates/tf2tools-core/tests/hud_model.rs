//! The Stage 1 part tree: which blocks are parts, merge order, conditionals and origins.

mod common;

use common::hud_stock::{HEALTH, HUDLAYOUT, stock_files, stock_files_with};
use tf2tools_core::{HudModel, PartId, PartKey, Platform, Rgba};

fn model() -> HudModel {
    HudModel::load(&stock_files(), Platform::Linux).unwrap()
}

fn key(text: &str) -> PartKey {
    PartKey::new(text)
}

#[test]
fn parts_are_code_elements_and_controls_in_table_order() {
    let m = model();
    let keys: Vec<&str> = m.parts().iter().map(|p| p.key().as_str()).collect();
    assert_eq!(
        keys,
        [
            "HudPlayerStatus",
            "HudPlayerStatus/HudPlayerHealth",
            "HudPlayerStatus/HudPlayerHealth/PlayerStatusHealthImage",
            "HudPlayerStatus/HudPlayerHealth/PlayerStatusHealthValue",
            "HudPlayerStatus/HudPlayerClass",
            "HudPlayerStatus/HudPlayerClass/classmodelholder",
            "HudPlayerStatus/HudPlayerClass/classmodelholder/classmodelpanel",
            "HudWeaponAmmo",
            "HudWeaponAmmo/AmmoInClip",
            "HudMedicCharge",
            "HudMedicCharge/ChargeLabel",
            "CHealthAccountPanel",
            "CDamageAccountPanel",
            "CMainTargetID",
            "CMainTargetID/TargetNameLabel",
            "HudItemEffectMeter",
            "HudItemEffectMeter/ItemEffectMeterLabel",
            "HudMatchStatus",
            "HudMatchStatus/ObjectiveStatusTimePanel",
            "HudMatchStatus/ObjectiveStatusTimePanel/TimePanelValue",
            "HudDeathNotice",
            "HudChat",
            "HudChat/ChatInputLine",
            "HudKothTimeStatus",
            "HudKothTimeStatus/BlueTimer",
            "HudKothTimeStatus/BlueTimer/TimePanelValue",
            "HudKothTimeStatus/RedTimer",
            "HudKothTimeStatus/RedTimer/TimePanelValue",
            "HudKothTimeStatus/ActiveTimerBG",
        ]
    );
    let value = m
        .part(&key(
            "HudPlayerStatus/HudPlayerHealth/PlayerStatusHealthValue",
        ))
        .unwrap();
    assert!(!value.is_root() && value.owner() == Some(PartId::Health));
    assert_eq!(value.class(), Some("CExLabel"));
    assert_eq!(
        value.parent().map(PartKey::as_str),
        Some("HudPlayerStatus/HudPlayerHealth")
    );
    assert!(
        m.part(&key("HudPlayerStatus/HudPlayerHealth"))
            .unwrap()
            .is_root()
    );
    assert_eq!(m.part(&key("HudPlayerStatus")).unwrap().owner(), None);
    assert_eq!(m.part(&key("HudWeaponAmmo")).unwrap().class(), None);
    assert_eq!(
        m.scheme().named_colour("TanLight"),
        Some(Rgba::new(235, 226, 202, 255))
    );
    assert_eq!(
        m.document("scripts/hudlayout.res").unwrap().as_bytes(),
        HUDLAYOUT.as_bytes()
    );
}

/// The game applies same-named sibling blocks to one panel.
#[test]
fn duplicate_sibling_names_are_one_part() {
    let twice = HEALTH.replace(
        "  HealthBonusSettings",
        "  playerstatushealthimage { ControlName ImagePanel }\n  HealthBonusSettings",
    );
    let m = HudModel::load(
        &stock_files_with(&[("resource/ui/hudplayerhealth.res", Some(twice.as_str()))]),
        Platform::Linux,
    )
    .unwrap();
    let image = "HudPlayerStatus/HudPlayerHealth/PlayerStatusHealthImage";
    assert!(m.part(&key(&format!("{image}#2"))).is_none());
    let part = m.part(&key(image)).unwrap();
    assert_eq!(part.name(), "PlayerStatusHealthImage");
    assert_eq!(part.value("xpos").map(|l| l.text()), Some("73"));
    assert_eq!(part.blocks().len(), 1);
}

#[test]
fn a_missing_or_unparsable_file_makes_only_its_part_read_only() {
    let deep = format!("\"R\"\n{}{}", "{\n".repeat(65), "}\n".repeat(65));
    let files = stock_files_with(&[
        ("resource/ui/hudammoweapons.res", None),
        ("resource/ui/hudmediccharge.res", Some(deep.as_str())),
    ]);
    let m = HudModel::load(&files, Platform::Linux).unwrap();
    for root in ["HudWeaponAmmo", "HudMedicCharge"] {
        assert!(!m.part(&key(root)).unwrap().is_readable(), "{root}");
    }
    assert!(m.part(&key("HudWeaponAmmo/AmmoInClip")).is_none());
    assert!(
        m.part(&key("HudPlayerStatus/HudPlayerHealth"))
            .unwrap()
            .is_readable()
    );
    assert_eq!(
        m.problems(),
        [
            "resource/ui/hudammoweapons.res",
            "resource/ui/hudmediccharge.res"
        ]
    );
}

#[test]
fn own_file_beats_hudlayout_and_active_if_blocks_beat_their_parent() {
    let m = model();
    let meter = m.part(&key("HudItemEffectMeter")).unwrap();
    assert_eq!(meter.value("xpos").unwrap().text(), "r174");
    assert_eq!(
        meter.value("xpos").unwrap().file(),
        "resource/ui/huditemeffectmeter.res"
    );
    assert_eq!(meter.value("wide").unwrap().file(), "scripts/hudlayout.res");
    let timer = m
        .part(&key("HudMatchStatus/ObjectiveStatusTimePanel"))
        .unwrap();
    assert_eq!(timer.value("XPOS").unwrap().text(), "cs-0.5");
    assert_eq!(
        timer.value("xpos").unwrap().block(),
        [
            "Resource/UI/HudMatchStatus.res",
            "ObjectiveStatusTimePanel",
            "if_match"
        ]
    );
    assert_eq!(timer.value("wide").unwrap().text(), "130");
    assert_eq!(
        m.part(&key("CMainTargetID"))
            .unwrap()
            .value("ypos")
            .unwrap()
            .text(),
        "355",
        "if_vr is inactive"
    );
}

#[test]
fn values_carry_their_origin_and_the_first_accepted_platform_line_wins() {
    let m = model();
    let value = m
        .part(&key(
            "HudPlayerStatus/HudPlayerHealth/PlayerStatusHealthValue",
        ))
        .unwrap();
    let colour = value.value("fgcolor").unwrap();
    assert_eq!(
        (colour.text(), colour.file(), colour.line()),
        ("TanDark", "resource/ui/hudplayerhealth.res", 32)
    );
    assert_eq!(
        colour.block(),
        ["Resource/UI/HudPlayerHealth.res", "PlayerStatusHealthValue"]
    );
    assert_eq!(
        value.own_block().1,
        ["Resource/UI/HudPlayerHealth.res", "PlayerStatusHealthValue"]
    );
    let x = m
        .part(&key("HudPlayerStatus/HudPlayerHealth"))
        .unwrap()
        .value("xpos")
        .unwrap();
    assert_eq!((x.text(), x.cond()), ("0", Some("$WIN32")));
    assert_eq!(
        m.part(&key("HudWeaponAmmo")).unwrap().own_block().0,
        "scripts/hudlayout.res"
    );
}

#[test]
fn lines_cover_every_platform_line_duplicate_and_if_block_of_the_winning_block() {
    let doubled = HEALTH.replace("    zpos 5\n", "    zpos 5\n    zpos 6\n");
    let m = HudModel::load(
        &stock_files_with(&[("resource/ui/hudplayerhealth.res", Some(doubled.as_str()))]),
        Platform::Linux,
    )
    .unwrap();
    let lines = |part: &str, k: &str| -> Vec<(String, Option<String>, usize)> {
        m.part(&key(part))
            .unwrap()
            .lines(k)
            .iter()
            .map(|l| {
                (
                    l.text().to_owned(),
                    l.cond().map(str::to_owned),
                    l.block().len(),
                )
            })
            .collect()
    };
    let s = |t: &str| t.to_owned();
    assert_eq!(
        lines("HudPlayerStatus/HudPlayerHealth", "xpos"),
        [
            (s("0"), Some(s("$WIN32")), 2),
            (s("32"), Some(s("$X360")), 2)
        ]
    );
    assert_eq!(
        lines("HudMatchStatus/ObjectiveStatusTimePanel", "xpos"),
        [(s("c-55"), None, 2), (s("cs-0.5"), None, 3)]
    );
    assert_eq!(
        lines("CMainTargetID", "ypos"),
        [(s("355"), None, 2), (s("290"), None, 3)]
    );
    assert_eq!(lines("HudItemEffectMeter", "xpos"), [(s("r174"), None, 2)]);
    let value = "HudPlayerStatus/HudPlayerHealth/PlayerStatusHealthValue";
    assert_eq!(lines(value, "zpos"), [(s("5"), None, 2), (s("6"), None, 2)]);
    assert_eq!(
        m.part(&key(value)).unwrap().value("zpos").unwrap().text(),
        "5",
        "first duplicate wins"
    );
    assert!(lines(value, "visible").is_empty());
}

#[test]
fn controls_are_read_only_when_their_part_is() {
    let files = stock_files_with(&[("scripts/hudlayout.res", None)]);
    let m = HudModel::load(&files, Platform::Linux).unwrap();
    assert!(!m.part(&key("HudWeaponAmmo")).unwrap().is_readable());
    assert!(
        !m.part(&key("HudWeaponAmmo/AmmoInClip"))
            .unwrap()
            .is_readable()
    );
}

#[test]
fn a_file_with_no_root_block_is_a_problem() {
    let files = stock_files_with(&[("resource/ui/hudmediccharge.res", Some("just text\n"))]);
    let m = HudModel::load(&files, Platform::Linux).unwrap();
    assert!(!m.part(&key("HudMedicCharge")).unwrap().is_readable());
    assert_eq!(m.problems(), ["resource/ui/hudmediccharge.res"]);
}
