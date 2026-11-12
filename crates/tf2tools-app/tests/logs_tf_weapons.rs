//! The class weapons panel: a played class card opens its weapon sums over the range.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use gpui_kit::{AppContext, ElementId, Entity, TestAppContext};
use tf2tools_app::ids;
use tf2tools_app::pages::logs_tf::LogsTfPage;
use tf2tools_app::pages::logs_tf::text::{NO_WEAPONS_TEXT, counting_text, weapons_title};
use tf2tools_core::{LogsRange, TfClass};

use crate::support;
use crate::support::logstf::{
    ARIE, HIGHLANDER, SIXES, day_ago, install_icons, install_names, list_body, load_icons,
    load_names, mount_list_for, mount_list_with, open_page, requests_to, shell, sid, to_end,
};

/// Serves Arie's one log, the 6v6 fixture, a day old.
fn serve_sixes(server: &support::StoreServer) {
    server.mount_bytes("/api/v1/log", list_body(&[(3, day_ago(1))]));
    server.mount_bytes("/json/3", SIXES.as_bytes().to_vec());
}

fn picked(cx: &mut TestAppContext, page: &Entity<LogsTfPage>) -> Option<TfClass> {
    cx.update(|cx| page.read(cx).selected_class())
}

fn shown(cx: &mut TestAppContext, page: &Entity<LogsTfPage>) -> Option<TfClass> {
    cx.update(|cx| page.read(cx).class_weapons().map(|(class, _)| class))
}

fn wait_for(cx: &mut TestAppContext, page: &Entity<LogsTfPage>, class: TfClass) {
    support::wait_real(cx, "the class weapons", |cx| shown(cx, page) == Some(class));
}

fn row(
    cx: &mut TestAppContext,
    s: &support::ShellWindow,
    class: TfClass,
    name: &str,
) -> Option<String> {
    support::label_of(cx, s.win, ids::logstf_class_weapon(class, name))
}

fn title(cx: &mut TestAppContext, s: &support::ShellWindow) -> Option<String> {
    support::label_of(cx, s.win, ids::LOGSTF_CLASS_WEAPONS_TITLE)
}

#[test]
fn the_panel_wording() {
    assert_eq!(
        weapons_title(TfClass::Scout, LogsRange::Months3, 1),
        "Scout weapons · 3 months · 1 log"
    );
    assert_eq!(
        weapons_title(TfClass::Heavy, LogsRange::All, 1234),
        "Heavy weapons · All · 1,234 logs"
    );
    assert_eq!(
        weapons_title(TfClass::Medic, LogsRange::Year1, 0),
        "Medic weapons · 1 year · 0 logs"
    );
    assert_eq!(counting_text(3), "Counting older logs, 3 left");
    assert_eq!(NO_WEAPONS_TEXT, "No weapon stats in these logs");
    let ElementId::Name(id) = ids::logstf_class_weapon(TfClass::Heavy, "long_heatmaker") else {
        panic!("a named id");
    };
    assert_eq!(id.to_string(), "logstf-class-weapon:heavy:long_heatmaker");
}

