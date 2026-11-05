//! Tinting an image by the colour the game draws it in.

mod common;

use common::hud_anims::{BONUS, HEALTH_RES, health_with_bonus};
use common::hud_stock::stock_files_with;
use tf2tools_core::{GameSituation, HudModel, PartKey, Platform, Rgba, tint_pixels};

#[test]
fn a_white_pixel_takes_the_tint_colour() {
    let mut px = [255, 255, 255, 255];
    tint_pixels(&mut px, Rgba::new(200, 40, 10, 255));
    assert_eq!(px, [200, 40, 10, 255]);
}

#[test]
fn the_tint_multiplies_the_pixel_s_alpha_by_the_colour_s() {
    let mut px = [255, 255, 255, 128, 255, 255, 255, 255];
    tint_pixels(&mut px, Rgba::new(255, 0, 0, 128));
    assert_eq!(px, [255, 0, 0, 64, 255, 0, 0, 128]);
}

fn model(health_keys: &str, bonus_keys: &str) -> HudModel {
    let health = health_with_bonus()
        .replace("    alpha 0\n", &format!("    visible 0\n{bonus_keys}"))
        .replacen(
            "    visible 1\n  }",
            &format!("    visible 1\n{health_keys}  }}"),
            1,
        );
    HudModel::load(
        &stock_files_with(&[(HEALTH_RES, Some(&health))]),
        Platform::Linux,
    )
    .unwrap()
}

fn colour_of(m: &HudModel, s: &GameSituation) -> Option<Rgba> {
    m.draw_colour(m.part(&PartKey::new(BONUS)).unwrap(), s)
}

#[test]
fn the_dying_pulse_draws_in_the_death_warning_colour() {
    let m = model("    HealthDeathWarningColor \"200 30 30 255\"\n", "");
    let dying = GameSituation::default().with_health(30);
    assert_eq!(colour_of(&m, &dying), Some(Rgba::new(200, 30, 30, 255)));
}

#[test]
fn the_overheal_pulse_draws_white_whatever_its_draw_colour() {
    let m = model(
        "    HealthDeathWarningColor \"200 30 30 255\"\n",
        "    drawcolor \"0 255 0 255\"\n",
    );
    let overhealed = GameSituation::default().with_health(260);
    assert_eq!(colour_of(&m, &overhealed), None);
}

#[test]
fn another_image_draws_in_its_own_draw_colour() {
    let cross = "HudPlayerStatus/HudPlayerHealth/PlayerStatusHealthImage";
    let health = health_with_bonus().replacen(
        "    wide 51\n",
        "    wide 51\n    drawcolor \"0 255 0 128\"\n",
        1,
    );
    let m = HudModel::load(
        &stock_files_with(&[(HEALTH_RES, Some(&health))]),
        Platform::Linux,
    )
    .unwrap();
    let part = m.part(&PartKey::new(cross)).unwrap();
    let s = GameSituation::default();
    assert_eq!(m.draw_colour(part, &s), Some(Rgba::new(0, 255, 0, 128)));
    let plain = model("", "");
    assert_eq!(
        plain.draw_colour(plain.part(&PartKey::new(cross)).unwrap(), &s),
        None
    );
}
