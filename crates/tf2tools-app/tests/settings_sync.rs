use std::cell::Cell;
use std::rc::Rc;

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{AnyWindowHandle, AppContext as _, Entity, ScrollDelta, TestAppContext, point, px};
use tf2tools_app::ids;
use tf2tools_app::pages::settings::{SYNC_CARDS, SettingsPage, format_synced};
use tf2tools_app::services::Tf2State;
use tf2tools_app::shell::route::Page;
use tf2tools_app::sync::{SyncHub, SyncSummary};
use tf2tools_app::sync_messages::SyncFailure;
use tf2tools_core::StoreKind;

use crate::support;
use support::ShellWindow;

const STAMP: &str = "2026-10-16T12:00:00+00:00";

fn open_settings(cx: &mut TestAppContext, s: &ShellWindow) -> Entity<SettingsPage> {
    support::click_id(cx, s.win, ids::nav(Page::Settings));
    let page = cx
        .update(|cx| s.shell.read(cx).settings_page())
        .expect("a settings page");
    support::wait_real(cx, "settings and sync status to load", |cx| {
        cx.update(|cx| page.read(cx).is_loaded() && page.read(cx).is_sync_loaded())
    });
    support::settle(cx, s.win);
    page
}

/// Scrolls the page until the store's Resync button sits mid-viewport, then clicks it.
fn click_resync(cx: &mut TestAppContext, win: AnyWindowHandle, kind: StoreKind) {
    let id = ids::settings_resync(kind);
    let page = support::bounds_of(cx, win, ids::page(Page::Settings)).expect("the settings page");
    let target = support::bounds_of(cx, win, id.clone()).expect("a resync button");
    // A negative delta moves the content up, as tests/settings.rs's click_save scrolls to Save.
    let dy = page.origin.y + page.size.height / 2. - target.origin.y;
    cx.update_window(win, |_, w, cx| {
        w.scroll(
            ids::page(Page::Settings),
            ScrollDelta::Pixels(point(px(0.), dy)),
            cx,
        )
    })
    .unwrap();
    support::click_id(cx, win, id);
}

fn status(cx: &mut TestAppContext, s: &ShellWindow, kind: StoreKind) -> String {
    support::label_of(cx, s.win, ids::settings_sync(kind)).unwrap_or_default()
}

fn stamp_synced(s: &ShellWindow, key: &str) {
    let conn = rusqlite::Connection::open(s.services.data.db_path()).unwrap();
    conn.execute(
        "INSERT OR REPLACE INTO store_cache_meta (key, value) VALUES (?1, ?2)",
        [format!("{key}_last_synced"), STAMP.to_owned()],
    )
    .unwrap();
}

fn shell(
    cx: &mut TestAppContext,
    env: &support::Env,
) -> (
    ShellWindow,
    Entity<SyncHub>,
    std::sync::mpsc::Receiver<support::FakeRun>,
) {
    let (hub, runs) = support::fake_hub(cx);
    let s = support::shell_window(cx, env, Tf2State::Missing { saved: None });
    (s, hub, runs)
}

#[test]
fn synced_times_format_like_v5() {
    assert_eq!(format_synced(None), "Never");
    assert_eq!(format_synced(Some("not a time")), "Unknown");
    let local = chrono::DateTime::parse_from_rfc3339(STAMP)
        .unwrap()
        .with_timezone(&chrono::Local)
        .format("%Y-%m-%d %H:%M")
        .to_string();
    assert_eq!(format_synced(Some(STAMP)), local);
}

#[test]
fn settings_sync_ids_carry_the_store_slug() {
    assert_eq!(
        ids::settings_sync(StoreKind::Addons),
        gpui_kit::ElementId::Name("settings-sync:addons".into())
    );
    assert_eq!(
        ids::settings_resync(StoreKind::GameBananaHuds),
        gpui_kit::ElementId::Name("settings-resync:gamebanana-huds".into())
    );
}

#[test]
fn the_sync_cards_name_their_sources_in_player_words() {
    // A repository path like owner/name means nothing to a player.
    for (_, title, description) in SYNC_CARDS {
        assert!(!description.contains('/'), "{title}: {description:?}");
    }
}

