mod support;

use std::sync::Arc;

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{AppContext as _, Entity, TestAppContext, px};
use tf2tools_app::audio::AudioCall;
use tf2tools_app::audio::player::NowPlaying;
use tf2tools_app::ids;
use tf2tools_app::pages::sounds::SoundsPage;
use tf2tools_app::picker::{PickKind, ScriptedPicker};
use tf2tools_app::services::Tf2State;
use tf2tools_app::shell::route::Page;
use tf2tools_app::shell::toast::{ToastKind, toast};
use tf2tools_core::{LogEntry, Provenance, SoundEntry, SoundKind, Tf2Install};

fn found(env: &support::Env) -> Tf2State {
    Tf2State::Found(Tf2Install::from_path(&env.game).unwrap())
}

/// Opens the Sounds page (the shell starts on HUDs) and waits for its load and the saved volume.
fn open_sounds(cx: &mut TestAppContext, s: &support::ShellWindow) -> Entity<SoundsPage> {
    support::click_id(cx, s.win, ids::nav(Page::Sounds));
    let page = cx
        .update(|cx| s.shell.read(cx).sounds_page())
        .expect("a Sounds page");
    support::wait_real(cx, "Sounds to load", |cx| {
        cx.update(|cx| {
            let page = page.read(cx);
            page.is_loaded() && page.player().read(cx).is_ready()
        })
    });
    support::settle(cx, s.win);
    page
}

/// Plays `bytes` through the page's player, as a play button would.
fn play(cx: &mut TestAppContext, page: &Entity<SoundsPage>, name: &str, bytes: Vec<u8>) {
    let player = cx.update(|cx| page.read(cx).player().clone());
    let now = NowPlaying {
        key: name.to_owned().into(),
        name: name.to_owned().into(),
        subtitle: "test".into(),
    };
    cx.update(|cx| player.update(cx, |p, cx| p.toggle(now, move |_| Ok(bytes), cx).detach()));
}

fn push_toast(cx: &mut TestAppContext, s: &support::ShellWindow) {
    cx.update_window(s.win, |_, window, cx| {
        toast(window, cx, ToastKind::Success, "Hello", "")
    })
    .unwrap();
    support::settle(cx, s.win);
}

#[gpui_kit::test]
fn sounds_is_a_real_page(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let _page = open_sounds(cx, &s);
    assert!(support::present(cx, s.win, ids::page(Page::Sounds)));
    assert!(!support::present(cx, s.win, ids::PLACEHOLDER_MESSAGE));
    assert_eq!(
        support::label_of(cx, s.win, ids::HEADER_TITLE).as_deref(),
        Some("Sounds")
    );
    support::assert_starts_under_top_bar(
        cx,
        s.win,
        ids::page(Page::Sounds),
        ids::sound_slot(SoundKind::Hitsound),
    );
}

#[gpui_kit::test]
fn the_page_opens_without_tf2(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, Tf2State::Missing { saved: None });
    let _page = open_sounds(cx, &s);
    assert!(support::present(cx, s.win, ids::SOUND_PLAYER));
}

#[gpui_kit::test]
fn the_player_floats_bottom_right(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let _page = open_sounds(cx, &s);
    let player = support::bounds_of(cx, s.win, ids::SOUND_PLAYER).unwrap();
    assert_eq!(player.size.width, px(288.));
    assert_eq!(player.right(), px(1200. - 16.));
    assert_eq!(player.bottom(), px(900. - 16.));
}

#[gpui_kit::test]
fn toasts_sit_above_the_idle_player(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let _page = open_sounds(cx, &s);
    push_toast(cx, &s);
    let toast = support::bounds_of(cx, s.win, ids::TOAST).unwrap();
    let player = support::bounds_of(cx, s.win, ids::SOUND_PLAYER).unwrap();
    assert!(
        toast.bottom() <= player.top(),
        "{toast:?} overlaps {player:?}"
    );
}

#[gpui_kit::test]
fn toasts_sit_above_the_playing_player(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let page = open_sounds(cx, &s);
    play(cx, &page, "Ding", vec![1]);
    support::wait_real(cx, "the open player", |cx| {
        support::present(cx, s.win, ids::SOUND_PLAYER_NAME)
    });
    push_toast(cx, &s);
    let toast = support::bounds_of(cx, s.win, ids::TOAST).unwrap();
    let player = support::bounds_of(cx, s.win, ids::SOUND_PLAYER).unwrap();
    assert!(
        toast.bottom() <= player.top(),
        "{toast:?} overlaps {player:?}"
    );
}

