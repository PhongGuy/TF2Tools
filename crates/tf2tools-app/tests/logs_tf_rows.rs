//! A log row's own columns: the start time, the tracked player's eight stats, and how the
//! page sits in wide and narrow windows.

use gpui_kit::{TestAppContext, px, size};
use tf2tools_app::ids;
use tf2tools_app::pages::logs_tf::STAT_COLUMNS;
use tf2tools_core::TfClass;

use crate::support;
use crate::support::logstf::{ARIE, open_log, open_page, serve_two_logs, shell, to_end};

#[gpui_kit::test]
fn a_row_shows_the_players_own_stats_in_logs_tf_order(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    serve_two_logs(&server);
    let s = shell(cx, &env, &server.uri(), &[(ARIE, "Arie")]);
    let page = open_page(cx, &s);
    to_end(cx, &page);

    assert_eq!(
        STAT_COLUMNS,
        ["K", "A", "D", "DA", "DA/M", "KA/D", "DT", "DT/M"]
    );
    // Arie in 4137103: 7/6/21, 3896 damage and 5964 taken over 1714 s.
    let values = ["7", "6", "21", "3896", "136", "0.6", "5964", "208"];
    let mut last_right = px(0.);
    for (column, value) in STAT_COLUMNS.iter().zip(values) {
        assert_eq!(
            support::label_of(cx, s.win, ids::logstf_log_stat(3, column)).as_deref(),
            Some(value),
            "{column}"
        );
        let cell = support::bounds_of(cx, s.win, ids::logstf_log_stat(3, column)).unwrap();
        let head = support::bounds_of(cx, s.win, ids::logstf_head(column)).unwrap();
        assert!(cell.left() >= last_right, "{column} {cell:?} in order");
        assert!(
            (cell.right() - head.right()).abs() < px(1.),
            "{column} {cell:?} under its header {head:?}"
        );
        last_right = cell.right();
    }
    let class = support::bounds_of(cx, s.win, ids::logstf_log_class(3, TfClass::Scout)).unwrap();
    let first = support::bounds_of(cx, s.win, ids::logstf_log_stat(3, "K")).unwrap();
    assert!(class.right() <= first.left(), "the class icon comes first");
}

#[gpui_kit::test]
fn a_row_date_carries_its_start_time(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    serve_two_logs(&server);
    let s = shell(cx, &env, &server.uri(), &[(ARIE, "Arie")]);
    let page = open_page(cx, &s);
    to_end(cx, &page);
    let date = support::label_of(cx, s.win, ids::logstf_log_date(3)).expect("the date cell");
    let (day, time) = date.split_once(' ').expect("a date and a time");
    assert_eq!((day.len(), time.len()), (10, 5), "{date:?}");
    assert_eq!(time.as_bytes()[2], b':', "{date:?}");
}

#[gpui_kit::test]
fn a_very_wide_window_keeps_the_page_together_in_the_middle(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    serve_two_logs(&server);
    let s = shell(cx, &env, &server.uri(), &[(ARIE, "Arie")]);
    cx.simulate_window_resize(s.win, size(px(2600.), px(1200.)));
    let page = open_page(cx, &s);
    open_log(cx, &s, &page, 3);
    let list = support::bounds_of(cx, s.win, ids::LOGSTF_LIST).unwrap();
    assert!(
        list.size.width > px(1800.),
        "the list itself spans {list:?}"
    );
    for id in [
        ids::logstf_log(3),
        ids::logstf_log_panel(3),
        ids::logstf_head("DT/M"),
    ] {
        let b = support::bounds_of(cx, s.win, id.clone()).unwrap();
        assert!(b.size.width <= px(1400.), "{id:?} {b:?} capped");
    }
    let row = support::bounds_of(cx, s.win, ids::logstf_log(3)).unwrap();
    assert!(
        (row.center().x - list.center().x).abs() < px(2.),
        "row {row:?} centred in {list:?}"
    );
    let title = support::bounds_of(cx, s.win, ids::logstf_log_title(3)).unwrap();
    let k = support::bounds_of(cx, s.win, ids::logstf_log_stat(3, "K")).unwrap();
    assert!(
        k.left() - title.right() < px(800.),
        "stats sit near the title"
    );
}

#[gpui_kit::test]
fn the_smallest_window_cuts_the_title_and_keeps_every_stat(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    serve_two_logs(&server);
    let s = shell(cx, &env, &server.uri(), &[(ARIE, "Arie")]);
    cx.simulate_window_resize(s.win, size(px(800.), px(600.)));
    let page = open_page(cx, &s);
    to_end(cx, &page);
    let list = support::bounds_of(cx, s.win, ids::LOGSTF_LIST).unwrap();
    let title = support::bounds_of(cx, s.win, ids::logstf_log_title(3)).unwrap();
    let mut last_right = title.right();
    for column in STAT_COLUMNS {
        for id in [ids::logstf_log_stat(3, column), ids::logstf_head(column)] {
            let b = support::bounds_of(cx, s.win, id.clone()).unwrap();
            assert!(b.right() <= list.right(), "{id:?} {b:?} past {list:?}");
        }
        let cell = support::bounds_of(cx, s.win, ids::logstf_log_stat(3, column)).unwrap();
        assert!(
            cell.left() >= last_right,
            "{column} {cell:?} clear of the cells before"
        );
        assert!(cell.size.width >= px(16.), "{column} {cell:?} readable");
        last_right = cell.right();
    }
}

#[gpui_kit::test]
fn the_range_sits_above_the_class_table(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    serve_two_logs(&server);
    let s = shell(cx, &env, &server.uri(), &[(ARIE, "Arie")]);
    let _page = open_page(cx, &s);
    let table = support::bounds_of(cx, s.win, ids::LOGSTF_CLASS_TABLE).unwrap();
    for id in [ids::LOGSTF_RANGE, ids::LOGSTF_SUMMARY] {
        let b = support::bounds_of(cx, s.win, id).unwrap();
        assert!(
            b.bottom() <= table.top(),
            "{id} {b:?} above the table {table:?}"
        );
    }
}

#[gpui_kit::test]
fn a_log_stored_without_damage_taken_gets_it_from_its_body(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    serve_two_logs(&server);
    let s = shell(cx, &env, &server.uri(), &[(ARIE, "Arie")]);
    let page = open_page(cx, &s);
    // As an earlier build stored them: no damage taken, waiting for the backfill.
    rusqlite::Connection::open(env.data.db_path())
        .unwrap()
        .execute_batch(
            "UPDATE logstf_class_rows SET dt = NULL; UPDATE logstf_logs SET dt_counted = 0;",
        )
        .unwrap();
    cx.update(|cx| page.update(cx, |p, cx| p.refresh(cx)));
    let dt = |cx: &mut TestAppContext| {
        cx.update(|cx| {
            page.read(cx)
                .rows()
                .iter()
                .find(|r| r.id() == 3)
                .map(|r| r.dt())
        })
    };
    support::wait_real(cx, "the reread without DT", |cx| dt(cx) == Some(None));
    to_end(cx, &page);
    assert_eq!(
        support::label_of(cx, s.win, ids::logstf_log_stat(3, "DT")).as_deref(),
        Some("\u{2013}")
    );

    cx.update(|cx| page.update(cx, |p, cx| p.count_older_weapons(cx)));
    while cx.dispatcher.scheduler().tick_background_only() {}
    support::wait_real(cx, "the filled DT", |cx| dt(cx) == Some(Some(5964)));
    to_end(cx, &page);
    assert_eq!(
        support::label_of(cx, s.win, ids::logstf_log_stat(3, "DT")).as_deref(),
        Some("5964")
    );
}
