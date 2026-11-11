//! The class weapons panel: a played class card opens its weapon sums over the range.

use std::cell::RefCell;
use std::rc::Rc;

use gpui_kit::{ElementId, Entity, TestAppContext};
use tf2tools_app::ids;
use tf2tools_app::pages::logs_tf::LogsTfPage;
use tf2tools_app::pages::logs_tf::text::{NO_WEAPONS_TEXT, counting_text, weapons_title};
use tf2tools_core::{LogsRange, TfClass};

use crate::support;
use crate::support::logstf::{ARIE, SIXES, day_ago, list_body, open_page, shell, to_end};

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
