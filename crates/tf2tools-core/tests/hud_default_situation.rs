//! The HUD scene's default situation: a RED Soldier holding a rocket launcher, so parts the
//! game shows only for other classes, weapons or effects start hidden by the game.

use crate::common;

use common::hud_stock::{HEALTH, block, stock_files_with, with_blocks};
use tf2tools_core::{
    HiddenBy, HudLayout, HudModel, PartKey, Platform, Scene, SceneRule, Screen, ShowsFor,
};

const HD: Screen = Screen::new(1920, 1080);
const HEALTH_PANEL: &str = "HudPlayerStatus/HudPlayerHealth";
const CLASS_PANEL: &str = "HudPlayerStatus/HudPlayerClass";

/// Stock-shaped class and ammo files carrying the code-toggled children the real ones have.
const CLASS: &str = r#""Resource/UI/HudPlayerClass.res" { HudPlayerClass { ControlName EditablePanel xpos 0 ypos 0 wide f0 tall 480 }
  PlayerStatusClassImage { ControlName CTFClassImage xpos 25 ypos r75 wide 75 tall 75 image ../hud/class_scoutred }
  PlayerStatusSpyImage { ControlName CTFImagePanel xpos 2 ypos r53 wide 55 tall 55 image ../hud/class_spyred }
  classmodelpanel { ControlName CTFPlayerModelPanel xpos 0 ypos r213 wide 100 tall 200 }
  CarryingWeapon { ControlName EditablePanel xpos 20 ypos r27 wide 500 tall 28 } }"#;
const AMMO: &str = r#""Resource/UI/HudAmmoWeapons.res" {
  AmmoInClip { ControlName CExLabel xpos 10 ypos 5 wide 55 tall 40 visible 0 labelText %Ammo% }
  AmmoNoClip { ControlName CExLabel xpos 6 ypos 7 wide 84 tall 40 visible 1 labelText %Ammo% } }"#;

fn layout() -> HudLayout {
    let blocks = block(
        "PlayerStatusBleedImage",
        "ImagePanel",
        &[
            ("xpos", "85"),
            ("ypos", "0"),
            ("wide", "32"),
            ("tall", "32"),
        ],
    ) + &block(
        "PlayerStatus_SpyMarked",
        "ImagePanel",
        &[
            ("xpos", "120"),
            ("ypos", "0"),
            ("wide", "30"),
            ("tall", "30"),
            ("visible", "1"),
            ("image", "\"\""),
        ],
    );
    let health = with_blocks(HEALTH, &blocks);
    let files = stock_files_with(&[
        ("resource/ui/hudplayerhealth.res", Some(&health)),
        ("resource/ui/hudplayerclass.res", Some(CLASS)),
        ("resource/ui/hudammoweapons.res", Some(AMMO)),
    ]);
    HudModel::load(&files, Platform::Linux).unwrap().layout(HD)
}

fn hidden(l: &HudLayout, key: &str) -> Option<HiddenBy> {
    l.get(&PartKey::new(key))
        .unwrap_or_else(|| panic!("{key}"))
        .hidden()
}

#[test]
fn other_classes_panels_start_hidden_by_the_game() {
    let l = layout();
    assert_eq!(hidden(&l, "HudMedicCharge"), Some(HiddenBy::Game));
    assert_eq!(
        hidden(&l, "HudMedicCharge/ChargeLabel"),
        Some(HiddenBy::Parent)
    );
    assert_eq!(hidden(&l, "HudItemEffectMeter"), Some(HiddenBy::Game));
    assert_eq!(
        hidden(&l, &format!("{CLASS_PANEL}/PlayerStatusSpyImage")),
        Some(HiddenBy::Game)
    );
    assert_eq!(
        hidden(&l, &format!("{CLASS_PANEL}/CarryingWeapon")),
        Some(HiddenBy::Game)
    );
    assert_eq!(
        hidden(&l, &format!("{HEALTH_PANEL}/PlayerStatusBleedImage")),
        Some(HiddenBy::Game)
    );
    assert_eq!(
        hidden(&l, &format!("{HEALTH_PANEL}/PlayerStatus_SpyMarked")),
        Some(HiddenBy::Game),
        "the game hides it over visible 1"
    );
}

#[test]
fn the_soldiers_own_parts_show() {
    let l = layout();
    assert_eq!(hidden(&l, "HudWeaponAmmo"), None);
    assert_eq!(
        hidden(&l, "HudWeaponAmmo/AmmoInClip"),
        None,
        "code shows it over visible 0"
    );
    assert_eq!(hidden(&l, "HudWeaponAmmo/AmmoNoClip"), Some(HiddenBy::Game));
    assert_eq!(
        hidden(&l, "CMainTargetID"),
        Some(HiddenBy::Game),
        "aiming at no one"
    );
    assert_eq!(hidden(&l, &format!("{CLASS_PANEL}/classmodelpanel")), None);
    assert_eq!(
        hidden(&l, &format!("{CLASS_PANEL}/PlayerStatusClassImage")),
        Some(HiddenBy::Game)
    );
    assert_eq!(
        hidden(&l, &format!("{HEALTH_PANEL}/PlayerStatusHealthValue")),
        None
    );
}

#[test]
fn every_hud_toggle_names_what_the_game_checks() {
    let hud = SceneRule::get(Scene::Hud);
    assert_eq!(hud.code_toggled().len(), 57);
    for t in hud.code_toggled() {
        let shows_for = t.shows_for().unwrap_or_else(|| panic!("{}", t.panel()));
        assert_eq!(t.shown_in_sample(), shows_for.in_default(), "{}", t.panel());
        assert!(t.shows_when().starts_with("Shows "), "{}", t.panel());
    }
    let toggle = |panel| hud.toggled(panel).unwrap();
    assert_eq!(
        toggle("hudmediccharge").shows_for(),
        Some(ShowsFor::Class("Medic"))
    );
    assert_eq!(
        toggle("hudmediccharge").shows_when(),
        "Shows when you hold a Medi Gun"
    );
    assert_eq!(
        toggle("AmmoInClip").shows_for(),
        Some(ShowsFor::Weapon("clip"))
    );
    assert_eq!(
        toggle("PlayerStatus_Parachute").shows_for(),
        Some(ShowsFor::Effect("condition"))
    );
    assert_eq!(
        toggle("PlayerStatus_RuneKing").shows_for(),
        Some(ShowsFor::Effect("powerup"))
    );
    assert_eq!(
        toggle("classmodelpanel").shows_for(),
        Some(ShowsFor::Mode("3D model"))
    );
    assert!(hud.toggled("PlayerStatusHealthValue").is_none());
}
