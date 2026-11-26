//! The Demo Viewer's playback stream: the still picture first, reading progress, the tracks,
//! a failed read, and a read cancelled by leaving the page.

use std::sync::atomic::Ordering;

use gpui_kit::component::slider::{SliderEvent, SliderValue};
use gpui_kit::{AppContext as _, TestAppContext};
use tf2tools_app::ids;
use tf2tools_app::pages::demo_viewer::ReadState;
use tf2tools_app::shell::route::Page;
use tf2tools_app::viewer::load::timeline_time;
use tf2tools_core::{DemoTracksError, PlaybackEvent, TrackCut};

use crate::support;
use crate::support::demo_viewer::{
    gated_playback, open_scripted, open_with_tracks, playback, read_state, still_then, walking,
    walking_tracks,
};

#[gpui_kit::test]
fn the_still_picture_shows_before_the_tracks_and_they_start_at_its_tick(cx: &mut TestAppContext) {
    let env = support::env();
    let (reader, gate) = still_then(|| vec![PlaybackEvent::Tracks(walking_tracks(None))]);
    let w = open_scripted(cx, &env, reader);
    assert_eq!(
        support::label_of(cx, w.s.win, ids::DEMO_VIEWER_CAPTION).as_deref(),
        Some("cp_synthetic, 3 players at 1:02")
    );
    assert!(playback(cx, &w).is_none());
    assert_eq!(
        read_state(cx, &w),
        Some(ReadState::Reading { percent: None })
    );
    gate.open();
    support::wait_real(cx, "the tracks", |cx| playback(cx, &w).is_some());
    let state = playback(cx, &w).unwrap();
    assert_eq!(state.tick, 4100.);
    assert!(!state.playing);
    assert_eq!(read_state(cx, &w), Some(ReadState::Ready));
}

#[gpui_kit::test]
fn progress_reads_as_a_whole_percent_until_the_tracks_arrive(cx: &mut TestAppContext) {
    let env = support::env();
    let (reader, gate) = gated_playback(
        || {
            vec![
                PlaybackEvent::Snapshot(Ok(walking(4100, 100.))),
                PlaybackEvent::Progress(0.409),
            ]
        },
        || vec![PlaybackEvent::Tracks(walking_tracks(None))],
    );
    let w = open_scripted(cx, &env, reader);
    support::wait_real(cx, "40 percent", |cx| {
        read_state(cx, &w) == Some(ReadState::Reading { percent: Some(40) })
    });
    gate.open();
    support::wait_real(cx, "ready", |cx| {
        read_state(cx, &w) == Some(ReadState::Ready)
    });
}

#[gpui_kit::test]
fn failed_tracks_keep_the_still_picture(cx: &mut TestAppContext) {
    let env = support::env();
    let (reader, gate) =
        still_then(|| vec![PlaybackEvent::TracksFailed(DemoTracksError::NoPlayers)]);
    let w = open_scripted(cx, &env, reader);
    gate.open();
    support::wait_real(cx, "failed", |cx| {
        read_state(cx, &w) == Some(ReadState::Failed)
    });
    assert!(support::present(cx, w.s.win, ids::DEMO_VIEWER_VIEWPORT));
    assert!(playback(cx, &w).is_none());
    let boxes = w.stubs.lock().unwrap()[0].players();
    assert_eq!(boxes.last().map(|b| b.len()), Some(3));
}

#[gpui_kit::test]
fn a_read_that_ends_without_a_word_shows_the_map_without_players(cx: &mut TestAppContext) {
    let env = support::env();
    let (reader, gate) = gated_playback(Vec::new, Vec::new);
    gate.open();
    let w = open_scripted(cx, &env, reader);
    support::wait_real(cx, "failed", |cx| {
        read_state(cx, &w) == Some(ReadState::Failed)
    });
    assert_eq!(
        support::label_of(cx, w.s.win, ids::DEMO_VIEWER_PLAYERS_NOTE).as_deref(),
        Some("Players couldn't be read from this demo.")
    );
}

#[gpui_kit::test]
fn leaving_the_page_cancels_the_read_and_closes_its_channel(cx: &mut TestAppContext) {
    let env = support::env();
    let (reader, gate) = still_then(|| vec![PlaybackEvent::Tracks(walking_tracks(None))]);
    let w = open_scripted(cx, &env, reader);
    support::click_nav(cx, w.s.win, Page::Huds);
    support::settle(cx, w.s.win);
    support::wait_real(cx, "the cancel", |_| {
        gate.cancelled.load(Ordering::SeqCst) && gate.broke.load(Ordering::SeqCst)
    });
}

#[test]
fn timeline_time_floors_both_sides() {
    assert_eq!(timeline_time(0., 0.), "0:00 / 0:00");
    assert_eq!(timeline_time(62.9, 3600.), "1:02 / 60:00");
}

#[gpui_kit::test]
fn play_pauses_and_steps_drive_the_viewport(cx: &mut TestAppContext) {
    let env = support::env();
    let w = open_with_tracks(cx, &env, None);
    // 100 ticks in at 60/3960 s a tick is 1.51 s of a 3.03 s range.
    assert_eq!(
        support::label_of(cx, w.s.win, ids::DEMO_VIEWER_TIME).as_deref(),
        Some("0:01 / 0:03")
    );
    support::click_id(cx, w.s.win, ids::DEMO_VIEWER_STEP_FORWARD);
    assert_eq!(playback(cx, &w).unwrap().tick, 4101.);
    support::click_id(cx, w.s.win, ids::DEMO_VIEWER_STEP_BACK);
    support::click_id(cx, w.s.win, ids::DEMO_VIEWER_STEP_BACK);
    assert_eq!(playback(cx, &w).unwrap().tick, 4099.);
    support::click_id(cx, w.s.win, ids::DEMO_VIEWER_PLAY);
    assert!(playback(cx, &w).unwrap().playing);
    support::click_id(cx, w.s.win, ids::DEMO_VIEWER_PLAY);
    assert!(!playback(cx, &w).unwrap().playing);
}

