use gpui_kit::{AnyWindowHandle, AppContext as _, Entity, TestAppContext};
use tf2tools_app::ids;
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
