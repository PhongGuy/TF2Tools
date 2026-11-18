//! Reusable widgets the pages share.

pub mod card_grid;
pub mod carousel;
pub mod class_icon;
pub mod crosshair_picker;
pub mod detail_header;
pub mod dialog_frame;
pub mod label_option;
pub mod profile_menu;
pub mod scroll;
pub mod store_error;
pub mod sync_banner;
pub mod sync_ring;
pub mod tip;
pub mod weapon_assigner;

use gpui_kit::component::ActiveTheme as _;
use gpui_kit::component::button::Button;
use gpui_kit::{
    App, Div, ElementId, FontWeight, InteractiveElement as _, IntoElement, ParentElement as _,
    Pixels, Rems, SharedString, StatefulInteractiveElement as _, TestSupportExt as _, TextRun,
    Window, font, rems,
};

/// Gives an icon-only button its words: the tooltip a player sees on hover and the name a
/// screen reader and the pilot read, kept the same so the two never drift apart.
pub(crate) trait Named {
    fn named(self, text: impl Into<SharedString>) -> Self;
}

impl Named for Button {
    fn named(self, text: impl Into<SharedString>) -> Self {
        let text = text.into();
        self.tooltip(text.clone()).accessibility_label(text)
    }
}

/// Gives `button` the label `text` when it shapes within `budget` at a button's text size,
/// else leaves it icon-only and `named` with `text`, so a long translation keeps a card's
/// action row inside the card.
pub(crate) fn label_or_named(
    button: Button,
    text: SharedString,
    budget: Pixels,
    window: &Window,
    cx: &App,
) -> Button {
    if label_width(&text, window, cx) <= budget {
        button.label(text)
    } else {
        button.named(text)
    }
}

/// The width `text` shapes to as a button's label, in the theme font at a button's text size.
pub(crate) fn label_width(text: &SharedString, window: &Window, cx: &App) -> Pixels {
    label_width_at(text, rems(0.875), window, cx)
}

/// The width `text` shapes to as a label in the theme font's medium weight at `size`.
pub(crate) fn label_width_at(text: &SharedString, size: Rems, window: &Window, cx: &App) -> Pixels {
    let run = TextRun {
        len: text.len(),
        font: gpui_kit::Font {
            weight: FontWeight::MEDIUM,
            ..font(cx.theme().font_family.clone())
        },
        ..Default::default()
    };
    let size = size.to_pixels(window.rem_size());
    window
        .text_system()
        .shape_line(text.clone(), size, &[run], None)
        .width
}

// A text element, styled by `base`, that the tests and the pilot read back by id.
pub(crate) fn labelled(
    base: Div,
    id: impl Into<ElementId>,
    text: impl Into<SharedString>,
) -> impl IntoElement {
    let text = text.into();
    base.id(id)
        .aria_label(text.clone())
        .test_support()
        .child(text)
}
