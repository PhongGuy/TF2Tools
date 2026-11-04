mod support;

use gpui_kit::component::v_flex;
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{
    AnyWindowHandle, AppContext as _, Bounds, Context, ElementId, InteractiveElement as _,
    IntoElement, ParentElement as _, Render, ScrollHandle, StatefulInteractiveElement as _,
    Styled as _, TestAppContext, TestSupportExt as _, Window, WindowBounds, WindowOptions, div,
    point, px, size,
};
use support::scroll::{
    assert_bar_beside, assert_bar_on_right_edge, assert_clear_of_bar, assert_track_scrolls,
    assert_track_scrolls_virtual, assert_track_stays, bounds, press_track_foot,
};
use tf2tools_app::ids;
use tf2tools_app::pages::profiles::probe::Tf2Probe;
use tf2tools_app::services::Tf2State;
use tf2tools_app::shell::route::Page;
use tf2tools_app::widgets::card_grid::CardGrid;
use tf2tools_app::widgets::scroll::scroll_area;
use tf2tools_core::{LogLevel, NewLogEntry, ProfileDraft, Provenance, Tf2Install};

const LIST: &str = "probe-list";
const BAR: &str = "probe-bar";

/// A list of `rows` 40 px rows in the shared scroll area.
struct Rows {
    scroll: ScrollHandle,
    rows: usize,
}

impl Render for Rows {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let list = v_flex()
            .id(LIST)
            .test_support()
            .size_full()
            .track_scroll(&self.scroll)
            .overflow_y_scroll()
            .children((0..self.rows).map(|ix| {
                div()
                    .id(("probe-row", ix))
                    .test_support()
                    .flex_none()
                    .h(px(40.))
                    .child(format!("row {ix}"))
            }));
        scroll_area(BAR, &self.scroll, list).size_full()
    }
}

fn rows_window(cx: &mut TestAppContext, rows: usize) -> AnyWindowHandle {
    support::init(cx);
    let options = WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(Bounds {
            origin: point(px(0.), px(0.)),
            size: size(px(400.), px(300.)),
        })),
        ..WindowOptions::default()
    };
    cx.update(|cx| {
        gpui_kit::open_window(options, cx, |_, cx| {
            cx.new(|_| Rows {
                scroll: ScrollHandle::new(),
                rows,
            })
        })
    })
    .unwrap()
    .0
}

#[gpui_kit::test]
fn an_overflowing_area_shows_a_bar_on_its_right_edge(cx: &mut TestAppContext) {
    let win = rows_window(cx, 30);
    assert_bar_on_right_edge(cx, win, LIST, BAR);
    let last = ElementId::from(("probe-row", 29usize));
    assert_track_scrolls(cx, win, LIST, last.clone());
    let (list, last) = (bounds(cx, win, LIST), bounds(cx, win, last));
    assert!(
        last.bottom() <= list.bottom() + px(1.),
        "{last:?} not in {list:?}"
    );
}

#[gpui_kit::test]
fn an_area_that_fits_stays_put(cx: &mut TestAppContext) {
    let win = rows_window(cx, 2);
    assert_bar_on_right_edge(cx, win, LIST, BAR);
    let first = ElementId::from(("probe-row", 0usize));
    let before = bounds(cx, win, first.clone());
    press_track_foot(cx, win, LIST);
    assert_eq!(bounds(cx, win, first), before);
}

fn found(env: &support::Env) -> Tf2State {
    Tf2State::Found(Tf2Install::from_path(&env.game).unwrap())
}

fn resize(cx: &mut TestAppContext, s: &support::ShellWindow, w: f32, h: f32) {
    cx.simulate_window_resize(s.win, size(px(w), px(h)));
    support::settle(cx, s.win);
}

fn open(cx: &mut TestAppContext, s: &support::ShellWindow, page: Page) {
    support::click_id(cx, s.win, ids::nav(page));
    support::settle(cx, s.win);
}

#[gpui_kit::test]
fn the_settings_page_shows_a_bar(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    resize(cx, &s, 800., 600.);
    open(cx, &s, Page::Settings);
    let page = cx.update(|cx| s.shell.read(cx).settings_page()).unwrap();
    support::wait_real(cx, "settings to load", |cx| {
        cx.update(|cx| page.read(cx).is_loaded())
    });
    let area = ids::page(Page::Settings);
    assert_bar_on_right_edge(cx, s.win, area.clone(), ids::page_scrollbar(Page::Settings));
    assert_track_scrolls(cx, s.win, area, ids::SETTINGS_NOTICES);
}

