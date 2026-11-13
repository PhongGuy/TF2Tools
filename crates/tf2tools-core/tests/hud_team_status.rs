//! The match HUD's player panels beside the timer tab, laid out from TeamStatus's settings the
//! way `CTFTeamStatus::PerformLayout` places them: the local RED Soldier and one BLU enemy.

use crate::common;

use common::hud_stock::{TEAM_STATUS, stock_files_with};
use tf2tools_core::{HudModel, Platform, Rect, Rgba, Scene, Screen, Team};

const HD: Screen = Screen::new(1920, 1080);
const MATCH: &str = "resource/ui/hudmatchstatus.res";

fn stock_match() -> String {
    let files = common::hud_stock::stock();
    files.into_iter().find(|(p, _)| p == MATCH).unwrap().1
}

/// The stock match file with its TeamStatus block replaced by `block`.
fn with_team_status(block: &str) -> HudModel {
    let text = stock_match().replace(TEAM_STATUS, block);
    let files = stock_files_with(&[(MATCH, Some(text.as_str()))]);
    HudModel::load(&files, Platform::Linux).unwrap()
}

#[test]
fn the_stock_panels_sit_either_side_of_the_tab_as_in_game() {
    let m = with_team_status(TEAM_STATUS);
    let panels = m.team_status(HD);
    assert_eq!(panels.len(), 2);
    let blue = panels.iter().find(|p| p.team() == Team::Blue).unwrap();
    let red = panels.iter().find(|p| p.team() == Team::Red).unwrap();
    // Measured in game at 1920x1080: the BLU panel spans x 817 to 857, the RED one starts at
    // 1065. The panel steps by max_size but is as wide as max_size normalised and scaled again.
    assert_eq!(blue.rect(), Rect::new(817, 0, 40, 112));
    assert_eq!(red.rect(), Rect::new(1065, 0, 40, 112));
    assert!(red.local() && !blue.local());
    // The portrait, centred and 19 units wide, is cut at the panel's edges.
    assert_eq!(blue.portrait(), Rect::new(817, 0, 40, 42));
    assert_eq!(blue.backing(), Rect::new(817, 0, 40, 42));
    // Only your own team's panels show health.
    assert_eq!(red.health(), Some(Rect::new(1065, 42, 40, 4)));
    assert_eq!(red.health_colour(), Rgba::new(84, 191, 58, 255));
    assert_eq!(blue.health(), None);
}

#[test]
fn the_backing_takes_the_files_colour_and_the_local_players_from_the_code() {
    let block = TEAM_STATUS.replace(r#""62 81 101 255""#, r#""1 2 3 255""#);
    let panels = with_team_status(&block).team_status(HD);
    let colour = |team| panels.iter().find(|p| p.team() == team).unwrap();
    assert_eq!(colour(Team::Blue).backing_colour(), Rgba::new(1, 2, 3, 255));
    // color_portrait_bg_red_local_player is unset, so the code's default holds.
    assert_eq!(
        colour(Team::Red).backing_colour(),
        Rgba::new(195, 169, 168, 255)
    );
}

#[test]
fn a_hud_that_moves_the_bases_and_size_moves_the_panels() {
    let block = TEAM_STATUS
        .replace("team1_base_x c-45", "team1_base_x c-100")
        .replace("max_size 19", "max_size 30");
    let panels = with_team_status(&block).team_status(HD);
    let blue = panels.iter().find(|p| p.team() == Team::Blue).unwrap();
    // c-100 is 960 - 225; the panel steps 67 px west of it and is 65 px wide.
    assert_eq!(blue.rect(), Rect::new(668, 0, 65, 112));
}

#[test]
fn without_a_team_status_block_or_outside_the_hud_no_panels_draw() {
    assert!(with_team_status("").team_status(HD).is_empty());
    let m = with_team_status(TEAM_STATUS);
    assert!(m.with_scene(Scene::Scoreboard).team_status(HD).is_empty());
}
