//! The profile editor: header, rename, Delete, Back, Activate and the save queue.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::support;

use gpui_kit::ElementId;
use gpui_kit::ScrollDelta;
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{AppContext as _, Entity, TestAppContext};
use tf2tools_app::audio::player::{NowPlaying, sound_player};
use tf2tools_app::ids;
use tf2tools_app::library_events::LibraryEvents;
use tf2tools_app::pages::profiles::ProfilesPage;
use tf2tools_app::pages::profiles::editor::ProfileEditor;
use tf2tools_app::pages::profiles::editor_model::Edit;
use tf2tools_app::pages::profiles::probe::Tf2Probe;
use tf2tools_app::services::Tf2State;
use tf2tools_app::shell::route::Page;
use tf2tools_core::{
    CrosshairRef, DraftCrosshair, DraftSlot, HudEntry, Profile, ProfileCrosshairRef, ProfileDraft,
    ProfileId, Provenance, SoundEntry, SoundKind, Tf2Install, TfClass, weapons,
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

// An edit that ends the active state lands before the page's reload; a page redraw in between
// still sees the old active id and must not count the edit as applied.
#[gpui_kit::test]
fn a_stale_active_id_keeps_changes_not_applied(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let comp = full_profile(&s, &env, "Comp");
    let budhud = seed_hud(&s, &env, "budhud");
    apply(&s, &env, &comp);
    let page = open_profiles(cx, &s);
    let editor = open_editor(cx, &s, &page, comp.id());
    // Redraw the page once, in the same tick the save lands.
    let fired = std::rc::Rc::new(std::cell::Cell::new(false));
    let (once, redraw) = (std::rc::Rc::clone(&fired), page.clone());
    let _watch = cx.update(|cx| {
        cx.observe(&editor, move |e, cx| {
            if e.read(cx).not_applied() && !once.replace(true) {
                redraw.update(cx, |_, cx| cx.notify());
            }
        })
    });
    cx.update(|cx| editor.update(cx, |e, cx| e.enqueue(Edit::SetHud(budhud.id().clone()), cx)));
    wait_idle(cx, &s, &editor);
    support::wait_real(cx, "the page's reload", |cx| {
        cx.update(|cx| page.read(cx).active().is_none())
    });
    assert!(fired.get());
    assert!(cx.update(|cx| editor.read(cx).not_applied()));
    assert!(support::present(cx, s.win, ids::PROFILE_EDITOR_NOT_APPLIED));
}

#[gpui_kit::test]
fn a_slot_save_clears_its_missing_mark(cx: &mut TestAppContext) {
    use tf2tools_app::pages::profiles::editor_model::{Slot, SlotState, slot_state};
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let rayshud = seed_hud(&s, &env, "rayshud");
    let budhud = seed_hud(&s, &env, "budhud");
    let comp = s
        .services
        .library
        .create_profile(&ProfileDraft::new("Comp").with_hud(DraftSlot::Item(rayshud.id().clone())))
        .unwrap();
    s.services.library.delete_hud(rayshud.id()).unwrap();
    let page = open_profiles(cx, &s);
    let editor = open_editor(cx, &s, &page, comp.id());
    let hud_state = |cx: &mut TestAppContext| {
        cx.update(|cx| {
            let e = editor.read(cx);
            slot_state(e.saved(), Slot::Hud, e.missing())
        })
    };
    assert!(matches!(hud_state(cx), SlotState::Missing(_)));
    cx.update(|cx| editor.update(cx, |e, cx| e.enqueue(Edit::SetHud(budhud.id().clone()), cx)));
    wait_idle(cx, &s, &editor);
    assert!(!matches!(hud_state(cx), SlotState::Missing(_)));
}

#[gpui_kit::test]
fn back_with_a_taken_name_stays_in_the_editor(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let comp = empty_profile(&s, "Comp");
    empty_profile(&s, "Pubs");
    let page = open_profiles(cx, &s);
    let editor = open_editor(cx, &s, &page, comp.id());
    type_name(cx, &s, &editor, "Pubs");
    support::click_id(cx, s.win, ids::PROFILE_EDITOR_BACK);
    wait_idle(cx, &s, &editor);
    assert!(support::present(cx, s.win, ids::PROFILE_EDITOR));
    assert!(support::present(cx, s.win, ids::PROFILE_EDITOR_NAME_TAKEN));
    assert_eq!(saved(&s, comp.id()).name(), "Comp");
}

#[gpui_kit::test]
fn activate_with_a_taken_name_does_not_activate(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let comp = full_profile(&s, &env, "Comp");
    empty_profile(&s, "Pubs");
    let page = open_profiles(cx, &s);
    let editor = open_editor(cx, &s, &page, comp.id());
    type_name(cx, &s, &editor, "Pubs");
    support::click_id(cx, s.win, ids::PROFILE_EDITOR_ACTIVATE);
    wait_idle(cx, &s, &editor);
    support::wait_real(cx, "no activate running", |cx| {
        cx.update(|cx| page.read(cx).activating().is_none())
    });
    assert!(support::present(cx, s.win, ids::PROFILE_EDITOR_NAME_TAKEN));
    assert_eq!(s.services.library.active_profile().unwrap(), None);
    assert_eq!(saved(&s, comp.id()).name(), "Comp");
}

// Enter queues the rename; the blur that follows while it runs must not queue it again.
#[gpui_kit::test]
fn enter_then_leaving_the_field_saves_once(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let comp = empty_profile(&s, "Comp");
    let page = open_profiles(cx, &s);
    let editor = open_editor(cx, &s, &page, comp.id());
    // Every distinct save stamp the editor has held, starting with the opened one.
    let stamps = std::rc::Rc::new(std::cell::RefCell::new(vec![comp.updated_at().to_owned()]));
    let sink = std::rc::Rc::clone(&stamps);
    let _watch = cx.update(|cx| {
        cx.observe(&editor, move |e, cx| {
            let stamp = e.read(cx).saved().updated_at().to_owned();
            if sink.borrow().last() != Some(&stamp) {
                sink.borrow_mut().push(stamp);
            }
        })
    });
    // A test window starts inactive, and an inactive window reports no focus changes.
    cx.update_window(s.win, |_, w, _| w.activate_window())
        .unwrap();
    type_name(cx, &s, &editor, "Pubs");
    cx.update_window(s.win, |_, w, cx| {
        gpui_kit::test::TestWindowExt::press(w, "enter", cx);
        w.blur(cx);
    })
    .unwrap();
    support::settle(cx, s.win);
    wait_idle(cx, &s, &editor);
    assert_eq!(saved(&s, comp.id()).name(), "Pubs");
    assert_eq!(stamps.borrow().len(), 2, "{:?}", stamps.borrow());
}

#[gpui_kit::test]
fn delete_saves_a_typed_name_first(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let comp = empty_profile(&s, "Comp");
    let page = open_profiles(cx, &s);
    let editor = open_editor(cx, &s, &page, comp.id());
    type_name(cx, &s, &editor, "Pubs");
    support::click_id(cx, s.win, ids::PROFILE_EDITOR_DELETE);
    support::wait_real(cx, "the confirm", |cx| {
        support::present(cx, s.win, ids::CONFIRM_DESCRIPTION)
    });
    assert_eq!(
        support::label_of(cx, s.win, ids::CONFIRM_DESCRIPTION).as_deref(),
        Some("Are you sure you want to delete \"Pubs\"? This cannot be undone.")
    );
    support::click_id(cx, s.win, ids::CONFIRM_OK);
    toast_is(cx, &s, "Profile deleted");
    let row = &support::log_rows(&s.services)[0];
    assert_eq!(row.message(), "Deleted profile: Pubs");
}

/// Every file under `dir` with its bytes, so a test can tell the game folder was left alone.
fn files_under(dir: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    let mut files = BTreeMap::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(next) = stack.pop() {
        for entry in std::fs::read_dir(&next).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                stack.push(path);
            } else {
                files.insert(path.clone(), std::fs::read(&path).unwrap());
            }
        }
    }
    files
}
#[gpui_kit::test]
fn slot_cards_show_the_profile(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let comp = full_profile(&s, &env, "Comp");
    let vanilla = empty_profile(&s, "Vanilla");
    let page = open_profiles(cx, &s);
    let _editor = open_editor(cx, &s, &page, comp.id());
    let texts = |cx: &mut TestAppContext| -> Vec<Option<String>> {
        ["hud", "hitsound", "killsound", "crosshairs"]
            .into_iter()
            .map(|k| support::label_of(cx, s.win, ids::profile_slot(k)))
            .collect()
    };
    let hud = comp.hud().unwrap().name().to_owned();
    assert_eq!(
        texts(cx),
        [
            Some(hud),
            Some("Classic Ding".into()),
            Some("Bell".into()),
            Some("1 weapon".into())
        ]
    );
    assert_eq!(
        support::selected_of(cx, s.win, ids::profile_slot("hud")),
        Some(true)
    );
    assert!(support::present(cx, s.win, ids::profile_picker("hud")));
    support::click_id(cx, s.win, ids::PROFILE_EDITOR_BACK);
    let _editor = open_editor(cx, &s, &page, vanilla.id());
    assert_eq!(
        texts(cx),
        [
            Some("None".into()),
            Some("None".into()),
            Some("None".into()),
            Some("Default crosshairs".into())
        ]
    );
    assert!(!support::present(cx, s.win, ids::profile_slot_clear("hud")));
}