#[gpui_kit::test]
fn a_played_card_opens_its_weapons_and_a_second_click_closes_them(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    serve_sixes(&server);
    let s = shell(cx, &env, &server.uri(), &[(ARIE, "Arie")]);
    let page = open_page(cx, &s);

    support::click_id(cx, s.win, ids::logstf_class(TfClass::Scout));
    wait_for(cx, &page, TfClass::Scout);
    assert_eq!(
        title(cx, &s).as_deref(),
        Some("Scout weapons · 3 months · 1 log")
    );
    // Shares are of the class's summed weapon kills (7) and damage (3511).
    assert_eq!(
        row(cx, &s, TfClass::Scout, "scattergun").as_deref(),
        Some("scattergun 7 (100%) 3334 (95%) 56%")
    );
    assert_eq!(
        row(cx, &s, TfClass::Scout, "pistol_scout").as_deref(),
        Some("pistol_scout 0 (0%) 177 (5%) 10%")
    );
    let top = |cx: &mut TestAppContext, name: &str| {
        support::bounds_of(cx, s.win, ids::logstf_class_weapon(TfClass::Scout, name))
            .unwrap()
            .top()
    };
    assert!(
        top(cx, "scattergun") < top(cx, "pistol_scout"),
        "damage high to low"
    );
    assert_eq!(
        support::selected_of(cx, s.win, ids::logstf_class(TfClass::Scout)),
        Some(true)
    );
    assert_eq!(
        support::selected_of(cx, s.win, ids::logstf_class(TfClass::Pyro)),
        Some(false)
    );

    support::click_id(cx, s.win, ids::logstf_class(TfClass::Scout));
    assert_eq!(picked(cx, &page), None);
    assert!(!support::present(cx, s.win, ids::LOGSTF_CLASS_WEAPONS));

    // Reopened, the top item grows, so the log row sits under the panel.
    support::click_id(cx, s.win, ids::logstf_class(TfClass::Scout));
    wait_for(cx, &page, TfClass::Scout);
    to_end(cx, &page);
    let panel =
        support::bounds_of(cx, s.win, ids::LOGSTF_CLASS_WEAPONS).expect("the panel is drawn");
    let log = support::bounds_of(cx, s.win, ids::logstf_log(3)).expect("the row is drawn");
    assert!(
        log.top() >= panel.bottom(),
        "row {log:?} overlaps panel {panel:?}"
    );
}

// Each column title stays on one line, so the title row is no taller than a weapon row.
#[gpui_kit::test]
fn the_column_titles_keep_to_one_line(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    serve_sixes(&server);
    let s = shell(cx, &env, &server.uri(), &[(ARIE, "Arie")]);
    let page = open_page(cx, &s);
    support::click_id(cx, s.win, ids::logstf_class(TfClass::Scout));
    wait_for(cx, &page, TfClass::Scout);
    let head = support::bounds_of(cx, s.win, ids::LOGSTF_CLASS_WEAPONS_HEAD).expect("the titles");
    let row = support::bounds_of(
        cx,
        s.win,
        ids::logstf_class_weapon(TfClass::Scout, "scattergun"),
    )
    .expect("a weapon row");
    assert!(
        head.size.height <= row.size.height,
        "titles {head:?} are taller than a row {row:?}"
    );
}

#[gpui_kit::test]
fn close_switch_unplayed_and_no_shots(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    serve_sixes(&server);
    let s = shell(cx, &env, &server.uri(), &[(ARIE, "Arie")]);
    let page = open_page(cx, &s);

    support::click_id(cx, s.win, ids::logstf_class(TfClass::Soldier));
    assert_eq!(picked(cx, &page), None, "an unplayed card opens nothing");
    assert_eq!(
        support::selected_of(cx, s.win, ids::logstf_class(TfClass::Soldier)),
        None
    );

    support::click_id(cx, s.win, ids::logstf_class(TfClass::Scout));
    wait_for(cx, &page, TfClass::Scout);
    // The ring moves at once; Scout's title and rows stay until Pyro's read lands.
    cx.update(|cx| page.update(cx, |p, cx| p.pick_class(TfClass::Pyro, cx)));
    assert_eq!(
        (picked(cx, &page), shown(cx, &page)),
        (Some(TfClass::Pyro), Some(TfClass::Scout))
    );
    wait_for(cx, &page, TfClass::Pyro);
    assert_eq!(
        support::selected_of(cx, s.win, ids::logstf_class(TfClass::Pyro)),
        Some(true)
    );
    assert_eq!(
        title(cx, &s).as_deref(),
        Some("Pyro weapons · 3 months · 1 log")
    );
    assert_eq!(
        row(cx, &s, TfClass::Pyro, "shotgun_pyro").as_deref(),
        Some("shotgun_pyro 0 (0%) 56 (55%) 54%")
    );
    assert_eq!(
        row(cx, &s, TfClass::Pyro, "flamethrower").as_deref(),
        Some("flamethrower 0 (0%) 46 (45%) -")
    );
    assert!(!support::present(
        cx,
        s.win,
        ids::logstf_class_weapon(TfClass::Scout, "scattergun")
    ));

    support::click_id(cx, s.win, ids::LOGSTF_CLASS_WEAPONS_CLOSE);
    assert_eq!(picked(cx, &page), None);
    assert!(!support::present(cx, s.win, ids::LOGSTF_CLASS_WEAPONS));
}

