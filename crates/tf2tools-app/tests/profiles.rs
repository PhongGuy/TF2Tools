use crate::support;

use std::cell::Cell;
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use gpui_kit::component::WindowExt as _;
use gpui_kit::{AppContext as _, ElementId, Entity, TestAppContext, px};
use support::scroll::{assert_bar_beside, assert_clear_of_bar, assert_track_scrolls};
use tf2tools_app::ids;
use tf2tools_app::library_events::{LibraryChanged, LibraryEvents};
use tf2tools_app::pages::profiles::ProfilesPage;
use tf2tools_app::pages::profiles::probe::Tf2Probe;
use tf2tools_app::services::Tf2State;
use tf2tools_app::shell::route::Page;
use tf2tools_core::{
    CrosshairRef, DraftCrosshair, DraftItem, HudEntry, Profile, ProfileDraft, Provenance, SlotMode,
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
        .with_hud(SlotMode::Set(DraftItem::Pick(hud.id().clone())))
        .with_sounds(SlotMode::Set((
            Some(DraftItem::Pick(hit.id().clone())),
            Some(DraftItem::Pick(kill.id().clone())),
        )))
        .with_crosshairs(SlotMode::Set(vec![DraftCrosshair::Set {
            weapon_script: "tf_weapon_scattergun".into(),
            crosshair: CrosshairRef::Builtin(first),
        }]));
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

fn lines(
    cx: &mut TestAppContext,
    s: &support::ShellWindow,
    id: &tf2tools_core::ProfileId,
) -> Vec<String> {
    ["hud", "sounds", "crosshairs", "graphics", "addons"]
        .map(|slot| {
            support::label_of(cx, s.win, ids::profile_card_line(id, slot)).unwrap_or_default()
        })
        .to_vec()
}

#[gpui_kit::test]
fn the_list_leads_with_its_line_and_new_profile(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let _page = open_profiles(cx, &s);
    assert_eq!(
        support::label_of(cx, s.win, ids::PROFILES_LEAD).as_deref(),
        Some(
            "A profile switches your whole setup at once: HUD, sounds, crosshairs, graphics and add-ons. A new one starts as a copy of what you have now."
        )
    );
    assert_eq!(
        support::label_of(cx, s.win, ids::PROFILES_NEW).as_deref(),
        Some("New profile")
    );
    assert!(!support::disabled_of(cx, s.win, ids::PROFILES_NEW));
    assert_eq!(
        support::label_of(cx, s.win, ids::PROFILES_EMPTY).as_deref(),
        Some("No profiles yet")
    );
    assert!(!support::present(cx, s.win, ids::PROFILES_NO_TF2));
}

#[gpui_kit::test]
fn each_card_shows_its_five_slots(cx: &mut TestAppContext) {
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
        support::label_of(cx, s.win, ids::profile_card_name(id)).as_deref(),
        Some("Competitive")
    );
    assert_eq!(
        lines(cx, &s, id),
        [
            "rayshud",
            "Hit: Classic Ding, kill: Bell",
            "1 weapon crosshair",
            "Left as is",
            "Left as is"
        ]
    );
    assert_eq!(
        lines(cx, &s, vanilla.id()),
        [
            "TF2's own HUD",
            "TF2 default sounds",
            "TF2's default crosshairs",
            "Left as is",
            "Left as is"
        ]
    );
    assert_eq!(
        support::label_of(cx, s.win, ids::profile_card_apply(id)).as_deref(),
        Some("Apply")
    );
    assert_eq!(
        support::label_of(cx, s.win, ids::profile_card_edit(id)).as_deref(),
        Some("Edit")
    );
    assert!(!support::present(cx, s.win, ids::profile_card_active(id)));
}

#[gpui_kit::test]
fn the_active_card_wears_the_pill_and_offers_no_apply(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let comp = full_profile(&s, &env, "Comp");
    let pub_ = empty_profile(&s, "Pub");
    s.services
        .library
        .apply_profile(comp.id(), &tf2(&env), &s.services.builtins, false)
        .unwrap();
    let _page = open_profiles(cx, &s);
    assert_eq!(
        support::label_of(cx, s.win, ids::profile_card_active(comp.id())).as_deref(),
        Some("Active")
    );
    assert!(!support::present(
        cx,
        s.win,
        ids::profile_card_apply(comp.id())
    ));
    assert!(support::present(
        cx,
        s.win,
        ids::profile_card_edit(comp.id())
    ));
    assert!(support::present(
        cx,
        s.win,
        ids::profile_card_apply(pub_.id())
    ));
    assert!(!support::present(
        cx,
        s.win,
        ids::profile_card_active(pub_.id())
    ));
}

