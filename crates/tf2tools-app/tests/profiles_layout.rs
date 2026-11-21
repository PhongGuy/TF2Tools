//! The Profiles page at small windows: the page scrolls as a whole over a grid of at least one
//! card row, as HUDs does, and its controls stay clear of the page's bar.

use crate::support;

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{
    AppContext as _, Bounds, ElementId, Entity, Pixels, ScrollDelta, ScrollHandle, TestAppContext,
    point, px, size,
};
use support::scroll::{
    assert_bar_beside, assert_bar_on_right_edge, assert_clear_of_bar, assert_track_scrolls,
    assert_track_scrolls_virtual, bounds,
};
use tf2tools_app::ids;
use tf2tools_app::pages::profiles::ProfilesPage;
use tf2tools_app::pages::profiles::probe::Tf2Probe;
use tf2tools_app::services::Tf2State;
use tf2tools_app::shell::CONTENT_PADDING;
use tf2tools_app::shell::route::Page;
use tf2tools_app::widgets::card_grid::GRID_GAP;
use tf2tools_core::{ProfileDraft, Tf2Install};

fn resize(cx: &mut TestAppContext, s: &support::ShellWindow, w: f32, h: f32) {
    cx.simulate_window_resize(s.win, size(px(w), px(h)));
    support::settle(cx, s.win);
}

/// Creates `n` empty profiles, opens Profiles and returns the page and the cards' ids.
fn profiles_page(
    cx: &mut TestAppContext,
    s: &support::ShellWindow,
    n: usize,
) -> (Entity<ProfilesPage>, Vec<ElementId>) {
    let cards = (0..n)
        .map(|ix| {
            let draft = ProfileDraft::new(format!("Profile {ix:02}"));
            ids::profile_card(s.services.library.create_profile(&draft).unwrap().id())
        })
        .collect();
    cx.update(|cx| Tf2Probe::install(cx, || false));
    support::click_id(cx, s.win, ids::nav(Page::Profiles));
    let page = cx.update(|cx| s.shell.read(cx).profiles_page()).unwrap();
    support::wait_real(cx, "Profiles to load", |cx| {
        cx.update(|cx| page.read(cx).is_loaded())
    });
    support::settle(cx, s.win);
    (page, cards)
}

/// The bounds of every card in `cards` the grid draws now.
fn drawn(
    cx: &mut TestAppContext,
    s: &support::ShellWindow,
    cards: &[ElementId],
) -> Vec<Bounds<Pixels>> {
    cards
        .iter()
        .filter_map(|id| support::bounds_of(cx, s.win, id.clone()))
        .collect()
}

fn to_end(cx: &mut TestAppContext, s: &support::ShellWindow, scroll: &ScrollHandle) {
    scroll.set_offset(point(px(0.), -scroll.max_offset().y));
    support::settle(cx, s.win);
}

