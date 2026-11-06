//! Finding an old TF2Tools 4 install from what Windows lists under Apps, over fake registry
//! entries and a fake file check.

mod common;

use std::path::{Path, PathBuf};

use tf2tools_core::{Database, OldV4, UninstallEntry, find_old_v4};

const V4_EXE: &str = r"C:\Users\pyro\AppData\Local\Programs\tf2tools\Uninstall TF2Tools.exe";

// What electron-builder's per-user NSIS installer writes for TF2Tools 4.8.0.
fn v4_entry() -> UninstallEntry {
    UninstallEntry::new("a1b2c3d4-0000-5000-8000-000000000000")
        .display_name("TF2Tools 4.8.0")
        .display_version("4.8.0")
        .uninstall_string(format!(r#""{V4_EXE}" /currentuser"#))
}

// What cargo-packager writes for v6 itself.
fn v6_entry() -> UninstallEntry {
    UninstallEntry::new("TF2Tools")
        .display_name("TF2Tools")
        .display_version("6.0.0")
        .uninstall_string(r#""C:\Users\pyro\AppData\Local\TF2Tools\uninstall.exe""#)
}

fn exists_only(path: &'static str) -> impl Fn(&Path) -> bool {
    move |p| p == Path::new(path)
}

#[test]
fn finds_v4_and_its_own_uninstaller() {
    let found: OldV4 = find_old_v4(&[v6_entry(), v4_entry()], exists_only(V4_EXE)).unwrap();
    assert_eq!(found.version(), "4.8.0");
    assert_eq!(found.uninstaller(), PathBuf::from(V4_EXE));
    assert_eq!(found.arguments(), ["/currentuser".to_owned()]);
}

#[test]
fn nothing_found_without_a_v4_entry() {
    assert!(find_old_v4(&[], |_| true).is_none());
    assert!(find_old_v4(&[v6_entry()], |_| true).is_none());
}

#[test]
fn an_entry_whose_uninstaller_is_gone_is_a_leftover_not_an_install() {
    assert!(find_old_v4(&[v4_entry()], |_| false).is_none());
}

#[test]
fn an_entry_without_an_uninstall_command_is_a_leftover() {
    let bare = UninstallEntry::new("x")
        .display_name("TF2Tools 4.8.0")
        .display_version("4.8.0");
    assert!(find_old_v4(&[bare], |_| true).is_none());
}

#[test]
fn other_apps_and_versions_are_never_taken_for_v4() {
    let other = UninstallEntry::new("y")
        .display_name("TF2 Hud Manager 4.1.0")
        .display_version("4.1.0")
        .uninstall_string(r#""C:\x\Uninstall TF2 Hud Manager.exe""#);
    let v5 = UninstallEntry::new("z")
        .display_name("TF2Tools 5.0.0")
        .display_version("5.0.0")
        .uninstall_string(format!(r#""{V4_EXE}" /currentuser"#));
    let wrong_exe = UninstallEntry::new("w")
        .display_name("TF2Tools 4.8.0")
        .display_version("4.8.0")
        .uninstall_string(r#""C:\x\uninstall.exe""#);
    assert!(find_old_v4(&[other, v5, wrong_exe], |_| true).is_none());
}

#[test]
fn an_unquoted_uninstall_command_still_parses() {
    let entry = UninstallEntry::new("u")
        .display_name("TF2Tools 4.7.1")
        .display_version("4.7.1")
        .uninstall_string(r"C:\Programs\tf2tools\Uninstall TF2Tools.exe /currentuser");
    let found = find_old_v4(&[entry], |_| true).unwrap();
    assert_eq!(
        found.uninstaller(),
        PathBuf::from(r"C:\Programs\tf2tools\Uninstall TF2Tools.exe")
    );
    assert_eq!(found.arguments(), ["/currentuser".to_owned()]);
}

#[cfg(not(windows))]
#[test]
fn nothing_is_detected_off_windows() {
    assert!(tf2tools_core::detect_old_v4().is_none());
}

#[test]
fn the_old_v4_question_is_remembered() {
    let (_tmp, dir) = common::temp_data_dir();
    let db = Database::open(&dir).unwrap();
    assert!(!db.settings().unwrap().old_v4_asked());
    db.set_old_v4_asked().unwrap();
    assert!(
        Database::open(&dir)
            .unwrap()
            .settings()
            .unwrap()
            .old_v4_asked()
    );
}