#[gpui_kit::test]
fn the_licence_notices_show_a_bar(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    open(cx, &s, Page::Settings);
    let page = cx.update(|cx| s.shell.read(cx).settings_page()).unwrap();
    support::wait_real(cx, "settings to load", |cx| {
        cx.update(|cx| page.read(cx).is_loaded())
    });
    cx.update_window(s.win, |_, w, cx| {
        let down = gpui_kit::ScrollDelta::Pixels(point(px(0.), px(-4000.)));
        w.scroll(ids::page(Page::Settings), down, cx)
    })
    .unwrap();
    support::click_id(cx, s.win, ids::SETTINGS_NOTICES);
    let view = ids::SETTINGS_NOTICES_VIEW;
    assert_bar_on_right_edge(cx, s.win, view, ids::SETTINGS_NOTICES_SCROLLBAR);
    let last = tf2tools_app::notices::all()
        .last()
        .unwrap()
        .file()
        .to_owned();
    assert_track_scrolls(cx, s.win, view, ids::settings_notice(&last));
}

#[gpui_kit::test]
fn the_crosshairs_page_shows_a_bar(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    resize(cx, &s, 800., 600.);
    open(cx, &s, Page::Crosshairs);
    let page = cx.update(|cx| s.shell.read(cx).crosshairs_page()).unwrap();
    support::wait_real(cx, "Crosshairs to load", |cx| {
        cx.update(|cx| page.read(cx).is_loaded())
    });
    let area = ids::page(Page::Crosshairs);
    assert_bar_on_right_edge(
        cx,
        s.win,
        area.clone(),
        ids::page_scrollbar(Page::Crosshairs),
    );
    assert_track_scrolls(cx, s.win, area, ids::CROSSHAIRS_BACKGROUND);
}

#[gpui_kit::test]
fn the_addons_page_shows_a_bar(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    open(cx, &s, Page::Addons);
    let page = cx.update(|cx| s.shell.read(cx).addons_page()).unwrap();
    support::wait_real(cx, "Addons to load", |cx| {
        cx.update(|cx| page.read(cx).is_loaded())
    });
    // Short enough that the empty library's card runs past the page; the sidebar then hides
    // its lower links, so the page opens first.
    resize(cx, &s, 800., 300.);
    let area = ids::page(Page::Addons);
    assert_bar_on_right_edge(cx, s.win, area.clone(), ids::page_scrollbar(Page::Addons));
    assert_track_scrolls(cx, s.win, area, ids::ADDONS_BROWSE);
}

#[gpui_kit::test]
fn the_crosshair_maker_panel_shows_a_bar(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, Tf2State::Missing { saved: None });
    open(cx, &s, Page::CrosshairMaker);
    resize(cx, &s, 800., 400.);
    let panel = ids::CROSSHAIR_MAKER_PANEL;
    assert_bar_on_right_edge(cx, s.win, panel, ids::CROSSHAIR_MAKER_PANEL_SCROLLBAR);
    assert_track_scrolls(cx, s.win, panel, ids::CROSSHAIR_MAKER_COLOR_HINT);
}

#[gpui_kit::test]
fn the_crosshair_maker_panel_keeps_its_controls_clear_of_the_bar(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, Tf2State::Missing { saved: None });
    open(cx, &s, Page::CrosshairMaker);
    resize(cx, &s, 800., 400.);
    let panel = support::bounds_of(cx, s.win, ids::CROSSHAIR_MAKER_PANEL).expect("the panel");
    let name = support::bounds_of(cx, s.win, ids::CROSSHAIR_MAKER_NAME).expect("the name field");
    assert!(
        name.right() <= panel.right() - px(12.),
        "the name field ends at {:?}, under the bar at the panel's edge {:?}",
        name.right(),
        panel.right()
    );
}