#[gpui_kit::test]
fn toasts_drop_back_down_off_the_sounds_page(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let page = open_sounds(cx, &s);
    let player_top = support::bounds_of(cx, s.win, ids::SOUND_PLAYER)
        .unwrap()
        .top();
    drop(page);
    support::click_id(cx, s.win, ids::nav(Page::Huds));
    push_toast(cx, &s);
    let toast = support::bounds_of(cx, s.win, ids::TOAST).unwrap();
    assert!(toast.bottom() > player_top, "{toast:?} still lifted");
}

// The app's one player outlives the page, so a load still in flight when the player leaves
// plays once it lands, and the player floats over the next page to show it.
#[gpui_kit::test]
fn a_load_that_lands_after_leaving_still_plays(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let page = open_sounds(cx, &s);
    let player = cx.update(|cx| page.read(cx).player().clone());
    drop(page);
    let now = NowPlaying {
        key: "Ding".into(),
        name: "Ding".into(),
        subtitle: "test".into(),
    };
    // Start the play and leave in one update, so the page is gone before the load lands.
    cx.update_window(s.win, |_, _, cx| {
        player.update(cx, |p, cx| p.toggle(now, |_| Ok(vec![1]), cx).detach());
        s.shell
            .update(cx, |shell, cx| shell.navigate(Page::Huds, cx));
    })
    .unwrap();
    drop(player);
    support::wait_real(cx, "the late play", |_| s.audio.plays().len() == 1);
    support::settle(cx, s.win);
    assert_eq!(
        support::label_of(cx, s.win, ids::SOUND_PLAYER_NAME).as_deref(),
        Some("Ding")
    );
}

/// A TF2-playable WAV: 16-bit stereo PCM at `rate` Hz, `frames` frames of silence.
fn wav_at(rate: u32, frames: u32) -> Vec<u8> {
    let data_len = frames * 4;
    let mut out = Vec::new();
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data_len).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&2u16.to_le_bytes());
    out.extend_from_slice(&rate.to_le_bytes());
    out.extend_from_slice(&(rate * 4).to_le_bytes());
    out.extend_from_slice(&4u16.to_le_bytes());
    out.extend_from_slice(&16u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_len.to_le_bytes());
    out.resize(out.len() + data_len as usize, 0);
    out
}

fn wav(frames: u32) -> Vec<u8> {
    wav_at(44_100, frames)
}

/// Imports `bytes` as `<name>.<ext>` into the library.
fn seed(
    s: &support::ShellWindow,
    env: &support::Env,
    name: &str,
    ext: &str,
    bytes: &[u8],
) -> SoundEntry {
    let path = env.tmp.path().join(format!("{name}.{ext}"));
    std::fs::write(&path, bytes).unwrap();
    s.services
        .library
        .import_sound(&path, name, Provenance::Local)
        .unwrap()
}

fn tf2(env: &support::Env) -> Tf2Install {
    Tf2Install::from_path(&env.game).unwrap()
}

/// Writes `tf/custom/<folder>/sound/ui/<file>` by hand, outside the library.
fn place(env: &support::Env, folder: &str, file: &str) {
    let dir = env.game.join("tf/custom").join(folder).join("sound/ui");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join(file), wav(441)).unwrap();
}

#[gpui_kit::test]
fn empty_slots_say_none(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let _page = open_sounds(cx, &s);
    for (kind, text) in [
        (SoundKind::Hitsound, "No hitsound installed"),
        (SoundKind::Killsound, "No killsound installed"),
    ] {
        assert_eq!(
            support::label_of(cx, s.win, ids::sound_slot_status(kind)).as_deref(),
            Some("None")
        );
        assert_eq!(
            support::label_of(cx, s.win, ids::sound_slot_name(kind)).as_deref(),
            Some(text)
        );
        assert!(!support::present(cx, s.win, ids::sound_slot_play(kind)));
        assert!(!support::present(
            cx,
            s.win,
            ids::sound_slot_uninstall(kind)
        ));
    }
    assert!(!support::present(cx, s.win, ids::SOUNDS_CONFLICTS));
}

