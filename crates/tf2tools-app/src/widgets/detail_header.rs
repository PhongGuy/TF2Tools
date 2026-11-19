//! The header row every store details window shares: the item's name block, then its actions.

use gpui_kit::component::{ActiveTheme as _, h_flex, v_flex};
use gpui_kit::{
    AnyElement, App, Div, ElementId, Font, FontWeight, ParentElement as _, Pixels, SharedString,
    Styled as _, TextRun, Window, black, px, rems,
};

use crate::widgets::labelled;

/// The name's line height, so one line of name is exactly this tall.
pub const NAME_LINE_HEIGHT: Pixels = px(28.);

/// The header: the name (read back by `name_id`) with `badge` after it and `author` under it,
/// then `actions`. A name keeps its words whole: when it cannot sit beside the actions they
/// move under it, and only a single word wider than the dialog breaks.
pub fn detail_header(
    name_id: impl Into<ElementId>,
    name: SharedString,
    badge: Option<AnyElement>,
    author: Option<AnyElement>,
    actions: impl IntoIterator<Item = AnyElement>,
    window: &Window,
    cx: &App,
) -> Div {
    let longest_word = longest_word_width(&name, window, cx);
    // The kit's close X floats into the top right corner, so the row keeps 22 px clear.
    h_flex()
        .flex_wrap()
        .items_start()
        .gap_3()
        .pr(px(22.))
        .child(
            // Based at the longest word, so the row wraps the actions before a word breaks;
            // capped at the row so a word wider than the dialog wraps inside it.
            v_flex()
                .flex_1()
                .flex_basis(longest_word)
                .min_w_0()
                .max_w_full()
                .child(
                    // The badge rides after the name, or drops under it when both do not fit.
                    h_flex()
                        .flex_wrap()
                        .items_center()
                        .gap_2()
                        .child(labelled(
                            gpui_kit::div()
                                .min_w_0()
                                .max_w_full()
                                .text_xl()
                                .line_height(NAME_LINE_HEIGHT)
                                .font_weight(FontWeight::SEMIBOLD),
                            name_id,
                            name.clone(),
                        ))
                        .children(badge),
                )
                .children(author),
        )
        .child(h_flex().flex_none().gap_2().children(actions))
}

// The widest word of `name` in the name's own font, rounded up so it never wraps itself.
fn longest_word_width(name: &str, window: &Window, cx: &App) -> Pixels {
    let font = Font {
        weight: FontWeight::SEMIBOLD,
        ..crate::theme::ui_font(cx.theme().font_family.clone())
    };
    let size = rems(1.25).to_pixels(window.rem_size());
    name.split_whitespace()
        .map(|word| {
            let run = TextRun {
                len: word.len(),
                font: font.clone(),
                color: black(),
                background_color: None,
                underline: None,
                strikethrough: None,
            };
            window
                .text_system()
                .shape_line(SharedString::from(word.to_owned()), size, &[run], None)
                .width
        })
        .fold(px(0.), Pixels::max)
        .ceil()
        + px(1.)
}
