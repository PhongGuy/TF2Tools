//! New profile's unsaved draft, Delete's active note and cancel, and the sidebar guard.

use crate::support;

use gpui_kit::TestAppContext;
use support::graphics::found;
use support::profiles::{edit, game_files, new_draft, page, seed, type_name};
use tf2tools_app::ids;
use tf2tools_app::shell::route::Page;
use tf2tools_core::{DraftItem, ProfileDraft, SlotMode};

fn hud_profile(
    s: &support::ShellWindow,
    name: &str,
    hud: &tf2tools_core::HudEntry,
) -> tf2tools_core::Profile {
    let draft = ProfileDraft::new(name).with_hud(SlotMode::Set(DraftItem::Pick(hud.id().clone())));
    s.services.library.create_profile(&draft).unwrap()
}

fn label(cx: &mut TestAppContext, s: &support::ShellWindow, id: &'static str) -> Option<String> {
    support::label_of(cx, s.win, id)
}

#[gpui_kit::test]
fn new_profile_opens_a_draft_of_what_is_installed(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let seeded = seed(&s, &env);
    let base = hud_profile(&s, "Base", &seeded.rayshud);
    let tf2 = tf2tools_core::Tf2Install::from_path(&env.game).unwrap();
    s.services
        .library
        .apply_profile(base.id(), &tf2, &s.services.builtins, false)
        .unwrap();
    let page = page(cx, &s);
    let before = game_files(&env);
    let e = new_draft(cx, &s, &page);
    assert_eq!(
        label(cx, &s, ids::PROFILE_EDITOR_CRUMB_NAME).as_deref(),
        Some("New profile")
    );
    let note = label(cx, &s, ids::PROFILE_EDITOR_NOTE).unwrap();
    assert!(
        note.starts_with("Filled in from what you have now"),
        "{note}"
    );
    assert!(note.contains("rayshud"), "{note}");
    assert!(!support::present(cx, s.win, ids::PROFILE_EDITOR_DELETE));
    assert!(
        support::disabled_of(cx, s.win, ids::PROFILE_EDITOR_SAVE),
        "the name is blank"
    );
    type_name(cx, &s, &e, "Copy");
    support::click_id(cx, s.win, ids::PROFILE_EDITOR_SAVE);
    support::wait_real(cx, "the list", |cx| {
        cx.update(|cx| page.read(cx).editor().is_none())
    });
    let made = s.services.library.profiles().unwrap();
    let copy = made.iter().find(|p| p.name() == "Copy").expect("saved");
    assert_eq!(
        copy.hud().as_set().map(|h| h.folder_name()),
        Some(seeded.rayshud.folder_name())
    );
    assert_eq!(game_files(&env), before);
}

#[gpui_kit::test]
fn new_profile_toasts_when_the_setup_cannot_be_read(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let custom = env.game.join("tf/custom");
    std::fs::remove_dir_all(&custom).unwrap();
    std::fs::write(&custom, b"not a folder").unwrap();
    let page = page(cx, &s);
    support::click_id(cx, s.win, ids::PROFILES_NEW);
    support::wait_real(cx, "the error toast", |cx| {
        support::present(cx, s.win, ids::TOAST)
    });
    assert!(cx.update(|cx| page.read(cx).editor().is_none()));
    assert!(!cx.update(|cx| page.read(cx).is_opening()));
}

#[gpui_kit::test]
fn delete_at_the_foot_says_when_the_profile_is_active(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let seeded = seed(&s, &env);
    let comp = hud_profile(&s, "Comp", &seeded.rayshud);
    let tf2 = tf2tools_core::Tf2Install::from_path(&env.game).unwrap();
    s.services
        .library
        .apply_profile(comp.id(), &tf2, &s.services.builtins, false)
        .unwrap();
    let page = page(cx, &s);
    let before = game_files(&env);
    edit(cx, &s, &page, comp.id());
    support::click_id(cx, s.win, ids::PROFILE_EDITOR_DELETE);
    support::wait_real(cx, "the confirm", |cx| {
        support::present(cx, s.win, ids::CONFIRM_DESCRIPTION)
    });
    let body = label(cx, &s, ids::CONFIRM_DESCRIPTION).unwrap();
    assert!(
        body.contains("It's your active profile. Your game stays as it is."),
        "{body}"
    );
    support::click_id(cx, s.win, ids::CONFIRM_OK);
    support::wait_real(cx, "the list", |cx| {
        cx.update(|cx| page.read(cx).editor().is_none())
    });
    assert!(s.services.library.profile(comp.id()).is_err());
    assert_eq!(s.services.library.active_profile().unwrap(), None);
    assert_eq!(game_files(&env), before);
}

#[gpui_kit::test]
fn cancelling_delete_stays_in_the_editor(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let seeded = seed(&s, &env);
    let comp = hud_profile(&s, "Comp", &seeded.rayshud);
    let page = page(cx, &s);
    edit(cx, &s, &page, comp.id());
    support::click_id(cx, s.win, ids::PROFILE_EDITOR_DELETE);
    support::wait_real(cx, "the confirm", |cx| {
        support::present(cx, s.win, ids::CONFIRM_CANCEL)
    });
    support::click_id(cx, s.win, ids::CONFIRM_CANCEL);
    support::settle(cx, s.win);
    assert!(cx.update(|cx| page.read(cx).editor().is_some()));
    assert!(s.services.library.profile(comp.id()).is_ok());
}

#[gpui_kit::test]
fn leaving_with_a_dirty_draft_asks_first(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let seeded = seed(&s, &env);
    let comp = hud_profile(&s, "Comp", &seeded.rayshud);
    let page = page(cx, &s);
    let e = edit(cx, &s, &page, comp.id());
    type_name(cx, &s, &e, "Changed");
    support::click_id(cx, s.win, ids::nav(Page::Huds));
    support::wait_real(cx, "the discard prompt", |cx| {
        support::present(cx, s.win, ids::CONFIRM_CANCEL)
    });
    support::click_id(cx, s.win, ids::CONFIRM_CANCEL);
    support::settle(cx, s.win);
    assert_eq!(cx.update(|cx| s.shell.read(cx).page()), Page::Profiles);
    support::click_id(cx, s.win, ids::PROFILE_EDITOR_CANCEL);
    support::wait_real(cx, "the discard prompt", |cx| {
        support::present(cx, s.win, ids::CONFIRM_OK)
    });
    support::click_id(cx, s.win, ids::CONFIRM_OK);
    support::settle(cx, s.win);
    support::click_id(cx, s.win, ids::nav(Page::Huds));
    support::settle(cx, s.win);
    assert!(
        !support::present(cx, s.win, ids::CONFIRM_TITLE),
        "a closed editor no longer asks"
    );
    assert_eq!(cx.update(|cx| s.shell.read(cx).page()), Page::Huds);
}
