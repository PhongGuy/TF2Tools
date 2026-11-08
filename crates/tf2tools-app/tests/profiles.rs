use crate::support;

use std::cell::Cell;
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use gpui_kit::component::WindowExt as _;
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{AppContext as _, ElementId, Entity, TestAppContext, px};
use support::scroll::{assert_bar_beside, assert_clear_of_bar, assert_track_scrolls};
use tf2tools_app::ids;
use tf2tools_app::library_events::{LibraryChanged, LibraryEvents};
use tf2tools_app::pages::profiles::ProfilesPage;
use tf2tools_app::pages::profiles::probe::Tf2Probe;
use tf2tools_app::pages::profiles::wizard::WizardMode;
use tf2tools_app::services::Tf2State;
use tf2tools_app::shell::route::Page;
use tf2tools_core::{
    CrosshairRef, DraftCrosshair, DraftSlot, HudEntry, Profile, ProfileDraft, Provenance,
    SoundEntry, Tf2Install,
};
use tf2tools_core::{GbId, LibraryKind};
use tf2tools_core::{LogEntry, LogLevel};

fn tf2(env: &support::Env) -> Tf2Install {
    Tf2Install::from_path(&env.game).unwrap()
}

fn found(env: &support::Env) -> Tf2State {
    Tf2State::Found(tf2(env))
}

/// 441 frames of 44.1 kHz stereo 16-bit silence, a file `import_sound` accepts.
fn wav() -> Vec<u8> {
    let data_len: u32 = 441 * 4;
    let mut out = Vec::new();
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data_len).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&2u16.to_le_bytes());
    out.extend_from_slice(&44_100u32.to_le_bytes());
    out.extend_from_slice(&(44_100u32 * 4).to_le_bytes());
    out.extend_from_slice(&4u16.to_le_bytes());
    out.extend_from_slice(&16u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_len.to_le_bytes());
    out.resize(out.len() + data_len as usize, 0);
    out
}

fn seed_hud(
    s: &support::ShellWindow,
    env: &support::Env,
    name: &str,
    provenance: Provenance,
) -> HudEntry {
    let path = env.tmp.path().join(format!("{name}.zip"));
    std::fs::write(&path, support::hud_zip_bytes(name)).unwrap();
    s.services.library.import_hud(&path, provenance).unwrap()
}

fn seed_sound(s: &support::ShellWindow, env: &support::Env, name: &str) -> SoundEntry {
    let path = env
        .tmp
        .path()
        .join(format!("{}.wav", name.replace(' ', "-")));
    std::fs::write(&path, wav()).unwrap();
    s.services
        .library
        .import_sound(&path, name, Provenance::Local)
        .unwrap()
}

/// "rayshud", "Classic Ding" as hitsound, "Bell" as killsound, the first builtin on the scattergun.
fn full_profile(s: &support::ShellWindow, env: &support::Env, name: &str) -> Profile {
    let hud = seed_hud(s, env, "rayshud", Provenance::Local);
    let hit = seed_sound(s, env, "Classic Ding");
    let kill = seed_sound(s, env, "Bell");
    let first = s.services.builtins.all().first().unwrap().name().to_owned();
    let draft = ProfileDraft::new(name)
        .with_hud(DraftSlot::Item(hud.id().clone()))
        .with_hitsound(DraftSlot::Item(hit.id().clone()))
        .with_killsound(DraftSlot::Item(kill.id().clone()))
        .with_crosshairs(vec![DraftCrosshair::Set {
            weapon_script: "tf_weapon_scattergun".into(),
            crosshair: CrosshairRef::Builtin(first),
        }]);
    s.services.library.create_profile(&draft).unwrap()
}

fn empty_profile(s: &support::ShellWindow, name: &str) -> Profile {
    s.services
        .library
        .create_profile(&ProfileDraft::new(name))
        .unwrap()
}

/// Pins the TF2 probe to "not running", opens Profiles from the sidebar, waits for the load.
fn open_profiles(cx: &mut TestAppContext, s: &support::ShellWindow) -> Entity<ProfilesPage> {
    cx.update(|cx| Tf2Probe::install(cx, || false));
    support::click_id(cx, s.win, ids::nav(Page::Profiles));
    let page = cx
        .update(|cx| s.shell.read(cx).profiles_page())
        .expect("a Profiles page");
    support::wait_real(cx, "Profiles to load", |cx| {
        cx.update(|cx| page.read(cx).is_loaded())
    });
    support::settle(cx, s.win);
    page
}

fn search(
    cx: &mut TestAppContext,
    s: &support::ShellWindow,
    page: &Entity<ProfilesPage>,
    text: &str,
) {
    let input = cx.update(|cx| page.read(cx).search_input().clone());
    cx.update_window(s.win, |_, window, cx| {
        input.update(cx, |st, cx| st.set_value(text, window, cx))
    })
    .unwrap();
    support::settle(cx, s.win);
}

fn has_dialog(cx: &mut TestAppContext, s: &support::ShellWindow) -> bool {
    support::settle(cx, s.win);
    cx.update_window(s.win, |_, w, cx| w.has_active_dialog(cx))
        .unwrap()
}

#[gpui_kit::test]
fn profiles_is_a_real_page(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    assert!(!Page::Profiles.is_placeholder());
    let _page = open_profiles(cx, &s);
    assert!(support::present(cx, s.win, ids::page(Page::Profiles)));
    assert!(!support::present(cx, s.win, ids::PLACEHOLDER_MESSAGE));
    support::click_id(cx, s.win, ids::nav(Page::Huds));
    assert!(cx.update(|cx| s.shell.read(cx).profiles_page()).is_none());
}

#[gpui_kit::test]
fn no_profiles_shows_v5_s_empty_states(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let _page = open_profiles(cx, &s);
    assert_eq!(
        support::label_of(cx, s.win, ids::PROFILES_NO_ACTIVE).as_deref(),
        Some("No active profile")
    );
    assert_eq!(
        support::label_of(cx, s.win, ids::PROFILES_EMPTY).as_deref(),
        Some("No profiles yet")
    );
    assert!(support::present(cx, s.win, ids::PROFILES_EMPTY_SAVE));
    assert!(support::present(cx, s.win, ids::PROFILES_EMPTY_CREATE));
    assert!(support::present(cx, s.win, ids::PROFILES_SEARCH));
    assert!(!support::present(cx, s.win, ids::PROFILES_NO_TF2));
}

