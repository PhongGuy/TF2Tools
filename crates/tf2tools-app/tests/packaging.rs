//! The packaging inputs: v5's icon set, the desktop entry and the cargo-packager metadata.

use std::fs;
use std::path::{Path, PathBuf};

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// Width and height from a PNG's IHDR chunk, which always follows the 8-byte signature.
fn png_size(path: &Path) -> (u32, u32) {
    let bytes = fs::read(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    assert_eq!(
        &bytes[..8],
        b"\x89PNG\r\n\x1a\n",
        "{} is not a PNG",
        path.display()
    );
    let be = |at: usize| u32::from_be_bytes(bytes[at..at + 4].try_into().unwrap());
    (be(16), be(20))
}

#[test]
fn the_png_icons_have_their_named_sizes() {
    let icons = repo().join("packaging/icons");
    for (file, side) in [
        ("32x32.png", 32),
        ("128x128.png", 128),
        ("128x128@2x.png", 256),
        ("icon.png", 512),
    ] {
        assert_eq!(png_size(&icons.join(file)), (side, side), "{file}");
    }
}

#[test]
fn the_windows_and_mac_icons_are_present_and_well_formed() {
    let icons = repo().join("packaging/icons");
    let ico = fs::read(icons.join("icon.ico")).expect("packaging/icons/icon.ico");
    ico_images(&ico).unwrap_or_else(|e| panic!("icon.ico: {e}"));
    let icns = fs::read(icons.join("icon.icns")).expect("packaging/icons/icon.icns");
    assert_eq!(&icns[..4], b"icns", "icon.icns magic");
    let declared = u32::from_be_bytes(icns[4..8].try_into().unwrap()) as usize;
    assert_eq!(declared, icns.len(), "icon.icns length field");
}

/// The image count of an ICO file, once its header, directory and every image it lists are
/// inside `ico`; otherwise what is out of bounds.
fn ico_images(ico: &[u8]) -> Result<usize, String> {
    if ico.len() < 6 || ico[..4] != [0, 0, 1, 0] {
        return Err("no ICO header".into());
    }
    let count = usize::from(u16::from_le_bytes([ico[4], ico[5]]));
    if count == 0 {
        return Err("holds no images".into());
    }
    let dir_end = 6 + 16 * count;
    if dir_end > ico.len() {
        return Err(format!(
            "{count} entries end at {dir_end}, past {}",
            ico.len()
        ));
    }
    for i in 0..count {
        let le = |at: usize| {
            let at = 6 + 16 * i + at;
            u32::from_le_bytes(ico[at..at + 4].try_into().unwrap()) as usize
        };
        let (size, offset) = (le(8), le(12));
        let end = offset
            .checked_add(size)
            .ok_or_else(|| format!("entry {i} overflows"))?;
        if offset < dir_end || end > ico.len() {
            return Err(format!(
                "entry {i}: bytes {offset}..{end} outside {dir_end}..{}",
                ico.len()
            ));
        }
    }
    Ok(count)
}

#[test]
fn a_truncated_icon_fails_the_ico_check() {
    let ico = fs::read(repo().join("packaging/icons/icon.ico")).unwrap();
    assert!(ico_images(&ico).is_ok(), "{:?}", ico_images(&ico));
    // The last image cut by one byte, every image gone, the directory cut mid-entry.
    for len in [ico.len() - 1, 102, 50] {
        assert!(
            ico_images(&ico[..len]).is_err(),
            "a copy cut to {len} bytes passed"
        );
    }
}

/// The `key=value` lines of the desktop entry after its `[Desktop Entry]` header.
fn desktop_entry() -> Vec<(String, String)> {
    let text = fs::read_to_string(repo().join("packaging/com.tf2tools.app.desktop"))
        .expect("packaging/com.tf2tools.app.desktop");
    let mut lines = text
        .lines()
        .filter(|l| !l.trim().is_empty() && !l.starts_with('#'));
    assert_eq!(lines.next(), Some("[Desktop Entry]"));
    lines
        .map(|l| {
            let (k, v) = l
                .split_once('=')
                .unwrap_or_else(|| panic!("not a key: {l}"));
            (k.to_string(), v.to_string())
        })
        .collect()
}

/// The app manifest from `[package.metadata.packager]` to the end of the file.
fn packager_metadata() -> String {
    let text =
        fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml")).unwrap();
    let at = text
        .find("[package.metadata.packager]")
        .expect("[package.metadata.packager] in the app's Cargo.toml");
    text[at..].to_string()
}

#[test]
fn the_desktop_entry_matches_the_window_app_id() {
    let entry = desktop_entry();
    let mut keys: Vec<&str> = entry.iter().map(|(k, _)| k.as_str()).collect();
    let total = keys.len();
    keys.sort_unstable();
    keys.dedup();
    assert_eq!(keys.len(), total, "duplicate key in {entry:?}");
    for (key, want) in [
        ("Type", "Application"),
        ("Name", "TF2Tools"),
        ("Exec", "tf2tools"),
        ("Icon", "tf2tools"),
        ("Terminal", "false"),
        ("Categories", "Game;"),
        ("StartupWMClass", tf2tools_app::app::APP_ID),
    ] {
        let got = entry
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.as_str());
        assert_eq!(got, Some(want), "{key}");
    }
}