#[gpui_kit::test]
fn an_installed_library_wav_shows_its_name(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let ding = seed(&s, &env, "Ding", "wav", &wav(441));
    s.services
        .library
        .install_sound(ding.id(), SoundKind::Hitsound, &tf2(&env))
        .unwrap();
    let _page = open_sounds(cx, &s);
    let hit = SoundKind::Hitsound;
    assert_eq!(
        support::label_of(cx, s.win, ids::sound_slot_status(hit)).as_deref(),
        Some("Installed")
    );
    assert_eq!(
        support::label_of(cx, s.win, ids::sound_slot_name(hit)).as_deref(),
        Some("Ding")
    );
    assert!(!support::present(cx, s.win, ids::sound_slot_vpk(hit)));
    assert!(support::present(cx, s.win, ids::sound_slot_uninstall(hit)));
    assert_eq!(
        support::label_of(cx, s.win, ids::sound_slot_status(SoundKind::Killsound)).as_deref(),
        Some("None")
    );
}

#[gpui_kit::test]
fn an_installed_vpk_shows_the_vpk_badge(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let pak = support::vpk_bytes(&[("sound/ui/killsound.wav", &wav(441))]);
    let boom = seed(&s, &env, "Boom", "vpk", &pak);
    s.services
        .library
        .install_sound(boom.id(), SoundKind::Killsound, &tf2(&env))
        .unwrap();
    let _page = open_sounds(cx, &s);
    let kill = SoundKind::Killsound;
    assert_eq!(
        support::label_of(cx, s.win, ids::sound_slot_vpk(kill)).as_deref(),
        Some("VPK")
    );
    assert_eq!(
        support::label_of(cx, s.win, ids::sound_slot_name(kill)).as_deref(),
        Some("Boom")
    );
}

#[gpui_kit::test]
fn a_hand_placed_hitsound_says_it_came_from_outside_tf2tools(cx: &mut TestAppContext) {
    let env = support::env();
    place(&env, "tf2tools-sounds", "hitsound.wav");
    let s = support::shell_window(cx, &env, found(&env));
    let _page = open_sounds(cx, &s);
    let hit = SoundKind::Hitsound;
    assert_eq!(
        support::label_of(cx, s.win, ids::sound_slot_status(hit)).as_deref(),
        Some("Installed")
    );
    assert_eq!(
        support::label_of(cx, s.win, ids::sound_slot_name(hit)).as_deref(),
        Some("A sound added outside TF2Tools")
    );
}

#[gpui_kit::test]
fn a_hand_placed_vpk_killsound_says_it_came_from_outside_tf2tools(cx: &mut TestAppContext) {
    let env = support::env();
    let pak = support::vpk_bytes(&[("sound/ui/killsound.wav", &wav(441))]);
    std::fs::write(env.game.join("tf/custom/tf2tools-killsound.vpk"), pak).unwrap();
    let s = support::shell_window(cx, &env, found(&env));
    let _page = open_sounds(cx, &s);
    let kill = SoundKind::Killsound;
    assert_eq!(
        support::label_of(cx, s.win, ids::sound_slot_vpk(kill)).as_deref(),
        Some("VPK")
    );
    assert_eq!(
        support::label_of(cx, s.win, ids::sound_slot_name(kill)).as_deref(),
        Some("A sound added outside TF2Tools")
    );
}

#[gpui_kit::test]
fn deleting_the_installed_wav_empties_its_slot(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let ding = seed(&s, &env, "Ding", "wav", &wav(441));
    s.services
        .library
        .install_sound(ding.id(), SoundKind::Hitsound, &tf2(&env))
        .unwrap();
    let _page = open_sounds(cx, &s);
    support::click_id(cx, s.win, ids::sound_delete(ding.folder_name()));
    support::click_id(cx, s.win, ids::CONFIRM_OK);
    support::wait_real(cx, "the delete toast", |cx| {
        support::present(cx, s.win, ids::TOAST)
    });
    let hit = SoundKind::Hitsound;
    assert_eq!(
        support::label_of(cx, s.win, ids::sound_slot_status(hit)).as_deref(),
        Some("None")
    );
    assert!(
        !env.game
            .join("tf/custom/tf2tools-sounds/sound/ui/hitsound.wav")
            .exists()
    );
}

