//! The repository layout after the Svelte/Tauri tree is removed and its assets moved.

use std::path::{Path, PathBuf};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn count_with_extension(dir: &Path, ext: &str) -> usize {
    std::fs::read_dir(dir)
        .unwrap_or_else(|e| panic!("read {}: {e}", dir.display()))
        .filter_map(Result::ok)
        .filter(|entry| entry.path().extension().is_some_and(|x| x == ext))
        .count()
}

#[test]
fn crosshair_assets_live_under_assets() {
    let crosshairs = repo_root().join("assets/crosshairs");
    assert_eq!(count_with_extension(&crosshairs.join("vtf"), "vtf"), 405);
    assert_eq!(count_with_extension(&crosshairs.join("vmt"), "vmt"), 402);
    assert_eq!(count_with_extension(&crosshairs.join("png"), "png"), 422);
    assert!(crosshairs.join("crosshairs.json").is_file());
}

#[test]
fn app_assets_live_under_assets() {
    let assets = repo_root().join("assets");
    assert_eq!(count_with_extension(&assets.join("backgrounds"), "png"), 3);
    assert_eq!(count_with_extension(&assets.join("weapon-icons"), "png"), 4);
}

#[test]
fn svelte_and_tauri_tree_is_gone() {
    let root = repo_root();
    for gone in [
        "src",
        "src-tauri",
        "static",
        "package.json",
        "bun.lock",
        "components.json",
        "svelte.config.js",
        "tsconfig.json",
        "vite.config.ts",
        ".github/workflows/test-build.yml",
    ] {
        assert!(!root.join(gone).exists(), "{gone} should be deleted");
    }
}

#[test]
fn v5_timing_tools_are_gone() {
    let root = repo_root();
    for gone in [
        "tools/v5-store-driver",
        "crates/tf2tools-core/examples/v5_timing.rs",
    ] {
        assert!(!root.join(gone).exists(), "{gone} should be deleted");
    }
    let manifest = std::fs::read_to_string(root.join("crates/tf2tools-core/Cargo.toml")).unwrap();
    assert!(!manifest.contains("v5_timing"), "{manifest}");
}