#[test]
fn the_packager_metadata_names_the_app_and_its_formats() {
    let meta = packager_metadata();
    for want in [
        "product-name = \"TF2Tools\"",
        "identifier = \"com.tf2tools.app\"",
        "formats = [\"deb\", \"appimage\"]",
        "generate-desktop-entry = false",
        "package-name = \"tf2tools\"",
        "resources = [{ src = \"../../assets\", target = \"assets\" }]",
    ] {
        assert!(meta.lines().any(|l| l.trim() == want), "missing `{want}`");
    }
}

#[test]
fn the_desktop_entry_ships_under_both_package_names() {
    let meta = packager_metadata();
    let src = "\"../../packaging/com.tf2tools.app.desktop\"";
    for target in [
        "usr/share/applications/com.tf2tools.app.desktop",
        "usr/share/applications/tf2tools.desktop",
    ] {
        let want = format!("{src} = \"{target}\"");
        assert!(meta.contains(&want), "missing `{want}`");
    }
}

#[test]
fn every_path_the_packager_metadata_names_exists() {
    let crate_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let meta = packager_metadata();
    let paths: Vec<&str> = meta
        .split('"')
        .skip(1)
        .step_by(2)
        .filter(|s| s.starts_with("../../"))
        .collect();
    assert!(
        paths.len() >= 6,
        "expected the assets, three icons and two desktop sources: {paths:?}"
    );
    for p in paths {
        assert!(crate_dir.join(p).exists(), "{p} does not exist");
    }
}

#[test]
fn the_deb_names_alsa_by_its_24_04_package_first() {
    // Reversed, apt on 24.04 satisfies the virtual libasound2 with the OSS shim instead of ALSA.
    let meta = packager_metadata();
    let depends = meta
        .lines()
        .find_map(|l| l.trim().strip_prefix("depends = ["))
        .expect("deb depends in the packager metadata");
    assert!(
        depends.starts_with("\"libasound2t64 | libasound2\","),
        "deb depends must start with `libasound2t64 | libasound2`: [{depends}"
    );
}

#[test]
fn the_nsis_installer_uses_the_ico_and_installs_per_user() {
    let meta = packager_metadata();
    let nsis = meta
        .split("[package.metadata.packager.nsis]")
        .nth(1)
        .expect("[package.metadata.packager.nsis]");
    let nsis = nsis.split("\n[").next().unwrap();
    for want in [
        "installer-icon = \"../../packaging/icons/icon.ico\"",
        "installer-mode = \"currentUser\"",
    ] {
        assert!(nsis.lines().any(|l| l.trim() == want), "missing `{want}`");
    }
}

#[test]
fn the_exe_icon_comes_from_the_shared_ico() {
    let crate_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let rc =
        fs::read_to_string(crate_dir.join("windows/tf2tools.rc")).expect("windows/tf2tools.rc");
    assert!(
        rc.lines().any(|l| l.trim() == "1 ICON \"icon.ico\""),
        "{rc}"
    );
    let build = fs::read_to_string(crate_dir.join("build.rs")).expect("build.rs");
    assert!(
        build.contains("CARGO_CFG_TARGET_OS"),
        "the target, not the host, decides"
    );
    assert!(build.contains("\"../../packaging/icons\""), "{build}");
    assert!(crate_dir.join("../../packaging/icons/icon.ico").is_file());
}

#[test]
fn release_builds_on_windows_open_no_console() {
    let main =
        fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("src/main.rs")).unwrap();
    assert!(
        main.contains(
            "#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = \"windows\")]"
        ),
        "{main}"
    );
}

/// The first image of `icon.ico`, from its directory entry (size at byte 14, offset at 18).
fn first_ico_image() -> Vec<u8> {
    let ico = fs::read(repo().join("packaging/icons/icon.ico")).unwrap();
    let le = |at: usize| u32::from_le_bytes(ico[at..at + 4].try_into().unwrap()) as usize;
    let (size, offset) = (le(14), le(18));
    ico[offset..offset + size].to_vec()
}

