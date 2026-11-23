//! The demo view's pointer lock: a seam the viewport locks through for a drag, so each platform
//! can hide and pin the cursor its own way and tests can watch every lock and unlock.

use std::sync::Arc;

use gpui_kit::{Pixels, Point, Window, point, px};

/// How a lock took hold: `Locked` hides the cursor and supplies motion; `Grabbed` is the visible-cursor drag.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LockMode {
    Locked,
    Grabbed,
}

/// Holds the pointer for one drag at a time.
pub trait PointerLock {
    /// Locks for a drag pressed at `press` on a viewport centred at `centre`, both in window coordinates.
    fn lock(&mut self, press: Point<Pixels>, centre: Point<Pixels>, window: &Window) -> LockMode;
    /// Motion since the last call in logical pixels, if this lock supplies its own; `None` means use event positions.
    fn motion(&mut self, window: &Window) -> Option<Point<Pixels>>;
    /// Releases the lock; does nothing when not locked.
    fn unlock(&mut self);
}

/// Makes a lock for each viewport; tests swap in a fake.
pub type PointerLockFactory = Arc<dyn Fn() -> Box<dyn PointerLock> + Send + Sync>;

/// The visible-cursor drag: never locks, never supplies motion.
pub struct FallbackLock;

impl PointerLock for FallbackLock {
    fn lock(&mut self, _: Point<Pixels>, _: Point<Pixels>, _: &Window) -> LockMode {
        LockMode::Grabbed
    }
    fn motion(&mut self, _: &Window) -> Option<Point<Pixels>> {
        None
    }
    fn unlock(&mut self) {}
}

/// Where to hold the cursor for a drag: the press point `start` (physical screen pixels) moved by the
/// logical offset from the press to the viewport centre, scaled to physical pixels.
#[cfg_attr(not(windows), allow(dead_code))]
pub(crate) fn anchor_for(
    start: (i32, i32),
    press: Point<Pixels>,
    centre: Point<Pixels>,
    scale: f32,
) -> (i32, i32) {
    let offset = |to: Pixels, from: Pixels| ((to - from).as_f32() * scale).round() as i32;
    (
        start.0 + offset(centre.x, press.x),
        start.1 + offset(centre.y, press.y),
    )
}

/// The logical motion of a cursor read at `now` while held at `anchor`, both in physical pixels.
#[cfg_attr(not(windows), allow(dead_code))]
pub(crate) fn motion_from(now: (i32, i32), anchor: (i32, i32), scale: f32) -> Point<Pixels> {
    let logical = |now: i32, anchor: i32| px((now - anchor) as f32 / scale);
    point(logical(now.0, anchor.0), logical(now.1, anchor.1))
}

/// The Windows lock: hides the cursor, holds it at the viewport centre and reads each move as an
/// offset from there, then puts it back where the drag was pressed.
#[cfg(windows)]
#[derive(Default)]
struct WindowsLock {
    /// The press point to restore and the anchor the cursor is held at, in physical pixels; `None` when unlocked.
    /// Hide and show follow this state alone, so the per-thread `ShowCursor` counter stays balanced.
    held: Option<((i32, i32), (i32, i32))>,
}

#[cfg(windows)]
impl PointerLock for WindowsLock {
    fn lock(&mut self, press: Point<Pixels>, centre: Point<Pixels>, window: &Window) -> LockMode {
        if self.held.is_some() {
            return LockMode::Locked;
        }
        // Without a readable cursor there is nothing to restore, so the drag stays visible.
        let Some(start) = tf2tools_core::cursor::position() else {
            return LockMode::Grabbed;
        };
        let anchor = anchor_for(start, press, centre, window.scale_factor());
        // A cursor that cannot reach the anchor would measure every move from the wrong place.
        if !tf2tools_core::cursor::set_position(anchor.0, anchor.1) {
            return LockMode::Grabbed;
        }
        tf2tools_core::cursor::hide();
        self.held = Some((start, anchor));
        LockMode::Locked
    }

    fn motion(&mut self, window: &Window) -> Option<Point<Pixels>> {
        let (_, anchor) = self.held?;
        // An unreadable cursor counts as no motion rather than falling back to event positions mid-drag.
        let now = tf2tools_core::cursor::position().unwrap_or(anchor);
        tf2tools_core::cursor::set_position(anchor.0, anchor.1);
        Some(motion_from(now, anchor, window.scale_factor()))
    }

    fn unlock(&mut self) {
        if let Some((start, _)) = self.held.take() {
            tf2tools_core::cursor::set_position(start.0, start.1);
            tf2tools_core::cursor::show();
        }
    }
}

#[cfg(windows)]
impl Drop for WindowsLock {
    // A viewport dropped mid-drag must not leave the cursor hidden.
    fn drop(&mut self) {
        self.unlock();
    }
}

/// The Linux lock: the vendored platform crate pins and hides the pointer and keeps delivering
/// ordinary MouseMoves at a virtual position, so the drag reads event positions as before.
#[cfg(target_os = "linux")]
#[derive(Default)]
struct LinuxLock {
    /// Whether this lock asked the client to lock, so unlock only releases a lock it took.
    held: bool,
}

#[cfg(target_os = "linux")]
impl PointerLock for LinuxLock {
    fn lock(&mut self, _: Point<Pixels>, _: Point<Pixels>, _: &Window) -> LockMode {
        use gpui_linux::pointer_lock::{PointerLockState, lock_pointer};
        // A pending request counts as held: the compositor confirms it a roundtrip later.
        match lock_pointer() {
            PointerLockState::Locked | PointerLockState::Pending => {
                self.held = true;
                LockMode::Locked
            }
            PointerLockState::Unsupported => LockMode::Grabbed,
        }
    }

    fn motion(&mut self, _: &Window) -> Option<Point<Pixels>> {
        None
    }

    fn unlock(&mut self) {
        if std::mem::take(&mut self.held) {
            gpui_linux::pointer_lock::unlock_pointer();
        }
    }
}

/// The lock `Services` holds in the real app: the cursor lock on Windows and Linux, the fallback elsewhere.
pub fn platform_pointer_locks() -> PointerLockFactory {
    #[cfg(windows)]
    {
        Arc::new(|| Box::new(WindowsLock::default()))
    }
    #[cfg(target_os = "linux")]
    {
        Arc::new(|| Box::new(LinuxLock::default()))
    }
    #[cfg(not(any(windows, target_os = "linux")))]
    {
        Arc::new(|| Box::new(FallbackLock))
    }
}

#[cfg(test)]
mod tests {
    use gpui_kit::{point, px};

    use super::{anchor_for, motion_from};

    #[test]
    fn the_anchor_sits_at_the_centre_in_physical_pixels() {
        // Pressed 100,50 logical left/up of centre at 150% scale: the anchor is 150,75 physical right/down of the press.
        assert_eq!(
            anchor_for(
                (1000, 800),
                point(px(200.), px(150.)),
                point(px(300.), px(200.)),
                1.5
            ),
            (1150, 875)
        );
    }

    #[test]
    fn motion_is_physical_offset_over_scale() {
        let m = motion_from((1015, 870), (1000, 800), 1.5);
        assert!(
            (m.x.as_f32() - 10.).abs() < 1e-4 && (m.y.as_f32() - 46.666_67).abs() < 1e-3,
            "{m:?}"
        );
    }
}
