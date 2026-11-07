//! The store's developer log, alone in its binary: tracing caches a callsite's interest the
//! first time any thread hits it, so a sibling test without a subscriber would silence this one.

use crate::support;

use std::sync::{Arc, Mutex};

use gpui_kit::{AppContext as _, TestAppContext};
use tf2tools_app::ids;
use tf2tools_app::pages::hud_store::bodies::gamebanana_body;
use tf2tools_app::store_detail::{DetailItem, open_detail};
use tf2tools_core::{GbId, StoreItemId, StoreKind};

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

// The raw error is for developers: a failed listing writes it to the log, never to the player.
#[gpui_kit::test]
fn a_failed_listing_logs_its_raw_detail(cx: &mut TestAppContext) {
    // It sets process-wide state, so it runs alone in a child process.
    if !support::in_own_process("store_log::a_failed_listing_logs_its_raw_detail") {
        return;
    }
    let buf = Arc::new(Mutex::new(Vec::new()));
    let sink = buf.clone();
    tracing_subscriber::fmt()
        .with_ansi(false)
        .with_writer(move || Capture(sink.clone()))
        .try_init()
        .expect("the only subscriber in this binary");
    let env = support::env();
    let server = support::StoreServer::start();
    let (win, _) = support::store_window(
        cx,
        &env,
        &server,
        |env| support::seed_gb_huds(env, &[(9, "Gone")]),
        |_, _| support::Blank,
    );
    let item = DetailItem {
        kind: StoreKind::GameBananaHuds,
        id: StoreItemId::GameBanana(GbId::new(9)),
        name: "Gone".into(),
        in_library: false,
    };
    cx.update_window(win, |_, window, cx| {
        open_detail(item, gamebanana_body(), window, cx)
    })
    .unwrap();
    support::wait_real(cx, "the inline error", |cx| {
        support::present(cx, win, ids::STORE_FILES_ERROR)
    });
    let shown = support::label_of(cx, win, ids::STORE_FILES_ERROR).unwrap();
    let log = String::from_utf8(buf.lock().unwrap().clone()).unwrap();
    assert!(log.contains("WARN"), "{log}");
    assert!(log.contains("store file listing failed: "), "{log}");
    assert!(
        log.contains(&server.uri()),
        "the log names the address: {log}"
    );
    assert!(!shown.contains(&server.uri()), "{shown}");
}
