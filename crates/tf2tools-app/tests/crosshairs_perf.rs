mod support;

use gpui_kit::{Entity, TestAppContext};
use tf2tools_app::ids;
use tf2tools_app::pages::crosshairs::CrosshairsPage;
use tf2tools_app::services::Tf2State;
use tf2tools_app::shell::route::Page;
use tf2tools_core::Tf2Install;

fn open_and_wait(cx: &mut TestAppContext, s: &support::ShellWindow) -> Entity<CrosshairsPage> {
    support::click_id(cx, s.win, ids::nav(Page::Crosshairs));
    let page = cx
        .update(|cx| s.shell.read(cx).crosshairs_page())
        .expect("a Crosshairs page");
    support::wait_real(cx, "the previews", |cx| {
        cx.update(|cx| page.read(cx).timings().previews_ready.is_some())
    });
    page
}

fn shell(cx: &mut TestAppContext, env: &support::Env) -> support::ShellWindow {
    let state = Tf2State::Found(Tf2Install::from_path(&env.game).unwrap());
    support::shell_window(cx, env, state)
}

#[gpui_kit::test]
fn the_page_records_its_stages_in_order(cx: &mut TestAppContext) {
    let env = support::env();
    let s = shell(cx, &env);
    for i in 0..3 {
        support::seed_crosshair(&s.services, &env, &format!("Dot {i}"));
    }
    let page = open_and_wait(cx, &s);
    let t = cx.update(|cx| page.read(cx).timings());
    let (first, loaded, ready) = (
        t.first_render.unwrap(),
        t.loaded.unwrap(),
        t.previews_ready.unwrap(),
    );
    assert!(first <= loaded, "{t:?}");
    assert!(loaded <= ready, "{t:?}");
}

#[gpui_kit::test]
fn an_empty_library_is_ready_at_load(cx: &mut TestAppContext) {
    let env = support::env();
    let s = shell(cx, &env);
    let page = open_and_wait(cx, &s);
    let t = cx.update(|cx| page.read(cx).timings());
    assert_eq!(t.previews_ready, t.loaded);
}

/// Run: CARGO_PROFILE_DEV_DEBUG=0 env -u WAYLAND_DISPLAY -u DISPLAY cargo test -p tf2tools-app
///      --test crosshairs_perf -- --ignored --nocapture
#[gpui_kit::test]
#[ignore = "timing probe; run with --ignored --nocapture"]
fn probe_the_page_on_400_library_crosshairs(cx: &mut TestAppContext) {
    let env = support::env();
    let s = shell(cx, &env);
    for i in 0..400 {
        support::seed_crosshair(&s.services, &env, &format!("Probe {i:03}"));
    }
    let page = open_and_wait(cx, &s);
    let t = cx.update(|cx| page.read(cx).timings());
    eprintln!(
        "crosshairs probe (400 library, {} builtins): first render {:?}, loaded {:?}, previews ready {:?}",
        s.services.builtins.len(),
        t.first_render.unwrap(),
        t.loaded.unwrap(),
        t.previews_ready.unwrap()
    );
}
