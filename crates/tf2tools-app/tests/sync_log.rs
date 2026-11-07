//! The sync hub's developer log, alone in its binary: tracing caches a callsite's interest the
//! first time any thread hits it, so a sibling test without a subscriber would silence this one.

use std::sync::{Arc, Mutex};

use gpui_kit::TestAppContext;
use tf2tools_core::StoreKind;

use crate::support;

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

// The raw error is for developers: it reaches the stderr log, never the player.
#[gpui_kit::test]
fn a_failure_logs_its_raw_detail(cx: &mut TestAppContext) {
    // It sets process-wide state, so it runs alone in a child process.
    if !support::in_own_process("sync_log::a_failure_logs_its_raw_detail") {
        return;
    }
    let buf = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&buf);
    tracing_subscriber::fmt()
        .with_ansi(false)
        .with_writer(move || Capture(Arc::clone(&sink)))
        .try_init()
        .expect("the only subscriber in this binary");
    let (hub, runs) = support::fake_hub(cx);
    assert!(cx.update(|cx| hub.update(cx, |h, cx| h.start(StoreKind::Sounds, false, cx))));
    runs.try_recv()
        .unwrap()
        .finish(Err(support::offline_failure()));
    support::wait_hub(cx, "the failure", |cx| {
        cx.update(|cx| hub.read(cx).failure(StoreKind::Sounds).is_some())
    });
    let log = String::from_utf8(buf.lock().unwrap().clone()).unwrap();
    assert!(log.contains("WARN"), "{log}");
    assert!(
        log.contains(
            "cannot reach https://gamebanana.com/apiv11/Util/Search/Results: connection refused"
        ),
        "{log}"
    );
}