#[gpui_kit::test]
fn picking_a_hud_saves_at_once_and_marks_it(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let comp = full_profile(&s, &env, "Comp");
    let budhud = seed_hud(&s, &env, "budhud");
    let page = open_profiles(cx, &s);
    let editor = open_editor(cx, &s, &page, comp.id());
    let rays = comp.hud().unwrap().folder_name().to_owned();
    assert_eq!(
        support::selected_of(cx, s.win, ids::profile_pick_hud(&rays)),
        Some(true)
    );
    assert_eq!(
        support::selected_of(cx, s.win, ids::profile_pick_hud(budhud.folder_name())),
        Some(false)
    );
    let custom = env.game.join("tf/custom");
    let before = files_under(&custom);
    support::click_id(cx, s.win, ids::profile_pick_hud(budhud.folder_name()));
    wait_idle(cx, &s, &editor);
    let after = saved(&s, comp.id());
    assert_eq!(
        after.hud().map(|h| h.folder_name()),
        Some(budhud.folder_name())
    );
    assert_eq!(after.hitsound(), comp.hitsound());
    assert_eq!(after.crosshairs(), comp.crosshairs());
    assert_eq!(
        support::label_of(cx, s.win, ids::profile_slot("hud")).as_deref(),
        Some(budhud.name())
    );
    assert_eq!(files_under(&custom), before, "an edit touched the game");
}
#[gpui_kit::test]
fn search_narrows_the_gallery(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let comp = full_profile(&s, &env, "Comp");
    let budhud = seed_hud(&s, &env, "budhud");
    let page = open_profiles(cx, &s);
    let editor = open_editor(cx, &s, &page, comp.id());
    let input = cx.update(|cx| editor.read(cx).search_input().clone());
    cx.update_window(s.win, |_, window, cx| {
        input.update(cx, |st, cx| st.set_value("bud", window, cx))
    })
    .unwrap();
    support::settle(cx, s.win);
    assert!(support::present(
        cx,
        s.win,
        ids::profile_pick_hud(budhud.folder_name())
    ));
    let rays = comp.hud().unwrap().folder_name().to_owned();
    assert!(!support::present(cx, s.win, ids::profile_pick_hud(&rays)));
}

