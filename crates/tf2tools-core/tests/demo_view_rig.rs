//! Class rigs from a game folder: no game, no model, bad bytes, and the real install.

use std::path::{Path, PathBuf};
use std::time::Instant;

use tf2tools_core::{ClassRig, RigLoadError, TfClass};

use crate::common;

fn tf_with(entries: &[(&str, &[u8])]) -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let tf = dir.path().join("tf");
    common::write_file(&tf.join("tf2_misc_dir.vpk"), &common::vpk_bytes(entries));
    (dir, tf)
}

#[test]
fn no_game_folder_is_no_install() {
    let dir = tempfile::tempdir().unwrap();
    assert!(matches!(
        ClassRig::load(&dir.path().join("tf"), TfClass::Scout),
        Err(RigLoadError::NoInstall)
    ));
}

#[test]
fn a_pak_without_the_model_is_missing() {
    let (_dir, tf) = tf_with(&[("models/player/soldier.mdl", b"x")]);
    assert!(matches!(
        ClassRig::load(&tf, TfClass::Scout),
        Err(RigLoadError::Missing { path }) if path == "models/player/scout.mdl"
    ));
}

#[test]
fn model_bytes_that_are_no_model_are_malformed() {
    let (_dir, tf) = tf_with(&[
        ("models/player/scout.mdl", b"not a model"),
        ("models/player/scout.vvd", b"x"),
        ("models/player/scout.dx90.vtx", b"x"),
    ]);
    assert!(matches!(
        ClassRig::load(&tf, TfClass::Scout),
        Err(RigLoadError::Malformed { .. })
    ));
}

#[test]
fn a_model_over_the_byte_cap_is_too_large() {
    let big = vec![0u8; 16 * 1024 * 1024 + 1];
    let (_dir, tf) = tf_with(&[("models/player/scout.mdl", &big)]);
    assert!(matches!(
        ClassRig::load(&tf, TfClass::Scout),
        Err(RigLoadError::TooLarge { .. })
    ));
}

fn real_tf() -> PathBuf {
    PathBuf::from(std::env::var("TF2TOOLS_TF2_GAME").expect("TF2TOOLS_TF2_GAME")).join("tf")
}

fn stem(class: TfClass) -> &'static str {
    let stems = [
        "scout", "soldier", "pyro", "demo", "heavy", "engineer", "medic", "sniper", "spy",
    ];
    stems[TfClass::ALL.iter().position(|c| *c == class).unwrap()]
}

/// (bones, LOD 0 vertices, default body group LOD 0 triangles) as vmdl 0.2.0 reads them.
fn vmdl_counts(tf: &Path, class: TfClass) -> (usize, usize, usize) {
    let pak = vpk::from_path(tf.join("tf2_misc_dir.vpk")).unwrap();
    let read = |ext: &str| {
        let name = format!("models/player/{}.{ext}", stem(class));
        pak.tree.get(&name).unwrap().get().unwrap().into_owned()
    };
    let mdl = vmdl::Mdl::read(&read("mdl")).unwrap();
    let vvd = vmdl::Vvd::read(&read("vvd")).unwrap();
    let vtx = vmdl::Vtx::read(&read("dx90.vtx")).unwrap();
    let triangles = vtx
        .body_parts
        .iter()
        .filter_map(|part| part.models.first())
        .filter_map(|model| model.lods.first())
        .flat_map(|lod| &lod.meshes)
        .flat_map(|mesh| &mesh.strip_groups)
        .map(|group| group.indices.len())
        .sum::<usize>()
        / 3;
    (mdl.bones.len(), vvd.vertices.len(), triangles)
}

#[test]
#[ignore = "needs a TF2 game folder in TF2TOOLS_TF2_GAME"]
fn every_class_loads_with_the_counts_vmdl_reads() {
    let tf = real_tf();
    let mut heap = 0;
    for class in TfClass::ALL {
        let rig = ClassRig::load(&tf, class).unwrap_or_else(|e| panic!("{class:?}: {e}"));
        let ours = (
            rig.skeleton().bones().len(),
            rig.mesh().vertex_count(),
            rig.mesh().triangle_count(),
        );
        assert_eq!(ours, vmdl_counts(&tf, class), "{class:?}");
        println!("{class:?}: {} heap bytes", rig.heap_bytes());
        heap += rig.heap_bytes();
    }
    println!("all nine rigs: {heap} heap bytes");
}

#[test]
#[ignore = "needs a TF2 game folder in TF2TOOLS_TF2_GAME"]
fn the_scout_rig_carries_the_data_posing_needs() {
    let rig = ClassRig::load(&real_tf(), TfClass::Scout).unwrap();
    assert!(
        rig.sequences()
            .iter()
            .any(|s| s.activity() == "ACT_MP_JUMP_START_primary"),
        "activity names keep their stored case"
    );
    for name in ["move_x", "move_y", "body_yaw", "body_pitch"] {
        assert!(rig.pose_param(name).is_some(), "{name}");
    }
    let pitch = &rig.pose_params()[rig.pose_param("body_pitch").unwrap()];
    assert_eq!(
        (pitch.start(), pitch.end()),
        (-45., 90.),
        "the merged range covers both files"
    );
    let run = rig
        .sequences()
        .iter()
        .find(|s| s.label().eq_ignore_ascii_case("run_PRIMARY"))
        .unwrap();
    assert_eq!((run.activity(), run.grid()), ("ACT_MP_RUN_PRIMARY", [3, 3]));
    let aim = &rig.sequences()[run.autolayers()[0].sequence()];
    assert_eq!(
        (aim.grid(), aim.pose_keys(1).map(<[f32]>::len)),
        ([3, 4], Some(4)),
        "{}",
        aim.label()
    );
    let moving = run
        .animations()
        .iter()
        .filter(|&&a| !rig.animations()[a].movements().is_empty())
        .count();
    assert_eq!(moving, 8, "every run animation but the centre one moves");
}

#[test]
#[ignore = "needs a TF2 game folder in TF2TOOLS_TF2_GAME; time it with --release"]
fn each_rig_loads_within_100_ms() {
    if cfg!(debug_assertions) {
        panic!("time a release build");
    }
    let tf = real_tf();
    for class in TfClass::ALL {
        let mut times: Vec<f64> = (0..5)
            .map(|_| {
                let started = Instant::now();
                ClassRig::load(&tf, class).unwrap();
                started.elapsed().as_secs_f64() * 1e3
            })
            .collect();
        times.sort_by(f64::total_cmp);
        println!("{class:?}: median {:.1} ms", times[2]);
        assert!(times[2] <= 100., "{class:?}: {:.1} ms", times[2]);
    }
}
