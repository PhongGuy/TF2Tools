//! The Demo Viewer page: where it sits, its Preview badge, its empty state and its Open button.

use std::sync::Arc;

use gpui_kit::TestAppContext;
use gpui_kit::assets::IconName;
use tf2tools_app::ids;
use tf2tools_app::picker::{PickKind, ScriptedPicker};
use tf2tools_app::services::Tf2State;
use tf2tools_app::shell::nav_icon;
use tf2tools_app::shell::route::Page;
use tf2tools_core::Tf2Install;

use crate::support;
use crate::support::demo_view_fixtures::demo_header;
use crate::support::demo_viewer::{StubAnswer, demo_file, demo_viewer_shell, install_map};

#[test]
fn the_demo_viewer_follows_logs_tf_with_its_own_icon() {
    let at = Page::ALL
        .iter()
        .position(|p| *p == Page::DemoViewer)
        .unwrap();
    assert_eq!(Page::ALL[at - 1], Page::LogsTf);
    assert_eq!(Page::ALL[at + 1], Page::Settings);
    let page = Page::DemoViewer;
    assert_eq!(
        (page.slug(), page.label().as_ref(), page.title().as_ref()),
        ("demo-viewer", "Demo Viewer", "Demo Viewer")
    );
    assert_eq!(nav_icon(page), IconName::Clapperboard);
    assert!(!page.is_placeholder());
}

#[gpui_kit::test]
fn the_page_opens_empty_with_a_preview_badge_beside_its_title(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, Tf2State::Missing { saved: None });
    support::click_nav(cx, s.win, Page::DemoViewer);
    assert_eq!(
        support::label_of(cx, s.win, ids::HEADER_TITLE).as_deref(),
        Some("Demo Viewer")
    );
    assert_eq!(
        support::label_of(cx, s.win, ids::DEMO_VIEWER_PREVIEW_BADGE).as_deref(),
        Some("Preview")
    );
    let title = support::bounds_of(cx, s.win, ids::HEADER_TITLE).unwrap();
    let badge = support::bounds_of(cx, s.win, ids::DEMO_VIEWER_PREVIEW_BADGE).unwrap();
    assert!(badge.origin.x > title.origin.x + title.size.width);
    assert_eq!(
        support::label_of(cx, s.win, ids::DEMO_VIEWER_STATUS).as_deref(),
        Some("Open a TF2 demo to see its map in 3D.")
    );
    assert_eq!(
        support::label_of(cx, s.win, ids::DEMO_VIEWER_OPEN).as_deref(),
        Some("Open demo")
    );
    assert!(!support::present(cx, s.win, ids::DEMO_VIEWER_VIEWPORT));
}

#[gpui_kit::test]
fn the_badge_shows_only_on_the_demo_viewer(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, Tf2State::Missing { saved: None });
    assert!(!support::present(cx, s.win, ids::DEMO_VIEWER_PREVIEW_BADGE));
}

#[gpui_kit::test]
fn open_asks_for_a_demo_and_a_cancel_changes_nothing(cx: &mut TestAppContext) {
    let env = support::env();
    let picker = Arc::new(ScriptedPicker::empty());
    let s = support::shell_window_with(
        cx,
        &env,
        Tf2State::Missing { saved: None },
        Arc::clone(&picker),
        support::no_steam(),
    );
    support::click_nav(cx, s.win, Page::DemoViewer);
    support::click_id(cx, s.win, ids::DEMO_VIEWER_OPEN);
    assert_eq!(picker.asked(), [PickKind::OpenDemo]);
    assert_eq!(
        support::label_of(cx, s.win, ids::DEMO_VIEWER_STATUS).as_deref(),
        Some("Open a TF2 demo to see its map in 3D.")
    );
    assert!(!support::present(cx, s.win, ids::DEMO_VIEWER_VIEWPORT));
}

fn found(env: &support::Env) -> Tf2State {
    Tf2State::Found(Tf2Install::from_path(&env.game).unwrap())
}

