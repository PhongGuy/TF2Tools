use crate::support;

use std::sync::Arc;

use gpui_kit::{AnyWindowHandle, App, AppContext as _, Entity, Task, TestAppContext, px};
use tf2tools_app::audio::player::{NowPlaying, Player};
use tf2tools_app::audio::{AudioCall, AudioError, RecordingAudio};
use tf2tools_app::ids;
use tf2tools_app::services::{OpError, Services};
use tf2tools_core::Volume;

/// A window whose root is a Player, over a fresh library saved at `volume` percent.
struct Host {
    win: AnyWindowHandle,
    player: Entity<Player>,
    audio: Arc<RecordingAudio>,
    services: Services,
    _env: support::Env,
}

fn host(cx: &mut TestAppContext, volume: u8) -> Host {
    let env = support::env();
    support::init(cx);
    let audio = Arc::new(RecordingAudio::new());
    let services = support::services_with_audio(&env, audio.clone());
    services
        .db
        .set_volume(Volume::new(volume).unwrap())
        .unwrap();
    support::install_services(cx, services.clone());
    let (win, player) = cx
        .update(|cx| {
            gpui_kit::open_window(support::window_options(), cx, |window, cx| {
                cx.new(|cx| Player::new(window, cx))
            })
        })
        .unwrap();
    support::wait_real(cx, "the saved volume", |cx| {
        cx.update(|cx| player.read(cx).is_ready())
    });
    support::settle(cx, win);
    Host {
        win,
        player,
        audio,
        services,
        _env: env,
    }
}

fn now(name: &str) -> NowPlaying {
    NowPlaying {
        key: name.to_owned().into(),
        name: name.to_owned().into(),
        subtitle: format!("{name}.wav").into(),
    }
}

fn toggle(cx: &mut TestAppContext, h: &Host, name: &str, bytes: Vec<u8>) {
    let now = now(name);
    cx.update(|cx| {
        h.player
            .update(cx, |p, cx| p.toggle(now, move |_| Ok(bytes), cx).detach())
    });
}

fn plays(h: &Host) -> Vec<Vec<u8>> {
    h.audio.plays().into_iter().map(|(wav, _)| wav).collect()
}

fn playing(cx: &mut TestAppContext, h: &Host) -> Option<String> {
    cx.update(|cx| h.player.read(cx).now_playing().map(|n| n.name.to_string()))
}

#[gpui_kit::test]
fn the_idle_player_shows_only_the_saved_volume(cx: &mut TestAppContext) {
    let h = host(cx, 70);
    assert_eq!(
        support::label_of(cx, h.win, ids::SOUND_PLAYER_VOLUME_VALUE).as_deref(),
        Some("70")
    );
    assert!(support::present(cx, h.win, ids::SOUND_PLAYER_VOLUME));
    assert!(!support::present(cx, h.win, ids::SOUND_PLAYER_STOP));
    assert!(!support::present(cx, h.win, ids::SOUND_PLAYER_NAME));
    let card = support::bounds_of(cx, h.win, ids::SOUND_PLAYER).unwrap();
    assert_eq!(card.size.width, px(288.));
}

#[gpui_kit::test]
fn toggle_plays_the_loaded_bytes_and_shows_the_sound(cx: &mut TestAppContext) {
    let h = host(cx, 50);
    toggle(cx, &h, "Ding", vec![1, 2, 3]);
    support::wait_real(cx, "the play", |_| h.audio.plays().len() == 1);
    assert_eq!(h.audio.plays(), vec![(vec![1, 2, 3], 0.5)]);
    assert_eq!(
        support::label_of(cx, h.win, ids::SOUND_PLAYER_NAME).as_deref(),
        Some("Ding")
    );
    assert_eq!(
        support::label_of(cx, h.win, ids::SOUND_PLAYER_SUBTITLE).as_deref(),
        Some("Ding.wav")
    );
    assert!(support::present(cx, h.win, ids::SOUND_PLAYER_STOP));
    assert_eq!(playing(cx, &h).as_deref(), Some("Ding"));
}

#[gpui_kit::test]
fn toggling_the_playing_sound_stops_it(cx: &mut TestAppContext) {
    let h = host(cx, 50);
    toggle(cx, &h, "Ding", vec![1]);
    support::wait_real(cx, "the play", |_| h.audio.plays().len() == 1);
    toggle(cx, &h, "Ding", vec![1]);
    support::settle(cx, h.win);
    assert_eq!(playing(cx, &h), None);
    assert_eq!(h.audio.calls().last(), Some(&AudioCall::Stop));
    assert_eq!(h.audio.plays().len(), 1);
    assert!(!support::present(cx, h.win, ids::SOUND_PLAYER_NAME));
}