#[gpui_kit::test]
fn cards_show_the_name_and_item_badges(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let comp = full_profile(&s, &env, "Competitive");
    let vanilla = empty_profile(&s, "Vanilla");
    let page = open_profiles(cx, &s);
    assert_eq!(cx.update(|cx| page.read(cx).profiles().len()), 2);
    let id = comp.id();
    assert_eq!(
        support::label_of(cx, s.win, ids::profile_card(id)).as_deref(),
        Some("Competitive")
    );
    assert_eq!(
        support::label_of(cx, s.win, ids::profile_hud(id)).as_deref(),
        Some("rayshud")
    );
    assert_eq!(
        support::label_of(cx, s.win, ids::profile_hitsound(id)).as_deref(),
        Some("Classic Ding")
    );
    assert_eq!(
        support::label_of(cx, s.win, ids::profile_killsound(id)).as_deref(),
        Some("Bell")
    );
    assert_eq!(
        support::label_of(cx, s.win, ids::profile_crosshairs(id)).as_deref(),
        Some("1 crosshair")
    );
    assert!(!support::present(cx, s.win, ids::profile_empty(id)));
    assert_eq!(
        support::label_of(cx, s.win, ids::profile_empty(vanilla.id())).as_deref(),
        Some("Empty profile")
    );
    assert_eq!(
        support::label_of(cx, s.win, ids::profile_activate(id)).as_deref(),
        Some("Activate")
    );
    assert!(support::present(cx, s.win, ids::profile_edit(id)));
    assert!(support::present(cx, s.win, ids::profile_delete(id)));
    assert!(!support::present(cx, s.win, ids::profile_active_badge(id)));
}

#[gpui_kit::test]
fn the_active_profile_heads_the_page(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let comp = full_profile(&s, &env, "Competitive");
    let vanilla = empty_profile(&s, "Vanilla");
    s.services
        .library
        .apply_profile(comp.id(), &tf2(&env), &s.services.builtins, false)
        .unwrap();
    let page = open_profiles(cx, &s);
    assert_eq!(
        cx.update(|cx| page.read(cx).active().cloned()),
        Some(comp.id().clone())
    );
    assert_eq!(
        support::label_of(cx, s.win, ids::PROFILES_ACTIVE_NAME).as_deref(),
        Some("Competitive")
    );
    assert!(support::present(cx, s.win, ids::PROFILES_ACTIVE_BADGE));
    assert_eq!(
        support::label_of(cx, s.win, ids::PROFILES_ACTIVE_HUD).as_deref(),
        Some("rayshud")
    );
    assert_eq!(
        support::label_of(cx, s.win, ids::PROFILES_ACTIVE_CROSSHAIRS).as_deref(),
        Some("1 crosshair")
    );
    assert!(!support::present(cx, s.win, ids::PROFILES_NO_ACTIVE));
    assert!(support::present(
        cx,
        s.win,
        ids::profile_active_badge(comp.id())
    ));
    assert_eq!(
        support::label_of(cx, s.win, ids::profile_activate(comp.id())).as_deref(),
        Some("Active")
    );
    assert_eq!(
        support::label_of(cx, s.win, ids::profile_activate(vanilla.id())).as_deref(),
        Some("Activate")
    );
}

#[gpui_kit::test]
fn an_empty_active_profile_reads_vanilla(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let vanilla = empty_profile(&s, "Vanilla");
    s.services
        .library
        .apply_profile(vanilla.id(), &tf2(&env), &s.services.builtins, false)
        .unwrap();
    let _page = open_profiles(cx, &s);
    assert_eq!(
        support::label_of(cx, s.win, ids::PROFILES_ACTIVE_EMPTY).as_deref(),
        Some("Empty profile (vanilla)")
    );
    assert!(!support::present(cx, s.win, ids::PROFILES_ACTIVE_HUD));
}

#[gpui_kit::test]
fn search_filters_the_cards(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let comp = empty_profile(&s, "Competitive");
    let casual = empty_profile(&s, "Casual");
    let page = open_profiles(cx, &s);
    search(cx, &s, &page, "COMP");
    assert!(support::present(cx, s.win, ids::profile_card(comp.id())));
    assert!(!support::present(cx, s.win, ids::profile_card(casual.id())));
    search(cx, &s, &page, "zzz");
    assert_eq!(
        support::label_of(cx, s.win, ids::PROFILES_NO_MATCH).as_deref(),
        Some("No profiles found")
    );
    assert!(!support::present(cx, s.win, ids::PROFILES_EMPTY));
    search(cx, &s, &page, "");
    assert!(support::present(cx, s.win, ids::profile_card(casual.id())));
}

#[gpui_kit::test]
fn without_tf2_the_page_warns(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, Tf2State::Missing { saved: None });
    let _comp = empty_profile(&s, "Competitive");
    let _page = open_profiles(cx, &s);
    assert!(support::present(cx, s.win, ids::PROFILES_NO_TF2));
}

#[gpui_kit::test]
fn a_library_change_reloads_the_page(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let _page = open_profiles(cx, &s);
    assert!(support::present(cx, s.win, ids::PROFILES_EMPTY));
    let comp = empty_profile(&s, "Competitive");
    cx.update(LibraryEvents::changed);
    support::wait_real(cx, "the reload", |cx| {
        support::present(cx, s.win, ids::profile_card(comp.id()))
    });
}

fn row(e: &LogEntry) -> (&str, &str, &str) {
    (e.category(), e.action(), e.message())
}

fn toast_is(cx: &mut TestAppContext, s: &support::ShellWindow, text: &str) {
    support::wait_real(cx, text, |cx| {
        support::label_of(cx, s.win, ids::TOAST).as_deref() == Some(text)
    });
}

fn type_name(
    cx: &mut TestAppContext,
    s: &support::ShellWindow,
    page: &Entity<ProfilesPage>,
    text: &str,
) {
    let input = cx.update(|cx| page.read(cx).save_name_input().clone());
    cx.update_window(s.win, |_, window, cx| {
        input.update(cx, |st, cx| st.set_value(text, window, cx))
    })
    .unwrap();
    support::settle(cx, s.win);
}

