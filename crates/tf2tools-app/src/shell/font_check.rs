//! The HUD fonts dialog: lists a HUD's missing fonts, protected names and Linux issues, and
//! asks whether to install, skip, or stop asking for this HUD.

use std::rc::Rc;

use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::{
    ActiveTheme as _, Icon, Sizable as _, Size, WindowExt as _, h_flex, v_flex,
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, AnyWindowHandle, App, ElementId, FontWeight, Hsla, InteractiveElement as _,
    IntoElement, ParentElement as _, ScrollHandle, SharedString, StatefulInteractiveElement as _,
    Styled as _, TestSupportExt as _, Window, div, px, rgb,
};

use tf2tools_core::Tf2Install;

use crate::font_check::{
    FontCheckSession, FontPrompt, dismiss_job, install_job, issue_text, prompt_job,
};
use crate::ids;
use crate::pages::common::ORANGE_500;
use crate::services::run_blocking;
use crate::shell::toast::{toast, toast_error};
use crate::widgets::dialog_frame::{DIALOG_WIDTH, DialogFrame};
use crate::widgets::scroll::{BAR_WIDTH, scroll_area};

/// The user's answer to the HUD fonts dialog.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FontAnswer {
    Install,
    Skip,
    DontAsk,
}

/// Claims `folder` up front, so a page load landing mid-check skips it, then runs `prompt_job`
/// in `run_blocking` (a check that never returns releases the folder); on a prompt, unless a
/// font dialog already shows, opens `font_check_dialog` on `win`. A check dropped behind
/// another HUD's dialog releases its folder. Install runs `install_job` in
/// `run_blocking` and toasts its `FontToast` (or `toast_error`); DontAsk runs `dismiss_job`
/// (an error toasts); Skip, Escape or a dropped dialog do nothing.
pub fn ask_about_fonts(win: AnyWindowHandle, tf2: Tf2Install, folder: String, cx: &mut App) {
    FontCheckSession::begin(cx);
    FontCheckSession::claim(cx, &folder);
    let named = folder.clone();
    let checking = run_blocking(cx, move |s| Ok(prompt_job(s, &tf2, Some(&named))));
    cx.spawn(async move |cx| {
        let checked = checking.await;
        // Count and open in one update, so a test that sees `finished` also sees the dialog,
        // and a dropped window never leaves `showing` set.
        let opened = cx.update(|cx| {
            FontCheckSession::end(cx);
            let Ok(prompt) = checked else {
                FontCheckSession::release(cx, &folder);
                return None;
            };
            let prompt = prompt?;
            if !FontCheckSession::try_show(cx, &prompt.folder) {
                return None;
            }
            match win.update(cx, |_, window, cx| font_check_dialog(&prompt, window, cx)) {
                Ok(rx) => Some((prompt, rx)),
                Err(_) => {
                    FontCheckSession::hide(cx);
                    None
                }
            }
        });
        let Some((prompt, rx)) = opened else { return };
        let answer = rx.recv().await.unwrap_or(FontAnswer::Skip);
        cx.update(FontCheckSession::hide);
        match answer {
            FontAnswer::Skip => {}
            FontAnswer::Install => {
                let installing = cx.update(|cx| run_blocking(cx, move |s| install_job(s, &prompt)));
                let done = installing.await;
                let _ = cx.update(|cx| {
                    win.update(cx, |_, window, cx| match done {
                        Ok(t) => toast(window, cx, t.kind, t.title, t.description),
                        Err(err) => toast_error(window, cx, &err),
                    })
                });
            }
            FontAnswer::DontAsk => {
                let saving = cx.update(|cx| run_blocking(cx, move |s| dismiss_job(s, &prompt)));
                if let Err(err) = saving.await {
                    let _ = cx
                        .update(|cx| win.update(cx, |_, window, cx| toast_error(window, cx, &err)));
                }
            }
        }
    })
    .detach();
}

