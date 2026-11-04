//! Scanning a HUD's declared fonts per platform, with the survey's absent-file rules.

mod common;

use std::path::PathBuf;

use common::hud_fonts::fixture_hud;
#[cfg(not(windows))]
use tf2tools_core::FontIssue;
use tf2tools_core::{FontCheckError, HudFonts, Platform, scan_hud_fonts};

fn summary(fonts: &HudFonts) -> Vec<(String, Vec<PathBuf>)> {
    let mut out: Vec<_> = fonts
        .fonts()
        .iter()
        .map(|f| (f.name().to_owned(), f.files().to_vec()))
        .collect();
    out.sort();
    out
}

fn entry(name: &str, file: &str) -> (String, Vec<PathBuf>) {
    (name.to_owned(), vec![PathBuf::from(file)])
}

// A case mismatch needs a case-sensitive file system, which Windows lacks.
#[cfg(not(windows))]
#[test]
fn linux_scan_matches_shipped_names_and_reports_file_problems() {
    let tmp = tempfile::tempdir().unwrap();
    let hud = fixture_hud(tmp.path());
    let fonts = scan_hud_fonts(&hud, Platform::Linux).unwrap();
    assert_eq!(
        summary(&fonts),
        [
            entry("Extra Sans", "resource/fonts/extra.ttf"),
            entry("Marlett", "resource/fonts/marlett.ttf"),
            entry("Surface", "resource/fonts/surface_semibold.ttf"),
            entry("code-pro-bold", "resource/fonts/Code-Pro.otf"),
        ]
    );
    let issues = fonts.issues();
    assert_eq!(issues.len(), 3, "{issues:?}");
    assert!(issues.contains(&FontIssue::PathCaseMismatch {
        declared: "resource/fonts/surface_semiBold.ttf".into(),
        on_disk: PathBuf::from("resource/fonts/surface_semibold.ttf"),
    }));
    assert!(issues.contains(&FontIssue::UppercaseFileName {
        file: PathBuf::from("resource/fonts/Code-Pro.otf"),
    }));
    assert!(issues.contains(&FontIssue::InsideVpk {
        declared: "resource/fonts/packed.ttf".into(),
        vpk: PathBuf::from("resource/pak_dir.vpk"),
    }));
}

#[test]
fn windows_scan_has_no_issues_and_reads_windows_conditionals() {
    let tmp = tempfile::tempdir().unwrap();
    let hud = fixture_hud(tmp.path());
    let fonts = scan_hud_fonts(&hud, Platform::Windows).unwrap();
    assert_eq!(
        summary(&fonts),
        [
            entry("Extra Sans", "resource/fonts/extra.ttf"),
            entry("Marlett", "resource/fonts/marlett.ttf"),
            entry("Surface", "resource/fonts/surface_semibold.ttf"),
            entry("WinFont", "resource/fonts/winfont.ttf"),
            entry("code-pro-bold", "resource/fonts/Code-Pro.otf"),
        ]
    );
    assert!(fonts.issues().is_empty());
}

#[test]
fn a_scheme_name_finds_a_file_by_any_family_it_carries() {
    // tf.ttf and tfd.ttf carry a Unicode-platform family and a different Windows one.
    let utf16 = |t: &str| -> Vec<u8> { t.encode_utf16().flat_map(u16::to_be_bytes).collect() };
    let font = |first: &str, second: &str| {
        common::sfnt::sfnt_raw_bytes(&[
            ([0, 0, 0, 1], &utf16(first)),
            ([3, 1, 0x0409, 1], &utf16(second)),
        ])
    };
    let tmp = tempfile::tempdir().unwrap();
    let scheme = r#""Scheme" { "Fonts" {
        "A" { "1" { "name" "Team Fortress" } }
        "B" { "1" { "name" "TFD" } }
    } "CustomFontFiles" { "1" "resource/tf.ttf" "2" "resource/tfd.ttf" } }"#;
    common::write_file(
        &tmp.path().join("resource/clientscheme.res"),
        scheme.as_bytes(),
    );
    common::write_file(
        &tmp.path().join("resource/tf.ttf"),
        &font("Counter-Strike", "Team Fortress"),
    );
    common::write_file(&tmp.path().join("resource/tfd.ttf"), &font("cs", "tfd"));
    let fonts = scan_hud_fonts(tmp.path(), Platform::Windows).unwrap();
    assert_eq!(
        summary(&fonts),
        [
            entry("TFD", "resource/tfd.ttf"),
            entry("Team Fortress", "resource/tf.ttf")
        ]
    );
}

