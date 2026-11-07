//! The test scheduler must not deadlock when app quit blocks with a timeout while a
//! tokio worker wakes a GPUI task, as `tokio_bridge` does on quit.

use std::io::Write as _;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, mpsc};
use std::time::Duration;

use gpui_kit::TestAppContext;

/// Aborts the process if not signalled within `limit`, so a deadlock fails the suite
/// instead of hanging it. Writes to stderr directly, which the test harness does not capture.
fn deadlock_watchdog(limit: Duration) -> mpsc::Sender<()> {
    let (done, finished) = mpsc::channel::<()>();
    std::thread::spawn(move || {
        if finished.recv_timeout(limit) == Err(mpsc::RecvTimeoutError::Timeout) {
            let _ = writeln!(std::io::stderr(), "deadlock: the test scheduler stopped");
            std::process::abort();
        }
    });
    done
}

// A timed block locks the scheduler's rng and state while another thread's wake
// locks state then rng. Racing the two many times catches an inverted lock order.
#[gpui_kit::test]
fn a_timed_block_and_a_cross_thread_wake_do_not_deadlock(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    let done = deadlock_watchdog(Duration::from_secs(60));
    for _ in 0..2_000 {
        let (tx, rx) = tokio::sync::oneshot::channel::<()>();
        let task = cx.executor().spawn(async move {
            let _ = rx.await;
        });
        cx.run_until_parked();
        let go = Arc::new(AtomicBool::new(false));
        let waker = {
            let go = Arc::clone(&go);
            std::thread::spawn(move || {
                while !go.load(Ordering::SeqCst) {
                    std::hint::spin_loop();
                }
                let _ = tx.send(());
            })
        };
        go.store(true, Ordering::SeqCst);
        for _ in 0..50 {
            let _ = cx
                .foreground_executor()
                .block_with_timeout(Duration::from_millis(1), async {});
        }
        waker.join().unwrap();
        drop(task);
    }
    let _ = done.send(());
}
