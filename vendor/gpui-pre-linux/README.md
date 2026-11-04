# Vendored gpui-pre-linux 0.3.8

A copy of `gpui-pre-linux` 0.3.8 from crates.io (a snapshot of
zed@279fe070bb389b79652e52065b2f001edcc0b11b, Apache-2.0, see `LICENSE-APACHE`),
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

## Re-syncing

Copy the new upstream version from `~/.cargo/registry/src/*/gpui-pre-linux-<ver>`,
delete `.cargo-ok`, `Cargo.toml.orig` and `Cargo.lock`, reapply the changes above and
update the version in `crates/tf2tools-app/tests/vendor_patch.rs`. Drop this
directory and the patch entry once upstream starts the renderer opaque.
