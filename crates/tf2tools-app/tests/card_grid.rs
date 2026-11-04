use std::cell::{Cell, RefCell};
use std::collections::BTreeSet;
use std::ops::Range;
use std::rc::Rc;

use gpui_kit::TestSupportExt as _;
use gpui_kit::prelude::*;
use gpui_kit::{
    AnyWindowHandle, Context, ElementId, Entity, ScrollStrategy, TestAppContext, Window,
    WindowOptions, div, px, size,
};
use tf2tools_app::shell::{CONTENT_PADDING, SIDEBAR_WIDTH};
use tf2tools_app::widgets::card_grid::{
    CardGrid, GRID_GAP, SHELL_CHROME_WIDTH, columns_for, columns_for_store,
};

mod support;

struct Host {
    grid: CardGrid,
    built: Rc<RefCell<BTreeSet<usize>>>,
}

impl Render for Host {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let built = self.built.clone();
        div()
            .size_full()
            .child(self.grid.render(200, px(120.), move |ix, _, _| {
                built.borrow_mut().insert(ix);
                div()
                    .id(ElementId::Name(format!("card-{ix}").into()))
                    .test_support()
                    .size_full()
                    .into_any_element()
            }))
    }
}

fn open(cx: &mut TestAppContext, width: f32) -> (AnyWindowHandle, Entity<Host>) {
    support::init(cx);
    let (win, host) = cx
        .update(|cx| {
            gpui_kit::open_window(WindowOptions::default(), cx, |_, cx| {
                cx.new(|_| Host {
                    grid: CardGrid::new("test-grid"),
                    built: Rc::default(),
                })
            })
        })
        .unwrap();
    cx.simulate_window_resize(win, size(px(width), px(900.)));
    support::settle(cx, win);
    (win, host)
}

#[test]
fn columns_follow_v5_breakpoints_less_the_shell_chrome() {
    // v5's 640 / 1024 px viewport breakpoints, less a 256 px sidebar and 2 x 48 px padding.
    assert_eq!(SHELL_CHROME_WIDTH, SIDEBAR_WIDTH + CONTENT_PADDING * 2.);
    assert_eq!(columns_for(px(200.)), 1);
    assert_eq!(columns_for(px(287.)), 1);
    assert_eq!(columns_for(px(288.)), 2);
    assert_eq!(columns_for(px(671.)), 2);
    assert_eq!(columns_for(px(672.)), 3);
    // v5's 1200 px window leaves the grid 848 px, where v5 shows three columns.
    assert_eq!(columns_for(px(848.)), 3);
    assert_eq!(columns_for(px(2000.)), 3);
}

#[gpui_kit::test]
fn a_wide_grid_has_three_columns_and_builds_only_visible_rows(cx: &mut TestAppContext) {
    let (win, host) = open(cx, 1200.);
    assert_eq!(cx.update(|cx| host.read(cx).grid.columns()), 3);
    assert!(support::present(cx, win, "card-0"));
    assert!(support::present(cx, win, "card-2"));
    assert!(!support::present(cx, win, "card-199"));
    // 900 px shows about seven 136 px rows; nothing near the end is ever built.
    let built = cx.update(|cx| host.read(cx).built.borrow().clone());
    assert!(built.iter().max().copied().unwrap_or(0) < 40, "{built:?}");
}

#[gpui_kit::test]
fn cards_in_a_row_share_the_width_and_gap(cx: &mut TestAppContext) {
    let (win, _host) = open(cx, 1200.);
    let a = support::bounds_of(cx, win, "card-0").unwrap();
    let b = support::bounds_of(cx, win, "card-1").unwrap();
    let below = support::bounds_of(cx, win, "card-3").unwrap();
    assert_eq!(a.size.height, px(120.));
    assert_eq!(a.size.width, b.size.width);
    assert_eq!(b.origin.x - (a.origin.x + a.size.width), GRID_GAP);
    assert_eq!(below.origin.y - (a.origin.y + a.size.height), GRID_GAP);
    assert_eq!(below.origin.x, a.origin.x);
}

#[gpui_kit::test]
fn scrolling_to_the_end_builds_the_last_row_without_stretching_it(cx: &mut TestAppContext) {
    let (win, host) = open(cx, 1200.);
    let first = support::bounds_of(cx, win, "card-0").unwrap();
    // 200 cards in 3 columns are 67 rows; the last holds cards 198 and 199.
    cx.update(|cx| {
        host.read(cx)
            .grid
            .scroll_handle()
            .scroll_to_item(66, ScrollStrategy::Bottom)
    });
    assert!(support::present(cx, win, "card-199"));
    assert!(support::present(cx, win, "card-198"));
    assert!(!support::present(cx, win, "card-0"));
    let last = support::bounds_of(cx, win, "card-198").unwrap();
    assert_eq!(last.size.width, first.size.width);
}

