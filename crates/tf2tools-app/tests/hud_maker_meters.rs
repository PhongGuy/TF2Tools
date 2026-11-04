//! The meter row of the situation menu, what it draws, and undo across meters.

mod support;

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{AppContext as _, Entity, TestAppContext};
use support::maker_page::open_maker;
use tf2tools_app::ids;
use tf2tools_app::pages::hud_maker::editor::HudEditor;
use tf2tools_app::pages::hud_maker::{Field, HudMakerPage};
use tf2tools_core::{GameSituation, HudModel, MeterKind, PartKey, Platform, SituationSwitch};

const COUNT: &str = "HudItemEffectMeter/ItemEffectMeterCount";

fn meter(cx: &mut TestAppContext, page: &Entity<HudMakerPage>) -> MeterKind {
    cx.update(|cx| page.read(cx).meter())
}

fn situation(cx: &mut TestAppContext, page: &Entity<HudMakerPage>) -> GameSituation {
    cx.update(|cx| page.read(cx).editor().unwrap().situation().clone())
}

// The menu and its meter list exist only while open.
fn pick(cx: &mut TestAppContext, s: &support::ShellWindow, kind: MeterKind) {
    if !support::present(cx, s.win, ids::HUD_MAKER_SITUATION_METER) {
        support::click_id(cx, s.win, ids::HUD_MAKER_SITUATION);
    }
    if !support::present(cx, s.win, ids::hud_maker_situation_meter(kind)) {
        support::click_id(cx, s.win, ids::HUD_MAKER_SITUATION_METER);
    }
    support::click_id(cx, s.win, ids::hud_maker_situation_meter(kind));
}

// "Back to the usual moment": the default situation and the plain meter.
fn reset(cx: &mut TestAppContext, s: &support::ShellWindow) {
    if !support::present(cx, s.win, ids::HUD_MAKER_SITUATION_RESET) {
        support::click_id(cx, s.win, ids::HUD_MAKER_SITUATION);
    }
    support::click_id(cx, s.win, ids::HUD_MAKER_SITUATION_RESET);
}

fn shown(cx: &mut TestAppContext, s: &support::ShellWindow, key: &str) -> bool {
    support::present(cx, s.win, ids::hud_maker_part(&PartKey::new(key)))
}

#[gpui_kit::test]
fn picking_a_meter_shows_it_and_reset_returns_to_plain(cx: &mut TestAppContext) {
    let (_env, s, page) = open_maker(cx);
    assert_eq!(meter(cx, &page), MeterKind::Plain);
    support::click_id(cx, s.win, ids::HUD_MAKER_SITUATION);
    assert_eq!(
        support::label_of(cx, s.win, ids::HUD_MAKER_SITUATION_METER).as_deref(),
        Some("Meter: Sandman, Jarate, banners and most items")
    );
    pick(cx, &s, MeterKind::Sword);
    assert_eq!(meter(cx, &page), MeterKind::Sword);
    assert!(
        situation(cx, &page).is_on(SituationSwitch::Meter),
        "picking shows the meter"
    );
    assert!(shown(cx, &s, COUNT));
    if !support::present(cx, s.win, ids::HUD_MAKER_SITUATION_RESET) {
        support::click_id(cx, s.win, ids::HUD_MAKER_SITUATION);
    }
    support::click_id(cx, s.win, ids::HUD_MAKER_SITUATION_RESET);
    assert_eq!(meter(cx, &page), MeterKind::Plain);
    assert!(
        !situation(cx, &page).is_on(SituationSwitch::Meter),
        "reset leaves the switch off"
    );
    assert!(!shown(cx, &s, COUNT));
}

#[test]
fn a_switch_reload_keeps_the_meter() {
    let load = || HudModel::load(&support::hud_stock::stock_files(), Platform::Linux).unwrap();
    let mut e = HudEditor::new(load(), Vec::new());
    e.set_meter(MeterKind::Sword, true);
    assert!(e.situation().is_on(SituationSwitch::Meter));
    let choices = e.switch_choices();
    assert!(e.rebase(load(), choices));
    assert_eq!(e.meter(), MeterKind::Sword);
    assert!(e.model().part(&PartKey::new(COUNT)).is_some());
}

fn commit(
    cx: &mut TestAppContext,
    s: &support::ShellWindow,
    page: &Entity<HudMakerPage>,
    field: Field,
    text: &str,
) {
    use gpui_kit::component::input::InputEvent;
    let input = cx.update(|cx| page.read(cx).field(field).clone());
    cx.update_window(s.win, |_, window, cx| {
        input.update(cx, |st, cx| {
            st.set_value(text, window, cx);
            cx.emit(InputEvent::PressEnter {
                secondary: false,
                shift: false,
            });
        })
    })
    .unwrap();
    support::settle(cx, s.win);
}

// X takes screen pixels, so the edit moves the selected count left by 27 of them (12 file
// units on the 1080p canvas) from wherever the field shows it.
fn nudge_x(cx: &mut TestAppContext, s: &support::ShellWindow, page: &Entity<HudMakerPage>) {
    let shown = cx.update(|cx| page.read(cx).field(Field::X).read(cx).value().to_string());
    let at: f64 = shown.trim().parse().expect("a number in the X field");
    commit(cx, s, page, Field::X, &format!("{}", (at - 27.0).round()));
}

fn count_x(cx: &mut TestAppContext, page: &Entity<HudMakerPage>) -> Option<String> {
    cx.update(|cx| {
        let e = page.read(cx).editor()?;
        let m = e.model().with_meter(MeterKind::Sword);
        Some(
            m.part(&PartKey::new(COUNT))?
                .value("xpos")?
                .text()
                .to_owned(),
        )
    })
}

