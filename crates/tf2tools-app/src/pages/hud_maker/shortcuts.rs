//! The HUD Maker's keyboard shortcuts sheet: a dialog listing every key and pointer trick the
//! page knows, opened by the "?" button beside the page title or the ? key.

use gpui_kit::component::button::Button;
use gpui_kit::component::{ActiveTheme as _, WindowExt as _, h_flex, v_flex};
use gpui_kit::{
    Context, FontWeight, InteractiveElement as _, IntoElement, ParentElement as _,
    StatefulInteractiveElement as _, Styled as _, TestSupportExt as _, Window, div, px,
};

use super::HudMakerPage;
use crate::ids;
use crate::widgets::dialog_frame::{DIALOG_WIDTH, DialogFrame};

/// The sheet's rows: the keys, then what they do.
pub const SHORTCUTS: [(&str, &str); 15] = [
    ("Arrow keys", "Move the selected part"),
    ("Shift + arrow keys", "Move it 10 at a time"),
    ("Ctrl + drag", "Move without snapping"),
    ("H or Delete", "Hide the selected part"),
    ("Escape", "Clear the selection"),
    ("Alt + click", "Pick a part inside a group"),
    ("Ctrl+Z", "Undo"),
    ("Ctrl+Y", "Redo"),
    ("Space + drag", "Move around the canvas"),
    ("Middle mouse drag", "Move around the canvas"),
    ("Mouse wheel", "Scroll the canvas, sideways with Shift"),
    ("Ctrl + mouse wheel", "Zoom in or out where the pointer is"),
    ("Ctrl + plus or minus", "Zoom in or out"),
    ("Ctrl+0", "Fit the screen to the window"),
    ("?", "Show these shortcuts"),
];

impl HudMakerPage {
    /// Opens the shortcuts sheet; Escape, the X or Close shuts it.
    pub fn show_shortcuts(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.let_go(cx);
        // Deferred so the pressed button cannot pull focus back out of the dialog.
        window.defer(cx, |window, cx| {
            let frame = DialogFrame::new(DIALOG_WIDTH);
            window.open_dialog(cx, move |dialog, window, cx| {
                frame
                    .apply(dialog, &*window, &*cx)
                    .on_ok(|_, _, _| true)
                    .on_cancel(|_, _, _| true)
                    .child(frame.measure(sheet(cx)))
            });
        });
    }
}

// The title, then one row per shortcut with its keys in a fixed column, then Close.
fn sheet(cx: &gpui_kit::App) -> impl IntoElement + use<> {
    let theme = cx.theme();
    let (muted, key_bg, border) = (theme.muted_foreground, theme.muted, theme.border);
    let rows = SHORTCUTS.iter().enumerate().map(|(i, (keys, does))| {
        h_flex()
            .id(ids::hud_maker_shortcut(i))
            .aria_label(format!("{keys}: {does}"))
            .test_support()
            .gap_3()
            .text_sm()
            .child(
                div().w(px(170.)).flex_none().child(
                    div()
                        .px_1p5()
                        .rounded_sm()
                        .border_1()
                        .border_color(border)
                        .bg(key_bg)
                        .text_xs()
                        .child(*keys),
                ),
            )
            .child(div().min_w_0().child(*does))
    });
    v_flex()
        .id(ids::HUD_MAKER_SHORTCUTS_SHEET)
        .test_support()
        .gap_4()
        .child(
            v_flex()
                .gap_2()
                .child(
                    div()
                        .text_lg()
                        .font_weight(FontWeight::SEMIBOLD)
                        .child("Keyboard shortcuts"),
                )
                .child(
                    div()
                        .text_sm()
                        .text_color(muted)
                        .child("Click a part first for the keys that change it."),
                ),
        )
        .child(v_flex().gap_2().items_start().children(rows))
        .child(
            h_flex().justify_end().child(
                Button::new(ids::HUD_MAKER_SHORTCUTS_CLOSE)
                    .outline()
                    .label("Close")
                    .on_click(|_, window, cx| window.close_dialog(cx)),
            ),
        )
}
