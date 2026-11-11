//! The history range: six stops, kept in the settings, a wider one syncs and a narrower one
//! reads the cache.

use gpui_kit::component::slider::{SliderEvent, SliderValue};
use std::cell::RefCell;
use std::rc::Rc;

use gpui_kit::{AppContext as _, Entity, TestAppContext};
use tf2tools_app::ids;
use tf2tools_app::pages::logs_tf::LogsTfPage;
use tf2tools_core::LogsRange;

use crate::support;
use crate::support::logstf::{
    ARIE, HIGHLANDER, SIXES, day_ago, list_body, list_requests, open_page, row_ids, shell,
};

fn release(
    cx: &mut TestAppContext,
    s: &support::ShellWindow,
    page: &Entity<LogsTfPage>,
    stop: f32,
) {
    let slider = cx.update(|cx| page.read(cx).range_slider().clone());
    cx.update_window(s.win, |_, window, cx| {
        slider.update(cx, |st, cx| {
            st.set_value(stop, window, cx);
            cx.emit(SliderEvent::Release(SliderValue::Single(stop)));
        })
    })
    .unwrap();
    support::settle(cx, s.win);
}

fn settled(cx: &mut TestAppContext, page: &Entity<LogsTfPage>, rows: &[u64]) {
    support::wait_real(cx, "the range's logs", |cx| {
        let quiet = cx.update(|cx| {
            let p = page.read(cx);
            p.is_loaded() && !p.is_syncing(cx)
        });
        quiet && row_ids(cx, page) == rows
    });
}

fn serve(server: &support::StoreServer) {
    // Log 2 is 200 days old: outside 3 months, inside a year.
    server.mount_bytes(
        "/api/v1/log",
        list_body(&[(3, day_ago(1)), (2, day_ago(200))]),
    );
    server.mount_bytes("/json/3", SIXES.as_bytes().to_vec());
    server.mount_bytes("/json/2", HIGHLANDER.as_bytes().to_vec());
}

#[gpui_kit::test]
fn widening_the_range_syncs_and_narrowing_reads_the_cache(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    serve(&server);
    let s = shell(cx, &env, &server.uri(), &[(ARIE, "Arie")]);
    let page = open_page(cx, &s);
    assert_eq!(cx.update(|cx| page.read(cx).range()), LogsRange::Months3);
    assert_eq!(row_ids(cx, &page), [3]);
    assert_eq!(
        support::label_of(cx, s.win, ids::LOGSTF_SUMMARY).as_deref(),
        Some("Last 3 months, 1 log")
    );
    assert_eq!(list_requests(&server), 1);

    release(cx, &s, &page, 3.);
    settled(cx, &page, &[3, 2]);
    assert_eq!(s.services.logs.cache().range().unwrap(), LogsRange::Year1);
    assert_eq!(list_requests(&server), 2, "a wider range lists again");
    assert_eq!(
        support::label_of(cx, s.win, ids::LOGSTF_SUMMARY).as_deref(),
        Some("Last year, 2 logs")
    );

    release(cx, &s, &page, 0.);
    settled(cx, &page, &[3]);
    assert_eq!(s.services.logs.cache().range().unwrap(), LogsRange::Month1);
    assert_eq!(
        list_requests(&server),
        2,
        "a narrower range asks logs.tf nothing"
    );
    assert_eq!(
        support::label_of(cx, s.win, ids::LOGSTF_SUMMARY).as_deref(),
        Some("Last month, 1 log")
    );
}

#[gpui_kit::test]
fn a_stop_label_picks_its_range_and_a_saved_range_is_kept(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    serve(&server);
    let s = shell(cx, &env, &server.uri(), &[(ARIE, "Arie")]);
    s.services.logs.cache().set_range(LogsRange::Year1).unwrap();
    let page = open_page(cx, &s);
    assert_eq!(cx.update(|cx| page.read(cx).range()), LogsRange::Year1);
    assert_eq!(row_ids(cx, &page), [3, 2]);
    let thumb = cx.update(|cx| page.read(cx).range_slider().read(cx).value().start());
    assert_eq!(thumb, 3., "the slider opens on the saved stop");
    for range in LogsRange::STOPS {
        assert!(
            support::present(cx, s.win, ids::logstf_range_stop(range)),
            "{range:?}"
        );
    }

    support::click_id(cx, s.win, ids::logstf_range_stop(LogsRange::All));
    settled(cx, &page, &[3, 2]);
    assert_eq!(s.services.logs.cache().range().unwrap(), LogsRange::All);
    assert_eq!(
        support::label_of(cx, s.win, ids::LOGSTF_SUMMARY).as_deref(),
        Some("All logs, 2")
    );
}

