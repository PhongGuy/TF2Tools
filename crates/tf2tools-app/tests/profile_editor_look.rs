//! The draft editor laid out as the mock draws it: labelled groups, a sticky narrow rail, the
//! HUD tile grid, the Sounds, Graphics and Add-ons rows, and a name held still while saving.

use crate::support;

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{AppContext as _, Keystroke, ScrollDelta, TestAppContext, point, px, size};
use support::graphics::found;
use support::profiles::{FLAT, PYRO, addon, edit, game_files, low_fps60, page, seed, type_name};
use tf2tools_app::ids;
use tf2tools_core::{DraftItem, ProfileDraft, SlotMode};

/// A slot card's padding plus its border, each side.
const CARD_INSET: f32 = 17.;

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

fn gfx_profile(s: &support::ShellWindow, env: &support::Env) -> tf2tools_core::Profile {
    let vpk = support::vpk_bytes(&[("cfg/addon.cfg", b"echo addon\n")]);
    for name in [PYRO, FLAT] {
        support::seed_library_addon(&s.services, env, name, "9.100.1", &vpk, false);
    }
    let draft = ProfileDraft::new("Potato")
        .with_graphics(SlotMode::Set(low_fps60()))
        .with_addons(SlotMode::Set((vec![DraftItem::Pick(addon(PYRO))], true)));
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
fn the_hud_tiles_fill_each_row_at_170_px_or_wider(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let seeded = seed(&s, &env);
    let p = full(&s, &seeded);
    let page = page(cx, &s);
    let before = game_files(&env);
    edit(cx, &s, &page, p.id());
    let (rayshud, budhud) = (seeded.rayshud.folder_name(), seeded.budhud.folder_name());
    for (w, h) in [(1280., 800.), (800., 600.)] {
        cx.simulate_window_resize(s.win, size(px(w), px(h)));
        support::wait_real(cx, "the HUD tiles", |cx| {
            support::present(cx, s.win, ids::profile_pick_hud(budhud))
        });
        let card = support::bounds_of(cx, s.win, ids::profile_slot("hud")).unwrap();
        let inner = card.size.width - px(CARD_INSET * 2.);
        let first = support::bounds_of(cx, s.win, ids::profile_pick_hud(rayshud)).unwrap();
        let second = support::bounds_of(cx, s.win, ids::profile_pick_hud(budhud)).unwrap();
        assert!(first.size.width >= px(170.), "{w}: {first:?}");
        assert!((first.size.width - second.size.width).abs() <= px(1.));
        // The columns of at least 170 px that fit, sharing the row with 16 px gaps.
        let gap = px(16.);
        let columns = ((inner + gap) / (px(170.) + gap)).floor().max(1.);
        let cell = (inner - gap * (columns - 1.)) / columns;
        assert!(
            (first.size.width - cell).abs() <= px(1.5),
            "{w}: {first:?} is one of {columns} cells in {inner:?}"
        );
        assert!(
            (first.left().min(second.left()) - (card.left() + px(CARD_INSET))).abs() <= px(1.),
            "{w}: the grid starts at the card's padding"
        );
        assert!(support::present(cx, s.win, ids::profile_hud_shot(rayshud)));
        assert!(
            !support::present(cx, s.win, ids::hud_shot(rayshud)),
            "no HUDs page ids in the editor"
        );
    }
    assert_eq!(game_files(&env), before);
}

#[gpui_kit::test]
fn a_sound_row_puts_play_before_its_select(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let seeded = seed(&s, &env);
    let p = full(&s, &seeded);
    let page = page(cx, &s);
    edit(cx, &s, &page, p.id());
    support::wait_real(cx, "the sound rows", |cx| {
        support::present(cx, s.win, ids::PROFILE_SOUND_HIT_PLAY)
    });
    for (play, select) in [
        (ids::PROFILE_SOUND_HIT_PLAY, ids::PROFILE_SOUND_HIT),
        (ids::PROFILE_SOUND_KILL_PLAY, ids::PROFILE_SOUND_KILL),
    ] {
        let play = support::bounds_of(cx, s.win, play).unwrap();
        let select = support::bounds_of(cx, s.win, select).unwrap();
        assert!(play.right() <= select.left(), "{play:?} before {select:?}");
    }
}

#[gpui_kit::test]
fn copy_heads_the_graphics_settings_card(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    support::seed_managed_mastercomfig(&env, "9.100.1", "medium");
    support::seed_mastercomfig_catalog(&env, "9.100.1");
    let p = gfx_profile(&s, &env);
    cx.simulate_window_resize(s.win, size(px(1280.), px(4000.)));
    let page = page(cx, &s);
    edit(cx, &s, &page, p.id());
    let gfx = ids::IdScope::PROFILE_GRAPHICS;
    support::wait_real(cx, "the rows", |cx| {
        support::present(cx, s.win, gfx.id(ids::graphics_row("fpscap")))
    });
    let card = support::bounds_of(cx, s.win, ids::PROFILE_GFX_SETTINGS).unwrap();
    let copy = support::bounds_of(cx, s.win, ids::PROFILE_GFX_COPY).unwrap();
    let row = support::bounds_of(cx, s.win, gfx.id(ids::graphics_row("fpscap"))).unwrap();
    let presets = support::bounds_of(
        cx,
        s.win,
        gfx.id(ids::graphics_preset(tf2tools_core::Preset::Low)),
    )
    .unwrap();
    assert!(
        card.top() > presets.bottom(),
        "the card comes after the presets"
    );
    assert!(card.contains(&copy.center()), "{copy:?} in {card:?}");
    assert!(card.contains(&row.center()), "{row:?} in {card:?}");
    assert!(copy.bottom() <= row.top(), "Copy heads the rows");
    assert!(
        copy.right() >= card.right() - px(32.),
        "Copy sits at the header's right: {copy:?}"
    );
}

#[gpui_kit::test]
fn an_addon_row_puts_its_switch_at_the_right(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let p = gfx_profile(&s, &env);
    cx.simulate_window_resize(s.win, size(px(1280.), px(4000.)));
    let page = page(cx, &s);
    edit(cx, &s, &page, p.id());
    support::wait_real(cx, "the switches", |cx| {
        support::present(cx, s.win, ids::profile_addon(FLAT))
    });
    let card = support::bounds_of(cx, s.win, ids::profile_slot("addons")).unwrap();
    for id in [
        ids::profile_addon(FLAT),
        ids::profile_addon(PYRO),
        ids::PROFILE_ADDONS_ONLY.into(),
    ] {
        let switch = support::bounds_of(cx, s.win, id.clone()).unwrap();
        assert!(
            (card.right() - px(CARD_INSET) - switch.right()).abs() <= px(1.),
            "{id:?} {switch:?} at the right of {card:?}"
        );
        assert!(switch.size.width < px(80.), "{id:?} is the switch alone");
    }
    assert_eq!(
        support::label_of(cx, s.win, ids::profile_addon(FLAT)).as_deref(),
        Some("Flat Mouse")
    );
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

fn many_addons(s: &support::ShellWindow, env: &support::Env) -> tf2tools_core::Profile {
    let vpk = support::vpk_bytes(&[("cfg/addon.cfg", b"echo addon\n")]);
    for i in 0..22 {
        let name = format!("mastercomfig-addon-test-{i:02}.vpk");
        support::seed_library_addon(&s.services, env, &name, "9.100.1", &vpk, false);
    }
    let draft = ProfileDraft::new("Many")
        .with_graphics(SlotMode::Set(low_fps60()))
        .with_addons(SlotMode::Set((Vec::new(), true)));
    s.services.library.create_profile(&draft).unwrap()
}

#[gpui_kit::test]
fn the_narrow_rail_never_rises_over_the_header(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let p = many_addons(&s, &env);
    cx.simulate_window_resize(s.win, size(px(800.), px(600.)));
    let page = page(cx, &s);
    let before = game_files(&env);
    edit(cx, &s, &page, p.id());
    support::wait_real(cx, "the add-on rows", |cx| {
        support::present(
            cx,
            s.win,
            ids::profile_addon("mastercomfig-addon-test-21.vpk"),
        )
    });
    let down = ScrollDelta::Lines(point(0., -1000.));
    cx.update_window(s.win, |_, w, cx| {
        w.scroll(ids::PROFILE_EDITOR_BODY, down, cx)
    })
    .unwrap();
    support::settle(cx, s.win);
    let body = support::bounds_of(cx, s.win, ids::PROFILE_EDITOR_BODY).unwrap();
    let rail = support::bounds_of(cx, s.win, ids::PROFILE_RAIL).unwrap();
    let addons = support::bounds_of(cx, s.win, ids::profile_slot("addons")).unwrap();
    assert!(
        addons.top() < body.top(),
        "the last card scrolled past: {addons:?}"
    );
    assert!(
        rail.top() >= body.top() - px(1.),
        "{rail:?} stays inside {body:?}"
    );
    support::click_id(cx, s.win, ids::PROFILE_EDITOR_CANCEL);
    assert!(
        cx.update(|cx| page.read(cx).editor().is_none()),
        "Cancel answers past the rail"
    );
    assert_eq!(game_files(&env), before);
}

#[gpui_kit::test]
fn a_narrow_rail_jump_shows_the_card_title_below_the_row(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let p = many_addons(&s, &env);
    cx.simulate_window_resize(s.win, size(px(800.), px(600.)));
    let page = page(cx, &s);
    edit(cx, &s, &page, p.id());
    support::wait_real(cx, "the add-on rows", |cx| {
        support::present(
            cx,
            s.win,
            ids::profile_addon("mastercomfig-addon-test-21.vpk"),
        )
    });
    for slot in ["graphics", "addons"] {
        support::click_id(cx, s.win, ids::profile_rail(slot));
        let rail = support::bounds_of(cx, s.win, ids::PROFILE_RAIL).unwrap();
        let card = support::bounds_of(cx, s.win, ids::profile_slot(slot)).unwrap();
        let help = support::bounds_of(cx, s.win, ids::profile_slot_help(slot)).unwrap();
        assert!(
            card.top() >= rail.bottom() - px(1.) && card.top() <= rail.bottom() + px(24.),
            "{slot}: {card:?} just below {rail:?}"
        );
        assert!(help.top() >= rail.bottom(), "{slot}: the help line shows");
    }
}

#[gpui_kit::test]
fn clicking_an_addon_name_flips_its_switch(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let p = gfx_profile(&s, &env);
    cx.simulate_window_resize(s.win, size(px(1280.), px(4000.)));
    let page = page(cx, &s);
    let before = game_files(&env);
    let e = edit(cx, &s, &page, p.id());
    support::wait_real(cx, "the switches", |cx| {
        support::present(cx, s.win, ids::profile_addon(FLAT))
    });
    support::click_id(cx, s.win, ids::profile_addon_name(FLAT));
    assert_eq!(
        support::checked_of(cx, s.win, ids::profile_addon(FLAT)),
        Some(true)
    );
    support::click_id(cx, s.win, ids::profile_addon_name(PYRO));
    assert_eq!(
        support::checked_of(cx, s.win, ids::profile_addon(PYRO)),
        Some(false)
    );
    support::click_id(cx, s.win, ids::PROFILE_ADDONS_ONLY_NAME);
    assert_eq!(
        support::checked_of(cx, s.win, ids::PROFILE_ADDONS_ONLY),
        Some(false)
    );
    let draft = cx.update(|cx| e.read(cx).draft().clone());
    assert_eq!(draft.addons.len(), 1);
    assert!(!draft.only_these);
    assert_eq!(game_files(&env), before);
}
