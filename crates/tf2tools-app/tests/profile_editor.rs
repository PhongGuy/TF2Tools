//! The draft editor's shell: crumb, Cancel, Esc, Save, name errors and Delete. Editing never
//! writes the game folder.

use crate::support;

use gpui_kit::{TestAppContext, px, size};
use support::graphics::found;
use support::profiles::{
    assert_crumb_fits, at_windows_800x600, edit, game_files, page, seed, type_name,
};
use tf2tools_app::ids;
use tf2tools_app::pages::profiles::editor_model::{Mode, SlotKind};
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
fn edit_opens_the_crumb_and_the_name(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let seeded = seed(&s, &env);
    let comp = hud_profile(&s, "Comp", &seeded.rayshud);
    let page = page(cx, &s);
    let before = game_files(&env);
    let e = edit(cx, &s, &page, comp.id());
    assert!(support::present(cx, s.win, ids::HEADER_SLOT));
    assert_eq!(
        label(cx, &s, ids::PROFILE_EDITOR_CRUMB).as_deref(),
        Some("Profiles")
    );
    assert_eq!(
        label(cx, &s, ids::PROFILE_EDITOR_CRUMB_NAME).as_deref(),
        Some("Comp")
    );
    assert_eq!(
        label(cx, &s, ids::PROFILE_EDITOR_CANCEL).as_deref(),
        Some("Cancel")
    );
    assert_eq!(
        label(cx, &s, ids::PROFILE_EDITOR_SAVE).as_deref(),
        Some("Save")
    );
    assert!(
        support::disabled_of(cx, s.win, ids::PROFILE_EDITOR_SAVE),
        "unchanged"
    );
    assert!(!support::present(cx, s.win, ids::PROFILE_EDITOR_NOTE));
    assert!(!support::present(cx, s.win, ids::PROFILE_EDITOR_NAME_ERROR));
    assert_eq!(
        label(cx, &s, ids::PROFILE_EDITOR_DELETE).as_deref(),
        Some("Delete profile")
    );
    assert!(!cx.update(|cx| e.read(cx).is_dirty()));
    assert_eq!(game_files(&env), before);
}

#[gpui_kit::test]
fn a_click_on_the_card_body_opens_the_editor(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let seeded = seed(&s, &env);
    let comp = hud_profile(&s, "Comp", &seeded.rayshud);
    let page = page(cx, &s);
    support::click_id(cx, s.win, ids::profile_card_name(comp.id()));
    support::wait_real(cx, "the editor", |cx| {
        cx.update(|cx| page.read(cx).editor().is_some())
    });
}

#[gpui_kit::test]
fn renaming_and_saving_updates_the_profile_and_returns(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let seeded = seed(&s, &env);
    let comp = hud_profile(&s, "Comp", &seeded.rayshud);
    let page = page(cx, &s);
    let before = game_files(&env);
    let e = edit(cx, &s, &page, comp.id());
    type_name(cx, &s, &e, "Pubs");
    assert_eq!(
        label(cx, &s, ids::PROFILE_EDITOR_CRUMB_NAME).as_deref(),
        Some("Pubs")
    );
    assert!(!support::disabled_of(cx, s.win, ids::PROFILE_EDITOR_SAVE));
    support::click_id(cx, s.win, ids::PROFILE_EDITOR_SAVE);
    support::wait_real(cx, "the list", |cx| {
        cx.update(|cx| page.read(cx).editor().is_none())
    });
    assert!(
        support::toast_labels(cx, s.win)
            .iter()
            .any(|t| t == "Pubs saved."),
        "{:?}",
        support::toast_labels(cx, s.win)
    );
    assert_eq!(
        s.services.library.profile(comp.id()).unwrap().name(),
        "Pubs"
    );
    assert!(!support::present(cx, s.win, ids::HEADER_SLOT));
    assert_eq!(
        support::log_rows(&s.services)[0].message(),
        "Saved profile: Pubs"
    );
    assert_eq!(game_files(&env), before);
}

#[gpui_kit::test]
fn saving_a_slot_change_to_the_active_profile_says_to_apply_it(cx: &mut TestAppContext) {
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
    let e = edit(cx, &s, &page, comp.id());
    cx.update(|cx| e.update(cx, |e, cx| e.set_mode(SlotKind::Hud, Mode::None, cx)));
    support::click_id(cx, s.win, ids::PROFILE_EDITOR_SAVE);
    support::wait_real(cx, "the toast", |cx| {
        support::toast_labels(cx, s.win)
            .iter()
            .any(|t| t == "Comp saved. Apply it to use the changes.")
    });
    assert_eq!(s.services.library.active_profile().unwrap(), None);
}

