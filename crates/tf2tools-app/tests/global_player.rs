//! One sound player for the whole app: a sound started on Sounds or in the Sound Store keeps
//! playing, and the player keeps floating, on every other page until it ends or is stopped.

use crate::support;

use gpui_kit::{AppContext as _, Entity, TestAppContext};
use tf2tools_app::audio::AudioCall;
use tf2tools_app::audio::player::{NowPlaying, Player};
use tf2tools_app::ids;
use tf2tools_app::services::Tf2State;
use tf2tools_app::shell::route::Page;
use tf2tools_app::shell::toast::{ToastKind, toast};
use tf2tools_core::{GbId, StoreItemId, StoreKind};

const S: StoreKind = StoreKind::Sounds;

fn missing() -> Tf2State {
    Tf2State::Missing { saved: None }
}

fn gb(id: u64) -> StoreItemId {
    StoreItemId::GameBanana(GbId::new(id))
}

fn go(cx: &mut TestAppContext, s: &support::ShellWindow, page: Page) {
    support::click_id(cx, s.win, ids::nav(page));
    support::settle(cx, s.win);
}

/// The Sounds page's player, once the page has loaded and read the saved volume.
fn sounds_player(cx: &mut TestAppContext, s: &support::ShellWindow) -> Entity<Player> {
    go(cx, s, Page::Sounds);
    let page = cx
        .update(|cx| s.shell.read(cx).sounds_page())
        .expect("a Sounds page");
    support::wait_real(cx, "Sounds to load", |cx| {
        cx.update(|cx| page.read(cx).is_loaded() && page.read(cx).player().read(cx).is_ready())
    });
    cx.update(|cx| page.read(cx).player().clone())
}

/// The Sound Store's player, once it has read the saved volume.
fn store_player(cx: &mut TestAppContext, s: &support::ShellWindow) -> Entity<Player> {
    go(cx, s, Page::SoundStore);
    let page = cx
        .update(|cx| s.shell.read(cx).sound_store_page())
        .expect("the Sound Store");
    let player = cx.update(|cx| page.read(cx).player().clone());
    support::wait_real(cx, "the player", |cx| {
        cx.update(|cx| player.read(cx).is_ready())
    });
    player
}

/// Plays `key` through `player` as a play button would, and waits for the sound to start.
fn play(cx: &mut TestAppContext, s: &support::ShellWindow, player: &Entity<Player>, key: &str) {
    let now = NowPlaying {
        key: key.to_owned().into(),
        name: "Ding".into(),
        subtitle: "by vex".into(),
    };
    let before = s.audio.plays().len();
    cx.update(|cx| player.update(cx, |p, cx| p.toggle(now, |_| Ok(vec![1]), cx).detach()));
    support::wait_real(cx, "the play", |_| s.audio.plays().len() == before + 1);
    support::settle(cx, s.win);
}

// Still playing: the player holds the sound, shows it, and nothing stopped the audio since.
fn assert_still_playing(
    cx: &mut TestAppContext,
    s: &support::ShellWindow,
    player: &Entity<Player>,
) {
    assert!(
        cx.update(|cx| player.read(cx).now_playing().is_some()),
        "the player still holds the sound"
    );
    assert_ne!(
        s.audio.calls().last(),
        Some(&AudioCall::Stop),
        "nothing stopped it"
    );
    assert!(
        support::present(cx, s.win, ids::SOUND_PLAYER),
        "the player still floats"
    );
    assert_eq!(
        support::label_of(cx, s.win, ids::SOUND_PLAYER_NAME).as_deref(),
        Some("Ding"),
        "the player should still name the sound"
    );
}

#[gpui_kit::test]
fn a_sound_from_the_sounds_page_keeps_playing_on_the_huds_page(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, missing());
    let player = sounds_player(cx, &s);
    play(cx, &s, &player, "Ding");
    go(cx, &s, Page::Huds);
    assert_still_playing(cx, &s, &player);
}

#[gpui_kit::test]
fn a_store_preview_keeps_playing_on_the_huds_page(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, missing());
    let player = store_player(cx, &s);
    play(cx, &s, &player, "store-sound:1");
    go(cx, &s, Page::Huds);
    assert_still_playing(cx, &s, &player);
}

