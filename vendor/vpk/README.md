# Vendored vpk 0.3.0

A copy of `vpk` 0.3.0 from crates.io (roman901/vpk-rs, MIT, see `LICENSE`), used through
`[patch.crates-io]` in the workspace `Cargo.toml` and in `tools/tf2tools-pilot/Cargo.toml`.

## The change

vpk 0.3.0 reads its four fixed-size headers with binrw 0.14, which a future Rust release
rejects (E0365, rust-lang/rust#127909), and it cannot take binrw 0.15. This copy reads them
with `from_le_bytes` instead (`src/structs.rs`, plus `VPKDirectoryEntry` in `src/entry.rs`),
so binrw and its derive stack leave the build. A short read is now `Error::ReadError`; the
`Error::BinReadError` variant is gone. `VPKEntry::get` spells out its elided lifetime, which
a newer compiler warns about, and the benches and their dev-dependencies are dropped.

## Re-syncing

Copy the new upstream version from `~/.cargo/registry/src/*/vpk-<ver>`, keep `src` and
`LICENSE`, reapply the change above and update `crates/tf2tools-core/tests/vendor_vpk.rs`.
Drop this directory and both patch entries once upstream stops using binrw 0.14.
