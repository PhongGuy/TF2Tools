//! The What's new dialog: the signed release notes between this version and the offer,
//! newest first, with Later and Update now (or the release page for a copy that cannot
//! update itself).

use chrono::NaiveDate;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::{ActiveTheme as _, Sizable as _, WindowExt as _, h_flex, v_flex};
use gpui_kit::{
    AnyElement, App, AppContext as _, Context, FontWeight, InteractiveElement as _, IntoElement,
    ParentElement as _, Render, ScrollHandle, SharedString, StatefulInteractiveElement as _,
    Styled as _, TestSupportExt as _, Window, div, px,
};

use super::{GroupKind, NotesGroup, NotesRelease, NotesState, Offer, updater};
use crate::i18n::{format, tr};
use crate::ids;
use crate::widgets::detail_header::detail_header;
use crate::widgets::dialog_frame::{DETAIL_WIDTH, DialogFrame, content_max_height};
use crate::widgets::labelled;
use crate::widgets::scroll::scroll_beside;

/// Opens the dialog (`ids::UPDATE_NOTES`) on `offer`, which it keeps even if the state moves on.
pub fn open(offer: Offer, window: &mut Window, cx: &mut App) {
    let view = cx.new(|_| WhatsNew {
        offer,
        scroll: ScrollHandle::new(),
    });
    let frame = DialogFrame::new(DETAIL_WIDTH);
    // Deferred, as the store detail does, so the opening click cannot pull focus back out.
    window.defer(cx, move |window, cx| {
        window.open_dialog(cx, move |dialog, window, cx| {
            frame
                .apply(dialog, &*window, &*cx)
                .child(frame.measure(view.clone()))
        });
    });
}

struct WhatsNew {
    offer: Offer,
    scroll: ScrollHandle,
}

impl Render for WhatsNew {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let muted = cx.theme().muted_foreground;
        let since = updater(cx).map(|u| {
            let current = u.read(cx).current().to_owned();
            labelled(
                div().text_sm().text_color(muted),
                ids::UPDATE_NOTES_SINCE,
                tr!("updater.notes.since", current = current),
            )
        });
        let english = (crate::i18n::current() != "en").then(|| {
            labelled(
                div().text_sm().text_color(muted),
                ids::UPDATE_NOTES_ENGLISH,
                tr!("updater.notes.english"),
            )
        });
        let subtitle = v_flex().children(since).children(english);
        let header = detail_header(
            ids::UPDATE_NOTES_TITLE,
            tr!("updater.notes.title"),
            None,
            Some(subtitle.into_any_element()),
            [],
            window,
            cx,
        );

        let notes = &self.offer.notes;
        let status = match notes.status {
            NotesState::Partial => Some(partial_line(&self.offer.history_page)),
            NotesState::Unavailable => Some(
                labelled(
                    div().text_sm(),
                    ids::UPDATE_NOTES_UNAVAILABLE,
                    tr!("updater.notes.unavailable"),
                )
                .into_any_element(),
            ),
            NotesState::Complete | NotesState::NotPublished => None,
        };
        // Unavailable notes carry no releases, so the list stays empty there by construction.
        let releases = notes
            .releases
            .iter()
            .enumerate()
            .map(|(r, release)| release_block(r, release, muted));
        let body = v_flex()
            .id(ids::UPDATE_NOTES_BODY)
            .test_support()
            .min_h_0()
            .track_scroll(&self.scroll)
            .overflow_y_scroll()
            .gap_4()
            .children(status)
            .children(releases);

        v_flex()
            .id(ids::UPDATE_NOTES)
            .test_support()
            .w_full()
            .max_h(content_max_height(window, cx))
            .gap_4()
            .child(header)
            .child(scroll_beside(ids::UPDATE_NOTES_SCROLLBAR, &self.scroll, body).min_h_0())
            .child(buttons(&self.offer))
    }
}