#[gpui_kit::test]
fn a_weapon_read_for_a_class_no_longer_picked_is_dropped(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    serve_sixes(&server);
    let s = shell(cx, &env, &server.uri(), &[(ARIE, "Arie")]);
    let page = open_page(cx, &s);
    // Scout's read runs off the UI thread and waits to land; then Pyro is picked.
    cx.update(|cx| page.update(cx, |p, cx| p.pick_class(TfClass::Scout, cx)));
    while cx.dispatcher.scheduler().tick_background_only() {}
    let seen = Rc::new(RefCell::new(Vec::new()));
    let _watch = cx.update(|cx| {
        let seen = Rc::clone(&seen);
        cx.observe(&page, move |page, cx| {
            seen.borrow_mut()
                .push(page.read(cx).class_weapons().map(|(c, _)| c))
        })
    });
    cx.update(|cx| page.update(cx, |p, cx| p.pick_class(TfClass::Pyro, cx)));
    wait_for(cx, &page, TfClass::Pyro);
    support::settle(cx, s.win);
    assert!(
        !seen.borrow().contains(&Some(TfClass::Scout)),
        "Scout's read landed: {:?}",
        seen.borrow()
    );
}

const CABO: u64 = 187_445_387;

fn shown_logs(cx: &mut TestAppContext, page: &Entity<LogsTfPage>, class: TfClass) -> Option<u32> {
    cx.update(|cx| {
        let p = page.read(cx);
        p.class_weapons()
            .filter(|(c, _)| *c == class)
            .map(|(_, w)| w.logs())
    })
}

#[gpui_kit::test]
fn the_panel_follows_a_range_pick(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    // Log 2 is the same 6v6 log 200 days back: outside 3 months, inside a year.
    server.mount_bytes(
        "/api/v1/log",
        list_body(&[(3, day_ago(1)), (2, day_ago(200))]),
    );
    server.mount_bytes("/json/3", SIXES.as_bytes().to_vec());
    server.mount_bytes("/json/2", SIXES.as_bytes().to_vec());
    let s = shell(cx, &env, &server.uri(), &[(ARIE, "Arie")]);
    let page = open_page(cx, &s);
    support::click_id(cx, s.win, ids::logstf_class(TfClass::Scout));
    wait_for(cx, &page, TfClass::Scout);
    assert_eq!(
        title(cx, &s).as_deref(),
        Some("Scout weapons · 3 months · 1 log")
    );

    support::click_id(cx, s.win, ids::logstf_range_stop(LogsRange::Year1));
    support::wait_real(cx, "a year of weapons", |cx| {
        let quiet = cx.update(|cx| !page.read(cx).is_syncing(cx));
        quiet && shown_logs(cx, &page, TfClass::Scout) == Some(2)
    });
    assert_eq!(
        title(cx, &s).as_deref(),
        Some("Scout weapons · 1 year · 2 logs")
    );
    assert_eq!(
        row(cx, &s, TfClass::Scout, "scattergun").as_deref(),
        Some("scattergun 14 (100%) 6668 (95%) 56%")
    );
}

