use crate::support;

use gpui_kit::component::WindowExt as _;
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{
    AppContext as _, Entity, InteractiveElement as _, ParentElement as _, ScrollDelta,
    TestAppContext, TestSupportExt as _,
};
use tf2tools_app::ids;
use tf2tools_app::pages::profiles::ProfilesPage;
use tf2tools_app::pages::profiles::probe::Tf2Probe;
use tf2tools_app::pages::profiles::wizard::ProfileWizard;
use tf2tools_app::pages::profiles::wizard_model::{CrosshairMode, WizardStep};
use tf2tools_app::services::Tf2State;
use tf2tools_app::shell::route::Page;
use tf2tools_core::{
    CrosshairRef, DraftCrosshair, DraftSlot, HudEntry, HudId, Profile, ProfileDraft, Provenance,
    SoundEntry, SoundId, Tf2Install,
};

fn found(env: &support::Env) -> Tf2State {
    Tf2State::Found(Tf2Install::from_path(&env.game).unwrap())
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

fn has_dialog(cx: &mut TestAppContext, s: &support::ShellWindow) -> bool {
    support::settle(cx, s.win);
    cx.update_window(s.win, |_, w, cx| w.has_active_dialog(cx))
        .unwrap()
}

fn toast_text(cx: &mut TestAppContext, s: &support::ShellWindow) -> String {
    support::wait_real(cx, "the toast", |cx| {
        support::present(cx, s.win, ids::TOAST)
    });
    support::label_of(cx, s.win, ids::TOAST).unwrap()
}

/// Clicks `button` and waits for the wizard it opens to load.
fn open_wizard(
    cx: &mut TestAppContext,
    s: &support::ShellWindow,
    page: &Entity<ProfilesPage>,
    button: impl Into<gpui_kit::ElementId>,
) -> Entity<ProfileWizard> {
    support::click_id(cx, s.win, button);
    let wizard = cx.update(|cx| page.read(cx).wizard()).expect("a wizard");
    support::wait_real(cx, "the wizard to load", |cx| {
        cx.update(|cx| wizard.read(cx).is_loaded())
    });
    support::settle(cx, s.win);
    wizard
}

fn set_name(
    cx: &mut TestAppContext,
    s: &support::ShellWindow,
    w: &Entity<ProfileWizard>,
    text: &str,
) {
    let input = cx.update(|cx| w.read(cx).name_input().clone());
    cx.update_window(s.win, |_, window, cx| {
        input.update(cx, |st, cx| st.set_value(text, window, cx))
    })
    .unwrap();
    support::settle(cx, s.win);
}

fn step(cx: &mut TestAppContext, w: &Entity<ProfileWizard>) -> WizardStep {
    cx.update(|cx| w.read(cx).step())
}

fn pick_hud(
    cx: &mut TestAppContext,
    s: &support::ShellWindow,
    w: &Entity<ProfileWizard>,
    v: DraftSlot<HudId>,
) {
    let select = cx.update(|cx| w.read(cx).hud_select().clone());
    cx.update_window(s.win, |_, window, cx| {
        select.update(cx, |st, cx| st.set_selected_value(&v, window, cx))
    })
    .unwrap();
    support::settle(cx, s.win);
}

fn pick_hitsound(
    cx: &mut TestAppContext,
    s: &support::ShellWindow,
    w: &Entity<ProfileWizard>,
    v: DraftSlot<SoundId>,
) {
    let select = cx.update(|cx| w.read(cx).hitsound_select().clone());
    cx.update_window(s.win, |_, window, cx| {
        select.update(cx, |st, cx| st.set_selected_value(&v, window, cx))
    })
    .unwrap();
    support::settle(cx, s.win);
}

/// Name "Comp", then Next to `to`.
fn walk_to(
    cx: &mut TestAppContext,
    s: &support::ShellWindow,
    w: &Entity<ProfileWizard>,
    to: WizardStep,
) {
    set_name(cx, s, w, "Comp");
    while step(cx, w) < to {
        support::click_id(cx, s.win, ids::WIZARD_NEXT);
    }
}

#[gpui_kit::test]
fn create_new_opens_the_wizard_on_the_name_step(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let page = open_profiles(cx, &s);
    let w = open_wizard(cx, &s, &page, ids::PROFILES_CREATE);
    assert!(has_dialog(cx, &s));
    assert!(support::present(cx, s.win, ids::WIZARD_DIALOG));
    assert_eq!(
        support::label_of(cx, s.win, ids::WIZARD_TITLE).as_deref(),
        Some("Create New Profile")
    );
    assert_eq!(
        support::label_of(cx, s.win, ids::WIZARD_DESCRIPTION).as_deref(),
        Some("Choose a name for your profile.")
    );
    assert_eq!(step(cx, &w), WizardStep::Name);
    assert!(!support::present(cx, s.win, ids::WIZARD_BACK));
    for n in 1..=4 {
        assert!(support::present(cx, s.win, ids::wizard_step(n)), "step {n}");
    }
}

// The name field takes the keyboard as the wizard opens, for Create and Edit alike.
#[gpui_kit::test]
fn the_wizard_focuses_its_name_field(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let comp = s
        .services
        .library
        .create_profile(&ProfileDraft::new("Comp"))
        .unwrap();
    let page = open_profiles(cx, &s);
    let name_focused = |cx: &mut TestAppContext, w: &Entity<ProfileWizard>| {
        let input = cx.update(|cx| w.read(cx).name_input().clone());
        cx.update_window(s.win, |_, win, cx| {
            use gpui_kit::Focusable as _;
            input.read(cx).focus_handle(cx).is_focused(win)
        })
        .unwrap()
    };
    let w = open_wizard(cx, &s, &page, ids::PROFILES_CREATE);
    assert!(name_focused(cx, &w), "Create");
    support::click_id(cx, s.win, ids::WIZARD_CANCEL);
    let w = open_wizard(cx, &s, &page, ids::profile_edit(comp.id()));
    assert!(name_focused(cx, &w), "Edit");
}

#[gpui_kit::test]
fn the_empty_state_create_opens_it_too(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let page = open_profiles(cx, &s);
    let _w = open_wizard(cx, &s, &page, ids::PROFILES_EMPTY_CREATE);
    assert!(has_dialog(cx, &s));
}

#[gpui_kit::test]
fn a_blank_name_keeps_next_disabled(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let page = open_profiles(cx, &s);
    let w = open_wizard(cx, &s, &page, ids::PROFILES_CREATE);
    support::click_id(cx, s.win, ids::WIZARD_NEXT);
    assert_eq!(step(cx, &w), WizardStep::Name);
    set_name(cx, &s, &w, "   ");
    support::click_id(cx, s.win, ids::WIZARD_NEXT);
    support::click_id(cx, s.win, ids::wizard_step(4));
    assert_eq!(step(cx, &w), WizardStep::Name);
    set_name(cx, &s, &w, "Comp");
    support::click_id(cx, s.win, ids::WIZARD_NEXT);
    assert_eq!(step(cx, &w), WizardStep::Hud);
}

// Another profile's name, in any case, holds the wizard on the name step and says why; an
// edited profile keeps its own name.
#[gpui_kit::test]
fn a_taken_name_keeps_next_disabled(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let comp = s
        .services
        .library
        .create_profile(&ProfileDraft::new("Comp"))
        .unwrap();
    let page = open_profiles(cx, &s);
    let w = open_wizard(cx, &s, &page, ids::PROFILES_CREATE);
    set_name(cx, &s, &w, "comp");
    assert_eq!(
        support::label_of(cx, s.win, ids::WIZARD_NAME_TAKEN).as_deref(),
        Some("You already have a profile called Comp. Pick another name.")
    );
    support::click_id(cx, s.win, ids::WIZARD_NEXT);
    support::click_id(cx, s.win, ids::wizard_step(4));
    assert_eq!(step(cx, &w), WizardStep::Name);
    support::click_id(cx, s.win, ids::WIZARD_CANCEL);
    assert!(!has_dialog(cx, &s));

    let w = open_wizard(cx, &s, &page, ids::profile_edit(comp.id()));
    assert!(!support::present(cx, s.win, ids::WIZARD_NAME_TAKEN));
    support::click_id(cx, s.win, ids::WIZARD_NEXT);
    assert_eq!(step(cx, &w), WizardStep::Hud);
}

#[gpui_kit::test]
fn the_stepper_jumps_forward_only_and_back_returns(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let page = open_profiles(cx, &s);
    let w = open_wizard(cx, &s, &page, ids::PROFILES_CREATE);
    set_name(cx, &s, &w, "Comp");
    support::click_id(cx, s.win, ids::wizard_step(3));
    assert_eq!(step(cx, &w), WizardStep::Sounds);
    support::click_id(cx, s.win, ids::wizard_step(1));
    assert_eq!(step(cx, &w), WizardStep::Sounds);
    support::click_id(cx, s.win, ids::WIZARD_BACK);
    assert_eq!(step(cx, &w), WizardStep::Hud);
    support::click_id(cx, s.win, ids::wizard_step(4));
    assert!(!support::present(cx, s.win, ids::WIZARD_NEXT));
    assert_eq!(
        support::label_of(cx, s.win, ids::WIZARD_SAVE).as_deref(),
        Some("Create Profile")
    );
}

#[gpui_kit::test]
fn cancel_closes_without_saving(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let page = open_profiles(cx, &s);
    let w = open_wizard(cx, &s, &page, ids::PROFILES_CREATE);
    walk_to(cx, &s, &w, WizardStep::Crosshairs);
    support::click_id(cx, s.win, ids::WIZARD_CANCEL);
    assert!(!has_dialog(cx, &s));
    assert!(s.services.library.profiles().unwrap().is_empty());
}

#[gpui_kit::test]
fn an_empty_library_shows_the_hints(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let page = open_profiles(cx, &s);
    let w = open_wizard(cx, &s, &page, ids::PROFILES_CREATE);
    walk_to(cx, &s, &w, WizardStep::Hud);
    assert_eq!(
        support::label_of(cx, s.win, ids::WIZARD_HUD_EMPTY).as_deref(),
        Some("No HUDs in your library. Import or download one first.")
    );
    support::click_id(cx, s.win, ids::WIZARD_NEXT);
    assert_eq!(
        support::label_of(cx, s.win, ids::WIZARD_SOUNDS_EMPTY).as_deref(),
        Some("No sounds in your library. Import or download some first.")
    );
    assert!(!support::present(cx, s.win, ids::WIZARD_HITSOUND_PLAY));
}

#[gpui_kit::test]
fn the_play_button_previews_the_selected_hitsound(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let ding = seed_sound(&s, &env, "Ding");
    let page = open_profiles(cx, &s);
    let w = open_wizard(cx, &s, &page, ids::PROFILES_CREATE);
    walk_to(cx, &s, &w, WizardStep::Sounds);
    pick_hitsound(cx, &s, &w, DraftSlot::Item(ding.id().clone()));
    support::click_id(cx, s.win, ids::WIZARD_HITSOUND_PLAY);
    support::wait_real(cx, "the preview to play", |_| s.audio.plays().len() == 1);
}

#[gpui_kit::test]
fn create_saves_the_profile_and_reloads_the_page(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let hud = seed_hud(&s, &env, "rayshud");
    let ding = seed_sound(&s, &env, "Ding");
    let page = open_profiles(cx, &s);
    let w = open_wizard(cx, &s, &page, ids::PROFILES_CREATE);
    walk_to(cx, &s, &w, WizardStep::Hud);
    pick_hud(cx, &s, &w, DraftSlot::Item(hud.id().clone()));
    support::click_id(cx, s.win, ids::WIZARD_NEXT);
    pick_hitsound(cx, &s, &w, DraftSlot::Item(ding.id().clone()));
    support::click_id(cx, s.win, ids::WIZARD_NEXT);
    assert_eq!(cx.update(|cx| w.read(cx).mode()), CrosshairMode::None);
    support::click_id(cx, s.win, ids::WIZARD_SAVE);
    assert_eq!(
        toast_text(cx, &s),
        "Profile created. \"Comp\" is ready to activate"
    );
    assert!(!has_dialog(cx, &s));
    let profiles = s.services.library.profiles().unwrap();
    assert_eq!(profiles.len(), 1);
    let comp = &profiles[0];
    assert_eq!(comp.name(), "Comp");
    assert_eq!(comp.hud().map(|h| h.folder_name()), Some(hud.folder_name()));
    assert_eq!(comp.hitsound().map(|h| h.name()), Some("Ding"));
    assert!(comp.killsound().is_none() && comp.crosshairs().is_empty());
    support::wait_real(cx, "the page to reload", |cx| {
        cx.update(|cx| page.read(cx).profiles().len() == 1)
    });
    assert!(support::present(cx, s.win, ids::profile_card(comp.id())));
    let row = &support::log_rows(&s.services)[0];
    assert_eq!(
        (row.category(), row.action(), row.message()),
        ("profile", "create", "Created profile: Comp")
    );
}

#[gpui_kit::test]
fn copy_current_copies_the_live_assignments(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let first = s.services.builtins.all()[0].name().to_owned();
    s.services
        .library
        .assign(
            "tf_weapon_scattergun",
            &CrosshairRef::Builtin(first.clone()),
        )
        .unwrap();
    let page = open_profiles(cx, &s);
    let w = open_wizard(cx, &s, &page, ids::PROFILES_CREATE);
    walk_to(cx, &s, &w, WizardStep::Crosshairs);
    assert_eq!(
        support::label_of(cx, s.win, ids::WIZARD_MODE_COPY).as_deref(),
        Some("Copy current (1)")
    );
    support::click_id(cx, s.win, ids::WIZARD_MODE_COPY);
    assert_eq!(cx.update(|cx| w.read(cx).mode()), CrosshairMode::Copy);
    assert_eq!(
        support::label_of(cx, s.win, ids::WIZARD_COPY_NOTE).as_deref(),
        Some("Will copy your 1 current crosshair assignment into this profile.")
    );
    support::click_id(cx, s.win, ids::WIZARD_SAVE);
    let _ = toast_text(cx, &s);
    let comp = &s.services.library.profiles().unwrap()[0];
    assert_eq!(comp.crosshairs().len(), 1);
    assert_eq!(comp.crosshairs()[0].weapon_script(), "tf_weapon_scattergun");
}

#[gpui_kit::test]
fn copy_is_disabled_without_assignments(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let page = open_profiles(cx, &s);
    let w = open_wizard(cx, &s, &page, ids::PROFILES_CREATE);
    walk_to(cx, &s, &w, WizardStep::Crosshairs);
    assert_eq!(
        support::label_of(cx, s.win, ids::WIZARD_MODE_COPY).as_deref(),
        Some("Copy current (0)")
    );
    support::click_id(cx, s.win, ids::WIZARD_MODE_COPY);
    assert_eq!(cx.update(|cx| w.read(cx).mode()), CrosshairMode::None);
}

fn full_profile(s: &support::ShellWindow, env: &support::Env, name: &str) -> Profile {
    let hud = seed_hud(s, env, "rayshud");
    let hit = seed_sound(s, env, "Classic Ding");
    let first = s.services.builtins.all()[0].name().to_owned();
    let draft = ProfileDraft::new(name)
        .with_hud(DraftSlot::Item(hud.id().clone()))
        .with_hitsound(DraftSlot::Item(hit.id().clone()))
        .with_crosshairs(vec![DraftCrosshair::Set {
            weapon_script: "tf_weapon_scattergun".into(),
            crosshair: CrosshairRef::Builtin(first),
        }]);
    s.services.library.create_profile(&draft).unwrap()
}

/// Clicks `script`'s row, first wheeling the compact weapon list (over its first Scout row)
/// to bring it to the top when it is out of view; the wizard's list shows a few rows in 144 px.
fn click_weapon_row(cx: &mut TestAppContext, s: &support::ShellWindow, script: &str) {
    let row = ids::weapon_row(script);
    support::settle(cx, s.win);
    let visible = cx
        .update_window(s.win, |_, w, _| {
            w.try_find(row.clone()).is_some_and(|e| e.visible())
        })
        .unwrap();
    if !visible {
        let first = ids::weapon_row("tf_weapon_pep_brawler_blaster");
        let top = support::bounds_of(cx, s.win, first.clone()).expect("the first row");
        let target = support::bounds_of(cx, s.win, row.clone()).expect("the row");
        let dy = top.origin.y - target.origin.y;
        cx.update_window(s.win, |_, w, cx| {
            w.scroll(
                first,
                ScrollDelta::Pixels(gpui_kit::point(gpui_kit::px(0.), dy)),
                cx,
            )
        })
        .unwrap();
    }
    support::click_id(cx, s.win, row);
}

fn select_first_builtin(
    cx: &mut TestAppContext,
    s: &support::ShellWindow,
    w: &Entity<ProfileWizard>,
) -> CrosshairRef {
    let first = CrosshairRef::Builtin(s.services.builtins.all()[0].name().to_owned());
    let picker = cx.update(|cx| w.read(cx).picker().clone());
    cx.update(|cx| picker.update(cx, |p, cx| p.select(Some(first.clone()), cx)));
    support::settle(cx, s.win);
    first
}

#[gpui_kit::test]
fn customize_assigns_the_picked_crosshair_to_a_weapon(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let page = open_profiles(cx, &s);
    let w = open_wizard(cx, &s, &page, ids::PROFILES_CREATE);
    walk_to(cx, &s, &w, WizardStep::Crosshairs);
    support::click_id(cx, s.win, ids::WIZARD_MODE_CUSTOM);
    assert_eq!(cx.update(|cx| w.read(cx).mode()), CrosshairMode::Customize);
    click_weapon_row(cx, &s, "tf_weapon_scattergun");
    assert!(!support::present(cx, s.win, ids::WIZARD_ASSIGNED_COUNT));
    let _first = select_first_builtin(cx, &s, &w);
    click_weapon_row(cx, &s, "tf_weapon_scattergun");
    assert_eq!(
        support::label_of(cx, s.win, ids::weapon_assigned("tf_weapon_scattergun")).as_deref(),
        Some(s.services.builtins.all()[0].display_name())
    );
    assert_eq!(
        support::label_of(cx, s.win, ids::WIZARD_ASSIGNED_COUNT).as_deref(),
        Some("1 weapon assigned")
    );
    support::click_id(cx, s.win, ids::WIZARD_SAVE);
    let _ = toast_text(cx, &s);
    let comp = &s.services.library.profiles().unwrap()[0];
    assert_eq!(comp.crosshairs().len(), 1);
    assert_eq!(comp.crosshairs()[0].weapon_script(), "tf_weapon_scattergun");
}

#[gpui_kit::test]
fn bulk_in_the_wizard_touches_only_the_draft(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let page = open_profiles(cx, &s);
    let w = open_wizard(cx, &s, &page, ids::PROFILES_CREATE);
    walk_to(cx, &s, &w, WizardStep::Crosshairs);
    support::click_id(cx, s.win, ids::WIZARD_MODE_CUSTOM);
    select_first_builtin(cx, &s, &w);
    support::click_id(cx, s.win, ids::CROSSHAIRS_BULK_ALL);
    let scout = tf2tools_core::weapons()
        .iter()
        .filter(|w| w.class() == tf2tools_core::TfClass::Scout)
        .count();
    assert_eq!(
        support::label_of(cx, s.win, ids::WIZARD_ASSIGNED_COUNT),
        Some(format!("{scout} weapons assigned"))
    );
    assert!(s.services.library.assignments().unwrap().is_empty());
}

#[gpui_kit::test]
fn edit_prefills_the_profile(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let comp = full_profile(&s, &env, "Competitive");
    let page = open_profiles(cx, &s);
    let w = open_wizard(cx, &s, &page, ids::profile_edit(comp.id()));
    assert_eq!(
        support::label_of(cx, s.win, ids::WIZARD_TITLE).as_deref(),
        Some("Edit Profile")
    );
    let name = cx.update(|cx| w.read(cx).name_input().read(cx).value().to_string());
    assert_eq!(name, "Competitive");
    let hud = cx.update(|cx| w.read(cx).hud_select().read(cx).selected_value().cloned());
    let rayshud = s.services.library.list_huds().unwrap()[0].id().clone();
    assert_eq!(hud, Some(DraftSlot::Item(rayshud)));
    assert_eq!(cx.update(|cx| w.read(cx).mode()), CrosshairMode::Customize);
    support::click_id(cx, s.win, ids::wizard_step(4));
    assert_eq!(
        support::label_of(cx, s.win, ids::weapon_assigned("tf_weapon_scattergun")).as_deref(),
        Some(s.services.builtins.all()[0].display_name())
    );
    assert_eq!(
        support::label_of(cx, s.win, ids::WIZARD_SAVE).as_deref(),
        Some("Save Changes")
    );
}

#[gpui_kit::test]
fn edit_saves_in_place(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let comp = full_profile(&s, &env, "Competitive");
    let page = open_profiles(cx, &s);
    let w = open_wizard(cx, &s, &page, ids::profile_edit(comp.id()));
    set_name(cx, &s, &w, "Pubs");
    support::click_id(cx, s.win, ids::wizard_step(4));
    support::click_id(cx, s.win, ids::WIZARD_MODE_NONE);
    support::click_id(cx, s.win, ids::WIZARD_SAVE);
    assert_eq!(
        toast_text(cx, &s),
        "Profile updated. \"Pubs\" has been saved"
    );
    let profiles = s.services.library.profiles().unwrap();
    assert_eq!(profiles.len(), 1);
    assert_eq!(profiles[0].id(), comp.id());
    assert_eq!(profiles[0].name(), "Pubs");
    assert_eq!(
        profiles[0].hud().map(|h| h.folder_name()),
        comp.hud().map(|h| h.folder_name())
    );
    assert!(profiles[0].crosshairs().is_empty());
    let row = &support::log_rows(&s.services)[0];
    assert_eq!(
        (row.action(), row.message()),
        ("update", "Updated profile: Pubs")
    );
    assert_eq!(row.details(), None);
}

/// Applies `profile` to the fake game, so the page opens with it active.
fn apply(s: &support::ShellWindow, env: &support::Env, profile: &Profile) {
    let tf2 = Tf2Install::from_path(&env.game).unwrap();
    s.services
        .library
        .apply_profile(profile.id(), &tf2, &s.services.builtins, false)
        .unwrap();
}

#[gpui_kit::test]
fn changing_the_hud_of_the_active_profile_offers_activate_again(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let comp = full_profile(&s, &env, "Comp");
    let other = seed_hud(&s, &env, "budhud");
    apply(&s, &env, &comp);
    let page = open_profiles(cx, &s);
    assert_eq!(
        support::label_of(cx, s.win, ids::profile_activate(comp.id())).as_deref(),
        Some("Active")
    );
    let custom = env.game.join("tf/custom");
    let old_hud = comp.hud().unwrap().folder_name().to_owned();

    let w = open_wizard(cx, &s, &page, ids::profile_edit(comp.id()));
    pick_hud(cx, &s, &w, DraftSlot::Item(other.id().clone()));
    support::click_id(cx, s.win, ids::wizard_step(4));
    support::click_id(cx, s.win, ids::WIZARD_SAVE);
    assert_eq!(
        toast_text(cx, &s),
        "Profile updated. \"Comp\" was saved. Activate it to use the changes in game"
    );
    support::wait_real(cx, "the card to stop claiming Active", |cx| {
        support::label_of(cx, s.win, ids::profile_activate(comp.id())).as_deref()
            == Some("Activate")
    });
    assert!(!support::present(cx, s.win, ids::PROFILES_ACTIVE_NAME));
    assert!(!support::present(
        cx,
        s.win,
        ids::profile_active_badge(comp.id())
    ));
    // The save only recorded the change; the game still has the old HUD until Activate.
    assert!(custom.join(&old_hud).is_dir());
    assert!(!custom.join(other.folder_name()).exists());

    support::click_id(cx, s.win, ids::profile_activate(comp.id()));
    support::wait_real(cx, "the card to say Active again", |cx| {
        support::label_of(cx, s.win, ids::profile_activate(comp.id())).as_deref() == Some("Active")
    });
    assert!(custom.join(other.folder_name()).is_dir());
    assert!(!custom.join(&old_hud).exists());
}

#[gpui_kit::test]
fn renaming_the_active_profile_keeps_the_card_active(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let comp = full_profile(&s, &env, "Comp");
    apply(&s, &env, &comp);
    let page = open_profiles(cx, &s);
    let w = open_wizard(cx, &s, &page, ids::profile_edit(comp.id()));
    set_name(cx, &s, &w, "Pubs");
    support::click_id(cx, s.win, ids::wizard_step(4));
    support::click_id(cx, s.win, ids::WIZARD_SAVE);
    assert_eq!(
        toast_text(cx, &s),
        "Profile updated. \"Pubs\" has been saved"
    );
    support::wait_real(cx, "the renamed active card", |cx| {
        support::label_of(cx, s.win, ids::PROFILES_ACTIVE_NAME).as_deref() == Some("Pubs")
    });
    assert_eq!(
        support::label_of(cx, s.win, ids::profile_activate(comp.id())).as_deref(),
        Some("Active")
    );
    assert_eq!(
        s.services.library.active_profile().unwrap().as_ref(),
        Some(comp.id())
    );
}

#[gpui_kit::test]
fn a_missing_item_is_kept_through_an_edit(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let comp = full_profile(&s, &env, "Competitive");
    let hud = s.services.library.list_huds().unwrap()[0].clone();
    s.services.library.delete_hud(hud.id()).unwrap();
    let page = open_profiles(cx, &s);
    let w = open_wizard(cx, &s, &page, ids::profile_edit(comp.id()));
    let value = cx.update(|cx| w.read(cx).hud_select().read(cx).selected_value().cloned());
    assert_eq!(value, Some(DraftSlot::Keep));
    support::click_id(cx, s.win, ids::wizard_step(4));
    support::click_id(cx, s.win, ids::WIZARD_SAVE);
    let _ = toast_text(cx, &s);
    let saved = s.services.library.profile(comp.id()).unwrap();
    assert_eq!(saved.hud(), comp.hud());
    assert_eq!(saved.crosshairs(), comp.crosshairs());
}

#[gpui_kit::test]
fn a_save_in_flight_ignores_escape_and_spares_a_dialog_on_top(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let page = open_profiles(cx, &s);
    let w = open_wizard(cx, &s, &page, ids::PROFILES_CREATE);
    walk_to(cx, &s, &w, WizardStep::Crosshairs);
    // No settle between these steps, so the save's background job stays queued.
    cx.update_window(s.win, |_, win, cx| win.click(ids::WIZARD_SAVE, cx))
        .unwrap();
    assert!(cx.update(|cx| w.read(cx).is_saving()));
    cx.update_window(s.win, |_, win, cx| {
        win.press("escape", cx);
        win.render_frame(cx);
    })
    .unwrap();
    assert!(
        cx.update_window(s.win, |_, win, cx| win.has_active_dialog(cx))
            .unwrap(),
        "Escape closed the wizard mid-save"
    );
    cx.update_window(s.win, |_, win, cx| {
        win.open_dialog(cx, |dialog, _, _| {
            dialog.child(
                gpui_kit::div()
                    .id("other-dialog")
                    .test_support()
                    .child("Other"),
            )
        });
        win.render_frame(cx);
    })
    .unwrap();
    assert_eq!(
        toast_text(cx, &s),
        "Profile created. \"Comp\" is ready to activate"
    );
    assert!(!cx.update(|cx| w.read(cx).is_saving()));
    assert!(support::present(cx, s.win, "other-dialog"));
}

#[gpui_kit::test]
fn back_is_inert_while_a_save_is_in_flight(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let page = open_profiles(cx, &s);
    let w = open_wizard(cx, &s, &page, ids::PROFILES_CREATE);
    walk_to(cx, &s, &w, WizardStep::Crosshairs);
    // No settle between these steps, so the save's background job stays queued.
    cx.update_window(s.win, |_, win, cx| {
        win.click(ids::WIZARD_SAVE, cx);
        win.render_frame(cx);
        win.click(ids::WIZARD_BACK, cx);
        win.render_frame(cx);
    })
    .unwrap();
    assert!(cx.update(|cx| w.read(cx).is_saving()));
    assert_eq!(cx.update(|cx| w.read(cx).step()), WizardStep::Crosshairs);
    assert_eq!(
        toast_text(cx, &s),
        "Profile created. \"Comp\" is ready to activate"
    );
    assert!(!has_dialog(cx, &s), "the wizard stayed open after a save");
}

#[gpui_kit::test]
fn a_save_closes_the_wizard_after_typing_in_the_name(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let page = open_profiles(cx, &s);
    let w = open_wizard(cx, &s, &page, ids::PROFILES_CREATE);
    // Typing leaves the focus in the name field, which stops rendering after the first step.
    let input = cx.update(|cx| w.read(cx).name_input().clone());
    cx.update_window(s.win, |_, window, cx| {
        input.update(cx, |st, cx| st.focus(window, cx))
    })
    .unwrap();
    walk_to(cx, &s, &w, WizardStep::Crosshairs);
    support::click_id(cx, s.win, ids::WIZARD_SAVE);
    assert_eq!(
        toast_text(cx, &s),
        "Profile created. \"Comp\" is ready to activate"
    );
    assert!(!has_dialog(cx, &s), "the wizard stayed open after a save");
    assert_eq!(s.services.library.profiles().unwrap().len(), 1);
}

#[gpui_kit::test]
fn a_failed_edit_load_spares_a_dialog_on_top(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let comp = full_profile(&s, &env, "Competitive");
    open_profiles(cx, &s);
    s.services.library.delete_profile(comp.id()).unwrap();
    // No settle between these steps, so the wizard's load stays queued until both are open.
    cx.update_window(s.win, |_, win, cx| {
        win.click(ids::profile_edit(comp.id()), cx)
    })
    .unwrap();
    cx.update_window(s.win, |_, win, cx| win.render_frame(cx))
        .unwrap();
    cx.update_window(s.win, |_, win, cx| {
        win.open_dialog(cx, |dialog, _, _| {
            dialog.child(
                gpui_kit::div()
                    .id("other-dialog")
                    .test_support()
                    .child("Other"),
            )
        });
        win.render_frame(cx);
    })
    .unwrap();
    assert_eq!(
        toast_text(cx, &s),
        "Failed to load profile. This profile no longer exists. Pick another profile."
    );
    assert!(support::present(cx, s.win, "other-dialog"));
}

/// Opens the wizard on the Sounds step with "Ding" previewing.
fn wizard_previewing(
    cx: &mut TestAppContext,
    s: &support::ShellWindow,
    env: &support::Env,
) -> Entity<ProfileWizard> {
    let ding = seed_sound(s, env, "Ding");
    let page = open_profiles(cx, s);
    let w = open_wizard(cx, s, &page, ids::PROFILES_CREATE);
    walk_to(cx, s, &w, WizardStep::Sounds);
    pick_hitsound(cx, s, &w, DraftSlot::Item(ding.id().clone()));
    support::click_id(cx, s.win, ids::WIZARD_HITSOUND_PLAY);
    support::wait_real(cx, "the preview to play", |_| s.audio.plays().len() == 1);
    w
}

fn stopped_last(s: &support::ShellWindow) -> bool {
    s.audio.calls().last() == Some(&tf2tools_app::audio::AudioCall::Stop)
}

#[gpui_kit::test]
fn cancel_stops_a_playing_preview(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let _w = wizard_previewing(cx, &s, &env);
    support::click_id(cx, s.win, ids::WIZARD_CANCEL);
    assert!(!has_dialog(cx, &s));
    assert!(stopped_last(&s), "calls: {:?}", s.audio.calls());
}

#[gpui_kit::test]
fn escape_stops_a_playing_preview(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let _w = wizard_previewing(cx, &s, &env);
    support::press(cx, s.win, "escape");
    assert!(!has_dialog(cx, &s));
    assert!(stopped_last(&s), "calls: {:?}", s.audio.calls());
}

#[gpui_kit::test]
fn the_x_stops_a_playing_preview(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let _w = wizard_previewing(cx, &s, &env);
    support::click_id(cx, s.win, "close");
    assert!(!has_dialog(cx, &s));
    assert!(stopped_last(&s), "calls: {:?}", s.audio.calls());
}

#[gpui_kit::test]
fn a_save_in_flight_ignores_the_x(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let page = open_profiles(cx, &s);
    let w = open_wizard(cx, &s, &page, ids::PROFILES_CREATE);
    walk_to(cx, &s, &w, WizardStep::Crosshairs);
    // No settle between these steps, so the save's background job stays queued.
    cx.update_window(s.win, |_, win, cx| {
        win.click(ids::WIZARD_SAVE, cx);
        win.render_frame(cx);
        win.click("close", cx);
        win.render_frame(cx);
    })
    .unwrap();
    assert!(cx.update(|cx| w.read(cx).is_saving()));
    assert!(
        cx.update_window(s.win, |_, win, cx| win.has_active_dialog(cx))
            .unwrap(),
        "the X closed the wizard mid-save"
    );
    assert_eq!(
        toast_text(cx, &s),
        "Profile created. \"Comp\" is ready to activate"
    );
    assert!(!has_dialog(cx, &s), "the wizard stayed open after a save");
}

#[gpui_kit::test]
fn a_failed_save_keeps_the_dialog_open(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let hud = seed_hud(&s, &env, "rayshud");
    let page = open_profiles(cx, &s);
    let w = open_wizard(cx, &s, &page, ids::PROFILES_CREATE);
    walk_to(cx, &s, &w, WizardStep::Hud);
    pick_hud(cx, &s, &w, DraftSlot::Item(hud.id().clone()));
    // The picked HUD vanishes before Save, so create_profile rejects the draft.
    s.services.library.delete_hud(hud.id()).unwrap();
    support::click_id(cx, s.win, ids::wizard_step(4));
    support::click_id(cx, s.win, ids::WIZARD_SAVE);
    let toast = toast_text(cx, &s);
    assert_eq!(
        toast,
        "Failed to create profile. An item you picked is no longer in your library. Pick another one, then try again."
    );
    assert!(has_dialog(cx, &s));
    assert!(support::present(cx, s.win, ids::WIZARD_DIALOG));
    assert!(!cx.update(|cx| w.read(cx).is_saving()));
    assert!(s.services.library.profiles().unwrap().is_empty());
}

// Visual checks against v5's CreateProfileDialog: the sizes and order its screenshots show.

#[gpui_kit::test]
fn the_x_closes_the_wizard_like_cancel(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let page = open_profiles(cx, &s);
    let _w = open_wizard(cx, &s, &page, ids::PROFILES_CREATE);
    assert!(support::present(cx, s.win, "close"), "the kit's X");
    support::click_id(cx, s.win, "close");
    assert!(!has_dialog(cx, &s));
    assert!(s.services.library.profiles().unwrap().is_empty());
}

#[gpui_kit::test]
fn the_footer_puts_back_left_and_cancel_beside_next(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let page = open_profiles(cx, &s);
    let w = open_wizard(cx, &s, &page, ids::PROFILES_CREATE);
    walk_to(cx, &s, &w, WizardStep::Hud);
    let back = support::bounds_of(cx, s.win, ids::WIZARD_BACK).unwrap();
    let cancel = support::bounds_of(cx, s.win, ids::WIZARD_CANCEL).unwrap();
    let next = support::bounds_of(cx, s.win, ids::WIZARD_NEXT).unwrap();
    assert!(back.right() < cancel.left(), "{back:?} {cancel:?}");
    assert!(cancel.right() < next.left(), "{cancel:?} {next:?}");
    assert!(
        next.left() - cancel.right() <= gpui_kit::px(8.),
        "{cancel:?} {next:?}"
    );
}

#[gpui_kit::test]
fn the_wizard_keeps_v5_s_padding_and_body_height(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let page = open_profiles(cx, &s);
    let _w = open_wizard(cx, &s, &page, ids::PROFILES_CREATE);
    // sm:max-w-2xl less a 1 px border and p-6 on each side: v5's name input is 622 px wide.
    let body = support::bounds_of(cx, s.win, ids::WIZARD_DIALOG).unwrap();
    assert_eq!(body.size.width, gpui_kit::px(622.));
    // Below the stepper's py-2: gap-4, the min-h-[200px] body, gap-4, border-t, pt-4. v5's shot
    // puts Next 257 px under the first circle.
    let circle = support::bounds_of(cx, s.win, ids::wizard_step(1)).unwrap();
    let next = support::bounds_of(cx, s.win, ids::WIZARD_NEXT).unwrap();
    assert!(
        next.top() - circle.bottom() >= gpui_kit::px(256.),
        "{circle:?} {next:?}"
    );
}

#[gpui_kit::test]
fn the_stepper_is_v5_s_compact_centred_row(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let page = open_profiles(cx, &s);
    let _w = open_wizard(cx, &s, &page, ids::PROFILES_CREATE);
    let body = support::bounds_of(cx, s.win, ids::WIZARD_DIALOG).unwrap();
    let first = support::bounds_of(cx, s.win, ids::wizard_step(1)).unwrap();
    let last = support::bounds_of(cx, s.win, ids::wizard_step(4)).unwrap();
    // size-7 circles joined by w-6 lines, centred rather than stretched across the dialog.
    assert_eq!(
        first.size,
        gpui_kit::size(gpui_kit::px(28.), gpui_kit::px(28.))
    );
    let span = last.left() - first.left();
    assert!(
        span > gpui_kit::px(280.) && span < gpui_kit::px(360.),
        "{span:?}"
    );
    assert!(
        first.left() - body.left() > gpui_kit::px(60.),
        "{first:?} {body:?}"
    );
}

#[gpui_kit::test]
fn the_wizard_is_centred(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let page = open_profiles(cx, &s);
    let _w = open_wizard(cx, &s, &page, ids::PROFILES_CREATE);
    let b = support::dialog_box(cx, s.win);
    let view = support::viewport(cx, s.win);
    assert_eq!(b.size.width, gpui_kit::px(672.));
    let off = b.origin.y + b.size.height / 2. - view.height / 2.;
    assert!(off.abs() <= gpui_kit::px(1.), "box {b:?} in {view:?}");
}

#[gpui_kit::test]
fn the_picker_and_modes_are_inert_while_a_save_is_in_flight(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let page = open_profiles(cx, &s);
    let w = open_wizard(cx, &s, &page, ids::PROFILES_CREATE);
    walk_to(cx, &s, &w, WizardStep::Crosshairs);
    support::click_id(cx, s.win, ids::WIZARD_MODE_CUSTOM);
    let first = s.services.builtins.all()[0].name().to_owned();
    let picker = cx.update(|cx| w.read(cx).picker().clone());
    // No settle between these steps, so the save's background job stays queued.
    cx.update_window(s.win, |_, win, cx| {
        win.click(ids::WIZARD_SAVE, cx);
        win.render_frame(cx);
        win.click(ids::crosshair_builtin(&first), cx);
        win.render_frame(cx);
        win.click(ids::WIZARD_MODE_NONE, cx);
        win.render_frame(cx);
    })
    .unwrap();
    assert!(cx.update(|cx| w.read(cx).is_saving()));
    assert_eq!(cx.update(|cx| picker.read(cx).selected().cloned()), None);
    assert_eq!(cx.update(|cx| w.read(cx).mode()), CrosshairMode::Customize);
    assert_eq!(
        toast_text(cx, &s),
        "Profile created. \"Comp\" is ready to activate"
    );
}

#[gpui_kit::test]
fn the_weapon_list_is_inert_while_a_save_is_in_flight(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let page = open_profiles(cx, &s);
    let w = open_wizard(cx, &s, &page, ids::PROFILES_CREATE);
    walk_to(cx, &s, &w, WizardStep::Crosshairs);
    support::click_id(cx, s.win, ids::WIZARD_MODE_CUSTOM);
    select_first_builtin(cx, &s, &w);
    let assigner = cx.update(|cx| w.read(cx).assigner().clone());
    let requests = std::rc::Rc::new(std::cell::Cell::new(0));
    let seen = requests.clone();
    let _sub = cx.update(|cx| {
        cx.subscribe(
            &assigner,
            move |_, _: &tf2tools_app::widgets::weapon_assigner::AssignRequested, _| {
                seen.set(seen.get() + 1)
            },
        )
    });
    cx.update_window(s.win, |_, win, cx| {
        win.click(ids::WIZARD_SAVE, cx);
        win.render_frame(cx);
        win.click(ids::CROSSHAIRS_BULK_ALL, cx);
        win.render_frame(cx);
    })
    .unwrap();
    assert!(cx.update(|cx| w.read(cx).is_saving()));
    assert_eq!(requests.get(), 0);
    let _ = toast_text(cx, &s);
    assert!(
        s.services.library.profiles().unwrap()[0]
            .crosshairs()
            .is_empty()
    );
}

#[gpui_kit::test]
fn the_crosshair_inputs_come_back_after_a_failed_save(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let hud = seed_hud(&s, &env, "rayshud");
    let page = open_profiles(cx, &s);
    let w = open_wizard(cx, &s, &page, ids::PROFILES_CREATE);
    walk_to(cx, &s, &w, WizardStep::Hud);
    pick_hud(cx, &s, &w, DraftSlot::Item(hud.id().clone()));
    // The picked HUD vanishes before Save, so the save fails on the Crosshairs step.
    s.services.library.delete_hud(hud.id()).unwrap();
    support::click_id(cx, s.win, ids::wizard_step(4));
    support::click_id(cx, s.win, ids::WIZARD_MODE_CUSTOM);
    support::click_id(cx, s.win, ids::WIZARD_SAVE);
    assert!(toast_text(cx, &s).starts_with("Failed to create profile."));
    assert!(!cx.update(|cx| w.read(cx).is_saving()));
    let first = s.services.builtins.all()[0].name().to_owned();
    let picker = cx.update(|cx| w.read(cx).picker().clone());
    support::click_id(cx, s.win, ids::crosshair_builtin(&first));
    assert_eq!(
        cx.update(|cx| picker.read(cx).selected().cloned()),
        Some(CrosshairRef::Builtin(first))
    );
    click_weapon_row(cx, &s, "tf_weapon_scattergun");
    assert_eq!(
        support::label_of(cx, s.win, ids::WIZARD_ASSIGNED_COUNT).as_deref(),
        Some("1 weapon assigned")
    );
    support::click_id(cx, s.win, ids::WIZARD_MODE_NONE);
    assert_eq!(cx.update(|cx| w.read(cx).mode()), CrosshairMode::None);
}
