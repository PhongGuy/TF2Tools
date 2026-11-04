//! Installing a HUD's missing fonts through a fake host; nothing touches the real system.

mod common;

use std::fs;
use std::path::PathBuf;

use common::fake_host::FakeHost;
use common::hud_fonts::fixture_hud;
use tf2tools_core::{
    FontInstallError, FontSkipReason, Platform, check_hud_fonts, install_hud_fonts, scan_hud_fonts,
};

struct Env {
    _tmp: tempfile::TempDir,
    hud: PathBuf,
    dir: PathBuf,
    missing: Vec<tf2tools_core::HudFont>,
    all: Vec<tf2tools_core::HudFont>,
}

fn env() -> Env {
    let tmp = tempfile::tempdir().unwrap();
    let hud = fixture_hud(tmp.path());
    let dir = tmp.path().join("fonts");
    let fonts = scan_hud_fonts(&hud, Platform::Linux).unwrap();
    let check = check_hud_fonts(&fonts, &FakeHost::new(&[], &dir)).unwrap();
    Env {
        hud,
        dir,
        missing: check.missing().to_vec(),
        all: fonts.fonts().to_vec(),
        _tmp: tmp,
    }
}

fn sorted(mut v: Vec<String>) -> Vec<String> {
    v.sort();
    v
}

/// A HUD whose scheme lists `resource/a/dup.ttf` and `resource/b/dup.ttf` with the given bytes.
fn dup_hud(root: &std::path::Path, a: &[u8], b: &[u8]) -> PathBuf {
    let hud = root.join("custom/dup");
    common::write_file(
        &hud.join("resource/clientscheme.res"),
        br#""Scheme" { "Fonts" { "D" { "1" { "name" "Dup" } } }
            "CustomFontFiles" { "1" "resource/a/dup.ttf" "2" "resource/b/dup.ttf" } }"#,
    );
    common::write_file(&hud.join("resource/a/dup.ttf"), a);
    common::write_file(&hud.join("resource/b/dup.ttf"), b);
    hud
}

#[test]
fn installs_registers_and_refreshes_once() {
    let env = env();
    let host = FakeHost::new(&[], &env.dir);
    let report = install_hud_fonts(&env.hud, &env.missing, &host).unwrap();
    assert_eq!(
        sorted(report.installed().to_vec()),
        [
            "resource/fonts/Code-Pro.otf",
            "resource/fonts/extra.ttf",
            "resource/fonts/surface_semibold.ttf",
        ]
    );
    assert!(report.skipped().is_empty());
    for file in ["Code-Pro.otf", "extra.ttf", "surface_semibold.ttf"] {
        assert_eq!(
            fs::read(env.dir.join(file)).unwrap(),
            fs::read(env.hud.join("resource/fonts").join(file)).unwrap()
        );
    }
    let mut registered = host.registered.lock().unwrap().clone();
    registered.sort();
    assert_eq!(
        registered,
        [
            (
                env.dir.join("Code-Pro.otf"),
                "code-pro-bold".to_owned(),
                true
            ),
            (
                env.dir.join("extra.ttf"),
                "Extra Sans Text Regular".to_owned(),
                false
            ),
            (
                env.dir.join("surface_semibold.ttf"),
                "Surface SemiBold".to_owned(),
                false
            ),
        ]
    );
    assert_eq!(*host.refreshes.lock().unwrap(), 1);
}

#[test]
fn protected_files_are_never_installed() {
    let env = env();
    let host = FakeHost::new(&[], &env.dir);
    let report = install_hud_fonts(&env.hud, &env.all, &host).unwrap();
    assert!(report.skipped().contains(&(
        "resource/fonts/marlett.ttf".to_owned(),
        FontSkipReason::ProtectedName
    )));
    assert!(!env.dir.join("marlett.ttf").exists());
}

#[test]
fn an_identical_file_counts_and_a_different_one_is_untouched() {
    let env = env();
    fs::create_dir_all(&env.dir).unwrap();
    fs::copy(
        env.hud.join("resource/fonts/extra.ttf"),
        env.dir.join("extra.ttf"),
    )
    .unwrap();
    fs::write(env.dir.join("surface_semibold.ttf"), b"someone else's font").unwrap();
    let host = FakeHost::new(&[], &env.dir);
    let report = install_hud_fonts(&env.hud, &env.missing, &host).unwrap();
    assert_eq!(
        sorted(report.installed().to_vec()),
        ["resource/fonts/Code-Pro.otf", "resource/fonts/extra.ttf"]
    );
    assert_eq!(
        report.skipped(),
        [(
            "resource/fonts/surface_semibold.ttf".to_owned(),
            FontSkipReason::FileExists
        )]
    );
    assert_eq!(
        fs::read(env.dir.join("surface_semibold.ttf")).unwrap(),
        b"someone else's font"
    );
}