#[gpui_kit::test]
fn the_panel_follows_the_chip_and_closes_for_a_player_without_the_class(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    mount_list_for(&server, ARIE, &[(3, day_ago(1))], Duration::ZERO);
    mount_list_for(&server, CABO, &[(2, day_ago(1))], Duration::ZERO);
    server.mount_bytes("/json/3", SIXES.as_bytes().to_vec());
    server.mount_bytes("/json/2", HIGHLANDER.as_bytes().to_vec());
    // Cabo is added last, so his chip is active and synced first; then Arie's.
    let s = shell(cx, &env, &server.uri(), &[(ARIE, "Arie"), (CABO, "cabo")]);
    let page = open_page(cx, &s);
    let switch = |cx: &mut TestAppContext, account: u64| {
        support::click_id(cx, s.win, ids::logstf_chip(sid(account)));
        support::wait_real(cx, "the switch", |cx| {
            cx.update(|cx| {
                let p = page.read(cx);
                p.active() == Some(sid(account)) && !p.is_syncing(cx)
            })
        });
    };
    switch(cx, ARIE);
    support::click_id(cx, s.win, ids::logstf_class(TfClass::Scout));
    wait_for(cx, &page, TfClass::Scout);

    switch(cx, CABO);
    support::wait_real(cx, "cabo's scout weapons", |cx| {
        let first = cx.update(|cx| {
            page.read(cx)
                .class_weapons()
                .and_then(|(_, w)| w.weapons().first().map(|x| x.kills()))
        });
        first == Some(17)
    });
    assert_eq!(
        row(cx, &s, TfClass::Scout, "scattergun").as_deref(),
        Some("scattergun 17 (85%) 10006 (87%) 63%")
    );

    // Arie played Pyro; Cabo never did, so his chip closes the panel.
    switch(cx, ARIE);
    support::click_id(cx, s.win, ids::logstf_class(TfClass::Pyro));
    wait_for(cx, &page, TfClass::Pyro);
    switch(cx, CABO);
    support::wait_real(cx, "the panel to close", |cx| picked(cx, &page).is_none());
    assert!(!support::present(cx, s.win, ids::LOGSTF_CLASS_WEAPONS));
}

#[gpui_kit::test]
fn removing_the_last_player_closes_the_panel(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    serve_sixes(&server);
    let s = shell(cx, &env, &server.uri(), &[(ARIE, "Arie")]);
    let page = open_page(cx, &s);
    support::click_id(cx, s.win, ids::logstf_class(TfClass::Scout));
    wait_for(cx, &page, TfClass::Scout);
    support::click_id(cx, s.win, ids::logstf_chip_remove(sid(ARIE)));
    support::click_id(cx, s.win, ids::CONFIRM_OK);
    support::wait_real(cx, "the removal", |cx| {
        cx.update(|cx| page.read(cx).players().is_empty())
    });
    assert_eq!((picked(cx, &page), shown(cx, &page)), (None, None));
    assert!(!support::present(cx, s.win, ids::LOGSTF_CLASS_WEAPONS));
}

#[gpui_kit::test]
fn a_reread_that_saw_the_old_class_does_not_bring_it_back(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    serve_sixes(&server);
    let s = shell(cx, &env, &server.uri(), &[(ARIE, "Arie")]);
    let page = open_page(cx, &s);
    // A pick and a reread both read Scout off the UI thread and wait to land; then Pyro is picked.
    cx.update(|cx| {
        page.update(cx, |p, cx| {
            p.pick_class(TfClass::Scout, cx);
            p.set_hidden(false, cx);
        })
    });
    while cx.dispatcher.scheduler().tick_background_only() {}
    let seen = Rc::new(RefCell::new(Vec::new()));
    let _watch = cx.update(|cx| {
        let seen = Rc::clone(&seen);
        cx.observe(&page, move |page, cx| {
            seen.borrow_mut()
                .push(page.read(cx).class_weapons().map(|(c, _)| c))
        })
    });
    cx.update(|cx| page.update(cx, |p, cx| p.pick_class(TfClass::Pyro, cx)));
    wait_for(cx, &page, TfClass::Pyro);
    support::settle(cx, s.win);
    assert!(
        !seen.borrow().contains(&Some(TfClass::Scout)),
        "a Scout read landed: {:?}",
        seen.borrow()
    );
}

