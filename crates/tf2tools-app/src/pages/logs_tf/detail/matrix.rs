//! The open log's kill matrix: every player's kills, kills plus assists or deaths against each
//! class, one tab per kind as on logs.tf.

use gpui_kit::component::button::Button;
use gpui_kit::component::{ActiveTheme as _, Selectable as _, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    App, Div, FontWeight, InteractiveElement as _, ParentElement as _, Pixels, Role,
    StatefulInteractiveElement as _, Styled as _, TestSupportExt as _, WeakEntity, div, px,
};
use tf2tools_core::{LogDetail, MatrixKind, SteamId, TfClass};

use super::{ICON, class_cell, team_cell};
use crate::ids;
use crate::pages::logs_tf::LogsTfPage;
use crate::pages::logs_tf::tables::matrix_rows;
use crate::services::Services;
use crate::widgets::class_icon::class_icon;
use crate::widgets::labelled;
use crate::widgets::scroll::sideways;
use crate::widgets::tip::tip;

// The fixed columns before the nine classes. The name is sized at its minimum so a long one
// never widens the table, and grows into any room the window leaves, truncating the rest.
const TEAM: Pixels = px(40.);
const NAME_MIN: Pixels = px(100.);
const MAIN: Pixels = px(24.);

// Each tab's title and its total column's header, as logs.tf names them.
const TABS: [(MatrixKind, &str, &str); 3] = [
    (MatrixKind::Kills, "Kills", "K"),
    (MatrixKind::KillsAssists, "Kills+Assists", "KA"),
    (MatrixKind::Deaths, "Deaths", "D"),
];

/// The tabs over `kind`'s matrix, a row for each player with a count, K high to low. A tab asks `page` to
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
    let tab = |(tab, title, _): (MatrixKind, &'static str, &'static str)| {
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
        .role(Role::TabList)
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
    let total = TABS.iter().find(|t| t.0 == kind).map_or("K", |t| t.2);
    let mut table = v_flex().w_full().child(
        h_flex()
            .gap_1()
            .px_1()
            .py_1()
            .text_color(muted)
            .font_weight(FontWeight::MEDIUM)
            .child(heading("Team").w(TEAM))
            .child(heading("Player").w(NAME_MIN).flex_grow(1.))
            .child(heading("C").w(MAIN))
            .children(TfClass::ALL.map(|class| {
                class_cell(class, cx)
                    .id(ids::logstf_matrix_head(class))
                    .test_support()
            }))
            .child(labelled(
                super::number(String::new()),
                ids::LOGSTF_MATRIX_TOTAL,
                total,
            )),
    );
    let assets = &cx.global::<Services>().assets;
    for (player, row) in matrix_rows(log, kind) {
        let id = row.steam_id();
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
            super::player_row(h_flex().id(ids::logstf_matrix_row(id)), is_tracked(id), cx)
                .test_support()
                .gap_1()
                .px_1()
                .py_1()
                .border_t_1()
                .border_color(cx.theme().border)
                .child(team_cell(player.team(), cx).w(TEAM))
                .child(
                    div()
                        .id(ids::logstf_matrix_name(id))
                        .test_support()
                        .tooltip(tip(ids::LOGSTF_NAME_TIP, player.name().to_owned()))
                        .w(NAME_MIN)
                        .flex_grow(1.)
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
