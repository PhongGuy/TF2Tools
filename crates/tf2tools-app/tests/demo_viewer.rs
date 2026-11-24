//! The Demo Viewer: where it sits, its Preview badge, its status, bar and Open button.

use std::sync::atomic::Ordering;

use gpui_kit::assets::IconName;
use gpui_kit::component::slider::{SliderEvent, SliderValue};
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{
    AnyWindowHandle, AppContext as _, Bounds, ElementId, InputEvent, Modifiers, MouseButton,
    MouseDownEvent, MouseMoveEvent, MouseUpEvent, Pixels, Point, TestAppContext, point, px, size,
};
use tf2tools_app::ids;
use tf2tools_app::pages::demo_viewer::BarPlace;
use tf2tools_app::picker::PickKind;
use tf2tools_app::services::Tf2State;
use tf2tools_app::shell::nav_icon;
use tf2tools_app::shell::route::Page;
use tf2tools_app::viewer::load::snapshot_time;
use tf2tools_core::{
    DemoSnapshot, DemoSnapshotError, LookSensitivity, SnapshotPlayer, Team, Tf2Install, TfClass,
};

use crate::support;
use crate::support::demo_view_fixtures::demo_header;
use crate::support::demo_viewer::{
    LockCall, NOT_A_DEMO, StubAnswer, counting_snapshots, demo_file, demo_viewer_shell,
    demo_viewer_shell_locked, demo_viewer_shell_with, inside, install_map, not_a_demo,
    open_not_a_demo,
};

#[test]
fn demos_tf_follows_logs_tf_with_its_own_icon() {
    let at = Page::ALL.iter().position(|p| *p == Page::DemosTf).unwrap();
    assert_eq!(Page::ALL[at - 1], Page::LogsTf);
    assert_eq!(Page::ALL[at + 1], Page::Settings);
    let page = Page::DemosTf;
    assert_eq!(
        (page.slug(), page.label().as_ref(), page.title().as_ref()),
        ("demos-tf", "Demos.tf", "Demos.tf")
    );
    assert_eq!(nav_icon(page), IconName::Clapperboard);
    assert!(!page.is_placeholder());
}

#[gpui_kit::test]
fn the_viewer_shows_a_preview_badge_beside_its_title(cx: &mut TestAppContext) {
    let env = support::env();
    let missing = Tf2State::Missing { saved: None };
    let w = demo_viewer_shell(
        cx,
        &env,
        missing,
        vec![not_a_demo(&env)],
        StubAnswer::Frames,
    );
    open_not_a_demo(cx, w.s.win);
    let s = &w.s;
    // The crumb stands in for the page title while a demo shows, and the badge follows it.
    assert_eq!(
        support::label_of(cx, s.win, ids::DEMOS_TF_CRUMB).as_deref(),
        Some("Back to the demo list")
    );
    assert_eq!(
        support::label_of(cx, s.win, ids::DEMO_VIEWER_PREVIEW_BADGE).as_deref(),
        Some("Preview")
    );
    let title = support::bounds_of(cx, s.win, ids::DEMOS_TF_CRUMB_NAME).unwrap();
    let badge = support::bounds_of(cx, s.win, ids::DEMO_VIEWER_PREVIEW_BADGE).unwrap();
    assert!(badge.origin.x > title.origin.x + title.size.width);
    assert_eq!(
        support::label_of(cx, s.win, ids::DEMO_VIEWER_STATUS).as_deref(),
        Some(NOT_A_DEMO)
    );
    assert_eq!(
        support::label_of(cx, s.win, ids::DEMO_VIEWER_OPEN).as_deref(),
        Some("Open demo")
    );
    assert!(!support::present(cx, s.win, ids::DEMO_VIEWER_VIEWPORT));
}

#[gpui_kit::test]
fn the_badge_shows_only_on_the_demos_tf_page(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, Tf2State::Missing { saved: None });
    assert!(!support::present(cx, s.win, ids::DEMO_VIEWER_PREVIEW_BADGE));
}