#[gpui_kit::test]
fn the_stop_button_stops(cx: &mut TestAppContext) {
    let h = host(cx, 50);
    toggle(cx, &h, "Ding", vec![1]);
    support::wait_real(cx, "the play", |_| h.audio.plays().len() == 1);
    support::click_id(cx, h.win, ids::SOUND_PLAYER_STOP);
    assert_eq!(playing(cx, &h), None);
    assert_eq!(h.audio.calls().last(), Some(&AudioCall::Stop));
}

#[gpui_kit::test]
fn a_newer_play_wins_over_a_slower_load(cx: &mut TestAppContext) {
    let h = host(cx, 50);
    cx.update(|cx| {
        h.player.update(cx, |p, cx| {
            p.toggle(now("A"), |_| Ok(vec![1]), cx).detach();
            p.toggle(now("B"), |_| Ok(vec![2]), cx).detach();
        })
    });
    support::wait_real(cx, "B to play", |_| plays(&h).contains(&vec![2]));
    support::settle(cx, h.win);
    assert_eq!(plays(&h), vec![vec![2]]);
    assert_eq!(
        support::label_of(cx, h.win, ids::SOUND_PLAYER_NAME).as_deref(),
        Some("B")
    );
}

#[gpui_kit::test]
fn a_stop_during_the_load_drops_it(cx: &mut TestAppContext) {
    let h = host(cx, 50);
    cx.update(|cx| {
        h.player.update(cx, |p, cx| {
            p.toggle(now("A"), |_| Ok(vec![1]), cx).detach();
            p.stop(cx);
        })
    });
    toggle(cx, &h, "B", vec![2]);
    support::wait_real(cx, "B to play", |_| plays(&h).contains(&vec![2]));
    support::settle(cx, h.win);
    assert_eq!(plays(&h), vec![vec![2]]);
}

#[gpui_kit::test]
fn a_sound_that_ends_clears_the_player(cx: &mut TestAppContext) {
    let h = host(cx, 50);
    toggle(cx, &h, "Ding", vec![1]);
    support::wait_real(cx, "the play", |_| h.audio.plays().len() == 1);
    h.audio.finish();
    support::wait_real(cx, "the player to clear", |cx| {
        !support::present(cx, h.win, ids::SOUND_PLAYER_NAME)
    });
    assert_eq!(playing(cx, &h), None);
}

#[gpui_kit::test]
fn a_failed_play_toasts_and_clears(cx: &mut TestAppContext) {
    let h = host(cx, 50);
    h.audio.fail_next(AudioError::Device("no device".into()));
    toggle(cx, &h, "Ding", vec![1]);
    support::wait_real(cx, "the error toast", |cx| {
        support::present(cx, h.win, ids::TOAST)
    });
    assert_eq!(
        support::label_of(cx, h.win, ids::TOAST).as_deref(),
        Some(
            "Failed to play sound. Couldn't play sound on this computer. Check that your speakers or headphones are connected, then try again."
        )
    );
    assert_eq!(playing(cx, &h), None);
}

#[gpui_kit::test]
fn a_failed_load_toasts_and_plays_nothing(cx: &mut TestAppContext) {
    let h = host(cx, 50);
    let now = now("Ding");
    cx.update(|cx| {
        h.player.update(cx, |p, cx| {
            p.toggle(
                now,
                |_| Err(OpError::core("Failed to play sound", "sound x not found")),
                cx,
            )
            .detach()
        })
    });
    support::wait_real(cx, "the error toast", |cx| {
        support::present(cx, h.win, ids::TOAST)
    });
    assert_eq!(
        support::label_of(cx, h.win, ids::TOAST).as_deref(),
        Some("Failed to play sound. sound x not found")
    );
    assert!(h.audio.plays().is_empty());
    assert_eq!(playing(cx, &h), None);
}

#[gpui_kit::test]
fn a_volume_change_is_live_and_unsaved(cx: &mut TestAppContext) {
    let h = host(cx, 70);
    let slider = cx.update(|cx| h.player.read(cx).volume().clone());
    cx.update_window(h.win, |_, window, cx| {
        slider.update(cx, |state, cx| state.set_value(30., window, cx))
    })
    .unwrap();
    support::settle(cx, h.win);
    assert_eq!(h.audio.calls().last(), Some(&AudioCall::SetVolume(0.3)));
    assert_eq!(
        support::label_of(cx, h.win, ids::SOUND_PLAYER_VOLUME_VALUE).as_deref(),
        Some("30")
    );
    assert_eq!(h.services.db.settings().unwrap().volume().get(), 70);
    toggle(cx, &h, "Ding", vec![1]);
    support::wait_real(cx, "the play", |_| h.audio.plays().len() == 1);
    assert_eq!(h.audio.plays()[0].1, 0.3);
}

