//! v5's compact like and view counts, shared by the store cards and detail bodies.

use gpui_kit::assets::IconName;
use gpui_kit::component::{Icon, Sizable as _, h_flex};
use gpui_kit::{ElementId, IntoElement, ParentElement as _, Styled as _, div};

use crate::i18n::format;
use crate::widgets::labelled;

/// v5's `formatCount`: "999", "1.3K", "62.5K", "1.2M" in English; rounds as JS `toFixed(1)`
/// (exact binary ties such as 1,250 round up, 1,150 reads "1.1K").
pub fn format_count(n: i64) -> String {
    format::compact_count(n)
}

/// A muted like/view stat: a 12 px `icon` then `format_count(count)` in a text element with
/// `id` as its id and accessible label. Text size and colour come from the parent.
pub(crate) fn count_stat(id: impl Into<ElementId>, icon: IconName, count: i64) -> impl IntoElement {
    h_flex()
        .gap_1()
        .items_center()
        .child(Icon::new(icon).xsmall())
        .child(labelled(div(), id, format_count(count)))
}
