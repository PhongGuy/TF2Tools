//! Stage 1 rules: hide writes, pin corners, font scaling, and each part's panel rule.

use tf2tools_core::{HideTechnique, PanelRule, PartId, PinCorner, font_tall_px};

#[test]
fn hide_techniques_write_these_keys() {
    assert_eq!(HideTechnique::VisibleZero.writes(), [("visible", "0")]);
    assert_eq!(HideTechnique::OffScreenY.writes(), [("ypos", "9999")]);
    assert_eq!(HideTechnique::OffScreenX.writes(), [("xpos", "9999")]);
    assert_eq!(
        HideTechnique::SizeZero.writes(),
        [("wide", "0"), ("tall", "0")]
    );
    assert!(HideTechnique::NotOffered.writes().is_empty());
}

#[test]
fn pin_corners_parse_like_the_engine() {
    use PinCorner::*;
    let cases = [
        (None, TopLeft),
        (Some("PIN_TOPRIGHT"), TopRight),
        (Some("pin_bottomleft"), BottomLeft),
        (Some("3"), BottomRight),
        (Some("6"), CenterBottom),
        (Some("7"), CenterLeft),
        (Some("PIN_CENTER_TOP"), CenterTop),
        (Some("10"), TopLeft),
        (Some("8"), TopLeft),
        (Some("x"), TopLeft),
    ];
    for (text, want) in cases {
        assert_eq!(PinCorner::parse(text), want, "{text:?}");
    }
    let points: Vec<(i32, i32)> = PinCorner::ALL.iter().map(|c| c.point(99, 40)).collect();
    assert_eq!(
        points,
        [
            (0, 0),
            (99, 0),
            (0, 40),
            (99, 40),
            (49, 0),
            (99, 20),
            (49, 40),
            (0, 20)
        ]
    );
}

#[test]
fn font_tall_scales_by_screen_height_pending_probe() {
    for (tall, screen, want) in [(20, 1080, 45), (24, 1200, 60), (18, 1024, 38)] {
        assert_eq!(font_tall_px(tall, screen), want, "{tall} at {screen}");
    }
}

#[test]
fn every_stage1_part_has_one_rule_in_table_order() {
    let ids: Vec<PartId> = PanelRule::all().iter().map(|r| r.id()).collect();
    use PartId::*;
    assert_eq!(
        ids,
        [
            Health,
            ClassPortrait,
            Ammo,
            UberMeter,
            HealNumbers,
            DamageNumbers,
            TargetId,
            ItemMeter,
            RoundTimer,
            Killfeed,
            Chat,
            KothTimer
        ]
    );
    for rule in PanelRule::all() {
        assert_eq!(PanelRule::get(rule.id()).panel(), rule.panel());
        assert!(!rule.label().is_empty() && rule.shows_when().starts_with("Shows "));
    }
    assert_eq!(
        PanelRule::get(UberMeter).shows_when(),
        "Shows when you hold a Medi Gun"
    );
}

#[test]
fn rules_name_the_survey_files_and_blocks() {
    let ui = |f: &str| format!("resource/ui/{f}");
    // (part, panel, parent, geometry blocks, children file)
    type Row<'a> = (
        PartId,
        &'a str,
        Option<&'a str>,
        Vec<(&'a str, &'a str)>,
        String,
    );
    let hudlayout = |b: &'static str| vec![("scripts/hudlayout.res", b)];
    let table: [Row; 10] = [
        (
            PartId::Health,
            "HudPlayerHealth",
            Some("HudPlayerStatus"),
            vec![],
            ui("hudplayerhealth.res"),
        ),
        (
            PartId::Ammo,
            "HudWeaponAmmo",
            None,
            vec![("scripts/hudlayout.res", "HudWeaponAmmo")],
            ui("hudammoweapons.res"),
        ),
        (
            PartId::ClassPortrait,
            "HudPlayerClass",
            Some("HudPlayerStatus"),
            vec![],
            ui("hudplayerclass.res"),
        ),
        (
            PartId::UberMeter,
            "HudMedicCharge",
            None,
            hudlayout("HudMedicCharge"),
            ui("hudmediccharge.res"),
        ),
        (
            PartId::HealNumbers,
            "CHealthAccountPanel",
            None,
            hudlayout("CHealthAccountPanel"),
            ui("hudhealthaccount.res"),
        ),
        (
            PartId::DamageNumbers,
            "CDamageAccountPanel",
            None,
            hudlayout("CDamageAccountPanel"),
            ui("huddamageaccount.res"),
        ),
        (
            PartId::TargetId,
            "CMainTargetID",
            None,
            hudlayout("CMainTargetID"),
            ui("targetid.res"),
        ),
        (
            PartId::ItemMeter,
            "HudItemEffectMeter",
            None,
            hudlayout("HudItemEffectMeter"),
            ui("huditemeffectmeter.res"),
        ),
        (
            PartId::RoundTimer,
            "ObjectiveStatusTimePanel",
            Some("HudMatchStatus"),
            vec![("resource/ui/hudmatchstatus.res", "ObjectiveStatusTimePanel")],
            ui("hudobjectivetimepanel.res"),
        ),
        (
            PartId::KothTimer,
            "HudKothTimeStatus",
            None,
            hudlayout("HudKothTimeStatus"),
            ui("hudobjectivekothtimepanel.res"),
        ),
    ];
    for (id, panel, parent, geometry, children) in table {
        let rule = PanelRule::get(id);
        assert_eq!((rule.panel(), rule.parent()), (panel, parent), "{id:?}");
        assert_eq!(rule.geometry(), geometry.as_slice(), "{id:?}");
        assert_eq!(rule.children_file(), children, "{id:?}");
    }
}

#[test]
fn hides_and_conditions_come_from_the_probe_tables() {
    assert_eq!(
        PanelRule::get(PartId::Health).hide(),
        HideTechnique::VisibleZero
    );
    assert_eq!(
        PanelRule::get(PartId::TargetId).hide(),
        HideTechnique::OffScreenY
    );
    assert_eq!(
        PanelRule::get(PartId::DamageNumbers).hide(),
        HideTechnique::NotOffered
    );
    assert_eq!(
        PanelRule::get(PartId::RoundTimer).conditions(),
        ["if_match"]
    );
    assert!(PanelRule::get(PartId::Health).conditions().is_empty());
}

#[test]
fn code_owned_keys() {
    let target = PanelRule::get(PartId::TargetId);
    assert!(target.ignores("xpos") && target.ignores("WIDE") && target.ignores("visible"));
    assert!(!target.ignores("ypos") && target.centres_x());
    let damage = PanelRule::get(PartId::DamageNumbers);
    assert!(
        ["xpos", "ypos", "wide", "tall"]
            .iter()
            .all(|k| damage.ignores(k))
    );
    assert!(PanelRule::get(PartId::RoundTimer).ignores("visible"));
    let health = PanelRule::get(PartId::Health);
    assert!(!health.ignores("visible") && !health.ignores("xpos") && !health.centres_x());
}
