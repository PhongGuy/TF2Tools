//! The one scroll area every scrolling page and panel shares: content with a vertical scrollbar
//! laid over its right edge, always drawn while the content overflows so the player sees there
//! is more below.

use gpui_kit::component::scroll::{Scrollbar, ScrollbarHandle, ScrollbarMode};
use gpui_kit::component::v_flex;
use gpui_kit::{
    Div, ElementId, InteractiveElement as _, IntoElement, ParentElement as _, Styled as _,
    TestSupportExt as _, div,
};

/// Wraps `content`, which tracks `handle` and scrolls itself, with a scrollbar over its right
/// edge. The bar's holder carries `bar_id` and spans the area. The caller sizes the returned
/// wrapper as it would have sized `content`, and keeps `handle` in its entity across frames.
pub fn scroll_area<H: ScrollbarHandle + Clone>(
    bar_id: impl Into<ElementId>,
    handle: &H,
    content: impl IntoElement,
) -> Div {
    v_flex().relative().child(content).child(
        div()
            .id(bar_id.into())
            .test_support()
            .absolute()
            .inset_0()
            .child(
                Scrollbar::vertical(handle)
                    .mode(ScrollbarMode::Always)
                    .viewport_from_layout(),
            ),
    )
}