#[gpui_kit::test]
fn open_asks_for_a_demo_and_a_cancel_changes_nothing(cx: &mut TestAppContext) {
    let env = support::env();
    let missing = Tf2State::Missing { saved: None };
    let w = demo_viewer_shell(
        cx,
        &env,
        missing,
        vec![not_a_demo(&env)],
        StubAnswer::Frames,
    );
    open_not_a_demo(cx, w.s.win);
    let (s, picker) = (&w.s, &w.picker);
    support::click_id(cx, s.win, ids::DEMO_VIEWER_OPEN);
    assert_eq!(picker.asked(), [PickKind::OpenDemo, PickKind::OpenDemo]);
    assert_eq!(
        support::label_of(cx, s.win, ids::DEMO_VIEWER_STATUS).as_deref(),
        Some(NOT_A_DEMO)
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
    support::click_id(cx, w.s.win, ids::DEMOS_TF_OPEN);
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
    support::click_id(cx, w.s.win, ids::DEMOS_TF_OPEN);
    support::wait_real(cx, "the viewport", |cx| {
        support::present(cx, w.s.win, ids::DEMO_VIEWER_VIEWPORT)
    });
    assert_eq!(
        support::label_of(cx, w.s.win, ids::DEMO_VIEWER_CAPTION).as_deref(),
        Some("cp_synthetic")
    );
    assert_eq!(
        support::label_of(cx, w.s.win, ids::DEMO_VIEWER_HINT).as_deref(),
        Some("Drag to look, WASD to move, Shift to slow down.")
    );
    // The core reader finds no players in a header-only demo, so the note shows.
    assert_eq!(
        support::label_of(cx, w.s.win, ids::DEMO_VIEWER_PLAYERS_NOTE).as_deref(),
        Some("Players couldn't be read from this demo.")
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
    support::click_id(cx, w.s.win, ids::DEMOS_TF_OPEN);
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
fn a_missing_map_that_serveme_cannot_send_ends_in_a_failure(cx: &mut TestAppContext) {
    // Shell tests point serveme.tf at a dead loopback port, so the download is refused.
    cx.executor().allow_parking();
    let env = support::env();
    let path = demo_file(&env, "match.dem", &demo_header("cp_missing", 60.0));
    let w = demo_viewer_shell(cx, &env, found(&env), vec![path], StubAnswer::Frames);
    support::click_id(cx, w.s.win, ids::DEMOS_TF_OPEN);
    let want = "Couldn't download cp_missing.";
    support::wait_real(cx, "the download to fail", |cx| {
        support::label_of(cx, w.s.win, ids::DEMO_VIEWER_STATUS).as_deref() == Some(want)
    });
    assert!(support::present(cx, w.s.win, ids::DEMO_VIEWER_RETRY));
    assert!(!support::present(cx, w.s.win, ids::DEMO_VIEWER_PROGRESS));
    assert!(!support::present(cx, w.s.win, ids::DEMO_VIEWER_VIEWPORT));
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
    support::click_without_settling(cx, w.s.win, ids::DEMOS_TF_OPEN);
    support::wait_real(cx, "the viewer", |cx| {
        support::present(cx, w.s.win, ids::DEMO_VIEWER)
    });
    support::click_without_settling(cx, w.s.win, ids::DEMO_VIEWER_OPEN);
    let want = "cp_second";
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
    support::click_id(cx, w.s.win, ids::DEMOS_TF_OPEN);
    support::wait_real(cx, "the viewport", |cx| {
        support::present(cx, w.s.win, ids::DEMO_VIEWER_VIEWPORT)
    });
    let stub = w.stubs.lock().unwrap()[0].clone();
    assert!(stub.is_alive());
    support::click_nav(cx, w.s.win, Page::Huds);
    support::settle(cx, w.s.win);
    assert!(!stub.is_alive(), "the renderer outlived its page");
}

fn three_players() -> DemoSnapshot {
    let at =
        |team, x| SnapshotPlayer::new(team, TfClass::Soldier, [x, 32., 0.], 0., 0., 68., false);
    DemoSnapshot::new(
        4100,
        vec![at(Team::Blue, 0.), at(Team::Red, 20.), at(Team::Red, 40.)],
    )
}

// Opens `header` with `reader` as the snapshot reader; waits for the viewport.
fn open_with_players(
    cx: &mut TestAppContext,
    env: &support::Env,
    header: Vec<u8>,
    reader: tf2tools_app::viewer::SnapshotReader,
) -> crate::support::demo_viewer::DemoViewerShell {
    install_map(env, "cp_synthetic");
    let demo = demo_file(env, "match.dem", &header);
    let w = demo_viewer_shell_with(
        cx,
        env,
        found(env),
        vec![demo],
        StubAnswer::Frames,
        Some(reader),
    );
    support::click_id(cx, w.s.win, ids::DEMOS_TF_OPEN);
    support::wait_real(cx, "the viewport", |cx| {
        support::present(cx, w.s.win, ids::DEMO_VIEWER_VIEWPORT)
    });
    w
}

#[test]
fn the_snapshot_time_floors_to_whole_seconds() {
    assert_eq!(snapshot_time(0, 0.015), "0:00");
    assert_eq!(snapshot_time(4100, 0.015), "1:01");
    assert_eq!(snapshot_time(1668, 1925.5 / 128_370.), "0:25");
    assert_eq!(snapshot_time(66 * 605 + 33, 1. / 66.), "10:05");
}

#[gpui_kit::test]
fn players_show_with_their_count_and_time_in_the_caption(cx: &mut TestAppContext) {
    let env = support::env();
    let (reader, asked) = counting_snapshots(|| Ok(three_players()));
    // 60 s over the fixture's 3960 ticks puts tick 4100 at 62.1 s.
    let w = open_with_players(cx, &env, demo_header("cp_synthetic", 60.0), reader);
    assert_eq!(
        support::label_of(cx, w.s.win, ids::DEMO_VIEWER_CAPTION).as_deref(),
        Some("cp_synthetic, 3 players at 1:02")
    );
    assert!(!support::present(
        cx,
        w.s.win,
        ids::DEMO_VIEWER_PLAYERS_NOTE
    ));
    assert_eq!(asked.load(Ordering::SeqCst), 1);
    let boxes = w.stubs.lock().unwrap()[0].players();
    assert_eq!(boxes.len(), 1);
    assert_eq!(boxes[0].len(), 3);
}

#[gpui_kit::test]
fn an_unfinalised_demo_counts_0_015_seconds_a_tick(cx: &mut TestAppContext) {
    let env = support::env();
    let mut header = demo_header("cp_synthetic", 60.0);
    header[1060..1064].copy_from_slice(&0i32.to_le_bytes());
    let (reader, _) = counting_snapshots(|| Ok(three_players()));
    let w = open_with_players(cx, &env, header, reader);
    assert_eq!(
        support::label_of(cx, w.s.win, ids::DEMO_VIEWER_CAPTION).as_deref(),
        Some("cp_synthetic, 3 players at 1:01")
    );
}

#[gpui_kit::test]
fn unreadable_players_leave_the_map_with_a_note(cx: &mut TestAppContext) {
    let env = support::env();
    let (reader, _) =
        counting_snapshots(|| Err(DemoSnapshotError::Unreadable("bad packet".into())));
    let w = open_with_players(cx, &env, demo_header("cp_synthetic", 60.0), reader);
    assert_eq!(
        support::label_of(cx, w.s.win, ids::DEMO_VIEWER_CAPTION).as_deref(),
        Some("cp_synthetic")
    );
    assert_eq!(
        support::label_of(cx, w.s.win, ids::DEMO_VIEWER_PLAYERS_NOTE).as_deref(),
        Some("Players couldn't be read from this demo.")
    );
    let boxes = w.stubs.lock().unwrap()[0].players();
    assert!(boxes[0].is_empty());
}

#[gpui_kit::test]
fn a_map_that_fails_never_reads_the_players(cx: &mut TestAppContext) {
    let env = support::env();
    let (reader, asked) = counting_snapshots(|| Ok(three_players()));
    std::fs::create_dir_all(env.game.join("tf/maps")).unwrap();
    std::fs::write(env.game.join("tf/maps/cp_bad.bsp"), b"VBSP but not a map").unwrap();
    let demo = demo_file(&env, "match.dem", &demo_header("cp_bad", 60.0));
    let w = demo_viewer_shell_with(
        cx,
        &env,
        found(&env),
        vec![demo],
        StubAnswer::Frames,
        Some(reader),
    );
    support::click_id(cx, w.s.win, ids::DEMOS_TF_OPEN);
    let want = "This map can't be shown.";
    support::wait_real(cx, want, |cx| {
        support::label_of(cx, w.s.win, ids::DEMO_VIEWER_STATUS).as_deref() == Some(want)
    });
    assert_eq!(asked.load(Ordering::SeqCst), 0);
}

// Asserts the bar sits at the page's bottom centre: centred across it within a pixel, and its
// foot in the page's lower part without leaving it.
fn assert_bar_at_bottom_centre(page: Bounds<Pixels>, bar: Bounds<Pixels>) {
    assert!(
        bar.size.width > px(0.) && bar.size.height > px(0.),
        "{bar:?}"
    );
    let page_mid = page.left() + page.size.width / 2.;
    let bar_mid = bar.left() + bar.size.width / 2.;
    assert!((bar_mid - page_mid).abs() <= px(1.), "{bar:?} in {page:?}");
    assert!(inside(bar, page), "{bar:?} leaves {page:?}");
    assert!(
        bar.bottom() > page.top() + page.size.height * 0.75,
        "{bar:?} is not low in {page:?}"
    );
}

#[gpui_kit::test]
fn a_loaded_map_fills_the_page_under_a_bottom_bar(cx: &mut TestAppContext) {
    let env = support::env();
    install_map(&env, "cp_synthetic");
    let demo = demo_file(&env, "match.dem", &demo_header("cp_synthetic", 60.0));
    let w = demo_viewer_shell(cx, &env, found(&env), vec![demo], StubAnswer::Frames);
    support::click_id(cx, w.s.win, ids::DEMOS_TF_OPEN);
    support::wait_real(cx, "the viewport", |cx| {
        support::present(cx, w.s.win, ids::DEMO_VIEWER_VIEWPORT)
    });
    let page = support::bounds_of(cx, w.s.win, ids::page(Page::DemosTf)).unwrap();
    let viewport = support::bounds_of(cx, w.s.win, ids::DEMO_VIEWER_VIEWPORT).unwrap();
    assert_eq!(viewport, page);
    let bar = support::bounds_of(cx, w.s.win, ids::DEMO_VIEWER_BAR).unwrap();
    assert_bar_at_bottom_centre(page, bar);
    for id in [
        ids::DEMO_VIEWER_BAR_GRIP,
        ids::DEMO_VIEWER_CAPTION,
        ids::DEMO_VIEWER_PLAYERS_NOTE,
        ids::DEMO_VIEWER_OPEN,
        ids::DEMO_VIEWER_HINT,
        ids::DEMO_VIEWER_BAR_COLLAPSE,
    ] {
        let part = support::bounds_of(cx, w.s.win, id).unwrap();
        assert!(
            inside(part, bar),
            "{id} at {part:?} is outside the bar {bar:?}"
        );
    }
}

#[gpui_kit::test]
fn a_viewer_without_a_map_keeps_the_bar_and_centres_its_status(cx: &mut TestAppContext) {
    let env = support::env();
    let missing = Tf2State::Missing { saved: None };
    let w = demo_viewer_shell(
        cx,
        &env,
        missing,
        vec![not_a_demo(&env)],
        StubAnswer::Frames,
    );
    open_not_a_demo(cx, w.s.win);
    let s = &w.s;
    let page = support::bounds_of(cx, s.win, ids::page(Page::DemosTf)).unwrap();
    let bar = support::bounds_of(cx, s.win, ids::DEMO_VIEWER_BAR).unwrap();
    assert_bar_at_bottom_centre(page, bar);
    for id in [
        ids::DEMO_VIEWER_BAR_GRIP,
        ids::DEMO_VIEWER_OPEN,
        ids::DEMO_VIEWER_BAR_COLLAPSE,
    ] {
        let part = support::bounds_of(cx, s.win, id).unwrap();
        assert!(
            inside(part, bar),
            "{id} at {part:?} is outside the bar {bar:?}"
        );
    }
    assert!(!support::present(cx, s.win, ids::DEMO_VIEWER_CAPTION));
    assert!(!support::present(cx, s.win, ids::DEMO_VIEWER_HINT));
    let status = support::bounds_of(cx, s.win, ids::DEMO_VIEWER_STATUS).unwrap();
    let centre = |b: Bounds<Pixels>| (b.left() + b.size.width / 2., b.top() + b.size.height / 2.);
    let (sx, sy) = centre(status);
    let (px_, py) = centre(page);
    assert!(
        (sx - px_).abs() <= px(1.) && (sy - py).abs() <= px(1.),
        "status {status:?} is not centred in {page:?}"
    );
}

// The bar place the database holds, read back through its setting text.
fn saved_bar(s: &support::ShellWindow) -> Option<BarPlace> {
    let settings = s.services.db.settings().unwrap();
    settings.demo_viewer_bar().and_then(BarPlace::from_setting)
}

fn send(cx: &mut TestAppContext, win: AnyWindowHandle, event: impl InputEvent) {
    cx.update_window(win, |_, w, cx| {
        let _ = w.dispatch_event(event.to_platform_input(), cx);
        w.render_frame(cx);
    })
    .unwrap();
    support::settle(cx, win);
}

// Presses at `from` and moves by `by` in four steps with the left button held; `release`
// lets go there.
fn drag(cx: &mut TestAppContext, win: AnyWindowHandle, from: Point<Pixels>, by: (f32, f32)) {
    hold_drag(cx, win, from, by);
    release(cx, win, from + point(px(by.0), px(by.1)));
}

fn hold_drag(cx: &mut TestAppContext, win: AnyWindowHandle, from: Point<Pixels>, by: (f32, f32)) {
    let modifiers = Modifiers::default();
    send(
        cx,
        win,
        MouseMoveEvent {
            position: from,
            pressed_button: None,
            modifiers,
        },
    );
    send(
        cx,
        win,
        MouseDownEvent {
            button: MouseButton::Left,
            position: from,
            modifiers,
            click_count: 1,
            first_mouse: false,
        },
    );
    for step in 1..=4 {
        let t = step as f32 / 4.;
        let position = from + point(px(by.0 * t), px(by.1 * t));
        send(
            cx,
            win,
            MouseMoveEvent {
                position,
                pressed_button: Some(MouseButton::Left),
                modifiers,
            },
        );
    }
}

fn release(cx: &mut TestAppContext, win: AnyWindowHandle, at: Point<Pixels>) {
    let up = MouseUpEvent {
        button: MouseButton::Left,
        position: at,
        modifiers: Modifiers::default(),
        click_count: 1,
    };
    send(cx, win, up);
}

fn bounds(
    cx: &mut TestAppContext,
    win: AnyWindowHandle,
    id: impl Into<ElementId>,
) -> Bounds<Pixels> {
    let id = id.into();
    support::bounds_of(cx, win, id.clone()).unwrap_or_else(|| panic!("no {id}"))
}

// The bar's centre as fractions of the page, as its setting stores it.
fn fractions(page: Bounds<Pixels>, bar: Bounds<Pixels>) -> (f32, f32) {
    let c = bar.center();
    (
        (c.x - page.left()) / page.size.width,
        (c.y - page.top()) / page.size.height,
    )
}

#[gpui_kit::test]
fn dragging_the_grip_moves_the_bar_and_saves_it_once_on_release(cx: &mut TestAppContext) {
    let env = support::env();
    let missing = Tf2State::Missing { saved: None };
    let w = demo_viewer_shell(
        cx,
        &env,
        missing,
        vec![not_a_demo(&env)],
        StubAnswer::Frames,
    );
    open_not_a_demo(cx, w.s.win);
    let s = &w.s;
    let before = bounds(cx, s.win, ids::DEMO_VIEWER_BAR);
    let grip = bounds(cx, s.win, ids::DEMO_VIEWER_BAR_GRIP).center();
    hold_drag(cx, s.win, grip, (-200., -300.));
    assert_eq!(
        saved_bar(s),
        None,
        "a move saved the place before the release"
    );
    release(cx, s.win, grip + point(px(-200.), px(-300.)));
    let after = bounds(cx, s.win, ids::DEMO_VIEWER_BAR);
    let moved = after.origin - before.origin;
    assert!(
        (moved.x - px(-200.)).abs() <= px(1.) && (moved.y - px(-300.)).abs() <= px(1.),
        "{before:?} -> {after:?}"
    );
    let page = bounds(cx, s.win, ids::page(Page::DemosTf));
    let (x, y) = fractions(page, after);
    let saved = saved_bar(s).expect("a saved place");
    assert!(
        (saved.x - x).abs() < 0.002 && (saved.y - y).abs() < 0.002,
        "{saved:?} vs {x} {y}"
    );
    assert!(!saved.collapsed);
}

#[gpui_kit::test]
fn minimising_hides_the_controls_and_expanding_brings_them_back(cx: &mut TestAppContext) {
    let env = support::env();
    let missing = Tf2State::Missing { saved: None };
    let w = demo_viewer_shell(
        cx,
        &env,
        missing,
        vec![not_a_demo(&env)],
        StubAnswer::Frames,
    );
    open_not_a_demo(cx, w.s.win);
    let s = &w.s;
    support::click_id(cx, s.win, ids::DEMO_VIEWER_BAR_COLLAPSE);
    assert_eq!(saved_bar(s).map(|p| p.collapsed), Some(true));
    assert!(!support::present(cx, s.win, ids::DEMO_VIEWER_OPEN));
    assert!(support::present(cx, s.win, ids::DEMO_VIEWER_BAR_GRIP));
    assert_eq!(
        support::label_of(cx, s.win, ids::DEMO_VIEWER_BAR_COLLAPSE).as_deref(),
        Some("Show controls")
    );
    support::click_id(cx, s.win, ids::DEMO_VIEWER_BAR_COLLAPSE);
    assert_eq!(saved_bar(s).map(|p| p.collapsed), Some(false));
    assert!(support::present(cx, s.win, ids::DEMO_VIEWER_OPEN));
    assert_eq!(
        support::label_of(cx, s.win, ids::DEMO_VIEWER_BAR_COLLAPSE).as_deref(),
        Some("Minimise controls")
    );
}

#[gpui_kit::test]
fn minimising_a_loaded_bar_hides_its_caption_and_hint_until_expanded(cx: &mut TestAppContext) {
    let env = support::env();
    install_map(&env, "cp_synthetic");
    let demo = demo_file(&env, "match.dem", &demo_header("cp_synthetic", 60.0));
    let w = demo_viewer_shell(cx, &env, found(&env), vec![demo], StubAnswer::Frames);
    support::click_id(cx, w.s.win, ids::DEMOS_TF_OPEN);
    support::wait_real(cx, "the viewport", |cx| {
        support::present(cx, w.s.win, ids::DEMO_VIEWER_VIEWPORT)
    });
    let words = [
        ids::DEMO_VIEWER_CAPTION,
        ids::DEMO_VIEWER_PLAYERS_NOTE,
        ids::DEMO_VIEWER_HINT,
    ];
    support::click_id(cx, w.s.win, ids::DEMO_VIEWER_BAR_COLLAPSE);
    for id in words {
        assert!(!support::present(cx, w.s.win, id), "{id} shows minimised");
    }
    support::click_id(cx, w.s.win, ids::DEMO_VIEWER_BAR_COLLAPSE);
    for id in words {
        assert!(support::present(cx, w.s.win, id), "{id} is gone expanded");
    }
}

#[gpui_kit::test]
fn a_press_on_the_grip_that_never_moves_saves_nothing(cx: &mut TestAppContext) {
    let env = support::env();
    let missing = Tf2State::Missing { saved: None };
    let w = demo_viewer_shell(
        cx,
        &env,
        missing,
        vec![not_a_demo(&env)],
        StubAnswer::Frames,
    );
    open_not_a_demo(cx, w.s.win);
    let s = &w.s;
    let before = bounds(cx, s.win, ids::DEMO_VIEWER_BAR);
    let grip = bounds(cx, s.win, ids::DEMO_VIEWER_BAR_GRIP).center();
    let press = MouseDownEvent {
        button: MouseButton::Left,
        position: grip,
        modifiers: Modifiers::default(),
        click_count: 1,
        first_mouse: false,
    };
    send(cx, s.win, press);
    release(cx, s.win, grip);
    assert_eq!(saved_bar(s), None, "a still press saved the place");
    assert_eq!(bounds(cx, s.win, ids::DEMO_VIEWER_BAR), before);
}

fn yaw(cx: &mut TestAppContext, s: &support::ShellWindow) -> f32 {
    cx.update(|cx| {
        let page = s
            .shell
            .read(cx)
            .demo_viewer_page(cx)
            .expect("the Demo Viewer");
        let viewport = page.read(cx).viewport().expect("a viewport");
        viewport.read(cx).camera().yaw()
    })
}

#[gpui_kit::test]
fn a_drag_on_the_bar_never_turns_the_camera(cx: &mut TestAppContext) {
    let env = support::env();
    install_map(&env, "cp_synthetic");
    let demo = demo_file(&env, "match.dem", &demo_header("cp_synthetic", 60.0));
    let w = demo_viewer_shell(cx, &env, found(&env), vec![demo], StubAnswer::Frames);
    support::click_id(cx, w.s.win, ids::DEMOS_TF_OPEN);
    support::wait_real(cx, "the viewport", |cx| {
        support::present(cx, w.s.win, ids::DEMO_VIEWER_VIEWPORT)
    });
    let start = yaw(cx, &w.s);
    let caption = bounds(cx, w.s.win, ids::DEMO_VIEWER_CAPTION).center();
    drag(cx, w.s.win, caption, (100., -200.));
    assert_eq!(
        yaw(cx, &w.s),
        start,
        "a drag from the bar turned the camera"
    );
    // The same drag on the open map does turn it, so the check above can fail.
    let page = bounds(cx, w.s.win, ids::page(Page::DemosTf));
    drag(cx, w.s.win, page.center(), (100., 0.));
    assert_ne!(yaw(cx, &w.s), start);
}

#[gpui_kit::test]
fn a_bar_left_in_the_corner_stays_inside_a_shrunken_window(cx: &mut TestAppContext) {
    let env = support::env();
    let missing = Tf2State::Missing { saved: None };
    let w = demo_viewer_shell(
        cx,
        &env,
        missing,
        vec![not_a_demo(&env)],
        StubAnswer::Frames,
    );
    open_not_a_demo(cx, w.s.win);
    let s = &w.s;
    let grip = bounds(cx, s.win, ids::DEMO_VIEWER_BAR_GRIP).center();
    drag(cx, s.win, grip, (2000., 2000.));
    let page = bounds(cx, s.win, ids::page(Page::DemosTf));
    let bar = bounds(cx, s.win, ids::DEMO_VIEWER_BAR);
    assert!(
        (bar.right() - page.right()).abs() <= px(1.)
            && (bar.bottom() - page.bottom()).abs() <= px(1.),
        "the drag left {bar:?} short of {page:?}'s corner"
    );
    cx.simulate_window_resize(s.win, size(px(800.), px(600.)));
    support::settle(cx, s.win);
    let page = bounds(cx, s.win, ids::page(Page::DemosTf));
    let bar = bounds(cx, s.win, ids::DEMO_VIEWER_BAR);
    assert!(inside(bar, page), "{bar:?} leaves {page:?}");
}

#[gpui_kit::test]
fn a_saved_place_is_where_the_bar_opens(cx: &mut TestAppContext) {
    let env = support::env();
    let missing = Tf2State::Missing { saved: None };
    let w = demo_viewer_shell(
        cx,
        &env,
        missing,
        vec![not_a_demo(&env)],
        StubAnswer::Frames,
    );
    let s = &w.s;
    s.services
        .db
        .set_demo_viewer_bar("0.250 0.300 collapsed")
        .unwrap();
    open_not_a_demo(cx, s.win);
    let page = bounds(cx, s.win, ids::page(Page::DemosTf));
    let bar = bounds(cx, s.win, ids::DEMO_VIEWER_BAR);
    let (x, y) = fractions(page, bar);
    assert!(
        (x - 0.25).abs() < 0.002 && (y - 0.3).abs() < 0.002,
        "{x} {y}"
    );
    assert!(!support::present(cx, s.win, ids::DEMO_VIEWER_OPEN));
}

#[gpui_kit::test]
fn a_loaded_bar_fits_the_narrowest_window(cx: &mut TestAppContext) {
    let env = support::env();
    install_map(&env, "cp_synthetic");
    let demo = demo_file(&env, "match.dem", &demo_header("cp_synthetic", 60.0));
    let w = demo_viewer_shell(cx, &env, found(&env), vec![demo], StubAnswer::Frames);
    support::click_id(cx, w.s.win, ids::DEMOS_TF_OPEN);
    support::wait_real(cx, "the viewport", |cx| {
        support::present(cx, w.s.win, ids::DEMO_VIEWER_VIEWPORT)
    });
    // The app's minimum window size.
    cx.simulate_window_resize(w.s.win, size(px(800.), px(600.)));
    support::settle(cx, w.s.win);
    let page = bounds(cx, w.s.win, ids::page(Page::DemosTf));
    for id in [
        ids::DEMO_VIEWER_BAR,
        ids::DEMO_VIEWER_HINT,
        ids::DEMO_VIEWER_OPEN,
        ids::DEMO_VIEWER_BAR_COLLAPSE,
    ] {
        let part = bounds(cx, w.s.win, id);
        assert!(inside(part, page), "{id} at {part:?} leaves {page:?}");
    }
}

// The look sensitivity the database holds, in percent.
fn saved_look(s: &support::ShellWindow) -> u16 {
    s.services.db.settings().unwrap().look_sensitivity().get()
}

// Lets the options slider go at `percent`, as a drag on it ending there would.
fn release_look(cx: &mut TestAppContext, s: &support::ShellWindow, percent: f32) {
    let slider = cx.update(|cx| {
        let page = s
            .shell
            .read(cx)
            .demo_viewer_page(cx)
            .expect("the Demo Viewer");
        page.read(cx).look_slider().clone()
    });
    cx.update_window(s.win, |_, window, cx| {
        slider.update(cx, |st, cx| {
            st.set_value(percent, window, cx);
            cx.emit(SliderEvent::Release(SliderValue::Single(percent)));
        })
    })
    .unwrap();
    support::settle(cx, s.win);
}

// The yaw change, wrapped to [-180, 180), of a 100 px rightward drag across the map.
fn yaw_turn(cx: &mut TestAppContext, s: &support::ShellWindow) -> f32 {
    let start = yaw(cx, s);
    let page = bounds(cx, s.win, ids::page(Page::DemosTf));
    drag(cx, s.win, page.center(), (100., 0.));
    (yaw(cx, s) - start + 540.).rem_euclid(360.) - 180.
}

// Opens the next picked demo, from the list the first time and the bar after, and waits for
// its viewport.
fn open_map(cx: &mut TestAppContext, s: &support::ShellWindow) {
    let open = match support::present(cx, s.win, ids::DEMOS_TF_LIST) {
        true => ids::DEMOS_TF_OPEN,
        false => ids::DEMO_VIEWER_OPEN,
    };
    support::click_id(cx, s.win, open);
    support::wait_real(cx, "the viewport", |cx| {
        support::present(cx, s.win, ids::DEMO_VIEWER_VIEWPORT)
    });
}

#[gpui_kit::test]
fn a_stored_look_sensitivity_turns_the_first_map_by_it(cx: &mut TestAppContext) {
    let env = support::env();
    install_map(&env, "cp_synthetic");
    let demo = demo_file(&env, "match.dem", &demo_header("cp_synthetic", 60.0));
    let stored = LookSensitivity::new(200);
    let (w, _lock) = demo_viewer_shell_locked(cx, &env, found(&env), vec![demo], stored);
    open_map(cx, &w.s);
    let turn = yaw_turn(cx, &w.s);
    assert!((turn + 44.).abs() < 1e-3, "{turn}");
}

#[gpui_kit::test]
fn a_set_look_sensitivity_reaches_the_next_map_loaded(cx: &mut TestAppContext) {
    let env = support::env();
    install_map(&env, "cp_synthetic");
    let first = demo_file(&env, "first.dem", &demo_header("cp_synthetic", 60.0));
    let second = demo_file(&env, "second.dem", &demo_header("cp_synthetic", 60.0));
    let (w, _lock) = demo_viewer_shell_locked(cx, &env, found(&env), vec![first, second], None);
    open_map(cx, &w.s);
    release_look(cx, &w.s, 200.);
    let live = yaw_turn(cx, &w.s);
    assert!((live + 44.).abs() < 1e-3, "the shown map turned {live}");
    let before = w.stubs.lock().unwrap().len();
    open_map(cx, &w.s);
    support::wait_real(cx, "a second viewport", |_| {
        w.stubs.lock().unwrap().len() > before
    });
    let next = yaw_turn(cx, &w.s);
    assert!((next + 44.).abs() < 1e-3, "the next map turned {next}");
}

#[gpui_kit::test]
fn clicks_in_the_options_never_lock_the_pointer(cx: &mut TestAppContext) {
    let env = support::env();
    install_map(&env, "cp_synthetic");
    let demo = demo_file(&env, "match.dem", &demo_header("cp_synthetic", 60.0));
    let (w, lock) = demo_viewer_shell_locked(cx, &env, found(&env), vec![demo], None);
    open_map(cx, &w.s);
    support::click_id(cx, w.s.win, ids::DEMO_VIEWER_OPTIONS);
    for id in [
        ids::DEMO_VIEWER_LOOK_VALUE,
        ids::DEMO_VIEWER_LOOK_SLIDER,
        ids::DEMO_VIEWER_LOOK_RESET,
    ] {
        assert!(
            support::present(cx, w.s.win, id),
            "{id} is not in the options"
        );
        support::click_id(cx, w.s.win, id);
    }
    let locks = lock
        .calls()
        .iter()
        .filter(|c| **c == LockCall::Lock)
        .count();
    assert_eq!(locks, 0, "a click in the options reached the map");
    // A press on the map itself does lock, so the count above can fail.
    let page = bounds(cx, w.s.win, ids::page(Page::DemosTf));
    drag(cx, w.s.win, page.center(), (10., 0.));
    assert!(lock.calls().contains(&LockCall::Lock));
}

#[gpui_kit::test]
fn reset_puts_the_look_sensitivity_back_to_100_and_saves_it(cx: &mut TestAppContext) {
    let env = support::env();
    let (w, _lock) = demo_viewer_shell_locked(
        cx,
        &env,
        Tf2State::Missing { saved: None },
        vec![not_a_demo(&env)],
        LookSensitivity::new(250),
    );
    open_not_a_demo(cx, w.s.win);
    support::click_id(cx, w.s.win, ids::DEMO_VIEWER_OPTIONS);
    assert_eq!(
        support::label_of(cx, w.s.win, ids::DEMO_VIEWER_LOOK_VALUE).as_deref(),
        Some("250%")
    );
    support::click_id(cx, w.s.win, ids::DEMO_VIEWER_LOOK_RESET);
    assert_eq!(saved_look(&w.s), 100);
    assert_eq!(
        support::label_of(cx, w.s.win, ids::DEMO_VIEWER_LOOK_VALUE).as_deref(),
        Some("100%")
    );
}

#[gpui_kit::test]
fn letting_the_slider_go_saves_the_look_sensitivity(cx: &mut TestAppContext) {
    let env = support::env();
    let missing = Tf2State::Missing { saved: None };
    let (w, _lock) = demo_viewer_shell_locked(cx, &env, missing, vec![not_a_demo(&env)], None);
    open_not_a_demo(cx, w.s.win);
    support::click_id(cx, w.s.win, ids::DEMO_VIEWER_OPTIONS);
    release_look(cx, &w.s, 150.);
    assert_eq!(saved_look(&w.s), 150);
    assert_eq!(
        support::label_of(cx, w.s.win, ids::DEMO_VIEWER_LOOK_VALUE).as_deref(),
        Some("150%")
    );
}
