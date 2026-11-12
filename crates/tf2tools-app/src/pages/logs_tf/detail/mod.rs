//! An open log's panel, laid out like its page on logs.tf: the score banner, the title, the
//! players table, the team table, the rounds, one block per Medic, the kill matrix and the
//! buttons that open the chat and the killstreaks and download the log.

mod banner;
pub(super) mod footer;
mod matrix;
mod medics;
mod players;
mod popover;
mod rounds;

use std::collections::{HashMap, HashSet};

use gpui_kit::assets::IconName;
use gpui_kit::component::{ActiveTheme as _, Icon, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, Div, ElementId, FontWeight, Hsla, InteractiveElement as _, IntoElement,
    ParentElement as _, Pixels, Stateful, StatefulInteractiveElement, Styled, TestSupportExt as _,
    WeakEntity, Window, div, px,
};
use tf2tools_core::{
    LogDetail, LogRow, LogState, MatrixKind, SteamId, Team, TfClass, TrackedPlayer,
};

use super::sort::PlayerSort;
use super::text::{LOG_LOADING, nickname_note, team_name};
use super::{LogsTfPage, OpenLog, OpenState};
use crate::ids;
use crate::services::Services;
use crate::widgets::class_icon::class_icon;
use crate::widgets::labelled;
use crate::widgets::scroll::sideways;
use crate::widgets::store_error::error_line;
use crate::widgets::tip::tip;

// A number column's width and a class icon's size, shared with the players table.
// Both are sized so every players column of a full log fits a 1280 px window, as on logs.tf.
const NUMBER: Pixels = px(40.);
const ICON: Pixels = px(20.);

/// The tracked players an open log marks, and the nicknames it names them by; there are no
/// nicknames while Settings turns them off.
#[derive(Clone, Default)]
pub(in crate::pages::logs_tf) struct Tracked {
    ids: HashSet<SteamId>,
    nicknames: HashMap<SteamId, String>,
}

impl Tracked {
    pub(in crate::pages::logs_tf) fn new(players: &[TrackedPlayer], nicknames: bool) -> Self {
        Tracked {
            ids: players.iter().map(TrackedPlayer::steam_id).collect(),
            nicknames: players
                .iter()
                .filter(|_| nicknames)
                .filter_map(|p| Some((p.steam_id(), p.nickname()?.to_owned())))
                .collect(),
        }
    }

    pub(super) fn is(&self, id: SteamId) -> bool {
        self.ids.contains(&id)
    }

    pub(super) fn nickname(&self, id: SteamId) -> Option<&str> {
        self.nicknames.get(&id).map(String::as_str)
    }

    /// The name the tables show for `id`: their nickname, else `log_name`.
    pub(super) fn shown(&self, id: SteamId, log_name: &str) -> String {
        self.nickname(id).unwrap_or(log_name).to_owned()
    }
}

// A table's name in `cell`, which the caller ids and sizes: cut to fit with the tooltip giving it
// whole. A nicknamed player shows the nickname, then a marker whose tooltip names them as
// the log does; the cell is labelled with the nickname.
pub(super) fn name_cell(
    cell: Stateful<Div>,
    player: SteamId,
    log_name: &str,
    tracked: &Tracked,
    place: &str,
) -> AnyElement {
    let Some(nickname) = tracked.nickname(player) else {
        return cell
            .test_support()
            .tooltip(tip(ids::LOGSTF_NAME_TIP, log_name.to_owned()))
            .truncate()
            .child(log_name.to_owned())
            .into_any_element();
    };
    cell.test_support()
        .flex()
        .items_center()
        .gap_1()
        .overflow_hidden()
        .aria_label(nickname.to_owned())
        .child(
            div()
                .id("logstf-nick-name")
                .tooltip(tip(ids::LOGSTF_NAME_TIP, nickname.to_owned()))
                .min_w_0()
                .truncate()
                .child(nickname.to_owned()),
        )
        .child(nick_marker(ids::logstf_nick(place, player), log_name))
        .into_any_element()
}

// The small mark after a nickname; hovering it says whose name the log really shows.
pub(super) fn nick_marker(id: ElementId, log_name: &str) -> impl IntoElement {
    h_flex()
        .id(id)
        .aria_label("Nickname")
        .test_support()
        .flex_none()
        .opacity(0.6)
        .tooltip(tip(ids::LOGSTF_NICK_TIP, nickname_note(log_name)))
        .child(Icon::new(IconName::Info).xsmall())
}

