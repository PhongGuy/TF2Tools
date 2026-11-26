//! The demo playback clock: play, pause, speed, seek, step and its frame-time rules.

use tf2tools_core::PlaybackClock;

const SPT: f32 = 1. / 66.;

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-6
}

#[test]
fn it_starts_paused_at_the_first_tick_at_normal_speed() {
    let clock = PlaybackClock::new(100, 1000, SPT);
    assert!(!clock.is_playing());
    assert_eq!((clock.tick(), clock.speed()), (100., 1.));
}

#[test]
fn a_paused_clock_does_not_move() {
    let mut clock = PlaybackClock::new(0, 1000, SPT);
    assert!(!clock.advance(0.1));
    assert_eq!(clock.tick(), 0.);
}

#[test]
fn one_second_at_66_tick_moves_66_ticks() {
    let mut clock = PlaybackClock::new(0, 10_000, SPT);
    clock.play();
    for _ in 0..4 {
        assert!(clock.advance(0.25));
    }
    assert!((clock.tick() - 66.).abs() < 1e-3, "{}", clock.tick());
}

#[test]
fn a_long_frame_counts_as_a_quarter_second() {
    let mut clock = PlaybackClock::new(0, 10_000, SPT);
    clock.play();
    clock.advance(10.);
    assert!((clock.tick() - 16.5).abs() < 1e-3, "{}", clock.tick());
}

#[test]
fn negative_and_nan_frames_do_not_move() {
    let mut clock = PlaybackClock::new(0, 10_000, SPT);
    clock.play();
    assert!(!clock.advance(-1.));
    assert!(!clock.advance(f32::NAN));
    assert_eq!(clock.tick(), 0.);
}

#[test]
fn speed_scales_time_and_is_clamped() {
    let mut clock = PlaybackClock::new(0, 10_000, SPT);
    clock.play();
    clock.set_speed(2.);
    clock.advance(0.25);
    assert!((clock.tick() - 33.).abs() < 1e-3, "{}", clock.tick());
    clock.set_speed(10.);
    assert_eq!(clock.speed(), 4.);
    clock.set_speed(0.01);
    assert_eq!(clock.speed(), 0.25);
    clock.set_speed(f32::NAN);
    assert_eq!(clock.speed(), 0.25);
}

#[test]
fn reaching_the_last_tick_stops_and_pauses() {
    let mut clock = PlaybackClock::new(0, 10, SPT);
    clock.play();
    assert!(clock.advance(0.25));
    assert_eq!(clock.tick(), 10.);
    assert!(!clock.is_playing());
    assert!(!clock.advance(0.25));
}

#[test]
fn play_at_the_end_starts_over() {
    let mut clock = PlaybackClock::new(5, 10, SPT);
    clock.seek(10.);
    clock.play();
    assert!(clock.is_playing());
    assert_eq!(clock.tick(), 5.);
}

#[test]
fn step_pauses_and_lands_on_whole_ticks() {
    let mut clock = PlaybackClock::new(0, 100, SPT);
    clock.seek(5.6);
    clock.play();
    clock.step(1);
    assert!(!clock.is_playing());
    assert_eq!(clock.tick(), 6.);
    clock.step(-1);
    assert_eq!(clock.tick(), 5.);
    clock.step(-100);
    assert_eq!(clock.tick(), 0.);
    clock.step(1000);
    assert_eq!(clock.tick(), 100.);
}

#[test]
fn seek_clamps_keeps_the_play_state_and_ignores_nan() {
    let mut clock = PlaybackClock::new(10, 100, SPT);
    clock.play();
    clock.seek(50.5);
    assert!(clock.is_playing());
    assert_eq!(clock.tick(), 50.5);
    clock.seek(f64::NAN);
    assert_eq!(clock.tick(), 50.5);
    clock.seek(-4.);
    assert_eq!(clock.tick(), 10.);
    clock.seek(1e9);
    assert_eq!(clock.tick(), 100.);
    clock.pause();
    assert!(!clock.is_playing());
}

#[test]
fn elapsed_and_total_seconds_count_from_the_first_tick() {
    let mut clock = PlaybackClock::new(100, 760, SPT);
    clock.seek(166.);
    assert!(
        close(clock.elapsed_seconds(), 1.),
        "{}",
        clock.elapsed_seconds()
    );
    assert!(
        close(clock.total_seconds(), 10.),
        "{}",
        clock.total_seconds()
    );
}

#[test]
fn a_backwards_range_is_empty_and_a_bad_interval_is_66_tick() {
    let clock = PlaybackClock::new(50, 10, SPT);
    assert_eq!(clock.total_seconds(), 0.);
    let mut clock = PlaybackClock::new(0, 1000, f32::NAN);
    clock.play();
    clock.advance(0.25);
    assert!((clock.tick() - 16.5).abs() < 1e-3, "{}", clock.tick());
}