// At 900x600 the page scrolls as a whole to a grid at least one card tall, the grid scrolls to its
// own last card, and the library row stays clear of the page's bar.
#[gpui_kit::test]
fn at_900x600_the_page_scrolls_to_a_whole_card_row(cx: &mut TestAppContext) {
    let env = support::env();
    let found = Tf2State::Found(Tf2Install::from_path(&env.game).unwrap());
    let s = support::shell_window(cx, &env, found);
    resize(cx, &s, 900., 600.);
    let (page, cards) = profiles_page(cx, &s, 12);
    let area = ids::page(Page::Profiles);
    let page_bar = ids::page_scrollbar(Page::Profiles);
    assert_bar_on_right_edge(cx, s.win, area.clone(), page_bar.clone());
    for control in [ids::PROFILES_LEAD, ids::PROFILES_NEW] {
        assert_clear_of_bar(cx, s.win, control, page_bar.clone());
    }
    assert_track_scrolls(cx, s.win, area.clone(), ids::PROFILES_LIBRARY);
    let bar = ids::grid_scrollbar(ids::PROFILES_GRID);
    assert_bar_beside(cx, s.win, ids::PROFILES_GRID, bar.clone());
    let scroll = cx.update(|cx| page.read(cx).page_scroll());
    to_end(cx, &s, &scroll);
    let (grid, card, area) = (
        bounds(cx, s.win, ids::PROFILES_GRID),
        drawn(cx, &s, &cards)[0],
        bounds(cx, s.win, area),
    );
    assert!(
        grid.size.height >= card.size.height && grid.bottom() <= area.bottom() + px(1.),
        "the grid {grid:?} shows no full {card:?} row in the page {area:?}"
    );
    let first = cards
        .iter()
        .find(|id| support::present(cx, s.win, (*id).clone()))
        .unwrap()
        .clone();
    assert_track_scrolls_virtual(cx, s.win, bar, first);
    let grid_scroll = cx.update(|cx| page.read(cx).grid_scroll());
    to_end(cx, &s, &grid_scroll);
    let last = drawn(cx, &s, &cards)
        .into_iter()
        .map(|b| b.bottom())
        .fold(px(0.), Pixels::max);
    assert!(
        last <= grid.bottom() + px(1.),
        "the last row ends at {last:?}, below the grid {grid:?}"
    );
}

// Wheels over the grid, as a player does, until the grid stops at its last row.
fn wheel_grid_to_end(cx: &mut TestAppContext, s: &support::ShellWindow, grid: &ScrollHandle) {
    for _ in 0..400 {
        if grid.offset().y <= -grid.max_offset().y {
            return;
        }
        cx.update_window(s.win, |_, w, cx| {
            w.scroll(ids::PROFILES_GRID, ScrollDelta::Lines(point(0., -3.)), cx)
        })
        .unwrap();
        support::settle(cx, s.win);
    }
    panic!("the grid never reached its end");
}

// Wheeled to the last profile, the lead is either all in view (`lead_stays`) or all scrolled
// away, never cut by the header, and the last card ends at the page's bottom padding.
fn assert_wheeled_to_end(w: f32, h: f32, lead_stays: bool, cx: &mut TestAppContext) {
    let env = support::env();
    let found = Tf2State::Found(Tf2Install::from_path(&env.game).unwrap());
    let s = support::shell_window(cx, &env, found);
    resize(cx, &s, w, h);
    let (page, cards) = profiles_page(cx, &s, 24);
    let grid = cx.update(|cx| page.read(cx).grid_scroll());
    wheel_grid_to_end(cx, &s, &grid);
    let (area, lead) = (
        bounds(cx, s.win, ids::page(Page::Profiles)),
        bounds(cx, s.win, ids::PROFILES_LEAD),
    );
    if lead_stays {
        assert!(
            lead.top() >= area.top(),
            "at {w}x{h} the lead {lead:?} is cut by {area:?}"
        );
    } else {
        assert!(
            lead.bottom() <= area.top(),
            "at {w}x{h} the lead {lead:?} still shows in {area:?}"
        );
    }
    let last = drawn(cx, &s, &cards)
        .into_iter()
        .map(|b| b.bottom())
        .fold(px(0.), Pixels::max);
    let floor = area.bottom() - CONTENT_PADDING;
    assert!(
        last <= floor + px(1.) && last >= floor - GRID_GAP - px(1.),
        "at {w}x{h} the last card ends at {last:?}, not at the padding above {:?}",
        area.bottom()
    );
}

// Too short for the lead and a card row, the whole page scrolls: the lead leaves entirely.
#[gpui_kit::test]
fn at_800x600_wheeling_the_grid_scrolls_the_lead_wholly_away(cx: &mut TestAppContext) {
    assert_wheeled_to_end(800., 600., false, cx);
}

// At a common size the page fits, so the lead stays and only the grid scrolls.
#[gpui_kit::test]
fn at_1280x800_wheeling_the_grid_keeps_the_lead_whole(cx: &mut TestAppContext) {
    assert_wheeled_to_end(1280., 800., true, cx);
}
