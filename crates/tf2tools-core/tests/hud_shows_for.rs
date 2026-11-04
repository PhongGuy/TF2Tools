//! What the game checks before it shows a code-toggled part, and the HUD's default situation.

use tf2tools_core::{Scene, SceneRule, ShowsFor};

#[test]
fn the_default_situation_is_a_soldier_with_a_clip_weapon() {
    assert!(ShowsFor::Class("Soldier").in_default());
    assert!(!ShowsFor::Class("Medic").in_default());
    assert!(ShowsFor::Weapon("clip").in_default());
    for kind in ["no clip", "meter", "low ammo"] {
        assert!(!ShowsFor::Weapon(kind).in_default(), "{kind}");
    }
    assert!(!ShowsFor::Effect("condition").in_default());
    assert!(ShowsFor::Mode("3D model").in_default());
    assert!(!ShowsFor::Mode("2D portrait").in_default());
    assert!(!ShowsFor::Mode("typing").in_default());
}

#[test]
fn scoreboard_defaults_stay_fixed() {
    let board = SceneRule::get(Scene::Scoreboard);
    assert!(board.code_toggled().iter().all(|t| t.shows_for().is_none()));
    assert!(board.toggled("BlueTeamImage").unwrap().shown_in_sample());
    assert!(!board.toggled("ClassImage").unwrap().shown_in_sample());
}

#[test]
fn the_target_id_shows_only_while_you_aim_at_a_player() {
    assert!(!ShowsFor::Aim("player").in_default());
    let hud = SceneRule::get(Scene::Hud);
    let target = hud.toggled("cmaintargetid").unwrap();
    assert_eq!(target.shows_for(), Some(ShowsFor::Aim("player")));
    assert_eq!(target.shows_when(), "Shows while you aim at a player");
    assert!(!target.shown_in_sample());
}