#[gpui_kit::test]
fn save_current_saves_the_installed_setup(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let hud = seed_hud(&s, &env, "rayshud", Provenance::Local);
    s.services
        .library
        .install_hud(hud.id(), &tf2(&env))
        .unwrap();
    let page = open_profiles(cx, &s);
    support::click_id(cx, s.win, ids::PROFILES_SAVE_CURRENT);
    assert!(support::present(cx, s.win, ids::PROFILES_SAVE_DIALOG));
    type_name(cx, &s, &page, "  Comp  ");
    support::click_id(cx, s.win, ids::PROFILES_SAVE_OK);
    toast_is(cx, &s, "Profile saved. \"Comp\" created from current setup");
    assert!(!has_dialog(cx, &s));
    let profiles = s.services.library.profiles().unwrap();
    assert_eq!(profiles.len(), 1);
    assert_eq!(profiles[0].name(), "Comp");
    assert_eq!(profiles[0].hud().map(|h| h.folder_name()), Some("rayshud"));
    support::wait_real(cx, "the card", |cx| {
        support::present(cx, s.win, ids::profile_card(profiles[0].id()))
    });
    let rows = support::log_rows(&s.services);
    assert_eq!(rows.len(), 1);
    assert_eq!(
        row(&rows[0]),
        (
            "profile",
            "save_current",
            "Saved current setup as profile: Comp"
        )
    );
}

#[gpui_kit::test]
fn the_empty_state_button_opens_save_current(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let _page = open_profiles(cx, &s);
    support::click_id(cx, s.win, ids::PROFILES_EMPTY_SAVE);
    assert!(support::present(cx, s.win, ids::PROFILES_SAVE_DIALOG));
    assert!(support::present(cx, s.win, ids::PROFILES_SAVE_NAME));
}

#[gpui_kit::test]
fn the_save_dialog_carries_v5_s_description(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let _page = open_profiles(cx, &s);
    support::click_id(cx, s.win, ids::PROFILES_SAVE_CURRENT);
    assert_eq!(ids::PROFILES_SAVE_DESCRIPTION, "profiles-save-description");
    assert_eq!(
        support::label_of(cx, s.win, ids::PROFILES_SAVE_DESCRIPTION).as_deref(),
        Some(
            "Save your currently installed HUD, sounds, and crosshair assignments as a reusable \
             profile."
        )
    );
}

// The name field takes the keyboard as the dialog opens, so the player can type at once.
#[gpui_kit::test]
fn the_save_dialog_focuses_its_name_field(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let page = open_profiles(cx, &s);
    support::click_id(cx, s.win, ids::PROFILES_SAVE_CURRENT);
    support::settle(cx, s.win);
    let input = cx.update(|cx| page.read(cx).save_name_input().clone());
    let focused = cx
        .update_window(s.win, |_, w, cx| {
            use gpui_kit::Focusable as _;
            input.read(cx).focus_handle(cx).is_focused(w)
        })
        .unwrap();
    assert!(focused, "the name field has the keyboard");
}

#[gpui_kit::test]
fn a_blank_name_cannot_be_saved(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let page = open_profiles(cx, &s);
    support::click_id(cx, s.win, ids::PROFILES_SAVE_CURRENT);
    support::click_id(cx, s.win, ids::PROFILES_SAVE_OK);
    type_name(cx, &s, &page, "   ");
    support::click_id(cx, s.win, ids::PROFILES_SAVE_OK);
    support::settle(cx, s.win);
    assert!(support::present(cx, s.win, ids::PROFILES_SAVE_DIALOG));
    assert!(s.services.library.profiles().unwrap().is_empty());
    support::click_id(cx, s.win, ids::PROFILES_SAVE_CANCEL);
    assert!(!has_dialog(cx, &s));
    assert!(s.services.library.profiles().unwrap().is_empty());
    assert!(support::log_rows(&s.services).is_empty());
}

// A name another profile has, in any case, keeps the dialog open and says why.
#[gpui_kit::test]
fn a_taken_name_cannot_be_saved(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    empty_profile(&s, "Competitive");
    let page = open_profiles(cx, &s);
    support::click_id(cx, s.win, ids::PROFILES_SAVE_CURRENT);
    type_name(cx, &s, &page, " competitive ");
    assert_eq!(
        support::label_of(cx, s.win, ids::PROFILES_SAVE_TAKEN).as_deref(),
        Some("You already have a profile called Competitive. Pick another name.")
    );
    support::click_id(cx, s.win, ids::PROFILES_SAVE_OK);
    support::settle(cx, s.win);
    assert!(support::present(cx, s.win, ids::PROFILES_SAVE_DIALOG));
    assert_eq!(s.services.library.profiles().unwrap().len(), 1);
    type_name(cx, &s, &page, "Competitive 2");
    assert!(!support::present(cx, s.win, ids::PROFILES_SAVE_TAKEN));
}

#[gpui_kit::test]
fn save_current_rests_without_tf2(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, Tf2State::Missing { saved: None });
    let _page = open_profiles(cx, &s);
    support::click_id(cx, s.win, ids::PROFILES_SAVE_CURRENT);
    assert!(!has_dialog(cx, &s));
    support::click_id(cx, s.win, ids::PROFILES_EMPTY_SAVE);
    assert!(!has_dialog(cx, &s));
}

#[gpui_kit::test]
fn delete_asks_then_removes_and_writes_a_row(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let comp = empty_profile(&s, "Competitive");
    let _page = open_profiles(cx, &s);
    support::click_id(cx, s.win, ids::profile_delete(comp.id()));
    assert_eq!(
        support::label_of(cx, s.win, ids::CONFIRM_DESCRIPTION).as_deref(),
        Some("Are you sure you want to delete \"Competitive\"? This cannot be undone.")
    );
    support::click_id(cx, s.win, ids::CONFIRM_OK);
    toast_is(cx, &s, "Profile deleted");
    assert!(s.services.library.profiles().unwrap().is_empty());
    support::wait_real(cx, "the reload", |cx| {
        support::present(cx, s.win, ids::PROFILES_EMPTY)
    });
    let rows = support::log_rows(&s.services);
    assert_eq!(
        row(&rows[0]),
        ("profile", "delete", "Deleted profile: Competitive")
    );
    assert_eq!(rows[0].details(), None);
}

#[gpui_kit::test]
fn cancelling_delete_keeps_the_profile(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let comp = empty_profile(&s, "Competitive");
    let _page = open_profiles(cx, &s);
    support::click_id(cx, s.win, ids::profile_delete(comp.id()));
    support::click_id(cx, s.win, ids::CONFIRM_CANCEL);
    assert_eq!(s.services.library.profiles().unwrap().len(), 1);
    assert!(support::log_rows(&s.services).is_empty());
}

