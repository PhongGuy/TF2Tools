use std::future::Future as _;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::task::{Context, Waker};

use tf2tools_app::crash::recent::Recent;
use tf2tools_app::crash::{CRASH_FILE, catching, install_hook, marked, marked_future};

use crate::support;

#[test]
fn the_hook_records_uncaught_panics_and_chains_to_the_previous_one() {
    // It sets process-wide state, so it runs alone in a child process.
    if !support::in_own_process(
        "crash_hook::the_hook_records_uncaught_panics_and_chains_to_the_previous_one",
    ) {
        return;
    }
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("crash");
    let calls = Arc::new(AtomicUsize::new(0));
    let seen = Arc::clone(&calls);
    // A counting hook stands in for std's default one; the harness shows what it prints only
    // when the test fails, which keeps a failed assert readable.
    drop(std::panic::take_hook());
    std::panic::set_hook(Box::new(move |info| {
        seen.fetch_add(1, Ordering::SeqCst);
        eprintln!("{info}");
    }));
    install_hook(dir.clone(), Recent::new());

    let caught = catching(|| -> u32 { panic!("caught boom") });
    assert!(caught.is_err());
    assert!(
        !dir.join(CRASH_FILE).exists(),
        "a caught panic leaves no file"
    );
    assert_eq!(
        calls.load(Ordering::SeqCst),
        1,
        "the previous hook still ran"
    );
    assert_eq!(catching(|| 7).unwrap(), 7);

    // A marked closure or future panicking on a thread of its own, as under tokio.
    let closure = std::thread::spawn(|| marked(|| panic!("blocking boom"))).join();
    assert!(closure.is_err());
    let polled = std::thread::spawn(|| {
        let mut task = std::pin::pin!(marked_future(async { panic!("polled boom") }));
        let _ = task.as_mut().poll(&mut Context::from_waker(Waker::noop()));
    })
    .join();
    assert!(polled.is_err());
    assert!(
        !dir.join(CRASH_FILE).exists(),
        "marked panics leave no file"
    );
    assert_eq!(calls.load(Ordering::SeqCst), 3);

    // The VTF parser panics on a truncated file; core catches that behind its own guard.
    let vtf = std::fs::read(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../assets/crosshairs/vtf/Crossdot.vtf"),
    )
    .unwrap();
    assert!(tf2tools_core::decode_vtf(&vtf[..vtf.len() - 1]).is_err());
    assert!(
        !dir.join(CRASH_FILE).exists(),
        "a panic caught in core leaves no file"
    );
    assert_eq!(calls.load(Ordering::SeqCst), 4);

    // A caught panic first, on the same thread: its mark must be gone by the uncaught one.
    let joined = std::thread::Builder::new()
        .name("worker".into())
        .spawn(|| {
            assert!(catching::<()>(|| panic!("caught on the worker")).is_err());
            panic!("uncaught boom")
        })
        .unwrap()
        .join();
    assert!(joined.is_err());
    let text = std::fs::read_to_string(dir.join(CRASH_FILE)).unwrap();
    let head = format!(
        "TF2Tools {} closed unexpectedly.\n",
        env!("CARGO_PKG_VERSION")
    );
    assert!(text.starts_with(&head), "{text}");
    assert!(text.contains("\nThread: worker\n"), "{text}");
    assert!(text.contains("\nMessage: uncaught boom\n"), "{text}");
    assert!(text.contains(&format!("{}:", file!())), "{text}");
    assert!(text.contains("\n\nBacktrace:\n"), "{text}");
    assert_eq!(calls.load(Ordering::SeqCst), 6);
}