#[gpui_kit::test]
fn a_read_in_flight_does_not_put_the_old_range_back(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    serve(&server);
    let s = shell(cx, &env, &server.uri(), &[(ARIE, "Arie")]);
    let page = open_page(cx, &s);
    // A reread reads the old range off the UI thread and waits to land; then a year is picked.
    cx.update(|cx| page.update(cx, |p, cx| p.set_hidden(false, cx)));
    while cx.dispatcher.scheduler().tick_background_only() {}
    cx.update_window(s.win, |_, window, cx| {
        page.update(cx, |p, cx| p.set_range(LogsRange::Year1, window, cx))
    })
    .unwrap();
    let seen = Rc::new(RefCell::new(Vec::new()));
    let _watch = cx.update(|cx| {
        let seen = Rc::clone(&seen);
        cx.observe(&page, move |page, cx| {
            seen.borrow_mut().push(page.read(cx).range())
        })
    });
    settled(cx, &page, &[3, 2]);
    assert!(
        !seen.borrow().contains(&LogsRange::Months3),
        "the range went back: {:?}",
        seen.borrow()
    );
}

#[gpui_kit::test]
fn a_reread_that_starts_after_a_pick_does_not_put_the_old_range_back(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    serve(&server);
    let s = shell(cx, &env, &server.uri(), &[(ARIE, "Arie")]);
    let page = open_page(cx, &s);
    let seen = Rc::new(RefCell::new(Vec::new()));
    let _watch = cx.update(|cx| {
        let seen = Rc::clone(&seen);
        cx.observe(&page, move |page, cx| {
            seen.borrow_mut().push(page.read(cx).range())
        })
    });
    // The pick's write and a reread started after it both wait off the UI thread, run in a
    // random order. A try whose first step leaves the old range saved ran the reread first.
    let mut raced = 0;
    for pick in [LogsRange::Year1, LogsRange::Months3].repeat(12) {
        let old = cx.update(|cx| page.read(cx).range());
        seen.borrow_mut().clear();
        cx.update_window(s.win, |_, window, cx| {
            page.update(cx, |p, cx| {
                p.set_range(pick, window, cx);
                p.set_hidden(false, cx);
            })
        })
        .unwrap();
        cx.dispatcher.scheduler().tick_background_only();
        if s.services.logs.cache().range().unwrap() == old {
            raced += 1;
        }
        let rows: &[u64] = if pick == LogsRange::Year1 {
            &[3, 2]
        } else {
            &[3]
        };
        settled(cx, &page, rows);
        assert!(
            !seen.borrow().contains(&old),
            "the range went back to {old:?}: {:?}",
            seen.borrow()
        );
    }
    assert!(raced > 0, "no try ran the reread before the write");
}

#[gpui_kit::test]
fn the_stop_labels_take_keyboard_focus(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    serve(&server);
    let s = shell(cx, &env, &server.uri(), &[(ARIE, "Arie")]);
    let _page = open_page(cx, &s);
    for range in LogsRange::STOPS {
        assert!(
            support::focused_of(cx, s.win, ids::logstf_range_stop(range)).is_some(),
            "{range:?} is no focus stop"
        );
    }
}

#[gpui_kit::test]
fn a_player_whose_last_game_is_old_still_sees_it_in_a_short_range(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    // The range reaches back from the newest log, not from today, so it is never empty.
    server.mount_bytes("/api/v1/log", list_body(&[(3, day_ago(400))]));
    server.mount_bytes("/json/3", SIXES.as_bytes().to_vec());
    let s = shell(cx, &env, &server.uri(), &[(ARIE, "Arie")]);
    let page = open_page(cx, &s);
    assert_eq!(row_ids(cx, &page), [3]);
    assert!(!support::present(cx, s.win, ids::LOGSTF_NO_LOGS));
}