#[gpui_kit::test]
fn deleting_the_active_profile_clears_the_active_card(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, Tf2State::Missing { saved: None });
    let vanilla = empty_profile(&s, "Vanilla");
    s.services
        .library
        .apply_profile(vanilla.id(), &tf2(&env), &s.services.builtins, false)
        .unwrap();
    let _page = open_profiles(cx, &s);
    assert!(support::present(cx, s.win, ids::PROFILES_ACTIVE));
    support::click_id(cx, s.win, ids::profile_delete(vanilla.id()));
    support::click_id(cx, s.win, ids::CONFIRM_OK);
    toast_is(cx, &s, "Profile deleted");
    support::wait_real(cx, "the reload", |cx| {
        support::present(cx, s.win, ids::PROFILES_NO_ACTIVE)
    });
}

/// Counts `LibraryChanged` from now on; keep the subscription alive.
fn count_changes(cx: &mut TestAppContext) -> (Rc<Cell<usize>>, gpui_kit::Subscription) {
    let count = Rc::new(Cell::new(0));
    let seen = Rc::clone(&count);
    let sub = cx.update(|cx| {
        let events = LibraryEvents::global(cx);
        cx.subscribe(&events, move |_, _: &LibraryChanged, _| {
            seen.set(seen.get() + 1)
        })
    });
    (count, sub)
}

#[gpui_kit::test]
fn activate_applies_everything_and_marks_it_active(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let comp = full_profile(&s, &env, "Comp");
    let _page = open_profiles(cx, &s);
    let (changes, _sub) = count_changes(cx);
    support::click_id(cx, s.win, ids::profile_activate(comp.id()));
    toast_is(
        cx,
        &s,
        "Profile \"Comp\" activated. Installed: HUD, Hitsound, Killsound, Crosshairs",
    );
    assert!(env.game.join("tf/custom/rayshud").is_dir());
    assert_eq!(
        s.services.library.active_profile().unwrap(),
        Some(comp.id().clone())
    );
    support::wait_real(cx, "LibraryChanged", |_| changes.get() >= 1);
    assert_eq!(changes.get(), 1);
    support::wait_real(cx, "the active card", |cx| {
        support::label_of(cx, s.win, ids::PROFILES_ACTIVE_NAME).as_deref() == Some("Comp")
    });
    assert_eq!(
        support::label_of(cx, s.win, ids::profile_activate(comp.id())).as_deref(),
        Some("Active")
    );
    let rows = support::log_rows(&s.services);
    assert_eq!(rows.len(), 1);
    assert_eq!(
        row(&rows[0]),
        ("profile", "activate", "Activated profile: Comp")
    );
    assert_eq!(rows[0].level(), LogLevel::Info);
    assert_eq!(
        rows[0].details(),
        Some("Applied: HUD, Hitsound, Killsound, Crosshairs")
    );
}

#[gpui_kit::test]
fn an_empty_profile_confirms_first(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let vanilla = empty_profile(&s, "Vanilla");
    let page = open_profiles(cx, &s);
    support::click_id(cx, s.win, ids::profile_activate(vanilla.id()));
    assert_eq!(
        support::label_of(cx, s.win, ids::CONFIRM_TITLE).as_deref(),
        Some("Activate Empty Profile")
    );
    support::click_id(cx, s.win, ids::CONFIRM_CANCEL);
    support::settle(cx, s.win);
    assert_eq!(s.services.library.active_profile().unwrap(), None);
    assert!(support::log_rows(&s.services).is_empty());
    assert!(cx.update(|cx| page.read(cx).activating().is_none()));
    support::click_id(cx, s.win, ids::profile_activate(vanilla.id()));
    support::click_id(cx, s.win, ids::CONFIRM_OK);
    toast_is(
        cx,
        &s,
        "Profile \"Vanilla\" activated. All customizations cleared",
    );
    assert_eq!(
        s.services.library.active_profile().unwrap(),
        Some(vanilla.id().clone())
    );
}

#[gpui_kit::test]
fn a_running_tf2_warns_before_applying(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let comp = full_profile(&s, &env, "Comp");
    let page = open_profiles(cx, &s);
    cx.update(|cx| Tf2Probe::install(cx, || true));
    support::click_id(cx, s.win, ids::profile_activate(comp.id()));
    support::wait_real(cx, "the TF2 warning", |cx| {
        support::label_of(cx, s.win, ids::CONFIRM_DESCRIPTION).as_deref()
            == Some("TF2 is running; changes take effect after a restart.")
    });
    support::click_id(cx, s.win, ids::CONFIRM_CANCEL);
    support::settle(cx, s.win);
    assert!(!env.game.join("tf/custom/rayshud").exists());
    assert!(cx.update(|cx| page.read(cx).activating().is_none()));
    support::click_id(cx, s.win, ids::profile_activate(comp.id()));
    support::wait_real(cx, "the TF2 warning", |cx| {
        support::present(cx, s.win, ids::CONFIRM_OK)
    });
    support::click_id(cx, s.win, ids::CONFIRM_OK);
    toast_is(
        cx,
        &s,
        "Profile \"Comp\" activated. Installed: HUD, Hitsound, Killsound, Crosshairs",
    );
    assert!(env.game.join("tf/custom/rayshud").is_dir());
}

// The test scheduler flags foreground runnables as the main thread, so the probe records
// where it ran.
#[gpui_kit::test]
fn the_tf2_probe_runs_off_the_ui_thread(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let comp = full_profile(&s, &env, "Comp");
    let _page = open_profiles(cx, &s);
    let calls = Arc::new(Mutex::new(Vec::<bool>::new()));
    let (log, executor) = (Arc::clone(&calls), cx.executor());
    cx.update(|cx| {
        Tf2Probe::install(cx, move || {
            log.lock().unwrap().push(executor.is_main_thread());
            false
        })
    });
    support::click_id(cx, s.win, ids::profile_activate(comp.id()));
    toast_is(
        cx,
        &s,
        "Profile \"Comp\" activated. Installed: HUD, Hitsound, Killsound, Crosshairs",
    );
    assert_eq!(
        *calls.lock().unwrap(),
        [false],
        "the probe ran once, off the UI thread"
    );
}

