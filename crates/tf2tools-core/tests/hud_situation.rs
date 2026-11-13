//! Game situations: what shows, the sample numbers and the events a moment fires.

use crate::common;

use common::hud_anims::HEALTH_KEY;
use common::hud_stock::{HEALTH, stock_files, stock_files_with};
use tf2tools_core::{
    Fired, GameSituation, HiddenBy, HudModel, PartKey, Platform, SampleSet, Screen, ShowsFor,
    SituationSwitch, TfClass, WeaponKind,
};

const HD: Screen = Screen::new(1920, 1080);

fn model() -> HudModel {
    HudModel::load(&stock_files(), Platform::Linux).unwrap()
}

fn hidden(m: &HudModel, s: &GameSituation, key: &str) -> Option<HiddenBy> {
    m.layout_in(HD, s).get(&PartKey::new(key)).unwrap().hidden()
}

#[test]
fn the_default_situation_is_today_s_preview() {
    let s = GameSituation::default();
    assert_eq!(
        (s.class(), s.health(), s.ammo_percent(), s.charge()),
        (TfClass::Soldier, 150, 100, 0)
    );
    assert_eq!(s.weapon(), WeaponKind::Clip);
    let m = model();
    let (a, b) = (m.layout(HD), m.layout_in(HD, &s));
    for part in m.parts() {
        assert_eq!(
            a.get(part.key()).map(|p| p.hidden()),
            b.get(part.key()).map(|p| p.hidden()),
            "{}",
            part.key().as_str()
        );
    }
}

#[test]
fn class_and_switches_drive_code_toggled_parts() {
    let m = model();
    let medic = GameSituation::default().with_class(TfClass::Medic);
    assert_eq!(
        hidden(&m, &GameSituation::default(), "HudMedicCharge"),
        Some(HiddenBy::Game)
    );
    assert_eq!(hidden(&m, &medic, "HudMedicCharge"), None);
    let aiming = GameSituation::default().with_switch(SituationSwitch::Aiming, true);
    assert_eq!(hidden(&m, &aiming, "CMainTargetID"), None);
    let s = GameSituation::default()
        .with_ammo_percent(39)
        .with_switch(SituationSwitch::Portrait, true);
    assert!(ShowsFor::Weapon("low ammo").holds(&s));
    assert!(ShowsFor::Mode("2D portrait").holds(&s) && !ShowsFor::Mode("3D model").holds(&s));
    assert!(
        ShowsFor::Weapon("no clip")
            .holds(&GameSituation::default().with_weapon(WeaponKind::NoClip))
    );
}

#[test]
fn a_medic_holds_the_medi_gun_so_the_ammo_hides() {
    // CTFHudWeaponAmmo::ShouldDraw is false for the Medi Gun, which the Medic situation holds.
    let m = model();
    let medic = GameSituation::default()
        .with_class(TfClass::Medic)
        .with_charge(100);
    assert_eq!(hidden(&m, &medic, "HudWeaponAmmo"), Some(HiddenBy::Game));
    assert_eq!(hidden(&m, &medic, "HudMedicCharge"), None);
    assert_eq!(hidden(&m, &GameSituation::default(), "HudWeaponAmmo"), None);
}

