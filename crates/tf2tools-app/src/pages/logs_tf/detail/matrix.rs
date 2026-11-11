//! The open log's kill matrix: every player's kills, kills plus assists or deaths against each
//! class, one tab per kind as on logs.tf.

use std::collections::HashMap;

use gpui_kit::component::button::Button;
use gpui_kit::component::{ActiveTheme as _, Selectable as _, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    App, Div, FontWeight, InteractiveElement as _, ParentElement as _, Pixels, Role,
    StatefulInteractiveElement as _, Styled as _, TestSupportExt as _, WeakEntity, div, px,
};
use tf2tools_core::{LogDetail, MatrixKind, PlayerLine, SteamId, TfClass};

use super::{ICON, class_cell, team_cell};
use crate::ids;
use crate::pages::logs_tf::LogsTfPage;
use crate::pages::logs_tf::tables::matrix_rows;
use crate::services::Services;
use crate::widgets::class_icon::class_icon;
use crate::widgets::scroll::sideways;

// The fixed columns before the nine classes; the name takes what is left.
const TEAM: Pixels = px(40.);
const NAME_MIN: Pixels = px(120.);
const MAIN: Pixels = px(24.);

const TABS: [(MatrixKind, &str); 3] = [
    (MatrixKind::Kills, "Kills"),
    (MatrixKind::KillsAssists, "Kills+Assists"),
    (MatrixKind::Deaths, "Deaths"),
];

/// The tabs over `kind`'s matrix, every player a row, K high to low. A tab asks `page` to
/// show its kind; tracked players stay highlighted as in the players table.
pub(super) fn matrix(
    log: &LogDetail,
    kind: MatrixKind,
    is_tracked: &dyn Fn(SteamId) -> bool,
    page: WeakEntity<LogsTfPage>,
    cx: &App,
) -> Div {
    // A kit Button paints its selected state but never reports it, so each sits in a tab
    // that carries the id and says whether it is shown.
    let tab = |(tab, title): (MatrixKind, &'static str)| {
        let page = page.clone();
        let button = Button::new(ids::logstf_matrix_tab_button(tab))
            .outline()
            .xsmall()
            .label(title)
            .selected(tab == kind)
            .on_click(move |_, _, cx| {
                let _ = page.update(cx, |page, cx| page.show_matrix(tab, cx));
            });
        div()
            .id(ids::logstf_matrix_tab(tab))
            .role(Role::Tab)
            .aria_label(title)
            .aria_selected(tab == kind)
            .test_support()
            .child(button)
    };
    let tabs = h_flex()
        .id(ids::LOGSTF_MATRIX_TABS)
        .test_support()
        .gap_1()
        .children(TABS.map(tab));
    v_flex().gap_2().child(tabs).child(sideways(
        ids::LOGSTF_MATRIX_SCROLL,
        table(log, kind, is_tracked, cx),
    ))
}

fn table(log: &LogDetail, kind: MatrixKind, is_tracked: &dyn Fn(SteamId) -> bool, cx: &App) -> Div {
    let muted = cx.theme().muted_foreground;
    let heading = |text: &'static str| div().flex_none().child(text);
    let mut table = v_flex().w_full().child(
        h_flex()
            .gap_1()
            .px_1()
            .py_1()
            .text_color(muted)
            .font_weight(FontWeight::MEDIUM)
            .child(heading("Team").w(TEAM))
            .child(heading("Player").flex_1().min_w(NAME_MIN))
            .child(heading("C").w(MAIN))
            .children(TfClass::ALL.map(|class| class_cell(class, cx)))
            .child(super::number("K".into())),
    );
    let players: HashMap<SteamId, &PlayerLine> =
        log.players().iter().map(|p| (p.steam_id(), p)).collect();
    let assets = &cx.global::<Services>().assets;
    for row in matrix_rows(log, kind) {
        let id = row.steam_id();
        let Some(player) = players.get(&id) else {
            continue;
        };
        let main = player.classes().first().map(|c| {
            let class = c.class();
            class_icon(class, assets.class_icon(class).as_deref(), ICON)
        });
        // Every count keeps an id so a pilot can read any cell; zeros fade as in the players
        // table.
        let cell = |class: Option<TfClass>, n: u32| {
            super::number(n.to_string())
                .id(ids::logstf_matrix_cell(id, class))
                .test_support()
                .when(n == 0, |el| el.text_color(muted))
        };
        let counts = row.counts();
        table = table.child(
            h_flex()
                .id(ids::logstf_matrix_row(id))
                .test_support()
                .gap_1()
                .px_1()
                .py_1()
                .border_t_1()
                .border_color(cx.theme().border)
                .when(is_tracked(id), |el| {
                    el.bg(cx.theme().primary.opacity(0.12))
                        .font_weight(FontWeight::SEMIBOLD)
                })
                .child(team_cell(player.team(), cx).w(TEAM))
                .child(
                    div()
                        .flex_1()
                        .min_w(NAME_MIN)
                        .truncate()
                        .child(player.name().to_owned()),
                )
                .child(div().w(MAIN).flex_none().children(main))
                .children(
                    TfClass::ALL
                        .into_iter()
                        .zip(counts)
                        .map(|(class, n)| cell(Some(class), n)),
                )
                .child(cell(None, row.total())),
        );
    }
    table
}
