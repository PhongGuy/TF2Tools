# Vendored gpui-pre-linux 0.3.12

A copy of `gpui-pre-linux` 0.3.12 from crates.io (a snapshot of
zed@d2a4fb6ded7c470be25d1541281da3882495b65b, Apache-2.0, see `LICENSE-APACHE`),
used through `[patch.crates-io]` in the workspace `Cargo.toml`.

## The changes

### Opaque start

In `WaylandWindowState::new` (`src/linux/wayland/window.rs`) the renderer starts
opaque when the surface holds an xdg-decoration object, which only a toplevel gets,
and only when the compositor offers the protocol:

    -                transparent: true,
    +                transparent: starts_transparent(surface_state.decoration().is_some()),

Popups and layer surfaces never hold one, so they keep starting transparent.

Upstream starts transparent, so gpui's default `request_decorations(Server)` makes
the window opaque and rebuilds every wgpu pipeline during startup. Without the
xdg-decoration protocol the window stays transparent, as before. The rebuild cost
about 35 ms of a warm start and 92 ms of a cold one.

### Restart helper

`LinuxPlatform::restart` (`src/linux/platform.rs`) runs the script in
`src/linux/restart.sh`, pulled in with `include_str!`. Upstream inlines a loop that
polls `kill -0` on the old process with no limit; ours treats a zombie as exited and
starts the app anyway after about 10 s. A parent that never reaps, such as a
gpui-pilot session, kept the updated app from reopening.
`crates/tf2tools-app/tests/restart_helper.rs` runs the script against stub processes.

### Pointer lock

The new file `src/linux/pointer_lock.rs` adds `gpui_linux::pointer_lock`, which the demo
viewer uses for mouselook: `lock_pointer()` returns `Locked`, `Pending` or `Unsupported`,
and `unlock_pointer()` releases a lock. Each Wayland or X11 client registers a weak
handle on its thread at startup, so both calls reach the running client and report
`Unsupported` without one. While locked, motion still arrives as ordinary `MouseMove`s
at a virtual position (`VirtualPointer`) that starts where the lock began and has no
bounds. Either backend ends the lock itself on a button release or on focus loss, and
also when the window closes.

- **Wayland** (`src/linux/wayland/client.rs`) binds `zwp_pointer_constraints_v1` and
  `zwp_relative_pointer_manager_v1` when offered. A lock is a oneshot pointer
  constraint with the cursor hidden; relative-pointer deltas move the virtual position,
  and absolute `wl_pointer` motion is ignored while locked so it does not count twice.
- **X11** (`src/linux/x11/client.rs`) takes an XInput2 grab on the client pointer, or
  on the first master pointer when no client pointer is set, with the invisible
  cursor and owner events on. Each motion warps the pointer back to the window centre,
  and `WarpTracker` filters each warp's echo so only real deltas count. Unlocking
  ungrabs, warps the pointer back to where the lock began and sends one buttonless
  `MouseMove` there.

`crates/tf2tools-app/tests/vendor_pointer_lock.rs` tests the pure helpers and the
headless fallback.

### Frame logging

With `TF2TOOLS_VIEWER_STATS` set, `src/linux/wayland/window.rs` logs a `wayland-frame:`
line on the log target `tf2tools_viewer_stats` for each scheduled frame (and whether it
was skipped awaiting a callback), each frame callback, each tick and its outcome, and
each draw. The app's `diagnostics.rs` forwards that target into its viewer stats.

## Re-syncing

Copy the new upstream version from `~/.cargo/registry/src/*/gpui-pre-linux-<ver>`,
delete `.cargo-ok`, `Cargo.toml.orig` and `Cargo.lock`, reapply the changes above and
update the version in `crates/tf2tools-app/tests/vendor_patch.rs`. Reapply the pointer
lock (the new module, its `pub mod`/`pub use` lines in `src/linux.rs` and
`src/gpui_linux.rs`, and both `client.rs` files) and the frame logging too. Drop this
directory and the patch entry once upstream starts the renderer opaque and offers a
pointer lock.
