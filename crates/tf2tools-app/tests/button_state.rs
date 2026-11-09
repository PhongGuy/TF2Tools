//! Pins the kit behaviour `support::disabled_of` reads: gpui-base 0.9's Button marks its a11y
//! node disabled only while it is.
use crate::support;

use gpui_kit::component::Disableable as _;
use gpui_kit::component::button::Button;
use gpui_kit::prelude::*;
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{Context, TestAppContext, Window, div};

struct Pair;

impl Render for Pair {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .child(Button::new("enabled").label("On"))
            .child(Button::new("disabled").label("Off").disabled(true))
    }
}

#[gpui_kit::test]
fn a_kit_button_reads_disabled_only_when_it_is(cx: &mut TestAppContext) {
    support::init(cx);
    let win = cx
        .update(|cx| gpui_kit::open_window(support::window_options(), cx, |_, cx| cx.new(|_| Pair)))
        .unwrap()
        .0;
    assert!(!support::disabled_of(cx, win, "enabled"));
    assert!(support::disabled_of(cx, win, "disabled"));
    let flags = cx
        .update_window(win, |_, w, _| {
            (w.find("enabled").disabled(), w.find("disabled").disabled())
        })
        .unwrap();
    assert_ne!(flags.0, Some(true), "the enabled button reads disabled");
    assert_eq!(flags.1, Some(true), "the disabled button lacks the flag");
}