#[gpui_kit::test]
fn another_folders_hitsound_is_a_conflict(cx: &mut TestAppContext) {
    let env = support::env();
    place(&env, "myhits", "hitsound.wav");
    let s = support::shell_window(cx, &env, found(&env));
    let _page = open_sounds(cx, &s);
    assert_eq!(
        support::label_of(cx, s.win, ids::SOUNDS_CONFLICTS).as_deref(),
        Some("Conflicts Detected. myhits: Hitsound found in 'myhits' — may override your hitsound")
    );
}

#[gpui_kit::test]
fn the_slots_follow_tf2_state(cx: &mut TestAppContext) {
    let env = support::env();
    place(&env, "tf2tools-sounds", "hitsound.wav");
    let s = support::shell_window(cx, &env, Tf2State::Detecting);
    let _page = open_sounds(cx, &s);
    let hit = SoundKind::Hitsound;
    assert_eq!(
        support::label_of(cx, s.win, ids::sound_slot_status(hit)).as_deref(),
        Some("None")
    );
    let state = found(&env);
    cx.update(|cx| {
        s.tf2.update(cx, |tf2, cx| {
            *tf2 = state;
            cx.notify();
        })
    });
    support::wait_real(cx, "the installed slot", |cx| {
        support::label_of(cx, s.win, ids::sound_slot_status(hit)).as_deref() == Some("Installed")
    });
}

#[gpui_kit::test]
fn uninstall_empties_the_slot(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let ding = seed(&s, &env, "Ding", "wav", &wav(441));
    s.services
        .library
        .install_sound(ding.id(), SoundKind::Hitsound, &tf2(&env))
        .unwrap();
    let _page = open_sounds(cx, &s);
    support::click_id(cx, s.win, ids::sound_slot_uninstall(SoundKind::Hitsound));
    support::wait_real(cx, "the uninstall toast", |cx| {
        support::present(cx, s.win, ids::TOAST)
    });
    assert_eq!(
        support::label_of(cx, s.win, ids::TOAST).as_deref(),
        Some("Hitsound uninstalled")
    );
    assert!(
        !env.game
            .join("tf/custom/tf2tools-sounds/sound/ui/hitsound.wav")
            .exists()
    );
    assert_eq!(
        support::label_of(cx, s.win, ids::sound_slot_status(SoundKind::Hitsound)).as_deref(),
        Some("None")
    );
}

#[gpui_kit::test]
fn the_slot_play_button_toggles_the_installed_sound(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let ding = seed(&s, &env, "Ding", "wav", &wav(441));
    s.services
        .library
        .install_sound(ding.id(), SoundKind::Hitsound, &tf2(&env))
        .unwrap();
    let _page = open_sounds(cx, &s);
    support::click_id(cx, s.win, ids::sound_slot_play(SoundKind::Hitsound));
    support::wait_real(cx, "the play", |_| s.audio.plays().len() == 1);
    let installed = s
        .services
        .library
        .installed_sound_bytes(SoundKind::Hitsound, &tf2(&env))
        .unwrap();
    assert_eq!(s.audio.plays()[0].0, installed);
    assert_eq!(
        support::label_of(cx, s.win, ids::SOUND_PLAYER_NAME).as_deref(),
        Some("Ding")
    );
    assert_eq!(
        support::label_of(cx, s.win, ids::SOUND_PLAYER_SUBTITLE).as_deref(),
        Some("Installed hitsound")
    );
    support::click_id(cx, s.win, ids::sound_slot_play(SoundKind::Hitsound));
    assert!(!support::present(cx, s.win, ids::SOUND_PLAYER_NAME));
    assert_eq!(s.audio.calls().last(), Some(&AudioCall::Stop));
}

fn search(
    cx: &mut TestAppContext,
    s: &support::ShellWindow,
    page: &Entity<SoundsPage>,
    text: &str,
) {
    let input = cx.update(|cx| page.read(cx).search_input().clone());
    cx.update_window(s.win, |_, window, cx| {
        input.update(cx, |st, cx| st.set_value(text, window, cx))
    })
    .unwrap();
    support::settle(cx, s.win);
}