#[gpui_kit::test]
fn logs_stored_before_weapon_rows_are_counted_after_the_first_load(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    // No /json/3: a download would 404 and mark the log gone, failing the test.
    server.mount_bytes("/api/v1/log", list_body(&[(3, day_ago(1))]));
    let s = shell(cx, &env, &server.uri(), &[(ARIE, "Arie")]);
    // Log 3 as an earlier build left it: body and class row stored, no weapon rows, uncounted.
    let steam = i64::try_from(sid(ARIE).id64()).unwrap();
    let conn = rusqlite::Connection::open(env.data.db_path()).unwrap();
    conn.execute(
        "INSERT INTO logstf_logs (id, title, map, date, length, body, state)
         VALUES (3, 'Log 3', 'cp_process_f12', ?1, 1714, ?2, 0)",
        rusqlite::params![day_ago(1), SIXES],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO logstf_player_logs (steam_id, log_id) VALUES (?1, 3)",
        [steam],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO logstf_class_rows (log_id, steam_id, class, seconds, kills, deaths, assists, dmg)
         VALUES (3, ?1, 'scout', 1559, 7, 0, 0, 3511)",
        [steam],
    )
    .unwrap();
    drop(conn);

    let page = open_page(cx, &s);
    support::click_id(cx, s.win, ids::logstf_class(TfClass::Scout));
    support::wait_real(cx, "the counted log", |cx| {
        cx.update(|cx| {
            page.read(cx)
                .class_weapons()
                .is_some_and(|(c, w)| c == TfClass::Scout && w.logs() == 1 && w.pending() == 0)
        })
    });
    assert_eq!(
        row(cx, &s, TfClass::Scout, "scattergun").as_deref(),
        Some("scattergun 7 (100%) 3334 (95%) 56%")
    );
    assert!(!support::present(cx, s.win, ids::LOGSTF_CLASS_WEAPONS_NOTE));
    assert_eq!(
        requests_to(&server, "/json/3"),
        0,
        "counted from the stored body, not downloaded"
    );
}

#[gpui_kit::test]
fn the_note_says_when_logs_wait_or_hold_no_weapons(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    serve_sixes(&server);
    let s = shell(cx, &env, &server.uri(), &[(ARIE, "Arie")]);
    let page = open_page(cx, &s);
    let conn = rusqlite::Connection::open(env.data.db_path()).unwrap();
    // Heavy keeps its counted marker but loses its rows; Scout's log waits to be counted.
    conn.execute_batch(
        "DELETE FROM logstf_weapon_rows WHERE class IN ('heavyweapons', 'scout');
         UPDATE logstf_logs SET weapons_counted = 0 WHERE id = 3;",
    )
    .unwrap();
    support::click_id(cx, s.win, ids::logstf_class(TfClass::Scout));
    wait_for(cx, &page, TfClass::Scout);
    assert_eq!(
        title(cx, &s).as_deref(),
        Some("Scout weapons · 3 months · 0 logs")
    );
    assert_eq!(
        support::label_of(cx, s.win, ids::LOGSTF_CLASS_WEAPONS_NOTE).as_deref(),
        Some("Counting older logs, 1 left")
    );

    conn.execute_batch("UPDATE logstf_logs SET weapons_counted = 1 WHERE id = 3;")
        .unwrap();
    support::click_id(cx, s.win, ids::logstf_class(TfClass::Heavy));
    wait_for(cx, &page, TfClass::Heavy);
    assert_eq!(
        support::label_of(cx, s.win, ids::LOGSTF_CLASS_WEAPONS_NOTE).as_deref(),
        Some(NO_WEAPONS_TEXT)
    );
}

#[gpui_kit::test]
fn a_backfill_batch_refills_the_open_panel(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    serve_sixes(&server);
    let s = shell(cx, &env, &server.uri(), &[(ARIE, "Arie")]);
    let page = open_page(cx, &s);
    support::click_id(cx, s.win, ids::logstf_class(TfClass::Scout));
    wait_for(cx, &page, TfClass::Scout);
    // The click read has landed with log 3. Log 4, the same body stored uncounted, joins the
    // window; only a batch's rerun can bring it into the panel.
    let steam = i64::try_from(sid(ARIE).id64()).unwrap();
    let conn = rusqlite::Connection::open(env.data.db_path()).unwrap();
    conn.execute(
        "INSERT INTO logstf_logs (id, title, map, date, length, body, state)
         VALUES (4, 'Log 4', 'cp_process_f12', ?1, 1714, ?2, 0)",
        rusqlite::params![day_ago(2), SIXES],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO logstf_player_logs (steam_id, log_id) VALUES (?1, 4)",
        [steam],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO logstf_class_rows (log_id, steam_id, class, seconds, kills, deaths, assists, dmg)
         VALUES (4, ?1, 'scout', 1559, 7, 0, 0, 3511)",
        [steam],
    )
    .unwrap();
    drop(conn);
    assert_eq!(shown_logs(cx, &page, TfClass::Scout), Some(1));

    cx.update(|cx| page.update(cx, |p, cx| p.count_older_weapons(cx)));
    while cx.dispatcher.scheduler().tick_background_only() {}
    support::wait_real(cx, "the refilled panel", |cx| {
        shown_logs(cx, &page, TfClass::Scout) == Some(2)
    });
    assert_eq!(
        title(cx, &s).as_deref(),
        Some("Scout weapons · 3 months · 2 logs")
    );
    assert_eq!(
        row(cx, &s, TfClass::Scout, "scattergun").as_deref(),
        Some("scattergun 14 (100%) 6668 (95%) 56%")
    );
}