#[gpui_kit::test]
fn the_column_count_follows_the_window_width(cx: &mut TestAppContext) {
    let (win, host) = open(cx, 1200.);
    cx.simulate_window_resize(win, size(px(500.), px(900.)));
    support::settle(cx, win);
    assert_eq!(cx.update(|cx| host.read(cx).grid.columns()), 2);
    cx.simulate_window_resize(win, size(px(250.), px(900.)));
    support::settle(cx, win);
    assert_eq!(cx.update(|cx| host.read(cx).grid.columns()), 1);
}

#[gpui_kit::test]
fn the_frame_that_crosses_a_breakpoint_keeps_cards_in_their_rows(cx: &mut TestAppContext) {
    let (win, host) = open(cx, 1200.);
    cx.update(|cx| {
        host.read(cx)
            .grid
            .scroll_handle()
            .scroll_to_item(66, ScrollStrategy::Bottom)
    });
    assert!(support::present(cx, win, "card-199"));
    // The resize draws one frame whose row count was taken at three columns; its rows must
    // still hold three cards each, or the last row shows cards from the middle of the list.
    cx.update(|cx| host.read(cx).built.borrow_mut().clear());
    cx.simulate_window_resize(win, size(px(500.), px(900.)));
    let built = cx.update(|cx| host.read(cx).built.borrow().clone());
    assert!(built.contains(&199), "{built:?}");
}

#[test]
fn store_columns_follow_v5_store_breakpoints_less_the_shell_chrome() {
    assert_eq!(columns_for_store(px(287.), 4), 1);
    assert_eq!(columns_for_store(px(288.), 4), 2);
    assert_eq!(columns_for_store(px(671.), 4), 2);
    assert_eq!(columns_for_store(px(672.), 4), 3);
    assert_eq!(columns_for_store(px(927.), 4), 3);
    assert_eq!(columns_for_store(px(928.), 4), 4);
    assert_eq!(columns_for_store(px(2000.), 3), 3);
}

struct Hooked {
    grid: CardGrid,
    count: usize,
    seen: Rc<RefCell<Vec<Range<usize>>>>,
    near: Rc<Cell<usize>>,
}

impl Render for Hooked {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().size_full().child(self.grid.render_sized(
            self.count,
            |width| width * (9. / 16.) + px(64.),
            |ix, _, _| {
                div()
                    .id(ElementId::Name(format!("card-{ix}").into()))
                    .test_support()
                    .size_full()
                    .into_any_element()
            },
        ))
    }
}

fn open_hooked(cx: &mut TestAppContext) -> (AnyWindowHandle, Entity<Hooked>) {
    support::init(cx);
    let (win, host) = cx
        .update(|cx| {
            gpui_kit::open_window(WindowOptions::default(), cx, |_, cx| {
                cx.new(|_| {
                    let seen: Rc<RefCell<Vec<Range<usize>>>> = Rc::default();
                    let near: Rc<Cell<usize>> = Rc::default();
                    let mut grid = CardGrid::store("hooked-grid", 4);
                    let log = seen.clone();
                    grid.on_visible(move |range, _| log.borrow_mut().push(range));
                    let hits = near.clone();
                    grid.on_near_end(move |_| hits.set(hits.get() + 1));
                    Hooked {
                        grid,
                        count: 200,
                        seen,
                        near,
                    }
                })
            })
        })
        .unwrap();
    cx.simulate_window_resize(win, size(px(1200.), px(900.)));
    support::settle(cx, win);
    (win, host)
}

#[gpui_kit::test]
fn a_sized_store_grid_has_four_columns_and_sixteen_by_nine_cards(cx: &mut TestAppContext) {
    let (win, host) = open_hooked(cx);
    assert_eq!(cx.update(|cx| host.read(cx).grid.columns()), 4);
    let card = support::bounds_of(cx, win, "card-0").unwrap();
    // 1200 px over 4 columns and 3 gaps: 288 px cells, 162 px of image plus 64.
    assert_eq!(card.size.width, px(288.));
    assert_eq!(card.size.height, px(226.));
    assert_eq!(
        cx.update(|cx| host.read(cx).grid.row_height()),
        Some(px(242.))
    );
}

#[gpui_kit::test]
fn the_visible_hook_reports_card_indices_and_never_the_measure_pass(cx: &mut TestAppContext) {
    let (win, host) = open_hooked(cx);
    let first = cx
        .update(|cx| host.read(cx).seen.borrow().last().cloned())
        .unwrap();
    assert_eq!(first.start, 0);
    assert!(first.end >= 12 && first.end.is_multiple_of(4), "{first:?}");
    cx.update(|cx| {
        host.read(cx)
            .grid
            .scroll_handle()
            .scroll_to_item(20, ScrollStrategy::Top)
    });
    support::settle(cx, win);
    let seen = cx.update(|cx| host.read(cx).seen.borrow().clone());
    let last = seen.last().unwrap();
    assert_eq!(last.start, 80, "{seen:?}");
    // uniform_list measures with 0..1 each frame; the hook must only ever see whole rows.
    assert!(
        seen.iter()
            .all(|r| r.start.is_multiple_of(4) && r.len() > 1),
        "{seen:?}"
    );
}

