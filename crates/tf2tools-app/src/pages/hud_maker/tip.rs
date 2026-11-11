//! The HUD Maker's tooltip helpers: holders that show a tooltip over a possibly disabled
//! control, and tooltips anchored beside a panel row.

use std::cell::Cell;
use std::rc::Rc;

use gpui_kit::{
    AnyElement, AnyView, App, Bounds, ElementId, InteractiveElement as _, IntoElement,
    ParentElement as _, Pixels, Point, SharedString, StatefulInteractiveElement as _, Styled as _,
    TestSupportExt as _, Window, canvas, div, point, px,
};

use crate::ids;
use crate::widgets::tip::{tip, tip_view};

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
        let shift = beside_row(window.mouse_position(), control.get(), row.get());
        tip_view(id, text.clone(), shift, cx)
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