#[gpui_kit::test]
fn a_second_activate_while_one_runs_does_nothing(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let comp = full_profile(&s, &env, "Comp");
    let other = s
        .services
        .library
        .create_profile(&ProfileDraft::new("Other").with_hud(DraftSlot::Item(
            s.services.library.list_huds().unwrap()[0].id().clone(),
        )))
        .unwrap();
    let page = open_profiles(cx, &s);
    let (a, b) = (comp.id().clone(), other.id().clone());
    cx.update_window(s.win, |_, window, cx| {
        page.update(cx, |p, cx| {
            p.activate(a.clone(), window, cx);
            p.activate(b.clone(), window, cx);
        })
    })
    .unwrap();
    assert_eq!(
        cx.update(|cx| page.read(cx).activating().cloned()),
        Some(a.clone())
    );
    toast_is(
        cx,
        &s,
        "Profile \"Comp\" activated. Installed: HUD, Hitsound, Killsound, Crosshairs",
    );
    support::wait_real(cx, "idle", |cx| {
        cx.update(|cx| page.read(cx).activating().is_none())
    });
    assert_eq!(s.services.library.active_profile().unwrap(), Some(a));
    assert_eq!(support::log_rows(&s.services).len(), 1);
}

#[cfg(unix)]
#[gpui_kit::test]
fn a_failed_step_toasts_the_steps_and_leaves_nothing_active(cx: &mut TestAppContext) {
    use std::os::unix::fs::PermissionsExt as _;
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let comp = full_profile(&s, &env, "Comp");
    let _page = open_profiles(cx, &s);
    let custom = env.game.join("tf/custom");
    // Restores the mode even when an assertion fails, so the tempdir can be removed.
    struct Writable(std::path::PathBuf);
    impl Drop for Writable {
        fn drop(&mut self) {
            let _ = std::fs::set_permissions(&self.0, std::fs::Permissions::from_mode(0o755));
        }
    }
    let _restore = Writable(custom.clone());
    std::fs::set_permissions(&custom, std::fs::Permissions::from_mode(0o555)).unwrap();
    // Root ignores the mode, so no step could fail; nothing to test then.
    if std::fs::write(custom.join("probe"), b"").is_ok() {
        return;
    }
    let (changes, _sub) = count_changes(cx);
    support::click_id(cx, s.win, ids::profile_activate(comp.id()));
    support::wait_real(cx, "the failure toast", |cx| {
        support::label_of(cx, s.win, ids::TOAST).is_some_and(|t| {
            t.starts_with("Failed to activate profile. Couldn't put these in place: HUD")
        })
    });
    std::fs::set_permissions(&custom, std::fs::Permissions::from_mode(0o755)).unwrap();
    assert_eq!(s.services.library.active_profile().unwrap(), None);
    support::wait_real(cx, "LibraryChanged", |_| changes.get() >= 1);
    assert_eq!(changes.get(), 1);
    support::wait_real(cx, "the reload", |cx| {
        support::present(cx, s.win, ids::PROFILES_NO_ACTIVE)
    });
    let rows = support::log_rows(&s.services);
    assert_eq!(
        row(&rows[0]),
        ("profile", "activate", "Failed to activate profile: Comp")
    );
    assert_eq!(rows[0].level(), LogLevel::Error);
    assert!(
        rows[0]
            .details()
            .is_some_and(|d| d.contains("Failed: HUD ("))
    );
}

#[gpui_kit::test]
fn activate_rests_without_tf2(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, Tf2State::Missing { saved: None });
    let vanilla = empty_profile(&s, "Vanilla");
    let page = open_profiles(cx, &s);
    support::click_id(cx, s.win, ids::profile_activate(vanilla.id()));
    assert!(!has_dialog(cx, &s));
    assert!(!support::present(cx, s.win, ids::TOAST));
    assert!(cx.update(|cx| page.read(cx).activating().is_none()));
}

/// "Comp" with a local HUD "rayshud" (deleted from the library) and a present hitsound.
fn profile_missing_its_hud(s: &support::ShellWindow, env: &support::Env) -> Profile {
    let hud = seed_hud(s, env, "rayshud", Provenance::Local);
    let hit = seed_sound(s, env, "Classic Ding");
    let draft = ProfileDraft::new("Comp")
        .with_hud(DraftSlot::Item(hud.id().clone()))
        .with_hitsound(DraftSlot::Item(hit.id().clone()));
    let profile = s.services.library.create_profile(&draft).unwrap();
    s.services.library.delete_hud(hud.id()).unwrap();
    profile
}

#[gpui_kit::test]
fn a_missing_item_opens_the_dialog_and_touches_nothing(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let comp = profile_missing_its_hud(&s, &env);
    let _page = open_profiles(cx, &s);
    let (changes, _sub) = count_changes(cx);
    support::click_id(cx, s.win, ids::profile_activate(comp.id()));
    support::wait_real(cx, "the Missing Items dialog", |cx| {
        support::present(cx, s.win, ids::PROFILES_MISSING_DIALOG)
    });
    assert_eq!(
        support::label_of(cx, s.win, ids::PROFILES_MISSING_DESCRIPTION).as_deref(),
        Some("Some items in \"Comp\" are no longer in your library.")
    );
    assert_eq!(
        support::label_of(cx, s.win, ids::missing_item(0)).as_deref(),
        Some("rayshud (HUD)")
    );
    assert_eq!(
        support::label_of(cx, s.win, ids::missing_local(0)).as_deref(),
        Some("Local import — can't re-download")
    );
    assert!(!support::present(cx, s.win, ids::missing_redownload(0)));
    assert!(!support::present(cx, s.win, ids::missing_item(1)));
    assert!(!env.game.join("tf/custom/tf2tools-sounds").exists());
    assert_eq!(s.services.library.active_profile().unwrap(), None);
    assert_eq!(changes.get(), 0);
    assert!(support::log_rows(&s.services).is_empty());
}

#[gpui_kit::test]
fn the_missing_items_header_sits_together_like_v5s(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let comp = profile_missing_its_hud(&s, &env);
    let _page = open_profiles(cx, &s);
    support::click_id(cx, s.win, ids::profile_activate(comp.id()));
    support::wait_real(cx, "the Missing Items dialog", |cx| {
        support::present(cx, s.win, ids::PROFILES_MISSING_DIALOG)
    });
    let title = support::bounds_of(cx, s.win, ids::PROFILES_MISSING_TITLE).unwrap();
    let description = support::bounds_of(cx, s.win, ids::PROFILES_MISSING_DESCRIPTION).unwrap();
    let first = support::bounds_of(cx, s.win, ids::missing_item(0)).unwrap();
    assert_eq!(
        description.top() - title.bottom(),
        px(8.),
        "v5's gap-2 header"
    );
    assert_eq!(
        first.top() - description.bottom(),
        px(16.),
        "v5's gap-4 content"
    );
}

