//! The demo viewer's map mesh: which faces are drawn, where brush entities go, the caps and the
//! panic guard, all on the synthetic map.

use crate::common::demo_view::{BspOptions, bsp};
use tf2tools_core::{
    MAX_MAP_BYTES, MAX_MAP_TRIANGLES, MapMesh, MapMeshError, load_map_file, load_map_file_capped,
    load_map_mesh, panic_mark,
};

fn near(a: [f32; 3], b: [f32; 3]) -> bool {
    (0..3).all(|i| (a[i] - b[i]).abs() < 1e-3)
}

fn mesh(opts: BspOptions) -> MapMesh {
    load_map_mesh(&bsp(&opts)).unwrap()
}

#[test]
fn only_drawn_faces_become_triangles() {
    let mesh = mesh(BspOptions::default());
    // The world floor and the brush's floor; the TOOLS/ face (z 8) and NODRAW face (z 16) go.
    assert_eq!(mesh.triangle_count(), 4);
    assert_eq!(mesh.positions().len(), 12);
    assert_eq!(mesh.normals().len(), 12);
    assert!(
        mesh.positions().iter().all(|p| p[2].abs() < 1e-3),
        "a dropped face leaked: {:?}",
        mesh.positions()
    );
}

#[test]
fn each_vertex_carries_its_faces_plane_normal() {
    let mesh = mesh(BspOptions::default());
    assert!(
        mesh.normals().iter().all(|n| near(*n, [0., 0., 1.])),
        "{:?}",
        mesh.normals()
    );
}

#[test]
fn a_brush_entity_is_turned_by_its_yaw_then_moved_by_its_origin() {
    // *1's floor spans 0..64 locally; yaw 90 turns it onto x -64..0, origin 100 0 0 moves it.
    let mesh = mesh(BspOptions::default());
    for corner in [
        [36., 0., 0.],
        [100., 0., 0.],
        [100., 64., 0.],
        [36., 64., 0.],
    ] {
        assert!(
            mesh.positions().iter().any(|p| near(*p, corner)),
            "no vertex at {corner:?}: {:?}",
            mesh.positions()
        );
    }
    let (min, max) = mesh.bounds();
    assert!(near(min, [0., 0., 0.]), "{min:?}");
    assert!(near(max, [100., 64., 0.]), "{max:?}");
}

#[test]
fn an_unturned_brush_entity_is_only_moved() {
    let mesh = mesh(BspOptions {
        brush: ("0 200 0", "0 0 0"),
        ..BspOptions::default()
    });
    assert!(mesh.positions().iter().any(|p| near(*p, [64., 264., 0.])));
    assert!(
        near(mesh.bounds().1, [64., 264., 0.]),
        "{:?}",
        mesh.bounds()
    );
}

#[test]
fn start_is_the_first_teamspawn_in_lump_order() {
    let (origin, yaw) = mesh(BspOptions::default()).start().expect("a spawn");
    assert!(near(origin, [10., 20., 30.]), "{origin:?}");
    assert!((yaw - 45.).abs() < 1e-3, "{yaw}");
}

#[test]
fn start_is_none_without_a_teamspawn() {
    let mesh = mesh(BspOptions {
        spawns: vec![],
        ..BspOptions::default()
    });
    assert_eq!(mesh.start(), None);
}

#[test]
fn bounds_are_finite() {
    let (min, max) = mesh(BspOptions::default()).bounds();
    assert!(min.iter().chain(max.iter()).all(|v| v.is_finite()));
}

#[test]
fn a_map_with_nothing_drawn_is_empty() {
    let only_tools = bsp(&BspOptions {
        only_tools: true,
        ..BspOptions::default()
    });
    assert!(matches!(
        load_map_mesh(&only_tools),
        Err(MapMeshError::Empty)
    ));
}

#[test]
fn garbage_is_unreadable() {
    assert!(matches!(
        load_map_mesh(b"not a map at all"),
        Err(MapMeshError::Unreadable(_))
    ));
}

#[test]
fn a_vbsp_panic_is_unreadable_and_leaves_the_thread_unmarked() {
    let bad = bsp(&BspOptions {
        bad_plane: true,
        ..BspOptions::default()
    });
    assert!(matches!(
        load_map_mesh(&bad),
        Err(MapMeshError::Unreadable(_))
    ));
    assert!(!panic_mark::is_marked());
}

#[test]
fn more_triangles_than_the_cap_is_too_large() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("cp_synthetic.bsp");
    std::fs::write(&path, bsp(&BspOptions::default())).unwrap();
    assert!(matches!(
        load_map_file_capped(&path, MAX_MAP_BYTES, 3),
        Err(MapMeshError::TooLarge)
    ));
    assert_eq!(
        load_map_file_capped(&path, MAX_MAP_BYTES, 4)
            .unwrap()
            .triangle_count(),
        4
    );
}

#[test]
fn a_file_over_the_byte_cap_is_too_large() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("cp_synthetic.bsp");
    let bytes = bsp(&BspOptions::default());
    std::fs::write(&path, &bytes).unwrap();
    let len = bytes.len() as u64;
    assert!(matches!(
        load_map_file_capped(&path, len - 1, MAX_MAP_TRIANGLES),
        Err(MapMeshError::TooLarge)
    ));
    assert!(load_map_file_capped(&path, len, MAX_MAP_TRIANGLES).is_ok());
    assert_eq!(load_map_file(&path).unwrap().triangle_count(), 4);
}

#[test]
fn a_missing_file_is_unreadable() {
    let dir = tempfile::tempdir().unwrap();
    assert!(matches!(
        load_map_file(&dir.path().join("nope.bsp")),
        Err(MapMeshError::Unreadable(_))
    ));
}

#[test]
fn the_caps_are_the_specs() {
    assert_eq!(MAX_MAP_TRIANGLES, 1_000_000);
    assert_eq!(MAX_MAP_BYTES, 512 * 1024 * 1024);
}