#[gpui_kit::test]
fn every_store_has_a_card_that_starts_never_synced(cx: &mut TestAppContext) {
    let env = support::env();
    let (s, _hub, _runs) = shell(cx, &env);
    open_settings(cx, &s);
    for kind in StoreKind::ALL {
        assert_eq!(
            status(cx, &s, kind),
            "0 items cached. Last synced: Never",
            "{kind:?}"
        );
        assert_eq!(
            support::label_of(cx, s.win, ids::settings_resync(kind)).as_deref(),
            Some("Resync")
        );
    }
}

#[gpui_kit::test]
fn a_stamped_store_shows_its_local_sync_time(cx: &mut TestAppContext) {
    let env = support::env();
    let (s, _hub, _runs) = shell(cx, &env);
    stamp_synced(&s, "sounds");
    open_settings(cx, &s);
    assert_eq!(
        status(cx, &s, StoreKind::Sounds),
        format!(
            "0 items cached. Last synced: {}",
            format_synced(Some(STAMP))
        )
    );
}

#[gpui_kit::test]
fn resync_forces_a_sync_and_reloads_the_status_when_it_ends(cx: &mut TestAppContext) {
    let env = support::env();
    let (s, _hub, runs) = shell(cx, &env);
    open_settings(cx, &s);
    click_resync(cx, s.win, StoreKind::Crosshairs);
    let run = runs.try_recv().expect("Resync starts a sync");
    assert_eq!((run.kind, run.force), (StoreKind::Crosshairs, true));
    let resync = ids::settings_resync(StoreKind::Crosshairs);
    assert_eq!(
        support::label_of(cx, s.win, resync.clone()).as_deref(),
        Some("Syncing...")
    );
    // The kit's Button exposes no a11y disabled flag; a second click while the store syncs
    // must still start no run, which the hub's refusal guarantees.
    click_resync(cx, s.win, StoreKind::Crosshairs);
    assert!(
        runs.try_recv().is_err(),
        "a running store's Resync starts nothing"
    );

    // The sync writes its stamp before it ends; the card must re-read it.
    stamp_synced(&s, "crosshairs");
    run.finish(Ok(SyncSummary::default()));
    let want = format!(
        "0 items cached. Last synced: {}",
        format_synced(Some(STAMP))
    );
    support::wait_hub(cx, "the card to reload", |cx| {
        status(cx, &s, StoreKind::Crosshairs) == want
    });
    assert_eq!(
        support::label_of(cx, s.win, resync).as_deref(),
        Some("Resync")
    );
}

#[gpui_kit::test]
fn resync_is_disabled_while_its_store_syncs(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    let env = support::env();
    let (s, hub, runs) = shell(cx, &env);
    open_settings(cx, &s);
    let crosshairs = ids::settings_resync(StoreKind::Crosshairs);
    let sounds = ids::settings_resync(StoreKind::Sounds);
    assert!(!support::disabled_of(cx, s.win, crosshairs.clone()));
    assert!(cx.update(|cx| hub.update(cx, |h, cx| h.start(StoreKind::Crosshairs, true, cx))));
    let run = runs.try_recv().expect("the sync starts");
    assert!(
        support::disabled_of(cx, s.win, crosshairs.clone()),
        "Resync is disabled while its store syncs"
    );
    assert!(
        !support::disabled_of(cx, s.win, sounds),
        "another store's Resync stays enabled"
    );
    run.finish(Ok(SyncSummary::default()));
    support::wait_hub(cx, "the sync to end", |cx| {
        !cx.update(|cx| hub.read(cx).is_running(StoreKind::Crosshairs))
    });
    assert!(!support::disabled_of(cx, s.win, crosshairs));
}

#[gpui_kit::test]
fn a_failure_shows_on_its_card_until_a_resync_succeeds(cx: &mut TestAppContext) {
    let env = support::env();
    let (s, hub, runs) = shell(cx, &env);
    open_settings(cx, &s);
    assert!(cx.update(|cx| hub.update(cx, |h, cx| h.start(StoreKind::Addons, false, cx))));
    runs.try_recv()
        .unwrap()
        .finish(Err(support::offline_failure()));
    support::wait_hub(cx, "the failure on the card", |cx| {
        status(cx, &s, StoreKind::Addons).ends_with(". Sync failed: Couldn't reach GameBanana. Check your internet connection and try again.")
    });
    click_resync(cx, s.win, StoreKind::Addons);
    let retry = runs.try_recv().expect("Resync starts a sync");
    assert!(
        status(cx, &s, StoreKind::Addons).ends_with(". Sync failed: Couldn't reach GameBanana. Check your internet connection and try again."),
        "the failure stays while the retry runs"
    );
    retry.finish(Ok(SyncSummary::default()));
    support::wait_hub(cx, "the failure to clear", |cx| {
        status(cx, &s, StoreKind::Addons) == "0 items cached. Last synced: Never"
    });
}

