//! Tooltips on GPUI's own tooltip, not the kit's window-wide one: it drops on any press or
//! scroll and whenever its control leaves the pointer, so it never stays behind when a panel
//! scrolls or redraws under it.

use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::{
    AnyView, App, AppContext as _, Context, InteractiveElement as _, IntoElement,
    ParentElement as _, Pixels, Point, Render, SharedString, StatefulInteractiveElement as _,
    Styled as _, TestSupportExt as _, Window, div,
};

// The tooltip's content, under `id` so tests and the pilot can read its words.
struct Tip {
    id: &'static str,
    text: SharedString,
    shift: Point<Pixels>,
}

impl Render for Tip {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let content = div()
            .id(self.id)
            .aria_label(self.text.clone())
            .test_support()
            .child(Tooltip::new(self.text.clone()).build(window, cx));
        if self.shift == Point::default() {
            return content.into_any_element();
        }
        // GPUI starts a tooltip at the pointer. A shifted one is an out-of-flow child instead,
        // so the shift moves where it paints without touching the size GPUI places it by.
        div()
            .relative()
            .child(content.absolute().left(self.shift.x).top(self.shift.y))
            .into_any_element()
    }
}

/// A builder for `.tooltip(...)` showing `text`, its content readable under `id`.
pub(crate) fn tip(
    id: &'static str,
    text: impl Into<SharedString>,
) -> impl Fn(&mut Window, &mut App) -> AnyView + 'static {
    let text = text.into();
    move |_, cx| tip_view(id, text.clone(), Point::default(), cx)
}

/// The view a tooltip builder returns: `text` under `id`, painted `shift` away from where GPUI
/// places it.
pub(crate) fn tip_view(
    id: &'static str,
    text: SharedString,
    shift: Point<Pixels>,
    cx: &mut App,
) -> AnyView {
    cx.new(|_| Tip { id, text, shift }).into()
}
