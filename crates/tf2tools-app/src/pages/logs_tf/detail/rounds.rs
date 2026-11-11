//! The open log's rounds table, where a click opens a round into its players and events.

use std::collections::BTreeSet;

use gpui_kit::assets::IconName;
use gpui_kit::component::{ActiveTheme as _, Icon, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    App, Div, FontWeight, HighlightStyle, InteractiveElement as _, IntoElement, ParentElement as _,
    Pixels, StatefulInteractiveElement as _, Styled as _, StyledText, TestSupportExt as _,
    WeakEntity, div, px,
};
use tf2tools_core::{LogDetail, RoundLine, SteamId, Team};

use super::{NUMBER, number, team_cell, team_colour};
use crate::ids;
use crate::pages::logs_tf::LogsTfPage;
use crate::pages::logs_tf::text::{EventPart, duration_text, event_parts, team_name};
use crate::widgets::labelled;

// The rounds table's fixed columns, and the open round's name and description columns.
const ROUND: Pixels = px(48.);
const LENGTH: Pixels = px(52.);
const SCORE: Pixels = px(56.);
const MIDFIGHTS: Pixels = px(64.);
const TEAM: Pixels = px(40.);
const NAME: Pixels = px(200.);
const TYPE: Pixels = px(96.);
const DESCRIPTION: Pixels = px(280.);

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
/// or close it; each round in `open` shows its players and events under its row.
pub(super) fn rounds_table(
    log: u64,
    detail: &LogDetail,
    open: &BTreeSet<usize>,
    name_of: &dyn Fn(SteamId) -> String,
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
    let fixed = |width: Pixels, text: &'static str| div().w(width).flex_none().child(text);
    let mut table = v_flex().child(
        h_flex()
            .gap_1()
            .px_1()
            .py_1()
            .text_color(cx.theme().muted_foreground)
            .font_weight(FontWeight::MEDIUM)
            .child(fixed(ROUND, "Round"))
            .child(fixed(LENGTH, "Length"))
            .child(fixed(SCORE, "Score"))
            .children(COLUMNS.map(|(title, _)| number(title.to_owned())))
            .child(fixed(MIDFIGHTS, "Midfights"))
            .child(div().w(px(16.)).flex_none()),
    );
    for (n, round) in detail.rounds().iter().enumerate() {
        let is_open = open.contains(&n);
        table = table.child(round_row(log, n, round, is_open, page.clone(), cx));
        if is_open {
            table = table.child(round_panel(log, n, round, name_of, &team_of, cx));
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
        .w(SCORE)
        .flex_none()
        .font_weight(FontWeight::SEMIBOLD)
        .child(score);
    let midfight = round.midfight().map_or("-", team_name);
    let midfight = labelled(
        div()
            .w(MIDFIGHTS)
            .flex_none()
            .when_some(colour(round.midfight()), |el, c| {
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
        .gap_1()
        .px_1()
        .py_1()
        .border_t_1()
        .border_color(cx.theme().border)
        .cursor_pointer()
        .hover(|s| s.bg(cx.theme().accent))
        .on_click(move |_, _, cx| {
            let _ = page.update(cx, |page, cx| page.toggle_round(n, cx));
        })
        .child(div().w(ROUND).flex_none().child((n + 1).to_string()))
        .child(
            div()
                .w(LENGTH)
                .flex_none()
                .child(duration_text(round.length())),
        )
        .child(score)
        .children(COLUMNS.map(|(_, read)| number(read(round).to_string())))
        .child(midfight)
        .child(Icon::new(chevron).xsmall().flex_none())
}

// An open round: its players by kills then damage, then its events on the log's clock with
// each name in a description in its own team's colour, as logs.tf shows them.
fn round_panel(
    log: u64,
    n: usize,
    round: &RoundLine,
    name_of: &dyn Fn(SteamId) -> String,
    team_of: &dyn Fn(SteamId) -> Option<Team>,
    cx: &App,
) -> impl IntoElement {
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
                .child(div().w(TEAM).flex_none().child("Team"))
                .child(div().w(NAME).flex_none().child("Name"))
                .child(number("Kills".to_owned()))
                .child(number("Damage".to_owned())),
        )
        .children(round.players().iter().map(|p| {
            let name = name_of(p.steam_id());
            line()
                .id(ids::logstf_round_player(log, n, p.steam_id()))
                .aria_label(format!("{name} {} {}", p.kills(), p.dmg()))
                .test_support()
                .child(team_cell(p.team(), cx).w(TEAM))
                .child(div().w(NAME).flex_none().truncate().child(name))
                .child(number(p.kills().to_string()))
                .child(number(p.dmg().to_string()))
        }));
    let events = v_flex()
        .child(
            head()
                .child(div().w(NUMBER).flex_none().child("Time"))
                .child(div().w(TYPE).flex_none().child("Type"))
                .child(div().w(DESCRIPTION).flex_none().child("Description")),
        )
        .children(round.events().iter().enumerate().map(|(ix, event)| {
            let time = duration_text(event.at());
            let (ty, parts) = event_parts(event.kind(), name_of, team_of);
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
                        .w(DESCRIPTION)
                        .flex_none()
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
