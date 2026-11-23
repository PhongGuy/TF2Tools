//! The sortable headers the logs.tf tables share. A titled header is a button filling its
//! column, its title beside a slot for the sorted column's arrow; the kill matrix's icon
//! headers carry the arrow in a row under them. Either keeps the arrow's room when unsorted,
//! so sorting never moves the table.

use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::{Icon, Selectable as _, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    Div, ElementId, InteractiveElement as _, IntoElement, ParentElement as _, Pixels, SharedString,
    StatefulInteractiveElement as _, Styled as _, TestSupportExt as _, div, px,
};

use crate::i18n::tr;

// The arrow slot's size, an xsmall icon's.
const ARROW: Pixels = px(12.);

/// The header button `id` reading `label` in an element `title`, the title and arrow slot
/// centred together, or at the button's start when `start`. `sorted` is the direction when
/// this column leads, and the arrow then carries `arrow` as its id. The caller sizes it.
pub(super) fn sortable(
    id: ElementId,
    title: ElementId,
    label: SharedString,
    arrow: ElementId,
    sorted: Option<bool>,
    start: bool,
) -> Button {
    let slot = div()
        .size(ARROW)
        .flex_none()
        .when_some(sorted, |el, descending| {
            el.child(sort_arrow(arrow, descending))
        });
    let content = h_flex()
        .flex_1()
        .min_w_0()
        .gap_0p5()
        .when(!start, |row| row.justify_center())
        .child(div().id(title).test_support().child(label.clone()))
        .child(slot);
    Button::new(id)
        .ghost()
        .xsmall()
        .accessibility_label(label)
        .child(content)
        .selected(sorted.is_some())
        .flex_none()
        .px_0()
}

/// `button` over its arrow row, for the kill matrix's icon headers; `sorted` is the direction
/// when this column leads, and the arrow then carries `arrow` as its id.
pub(super) fn arrow_under(button: Button, arrow: ElementId, sorted: Option<bool>) -> Div {
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
        (IconName::ChevronDown, tr!("logs_tf.sort.descending"))
    } else {
        (IconName::ChevronUp, tr!("logs_tf.sort.ascending"))
    };
    div()
        .id(id)
        .aria_label(words)
        .test_support()
        .child(Icon::new(icon).xsmall())
}
