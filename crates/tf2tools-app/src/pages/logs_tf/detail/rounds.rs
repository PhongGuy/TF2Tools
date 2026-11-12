//! The open log's rounds table, where a click opens a round into its players and events.

use std::collections::BTreeSet;

use gpui_kit::assets::IconName;
use gpui_kit::component::{ActiveTheme as _, Icon, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    App, Div, ElementId, FontWeight, HighlightStyle, InteractiveElement as _, IntoElement,
    ParentElement as _, Pixels, StatefulInteractiveElement as _, Styled as _, StyledText,
    TestSupportExt as _, WeakEntity, div, px,
};
use tf2tools_core::{LogDetail, RoundLine, SteamId, Team};

use super::{NUMBER, team_cell, team_colour};
use crate::ids;
use crate::pages::logs_tf::LogsTfPage;
use crate::pages::logs_tf::text::{EventPart, event_parts, round_length_text, team_name};
use crate::widgets::labelled;
use crate::widgets::scroll::sideways;

// The rounds table's least column widths, which share out the table's width as on logs.tf,
// and the open round's fixed columns.
const ROUND: Pixels = px(48.);
const LENGTH: Pixels = px(52.);
const SCORE: Pixels = px(56.);
const TEAM_NUMBER: Pixels = px(52.);
// Wide enough for the "Midfights" header, so the header lays out like the rows under it.
const MIDFIGHTS: Pixels = px(68.);
const TEAM: Pixels = px(40.);
const NAME: Pixels = px(200.);
const ROUND_NUMBER: Pixels = px(56.);
const TYPE: Pixels = px(96.);
// A description wraps within the panel down to this width.
const DESCRIPTION_MIN: Pixels = px(160.);
// The least space between two rounds columns, so a right-aligned number never meets the
// next column's text.
const COLUMN_GAP: Pixels = px(12.);
// The open/closed chevron's box, which the header leaves empty so the columns line up.
const CHEVRON: Pixels = px(16.);