/// The panel under open log `row`. The score banner heads it once `open` is ready; the
/// title comes from the row at once; under it the parsed log, or a line saying it is loading
/// or why it failed. A failed fetch of a pending log offers Retry, which asks `page` to fetch
/// it again.
pub(super) fn panel(
    row: &LogRow,
    open: &OpenLog,
    tracked: &Tracked,
    sort: PlayerSort,
    matrix: MatrixKind,
    page: WeakEntity<LogsTfPage>,
    cx: &App,
) -> AnyElement {
    let id = row.id();
    let status = |text: &'static str| {
        let line = div().text_sm().text_color(cx.theme().muted_foreground);
        labelled(line, ids::logstf_log_status(id), text).into_any_element()
    };
    let opened = &open.state;
    let ready = match opened {
        OpenState::Ready(log) => Some(log.as_ref()),
        OpenState::Loading | OpenState::Failed(_) => None,
    };
    let body = match opened {
        OpenState::Ready(log) => {
            log_body(open, log, tracked, sort, matrix, page, cx).into_any_element()
        }
        OpenState::Loading => status(LOG_LOADING),
        OpenState::Failed(why) if row.state() == LogState::Pending => {
            let on_retry = move |_: &mut Window, cx: &mut App| {
                let _ = page.update(cx, |page, cx| page.retry(id, cx));
            };
            let retry = ids::logstf_log_retry(id);
            error_line(ids::logstf_log_status(id), retry, *why, on_retry, cx).into_any_element()
        }
        OpenState::Failed(why) => status(why),
    };
    v_flex()
        .id(ids::logstf_log_panel(row.id()))
        .test_support()
        .w_full()
        .gap_4()
        .px_3()
        .py_3()
        .bg(cx.theme().muted)
        .when_some(ready, |el, log| el.child(banner::banner(id, log)))
        .child(banner::title(row, cx))
        .child(body)
        .into_any_element()
}

// Everything the parsed body adds, in logs.tf's order.
fn log_body(
    open: &OpenLog,
    log: &LogDetail,
    tracked: &Tracked,
    sort: PlayerSort,
    matrix: MatrixKind,
    page: WeakEntity<LogsTfPage>,
    cx: &App,
) -> impl IntoElement {
    v_flex()
        .w_full()
        .gap_6()
        .text_xs()
        .child(sideways(
            ids::LOGSTF_PLAYERS_SCROLL,
            players::players_table(log, sort, tracked, page.clone(), cx),
        ))
        .child(sideways(
            ids::LOGSTF_TEAMS_SCROLL,
            banner::team_table(log, cx),
        ))
        .when(!log.rounds().is_empty(), |el| {
            let table = rounds::rounds_table(open.id, log, &open.rounds, tracked, page.clone(), cx);
            el.child(
                section("Rounds", table, cx)
                    .id(ids::LOGSTF_ROUNDS)
                    .test_support(),
            )
        })
        .when(!log.medics().is_empty(), |el| {
            el.child(section("Medics", medics::medics(log, tracked, cx), cx))
        })
        .child(
            section(
                "Kill matrix",
                matrix::matrix(log, matrix, tracked, page.clone(), cx),
                cx,
            )
            .id(ids::LOGSTF_MATRIX)
            .test_support(),
        )
        .child(footer::footer(open.id, log, page))
}

// A row of any player table: a tracked player's is tinted and bold, and says so in words for
// readers that cannot see the tint.
pub(super) fn player_row<E>(row: E, tracked: bool, cx: &App) -> E
where
    E: StatefulInteractiveElement + Styled,
{
    if !tracked {
        return row;
    }
    row.bg(cx.theme().primary.opacity(0.12))
        .font_weight(FontWeight::SEMIBOLD)
        .aria_description("Tracked player")
}

// A titled part of the body.
pub(super) fn section(title: &'static str, content: impl IntoElement, cx: &App) -> Div {
    v_flex()
        .gap_2()
        .child(
            div()
                .text_sm()
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(cx.theme().foreground)
                .child(title),
        )
        .child(content)
}

// A team's name in its colour.
pub(super) fn team_cell(team: Team, cx: &App) -> Div {
    div()
        .flex_none()
        .font_weight(FontWeight::BOLD)
        .text_color(team_colour(team, cx))
        .child(team_name(team))
}

// A team's colour from the theme; a team core adds later reads as BLU's rather than none.
pub(super) fn team_colour(team: Team, cx: &App) -> Hsla {
    match team {
        Team::Red => cx.theme().red,
        Team::Blue | _ => cx.theme().blue,
    }
}

fn class_cell(class: TfClass, cx: &App) -> Div {
    let assets = &cx.global::<Services>().assets;
    div()
        .w(NUMBER)
        .flex_none()
        .flex()
        .justify_end()
        .child(class_icon(class, assets.class_icon(class).as_deref(), ICON))
}

pub(super) fn number(text: String) -> Div {
    div().w(NUMBER).flex_none().text_right().child(text)
}
