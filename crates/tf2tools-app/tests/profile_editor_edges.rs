//! The draft editor's edges: the rail's jumps and pin, the Graphics catalogue fallback and
//! Copy, and the add-on and sound entries the library no longer has.

use crate::support;

use gpui_kit::component::select::SelectEvent;
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{AppContext as _, ScrollDelta, TestAppContext, point, px, size};
use support::graphics::found;
use support::profiles::{FLAT, PYRO, addon, edit, game_files, low_fps60, page, seed};
use tf2tools_app::ids;
use tf2tools_app::pages::profiles::draft_editor::SoundOption;
use tf2tools_app::services::Tf2State;
use tf2tools_app::shell::CONTENT_PADDING;
use tf2tools_app::shell::route::Page;
use tf2tools_core::{DraftItem, ProfileDraft, SlotMode, SoundKind};

fn crosshairs_profile(
    s: &support::ShellWindow,
    seeded: &support::profiles::Seeded,
) -> tf2tools_core::Profile {
    let draft = ProfileDraft::new("Long")
        .with_hud(SlotMode::Set(DraftItem::Pick(seeded.rayshud.id().clone())))
        .with_sounds(SlotMode::Set((None, None)))
        .with_crosshairs(SlotMode::Set(Vec::new()));
    s.services.library.create_profile(&draft).unwrap()
}

fn seed_addons(s: &support::ShellWindow, env: &support::Env) {
    let vpk = support::vpk_bytes(&[("cfg/addon.cfg", b"echo addon\n")]);
    for name in [PYRO, FLAT] {
        support::seed_library_addon(&s.services, env, name, "9.100.1", &vpk, false);
    }
}

fn gfx_profile(s: &support::ShellWindow, env: &support::Env) -> tf2tools_core::Profile {
    seed_addons(s, env);
    let draft = ProfileDraft::new("Potato")
        .with_graphics(SlotMode::Set(low_fps60()))
        .with_addons(SlotMode::Set((vec![DraftItem::Pick(addon(PYRO))], true)));
    s.services.library.create_profile(&draft).unwrap()
}

fn wheel_body(cx: &mut TestAppContext, s: &support::ShellWindow, notches: f32) {
    let down = ScrollDelta::Lines(point(0., -notches));
    cx.update_window(s.win, |_, w, cx| {
        w.scroll(ids::PROFILE_EDITOR_BODY, down, cx)
    })
    .unwrap();
    support::settle(cx, s.win);
}

#[gpui_kit::test]
fn a_rail_click_scrolls_its_card_to_the_top(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let seeded = seed(&s, &env);
    let p = crosshairs_profile(&s, &seeded);
    cx.simulate_window_resize(s.win, size(px(1280.), px(600.)));
    let page = page(cx, &s);
    let before = game_files(&env);
    edit(cx, &s, &page, p.id());
    let body = support::bounds_of(cx, s.win, ids::PROFILE_EDITOR_BODY).unwrap();
    let at_top = |card: gpui_kit::Bounds<gpui_kit::Pixels>| {
        card.top() >= body.top() - px(1.) && card.top() <= body.top() + CONTENT_PADDING + px(1.)
    };
    let sounds = support::bounds_of(cx, s.win, ids::profile_slot("sounds")).unwrap();
    assert!(!at_top(sounds), "starts lower down: {sounds:?}");
    support::click_id(cx, s.win, ids::profile_rail("sounds"));
    let sounds = support::bounds_of(cx, s.win, ids::profile_slot("sounds")).unwrap();
    assert!(at_top(sounds), "{sounds:?} at the top of {body:?}");
    // The last card cannot reach the top, so its jump ends with it in view.
    let far = support::bounds_of(cx, s.win, ids::profile_slot("addons")).unwrap();
    assert!(far.top() > body.bottom(), "starts below the fold: {far:?}");
    support::click_id(cx, s.win, ids::profile_rail("addons"));
    let far = support::bounds_of(cx, s.win, ids::profile_slot("addons")).unwrap();
    assert!(
        far.top() >= body.top() && far.bottom() <= body.bottom(),
        "{far:?} in view in {body:?}"
    );
    assert_eq!(game_files(&env), before);
}

#[gpui_kit::test]
fn the_wide_rail_stays_in_view_while_the_cards_scroll(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let seeded = seed(&s, &env);
    let p = crosshairs_profile(&s, &seeded);
    cx.simulate_window_resize(s.win, size(px(1280.), px(600.)));
    let page = page(cx, &s);
    edit(cx, &s, &page, p.id());
    wheel_body(cx, &s, 8.);
    let body = support::bounds_of(cx, s.win, ids::PROFILE_EDITOR_BODY).unwrap();
    let hud = support::bounds_of(cx, s.win, ids::profile_slot("hud")).unwrap();
    let rail = support::bounds_of(cx, s.win, ids::PROFILE_RAIL).unwrap();
    assert!(hud.top() < body.top(), "the cards scrolled: {hud:?}");
    assert!(
        rail.top() >= body.top() - px(1.) && rail.top() <= body.top() + CONTENT_PADDING + px(1.),
        "{rail:?} pinned at the top of {body:?}"
    );
}