/// Leaves for HUDs and comes back, which builds a fresh page.
fn revisit(cx: &mut TestAppContext, s: &support::ShellWindow) -> Entity<SoundsPage> {
    support::click_id(cx, s.win, ids::nav(Page::Huds));
    open_sounds(cx, s)
}

#[gpui_kit::test]
fn the_library_shows_a_card_per_sound(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let ding = seed(&s, &env, "Ding", "wav", &wav(441));
    let boom = seed(
        &s,
        &env,
        "Boom",
        "vpk",
        &support::vpk_bytes(&[("sound/ui/hitsound.wav", &wav(441))]),
    );
    let _page = open_sounds(cx, &s);
    assert_eq!(
        support::label_of(cx, s.win, ids::sound_card(ding.folder_name())).as_deref(),
        Some("Ding")
    );
    assert_eq!(
        support::label_of(cx, s.win, ids::sound_card(boom.folder_name())).as_deref(),
        Some("Boom")
    );
    assert_eq!(
        support::label_of(cx, s.win, ids::sound_vpk(boom.folder_name())).as_deref(),
        Some("VPK")
    );
    assert!(!support::present(
        cx,
        s.win,
        ids::sound_vpk(ding.folder_name())
    ));
    assert!(support::present(
        cx,
        s.win,
        ids::sound_play(ding.folder_name())
    ));
    assert!(!support::present(cx, s.win, ids::SOUNDS_EMPTY));
}

#[gpui_kit::test]
fn an_empty_library_says_so(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let _page = open_sounds(cx, &s);
    assert_eq!(
        support::label_of(cx, s.win, ids::SOUNDS_EMPTY).as_deref(),
        Some("No sounds in your library yet")
    );
}

#[gpui_kit::test]
fn search_filters_by_name_ignoring_case(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let ding = seed(&s, &env, "Ding", "wav", &wav(441));
    let boom = seed(&s, &env, "Boom", "wav", &wav(441));
    let page = open_sounds(cx, &s);
    search(cx, &s, &page, "DING");
    assert!(support::present(
        cx,
        s.win,
        ids::sound_card(ding.folder_name())
    ));
    assert!(!support::present(
        cx,
        s.win,
        ids::sound_card(boom.folder_name())
    ));
    search(cx, &s, &page, "zzz");
    assert_eq!(
        support::label_of(cx, s.win, ids::SOUNDS_NO_MATCH).as_deref(),
        Some("No sounds match your search")
    );
    assert!(!support::present(cx, s.win, ids::SOUNDS_EMPTY));
    search(cx, &s, &page, "");
    assert!(support::present(
        cx,
        s.win,
        ids::sound_card(boom.folder_name())
    ));
}

#[gpui_kit::test]
fn a_revisit_reloads_the_library(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let _page = open_sounds(cx, &s);
    assert!(support::present(cx, s.win, ids::SOUNDS_EMPTY));
    let ding = seed(&s, &env, "Ding", "wav", &wav(441));
    let _page = revisit(cx, &s);
    assert!(support::present(
        cx,
        s.win,
        ids::sound_card(ding.folder_name())
    ));
}

#[gpui_kit::test]
fn a_card_play_button_toggles_the_library_sound(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let ding = seed(&s, &env, "Ding", "wav", &wav(441));
    let _page = open_sounds(cx, &s);
    support::click_id(cx, s.win, ids::sound_play(ding.folder_name()));
    support::wait_real(cx, "the play", |_| s.audio.plays().len() == 1);
    let bytes = s.services.library.sound_bytes(ding.id()).unwrap();
    assert_eq!(s.audio.plays(), vec![(bytes, 0.5)]);
    assert_eq!(
        support::label_of(cx, s.win, ids::SOUND_PLAYER_SUBTITLE).as_deref(),
        Some("Ding.wav")
    );
    support::click_id(cx, s.win, ids::sound_play(ding.folder_name()));
    assert!(!support::present(cx, s.win, ids::SOUND_PLAYER_NAME));
}

#[gpui_kit::test]
fn playing_another_sound_switches_the_player(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let ding = seed(&s, &env, "Ding", "wav", &wav(441));
    let boom = seed(&s, &env, "Boom", "wav", &wav(882));
    let _page = open_sounds(cx, &s);
    support::click_id(cx, s.win, ids::sound_play(ding.folder_name()));
    support::wait_real(cx, "Ding", |_| s.audio.plays().len() == 1);
    support::click_id(cx, s.win, ids::sound_play(boom.folder_name()));
    support::wait_real(cx, "Boom", |_| s.audio.plays().len() == 2);
    assert_eq!(
        support::label_of(cx, s.win, ids::SOUND_PLAYER_NAME).as_deref(),
        Some("Boom")
    );
}