#[gpui_kit::test]
fn the_picture_says_tf2s_own_hud_only_under_none(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let vanilla = empty_profile(&s, "Vanilla");
    let untouched = s
        .services
        .library
        .create_profile(&ProfileDraft::new("Untouched").with_hud(tf2tools_core::SlotMode::Leave))
        .unwrap();
    let _page = open_profiles(cx, &s);
    assert_eq!(
        support::label_of(cx, s.win, ids::profile_card_picture(vanilla.id())).as_deref(),
        Some("TF2's own HUD")
    );
    assert!(support::present(
        cx,
        s.win,
        ids::profile_card_picture(untouched.id())
    ));
    assert_eq!(
        support::label_of(cx, s.win, ids::profile_card_picture(untouched.id())),
        None
    );
}

#[gpui_kit::test]
fn without_tf2_new_profile_and_apply_are_disabled(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, Tf2State::Missing { saved: None });
    let comp = empty_profile(&s, "Comp");
    let _page = open_profiles(cx, &s);
    assert!(support::present(cx, s.win, ids::PROFILES_NO_TF2));
    assert!(support::disabled_of(cx, s.win, ids::PROFILES_NEW));
    assert!(support::disabled_of(
        cx,
        s.win,
        ids::profile_card_apply(comp.id())
    ));
    assert!(!support::disabled_of(
        cx,
        s.win,
        ids::profile_card_edit(comp.id())
    ));
}

#[gpui_kit::test]
fn at_800x600_the_list_fits_and_cards_keep_250_px(cx: &mut TestAppContext) {
    use gpui_kit::size;
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let comp = full_profile(&s, &env, "Comp");
    let pub_ = empty_profile(&s, "Pub");
    cx.simulate_window_resize(s.win, size(px(800.), px(600.)));
    let _page = open_profiles(cx, &s);
    let page = support::bounds_of(cx, s.win, ids::page(Page::Profiles)).unwrap();
    let margin = tf2tools_app::shell::CONTENT_PADDING;
    let (lead, new) = (
        support::bounds_of(cx, s.win, ids::PROFILES_LEAD).unwrap(),
        support::bounds_of(cx, s.win, ids::PROFILES_NEW).unwrap(),
    );
    for b in [lead, new] {
        assert!(
            b.left() >= page.left() + margin - px(1.)
                && b.right() <= page.right() - margin + px(1.),
            "{b:?} past the margin of {page:?}"
        );
    }
    assert!(!lead.intersects(&new), "{lead:?} and {new:?} overlap");
    let card = support::bounds_of(cx, s.win, ids::profile_card(comp.id())).unwrap();
    assert!(card.size.width >= px(250.), "the card is {card:?}");
    assert!(
        card.right() <= page.right() - margin + px(1.),
        "{card:?} leaves {page:?}"
    );
    for slot in ["hud", "sounds", "crosshairs", "graphics", "addons"] {
        let line = support::bounds_of(cx, s.win, ids::profile_card_line(comp.id(), slot)).unwrap();
        assert!(
            line.right() <= card.right() && line.bottom() <= card.bottom(),
            "{slot} {line:?} leaves {card:?}"
        );
    }
    for button in [
        ids::profile_card_apply(comp.id()),
        ids::profile_card_edit(comp.id()),
    ] {
        let b = support::bounds_of(cx, s.win, button.clone()).unwrap();
        assert!(
            b.right() <= card.right() && b.bottom() <= card.bottom(),
            "{button:?} {b:?} leaves {card:?}"
        );
    }
    assert!(support::present(cx, s.win, ids::profile_card(pub_.id())));
}

