//! The game situation menu and Show it, headless.

use crate::support;

use gpui_kit::component::slider::{SliderEvent, SliderValue};
use gpui_kit::{AppContext as _, Entity, TestAppContext};
use support::maker_page::{VALUE, click_in_panel, open_maker};
use tf2tools_app::ids;
use tf2tools_app::pages::hud_maker::{HudMakerPage, SituationSlider};
use tf2tools_core::{GameSituation, PartKey, Scene, SituationSwitch, TfClass};

fn situation(cx: &mut TestAppContext, page: &Entity<HudMakerPage>) -> GameSituation {
    cx.update(|cx| page.read(cx).editor().unwrap().situation().clone())
}

fn in_menu(cx: &mut TestAppContext, s: &support::ShellWindow, id: impl Into<gpui_kit::ElementId>) {
    let id = id.into();
    if !support::present(cx, s.win, id.clone()) {
        support::click_id(cx, s.win, ids::HUD_MAKER_SITUATION);
    }
    support::click_id(cx, s.win, id);
}

#[gpui_kit::test]
fn the_menu_sets_class_and_switches_and_resets(cx: &mut TestAppContext) {
    let (_env, s, page) = open_maker(cx);
    assert_eq!(
        support::label_of(cx, s.win, ids::HUD_MAKER_SITUATION).as_deref(),
        Some("Soldier, 150 health")
    );
    in_menu(cx, &s, ids::hud_maker_situation_class(TfClass::Medic));
    assert_eq!(situation(cx, &page).class(), TfClass::Medic);
    assert_eq!(
        support::label_of(cx, s.win, ids::HUD_MAKER_SITUATION).as_deref(),
        Some("Medic, 150 health")
    );
    in_menu(
        cx,
        &s,
        ids::hud_maker_situation_switch(SituationSwitch::Aiming),
    );
    assert!(situation(cx, &page).is_on(SituationSwitch::Aiming));
    in_menu(cx, &s, ids::HUD_MAKER_SITUATION_RESET);
    assert_eq!(situation(cx, &page), GameSituation::default());
}

#[gpui_kit::test]
fn the_health_slider_changes_the_number_on_the_canvas(cx: &mut TestAppContext) {
    let (_env, s, page) = open_maker(cx);
    support::click_id(cx, s.win, ids::HUD_MAKER_SITUATION);
    let slider = cx.update(|cx| page.read(cx).situation_slider(SituationSlider::Health));
    cx.update_window(s.win, |_, window, cx| {
        slider.update(cx, |st, cx| {
            st.set_value(60., window, cx);
            cx.emit(SliderEvent::Release(SliderValue::Single(60.)));
        })
    })
    .unwrap();
    support::settle(cx, s.win);
    assert_eq!(situation(cx, &page).health(), 60);
    assert_eq!(
        support::label_of(cx, s.win, ids::hud_maker_text(&PartKey::new(VALUE))).as_deref(),
        Some("60")
    );
}

#[gpui_kit::test]
fn the_menu_shows_only_on_the_hud_scene(cx: &mut TestAppContext) {
    let (_env, s, _page) = open_maker(cx);
    assert!(support::present(cx, s.win, ids::HUD_MAKER_SITUATION));
    support::click_scene(cx, s.win, Scene::Scoreboard);
    assert!(!support::present(cx, s.win, ids::HUD_MAKER_SITUATION));
    support::click_scene(cx, s.win, Scene::Hud);
    assert!(support::present(cx, s.win, ids::HUD_MAKER_SITUATION));
}

#[gpui_kit::test]
fn show_it_shows_a_part_the_situation_hides(cx: &mut TestAppContext) {
    let (_env, s, page) = open_maker(cx);
    cx.update(|cx| {
        page.update(cx, |p, cx| {
            p.editor_mut()
                .unwrap()
                .select(Some(PartKey::new("HudMedicCharge")));
            cx.notify();
        })
    });
    support::settle(cx, s.win);
    click_in_panel(cx, &s, ids::HUD_MAKER_PANEL_SHOW_IT.into());
    assert_eq!(situation(cx, &page).class(), TfClass::Medic);
}

