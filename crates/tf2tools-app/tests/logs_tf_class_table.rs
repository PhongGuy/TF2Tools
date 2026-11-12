//! The class averages table: one compact row per class under shared column titles, a played
//! row opening its weapons right under it, scrolling sideways in a narrow window.

use gpui_kit::{
    AppContext as _, InputEvent as _, ScrollDelta, ScrollEvent, TestAppContext, point, px, size,
};
use tf2tools_app::ids;
use tf2tools_app::pages::logs_tf::text::one_decimal;
use tf2tools_core::TfClass;

use crate::support;
use crate::support::logstf::{ARIE, SIXES, day_ago, list_body, open_page, serve_two_logs, shell};

const COLUMNS: [&str; 7] = [
    "Played",
    "Logs",
    "Kills/min",
    "Assists/min",
    "Deaths/min",
    "Dmg/min",
    "KA/D",
];

fn serve_sixes(server: &support::StoreServer) {
    server.mount_bytes("/api/v1/log", list_body(&[(3, day_ago(1))]));
    server.mount_bytes("/json/3", SIXES.as_bytes().to_vec());
}

#[gpui_kit::test]
fn the_classes_stack_as_compact_rows_in_one_box(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    serve_two_logs(&server);
    let s = shell(cx, &env, &server.uri(), &[(ARIE, "Arie")]);
    let _page = open_page(cx, &s);
    let table = support::bounds_of(cx, s.win, ids::LOGSTF_CLASS_TABLE).expect("the table");
    let rows: Vec<_> = TfClass::ALL
        .into_iter()
        .map(|class| support::bounds_of(cx, s.win, ids::logstf_class(class)).unwrap())
        .collect();
    for pair in rows.windows(2) {
        assert!(pair[0].bottom() <= pair[1].top(), "{pair:?} out of order");
        assert_eq!(pair[0].left(), pair[1].left(), "{pair:?} not one column");
    }
    for row in &rows {
        assert!(row.size.height <= px(40.), "row {row:?} is not compact");
        assert!(row.left() >= table.left() && row.right() <= table.right());
    }
}

#[gpui_kit::test]
fn each_value_sits_right_aligned_under_its_column_title(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    serve_sixes(&server);
    let s = shell(cx, &env, &server.uri(), &[(ARIE, "Arie")]);
    let page = open_page(cx, &s);
    for label in COLUMNS {
        let head = support::bounds_of(cx, s.win, ids::logstf_class_head(label)).expect(label);
        for class in [TfClass::Scout, TfClass::Heavy] {
            let value = support::bounds_of(cx, s.win, ids::logstf_class_value(class, label))
                .unwrap_or_else(|| panic!("{class:?} {label}"));
            assert_eq!(value.right(), head.right(), "{class:?} {label}");
        }
    }
    assert_eq!(
        support::label_of(cx, s.win, ids::logstf_class_value(TfClass::Scout, "Logs")).as_deref(),
        Some("1")
    );
    // Assists read like kills, one decimal, in the column between Kills/min and Deaths/min.
    let scout = cx.update(|cx| page.read(cx).averages().unwrap()[0].clone());
    assert_eq!(
        support::label_of(
            cx,
            s.win,
            ids::logstf_class_value(TfClass::Scout, "Assists/min")
        ),
        Some(one_decimal(scout.assists_per_min()))
    );
    let head = |cx: &mut TestAppContext, label| {
        support::bounds_of(cx, s.win, ids::logstf_class_head(label)).unwrap()
    };
    let (kills, assists, deaths) = (
        head(cx, "Kills/min"),
        head(cx, "Assists/min"),
        head(cx, "Deaths/min"),
    );
    assert!(kills.right() <= assists.left() && assists.right() <= deaths.left());
    // Heals and ubers belong to Medic, so a Scout row leaves them empty.
    for label in ["Heals/min", "Ubers/10 min"] {
        assert!(support::present(cx, s.win, ids::logstf_class_head(label)));
        assert!(!support::present(
            cx,
            s.win,
            ids::logstf_class_value(TfClass::Scout, label)
        ));
    }
}