#[gpui_kit::test]
fn undo_of_a_meter_edit_shows_that_meter_again(cx: &mut TestAppContext) {
    let (_env, s, page) = open_maker(cx);
    pick(cx, &s, MeterKind::Sword);
    if !support::present(cx, s.win, ids::HUD_MAKER_PARTS) {
        support::click_id(cx, s.win, ids::HUD_MAKER_PARTS_PANEL_COLLAPSE);
    }
    support::click_part_row(cx, s.win, &PartKey::new(COUNT));
    nudge_x(cx, &s, &page);
    let moved = count_x(cx, &page);
    assert!(
        moved.is_some() && moved.as_deref() != Some("73"),
        "{moved:?}"
    );
    pick(cx, &s, MeterKind::Plain);
    assert_eq!(meter(cx, &page), MeterKind::Plain);
    // The usual moment hides the meter, so undo must turn its switch back on.
    reset(cx, &s);
    assert!(!situation(cx, &page).is_on(SituationSwitch::Meter));
    support::click_id(cx, s.win, ids::HUD_MAKER_UNDO);
    assert_eq!(
        meter(cx, &page),
        MeterKind::Sword,
        "undo shows the meter it changed"
    );
    assert_eq!(count_x(cx, &page).as_deref(), Some("73"));
    assert!(
        situation(cx, &page).is_on(SituationSwitch::Meter),
        "undo shows the meter part"
    );
    support::click_id(cx, s.win, ids::HUD_MAKER_REDO);
    assert_eq!(count_x(cx, &page), moved);
}

#[gpui_kit::test]
fn redo_from_another_meter_shows_the_meter_it_changed(cx: &mut TestAppContext) {
    let (_env, s, page) = open_maker(cx);
    pick(cx, &s, MeterKind::Sword);
    if !support::present(cx, s.win, ids::HUD_MAKER_PARTS) {
        support::click_id(cx, s.win, ids::HUD_MAKER_PARTS_PANEL_COLLAPSE);
    }
    support::click_part_row(cx, s.win, &PartKey::new(COUNT));
    nudge_x(cx, &s, &page);
    let moved = count_x(cx, &page);
    assert!(
        moved.is_some() && moved.as_deref() != Some("73"),
        "{moved:?}"
    );
    support::click_id(cx, s.win, ids::HUD_MAKER_UNDO);
    pick(cx, &s, MeterKind::Plain);
    assert_eq!(meter(cx, &page), MeterKind::Plain);
    support::click_id(cx, s.win, ids::HUD_MAKER_REDO);
    assert_eq!(
        meter(cx, &page),
        MeterKind::Sword,
        "redo shows the meter it changed"
    );
    assert_eq!(count_x(cx, &page), moved);
    pick(cx, &s, MeterKind::Plain);
    support::click_id(cx, s.win, ids::HUD_MAKER_UNDO);
    assert_eq!(
        meter(cx, &page),
        MeterKind::Sword,
        "a redone step still knows its meter"
    );
    assert_eq!(count_x(cx, &page).as_deref(), Some("73"));
}

#[gpui_kit::test]
fn an_outside_click_closes_the_meter_list(cx: &mut TestAppContext) {
    let (_env, s, _page) = open_maker(cx);
    support::click_id(cx, s.win, ids::HUD_MAKER_SITUATION);
    support::click_id(cx, s.win, ids::HUD_MAKER_SITUATION_METER);
    assert!(support::present(cx, s.win, ids::HUD_MAKER_SITUATION_METERS));
    support::click_id(cx, s.win, ids::HUD_MAKER_UNDO);
    assert!(
        !support::present(cx, s.win, ids::HUD_MAKER_SITUATION_METER),
        "the menu closed"
    );
    support::click_id(cx, s.win, ids::HUD_MAKER_SITUATION);
    assert!(
        support::present(cx, s.win, ids::HUD_MAKER_SITUATION_RESET),
        "the whole menu shows"
    );
    assert!(!support::present(
        cx,
        s.win,
        ids::HUD_MAKER_SITUATION_METERS
    ));
}

#[gpui_kit::test]
fn the_meter_list_fits_a_small_window(cx: &mut TestAppContext) {
    let (_env, s, page) = open_maker(cx);
    cx.simulate_window_resize(
        s.win,
        gpui_kit::size(gpui_kit::px(800.), gpui_kit::px(600.)),
    );
    support::settle(cx, s.win);
    support::click_id(cx, s.win, ids::HUD_MAKER_SITUATION);
    support::click_id(cx, s.win, ids::HUD_MAKER_SITUATION_METER);
    let row = support::bounds_of(cx, s.win, ids::HUD_MAKER_SITUATION_METER).unwrap();
    let list = support::bounds_of(cx, s.win, ids::HUD_MAKER_SITUATION_METERS).unwrap();
    assert!(row.top() >= gpui_kit::px(0.), "{row:?}");
    assert!(list.bottom() <= gpui_kit::px(600.), "{list:?}");
    // The last meter is reachable by scrolling the list.
    let down =
        gpui_kit::ScrollDelta::Pixels(gpui_kit::point(gpui_kit::px(0.), gpui_kit::px(-2000.)));
    cx.update_window(s.win, |_, w, cx| {
        w.scroll(ids::HUD_MAKER_SITUATION_METERS, down, cx)
    })
    .unwrap();
    support::settle(cx, s.win);
    support::click_id(
        cx,
        s.win,
        ids::hud_maker_situation_meter(MeterKind::Canteen),
    );
    assert_eq!(meter(cx, &page), MeterKind::Canteen);
}
