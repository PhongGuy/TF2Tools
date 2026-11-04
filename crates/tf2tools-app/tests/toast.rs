use gpui_kit::{AppContext as _, TestAppContext};
use tf2tools_app::services::OpError;
use tf2tools_app::shell::toast::{ToastKind, toast, toast_count, toast_error};

mod support;

fn count(cx: &mut TestAppContext, win: gpui_kit::AnyWindowHandle) -> usize {
    cx.update_window(win, |_, w, cx| toast_count(w, cx))
        .unwrap()
}

#[gpui_kit::test]
fn a_success_toast_reads_its_title_and_description(cx: &mut TestAppContext) {
    support::init(cx);
    let win = support::blank_window(cx);
    assert!(!support::present(cx, win, "toast"));
    cx.update_window(win, |_, w, cx| {
        toast(w, cx, ToastKind::Success, "HUD installed", "budhud")
    })
    .unwrap();
    assert_eq!(
        support::label_of(cx, win, "toast").as_deref(),
        Some("HUD installed. budhud")
    );
    assert_eq!(count(cx, win), 1);
}

#[gpui_kit::test]
fn an_op_error_toast_shows_the_action_and_the_core_message(cx: &mut TestAppContext) {
    support::init(cx);
    let win = support::blank_window(cx);
    cx.update_window(win, |_, w, cx| {
        toast_error(w, cx, &OpError::core("Install HUD", "disk full"))
    })
    .unwrap();
    assert_eq!(
        support::label_of(cx, win, "toast").as_deref(),
        Some("Install HUD. disk full")
    );
}

#[gpui_kit::test]
fn the_no_tf2_toast_is_v5s_text(cx: &mut TestAppContext) {
    support::init(cx);
    let win = support::blank_window(cx);
    cx.update_window(win, |_, w, cx| toast_error(w, cx, &OpError::NoTf2))
        .unwrap();
    assert_eq!(
        support::label_of(cx, win, "toast").as_deref(),
        Some("TF2 path not set")
    );
}

#[gpui_kit::test]
fn an_info_toast_reads_its_title_and_description(cx: &mut TestAppContext) {
    support::init(cx);
    let win = support::blank_window(cx);
    cx.update_window(win, |_, w, cx| {
        toast(
            w,
            cx,
            ToastKind::Info,
            "Another download is running",
            "Wait for it to finish, then try again.",
        )
    })
    .unwrap();
    assert_eq!(
        support::label_of(cx, win, "toast").as_deref(),
        Some("Another download is running. Wait for it to finish, then try again.")
    );
    assert_eq!(count(cx, win), 1);
}