#[gpui_kit::test]
fn cancel_in_missing_items_changes_nothing(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let comp = profile_missing_its_hud(&s, &env);
    let page = open_profiles(cx, &s);
    support::click_id(cx, s.win, ids::profile_activate(comp.id()));
    support::wait_real(cx, "the Missing Items dialog", |cx| {
        support::present(cx, s.win, ids::PROFILES_MISSING_DIALOG)
    });
    assert!(cx.update(|cx| page.read(cx).activating().is_some()));
    support::click_id(cx, s.win, ids::PROFILES_MISSING_CANCEL);
    assert!(!has_dialog(cx, &s));
    assert!(cx.update(|cx| page.read(cx).activating().is_none()));
    assert_eq!(s.services.library.active_profile().unwrap(), None);
    assert!(support::log_rows(&s.services).is_empty());
}

#[gpui_kit::test]
fn install_anyway_skips_the_missing_item(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let comp = profile_missing_its_hud(&s, &env);
    let _page = open_profiles(cx, &s);
    let (changes, _sub) = count_changes(cx);
    support::click_id(cx, s.win, ids::profile_activate(comp.id()));
    support::wait_real(cx, "the Missing Items dialog", |cx| {
        support::present(cx, s.win, ids::PROFILES_MISSING_INSTALL)
    });
    support::click_id(cx, s.win, ids::PROFILES_MISSING_INSTALL);
    toast_is(
        cx,
        &s,
        "Profile \"Comp\" applied. Installed: Hitsound. Kept what you had for: HUD (missing from your library)",
    );
    assert!(!has_dialog(cx, &s));
    // The game does not match the profile, so it is not marked Active.
    assert_eq!(s.services.library.active_profile().unwrap(), None);
    support::wait_real(cx, "LibraryChanged", |_| changes.get() >= 1);
    assert_eq!(changes.get(), 1);
    let rows = support::log_rows(&s.services);
    assert_eq!(rows.len(), 1);
    assert_eq!(
        row(&rows[0]),
        ("profile", "activate", "Applied profile: Comp")
    );
    assert!(
        rows[0]
            .details()
            .is_some_and(|d| d.contains("Skipped: rayshud"))
    );
}

#[gpui_kit::test]
fn a_store_item_offers_re_download(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    server.mount_gb_files(
        4242,
        &[("flawhud.zip", support::hud_zip_bytes("flawhud"))],
        Duration::ZERO,
    );
    let s = support::shell_window(cx, &env, found(&env));
    support::store_ready(cx, &server);
    support::seed_gb_huds(&env, &[(4242, "FlawHUD")]);
    let gb = Provenance::GameBanana {
        gb_id: GbId::new(4242),
        author: "vex".into(),
    };
    let hud = seed_hud(&s, &env, "flawhud", gb);
    let comp = s
        .services
        .library
        .create_profile(&ProfileDraft::new("Comp").with_hud(DraftSlot::Item(hud.id().clone())))
        .unwrap();
    s.services.library.delete_hud(hud.id()).unwrap();
    let _page = open_profiles(cx, &s);
    support::click_id(cx, s.win, ids::profile_activate(comp.id()));
    support::wait_real(cx, "the Re-download button", |cx| {
        support::present(cx, s.win, ids::missing_redownload(0))
    });
    assert!(!support::present(cx, s.win, ids::missing_local(0)));
    support::click_id(cx, s.win, ids::missing_redownload(0));
    support::wait_real(cx, "the store detail", |cx| {
        support::present(cx, s.win, ids::STORE_DETAIL)
    });
    assert!(support::present(cx, s.win, ids::PROFILES_MISSING_DIALOG));
}

#[gpui_kit::test]
fn the_profile_buttons_are_v5_s_32_px(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let p = empty_profile(&s, "Casual");
    let _page = open_profiles(cx, &s);
    let targets: [gpui_kit::ElementId; 5] = [
        ids::PROFILES_CREATE.into(),
        ids::PROFILES_SAVE_CURRENT.into(),
        ids::profile_activate(p.id()),
        ids::profile_edit(p.id()),
        ids::profile_delete(p.id()),
    ];
    for id in targets {
        let b = support::bounds_of(cx, s.win, id.clone()).expect("drawn");
        assert_eq!(b.size.height, gpui_kit::px(32.), "{id:?}");
    }
}

#[gpui_kit::test]
fn the_empty_state_buttons_are_v5_s_32_px(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let _page = open_profiles(cx, &s);
    for id in [ids::PROFILES_EMPTY_CREATE, ids::PROFILES_EMPTY_SAVE] {
        let b = support::bounds_of(cx, s.win, id).expect(id);
        assert_eq!(b.size.height, gpui_kit::px(32.), "{id}");
    }
}

#[gpui_kit::test]
fn the_save_dialog_is_v5_s_512_px_and_centred(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    empty_profile(&s, "Casual");
    let _page = open_profiles(cx, &s);
    support::click_id(cx, s.win, ids::PROFILES_SAVE_CURRENT);
    assert!(support::present(cx, s.win, ids::PROFILES_SAVE_DIALOG));
    let b = support::dialog_box(cx, s.win);
    let view = support::viewport(cx, s.win);
    assert_eq!(b.size.width, gpui_kit::px(512.));
    let off = b.origin.y + b.size.height / 2. - view.height / 2.;
    assert!(off.abs() <= gpui_kit::px(1.), "box {b:?} in {view:?}");
}

#[gpui_kit::test]
fn nothing_else_starts_while_an_activate_runs(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let comp = full_profile(&s, &env, "Comp");
    let other = empty_profile(&s, "Other");
    let page = open_profiles(cx, &s);
    let (a, b) = (comp.id().clone(), other.id().clone());
    cx.update_window(s.win, |_, window, cx| {
        page.update(cx, |p, cx| {
            p.activate(a.clone(), window, cx);
            p.delete(b.clone(), "Other", window, cx);
            p.open_save(window, cx);
            p.open_wizard(WizardMode::Create, window, cx);
        })
    })
    .unwrap();
    assert!(
        !has_dialog(cx, &s),
        "no confirm, save dialog or wizard while activating"
    );
    assert!(cx.update(|cx| page.read(cx).wizard().is_none()));
    toast_is(
        cx,
        &s,
        "Profile \"Comp\" activated. Installed: HUD, Hitsound, Killsound, Crosshairs",
    );
    assert_eq!(s.services.library.profiles().unwrap().len(), 2);
}

