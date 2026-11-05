mod support;

use std::cell::Cell;
use std::rc::Rc;

use gpui_kit::AppContext as _;

use gpui_kit::TestAppContext;
use tf2tools_app::audio::AudioCall;
use tf2tools_app::ids;
use tf2tools_core::{GbId, StoreItemId, StoreKind};

const S: StoreKind = StoreKind::Sounds;

fn gb(id: u64) -> StoreItemId {
    StoreItemId::GameBanana(GbId::new(id))
}

#[gpui_kit::test]
fn the_play_button_plays_the_preview_and_a_second_click_stops_it(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    let url = server.mount_preview(1, vec![4, 5, 6]);
    let w = support::sound_store_window(cx, &env, &server, |env| {
        support::seed_sounds(
            env,
            &[
                (1, "Ding", "hitsound", Some(url.as_str())),
                (2, "Boom", "killsound", None),
            ],
        );
    });
    assert!(
        !support::present(cx, w.win, ids::store_play(S, &gb(2))),
        "no preview, no play button"
    );
    support::click_id(cx, w.win, ids::store_play(S, &gb(1)));
    support::wait_real(cx, "the preview", |_| w.audio.plays().len() == 1);
    assert_eq!(w.audio.plays()[0].0, vec![4, 5, 6]);
    assert_eq!(
        cx.update(|cx| w.page.read(cx).playing(cx)),
        Some(GbId::new(1))
    );
    assert_eq!(
        support::label_of(cx, w.win, ids::SOUND_PLAYER_NAME).as_deref(),
        Some("Ding")
    );
    assert_eq!(
        support::label_of(cx, w.win, ids::SOUND_PLAYER_SUBTITLE).as_deref(),
        Some("by vex")
    );
    support::click_id(cx, w.win, ids::store_play(S, &gb(1)));
    assert_eq!(cx.update(|cx| w.page.read(cx).playing(cx)), None);
    assert_eq!(w.audio.calls().last(), Some(&AudioCall::Stop));
    assert_eq!(w.audio.plays().len(), 1, "a stop fetches and plays nothing");
}

#[gpui_kit::test]
fn a_failed_preview_toasts_and_clears_the_player(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    let missing = format!("{}/preview/1.mp3", server.uri());
    let w = support::sound_store_window(cx, &env, &server, |env| {
        support::seed_sounds(env, &[(1, "Ding", "hitsound", Some(missing.as_str()))]);
    });
    support::click_id(cx, w.win, ids::store_play(S, &gb(1)));
    support::wait_real(cx, "the error toast", |cx| {
        support::present(cx, w.win, ids::TOAST)
    });
    let toast = support::label_of(cx, w.win, ids::TOAST).unwrap();
    assert!(toast.starts_with("Could not play preview."), "{toast}");
    assert_eq!(cx.update(|cx| w.page.read(cx).playing(cx)), None);
    assert!(w.audio.plays().is_empty());
}

// The card's play button names what a click does: Stop while its preview plays, Play again
// after a stop and after a preview that fails.
#[gpui_kit::test]
fn the_card_play_button_reads_stop_while_playing_and_play_after(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    let env = support::env();
    let server = support::StoreServer::start();
    let url = server.mount_preview(1, vec![4, 5, 6]);
    let missing = format!("{}/preview/2.mp3", server.uri());
    let w = support::sound_store_window(cx, &env, &server, |env| {
        support::seed_sounds(
            env,
            &[
                (1, "Ding", "hitsound", Some(url.as_str())),
                (2, "Boom", "killsound", Some(missing.as_str())),
            ],
        );
    });
    let win = w.win;
    let label = move |cx: &mut TestAppContext, id: u64| {
        support::label_of(cx, win, ids::store_play(S, &gb(id)))
    };
    assert_eq!(label(cx, 1).as_deref(), Some("Play"));
    support::click_id(cx, win, ids::store_play(S, &gb(1)));
    support::wait_real(cx, "the preview", |_| w.audio.plays().len() == 1);
    assert_eq!(label(cx, 1).as_deref(), Some("Stop"));
    assert_eq!(
        label(cx, 2).as_deref(),
        Some("Play"),
        "only the playing card flips"
    );
    support::click_id(cx, win, ids::store_play(S, &gb(1)));
    assert_eq!(cx.update(|cx| w.page.read(cx).playing(cx)), None);
    assert_eq!(label(cx, 1).as_deref(), Some("Play"), "after a stop");
    support::click_id(cx, win, ids::store_play(S, &gb(2)));
    support::wait_real(cx, "the error toast", |cx| {
        support::present(cx, win, ids::TOAST)
    });
    assert_eq!(cx.update(|cx| w.page.read(cx).playing(cx)), None);
    assert_eq!(
        label(cx, 2).as_deref(),
        Some("Play"),
        "after a failed preview"
    );
}

