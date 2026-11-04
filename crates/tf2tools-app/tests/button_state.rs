//! Pins the kit behaviour `support::disabled_of` reads: gpui-base 0.7.1's Button binds its
//! focus handle only while enabled, and exposes no a11y disabled flag.
mod support;

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
    // When the kit grows an a11y disabled flag, read that in `button_disabled` instead.
    let flag = cx
        .update_window(win, |_, w, _| w.find("disabled").disabled())
        .unwrap();
    assert_eq!(flag, None);
}