#[test]
fn a_file_listed_twice_is_installed_once() {
    let env = env();
    let host = FakeHost::new(&[], &env.dir);
    let twice = [env.missing.clone(), env.missing.clone()].concat();
    let report = install_hud_fonts(&env.hud, &twice, &host).unwrap();
    assert_eq!(report.installed().len(), 3);
    assert_eq!(host.registered.lock().unwrap().len(), 3);
}

#[cfg(unix)]
#[test]
fn a_file_that_now_points_outside_the_hud_is_refused() {
    let env = env();
    let inside = env.hud.join("resource/fonts/extra.ttf");
    let outside = env.hud.join("../../elsewhere.ttf");
    fs::rename(&inside, &outside).unwrap();
    std::os::unix::fs::symlink(&outside, &inside).unwrap();
    let host = FakeHost::new(&[], &env.dir);
    let report = install_hud_fonts(&env.hud, &env.missing, &host).unwrap();
    assert!(report.skipped().contains(&(
        "resource/fonts/extra.ttf".to_owned(),
        FontSkipReason::OutsideHud
    )));
    assert!(!env.dir.join("extra.ttf").exists());
}

#[test]
fn a_register_failure_names_the_font_and_keeps_the_copy() {
    let env = env();
    let host = FakeHost {
        fail_register: true,
        ..FakeHost::new(&[], &env.dir)
    };
    match install_hud_fonts(&env.hud, &env.missing, &host) {
        Err(FontInstallError::Register { name, .. }) => assert_eq!(name, "code-pro-bold"),
        other => panic!("expected a register error, got {other:?}"),
    }
    assert!(env.dir.join("Code-Pro.otf").exists());
}

#[test]
fn a_refresh_failure_is_reported_after_the_copies() {
    let env = env();
    let host = FakeHost {
        fail_refresh: true,
        ..FakeHost::new(&[], &env.dir)
    };
    let err = install_hud_fonts(&env.hud, &env.missing, &host).unwrap_err();
    assert!(matches!(err, FontInstallError::Refresh { .. }), "{err:?}");
    assert!(env.dir.join("surface_semibold.ttf").exists());
}

#[test]
fn nothing_to_install_never_refreshes() {
    let env = env();
    let host = FakeHost::new(&[], &env.dir);
    let report = install_hud_fonts(&env.hud, &[], &host).unwrap();
    assert!(report.installed().is_empty());
    assert_eq!(*host.refreshes.lock().unwrap(), 0);
}