/// Opens Logs over one entry with `lines` lines of details and expands them.
fn expanded_details(cx: &mut TestAppContext, s: &support::ShellWindow, lines: usize) -> i64 {
    let details: Vec<String> = (0..lines).map(|ix| format!("line {ix}")).collect();
    s.services
        .activity
        .append(
            &NewLogEntry::new(LogLevel::Error, "store", "download", "failed")
                .with_details(details.join("\n")),
        )
        .unwrap();
    let id = support::log_rows(&s.services)[0].id();
    open(cx, s, Page::Logs);
    let page = cx.update(|cx| s.shell.read(cx).logs_page()).unwrap();
    support::wait_real(cx, "logs to load", |cx| {
        cx.update(|cx| page.read(cx).is_loaded())
    });
    support::click_id(cx, s.win, ids::log_details_toggle(id));
    id
}

#[gpui_kit::test]
fn long_log_details_show_a_bar(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, Tf2State::Missing { saved: None });
    let id = expanded_details(cx, &s, 40);
    let details = bounds(cx, s.win, ids::log_details(id));
    assert_eq!(details.size.height, px(192.), "the box keeps its cap");
    assert_bar_on_right_edge(
        cx,
        s.win,
        ids::log_details(id),
        ids::log_details_scrollbar(id),
    );
}

#[gpui_kit::test]
fn short_log_details_keep_their_height(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, Tf2State::Missing { saved: None });
    let id = expanded_details(cx, &s, 2);
    let details = bounds(cx, s.win, ids::log_details(id));
    assert!(details.size.height < px(192.), "{details:?}");
    assert_bar_on_right_edge(
        cx,
        s.win,
        ids::log_details(id),
        ids::log_details_scrollbar(id),
    );
}

const GRID: &str = "probe-grid";

/// A four-column store grid of `cards` 120 px cards inside the shell's page padding.
struct Grid {
    grid: CardGrid,
    cards: usize,
}

impl Render for Grid {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .p(tf2tools_app::shell::CONTENT_PADDING)
            .child(self.grid.render(self.cards, px(120.), |ix, _, _| {
                div()
                    .id(("probe-card", ix))
                    .test_support()
                    .size_full()
                    .into_any_element()
            }))
    }
}

fn grid_window(cx: &mut TestAppContext, cards: usize) -> AnyWindowHandle {
    support::init(cx);
    cx.update(|cx| {
        gpui_kit::open_window(support::window_options(), cx, |_, cx| {
            cx.new(|_| Grid {
                grid: CardGrid::store(GRID, 4),
                cards,
            })
        })
    })
    .unwrap()
    .0
}

#[gpui_kit::test]
fn a_card_grid_shows_a_bar_beside_its_cards(cx: &mut TestAppContext) {
    let win = grid_window(cx, 200);
    let bar = ids::grid_scrollbar(GRID);
    assert_bar_beside(cx, win, GRID, bar.clone());
    assert_clear_of_bar(cx, win, ("probe-card", 3usize), bar.clone());
    assert_track_scrolls_virtual(cx, win, bar, ("probe-card", 0usize));
}

#[gpui_kit::test]
fn a_card_grid_that_fits_stays_put(cx: &mut TestAppContext) {
    let win = grid_window(cx, 2);
    let bar = ids::grid_scrollbar(GRID);
    assert_bar_beside(cx, win, GRID, bar.clone());
    assert_track_stays(cx, win, bar, ("probe-card", 0usize));
}

/// Imports `n` HUDs named hud00, hud01 and so on.
fn seed_huds(s: &support::ShellWindow, env: &support::Env, n: usize) {
    for ix in 0..n {
        let name = format!("hud{ix:02}");
        let path = env.tmp.path().join(format!("{name}.zip"));
        std::fs::write(&path, support::hud_zip_bytes(&name)).unwrap();
        s.services
            .library
            .import_hud(&path, Provenance::Local)
            .unwrap();
    }
}

/// Opens a fresh HUDs page, by way of Settings, so it lists the HUDs seeded since the shell opened.
fn open_huds(cx: &mut TestAppContext, s: &support::ShellWindow) {
    open(cx, s, Page::Settings);
    open(cx, s, Page::Huds);
    let page = cx.update(|cx| s.shell.read(cx).huds_page()).unwrap();
    support::wait_real(cx, "HUDs to load", |cx| {
        cx.update(|cx| page.read(cx).is_loaded())
    });
    support::settle(cx, s.win);
}