#[gpui_kit::test]
fn renaming_the_active_profile_keeps_it_active(cx: &mut TestAppContext) {
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
    let e = edit(cx, &s, &page, comp.id());
    type_name(cx, &s, &e, "Comp 2");
    support::click_id(cx, s.win, ids::PROFILE_EDITOR_SAVE);
    support::wait_real(cx, "the toast", |cx| {
        support::toast_labels(cx, s.win)
            .iter()
            .any(|t| t == "Comp 2 saved.")
    });
    assert_eq!(
        s.services.library.active_profile().unwrap(),
        Some(comp.id().clone())
    );
}

#[gpui_kit::test]
fn a_blank_or_taken_name_shows_why_and_blocks_save(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let seeded = seed(&s, &env);
    let comp = hud_profile(&s, "Comp", &seeded.rayshud);
    hud_profile(&s, "Casual", &seeded.budhud);
    let page = page(cx, &s);
    let e = edit(cx, &s, &page, comp.id());
    type_name(cx, &s, &e, "  ");
    assert_eq!(
        label(cx, &s, ids::PROFILE_EDITOR_NAME_ERROR).as_deref(),
        Some("Give this profile a name")
    );
    assert!(support::disabled_of(cx, s.win, ids::PROFILE_EDITOR_SAVE));
    type_name(cx, &s, &e, "casual");
    assert_eq!(
        label(cx, &s, ids::PROFILE_EDITOR_NAME_ERROR).as_deref(),
        Some("You already have a profile called Casual. Pick another name.")
    );
    assert!(support::disabled_of(cx, s.win, ids::PROFILE_EDITOR_SAVE));
}

#[gpui_kit::test]
fn set_with_no_hud_picked_says_pick_one(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let bare = s
        .services
        .library
        .create_profile(&ProfileDraft::new("Bare"))
        .unwrap();
    let page = page(cx, &s);
    let e = edit(cx, &s, &page, bare.id());
    cx.update(|cx| e.update(cx, |e, cx| e.set_mode(SlotKind::Hud, Mode::Set, cx)));
    support::settle(cx, s.win);
    assert_eq!(
        label(cx, &s, ids::PROFILE_EDITOR_NAME_ERROR).as_deref(),
        Some("Pick a HUD, or choose None")
    );
    assert!(support::disabled_of(cx, s.win, ids::PROFILE_EDITOR_SAVE));
}

#[gpui_kit::test]
fn saving_a_profile_deleted_meanwhile_returns_to_the_list(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let seeded = seed(&s, &env);
    let comp = hud_profile(&s, "Comp", &seeded.rayshud);
    let page = page(cx, &s);
    let e = edit(cx, &s, &page, comp.id());
    type_name(cx, &s, &e, "Pubs");
    s.services.library.delete_profile(comp.id()).unwrap();
    support::click_id(cx, s.win, ids::PROFILE_EDITOR_SAVE);
    support::wait_real(cx, "the list", |cx| {
        cx.update(|cx| page.read(cx).editor().is_none())
    });
    assert!(support::present(cx, s.win, ids::TOAST));
    assert!(
        s.services.library.profiles().unwrap().is_empty(),
        "Save made no new profile"
    );
}

#[gpui_kit::test]
fn delete_at_the_foot_confirms_then_returns(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let seeded = seed(&s, &env);
    let comp = hud_profile(&s, "Comp", &seeded.rayshud);
    let page = page(cx, &s);
    edit(cx, &s, &page, comp.id());
    assert_eq!(
        label(cx, &s, ids::PROFILE_EDITOR_DELETE).as_deref(),
        Some("Delete profile")
    );
    support::profiles::click_delete(cx, &s);
    support::wait_real(cx, "the confirm", |cx| {
        support::present(cx, s.win, ids::CONFIRM_OK)
    });
    support::click_id(cx, s.win, ids::CONFIRM_OK);
    support::wait_real(cx, "the list", |cx| {
        cx.update(|cx| page.read(cx).editor().is_none())
    });
    assert!(s.services.library.profile(comp.id()).is_err());
}

#[gpui_kit::test]
fn a_name_taken_at_save_keeps_the_editor_open(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let seeded = seed(&s, &env);
    let comp = hud_profile(&s, "Comp", &seeded.rayshud);
    let page = page(cx, &s);
    let e = edit(cx, &s, &page, comp.id());
    type_name(cx, &s, &e, "Late");
    hud_profile(&s, "Late", &seeded.budhud);
    support::click_id(cx, s.win, ids::PROFILE_EDITOR_SAVE);
    support::wait_real(cx, "the inline error", |cx| {
        support::present(cx, s.win, ids::PROFILE_EDITOR_NAME_ERROR)
    });
    assert!(cx.update(|cx| page.read(cx).editor().is_some()));
    assert_eq!(
        s.services.library.profile(comp.id()).unwrap().name(),
        "Comp"
    );
}