#[test]
fn a_hud_without_a_scheme_is_empty() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(tmp.path().join("resource")).unwrap();
    let fonts = scan_hud_fonts(tmp.path(), Platform::Linux).unwrap();
    assert!(fonts.fonts().is_empty());
    assert!(fonts.issues().is_empty());
}

#[test]
fn a_missing_hud_dir_is_an_io_error() {
    let tmp = tempfile::tempdir().unwrap();
    let err = scan_hud_fonts(&tmp.path().join("nope"), Platform::Linux).unwrap_err();
    assert!(matches!(err, FontCheckError::Io { .. }), "{err:?}");
}

/// Scans `hud` on a worker thread, failing instead of hanging when the scan blocks.
#[cfg(unix)]
fn scan_within_timeout(hud: std::path::PathBuf) -> Result<HudFonts, FontCheckError> {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(scan_hud_fonts(&hud, Platform::Linux));
    });
    rx.recv_timeout(std::time::Duration::from_secs(10))
        .expect("scan_hud_fonts blocked on a FIFO")
}

#[cfg(unix)]
fn mkfifo(path: &std::path::Path) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let status = std::process::Command::new("mkfifo")
        .arg(path)
        .status()
        .unwrap();
    assert!(status.success(), "mkfifo failed");
}

#[cfg(unix)]
#[test]
fn a_fifo_scheme_or_base_does_not_block_the_scan() {
    let tmp = tempfile::tempdir().unwrap();
    let scheme_fifo = tmp.path().join("a");
    mkfifo(&scheme_fifo.join("resource/clientscheme.res"));
    let fonts = scan_within_timeout(scheme_fifo).unwrap();
    assert!(fonts.fonts().is_empty());

    let base_fifo = tmp.path().join("b");
    common::write_file(
        &base_fifo.join("resource/clientscheme.res"),
        br#"#base "pipe.res"
"Scheme" { "Fonts" { "M" { "1" { "name" "Unused" } } } "CustomFontFiles" { "1" "resource/fonts/unused.ttf" } }"#,
    );
    common::write_file(
        &base_fifo.join("resource/fonts/unused.ttf"),
        &common::sfnt::sfnt_bytes(&[(1, "Unused")], false),
    );
    mkfifo(&base_fifo.join("resource/pipe.res"));
    let fonts = scan_within_timeout(base_fifo).unwrap();
    assert_eq!(fonts.fonts().len(), 1);
}

#[cfg(unix)]
#[test]
fn the_uppercase_check_reads_the_declared_name_not_a_symlink_target() {
    let tmp = tempfile::tempdir().unwrap();
    let hud = tmp.path().join("hud");
    common::write_file(
        &hud.join("resource/clientscheme.res"),
        br#""Scheme" { "Fonts" { "A" { "1" { "name" "Lower" } } "B" { "1" { "name" "Upper" } } }
"CustomFontFiles" { "1" "resource/fonts/lower.ttf" "2" "resource/fonts/Upper.ttf" } }"#,
    );
    let fonts_dir = hud.join("resource/fonts");
    common::write_file(
        &fonts_dir.join("Real.ttf"),
        &common::sfnt::sfnt_bytes(&[(1, "Lower")], false),
    );
    common::write_file(
        &fonts_dir.join("real_upper.ttf"),
        &common::sfnt::sfnt_bytes(&[(1, "Upper")], false),
    );
    std::os::unix::fs::symlink("Real.ttf", fonts_dir.join("lower.ttf")).unwrap();
    std::os::unix::fs::symlink("real_upper.ttf", fonts_dir.join("Upper.ttf")).unwrap();
    let fonts = scan_hud_fonts(&hud, Platform::Linux).unwrap();
    assert_eq!(
        fonts.issues(),
        [FontIssue::UppercaseFileName {
            file: PathBuf::from("resource/fonts/Upper.ttf"),
        }]
    );
}

