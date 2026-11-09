//! Reusable widgets the pages share.

pub mod card_grid;
pub mod carousel;
pub mod crosshair_picker;
pub mod dialog_frame;
pub mod label_option;
pub mod scroll;
pub mod store_error;
pub mod sync_banner;
pub mod sync_ring;
pub mod weapon_assigner;

use gpui_kit::component::button::Button;
use gpui_kit::{
    Div, ElementId, InteractiveElement as _, IntoElement, ParentElement as _, SharedString,
    StatefulInteractiveElement as _, TestSupportExt as _,
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
