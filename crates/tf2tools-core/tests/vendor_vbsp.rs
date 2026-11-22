//! The BSP reader is the vendored `vbsp` copy, built without zip-lzma's `lzma` feature, so the
//! WTFPL `lzma` crate never enters the build.

use crate::vendor_vpk::{lock_blocks, repo};

#[test]
fn vbsp_resolves_to_the_vendored_copy() {
    let blocks = lock_blocks("vbsp");
    assert_eq!(blocks.len(), 1, "{blocks:?}");
    // A path package has no source line; a registry package names crates.io.
    assert!(!blocks[0].contains("source ="), "{}", blocks[0]);
    let vendor = repo().join("vendor/vbsp");
    assert!(vendor.join("README.md").is_file());
    assert!(vendor.join("LICENSE").is_file());
}

#[test]
fn lzma_is_gone_from_the_build() {
    assert_eq!(lock_blocks("lzma"), Vec::<String>::new());
}

#[test]
fn the_vendored_zip_dependency_has_no_lzma_feature() {
    let manifest = std::fs::read_to_string(repo().join("vendor/vbsp/Cargo.toml")).unwrap();
    let zip = manifest
        .split("\n[")
        .find(|section| section.starts_with("dependencies.zip]"))
        .expect("a [dependencies.zip] section");
    assert!(zip.contains("package = \"zip-lzma\""), "{zip}");
    assert!(zip.contains("default-features = false"), "{zip}");
    assert!(!zip.contains("features = ["), "{zip}");
}

#[test]
fn the_pilot_workspace_patches_vbsp_too() {
    let pilot = std::fs::read_to_string(repo().join("tools/tf2tools-pilot/Cargo.toml")).unwrap();
    assert!(
        pilot
            .lines()
            .any(|l| l.trim() == r#"vbsp = { path = "../../vendor/vbsp" }"#),
        "{pilot}"
    );
}
