use std::sync::Arc;

use gpui_kit::{AnyWindowHandle, TestAppContext};
use tf2tools_app::app::{Boot, start};
use tf2tools_app::audio::RecordingAudio;
use tf2tools_app::ids;
use tf2tools_app::picker::ScriptedPicker;
use tf2tools_app::shell::route::Page;
use tf2tools_app::updater::{UpdateBoot, UpdateState, updater};
use tf2tools_core::Endpoints;

use crate::support;
use support::update_fake::{fake_source, offer, state, visible, wait_state};

fn boot(env: &support::Env, updates: UpdateBoot) -> Boot {
    Boot {
        data: env.data.clone(),
        assets: Ok(support::repo_asset_root()),
        picker: Arc::new(ScriptedPicker::empty()),
        detector: support::no_steam(),
        audio: Arc::new(RecordingAudio::new()),
        // Port 1 refuses at once, so nothing leaves the machine.
        endpoints: Endpoints::default()
            .with_gamebanana("http://127.0.0.1:1")
            .with_github_api("http://127.0.0.1:1")
            .with_github("http://127.0.0.1:1"),
        auto_sync: false,
        offline: false,
        font_host: None,
        updates,
    }
}

fn shell_up(cx: &mut TestAppContext, win: AnyWindowHandle) {
    support::wait_real(cx, "the shell", |cx| {
        support::present(cx, win, ids::page(Page::Huds))
    });
}

#[gpui_kit::test]
fn startup_checks_once_after_the_first_frame(cx: &mut TestAppContext) {
    let env = support::env();
    let (source, _restart, fake) = fake_source(true);
    cx.executor().allow_parking();
    support::init(cx);
    let win = cx
        .update(|cx| start(boot(&env, UpdateBoot::Source(source)), cx))
        .unwrap();
    // Tasks only, no frame yet: the updater exists but has not checked.
    support::wait_real(cx, "the updater", |cx| {
        cx.update(|cx| updater(cx).is_some())
    });
    assert!(!fake.check_pending(), "the check waits for the first frame");
    shell_up(cx, win);
    let u = cx.update(|cx| updater(cx)).unwrap();
    assert_eq!(state(cx, &u), UpdateState::Checking);
    fake.next_check(cx)
        .send(Ok(Some(offer("6.0.1", true))))
        .unwrap();
    wait_state(cx, &u, "Available", |s| {
        matches!(s, UpdateState::Available(_))
    });
    assert!(visible(cx, &u));
    assert!(support::present(cx, win, ids::UPDATE_BANNER));
    for _ in 0..3 {
        support::next_repaint(cx);
        support::settle(cx, win);
    }
    assert!(
        !fake.check_pending(),
        "one startup check, not one per frame"
    );
}

#[gpui_kit::test]
fn updates_off_installs_an_unavailable_updater_and_stays_quiet(cx: &mut TestAppContext) {
    let env = support::env();
    cx.executor().allow_parking();
    support::init(cx);
    let win = cx
        .update(|cx| start(boot(&env, UpdateBoot::Off), cx))
        .unwrap();
    shell_up(cx, win);
    for _ in 0..3 {
        support::next_repaint(cx);
        support::settle(cx, win);
    }
    let u = cx
        .update(|cx| updater(cx))
        .expect("start installs the updater");
    assert_eq!(state(cx, &u), UpdateState::Unavailable);
    assert!(!visible(cx, &u));
    assert!(!support::present(cx, win, ids::UPDATE_BANNER));
    assert!(
        support::toast_labels(cx, win)
            .iter()
            .all(|t| !t.contains("Update")),
        "startup never toasts about updates"
    );
}

// A timing run measures the app, so it never reaches for a feed, even once a key ships.
#[test]
fn a_timing_boot_turns_updates_off() {
    let env = support::env();
    let timed = boot(&env, UpdateBoot::Environment).for_timing(None);
    assert!(matches!(timed.updates, UpdateBoot::Off));
}
