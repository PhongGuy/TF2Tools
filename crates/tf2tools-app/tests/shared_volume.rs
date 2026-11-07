//! One saved volume: the sound player and Settings draw it, change it and save it together.

use crate::support;

use std::sync::Arc;

use gpui_kit::component::slider::{SliderEvent, SliderState, SliderValue};
use gpui_kit::{
    AnyWindowHandle, AppContext as _, Entity, InputEvent as _, Modifiers, MouseButton,
    MouseDownEvent, MouseUpEvent, TestAppContext, point,
};
use tf2tools_app::audio::player::{NowPlaying, Player};
use tf2tools_app::audio::{AudioCall, RecordingAudio};
use tf2tools_app::ids;
use tf2tools_app::pages::settings::SettingsPage;
use tf2tools_app::services::{Services, Tf2State};
use tf2tools_app::shell::route::Page;
use tf2tools_core::{Tf2Install, Volume};

fn shell(cx: &mut TestAppContext, env: &support::Env) -> support::ShellWindow {
    let found = Tf2State::Found(Tf2Install::from_path(&env.game).unwrap());
    support::shell_window(cx, env, found)
}

fn saved(services: &Services) -> u8 {
    services.db.settings().unwrap().volume().get()
}

/// Moves `slider` the way a pointer does, without letting go.
fn slide(
    cx: &mut TestAppContext,
    win: AnyWindowHandle,
    slider: &Entity<SliderState>,
    percent: f32,
) {
    cx.update_window(win, |_, window, cx| {
        slider.update(cx, |state, cx| state.set_value(percent, window, cx))
    })
    .unwrap();
    support::settle(cx, win);
}

/// Sends the end of a drag at `percent`.
fn release(
    cx: &mut TestAppContext,
    win: AnyWindowHandle,
    slider: &Entity<SliderState>,
    percent: f32,
) {
    cx.update_window(win, |_, window, cx| {
        slider.update(cx, |state, cx| {
            state.set_value(percent, window, cx);
            cx.emit(SliderEvent::Release(SliderValue::Single(percent)));
        })
    })
    .unwrap();
}

fn open_settings(cx: &mut TestAppContext, s: &support::ShellWindow) -> Entity<SettingsPage> {
    support::click_id(cx, s.win, ids::nav(Page::Settings));
    let page = cx.update(|cx| s.shell.read(cx).settings_page()).unwrap();
    support::wait_real(cx, "settings to load", |cx| {
        cx.update(|cx| page.read(cx).is_loaded())
    });
    support::settle(cx, s.win);
    page
}

fn open_sounds_player(cx: &mut TestAppContext, s: &support::ShellWindow) -> Entity<Player> {
    support::click_id(cx, s.win, ids::nav(Page::Sounds));
    let page = cx.update(|cx| s.shell.read(cx).sounds_page()).unwrap();
    let player = cx.update(|cx| page.read(cx).player().clone());
    support::wait_real(cx, "the player", |cx| {
        cx.update(|cx| player.read(cx).is_ready())
    });
    support::settle(cx, s.win);
    player
}

/// A floating player beside the open page's own: both are alive at once.
fn floating_player(cx: &mut TestAppContext, win: AnyWindowHandle) -> Entity<Player> {
    let player = cx
        .update_window(win, |_, window, cx| cx.new(|cx| Player::new(window, cx)))
        .unwrap();
    support::wait_real(cx, "the player", |cx| {
        cx.update(|cx| player.read(cx).is_ready())
    });
    support::settle(cx, win);
    player
}

fn player_slider(cx: &mut TestAppContext, player: &Entity<Player>) -> Entity<SliderState> {
    cx.update(|cx| player.read(cx).volume().clone())
}

fn settings_slider(cx: &mut TestAppContext, page: &Entity<SettingsPage>) -> Entity<SliderState> {
    cx.update(|cx| page.read(cx).volume().clone())
}

fn value_of(cx: &mut TestAppContext, slider: &Entity<SliderState>) -> f32 {
    cx.update(|cx| slider.read(cx).value().start())
}

/// A window whose root is a Player, over a library saved at `volume` percent.
fn player_window(
    cx: &mut TestAppContext,
    env: &support::Env,
    volume: u8,
) -> (
    AnyWindowHandle,
    Entity<Player>,
    Services,
    Arc<RecordingAudio>,
) {
    support::init(cx);
    let audio = Arc::new(RecordingAudio::new());
    let services = support::services_with_audio(env, Arc::clone(&audio));
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
    (win, player, services, audio)
}

