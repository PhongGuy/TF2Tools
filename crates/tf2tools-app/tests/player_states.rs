//! The player's phases: a toggle loads first and plays once its bytes arrive, and the floating
//! player shows a spinner, not Stop, while the bytes are on their way.

use async_channel::Sender;
use gpui_kit::{AppContext as _, TestAppContext};
use tf2tools_app::audio::AudioCall;
use tf2tools_app::audio::player::PlayState;
use tf2tools_app::ids;
use tf2tools_app::services::OpError;

use crate::player::{Host, host, now, playing, plays};
use crate::support;

type Load = Result<Vec<u8>, OpError>;

// Toggles `name` with a load that resolves only when the test sends through the returned gate.
fn toggle_gated(cx: &mut TestAppContext, h: &Host, name: &str) -> Sender<Load> {
    let (tx, rx) = async_channel::bounded::<Load>(1);
    let now = now(name);
    cx.update(|cx| {
        h.player.update(cx, |p, cx| {
            p.toggle_with(
                now,
                move |cx| {
                    cx.background_spawn(async move {
                        rx.recv()
                            .await
                            .unwrap_or_else(|_| Err(OpError::core("load".into(), "gate dropped")))
                    })
                },
                cx,
            )
            .detach()
        })
    });
    tx
}

fn state(cx: &mut TestAppContext, h: &Host, key: &str) -> Option<PlayState> {
    cx.update(|cx| h.player.read(cx).state_of(key))
}

fn release(tx: &Sender<Load>, load: Load) {
    tx.try_send(load).unwrap();
}

#[gpui_kit::test]
fn a_toggle_loads_then_plays_then_ends(cx: &mut TestAppContext) {
    let h = host(cx, 50);
    assert_eq!(state(cx, &h, "Ding"), None);
    let gate = toggle_gated(cx, &h, "Ding");
    support::settle(cx, h.win);
    assert_eq!(state(cx, &h, "Ding"), Some(PlayState::Loading));
    assert!(plays(&h).is_empty());
    assert!(support::present(cx, h.win, ids::SOUND_PLAYER_SPINNER));
    assert!(!support::present(cx, h.win, ids::SOUND_PLAYER_STOP));
    assert_eq!(
        support::label_of(cx, h.win, ids::SOUND_PLAYER_CANCEL).as_deref(),
        Some("Cancel")
    );
    assert_eq!(
        support::label_of(cx, h.win, ids::SOUND_PLAYER_NAME).as_deref(),
        Some("Ding")
    );

    release(&gate, Ok(vec![4, 2]));
    support::wait_real(cx, "the play", |_| plays(&h).len() == 1);
    assert_eq!(state(cx, &h, "Ding"), Some(PlayState::Playing));
    assert_eq!(plays(&h), [vec![4, 2]]);
    assert!(support::present(cx, h.win, ids::SOUND_PLAYER_STOP));
    assert!(!support::present(cx, h.win, ids::SOUND_PLAYER_SPINNER));

    h.audio.finish();
    support::wait_real(cx, "the end", |cx| {
        cx.update(|cx| h.player.read(cx).now_playing().is_none())
    });
    assert_eq!(state(cx, &h, "Ding"), None);
}

#[gpui_kit::test]
fn a_second_toggle_while_loading_cancels(cx: &mut TestAppContext) {
    let h = host(cx, 50);
    let gate = toggle_gated(cx, &h, "Ding");
    support::settle(cx, h.win);
    let _again = toggle_gated(cx, &h, "Ding");
    support::settle(cx, h.win);
    assert_eq!(state(cx, &h, "Ding"), None);
    assert_eq!(playing(cx, &h), None);
    release(&gate, Ok(vec![1]));
    support::settle(cx, h.win);
    assert!(plays(&h).is_empty());
    assert_eq!(state(cx, &h, "Ding"), None);
}