// The Missing Items dialog holds the activate open, so every button is read in a steady busy state.
#[gpui_kit::test]
fn every_profile_button_renders_disabled_while_an_activate_runs(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let comp = profile_missing_its_hud(&s, &env);
    let other = empty_profile(&s, "Other");
    let page = open_profiles(cx, &s);
    let buttons: Vec<ElementId> = vec![
        ids::PROFILES_CREATE.into(),
        ids::PROFILES_SAVE_CURRENT.into(),
        ids::profile_activate(comp.id()),
        ids::profile_edit(comp.id()),
        ids::profile_delete(comp.id()),
        ids::profile_activate(other.id()),
        ids::profile_edit(other.id()),
        ids::profile_delete(other.id()),
    ];
    assert_eq!(
        support::disabled_ids(cx, s.win, &buttons),
        Vec::<ElementId>::new()
    );
    support::click_id(cx, s.win, ids::profile_activate(comp.id()));
    support::wait_real(cx, "the Missing Items dialog", |cx| {
        support::present(cx, s.win, ids::PROFILES_MISSING_DIALOG)
    });
    assert!(cx.update(|cx| page.read(cx).is_busy()));
    assert_eq!(support::disabled_ids(cx, s.win, &buttons), buttons);
    support::click_id(cx, s.win, ids::PROFILES_MISSING_CANCEL);
    assert!(cx.update(|cx| !page.read(cx).is_busy()));
    assert_eq!(
        support::disabled_ids(cx, s.win, &buttons),
        Vec::<ElementId>::new()
    );
}

// The empty state leaves once a profile exists, so its buttons are read in the frame the
// Save click draws, before the executor runs the save job.
#[gpui_kit::test]
fn the_empty_state_buttons_render_disabled_while_a_save_runs(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let page = open_profiles(cx, &s);
    let empty: Vec<ElementId> = vec![
        ids::PROFILES_EMPTY_CREATE.into(),
        ids::PROFILES_EMPTY_SAVE.into(),
    ];
    assert_eq!(
        support::disabled_ids(cx, s.win, &empty),
        Vec::<ElementId>::new()
    );
    support::click_id(cx, s.win, ids::PROFILES_EMPTY_SAVE);
    type_name(cx, &s, &page, "Comp");
    let disabled = cx
        .update_window(s.win, |_, w, cx| {
            w.click(ids::PROFILES_SAVE_OK, cx);
            w.render_frame(cx);
            w.simulate_next_frame(cx);
            empty
                .iter()
                .filter(|id| support::button_disabled(w, (*id).clone()))
                .cloned()
                .collect::<Vec<_>>()
        })
        .unwrap();
    assert_eq!(disabled, empty);
    support::wait_real(cx, "the save to end", |cx| {
        cx.update(|cx| !page.read(cx).is_busy())
    });
}

#[gpui_kit::test]
fn the_save_dialogs_x_closes_without_saving(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let page = open_profiles(cx, &s);
    support::click_id(cx, s.win, ids::PROFILES_SAVE_CURRENT);
    type_name(cx, &s, &page, "Mine");
    support::click_id(cx, s.win, "close");
    assert!(!has_dialog(cx, &s));
    assert!(s.services.library.profiles().unwrap().is_empty());
}

#[gpui_kit::test]
fn enter_with_a_blank_name_does_not_save(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let page = open_profiles(cx, &s);
    support::click_id(cx, s.win, ids::PROFILES_SAVE_CURRENT);
    type_name(cx, &s, &page, "   ");
    support::press(cx, s.win, "enter");
    assert!(support::present(cx, s.win, ids::PROFILES_SAVE_DIALOG));
    assert!(s.services.library.profiles().unwrap().is_empty());
}

#[gpui_kit::test]
fn escape_on_missing_items_answers_cancel(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let comp = profile_missing_its_hud(&s, &env);
    let page = open_profiles(cx, &s);
    support::click_id(cx, s.win, ids::profile_activate(comp.id()));
    support::wait_real(cx, "the Missing Items dialog", |cx| {
        support::present(cx, s.win, ids::PROFILES_MISSING_DIALOG)
    });
    support::press(cx, s.win, "escape");
    assert!(!has_dialog(cx, &s));
    support::wait_real(cx, "the flow to end", |cx| {
        cx.update(|cx| page.read(cx).activating().is_none())
    });
    assert_eq!(s.services.library.active_profile().unwrap(), None);
}

#[gpui_kit::test]
fn closing_the_stacked_store_detail_keeps_missing_items_open(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    server.mount_gb_files(
        4242,
        &[("flawhud.zip", support::hud_zip_bytes("flawhud"))],
        Duration::ZERO,
    );
    let s = support::shell_window(cx, &env, found(&env));
    support::store_ready(cx, &server);
    support::seed_gb_huds(&env, &[(4242, "FlawHUD")]);
    let gb = Provenance::GameBanana {
        gb_id: GbId::new(4242),
        author: "vex".into(),
    };
    let hud = seed_hud(&s, &env, "flawhud", gb);
    let comp = s
        .services
        .library
        .create_profile(&ProfileDraft::new("Comp").with_hud(DraftSlot::Item(hud.id().clone())))
        .unwrap();
    s.services.library.delete_hud(hud.id()).unwrap();
    let page = open_profiles(cx, &s);
    support::click_id(cx, s.win, ids::profile_activate(comp.id()));
    support::wait_real(cx, "the Re-download button", |cx| {
        support::present(cx, s.win, ids::missing_redownload(0))
    });
    support::click_id(cx, s.win, ids::missing_redownload(0));
    support::wait_real(cx, "the store detail", |cx| {
        support::present(cx, s.win, ids::STORE_DETAIL)
    });
    support::press(cx, s.win, "escape");
    assert!(!support::present(cx, s.win, ids::STORE_DETAIL));
    assert!(support::present(cx, s.win, ids::PROFILES_MISSING_DIALOG));
    assert!(cx.update(|cx| page.read(cx).activating().is_some()));
}

