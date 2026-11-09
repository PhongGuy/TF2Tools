use crate::support;

use gpui_kit::component::select::SelectEvent;
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{AppContext as _, Entity, TestAppContext, px};
use tf2tools_app::ids;
use tf2tools_app::pages::logs::LogsPage;
use tf2tools_app::services::Tf2State;
use tf2tools_app::shell::route::Page;
use tf2tools_core::{LogLevel, NewLogEntry};

fn missing() -> Tf2State {
    Tf2State::Missing { saved: None }
}

fn append(s: &support::ShellWindow, level: LogLevel, cat: &str, action: &str, msg: &str) -> i64 {
    s.services
        .activity
        .append(&NewLogEntry::new(level, cat, action, msg))
        .unwrap();
    support::log_rows(&s.services)[0].id()
}

/// Opens the Logs page through the sidebar and waits for its first load.
fn open_logs(cx: &mut TestAppContext, s: &support::ShellWindow) -> Entity<LogsPage> {
    support::click_id(cx, s.win, ids::nav(Page::Logs));
    let page = cx
        .update(|cx| s.shell.read(cx).logs_page())
        .expect("a Logs page");
    support::wait_real(cx, "logs to load", |cx| {
        cx.update(|cx| page.read(cx).is_loaded())
    });
    support::settle(cx, s.win);
    page
}

fn shown(cx: &mut TestAppContext, page: &Entity<LogsPage>) -> Vec<String> {
    cx.update(|cx| {
        page.read(cx)
            .entries()
            .iter()
            .map(|e| e.message().to_owned())
            .collect()
    })
}

fn filter(
    cx: &mut TestAppContext,
    page: &Entity<LogsPage>,
    category: Option<&'static str>,
    level: Option<LogLevel>,
) {
    cx.update(|cx| {
        page.update(cx, |p, cx| {
            p.set_category(category, cx);
            p.set_level(level, cx);
        })
    });
    support::wait_real(cx, "the filtered load", |cx| {
        cx.update(|cx| page.read(cx).is_loaded())
    });
}

#[gpui_kit::test]
fn logs_is_a_real_page(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, missing());
    let _page = open_logs(cx, &s);
    assert!(!Page::Logs.is_placeholder());
    assert!(support::present(cx, s.win, ids::page(Page::Logs)));
    assert!(!support::present(cx, s.win, ids::PLACEHOLDER_MESSAGE));
    assert_eq!(
        support::label_of(cx, s.win, ids::HEADER_TITLE).as_deref(),
        Some("Activity Log")
    );
    support::assert_starts_under_top_bar(cx, s.win, ids::page(Page::Logs), ids::LOGS_CATEGORY);
    assert!(support::present(cx, s.win, ids::LOGS_CATEGORY));
    assert!(support::present(cx, s.win, ids::LOGS_LEVEL));
}

#[gpui_kit::test]
fn an_empty_log_says_so(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, missing());
    let _page = open_logs(cx, &s);
    assert_eq!(
        support::label_of(cx, s.win, ids::LOGS_EMPTY).as_deref(),
        Some("No log entries found")
    );
    assert!(!support::present(cx, s.win, ids::LOGS_MORE));
}

#[gpui_kit::test]
fn the_empty_state_stays_while_a_reload_runs(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, missing());
    let page = open_logs(cx, &s);
    assert!(support::present(cx, s.win, ids::LOGS_EMPTY));
    cx.update(|cx| page.update(cx, |p, cx| p.set_category(Some("hud"), cx)));
    assert!(
        !cx.update(|cx| page.read(cx).is_loaded()),
        "the reload is still running"
    );
    // One frame with the reload still off the UI thread: no background work runs here.
    let empty = cx
        .update_window(s.win, |_, w, cx| {
            w.render_frame(cx);
            w.try_find(ids::LOGS_EMPTY).is_some()
        })
        .unwrap();
    assert!(empty, "the empty state flipped to a blank list");
    support::wait_real(cx, "the reload", |cx| {
        cx.update(|cx| page.read(cx).is_loaded())
    });
    assert!(support::present(cx, s.win, ids::LOGS_EMPTY));
}

