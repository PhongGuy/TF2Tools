//! The HUD font check on a real Windows machine: the system font scan, then installing one font
//! for the current user through `RegistryHost` and removing it again. Run only by
//! `.github/workflows/windows-checks.yml`, since it changes the machine it runs on.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use tf2tools_core::{
    Platform, check_hud_fonts, install_hud_fonts, scan_hud_fonts, system_font_host,
};

use crate::common::write_file;

const FONTS_KEY: &str = r"Software\Microsoft\Windows NT\CurrentVersion\Fonts";
const FONT_FILE: &str = "Inter-Regular.ttf";
const SCHEME: &str = r#"
"Scheme"
{
    "Fonts" { "Main" { "1" { "name" "Inter" "tall" "12" } } }
    "CustomFontFiles" { "1" "resource/fonts/Inter-Regular.ttf" }
}
"#;

/// Fails unless the run opted in, so `--ignored` alone never touches a developer's fonts.
fn require_opt_in() {
    assert_eq!(
        std::env::var("TF2TOOLS_REAL_WINDOWS").as_deref(),
        Ok("1"),
        "set TF2TOOLS_REAL_WINDOWS=1 to let this test change the machine's fonts"
    );
}

fn timed<T>(what: &str, run: impl FnOnce() -> T) -> (T, Duration) {
    let started = Instant::now();
    let out = run();
    let took = started.elapsed();
    println!("TIMING {what}: {} ms", took.as_millis());
    (out, took)
}

#[test]
#[ignore = "changes the real machine; run by windows-checks.yml"]
fn a_cold_system_font_scan_reads_the_real_registry() {
    require_opt_in();
    let host = system_font_host().expect("Windows has a font host");
    // Named to run first, so round 1 meets a cold file cache. The folder's size, read from
    // metadata alone, shows how much a scan of whole files would have cost.
    let fonts_dir = std::env::var_os("WINDIR")
        .map(PathBuf::from)
        .unwrap()
        .join("Fonts");
    let (files, bytes) = std::fs::read_dir(&fonts_dir)
        .unwrap()
        .flatten()
        .filter_map(|e| e.metadata().ok())
        .filter(|m| m.is_file())
        .fold((0u64, 0u64), |(n, b), m| (n + 1, b + m.len()));
    println!(
        "system font dir {}: {files} files, {} MiB",
        fonts_dir.display(),
        bytes / (1024 * 1024)
    );
    for round in 1..=3 {
        let (names, _) = timed(&format!("system font scan round {round}"), || {
            host.installed_names().expect("the font registry reads")
        });
        println!("system font scan round {round}: {} names", names.len());
        assert!(
            names.iter().any(|n| n.eq_ignore_ascii_case("Arial")),
            "Arial is on every Windows install"
        );
    }
}

#[test]
#[ignore = "changes the real machine; run by windows-checks.yml"]
fn a_missing_hud_font_installs_registers_and_is_seen() {
    require_opt_in();
    let tmp = tempfile::tempdir().unwrap();
    let hud = tmp.path().join("myhud");
    let font = std::fs::read(inter_font()).unwrap();
    write_file(&hud.join("resource/clientscheme.res"), SCHEME.as_bytes());
    write_file(&hud.join("resource/fonts").join(FONT_FILE), &font);

    let host = system_font_host().unwrap();
    let dest = host.fonts_dir().unwrap().join(FONT_FILE);
    let cleanup = Cleanup(dest.clone());
    let fonts = scan_hud_fonts(&hud, Platform::Windows).unwrap();
    let (before, _) = timed("check before install", || {
        check_hud_fonts(&fonts, host.as_ref()).unwrap()
    });
    let missing: Vec<&str> = before.missing().iter().map(|f| f.name()).collect();
    assert_eq!(missing, ["Inter"], "the runner must not have Inter already");

    let (report, _) = timed("install and register", || {
        install_hud_fonts(&hud, before.missing(), host.as_ref()).unwrap()
    });
    assert_eq!(
        report.installed(),
        ["resource/fonts/Inter-Regular.ttf"],
        "the one shipped file installs"
    );
    assert!(report.skipped().is_empty(), "{:?}", report.skipped());
    assert_eq!(std::fs::read(&dest).unwrap(), font, "copied byte for byte");

    let value = user_font_value("Inter Regular (TrueType)");
    println!("HKCU Fonts \"Inter Regular (TrueType)\" = {value:?}");
    assert_eq!(
        value.as_deref().map(Path::new),
        Some(dest.as_path()),
        "the HKCU value points at the copy"
    );

    // A fresh host reads the registry again, as the app does on its next check.
    let fresh = system_font_host().unwrap();
    let (after, _) = timed("check after install", || {
        check_hud_fonts(&fonts, fresh.as_ref()).unwrap()
    });
    assert!(after.missing().is_empty(), "{:?}", after.missing());
    assert_eq!(gdi::selected_face("Inter"), "Inter", "GDI picks the font");

    drop(cleanup);
    assert!(!dest.exists(), "cleanup left {}", dest.display());
    assert_eq!(
        user_font_value("Inter Regular (TrueType)"),
        None,
        "cleanup left the registry value"
    );
}