#[test]
fn the_charge_meter_fills_to_the_medic_s_charge() {
    // CHudMedicChargeMeter sets ChargeMeter's progress to the charge; other bars draw whole.
    let charge = r#""Resource/UI/HudMedicCharge.res" { ChargeLabel { ControlName CExLabel xpos 0 ypos 0 wide 100 tall 20 } ChargeMeter { ControlName ContinuousProgressBar xpos 30 ypos 38 wide 86 tall 8 }
  IndividualChargesLabel { ControlName CExLabel xpos 30 ypos 24 wide 90 tall 15 }
  ChargeMeter1 { ControlName ContinuousProgressBar xpos 30 ypos 38 wide 19 tall 8 }
  ChargeMeter4 { ControlName ContinuousProgressBar xpos 96 ypos 38 wide 19 tall 8 } }"#;
    let m = HudModel::load(
        &stock_files_with(&[("resource/ui/hudmediccharge.res", Some(charge))]),
        Platform::Linux,
    )
    .unwrap();
    // Without the Vaccinator the game hides the per-charge bars and their label.
    let medic = GameSituation::default().with_class(TfClass::Medic);
    for bar in ["IndividualChargesLabel", "ChargeMeter1", "ChargeMeter4"] {
        let key = format!("HudMedicCharge/{bar}");
        assert_eq!(hidden(&m, &medic, &key), Some(HiddenBy::Game), "{bar}");
    }
    let meter = m.part(&PartKey::new("HudMedicCharge/ChargeMeter")).unwrap();
    let medic = GameSituation::default()
        .with_class(TfClass::Medic)
        .with_charge(75);
    assert_eq!(m.bar_fill(meter, &medic), Some(0.75));
    let label = m.part(&PartKey::new("HudMedicCharge/ChargeLabel")).unwrap();
    assert_eq!(m.bar_fill(label, &medic), None);
}

#[test]
fn health_is_clamped_to_the_class_s_overheal() {
    let s = GameSituation::default().with_health(999);
    assert_eq!(
        (s.max_health(), s.overheal_cap(), s.health()),
        (200, 300, 300)
    );
    let scout = s.with_class(TfClass::Scout);
    assert_eq!((scout.overheal_cap(), scout.health()), (185, 185));
    assert_eq!(GameSituation::default().with_health(0).health(), 1);
}

#[test]
fn sample_numbers_follow_the_situation() {
    let s = GameSituation::default()
        .with_health(60)
        .with_ammo_percent(50)
        .with_charge(75);
    assert_eq!(
        SampleSet::Empty.fill_in("%Health% / %Ammo% / %AmmoInReserve% / %Charge%", &s),
        "60 / 20 / 75 / 75%"
    );
    assert_eq!(SampleSet::Empty.fill("%Health%"), "150");
}

#[test]
fn the_max_health_label_shows_once_five_or_more_below_max() {
    // CTFHudPlayerHealth::SetHealth sets MaxHealth only when max - health >= 5.
    let s = GameSituation::default();
    let max = |h: u16| SampleSet::Empty.fill_in("%MaxHealth%", &s.clone().with_health(h));
    assert_eq!(max(60), "200");
    assert_eq!(max(195), "200");
    assert_eq!(max(196), "");
    assert_eq!(max(250), "");
}

#[test]
fn a_moment_fires_the_events_the_game_would() {
    let m = model();
    let health = Some(PartKey::new(HEALTH_KEY));
    let low = GameSituation::default().with_health(60);
    assert_eq!(
        low.fired(&m),
        [Fired::new("HudHealthDyingPulse", health.clone())]
    );
    let over = GameSituation::default().with_health(250);
    assert_eq!(
        over.fired(&m),
        [Fired::new("HudHealthBonusPulse", health.clone())]
    );
    assert!(GameSituation::default().fired(&m).is_empty());
    let busy = GameSituation::default()
        .with_class(TfClass::Medic)
        .with_charge(100)
        .with_ammo_percent(30)
        .with_switch(SituationSwitch::JustHit, true);
    assert_eq!(
        busy.fired(&m),
        [
            Fired::new("HudLowAmmoPulse", Some(PartKey::new("HudWeaponAmmo"))),
            Fired::new("HudMedicCharged", Some(PartKey::new("HudMedicCharge"))),
            Fired::new("DamagedPlayer", None),
        ]
    );
}

#[test]
fn the_dying_threshold_is_read_from_the_health_panel() {
    let health = HEALTH.replacen(
        "    visible 1\n  }",
        "    visible 1\n    HealthDeathWarning 0.8\n  }",
        1,
    );
    assert_ne!(health, HEALTH);
    let m = HudModel::load(
        &stock_files_with(&[("resource/ui/hudplayerhealth.res", Some(&health))]),
        Platform::Linux,
    )
    .unwrap();
    let fired = GameSituation::default().fired(&m);
    assert_eq!(
        fired,
        [Fired::new(
            "HudHealthDyingPulse",
            Some(PartKey::new(HEALTH_KEY))
        )]
    );
}