#[gpui_kit::test]
fn at_1280_three_cards_share_a_row(cx: &mut TestAppContext) {
    use gpui_kit::size;
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let ps = [
        empty_profile(&s, "A"),
        empty_profile(&s, "B"),
        empty_profile(&s, "C"),
    ];
    cx.simulate_window_resize(s.win, size(px(1280.), px(860.)));
    let _page = open_profiles(cx, &s);
    let cards: Vec<_> = ps
        .iter()
        .map(|p| support::bounds_of(cx, s.win, ids::profile_card(p.id())).unwrap())
        .collect();
    assert!(cards.iter().all(|c| c.top() == cards[0].top()), "{cards:?}");
    assert!(cards.iter().all(|c| c.size.width >= px(250.)), "{cards:?}");
}

fn row(e: &LogEntry) -> (&str, &str, &str) {
    (e.category(), e.action(), e.message())
}

fn toast_is(cx: &mut TestAppContext, s: &support::ShellWindow, text: &str) {
    support::wait_real(cx, text, |cx| {
        support::label_of(cx, s.win, ids::TOAST).as_deref() == Some(text)
    });
}

#[gpui_kit::test]
fn delete_asks_then_removes_and_writes_a_row(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let comp = empty_profile(&s, "Competitive");
    let _page = open_profiles(cx, &s);
    support::click_id(cx, s.win, ids::profile_card_edit(comp.id()));
    support::click_id(cx, s.win, ids::PROFILE_EDITOR_DELETE);
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
    support::click_id(cx, s.win, ids::profile_card_edit(comp.id()));
    support::click_id(cx, s.win, ids::PROFILE_EDITOR_DELETE);
    support::click_id(cx, s.win, ids::CONFIRM_CANCEL);
    assert_eq!(s.services.library.profiles().unwrap().len(), 1);
    assert!(support::log_rows(&s.services).is_empty());
}

#[gpui_kit::test]
fn deleting_the_active_profile_clears_the_active_mark(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, Tf2State::Missing { saved: None });
    let vanilla = empty_profile(&s, "Vanilla");
    s.services
        .library
        .apply_profile(vanilla.id(), &tf2(&env), &s.services.builtins, false)
        .unwrap();
    let _page = open_profiles(cx, &s);
    support::click_id(cx, s.win, ids::profile_card_edit(vanilla.id()));
    support::click_id(cx, s.win, ids::PROFILE_EDITOR_DELETE);
    support::click_id(cx, s.win, ids::CONFIRM_OK);
    toast_is(cx, &s, "Profile deleted");
    assert_eq!(s.services.library.active_profile().unwrap(), None);
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
    support::click_apply(cx, s.win, comp.id());
    toast_is(
        cx,
        &s,
        "Comp applied. TF2 picks it up the next time it starts.",
    );
    assert!(env.game.join("tf/custom/rayshud").is_dir());
    assert_eq!(
        s.services.library.active_profile().unwrap(),
        Some(comp.id().clone())
    );
    support::wait_real(cx, "LibraryChanged", |_| changes.get() >= 1);
    assert_eq!(changes.get(), 1);
    support::wait_real(cx, "the active pill", |cx| {
        support::label_of(cx, s.win, ids::profile_card_active(comp.id())).as_deref()
            == Some("Active")
    });
    assert!(!support::present(
        cx,
        s.win,
        ids::profile_card_apply(comp.id())
    ));
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

fn row_label(cx: &mut TestAppContext, s: &support::ShellWindow, key: &str) -> Option<String> {
    support::label_of(cx, s.win, ids::profile_apply_row(key))
}

