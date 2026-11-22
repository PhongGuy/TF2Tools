# Vendored vbsp 0.9.2

A copy of `vbsp` 0.9.2 from crates.io (codeberg.org/icewind/vbsp, MIT, see `LICENSE`), used through
`[patch.crates-io]` in the workspace `Cargo.toml` and in `tools/tf2tools-pilot/Cargo.toml`.

## The change

vbsp 0.9.2 pulls `zip-lzma` with its `lzma` feature, which brings in the `lzma` crate (0.2.2).
That crate is WTFPL, a licence that is not in `about.toml`. vbsp uses zip only to read the
pakfile lump, which TF2Tools never reads, so this copy drops the feature in `Cargo.toml`. The
examples, bench, dev-dependencies and `[profile.dev]` block are dropped too. Nothing in `src`
changes.

## Re-syncing

Copy the new upstream version from `~/.cargo/registry/src/*/vbsp-<ver>`, keep `src` and
`LICENSE`, reapply the manifest change above and update `crates/tf2tools-core/tests/vendor_vbsp.rs`.
Drop this directory and both patch entries once upstream stops requiring the `lzma` feature.
