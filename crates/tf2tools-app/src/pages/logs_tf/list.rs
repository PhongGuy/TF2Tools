//! The logs.tf page's one scrolling list: the chips and grid as its first item, then one row
//! per log in logs.tf's column order, the open row followed by its panel.

use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::{ActiveTheme as _, Icon, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, Context, Div, FontWeight, InteractiveElement as _, IntoElement,
    ParentElement as _, Pixels, StatefulInteractiveElement as _, Styled as _, TestSupportExt as _,
    WeakEntity, div, list, px,
};
use tf2tools_core::{LogRow, LogState, SteamId};

use super::LogsTfPage;
use super::text::{
    LOG_GONE, LOG_PENDING, LOG_UNREADABLE, NO_LOGS_TEXT, date_text, duration_text, log_url,
    profile_url, whole,
};
use crate::ids;
use crate::services::Services;
use crate::widgets::class_icon::class_icon;
use crate::widgets::labelled;
use crate::widgets::scroll::scroll_beside;

// Column widths shared by the header and every row, so the columns line up like a table.
const CHEVRON: Pixels = px(16.);
const DATE: Pixels = px(88.);
const MAP: Pixels = px(160.);
const LENGTH: Pixels = px(56.);
const CLASSES: Pixels = px(72.);
const KDA: Pixels = px(80.);
const DPM: Pixels = px(48.);
const GAP: Pixels = px(16.);
const ICON: Pixels = px(18.);

/// The whole page as a virtualized list over `page`'s list state: item 0 is everything above
/// the logs, item `n` is the log at `rows[n - 1]`. The caller sizes it to fill the page.
pub(super) fn page_list(page: &LogsTfPage, cx: &mut Context<LogsTfPage>) -> Div {
    let this = cx.entity().downgrade();
    let rows = div().id(ids::LOGSTF_LIST).test_support().size_full().child(
        list(page.list.clone(), move |ix, window, cx| {
            let Some(page) = this.upgrade() else {
                return div().into_any_element();
            };
            if ix == 0 {
                // The chips and buttons take listeners, so the top builds inside the page.
                return page.update(cx, |page, cx| page.top(window, cx).into_any_element());
            }
            let page = page.read(cx);
            match page.rows.get(ix - 1) {
                Some(row) => log_item(row, page.expanded == Some(row.id()), this.clone(), cx),
                None => div().into_any_element(),
            }
        })
        .size_full(),
    );
    scroll_beside(ids::LOGSTF_LIST_SCROLLBAR, &page.list, rows)
}

/// The column titles over the rows, in the rows' widths.
pub(super) fn column_header(cx: &App) -> impl IntoElement {
    let cell = |w: Pixels, text: &'static str| div().w(w).flex_none().child(text);
    h_flex()
        .gap(GAP)
        .px_3()
        .py_2()
        .border_b_1()
        .border_color(cx.theme().border)
        .text_xs()
        .font_weight(FontWeight::MEDIUM)
        .text_color(cx.theme().muted_foreground)
        .child(div().w(CHEVRON).flex_none())
        .child(cell(DATE, "Date"))
        .child(div().flex_1().min_w_0().child("Title"))
        .child(cell(MAP, "Map"))
        .child(cell(LENGTH, "Length").text_right())
        .child(cell(CLASSES, "Class"))
        .child(cell(KDA, "K/D/A").text_right())
        .child(cell(DPM, "DPM").text_right())
}

/// The line for a player with no logs in the window, with a way to their logs.tf profile.
pub(super) fn no_logs(id: SteamId, cx: &App) -> impl IntoElement {
    h_flex()
        .gap_2()
        .py_4()
        .child(labelled(
            div().text_sm().text_color(cx.theme().muted_foreground),
            ids::LOGSTF_NO_LOGS,
            NO_LOGS_TEXT,
        ))
        .child(
            Button::new(ids::LOGSTF_NO_LOGS_PROFILE)
                .ghost()
                .small()
                .icon(IconName::ExternalLink)
                .label("Open profile on logs.tf")
                .on_click(move |_, _, cx| cx.open_url(&profile_url(id))),
        )
}

