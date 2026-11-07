//! The header's Play TF2 button: it starts the game through Steam, waits while TF2 already
//! runs and stays out of the way while TF2 is not found.
mod support;

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use gpui_kit::{TestAppContext, px, size};
use tf2tools_app::ids;
use tf2tools_app::pages::profiles::probe::Tf2Probe;
use tf2tools_app::services::Tf2State;
use tf2tools_app::shell::route::Page;
use tf2tools_core::Tf2Install;

fn found(env: &support::Env) -> Tf2State {
    Tf2State::Found(Tf2Install::from_path(&env.game).unwrap())
}

fn pin_running(cx: &mut TestAppContext, running: bool) {
    cx.update(|cx| Tf2Probe::install(cx, move || running));
}

#[gpui_kit::test]
fn play_starts_tf2_through_steam(cx: &mut TestAppContext) {
    let env = support::env();
    pin_running(cx, false);
    let s = support::shell_window(cx, &env, found(&env));
    assert_eq!(
        support::label_of(cx, s.win, ids::PLAY_TF2).as_deref(),
        Some("Play TF2")
    );
    assert!(!support::disabled_of(cx, s.win, ids::PLAY_TF2));
    support::click_id(cx, s.win, ids::PLAY_TF2);
    assert_eq!(cx.opened_url().as_deref(), Some("steam://run/440"));
}

#[gpui_kit::test]
fn play_waits_while_tf2_is_already_running(cx: &mut TestAppContext) {
    let env = support::env();
    pin_running(cx, true);
    let s = support::shell_window(cx, &env, found(&env));
    assert!(support::disabled_of(cx, s.win, ids::PLAY_TF2));
    support::click_id(cx, s.win, ids::PLAY_TF2);
    assert_eq!(cx.opened_url(), None);
}

#[gpui_kit::test]
fn play_is_hidden_while_tf2_is_not_found(cx: &mut TestAppContext) {
    let env = support::env();
    pin_running(cx, false);
    let s = support::shell_window(cx, &env, Tf2State::Missing { saved: None });
    assert!(!support::present(cx, s.win, ids::PLAY_TF2));
    let s = support::shell_window(cx, &env, Tf2State::Detecting);
    assert!(!support::present(cx, s.win, ids::PLAY_TF2));
}

// The header redraws often, so it reads a remembered answer; changing page asks again.
#[gpui_kit::test]
fn play_catches_up_when_the_player_changes_page(cx: &mut TestAppContext) {
    let env = support::env();
    let running = Arc::new(AtomicBool::new(false));
    let probe = running.clone();
    cx.update(|cx| Tf2Probe::install(cx, move || probe.load(Ordering::SeqCst)));
    let s = support::shell_window(cx, &env, found(&env));
    assert!(!support::disabled_of(cx, s.win, ids::PLAY_TF2));
    running.store(true, Ordering::SeqCst);
    support::click_nav(cx, s.win, Page::Sounds);
    assert!(support::disabled_of(cx, s.win, ids::PLAY_TF2));
    running.store(false, Ordering::SeqCst);
    support::click_nav(cx, s.win, Page::Huds);
    assert!(!support::disabled_of(cx, s.win, ids::PLAY_TF2));
}

#[gpui_kit::test]
fn play_fits_the_header_at_800_by_600(cx: &mut TestAppContext) {
    let env = support::env();
    pin_running(cx, false);
    let s = support::shell_window(cx, &env, found(&env));
    cx.simulate_window_resize(s.win, size(px(800.), px(600.)));
    support::settle(cx, s.win);
    let header = support::bounds_of(cx, s.win, ids::HEADER).unwrap();
    let title = support::bounds_of(cx, s.win, ids::HEADER_TITLE).unwrap();
    let play = support::bounds_of(cx, s.win, ids::PLAY_TF2).unwrap();
    let badge = support::bounds_of(cx, s.win, ids::TF2_BADGE).unwrap();
    assert!(play.left() > title.right(), "{play:?} {title:?}");
    assert!(badge.left() >= play.right(), "{badge:?} {play:?}");
    assert!(badge.right() <= header.right(), "{badge:?} {header:?}");
    assert!(play.top() >= header.top() && play.bottom() <= header.bottom());
}