#[test]
fn a_win32_font_is_read_on_linux() {
    let tmp = tempfile::tempdir().unwrap();
    let hud = tmp.path().join("hud");
    common::write_file(
        &hud.join("resource/clientscheme.res"),
        br#""Scheme" { "Fonts" { "P" { "1" { "name" "Pc Font" [$WIN32] } } }
"CustomFontFiles" { "1" "resource/fonts/pc.ttf" [$WIN32] } }"#,
    );
    common::write_file(
        &hud.join("resource/fonts/pc.ttf"),
        &common::sfnt::sfnt_bytes(&[(1, "Pc Font")], false),
    );
    let fonts = scan_hud_fonts(&hud, Platform::Linux).unwrap();
    assert_eq!(summary(&fonts), [entry("Pc Font", "resource/fonts/pc.ttf")]);
}

#[test]
fn a_base_override_changes_the_reported_font() {
    let tmp = tempfile::tempdir().unwrap();
    let hud = tmp.path().join("hud");
    common::write_file(
        &hud.join("resource/clientscheme.res"),
        br#"#base "scheme/base.res"
"Scheme" { "Fonts" { "Default" { "1" { "name" "Own Font" } } }
"CustomFontFiles" { "1" "resource/fonts/own.ttf" } }"#,
    );
    common::write_file(
        &hud.join("resource/scheme/base.res"),
        br#""Scheme" { "Fonts" { "Default" { "1" { "name" "Base Font" } } }
"CustomFontFiles" { "1" "resource/fonts/base.ttf" } }"#,
    );
    for (file, name) in [("own.ttf", "Own Font"), ("base.ttf", "Base Font")] {
        common::write_file(
            &hud.join("resource/fonts").join(file),
            &common::sfnt::sfnt_bytes(&[(1, name)], false),
        );
    }
    let fonts = scan_hud_fonts(&hud, Platform::Linux).unwrap();
    assert_eq!(
        summary(&fonts),
        [entry("Own Font", "resource/fonts/own.ttf")]
    );
}

#[test]
fn a_base_font_overridden_under_another_case_is_not_reported() {
    let tmp = tempfile::tempdir().unwrap();
    let hud = tmp.path().join("hud");
    common::write_file(
        &hud.join("resource/clientscheme.res"),
        br#"#base "scheme/base.res"
"Scheme" { "fonts" { "default" { "1" { "name" "Own Font" } } }
"CustomFontFiles" { "1" "resource/fonts/own.ttf" } }"#,
    );
    common::write_file(
        &hud.join("resource/scheme/base.res"),
        br#""Scheme" { "Fonts" { "Default" { "1" { "name" "Base Font" } } }
"CustomFontFiles" { "2" "resource/fonts/base.ttf" } }"#,
    );
    for (file, name) in [("own.ttf", "Own Font"), ("base.ttf", "Base Font")] {
        common::write_file(
            &hud.join("resource/fonts").join(file),
            &common::sfnt::sfnt_bytes(&[(1, name)], false),
        );
    }
    let fonts = scan_hud_fonts(&hud, Platform::Linux).unwrap();
    assert_eq!(
        summary(&fonts),
        [entry("Own Font", "resource/fonts/own.ttf")]
    );
}