#[test]
fn an_install_leaves_no_temporary_files() {
    let env = env();
    let host = FakeHost::new(&[], &env.dir);
    install_hud_fonts(&env.hud, &env.missing, &host).unwrap();
    let mut names: Vec<String> = fs::read_dir(&env.dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    assert_eq!(names, ["Code-Pro.otf", "extra.ttf", "surface_semibold.ttf"]);
}

#[cfg(unix)]
fn mkfifo(path: &std::path::Path) {
    let status = std::process::Command::new("mkfifo")
        .arg(path)
        .status()
        .unwrap();
    assert!(status.success());
}

#[cfg(unix)]
#[test]
fn a_file_that_became_a_fifo_is_never_opened() {
    let env = env();
    let inside = env.hud.join("resource/fonts/extra.ttf");
    fs::remove_file(&inside).unwrap();
    mkfifo(&inside);
    let host = FakeHost::new(&[], &env.dir);
    let err = install_hud_fonts(&env.hud, &env.missing, &host).unwrap_err();
    assert!(matches!(err, FontInstallError::Io { .. }), "{err:?}");
    assert!(!env.dir.join("extra.ttf").exists());
}

#[cfg(unix)]
#[test]
fn a_fifo_or_symlink_in_the_fonts_dir_is_left_alone() {
    let env = env();
    fs::create_dir_all(&env.dir).unwrap();
    mkfifo(&env.dir.join("extra.ttf"));
    let elsewhere = env.dir.join("../target.ttf");
    fs::copy(
        env.hud.join("resource/fonts/surface_semibold.ttf"),
        &elsewhere,
    )
    .unwrap();
    std::os::unix::fs::symlink(&elsewhere, env.dir.join("surface_semibold.ttf")).unwrap();
    let host = FakeHost::new(&[], &env.dir);
    let report = install_hud_fonts(&env.hud, &env.missing, &host).unwrap();
    assert_eq!(report.installed(), ["resource/fonts/Code-Pro.otf"]);
    assert_eq!(
        sorted(report.skipped().iter().map(|(f, _)| f.clone()).collect()),
        [
            "resource/fonts/extra.ttf",
            "resource/fonts/surface_semibold.ttf"
        ]
    );
    assert!(
        report
            .skipped()
            .iter()
            .all(|(_, reason)| *reason == FontSkipReason::FileExists)
    );
    assert!(
        fs::symlink_metadata(env.dir.join("surface_semibold.ttf"))
            .unwrap()
            .file_type()
            .is_symlink()
    );
}

/// Replaces the HUD's `extra.ttf` after the scan, so install alone must judge the new bytes.
fn swap_extra(env: &Env, bytes: &[u8]) {
    fs::write(env.hud.join("resource/fonts/extra.ttf"), bytes).unwrap();
}

#[test]
fn mac_roman_and_padded_protected_names_are_never_installed() {
    for bytes in [
        common::sfnt::sfnt_mac_bytes(&[(1, b"Marlett"), (4, b"Marlett")]),
        common::sfnt::sfnt_bytes(&[(1, "Marlett\0"), (4, "Marlett ")], false),
    ] {
        let env = env();
        swap_extra(&env, &bytes);
        let host = FakeHost::new(&[], &env.dir);
        let report = install_hud_fonts(&env.hud, &env.missing, &host).unwrap();
        assert!(report.skipped().contains(&(
            "resource/fonts/extra.ttf".to_owned(),
            FontSkipReason::ProtectedName
        )));
        assert!(!env.dir.join("extra.ttf").exists());
    }
}

#[test]
fn a_file_with_no_names_is_never_installed() {
    let env = env();
    swap_extra(&env, &common::sfnt::sfnt_bytes(&[(2, "Regular")], false));
    let host = FakeHost::new(&[], &env.dir);
    let report = install_hud_fonts(&env.hud, &env.missing, &host).unwrap();
    assert!(report.skipped().contains(&(
        "resource/fonts/extra.ttf".to_owned(),
        FontSkipReason::Unnamed
    )));
    assert!(!env.dir.join("extra.ttf").exists());
}

#[test]
fn a_name_in_an_undecoded_encoding_is_never_installed() {
    // fontconfig transcodes ShiftJIS, so this ASCII "Marlett" is a name the system would read.
    let utf16 = |t: &str| -> Vec<u8> { t.encode_utf16().flat_map(u16::to_be_bytes).collect() };
    let foo = utf16("Foo");
    let bytes = common::sfnt::sfnt_raw_bytes(&[
        ([3, 1, 0x0409, 1], &foo),
        ([3, 1, 0x0409, 4], &foo),
        ([3, 2, 0x0411, 1], b"Marlett"),
    ]);
    let env = env();
    swap_extra(&env, &bytes);
    let host = FakeHost::new(&[], &env.dir);
    let report = install_hud_fonts(&env.hud, &env.missing, &host).unwrap();
    assert!(report.skipped().contains(&(
        "resource/fonts/extra.ttf".to_owned(),
        FontSkipReason::Unnamed
    )));
    assert!(!env.dir.join("extra.ttf").exists());
}

#[test]
fn a_failure_part_way_still_refreshes_what_was_placed() {
    let env = env();
    let host = FakeHost {
        fail_register: true,
        ..FakeHost::new(&[], &env.dir)
    };
    let err = install_hud_fonts(&env.hud, &env.missing, &host).unwrap_err();
    assert!(matches!(err, FontInstallError::Register { .. }), "{err:?}");
    assert_eq!(*host.refreshes.lock().unwrap(), 1);
}

#[test]
fn identical_files_for_one_destination_count_and_register_once() {
    let tmp = tempfile::tempdir().unwrap();
    let font = common::sfnt::sfnt_bytes(&[(1, "Dup"), (4, "Dup Regular")], false);
    let hud = dup_hud(tmp.path(), &font, &font);
    let fonts = scan_hud_fonts(&hud, Platform::Linux).unwrap();
    let dir = tmp.path().join("fonts");
    let host = FakeHost::new(&[], &dir);
    let report = install_hud_fonts(&hud, fonts.fonts(), &host).unwrap();
    assert_eq!(report.installed(), ["resource/a/dup.ttf"]);
    assert!(report.skipped().is_empty(), "{:?}", report.skipped());
    assert_eq!(host.registered.lock().unwrap().len(), 1);
    assert_eq!(*host.refreshes.lock().unwrap(), 1);
}

#[test]
fn a_different_file_for_a_taken_destination_is_still_skipped() {
    let tmp = tempfile::tempdir().unwrap();
    let a = common::sfnt::sfnt_bytes(&[(1, "Dup"), (4, "Dup Regular")], false);
    let b = common::sfnt::sfnt_bytes(&[(1, "Dup"), (4, "Dup Bold")], false);
    let hud = dup_hud(tmp.path(), &a, &b);
    let fonts = scan_hud_fonts(&hud, Platform::Linux).unwrap();
    let dir = tmp.path().join("fonts");
    let host = FakeHost::new(&[], &dir);
    let report = install_hud_fonts(&hud, fonts.fonts(), &host).unwrap();
    assert_eq!(report.installed(), ["resource/a/dup.ttf"]);
    assert_eq!(
        report.skipped(),
        [("resource/b/dup.ttf".to_owned(), FontSkipReason::FileExists)]
    );
    assert_eq!(fs::read(dir.join("dup.ttf")).unwrap(), a);
}
