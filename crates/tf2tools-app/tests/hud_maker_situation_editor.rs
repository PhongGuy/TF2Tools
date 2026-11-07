//! The editor's game situation, played events and moment, without GPUI.

use crate::support;

use support::hud_stock::{HEALTH, block, stock_files_with, with_blocks};
use support::maker_page::{VALUE, editor, overrides};
use tf2tools_app::pages::hud_maker::editor::HudEditor;
use tf2tools_core::{
    Fired, GameSituation, HudModel, PartKey, Platform, Rgba, SituationSwitch, TfClass,
};

#[test]
fn the_situation_starts_at_today_s_preview_and_relayouts() {
    let mut e = editor();
    assert_eq!(e.situation(), &GameSituation::default());
    let charge = PartKey::new("HudMedicCharge");
    assert!(e.layout().get(&charge).unwrap().hidden().is_some());
    e.set_situation(GameSituation::default().with_class(TfClass::Medic));
    assert!(e.layout().get(&charge).unwrap().hidden().is_none());
    e.select(Some(PartKey::new(VALUE)));
    e.recolour_selected("255 0 0 255").unwrap();
    assert!(
        e.layout().get(&charge).unwrap().hidden().is_none(),
        "a replay keeps the situation"
    );
    assert!(e.undo());
    assert_eq!(
        e.situation().class(),
        TfClass::Medic,
        "undo leaves the situation"
    );
}

#[test]
fn low_health_plays_the_dying_pulse_at_the_moment() {
    let mut e = editor();
    e.set_situation(GameSituation::default().with_health(60));
    assert_eq!(
        e.fired(),
        [Fired::new(
            "HudHealthDyingPulse",
            Some(PartKey::new("HudPlayerStatus/HudPlayerHealth"))
        )]
    );
    e.set_moment(0.025);
    assert_eq!(
        e.frame().get(&PartKey::new(VALUE)).unwrap().fg(),
        Some(Rgba::new(133, 103, 90, 255))
    );
    e.set_moment(99.0);
    assert_eq!((e.moment(), e.length()), (2.0, 2.0));
    e.set_situation(GameSituation::default());
    assert_eq!(e.moment(), 0.0);
}

#[test]
fn a_played_event_starts_where_its_panels_live() {
    let mut e = editor();
    assert!(e.play_event("HudMedicCharged"));
    assert!(e.play_event("DamagedPlayer"));
    assert!(!e.play_event("NoSuchEvent"));
    assert_eq!(
        e.fired(),
        [
            Fired::new("HudMedicCharged", Some(PartKey::new("HudMedicCharge"))),
            Fired::new("DamagedPlayer", None),
        ]
    );
    e.set_situation(GameSituation::default().with_switch(SituationSwitch::JustHit, true));
    assert_eq!(
        e.fired()
            .iter()
            .filter(|f| f.event() == "DamagedPlayer")
            .count(),
        1
    );
    e.clear_played();
    assert_eq!(e.fired(), [Fired::new("DamagedPlayer", None)]);
}

#[test]
fn show_it_picks_the_situation_a_part_needs() {
    let mut e = editor();
    assert!(e.show_part_situation(&PartKey::new("HudMedicCharge/ChargeLabel")));
    assert_eq!(e.situation().class(), TfClass::Medic);
    assert!(e.show_part_situation(&PartKey::new("CMainTargetID")));
    assert!(e.situation().is_on(SituationSwitch::Aiming));
    assert!(!e.show_part_situation(&PartKey::new(VALUE)));
}

const BONUS: &str = "HudPlayerStatus/HudPlayerHealth/PlayerStatusHealthBonusImage";

// An editor over the stock files whose health file is `health` plus a hidden pulse image.
fn pulse_editor(health: &str) -> HudEditor {
    let keys = [("wide", "80"), ("tall", "80"), ("visible", "0")];
    let health = with_blocks(
        health,
        &block("PlayerStatusHealthBonusImage", "ImagePanel", &keys),
    );
    let mut files = overrides().to_vec();
    files.push(("resource/ui/hudplayerhealth.res", Some(&health)));
    let model = HudModel::load(&stock_files_with(&files), Platform::Linux).unwrap();
    HudEditor::new(model, Vec::new())
}

#[test]
fn show_it_brings_up_the_health_pulse_image() {
    let bonus = PartKey::new(BONUS);
    let mut e = pulse_editor(HEALTH);
    assert!(e.layout().get(&bonus).unwrap().hidden().is_some());
    assert!(e.show_part_situation(&bonus));
    assert_eq!(e.layout().get(&bonus).unwrap().hidden(), None);
}

#[test]
fn show_it_overheals_when_the_hud_never_warns_of_dying() {
    // A warning of 0 never shows the image at low health, so only overheal can.
    let health = HEALTH.replacen(
        "    visible 1\n  }",
        "    visible 1\n    HealthDeathWarning 0\n  }",
        1,
    );
    assert_ne!(health, HEALTH);
    let bonus = PartKey::new(BONUS);
    let mut e = pulse_editor(&health);
    assert!(e.show_part_situation(&bonus));
    assert_eq!(e.situation().health(), e.situation().overheal_cap());
    assert_eq!(e.layout().get(&bonus).unwrap().hidden(), None);
}
