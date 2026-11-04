//! The stand-in body for a page not built yet, drawn like v5's EmptyState.

use gpui_kit::assets::IconName;
use gpui_kit::component::{ActiveTheme as _, Icon, Sizable as _, Size, v_flex};
use gpui_kit::{
    App, FontWeight, InteractiveElement as _, IntoElement, ParentElement as _,
    StatefulInteractiveElement as _, Styled as _, TestSupportExt as _, div, px,
};

use crate::ids;
use crate::shell::CONTENT_PADDING;
use crate::shell::route::Page;

/// Element `page-<slug>` holding the page title and `placeholder-message`, centred.
pub fn render(page: Page, cx: &App) -> impl IntoElement + use<> {
    let muted = cx.theme().muted_foreground;
    v_flex()
        .id(ids::page(page))
        .test_support()
        .size_full()
        .p(CONTENT_PADDING)
        .items_center()
        .justify_center()
        .gap_2()
        .child(
            Icon::new(IconName::PackageOpen)
                .with_size(Size::Size(px(48.)))
                .text_color(muted),
        )
        .child(
            div()
                .text_lg()
                .font_weight(FontWeight::SEMIBOLD)
                .child(page.title()),
        )
        .child(
            div()
                .id(ids::PLACEHOLDER_MESSAGE)
                .aria_label("Coming in a later version")
                .test_support()
                .text_sm()
                .text_color(muted)
                .child("Coming in a later version"),
        )
}
