//! "Add to profile" on library cards: the profile menu and what a pick saves.

use crate::support;

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{AppContext as _, TestAppContext};
use tf2tools_app::ids;
use tf2tools_app::services::Tf2State;
use tf2tools_app::shell::route::Page;
use tf2tools_core::{DraftSlot, HudEntry, Profile, ProfileDraft, Provenance, Tf2Install};

fn found(env: &support::Env) -> Tf2State {
    Tf2State::Found(Tf2Install::from_path(&env.game).unwrap())
}

fn seed_hud(s: &support::ShellWindow, env: &support::Env, name: &str) -> HudEntry {
    let path = env.tmp.path().join(format!("{name}.zip"));
    std::fs::write(&path, support::hud_zip_bytes(name)).unwrap();
    s.services
        .library
        .import_hud(&path, Provenance::Local)
        .unwrap()
}

fn profile(s: &support::ShellWindow, draft: ProfileDraft) -> Profile {
    s.services.library.create_profile(&draft).unwrap()
}

fn reload(s: &support::ShellWindow, p: &Profile) -> Profile {
    s.services.library.profile(p.id()).unwrap()
}

/// Leaves for Settings and comes back, so a fresh HUDs page lists the seeded HUDs.
fn open_huds(cx: &mut TestAppContext, s: &support::ShellWindow) {
    support::click_id(cx, s.win, ids::nav(Page::Settings));
    support::click_id(cx, s.win, ids::nav(Page::Huds));
    let page = cx
        .update(|cx| s.shell.read(cx).huds_page())
        .expect("a HUDs page");
    support::wait_real(cx, "HUDs to load", |cx| {
        cx.update(|cx| page.read(cx).is_loaded())
    });
    support::settle(cx, s.win);
}

fn toast_is(cx: &mut TestAppContext, s: &support::ShellWindow, text: &str) {
    support::wait_real(cx, text, |cx| {
        support::label_of(cx, s.win, ids::TOAST).as_deref() == Some(text)
    });
}

#[gpui_kit::test]
fn picking_a_profile_sets_its_hud(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let hud = seed_hud(&s, &env, "rayshud");
    let a = profile(&s, ProfileDraft::new("Alpha"));
    let b = profile(&s, ProfileDraft::new("Bravo"));
    open_huds(cx, &s);
    support::click_id(cx, s.win, ids::hud_add_to_profile("rayshud"));
    support::click_id(cx, s.win, ids::hud_profile_pick(b.id()));
    toast_is(cx, &s, "Added rayshud to Bravo");
    let saved = reload(&s, &b);
    assert_eq!(
        saved.hud().map(|h| h.folder_name()),
        Some(hud.folder_name())
    );
    assert_eq!(reload(&s, &a), a);
}

#[gpui_kit::test]
fn a_pick_names_the_hud_it_replaced(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let old = seed_hud(&s, &env, "toonhud");
    seed_hud(&s, &env, "rayshud");
    let p = profile(
        &s,
        ProfileDraft::new("Comp").with_hud(DraftSlot::Item(old.id().clone())),
    );
    open_huds(cx, &s);
    support::click_id(cx, s.win, ids::hud_add_to_profile("rayshud"));
    support::click_id(cx, s.win, ids::hud_profile_pick(p.id()));
    toast_is(cx, &s, "Added rayshud to Comp (replaced toonhud)");
    assert_eq!(
        reload(&s, &p).hud().map(|h| h.folder_name().to_owned()),
        Some("rayshud".to_owned())
    );
}

#[gpui_kit::test]
fn with_no_profiles_the_menu_says_so(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    seed_hud(&s, &env, "rayshud");
    open_huds(cx, &s);
    support::click_id(cx, s.win, ids::hud_add_to_profile("rayshud"));
    assert_eq!(
        support::label_of(cx, s.win, ids::PROFILE_MENU_EMPTY).as_deref(),
        Some("No profiles yet")
    );
    support::click_id(cx, s.win, ids::PROFILE_MENU_EMPTY);
    assert!(s.services.library.profiles().unwrap().is_empty());
    assert!(support::toast_labels(cx, s.win).is_empty());
}

#[gpui_kit::test]
fn a_profile_deleted_while_the_menu_is_open_toasts_an_error(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    seed_hud(&s, &env, "rayshud");
    let p = profile(&s, ProfileDraft::new("Comp"));
    open_huds(cx, &s);
    support::click_id(cx, s.win, ids::hud_add_to_profile("rayshud"));
    s.services.library.delete_profile(p.id()).unwrap();
    support::click_id(cx, s.win, ids::hud_profile_pick(p.id()));
    support::wait_real(cx, "an error toast", |cx| {
        support::toast_labels(cx, s.win)
            .iter()
            .any(|l| l.contains("This profile no longer exists"))
    });
    assert!(s.services.library.profiles().unwrap().is_empty());
}

#[gpui_kit::test]
fn adding_to_the_active_profile_says_it_is_not_applied(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    seed_hud(&s, &env, "rayshud");
    let p = profile(&s, ProfileDraft::new("Comp"));
    let tf2 = Tf2Install::from_path(&env.game).unwrap();
    s.services
        .library
        .apply_profile(p.id(), &tf2, &s.services.builtins, false)
        .unwrap();
    open_huds(cx, &s);
    support::click_id(cx, s.win, ids::hud_add_to_profile("rayshud"));
    support::click_id(cx, s.win, ids::hud_profile_pick(p.id()));
    toast_is(cx, &s, "Added rayshud to Comp. Changes not applied yet");
}

#[gpui_kit::test]
fn the_button_fits_inside_its_card(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    seed_hud(&s, &env, "rayshud");
    open_huds(cx, &s);
    let (card, button) = cx
        .update_window(s.win, |_, w, _| {
            let card = w.find(ids::hud_card("rayshud")).bounds();
            (card, w.find(ids::hud_add_to_profile("rayshud")).bounds())
        })
        .unwrap();
    assert!(
        button.left() >= card.left() && button.right() <= card.right(),
        "button {button:?} in card {card:?}"
    );
}