#[gpui_kit::test]
fn clear_empties_a_slot_at_once(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let comp = full_profile(&s, &env, "Comp");
    let page = open_profiles(cx, &s);
    let editor = open_editor(cx, &s, &page, comp.id());
    support::click_id(cx, s.win, ids::profile_slot_clear("hud"));
    wait_idle(cx, &s, &editor);
    assert!(saved(&s, comp.id()).hud().is_none());
    assert_eq!(
        support::label_of(cx, s.win, ids::profile_slot("hud")).as_deref(),
        Some("None")
    );
    assert!(!support::present(cx, s.win, ids::profile_slot_clear("hud")));
    assert_eq!(saved(&s, comp.id()).hitsound(), comp.hitsound());
}
#[gpui_kit::test]
fn picking_a_hud_deleted_meanwhile_toasts(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let comp = full_profile(&s, &env, "Comp");
    let budhud = seed_hud(&s, &env, "budhud");
    let page = open_profiles(cx, &s);
    let editor = open_editor(cx, &s, &page, comp.id());
    s.services.library.delete_hud(budhud.id()).unwrap();
    support::click_id(cx, s.win, ids::profile_pick_hud(budhud.folder_name()));
    toast_is(cx, &s, "That HUD is no longer in your library");
    support::wait_real(cx, "the gallery to reload", |cx| {
        !support::present(cx, s.win, ids::profile_pick_hud(budhud.folder_name()))
    });
    wait_idle(cx, &s, &editor);
    assert_eq!(saved(&s, comp.id()).hud(), comp.hud());
}

#[gpui_kit::test]
fn an_empty_library_points_to_the_store(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let vanilla = empty_profile(&s, "Vanilla");
    let page = open_profiles(cx, &s);
    let _editor = open_editor(cx, &s, &page, vanilla.id());
    assert_eq!(
        support::label_of(cx, s.win, ids::profile_picker_empty("hud")).as_deref(),
        Some("No HUDs in your library yet")
    );
    support::click_id(cx, s.win, ids::profile_slot("killsound"));
    assert_eq!(
        support::label_of(cx, s.win, ids::profile_picker_empty("killsound")).as_deref(),
        Some("No sounds in your library yet")
    );
    support::click_id(cx, s.win, ids::profile_picker_store("killsound"));
    assert_eq!(cx.update(|cx| s.shell.read(cx).page()), Page::SoundStore);
}

fn sound_named(s: &support::ShellWindow, name: &str) -> SoundEntry {
    s.services
        .library
        .list_sounds()
        .unwrap()
        .into_iter()
        .find(|x| x.name() == name)
        .unwrap()
}

