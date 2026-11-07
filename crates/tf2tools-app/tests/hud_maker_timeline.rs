//! The timeline plays the situation's events on the canvas, headless.

use crate::support;

use gpui_kit::TestAppContext;
use support::maker_page::{ANIMS, VALUE, on_page, open_maker, open_maker_with};
use tf2tools_app::ids;
use tf2tools_core::{GameSituation, PartKey};

#[gpui_kit::test]
fn low_health_shows_the_timeline_with_its_pulse(cx: &mut TestAppContext) {
    let (_env, s, page) = open_maker(cx);
    assert!(!support::present(cx, s.win, ids::HUD_MAKER_TIMELINE));
    on_page(cx, &s, &page, |p, w, cx| {
        p.set_situation(GameSituation::default().with_health(60), w, cx)
    });
    assert_eq!(
        support::label_of(cx, s.win, ids::HUD_MAKER_TIMELINE_EVENTS).as_deref(),
        Some("Low health pulse")
    );
    support::click_id(cx, s.win, ids::HUD_MAKER_TIMELINE_PLAY);
    assert!(cx.update(|cx| page.read(cx).is_playing()));
    support::click_id(cx, s.win, ids::HUD_MAKER_TIMELINE_PLAY);
    assert!(!cx.update(|cx| page.read(cx).is_playing()));
}

#[gpui_kit::test]
fn a_move_animation_moves_the_part_at_the_moment_and_clicks_still_pick(cx: &mut TestAppContext) {
    let (_env, s, page) = open_maker(cx);
    let value = PartKey::new(VALUE);
    let before = support::bounds_of(cx, s.win, ids::hud_maker_part(&value)).unwrap();
    on_page(cx, &s, &page, |p, w, cx| {
        assert!(p.play_event("TestNudge", w, cx));
        p.set_moment(0.5, w, cx);
    });
    let after = support::bounds_of(cx, s.win, ids::hud_maker_part(&value)).unwrap();
    assert!(after.left() < before.left(), "{before:?} -> {after:?}");
    // Alt takes the value itself rather than its health group.
    let alt = gpui_kit::Modifiers {
        alt: true,
        ..Default::default()
    };
    let (button, position) = (gpui_kit::MouseButton::Left, after.center());
    gpui_kit::AppContext::update_window(cx, s.win, |_, w, cx| {
        let down = gpui_kit::MouseDownEvent {
            button,
            position,
            modifiers: alt,
            click_count: 1,
            first_mouse: false,
        };
        let up = gpui_kit::MouseUpEvent {
            button,
            position,
            modifiers: alt,
            click_count: 1,
        };
        let _ = w.dispatch_event(gpui_kit::InputEvent::to_platform_input(down), cx);
        let _ = w.dispatch_event(gpui_kit::InputEvent::to_platform_input(up), cx);
    })
    .unwrap();
    support::settle(cx, s.win);
    assert_eq!(
        cx.update(|cx| page.read(cx).editor().unwrap().selected().cloned()),
        Some(value.clone())
    );
    on_page(cx, &s, &page, |p, _, cx| {
        p.editor_mut()
            .unwrap()
            .recolour_selected("255 0 0 255")
            .unwrap();
        cx.notify();
    });
    assert_eq!(
        cx.update(|cx| page.read(cx).editor().unwrap().moment()),
        0.0,
        "an edit returns to 0"
    );
    let back = support::bounds_of(cx, s.win, ids::hud_maker_part(&value)).unwrap();
    assert_eq!(back.left(), before.left());
    support::click_id(cx, s.win, ids::HUD_MAKER_TIMELINE_CLEAR);
    assert!(!support::present(cx, s.win, ids::HUD_MAKER_TIMELINE));
}

#[gpui_kit::test]
fn the_time_shows_the_events_real_end(cx: &mut TestAppContext) {
    let (_env, s, page) = open_maker(cx);
    on_page(cx, &s, &page, |p, w, cx| {
        assert!(p.play_event("HudMedicCharged", w, cx));
        p.set_moment(0.5, w, cx);
    });
    assert_eq!(cx.update(|cx| page.read(cx).editor().unwrap().end()), 0.1);
    assert_eq!(
        support::label_of(cx, s.win, ids::HUD_MAKER_TIMELINE_TIME).as_deref(),
        Some("0.1 s of 0.1 s")
    );
}

#[gpui_kit::test]
fn an_edit_from_the_panel_stops_playback(cx: &mut TestAppContext) {
    let (_env, s, page) = open_maker(cx);
    on_page(cx, &s, &page, |p, w, cx| {
        p.editor_mut().unwrap().select(Some(PartKey::new(VALUE)));
        assert!(p.play_event("TestNudge", w, cx));
        p.toggle_playing(w, cx);
    });
    assert!(cx.update(|cx| page.read(cx).is_playing()));
    support::maker_page::click_in_panel(cx, &s, ids::HUD_MAKER_HIT_FLASH.into());
    assert!(cx.update(|cx| page.read(cx).editor().unwrap().hit_flash_selected()));
    assert!(
        !cx.update(|cx| page.read(cx).is_playing()),
        "an edit stops playback"
    );
}

#[gpui_kit::test]
fn the_timeline_names_events_in_player_words(cx: &mut TestAppContext) {
    let (_env, s, page) = open_maker(cx);
    on_page(cx, &s, &page, |p, w, cx| {
        assert!(p.play_event("HudMedicCharged", w, cx));
        assert!(p.play_event("TestNudge", w, cx));
    });
    assert_eq!(
        support::label_of(cx, s.win, ids::HUD_MAKER_TIMELINE_EVENTS).as_deref(),
        Some("ÜberCharge ready, Test nudge")
    );
}

#[gpui_kit::test]
fn an_event_that_moves_nothing_shows_no_timeline(cx: &mut TestAppContext) {
    let (_env, s, page) = open_maker(cx);
    on_page(cx, &s, &page, |p, w, cx| {
        assert!(p.play_event("DamagedPlayer", w, cx));
    });
    assert!(!support::present(cx, s.win, ids::HUD_MAKER_TIMELINE));
}

#[gpui_kit::test]
fn low_health_on_a_hud_without_its_pulse_shows_no_timeline(cx: &mut TestAppContext) {
    let anims = ANIMS.replace("event HudHealthDyingPulse\n", "event SomethingElse\n");
    assert_ne!(anims, ANIMS);
    let (_env, s, page) = open_maker_with(cx, &anims);
    on_page(cx, &s, &page, |p, w, cx| {
        p.set_situation(GameSituation::default().with_health(60), w, cx)
    });
    assert!(!support::present(cx, s.win, ids::HUD_MAKER_TIMELINE));
    assert!(!support::present(cx, s.win, ids::HUD_MAKER_TIMELINE_TIME));
}
