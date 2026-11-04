mod support;

use gpui_kit::component::v_flex;
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{
    AnyWindowHandle, AppContext as _, Bounds, Context, ElementId, InteractiveElement as _,
    IntoElement, ParentElement as _, Pixels, Render, ScrollHandle, StatefulInteractiveElement as _,
    Styled as _, TestAppContext, TestSupportExt as _, Window, WindowBounds, WindowOptions, div,
    point, px, size,
};
use tf2tools_app::ids;
use tf2tools_app::services::Tf2State;
use tf2tools_app::shell::route::Page;
use tf2tools_app::widgets::scroll::scroll_area;
use tf2tools_core::{LogLevel, NewLogEntry, Tf2Install};

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

fn bounds(
    cx: &mut TestAppContext,
    win: AnyWindowHandle,
    id: impl Into<ElementId>,
) -> Bounds<Pixels> {
    let id = id.into();
    support::bounds_of(cx, win, id.clone()).unwrap_or_else(|| panic!("no {id:?}"))
}

/// Asserts the scrollbar `bar` spans the right edge of `area`, top to bottom.
fn assert_bar_on_right_edge(
    cx: &mut TestAppContext,
    win: AnyWindowHandle,
    area: impl Into<ElementId>,
    bar: impl Into<ElementId>,
) {
    let (area, bar) = (bounds(cx, win, area), bounds(cx, win, bar));
    let near = |a: Pixels, b: Pixels| (a - b).abs() <= px(1.);
    assert!(
        near(bar.right(), area.right())
            && near(bar.top(), area.top())
            && near(bar.bottom(), area.bottom()),
        "the bar {bar:?} is not on the right edge of {area:?}"
    );
}

/// Presses the foot of `area`'s scrollbar track; a drawn bar scrolls toward the end.
fn press_track_foot(cx: &mut TestAppContext, win: AnyWindowHandle, area: impl Into<ElementId>) {
    let area = area.into();
    let b = bounds(cx, win, area.clone());
    cx.update_window(win, |_, w, cx| {
        w.click_at(
            area,
            point(b.size.width - px(6.), b.size.height - px(6.)),
            cx,
        )
    })
    .unwrap();
    support::settle(cx, win);
}

/// Asserts a press on `area`'s track foot moves `probe` up by more than a few pixels.
fn assert_track_scrolls(
    cx: &mut TestAppContext,
    win: AnyWindowHandle,
    area: impl Into<ElementId>,
    probe: impl Into<ElementId>,
) {
    let (area, probe) = (area.into(), probe.into());
    let before = bounds(cx, win, probe.clone());
    press_track_foot(cx, win, area.clone());
    let after = bounds(cx, win, probe);
    assert!(
        after.top() < before.top() - px(20.),
        "no scrollbar took the press in {area:?}: {before:?} -> {after:?}"
    );
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