#[gpui_kit::test]
fn add_sound_imports_the_picked_wav(cx: &mut TestAppContext) {
    let env = support::env();
    let path = env.tmp.path().join("Classic Ding.wav");
    std::fs::write(&path, wav(441)).unwrap();
    let picker = Arc::new(ScriptedPicker::new([path]));
    let s = support::shell_window_with(cx, &env, found(&env), picker.clone(), support::no_steam());
    let _page = open_sounds(cx, &s);
    support::click_id(cx, s.win, ids::SOUNDS_ADD);
    support::wait_real(cx, "the import toast", |cx| {
        support::present(cx, s.win, ids::TOAST)
    });
    assert_eq!(
        support::label_of(cx, s.win, ids::TOAST).as_deref(),
        Some("Sound imported. Classic Ding added to your library")
    );
    assert_eq!(picker.asked(), vec![PickKind::SoundFile]);
    let sounds = s.services.library.list_sounds().unwrap();
    assert_eq!(sounds.len(), 1);
    assert_eq!(sounds[0].name(), "Classic Ding");
    assert!(support::present(
        cx,
        s.win,
        ids::sound_card(sounds[0].folder_name())
    ));
}

#[gpui_kit::test]
fn a_48_khz_wav_is_refused_in_plain_words(cx: &mut TestAppContext) {
    let env = support::env();
    let path = env.tmp.path().join("Fast.wav");
    std::fs::write(&path, wav_at(48_000, 441)).unwrap();
    let picker = Arc::new(ScriptedPicker::new([path]));
    let s = support::shell_window_with(cx, &env, found(&env), picker, support::no_steam());
    let _page = open_sounds(cx, &s);
    support::click_id(cx, s.win, ids::SOUNDS_ADD);
    support::wait_real(cx, "the error toast", |cx| {
        support::present(cx, s.win, ids::TOAST)
    });
    assert_eq!(
        support::label_of(cx, s.win, ids::TOAST).as_deref(),
        Some(concat!(
            "Failed to import sound. TF2 can't play this sound's format. ",
            "Save it as a CD-quality stereo WAV, then try again."
        ))
    );
    assert!(s.services.library.list_sounds().unwrap().is_empty());
}

#[gpui_kit::test]
fn a_cancelled_pick_changes_nothing(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let _page = open_sounds(cx, &s);
    support::click_id(cx, s.win, ids::SOUNDS_ADD);
    support::settle(cx, s.win);
    assert!(!support::present(cx, s.win, ids::TOAST));
    assert!(support::present(cx, s.win, ids::SOUNDS_EMPTY));
}

#[gpui_kit::test]
fn hit_installs_a_library_sound_as_the_hitsound(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let ding = seed(&s, &env, "Ding", "wav", &wav(441));
    let _page = open_sounds(cx, &s);
    support::click_id(cx, s.win, ids::sound_hit(ding.folder_name()));
    support::wait_real(cx, "the install toast", |cx| {
        support::present(cx, s.win, ids::TOAST)
    });
    assert_eq!(
        support::label_of(cx, s.win, ids::TOAST).as_deref(),
        Some("Hitsound installed")
    );
    assert!(
        env.game
            .join("tf/custom/tf2tools-sounds/sound/ui/hitsound.wav")
            .is_file()
    );
    assert_eq!(
        support::label_of(cx, s.win, ids::sound_slot_name(SoundKind::Hitsound)).as_deref(),
        Some("Ding")
    );
}

#[gpui_kit::test]
fn kill_installs_a_vpk_as_the_killsound(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let pak = support::vpk_bytes(&[("sound/ui/killsound.wav", &wav(441))]);
    let boom = seed(&s, &env, "Boom", "vpk", &pak);
    let _page = open_sounds(cx, &s);
    support::click_id(cx, s.win, ids::sound_kill(boom.folder_name()));
    support::wait_real(cx, "the install toast", |cx| {
        support::present(cx, s.win, ids::TOAST)
    });
    assert_eq!(
        support::label_of(cx, s.win, ids::TOAST).as_deref(),
        Some("Killsound installed")
    );
    assert!(env.game.join("tf/custom/tf2tools-killsound.vpk").is_file());
    assert_eq!(
        support::label_of(cx, s.win, ids::sound_slot_vpk(SoundKind::Killsound)).as_deref(),
        Some("VPK")
    );
}