#[gpui_kit::test]
fn each_row_is_a_card_newest_first(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, missing());
    append(
        &s,
        LogLevel::Info,
        "hud",
        "install",
        "Installed HUD: budhud",
    );
    let id = append(
        &s,
        LogLevel::Error,
        "hud_store",
        "download",
        "HUD download failed (GameBanana ID: 7)",
    );
    let page = open_logs(cx, &s);
    assert_eq!(
        shown(cx, &page),
        [
            "HUD download failed (GameBanana ID: 7)",
            "Installed HUD: budhud"
        ]
    );
    assert_eq!(
        support::label_of(cx, s.win, ids::log_level(id)).as_deref(),
        Some("error")
    );
    assert_eq!(
        support::label_of(cx, s.win, ids::log_time(id)).as_deref(),
        Some("Just now")
    );
    assert_eq!(
        support::label_of(cx, s.win, ids::log_category(id)).as_deref(),
        Some("HUD Store")
    );
    assert_eq!(
        support::label_of(cx, s.win, ids::log_action(id)).as_deref(),
        Some("Download")
    );
    assert_eq!(
        support::label_of(cx, s.win, ids::log_message(id)).as_deref(),
        Some("HUD download failed (GameBanana ID: 7)")
    );
    assert!(!support::present(cx, s.win, ids::log_details_toggle(id)));
}

#[gpui_kit::test]
fn filters_narrow_by_category_with_its_store_and_by_level(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, missing());
    append(&s, LogLevel::Info, "hud", "install", "a");
    append(&s, LogLevel::Error, "hud_store", "download", "b");
    append(&s, LogLevel::Info, "sound", "install", "c");
    append(&s, LogLevel::Info, "addon_store", "sync", "d");
    let page = open_logs(cx, &s);
    filter(cx, &page, Some("hud"), None);
    assert_eq!(shown(cx, &page), ["b", "a"]);
    filter(cx, &page, Some("hud"), Some(LogLevel::Error));
    assert_eq!(shown(cx, &page), ["b"]);
    filter(cx, &page, Some("addon"), None);
    assert_eq!(shown(cx, &page), ["d"]);
    filter(cx, &page, None, None);
    assert_eq!(shown(cx, &page), ["d", "c", "b", "a"]);
}

#[gpui_kit::test]
fn the_selects_drive_the_filters(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, missing());
    append(&s, LogLevel::Info, "hud", "install", "a");
    append(&s, LogLevel::Warning, "sound", "install", "c");
    let page = open_logs(cx, &s);
    let (cat, level) = cx.update(|cx| {
        let p = page.read(cx);
        (p.category_select().clone(), p.level_select().clone())
    });
    cx.update(|cx| cat.update(cx, |_, cx| cx.emit(SelectEvent::Confirm(Some("Sounds")))));
    support::wait_real(cx, "the load", |cx| {
        cx.update(|cx| page.read(cx).is_loaded())
    });
    assert_eq!(cx.update(|cx| page.read(cx).category()), Some("sound"));
    assert_eq!(shown(cx, &page), ["c"]);
    cx.update(|cx| level.update(cx, |_, cx| cx.emit(SelectEvent::Confirm(Some("Info")))));
    support::wait_real(cx, "the load", |cx| {
        cx.update(|cx| page.read(cx).is_loaded())
    });
    assert_eq!(cx.update(|cx| page.read(cx).level()), Some(LogLevel::Info));
    assert!(shown(cx, &page).is_empty());
}

#[gpui_kit::test]
fn a_filter_change_drops_the_stale_load(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, missing());
    append(&s, LogLevel::Info, "hud", "install", "a");
    append(&s, LogLevel::Info, "sound", "install", "c");
    let page = open_logs(cx, &s);
    // Two changes before either load lands: only the second may show.
    cx.update(|cx| {
        page.update(cx, |p, cx| {
            p.set_category(Some("hud"), cx);
            p.set_category(Some("sound"), cx);
        })
    });
    support::wait_real(cx, "the load", |cx| {
        cx.update(|cx| page.read(cx).is_loaded())
    });
    support::settle(cx, s.win);
    assert_eq!(shown(cx, &page), ["c"]);
}

#[gpui_kit::test]
fn a_failed_query_shows_an_inline_error_with_retry(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, missing());
    append(&s, LogLevel::Info, "hud", "install", "a");
    support::rename_table(&env, "logs", "logs_away");
    let page = open_logs(cx, &s);
    let error = support::label_of(cx, s.win, ids::LOGS_ERROR).unwrap();
    assert_eq!(
        error,
        "TF2Tools couldn't open the activity log. Restart TF2Tools and try again."
    );
    assert!(!support::present(cx, s.win, ids::LOGS_EMPTY));
    support::rename_table(&env, "logs_away", "logs");
    support::click_id(cx, s.win, ids::LOGS_RETRY);
    support::wait_real(cx, "the retry", |cx| {
        cx.update(|cx| page.read(cx).entries().len() == 1)
    });
    assert!(!support::present(cx, s.win, ids::LOGS_ERROR));
}

#[gpui_kit::test]
fn a_revisit_requeries(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, missing());
    let _page = open_logs(cx, &s);
    append(&s, LogLevel::Info, "settings", "update", "Settings updated");
    support::click_id(cx, s.win, ids::nav(Page::Settings));
    let page = open_logs(cx, &s);
    assert_eq!(shown(cx, &page), ["Settings updated"]);
}

