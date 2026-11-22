//! The viewer's camera math and readback unpadding, with no GPU.

use crate::support::demo_view_fixtures::{BspOptions, bsp};
use tf2tools_app::viewer::math::{self, EYE_HEIGHT, FAR, NEAR};
use tf2tools_core::load_map_mesh;

// `p` through `m`, divided by w: [x, y, depth].
fn project(m: [[f32; 4]; 4], p: [f32; 3]) -> [f32; 3] {
    let v = [p[0], p[1], p[2], 1.0];
    let clip: Vec<f32> = (0..4)
        .map(|row| (0..4).map(|col| m[col][row] * v[col]).sum())
        .collect();
    [clip[0] / clip[3], clip[1] / clip[3], clip[2] / clip[3]]
}

fn on_screen_at(m: [[f32; 4]; 4], p: [f32; 3], want: [f32; 2]) {
    let got = project(m, p);
    assert!(
        (got[0] - want[0]).abs() < 1e-3 && (got[1] - want[1]).abs() < 1e-3,
        "{p:?} projected to {got:?}, want {want:?}"
    );
}

#[test]
fn a_point_straight_ahead_is_mid_screen_between_near_and_far() {
    let m = math::view_proj([0., 0., 0.], 0., 0., 16. / 9.);
    on_screen_at(m, [100., 0., 0.], [0., 0.]);
    let depth = project(m, [100., 0., 0.])[2];
    assert!(depth > 0. && depth < 1., "{depth}");
    assert!(project(m, [NEAR, 0., 0.])[2].abs() < 1e-4);
    assert!((project(m, [FAR, 0., 0.])[2] - 1.).abs() < 1e-4);
}

#[test]
fn the_vertical_fov_is_the_same_at_every_aspect() {
    for aspect in [4. / 3., 16. / 9., 1.] {
        let m = math::view_proj([0., 0., 0.], 0., 0., aspect);
        // tan(73.74 / 2) = 0.75: 75 up at 100 ahead touches the top edge.
        on_screen_at(m, [100., 0., 75.], [0., 1.]);
    }
}

#[test]
fn ninety_degrees_across_at_4_3_and_wider_at_16_9() {
    // -Y is right of a camera facing +X.
    on_screen_at(
        math::view_proj([0.; 3], 0., 0., 4. / 3.),
        [100., -100., 0.],
        [1., 0.],
    );
    let wide = 100. * 0.75 * 16. / 9.;
    on_screen_at(
        math::view_proj([0.; 3], 0., 0., 16. / 9.),
        [100., -wide, 0.],
        [1., 0.],
    );
}

#[test]
fn yaw_pitch_and_eye_follow_source_angles() {
    on_screen_at(
        math::view_proj([0.; 3], 90., 0., 1.),
        [0., 100., 0.],
        [0., 0.],
    );
    // Positive pitch looks down.
    on_screen_at(
        math::view_proj([0.; 3], 0., 45., 1.),
        [100., 0., -100.],
        [0., 0.],
    );
    on_screen_at(
        math::view_proj([10., 20., 30.], 0., 0., 1.),
        [110., 20., 30.],
        [0., 0.],
    );
}

#[test]
fn dragging_right_turns_right_and_dragging_down_looks_down() {
    assert_eq!(math::drag_look(100., 0.), (-22., 0.));
    assert_eq!(math::drag_look(0., 50.), (0., 11.));
}

#[test]
fn rows_pad_to_256_bytes_and_unpad_back() {
    assert_eq!(math::padded_row_bytes(3), 256);
    assert_eq!(math::padded_row_bytes(64), 256);
    assert_eq!(math::padded_row_bytes(65), 512);
    // Two rows of three BGRA pixels, each row padded with 0xEE to 256 bytes.
    let mut padded = vec![0xEE; 512];
    for row in 0..2 {
        for i in 0..12 {
            padded[row * 256 + i] = (row * 16 + i) as u8;
        }
    }
    let rows = math::unpad_rows(&padded, 3, 2);
    let want: Vec<u8> = (0..2)
        .flat_map(|row| (0..12).map(move |i| (row * 16 + i) as u8))
        .collect();
    assert_eq!(rows, want);
}

#[test]
fn the_camera_starts_at_the_spawn_at_eye_height() {
    let mesh = load_map_mesh(&bsp(&BspOptions::default())).unwrap();
    let (eye, yaw) = math::start_camera(&mesh);
    assert!(
        (eye[0] - 10.).abs() < 1e-3 && (eye[1] - 20.).abs() < 1e-3,
        "{eye:?}"
    );
    assert!((eye[2] - (30. + EYE_HEIGHT)).abs() < 1e-3, "{eye:?}");
    assert!((yaw - 45.).abs() < 1e-3);
}

#[test]
fn a_map_without_spawns_starts_mid_bounds_looking_along_x() {
    let mesh = load_map_mesh(&bsp(&BspOptions {
        spawns: vec![],
        ..BspOptions::default()
    }))
    .unwrap();
    let (eye, yaw) = math::start_camera(&mesh);
    assert!(
        (eye[0] - 50.).abs() < 1e-3 && (eye[1] - 32.).abs() < 1e-3 && eye[2].abs() < 1e-3,
        "{eye:?}"
    );
    assert_eq!(yaw, 0.);
}