// Sets the shared TF2 state, as Settings or a recheck would.
fn set_tf2(cx: &mut TestAppContext, s: &support::ShellWindow, state: Tf2State) {
    cx.update(|cx| {
        s.tf2.update(cx, |tf2, cx| {
            *tf2 = state;
            cx.notify();
        })
    });
    support::settle(cx, s.win);
}

#[gpui_kit::test]
fn without_tf2_the_page_warns_and_shuts_installs(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, Tf2State::Missing { saved: None });
    let ding = seed(&s, &env, "Ding", "wav", &wav(441));
    let _page = open_sounds(cx, &s);
    let folder = ding.folder_name();
    assert!(support::present(cx, s.win, ids::SOUNDS_NO_TF2));
    assert!(support::disabled_of(cx, s.win, ids::sound_hit(folder)));
    assert!(support::disabled_of(cx, s.win, ids::sound_kill(folder)));
    assert!(!support::disabled_of(cx, s.win, ids::SOUNDS_ADD));
    assert!(!support::disabled_of(cx, s.win, ids::sound_play(folder)));
    assert!(!support::disabled_of(cx, s.win, ids::sound_delete(folder)));
    support::click_id(cx, s.win, ids::SOUNDS_NO_TF2_SETTINGS);
    assert!(support::present(cx, s.win, ids::page(Page::Settings)));
    assert!(!env.game.join("tf/custom/tf2tools-sounds").exists());
}

#[gpui_kit::test]
fn finding_tf2_clears_the_warning_and_opens_installs(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, Tf2State::Missing { saved: None });
    let ding = seed(&s, &env, "Ding", "wav", &wav(441));
    let _page = open_sounds(cx, &s);
    assert!(support::present(cx, s.win, ids::SOUNDS_NO_TF2));
    set_tf2(cx, &s, found(&env));
    assert!(!support::present(cx, s.win, ids::SOUNDS_NO_TF2));
    assert!(!support::disabled_of(
        cx,
        s.win,
        ids::sound_hit(ding.folder_name())
    ));
}

#[gpui_kit::test]
fn delete_asks_then_removes(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let ding = seed(&s, &env, "Ding", "wav", &wav(441));
    let _page = open_sounds(cx, &s);
    support::click_id(cx, s.win, ids::sound_delete(ding.folder_name()));
    assert_eq!(
        support::label_of(cx, s.win, ids::CONFIRM_DESCRIPTION).as_deref(),
        Some(
            "Are you sure you want to delete this sound from your library? This cannot be undone."
        )
    );
    support::click_id(cx, s.win, ids::CONFIRM_OK);
    support::wait_real(cx, "the delete toast", |cx| {
        support::present(cx, s.win, ids::TOAST)
    });
    assert_eq!(
        support::label_of(cx, s.win, ids::TOAST).as_deref(),
        Some("Sound deleted")
    );
    assert!(s.services.library.list_sounds().unwrap().is_empty());
    assert!(!support::present(
        cx,
        s.win,
        ids::sound_card(ding.folder_name())
    ));
}

#[gpui_kit::test]
fn deleting_the_playing_sound_stops_the_player(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let ding = seed(&s, &env, "Ding", "wav", &wav(441));
    let _page = open_sounds(cx, &s);
    support::click_id(cx, s.win, ids::sound_play(ding.folder_name()));
    support::wait_real(cx, "the play", |_| s.audio.plays().len() == 1);
    support::click_id(cx, s.win, ids::sound_delete(ding.folder_name()));
    support::click_id(cx, s.win, ids::CONFIRM_OK);
    support::wait_real(cx, "the delete toast", |cx| {
        support::present(cx, s.win, ids::TOAST)
    });
    assert!(!support::present(cx, s.win, ids::SOUND_PLAYER_NAME));
}

