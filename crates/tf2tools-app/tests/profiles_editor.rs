//! The profile editor: header, rename, Delete, Back, Activate and the save queue.

use crate::support;

use gpui_kit::{AppContext as _, Entity, TestAppContext};
use tf2tools_app::ids;
use tf2tools_app::library_events::LibraryEvents;
use tf2tools_app::pages::profiles::ProfilesPage;
use tf2tools_app::pages::profiles::editor::ProfileEditor;
use tf2tools_app::pages::profiles::editor_model::Edit;
use tf2tools_app::pages::profiles::probe::Tf2Probe;
use tf2tools_app::services::Tf2State;
use tf2tools_app::shell::route::Page;
use tf2tools_core::{
    CrosshairRef, DraftCrosshair, DraftSlot, HudEntry, Profile, ProfileDraft, ProfileId,
    Provenance, SoundEntry, Tf2Install,
};

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

fn seed_hud(s: &support::ShellWindow, env: &support::Env, name: &str) -> HudEntry {
    let path = env.tmp.path().join(format!("{name}.zip"));
    std::fs::write(&path, support::hud_zip_bytes(name)).unwrap();
    s.services
        .library
        .import_hud(&path, Provenance::Local)
        .unwrap()
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
    let hud = seed_hud(s, env, "rayshud");
    let hit = seed_sound(s, env, "Classic Ding");
    let kill = seed_sound(s, env, "Bell");
    let first = s.services.builtins.all()[0].name().to_owned();
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

fn saved(s: &support::ShellWindow, id: &ProfileId) -> Profile {
    s.services.library.profile(id).unwrap()
}
/// Applies `profile` to the fake game, so it opens active.
fn apply(s: &support::ShellWindow, env: &support::Env, profile: &Profile) {
    s.services
        .library
        .apply_profile(profile.id(), &tf2(env), &s.services.builtins, false)
        .unwrap();
}
/// Focuses the name field and replaces its text, as typing does.
fn type_name(
    cx: &mut TestAppContext,
    s: &support::ShellWindow,
    e: &Entity<ProfileEditor>,
    text: &str,
) {
    let input = cx.update(|cx| e.read(cx).name_input().clone());
    cx.update_window(s.win, |_, window, cx| {
        input.update(cx, |st, cx| {
            st.focus(window, cx);
            st.set_value(text, window, cx)
        })
    })
    .unwrap();
    support::settle(cx, s.win);
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

fn wait_idle(cx: &mut TestAppContext, s: &support::ShellWindow, editor: &Entity<ProfileEditor>) {
    support::wait_real(cx, "the editor's queue", |cx| {
        cx.update(|cx| editor.read(cx).is_idle())
    });
    support::settle(cx, s.win);
}

/// Opens the editor on `id` through the page and waits for its first refresh.
fn open_editor(
    cx: &mut TestAppContext,
    s: &support::ShellWindow,
    page: &Entity<ProfilesPage>,
    id: &ProfileId,
) -> Entity<ProfileEditor> {
    let id = id.clone();
    cx.update_window(s.win, |_, window, cx| {
        page.update(cx, |p, cx| p.open_editor(id, window, cx))
    })
    .unwrap();
    let editor = cx.update(|cx| page.read(cx).editor()).expect("an editor");
    wait_idle(cx, s, &editor);
    editor
}

fn field(cx: &mut TestAppContext, e: &Entity<ProfileEditor>) -> String {
    cx.update(|cx| e.read(cx).name_input().read(cx).value().to_string())
}

fn toast_is(cx: &mut TestAppContext, s: &support::ShellWindow, text: &str) {
    support::wait_real(cx, text, |cx| {
        support::label_of(cx, s.win, ids::TOAST).as_deref() == Some(text)
    });
}
#[gpui_kit::test]
fn the_editor_replaces_the_grid(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let comp = empty_profile(&s, "Comp");
    let page = open_profiles(cx, &s);
    let editor = open_editor(cx, &s, &page, comp.id());
    assert!(support::present(cx, s.win, ids::PROFILE_EDITOR));
    assert!(!support::present(cx, s.win, ids::PROFILES_LIBRARY));
    assert!(!support::present(cx, s.win, ids::PROFILES_NO_ACTIVE));
    for id in [
        ids::PROFILE_EDITOR_BACK,
        ids::PROFILE_EDITOR_NAME,
        ids::PROFILE_EDITOR_ACTIVATE,
        ids::PROFILE_EDITOR_DELETE,
    ] {
        assert!(support::present(cx, s.win, id), "{id}");
    }
    assert!(!support::present(
        cx,
        s.win,
        ids::PROFILE_EDITOR_ACTIVE_BADGE
    ));
    assert!(!support::present(
        cx,
        s.win,
        ids::PROFILE_EDITOR_NOT_APPLIED
    ));
    assert_eq!(field(cx, &editor), "Comp");
}

#[gpui_kit::test]
fn back_returns_to_the_grid(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let comp = empty_profile(&s, "Comp");
    let page = open_profiles(cx, &s);
    let _editor = open_editor(cx, &s, &page, comp.id());
    support::click_id(cx, s.win, ids::PROFILE_EDITOR_BACK);
    assert!(!support::present(cx, s.win, ids::PROFILE_EDITOR));
    assert!(support::present(cx, s.win, ids::profile_card(comp.id())));
    assert!(cx.update(|cx| page.read(cx).editor()).is_none());
}
#[gpui_kit::test]
fn delete_confirms_then_returns_to_the_grid(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let comp = empty_profile(&s, "Comp");
    let page = open_profiles(cx, &s);
    let _editor = open_editor(cx, &s, &page, comp.id());
    support::click_id(cx, s.win, ids::PROFILE_EDITOR_DELETE);
    assert_eq!(
        support::label_of(cx, s.win, ids::CONFIRM_DESCRIPTION).as_deref(),
        Some("Are you sure you want to delete \"Comp\"? This cannot be undone.")
    );
    support::click_id(cx, s.win, ids::CONFIRM_OK);
    toast_is(cx, &s, "Profile deleted");
    support::wait_real(cx, "the grid", |cx| {
        support::present(cx, s.win, ids::PROFILES_EMPTY)
    });
    assert!(!support::present(cx, s.win, ids::PROFILE_EDITOR));
    let row = &support::log_rows(&s.services)[0];
    assert_eq!(
        (row.category(), row.action(), row.message()),
        ("profile", "delete", "Deleted profile: Comp")
    );
}

#[gpui_kit::test]
fn cancelling_delete_stays_in_the_editor(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let comp = empty_profile(&s, "Comp");
    let page = open_profiles(cx, &s);
    let _editor = open_editor(cx, &s, &page, comp.id());
    support::click_id(cx, s.win, ids::PROFILE_EDITOR_DELETE);
    support::click_id(cx, s.win, ids::CONFIRM_CANCEL);
    assert!(support::present(cx, s.win, ids::PROFILE_EDITOR));
    assert_eq!(s.services.library.profiles().unwrap().len(), 1);
}
#[gpui_kit::test]
fn a_library_change_that_removes_the_profile_closes_the_editor(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let comp = empty_profile(&s, "Comp");
    let page = open_profiles(cx, &s);
    let _editor = open_editor(cx, &s, &page, comp.id());
    s.services.library.delete_profile(comp.id()).unwrap();
    cx.update(LibraryEvents::changed);
    support::wait_real(cx, "the grid", |cx| {
        !support::present(cx, s.win, ids::PROFILE_EDITOR)
    });
}

#[gpui_kit::test]
fn activate_in_the_editor_applies_the_profile(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let comp = full_profile(&s, &env, "Comp");
    let page = open_profiles(cx, &s);
    let _editor = open_editor(cx, &s, &page, comp.id());
    support::click_id(cx, s.win, ids::PROFILE_EDITOR_ACTIVATE);
    toast_is(
        cx,
        &s,
        "Profile \"Comp\" activated. Installed: HUD, Hitsound, Killsound, Crosshairs",
    );
    support::wait_real(cx, "the Active badge", |cx| {
        support::present(cx, s.win, ids::PROFILE_EDITOR_ACTIVE_BADGE)
    });
    assert!(env.game.join("tf/custom/rayshud").is_dir());
}

#[gpui_kit::test]
fn without_tf2_activate_is_disabled(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, Tf2State::Missing { saved: None });
    let comp = empty_profile(&s, "Comp");
    let page = open_profiles(cx, &s);
    let _editor = open_editor(cx, &s, &page, comp.id());
    assert!(support::disabled_of(
        cx,
        s.win,
        ids::PROFILE_EDITOR_ACTIVATE
    ));
}

// Two edits queued in one go: the rename's draft is built from the profile the HUD pick saved,
// so neither is lost.
#[gpui_kit::test]
fn queued_edits_build_on_each_other(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let comp = full_profile(&s, &env, "Comp");
    let budhud = seed_hud(&s, &env, "budhud");
    let page = open_profiles(cx, &s);
    let editor = open_editor(cx, &s, &page, comp.id());
    cx.update(|cx| {
        editor.update(cx, |e, cx| {
            e.enqueue(Edit::SetHud(budhud.id().clone()), cx);
            e.enqueue(Edit::Rename("Pubs".into()), cx);
        })
    });
    wait_idle(cx, &s, &editor);
    let after = saved(&s, comp.id());
    assert_eq!(after.name(), "Pubs");
    assert_eq!(
        after.hud().map(|h| h.folder_name()),
        Some(budhud.folder_name())
    );
    assert_eq!(after.hitsound(), comp.hitsound());
    assert_eq!(after.crosshairs(), comp.crosshairs());
    assert_eq!(cx.update(|cx| editor.read(cx).saved().clone()), after);
    assert!(
        support::log_rows(&s.services).is_empty(),
        "edits write no activity rows"
    );
}
#[gpui_kit::test]
fn an_edit_to_a_profile_deleted_elsewhere_returns_to_the_grid(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let comp = empty_profile(&s, "Comp");
    let page = open_profiles(cx, &s);
    let editor = open_editor(cx, &s, &page, comp.id());
    s.services.library.delete_profile(comp.id()).unwrap();
    cx.update(|cx| editor.update(cx, |e, cx| e.enqueue(Edit::Rename("Pubs".into()), cx)));
    toast_is(
        cx,
        &s,
        "Failed to save profile. This profile no longer exists. Pick another profile.",
    );
    support::wait_real(cx, "the grid", |cx| {
        !support::present(cx, s.win, ids::PROFILE_EDITOR)
    });
}
#[gpui_kit::test]
fn enter_renames_the_profile(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let comp = empty_profile(&s, "Comp");
    let page = open_profiles(cx, &s);
    let editor = open_editor(cx, &s, &page, comp.id());
    type_name(cx, &s, &editor, "  Pubs ");
    support::press(cx, s.win, "enter");
    wait_idle(cx, &s, &editor);
    assert_eq!(saved(&s, comp.id()).name(), "Pubs");
    support::click_id(cx, s.win, ids::PROFILE_EDITOR_BACK);
    support::wait_real(cx, "the renamed card", |cx| {
        support::label_of(cx, s.win, ids::profile_card(comp.id())).as_deref() == Some("Pubs")
    });
    assert!(
        support::log_rows(&s.services).is_empty(),
        "a rename writes no activity row"
    );
}

#[gpui_kit::test]
fn leaving_the_name_field_renames_too(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let comp = empty_profile(&s, "Comp");
    let page = open_profiles(cx, &s);
    let editor = open_editor(cx, &s, &page, comp.id());
    // A test window starts inactive, and an inactive window reports no focus changes.
    cx.update_window(s.win, |_, w, _| w.activate_window())
        .unwrap();
    type_name(cx, &s, &editor, "Pubs");
    cx.update_window(s.win, |_, w, cx| w.blur(cx)).unwrap();
    support::settle(cx, s.win);
    wait_idle(cx, &s, &editor);
    assert_eq!(saved(&s, comp.id()).name(), "Pubs");
}

#[gpui_kit::test]
fn a_blank_name_reverts_to_the_saved_one(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let comp = empty_profile(&s, "Comp");
    let page = open_profiles(cx, &s);
    let editor = open_editor(cx, &s, &page, comp.id());
    type_name(cx, &s, &editor, "   ");
    support::press(cx, s.win, "enter");
    wait_idle(cx, &s, &editor);
    assert_eq!(field(cx, &editor), "Comp");
    assert_eq!(saved(&s, comp.id()).name(), "Comp");
}

#[gpui_kit::test]
fn a_taken_name_shows_the_line_and_keeps_the_old_name(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let comp = empty_profile(&s, "Comp");
    empty_profile(&s, "Pubs");
    let page = open_profiles(cx, &s);
    let editor = open_editor(cx, &s, &page, comp.id());
    type_name(cx, &s, &editor, "pubs");
    assert_eq!(
        support::label_of(cx, s.win, ids::PROFILE_EDITOR_NAME_TAKEN).as_deref(),
        Some("You already have a profile called Pubs. Pick another name.")
    );
    support::press(cx, s.win, "enter");
    wait_idle(cx, &s, &editor);
    assert_eq!(saved(&s, comp.id()).name(), "Comp");
    type_name(cx, &s, &editor, "Pubs 2");
    assert!(!support::present(cx, s.win, ids::PROFILE_EDITOR_NAME_TAKEN));
}

// The page's list is stale, so only core sees the clash; the editor says it the same way.
#[gpui_kit::test]
fn a_name_taken_in_a_race_shows_the_same_line(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let comp = empty_profile(&s, "Comp");
    let page = open_profiles(cx, &s);
    let editor = open_editor(cx, &s, &page, comp.id());
    empty_profile(&s, "Pubs");
    type_name(cx, &s, &editor, "Pubs");
    support::press(cx, s.win, "enter");
    wait_idle(cx, &s, &editor);
    assert_eq!(
        support::label_of(cx, s.win, ids::PROFILE_EDITOR_NAME_TAKEN).as_deref(),
        Some("You already have a profile called Pubs. Pick another name.")
    );
    assert_eq!(saved(&s, comp.id()).name(), "Comp");
}

#[gpui_kit::test]
fn renaming_the_active_profile_keeps_it_active(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let comp = full_profile(&s, &env, "Comp");
    apply(&s, &env, &comp);
    let page = open_profiles(cx, &s);
    let editor = open_editor(cx, &s, &page, comp.id());
    assert!(support::present(
        cx,
        s.win,
        ids::PROFILE_EDITOR_ACTIVE_BADGE
    ));
    assert!(!support::present(cx, s.win, ids::PROFILE_EDITOR_ACTIVATE));
    type_name(cx, &s, &editor, "Pubs");
    support::press(cx, s.win, "enter");
    wait_idle(cx, &s, &editor);
    assert!(support::present(
        cx,
        s.win,
        ids::PROFILE_EDITOR_ACTIVE_BADGE
    ));
    assert!(!support::present(
        cx,
        s.win,
        ids::PROFILE_EDITOR_NOT_APPLIED
    ));
    assert_eq!(
        s.services.library.active_profile().unwrap().as_ref(),
        Some(comp.id())
    );
}
// A kit button never takes the focus, so the field never blurs: Back saves what was typed.
#[gpui_kit::test]
fn back_saves_a_typed_name(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let comp = empty_profile(&s, "Comp");
    let page = open_profiles(cx, &s);
    let editor = open_editor(cx, &s, &page, comp.id());
    type_name(cx, &s, &editor, "Pubs");
    support::click_id(cx, s.win, ids::PROFILE_EDITOR_BACK);
    support::wait_real(cx, "the grid", |cx| {
        !support::present(cx, s.win, ids::PROFILE_EDITOR)
    });
    assert_eq!(saved(&s, comp.id()).name(), "Pubs");
    assert!(cx.update(|cx| page.read(cx).editor()).is_none());
}

#[gpui_kit::test]
fn activate_saves_a_typed_name_first(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let comp = full_profile(&s, &env, "Comp");
    let page = open_profiles(cx, &s);
    let editor = open_editor(cx, &s, &page, comp.id());
    type_name(cx, &s, &editor, "Pubs");
    support::click_id(cx, s.win, ids::PROFILE_EDITOR_ACTIVATE);
    support::wait_real(cx, "the Active badge", |cx| {
        support::present(cx, s.win, ids::PROFILE_EDITOR_ACTIVE_BADGE)
    });
    assert_eq!(saved(&s, comp.id()).name(), "Pubs");
    assert_eq!(
        s.services.library.active_profile().unwrap().as_ref(),
        Some(comp.id())
    );
}