// Answers a player's list at once until told to hold, then waits up to ten seconds for
// the release, so a sync can be kept running without a gate on its first request.
#[derive(Clone, Default)]
struct HoldLater {
    hold: Arc<AtomicBool>,
    release: Arc<AtomicBool>,
}

impl wiremock::Respond for HoldLater {
    fn respond(&self, _: &wiremock::Request) -> wiremock::ResponseTemplate {
        if self.hold.load(Ordering::SeqCst) {
            let deadline = Instant::now() + Duration::from_secs(10);
            while !self.release.load(Ordering::SeqCst) && Instant::now() < deadline {
                std::thread::sleep(Duration::from_millis(10));
            }
        }
        wiremock::ResponseTemplate::new(200).set_body_bytes(list_body(&[(3, day_ago(1))]))
    }
}

#[gpui_kit::test]
fn a_pick_while_a_load_reads_gets_its_weapons_once_the_load_lands(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    let arie = HoldLater::default();
    mount_list_with(&server, ARIE, arie.clone());
    mount_list_for(&server, CABO, &[(2, day_ago(1))], Duration::ZERO);
    server.mount_bytes("/json/3", SIXES.as_bytes().to_vec());
    server.mount_bytes("/json/2", HIGHLANDER.as_bytes().to_vec());
    let s = shell(cx, &env, &server.uri(), &[(ARIE, "Arie"), (CABO, "cabo")]);
    let page = open_page(cx, &s);
    support::click_id(cx, s.win, ids::logstf_chip(sid(ARIE)));
    support::wait_real(cx, "arie", |cx| {
        cx.update(|cx| {
            let p = page.read(cx);
            p.active() == Some(sid(ARIE)) && !p.is_syncing(cx)
        })
    });
    support::click_id(cx, s.win, ids::logstf_class(TfClass::Pyro));
    wait_for(cx, &page, TfClass::Pyro);

    // Arie's held refresh keeps Cabo's sync queued, so no sync reread follows the load. The
    // load reads Cabo with Pyro selected; Scout is picked while it waits to land, and the
    // click read sees Arie, so only the landing can fetch Cabo's Scout weapons.
    arie.hold.store(true, Ordering::SeqCst);
    cx.update(|cx| page.update(cx, |p, cx| p.refresh(cx)));
    s.services
        .logs
        .cache()
        .set_active_player(sid(CABO))
        .unwrap();
    cx.update(|cx| page.update(cx, |p, cx| p.set_hidden(false, cx)));
    while cx.dispatcher.scheduler().tick_background_only() {}
    cx.update(|cx| page.update(cx, |p, cx| p.pick_class(TfClass::Scout, cx)));
    support::settle(cx, s.win);
    let first = cx.update(|cx| {
        let p = page.read(cx);
        p.class_weapons()
            .map(|(c, w)| (c, w.weapons().first().map(|x| x.kills())))
    });
    arie.release.store(true, Ordering::SeqCst);
    assert_eq!(cx.update(|cx| page.read(cx).active()), Some(sid(CABO)));
    assert_eq!(first, Some((TfClass::Scout, Some(17))));
}