#[gpui_kit::test]
fn cancelling_delete_keeps_the_sound(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let ding = seed(&s, &env, "Ding", "wav", &wav(441));
    let _page = open_sounds(cx, &s);
    support::click_id(cx, s.win, ids::sound_delete(ding.folder_name()));
    support::click_id(cx, s.win, ids::CONFIRM_CANCEL);
    support::settle(cx, s.win);
    assert_eq!(s.services.library.list_sounds().unwrap().len(), 1);
    assert!(!support::present(cx, s.win, ids::TOAST));
}

fn row(e: &LogEntry) -> (&str, &str, &str) {
    (e.category(), e.action(), e.message())
}

#[gpui_kit::test]
fn sound_operations_write_v5s_rows(cx: &mut TestAppContext) {
    let env = support::env();
    let path = env.tmp.path().join("Ding.wav");
    std::fs::write(&path, wav(441)).unwrap();
    let picker = Arc::new(ScriptedPicker::new([path]));
    let s = support::shell_window_with(cx, &env, found(&env), picker, support::no_steam());
    let _page = open_sounds(cx, &s);
    // Each step waits for its row, then leaves and reopens Sounds, so the next click lands on
    // a fresh, idle page.
    let rows_then_revisit = |cx: &mut TestAppContext, n: usize| {
        support::wait_real(cx, "the row", |_| support::log_rows(&s.services).len() == n);
        support::click_id(cx, s.win, ids::nav(Page::Settings));
        open_sounds(cx, &s);
    };
    support::click_id(cx, s.win, ids::SOUNDS_ADD);
    rows_then_revisit(cx, 1);
    let folder = s.services.library.list_sounds().unwrap()[0]
        .folder_name()
        .to_owned();
    support::click_id(cx, s.win, ids::sound_hit(&folder));
    rows_then_revisit(cx, 2);
    support::click_id(cx, s.win, ids::sound_slot_uninstall(SoundKind::Hitsound));
    rows_then_revisit(cx, 3);
    support::click_id(cx, s.win, ids::sound_delete(&folder));
    support::click_id(cx, s.win, ids::CONFIRM_OK);
    support::wait_real(cx, "the row", |_| support::log_rows(&s.services).len() == 4);
    let rows = support::log_rows(&s.services);
    let got: Vec<_> = rows.iter().rev().map(row).collect();
    assert_eq!(
        got,
        [
            ("sound", "import", "Imported sound: Ding"),
            // The sound's name, never its library folder.
            ("sound", "install", "Installed hitsound: Ding"),
            ("sound", "uninstall", "Uninstalled hitsound"),
            ("sound", "delete", "Deleted sound: Ding"),
        ]
    );
    assert_eq!(
        rows[3].details(),
        Some("Format: 16-bit PCM, 44100Hz, Stereo")
    );
}

#[gpui_kit::test]
fn a_library_change_reloads_the_open_page(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let _page = open_sounds(cx, &s);
    assert!(support::present(cx, s.win, ids::SOUNDS_EMPTY));
    let ding = seed(&s, &env, "Ding", "wav", &wav(441));
    cx.update(tf2tools_app::library_events::LibraryEvents::changed);
    support::wait_real(cx, "the reload", |cx| {
        support::present(cx, s.win, ids::sound_card(ding.folder_name()))
    });
}

#[gpui_kit::test]
fn an_uninstall_that_ends_after_leaving_still_announces_the_change(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let ding = seed(&s, &env, "Ding", "wav", &wav(441));
    s.services
        .library
        .install_sound(ding.id(), SoundKind::Hitsound, &tf2(&env))
        .unwrap();
    let page = open_sounds(cx, &s);
    let (count, _sub) = support::library_changes(cx);
    // Start the uninstall and leave in one update, so the page is gone when the task resumes.
    cx.update_window(s.win, |_, window, cx| {
        window.click(ids::sound_slot_uninstall(SoundKind::Hitsound), cx);
        s.shell
            .update(cx, |shell, cx| shell.navigate(Page::Huds, cx));
    })
    .unwrap();
    drop(page);
    support::wait_real(cx, "the uninstall toast", |cx| {
        support::present(cx, s.win, ids::TOAST)
    });
    assert!(cx.update(|cx| s.shell.read(cx).sounds_page()).is_none());
    assert_eq!(count.get(), 1);
}
