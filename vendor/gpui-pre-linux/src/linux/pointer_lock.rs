//! A pointer lock for the running Linux client, for mouselook-style views.
//!
//! Each Wayland or X11 client registers a weak handle on its thread when it starts;
//! [`lock_pointer`] and [`unlock_pointer`] reach the newest live one. The pure helpers
//! [`VirtualPointer`] and [`WarpTracker`] turn the backends' motion into a position.

use std::{cell::RefCell, rc::Rc};

/// What a lock request achieved.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PointerLockState {
    /// The pointer is held and motion arrives as deltas.
    Locked,
    /// The request was sent; the display server has not confirmed it yet.
    Pending,
    /// No client on this thread can lock the pointer.
    Unsupported,
}

/// A client's side of the lock, implemented by a weak handle to its state.
pub(crate) trait PointerLockTarget {
    fn lock(&self) -> PointerLockState;
    fn unlock(&self);
}

thread_local! {
    static TARGET: RefCell<Option<Rc<dyn PointerLockTarget>>> = const { RefCell::new(None) };
}

/// Makes `target` the client that lock requests on this thread reach, replacing any earlier one.
pub(crate) fn register(target: Rc<dyn PointerLockTarget>) {
    TARGET.with(|slot| *slot.borrow_mut() = Some(target));
}

// The handle is cloned out before the call, so a client may touch the registry while locking.
fn target() -> Option<Rc<dyn PointerLockTarget>> {
    TARGET.with(|slot| slot.borrow().clone())
}

/// Locks the pointer to the mouse-focused window of the running Linux client; `Unsupported` without one.
pub fn lock_pointer() -> PointerLockState {
    target().map_or(PointerLockState::Unsupported, |target| target.lock())
}

/// Releases a lock; does nothing when none is held.
pub fn unlock_pointer() {
    if let Some(target) = target() {
        target.unlock();
    }
}

/// A pointer position moved by relative deltas from where the lock began.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VirtualPointer {
    x: f32,
    y: f32,
}

impl VirtualPointer {
    /// Starts at `start`, usually where the pointer was when the lock began.
    pub fn new(start: (f32, f32)) -> Self {
        Self {
            x: start.0,
            y: start.1,
        }
    }

    /// Moves by `(dx, dy)` and returns the new position; unbounded.
    pub fn apply(&mut self, dx: f32, dy: f32) -> (f32, f32) {
        self.x += dx;
        self.y += dy;
        self.position()
    }

    /// The current position.
    pub fn position(&self) -> (f32, f32) {
        (self.x, self.y)
    }
}

// Reported positions pass through FP16.16 and window rounding, so the warp's echo lands near, not on, its target.
const ECHO_TOLERANCE: f32 = 1.0;

fn within_tolerance(a: (f32, f32), b: (f32, f32)) -> bool {
    (a.0 - b.0).abs() <= ECHO_TOLERANCE && (a.1 - b.1).abs() <= ECHO_TOLERANCE
}

/// Turns X11 absolute motion under a warp-to-centre lock into deltas, ignoring the motion the warp itself causes.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WarpTracker {
    last: (f32, f32),
    warp_target: Option<(f32, f32)>,
}

impl WarpTracker {
    /// Starts with the pointer at `at` and no warp in flight.
    pub fn new(at: (f32, f32)) -> Self {
        Self {
            last: at,
            warp_target: None,
        }
    }

    /// Feeds one real motion; returns the delta to apply, zero for the warp's own echo.
    pub fn motion(&mut self, at: (f32, f32)) -> (f32, f32) {
        // The server applies events in order, so motion queued before the warp still counts
        // from the old position; the first one at the target is the warp's own and moves nothing.
        if let Some(target) = self.warp_target
            && within_tolerance(at, target)
        {
            self.last = at;
            self.warp_target = None;
            return (0.0, 0.0);
        }
        let delta = (at.0 - self.last.0, at.1 - self.last.1);
        self.last = at;
        delta
    }

    /// True when a warp to `centre` should be sent now (none in flight and the pointer is away from it); records it.
    pub fn should_warp(&mut self, centre: (f32, f32)) -> bool {
        if self.warp_target.is_some() || within_tolerance(self.last, centre) {
            return false;
        }
        self.warp_target = Some(centre);
        true
    }
}
