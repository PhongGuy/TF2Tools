# Vendored gpui-pre-scheduler 0.3.8

A copy of `gpui-pre-scheduler` 0.3.8 from crates.io (a snapshot of Zed's
`scheduler` crate, Apache-2.0, see `LICENSE-APACHE`), used through
`[patch.crates-io]` in the workspace `Cargo.toml`.

## Re-syncing

Copy the new upstream version from `~/.cargo/registry/src/*/gpui-pre-scheduler-<ver>`,
delete `.cargo-ok`, `Cargo.toml.orig` and `Cargo.lock`, reapply the change below and
update the version in `crates/tf2tools-app/tests/vendor_patch.rs`.

## The one change

In `TestScheduler::block_until` (`src/test_scheduler.rs`) the timeout tick range is
read before the rng is locked:

    -            self.rng
    -                .lock()
    -                .random_range(self.state.lock().timeout_ticks.clone())
    +            let timeout_ticks = self.state.lock().timeout_ticks.clone();
    +            self.rng.lock().random_range(timeout_ticks)

Upstream holds the rng lock while taking the state lock, the reverse of
`schedule_background_with_priority`, so a timed block (app quit) racing a wake from
another thread (a tokio worker) deadlocks. Only tests use this scheduler.
`crates/tf2tools-app/tests/scheduler_lock_order.rs` reproduces it. Drop this
directory and the patch entry once upstream takes the locks in one order.