#[gpui_kit::test]
fn apply_first_shows_what_changes_and_cancel_applies_nothing(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let comp = full_profile(&s, &env, "Comp");
    let page = open_profiles(cx, &s);
    support::click_id(cx, s.win, ids::profile_card_apply(comp.id()));
    support::wait_real(cx, "the summary", |cx| {
        support::present(cx, s.win, ids::PROFILE_APPLY_DIALOG)
    });
    assert_eq!(
        support::label_of(cx, s.win, ids::PROFILE_APPLY_TITLE).as_deref(),
        Some("Switch to Comp?")
    );
    assert_eq!(
        support::label_of(cx, s.win, ids::PROFILE_APPLY_INTRO).as_deref(),
        Some("These change now. Everything else stays how it is.")
    );
    assert_eq!(
        row_label(cx, &s, "hud").as_deref(),
        Some("HUD: TF2's own HUD → rayshud")
    );
    assert_eq!(
        row_label(cx, &s, "hitsound").as_deref(),
        Some("Hitsound: TF2 default → Classic Ding")
    );
    assert_eq!(
        row_label(cx, &s, "killsound").as_deref(),
        Some("Killsound: TF2 default → Bell")
    );
    assert_eq!(
        row_label(cx, &s, "crosshairs").as_deref(),
        Some("Crosshairs: TF2's default → 1 weapon")
    );
    assert_eq!(
        support::label_of(cx, s.win, ids::PROFILE_APPLY_LEFT).as_deref(),
        Some("Stays as it is: Graphics, Add-ons.")
    );
    assert_eq!(
        support::label_of(cx, s.win, ids::PROFILE_APPLY_OK).as_deref(),
        Some("Apply Comp")
    );
    support::click_id(cx, s.win, ids::PROFILE_APPLY_CANCEL);
    support::settle(cx, s.win);
    assert!(!support::present(cx, s.win, ids::PROFILE_APPLY_DIALOG));
    assert!(!env.game.join("tf/custom/rayshud").exists());
    assert_eq!(s.services.library.active_profile().unwrap(), None);
    assert!(support::log_rows(&s.services).is_empty());
    assert!(cx.update(|cx| page.read(cx).activating().is_none()));
}

#[gpui_kit::test]
fn cancel_in_the_summary_applies_nothing(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let vanilla = empty_profile(&s, "Vanilla");
    let page = open_profiles(cx, &s);
    support::click_id(cx, s.win, ids::profile_card_apply(vanilla.id()));
    support::wait_real(cx, "the summary", |cx| {
        support::present(cx, s.win, ids::PROFILE_APPLY_DIALOG)
    });
    support::press(cx, s.win, "escape");
    assert!(!support::present(cx, s.win, ids::PROFILE_APPLY_DIALOG));
    assert_eq!(s.services.library.active_profile().unwrap(), None);
    assert!(cx.update(|cx| page.read(cx).activating().is_none()));
    assert!(!support::disabled_of(
        cx,
        s.win,
        ids::profile_card_apply(vanilla.id())
    ));
}

#[gpui_kit::test]
fn a_profile_that_matches_says_nothing_changes_and_still_becomes_active(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let vanilla = empty_profile(&s, "Vanilla");
    let _page = open_profiles(cx, &s);
    support::click_id(cx, s.win, ids::profile_card_apply(vanilla.id()));
    support::wait_real(cx, "the summary", |cx| {
        support::present(cx, s.win, ids::PROFILE_APPLY_DIALOG)
    });
    assert_eq!(
        support::label_of(cx, s.win, ids::PROFILE_APPLY_NOTHING).as_deref(),
        Some("This already matches what you have now. Applying marks it as your active profile.")
    );
    assert!(!support::present(cx, s.win, ids::PROFILE_APPLY_ROWS));
    support::click_id(cx, s.win, ids::PROFILE_APPLY_OK);
    support::wait_real(cx, "the mark", |_| {
        s.services.library.active_profile().unwrap() == Some(vanilla.id().clone())
    });
}

#[gpui_kit::test]
fn a_running_tf2_still_asks_after_the_summary(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let comp = full_profile(&s, &env, "Comp");
    let _page = open_profiles(cx, &s);
    cx.update(|cx| Tf2Probe::install(cx, || true));
    support::click_apply(cx, s.win, comp.id());
    support::wait_real(cx, "the running confirm", |cx| {
        support::label_of(cx, s.win, ids::CONFIRM_TITLE).as_deref() == Some("TF2 is running")
    });
    support::click_id(cx, s.win, ids::CONFIRM_CANCEL);
    support::settle(cx, s.win);
    assert!(!env.game.join("tf/custom/rayshud").exists());
}