#[gpui_kit::test]
fn sounds_and_the_sound_store_share_one_player(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, missing());
    let sounds = sounds_player(cx, &s);
    play(cx, &s, &sounds, "Ding");
    let store = store_player(cx, &s);
    assert_eq!(sounds.entity_id(), store.entity_id());
    assert_still_playing(cx, &s, &store);
}

#[gpui_kit::test]
fn the_player_stops_from_another_page(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, missing());
    let player = sounds_player(cx, &s);
    play(cx, &s, &player, "Ding");
    go(cx, &s, Page::Settings);
    support::click_id(cx, s.win, ids::SOUND_PLAYER_STOP);
    support::settle(cx, s.win);
    assert_eq!(s.audio.calls().last(), Some(&AudioCall::Stop));
    assert!(cx.update(|cx| player.read(cx).now_playing().is_none()));
    assert!(
        !support::present(cx, s.win, ids::SOUND_PLAYER),
        "off the sound pages the idle player hides"
    );
}

#[gpui_kit::test]
fn the_idle_player_shows_only_on_the_sound_pages(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, missing());
    assert!(!support::present(cx, s.win, ids::SOUND_PLAYER), "HUDs");
    let _ = sounds_player(cx, &s);
    assert!(support::present(cx, s.win, ids::SOUND_PLAYER), "Sounds");
    let _ = store_player(cx, &s);
    assert!(
        support::present(cx, s.win, ids::SOUND_PLAYER),
        "Sound Store"
    );
    go(cx, &s, Page::Crosshairs);
    assert!(
        !support::present(cx, s.win, ids::SOUND_PLAYER),
        "Crosshairs"
    );
}

#[gpui_kit::test]
fn toasts_sit_above_the_player_while_it_plays_on_another_page(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, missing());
    let player = sounds_player(cx, &s);
    play(cx, &s, &player, "Ding");
    go(cx, &s, Page::Huds);
    cx.update_window(s.win, |_, window, cx| {
        toast(window, cx, ToastKind::Success, "Hello", "")
    })
    .unwrap();
    support::settle(cx, s.win);
    let toast = support::bounds_of(cx, s.win, ids::TOAST).unwrap();
    let floating = support::bounds_of(cx, s.win, ids::SOUND_PLAYER).unwrap();
    assert!(
        toast.bottom() <= floating.top(),
        "{toast:?} overlaps {floating:?}"
    );
}

// The kept Sound Store's cards read the one player, so a return shows Stop on the card still
// playing, and Play once the sound was stopped elsewhere.
#[gpui_kit::test]
fn returning_to_the_sound_store_shows_what_plays(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, missing());
    support::seed_sounds(
        &env,
        &[
            (1, "Ding", "hitsound", Some("http://127.0.0.1:9/1.mp3")),
            (2, "Boom", "killsound", Some("http://127.0.0.1:9/2.mp3")),
        ],
    );
    let player = store_player(cx, &s);
    let page = cx.update(|cx| s.shell.read(cx).sound_store_page()).unwrap();
    support::wait_real(cx, "the grid", |cx| {
        cx.update(|cx| page.read(cx).grid().read(cx).is_loaded())
    });
    support::settle(cx, s.win);
    let label = |cx: &mut TestAppContext, id: u64| {
        support::label_of(cx, s.win, ids::store_play(S, &gb(id)))
    };
    play(cx, &s, &player, "store-sound:1");
    assert_eq!(label(cx, 1).as_deref(), Some("Stop"));

    go(cx, &s, Page::Huds);
    go(cx, &s, Page::SoundStore);
    assert_eq!(
        label(cx, 1).as_deref(),
        Some("Stop"),
        "still playing on return"
    );
    assert_eq!(label(cx, 2).as_deref(), Some("Play"));

    go(cx, &s, Page::Huds);
    support::click_id(cx, s.win, ids::SOUND_PLAYER_STOP);
    go(cx, &s, Page::SoundStore);
    assert_eq!(label(cx, 1).as_deref(), Some("Play"), "stopped while away");
}