#[gpui_kit::test]
fn picking_sounds_fills_each_sound_slot(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let comp = full_profile(&s, &env, "Comp");
    let (ding, bell) = (sound_named(&s, "Classic Ding"), sound_named(&s, "Bell"));
    let page = open_profiles(cx, &s);
    let editor = open_editor(cx, &s, &page, comp.id());
    support::click_id(cx, s.win, ids::profile_slot("hitsound"));
    assert!(support::present(cx, s.win, ids::profile_picker("hitsound")));
    support::click_id(
        cx,
        s.win,
        ids::profile_pick_sound("hitsound", bell.folder_name()),
    );
    wait_idle(cx, &s, &editor);
    support::click_id(cx, s.win, ids::profile_slot("killsound"));
    support::click_id(
        cx,
        s.win,
        ids::profile_pick_sound("killsound", ding.folder_name()),
    );
    wait_idle(cx, &s, &editor);
    let after = saved(&s, comp.id());
    assert_eq!(after.hitsound().map(|x| x.name()), Some("Bell"));
    assert_eq!(after.killsound().map(|x| x.name()), Some("Classic Ding"));
}

#[gpui_kit::test]
fn the_play_buttons_preview_without_picking(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let comp = full_profile(&s, &env, "Comp");
    let bell = sound_named(&s, "Bell");
    let page = open_profiles(cx, &s);
    let editor = open_editor(cx, &s, &page, comp.id());
    support::click_id(cx, s.win, ids::profile_slot_play("hitsound"));
    support::wait_real(cx, "the slot preview", |_| s.audio.plays().len() == 1);
    support::click_id(cx, s.win, ids::profile_slot("hitsound"));
    support::click_id(cx, s.win, ids::profile_pick_play(bell.folder_name()));
    support::wait_real(cx, "the row preview", |_| s.audio.plays().len() >= 2);
    wait_idle(cx, &s, &editor);
    assert_eq!(saved(&s, comp.id()).hitsound(), comp.hitsound());
}

// A gallery pick and a slot Clear in the same frame: the queue keeps both.
#[gpui_kit::test]
fn two_clicks_in_one_frame_both_land(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let comp = full_profile(&s, &env, "Comp");
    let budhud = seed_hud(&s, &env, "budhud");
    let page = open_profiles(cx, &s);
    let editor = open_editor(cx, &s, &page, comp.id());
    support::settle(cx, s.win);
    support::click_without_settling(cx, s.win, ids::profile_pick_hud(budhud.folder_name()));
    support::click_without_settling(cx, s.win, ids::profile_slot_clear("hitsound"));
    wait_idle(cx, &s, &editor);
    let after = saved(&s, comp.id());
    assert_eq!(
        after.hud().map(|h| h.folder_name()),
        Some(budhud.folder_name())
    );
    assert!(after.hitsound().is_none());
    assert_eq!(after.killsound(), comp.killsound());
}

#[gpui_kit::test]
fn editing_the_active_profile_says_changes_are_not_applied(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let comp = full_profile(&s, &env, "Comp");
    apply(&s, &env, &comp);
    let page = open_profiles(cx, &s);
    let editor = open_editor(cx, &s, &page, comp.id());
    let custom = env.game.join("tf/custom");
    let before = files_under(&custom);
    support::click_id(cx, s.win, ids::profile_slot_clear("hud"));
    wait_idle(cx, &s, &editor);
    assert_eq!(
        support::label_of(cx, s.win, ids::PROFILE_EDITOR_NOT_APPLIED).as_deref(),
        Some("Changes not applied yet")
    );
    assert!(cx.update(|cx| editor.read(cx).not_applied()));
    assert!(support::present(cx, s.win, ids::PROFILE_EDITOR_ACTIVATE));
    assert_eq!(
        files_under(&custom),
        before,
        "only Activate touches the game"
    );
    support::click_id(cx, s.win, ids::PROFILE_EDITOR_BACK);
    support::wait_real(cx, "no active profile", |cx| {
        support::present(cx, s.win, ids::PROFILES_NO_ACTIVE)
    });
}

#[gpui_kit::test]
fn a_deleted_item_shows_missing(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let comp = full_profile(&s, &env, "Comp");
    let rays = s.services.library.list_huds().unwrap()[0].clone();
    s.services.library.delete_hud(rays.id()).unwrap();
    let page = open_profiles(cx, &s);
    let editor = open_editor(cx, &s, &page, comp.id());
    let hud = comp.hud().unwrap().name().to_owned();
    assert_eq!(
        support::label_of(cx, s.win, ids::profile_slot_missing("hud")).as_deref(),
        Some(format!("Missing: {hud}").as_str())
    );
    assert!(support::present(cx, s.win, ids::profile_slot_clear("hud")));
    let ding = sound_named(&s, "Classic Ding");
    s.services.library.delete_sound(ding.id(), None).unwrap();
    cx.update(LibraryEvents::changed);
    support::wait_real(cx, "the hitsound to go missing", |cx| {
        support::present(cx, s.win, ids::profile_slot_missing("hitsound"))
    });
    assert!(!support::present(
        cx,
        s.win,
        ids::profile_slot_play("hitsound")
    ));
    wait_idle(cx, &s, &editor);
}

