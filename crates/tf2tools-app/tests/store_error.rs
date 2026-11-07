use crate::support;

use std::cell::Cell;
use std::rc::Rc;

use gpui_kit::prelude::*;
use gpui_kit::{Context, TestAppContext, TestSupportExt as _, Window, div, px};
use tf2tools_app::ids;
use tf2tools_app::widgets::store_error::{error_line, store_error_line};

struct Host {
    retries: Rc<Cell<usize>>,
}

impl Render for Host {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let retries = self.retries.clone();
        div().size_full().child(store_error_line(
            "Failed to load the store",
            move |_, _| retries.set(retries.get() + 1),
            cx,
        ))
    }
}

#[gpui_kit::test]
fn the_line_shows_the_error_and_retry_calls_back(cx: &mut TestAppContext) {
    support::init(cx);
    let retries = Rc::new(Cell::new(0));
    let host_retries = retries.clone();
    let (win, _host) = cx
        .update(|cx| {
            gpui_kit::open_window(support::window_options(), cx, move |_, cx| {
                cx.new(|_| Host {
                    retries: host_retries,
                })
            })
        })
        .unwrap();
    assert_eq!(
        support::label_of(cx, win, ids::STORE_ERROR).as_deref(),
        Some("Failed to load the store")
    );
    assert_eq!(
        support::label_of(cx, win, ids::STORE_RETRY).as_deref(),
        Some("Retry")
    );
    support::click_id(cx, win, ids::STORE_RETRY);
    assert_eq!(retries.get(), 1);
}

// The longest store line, in a pane narrower than the text: it wraps and Retry stays inside.
struct Narrow;

impl Render for Narrow {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .id("narrow-host")
            .w(px(320.))
            .child(store_error_line(
                "TF2Tools couldn't read the store. Restart TF2Tools and try again.",
                |_, _| {},
                cx,
            ))
            // Last: it returns an ObservedElement, which makes the host findable by id.
            .test_support()
    }
}

#[gpui_kit::test]
fn retry_stays_inside_a_narrow_pane(cx: &mut TestAppContext) {
    support::init(cx);
    let (win, _host) = cx
        .update(|cx| {
            gpui_kit::open_window(support::window_options(), cx, |_, cx| cx.new(|_| Narrow))
        })
        .unwrap();
    let host = support::bounds_of(cx, win, "narrow-host").expect("the host");
    let text = support::bounds_of(cx, win, ids::STORE_ERROR).expect("the message");
    let retry = support::bounds_of(cx, win, ids::STORE_RETRY).expect("Retry");
    assert!(
        retry.right() <= host.right(),
        "{retry:?} runs past {host:?}"
    );
    assert!(
        text.right() <= retry.left(),
        "{text:?} runs under {retry:?}"
    );
    // One text_sm line measures 22.5 px; two lines clear 36 px.
    assert!(
        text.size.height >= px(36.),
        "the message wraps onto two lines: {text:?}"
    );
}

// The Logs page's line shares the store line's layout: in a narrow pane it wraps and Retry stays inside.
struct NarrowLogs;

impl Render for NarrowLogs {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .id("narrow-logs-host")
            .w(px(320.))
            .child(error_line(
                ids::LOGS_ERROR,
                ids::LOGS_RETRY,
                "TF2Tools couldn't open the activity log. Restart TF2Tools and try again.",
                |_, _| {},
                cx,
            ))
            // Last: it returns an ObservedElement, which makes the host findable by id.
            .test_support()
    }
}

#[gpui_kit::test]
fn the_logs_line_keeps_retry_inside_a_narrow_pane(cx: &mut TestAppContext) {
    support::init(cx);
    let (win, _host) = cx
        .update(|cx| {
            gpui_kit::open_window(support::window_options(), cx, |_, cx| {
                cx.new(|_| NarrowLogs)
            })
        })
        .unwrap();
    let host = support::bounds_of(cx, win, "narrow-logs-host").expect("the host");
    let text = support::bounds_of(cx, win, ids::LOGS_ERROR).expect("the message");
    let retry = support::bounds_of(cx, win, ids::LOGS_RETRY).expect("Retry");
    assert!(
        retry.right() <= host.right(),
        "{retry:?} runs past {host:?}"
    );
    assert!(
        text.right() <= retry.left(),
        "{text:?} runs under {retry:?}"
    );
    assert!(
        text.size.height >= px(36.),
        "the message wraps onto two lines: {text:?}"
    );
}
