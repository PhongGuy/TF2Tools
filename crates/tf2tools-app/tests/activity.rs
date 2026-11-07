use crate::support;

use gpui_kit::TestAppContext;
use tf2tools_app::activity;
use tf2tools_app::ids;
use tf2tools_app::services::Services;
use tf2tools_app::store_download::{DownloadEvent, DownloadTarget};
use tf2tools_app::sync::SyncSummary;
use tf2tools_core::{AddonName, GbId, StoreItemId, StoreKind};
use tf2tools_core::{LOG_KEEP, LogLevel, NewLogEntry};

fn installed(cx: &mut TestAppContext, env: &support::Env) -> Services {
    support::init(cx);
    let services = support::services(env, support::no_steam());
    support::install_services(cx, services.clone());
    services
}

#[gpui_kit::test]
fn write_appends_now(cx: &mut TestAppContext) {
    let env = support::env();
    let services = installed(cx, &env);
    activity::write(
        &services,
        &NewLogEntry::new(LogLevel::Info, "hud", "install", "Installed HUD: budhud"),
    );
    let rows = support::log_rows(&services);
    assert_eq!(rows.len(), 1);
    assert_eq!(
        (rows[0].category(), rows[0].action(), rows[0].message()),
        ("hud", "install", "Installed HUD: budhud")
    );
}

#[gpui_kit::test]
fn record_appends_off_the_ui_thread(cx: &mut TestAppContext) {
    let env = support::env();
    let services = installed(cx, &env);
    cx.update(|cx| {
        activity::record(
            cx,
            NewLogEntry::new(LogLevel::Warning, "system", "check", "Heads up").with_details("d"),
        )
    });
    support::wait_real(cx, "the row", |_| support::log_rows(&services).len() == 1);
    let row = &support::log_rows(&services)[0];
    assert_eq!(row.level(), LogLevel::Warning);
    assert_eq!(row.details(), Some("d"));
}

#[gpui_kit::test]
fn a_failed_append_neither_panics_nor_toasts(cx: &mut TestAppContext) {
    let env = support::env();
    let services = installed(cx, &env);
    let win = support::blank_window(cx);
    support::rename_table(&env, "logs", "logs_away");
    activity::write(
        &services,
        &NewLogEntry::new(LogLevel::Info, "hud", "x", "lost"),
    );
    cx.update(|cx| activity::record(cx, NewLogEntry::new(LogLevel::Info, "hud", "y", "lost")));
    cx.run_until_parked();
    assert!(!support::present(cx, win, ids::TOAST));
    support::rename_table(&env, "logs_away", "logs");
    assert!(support::log_rows(&services).is_empty());
}

#[gpui_kit::test]
fn startup_writes_its_row_then_trims_to_the_cap(cx: &mut TestAppContext) {
    let env = support::env();
    let services = installed(cx, &env);
    support::seed_log_rows(&env, LOG_KEEP + 3);
    cx.update(|cx| activity::startup(cx));
    // The append takes the table to LOG_KEEP + 4; only the trim brings it back to the cap.
    support::wait_real(cx, "startup", |_| {
        support::log_rows(&services).len() == LOG_KEEP as usize
    });
    let rows = support::log_rows(&services);
    assert_eq!(rows.len(), LOG_KEEP as usize);
    assert_eq!(
        (rows[0].category(), rows[0].action(), rows[0].message()),
        ("system", "startup", "TF2Tools started")
    );
    // The database's path is developer detail, kept out of the player's log.
    assert_eq!(rows[0].details(), None);
    // LOG_KEEP + 4 rows trimmed to LOG_KEEP drops seed rows 1 to 4.
    assert_eq!(rows.last().unwrap().message(), "row 5");
}

fn summary(fresh: bool, fetched: u32, rows: u64) -> SyncSummary {
    SyncSummary {
        fresh,
        fetched,
        rows,
    }
}

fn info(cat: &str, action: &str, msg: &str) -> NewLogEntry {
    NewLogEntry::new(LogLevel::Info, cat, action, msg)
}

#[test]
fn sync_rows_read_like_v5() {
    use activity::sync_entry;
    assert_eq!(sync_entry(StoreKind::Sounds, summary(true, 0, 500)), None);
    assert_eq!(
        sync_entry(StoreKind::CuratedHuds, summary(false, 3, 41)),
        Some(info("store", "sync", "Synced 41 curated HUDs"))
    );
    assert_eq!(
        sync_entry(StoreKind::GameBananaHuds, summary(false, 12, 900)),
        Some(info("hud_store", "sync", "Synced 12 HUDs from GameBanana"))
    );
    assert_eq!(
        sync_entry(StoreKind::Sounds, summary(false, 30, 900)),
        Some(info(
            "sound_store",
            "sync",
            "Synced 30 sounds from GameBanana"
        ))
    );
    assert_eq!(
        sync_entry(StoreKind::Crosshairs, summary(false, 5, 9)),
        Some(info(
            "crosshair_store",
            "sync",
            "Synced 5 crosshairs from GameBanana"
        ))
    );
    assert_eq!(
        sync_entry(StoreKind::Addons, summary(false, 1, 14)),
        Some(info(
            "addon_store",
            "sync",
            "Synced 14 addons from mastercomfig"
        ))
    );
}

