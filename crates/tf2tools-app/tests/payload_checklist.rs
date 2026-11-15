use gpui_kit::{AnyWindowHandle, AppContext as _, Entity, TestAppContext};
use gpui_kit::{px, size};
use tf2tools_app::ids;
use tf2tools_app::payload_checklist::open_map_checklist;
use tf2tools_app::payload_checklist::{ChecklistItem, PayloadChecklist, open_payload_checklist};

use crate::support;

type Answer = async_channel::Receiver<Option<Vec<String>>>;

fn open(
    cx: &mut TestAppContext,
    names: &[&str],
) -> (AnyWindowHandle, Entity<PayloadChecklist>, Answer) {
    support::init(cx);
    let win = support::blank_window(cx);
    let items = names
        .iter()
        .map(|n| ChecklistItem {
            name: (*n).to_owned(),
            label: format!("label {n}"),
            present: false,
        })
        .collect();
    let (list, rx) = cx
        .update_window(win, |_, w, cx| {
            open_payload_checklist("Toon FX", items, w, cx)
        })
        .unwrap();
    support::settle(cx, win);
    (win, list, rx)
}

#[gpui_kit::test]
fn the_first_payload_starts_checked_and_rows_show_labels(cx: &mut TestAppContext) {
    let (win, list, _rx) = open(cx, &["A.vpk", "B.vpk", "C"]);
    assert!(support::present(cx, win, ids::PAYLOAD_CHECKLIST));
    assert_eq!(
        cx.update(|cx| list.read(cx).checked().to_vec()),
        [true, false, false]
    );
    assert_eq!(
        support::label_of(cx, win, ids::payload_check(1)).as_deref(),
        Some("label B.vpk")
    );
}

#[gpui_kit::test]
fn install_answers_the_checked_names_in_row_order(cx: &mut TestAppContext) {
    let (win, _list, rx) = open(cx, &["A.vpk", "B.vpk", "C"]);
    support::click_id(cx, win, ids::payload_check(2));
    support::click_id(cx, win, ids::payload_check(0));
    support::click_id(cx, win, ids::payload_check(0));
    support::click_id(cx, win, ids::PAYLOAD_INSTALL);
    assert_eq!(
        rx.try_recv().unwrap(),
        Some(vec!["A.vpk".to_owned(), "C".to_owned()])
    );
    assert!(
        !support::present(cx, win, ids::PAYLOAD_CHECKLIST),
        "install closes it"
    );
}

#[gpui_kit::test]
fn cancel_answers_none(cx: &mut TestAppContext) {
    let (win, _list, rx) = open(cx, &["A.vpk", "B.vpk"]);
    support::click_id(cx, win, ids::PAYLOAD_CANCEL);
    assert_eq!(rx.try_recv().unwrap(), None);
}

#[gpui_kit::test]
fn nothing_checked_cannot_install(cx: &mut TestAppContext) {
    let (win, list, rx) = open(cx, &["A.vpk", "B.vpk"]);
    support::click_id(cx, win, ids::payload_check(0));
    assert_eq!(cx.update(|cx| list.read(cx).chosen()), Vec::<String>::new());
    support::click_id(cx, win, ids::PAYLOAD_INSTALL);
    assert!(rx.try_recv().is_err(), "a disabled Install sends nothing");
    assert!(support::present(cx, win, ids::PAYLOAD_CHECKLIST));
}

#[gpui_kit::test]
fn toggling_survives_a_redraw(cx: &mut TestAppContext) {
    let (win, list, _rx) = open(cx, &["A.vpk", "B.vpk"]);
    support::click_id(cx, win, ids::payload_check(1));
    support::settle(cx, win);
    support::settle(cx, win);
    assert_eq!(
        cx.update(|cx| list.read(cx).checked().to_vec()),
        [true, true]
    );
}

fn map_item(name: &str, present: bool) -> ChecklistItem {
    ChecklistItem {
        name: name.to_owned(),
        label: name.to_owned(),
        present,
    }
}

fn open_maps(
    cx: &mut TestAppContext,
    items: Vec<ChecklistItem>,
    skipped: bool,
) -> (AnyWindowHandle, Entity<PayloadChecklist>, Answer) {
    support::init(cx);
    let win = support::blank_window(cx);
    cx.simulate_window_resize(win, size(px(800.), px(600.)));
    let (list, rx) = cx
        .update_window(win, |_, w, cx| {
            open_map_checklist("City Pack", items, skipped, w, cx)
        })
        .unwrap();
    support::settle(cx, win);
    (win, list, rx)
}

#[gpui_kit::test]
fn a_map_pack_starts_with_every_new_map_checked(cx: &mut TestAppContext) {
    let items = vec![
        map_item("cp_a", false),
        map_item("cp_b", true),
        map_item("cp_c", false),
    ];
    let (win, list, rx) = open_maps(cx, items, true);
    assert_eq!(
        cx.update(|cx| list.read(cx).checked().to_vec()),
        [true, false, true]
    );
    assert_eq!(
        support::label_of(cx, win, ids::payload_check(1)).as_deref(),
        Some("cp_b · Already in the game")
    );
    assert_eq!(
        support::label_of(cx, win, ids::PAYLOAD_CHECKLIST_DESCRIPTION).as_deref(),
        Some(
            "City Pack holds several maps. Pick the ones to install. Some extra files in this download were skipped."
        )
    );
    support::click_id(cx, win, ids::payload_check(1));
    cx.update(|cx| list.update(cx, |l, cx| l.toggle(1, cx)));
    assert_eq!(
        cx.update(|cx| list.read(cx).checked().to_vec()),
        [true, false, true],
        "a map the game has can't be ticked"
    );
    support::click_id(cx, win, ids::PAYLOAD_INSTALL);
    assert_eq!(
        rx.try_recv().unwrap(),
        Some(vec!["cp_a".to_owned(), "cp_c".to_owned()])
    );
}

#[gpui_kit::test]
fn sixty_four_maps_scroll_and_keep_install_on_screen(cx: &mut TestAppContext) {
    let items = (0..64)
        .map(|i| map_item(&format!("cp_m{i}"), false))
        .collect();
    let (win, list, _rx) = open_maps(cx, items, false);
    assert!(cx.update(|cx| list.read(cx).checked().iter().all(|on| *on)));
    let install = support::bounds_of(cx, win, ids::PAYLOAD_INSTALL).expect("Install is drawn");
    assert!(
        install.origin.y + install.size.height <= px(600.),
        "{install:?}"
    );
    let rows = support::bounds_of(cx, win, ids::PAYLOAD_CHECKLIST_ROWS).unwrap();
    assert!(rows.size.height <= px(256.), "{rows:?}");
}