#[gpui_kit::test]
fn a_read_from_before_a_close_never_lands_after_a_reopen(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    serve_sixes(&server);
    let s = shell(cx, &env, &server.uri(), &[(ARIE, "Arie")]);
    let page = open_page(cx, &s);
    // Scout's first read finishes off the UI thread with one log and waits to land.
    cx.update(|cx| page.update(cx, |p, cx| p.pick_class(TfClass::Scout, cx)));
    while cx.dispatcher.scheduler().tick_background_only() {}
    // Scout's weapon rows go, so a read from here on sees no logs and tells the two apart.
    rusqlite::Connection::open(env.data.db_path())
        .unwrap()
        .execute("DELETE FROM logstf_weapon_rows WHERE class = 'scout'", [])
        .unwrap();
    let seen = Rc::new(RefCell::new(Vec::new()));
    let _watch = cx.update(|cx| {
        let seen = Rc::clone(&seen);
        cx.observe(&page, move |page, cx| {
            seen.borrow_mut()
                .push(page.read(cx).class_weapons().map(|(c, w)| (c, w.logs())))
        })
    });
    cx.update(|cx| {
        page.update(cx, |p, cx| {
            p.close_class(cx);
            p.pick_class(TfClass::Scout, cx);
        })
    });
    support::wait_real(cx, "the reopened read", |cx| {
        cx.update(|cx| page.read(cx).class_weapons().map(|(c, w)| (c, w.logs())))
            == Some((TfClass::Scout, 0))
    });
    support::settle(cx, s.win);
    assert!(
        !seen.borrow().contains(&Some((TfClass::Scout, 1))),
        "the read from before the close landed: {:?}",
        seen.borrow()
    );
    assert_eq!(picked(cx, &page), Some(TfClass::Scout));
}

// A weapon read that fails inside a page load costs only the panel: the averages still land.
#[gpui_kit::test]
fn a_failed_weapon_read_does_not_stop_the_page_load(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    serve_sixes(&server);
    let s = shell(cx, &env, &server.uri(), &[(ARIE, "Arie")]);
    let page = open_page(cx, &s);
    support::click_id(cx, s.win, ids::logstf_class(TfClass::Scout));
    wait_for(cx, &page, TfClass::Scout);
    let scout_kills = |cx: &mut TestAppContext| {
        cx.update(|cx| page.read(cx).averages().map(|a| a[0].kills_per_min()))
    };
    let before = scout_kills(cx);

    // Only the weapon query breaks, and Scout gains kills the next load must show.
    let conn = support::sql(&env);
    conn.execute_batch(
        "ALTER TABLE logstf_weapon_rows RENAME TO logstf_weapon_rows_away;
         UPDATE logstf_class_rows SET kills = kills + 100 WHERE class = 'scout';",
    )
    .unwrap();
    cx.update_window(s.win, |_, window, cx| {
        page.update(cx, |p, cx| p.select(sid(ARIE), window, cx))
    })
    .unwrap();
    support::wait_real(cx, "the averages after a failed weapon read", |cx| {
        scout_kills(cx) != before
    });
    assert_eq!(
        shown(cx, &page),
        Some(TfClass::Scout),
        "the panel keeps its rows"
    );
    conn.execute(
        "ALTER TABLE logstf_weapon_rows_away RENAME TO logstf_weapon_rows",
        [],
    )
    .unwrap();
}

#[gpui_kit::test]
fn the_panel_switches_to_game_names_when_they_load(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    serve_sixes(&server);
    let s = shell(cx, &env, &server.uri(), &[(ARIE, "Arie")]);
    let names = install_names(cx);
    let page = open_page(cx, &s);
    support::click_id(cx, s.win, ids::logstf_class(TfClass::Scout));
    wait_for(cx, &page, TfClass::Scout);
    assert_eq!(
        row(cx, &s, TfClass::Scout, "scattergun").as_deref(),
        Some("scattergun 7 (100%) 3334 (95%) 56%")
    );
    load_names(cx, &env, &names);
    support::settle(cx, s.win);
    assert_eq!(
        row(cx, &s, TfClass::Scout, "scattergun").as_deref(),
        Some("Scattergun 7 (100%) 3334 (95%) 56%"),
        "the id keeps the log name"
    );
    assert_eq!(
        row(cx, &s, TfClass::Scout, "pistol_scout").as_deref(),
        Some("pistol_scout 0 (0%) 177 (5%) 10%"),
        "an unknown name stays raw"
    );
}

