//! The background trim job: its first pass, a pass after each sync and on a new keep choice,
//! and the logs.tf page rereading once a pass lands.

use gpui_kit::{Entity, TestAppContext};
use tf2tools_app::logs_trim::LogsTrim;
use tf2tools_app::services::Tf2State;
use tf2tools_core::{KeepFullLogs, LogState, LogsCache};

use crate::support;
use crate::support::logstf::{ARIE, open_page, serve_two_logs, shell};

fn trim_job(cx: &mut TestAppContext) -> Entity<LogsTrim> {
    cx.update(LogsTrim::global)
}

// Waits for the job to go idle, then returns how many passes have landed.
fn settled_passes(cx: &mut TestAppContext, trim: &Entity<LogsTrim>) -> u64 {
    support::wait_real(cx, "the trim job to go idle", |cx| {
        cx.update(|cx| !trim.read(cx).is_running())
    });
    cx.update(|cx| trim.read(cx).passes())
}

fn wait_pass_after(cx: &mut TestAppContext, trim: &Entity<LogsTrim>, before: u64) {
    support::wait_real(cx, "another trim pass", |cx| {
        cx.update(|cx| trim.read(cx).passes() > before && !trim.read(cx).is_running())
    });
}

#[gpui_kit::test]
fn the_first_use_runs_a_pass_that_measures_the_cache(cx: &mut TestAppContext) {
    let env = support::env();
    let _s = support::shell_window(cx, &env, Tf2State::Missing { saved: None });
    let trim = trim_job(cx);
    wait_pass_after(cx, &trim, 0);
    let (keep, full_logs) = cx.update(|cx| {
        let trim = trim.read(cx);
        (trim.keep(), trim.space().map(|space| space.full_logs()))
    });
    assert_eq!(keep, Some(KeepFullLogs::Days7));
    assert_eq!(full_logs, Some(0));
}

#[gpui_kit::test]
fn a_finished_sync_runs_a_pass(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    serve_two_logs(&server);
    let s = shell(cx, &env, &server.uri(), &[(ARIE, "Arie")]);
    let trim = trim_job(cx);
    let before = settled_passes(cx, &trim);
    let _page = open_page(cx, &s);
    wait_pass_after(cx, &trim, before);
}

#[gpui_kit::test]
fn a_pass_trims_old_logs_and_the_page_shows_them_trimmed(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    serve_two_logs(&server);
    let s = shell(cx, &env, &server.uri(), &[(ARIE, "Arie")]);
    let trim = trim_job(cx);
    let page = open_page(cx, &s);
    // As if the sync ran a month ago: upload and fetch both move back thirty days.
    support::sql(&env)
        .execute_batch(
            "UPDATE logstf_logs SET date = date - 2592000, fetched_at = fetched_at - 2592000",
        )
        .unwrap();
    let before = settled_passes(cx, &trim);
    trim.update(cx, |trim, cx| trim.request(cx));
    wait_pass_after(cx, &trim, before);
    support::wait_real(cx, "the page to reread", |cx| {
        cx.update(|cx| {
            let rows = page.read(cx).rows();
            rows.len() == 2 && rows.iter().all(|r| r.state() == LogState::Trimmed)
        })
    });
    let full_logs = cx.update(|cx| trim.read(cx).space().map(|space| space.full_logs()));
    assert_eq!(full_logs, Some(0));
}

#[gpui_kit::test]
fn a_new_keep_choice_is_taken_at_once_saved_and_applied(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, Tf2State::Missing { saved: None });
    let trim = trim_job(cx);
    let before = settled_passes(cx, &trim);
    trim.update(cx, |trim, cx| trim.set_keep(KeepFullLogs::Days30, cx));
    assert_eq!(
        cx.update(|cx| trim.read(cx).keep()),
        Some(KeepFullLogs::Days30)
    );
    wait_pass_after(cx, &trim, before);
    let saved = LogsCache::new(s.services.db.clone())
        .keep_full_logs()
        .unwrap();
    assert_eq!(saved, KeepFullLogs::Days30);
}

#[gpui_kit::test]
fn a_keep_choice_whose_save_fails_is_saved_by_the_next_pass(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, Tf2State::Missing { saved: None });
    let trim = trim_job(cx);
    let before = settled_passes(cx, &trim);
    // A full disk refuses the keep choice's write for one pass.
    support::sql(&env)
        .execute_batch(
            "CREATE TRIGGER no_keep BEFORE INSERT ON settings
             WHEN NEW.key = 'logstf_keep_full_logs'
             BEGIN SELECT RAISE(ABORT, 'disk full'); END;",
        )
        .unwrap();
    trim.update(cx, |trim, cx| trim.set_keep(KeepFullLogs::Days30, cx));
    wait_pass_after(cx, &trim, before);
    let failed = cx.update(|cx| trim.read(cx).passes());
    support::sql(&env)
        .execute_batch("DROP TRIGGER no_keep")
        .unwrap();
    trim.update(cx, |trim, cx| trim.request(cx));
    wait_pass_after(cx, &trim, failed);
    assert_eq!(
        cx.update(|cx| trim.read(cx).keep()),
        Some(KeepFullLogs::Days30)
    );
    let saved = LogsCache::new(s.services.db.clone())
        .keep_full_logs()
        .unwrap();
    assert_eq!(saved, KeepFullLogs::Days30);
}
