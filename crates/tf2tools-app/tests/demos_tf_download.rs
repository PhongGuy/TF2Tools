//! Download on a Demos.tf row: the save dialog, the cache copy, progress and failures.
use std::path::PathBuf;
use std::sync::Arc;

use gpui_kit::{TestAppContext, px, size};
use tf2tools_app::ids;
use tf2tools_app::picker::{PickKind, ScriptedPicker};
use tf2tools_core::DemoCache;

use crate::support;
use crate::support::demo_viewer::inside;
use crate::support::demostf::{
    Demo, DemosTf, demo, demos_tf_shell_picking, list_body, mount_before, mount_first, wait_end,
    wait_ids,
};
use crate::support::logstf::{Gate, mount_held, requests_to};

const FILE: &[u8] = b"a demo file";
const ROUTE: &str = "/files/7.dem";

struct Saving {
    env: support::Env,
    server: support::StoreServer,
    d: DemosTf,
    picker: Arc<ScriptedPicker>,
}

// demos.tf listing `demos` (ids 7 and up), then nothing older; `picks` queued for save dialogs.
fn served(cx: &mut TestAppContext, demos: &[Demo], picks: Vec<PathBuf>) -> Saving {
    let (env, server) = (support::env(), support::StoreServer::start());
    mount_first(&server, list_body(&server.uri(), demos));
    mount_before(&server, 7, 200, b"[]".to_vec());
    let picker = Arc::new(ScriptedPicker::new(picks));
    let d = demos_tf_shell_picking(cx, &env, &server.uri(), &[], Arc::clone(&picker));
    wait_ids(cx, &d.page, &demos.iter().map(|d| d.id).collect::<Vec<_>>());
    wait_end(cx, &d.page);
    support::settle(cx, d.s.win);
    Saving {
        env,
        server,
        d,
        picker,
    }
}

fn toast(cx: &mut TestAppContext, s: &Saving) -> String {
    support::wait_real(cx, "the toast", |cx| {
        support::present(cx, s.d.s.win, ids::TOAST)
    });
    support::label_of(cx, s.d.s.win, ids::TOAST).unwrap()
}

fn idle(cx: &mut TestAppContext, s: &Saving) {
    support::wait_real(cx, "the save to end", |cx| {
        cx.update(|cx| s.d.page.read(cx).save_progress(7).is_none())
    });
}

#[gpui_kit::test]
fn download_saves_the_demo_where_the_player_chose(cx: &mut TestAppContext) {
    let target = tempfile::tempdir().unwrap();
    let path = target.path().join("my demo.dem");
    let s = served(cx, &[demo(7)], vec![path.clone()]);
    s.server.mount_bytes(ROUTE, FILE.to_vec());
    assert_eq!(
        support::label_of(cx, s.d.s.win, ids::demos_tf_download(7)).as_deref(),
        Some("Download demo")
    );
    support::click_id(cx, s.d.s.win, ids::demos_tf_download(7));
    let said = toast(cx, &s);
    assert!(said.starts_with("Demo downloaded"), "{said}");
    assert!(said.contains("my demo.dem"), "{said}");
    assert_eq!(std::fs::read(&path).unwrap(), FILE);
    assert_eq!(s.picker.suggested(), ["match-7.dem"]);
    assert_eq!(s.picker.asked(), [PickKind::Demo]);
    idle(cx, &s);
    let cached = DemoCache::new(s.env.data.demo_cache_dir()).path_for(7);
    assert!(!cached.exists(), "a save never fills the viewer cache");
}

#[gpui_kit::test]
fn a_demo_cached_by_watch_saves_without_a_file_request(cx: &mut TestAppContext) {
    let target = tempfile::tempdir().unwrap();
    let path = target.path().join("m.dem");
    let s = served(cx, &[demo(7)], vec![path.clone()]);
    let cached = DemoCache::new(s.env.data.demo_cache_dir()).path_for(7);
    std::fs::create_dir_all(cached.parent().unwrap()).unwrap();
    std::fs::write(&cached, FILE).unwrap();
    support::click_id(cx, s.d.s.win, ids::demos_tf_download(7));
    let said = toast(cx, &s);
    assert!(said.starts_with("Demo downloaded"), "{said}");
    assert_eq!(std::fs::read(&path).unwrap(), FILE);
    assert_eq!(requests_to(&s.server, ROUTE), 0, "no file request");
}

