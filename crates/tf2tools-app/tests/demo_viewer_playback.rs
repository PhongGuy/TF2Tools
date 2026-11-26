//! The Demo Viewer's playback stream: the still picture first, reading progress, the tracks,
//! a failed read, and a read cancelled by leaving the page.

use std::sync::atomic::Ordering;

use gpui_kit::TestAppContext;
use tf2tools_app::ids;
use tf2tools_app::pages::demo_viewer::ReadState;
use tf2tools_app::shell::route::Page;
use tf2tools_core::{DemoTracksError, PlaybackEvent};

use crate::support;
use crate::support::demo_viewer::{
    gated_playback, open_scripted, playback, read_state, still_then, walking, walking_tracks,
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