// The partial line with a small button to the list of every release, for the missing ones.
fn partial_line(history_page: &str) -> AnyElement {
    let page = history_page.to_owned();
    v_flex()
        .gap_2()
        .items_start()
        .child(labelled(
            div().text_sm(),
            ids::UPDATE_NOTES_PARTIAL,
            tr!("updater.notes.partial"),
        ))
        .child(
            Button::new(ids::UPDATE_NOTES_HISTORY)
                .outline()
                .small()
                .label(tr!("updater.button.release_page"))
                .on_click(move |_, _, cx| cx.open_url(&page)),
        )
        .into_any_element()
}

// One release: its version with the date muted beside it, the intro, then its groups.
fn release_block(r: usize, release: &NotesRelease, muted: gpui_kit::Hsla) -> AnyElement {
    let date = NaiveDate::parse_from_str(&release.date, "%Y-%m-%d")
        .ok()
        .map(|d| {
            labelled(
                div().text_sm().text_color(muted),
                ids::update_notes_date(r),
                format::numeric_date(d),
            )
        });
    let heading = h_flex()
        .items_baseline()
        .gap_2()
        .child(labelled(
            div().font_weight(FontWeight::SEMIBOLD),
            ids::update_notes_release(r),
            SharedString::from(format!("v{}", release.version)),
        ))
        .children(date);
    let intro = release
        .intro
        .as_ref()
        .map(|text| labelled(div().text_sm(), ids::update_notes_intro(r), text.clone()));
    v_flex()
        .gap_2()
        .child(heading)
        .children(intro)
        .children(
            release
                .groups
                .iter()
                .enumerate()
                .map(|(g, group)| group_block(r, g, group, muted)),
        )
        .into_any_element()
}

// A group's title, translated for the known kinds, over its items as plain bullets.
fn group_block(r: usize, g: usize, group: &NotesGroup, muted: gpui_kit::Hsla) -> AnyElement {
    let title: SharedString = match group.kind {
        Some(GroupKind::Features) => tr!("updater.notes.kind.features"),
        Some(GroupKind::Fixes) => tr!("updater.notes.kind.fixes"),
        Some(GroupKind::Usability) => tr!("updater.notes.kind.usability"),
        Some(GroupKind::Performance) => tr!("updater.notes.kind.performance"),
        Some(GroupKind::Other) | None => group.title.clone().into(),
    };
    let items = group.items.iter().enumerate().map(|(i, item)| {
        h_flex()
            .items_start()
            .gap_2()
            .text_sm()
            // The dot sits on the first line's middle, so a wrapped item hangs under its text.
            .child(
                div()
                    .flex_none()
                    .mt(px(8.))
                    .size(px(4.))
                    .rounded_full()
                    .bg(muted),
            )
            .child(labelled(
                div().flex_1().min_w_0(),
                ids::update_notes_item(r, g, i),
                item.text.clone(),
            ))
    });
    v_flex()
        .gap_1()
        .child(labelled(
            div().text_sm().font_weight(FontWeight::MEDIUM),
            ids::update_notes_group(r, g),
            title,
        ))
        .children(items)
        .into_any_element()
}

// Later only closes; the primary button downloads, or opens the release page when this copy
// cannot update itself, then closes too.
fn buttons(offer: &Offer) -> impl IntoElement {
    let later = Button::new(ids::UPDATE_NOTES_LATER)
        .outline()
        .label(tr!("updater.notes.later"))
        .on_click(|_, window, cx| window.close_dialog(cx));
    let update = if offer.installable {
        Button::new(ids::UPDATE_NOTES_UPDATE)
            .primary()
            .label(tr!("updater.notes.update"))
            .on_click(|_, window, cx| {
                if let Some(u) = updater(cx) {
                    u.update(cx, |u, cx| u.download(cx));
                }
                window.close_dialog(cx);
            })
    } else {
        let page = offer.release_page.clone();
        Button::new(ids::UPDATE_NOTES_UPDATE)
            .primary()
            .label(tr!("updater.button.release_page"))
            .on_click(move |_, window, cx| {
                cx.open_url(&page);
                window.close_dialog(cx);
            })
    };
    h_flex()
        .flex_wrap()
        .justify_end()
        .gap_2()
        .child(later)
        .child(update)
}
