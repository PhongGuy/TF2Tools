//! The HUD Maker's checking tools over the editor, without GPUI.

use crate::support;

use support::hud_stock::{HEALTH, label, stock_files_with, with_blocks};
use tf2tools_app::pages::hud_maker::checks::Checks;
use tf2tools_app::pages::hud_maker::editor::HudEditor;
use tf2tools_core::{
    Backdrop, HudModel, PartKey, Platform, Screen, ScreenProblem, SituationSwitch, Vision,
};

const UNDER: &str = "HudPlayerStatus/HudPlayerHealth/";
const HV: &str = "HudPlayerStatus/HudPlayerHealth/PlayerStatusHealthValue";

fn k(name: &str) -> PartKey {
    PartKey::new(format!("{UNDER}{name}"))
}

/// The stock HUD plus two labels: Left at `left` units meets Right only at 1280x1024 when it is
/// 400, and nowhere when it is 312.
fn editor_with(left: &str) -> HudEditor {
    let blocks = label("Right", "r200", "100", &[("labelText", "R")])
        + &label("Left", left, "100", &[("labelText", "L")]);
    let text = with_blocks(HEALTH, &blocks);
    let files = stock_files_with(&[("resource/ui/hudplayerhealth.res", Some(&text))]);
    HudEditor::new(HudModel::load(&files, Platform::Linux).unwrap(), Vec::new())
}

/// The two labels apart; a move of Left 200 px right makes them meet at 1280x1024 only.
fn editor() -> HudEditor {
    editor_with("312")
}

fn overlaps_at_5_4(checks: &[tf2tools_core::ScreenCheck]) -> bool {
    checks[3]
        .flags()
        .iter()
        .any(|f| f.part() == &k("Right") && f.problem() == &ScreenProblem::Overlaps(k("Left")))
}

#[test]
fn the_screens_are_checked_once_per_change() {
    let (mut e, mut checks) = (editor(), Checks::default());
    let first = checks.screens(&e);
    assert_eq!(first.len(), 6);
    assert_eq!(first[3].screen(), Screen::new(1280, 1024));
    assert!(!overlaps_at_5_4(&first));
    // Asking again, or selecting, reads the same result.
    for _ in 0..5 {
        checks.screens(&e);
    }
    e.select(Some(k("Left")));
    checks.screens(&e);
    assert_eq!(checks.screen_runs(), 1);
    // 200 px right at 1920x1080 brings Left onto Right at 1280x1024, a new overlap.
    e.move_selected(200, 0).unwrap();
    assert!(overlaps_at_5_4(&checks.screens(&e)));
    assert_eq!(checks.screen_runs(), 2);
    assert!(e.undo());
    assert!(!overlaps_at_5_4(&checks.screens(&e)));
    assert_eq!(checks.screen_runs(), 3);
}

#[test]
fn a_running_drag_is_checked_as_it_shows() {
    let (mut e, mut checks) = (editor(), Checks::default());
    checks.screens(&e);
    e.select(Some(k("Left")));
    e.drag(tf2tools_app::pages::hud_maker::editor::Handle::Move, 200, 0)
        .unwrap();
    assert!(overlaps_at_5_4(&checks.screens(&e)));
    e.cancel_drag();
    assert!(!overlaps_at_5_4(&checks.screens(&e)));
    assert_eq!(checks.screen_runs(), 3);
}

#[test]
fn the_screens_are_checked_against_the_hud_as_opened() {
    // Left (plain 400, 100) dragged 729 px left and 108 up lands on the health value (76, 52) on every
    // shape; the HUD as opened had them apart, so every shape flags it.
    let (mut e, mut checks) = (editor_with("400"), Checks::default());
    e.select(Some(k("Left")));
    e.move_selected(-729, -108).unwrap();
    let after = checks.screens(&e);
    let meets = after
        .iter()
        .filter(|c| {
            c.flags().iter().any(|f| {
                f.part() == &k("Left") && f.problem() == &ScreenProblem::Overlaps(PartKey::new(HV))
            })
        })
        .count();
    assert_eq!(meets, 6);
}

#[test]
fn hard_to_read_follows_the_situation_shown() {
    // Each game mode shows its own timer panel, so the labels to judge differ between them.
    let (mut e, mut checks) = (editor(), Checks::default());
    let white = Backdrop::new(1, 1, vec![255, 255, 255, 255]).unwrap();
    let usual = checks.hard_to_read(&e, &white, 1, Vision::Normal);
    let koth = e
        .situation()
        .clone()
        .with_switch(SituationSwitch::Koth, true);
    e.set_situation(koth);
    let shown = checks.hard_to_read(&e, &white, 1, Vision::Normal);
    assert_ne!(shown, usual);
    assert_eq!(
        shown,
        Checks::default().hard_to_read(&e, &white, 1, Vision::Normal)
    );
    checks.hard_to_read(&e, &white, 1, Vision::Normal);
    assert_eq!(checks.hard_runs(), 2);
}

#[test]
fn hard_to_read_runs_again_only_when_its_inputs_change() {
    let (mut e, mut checks) = (editor(), Checks::default());
    let clip = PartKey::new("HudWeaponAmmo/AmmoInClip");
    let black = Backdrop::new(1, 1, vec![0, 0, 0, 255]).unwrap();
    let beige = Backdrop::new(1, 1, vec![235, 226, 202, 255]).unwrap();
    assert!(
        !checks
            .hard_to_read(&e, &black, 1, Vision::Normal)
            .contains(&clip)
    );
    checks.hard_to_read(&e, &black, 1, Vision::Normal);
    assert_eq!(checks.hard_runs(), 1);
    assert!(
        checks
            .hard_to_read(&e, &beige, 2, Vision::Normal)
            .contains(&clip)
    );
    checks.hard_to_read(&e, &beige, 2, Vision::Protanopia);
    e.set_screen(Screen::new(1280, 1024));
    checks.hard_to_read(&e, &beige, 2, Vision::Protanopia);
    assert_eq!(checks.hard_runs(), 4);
}