#[gpui_kit::test]
fn show_it_is_offered_only_while_the_part_is_hidden(cx: &mut TestAppContext) {
    let (_env, s, page) = open_maker(cx);
    cx.update(|cx| {
        page.update(cx, |p, cx| {
            p.editor_mut()
                .unwrap()
                .select(Some(PartKey::new("HudMedicCharge")));
            cx.notify();
        })
    });
    support::settle(cx, s.win);
    // A Soldier never sees the charge meter, so Show it is there to bring it up.
    assert!(support::present(cx, s.win, ids::HUD_MAKER_PANEL_SHOW_IT));
    click_in_panel(cx, &s, ids::HUD_MAKER_PANEL_SHOW_IT.into());
    assert_eq!(situation(cx, &page).class(), TfClass::Medic);
    assert!(
        !support::present(cx, s.win, ids::HUD_MAKER_PANEL_SHOW_IT),
        "Show it stays offered while the meter already shows"
    );
    // Back to a class that hides it, Show it returns.
    in_menu(cx, &s, ids::hud_maker_situation_class(TfClass::Scout));
    assert!(support::present(cx, s.win, ids::HUD_MAKER_PANEL_SHOW_IT));
}

/// Opens the menu behind `trigger` and checks that its item `first` lies below the button.
fn opens_below(
    cx: &mut TestAppContext,
    s: &support::ShellWindow,
    trigger: &'static str,
    first: gpui_kit::ElementId,
) {
    support::click_id(cx, s.win, trigger);
    let button = support::bounds_of(cx, s.win, trigger).unwrap();
    let item = support::bounds_of(cx, s.win, first).unwrap();
    assert!(
        item.top() >= button.bottom(),
        "{trigger}: {item:?} not below {button:?}"
    );
    support::press(cx, s.win, "escape");
    support::settle(cx, s.win);
}

#[gpui_kit::test]
fn the_strip_menus_open_below_their_buttons(cx: &mut TestAppContext) {
    let (_env, s, _page) = open_maker(cx);
    let first = ids::hud_maker_situation_class(TfClass::Scout);
    opens_below(cx, &s, ids::HUD_MAKER_SITUATION, first);
    // The scene tabs fold into a menu in a narrow window.
    cx.simulate_window_resize(
        s.win,
        gpui_kit::size(gpui_kit::px(900.), gpui_kit::px(800.)),
    );
    support::settle(cx, s.win);
    let hud = ids::hud_maker_scene(Scene::Hud);
    opens_below(cx, &s, ids::HUD_MAKER_SCENE_MENU, hud);
}

#[gpui_kit::test]
fn switches_stay_open_so_several_combine_in_one_trip(cx: &mut TestAppContext) {
    let (_env, s, page) = open_maker(cx);
    support::click_id(cx, s.win, ids::HUD_MAKER_SITUATION);
    for sw in [SituationSwitch::Typing, SituationSwitch::Aiming] {
        support::click_id(cx, s.win, ids::hud_maker_situation_switch(sw));
        assert!(situation(cx, &page).is_on(sw), "{sw:?}");
    }
    support::click_id(cx, s.win, ids::HUD_MAKER_MINMODE);
    assert!(support::present(
        cx,
        s.win,
        ids::hud_maker_situation_switch(SituationSwitch::Koth)
    ));
}

#[gpui_kit::test]
fn a_slider_value_sits_clear_of_the_handle_at_the_top(cx: &mut TestAppContext) {
    let (_env, s, page) = open_maker(cx);
    support::click_id(cx, s.win, ids::HUD_MAKER_SITUATION);
    for (which, slug, top) in [
        (SituationSlider::Health, "health", 300.),
        (SituationSlider::Ammo, "ammo", 100.),
    ] {
        let slider = cx.update(|cx| page.read(cx).situation_slider(which));
        cx.update_window(s.win, |_, window, cx| {
            slider.update(cx, |st, cx| {
                st.set_value(top, window, cx);
                cx.emit(SliderEvent::Release(SliderValue::Single(top)));
            })
        })
        .unwrap();
        support::settle(cx, s.win);
        // At its top the handle is centred on the track's end: 8 px past it, plus a 3 px ring
        // while hovered or dragged.
        let track = support::bounds_of(cx, s.win, ids::hud_maker_situation_slider(slug)).unwrap();
        let value = support::bounds_of(cx, s.win, ids::hud_maker_situation_value(slug)).unwrap();
        let reach = track.right() + gpui_kit::px(11.);
        assert!(
            value.left() >= reach,
            "{slug}: {value:?} under the handle ending {reach:?}"
        );
    }
}
