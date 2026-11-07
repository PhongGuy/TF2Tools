use crate::support;

use std::rc::Rc;

use gpui_kit::prelude::*;
use gpui_kit::{AnyWindowHandle, Context, Entity, TestAppContext, Window, div};
use tf2tools_app::ids;
use tf2tools_app::services::Services;
use tf2tools_app::widgets::crosshair_picker::CrosshairPicker;
use tf2tools_core::CrosshairRef;

struct Host {
    picker: Entity<CrosshairPicker>,
}

impl Render for Host {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().size_full().child(self.picker.clone())
    }
}

fn open(
    cx: &mut TestAppContext,
    env: &support::Env,
    compact: bool,
) -> (AnyWindowHandle, Entity<CrosshairPicker>, Services) {
    support::init(cx);
    let services = support::services(env, support::no_steam());
    support::install_services(cx, services.clone());
    let (win, host) = cx
        .update(|cx| {
            gpui_kit::open_window(support::window_options(), cx, |window, cx| {
                let picker = cx.new(|cx| CrosshairPicker::new(compact, window, cx));
                cx.new(|_| Host { picker })
            })
        })
        .unwrap();
    let picker = cx.update(|cx| host.read(cx).picker.clone());
    support::settle(cx, win);
    (win, picker, services)
}

#[gpui_kit::test]
fn it_opens_on_the_builtin_group_with_no_selection(cx: &mut TestAppContext) {
    let env = support::env();
    let (win, picker, services) = open(cx, &env, false);
    assert_eq!(
        support::label_of(cx, win, ids::CROSSHAIRS_GROUP_BUILTIN),
        Some(format!("Built-in ({})", services.builtins.len()))
    );
    assert!(!support::present(cx, win, ids::CROSSHAIRS_GROUP_LIBRARY));
    assert!(!support::present(cx, win, ids::CROSSHAIRS_SWATCH));
    assert_eq!(cx.update(|cx| picker.read(cx).selected().cloned()), None);
}

#[gpui_kit::test]
fn set_library_adds_the_library_group(cx: &mut TestAppContext) {
    let env = support::env();
    let (win, picker, services) = open(cx, &env, false);
    let zebra = support::seed_crosshair(&services, &env, "Zebra Dot");
    let library: Rc<[_]> = services.library.list_crosshairs().unwrap().into();
    cx.update(|cx| picker.update(cx, |p, cx| p.set_library(library, cx)));
    let input = cx.update(|cx| picker.read(cx).search_input().clone());
    cx.update_window(win, |_, window, cx| {
        input.update(cx, |st, cx| st.set_value("zebra", window, cx))
    })
    .unwrap();
    assert_eq!(
        support::label_of(cx, win, ids::CROSSHAIRS_GROUP_LIBRARY).as_deref(),
        Some("My Crosshairs (1)")
    );
    assert_eq!(
        support::label_of(cx, win, ids::crosshair_library(zebra.folder_name())).as_deref(),
        Some("Zebra Dot")
    );
    let zebra_ref = CrosshairRef::Library(zebra.id().clone());
    assert_eq!(
        cx.update(|cx| picker.read(cx).name_of(&zebra_ref))
            .as_deref(),
        Some("Zebra Dot")
    );
}

#[gpui_kit::test]
fn clicking_an_option_selects_it_and_select_none_clears_it(cx: &mut TestAppContext) {
    let env = support::env();
    let (win, picker, services) = open(cx, &env, false);
    let first = services.builtins.all()[0].clone();
    support::click_id(cx, win, ids::crosshair_builtin(first.name()));
    assert_eq!(
        cx.update(|cx| picker.read(cx).selected().cloned()),
        Some(CrosshairRef::Builtin(first.name().to_owned()))
    );
    assert_eq!(
        support::label_of(cx, win, ids::CROSSHAIRS_SWATCH).as_deref(),
        Some(first.display_name())
    );
    cx.update(|cx| picker.update(cx, |p, cx| p.select(None, cx)));
    assert!(!support::present(cx, win, ids::CROSSHAIRS_SWATCH));
}

#[gpui_kit::test]
fn the_list_scrolls(cx: &mut TestAppContext) {
    let env = support::env();
    let (win, picker, _services) = open(cx, &env, false);
    support::settle(cx, win);
    assert!(cx.update(|cx| picker.read(cx).scroll()).max_offset().y > gpui_kit::px(0.));
    let list = support::bounds_of(cx, win, ids::CROSSHAIRS_LIST).unwrap();
    assert_eq!(list.size.height, gpui_kit::px(160.));
}

#[gpui_kit::test]
fn compact_is_v5s_dialog_size(cx: &mut TestAppContext) {
    let env = support::env();
    let (win, picker, services) = open(cx, &env, true);
    let list = support::bounds_of(cx, win, ids::CROSSHAIRS_LIST).unwrap();
    assert_eq!(list.size.height, gpui_kit::px(128.));
    let first = services.builtins.all()[0].name().to_owned();
    cx.update(|cx| picker.update(cx, |p, cx| p.select(Some(CrosshairRef::Builtin(first)), cx)));
    let swatch = support::bounds_of(cx, win, ids::CROSSHAIRS_SWATCH).unwrap();
    assert_eq!(swatch.size.height, gpui_kit::px(28.));
}

#[gpui_kit::test]
fn the_list_shows_a_bar_clear_of_its_options(cx: &mut TestAppContext) {
    use support::scroll::{
        assert_bar_on_right_edge, assert_clear_of_bar, assert_track_scrolls_virtual,
    };
    let env = support::env();
    let (win, _picker, services) = open(cx, &env, false);
    let first = ids::crosshair_builtin(services.builtins.all()[0].name());
    let bar = ids::CROSSHAIRS_LIST_SCROLLBAR;
    assert_bar_on_right_edge(cx, win, ids::CROSSHAIRS_LIST, bar);
    assert_clear_of_bar(cx, win, first.clone(), bar);
    assert_track_scrolls_virtual(cx, win, bar, first);
}
