//! A per-thread "this panic is handled" mark. Code that catches its own panics runs them under
//! `marked`, and a panic hook asks `is_marked` to tell a caught panic from a crash.

use std::cell::Cell;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::pin::Pin;
use std::task::{Context, Poll};

thread_local!(static DEPTH: Cell<u32> = const { Cell::new(0) });

/// True while the current thread runs inside `marked` (or a poll of `marked_future`).
/// Never panics, so a panic hook may call it.
pub fn is_marked() -> bool {
    DEPTH.try_with(Cell::get).unwrap_or(0) > 0
}

// Lifts one level of the mark when dropped, so an unwinding panic cannot leave a thread marked.
struct Mark;

impl Mark {
    fn set() -> Self {
        let _ = DEPTH.try_with(|d| d.set(d.get().saturating_add(1)));
        Mark
    }
}

impl Drop for Mark {
    fn drop(&mut self) {
        let _ = DEPTH.try_with(|d| d.set(d.get().saturating_sub(1)));
    }
}

/// Runs `f` with the thread marked; the mark is lifted on return and on unwind. Marks nest.
pub fn marked<R>(f: impl FnOnce() -> R) -> R {
    let _mark = Mark::set();
    f()
}

/// `f`, with every poll run under `marked`, for futures whose panic the executor catches.
pub fn marked_future<F: Future>(f: F) -> impl Future<Output = F::Output> {
    let mut f = Box::pin(f);
    std::future::poll_fn(move |cx: &mut Context<'_>| -> Poll<F::Output> {
        marked(|| Pin::as_mut(&mut f).poll(cx))
    })
}

/// `catch_unwind(AssertUnwindSafe(|| marked(f)))`: catches a panic in `f` that the hook should
/// treat as handled.
pub fn catching<R>(f: impl FnOnce() -> R) -> std::thread::Result<R> {
    catch_unwind(AssertUnwindSafe(|| marked(f)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_mark_nests_and_is_lifted_on_return_and_on_unwind() {
        assert!(!is_marked());
        marked(|| {
            assert!(is_marked());
            marked(|| assert!(is_marked()));
            assert!(is_marked());
        });
        assert!(!is_marked());
        assert!(catching::<()>(|| panic!("lifted")).is_err());
        assert!(!is_marked(), "an unwinding panic left the thread marked");
    }
}
