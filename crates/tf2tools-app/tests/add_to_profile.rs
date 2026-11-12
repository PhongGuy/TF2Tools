//! "Add to profile" on library cards: the profile menu and what a pick saves.

use crate::support;

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{AppContext as _, TestAppContext};
use tf2tools_app::ids;
use tf2tools_app::pages::profiles::editor_model::Slot;
use tf2tools_app::services::Tf2State;
use tf2tools_app::shell::route::Page;
use tf2tools_core::{
    CrosshairEntry, CrosshairRef, DraftSlot, HudEntry, Profile, ProfileDraft, Provenance,
    RgbaImage, SoundEntry, Tf2Install, rgba_to_vtf,
};

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

fn seed_sound(s: &support::ShellWindow, env: &support::Env, name: &str) -> SoundEntry {
    let path = env.tmp.path().join(format!("{name}.wav"));
    std::fs::write(&path, support::wav_bytes()).unwrap();
    s.services
        .library
        .import_sound(&path, name, Provenance::Local)
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

fn open_sounds(cx: &mut TestAppContext, s: &support::ShellWindow) {
    support::click_id(cx, s.win, ids::nav(Page::Sounds));
    let page = cx
        .update(|cx| s.shell.read(cx).sounds_page())
        .expect("a Sounds page");
    support::wait_real(cx, "Sounds to load", |cx| {
        cx.update(|cx| page.read(cx).is_loaded())
    });
    support::settle(cx, s.win);
}

fn sound_id(p: Option<&tf2tools_core::ProfileItem>) -> Option<String> {
    p.map(|i| i.folder_name().to_owned())
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

#[gpui_kit::test]
fn a_killsound_pick_keeps_the_hitsound(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let ding = seed_sound(&s, &env, "ding");
    let bell = seed_sound(&s, &env, "bell");
    let p = profile(
        &s,
        ProfileDraft::new("Comp").with_hitsound(DraftSlot::Item(ding.id().clone())),
    );
    open_sounds(cx, &s);
    support::click_id(cx, s.win, ids::sound_add_to_profile(bell.folder_name()));
    support::click_id(cx, s.win, ids::sound_kill_pick(p.id()));
    toast_is(cx, &s, "Added bell to Comp as its killsound");
    let saved = reload(&s, &p);
    assert_eq!(
        sound_id(saved.killsound()),
        Some(bell.folder_name().to_owned())
    );
    assert_eq!(
        sound_id(saved.hitsound()),
        Some(ding.folder_name().to_owned())
    );
}

#[gpui_kit::test]
fn a_hitsound_pick_names_the_sound_it_replaced(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let ding = seed_sound(&s, &env, "ding");
    let bell = seed_sound(&s, &env, "bell");
    let p = profile(
        &s,
        ProfileDraft::new("Comp").with_hitsound(DraftSlot::Item(ding.id().clone())),
    );
    open_sounds(cx, &s);
    support::click_id(cx, s.win, ids::sound_add_to_profile(bell.folder_name()));
    support::click_id(cx, s.win, ids::sound_hit_pick(p.id()));
    toast_is(cx, &s, "Added bell to Comp as its hitsound (replaced ding)");
    let saved = reload(&s, &p);
    assert_eq!(
        sound_id(saved.hitsound()),
        Some(bell.folder_name().to_owned())
    );
    assert_eq!(saved.killsound(), None);
}

#[gpui_kit::test]
fn a_sound_added_to_the_active_profile_is_not_applied(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let bell = seed_sound(&s, &env, "bell");
    let p = profile(&s, ProfileDraft::new("Comp"));
    let tf2 = Tf2Install::from_path(&env.game).unwrap();
    s.services
        .library
        .apply_profile(p.id(), &tf2, &s.services.builtins, false)
        .unwrap();
    open_sounds(cx, &s);
    support::click_id(cx, s.win, ids::sound_add_to_profile(bell.folder_name()));
    support::click_id(cx, s.win, ids::sound_hit_pick(p.id()));
    toast_is(
        cx,
        &s,
        "Added bell to Comp as its hitsound. Changes not applied yet",
    );
}

#[gpui_kit::test]
fn the_sound_menu_says_no_profiles_once(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let bell = seed_sound(&s, &env, "bell");
    open_sounds(cx, &s);
    support::click_id(cx, s.win, ids::sound_add_to_profile(bell.folder_name()));
    assert_eq!(
        support::label_of(cx, s.win, ids::PROFILE_MENU_EMPTY).as_deref(),
        Some("No profiles yet")
    );
    let notes = cx
        .update_window(s.win, |_, w, _| w.find_all(ids::PROFILE_MENU_EMPTY).len())
        .unwrap();
    assert_eq!(notes, 1);
}

#[gpui_kit::test]
fn the_sound_button_fits_inside_its_card(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let bell = seed_sound(&s, &env, "bell");
    open_sounds(cx, &s);
    let folder = bell.folder_name();
    let (card, buttons) = cx
        .update_window(s.win, |_, w, _| {
            let card = w.find(ids::sound_card(folder)).bounds();
            let ids = [ids::sound_add_to_profile(folder), ids::sound_kill(folder)];
            (card, ids.map(|id| w.find(id).bounds()))
        })
        .unwrap();
    for button in buttons {
        assert!(
            button.left() >= card.left() && button.right() <= card.right(),
            "button {button:?} in card {card:?}"
        );
    }
}

fn seed_crosshair(s: &support::ShellWindow, env: &support::Env, name: &str) -> CrosshairEntry {
    let image = RgbaImage::from_pixels(8, 8, [255, 0, 0, 255].repeat(64)).unwrap();
    let path = env.tmp.path().join(format!("{name}.vtf"));
    std::fs::write(&path, rgba_to_vtf(&image).unwrap()).unwrap();
    s.services
        .library
        .import_crosshair(&path, name, Provenance::Local)
        .unwrap()
}

fn open_crosshairs(cx: &mut TestAppContext, s: &support::ShellWindow) {
    support::click_id(cx, s.win, ids::nav(Page::Crosshairs));
    let page = cx
        .update(|cx| s.shell.read(cx).crosshairs_page())
        .expect("a Crosshairs page");
    support::wait_real(cx, "Crosshairs to load", |cx| {
        cx.update(|cx| page.read(cx).is_loaded())
    });
    support::settle(cx, s.win);
}

/// Picks profile `p` from the selection panel's menu and checks the Profiles page opened `p`'s
/// editor on its Crosshairs slot with `crosshair` picked and nothing saved.
fn pick_opens_editor(
    cx: &mut TestAppContext,
    s: &support::ShellWindow,
    p: &Profile,
    crosshair: CrosshairRef,
) {
    support::click_id(cx, s.win, ids::CROSSHAIRS_ADD_TO_PROFILE);
    support::click_id(cx, s.win, ids::crosshair_profile_pick(p.id()));
    assert_eq!(
        cx.update(|cx| s.shell.read(cx).page()),
        Page::Profiles,
        "the pick shows Profiles"
    );
    let page = cx
        .update(|cx| s.shell.read(cx).profiles_page())
        .expect("a Profiles page");
    // The page's first load ends after the editor opened; it must leave the editor as it is.
    support::wait_real(cx, "Profiles to load", |cx| {
        cx.update(|cx| page.read(cx).is_loaded())
    });
    support::settle(cx, s.win);
    let editor = cx.update(|cx| page.read(cx).editor()).expect("the editor");
    let (id, slot, picked) = cx.update(|cx| {
        let e = editor.read(cx);
        (
            e.id().clone(),
            e.selected(),
            e.picker().read(cx).selected().cloned(),
        )
    });
    assert_eq!(&id, p.id(), "the picked profile's editor");
    assert_eq!(slot, Slot::Crosshairs, "on the Crosshairs slot");
    assert_eq!(picked, Some(crosshair), "with the crosshair picked");
    assert_eq!(
        reload(s, p).crosshairs(),
        p.crosshairs(),
        "nothing saved yet"
    );
}

#[gpui_kit::test]
fn a_library_crosshair_opens_the_profile_editor_picked(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let entry = seed_crosshair(&s, &env, "Dot");
    profile(&s, ProfileDraft::new("Alpha"));
    let p = profile(&s, ProfileDraft::new("Comp"));
    open_crosshairs(cx, &s);
    support::click_id(cx, s.win, ids::crosshair_library(entry.folder_name()));
    pick_opens_editor(cx, &s, &p, CrosshairRef::Library(entry.id().clone()));
}

#[gpui_kit::test]
fn a_builtin_crosshair_opens_the_profile_editor_picked(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let p = profile(&s, ProfileDraft::new("Comp"));
    open_crosshairs(cx, &s);
    let first = s.services.builtins.all()[0].clone();
    support::click_id(cx, s.win, ids::crosshair_builtin(first.name()));
    pick_opens_editor(cx, &s, &p, CrosshairRef::Builtin(first.name().to_owned()));
}

#[gpui_kit::test]
fn the_crosshair_menu_says_no_profiles_and_stays(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    open_crosshairs(cx, &s);
    let first = s.services.builtins.all()[0].clone();
    support::click_id(cx, s.win, ids::crosshair_builtin(first.name()));
    support::click_id(cx, s.win, ids::CROSSHAIRS_ADD_TO_PROFILE);
    assert_eq!(
        support::label_of(cx, s.win, ids::PROFILE_MENU_EMPTY).as_deref(),
        Some("No profiles yet")
    );
    support::click_id(cx, s.win, ids::PROFILE_MENU_EMPTY);
    assert_eq!(cx.update(|cx| s.shell.read(cx).page()), Page::Crosshairs);
}
