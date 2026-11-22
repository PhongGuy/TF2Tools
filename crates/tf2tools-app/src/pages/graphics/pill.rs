//! The dark pill a segmented control sits in, shared by the preset and the row choices.

use gpui_kit::component::ActiveTheme as _;
use gpui_kit::component::h_flex;
use gpui_kit::{App, Div, FontWeight, Styled, px};

/// The dark track a segmented control sits in: secondary fill, 4 px inset, rounded.
pub(crate) fn track(cx: &App) -> Div {
    h_flex()
        .gap_1()
        .p_1()
        .rounded(px(8.))
        .bg(cx.theme().secondary)
}

/// The tighter track a settings row's choices sit in: 3 px inset with 2 px between segments.
/// It wraps its segments onto a second line when the row is narrower than the track.
pub(crate) fn row_track(cx: &App) -> Div {
    track(cx).p(px(3.)).gap(px(2.)).flex_wrap()
}

/// Styles one segment: background fill, foreground text and a small shadow when checked;
/// transparent with muted text otherwise.
pub(crate) fn segment_look<E: Styled>(el: E, checked: bool, cx: &App) -> E {
    let theme = cx.theme();
    let el = el
        .rounded(px(6.))
        .border_0()
        .font_weight(FontWeight::MEDIUM);
    if checked {
        el.bg(theme.background)
            .text_color(theme.foreground)
            .shadow_xs()
    } else {
        el.bg(gpui_kit::transparent_black())
            .text_color(theme.muted_foreground)
    }
}
