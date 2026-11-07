//! Checking a scanned HUD against the installed fonts through a fake host.

use crate::common;

use common::fake_host::FakeHost;
use common::hud_fonts::fixture_hud;
use tf2tools_core::{FontCheckError, Platform, check_hud_fonts, scan_hud_fonts};

fn missing_names(check: &tf2tools_core::FontCheck) -> Vec<String> {
    let mut names: Vec<_> = check
        .missing()
        .iter()
        .map(|f| f.name().to_owned())
        .collect();
    names.sort();
    names
}

#[test]
fn installed_names_match_case_insensitively() {
    let tmp = tempfile::tempdir().unwrap();
    let fonts = scan_hud_fonts(&fixture_hud(tmp.path()), Platform::Linux).unwrap();
    let host = FakeHost::new(&["surface", "Verdana"], tmp.path());
    let check = check_hud_fonts(&fonts, &host).unwrap();
    assert_eq!(missing_names(&check), ["Extra Sans", "code-pro-bold"]);
    assert_eq!(check.protected(), ["Marlett"]);
    assert_eq!(check.issues().len(), 3);
    assert!(check.needs_attention());
}

#[test]
fn everything_installed_on_windows_needs_no_attention() {
    let tmp = tempfile::tempdir().unwrap();
    let fonts = scan_hud_fonts(&fixture_hud(tmp.path()), Platform::Windows).unwrap();
    let host = FakeHost::new(
        &["Surface", "CODE-PRO-BOLD", "Extra Sans", "WinFont"],
        tmp.path(),
    );
    let check = check_hud_fonts(&fonts, &host).unwrap();
    assert!(check.missing().is_empty());
    assert_eq!(
        check.protected(),
        ["Marlett"],
        "a protected name is listed, never asked about"
    );
    assert!(check.issues().is_empty());
    assert!(!check.needs_attention());
    assert!(check.fingerprint().is_empty());
}

// A case mismatch needs a case-sensitive file system, which Windows lacks.
#[cfg(not(windows))]
#[test]
fn the_fingerprint_is_sorted_keys() {
    let tmp = tempfile::tempdir().unwrap();
    let fonts = scan_hud_fonts(&fixture_hud(tmp.path()), Platform::Linux).unwrap();
    let check = check_hud_fonts(&fonts, &FakeHost::new(&[], tmp.path())).unwrap();
    assert_eq!(
        check.fingerprint(),
        [
            "font:code-pro-bold",
            "font:extra sans",
            "font:surface",
            "issue:case:resource/fonts/surface_semiBold.ttf",
            "issue:uppercase:resource/fonts/Code-Pro.otf",
            "issue:vpk:resource/fonts/packed.ttf",
        ]
    );
}

#[test]
fn a_host_failure_is_a_host_error() {
    let tmp = tempfile::tempdir().unwrap();
    let fonts = scan_hud_fonts(&fixture_hud(tmp.path()), Platform::Linux).unwrap();
    let host = FakeHost {
        fail_names: true,
        ..FakeHost::new(&[], tmp.path())
    };
    match check_hud_fonts(&fonts, &host) {
        Err(FontCheckError::Host { reason }) => assert!(reason.contains("fc-list is missing")),
        other => panic!("expected a host error, got {other:?}"),
    }
}

#[test]
fn a_hud_with_nothing_to_check_never_asks_the_host() {
    let tmp = tempfile::tempdir().unwrap();
    let fonts = scan_hud_fonts(tmp.path(), Platform::Linux).unwrap();
    let host = FakeHost {
        fail_names: true,
        ..FakeHost::new(&[], tmp.path())
    };
    let check = check_hud_fonts(&fonts, &host).unwrap();
    assert!(!check.needs_attention());
}

#[test]
fn issues_without_shipped_fonts_never_ask_the_host() {
    let tmp = tempfile::tempdir().unwrap();
    let hud = tmp.path().join("vpkonly");
    common::write_file(
        &hud.join("resource/clientscheme.res"),
        br#""Scheme" { "Fonts" { "Pak" { "1" { "name" "Packed" } } }
            "CustomFontFiles" { "1" "resource/fonts/packed.ttf" } }"#,
    );
    let packed = common::sfnt::sfnt_bytes(&[(1, "Packed")], false);
    common::write_file(
        &hud.join("resource/pak_dir.vpk"),
        &common::vpk_bytes(&[("resource/fonts/packed.ttf", &packed)]),
    );
    let fonts = scan_hud_fonts(&hud, Platform::Linux).unwrap();
    assert!(fonts.fonts().is_empty(), "{:?}", fonts.fonts());
    assert_eq!(fonts.issues().len(), 1, "{:?}", fonts.issues());
    let host = FakeHost {
        fail_names: true,
        ..FakeHost::new(&[], tmp.path())
    };
    let check = check_hud_fonts(&fonts, &host).unwrap();
    assert_eq!(check.issues(), fonts.issues());
    assert!(check.missing().is_empty());
}