#[gpui_kit::test]
fn the_preview_buttons_have_v5_s_sizes(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    let url = server.mount_preview(1, vec![4, 5, 6]);
    let w = support::sound_store_window(cx, &env, &server, |env| {
        support::seed_sounds(env, &[(1, "Ding", "hitsound", Some(url.as_str()))]);
    });
    let card = support::bounds_of(cx, w.win, ids::store_card(S, &gb(1))).unwrap();
    let play = support::bounds_of(cx, w.win, ids::store_play(S, &gb(1))).unwrap();
    // v5's size="icon" ghost button (size-9), at the card's right beside name and author.
    assert_eq!(
        (play.size.width, play.size.height),
        (gpui_kit::px(36.), gpui_kit::px(36.))
    );
    assert!(play.origin.x > card.origin.x + card.size.width / 2.);
    support::click_id(cx, w.win, ids::store_card(S, &gb(1)));
    support::wait_real(cx, "the detail's preview button", |cx| {
        support::present(cx, w.win, ids::STORE_PREVIEW)
    });
    let preview = support::bounds_of(cx, w.win, ids::STORE_PREVIEW).unwrap();
    assert_eq!(preview.size.height, gpui_kit::px(32.));
    assert!(
        preview.size.width < gpui_kit::px(200.),
        "{:?}",
        preview.size.width
    );
}

#[gpui_kit::test]
fn a_sound_with_an_empty_preview_url_has_no_play_button(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    let w = support::sound_store_window(cx, &env, &server, |env| {
        support::seed_sounds(env, &[(3, "Quiet", "hitsound", Some(""))]);
    });
    assert!(support::present(cx, w.win, ids::store_card(S, &gb(3))));
    assert!(!support::present(cx, w.win, ids::store_play(S, &gb(3))));
}

#[gpui_kit::test]
fn a_volume_change_does_not_redraw_the_cards(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    let w = support::sound_store_window(cx, &env, &server, |env| {
        support::seed_sounds(env, &[(1, "Ding", "hitsound", None)]);
    });
    let grid = cx.update(|cx| w.page.read(cx).grid().clone());
    let redraws = Rc::new(Cell::new(0));
    let seen = redraws.clone();
    let _sub = cx.update(|cx| cx.observe(&grid, move |_, _| seen.set(seen.get() + 1)));
    let slider = cx.update(|cx| w.page.read(cx).player().read(cx).volume().clone());
    for value in [30., 40., 50.] {
        cx.update_window(w.win, |_, window, cx| {
            slider.update(cx, |state, cx| state.set_value(value, window, cx))
        })
        .unwrap();
        support::settle(cx, w.win);
    }
    assert_eq!(redraws.get(), 0);
}

/// Twenty playable sounds, so the grid's last cards sit under the floating player.
fn seed_many(env: &support::Env, url: &str) {
    let names: Vec<String> = (1..=20).map(|n| format!("Sound {n}")).collect();
    let rows: Vec<(i64, &str, &str, Option<&str>)> = names
        .iter()
        .zip(1..)
        .map(|(name, id)| (id, name.as_str(), "hitsound", Some(url)))
        .collect();
    support::seed_sounds(env, &rows);
}

/// The first of the twenty sounds whose card the grid draws.
fn shown(cx: &mut TestAppContext, w: &support::SoundStoreWindow) -> u64 {
    (1..=20)
        .find(|&n| support::present(cx, w.win, ids::store_play(S, &gb(n))))
        .expect("a drawn card")
}

#[gpui_kit::test]
fn the_player_s_stop_over_a_card_only_stops(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    let url = server.mount_preview(1, vec![4, 5, 6]);
    let w = support::sound_store_window(cx, &env, &server, |env| seed_many(env, &url));
    let n = shown(cx, &w);
    support::click_id(cx, w.win, ids::store_play(S, &gb(n)));
    support::wait_real(cx, "the preview", |_| w.audio.plays().len() == 1);
    let stop = support::bounds_of(cx, w.win, ids::SOUND_PLAYER_STOP).expect("the stop button");
    assert!(
        (1..=20).any(
            |n| support::bounds_of(cx, w.win, ids::store_card(S, &gb(n)))
                .is_some_and(|card| card.contains(&stop.center()))
        ),
        "a card must sit under the player for this test to mean anything"
    );
    support::click_id(cx, w.win, ids::SOUND_PLAYER_STOP);
    assert_eq!(cx.update(|cx| w.page.read(cx).playing(cx)), None);
    assert!(
        !support::present(cx, w.win, ids::STORE_DETAIL),
        "the click stops at the player and never opens the card under it"
    );
}

#[gpui_kit::test]
fn the_player_stays_usable_over_an_open_detail(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    let url = server.mount_preview(1, vec![4, 5, 6]);
    let w = support::sound_store_window(cx, &env, &server, |env| seed_many(env, &url));
    let n = shown(cx, &w);
    support::click_id(cx, w.win, ids::store_card(S, &gb(n)));
    assert!(support::present(cx, w.win, ids::STORE_DETAIL));
    support::click_id(cx, w.win, ids::STORE_PREVIEW);
    support::wait_real(cx, "the preview", |_| w.audio.plays().len() == 1);
    support::click_id(cx, w.win, ids::SOUND_PLAYER_STOP);
    assert_eq!(
        cx.update(|cx| w.page.read(cx).playing(cx)),
        None,
        "the player draws above the dialog's backdrop, so its stop is reachable"
    );
    assert!(
        support::present(cx, w.win, ids::STORE_DETAIL),
        "using the player leaves the dialog open"
    );
}