#[gpui_kit::test]
fn the_save_dialog_is_padded_like_v5(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    empty_profile(&s, "Casual");
    let _page = open_profiles(cx, &s);
    support::click_id(cx, s.win, ids::PROFILES_SAVE_CURRENT);
    let b = support::dialog_box(cx, s.win);
    let body = support::bounds_of(cx, s.win, ids::PROFILES_SAVE_DIALOG).unwrap();
    let ok = support::bounds_of(cx, s.win, ids::PROFILES_SAVE_OK).unwrap();
    assert_eq!(
        body.left() - b.left(),
        gpui_kit::px(25.),
        "{body:?} in {b:?}"
    );
    assert_eq!(b.right() - ok.right(), gpui_kit::px(25.), "{ok:?} in {b:?}");
}

#[gpui_kit::test]
fn missing_items_is_padded_like_v5(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let comp = profile_missing_its_hud(&s, &env);
    let _page = open_profiles(cx, &s);
    support::click_id(cx, s.win, ids::profile_activate(comp.id()));
    support::wait_real(cx, "the Missing Items dialog", |cx| {
        support::present(cx, s.win, ids::PROFILES_MISSING_DIALOG)
    });
    let b = support::dialog_box(cx, s.win);
    let body = support::bounds_of(cx, s.win, ids::PROFILES_MISSING_DIALOG).unwrap();
    assert_eq!(
        body.left() - b.left(),
        gpui_kit::px(25.),
        "{body:?} in {b:?}"
    );
}

#[gpui_kit::test]
fn a_profile_whose_hud_name_is_taken_warns(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let hud = seed_hud(&s, &env, "m0rehud-abc1234", Provenance::Local);
    std::fs::write(
        env.data
            .library_dir(LibraryKind::Huds)
            .join("m0rehud-abc1234/resource/ui/extra.res"),
        "#base \"../../custom/m0rehud/resource/ui/base.res\"\n",
    )
    .unwrap();
    let draft = ProfileDraft::new("Comp").with_hud(DraftSlot::Item(hud.id().clone()));
    let comp = s.services.library.create_profile(&draft).unwrap();
    let mine = env.game.join("tf/custom/m0rehud");
    std::fs::create_dir_all(&mine).unwrap();
    std::fs::write(mine.join("notes.txt"), "mine").unwrap();
    let _page = open_profiles(cx, &s);
    support::click_id(cx, s.win, ids::profile_activate(comp.id()));
    toast_is(
        cx,
        &s,
        "Profile \"Comp\" activated. A folder named m0rehud is already in your custom \
         folder, so parts of this HUD may not show. Move or rename that folder and install \
         again.",
    );
    assert!(
        env.game
            .join("tf/custom/m0rehud-abc1234/info.vdf")
            .is_file()
    );
    assert_eq!(std::fs::read(mine.join("notes.txt")).unwrap(), b"mine");
}

// At the 800x600 minimum with the sidebar open, the header keeps the page's margin and a usable
// search box (the row wraps under the title), and scrolling the page shows at least one whole
// 196 px card row.
#[gpui_kit::test]
fn at_800x600_the_profiles_header_fits_and_the_cards_have_room(cx: &mut TestAppContext) {
    use gpui_kit::size;
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    empty_profile(&s, "Comp");
    empty_profile(&s, "Pub");
    cx.simulate_window_resize(s.win, size(px(800.), px(600.)));
    let _page = open_profiles(cx, &s);
    let bounds = |cx: &mut TestAppContext, id: &'static str| {
        support::bounds_of(cx, s.win, id).unwrap_or_else(|| panic!("no {id}"))
    };
    let page = support::bounds_of(cx, s.win, ids::page(Page::Profiles)).unwrap();
    let (save, create, search) = (
        bounds(cx, ids::PROFILES_SAVE_CURRENT),
        bounds(cx, ids::PROFILES_CREATE),
        bounds(cx, ids::PROFILES_SEARCH),
    );
    let margin = tf2tools_app::shell::CONTENT_PADDING;
    for b in [save, create, search] {
        assert!(
            b.left() >= page.left() + margin - px(1.)
                && b.right() <= page.right() - margin + px(1.),
            "{b:?} past the margin of {page:?}"
        );
    }
    assert!(
        search.size.width >= px(160.),
        "the search box is {search:?}"
    );
    assert!(
        !create.intersects(&search) && !save.intersects(&search) && !save.intersects(&create),
        "{search:?}, {create:?} and {save:?} overlap"
    );
    let library = bounds(cx, ids::PROFILES_LIBRARY);
    assert!(
        library.size.height >= px(196.),
        "the cards sit in a {library:?} strip"
    );
    // Scrolling the page to its end shows the whole card area.
    cx.update_window(s.win, |_, w, cx| {
        let wheel = gpui_kit::ScrollEvent {
            position: gpui_kit::point(page.center().x, page.top() + px(80.)),
            delta: gpui_kit::ScrollDelta::Lines(gpui_kit::point(0., -40.)),
            modifiers: gpui_kit::Modifiers::default(),
            touch_phase: gpui_kit::TouchPhase::Moved,
            source: Default::default(),
        };
        let _ = w.dispatch_event(gpui_kit::InputEvent::to_platform_input(wheel), cx);
    })
    .unwrap();
    support::settle(cx, s.win);
    let library = bounds(cx, ids::PROFILES_LIBRARY);
    assert!(
        library.bottom() <= page.bottom() + px(1.),
        "{library:?} still runs past {page:?}"
    );
}

#[gpui_kit::test]
fn a_long_missing_items_list_shows_a_bar_beside_its_rows(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    // Builtin crosshairs the catalog does not know, each its own missing row.
    let crosshairs = (0..10)
        .map(|ix| DraftCrosshair::Set {
            weapon_script: format!("tf_weapon_probe_{ix}"),
            crosshair: CrosshairRef::Builtin(format!("gone{ix}")),
        })
        .collect();
    let comp = s
        .services
        .library
        .create_profile(&ProfileDraft::new("Comp").with_crosshairs(crosshairs))
        .unwrap();
    let _page = open_profiles(cx, &s);
    support::click_id(cx, s.win, ids::profile_activate(comp.id()));
    support::wait_real(cx, "the Missing Items dialog", |cx| {
        support::present(cx, s.win, ids::PROFILES_MISSING_DIALOG)
    });
    support::dialog_box(cx, s.win);
    let bar = ids::PROFILES_MISSING_LIST_SCROLLBAR;
    assert_bar_beside(cx, s.win, ids::PROFILES_MISSING_LIST, bar);
    assert_clear_of_bar(cx, s.win, ids::missing_item(0), bar);
    assert_track_scrolls(cx, s.win, bar, ids::missing_item(0));
}
