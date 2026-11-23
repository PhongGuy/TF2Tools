//! The Demo Viewer page: where it sits, its Preview badge, its empty state and its Open button.

use std::sync::Arc;
use std::sync::atomic::Ordering;

use gpui_kit::assets::IconName;
use gpui_kit::{Bounds, Pixels, TestAppContext, px};
use tf2tools_app::ids;
use tf2tools_app::picker::{PickKind, ScriptedPicker};
use tf2tools_app::services::Tf2State;
use tf2tools_app::shell::nav_icon;
use tf2tools_app::shell::route::Page;
use tf2tools_app::viewer::load::snapshot_time;
use tf2tools_core::{DemoSnapshot, DemoSnapshotError, SnapshotPlayer, Team, Tf2Install, TfClass};

use crate::support;
use crate::support::demo_view_fixtures::demo_header;
use crate::support::demo_viewer::{
    StubAnswer, counting_snapshots, demo_file, demo_viewer_shell, demo_viewer_shell_with,
    install_map,
};

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
    support::click_id(cx, w.s.win, ids::DEMO_VIEWER_OPEN);
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
    let demo = demo_file(&env, "match.dem", &demo_header("cp_missing", 60.0));
    let w = demo_viewer_shell_with(
        cx,
        &env,
        found(&env),
        vec![demo],
        StubAnswer::Frames,
        Some(reader),
    );
    support::click_id(cx, w.s.win, ids::DEMO_VIEWER_OPEN);
    let want = "The map cp_missing isn't installed. Play it once in TF2, or get a custom map from the Maps page.";
    support::wait_real(cx, want, |cx| {
        support::label_of(cx, w.s.win, ids::DEMO_VIEWER_STATUS).as_deref() == Some(want)
    });
    assert_eq!(asked.load(Ordering::SeqCst), 0);
}

// Whether `inner` lies wholly inside `outer`, give or take half a pixel of rounding.
fn inside(inner: Bounds<Pixels>, outer: Bounds<Pixels>) -> bool {
    let slack = px(0.5);
    inner.left() >= outer.left() - slack
        && inner.top() >= outer.top() - slack
        && inner.right() <= outer.right() + slack
        && inner.bottom() <= outer.bottom() + slack
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
    support::click_id(cx, w.s.win, ids::DEMO_VIEWER_OPEN);
    support::wait_real(cx, "the viewport", |cx| {
        support::present(cx, w.s.win, ids::DEMO_VIEWER_VIEWPORT)
    });
    let page = support::bounds_of(cx, w.s.win, ids::page(Page::DemoViewer)).unwrap();
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
fn the_empty_page_keeps_the_bar_and_centres_its_status(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, Tf2State::Missing { saved: None });
    support::click_nav(cx, s.win, Page::DemoViewer);
    let page = support::bounds_of(cx, s.win, ids::page(Page::DemoViewer)).unwrap();
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
