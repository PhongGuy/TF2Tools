//! The add-on flow's developer log, alone in its binary (see `store_log.rs` for why).

use crate::gb_addon_installs::{profile, toast_is, vpk, window};
use crate::support;

use std::sync::{Arc, Mutex};

use gpui_kit::TestAppContext;
use tf2tools_app::gb_addon_installs::GbAddonInstalls;
use tf2tools_core::AddonSection;

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

#[gpui_kit::test]
fn an_unsafe_download_is_refused_and_logged(cx: &mut TestAppContext) {
    if !support::in_own_process("gb_addon_log::an_unsafe_download_is_refused_and_logged") {
        return;
    }
    let buf = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&buf);
    tracing_subscriber::fmt()
        .with_ansi(false)
        .with_writer(move || Capture(Arc::clone(&sink)))
        .try_init()
        .expect("the only subscriber in this binary");
    let (env, server) = (support::env(), support::StoreServer::start());
    let body = support::zip_of(&[("../evil.vpk", &vpk(b"v"))]);
    server.mount_gb_addon(30, "Evil", &[("evil.zip", body, 100)]);
    let w = window(cx, &env, &server);
    let profile = profile(cx, 30);
    let installs = cx.update(GbAddonInstalls::global);
    let (win, section) = (w.win, AddonSection::by_key("effects").unwrap());
    assert!(cx.update(|cx| installs.update(cx, |i, cx| i.install(section, profile, win, cx))));
    toast_is(
        cx,
        w.win,
        "Couldn't install the add-on. This download looks unsafe and was not installed.",
    );
    let log = String::from_utf8(buf.lock().unwrap().clone()).unwrap();
    assert!(log.contains("add-on download failed: "), "{log}");
    assert!(
        log.contains("../evil.vpk"),
        "the log names the entry: {log}"
    );
    assert!(!env.game.join("tf/evil.vpk").exists());
}