#[gpui_kit::test]
fn a_private_demo_cannot_be_downloaded(cx: &mut TestAppContext) {
    let private = Demo {
        public: false,
        ..demo(7)
    };
    let s = served(cx, &[private], Vec::new());
    assert!(support::disabled_of(
        cx,
        s.d.s.win,
        ids::demos_tf_download(7)
    ));
}

#[gpui_kit::test]
fn a_running_save_says_so_and_is_disabled(cx: &mut TestAppContext) {
    let target = tempfile::tempdir().unwrap();
    let path = target.path().join("m.dem");
    let s = served(cx, &[demo(7)], vec![path.clone()]);
    let gate = Gate::default();
    mount_held(&s.server, &gate, ROUTE, None, FILE.to_vec());
    support::click_id(cx, s.d.s.win, ids::demos_tf_download(7));
    support::wait_real(cx, "the file request", |_| gate.arrived() == 1);
    support::settle(cx, s.d.s.win);
    assert_eq!(
        support::label_of(cx, s.d.s.win, ids::demos_tf_download(7)).as_deref(),
        Some("Downloading demo")
    );
    assert!(support::disabled_of(
        cx,
        s.d.s.win,
        ids::demos_tf_download(7)
    ));
    gate.open();
    let said = toast(cx, &s);
    assert!(said.starts_with("Demo downloaded"), "{said}");
    assert_eq!(std::fs::read(&path).unwrap(), FILE);
    assert_eq!(gate.arrived(), 1);
}

#[gpui_kit::test]
fn a_cancelled_save_requests_nothing(cx: &mut TestAppContext) {
    let s = served(cx, &[demo(7)], Vec::new());
    s.server.mount_bytes(ROUTE, FILE.to_vec());
    support::click_id(cx, s.d.s.win, ids::demos_tf_download(7));
    support::wait_real(cx, "the save picker is asked", |_| {
        !s.picker.suggested().is_empty()
    });
    support::settle(cx, s.d.s.win);
    assert!(!support::present(cx, s.d.s.win, ids::TOAST), "no toast");
    assert_eq!(requests_to(&s.server, ROUTE), 0, "nothing downloaded");
    assert_eq!(cx.update(|cx| s.d.page.read(cx).save_progress(7)), None);
}

#[gpui_kit::test]
fn a_failed_save_says_why_and_writes_nothing(cx: &mut TestAppContext) {
    let target = tempfile::tempdir().unwrap();
    let path = target.path().join("m.dem");
    let s = served(cx, &[demo(7)], vec![path.clone()]);
    s.server.mount_status(ROUTE, 404);
    support::click_id(cx, s.d.s.win, ids::demos_tf_download(7));
    let said = toast(cx, &s);
    assert!(said.starts_with("Couldn't download this demo."), "{said}");
    assert!(said.contains("demos.tf no longer has this demo."), "{said}");
    assert!(!path.exists(), "nothing is written");
    idle(cx, &s);
}

#[gpui_kit::test]
fn download_and_watch_fit_an_800_px_window(cx: &mut TestAppContext) {
    let s = served(cx, &[demo(7)], Vec::new());
    cx.simulate_window_resize(s.d.s.win, size(px(800.), px(600.)));
    support::settle(cx, s.d.s.win);
    let list = support::bounds_of(cx, s.d.s.win, ids::DEMOS_TF_LIST).unwrap();
    let length = support::bounds_of(cx, s.d.s.win, ids::demos_tf_row_duration(7)).unwrap();
    for id in [ids::demos_tf_download(7), ids::demos_tf_watch(7)] {
        let part = support::bounds_of(cx, s.d.s.win, id.clone()).unwrap();
        assert!(inside(part, list), "{id:?} at {part:?} leaves {list:?}");
    }
    let download = support::bounds_of(cx, s.d.s.win, ids::demos_tf_download(7)).unwrap();
    let watch = support::bounds_of(cx, s.d.s.win, ids::demos_tf_watch(7)).unwrap();
    assert!(
        download.left() >= length.right(),
        "Download {download:?} covers Length {length:?}"
    );
    assert!(
        watch.left() >= download.right(),
        "Watch {watch:?} covers Download {download:?}"
    );
}
