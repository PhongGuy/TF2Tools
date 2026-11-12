//! The open log's kill matrix: every player's kills, kills plus assists or deaths against each
//! class, one tab per kind as on logs.tf.

use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::{ActiveTheme as _, Selectable as _, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    App, Div, FontWeight, InteractiveElement as _, ParentElement as _, Pixels, Role,
    StatefulInteractiveElement as _, Styled as _, TestSupportExt as _, WeakEntity, div, px,
};
use tf2tools_core::{LogDetail, MatrixKind, TfClass};

use super::{ICON, NUMBER, team_cell};
use crate::ids;
use crate::pages::logs_tf::LogsTfPage;
use crate::pages::logs_tf::head::sortable;
use crate::pages::logs_tf::sort::{MatrixColumn, MatrixSort, sort_matrix_rows};
use crate::pages::logs_tf::tables::matrix_rows;
use crate::services::Services;
use crate::widgets::class_icon::class_icon;
use crate::widgets::scroll::sideways;

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

/// The tabs over `kind`'s matrix, a row for each player with a count, K high to low or by
/// `sort`. A tab asks `page` to show its kind and a header to sort by its column; tracked
/// players stay highlighted as in the players table.
pub(super) fn matrix(
    log: &LogDetail,
    (kind, sort): (MatrixKind, Option<MatrixSort>),
    tracked: &super::Tracked,
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
        table(log, kind, sort, tracked, page, cx),
    ))
}

fn table(
    log: &LogDetail,
    kind: MatrixKind,
    sort: Option<MatrixSort>,
    tracked: &super::Tracked,
    page: WeakEntity<LogsTfPage>,
    cx: &App,
) -> Div {
    let muted = cx.theme().muted_foreground;
    // The plain headings sit at the buttons' height, level with their titles.
    let heading = |text: &'static str| div().flex_none().h_5().flex().items_center().child(text);
    let total = TABS.iter().find(|t| t.0 == kind).map_or("K", |t| t.2);
    let assets = &cx.global::<Services>().assets;
    // Each count column's header is a button that sorts by it, its arrow centred under it.
    let header = |column: MatrixColumn| {
        let page = page.clone();
        let sorted = sort.filter(|s| s.column == column);
        let (id, class) = match column {
            MatrixColumn::Class(class) => (ids::logstf_matrix_head(class), Some(class)),
            MatrixColumn::Total => (ids::LOGSTF_MATRIX_TOTAL.into(), None),
        };
        let button = Button::new(id)
            .ghost()
            .xsmall()
            .compact()
            .selected(sorted.is_some())
            .w_full()
            .justify_end()
            .px_0()
            .on_click(move |_, _, cx| {
                let _ = page.update(cx, |page, cx| page.sort_matrix_by(column, cx));
            });
        let button = match class {
            Some(class) => button.accessibility_label(class.name()).child(class_icon(
                class,
                assets.class_icon(class).as_deref(),
                ICON,
            )),
            None => button.label(total),
        };
        sortable(
            button,
            ids::logstf_matrix_arrow(class),
            sorted.map(|s| s.descending),
        )
        .w(NUMBER)
        .items_end()
    };
    let mut table = v_flex().w_full().child(
        h_flex()
            .items_start()
            .gap_1()
            .px_1()
            .py_1()
            .text_color(muted)
            .font_weight(FontWeight::MEDIUM)
            .child(heading("Team").w(TEAM))
            .child(heading("Player").w(NAME_MIN).flex_grow(1.))
            .child(heading("C").w(MAIN))
            .children(TfClass::ALL.map(|class| header(MatrixColumn::Class(class))))
            .child(header(MatrixColumn::Total)),
    );
    for (player, row) in sort_matrix_rows(matrix_rows(log, kind), sort) {
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
            super::player_row(h_flex().id(ids::logstf_matrix_row(id)), tracked.is(id), cx)
                .test_support()
                .gap_1()
                .px_1()
                .py_1()
                .border_t_1()
                .border_color(cx.theme().border)
                .child(team_cell(player.team(), cx).w(TEAM))
                .child(super::name_cell(
                    div()
                        .id(ids::logstf_matrix_name(id))
                        .w(NAME_MIN)
                        .flex_grow(1.),
                    id,
                    player.name(),
                    tracked,
                    "matrix",
                ))
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
