//! Tooltips for the HUD Maker's panels and the logs.tf tables' names, on GPUI's own tooltip,
//! not the kit's window-wide one: it drops on any press or scroll and whenever its control
//! leaves the pointer, so it never stays behind when a panel scrolls or redraws under it.

use std::cell::Cell;
use std::rc::Rc;

use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::{
    AnyElement, AnyView, App, AppContext as _, Bounds, Context, ElementId, InteractiveElement as _,
    IntoElement, ParentElement as _, Pixels, Point, Render, SharedString,
    StatefulInteractiveElement as _, Styled as _, TestSupportExt as _, Window, canvas, div, point,
    px,
};

use crate::ids;

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
    move |_, cx| {
        let text = text.clone();
        cx.new(|_| Tip {
            id,
            text,
            shift: Point::default(),
        })
        .into()
    }
}

/// `control` inside a holder (`ids::hud_maker_tip_for(of)`) that shows `text` as a tooltip read
/// under `ids::HUD_MAKER_TIP`; the holder still shows it while the control is disabled.
pub(super) fn tipped(
    of: &ElementId,
    text: impl Into<SharedString>,
    control: impl IntoElement,
) -> AnyElement {
    div()
        .id(ids::hud_maker_tip_for(of))
        .test_support()
        .tooltip(tip(ids::HUD_MAKER_TIP, text))
        .child(control)
        .into_any_element()
}

/// Where a control's bounds sit while it draws, for a tooltip that anchors to it.
pub(super) type Spot = Rc<Cell<Bounds<Pixels>>>;

/// An invisible child that records its parent's bounds into `spot` each frame.
pub(super) fn record(spot: &Spot) -> impl IntoElement + use<> {
    let spot = Rc::clone(spot);
    canvas(move |bounds, _, _| spot.set(bounds), |_, _, _, _| {})
        .absolute()
        .top_0()
        .left_0()
        .size_full()
}

/// How far to move a tooltip that GPUI puts a pixel below and right of `mouse`, so it starts
/// level with the top of `control` and just right of `row`, clear of the buttons beside it.
pub(super) fn beside_row(
    mouse: Point<Pixels>,
    control: Bounds<Pixels>,
    row: Bounds<Pixels>,
) -> Point<Pixels> {
    point(
        row.right() + px(4.) - (mouse.x + px(1.)),
        control.top() - (mouse.y + px(1.)),
    )
}

/// A builder for `.tooltip(...)` that shows `text` level with `control` and right of `row`,
/// however far into the control the pointer stopped.
pub(super) fn tip_beside_row(
    id: &'static str,
    text: impl Into<SharedString>,
    control: Spot,
    row: Spot,
) -> impl Fn(&mut Window, &mut App) -> AnyView + 'static {
    let text = text.into();
    move |window, cx| {
        let text = text.clone();
        let shift = beside_row(window.mouse_position(), control.get(), row.get());
        cx.new(|_| Tip { id, text, shift }).into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui_kit::size;

    #[test]
    fn a_tooltip_moves_level_with_its_control_and_right_of_its_row() {
        let control = Bounds::new(point(px(100.), px(40.)), size(px(28.), px(28.)));
        let row = Bounds::new(point(px(10.), px(40.)), size(px(150.), px(28.)));
        let at = beside_row(point(px(120.), px(66.)), control, row);
        // GPUI starts the tooltip at (121, 67); it must start at (164, 40).
        assert_eq!((at.x, at.y), (px(43.), px(-27.)));
    }
}
