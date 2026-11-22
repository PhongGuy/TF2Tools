//! The VPK reader is the vendored `vpk` copy, which reads its headers without binrw.

use std::path::{Path, PathBuf};

pub(crate) fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// The `[[package]]` blocks of Cargo.lock whose name is exactly `name`.
pub(crate) fn lock_blocks(name: &str) -> Vec<String> {
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

/// binrw 0.14 is the release a future Rust rejects (E0365); vbsp needs binrw 0.15, which builds.
#[test]
fn every_locked_binrw_is_0_15() {
    for name in ["binrw", "binrw_derive"] {
        let old: Vec<String> = lock_blocks(name)
            .into_iter()
            .filter(|block| {
                !block
                    .lines()
                    .any(|l| l.trim().starts_with("version = \"0.15."))
            })
            .collect();
        assert_eq!(old, Vec::<String>::new(), "{name}");
    }
}
