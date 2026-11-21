//! The draft editor laid out as the mock draws it: labelled groups, a sticky narrow rail and a
//! name held still while saving.

use crate::support;

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{AppContext as _, Keystroke, ScrollDelta, TestAppContext, point, px, size};
use support::graphics::found;
use support::profiles::{edit, game_files, page, seed, type_name};
use tf2tools_app::ids;
use tf2tools_core::{DraftItem, ProfileDraft, SlotMode};


fn full(s: &support::ShellWindow, seeded: &support::profiles::Seeded) -> tf2tools_core::Profile {
    let draft = ProfileDraft::new("Comp")
        .with_hud(SlotMode::Set(DraftItem::Pick(seeded.rayshud.id().clone())))
        .with_sounds(SlotMode::Set((
            Some(DraftItem::Pick(seeded.ding.id().clone())),
            None,
        )))
        .with_crosshairs(SlotMode::Set(Vec::new()));
    s.services.library.create_profile(&draft).unwrap()
}

#[gpui_kit::test]
fn the_mode_groups_and_the_crumb_are_labelled(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let seeded = seed(&s, &env);
    let p = full(&s, &seeded);
    let page = page(cx, &s);
    let before = game_files(&env);
    edit(cx, &s, &page, p.id());
    for (slot, label) in [("hud", "HUD"), ("sounds", "Sounds"), ("addons", "Add-ons")] {
        assert_eq!(
            support::label_of(cx, s.win, ids::profile_slot_modes(slot)),
            Some(format!("What this profile does with {label}"))
        );
    }
    assert_eq!(
        support::label_of(cx, s.win, ids::PROFILE_EDITOR_HEADER).as_deref(),
        Some("Profile editor")
    );
    assert_eq!(game_files(&env), before);
}

#[gpui_kit::test]
fn the_narrow_rail_sticks_above_the_cards_while_the_body_scrolls(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let seeded = seed(&s, &env);
    let p = full(&s, &seeded);
    cx.simulate_window_resize(s.win, size(px(800.), px(600.)));
    let page = page(cx, &s);
    let before = game_files(&env);
    edit(cx, &s, &page, p.id());
    let name = support::bounds_of(cx, s.win, ids::PROFILE_EDITOR_NAME).unwrap();
    let rail = support::bounds_of(cx, s.win, ids::PROFILE_RAIL).unwrap();
    assert!(
        rail.top() > name.bottom(),
        "starts under the name: {rail:?}"
    );
    let down = ScrollDelta::Lines(point(0., -20.));
    cx.update_window(s.win, |_, w, cx| {
        w.scroll(ids::PROFILE_EDITOR_BODY, down, cx)
    })
    .unwrap();
    support::settle(cx, s.win);
    let body = support::bounds_of(cx, s.win, ids::PROFILE_EDITOR_BODY).unwrap();
    let rail = support::bounds_of(cx, s.win, ids::PROFILE_RAIL).unwrap();
    let hud = support::bounds_of(cx, s.win, ids::profile_slot("hud")).unwrap();
    assert!(hud.top() < body.top(), "the cards scrolled: {hud:?}");
    assert!(
        rail.top() >= body.top() - px(1.) && rail.top() <= body.top() + px(8.),
        "{rail:?} stuck at the top of {body:?}"
    );
    // A stuck link still answers: it scrolls its card up under the rail.
    let far = support::bounds_of(cx, s.win, ids::profile_slot("addons")).unwrap();
    support::click_id(cx, s.win, ids::profile_rail("addons"));
    let moved = support::bounds_of(cx, s.win, ids::profile_slot("addons")).unwrap();
    assert!(moved.top() < far.top(), "{moved:?} came up from {far:?}");
    assert_eq!(game_files(&env), before);
}

#[gpui_kit::test]
fn the_name_is_read_only_while_saving(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let seeded = seed(&s, &env);
    let p = full(&s, &seeded);
    let page = page(cx, &s);
    let before = game_files(&env);
    let e = edit(cx, &s, &page, p.id());
    type_name(cx, &s, &e, "Busy two");
    // Save starts and the busy editor draws, then keys reach the focused name before the
    // save's answer can land.
    cx.update_window(s.win, |_, w, cx| {
        e.update(cx, |e, cx| {
            e.focus_name(w, cx);
            e.save(w, cx);
        });
        w.render_frame(cx);
        for key in ["z", "z"] {
            w.dispatch_keystroke(Keystroke::parse(key).unwrap(), cx);
        }
    })
    .unwrap();
    let input = cx.update(|cx| e.read(cx).name_input().clone());
    assert_eq!(
        cx.update(|cx| input.read(cx).value().to_string()),
        "Busy two"
    );
    support::wait_real(cx, "the list", |cx| {
        cx.update(|cx| page.read(cx).editor().is_none())
    });
    assert_eq!(
        s.services.library.profile(p.id()).unwrap().name(),
        "Busy two"
    );
    assert_eq!(game_files(&env), before);
}
