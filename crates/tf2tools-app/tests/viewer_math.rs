//! The viewer's camera math and readback unpadding, with no GPU.

use crate::support::demo_view_fixtures::{BspOptions, bsp};
use tf2tools_app::viewer::PlayerBox;
use tf2tools_app::viewer::math::{self, BLU, BoxVertex, EYE_HEIGHT, FAR, NEAR, RED};
use tf2tools_core::{DemoSnapshot, SnapshotPlayer, Team, TfClass, load_map_mesh};

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
    assert_eq!(math::drag_look(100., 0., 1.), (-22., 0.));
    assert_eq!(math::drag_look(0., 50., 1.), (0., 11.));
}

#[test]
fn a_look_scale_of_two_doubles_both_turns() {
    assert_eq!(math::drag_look(100., 50., 2.), (-44., 22.));
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

// Three rows of five pixels in a 256-byte stride, every byte distinct; a short buffer keeps
// only the rows it holds, the last one possibly cut.
#[test]
fn unpad_rows_keeps_each_rows_pixels_across_a_padded_stride() {
    let stride = math::padded_row_bytes(5) as usize;
    assert_eq!(stride, 256);
    let mut padded = vec![0xEE; stride * 3];
    for row in 0..3 {
        for i in 0..20 {
            padded[row * stride + i] = (row * 20 + i) as u8;
        }
    }
    let want: Vec<u8> = (0..60).collect();
    assert_eq!(math::unpad_rows(&padded, 5, 3), want);

    let short = &padded[..stride + 12];
    let want_short: Vec<u8> = (0..20).chain(20..32).collect();
    assert_eq!(math::unpad_rows(short, 5, 3), want_short);
}

#[test]
fn the_camera_starts_at_the_spawn_at_eye_height() {
    let mesh = load_map_mesh(&bsp(&BspOptions::default())).unwrap();
    let (eye, yaw, pitch) = math::start_camera(&mesh, None);
    assert!(
        (eye[0] - 10.).abs() < 1e-3 && (eye[1] - 20.).abs() < 1e-3,
        "{eye:?}"
    );
    assert!((eye[2] - (30. + EYE_HEIGHT)).abs() < 1e-3, "{eye:?}");
    assert!((yaw - 45.).abs() < 1e-3);
    assert_eq!(pitch, 0.);
}

#[test]
fn a_map_without_spawns_starts_mid_bounds_looking_along_x() {
    let mesh = load_map_mesh(&bsp(&BspOptions {
        spawns: vec![],
        ..BspOptions::default()
    }))
    .unwrap();
    let (eye, yaw, pitch) = math::start_camera(&mesh, None);
    assert!(
        (eye[0] - 50.).abs() < 1e-3 && (eye[1] - 32.).abs() < 1e-3 && eye[2].abs() < 1e-3,
        "{eye:?}"
    );
    assert_eq!(yaw, 0.);
    assert_eq!(pitch, 0.);
}

fn player(team: Team, feet: [f32; 3], yaw: f32, pitch: f32, eye: f32) -> SnapshotPlayer {
    SnapshotPlayer::new(team, TfClass::Soldier, feet, yaw, pitch, eye, false)
}

#[test]
fn the_camera_starts_at_the_first_red_players_eye() {
    let mesh = load_map_mesh(&bsp(&BspOptions::default())).unwrap();
    let snapshot = DemoSnapshot::new(
        1668,
        vec![
            player(Team::Blue, [0., 0., 64.], 90., 0., 75.),
            player(Team::Red, [100., 200., 64.], 270., 12., 68.),
            player(Team::Red, [300., 300., 64.], 0., 0., 65.),
        ],
    );
    let (eye, yaw, pitch) = math::start_camera(&mesh, Some(&snapshot));
    assert_eq!((eye, yaw, pitch), ([100., 200., 132.], 270., 12.));
}

#[test]
fn with_no_red_player_the_camera_takes_the_first_and_clamps_its_pitch() {
    let mesh = load_map_mesh(&bsp(&BspOptions::default())).unwrap();
    for (pitch, want) in [(120., 89.), (-120., -89.)] {
        let snapshot = DemoSnapshot::new(
            9,
            vec![
                player(Team::Blue, [10., 20., 30.], 45., pitch, 45.),
                player(Team::Blue, [0., 0., 0.], 0., 0., 68.),
            ],
        );
        let (eye, yaw, got) = math::start_camera(&mesh, Some(&snapshot));
        assert_eq!((eye, yaw, got), ([10., 20., 75.], 45., want));
    }
}

#[test]
fn a_snapshot_without_players_starts_at_the_spawn() {
    let mesh = load_map_mesh(&bsp(&BspOptions::default())).unwrap();
    let empty = DemoSnapshot::new(9, vec![]);
    let (eye, yaw, pitch) = math::start_camera(&mesh, Some(&empty));
    assert_eq!((eye, yaw, pitch), ([10., 20., 30. + EYE_HEIGHT], 45., 0.));
}

#[test]
fn player_boxes_keep_the_snapshots_order_and_fields() {
    assert!(math::player_boxes(None).is_empty());
    let ducked = SnapshotPlayer::new(Team::Blue, TfClass::Spy, [1., 2., 3.], 30., 5., 45., true);
    let snapshot = DemoSnapshot::new(
        9,
        vec![ducked, player(Team::Red, [4., 5., 6.], 60., 0., 68.)],
    );
    let boxes = math::player_boxes(Some(&snapshot));
    assert_eq!(
        &boxes[..],
        [
            PlayerBox {
                feet: [1., 2., 3.],
                yaw_deg: 30.,
                ducked: true,
                team: Team::Blue
            },
            PlayerBox {
                feet: [4., 5., 6.],
                yaw_deg: 60.,
                ducked: false,
                team: Team::Red
            },
        ]
    );
}

fn a_box(team: Team, yaw_deg: f32, ducked: bool) -> PlayerBox {
    PlayerBox {
        feet: [100., 200., 64.],
        yaw_deg,
        ducked,
        team,
    }
}

fn lighter(c: [f32; 3]) -> [f32; 3] {
    c.map(|v| v + (1. - v) * 0.25)
}

fn near(a: [f32; 3], b: [f32; 3]) -> bool {
    (0..3).all(|i| (a[i] - b[i]).abs() < 1e-3)
}

fn extent(vertices: &[BoxVertex]) -> ([f32; 3], [f32; 3]) {
    let mut min = [f32::INFINITY; 3];
    let mut max = [f32::NEG_INFINITY; 3];
    for v in vertices {
        for i in 0..3 {
            min[i] = min[i].min(v.position[i]);
            max[i] = max[i].max(v.position[i]);
        }
    }
    (min, max)
}

#[test]
fn a_standing_box_is_48_wide_and_82_tall_on_the_feet() {
    let vertices = math::player_box_vertices(&a_box(Team::Red, 0., false));
    let (min, max) = extent(&vertices);
    assert!(
        near(min, [76., 176., 64.]) && near(max, [124., 224., 146.]),
        "{min:?} {max:?}"
    );
}

#[test]
fn a_ducked_box_is_62_tall() {
    let (min, max) = extent(&math::player_box_vertices(&a_box(Team::Red, 0., true)));
    assert!(
        (min[2] - 64.).abs() < 1e-3 && (max[2] - 126.).abs() < 1e-3,
        "{min:?} {max:?}"
    );
}

#[test]
fn a_box_turns_with_its_yaw() {
    // At 45 degrees a corner points along +X: half the diagonal, 24 * sqrt(2).
    let (min, max) = extent(&math::player_box_vertices(&a_box(Team::Red, 45., false)));
    let half = 24. * 2f32.sqrt();
    assert!((max[0] - (100. + half)).abs() < 1e-2, "{max:?}");
    assert!((min[1] - (200. - half)).abs() < 1e-2, "{min:?}");
}

#[test]
fn the_lighter_face_looks_along_the_yaw() {
    for (yaw, dir) in [
        (0., [1., 0.]),
        (90., [0., 1.]),
        (180., [-1., 0.]),
        (270., [0., -1.]),
    ] {
        let vertices = math::player_box_vertices(&a_box(Team::Red, yaw, false));
        let front: Vec<&BoxVertex> = vertices
            .iter()
            .filter(|v| near(v.color, lighter(RED)))
            .collect();
        assert_eq!(front.len(), 6, "yaw {yaw}");
        assert_eq!(
            vertices.iter().filter(|v| v.color == RED).count(),
            30,
            "yaw {yaw}"
        );
        let mid = [0, 1, 2].map(|i| front.iter().map(|v| v.position[i]).sum::<f32>() / 6.);
        let want = [100. + 24. * dir[0], 200. + 24. * dir[1], 64. + 41.];
        assert!(near(mid, want), "yaw {yaw}: {mid:?} want {want:?}");
    }
}

#[test]
fn blu_boxes_are_blu() {
    let vertices = math::player_box_vertices(&a_box(Team::Blue, 0., false));
    assert_eq!(vertices.iter().filter(|v| v.color == BLU).count(), 30);
    assert_eq!(
        vertices
            .iter()
            .filter(|v| near(v.color, lighter(BLU)))
            .count(),
        6
    );
}

#[test]
fn every_triangle_faces_out_of_the_box() {
    for (yaw, ducked) in [(0., false), (37., false), (200., true)] {
        let vertices = math::player_box_vertices(&a_box(Team::Red, yaw, ducked));
        let height = if ducked { 62. } else { 82. };
        let centre = [100., 200., 64. + height / 2.];
        for tri in vertices.chunks(3) {
            let [a, b, c] = [tri[0].position, tri[1].position, tri[2].position];
            let (u, v) = (
                [0, 1, 2].map(|i| b[i] - a[i]),
                [0, 1, 2].map(|i| c[i] - a[i]),
            );
            let normal = [
                u[1] * v[2] - u[2] * v[1],
                u[2] * v[0] - u[0] * v[2],
                u[0] * v[1] - u[1] * v[0],
            ];
            let out = [0, 1, 2].map(|i| (a[i] + b[i] + c[i]) / 3. - centre[i]);
            let facing: f32 = (0..3).map(|i| normal[i] * out[i]).sum();
            assert!(facing > 0., "yaw {yaw} ducked {ducked}: {tri:?}");
        }
    }
}

#[test]
fn a_hidden_player_has_no_box() {
    use tf2tools_core::DemoTracks;
    let at = |tick| {
        DemoSnapshot::new(
            tick,
            vec![
                SnapshotPlayer::new(Team::Blue, TfClass::Scout, [0., 0., 0.], 0., 0., 65., false),
                SnapshotPlayer::new(
                    Team::Red,
                    TfClass::Medic,
                    [100., 0., 0.],
                    0.,
                    0.,
                    75.,
                    false,
                ),
            ],
        )
    };
    let tracks = DemoTracks::from_keyframes(1. / 66., &[at(0), at(10)], vec![], None).unwrap();
    let snapshot = tracks.at(5.);
    let red = snapshot.players()[1].id();
    let boxes = math::player_boxes_except(Some(&snapshot), Some(red));
    assert_eq!(boxes.len(), 1);
    assert_eq!(boxes[0].team, Team::Blue);
    assert_eq!(math::player_boxes_except(Some(&snapshot), None).len(), 2);
    assert_eq!(
        &*math::player_boxes(Some(&snapshot)),
        &*math::player_boxes_except(Some(&snapshot), None)
    );
}
