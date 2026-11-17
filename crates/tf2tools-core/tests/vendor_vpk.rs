//! The VPK reader is the vendored `vpk` copy, which reads its headers without binrw.

use std::path::{Path, PathBuf};

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// The `[[package]]` blocks of Cargo.lock whose name is exactly `name`.
fn lock_blocks(name: &str) -> Vec<String> {
    let lock = std::fs::read_to_string(repo().join("Cargo.lock")).unwrap();
    let line = format!("name = \"{name}\"");
    lock.split("[[package]]")
        .filter(|block| block.lines().any(|l| l.trim() == line))
        .map(str::to_owned)
        .collect()
}

#[test]
fn vpk_resolves_to_the_vendored_copy() {
    let blocks = lock_blocks("vpk");
    assert_eq!(blocks.len(), 1, "{blocks:?}");
    // A path package has no source line; a registry package names crates.io.
    assert!(!blocks[0].contains("source ="), "{}", blocks[0]);
    let vendor = repo().join("vendor/vpk");
    assert!(vendor.join("README.md").is_file());
    assert!(vendor.join("LICENSE").is_file());
}

#[test]
fn binrw_is_gone_from_the_build() {
    for name in ["binrw", "binrw_derive"] {
        assert_eq!(lock_blocks(name), Vec::<String>::new(), "{name}");
    }
}
