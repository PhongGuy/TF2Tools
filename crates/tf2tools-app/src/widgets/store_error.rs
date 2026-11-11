//! The inline error line the stores and the Logs page share.

use gpui_kit::component::button::Button;
use gpui_kit::component::{ActiveTheme as _, h_flex};
use gpui_kit::{
    App, ElementId, IntoElement, ParentElement as _, SharedString, Styled as _, Window, div,
};

use crate::ids;
use crate::widgets::labelled;

/// The stores' inline error: `error_line` with `ids::STORE_ERROR` and `ids::STORE_RETRY`.
pub fn store_error_line(
    error: impl Into<SharedString>,
    on_retry: impl Fn(&mut Window, &mut App) + 'static,
    cx: &App,
) -> impl IntoElement {
    error_line(ids::STORE_ERROR, ids::STORE_RETRY, error, on_retry, cx)
}

/// An inline error: the message in the danger colour (`text_id`, labelled with the message)
/// and an outline "Retry" button (`retry_id`) that calls `on_retry`.
pub fn error_line(
    text_id: impl Into<ElementId>,
    retry_id: impl Into<ElementId>,
    error: impl Into<SharedString>,
    on_retry: impl Fn(&mut Window, &mut App) + 'static,
    cx: &App,
) -> impl IntoElement {
    h_flex()
        .gap_3()
        .text_sm()
        .text_color(cx.theme().danger)
        // The message takes the room left of Retry and wraps there, so Retry never leaves a
        // narrow pane.
        .child(labelled(div().flex_1().min_w_0(), text_id, error))
        .child(
            Button::new(retry_id)
                .outline()
                .label("Retry")
                .on_click(move |_, window, cx| on_retry(window, cx)),
        )
}
