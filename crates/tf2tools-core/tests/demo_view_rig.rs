//! Class rigs from a game folder: no game, no model, bad bytes, and the real install.

use std::path::PathBuf;

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