// Run by hand or in the release job: TF2TOOLS_WINDOWS_EXE=<path to tf2tools.exe> cargo test
// -p tf2tools-app --test packaging -- --ignored. The resource compiler copies icon images verbatim.
#[test]
#[ignore = "needs a Windows build named by TF2TOOLS_WINDOWS_EXE"]
fn a_windows_exe_carries_the_icon() {
    let exe = std::env::var_os("TF2TOOLS_WINDOWS_EXE").expect("set TF2TOOLS_WINDOWS_EXE");
    let exe = fs::read(&exe).unwrap_or_else(|e| panic!("read {}: {e}", exe.display()));
    assert_eq!(&exe[..2], b"MZ", "not a Windows executable");
    let image = first_ico_image();
    assert!(
        image.len() > 64,
        "icon.ico's first image is {} bytes",
        image.len()
    );
    let found = exe
        .windows(64)
        .enumerate()
        .filter(|(_, w)| *w == &image[..64])
        .any(|(at, _)| exe.get(at..at + image.len()) == Some(&image[..]));
    assert!(found, "tf2tools.exe does not hold icon.ico's first image");
}

/// The keys cargo-packager 0.11.8 accepts per table, copied from `src/config/mod.rs`
/// (`Config`, `LinuxConfig`, `DebianConfig`, `AppImageConfig`, `NsisConfig`): each struct is
/// camelCase with `deny_unknown_fields`, so a misspelt key fails the whole packaging run.
#[rustfmt::skip]
const PACKAGER_KEYS: &[(&str, &[&str])] = &[
    (
        "",
        &[
            "productName", "product-name", "product_name", "version", "binaries", "identifier",
            "beforePackagingCommand", "before-packaging-command", "before_packaging_command",
            "beforeEachPackageCommand", "before-each-package-command",
            "before_each_package_command", "logLevel", "log-level", "log_level", "formats",
            "outDir", "out-dir", "out_dir", "binariesDir", "binaries-dir", "binaries_dir",
            "targetTriple", "target-triple", "target_triple", "description", "longDescription",
            "long-description", "long_description", "homepage", "authors", "publisher",
            "licenseFile", "license-file", "license_file", "copyright", "category", "icons",
            "fileAssociations", "file-associations", "file_associations", "deepLinkProtocols",
            "deep-link-protocols", "deep_link_protocols", "resources", "externalBinaries",
            "external-binaries", "external_binaries",
        ],
    ),
    (
        "linux",
        &[
            "generateDesktopEntry", "generate-desktop-entry", "generate_desktop_entry",
        ],
    ),
    (
        "deb",
        &[
            "depends", "desktopTemplate", "desktop-template", "desktop_template", "section",
            "priority", "files", "packageName", "package-name", "package_name",
        ],
    ),
    (
        "appimage",
        &[
            "libs", "bins", "files", "linuxdeployPlugins", "linuxdeploy-plugins",
            "linuxdeploy_plugins", "excludedLibs", "excluded-libraries", "excluded_libraries",
        ],
    ),
    (
        "nsis",
        &[
            "compression", "template", "preinstallSection", "preinstall-section",
            "preinstall_section", "headerImage", "header-image", "header_image", "installerIcon",
            "installer-icon", "installer_icon", "installMode", "installer-mode", "installer_mode",
            "languages", "customLanguageFiles", "custom-language-file", "custom_language_file",
            "displayLanguageSelector", "display-language-selector", "display_language_selector",
            "appdataPaths", "appdata-paths", "appdata_paths",
        ],
    ),
];

#[test]
fn every_packager_key_is_one_cargo_packager_reads() {
    // Walk the tables line by line: a header picks the allow-list, and an unindented
    // `key =` line is a key of that table (indented lines continue a multi-line value).
    let mut table = None;
    for line in packager_metadata().lines() {
        if let Some(name) = line.strip_prefix("[package.metadata.packager") {
            let name = name.trim_end_matches(']').trim_start_matches('.');
            let keys = PACKAGER_KEYS.iter().find(|(t, _)| *t == name);
            table = Some(keys.unwrap_or_else(|| panic!("unchecked packager table `{name}`")));
        } else if line.starts_with('[') {
            break;
        } else if let Some((key, _)) = line.split_once(" = ")
            && !line.starts_with([' ', '#'])
        {
            let (name, keys) = table.expect("a key before any packager table");
            assert!(
                keys.contains(&key),
                "cargo-packager rejects `{key}` in `{name}`"
            );
        }
    }
}