/// The app's own bundled Inter, a real TrueType file GDI accepts.
fn inter_font() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../tf2tools-app/assets/fonts")
        .join(FONT_FILE)
}

fn user_font_value(name: &str) -> Option<String> {
    use winreg::RegKey;
    use winreg::enums::HKEY_CURRENT_USER;
    RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey(FONTS_KEY)
        .ok()?
        .get_value::<String, _>(name)
        .ok()
}

/// Undoes the install even when an assertion fails: unloads the font, deletes the file and
/// every HKCU `Fonts` value pointing at it.
struct Cleanup(PathBuf);

impl Drop for Cleanup {
    fn drop(&mut self) {
        use winreg::RegKey;
        use winreg::enums::{HKEY_CURRENT_USER, KEY_READ, KEY_SET_VALUE};
        use winreg::types::FromRegValue;
        gdi::remove_font_resource(&self.0);
        let _ = std::fs::remove_file(&self.0);
        let Ok(key) = RegKey::predef(HKEY_CURRENT_USER)
            .open_subkey_with_flags(FONTS_KEY, KEY_READ | KEY_SET_VALUE)
        else {
            return;
        };
        let ours: Vec<String> = key
            .enum_values()
            .filter_map(Result::ok)
            .filter(|(_, data)| {
                String::from_reg_value(data)
                    .is_ok_and(|text| text.eq_ignore_ascii_case(&self.0.to_string_lossy()))
            })
            .map(|(name, _)| name)
            .collect();
        for name in ours {
            let _ = key.delete_value(&name);
        }
    }
}

#[allow(unsafe_code)]
mod gdi {
    use std::os::windows::ffi::OsStrExt;
    use std::path::Path;

    use windows_sys::Win32::Graphics::Gdi::{
        CLIP_DEFAULT_PRECIS, CreateFontW, DEFAULT_CHARSET, DEFAULT_QUALITY, DeleteObject,
        FW_NORMAL, GetDC, GetTextFaceW, OUT_DEFAULT_PRECIS, ReleaseDC, RemoveFontResourceW,
        SelectObject,
    };

    /// The face GDI actually selects when asked for `face`; an unknown face falls back to
    /// another, so equality proves the font is loaded.
    pub(super) fn selected_face(face: &str) -> String {
        let wide: Vec<u16> = face.encode_utf16().chain([0]).collect();
        let mut buf = [0u16; 64];
        // SAFETY: the DC and font are released before returning; `wide` and `buf` outlive
        // every call that reads or writes them.
        let copied = unsafe {
            let dc = GetDC(std::ptr::null_mut());
            let font = CreateFontW(
                0,
                0,
                0,
                0,
                FW_NORMAL as i32,
                0,
                0,
                0,
                u32::from(DEFAULT_CHARSET),
                u32::from(OUT_DEFAULT_PRECIS),
                u32::from(CLIP_DEFAULT_PRECIS),
                u32::from(DEFAULT_QUALITY),
                0,
                wide.as_ptr(),
            );
            let old = SelectObject(dc, font);
            let copied = GetTextFaceW(dc, buf.len() as i32, buf.as_mut_ptr());
            SelectObject(dc, old);
            DeleteObject(font);
            ReleaseDC(std::ptr::null_mut(), dc);
            copied
        };
        assert!(copied > 0, "GetTextFaceW failed");
        let end = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
        String::from_utf16_lossy(&buf[..end])
    }

    pub(super) fn remove_font_resource(path: &Path) {
        let wide: Vec<u16> = path.as_os_str().encode_wide().chain([0]).collect();
        // SAFETY: `wide` is a NUL-terminated UTF-16 path that outlives the call.
        unsafe {
            RemoveFontResourceW(wide.as_ptr());
        }
    }
}
