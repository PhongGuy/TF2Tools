//! The HUD Maker's keyboard shortcuts sheet: a dialog listing every key and pointer trick the
//! page knows, opened by the "?" button beside the page title or the ? key.

use gpui_kit::component::button::Button;
use gpui_kit::component::{ActiveTheme as _, WindowExt as _, h_flex, v_flex};
use gpui_kit::{
    Context, FontWeight, InteractiveElement as _, IntoElement, ParentElement as _, SharedString,
    StatefulInteractiveElement as _, Styled as _, TestSupportExt as _, Window, div, px,
};

use super::HudMakerPage;
use crate::i18n::tr;
use crate::ids;
use crate::widgets::dialog_frame::{DIALOG_WIDTH, DialogFrame};

/// The sheet's rows: the keys, then what they do. Bare key caps print as the keyboard does.
pub fn shortcuts() -> [(SharedString, SharedString); 17] {
    [
        (
            tr!("hud_maker.shortcuts.keys.arrows"),
            tr!("hud_maker.shortcuts.does.move"),
        ),
        (
            tr!("hud_maker.shortcuts.keys.shift_arrows"),
            tr!("hud_maker.shortcuts.does.move_ten"),
        ),
        (
            tr!("hud_maker.shortcuts.keys.ctrl_drag"),
            tr!("hud_maker.shortcuts.does.free"),
        ),
        (
            tr!("hud_maker.shortcuts.keys.hide"),
            tr!("hud_maker.shortcuts.does.hide"),
        ),
        ("Escape".into(), tr!("hud_maker.shortcuts.does.clear")),
        (
            tr!("hud_maker.shortcuts.keys.alt_click"),
            tr!("hud_maker.shortcuts.does.pick_inside"),
        ),
        (
            tr!("hud_maker.shortcuts.keys.shift_click"),
            tr!("hud_maker.shortcuts.does.add_or_remove"),
        ),
        (
            tr!("hud_maker.shortcuts.keys.box"),
            tr!("hud_maker.shortcuts.does.box"),
        ),
        ("Ctrl+Z".into(), tr!("hud_maker.shortcuts.does.undo")),
        ("Ctrl+Y".into(), tr!("hud_maker.shortcuts.does.redo")),
        (
            tr!("hud_maker.shortcuts.keys.space_drag"),
            tr!("hud_maker.shortcuts.does.pan"),
        ),
        (
            tr!("hud_maker.shortcuts.keys.middle_drag"),
            tr!("hud_maker.shortcuts.does.pan"),
        ),
        (
            tr!("hud_maker.shortcuts.keys.wheel"),
            tr!("hud_maker.shortcuts.does.scroll"),
        ),
        (
            tr!("hud_maker.shortcuts.keys.ctrl_wheel"),
            tr!("hud_maker.shortcuts.does.zoom_at_pointer"),
        ),
        (
            tr!("hud_maker.shortcuts.keys.ctrl_plus_minus"),
            tr!("hud_maker.shortcuts.does.zoom"),
        ),
        ("Ctrl+0".into(), tr!("hud_maker.shortcuts.does.fit")),
        ("?".into(), tr!("hud_maker.shortcuts.does.show")),
    ]
}

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
    let rows = shortcuts()
        .into_iter()
        .enumerate()
        .map(|(i, (keys, does))| {
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
                            .child(keys),
                    ),
                )
                .child(div().flex_1().min_w_0().child(does))
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
                        .child(tr!("hud_maker.shortcuts.title")),
                )
                .child(
                    div()
                        .text_sm()
                        .text_color(muted)
                        .child(tr!("hud_maker.shortcuts.hint")),
                ),
        )
        // Rows span the sheet so a long description wraps in its column instead of overflowing.
        .child(v_flex().gap_2().children(rows))
        .child(
            h_flex().justify_end().child(
                Button::new(ids::HUD_MAKER_SHORTCUTS_CLOSE)
                    .outline()
                    .label(tr!("hud_maker.shortcuts.close"))
                    .on_click(|_, window, cx| window.close_dialog(cx)),
            ),
        )
}