#[gpui_kit::test]
fn leaving_sounds_and_coming_back_keeps_the_volume(cx: &mut TestAppContext) {
    let env = support::env();
    let s = shell(cx, &env);
    let player = open_sounds_player(cx, &s);
    let slider = player_slider(cx, &player);
    release(cx, s.win, &slider, 13.);
    support::wait_real(cx, "the saved volume", |_| saved(&s.services) == 13);

    open_settings(cx, &s);
    assert_eq!(
        support::label_of(cx, s.win, ids::SETTINGS_VOLUME_LABEL).as_deref(),
        Some("Volume: 13%")
    );

    let back = open_sounds_player(cx, &s);
    assert_eq!(
        support::label_of(cx, s.win, ids::SOUND_PLAYER_VOLUME_VALUE).as_deref(),
        Some("13")
    );
    let slider = player_slider(cx, &back);
    assert_eq!(value_of(cx, &slider), 13.);
}

#[gpui_kit::test]
fn a_change_in_settings_reaches_the_player_and_the_sound(cx: &mut TestAppContext) {
    let env = support::env();
    let s = shell(cx, &env);
    let page = open_settings(cx, &s);
    let player = floating_player(cx, s.win);
    let (mine, theirs) = (settings_slider(cx, &page), player_slider(cx, &player));

    slide(cx, s.win, &mine, 40.);

    assert_eq!(value_of(cx, &theirs), 40.);
    assert_eq!(s.audio.calls().last(), Some(&AudioCall::SetVolume(0.4)));
    release(cx, s.win, &mine, 40.);
    support::wait_real(cx, "the saved volume", |_| saved(&s.services) == 40);
}

#[gpui_kit::test]
fn a_change_in_the_player_shows_in_settings_at_once(cx: &mut TestAppContext) {
    let env = support::env();
    let s = shell(cx, &env);
    let page = open_settings(cx, &s);
    let player = floating_player(cx, s.win);
    let (mine, theirs) = (player_slider(cx, &player), settings_slider(cx, &page));

    slide(cx, s.win, &mine, 13.);

    assert_eq!(value_of(cx, &theirs), 13.);
    assert_eq!(
        support::label_of(cx, s.win, ids::SETTINGS_VOLUME_LABEL).as_deref(),
        Some("Volume: 13%")
    );
}

#[gpui_kit::test]
fn a_drag_saves_when_the_pointer_lets_go(cx: &mut TestAppContext) {
    let env = support::env();
    let (win, player, services, _audio) = player_window(cx, &env, 70);
    support::wait_real(cx, "the saved volume", |cx| {
        cx.update(|cx| player.read(cx).is_ready())
    });
    support::settle(cx, win);
    let track = support::bounds_of(cx, win, ids::SOUND_PLAYER_VOLUME).unwrap();
    let at = point(
        track.left() + track.size.width * 0.25,
        support::mid_y(track),
    );

    cx.update_window(win, |_, window, cx| {
        let down = MouseDownEvent {
            button: MouseButton::Left,
            position: at,
            modifiers: Modifiers::default(),
            click_count: 1,
            first_mouse: false,
        };
        let _ = window.dispatch_event(down.to_platform_input(), cx);
    })
    .unwrap();
    support::settle(cx, win);
    let dragged = cx.update(|cx| player.read(cx).volume().read(cx).value().start());
    assert!(dragged < 70., "the press moves the slider");
    assert_eq!(saved(&services), 70, "nothing is saved during the drag");

    cx.update_window(win, |_, window, cx| {
        let up = MouseUpEvent {
            button: MouseButton::Left,
            position: at,
            modifiers: Modifiers::default(),
            click_count: 1,
        };
        let _ = window.dispatch_event(up.to_platform_input(), cx);
    })
    .unwrap();
    support::wait_real(cx, "the saved volume", |_| {
        f32::from(saved(&services)) == dragged
    });
}

#[gpui_kit::test]
fn quick_releases_end_on_the_last_value(cx: &mut TestAppContext) {
    let env = support::env();
    let (win, player, services, _audio) = player_window(cx, &env, 70);
    support::wait_real(cx, "the saved volume", |cx| {
        cx.update(|cx| player.read(cx).is_ready())
    });
    let slider = player_slider(cx, &player);
    for percent in [20., 30., 40.] {
        release(cx, win, &slider, percent);
    }
    support::wait_real(cx, "the last volume", |_| saved(&services) == 40);
    // A stale write landing late would show here, once the writer has gone quiet.
    support::settle(cx, win);
    assert_eq!(saved(&services), 40);
}

