//! The workspace's Cargo profiles: release is tuned for size,
//! and nothing that builds the app aborts on panic, since the archive and pak guards rely on
//! `catch_unwind`.

use std::path::{Path, PathBuf};

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// One manifest or config line with its comment and all whitespace removed.
fn squeeze(line: &str) -> String {
    let code = line.split('#').next().unwrap_or_default();
    code.chars().filter(|c| !c.is_whitespace()).collect()
}

/// The squeezed, unquoted, lowercased lines of `text` that make panics abort: `panic = "abort"`
/// as a TOML key, dotted key, inline table or `--config` value, `-C panic=abort` in any
/// RUSTFLAGS spelling, and `CARGO_PROFILE_<NAME>_PANIC=abort` as a shell or YAML setting.
/// A comment is cut at the first `#`, so a `#` earlier on the line hides the rest of it.
fn panic_abort_lines(text: &str) -> Vec<String> {
    text.lines()
        .map(|l| squeeze(l).replace(['"', '\''], "").to_lowercase())
        .filter(|l| l.contains("panic=abort") || l.contains("panic:abort"))
        .collect()
}

/// Every file that can set a build's panic strategy, sorted: the manifests, cargo's config,
/// the Makefile, and every file under `scripts/` and `.github/workflows/`.
fn guarded_files(root: &Path) -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = [
        "Cargo.toml",
        "crates/tf2tools-core/Cargo.toml",
        "crates/tf2tools-app/Cargo.toml",
        ".cargo/config.toml",
        ".cargo/config",
        "Makefile",
    ]
    .iter()
    .map(|rel| root.join(rel))
    .filter(|p| p.is_file())
    .collect();
    walk(&root.join("scripts"), &mut files);
    walk(&root.join(".github/workflows"), &mut files);
    files.sort();
    files
}

/// Pushes every file under `dir`, recursively; a missing `dir` adds nothing.
fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            walk(&path, out);
        } else {
            out.push(path);
        }
    }
}

/// `<path relative to root, with />: <squeezed line>` for every panic-abort setting in
/// `guarded_files(root)`, in file order.
fn abort_settings(root: &Path) -> Vec<String> {
    let mut hits = Vec::new();
    for path in guarded_files(root) {
        let text = String::from_utf8_lossy(&std::fs::read(&path).unwrap()).into_owned();
        let rel = path
            .strip_prefix(root)
            .unwrap_or(&path)
            .to_string_lossy()
            .replace('\\', "/");
        hits.extend(
            panic_abort_lines(&text)
                .into_iter()
                .map(|l| format!("{rel}: {l}")),
        );
    }
    hits
}

/// The squeezed, non-empty lines of `[name]` up to the next header, or None without it.
fn section(text: &str, name: &str) -> Option<Vec<String>> {
    let header = format!("[{name}]");
    let mut lines = text.lines().map(str::trim);
    lines.by_ref().find(|l| *l == header)?;
    Some(
        lines
            .take_while(|l| !l.starts_with('['))
            .map(squeeze)
            .filter(|l| !l.is_empty())
            .collect(),
    )
}

#[test]
fn the_detector_sees_every_form_of_panic_abort() {
    let text = r#"
[profile.release]
panic = "abort"
[profile.dev]
panic='abort' # trailing comment
profile.timing.panic = "abort"
x = { panic = "abort" }
# panic = "abort" in a comment does not count
[profile.ok]
panic = "unwind"
rustflags = ["-C", "panic=abort"]
RUSTFLAGS="-Cpanic=abort" cargo build
CARGO_ENCODED_RUSTFLAGS=$'-C\x1fpanic=abort'
CARGO_PROFILE_RELEASE_PANIC=abort
  CARGO_PROFILE_DIST_PANIC: "abort"
RUSTFLAGS="-C panic=unwind"
"#;
    let hits = panic_abort_lines(text);
    assert_eq!(hits.len(), 9, "{hits:?}");
}

#[test]
fn the_section_reader_stops_at_the_next_header() {
    let text = "[a]\nx = 1 # note\n\n[b]\ny = 2\n";
    assert_eq!(section(text, "a"), Some(vec!["x=1".to_string()]));
    assert_eq!(section(text, "b"), Some(vec!["y=2".to_string()]));
    assert_eq!(section(text, "c"), None);
}

#[test]
fn nothing_in_the_build_aborts_on_panic() {
    let root = repo();
    let files = guarded_files(&root);
    for want in [
        "Cargo.toml",
        "crates/tf2tools-core/Cargo.toml",
        "crates/tf2tools-app/Cargo.toml",
        "Makefile",
        ".github/workflows/ci.yml",
        ".github/workflows/release.yml",
        "scripts/deb-prerelease.sh",
    ] {
        assert!(
            files.contains(&root.join(want)),
            "{want} is not guarded: {files:?}"
        );
    }
    let hits = abort_settings(&root);
    assert!(hits.is_empty(), "panics abort in: {hits:?}");
}

#[test]
fn a_planted_setting_anywhere_in_the_build_fails_the_guard() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    let plant = |rel: &str, text: &str| {
        let path = root.join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    };
    plant("Cargo.toml", "[profile.release]\nlto = \"thin\"\n");
    plant(
        "scripts/release/pack.sh",
        "#!/bin/sh\nexport RUSTFLAGS=\"-C panic=abort\"\ncargo build --release\n",
    );
    plant(
        ".github/workflows/release.yml",
        "env:\n  CARGO_PROFILE_RELEASE_PANIC: abort\n",
    );
    plant(
        "Makefile",
        "package:\n\tcargo build --release --config 'profile.release.panic=\"abort\"'\n",
    );
    plant(
        "scripts/clean.sh",
        "# RUSTFLAGS=\"-C panic=abort\" is never set here\ncargo build\n",
    );
    assert_eq!(
        abort_settings(root),
        [
            ".github/workflows/release.yml: cargo_profile_release_panic:abort",
            "Makefile: cargobuild--release--configprofile.release.panic=abort",
            "scripts/release/pack.sh: exportrustflags=-cpanic=abort",
        ]
    );
}

#[test]
fn the_release_profile_is_thin_lto_one_codegen_unit_and_stripped() {
    let text = std::fs::read_to_string(repo().join("Cargo.toml")).unwrap();
    let release = section(&text, "profile.release").expect("[profile.release] in Cargo.toml");
    for want in ["lto=\"thin\"", "codegen-units=1", "strip=true"] {
        assert!(
            release.iter().any(|l| l == want),
            "missing {want}: {release:?}"
        );
    }
    assert!(
        !release.iter().any(|l| l.starts_with("panic=")),
        "{release:?}"
    );
}

/// The GPUI tests crawl on unoptimised dependencies, so dev and test builds optimise every
/// crate outside the workspace while the workspace itself stays quick to rebuild.
#[test]
fn dev_builds_optimise_every_dependency() {
    let manifest = std::fs::read_to_string(repo().join("Cargo.toml")).unwrap();
    let section: Vec<String> = manifest
        .lines()
        .skip_while(|l| l.trim() != r#"[profile.dev.package."*"]"#)
        .skip(1)
        .take_while(|l| !l.trim_start().starts_with('['))
        .map(squeeze)
        .collect();
    assert!(
        section.iter().any(|l| l == "opt-level=2"),
        "Cargo.toml needs [profile.dev.package.\"*\"] with opt-level = 2: {section:?}"
    );
}