// One team number column: its logs.tf title and how a round reads it.
type Column = (&'static str, fn(&RoundLine) -> u32);

const COLUMNS: [Column; 6] = [
    ("BLU K", |r| r.blue().kills()),
    ("RED K", |r| r.red().kills()),
    ("BLU UC", |r| r.blue().ubers()),
    ("RED UC", |r| r.red().ubers()),
    ("BLU DA", |r| r.blue().dmg()),
    ("RED DA", |r| r.red().dmg()),
];

/// Log `log`'s rounds in logs.tf's columns, one row each. A click on a row asks `page` to open
/// or close it; each round in `open` shows its players and events under its row. The rows
/// scroll sideways in boxes that break at each open round, so a box's bar sits under its rows
/// and the round's panel, sized to the window, never scrolls with them. The boxes share one
/// offset, so every part stays under the header.
pub(super) fn rounds_table(
    log: u64,
    detail: &LogDetail,
    open: &BTreeSet<usize>,
    tracked: &super::Tracked,
    page: WeakEntity<LogsTfPage>,
    cx: &App,
) -> Div {
    let team_of = |id: SteamId| {
        detail
            .players()
            .iter()
            .find(|p| p.steam_id() == id)
            .map(|p| p.team())
    };
    // Players go by their names in the log; the round panel puts nicknames over them.
    let log_name = |id: SteamId| {
        detail
            .players()
            .iter()
            .find(|p| p.steam_id() == id)
            .map_or_else(|| id.id64().to_string(), |p| p.name().to_owned())
    };
    let head = h_flex()
        .id(ids::LOGSTF_ROUNDS_HEAD)
        .test_support()
        .gap(COLUMN_GAP)
        .px_1()
        .py_1()
        .text_color(cx.theme().muted_foreground)
        .font_weight(FontWeight::MEDIUM)
        .child(spread(ROUND).child("Round"))
        .child(spread(LENGTH).child("Length"))
        .child(spread(SCORE).child("Score"))
        .children(COLUMNS.map(|(title, _)| {
            spread(TEAM_NUMBER)
                .id(ids::logstf_rounds_column(title))
                .test_support()
                .text_right()
                .child(title)
        }))
        .child(spread(MIDFIGHTS).child("Midfights"))
        .child(div().w(CHEVRON).flex_none());
    let mut table = v_flex();
    let mut rows = v_flex().child(head);
    let mut scroll_id: ElementId = ids::LOGSTF_ROUNDS_SCROLL.into();
    let last = detail.rounds().len().saturating_sub(1);
    for (n, round) in detail.rounds().iter().enumerate() {
        let is_open = open.contains(&n);
        rows = rows.child(round_row(log, n, round, is_open, page.clone(), cx));
        if is_open || n == last {
            let part = sideways(scroll_id, std::mem::replace(&mut rows, v_flex()));
            // Keyed by log too, so another log opens at its own start.
            table = table.child(part.shared((ids::LOGSTF_ROUNDS_SCROLL, log)));
            scroll_id = ids::logstf_rounds_scroll_after(n);
        }
        if is_open {
            table = table.child(round_panel(log, n, round, &log_name, tracked, &team_of, cx));
        }
    }
    table
}

// One round's line: number, length, the score in its winner's colour, each team's kills,
// ubers and damage, the midfight winner and a chevron showing whether it is open.
fn round_row(
    log: u64,
    n: usize,
    round: &RoundLine,
    is_open: bool,
    page: WeakEntity<LogsTfPage>,
    cx: &App,
) -> impl IntoElement {
    let colour = |team: Option<Team>| team.map(|t| team_colour(t, cx));
    let score = format!("{} - {}", round.blue().score(), round.red().score());
    // The winner's colour is said in words too, for readers that cannot see it.
    let score = div()
        .id(ids::logstf_round_score(log, n))
        .aria_label(score.clone())
        .when_some(round.winner(), |el, team| {
            el.text_color(team_colour(team, cx))
                .aria_description(format!("{} won", team_name(team)))
        })
        .test_support()
        .flex_1()
        .min_w(SCORE)
        .font_weight(FontWeight::SEMIBOLD)
        .child(score);
    let midfight = round.midfight().map_or("-", team_name);
    let midfight = labelled(
        spread(MIDFIGHTS).when_some(colour(round.midfight()), |el, c| {
            el.text_color(c).font_weight(FontWeight::BOLD)
        }),
        ids::logstf_round_midfight(log, n),
        midfight,
    );
    let chevron = if is_open {
        IconName::ChevronDown
    } else {
        IconName::ChevronRight
    };
    h_flex()
        .id(ids::logstf_round(log, n))
        .test_support()
        .gap(COLUMN_GAP)
        .px_1()
        .py_1()
        .border_t_1()
        .border_color(cx.theme().border)
        .cursor_pointer()
        .hover(|s| s.bg(cx.theme().accent))
        .on_click(move |_, _, cx| {
            let _ = page.update(cx, |page, cx| page.toggle_round(n, cx));
        })
        .child(spread(ROUND).child((n + 1).to_string()))
        .child(spread(LENGTH).child(round_length_text(round.length())))
        .child(score)
        .children(COLUMNS.map(|(title, read)| {
            spread(TEAM_NUMBER)
                .id(ids::logstf_round_cell(log, n, title))
                .test_support()
                .text_right()
                .child(read(round).to_string())
        }))
        .child(midfight)
        .child(
            h_flex()
                .w(CHEVRON)
                .flex_none()
                .justify_center()
                .child(Icon::new(chevron).xsmall()),
        )
}

// An open round: its players by kills then damage, then its events on the log's clock with
// each name in a description in its own team's colour, as logs.tf shows them.
fn round_panel(
    log: u64,
    n: usize,
    round: &RoundLine,
    log_name: &dyn Fn(SteamId) -> String,
    tracked: &super::Tracked,
    team_of: &dyn Fn(SteamId) -> Option<Team>,
    cx: &App,
) -> impl IntoElement {
    let name_of = |id: SteamId| tracked.shown(id, &log_name(id));
    let head = || {
        h_flex()
            .gap_1()
            .py_1()
            .text_color(cx.theme().muted_foreground)
            .font_weight(FontWeight::MEDIUM)
    };
    let line = || {
        h_flex()
            .gap_1()
            .py_0p5()
            .border_t_1()
            .border_color(cx.theme().border)
    };
    let players = v_flex()
        .child(
            head()
                .id(ids::logstf_round_players_head(log, n))
                .test_support()
                .child(div().w(TEAM).flex_none().child("Team"))
                .child(div().w(NAME).flex_none().child("Name"))
                .child(round_number("Kills".to_owned()))
                .child(round_number("Damage".to_owned())),
        )
        .children(round.players().iter().map(|p| {
            let name = name_of(p.steam_id());
            let in_log = log_name(p.steam_id());
            let row = line().id(ids::logstf_round_player(log, n, p.steam_id()));
            super::player_row(row, tracked.is(p.steam_id()), cx)
                .aria_label(format!("{name} {} {}", p.kills(), p.dmg()))
                .test_support()
                .child(team_cell(p.team(), cx).w(TEAM))
                .child(super::name_cell(
                    div().id(ids::LOGSTF_ROUND_NAME).w(NAME).flex_none(),
                    p.steam_id(),
                    &in_log,
                    tracked,
                    &format!("round:{log}:{n}"),
                ))
                .child(round_number(p.kills().to_string()))
                .child(round_number(p.dmg().to_string()))
        }));
    let events = v_flex()
        .child(
            head()
                .child(div().w(NUMBER).flex_none().child("Time"))
                .child(div().w(TYPE).flex_none().child("Type"))
                .child(div().flex_1().min_w(DESCRIPTION_MIN).child("Description")),
        )
        .children(round.events().iter().enumerate().map(|(ix, event)| {
            let time = round_length_text(event.at());
            let (ty, parts) = event_parts(event.kind(), &name_of, team_of);
            let (description, colours) = coloured(&parts, cx);
            let label = [time.as_str(), &ty, &description]
                .into_iter()
                .filter(|part| !part.is_empty())
                .collect::<Vec<_>>()
                .join(" ");
            line()
                .id(ids::logstf_event(log, n, ix))
                .aria_label(label)
                .test_support()
                .child(div().w(NUMBER).flex_none().child(time))
                .child(div().w(TYPE).flex_none().child(ty))
                .child(
                    div()
                        .flex_1()
                        .min_w(DESCRIPTION_MIN)
                        .child(StyledText::new(description).with_highlights(colours)),
                )
        }));
    v_flex()
        .id(ids::logstf_round_panel(log, n))
        .test_support()
        .gap_3()
        .pl_4()
        .py_2()
        .border_t_1()
        .border_color(cx.theme().border)
        .child(players)
        .child(events)
}

// A rounds table column at least `width` wide that takes its share of the spare width and
// keeps its text on one line.
fn spread(width: Pixels) -> Div {
    div().flex_1().min_w(width).whitespace_nowrap()
}

// A number in an open round's players, wide enough for "Damage" on one line.
fn round_number(text: String) -> Div {
    div()
        .w(ROUND_NUMBER)
        .flex_none()
        .text_right()
        .whitespace_nowrap()
        .child(text)
}

// An event's description as one string, with each stretch that names a team or one of its
// players highlighted in that team's colour.
fn coloured(
    parts: &[EventPart],
    cx: &App,
) -> (String, Vec<(std::ops::Range<usize>, HighlightStyle)>) {
    let mut text = String::new();
    let mut colours = Vec::new();
    for (part, team) in parts {
        let start = text.len();
        text.push_str(part);
        if let Some(team) = team {
            let style = HighlightStyle {
                color: Some(team_colour(*team, cx)),
                ..HighlightStyle::default()
            };
            colours.push((start..text.len(), style));
        }
    }
    (text, colours)
}