// One log: its row, then its panel while open. Only a downloaded log opens; the rest carry
// a note in place of the stats, and an unreadable one links to logs.tf instead.
fn log_item(row: &LogRow, open: bool, page: WeakEntity<LogsTfPage>, cx: &App) -> AnyElement {
    let theme = cx.theme();
    let id = row.id();
    let ready = row.state() == LogState::Ready;
    let chevron = div().w(CHEVRON).flex_none().children(ready.then(|| {
        Icon::new(if open {
            IconName::ChevronDown
        } else {
            IconName::ChevronRight
        })
        .size_4()
        .text_color(theme.muted_foreground)
    }));
    let mut line = h_flex()
        .id(ids::logstf_log(id))
        .aria_label(row.title().to_owned())
        .test_support()
        .gap(GAP)
        .px_3()
        .py_2()
        .text_sm()
        .when(open, |el| el.bg(theme.muted))
        .child(chevron)
        .child(div().w(DATE).flex_none().child(date_text(row.date())))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .truncate()
                .child(row.title().to_owned()),
        )
        .child(
            div()
                .w(MAP)
                .flex_none()
                .truncate()
                .child(row.map().to_owned()),
        );
    line = match row.state() {
        LogState::Ready => line
            .children(stats(row, cx))
            .cursor_pointer()
            .hover(|s| s.bg(theme.muted))
            .on_click(move |_, _, cx| {
                let _ = page.update(cx, |page, cx| page.toggle(id, cx));
            }),
        state @ (LogState::Pending | LogState::Gone | LogState::Unreadable | _) => {
            line.child(note(id, state, cx))
        }
    };
    v_flex()
        .w_full()
        .border_b_1()
        .border_color(theme.border)
        .child(line)
        .children(open.then(|| panel(id, cx)))
        .into_any_element()
}

// A downloaded log's numbers for the player: length, classes, K/D/A and damage per minute.
fn stats(row: &LogRow, cx: &App) -> [AnyElement; 4] {
    let assets = &cx.global::<Services>().assets;
    let icons = h_flex().w(CLASSES).flex_none().gap_1().children(
        row.classes()
            .iter()
            .map(|&class| class_icon(class, assets.class_icon(class).as_deref(), ICON)),
    );
    let length = row.length().map(duration_text).unwrap_or_default();
    let kda = format!("{}/{}/{}", row.kills(), row.deaths(), row.assists());
    let dpm = row.dmg_per_min().map(whole).unwrap_or_else(|| "-".into());
    [
        div()
            .w(LENGTH)
            .flex_none()
            .text_right()
            .child(length)
            .into_any_element(),
        icons.into_any_element(),
        div()
            .w(KDA)
            .flex_none()
            .text_right()
            .child(kda)
            .into_any_element(),
        div()
            .w(DPM)
            .flex_none()
            .text_right()
            .child(dpm)
            .into_any_element(),
    ]
}

// Why a row has no stats, muted and spanning the stats columns.
fn note(id: u64, state: LogState, cx: &App) -> impl IntoElement {
    let text = match state {
        LogState::Gone => LOG_GONE,
        LogState::Unreadable => LOG_UNREADABLE,
        LogState::Pending | LogState::Ready | _ => LOG_PENDING,
    };
    h_flex()
        .w(LENGTH + CLASSES + KDA + DPM + GAP * 3.)
        .flex_none()
        .gap_2()
        .child(labelled(
            div().truncate().text_color(cx.theme().muted_foreground),
            ids::logstf_log_note(id),
            text,
        ))
        .children((state == LogState::Unreadable).then(|| open_link(id)))
}

// The open log's panel; Task 8 fills it with the log itself.
fn panel(id: u64, cx: &App) -> impl IntoElement {
    v_flex()
        .id(ids::logstf_log_panel(id))
        .test_support()
        .items_start()
        .px_3()
        .py_3()
        .bg(cx.theme().muted)
        .child(open_link(id))
}

fn open_link(id: u64) -> Button {
    Button::new(ids::logstf_log_open(id))
        .ghost()
        .xsmall()
        .icon(IconName::ExternalLink)
        .label("Open on logs.tf")
        .on_click(move |_, _, cx| cx.open_url(&log_url(id)))
}