#[gpui_kit::test]
fn details_expand_and_collapse(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, missing());
    append(&s, LogLevel::Info, "sound", "install", "below");
    s.services
        .activity
        .append(
            &NewLogEntry::new(
                LogLevel::Error,
                "store",
                "download",
                "HUD download failed (ID: x)",
            )
            .with_details("line one\nline two\nline three"),
        )
        .unwrap();
    let rows = support::log_rows(&s.services);
    let (id, below) = (rows[0].id(), rows[1].id());
    let page = open_logs(cx, &s);
    assert_eq!(
        support::label_of(cx, s.win, ids::log_details_toggle(id)).as_deref(),
        Some("Details")
    );
    assert!(!support::present(cx, s.win, ids::log_details(id)));
    let before = support::bounds_of(cx, s.win, ids::log_row(below)).unwrap();
    support::click_id(cx, s.win, ids::log_details_toggle(id));
    assert!(cx.update(|cx| page.read(cx).is_expanded(id)));
    assert_eq!(
        support::label_of(cx, s.win, ids::log_details(id)).as_deref(),
        Some("line one\nline two\nline three")
    );
    let after = support::bounds_of(cx, s.win, ids::log_row(below)).unwrap();
    assert!(
        after.origin.y > before.origin.y + px(20.),
        "{before:?} -> {after:?}"
    );
    support::click_id(cx, s.win, ids::log_details_toggle(id));
    assert!(!support::present(cx, s.win, ids::log_details(id)));
}

#[gpui_kit::test]
fn load_more_pages_by_keyset(cx: &mut TestAppContext) {
    let env = support::env();
    support::seed_log_rows(&env, 150);
    let s = support::shell_window(cx, &env, missing());
    let page = open_logs(cx, &s);
    assert_eq!(shown(cx, &page).len(), 100);
    assert_eq!(shown(cx, &page)[0], "row 150");
    assert!(cx.update(|cx| page.read(cx).has_more()));
    let state = cx.update(|cx| page.read(cx).list_state().clone());
    state.scroll_to_end();
    support::click_id(cx, s.win, ids::LOGS_MORE);
    support::wait_real(cx, "the next page", |cx| {
        cx.update(|cx| page.read(cx).entries().len() == 150)
    });
    let shown = shown(cx, &page);
    assert_eq!(
        (shown[99].as_str(), shown[100].as_str()),
        ("row 51", "row 50")
    );
    assert_eq!(shown[149], "row 1");
    assert!(!cx.update(|cx| page.read(cx).has_more()));
    assert!(!support::present(cx, s.win, ids::LOGS_MORE));
}

#[gpui_kit::test]
fn load_more_waits_for_a_filter_reload(cx: &mut TestAppContext) {
    let env = support::env();
    let db = tf2tools_core::Database::open(&env.data).unwrap();
    let sound = NewLogEntry::new(LogLevel::Info, "sound", "install", "c");
    tf2tools_core::ActivityLog::new(db).append(&sound).unwrap();
    support::seed_log_rows(&env, 150);
    let s = support::shell_window(cx, &env, missing());
    let page = open_logs(cx, &s);
    // A Load more click that lands while the filter reload runs must not page the new filter
    // below the old filter's rows.
    cx.update(|cx| {
        page.update(cx, |p, cx| {
            p.set_category(Some("sound"), cx);
            p.load_more(cx);
        })
    });
    support::wait_real(cx, "a load to land", |cx| {
        cx.update(|cx| page.read(cx).is_loaded() || page.read(cx).entries().len() != 100)
    });
    support::settle(cx, s.win);
    assert_eq!(shown(cx, &page), ["c"]);
}

#[gpui_kit::test]
fn exactly_one_page_offers_more_and_finds_none(cx: &mut TestAppContext) {
    let env = support::env();
    support::seed_log_rows(&env, 100);
    let s = support::shell_window(cx, &env, missing());
    let page = open_logs(cx, &s);
    assert!(cx.update(|cx| page.read(cx).has_more()));
    cx.update(|cx| page.update(cx, |p, cx| p.load_more(cx)));
    support::wait_real(cx, "the empty page", |cx| {
        cx.update(|cx| !page.read(cx).has_more())
    });
    assert_eq!(shown(cx, &page).len(), 100);
}