fn scout_first() -> &'static str {
    weapons()
        .iter()
        .find(|w| w.class() == TfClass::Scout)
        .unwrap()
        .script_name()
}

/// Brings `script`'s row to the top of the compact weapon list when it is out of view, by
/// wheeling the list over its first Scout row, then clicks `target`.
fn click_in_row(
    cx: &mut TestAppContext,
    s: &support::ShellWindow,
    script: &str,
    target: ElementId,
) {
    let row = ids::weapon_row(script);
    support::settle(cx, s.win);
    let visible = cx
        .update_window(s.win, |_, w, _| {
            w.try_find(row.clone()).is_some_and(|e| e.visible())
        })
        .unwrap();
    if !visible {
        let first = ids::weapon_row(scout_first());
        let top = support::bounds_of(cx, s.win, first.clone()).expect("the first row");
        let at = support::bounds_of(cx, s.win, row.clone()).expect("the row");
        let dy = top.origin.y - at.origin.y;
        cx.update_window(s.win, |_, w, cx| {
            w.scroll(
                first,
                ScrollDelta::Pixels(gpui_kit::point(gpui_kit::px(0.), dy)),
                cx,
            )
        })
        .unwrap();
    }
    support::click_id(cx, s.win, target);
}

fn select_builtin(
    cx: &mut TestAppContext,
    s: &support::ShellWindow,
    e: &Entity<ProfileEditor>,
    ix: usize,
) -> CrosshairRef {
    let pick = CrosshairRef::Builtin(s.services.builtins.all()[ix].name().to_owned());
    let picker = cx.update(|cx| e.read(cx).picker().clone());
    cx.update(|cx| picker.update(cx, |p, cx| p.select(Some(pick.clone()), cx)));
    support::settle(cx, s.win);
    pick
}

fn scout_count() -> usize {
    weapons()
        .iter()
        .filter(|w| w.class() == TfClass::Scout)
        .count()
}

#[gpui_kit::test]
fn the_crosshair_slot_shows_saved_weapons(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let comp = full_profile(&s, &env, "Comp");
    let page = open_profiles(cx, &s);
    let _editor = open_editor(cx, &s, &page, comp.id());
    support::click_id(cx, s.win, ids::profile_slot("crosshairs"));
    assert!(support::present(
        cx,
        s.win,
        ids::profile_picker("crosshairs")
    ));
    assert_eq!(
        support::label_of(cx, s.win, ids::weapon_assigned("tf_weapon_scattergun")).as_deref(),
        Some(s.services.builtins.all()[0].display_name())
    );
    assert!(support::present(
        cx,
        s.win,
        ids::weapon_clear("tf_weapon_scattergun")
    ));
}

#[gpui_kit::test]
fn a_weapon_click_saves_the_picked_crosshair(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let vanilla = empty_profile(&s, "Vanilla");
    let page = open_profiles(cx, &s);
    let editor = open_editor(cx, &s, &page, vanilla.id());
    support::click_id(cx, s.win, ids::profile_slot("crosshairs"));
    let custom = env.game.join("tf/custom");
    let before = files_under(&custom);
    let second = select_builtin(cx, &s, &editor, 1);
    click_in_row(
        cx,
        &s,
        "tf_weapon_scattergun",
        ids::weapon_row("tf_weapon_scattergun"),
    );
    wait_idle(cx, &s, &editor);
    let after = saved(&s, vanilla.id());
    assert_eq!(after.crosshairs().len(), 1);
    assert_eq!(
        after.crosshairs()[0].weapon_script(),
        "tf_weapon_scattergun"
    );
    let CrosshairRef::Builtin(name) = second else {
        unreachable!()
    };
    assert_eq!(
        after.crosshairs()[0].crosshair(),
        &ProfileCrosshairRef::Builtin(name)
    );
    assert!(
        s.services.library.assignments().unwrap().is_empty(),
        "the live table moved"
    );
    assert_eq!(files_under(&custom), before, "an edit touched the game");
}

#[gpui_kit::test]
fn a_bulk_button_sets_every_target(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let vanilla = empty_profile(&s, "Vanilla");
    let page = open_profiles(cx, &s);
    let editor = open_editor(cx, &s, &page, vanilla.id());
    support::click_id(cx, s.win, ids::profile_slot("crosshairs"));
    select_builtin(cx, &s, &editor, 0);
    support::click_id(cx, s.win, ids::CROSSHAIRS_BULK_ALL);
    wait_idle(cx, &s, &editor);
    assert_eq!(saved(&s, vanilla.id()).crosshairs().len(), scout_count());
    assert_eq!(
        support::label_of(cx, s.win, ids::profile_slot("crosshairs")),
        Some(format!("{} weapons", scout_count()))
    );
    assert!(s.services.library.assignments().unwrap().is_empty());
}