/// Opens the dialog over `window`; the receiver yields one answer, and errs if the dialog
/// goes away unanswered. Escape and Enter answer Skip.
pub fn font_check_dialog(
    prompt: &FontPrompt,
    window: &mut Window,
    cx: &mut App,
) -> async_channel::Receiver<FontAnswer> {
    // A one-slot channel and try_send: the first answer wins, any later one drops.
    let (tx, rx) = async_channel::bounded(1);
    let description: SharedString = format!(
        "{} uses fonts that are not installed, so TF2 would show its default fonts instead. \
         Restart TF2 after installing them.",
        prompt.folder
    )
    .into();
    // Font rows read missing fonts first, then protected names the install never touches.
    let fonts: Rc<[SharedString]> = prompt
        .check
        .missing()
        .iter()
        .map(|font| {
            let files: Vec<String> = font
                .files()
                .iter()
                .map(|file| file.display().to_string())
                .collect();
            format!("{}: {}", font.name(), files.join(", ")).into()
        })
        .chain(
            prompt
                .check
                .protected()
                .iter()
                .map(|name| format!("{name}: not installed, system font name").into()),
        )
        .collect();
    let issues: Rc<[SharedString]> = prompt
        .check
        .issues()
        .iter()
        .map(|issue| issue_text(issue).into())
        .collect();
    let can_install = !prompt.check.missing().is_empty();
    // The list's scroll position outlives the builder, which runs every frame.
    let list_scroll = ScrollHandle::new();
    // Deferred so a menu restoring focus after its click handler cannot pull focus out.
    window.defer(cx, move |window, cx| {
        let frame = DialogFrame::new(DIALOG_WIDTH);
        window.open_dialog(cx, move |dialog, window, cx| {
            // The builder runs every frame, so each handler clones what it captures.
            let answer = |reply: FontAnswer| {
                let tx = tx.clone();
                move |_: &_, _: &mut Window, _: &mut App| {
                    let _ = tx.try_send(reply);
                    true
                }
            };
            let button = |id: &'static str, reply: FontAnswer| {
                let tx = tx.clone();
                Button::new(id).on_click(move |_, window, cx| {
                    let _ = tx.try_send(reply);
                    window.close_dialog(cx);
                })
            };
            let muted = cx.theme().muted_foreground;
            let font_rows = fonts
                .iter()
                .enumerate()
                .map(|(ix, label)| font_row(ids::hud_fonts_font(ix), label.clone(), cx));
            let issue_rows = issues
                .iter()
                .enumerate()
                .map(|(ix, label)| issue_row(ix, label.clone(), cx));
            frame
                .apply(dialog, &*window, &*cx)
                .close_button(false)
                // Enter and Escape both take the safe choice; only a click installs.
                .on_ok(answer(FontAnswer::Skip))
                .on_cancel(answer(FontAnswer::Skip))
                .child(
                    frame.measure(
                        v_flex()
                            .id(ids::HUD_FONTS_DIALOG)
                            .test_support()
                            .gap_4()
                            // v5's dialog header: title and description 8 px apart.
                            .child(
                                v_flex()
                                    .gap_2()
                                    .child(
                                        div()
                                            .id(ids::HUD_FONTS_TITLE)
                                            .aria_label("HUD fonts")
                                            .test_support()
                                            .text_lg()
                                            .font_weight(FontWeight::SEMIBOLD)
                                            .child("HUD fonts"),
                                    )
                                    .child(
                                        div()
                                            .id(ids::HUD_FONTS_DESCRIPTION)
                                            .aria_label(description.clone())
                                            .test_support()
                                            .text_sm()
                                            .text_color(muted)
                                            .child(description.clone()),
                                    ),
                            )
                            .child(scroll_area(
                                ids::HUD_FONTS_LIST_SCROLLBAR,
                                &list_scroll,
                                v_flex()
                                    .id(ids::HUD_FONTS_LIST)
                                    .track_scroll(&list_scroll)
                                    .test_support()
                                    .gap_2()
                                    .max_h(px(256.))
                                    .overflow_y_scroll()
                                    // The rows fill the width, so they leave room for the bar.
                                    .pr(BAR_WIDTH)
                                    .children(font_rows)
                                    .children(issue_rows),
                            ))
                            .child(
                                h_flex()
                                    .justify_end()
                                    .gap_2()
                                    .child(
                                        button(ids::HUD_FONTS_DONT_ASK, FontAnswer::DontAsk)
                                            .ghost()
                                            .label("Don't ask again for this HUD"),
                                    )
                                    .child(
                                        button(ids::HUD_FONTS_SKIP, FontAnswer::Skip)
                                            .outline()
                                            .label("Skip"),
                                    )
                                    .when(can_install, |row| {
                                        row.child(
                                            button(ids::HUD_FONTS_INSTALL, FontAnswer::Install)
                                                .primary()
                                                .label("Install fonts"),
                                        )
                                    }),
                            ),
                    ),
                )
        });
    });
    rx
}

/// A bordered row naming one font and where the HUD ships it.
fn font_row(id: ElementId, label: SharedString, cx: &App) -> AnyElement {
    div()
        .id(id)
        .aria_label(label.clone())
        .test_support()
        .px_3()
        .py_2()
        .rounded(cx.theme().radius_lg)
        .border_1()
        .border_color(cx.theme().border)
        .text_sm()
        .child(label)
        .into_any_element()
}

/// A Linux issue, flagged with v5's orange warning triangle.
fn issue_row(ix: usize, label: SharedString, cx: &App) -> AnyElement {
    let orange: Hsla = rgb(ORANGE_500).into();
    h_flex()
        .id(ids::hud_fonts_issue(ix))
        .aria_label(label.clone())
        .test_support()
        .items_start()
        .gap_2()
        .px_3()
        .py_2()
        .rounded(cx.theme().radius_lg)
        .border_1()
        .border_color(cx.theme().border)
        .child(
            Icon::new(IconName::TriangleAlert)
                .with_size(Size::Size(px(16.)))
                .text_color(orange),
        )
        .child(
            div()
                .id(ids::hud_fonts_issue_text(ix))
                .test_support()
                // The text takes the room beside the icon and may shrink below its one-line
                // width, so a long warning wraps inside the row.
                .flex_1()
                .min_w_0()
                .text_sm()
                .text_color(cx.theme().muted_foreground)
                .child(label),
        )
        .into_any_element()
}
