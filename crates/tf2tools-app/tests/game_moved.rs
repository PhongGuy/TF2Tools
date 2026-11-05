//! A game folder moved away while the app runs: installs say so and create nothing, and the
//! header badge catches up when the player changes page or comes back to the window.

mod support;

use std::io::Write as _;
use std::path::{Path, PathBuf};

use gpui_kit::{AppContext as _, TestAppContext, VisualTestContext};
use tf2tools_app::ids;
use tf2tools_app::services::Tf2State;
use tf2tools_app::shell::route::Page;
use tf2tools_core::{Provenance, Tf2Install};

const MOVED: &str = "Failed to install HUD. TF2 wasn't found where it used to be. \
                     Check the TF2 folder in Settings.";

fn found(env: &support::Env) -> Tf2State {
    Tf2State::Found(Tf2Install::from_path(&env.game).unwrap())
}

fn hud_zip(dir: &Path, name: &str) -> PathBuf {
    let path = dir.join(format!("{name}.zip"));
    let mut zip = zip::ZipWriter::new(std::fs::File::create(&path).unwrap());
    let options = zip::write::SimpleFileOptions::default();
    zip.start_file(format!("{name}/info.vdf"), options).unwrap();
    write!(zip, "\"{name}\"\n{{\n}}\n").unwrap();
    zip.finish().unwrap();
    path
}

fn moved_to(env: &support::Env) -> PathBuf {
    env.tmp.path().join("moved")
}

fn badge(cx: &mut TestAppContext, s: &support::ShellWindow) -> Option<String> {
    support::settle(cx, s.win);
    support::label_of(cx, s.win, ids::TF2_BADGE)
}

#[gpui_kit::test]
fn installing_into_a_moved_game_says_so_and_creates_nothing(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let archive = hud_zip(env.tmp.path(), "budhud");
    s.services
        .library
        .import_hud(&archive, Provenance::Local)
        .unwrap();
    support::click_id(cx, s.win, ids::nav(Page::Settings));
    support::click_id(cx, s.win, ids::nav(Page::Huds));
    std::fs::rename(&env.game, moved_to(&env)).unwrap();
    support::click_id(cx, s.win, ids::hud_install("budhud"));
    support::wait_real(cx, "the toast", |cx| {
        support::present(cx, s.win, ids::TOAST)
    });
    assert_eq!(
        support::label_of(cx, s.win, ids::TOAST).as_deref(),
        Some(MOVED)
    );
    assert!(!env.game.exists(), "the game folder was recreated");
}

#[gpui_kit::test]
fn changing_page_rechecks_the_game_folder(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    assert_eq!(badge(cx, &s).as_deref(), Some("TF2 Connected"));
    std::fs::rename(&env.game, moved_to(&env)).unwrap();
    support::click_id(cx, s.win, ids::nav(Page::Settings));
    assert_eq!(badge(cx, &s).as_deref(), Some("TF2 Not Found"));
    std::fs::rename(moved_to(&env), &env.game).unwrap();
    support::click_id(cx, s.win, ids::nav(Page::Huds));
    assert_eq!(badge(cx, &s).as_deref(), Some("TF2 Connected"));
}

#[gpui_kit::test]
fn a_saved_folder_that_returns_after_a_relaunch_connects_again(cx: &mut TestAppContext) {
    let env = support::env();
    let saved = Tf2State::Missing {
        saved: Some(env.game.clone()),
    };
    let s = support::shell_window(cx, &env, saved);
    assert_eq!(badge(cx, &s).as_deref(), Some("TF2 Not Found"));
    support::click_id(cx, s.win, ids::nav(Page::Sounds));
    assert_eq!(badge(cx, &s).as_deref(), Some("TF2 Connected"));
}

#[gpui_kit::test]
fn coming_back_to_the_window_rechecks_the_game_folder(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    assert_eq!(badge(cx, &s).as_deref(), Some("TF2 Connected"));
    VisualTestContext::from_window(s.win, cx).deactivate_window();
    std::fs::rename(&env.game, moved_to(&env)).unwrap();
    cx.update_window(s.win, |_, w, _| w.activate_window())
        .unwrap();
    assert_eq!(badge(cx, &s).as_deref(), Some("TF2 Not Found"));
}

#[gpui_kit::test]
fn an_unsaved_missing_game_stays_not_found(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, Tf2State::Missing { saved: None });
    support::click_id(cx, s.win, ids::nav(Page::Sounds));
    assert_eq!(badge(cx, &s).as_deref(), Some("TF2 Not Found"));
}