// The longest message describe_sync writes; the card must keep Resync beside it.
const DISK: &str = "Couldn't save the store data. Make sure your disk has free space and TF2Tools can write to its folder, then try again.";

#[gpui_kit::test]
fn resync_stays_in_view_beside_the_longest_failure(cx: &mut TestAppContext) {
    let env = support::env();
    let (s, hub, runs) = shell(cx, &env);
    open_settings(cx, &s);
    assert!(cx.update(|cx| hub.update(cx, |h, cx| h.start(StoreKind::Addons, false, cx))));
    runs.try_recv().unwrap().finish(Err(SyncFailure::new(
        DISK,
        "cannot read or write /data/store: No space left on device (os error 28)",
    )));
    support::wait_hub(cx, "the failure on the card", |cx| {
        status(cx, &s, StoreKind::Addons).ends_with(&format!(". Sync failed: {DISK}"))
    });
    support::settle(cx, s.win);
    let page = support::bounds_of(cx, s.win, ids::page(Page::Settings)).expect("the settings page");
    let lines = support::bounds_of(cx, s.win, ids::settings_sync(StoreKind::Addons))
        .expect("the status lines");
    let resync = support::bounds_of(cx, s.win, ids::settings_resync(StoreKind::Addons))
        .expect("a resync button");
    assert!(
        resync.right() <= page.right(),
        "{resync:?} runs past {page:?}"
    );
    assert!(
        lines.right() <= resync.left(),
        "{lines:?} runs under {resync:?}"
    );
    click_resync(cx, s.win, StoreKind::Addons);
    assert!(runs.try_recv().is_ok(), "Resync starts a sync");
}

#[gpui_kit::test]
fn the_resync_buttons_are_v5_s_32_px(cx: &mut TestAppContext) {
    let env = support::env();
    let (s, _hub, _runs) = shell(cx, &env);
    open_settings(cx, &s);
    for kind in StoreKind::ALL {
        let b = support::bounds_of(cx, s.win, ids::settings_resync(kind)).expect("drawn");
        assert_eq!(b.size.height, px(32.), "{kind:?}");
    }
}

#[gpui_kit::test]
fn a_failing_status_read_toasts_once_across_sync_events(cx: &mut TestAppContext) {
    let env = support::env();
    let (s, hub, runs) = shell(cx, &env);
    let page = open_settings(cx, &s);
    support::rename_table(&env, "store_cache_meta", "store_cache_meta_away");
    for _ in 0..3 {
        assert!(cx.update(|cx| hub.update(cx, |h, cx| h.start(StoreKind::Addons, false, cx))));
        runs.try_recv().unwrap().finish(Ok(SyncSummary::default()));
        support::wait_hub(cx, "the run to end", |cx| {
            cx.update(|cx| !hub.read(cx).is_running(StoreKind::Addons))
        });
        support::next_repaint(cx);
    }
    let _ = page;
    let failures = support::toast_labels(cx, s.win)
        .iter()
        .filter(|t| t.starts_with("Failed to read store status"))
        .count();
    assert_eq!(failures, 1);
}

#[gpui_kit::test]
fn progress_ticks_do_not_redraw_settings(cx: &mut TestAppContext) {
    let env = support::env();
    let (s, hub, runs) = shell(cx, &env);
    let page = open_settings(cx, &s);
    assert!(cx.update(|cx| hub.update(cx, |h, cx| h.start(StoreKind::Sounds, false, cx))));
    let run = runs.try_recv().unwrap();
    support::next_repaint(cx);
    let redraws = Rc::new(Cell::new(0));
    let seen = Rc::clone(&redraws);
    let _sub = cx.update(|cx| cx.observe(&page, move |_, _| seen.set(seen.get() + 1)));
    for n in 1..=5 {
        run.report(n * 100, 1000);
        support::next_repaint(cx);
    }
    assert_eq!(redraws.get(), 0, "progress alone must not redraw Settings");
    run.finish(Ok(SyncSummary::default()));
    support::wait_hub(cx, "Settings to redraw at the end", |_| redraws.get() > 0);
}