fn target(kind: StoreKind, item: StoreItemId, name: &str) -> DownloadTarget {
    DownloadTarget {
        kind,
        item,
        name: name.into(),
    }
}

#[test]
fn download_rows_read_like_v5() {
    use activity::download_entry;
    let gb = StoreItemId::GameBanana(GbId::new(7));
    let done = |t| DownloadEvent::Finished { target: t };
    let failed = |t| DownloadEvent::Failed {
        target: t,
        message: "boom".into(),
    };
    assert_eq!(
        download_entry(&done(target(
            StoreKind::GameBananaHuds,
            gb.clone(),
            "FlawHUD"
        ))),
        info(
            "hud_store",
            "download",
            "Downloaded HUD from GameBanana: FlawHUD"
        )
        .with_details("GameBanana ID: 7")
    );
    assert_eq!(
        download_entry(&done(target(
            StoreKind::CuratedHuds,
            StoreItemId::Curated("budhud".into()),
            "budhud"
        ))),
        info("store", "download", "Downloaded HUD: budhud")
    );
    assert_eq!(
        download_entry(&done(target(StoreKind::Sounds, gb.clone(), "Ding"))),
        info(
            "sound_store",
            "download",
            "Downloaded sound from store: Ding"
        )
        .with_details("GameBanana ID: 7")
    );
    assert_eq!(
        download_entry(&failed(target(StoreKind::Crosshairs, gb.clone(), "Ring"))),
        NewLogEntry::new(
            LogLevel::Error,
            "crosshair_store",
            "download",
            "Crosshair download failed (GameBanana ID: 7)"
        )
        .with_details("boom")
    );
    assert_eq!(
        download_entry(&failed(target(
            StoreKind::CuratedHuds,
            StoreItemId::Curated("budhud".into()),
            "budhud"
        ))),
        NewLogEntry::new(
            LogLevel::Error,
            "store",
            "download",
            "HUD download failed: budhud"
        )
        .with_details("boom")
    );
    let foot = AddonName::new("mastercomfig-addon-no-footsteps.vpk").unwrap();
    assert_eq!(
        download_entry(&done(target(
            StoreKind::Addons,
            StoreItemId::Addon(foot.clone()),
            "No Footsteps"
        ))),
        info(
            "addon_store",
            "download",
            "Downloaded addon to library: No Footsteps"
        )
    );
    assert_eq!(
        download_entry(&failed(target(
            StoreKind::Addons,
            StoreItemId::Addon(foot),
            "No Footsteps"
        ))),
        NewLogEntry::new(
            LogLevel::Error,
            "addon_store",
            "download",
            "Addon download failed: No Footsteps"
        )
        .with_details("boom")
    );
}

#[gpui_kit::test]
fn a_finished_sync_writes_its_row(cx: &mut TestAppContext) {
    let env = support::env();
    let services = installed(cx, &env);
    let (hub, runs) = support::fake_hub(cx);
    cx.update(activity::watch_stores);
    cx.update(|cx| hub.update(cx, |h, cx| h.start(StoreKind::Sounds, true, cx)));
    runs.try_recv().unwrap().finish(Ok(summary(false, 30, 30)));
    support::wait_hub(cx, "the row", |_| support::log_rows(&services).len() == 1);
    assert_eq!(
        support::log_rows(&services)[0].message(),
        "Synced 30 sounds from GameBanana"
    );
}

#[gpui_kit::test]
fn a_fresh_or_failed_sync_writes_no_row(cx: &mut TestAppContext) {
    let env = support::env();
    let services = installed(cx, &env);
    let (hub, runs) = support::fake_hub(cx);
    cx.update(activity::watch_stores);
    cx.update(|cx| hub.update(cx, |h, cx| h.start(StoreKind::Sounds, false, cx)));
    runs.try_recv().unwrap().finish(Ok(summary(true, 0, 30)));
    support::wait_hub(cx, "the run", |cx| {
        cx.update(|cx| !hub.read(cx).is_running(StoreKind::Sounds))
    });
    cx.update(|cx| hub.update(cx, |h, cx| h.start(StoreKind::Addons, true, cx)));
    runs.try_recv()
        .unwrap()
        .finish(Err(support::offline_failure()));
    support::wait_hub(cx, "the run", |cx| {
        cx.update(|cx| !hub.read(cx).is_running(StoreKind::Addons))
    });
    cx.run_until_parked();
    assert!(support::log_rows(&services).is_empty());
}