#[gpui_kit::test]
fn near_end_fires_once_per_count_within_two_rows_of_the_end(cx: &mut TestAppContext) {
    let (win, host) = open_hooked(cx);
    assert_eq!(cx.update(|cx| host.read(cx).near.get()), 0);
    // 200 cards in 4 columns are 50 rows.
    cx.update(|cx| {
        host.read(cx)
            .grid
            .scroll_handle()
            .scroll_to_item(49, ScrollStrategy::Bottom)
    });
    support::settle(cx, win);
    support::settle(cx, win);
    assert_eq!(cx.update(|cx| host.read(cx).near.get()), 1);
    cx.update(|cx| {
        host.update(cx, |h, cx| {
            h.count = 400;
            cx.notify();
        })
    });
    support::settle(cx, win);
    assert_eq!(cx.update(|cx| host.read(cx).near.get()), 1);
    cx.update(|cx| {
        host.read(cx)
            .grid
            .scroll_handle()
            .scroll_to_item(99, ScrollStrategy::Bottom)
    });
    support::settle(cx, win);
    assert_eq!(cx.update(|cx| host.read(cx).near.get()), 2);
}

#[gpui_kit::test]
fn a_rearmed_grid_fires_near_end_again_after_a_reset_to_the_same_count(cx: &mut TestAppContext) {
    let (win, host) = open_hooked(cx);
    cx.update(|cx| {
        host.update(cx, |h, cx| {
            h.count = 60;
            cx.notify();
        })
    });
    support::settle(cx, win);
    assert_eq!(cx.update(|cx| host.read(cx).near.get()), 0);
    // 60 cards in 4 columns are 15 rows.
    let to_end = |cx: &mut TestAppContext| {
        cx.update(|cx| {
            host.read(cx)
                .grid
                .scroll_handle()
                .scroll_to_item(14, ScrollStrategy::Bottom)
        });
        support::settle(cx, win);
        support::settle(cx, win);
    };
    to_end(cx);
    assert_eq!(cx.update(|cx| host.read(cx).near.get()), 1);
    // A store query reset: empty, rearmed, back at the top, then 60 again.
    cx.update(|cx| {
        host.update(cx, |h, cx| {
            h.count = 0;
            h.grid.rearm();
            h.grid
                .scroll_handle()
                .scroll_to_item(0, ScrollStrategy::Top);
            cx.notify();
        })
    });
    support::settle(cx, win);
    cx.update(|cx| {
        host.update(cx, |h, cx| {
            h.count = 60;
            cx.notify();
        })
    });
    support::settle(cx, win);
    assert_eq!(cx.update(|cx| host.read(cx).near.get()), 1);
    to_end(cx);
    assert_eq!(cx.update(|cx| host.read(cx).near.get()), 2);
}

#[gpui_kit::test]
fn a_rearmed_grid_reports_the_same_visible_range_again(cx: &mut TestAppContext) {
    let (win, host) = open_hooked(cx);
    let before = cx.update(|cx| host.read(cx).seen.borrow().clone());
    // A reset whose first page lands before a frame draws: same range, same count.
    cx.update(|cx| {
        host.update(cx, |h, cx| {
            h.grid.rearm();
            cx.notify();
        })
    });
    support::settle(cx, win);
    let after = cx.update(|cx| host.read(cx).seen.borrow().clone());
    assert_eq!(after.len(), before.len() + 1, "{after:?}");
    assert_eq!(after.last(), before.last());
}

#[test]
fn the_row_height_is_unknown_before_the_first_paint() {
    assert_eq!(CardGrid::new("unpainted").row_height(), None);
    assert_eq!(CardGrid::store("unpainted-store", 4).row_height(), None);
}

#[gpui_kit::test]
fn a_visible_hook_set_after_the_first_frame_hears_the_current_range(cx: &mut TestAppContext) {
    let (win, host) = open_hooked(cx);
    let current = cx
        .update(|cx| host.read(cx).seen.borrow().last().cloned())
        .unwrap();
    let late: Rc<RefCell<Vec<Range<usize>>>> = Rc::default();
    let log = late.clone();
    cx.update(|cx| {
        host.update(cx, |h, cx| {
            h.grid
                .on_visible(move |range, _| log.borrow_mut().push(range));
            cx.notify();
        })
    });
    support::settle(cx, win);
    assert_eq!(*late.borrow(), vec![current]);
}

#[gpui_kit::test]
fn a_near_end_hook_set_after_it_fired_hears_the_current_count(cx: &mut TestAppContext) {
    let (win, host) = open_hooked(cx);
    // 200 cards in 4 columns are 50 rows.
    cx.update(|cx| {
        host.read(cx)
            .grid
            .scroll_handle()
            .scroll_to_item(49, ScrollStrategy::Bottom)
    });
    support::settle(cx, win);
    support::settle(cx, win);
    assert_eq!(cx.update(|cx| host.read(cx).near.get()), 1);
    let late: Rc<Cell<usize>> = Rc::default();
    let hits = late.clone();
    cx.update(|cx| {
        host.update(cx, |h, cx| {
            h.grid.on_near_end(move |_| hits.set(hits.get() + 1));
            cx.notify();
        })
    });
    support::settle(cx, win);
    assert_eq!(late.get(), 1);
}