#[gpui_kit::test]
fn a_row_clear_removes_one_weapon(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let first = CrosshairRef::Builtin(s.services.builtins.all()[0].name().to_owned());
    let draft = ProfileDraft::new("Two").with_crosshairs(vec![
        DraftCrosshair::Set {
            weapon_script: "tf_weapon_scattergun".into(),
            crosshair: first.clone(),
        },
        DraftCrosshair::Set {
            weapon_script: "tf_weapon_pistol_scout".into(),
            crosshair: first,
        },
    ]);
    let two = s.services.library.create_profile(&draft).unwrap();
    let page = open_profiles(cx, &s);
    let editor = open_editor(cx, &s, &page, two.id());
    support::click_id(cx, s.win, ids::profile_slot("crosshairs"));
    click_in_row(
        cx,
        &s,
        "tf_weapon_scattergun",
        ids::weapon_clear("tf_weapon_scattergun"),
    );
    wait_idle(cx, &s, &editor);
    let left: Vec<String> = saved(&s, two.id())
        .crosshairs()
        .iter()
        .map(|c| c.weapon_script().to_owned())
        .collect();
    assert_eq!(left, ["tf_weapon_pistol_scout"]);
}

#[gpui_kit::test]
fn clear_all_asks_first_and_cancel_keeps_them(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let comp = full_profile(&s, &env, "Comp");
    let page = open_profiles(cx, &s);
    let editor = open_editor(cx, &s, &page, comp.id());
    support::click_id(cx, s.win, ids::profile_slot_clear("crosshairs"));
    assert_eq!(
        support::label_of(cx, s.win, ids::CONFIRM_DESCRIPTION).as_deref(),
        Some("Remove all crosshairs from this profile?")
    );
    support::click_id(cx, s.win, ids::CONFIRM_CANCEL);
    wait_idle(cx, &s, &editor);
    assert_eq!(saved(&s, comp.id()).crosshairs().len(), 1);
    support::click_id(cx, s.win, ids::profile_slot_clear("crosshairs"));
    support::click_id(cx, s.win, ids::CONFIRM_OK);
    support::wait_real(cx, "the crosshairs to clear", |_| {
        saved(&s, comp.id()).crosshairs().is_empty()
    });
    wait_idle(cx, &s, &editor);
    assert_eq!(
        support::label_of(cx, s.win, ids::profile_slot("crosshairs")).as_deref(),
        Some("Default crosshairs")
    );
}

#[gpui_kit::test]
fn a_missing_library_crosshair_reads_missing(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let png = env.tmp.path().join("z.png");
    std::fs::write(&png, support::crosshair_png([255, 0, 0, 255])).unwrap();
    let zebra = s
        .services
        .library
        .import_crosshair(&png, "Zebra", Provenance::Local)
        .unwrap();
    let draft = ProfileDraft::new("Z").with_crosshairs(vec![DraftCrosshair::Set {
        weapon_script: "tf_weapon_scattergun".into(),
        crosshair: CrosshairRef::Library(zebra.id().clone()),
    }]);
    let z = s.services.library.create_profile(&draft).unwrap();
    s.services.library.delete_crosshair(zebra.id()).unwrap();
    let page = open_profiles(cx, &s);
    let _editor = open_editor(cx, &s, &page, z.id());
    support::click_id(cx, s.win, ids::profile_slot("crosshairs"));
    let label = support::label_of(cx, s.win, ids::weapon_assigned("tf_weapon_scattergun")).unwrap();
    assert!(label.ends_with(" (missing)"), "{label}");
}

#[gpui_kit::test]
fn assigning_a_deleted_crosshair_toasts(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let png = env.tmp.path().join("z.png");
    std::fs::write(&png, support::crosshair_png([255, 0, 0, 255])).unwrap();
    let zebra = s
        .services
        .library
        .import_crosshair(&png, "Zebra", Provenance::Local)
        .unwrap();
    let vanilla = empty_profile(&s, "Vanilla");
    let page = open_profiles(cx, &s);
    let editor = open_editor(cx, &s, &page, vanilla.id());
    support::click_id(cx, s.win, ids::profile_slot("crosshairs"));
    let picker = cx.update(|cx| editor.read(cx).picker().clone());
    let pick = CrosshairRef::Library(zebra.id().clone());
    cx.update(|cx| picker.update(cx, |p, cx| p.select(Some(pick), cx)));
    s.services.library.delete_crosshair(zebra.id()).unwrap();
    support::click_id(cx, s.win, ids::CROSSHAIRS_BULK_ALL);
    toast_is(cx, &s, "That crosshair is no longer in your library");
    wait_idle(cx, &s, &editor);
    assert!(saved(&s, vanilla.id()).crosshairs().is_empty());
}

