//! The one scroll area every scrolling page and panel shares: content with a vertical scrollbar
//! laid over its right edge, always drawn while the content overflows so the player sees there
//! is more below.

use gpui_kit::component::scroll::{ScrollableMask, Scrollbar, ScrollbarHandle, ScrollbarMode};
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::{
    AnyElement, App, Axis, Div, ElementId, InteractiveElement as _, IntoElement,
    ParentElement as _, Pixels, RenderOnce, ScrollHandle, StatefulInteractiveElement as _,
    Styled as _, TestSupportExt as _, Window, div, px,
};

/// The width of the bar's track, drawn at the right edge of its holder.
pub const BAR_WIDTH: Pixels = px(16.);

/// Wraps `content`, which tracks `handle` and scrolls itself, with a scrollbar over its right
/// edge. The bar's holder carries `bar_id` and spans the area. The caller sizes the returned
/// wrapper as it would have sized `content`, and keeps `handle` in its entity across frames.
pub fn scroll_area<H: ScrollbarHandle + Clone>(
    bar_id: impl Into<ElementId>,
    handle: &H,
    content: impl IntoElement,
) -> Div {
    v_flex()
        .relative()
        .child(content)
        .child(bar(bar_id, handle, div().absolute().inset_0()))
}

/// Like `scroll_area`, with the bar in a `BAR_WIDTH` gutter just right of `content`, for a
/// list whose rows fill its width. The gutter lies outside the wrapper, in the page padding.
pub fn scroll_beside<H: ScrollbarHandle + Clone>(
    bar_id: impl Into<ElementId>,
    handle: &H,
    content: impl IntoElement,
) -> Div {
    let holder = div()
        .absolute()
        .top_0()
        .bottom_0()
        .left_0()
        .right(-BAR_WIDTH);
    v_flex()
        .relative()
        .child(content)
        .child(bar(bar_id, handle, holder))
}

// Fills the placed `holder` with the bar, which draws its track at the holder's right edge.
fn bar<H: ScrollbarHandle + Clone>(
    bar_id: impl Into<ElementId>,
    handle: &H,
    holder: Div,
) -> impl IntoElement {
    holder.id(bar_id.into()).test_support().child(
        Scrollbar::vertical(handle)
            .mode(ScrollbarMode::Always)
            .viewport_from_layout(),
    )
}

/// A full-width box that scrolls `content` sideways, for a table wider than the window. It
/// takes only horizontal wheel input (a trackpad swipe, shift+wheel), so a plain wheel over it
/// scrolls the page around it. The box carries `id` and keeps its offset under that id.
pub fn sideways(id: impl Into<ElementId>, content: impl IntoElement) -> Sideways {
    Sideways {
        id: id.into(),
        content: content.into_any_element(),
    }
}

/// The element `sideways` builds.
#[derive(IntoElement)]
pub struct Sideways {
    id: ElementId,
    content: AnyElement,
}

impl RenderOnce for Sideways {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let handle = window
            .use_keyed_state(self.id.clone(), cx, |_, _| ScrollHandle::default())
            .read(cx)
            .clone();
        // The kit's `overflow_x_scrollbar` turns a plain wheel into sideways scrolling, which
        // traps the page. A horizontal mask beside a clipped area, as the kit's own markdown
        // tables use, consumes only horizontal-dominant deltas and lets the rest bubble.
        div()
            .id(self.id.clone())
            .test_support()
            .relative()
            .w_full()
            .child(
                h_flex()
                    .id((self.id.clone(), "area"))
                    .w_full()
                    .overflow_hidden()
                    .track_scroll(&handle)
                    .child(div().flex_none().min_w_full().child(self.content)),
            )
            .child(ScrollableMask::new(Axis::Horizontal, &handle).id(self.id))
            .child(
                div()
                    .absolute()
                    .inset_0()
                    .child(Scrollbar::horizontal(&handle).viewport_from_layout()),
            )
    }
}
