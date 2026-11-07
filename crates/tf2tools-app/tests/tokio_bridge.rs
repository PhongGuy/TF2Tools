use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use gpui_kit::TestAppContext;
use tf2tools_app::tokio_bridge;

use crate::support;

/// Flips its flag when dropped, so a test sees a tokio future being dropped.
struct SetOnDrop(Arc<AtomicBool>);

impl Drop for SetOnDrop {
    fn drop(&mut self) {
        self.0.store(true, Ordering::SeqCst);
    }
}

fn pending_with_flag(
    cx: &mut TestAppContext,
) -> (
    Arc<AtomicBool>,
    gpui_kit::Task<Result<(), tokio_bridge::JoinError>>,
) {
    let dropped = Arc::new(AtomicBool::new(false));
    let guard = SetOnDrop(dropped.clone());
    let task = cx.update(|cx| {
        tokio_bridge::spawn(cx, async move {
            let _guard = guard;
            std::future::pending::<()>().await;
        })
    });
    (dropped, task)
}

#[gpui_kit::test]
fn spawn_runs_on_the_multi_thread_runtime_and_resolves_on_the_foreground(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    assert!(!cx.update(|cx| tokio_bridge::is_running(cx)));
    cx.update(tokio_bridge::init);
    assert!(cx.update(|cx| tokio_bridge::is_running(cx)));
    // tokio's sleep panics outside a runtime with timers, so finishing proves where it ran.
    let task = cx.update(|cx| {
        tokio_bridge::spawn(cx, async {
            tokio::time::sleep(Duration::from_millis(5)).await;
            tokio::runtime::Handle::current().runtime_flavor()
        })
    });
    let out = Arc::new(Mutex::new(None));
    let slot = out.clone();
    cx.update(|cx| {
        cx.spawn(async move |_| {
            *slot.lock().unwrap() = Some(task.await);
        })
        .detach()
    });
    support::wait_real(cx, "the tokio task", |_| out.lock().unwrap().is_some());
    let flavor = out.lock().unwrap().take().unwrap().expect("no join error");
    assert_eq!(flavor, tokio::runtime::RuntimeFlavor::MultiThread);
}

#[gpui_kit::test]
fn a_second_init_keeps_the_first_runtime(cx: &mut TestAppContext) {
    cx.update(tokio_bridge::init);
    let first = cx.update(|cx| tokio_bridge::handle(cx));
    cx.update(tokio_bridge::init);
    // A replaced runtime is shut down, and a shut-down runtime cancels what it is given.
    let joined = first.block_on(first.spawn(async { 7 }));
    assert_eq!(joined.expect("the first runtime still runs"), 7);
}

#[gpui_kit::test]
fn dropping_the_task_aborts_the_tokio_future(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    cx.update(tokio_bridge::init);
    let (dropped, task) = pending_with_flag(cx);
    cx.run_until_parked();
    assert!(!dropped.load(Ordering::SeqCst));
    drop(task);
    support::wait_real(cx, "the abort", |_| dropped.load(Ordering::SeqCst));
}

#[gpui_kit::test]
fn quitting_the_app_shuts_the_runtime_down(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    cx.update(tokio_bridge::init);
    let (dropped, task) = pending_with_flag(cx);
    cx.run_until_parked();
    cx.quit();
    assert!(!cx.update(|cx| tokio_bridge::is_running(cx)));
    let deadline = Instant::now() + Duration::from_secs(5);
    while !dropped.load(Ordering::SeqCst) {
        assert!(
            Instant::now() < deadline,
            "shutdown left the future running"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
    drop(task);
}

#[gpui_kit::test]
fn shutdown_without_init_is_a_no_op(cx: &mut TestAppContext) {
    cx.update(tokio_bridge::shutdown);
    assert!(!cx.update(|cx| tokio_bridge::is_running(cx)));
}