// The sound list builds only the rows in view, yet the last of a long library can be reached.
#[gpui_kit::test]
fn a_long_sound_library_scrolls_to_its_last_row(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let comp = empty_profile(&s, "Comp");
    for n in 0..60 {
        seed_sound(&s, &env, &format!("Sound {n:02}"));
    }
    let last = s.services.library.list_sounds().unwrap().pop().unwrap();
    let page = open_profiles(cx, &s);
    let editor = open_editor(cx, &s, &page, comp.id());
    support::click_id(cx, s.win, ids::profile_slot("hitsound"));
    let row = ids::profile_pick_sound("hitsound", last.folder_name());
    assert!(
        !support::present(cx, s.win, row.clone()),
        "rows out of view are not built"
    );
    let down = gpui_kit::ScrollDelta::Lines(gpui_kit::point(0., -1000.));
    cx.update_window(s.win, |_, w, cx| {
        w.scroll(ids::PROFILE_PICKER_SOUNDS, down, cx)
    })
    .unwrap();
    support::settle(cx, s.win);
    support::click_id(cx, s.win, row);
    wait_idle(cx, &s, &editor);
    assert_eq!(
        saved(&s, comp.id()).hitsound().map(|x| x.folder_name()),
        Some(last.folder_name())
    );
}

// A slot change stops the editor's own preview, never a sound started elsewhere.
#[gpui_kit::test]
fn changing_slot_stops_only_the_editors_preview(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let comp = full_profile(&s, &env, "Comp");
    let page = open_profiles(cx, &s);
    let _editor = open_editor(cx, &s, &page, comp.id());
    let player = cx
        .update_window(s.win, |_, w, cx| sound_player(w, cx))
        .unwrap();
    let now = NowPlaying {
        key: "elsewhere".into(),
        name: "Ding".into(),
        subtitle: "".into(),
    };
    cx.update(|cx| player.update(cx, |p, cx| p.toggle(now, |_| Ok(vec![1]), cx).detach()));
    support::wait_real(cx, "the outside play", |_| s.audio.plays().len() == 1);
    support::click_id(cx, s.win, ids::profile_slot("hitsound"));
    let key = cx.update(|cx| player.read(cx).now_playing().map(|n| n.key.to_string()));
    assert_eq!(key.as_deref(), Some("elsewhere"), "a slot change kept it");
    support::click_id(cx, s.win, ids::profile_slot_play("hitsound"));
    support::wait_real(cx, "the slot preview", |_| s.audio.plays().len() == 2);
    support::click_id(cx, s.win, ids::profile_slot("killsound"));
    assert!(cx.update(|cx| player.read(cx).now_playing().is_none()));
}

/// Opens the New profile menu by its caret, the kit's "popup" button inside the split button.
fn open_new_menu(cx: &mut TestAppContext, s: &support::ShellWindow) {
    support::settle(cx, s.win);
    cx.update_window(s.win, |_, w, cx| {
        w.within(ids::PROFILES_NEW_MENU).click("popup", cx)
    })
    .unwrap();
    support::settle(cx, s.win);
}

fn wait_editor(
    cx: &mut TestAppContext,
    s: &support::ShellWindow,
    page: &Entity<ProfilesPage>,
) -> Entity<ProfileEditor> {
    support::wait_real(cx, "the editor", |cx| {
        cx.update(|cx| page.read(cx).editor().is_some())
    });
    let editor = cx.update(|cx| page.read(cx).editor()).unwrap();
    wait_idle(cx, s, &editor);
    editor
}

#[gpui_kit::test]
fn new_profile_saves_the_installed_setup_and_opens_it(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let hud = seed_hud(&s, &env, "rayshud");
    s.services
        .library
        .install_hud(hud.id(), &tf2(&env))
        .unwrap();
    let ding = seed_sound(&s, &env, "Ding");
    s.services
        .library
        .install_sound(ding.id(), SoundKind::Hitsound, &tf2(&env))
        .unwrap();
    let first = s.services.builtins.all()[0].name().to_owned();
    s.services
        .library
        .assign("tf_weapon_scattergun", &CrosshairRef::Builtin(first))
        .unwrap();
    let page = open_profiles(cx, &s);
    support::click_id(cx, s.win, ids::PROFILES_EMPTY_NEW);
    let editor = wait_editor(cx, &s, &page);
    toast_is(cx, &s, "Profile created from your current setup");
    let profiles = s.services.library.profiles().unwrap();
    assert_eq!(profiles.len(), 1);
    let made = &profiles[0];
    assert_eq!(made.name(), "Profile 1");
    assert_eq!(made.hud().map(|h| h.folder_name()), Some("rayshud"));
    assert_eq!(made.hitsound().map(|h| h.name()), Some("Ding"));
    assert!(made.killsound().is_none());
    assert_eq!(made.crosshairs().len(), 1);
    let input = cx.update(|cx| editor.read(cx).name_input().clone());
    let (focused, selected) = cx
        .update_window(s.win, |_, w, cx| {
            use gpui_kit::Focusable as _;
            let st = input.read(cx);
            (
                st.focus_handle(cx).is_focused(w),
                st.selected_value().to_string(),
            )
        })
        .unwrap();
    assert!(focused, "the name field has the keyboard");
    assert_eq!(selected, "Profile 1");
    let row = &support::log_rows(&s.services)[0];
    assert_eq!(
        (row.category(), row.action(), row.message()),
        (
            "profile",
            "save_current",
            "Saved current setup as profile: Profile 1"
        )
    );
}