// Opens `demo` and waits for the status line to read `want`, with no viewport left.
fn status_after_opening(
    cx: &mut TestAppContext,
    env: &support::Env,
    state: Tf2State,
    demo: &[u8],
    answer: StubAnswer,
    want: &str,
) {
    let path = demo_file(env, "match.dem", demo);
    let w = demo_viewer_shell(cx, env, state, vec![path], answer);
    support::click_id(cx, w.s.win, ids::DEMO_VIEWER_OPEN);
    support::wait_real(cx, want, |cx| {
        support::label_of(cx, w.s.win, ids::DEMO_VIEWER_STATUS).as_deref() == Some(want)
    });
    assert!(
        !support::present(cx, w.s.win, ids::DEMO_VIEWER_VIEWPORT),
        "a failed load left a viewport"
    );
}

#[gpui_kit::test]
fn an_installed_map_opens_in_the_viewport_under_its_caption(cx: &mut TestAppContext) {
    let env = support::env();
    install_map(&env, "cp_synthetic");
    let demo = demo_file(&env, "match.dem", &demo_header("cp_synthetic", 60.0));
    let w = demo_viewer_shell(cx, &env, found(&env), vec![demo], StubAnswer::Frames);
    support::click_id(cx, w.s.win, ids::DEMO_VIEWER_OPEN);
    support::wait_real(cx, "the viewport", |cx| {
        support::present(cx, w.s.win, ids::DEMO_VIEWER_VIEWPORT)
    });
    assert_eq!(
        support::label_of(cx, w.s.win, ids::DEMO_VIEWER_CAPTION).as_deref(),
        Some("cp_synthetic. Players and playback come later.")
    );
    assert_eq!(
        support::label_of(cx, w.s.win, ids::DEMO_VIEWER_HINT).as_deref(),
        Some("Drag to look, WASD to move, Shift to slow down.")
    );
    assert!(!support::present(cx, w.s.win, ids::DEMO_VIEWER_STATUS));
    assert!(support::present(cx, w.s.win, ids::DEMO_VIEWER_OPEN));
    let stubs = w.stubs.lock().unwrap();
    assert_eq!(stubs.len(), 1);
    assert_eq!(stubs[0].scenes(), 1);
}

#[gpui_kit::test]
fn a_cancelled_second_open_keeps_the_map(cx: &mut TestAppContext) {
    let env = support::env();
    install_map(&env, "cp_synthetic");
    let demo = demo_file(&env, "match.dem", &demo_header("cp_synthetic", 60.0));
    let w = demo_viewer_shell(cx, &env, found(&env), vec![demo], StubAnswer::Frames);
    support::click_id(cx, w.s.win, ids::DEMO_VIEWER_OPEN);
    support::wait_real(cx, "the viewport", |cx| {
        support::present(cx, w.s.win, ids::DEMO_VIEWER_VIEWPORT)
    });
    support::click_id(cx, w.s.win, ids::DEMO_VIEWER_OPEN);
    assert_eq!(w.picker.asked().len(), 2);
    assert!(support::present(cx, w.s.win, ids::DEMO_VIEWER_VIEWPORT));
}

#[gpui_kit::test]
fn a_file_that_is_not_a_demo_says_so(cx: &mut TestAppContext) {
    let env = support::env();
    status_after_opening(
        cx,
        &env,
        found(&env),
        b"hello",
        StubAnswer::Frames,
        "This file isn't a TF2 demo.",
    );
}

#[gpui_kit::test]
fn a_truncated_header_reads_as_damaged(cx: &mut TestAppContext) {
    let env = support::env();
    let demo = demo_header("cp_synthetic", 60.0);
    status_after_opening(
        cx,
        &env,
        found(&env),
        &demo[..500],
        StubAnswer::Frames,
        "This demo is damaged and can't be read.",
    );
}

#[gpui_kit::test]
fn a_workshop_map_cannot_be_loaded_yet(cx: &mut TestAppContext) {
    let env = support::env();
    let demo = demo_header("workshop/cp_x.ugc123", 60.0);
    status_after_opening(
        cx,
        &env,
        found(&env),
        &demo,
        StubAnswer::Frames,
        "The map workshop/cp_x.ugc123 can't be loaded yet.",
    );
}

#[gpui_kit::test]
fn no_tf2_install_says_maps_cannot_load(cx: &mut TestAppContext) {
    let env = support::env();
    let demo = demo_header("cp_synthetic", 60.0);
    status_after_opening(
        cx,
        &env,
        Tf2State::Missing { saved: None },
        &demo,
        StubAnswer::Frames,
        "TF2 wasn't found, so its maps can't be loaded.",
    );
}