#[gpui_kit::test]
fn releasing_the_player_stops_the_sound(cx: &mut TestAppContext) {
    let h = host(cx, 50);
    toggle(cx, &h, "Ding", vec![1]);
    support::wait_real(cx, "the play", |_| h.audio.plays().len() == 1);
    let Host {
        win,
        player,
        audio,
        _env,
        ..
    } = h;
    let weak = player.downgrade();
    cx.update_window(win, |_, window, _| window.remove_window())
        .unwrap();
    drop(player);
    cx.run_until_parked();
    // Release observers run when the app next flushes its effects.
    cx.update(|_| ());
    assert!(cx.update(|_| weak.upgrade().is_none()));
    assert_eq!(audio.calls().last(), Some(&AudioCall::Stop));
}

#[gpui_kit::test]
fn the_idle_player_is_as_big_as_v5s(cx: &mut TestAppContext) {
    let h = host(cx, 50);
    let card = support::bounds_of(cx, h.win, ids::SOUND_PLAYER).unwrap();
    // v5: w-72, a 1 px border, p-3 and a 16 px volume row.
    assert_eq!((card.size.width, card.size.height), (px(288.), px(42.)));
}

#[gpui_kit::test]
fn the_volume_slider_fits_its_wrapper(cx: &mut TestAppContext) {
    let h = host(cx, 50);
    let card = support::bounds_of(cx, h.win, ids::SOUND_PLAYER).unwrap();
    let wrapper = support::bounds_of(cx, h.win, ids::SOUND_PLAYER_VOLUME).unwrap();
    let value = support::bounds_of(cx, h.win, ids::SOUND_PLAYER_VOLUME_VALUE).unwrap();
    // gpui-component's slider track takes the mouse-down that sets the value: the hit area.
    let track = support::bounds_of(cx, h.win, "slider-bar-container").unwrap();
    assert!(
        wrapper.top() <= track.top() && track.bottom() <= wrapper.bottom(),
        "track {track:?} overflows wrapper {wrapper:?}"
    );
    assert!(wrapper.left() <= track.left() && track.right() <= wrapper.right());
    assert!(
        card.top() <= wrapper.top() && wrapper.bottom() <= card.bottom(),
        "wrapper {wrapper:?} leaves card {card:?}"
    );
    assert_eq!(support::mid_y(track), support::mid_y(value), "one row");
    assert_eq!(card.size.height, px(42.), "v5's idle card");
}

#[gpui_kit::test]
fn the_playing_player_is_as_tall_as_v5s(cx: &mut TestAppContext) {
    let h = host(cx, 50);
    toggle(cx, &h, "Ding", vec![1]);
    support::wait_real(cx, "the play", |_| h.audio.plays().len() == 1);
    let card = support::bounds_of(cx, h.win, ids::SOUND_PLAYER).unwrap();
    // v5 adds mb-2 and a 36 px row: a 20 px text-sm name over a 16 px text-xs subtitle.
    assert_eq!(card.size.height, px(86.));
}

#[gpui_kit::test]
fn toggle_with_plays_what_the_started_task_loads(cx: &mut TestAppContext) {
    let h = host(cx, 50);
    let now = now("Ding");
    cx.update(|cx| {
        h.player.update(cx, |p, cx| {
            p.toggle_with(
                now,
                |cx| cx.background_spawn(async { Ok::<_, OpError>(vec![7]) }),
                cx,
            )
            .detach()
        })
    });
    support::wait_real(cx, "the play", |_| h.audio.plays().len() == 1);
    assert_eq!(plays(&h), [vec![7]]);
    assert_eq!(playing(cx, &h).as_deref(), Some("Ding"));
}

#[gpui_kit::test]
fn toggle_with_on_the_playing_key_stops_without_starting_a_load(cx: &mut TestAppContext) {
    let h = host(cx, 50);
    toggle(cx, &h, "Ding", vec![1]);
    support::wait_real(cx, "the play", |_| h.audio.plays().len() == 1);
    let now = now("Ding");
    cx.update(|cx| {
        h.player.update(cx, |p, cx| {
            p.toggle_with(
                now,
                |_: &mut App| -> Task<Result<Vec<u8>, OpError>> { panic!("a stop must not load") },
                cx,
            )
            .detach()
        })
    });
    assert_eq!(playing(cx, &h), None);
    assert_eq!(h.audio.calls().last(), Some(&AudioCall::Stop));
}
