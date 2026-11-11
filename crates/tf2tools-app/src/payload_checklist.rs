//! "Choose what to install": the checklist shown when a GameBanana add-on holds several parts.
//! It answers over a channel, like the confirm dialog, and keeps its ticks in an entity because
//! the kit's dialog builder runs every frame.

use std::rc::Rc;

use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::checkbox::Checkbox;
use gpui_kit::component::{ActiveTheme as _, Disableable as _, WindowExt as _, h_flex, v_flex};
use gpui_kit::{
    App, AppContext as _, Context, Entity, FontWeight, InteractiveElement as _, IntoElement,
    ParentElement as _, Render, ScrollHandle, SharedString, StatefulInteractiveElement as _,
    Styled as _, TestSupportExt as _, Window, div, px,
};

use crate::ids;
use crate::widgets::dialog_frame::{DIALOG_WIDTH, DialogFrame};
use crate::widgets::scroll::scroll_beside;

/// One row: the payload's name (what is answered) and the label shown.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChecklistItem {
    pub name: String,
    pub label: String,
}

type Answer = Option<Vec<String>>;

/// The checklist's state: which rows are checked. The first starts checked.
pub struct PayloadChecklist {
    description: SharedString,
    items: Rc<[ChecklistItem]>,
    checked: Vec<bool>,
    answer: async_channel::Sender<Answer>,
    scroll: ScrollHandle,
}

impl PayloadChecklist {
    /// Which rows are checked, in row order.
    pub fn checked(&self) -> &[bool] {
        &self.checked
    }

    /// Flips row `index`; an index past the last row changes nothing.
    pub fn toggle(&mut self, index: usize, cx: &mut Context<Self>) {
        if let Some(tick) = self.checked.get_mut(index) {
            *tick = !*tick;
            cx.notify();
        }
    }

    /// The checked names in row order.
    pub fn chosen(&self) -> Vec<String> {
        self.items
            .iter()
            .zip(&self.checked)
            .filter(|(_, on)| **on)
            .map(|(item, _)| item.name.clone())
            .collect()
    }
}

impl Render for PayloadChecklist {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let muted = cx.theme().muted_foreground;
        let rows = self
            .items
            .iter()
            .zip(&self.checked)
            .enumerate()
            .map(|(ix, (item, on))| {
                Checkbox::new(ids::payload_check(ix))
                    .label(item.label.clone())
                    .checked(*on)
                    .on_click(cx.listener(move |list, _: &bool, _, cx| list.toggle(ix, cx)))
            })
            .collect::<Vec<_>>();
        let chosen = self.chosen();
        let nothing = chosen.is_empty();
        // A one-slot channel: the first answer wins, so Escape after Install changes nothing.
        let install = self.answer.clone();
        let cancel = self.answer.clone();
        v_flex()
            .id(ids::PAYLOAD_CHECKLIST)
            .test_support()
            .gap_4()
            // v5's dialog header: title and description 8 px apart.
            .child(
                v_flex()
                    .gap_2()
                    .child(
                        div()
                            .id(ids::PAYLOAD_CHECKLIST_TITLE)
                            .aria_label("Choose what to install")
                            .test_support()
                            .text_lg()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child("Choose what to install"),
                    )
                    .child(
                        div()
                            .id(ids::PAYLOAD_CHECKLIST_DESCRIPTION)
                            .aria_label(self.description.clone())
                            .test_support()
                            .text_sm()
                            .text_color(muted)
                            .child(self.description.clone()),
                    ),
            )
            // Capped like the Missing Items list, so a long list scrolls inside an 800x600 window.
            .child(scroll_beside(
                ids::PAYLOAD_CHECKLIST_SCROLLBAR,
                &self.scroll,
                v_flex()
                    .id(ids::PAYLOAD_CHECKLIST_ROWS)
                    .track_scroll(&self.scroll)
                    .gap_3()
                    .max_h(px(256.))
                    .overflow_y_scroll()
                    .children(rows),
            ))
            .child(
                h_flex()
                    .justify_end()
                    .gap_2()
                    .child(
                        Button::new(ids::PAYLOAD_CANCEL)
                            .outline()
                            .label("Cancel")
                            .on_click(move |_, window, cx| {
                                let _ = cancel.try_send(None);
                                window.close_dialog(cx);
                            }),
                    )
                    .child(
                        Button::new(ids::PAYLOAD_INSTALL)
                            .primary()
                            .label("Install")
                            .disabled(nothing)
                            .on_click(move |_, window, cx| {
                                let _ = install.try_send(Some(chosen.clone()));
                                window.close_dialog(cx);
                            }),
                    ),
            )
    }
}

/// Opens "Choose what to install" for add-on `addon_name`. The receiver yields the chosen
/// names on Install, or None on Cancel, Escape or the dialog going away.
pub fn open_payload_checklist(
    addon_name: &str,
    items: Vec<ChecklistItem>,
    window: &mut Window,
    cx: &mut App,
) -> (
    Entity<PayloadChecklist>,
    async_channel::Receiver<Option<Vec<String>>>,
) {
    let (tx, rx) = async_channel::bounded(1);
    let checked = (0..items.len()).map(|ix| ix == 0).collect();
    let list = cx.new(|_| PayloadChecklist {
        description: format!("{addon_name} has several parts. Pick the ones to install.").into(),
        items: items.into(),
        checked,
        answer: tx.clone(),
        scroll: ScrollHandle::new(),
    });
    let body = list.clone();
    // Deferred so a menu restoring focus after its click handler cannot pull focus out.
    window.defer(cx, move |window, cx| {
        let frame = DialogFrame::new(DIALOG_WIDTH);
        window.open_dialog(cx, move |dialog, window, cx| {
            // The builder holds only handles; every tick lives in the entity.
            let back_out = || {
                let tx = tx.clone();
                move |_: &_, _: &mut Window, _: &mut App| {
                    let _ = tx.try_send(None);
                    true
                }
            };
            frame
                .apply(dialog, &*window, &*cx)
                .close_button(false)
                // Enter and Escape both back out; only a click on Install installs.
                .on_ok(back_out())
                .on_cancel(back_out())
                .child(frame.measure(body.clone()))
        });
    });
    (list, rx)
}
