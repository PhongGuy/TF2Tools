//! The local developer log, alone in its binary: tracing caches a callsite's interest the
//! first time any thread hits it, so a sibling test without a subscriber would silence this one.

use crate::support;

use std::sync::{Arc, Mutex};

use gpui_kit::TestAppContext;
use tf2tools_app::local_messages::{INTERNAL, LocalFailure};
use tf2tools_app::services::{OpError, run_blocking};

struct Capture(Arc<Mutex<Vec<u8>>>);

impl std::io::Write for Capture {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

// The raw error is for developers: a failed action and a caught panic both write it to the
// log, and neither shows it to the player.
#[gpui_kit::test]
async fn raw_details_go_to_the_developer_log(cx: &mut TestAppContext) {
    // It sets process-wide state, so it runs alone in a child process.
    if !support::in_own_process("local_log::raw_details_go_to_the_developer_log") {
        return;
    }
    let buf = Arc::new(Mutex::new(Vec::new()));
    let sink = buf.clone();
    tracing_subscriber::fmt()
        .with_ansi(false)
        .with_writer(move || Capture(sink.clone()))
        .try_init()
        .expect("the only subscriber in this binary");
    let shown = LocalFailure::new(
        "Try again.",
        "library database error: no such table: library_huds",
    )
    .into_op("Failed to load HUDs");
    assert_eq!(shown.description(), "Try again.");
    support::init(cx);
    let env = support::env();
    support::install_services(cx, support::services(&env, support::no_steam()));
    let task = cx.update(|cx| {
        run_blocking(cx, |_| -> Result<(), OpError> {
            panic!("boom at the bottom")
        })
    });
    let panicked = task.await.unwrap_err();
    assert_eq!(panicked.description(), INTERNAL);
    let log = String::from_utf8(buf.lock().unwrap().clone()).unwrap();
    assert!(
        log.contains("Failed to load HUDs: library database error: no such table: library_huds"),
        "{log}"
    );
    assert!(log.contains("WARN"), "{log}");
    assert!(log.contains("ERROR"), "{log}");
    assert!(log.contains("boom at the bottom"), "{log}");
}