#[gpui_kit::test]
fn a_running_tf2_warns_before_applying(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let comp = full_profile(&s, &env, "Comp");
    let page = open_profiles(cx, &s);
    cx.update(|cx| Tf2Probe::install(cx, || true));
    support::click_apply(cx, s.win, comp.id());
    support::wait_real(cx, "the TF2 warning", |cx| {
        support::label_of(cx, s.win, ids::CONFIRM_DESCRIPTION).as_deref()
            == Some("TF2 is running; changes take effect after a restart.")
    });
    support::click_id(cx, s.win, ids::CONFIRM_CANCEL);
    support::settle(cx, s.win);
    assert!(!env.game.join("tf/custom/rayshud").exists());
    assert!(cx.update(|cx| page.read(cx).activating().is_none()));
    support::click_apply(cx, s.win, comp.id());
    support::wait_real(cx, "the TF2 warning", |cx| {
        support::present(cx, s.win, ids::CONFIRM_OK)
    });
    support::click_id(cx, s.win, ids::CONFIRM_OK);
    toast_is(
        cx,
        &s,
        "Comp applied. TF2 picks it up the next time it starts.",
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
    support::click_apply(cx, s.win, comp.id());
    toast_is(
        cx,
        &s,
        "Comp applied. TF2 picks it up the next time it starts.",
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
        .create_profile(
            &ProfileDraft::new("Other").with_hud(SlotMode::Set(DraftItem::Pick(
                s.services.library.list_huds().unwrap()[0].id().clone(),
            ))),
        )
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
    support::confirm_apply_summary(cx, s.win);
    toast_is(
        cx,
        &s,
        "Comp applied. TF2 picks it up the next time it starts.",
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
    support::click_apply(cx, s.win, comp.id());
    support::wait_real(cx, "the failure toast", |cx| {
        support::label_of(cx, s.win, ids::TOAST)
            .is_some_and(|t| t.starts_with("Couldn't apply Comp. Couldn't put these in place: HUD"))
    });
    std::fs::set_permissions(&custom, std::fs::Permissions::from_mode(0o755)).unwrap();
    assert_eq!(s.services.library.active_profile().unwrap(), None);
    support::wait_real(cx, "LibraryChanged", |_| changes.get() >= 1);
    assert_eq!(changes.get(), 1);
    support::wait_real(cx, "the reload", |cx| {
        !support::present(cx, s.win, ids::profile_card_active(comp.id()))
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
    support::click_id(cx, s.win, ids::profile_card_apply(vanilla.id()));
    assert!(!has_dialog(cx, &s));
    assert!(!support::present(cx, s.win, ids::TOAST));
    assert!(cx.update(|cx| page.read(cx).activating().is_none()));
}

/// "Comp" with a local HUD "rayshud" (deleted from the library) and a present hitsound.
fn profile_missing_its_hud(s: &support::ShellWindow, env: &support::Env) -> Profile {
    let hud = seed_hud(s, env, "rayshud", Provenance::Local);
    let hit = seed_sound(s, env, "Classic Ding");
    let draft = ProfileDraft::new("Comp")
        .with_hud(SlotMode::Set(DraftItem::Pick(hud.id().clone())))
        .with_sounds(SlotMode::Set((
            Some(DraftItem::Pick(hit.id().clone())),
            None,
        )));
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
    support::click_apply(cx, s.win, comp.id());
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
    support::click_apply(cx, s.win, comp.id());
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
    support::click_apply(cx, s.win, comp.id());
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
    support::click_apply(cx, s.win, comp.id());
    support::wait_real(cx, "the Missing Items dialog", |cx| {
        support::present(cx, s.win, ids::PROFILES_MISSING_INSTALL)
    });
    support::click_id(cx, s.win, ids::PROFILES_MISSING_INSTALL);
    toast_is(
        cx,
        &s,
        "Comp applied. TF2 picks it up the next time it starts. Kept what you had for: HUD (missing from your library).",
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
        .create_profile(
            &ProfileDraft::new("Comp").with_hud(SlotMode::Set(DraftItem::Pick(hud.id().clone()))),
        )
        .unwrap();
    s.services.library.delete_hud(hud.id()).unwrap();
    let _page = open_profiles(cx, &s);
    support::click_apply(cx, s.win, comp.id());
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
    let targets: [gpui_kit::ElementId; 2] = [
        ids::profile_card_apply(p.id()),
        ids::profile_card_edit(p.id()),
    ];
    for id in targets {
        let b = support::bounds_of(cx, s.win, id.clone()).expect("drawn");
        assert_eq!(b.size.height, gpui_kit::px(32.), "{id:?}");
    }
    // The mock's New profile button stands taller, beside the lead line.
    let new = support::bounds_of(cx, s.win, ids::PROFILES_NEW).expect("drawn");
    assert_eq!(new.size.height, gpui_kit::px(36.));
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
            p.new_profile(window, cx);
            p.open_editor(b.clone(), window, cx);
        })
    })
    .unwrap();
    // Only the first activate's summary opens: no delete confirm, no new profile.
    support::wait_real(cx, "the summary", |cx| {
        support::present(cx, s.win, ids::PROFILE_APPLY_DIALOG)
    });
    assert!(
        !support::present(cx, s.win, ids::CONFIRM_TITLE),
        "no confirm or new profile while activating"
    );
    assert!(cx.update(|cx| page.read(cx).editor().is_none()));
    support::confirm_apply_summary(cx, s.win);
    toast_is(
        cx,
        &s,
        "Comp applied. TF2 picks it up the next time it starts.",
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
        ids::PROFILES_NEW.into(),
        ids::profile_card_apply(comp.id()),
        ids::profile_card_edit(comp.id()),
        ids::profile_card_apply(other.id()),
        ids::profile_card_edit(other.id()),
    ];
    assert_eq!(
        support::disabled_ids(cx, s.win, &buttons),
        Vec::<ElementId>::new()
    );
    support::click_apply(cx, s.win, comp.id());
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

#[gpui_kit::test]
fn escape_on_missing_items_answers_cancel(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let comp = profile_missing_its_hud(&s, &env);
    let page = open_profiles(cx, &s);
    support::click_apply(cx, s.win, comp.id());
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
        .create_profile(
            &ProfileDraft::new("Comp").with_hud(SlotMode::Set(DraftItem::Pick(hud.id().clone()))),
        )
        .unwrap();
    s.services.library.delete_hud(hud.id()).unwrap();
    let page = open_profiles(cx, &s);
    support::click_apply(cx, s.win, comp.id());
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
fn missing_items_is_padded_like_v5(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let comp = profile_missing_its_hud(&s, &env);
    let _page = open_profiles(cx, &s);
    support::click_apply(cx, s.win, comp.id());
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
    s.services.library.unpack_hud_for_tests(hud.id()).unwrap();
    std::fs::write(
        env.data
            .library_dir(LibraryKind::Huds)
            .join("m0rehud-abc1234/resource/ui/extra.res"),
        "#base \"../../custom/m0rehud/resource/ui/base.res\"\n",
    )
    .unwrap();
    let draft =
        ProfileDraft::new("Comp").with_hud(SlotMode::Set(DraftItem::Pick(hud.id().clone())));
    let comp = s.services.library.create_profile(&draft).unwrap();
    let mine = env.game.join("tf/custom/m0rehud");
    std::fs::create_dir_all(&mine).unwrap();
    std::fs::write(mine.join("notes.txt"), "mine").unwrap();
    let _page = open_profiles(cx, &s);
    support::click_apply(cx, s.win, comp.id());
    toast_is(
        cx,
        &s,
        "Comp applied. TF2 picks it up the next time it starts. A folder named m0rehud is \
         already in your custom folder, so parts of this HUD may not show. Move or rename that \
         folder and install again.",
    );
    assert!(
        env.game
            .join("tf/custom/m0rehud-abc1234/info.vdf")
            .is_file()
    );
    assert_eq!(std::fs::read(mine.join("notes.txt")).unwrap(), b"mine");
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
        .create_profile(&ProfileDraft::new("Comp").with_crosshairs(SlotMode::Set(crosshairs)))
        .unwrap();
    let _page = open_profiles(cx, &s);
    support::click_apply(cx, s.win, comp.id());
    support::wait_real(cx, "the Missing Items dialog", |cx| {
        support::present(cx, s.win, ids::PROFILES_MISSING_DIALOG)
    });
    support::dialog_box(cx, s.win);
    let bar = ids::PROFILES_MISSING_LIST_SCROLLBAR;
    assert_bar_beside(cx, s.win, ids::PROFILES_MISSING_LIST, bar);
    assert_clear_of_bar(cx, s.win, ids::missing_item(0), bar);
    assert_track_scrolls(cx, s.win, bar, ids::missing_item(0));
}