#[gpui_kit::test]
fn only_one_write_runs_at_a_time(cx: &mut TestAppContext) {
    let env = support::env();
    let (win, player, services, _audio) = player_window(cx, &env, 70);
    support::wait_real(cx, "the saved volume", |cx| {
        cx.update(|cx| player.read(cx).is_ready())
    });
    // A trigger keeps the order of every volume write; the first takes 20 and the rest fold into
    // one write of the newest value.
    let connection = rusqlite::Connection::open(env.data.db_path()).unwrap();
    connection
        .execute_batch(
            "CREATE TABLE volume_writes (id INTEGER PRIMARY KEY, value TEXT);
             CREATE TRIGGER log_volume AFTER INSERT ON settings WHEN NEW.key = 'volume'
             BEGIN INSERT INTO volume_writes (value) VALUES (NEW.value); END;",
        )
        .unwrap();
    let slider = player_slider(cx, &player);
    for percent in [20., 30., 40.] {
        release(cx, win, &slider, percent);
    }
    support::wait_real(cx, "the last volume", |_| saved(&services) == 40);
    support::settle(cx, win);
    let mut writes = connection
        .prepare("SELECT value FROM volume_writes ORDER BY id")
        .unwrap();
    let writes: Vec<String> = writes
        .query_map([], |row| row.get(0))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    assert_eq!(writes, ["20", "40"]);
}

#[gpui_kit::test(iterations = 50)]
fn settings_save_never_overwrites_a_newer_volume(cx: &mut TestAppContext) {
    let env = support::env();
    let s = shell(cx, &env);
    let page = open_settings(cx, &s);
    let slider = settings_slider(cx, &page);
    slide(cx, s.win, &slider, 60.);

    cx.update_window(s.win, |_, window, cx| {
        page.update(cx, |page, cx| page.save(window, cx))
    })
    .unwrap();
    release(cx, s.win, &slider, 30.);

    support::wait_real(cx, "the Save toast", |cx| {
        support::present(cx, s.win, ids::TOAST)
    });
    support::settle(cx, s.win);
    assert_eq!(value_of(cx, &slider), 30.);
    assert_eq!(saved(&s.services), 30);
}

#[gpui_kit::test]
fn a_change_before_the_saved_volume_arrives_is_kept(cx: &mut TestAppContext) {
    let env = support::env();
    let (win, player, _services, _audio) = player_window(cx, &env, 70);
    let slider = player_slider(cx, &player);
    slide(cx, win, &slider, 30.);
    support::wait_real(cx, "the saved volume", |cx| {
        cx.update(|cx| player.read(cx).is_ready())
    });
    support::settle(cx, win);
    assert_eq!(
        support::label_of(cx, win, ids::SOUND_PLAYER_VOLUME_VALUE).as_deref(),
        Some("30")
    );
}

#[gpui_kit::test]
fn the_next_play_uses_the_settings_volume(cx: &mut TestAppContext) {
    let env = support::env();
    let s = shell(cx, &env);
    let page = open_settings(cx, &s);
    let player = floating_player(cx, s.win);
    let slider = settings_slider(cx, &page);
    slide(cx, s.win, &slider, 25.);
    let now = NowPlaying {
        key: "Ding".into(),
        name: "Ding".into(),
        subtitle: "ding.wav".into(),
    };
    cx.update(|cx| player.update(cx, |p, cx| p.toggle(now, |_| Ok(vec![1]), cx).detach()));
    support::wait_real(cx, "the play", |_| s.audio.plays().len() == 1);
    assert_eq!(s.audio.plays()[0].1, 0.25);
}

#[gpui_kit::test]
fn a_failed_save_tells_the_player_and_keeps_the_volume(cx: &mut TestAppContext) {
    let env = support::env();
    let s = shell(cx, &env);
    let player = open_sounds_player(cx, &s);
    let connection = rusqlite::Connection::open(env.data.db_path()).unwrap();
    connection
        .execute_batch(
            "CREATE TRIGGER refuse_insert BEFORE INSERT ON settings BEGIN SELECT RAISE(ABORT, 'read-only'); END;
             CREATE TRIGGER refuse_update BEFORE UPDATE ON settings BEGIN SELECT RAISE(ABORT, 'read-only'); END;",
        )
        .unwrap();
    let slider = player_slider(cx, &player);
    release(cx, s.win, &slider, 13.);
    support::wait_real(cx, "the error toast", |cx| {
        support::present(cx, s.win, ids::TOAST)
    });
    assert_eq!(
        support::label_of(cx, s.win, ids::TOAST).as_deref(),
        Some(
            "Failed to save. TF2Tools couldn't read or save its settings. Restart TF2Tools and try again."
        )
    );
    assert_eq!(
        support::label_of(cx, s.win, ids::SOUND_PLAYER_VOLUME_VALUE).as_deref(),
        Some("13")
    );
}