#[gpui_kit::test]
fn without_a_managed_version_the_newest_cached_catalogue_shows(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    // By number 9.100.1 is the newer, though "9.99.0" sorts after it as text.
    support::seed_mastercomfig_catalog(&env, "9.99.0");
    support::seed_full_mastercomfig_catalog(&env, "9.100.1");
    let p = gfx_profile(&s, &env);
    cx.simulate_window_resize(s.win, size(px(1280.), px(4000.)));
    let page = page(cx, &s);
    let before = game_files(&env);
    edit(cx, &s, &page, p.id());
    let gfx = ids::IdScope::PROFILE_GRAPHICS;
    support::wait_real(cx, "the rows", |cx| {
        support::present(cx, s.win, gfx.id(ids::graphics_row("fpscap")))
    });
    assert!(
        support::present(cx, s.win, gfx.id(ids::graphics_row("shadows"))),
        "only the full 9.100.1 catalogue has shadows"
    );
    assert!(!support::present(cx, s.win, ids::PROFILE_GFX_NO_CATALOG));
    assert_eq!(game_files(&env), before);
}

#[gpui_kit::test]
fn copy_toasts_when_the_graphics_choices_cannot_be_read(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    support::seed_managed_mastercomfig(&env, "9.100.1", "medium");
    support::seed_mastercomfig_catalog(&env, "9.100.1");
    let p = gfx_profile(&s, &env);
    // The saved choices live in this table; hidden, their read fails.
    rusqlite::Connection::open(env.data.db_path())
        .unwrap()
        .execute_batch("ALTER TABLE mastercomfig_state RENAME TO mastercomfig_state_hidden")
        .unwrap();
    cx.simulate_window_resize(s.win, size(px(1280.), px(4000.)));
    let page = page(cx, &s);
    let before = game_files(&env);
    let e = edit(cx, &s, &page, p.id());
    support::wait_real(cx, "Copy", |cx| {
        support::present(cx, s.win, ids::PROFILE_GFX_COPY)
            && !support::disabled_of(cx, s.win, ids::PROFILE_GFX_COPY)
    });
    support::click_id(cx, s.win, ids::PROFILE_GFX_COPY);
    let toasts = support::toast_labels(cx, s.win);
    assert_eq!(toasts.len(), 1, "{toasts:?}");
    support::assert_plain(&toasts[0]);
    assert!(
        !cx.update(|cx| e.read(cx).is_dirty()),
        "a failed Copy changes nothing"
    );
    assert_eq!(game_files(&env), before);
}

#[gpui_kit::test]
fn copy_is_off_without_tf2(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, Tf2State::Missing { saved: None });
    support::seed_managed_mastercomfig(&env, "9.100.1", "ultra");
    support::seed_mastercomfig_catalog(&env, "9.100.1");
    support::seed_mastercomfig_choices(&env, r#"{"fpscap":"144"}"#);
    let p = gfx_profile(&s, &env);
    cx.simulate_window_resize(s.win, size(px(1280.), px(4000.)));
    let page = page(cx, &s);
    let e = edit(cx, &s, &page, p.id());
    // The catalogue lands with the saved choices, so Copy has its answer by then.
    support::wait_real(cx, "the Graphics read", |cx| {
        cx.update(|cx| e.read(cx).graphics().read(cx).catalog().is_some())
    });
    assert!(support::disabled_of(cx, s.win, ids::PROFILE_GFX_COPY));
}

#[gpui_kit::test]
fn the_addon_store_link_opens_the_store(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let p = s
        .services
        .library
        .create_profile(
            &ProfileDraft::new("None yet").with_addons(SlotMode::Set((Vec::new(), false))),
        )
        .unwrap();
    cx.simulate_window_resize(s.win, size(px(1280.), px(4000.)));
    let page = page(cx, &s);
    edit(cx, &s, &page, p.id());
    support::wait_real(cx, "the store link", |cx| {
        support::present(cx, s.win, ids::PROFILE_ADDONS_STORE)
    });
    support::click_id(cx, s.win, ids::PROFILE_ADDONS_STORE);
    support::wait_real(cx, "the Addon Store", |cx| {
        support::present(cx, s.win, ids::page(Page::AddonStore))
    });
}

#[gpui_kit::test]
fn a_kept_sound_missing_from_the_library_keeps_its_select_entry(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let seeded = seed(&s, &env);
    let draft = ProfileDraft::new("Kept sound").with_sounds(SlotMode::Set((
        Some(DraftItem::Pick(seeded.ding.id().clone())),
        None,
    )));
    let p = s.services.library.create_profile(&draft).unwrap();
    s.services
        .library
        .delete_sound(seeded.ding.id(), None)
        .unwrap();
    let page = page(cx, &s);
    let before = game_files(&env);
    let e = edit(cx, &s, &page, p.id());
    support::wait_real(cx, "the sound rows", |cx| {
        support::present(cx, s.win, ids::PROFILE_SOUND_HIT)
    });
    let hit = cx.update(|cx| e.read(cx).sound_select(SoundKind::Hitsound).clone());
    let shown = cx.update(|cx| hit.read(cx).selected_value().cloned());
    let ding = SoundOption {
        id: None,
        label: "Ding".into(),
    };
    assert_eq!(
        shown,
        Some(ding.clone()),
        "the missing pick is shown by name"
    );
    assert!(
        support::disabled_of(cx, s.win, ids::PROFILE_SOUND_HIT_PLAY),
        "a missing sound has nothing to play"
    );
    let bell = SoundOption {
        id: Some(seeded.bell.id().clone()),
        label: "Bell".into(),
    };
    for option in [bell, ding] {
        cx.update(|cx| {
            hit.update(cx, |_, cx| {
                cx.emit(SelectEvent::<Vec<SoundOption>>::Confirm(Some(option)))
            })
        });
        support::settle(cx, s.win);
    }
    assert!(
        !cx.update(|cx| e.read(cx).is_dirty()),
        "the missing entry puts the kept pick back"
    );
    assert_eq!(game_files(&env), before);
}
