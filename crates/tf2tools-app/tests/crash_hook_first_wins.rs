//! Only the first uncaught panic of a process is recorded. It runs in a child process of its own:
//! it installs the process-wide panic hook.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use tf2tools_app::crash::recent::Recent;
use tf2tools_app::crash::{CRASH_FILE, install_hook};

use crate::support;

#[test]
fn the_first_uncaught_panic_keeps_the_crash_file() {
    // It sets process-wide state, so it runs alone in a child process.
    if !support::in_own_process(
        "crash_hook_first_wins::the_first_uncaught_panic_keeps_the_crash_file",
    ) {
        return;
    }
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("crash");
    let calls = Arc::new(AtomicUsize::new(0));
    let seen = calls.clone();
    drop(std::panic::take_hook());
    std::panic::set_hook(Box::new(move |info| {
        seen.fetch_add(1, Ordering::SeqCst);
        eprintln!("{info}");
    }));
    install_hook(dir.clone(), Recent::new());

    for (name, message) in [("first", "first boom"), ("second", "second boom")] {
        let joined = std::thread::Builder::new()
            .name(name.into())
            .spawn(move || panic!("{message}"))
            .unwrap()
            .join();
        assert!(joined.is_err());
    }

    let text = std::fs::read_to_string(dir.join(CRASH_FILE)).unwrap();
    assert!(text.contains("\nMessage: first boom\n"), "{text}");
    assert!(!text.contains("second boom"), "{text}");
    assert_eq!(
        calls.load(Ordering::SeqCst),
        2,
        "every panic reaches the previous hook"
    );
}
