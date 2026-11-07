//! The health pulse image: the game shows it only while you are overhealed or low on health.

use crate::common;

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

// The bonus image's box, shown, with `extra` keys on the health panel.
fn pulse_box(extra: &str, s: &GameSituation) -> (i32, i32, i32, i32) {
    let health = health_with_bonus()
        .replace("    alpha 0\n", "    visible 0\n")
        .replacen(
            "    visible 1\n  }",
            &format!("    visible 1\n{extra}  }}"),
            1,
        );
    let m = model_with(&health);
    let layout = m.layout_in(HD, s);
    let b = layout.get(&PartKey::new(BONUS)).unwrap();
    (b.x(), b.y(), b.wide(), b.tall())
}

// The box grown by `adj` pixels on every side.
fn grown((x, y, w, t): (i32, i32, i32, i32), adj: i32) -> (i32, i32, i32, i32) {
    (x - adj, y - adj, w + 2 * adj, t + 2 * adj)
}

#[test]
fn the_dying_pulse_grows_as_health_falls_below_the_warning() {
    // A Soldier's warning line is 98 of 200; the game grows by HealthBonusPosAdj (25) times
    // how far under the line health sits.
    let s = GameSituation::default();
    let file = pulse_box("", &s);
    assert_eq!(pulse_box("", &s.clone().with_health(97)), grown(file, 0));
    assert_eq!(pulse_box("", &s.clone().with_health(30)), grown(file, 17));
    assert_eq!(pulse_box("", &s.clone().with_health(1)), grown(file, 25));
}

#[test]
fn the_hud_s_own_pos_adj_sets_how_far_the_pulse_grows() {
    let s = GameSituation::default().with_health(49);
    let file = pulse_box("", &GameSituation::default());
    assert_eq!(pulse_box("    HealthBonusPosAdj 40\n", &s), grown(file, 20));
}

#[test]
fn the_overheal_pulse_grows_with_the_bonus_up_to_the_cap() {
    // A Soldier overheals from 200 to 300.
    let s = GameSituation::default();
    let file = pulse_box("", &s);
    assert_eq!(pulse_box("", &s.clone().with_health(260)), grown(file, 15));
    assert_eq!(pulse_box("", &s.with_health(400)), grown(file, 25));
}
