//! TF2's spectator free-fly (hltvcamera.cpp CalcRoamingView): speeds, frame-rate independence,
//! stopping and the clamps.

use tf2tools_core::{FlyInput, FreeFly};

fn fly(camera: &mut FreeFly, seconds: f32, fps: f32, input: FlyInput) {
    for _ in 0..(seconds * fps).round() as usize {
        camera.advance(1.0 / fps, input);
    }
}

fn forward() -> FlyInput {
    FlyInput::new(1.0, 0.0, false)
}

fn distance(a: [f32; 3], b: [f32; 3]) -> f32 {
    (0..3).map(|i| (a[i] - b[i]).powi(2)).sum::<f32>().sqrt()
}

#[test]
fn full_speed_settles_at_944_units_a_second() {
    let mut camera = FreeFly::new([0.; 3], 0., 0.);
    fly(&mut camera, 2.0, 60., forward());
    assert!((camera.speed() - 944.).abs() < 1., "{}", camera.speed());
}

#[test]
fn a_diagonal_clamps_to_the_same_speed() {
    let mut camera = FreeFly::new([0.; 3], 0., 0.);
    fly(&mut camera, 2.0, 60., FlyInput::new(1.0, 1.0, false));
    assert!((camera.speed() - 944.).abs() < 1., "{}", camera.speed());
}

#[test]
fn shift_settles_at_664() {
    let mut camera = FreeFly::new([0.; 3], 0., 0.);
    fly(&mut camera, 2.0, 60., FlyInput::new(1.0, 0.0, true));
    assert!((camera.speed() - 663.75).abs() < 1., "{}", camera.speed());
}

#[test]
fn the_path_is_the_same_at_30_and_144_fps() {
    let (mut slow, mut fast) = (FreeFly::new([0.; 3], 0., 0.), FreeFly::new([0.; 3], 0., 0.));
    fly(&mut slow, 1.0, 30., forward());
    fly(&mut fast, 1.0, 144., forward());
    assert!(slow.origin()[0] > 700., "{:?}", slow.origin());
    // One 1/240 s substep at the 960 cap is 4 units.
    let gap = distance(slow.origin(), fast.origin());
    assert!(gap <= 4.01, "{gap}");
}

#[test]
fn letting_go_stops_within_a_second() {
    let mut camera = FreeFly::new([0.; 3], 0., 0.);
    fly(&mut camera, 2.0, 60., forward());
    fly(&mut camera, 1.0, 60., FlyInput::default());
    assert_eq!(camera.speed(), 0.);
    assert!(!camera.is_moving());
    let rest = camera.origin();
    fly(&mut camera, 0.5, 60., FlyInput::default());
    assert_eq!(camera.origin(), rest);
}

#[test]
fn yaw_turns_left_pitch_looks_down_and_side_moves_right() {
    let mut left = FreeFly::new([0.; 3], 90., 0.);
    fly(&mut left, 1.0, 60., forward());
    assert!(
        left.origin()[0].abs() < 0.1 && left.origin()[1] > 700.,
        "{:?}",
        left.origin()
    );
    let mut down = FreeFly::new([0.; 3], 0., 89.);
    fly(&mut down, 1.0, 60., forward());
    assert!(down.origin()[2] < -700., "{:?}", down.origin());
    let mut right = FreeFly::new([0.; 3], 0., 0.);
    fly(&mut right, 1.0, 60., FlyInput::new(0.0, 1.0, false));
    assert!(right.origin()[1] < -700., "{:?}", right.origin());
}

#[test]
fn look_adds_degrees_and_clamps_pitch() {
    let mut camera = FreeFly::new([0.; 3], 10., 0.);
    camera.look(5., 100.);
    assert_eq!((camera.yaw(), camera.pitch()), (15., 89.));
    camera.look(0., -500.);
    assert_eq!(camera.pitch(), -89.);
    assert_eq!(FreeFly::new([0.; 3], 0., 120.).pitch(), 89.);
}

#[test]
fn a_long_frame_counts_as_a_quarter_second() {
    let (mut long, mut quarter) = (FreeFly::new([0.; 3], 0., 0.), FreeFly::new([0.; 3], 0., 0.));
    long.advance(10.0, forward());
    quarter.advance(0.25, forward());
    assert_eq!(long, quarter);
}

#[test]
fn moving_means_at_least_one_unit_a_second() {
    let mut camera = FreeFly::new([0.; 3], 0., 0.);
    assert!(!camera.is_moving());
    camera.advance(1.0 / 60.0, forward());
    assert!(camera.is_moving());
}

#[test]
fn a_negative_frame_does_not_drain_the_step_accumulator() {
    let mut camera = FreeFly::new([0.; 3], 0., 0.);
    camera.advance(-1.0, forward());
    camera.advance(1.0 / 240.0 + 1e-5, forward());
    assert!(camera.origin()[0] > 0.0, "{:?}", camera.origin());
}

#[test]
fn a_nan_frame_does_not_move_the_camera() {
    let mut camera = FreeFly::new([0.; 3], 0., 0.);
    camera.advance(f32::NAN, forward());
    assert_eq!(camera.origin(), [0.; 3]);
}