#[test]
fn the_icon_ids() {
    let name = |id: ElementId| match id {
        ElementId::Name(n) => n.to_string(),
        other => panic!("a named id, got {other:?}"),
    };
    assert_eq!(
        name(ids::logstf_class_weapon_icon(TfClass::Heavy, "minigun")),
        "logstf-class-weapon-icon:heavy:minigun"
    );
    assert_eq!(
        name(ids::logstf_weapon_icon(
            sid(ARIE),
            TfClass::Scout,
            "scattergun"
        )),
        format!("logstf-weapon-icon:{}:scout:scattergun", sid(ARIE).id64())
    );
}

#[gpui_kit::test]
fn the_panel_shows_a_kill_icon_where_the_game_has_one(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    serve_sixes(&server);
    let s = shell(cx, &env, &server.uri(), &[(ARIE, "Arie")]);
    let icons = install_icons(cx);
    load_icons(cx, &env, &icons);
    let page = open_page(cx, &s);
    support::click_id(cx, s.win, ids::logstf_class(TfClass::Scout));
    wait_for(cx, &page, TfClass::Scout);
    let icon = support::bounds_of(
        cx,
        s.win,
        ids::logstf_class_weapon_icon(TfClass::Scout, "scattergun"),
    )
    .expect("its icon");
    assert!((f32::from(icon.size.height) - 16.).abs() <= 0.5, "{icon:?}");
    assert!((f32::from(icon.size.width) - 32.).abs() <= 0.5, "{icon:?}");
    assert!(row(cx, &s, TfClass::Scout, "pistol_scout").is_some());
    let pistol = ids::logstf_class_weapon_icon(TfClass::Scout, "pistol_scout");
    assert!(!support::present(cx, s.win, pistol), "no icon, text only");
}

#[gpui_kit::test]
fn an_open_panel_gains_icons_when_they_load(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    serve_sixes(&server);
    let s = shell(cx, &env, &server.uri(), &[(ARIE, "Arie")]);
    let icons = install_icons(cx);
    let page = open_page(cx, &s);
    support::click_id(cx, s.win, ids::logstf_class(TfClass::Scout));
    wait_for(cx, &page, TfClass::Scout);
    let icon = ids::logstf_class_weapon_icon(TfClass::Scout, "scattergun");
    assert!(
        !support::present(cx, s.win, icon.clone()),
        "no game, no icons"
    );
    load_icons(cx, &env, &icons);
    support::settle(cx, s.win);
    assert!(
        support::present(cx, s.win, icon),
        "the page re-renders when they land"
    );
}

// The left edges of a panel row and of its name text.
fn name_edges(cx: &mut TestAppContext, s: &support::ShellWindow, weapon: &str) -> (f32, f32) {
    let row = support::bounds_of(cx, s.win, ids::logstf_class_weapon(TfClass::Scout, weapon));
    let name = support::bounds_of(
        cx,
        s.win,
        ids::logstf_class_weapon_name(TfClass::Scout, weapon),
    );
    let (row, name) = (row.expect("the row"), name.expect("its name"));
    (f32::from(row.left()), f32::from(name.left()))
}

#[gpui_kit::test]
fn panel_names_line_up_behind_an_icon_slot_only_when_icons_show(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    serve_sixes(&server);
    let s = shell(cx, &env, &server.uri(), &[(ARIE, "Arie")]);
    let icons = install_icons(cx);
    let page = open_page(cx, &s);
    support::click_id(cx, s.win, ids::logstf_class(TfClass::Scout));
    wait_for(cx, &page, TfClass::Scout);
    let (row, name) = name_edges(cx, &s, "scattergun");
    assert!((name - row).abs() <= 0.5, "no game, no slot: {row} {name}");

    load_icons(cx, &env, &icons);
    support::settle(cx, s.win);
    let (row, iconed) = name_edges(cx, &s, "scattergun");
    let (_, plain) = name_edges(cx, &s, "pistol_scout");
    assert!(
        (iconed - plain).abs() <= 0.5,
        "names line up: {iconed} {plain}"
    );
    assert!(iconed - row >= 64., "a 64 px slot first: {row} {iconed}");
}
