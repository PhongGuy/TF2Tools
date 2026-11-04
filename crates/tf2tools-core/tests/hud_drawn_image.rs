//! The texture an image part draws for the local player's team: a team-coloured panel's
//! `teambg_<team>`, else its `image`, else the texture the game's code paints into it.

mod common;

use common::hud_stock::{HEALTH, block, stock_files_with, with_blocks};
use tf2tools_core::{HudModel, PartKey, Platform, Team, drawn_image};

const PANEL: &str = "HudPlayerStatus/HudPlayerHealth";

fn model(blocks: &str) -> HudModel {
    let health = with_blocks(HEALTH, blocks);
    let files = stock_files_with(&[("resource/ui/hudplayerhealth.res", Some(&health))]);
    HudModel::load(&files, Platform::Linux).unwrap()
}

fn drawn(m: &HudModel, name: &str, team: Team) -> Option<String> {
    let part = m.part(&PartKey::new(format!("{PANEL}/{name}"))).unwrap();
    drawn_image(part, team).map(str::to_owned)
}

const TEAM_KEYS: [(&str, &str); 3] = [
    ("image", "../hud/ammo_blue_bg"),
    ("teambg_2", "../hud/ammo_red_bg"),
    ("teambg_3", "../hud/ammo_blue_bg"),
];

#[test]
fn a_team_coloured_panel_draws_its_teams_background() {
    let m = model(&block("AmmoBG", "CTFImagePanel", &TEAM_KEYS));
    assert_eq!(
        drawn(&m, "AmmoBG", Team::Red).as_deref(),
        Some("../hud/ammo_red_bg")
    );
    assert_eq!(
        drawn(&m, "AmmoBG", Team::Blue).as_deref(),
        Some("../hud/ammo_blue_bg")
    );
}

#[test]
fn a_plain_image_panel_ignores_team_keys() {
    let m = model(&block("LowAmmo", "ImagePanel", &TEAM_KEYS));
    assert_eq!(
        drawn(&m, "LowAmmo", Team::Red).as_deref(),
        Some("../hud/ammo_blue_bg")
    );
}

#[test]
fn a_blank_team_key_falls_back_on_the_image() {
    let m = model(&block(
        "AmmoBG",
        "ctfimagepanel",
        &[("image", "../hud/ammo_blue_bg"), ("teambg_2", "\"\"")],
    ));
    assert_eq!(
        drawn(&m, "AmmoBG", Team::Red).as_deref(),
        Some("../hud/ammo_blue_bg")
    );
}

#[test]
fn the_health_cross_draws_the_texture_its_code_paints() {
    // The stock HEALTH fixture's PlayerStatusHealthImage names no image, as the game's does.
    let m = model("");
    assert_eq!(
        drawn(&m, "PlayerStatusHealthImage", Team::Red).as_deref(),
        Some("../hud/health_color")
    );
    assert_eq!(drawn(&m, "PlayerStatusHealthValue", Team::Red), None);
}

#[test]
fn a_file_image_wins_over_the_codes() {
    let health = HEALTH.replacen(
        "PlayerStatusHealthImage\n  {\n    ControlName ImagePanel",
        "PlayerStatusHealthImage\n  {\n    ControlName ImagePanel\n    image ../hud/my_cross",
        1,
    );
    assert_ne!(health, HEALTH, "the fixture names the part");
    let files = stock_files_with(&[("resource/ui/hudplayerhealth.res", Some(&health))]);
    let m = HudModel::load(&files, Platform::Linux).unwrap();
    assert_eq!(
        drawn(&m, "PlayerStatusHealthImage", Team::Red).as_deref(),
        Some("../hud/my_cross")
    );
}