#[gpui_kit::test]
fn cancel_on_a_clean_draft_leaves_at_once(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let seeded = seed(&s, &env);
    let comp = hud_profile(&s, "Comp", &seeded.rayshud);
    let page = page(cx, &s);
    edit(cx, &s, &page, comp.id());
    support::click_id(cx, s.win, ids::PROFILE_EDITOR_CANCEL);
    support::settle(cx, s.win);
    assert!(cx.update(|cx| page.read(cx).editor().is_none()));
    assert!(!support::present(cx, s.win, ids::CONFIRM_TITLE));
    edit(cx, &s, &page, comp.id());
    support::press(cx, s.win, "escape");
    assert!(
        cx.update(|cx| page.read(cx).editor().is_none()),
        "Esc leaves too"
    );
    edit(cx, &s, &page, comp.id());
    support::click_id(cx, s.win, ids::PROFILE_EDITOR_CRUMB);
    support::settle(cx, s.win);
    assert!(
        cx.update(|cx| page.read(cx).editor().is_none()),
        "so does the crumb"
    );
}

#[gpui_kit::test]
fn cancel_on_a_dirty_draft_asks_and_keeps_editing_on_no(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let seeded = seed(&s, &env);
    let comp = hud_profile(&s, "Comp", &seeded.rayshud);
    let page = page(cx, &s);
    let before = game_files(&env);
    let e = edit(cx, &s, &page, comp.id());
    type_name(cx, &s, &e, "Changed");
    support::click_id(cx, s.win, ids::PROFILE_EDITOR_CANCEL);
    support::wait_real(cx, "the discard prompt", |cx| {
        support::present(cx, s.win, ids::CONFIRM_TITLE)
    });
    assert_eq!(
        label(cx, &s, ids::CONFIRM_TITLE).as_deref(),
        Some("Unsaved changes")
    );
    support::click_id(cx, s.win, ids::CONFIRM_CANCEL);
    support::settle(cx, s.win);
    assert!(cx.update(|cx| page.read(cx).editor().is_some()));
    support::press(cx, s.win, "escape");
    support::wait_real(cx, "the prompt again", |cx| {
        support::present(cx, s.win, ids::CONFIRM_OK)
    });
    support::click_id(cx, s.win, ids::CONFIRM_OK);
    support::settle(cx, s.win);
    assert!(cx.update(|cx| page.read(cx).editor().is_none()));
    assert_eq!(
        s.services.library.profile(comp.id()).unwrap().name(),
        "Comp"
    );
    assert_eq!(game_files(&env), before);
}

#[gpui_kit::test]
fn at_800x600_the_crumb_fits_the_header(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let seeded = seed(&s, &env);
    let comp = hud_profile(
        &s,
        "A rather long profile name for the header",
        &seeded.rayshud,
    );
    cx.simulate_window_resize(s.win, size(px(800.), px(600.)));
    let page = page(cx, &s);
    edit(cx, &s, &page, comp.id());
    assert_crumb_fits(cx, &s);
}

// Windows' title bar holds the window controls, which leave the slot 190 px at 800x600; there
// the name gives way first and then "Profiles /".
#[gpui_kit::test]
fn in_a_header_narrowed_by_window_controls_cancel_and_save_still_fit(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let seeded = seed(&s, &env);
    let comp = hud_profile(
        &s,
        "A rather long profile name for the header",
        &seeded.rayshud,
    );
    at_windows_800x600(cx, &s);
    let page = page(cx, &s);
    edit(cx, &s, &page, comp.id());
    let slot = support::bounds_of(cx, s.win, ids::HEADER_SLOT).unwrap();
    assert!(
        (slot.size.width - px(190.)).abs() <= px(2.),
        "the slot is Windows' at 800x600: {slot:?}"
    );
    assert_crumb_fits(cx, &s);
    assert!(
        !support::present(cx, s.win, ids::PROFILE_EDITOR_CRUMB),
        "\"Profiles /\" gives way once the name has none left to give"
    );
}
#[gpui_kit::test]
fn a_short_name_in_a_wide_header_keeps_the_profiles_link(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let seeded = seed(&s, &env);
    let comp = hud_profile(&s, "Comp", &seeded.rayshud);
    let page = page(cx, &s);
    edit(cx, &s, &page, comp.id());
    assert!(support::present(cx, s.win, ids::PROFILE_EDITOR_CRUMB));
    assert_eq!(
        label(cx, &s, ids::PROFILE_EDITOR_CRUMB_NAME).as_deref(),
        Some("Comp")
    );
}