#[gpui_kit::test]
fn an_unplayed_row_says_so_in_the_played_column_and_takes_no_keyboard(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    serve_sixes(&server);
    let s = shell(cx, &env, &server.uri(), &[(ARIE, "Arie")]);
    let page = open_page(cx, &s);
    let note = support::bounds_of(cx, s.win, ids::logstf_class_unplayed(TfClass::Soldier))
        .expect("the note");
    let played = support::bounds_of(cx, s.win, ids::logstf_class_head("Played")).unwrap();
    assert_eq!(note.right(), played.right(), "{note:?} under {played:?}");
    for label in COLUMNS {
        assert!(!support::present(
            cx,
            s.win,
            ids::logstf_class_value(TfClass::Soldier, label)
        ));
    }
    assert_eq!(
        support::focused_of(cx, s.win, ids::logstf_class(TfClass::Soldier)),
        None
    );

    // A played row is a tab stop, and Enter opens its weapons.
    support::tab_to(cx, s.win, ids::logstf_class(TfClass::Scout), 64);
    support::press(cx, s.win, "enter");
    assert_eq!(
        cx.update(|cx| page.read(cx).selected_class()),
        Some(TfClass::Scout)
    );
}

#[gpui_kit::test]
fn the_weapons_open_indented_under_their_row_and_move_with_the_pick(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    serve_sixes(&server);
    let s = shell(cx, &env, &server.uri(), &[(ARIE, "Arie")]);
    let page = open_page(cx, &s);
    let under = |cx: &mut TestAppContext, class: TfClass, next: TfClass| {
        support::click_id(cx, s.win, ids::logstf_class(class));
        support::wait_real(cx, "the weapons", |cx| {
            cx.update(|cx| page.read(cx).class_weapons().map(|(c, _)| c)) == Some(class)
        });
        let row = support::bounds_of(cx, s.win, ids::logstf_class(class)).unwrap();
        let block = support::bounds_of(cx, s.win, ids::LOGSTF_CLASS_WEAPONS).expect("the block");
        let after = support::bounds_of(cx, s.win, ids::logstf_class(next)).unwrap();
        assert!(block.top() >= row.bottom(), "{block:?} not under {row:?}");
        assert!(
            block.bottom() <= after.top(),
            "{block:?} not over {after:?}"
        );
        assert!(block.left() > row.left(), "{block:?} not indented");
    };
    under(cx, TfClass::Scout, TfClass::Soldier);
    under(cx, TfClass::Pyro, TfClass::Demoman);
    assert!(!support::present(
        cx,
        s.win,
        ids::logstf_class_weapon(TfClass::Scout, "scattergun")
    ));
}

#[gpui_kit::test]
fn the_table_scrolls_sideways_in_the_smallest_window(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    serve_sixes(&server);
    let s = shell(cx, &env, &server.uri(), &[(ARIE, "Arie")]);
    cx.simulate_window_resize(s.win, size(px(800.), px(600.)));
    let _page = open_page(cx, &s);
    let table = support::bounds_of(cx, s.win, ids::LOGSTF_CLASS_TABLE).unwrap();
    let last = ids::logstf_class_head("Ubers/10 min");
    let ubers = support::bounds_of(cx, s.win, last.clone()).unwrap();
    assert!(
        ubers.right() > table.right(),
        "the table overflows at 800 px"
    );
    // The class names keep their room rather than squashing.
    let name = support::bounds_of(cx, s.win, ids::logstf_class_head("Class")).unwrap();
    assert!(name.size.width >= px(100.), "class column {name:?}");
    let wheel = ScrollEvent {
        position: point(table.center().x, table.top() + px(10.)),
        delta: ScrollDelta::Pixels(point(px(-2000.), px(0.))),
        ..Default::default()
    };
    cx.update_window(s.win, |_, w, cx| {
        let _ = w.dispatch_event(wheel.to_platform_input(), cx);
    })
    .unwrap();
    support::settle(cx, s.win);
    let table = support::bounds_of(cx, s.win, ids::LOGSTF_CLASS_TABLE).unwrap();
    let ubers = support::bounds_of(cx, s.win, last).unwrap();
    assert!(
        ubers.right() <= table.right() + px(1.) && ubers.left() >= table.left(),
        "Ubers {ubers:?} outside {table:?}"
    );
}