// The library keeps a 320 px floor, so the page itself still scrolls a little at 1280x800.
#[gpui_kit::test]
fn at_1280x1024_only_the_huds_grid_scrolls(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    seed_huds(&s, &env, 24);
    resize(cx, &s, 1280., 1024.);
    open_huds(cx, &s);
    let bar = ids::grid_scrollbar(ids::HUDS_GRID);
    assert_bar_beside(cx, s.win, ids::HUDS_GRID, bar.clone());
    assert_track_stays(cx, s.win, ids::page(Page::Huds), ids::HUDS_LIBRARY);
    assert_track_scrolls_virtual(cx, s.win, bar, ids::hud_card("hud00"));
}

#[gpui_kit::test]
fn at_800x600_the_huds_page_and_its_grid_each_show_a_bar(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    seed_huds(&s, &env, 24);
    resize(cx, &s, 800., 600.);
    open_huds(cx, &s);
    let page = ids::page(Page::Huds);
    assert_bar_on_right_edge(cx, s.win, page.clone(), ids::page_scrollbar(Page::Huds));
    assert_track_scrolls(cx, s.win, page, ids::HUDS_LIBRARY);
    let bar = ids::grid_scrollbar(ids::HUDS_GRID);
    assert_bar_beside(cx, s.win, ids::HUDS_GRID, bar.clone());
    assert_track_scrolls_virtual(cx, s.win, bar, ids::hud_card("hud00"));
}

/// Creates `n` empty profiles and opens Profiles; returns the first profile's card id.
fn profiles_page(cx: &mut TestAppContext, s: &support::ShellWindow, n: usize) -> ElementId {
    let profiles: Vec<_> = (0..n)
        .map(|ix| {
            s.services
                .library
                .create_profile(&ProfileDraft::new(format!("Profile {ix:02}")))
                .unwrap()
        })
        .collect();
    let first = &profiles[0];
    cx.update(|cx| Tf2Probe::install(cx, || false));
    open(cx, s, Page::Profiles);
    let page = cx.update(|cx| s.shell.read(cx).profiles_page()).unwrap();
    support::wait_real(cx, "Profiles to load", |cx| {
        cx.update(|cx| page.read(cx).is_loaded())
    });
    support::settle(cx, s.win);
    ids::profile_card(first.id())
}

#[gpui_kit::test]
fn at_1280x800_only_the_profiles_grid_scrolls(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    resize(cx, &s, 1280., 800.);
    let first = profiles_page(cx, &s, 24);
    let bar = ids::grid_scrollbar(ids::PROFILES_GRID);
    assert_bar_beside(cx, s.win, ids::PROFILES_GRID, bar.clone());
    assert_track_stays(cx, s.win, ids::page(Page::Profiles), ids::PROFILES_LIBRARY);
    assert_track_scrolls_virtual(cx, s.win, bar, first);
}

#[gpui_kit::test]
fn at_800x600_the_profiles_page_and_its_grid_each_show_a_bar(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    resize(cx, &s, 800., 600.);
    let first = profiles_page(cx, &s, 24);
    let page = ids::page(Page::Profiles);
    assert_bar_on_right_edge(cx, s.win, page.clone(), ids::page_scrollbar(Page::Profiles));
    assert_track_scrolls(cx, s.win, page, ids::PROFILES_LIBRARY);
    let bar = ids::grid_scrollbar(ids::PROFILES_GRID);
    assert_bar_beside(cx, s.win, ids::PROFILES_GRID, bar.clone());
    assert_track_scrolls_virtual(cx, s.win, bar, first);
}

#[gpui_kit::test]
fn a_long_log_shows_a_bar_beside_its_rows(cx: &mut TestAppContext) {
    let env = support::env();
    support::seed_log_rows(&env, 60);
    let s = support::shell_window(cx, &env, Tf2State::Missing { saved: None });
    let first = support::log_rows(&s.services)[0].id();
    open(cx, &s, Page::Logs);
    let page = cx.update(|cx| s.shell.read(cx).logs_page()).unwrap();
    support::wait_real(cx, "logs to load", |cx| {
        cx.update(|cx| page.read(cx).is_loaded())
    });
    assert_bar_beside(cx, s.win, ids::LOGS_LIST, ids::LOGS_LIST_SCROLLBAR);
    assert_clear_of_bar(cx, s.win, ids::log_row(first), ids::LOGS_LIST_SCROLLBAR);
    assert_track_scrolls_virtual(cx, s.win, ids::LOGS_LIST_SCROLLBAR, ids::log_row(first));
}
