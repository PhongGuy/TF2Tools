use crate::common;

use tf2tools_core::{DetectError, Tf2Install};

#[test]
fn from_path_accepts_a_game_dir() {
    let (_tmp, game) = common::temp_tf2_tree();
    let install = Tf2Install::from_path(&game).expect("valid install");
    assert_eq!(install.game_dir(), game);
    assert_eq!(install.tf_dir(), game.join("tf"));
    assert_eq!(install.custom_dir(), game.join("tf/custom"));
}

#[test]
fn from_path_creates_a_missing_custom_dir() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(tmp.path().join("tf")).unwrap();
    let install = Tf2Install::from_path(tmp.path()).expect("valid install");
    assert!(install.custom_dir().is_dir());
}

#[test]
fn from_path_rejects_a_dir_without_tf() {
    let tmp = tempfile::tempdir().unwrap();
    let err = Tf2Install::from_path(tmp.path()).expect_err("no tf/");
    assert!(
        matches!(&err, DetectError::NotTf2 { path } if path == tmp.path()),
        "{err:?}"
    );
}

#[test]
fn from_path_rejects_tf_as_a_file_and_a_missing_dir() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(tmp.path().join("tf"), b"").unwrap();
    assert!(matches!(
        Tf2Install::from_path(tmp.path()),
        Err(DetectError::NotTf2 { .. })
    ));
    let missing = tmp.path().join("nope");
    assert!(matches!(
        Tf2Install::from_path(&missing),
        Err(DetectError::NotTf2 { .. })
    ));
}

#[cfg(unix)]
#[test]
fn unwritable_tf_dir_reports_io_with_the_custom_path() {
    use std::os::unix::fs::PermissionsExt;
    let tmp = tempfile::tempdir().unwrap();
    let tf = tmp.path().join("tf");
    std::fs::create_dir_all(&tf).unwrap();
    std::fs::set_permissions(&tf, std::fs::Permissions::from_mode(0o555)).unwrap();
    if std::fs::write(tf.join("probe"), b"").is_ok() {
        return; // running as root
    }
    let err = Tf2Install::from_path(tmp.path()).expect_err("read-only tf/");
    std::fs::set_permissions(&tf, std::fs::Permissions::from_mode(0o755)).unwrap();
    assert!(
        matches!(&err, DetectError::Io { path, .. } if *path == tf.join("custom")),
        "{err:?}"
    );
}
