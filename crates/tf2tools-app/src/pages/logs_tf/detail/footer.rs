//! The open log's footer buttons, Show Chat and Show Killstreaks, and the dialogs they open.

use gpui_kit::component::button::Button;
use gpui_kit::component::{ActiveTheme as _, Disableable as _, Sizable as _, h_flex, v_flex};
use gpui_kit::{
    AnyElement, App, Div, FontWeight, InteractiveElement as _, IntoElement, ParentElement as _,
    Pixels, SharedString, StatefulInteractiveElement as _, Styled as _, TestSupportExt as _,
    WeakEntity, Window, div, px,
};
use tf2tools_core::{LogDetail, Team};

use super::{team_cell, team_colour};
use crate::ids;
use crate::pages::logs_tf::LogsTfPage;
use crate::pages::logs_tf::tables::ChatRow;
use crate::pages::logs_tf::text::{duration_text, streaks_label};
use crate::widgets::dialog_frame::content_max_height;

// The team badge's column, kept empty for a sender who is not a player so names line up.
const BADGE: Pixels = px(32.);
const TIME: Pixels = px(48.);
const NAME: Pixels = px(200.);

/// One killstreak as the dialog lists it, its time already worded.
#[derive(Clone)]
pub(in crate::pages::logs_tf) struct StreakRow {
    time: String,
    team: Option<Team>,
    name: String,
    streak: u32,
}

/// The log's killstreaks in core's order, each named from its player's line.
pub(in crate::pages::logs_tf) fn streak_rows(log: &LogDetail) -> Vec<StreakRow> {
    log.killstreaks()
        .iter()
        .map(|k| {
            let player = log.players().iter().find(|p| p.steam_id() == k.steam_id());
            StreakRow {
                time: duration_text(k.at()),
                team: player.map(|p| p.team()),
                name: player.map_or_else(|| k.steam_id().id64().to_string(), |p| p.name().into()),
                streak: k.streak(),
            }
        })
        .collect()
}

/// The two buttons under the kill matrix; each asks `page` to open its dialog, and Show
/// Killstreaks is disabled when the log has none.
pub(super) fn footer(log: &LogDetail, page: WeakEntity<LogsTfPage>) -> Div {
    let chat_page = page.clone();
    let streaks = log.killstreaks().len();
    h_flex()
        .gap_2()
        .child(
            Button::new(ids::LOGSTF_CHAT_TOGGLE)
                .outline()
                .small()
                .label("Show Chat")
                .on_click(move |_, window, cx| {
                    let _ = chat_page.update(cx, |page, cx| page.open_chat(window, cx));
                }),
        )
        .child(
            Button::new(ids::LOGSTF_STREAKS_TOGGLE)
                .outline()
                .small()
                .label(streaks_label(streaks))
                .disabled(streaks == 0)
                .on_click(move |_, window, cx| {
                    let _ = page.update(cx, |page, cx| page.open_streaks(window, cx));
                }),
        )
}

/// The chat dialog's content: each line's team badge, the name in its team's colour and the
/// message, scrolling inside the dialog when it outgrows the window.
pub(in crate::pages::logs_tf) fn chat_dialog(
    rows: &[ChatRow],
    window: &Window,
    cx: &App,
) -> AnyElement {
    let lines = rows.iter().enumerate().map(|(ix, row)| {
        let label = SharedString::from(format!("{}: {}", row.name, row.msg));
        h_flex()
            .id(ids::logstf_chat_line(ix))
            .aria_label(label)
            .test_support()
            .items_start()
            .gap_2()
            .child(badge(row.team, cx))
            .child(name(&row.name, row.team, cx))
            .child(div().flex_1().min_w_0().child(row.msg.clone()))
    });
    let body = if rows.is_empty() {
        vec![muted("Nobody chatted in this log.", cx).into_any_element()]
    } else {
        lines.map(IntoElement::into_any_element).collect()
    };
    dialog_body("In-game Chat", ids::LOGSTF_CHAT, body, window, cx)
}

/// The killstreaks dialog's content: when, who and how many, earliest first.
pub(in crate::pages::logs_tf) fn streaks_dialog(
    rows: &[StreakRow],
    window: &Window,
    cx: &App,
) -> AnyElement {
    let header = h_flex()
        .gap_2()
        .text_color(cx.theme().muted_foreground)
        .child(div().w(TIME).flex_none().child("Time"))
        .child(div().flex_1().child("Player"))
        .child(div().w(TIME).flex_none().text_right().child("Streak"));
    let lines = rows.iter().enumerate().map(|(ix, row)| {
        let label = SharedString::from(format!("{} {} {}", row.time, row.name, row.streak));
        h_flex()
            .id(ids::logstf_streak(ix))
            .aria_label(label)
            .test_support()
            .gap_2()
            .child(div().w(TIME).flex_none().child(row.time.clone()))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .child(name(&row.name, row.team, cx)),
            )
            .child(
                div()
                    .w(TIME)
                    .flex_none()
                    .text_right()
                    .child(row.streak.to_string()),
            )
            .into_any_element()
    });
    let body = std::iter::once(header.into_any_element())
        .chain(lines)
        .collect();
    dialog_body("Killstreaks", ids::LOGSTF_STREAKS, body, window, cx)
}

// The title over `rows`, which scroll on their own once the dialog reaches the window's cap.
// The kit's close X floats into the top right corner, so the title keeps clear of it.
fn dialog_body(
    title: &'static str,
    id: &'static str,
    rows: Vec<AnyElement>,
    window: &Window,
    cx: &App,
) -> AnyElement {
    v_flex()
        .w_full()
        .max_h(content_max_height(window, cx))
        .gap_4()
        .child(
            div()
                .pr(px(22.))
                .text_lg()
                .font_weight(FontWeight::SEMIBOLD)
                .child(title),
        )
        .child(
            v_flex()
                .id(id)
                .test_support()
                .min_h_0()
                .overflow_y_scroll()
                .gap_1()
                .text_sm()
                .children(rows),
        )
        .into_any_element()
}

fn badge(team: Option<Team>, cx: &App) -> Div {
    match team {
        Some(team) => team_cell(team, cx).w(BADGE),
        None => div().w(BADGE).flex_none(),
    }
}

// A sender's name in their team's colour, cut short so a long one leaves room for the text.
fn name(text: &str, team: Option<Team>, cx: &App) -> Div {
    let colour = team.map_or(cx.theme().foreground, |t| team_colour(t, cx));
    div()
        .flex_none()
        .max_w(NAME)
        .truncate()
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(colour)
        .child(text.to_owned())
}

fn muted(text: &'static str, cx: &App) -> Div {
    div().text_color(cx.theme().muted_foreground).child(text)
}
