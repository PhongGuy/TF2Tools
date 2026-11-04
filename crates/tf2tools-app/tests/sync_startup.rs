use std::sync::Arc;

use gpui_kit::{AnyWindowHandle, TestAppContext};
use tf2tools_app::app::{Boot, CLOSED_BASE, start};
use tf2tools_app::audio::RecordingAudio;
use tf2tools_app::ids;
use tf2tools_app::picker::ScriptedPicker;
use tf2tools_app::remote_images::RemoteImages;
use tf2tools_app::services::Services;
use tf2tools_app::shell::route::Page;
use tf2tools_app::store_download::StoreDownloads;
use tf2tools_app::sync::{SyncHub, store_runner, sync_hub};
use tf2tools_app::tokio_bridge;
use tf2tools_core::{Endpoints, ImageError, StoreKind};

mod support;

/// Port 1 refuses at once, so every real sync fails fast and never leaves the machine.
fn closed_endpoints() -> Endpoints {
    Endpoints::default()
        .with_gamebanana("http://127.0.0.1:1")
        .with_github_api("http://127.0.0.1:1")
        .with_github("http://127.0.0.1:1")
}

fn boot(env: &support::Env, auto_sync: bool) -> Boot {
    Boot {
        data: env.data.clone(),
        assets: Ok(support::repo_asset_root()),
        picker: Arc::new(ScriptedPicker::empty()),
        detector: support::no_steam(),
        audio: Arc::new(RecordingAudio::new()),
        endpoints: closed_endpoints(),
        auto_sync,
        offline: false,
        font_host: None,
        updates: tf2tools_app::updater::UpdateBoot::Off,
    }
}

fn started(cx: &mut TestAppContext, boot: Boot) -> AnyWindowHandle {
    cx.executor().allow_parking();
    support::init(cx);
    let win = cx.update(|cx| start(boot, cx)).unwrap();
    // The hub is installed with the shell, but no frame has been drawn yet: run tasks only.
    support::wait_real(cx, "the hub", |cx| cx.update(|cx| sync_hub(cx).is_some()));
    assert!(cx.update(|cx| RemoteImages::global(cx).is_some()));
    assert!(cx.update(|cx| StoreDownloads::global(cx).is_some()));
    assert!(
        !cx.update(|cx| tokio_bridge::is_running(cx)),
        "the runtime waits for the first frame"
    );
    support::wait_real(cx, "the shell", |cx| {
        support::present(cx, win, ids::page(Page::Huds))
    });
    win
}

#[gpui_kit::test]
fn startup_syncs_every_store_after_the_first_frame(cx: &mut TestAppContext) {
    let env = support::env();
    started(cx, boot(&env, true));
    support::wait_hub(cx, "every store to fail against the closed port", |cx| {
        cx.update(|cx| {
            let hub = sync_hub(cx).expect("start installs the hub");
            StoreKind::ALL
                .iter()
                .all(|k| hub.read(cx).failure(*k).is_some())
        })
    });
}

#[gpui_kit::test]
fn a_boot_without_auto_sync_starts_the_runtime_and_no_sync(cx: &mut TestAppContext) {
    let env = support::env();
    let win = started(cx, boot(&env, false));
    for _ in 0..5 {
        support::next_repaint(cx);
        support::settle(cx, win);
    }
    assert!(cx.update(|cx| tokio_bridge::is_running(cx)));
    cx.update(|cx| {
        let hub = sync_hub(cx).expect("start installs the hub");
        for kind in StoreKind::ALL {
            let hub = hub.read(cx);
            assert!(
                !hub.is_running(kind) && hub.last(kind).is_none() && hub.failure(kind).is_none(),
                "{kind:?}"
            );
        }
    });
}

#[gpui_kit::test]
fn the_real_runner_reports_a_refused_connection_as_a_failure(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    let env = support::env();
    let services = support::services(&env, support::no_steam()).with_endpoints(closed_endpoints());
    let hub = cx.update(|cx| SyncHub::install(cx, store_runner(services.store.clone())));
    assert!(cx.update(|cx| hub.update(cx, |h, cx| h.start(StoreKind::Sounds, false, cx))));
    let failure = |cx: &mut TestAppContext| {
        cx.update(|cx| hub.read(cx).failure(StoreKind::Sounds).map(str::to_owned))
    };
    support::wait_hub(cx, "the sound sync to fail", |cx| failure(cx).is_some());
    assert_eq!(
        failure(cx).as_deref(),
        Some("Couldn't reach GameBanana. Check your internet connection and try again.")
    );
}

#[test]
fn a_timing_boot_never_syncs() {
    let env = support::env();
    assert!(!boot(&env, true).for_timing(None).auto_sync);
}

#[test]
fn a_timing_boot_is_offline_on_a_closed_port_by_default() {
    let env = support::env();
    let boot = boot(&env, true).for_timing(None);
    assert!(boot.offline && !boot.auto_sync);
    assert_eq!(boot.endpoints.gamebanana(), format!("{CLOSED_BASE}/apiv11"));
}

#[test]
fn a_timing_boot_takes_a_loopback_base() {
    let env = support::env();
    let boot = boot(&env, true).for_timing(Some("http://127.0.0.1:47613"));
    assert!(boot.offline);
    assert_eq!(boot.endpoints.gamebanana(), "http://127.0.0.1:47613/apiv11");
    assert_eq!(
        boot.endpoints.image_prefixes(),
        ["http://127.0.0.1:47613/img/"]
    );
}

#[gpui_kit::test]
fn an_offline_app_refuses_remote_requests(cx: &mut TestAppContext) {
    let env = support::env();
    let _win = started(cx, boot(&env, true).offline(None));
    support::settle(cx, _win);
    let store = cx.update(|cx| cx.global::<Services>().store.clone());
    assert!(store.is_loopback_only());
    let hub = cx.update(|cx| sync_hub(cx)).unwrap();
    assert!(!cx.update(|cx| hub.read(cx).is_running(StoreKind::Sounds)));
    // The closed port refuses at once, so an image fetch fails fast instead of hanging.
    let rt = tokio::runtime::Runtime::new().unwrap();
    let result = rt.block_on(store.fetch_image(&format!("{CLOSED_BASE}/img/x.jpg")));
    assert!(result.is_err(), "{result:?}");
    // A remote image never leaves the machine: the offline image allowlist is loopback only,
    // so the fetch is refused before any request (core's store_offline covers the "offline"
    // error for a client whose allowlist still names the remote hosts).
    let remote = rt.block_on(store.fetch_image("https://images.gamebanana.com/img/ss/x.jpg"));
    assert!(
        matches!(&remote, Err(ImageError::UntrustedUrl { .. })),
        "{remote:?}"
    );
}