#[gpui_kit::test]
fn the_speed_button_cycles_through_five_speeds(cx: &mut TestAppContext) {
    let env = support::env();
    let w = open_with_tracks(cx, &env, None);
    let label = |cx: &mut TestAppContext| support::label_of(cx, w.s.win, ids::DEMO_VIEWER_SPEED);
    assert_eq!(label(cx).as_deref(), Some("1×"));
    let mut seen = Vec::new();
    for _ in 0..5 {
        support::click_id(cx, w.s.win, ids::DEMO_VIEWER_SPEED);
        seen.push((
            label(cx).unwrap_or_default(),
            playback(cx, &w).unwrap().speed,
        ));
    }
    let expected = [
        ("2×", 2.),
        ("4×", 4.),
        ("0.25×", 0.25),
        ("0.5×", 0.5),
        ("1×", 1.),
    ];
    let got: Vec<(&str, f32)> = seen.iter().map(|(l, s)| (l.as_ref(), *s)).collect();
    assert_eq!(got, expected);
}

#[gpui_kit::test]
fn a_round_marker_seeks_to_its_round(cx: &mut TestAppContext) {
    let env = support::env();
    let w = open_with_tracks(cx, &env, None);
    assert!(support::present(
        cx,
        w.s.win,
        ids::demo_viewer_round_marker(0)
    ));
    assert!(!support::present(
        cx,
        w.s.win,
        ids::demo_viewer_round_marker(2)
    ));
    support::click_id(cx, w.s.win, ids::demo_viewer_round_marker(1));
    assert_eq!(playback(cx, &w).unwrap().tick, 4150.);
}

#[gpui_kit::test]
fn moving_the_timeline_seeks_the_viewport(cx: &mut TestAppContext) {
    let env = support::env();
    let w = open_with_tracks(cx, &env, None);
    let slider = cx.update(|cx| {
        let page =
            w.s.shell
                .read(cx)
                .demo_viewer_page(cx)
                .expect("the Demo Viewer");
        page.read(cx)
            .timeline_slider()
            .cloned()
            .expect("the tracks' slider")
    });
    // A drag on the slider ends in the same Change event this sends.
    cx.update_window(w.s.win, |_, window, cx| {
        slider.update(cx, |st, cx| {
            st.set_value(4050., window, cx);
            cx.emit(SliderEvent::Change(SliderValue::Single(4050.)));
        })
    })
    .unwrap();
    support::settle(cx, w.s.win);
    assert_eq!(playback(cx, &w).unwrap().tick, 4050.);
}

#[gpui_kit::test]
fn the_reading_note_shows_until_the_tracks_and_the_controls_wait(cx: &mut TestAppContext) {
    let env = support::env();
    let (reader, gate) = gated_playback(
        || {
            vec![
                PlaybackEvent::Snapshot(Ok(walking(4100, 100.))),
                PlaybackEvent::Progress(0.4),
            ]
        },
        || vec![PlaybackEvent::Tracks(walking_tracks(None))],
    );
    let w = open_scripted(cx, &env, reader);
    support::wait_real(cx, "the note", |cx| {
        support::label_of(cx, w.s.win, ids::DEMO_VIEWER_READING).as_deref()
            == Some("Reading demo 40%")
    });
    support::click_id(cx, w.s.win, ids::DEMO_VIEWER_PLAY);
    assert!(playback(cx, &w).is_none());
    gate.open();
    support::wait_real(cx, "the tracks", |cx| playback(cx, &w).is_some());
    support::settle(cx, w.s.win);
    assert!(!support::present(cx, w.s.win, ids::DEMO_VIEWER_READING));
}

#[gpui_kit::test]
fn a_cut_says_where_playback_ends(cx: &mut TestAppContext) {
    let env = support::env();
    let w = open_with_tracks(cx, &env, Some(TrackCut::EndedEarly { at_tick: 4200 }));
    // 4200 ticks at 60/3960 s is 63.6 s.
    assert_eq!(
        support::label_of(cx, w.s.win, ids::DEMO_VIEWER_PLAYBACK_NOTE).as_deref(),
        Some("This demo ends early, at 1:03.")
    );
}

#[gpui_kit::test]
fn failed_tracks_hide_the_timeline_and_say_why(cx: &mut TestAppContext) {
    let env = support::env();
    let (reader, gate) =
        still_then(|| vec![PlaybackEvent::TracksFailed(DemoTracksError::NoPlayers)]);
    gate.open();
    let w = open_scripted(cx, &env, reader);
    support::wait_real(cx, "the note", |cx| {
        support::label_of(cx, w.s.win, ids::DEMO_VIEWER_PLAYBACK_NOTE).as_deref()
            == Some("Players can't move in this demo.")
    });
    assert!(!support::present(cx, w.s.win, ids::DEMO_VIEWER_PLAY));
    assert!(!support::present(cx, w.s.win, ids::DEMO_VIEWER_TIMELINE));
}