#[gpui_kit::test]
fn clear_all_asks_then_empties(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, missing());
    append(&s, LogLevel::Info, "hud", "install", "a");
    let page = open_logs(cx, &s);
    support::click_id(cx, s.win, ids::LOGS_CLEAR);
    assert_eq!(
        support::label_of(cx, s.win, ids::CONFIRM_TITLE).as_deref(),
        Some("Clear All Logs")
    );
    support::click_id(cx, s.win, ids::CONFIRM_OK);
    support::wait_real(cx, "the toast", |cx| {
        support::present(cx, s.win, ids::TOAST)
    });
    assert_eq!(
        support::label_of(cx, s.win, ids::TOAST).as_deref(),
        Some("Logs cleared")
    );
    assert!(support::log_rows(&s.services).is_empty());
    support::wait_real(cx, "the reload", |cx| {
        cx.update(|cx| page.read(cx).entries().is_empty())
    });
    assert!(support::present(cx, s.win, ids::LOGS_EMPTY));
}

#[gpui_kit::test]
fn cancelling_clear_keeps_the_rows(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, missing());
    append(&s, LogLevel::Info, "hud", "install", "a");
    let page = open_logs(cx, &s);
    support::click_id(cx, s.win, ids::LOGS_CLEAR);
    support::click_id(cx, s.win, ids::CONFIRM_CANCEL);
    assert_eq!(support::log_rows(&s.services).len(), 1);
    assert_eq!(shown(cx, &page), ["a"]);
}

#[gpui_kit::test]
fn clear_and_load_more_are_v5_s_32_px(cx: &mut TestAppContext) {
    let env = support::env();
    support::seed_log_rows(&env, 150);
    let s = support::shell_window(cx, &env, missing());
    let page = open_logs(cx, &s);
    let state = cx.update(|cx| page.read(cx).list_state().clone());
    state.scroll_to_end();
    for id in [ids::LOGS_CLEAR, ids::LOGS_MORE] {
        let b = support::bounds_of(cx, s.win, id).expect(id);
        assert_eq!(b.size.height, px(32.), "{id}");
    }
}

#[gpui_kit::test]
// A status badge keeps its full text in a crowded row; only store card tags ellipsise.
fn the_category_badge_keeps_its_width_in_a_narrow_window(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, missing());
    let id = append(
        &s,
        LogLevel::Info,
        "hud_store",
        "download_of_a_long_unbroken_action_name_that_crowds_the_row",
        "m",
    );
    let _page = open_logs(cx, &s);
    let wide = support::bounds_of(cx, s.win, ids::log_category(id)).unwrap();
    cx.simulate_window_resize(s.win, gpui_kit::size(px(640.), px(900.)));
    support::settle(cx, s.win);
    let narrow = support::bounds_of(cx, s.win, ids::log_category(id)).unwrap();
    assert_eq!(narrow.size.width, wide.size.width, "{narrow:?} vs {wide:?}");
}

// v5 draws details as `<pre class="mt-2 max-h-48 rounded p-2 text-xs">`: 8 px under the
// toggle, 16 px lines inside 8 px padding, and a 192 px cap past which it scrolls.
#[gpui_kit::test]
fn expanded_details_match_v5_spacing(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, missing());
    let long = (1..=20).map(|n| format!("line {n}")).collect::<Vec<_>>();
    for details in ["line one\nline two\nline three".to_owned(), long.join("\n")] {
        s.services
            .activity
            .append(
                &NewLogEntry::new(LogLevel::Error, "store", "download", "failed")
                    .with_details(details),
            )
            .unwrap();
    }
    let rows = support::log_rows(&s.services);
    let (tall, short) = (rows[0].id(), rows[1].id());
    open_logs(cx, &s);
    support::click_id(cx, s.win, ids::log_details_toggle(short));
    support::click_id(cx, s.win, ids::log_details_toggle(tall));
    support::settle(cx, s.win);
    let toggle = support::bounds_of(cx, s.win, ids::log_details_toggle(short)).unwrap();
    let block = support::bounds_of(cx, s.win, ids::log_details(short)).unwrap();
    assert_eq!(
        block.origin.y - toggle.bottom(),
        px(8.),
        "{toggle:?} {block:?}"
    );
    assert_eq!(block.size.height, px(64.), "{block:?}");
    let capped = support::bounds_of(cx, s.win, ids::log_details(tall)).unwrap();
    assert_eq!(capped.size.height, px(192.), "{capped:?}");
}

#[gpui_kit::test]
fn the_select_rows_carry_ids_and_pick_the_filters(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, missing());
    let page = open_logs(cx, &s);
    support::click_id(cx, s.win, ids::LOGS_CATEGORY);
    support::click_id(cx, s.win, ids::select_option(ids::LOGS_CATEGORY, "Sounds"));
    assert_eq!(cx.update(|cx| page.read(cx).category()), Some("sound"));
    support::click_id(cx, s.win, ids::LOGS_LEVEL);
    support::click_id(cx, s.win, ids::select_option(ids::LOGS_LEVEL, "Error"));
    assert_eq!(cx.update(|cx| page.read(cx).level()), Some(LogLevel::Error));
}
