//! v5's compact like and view counts, shared by the store cards and detail bodies.

use gpui_kit::assets::IconName;
use gpui_kit::component::{Icon, Sizable as _, h_flex};
use gpui_kit::{ElementId, IntoElement, ParentElement as _, Styled as _, div};

use crate::widgets::labelled;

/// v5's `formatCount`: "999", "1.3K", "62.5K", "1.2M"; rounds as JS `toFixed(1)` on the f64 quotient
/// (exact binary ties such as 1,250 round up, 1,150 reads "1.1K").
pub fn format_count(n: i64) -> String {
    if n >= 1_000_000 {
        compact(n, 1_000_000, 'M')
    } else if n >= 1_000 {
        compact(n, 1_000, 'K')
    } else {
        n.to_string()
    }
}

// Rust's `{:.1}` already rounds the exact binary value as `toFixed` does, except on a tie exact in
// binary (a quarter or three quarters of a unit), where it rounds to even and `toFixed` rounds up.
fn compact(n: i64, unit: i64, suffix: char) -> String {
    if n % (unit / 2) == unit / 4 {
        let tenths = (n * 10 + unit / 2) / unit;
        format!("{}.{}{suffix}", tenths / 10, tenths % 10)
    } else {
        format!("{:.1}{suffix}", n as f64 / unit as f64)
    }
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
