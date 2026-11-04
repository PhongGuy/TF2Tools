//! The game situation menu and Show it, headless.

mod support;

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