#[gpui_kit::test]
fn a_map_that_is_not_installed_names_it(cx: &mut TestAppContext) {
    let env = support::env();
    let demo = demo_header("cp_missing", 60.0);
    status_after_opening(
        cx,
        &env,
        found(&env),
        &demo,
        StubAnswer::Frames,
        "The map cp_missing isn't installed. Play it once in TF2, or get a custom map from the Maps page.",
    );
}

#[gpui_kit::test]
fn an_unreadable_map_cannot_be_shown(cx: &mut TestAppContext) {
    let env = support::env();
    std::fs::create_dir_all(env.game.join("tf/maps")).unwrap();
    std::fs::write(env.game.join("tf/maps/cp_bad.bsp"), b"VBSP but not a map").unwrap();
    let demo = demo_header("cp_bad", 60.0);
    status_after_opening(
        cx,
        &env,
        found(&env),
        &demo,
        StubAnswer::Frames,
        "This map can't be shown.",
    );
}

#[gpui_kit::test]
fn no_gpu_adapter_says_3d_is_unsupported(cx: &mut TestAppContext) {
    let env = support::env();
    install_map(&env, "cp_synthetic");
    let demo = demo_header("cp_synthetic", 60.0);
    status_after_opening(
        cx,
        &env,
        found(&env),
        &demo,
        StubAnswer::NoAdapter,
        "3D view isn't supported on this graphics setup.",
    );
}

#[gpui_kit::test]
fn a_worker_that_stops_after_no_adapter_keeps_the_unsupported_text(cx: &mut TestAppContext) {
    let env = support::env();
    install_map(&env, "cp_synthetic");
    let demo = demo_header("cp_synthetic", 60.0);
    status_after_opening(
        cx,
        &env,
        found(&env),
        &demo,
        StubAnswer::NoAdapterThenStop,
        "3D view isn't supported on this graphics setup.",
    );
}

#[gpui_kit::test]
fn a_second_open_replaces_the_load_in_flight(cx: &mut TestAppContext) {
    let env = support::env();
    install_map(&env, "cp_first");
    install_map(&env, "cp_second");
    let first = demo_file(&env, "first.dem", &demo_header("cp_first", 60.0));
    let second = demo_file(&env, "second.dem", &demo_header("cp_second", 60.0));
    let w = demo_viewer_shell(
        cx,
        &env,
        found(&env),
        vec![first, second],
        StubAnswer::Frames,
    );
    support::click_without_settling(cx, w.s.win, ids::DEMO_VIEWER_OPEN);
    support::click_without_settling(cx, w.s.win, ids::DEMO_VIEWER_OPEN);
    let want = "cp_second. Players and playback come later.";
    support::wait_real(cx, "the second map", |cx| {
        support::label_of(cx, w.s.win, ids::DEMO_VIEWER_CAPTION).as_deref() == Some(want)
    });
    for _ in 0..20 {
        support::settle(cx, w.s.win);
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    assert_eq!(
        support::label_of(cx, w.s.win, ids::DEMO_VIEWER_CAPTION).as_deref(),
        Some(want)
    );
    assert_eq!(w.picker.asked().len(), 2);
}

#[gpui_kit::test]
fn leaving_the_page_drops_its_renderer(cx: &mut TestAppContext) {
    let env = support::env();
    install_map(&env, "cp_synthetic");
    let demo = demo_file(&env, "match.dem", &demo_header("cp_synthetic", 60.0));
    let w = demo_viewer_shell(cx, &env, found(&env), vec![demo], StubAnswer::Frames);
    support::click_id(cx, w.s.win, ids::DEMO_VIEWER_OPEN);
    support::wait_real(cx, "the viewport", |cx| {
        support::present(cx, w.s.win, ids::DEMO_VIEWER_VIEWPORT)
    });
    let stub = w.stubs.lock().unwrap()[0].clone();
    assert!(stub.is_alive());
    support::click_nav(cx, w.s.win, Page::Huds);
    support::settle(cx, w.s.win);
    assert!(!stub.is_alive(), "the renderer outlived its page");
}
