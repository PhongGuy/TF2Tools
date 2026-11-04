//! The one scroll area every scrolling page and panel shares: content with a vertical scrollbar
//! laid over its right edge, always drawn while the content overflows so the player sees there
//! is more below.

use gpui_kit::component::scroll::{Scrollbar, ScrollbarHandle, ScrollbarMode};
use gpui_kit::component::v_flex;
use gpui_kit::{
    Div, ElementId, InteractiveElement as _, IntoElement, ParentElement as _, Pixels, Styled as _,
    TestSupportExt as _, div, px,
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
