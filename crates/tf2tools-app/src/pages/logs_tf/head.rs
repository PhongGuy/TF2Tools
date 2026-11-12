//! The sortable header the logs.tf tables share: a button over a row that holds the sorted
//! column's arrow, kept empty when unsorted so sorting never moves the table.

use gpui_kit::assets::IconName;
use gpui_kit::component::button::Button;
use gpui_kit::component::{Icon, Sizable as _, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    Div, ElementId, InteractiveElement as _, IntoElement, ParentElement as _, Pixels,
    StatefulInteractiveElement as _, Styled as _, TestSupportExt as _, div, px,
};

// The arrow row's height, an xsmall icon's.
const ARROW: Pixels = px(12.);

/// `button` over its arrow row; `sorted` is the direction when this column leads, and the
/// arrow then carries `arrow` as its id.
pub(super) fn sortable(button: Button, arrow: ElementId, sorted: Option<bool>) -> Div {
    let row = div()
        .h(ARROW)
        .flex_none()
        .when_some(sorted, |el, descending| {
            el.child(sort_arrow(arrow, descending))
        });
    v_flex().flex_none().items_center().child(button).child(row)
}

// The direction as a small chevron, said in words for readers.
fn sort_arrow(id: ElementId, descending: bool) -> impl IntoElement {
    let (icon, words) = if descending {
        (IconName::ChevronDown, "high to low")
    } else {
        (IconName::ChevronUp, "low to high")
    };
    div()
        .id(id)
        .aria_label(words)
        .test_support()
        .child(Icon::new(icon).xsmall())
}
