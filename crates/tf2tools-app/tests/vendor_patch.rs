//! The build must use the vendored gpui-pre-linux and gpui-pre-scheduler; a gpui-kit bump
//! that moves gpui-pre past 0.3.12 fails here until both are re-synced from the new upstream.

use std::path::PathBuf;

fn repo() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// The `[[package]]` blocks of Cargo.lock whose name is exactly `name`.
fn lock_blocks(name: &str) -> Vec<String> {
    lock_blocks_in("Cargo.lock", name)
}

/// The `[[package]]` blocks named `name` in the lock file at `lock`, relative to the repo.
fn lock_blocks_in(lock: &str, name: &str) -> Vec<String> {
    let lock = std::fs::read_to_string(repo().join(lock)).unwrap();
    let line = format!("name = \"{name}\"");
    lock.split("[[package]]")
        .filter(|block| block.lines().any(|l| l.trim() == line))
        .map(str::to_owned)
        .collect()
}

#[test]
fn the_build_uses_the_vendored_gpui_pre_crates_0_3_12() {
    for name in ["gpui-pre-linux", "gpui-pre-scheduler"] {
        let blocks = lock_blocks(name);
        assert_eq!(blocks.len(), 1, "{name}: {blocks:?}");
        let block = &blocks[0];
        assert!(block.contains("version = \"0.3.12\""), "{block}");
        // A path package has no source line; a registry package names crates.io.
        assert!(
            !block.contains("source ="),
            "resolved from a registry: {block}"
        );
    }
}

// The pilot harness is its own workspace, so it needs the same patches or pilot builds break.
#[test]
fn the_pilot_harness_uses_the_vendored_gpui_pre_crates_too() {
    for name in ["gpui-pre-linux", "gpui-pre-scheduler"] {
        let blocks = lock_blocks_in("tools/tf2tools-pilot/Cargo.lock", name);
        assert_eq!(blocks.len(), 1, "{name}: {blocks:?}");
        assert!(
            !blocks[0].contains("source ="),
            "pilot resolves {name} from a registry: {}",
            blocks[0]
        );
    }
}

#[test]
fn gpui_pre_is_still_the_vendored_base() {
    for name in ["gpui-pre", "gpui-pre-wgpu"] {
        let blocks = lock_blocks(name);
        assert_eq!(blocks.len(), 1, "{name}: {blocks:?}");
        assert!(
            blocks[0].contains("version = \"0.3.12\""),
            "{name}: {}",
            blocks[0]
        );
    }
}

#[test]
fn the_vendored_copy_carries_the_opaque_start() {
    let vendor = repo().join("vendor/gpui-pre-linux");
    let manifest = std::fs::read_to_string(vendor.join("Cargo.toml")).unwrap();
    assert!(manifest.contains("version = \"0.3.12\""));
    let window = std::fs::read_to_string(vendor.join("src/linux/wayland/window.rs")).unwrap();
    assert!(
        window.contains("transparent: starts_transparent(surface_state.decoration().is_some()),")
    );
    assert!(vendor.join("README.md").is_file());
    assert!(vendor.join("LICENSE-APACHE").is_file());
}

#[test]
fn the_vendored_scheduler_carries_the_lock_order_fix() {
    let vendor = repo().join("vendor/gpui-pre-scheduler");
    let manifest = std::fs::read_to_string(vendor.join("Cargo.toml")).unwrap();
    assert!(manifest.contains("version = \"0.3.12\""));
    let scheduler = std::fs::read_to_string(vendor.join("src/test_scheduler.rs")).unwrap();
    assert!(scheduler.contains("let timeout_ticks = self.state.lock().timeout_ticks.clone();"));
    assert!(vendor.join("README.md").is_file());
    assert!(vendor.join("LICENSE-APACHE").is_file());
}