#[gpui_kit::test]
fn start_empty_from_the_menu_ignores_what_is_installed(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let hud = seed_hud(&s, &env, "rayshud");
    s.services
        .library
        .install_hud(hud.id(), &tf2(&env))
        .unwrap();
    empty_profile(&s, "Profile 1");
    let page = open_profiles(cx, &s);
    open_new_menu(cx, &s);
    support::click_id(cx, s.win, ids::PROFILES_NEW_EMPTY);
    let editor = wait_editor(cx, &s, &page);
    toast_is(cx, &s, "Empty profile created");
    let made = cx.update(|cx| editor.read(cx).saved().clone());
    assert_eq!(made.name(), "Profile 2");
    assert!(made.hud().is_none() && made.crosshairs().is_empty());
    let row = &support::log_rows(&s.services)[0];
    assert_eq!(
        (row.action(), row.message()),
        ("create", "Created profile: Profile 2")
    );
}

#[gpui_kit::test]
fn the_empty_state_offers_both_ways(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let page = open_profiles(cx, &s);
    assert!(support::present(cx, s.win, ids::PROFILES_EMPTY_NEW));
    support::click_id(cx, s.win, ids::PROFILES_EMPTY_START_EMPTY);
    let editor = wait_editor(cx, &s, &page);
    assert_eq!(
        cx.update(|cx| editor.read(cx).saved().name().to_owned()),
        "Profile 1"
    );
}

// The page loaded before "Profile 1" existed, so it asks for that name; core refuses it and
// the create moves on to the next free number.
#[gpui_kit::test]
fn a_name_taken_in_a_race_moves_to_the_next_number(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let page = open_profiles(cx, &s);
    empty_profile(&s, "Profile 1");
    support::click_id(cx, s.win, ids::PROFILES_EMPTY_START_EMPTY);
    let editor = wait_editor(cx, &s, &page);
    assert_eq!(
        cx.update(|cx| editor.read(cx).saved().name().to_owned()),
        "Profile 2"
    );
    assert_eq!(s.services.library.profiles().unwrap().len(), 2);
    support::settle(cx, s.win);
    assert!(
        support::present(cx, s.win, ids::PROFILE_EDITOR),
        "a reload closed the new editor"
    );
}

// A load whose job read the library before the create lacks the new profile; its result must
// not close the editor the create opens.
#[gpui_kit::test]
fn a_load_running_during_a_create_leaves_the_new_editor_open(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let page = open_profiles(cx, &s);
    let update = |cx: &mut TestAppContext,
                  f: fn(
        &mut ProfilesPage,
        &mut gpui_kit::Window,
        &mut gpui_kit::Context<ProfilesPage>,
    )| {
        cx.update_window(s.win, |_, window, cx| {
            page.update(cx, |p, cx| f(p, window, cx))
        })
        .unwrap();
        while cx.dispatcher.tick(true) {}
    };
    update(cx, |p, _, cx| p.reload(cx));
    update(cx, |p, window, cx| p.new_profile(false, window, cx));
    let editor = wait_editor(cx, &s, &page);
    let made = cx.update(|cx| editor.read(cx).id().clone());
    support::settle(cx, s.win);
    let open = cx.update(|cx| page.read(cx).editor().map(|e| e.read(cx).id().clone()));
    assert_eq!(open, Some(made), "a load closed the new editor");
}

#[gpui_kit::test]
fn without_tf2_only_start_empty_works(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, Tf2State::Missing { saved: None });
    let page = open_profiles(cx, &s);
    assert!(support::disabled_of(cx, s.win, ids::PROFILES_EMPTY_NEW));
    assert!(!support::disabled_of(
        cx,
        s.win,
        ids::PROFILES_EMPTY_START_EMPTY
    ));
    empty_profile(&s, "Old");
    cx.update(LibraryEvents::changed);
    support::wait_real(cx, "the grid", |cx| {
        support::present(cx, s.win, ids::PROFILES_NEW)
    });
    assert!(support::disabled_of(cx, s.win, ids::PROFILES_NEW));
    support::click_id(cx, s.win, ids::PROFILES_NEW);
    assert!(cx.update(|cx| page.read(cx).editor()).is_none());
    open_new_menu(cx, &s);
    support::click_id(cx, s.win, ids::PROFILES_NEW_EMPTY);
    let editor = wait_editor(cx, &s, &page);
    assert_eq!(
        cx.update(|cx| editor.read(cx).saved().name().to_owned()),
        "Profile 1"
    );
}
