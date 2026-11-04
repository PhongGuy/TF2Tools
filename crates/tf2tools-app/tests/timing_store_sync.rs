mod support;

use std::sync::mpsc::Receiver;

use gpui_kit::TestAppContext;
use tf2tools_app::remote_images::RemoteImages;
use tf2tools_app::remote_images::decode::decode_image;
use tf2tools_app::remote_images::queue::IMAGE_BUDGET;
use tf2tools_app::services::Tf2State;
use tf2tools_app::sync::SyncSummary;
use tf2tools_app::timing::Case;
use tf2tools_app::timing::cases::{CaseConfig, CaseReport};
use tf2tools_app::tokio_bridge;
use tf2tools_core::StoreKind;

fn quick() -> CaseConfig {
    CaseConfig {
        intervals: 60,
        scroll_step: 120.0,
        ..CaseConfig::default()
    }
}

/// Runs the sync-scroll case while the fake sync reports progress every frame; `finish_at`
/// ends the sync that many frames after the grid has paged to its end.
fn sync_scroll(
    cx: &mut TestAppContext,
    s: &support::ShellWindow,
    runs: &Receiver<support::FakeRun>,
    finish_at: Option<usize>,
) -> (Result<CaseReport, String>, Option<support::FakeRun>) {
    let mut run: Option<support::FakeRun> = None;
    let (mut fetched, mut since_end) = (0, 0);
    let shell = s.shell.clone();
    let report =
        support::run_frame_case_with(cx, s, Case::SoundStoreSyncScroll, quick(), true, |cx| {
            if run.is_none() {
                run = runs.try_recv().ok();
            }
            let ended = cx.update(|cx| {
                shell
                    .read(cx)
                    .sound_store_page()
                    .is_some_and(|p| p.read(cx).grid().read(cx).is_end())
            });
            since_end += usize::from(ended);
            if finish_at.is_some_and(|at| since_end >= at)
                && let Some(done) = run.take()
            {
                done.finish(Ok(SyncSummary::default()));
            }
            if let Some(live) = &run {
                fetched += 1;
                live.report(fetched, 20_000);
            }
        });
    (report, run)
}

#[gpui_kit::test]
fn sync_scroll_pages_to_the_end_then_records_while_a_forced_sync_runs(cx: &mut TestAppContext) {
    let env = support::env();
    let (s, runs) = support::store_shell_window(cx, &env, |env| support::seed_n_sounds(env, 300));
    let (report, run) = sync_scroll(cx, &s, &runs, None);
    let report = report.unwrap();
    let run = run.expect("the case started a sync");
    assert_eq!(run.kind, StoreKind::Sounds);
    assert!(run.force);
    assert_eq!(support::note(&report, "cards"), "300");
    assert_eq!(support::note(&report, "sync_running"), "true");
    assert!(
        support::note(&report, "requeries")
            .parse::<usize>()
            .unwrap()
            > 0,
        "{report:?}"
    );
}

#[gpui_kit::test]
fn sync_scroll_fails_when_the_sync_ends_before_the_recording_does(cx: &mut TestAppContext) {
    let env = support::env();
    let (s, runs) = support::store_shell_window(cx, &env, |env| support::seed_n_sounds(env, 300));
    // Warming takes 30 frames after the end, so 45 lands inside the 60 recorded intervals.
    let (report, _) = sync_scroll(cx, &s, &runs, Some(45));
    let err = report.unwrap_err();
    assert!(err.contains("sync ended"), "{err}");
}

#[gpui_kit::test]
fn sync_scroll_fails_when_the_grid_never_requeries(cx: &mut TestAppContext) {
    // The sync runs but reports no progress, so nothing asks the grid to re-query.
    let env = support::env();
    let (s, runs) = support::store_shell_window(cx, &env, |env| support::seed_n_sounds(env, 300));
    let err = support::run_frame_case(cx, &s, Case::SoundStoreSyncScroll, quick()).unwrap_err();
    drop(runs);
    assert!(err.contains("no re-query"), "{err}");
}

#[gpui_kit::test]
fn sync_scroll_fails_when_no_sync_can_run(cx: &mut TestAppContext) {
    // No hub is installed, so nothing can start the sync and the case must not record.
    let env = support::env();
    tf2tools_core::Database::open(&env.data).unwrap();
    support::seed_n_sounds(&env, 100);
    let s = support::shell_window(cx, &env, Tf2State::Missing { saved: None });
    cx.executor().allow_parking();
    cx.update(|cx| {
        RemoteImages::install_with(cx, support::thumbnail_fetch(), IMAGE_BUDGET, decode_image);
        tokio_bridge::init(cx);
    });
    let err = support::run_frame_case(cx, &s, Case::SoundStoreSyncScroll, quick()).unwrap_err();
    assert!(err.contains("sync is not running"), "{err}");
}