#[gpui_kit::test]
fn a_failed_load_leaves_no_phase(cx: &mut TestAppContext) {
    let h = host(cx, 50);
    let gate = toggle_gated(cx, &h, "Ding");
    support::settle(cx, h.win);
    release(
        &gate,
        Err(OpError::core(
            "Failed to play sound".into(),
            "sound x not found",
        )),
    );
    support::wait_real(cx, "the error toast", |cx| {
        support::present(cx, h.win, ids::TOAST)
    });
    assert_eq!(
        support::label_of(cx, h.win, ids::TOAST).as_deref(),
        Some("Failed to play sound. sound x not found")
    );
    assert_eq!(state(cx, &h, "Ding"), None);
    assert!(plays(&h).is_empty());
    assert!(!support::present(cx, h.win, ids::SOUND_PLAYER_SPINNER));
}

#[gpui_kit::test]
fn a_newer_toggle_replaces_a_loading_one(cx: &mut TestAppContext) {
    let h = host(cx, 50);
    let a = toggle_gated(cx, &h, "A");
    support::settle(cx, h.win);
    let b = toggle_gated(cx, &h, "B");
    support::settle(cx, h.win);
    assert_eq!(state(cx, &h, "A"), None);
    assert_eq!(state(cx, &h, "B"), Some(PlayState::Loading));
    release(&a, Ok(vec![1]));
    support::settle(cx, h.win);
    assert!(plays(&h).is_empty());
    release(&b, Ok(vec![2]));
    support::wait_real(cx, "B to play", |_| plays(&h).len() == 1);
    assert_eq!(plays(&h), [vec![2]]);
    assert_eq!(state(cx, &h, "B"), Some(PlayState::Playing));
}

#[gpui_kit::test]
fn clicking_the_loading_button_cancels(cx: &mut TestAppContext) {
    let h = host(cx, 50);
    let gate = toggle_gated(cx, &h, "Ding");
    support::settle(cx, h.win);
    support::click_id(cx, h.win, ids::SOUND_PLAYER_SPINNER);
    assert_eq!(state(cx, &h, "Ding"), None);
    assert_eq!(playing(cx, &h), None);
    assert_eq!(h.audio.calls().last(), Some(&AudioCall::Stop));
    release(&gate, Ok(vec![1]));
    support::settle(cx, h.win);
    assert!(plays(&h).is_empty());
}

// The loading button swaps in for Stop, so it must take Stop's exact place and size.
#[gpui_kit::test]
fn the_loading_button_sits_where_stop_does(cx: &mut TestAppContext) {
    let h = host(cx, 50);
    let gate = toggle_gated(cx, &h, "Ding");
    support::settle(cx, h.win);
    let cancel =
        support::bounds_of(cx, h.win, ids::SOUND_PLAYER_CANCEL).expect("the cancel button");
    release(&gate, Ok(vec![1]));
    support::wait_real(cx, "the play", |_| plays(&h).len() == 1);
    let stop = support::bounds_of(cx, h.win, ids::SOUND_PLAYER_STOP).expect("the stop button");
    assert_eq!(cancel, stop);
}

// The spinner stands in for the 16 px icon: drawn inside the button, never larger than the icon.
#[gpui_kit::test]
fn the_player_spinner_fits_inside_its_button(cx: &mut TestAppContext) {
    let h = host(cx, 50);
    let _gate = toggle_gated(cx, &h, "Ding");
    support::settle(cx, h.win);
    let button = support::bounds_of(cx, h.win, ids::SOUND_PLAYER_CANCEL).unwrap();
    let spinner = support::bounds_of(cx, h.win, ids::SOUND_PLAYER_SPINNER).unwrap();
    assert!(
        button.contains(&spinner.origin) && button.contains(&spinner.bottom_right()),
        "{spinner:?} inside {button:?}"
    );
    assert!(
        spinner.size.width <= gpui_kit::px(16.) && spinner.size.height <= gpui_kit::px(16.),
        "{:?}",
        spinner.size
    );
}
