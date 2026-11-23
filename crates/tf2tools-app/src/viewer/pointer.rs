//! The demo view's pointer lock: a seam the viewport locks through for a drag, so each platform
//! can hide and pin the cursor its own way and tests can watch every lock and unlock.

use std::sync::Arc;

use gpui_kit::{Pixels, Point, Window};

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

/// The lock `Services` holds in the real app: the fallback on every platform for now.
pub fn platform_pointer_locks() -> PointerLockFactory {
    Arc::new(|| Box::new(FallbackLock))
}
