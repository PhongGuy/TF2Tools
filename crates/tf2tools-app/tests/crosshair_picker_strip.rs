//! The picker's strip layout: one line of search, chips and Import over one row of tiles,
//! with the host's id scope.

use crate::support;

use std::cell::Cell;
use std::rc::Rc;

use gpui_kit::prelude::*;
use gpui_kit::{AnyWindowHandle, Context, Entity, Subscription, TestAppContext, Window, div, px};
use tf2tools_app::ids::{self, IdScope};
use tf2tools_app::services::Services;
use tf2tools_app::widgets::crosshair_picker::{CrosshairPicker, ImportCrosshair};
use tf2tools_core::CrosshairRef;

const SCOPE: IdScope = IdScope::PROFILE_CROSSHAIRS;

struct Host {
    picker: Entity<CrosshairPicker>,
    _import: Subscription,
}

impl Render for Host {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().w_full().child(self.picker.clone())
    }
}

fn open(
    cx: &mut TestAppContext,
    env: &support::Env,
) -> (
    AnyWindowHandle,
    Entity<CrosshairPicker>,
    Services,
    Rc<Cell<usize>>,
) {
    support::init(cx);
    let services = support::services(env, support::no_steam());
    support::install_services(cx, services.clone());
    let imports = Rc::new(Cell::new(0));
    let count = Rc::clone(&imports);
    let (win, host) = cx
        .update(|cx| {
            gpui_kit::open_window(support::window_options(), cx, |window, cx| {
                let picker = cx.new(|cx| CrosshairPicker::strip(SCOPE, window, cx));
                cx.new(|cx| {
                    let _import = cx.subscribe(&picker, move |_, _, _: &ImportCrosshair, _| {
                        count.set(count.get() + 1)
                    });
                    Host { picker, _import }
                })
            })
        })
        .unwrap();
    let picker = cx.update(|cx| host.read(cx).picker.clone());
    support::settle(cx, win);
    (win, picker, services, imports)
}

fn tile(services: &Services, ix: usize) -> gpui_kit::ElementId {
    SCOPE.id(ids::crosshair_builtin(services.builtins.all()[ix].name()))
}

#[gpui_kit::test]
fn search_chips_and_import_share_one_line_above_the_tiles(cx: &mut TestAppContext) {
    let env = support::env();
    let (win, _, services, _) = open(cx, &env);
    let search = support::bounds_of(cx, win, SCOPE.id(ids::CROSSHAIRS_SEARCH)).unwrap();
    let all = support::bounds_of(cx, win, SCOPE.id(ids::CROSSHAIRS_GROUP_ALL)).unwrap();
    let import = support::bounds_of(cx, win, SCOPE.id(ids::CROSSHAIRS_IMPORT)).unwrap();
    for other in [all, import] {
        let gap = (support::mid_y(search) - support::mid_y(other)).abs();
        assert!(gap < px(8.), "{search:?} {other:?}");
    }
    assert!(search.right() <= all.left() && all.right() <= import.left());
    let first = support::bounds_of(cx, win, tile(&services, 0)).unwrap();
    assert!(first.top() >= search.bottom(), "{first:?} under {search:?}");
}

#[gpui_kit::test]
fn the_tiles_run_in_one_row_and_only_the_visible_ones_draw(cx: &mut TestAppContext) {
    let env = support::env();
    let (win, _, services, _) = open(cx, &env);
    let first = support::bounds_of(cx, win, tile(&services, 0)).unwrap();
    let fifth = support::bounds_of(cx, win, tile(&services, 4)).unwrap();
    assert_eq!(first.top(), fifth.top());
    assert!(fifth.left() > first.right());
    let last = services.builtins.len() - 1;
    assert!(!support::present(cx, win, tile(&services, last)));
}

#[gpui_kit::test]
fn clicking_a_tile_selects_it(cx: &mut TestAppContext) {
    let env = support::env();
    let (win, picker, services, _) = open(cx, &env);
    support::click_id(cx, win, tile(&services, 1));
    let name = services.builtins.all()[1].name().to_owned();
    assert_eq!(
        cx.update(|cx| picker.read(cx).selected().cloned()),
        Some(CrosshairRef::Builtin(name))
    );
}

#[gpui_kit::test]
fn import_asks_the_host_unless_disabled(cx: &mut TestAppContext) {
    let env = support::env();
    let (win, picker, _, imports) = open(cx, &env);
    support::click_id(cx, win, SCOPE.id(ids::CROSSHAIRS_IMPORT));
    assert_eq!(imports.get(), 1);
    cx.update(|cx| picker.update(cx, |p, cx| p.set_disabled(true, cx)));
    support::click_id(cx, win, SCOPE.id(ids::CROSSHAIRS_IMPORT));
    assert_eq!(imports.get(), 1);
}

#[gpui_kit::test]
fn the_strip_draws_none_of_the_page_ids(cx: &mut TestAppContext) {
    let env = support::env();
    let (win, _, services, _) = open(cx, &env);
    assert!(support::present(cx, win, SCOPE.id(ids::CROSSHAIRS_SEARCH)));
    assert!(!support::present(cx, win, ids::CROSSHAIRS_SEARCH));
    assert!(!support::present(cx, win, ids::CROSSHAIRS_GROUP_ALL));
    let page_tile = ids::crosshair_builtin(services.builtins.all()[0].name());
    assert!(!support::present(cx, win, page_tile));
}
