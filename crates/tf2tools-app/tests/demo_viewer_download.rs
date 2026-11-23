//! The Demo Viewer fetching a demo's missing map from serveme.tf before showing it.

use std::time::Duration;

use gpui_kit::TestAppContext;
use tf2tools_app::ids;

use crate::support;
use crate::support::demo_view_fixtures::{BspOptions, bsp, demo_header};
use crate::support::demo_viewer::{StubAnswer, demo_file, demo_viewer_shell_served};

const MAP: &str = "cp_fetched";

// Serves the synthetic map at serveme.tf's file server after `delay`, and opens a demo on it.
fn open_missing(
    cx: &mut TestAppContext,
    env: &support::Env,
    server: &support::StoreServer,
    delay: Duration,
) -> support::demo_viewer::DemoViewerShell {
    let route = format!("/fastdl/maps/{MAP}.bsp");
    server.mount_bytes_after(&route, bsp(&BspOptions::default()), delay);
    let demo = demo_file(env, "match.dem", &demo_header(MAP, 60.0));
    let w = demo_viewer_shell_served(cx, env, server, vec![demo], StubAnswer::Frames);
    support::click_id(cx, w.s.win, ids::DEMO_VIEWER_OPEN);
    w
}

#[gpui_kit::test]
fn a_missing_map_shows_its_download_while_it_runs(cx: &mut TestAppContext) {
    let (env, server) = (support::env(), support::StoreServer::start());
    // Far longer than any wait, so a loaded machine cannot finish the download before the check.
    let w = open_missing(cx, &env, &server, Duration::from_secs(60));
    let want = format!("Downloading {MAP} from serveme.tf...");
    support::wait_real(cx, &want, |cx| {
        support::label_of(cx, w.s.win, ids::DEMO_VIEWER_STATUS).as_deref() == Some(want.as_str())
    });
    assert!(support::present(cx, w.s.win, ids::DEMO_VIEWER_PROGRESS));
    assert!(support::present(
        cx,
        w.s.win,
        ids::DEMO_VIEWER_PROGRESS_LABEL
    ));
    assert!(!support::present(cx, w.s.win, ids::DEMO_VIEWER_VIEWPORT));
}

#[gpui_kit::test]
fn a_downloaded_map_opens_and_is_kept_as_downloaded(cx: &mut TestAppContext) {
    let (env, server) = (support::env(), support::StoreServer::start());
    let w = open_missing(cx, &env, &server, Duration::from_millis(200));
    support::wait_real(cx, "the viewport", |cx| {
        support::present(cx, w.s.win, ids::DEMO_VIEWER_VIEWPORT)
    });
    assert_eq!(
        support::label_of(cx, w.s.win, ids::DEMO_VIEWER_CAPTION).as_deref(),
        Some(MAP)
    );
    assert!(!support::present(cx, w.s.win, ids::DEMO_VIEWER_PROGRESS));
    assert!(env.game.join(format!("tf/maps/{MAP}.bsp")).is_file());
    // The Maps page reads a map as TF2Tools' own, removable download from this row.
    let rows: i64 = support::sql(&env)
        .query_row(
            "SELECT count(*) FROM downloaded_maps WHERE name = ?1",
            [MAP],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(rows, 1);
}
