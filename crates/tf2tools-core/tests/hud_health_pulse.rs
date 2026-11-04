//! The health pulse image: the game shows it only while you are overhealed or low on health.

mod common;

use common::hud_anims::{BONUS, HEALTH_RES, health_with_bonus};
use common::hud_stock::stock_files_with;
use tf2tools_core::{
    GameSituation, HiddenBy, HudModel, PartKey, Platform, Scene, SceneRule, Screen, TfClass,
};

const HD: Screen = Screen::new(1920, 1080);

// The stock health file with its pulse image as the game ships it: hidden in the file.
fn model_with(health: &str) -> HudModel {
    HudModel::load(
        &stock_files_with(&[(HEALTH_RES, Some(health))]),
        Platform::Linux,
    )
    .unwrap()
}

fn hidden(m: &HudModel, s: &GameSituation) -> Option<HiddenBy> {
    m.layout_in(HD, s)
        .get(&PartKey::new(BONUS))
        .unwrap()
        .hidden()
}

#[test]
fn the_health_pulse_image_shows_only_when_overhealed_or_dying() {
    let m = model_with(&health_with_bonus().replace("    alpha 0\n", "    visible 0\n"));
    let s = GameSituation::default();
    assert_eq!(hidden(&m, &s), Some(HiddenBy::Game));
    assert_eq!(hidden(&m, &s.clone().with_health(60)), None);
    assert_eq!(hidden(&m, &s.with_health(250)), None);
}

#[test]
fn the_health_pulse_image_is_game_toggled_in_the_hud_scene() {
    let hud = SceneRule::get(Scene::Hud);
    let toggle = hud.toggled("PlayerStatusHealthBonusImage");
    assert!(toggle.is_some());
    assert_eq!(
        toggle.unwrap().shows_when(),
        "Shows while you are overhealed or low on health"
    );
}

#[test]
fn the_hud_s_own_death_warning_decides_the_dying_pulse_image() {
    // A Heavy at 150 of 300 is dying under a 0.8 warning but not under the game's 0.49.
    let warned = health_with_bonus()
        .replace("    alpha 0\n", "    visible 0\n")
        .replacen(
            "    visible 1\n  }",
            "    visible 1\n    HealthDeathWarning 0.8\n  }",
            1,
        );
    let m = model_with(&warned);
    let heavy = GameSituation::default().with_class(TfClass::Heavy);
    assert_eq!(hidden(&m, &heavy.clone().with_health(150)), None);
    assert_eq!(hidden(&m, &heavy.with_health(250)), Some(HiddenBy::Game));
}

#[test]
fn health_exactly_at_the_death_warning_is_not_dying() {
    // A Soldier at 100 of 200 under a 0.5 warning sits on the line, which is not below it.
    let warned = health_with_bonus()
        .replace("    alpha 0\n", "    visible 0\n")
        .replacen(
            "    visible 1\n  }",
            "    visible 1\n    HealthDeathWarning 0.5\n  }",
            1,
        );
    let m = model_with(&warned);
    let s = GameSituation::default();
    assert_eq!(
        hidden(&m, &s.clone().with_health(100)),
        Some(HiddenBy::Game)
    );
    assert_eq!(hidden(&m, &s.with_health(99)), None);
}
