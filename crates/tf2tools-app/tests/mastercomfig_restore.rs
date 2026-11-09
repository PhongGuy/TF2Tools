//! App start writes back mastercomfig files another tool cleared.

use std::sync::{Arc, Mutex};

use gpui_kit::{AnyWindowHandle, AppContext as _, TestAppContext};
use tf2tools_app::app::{Boot, start};
use tf2tools_app::audio::RecordingAudio;
use tf2tools_app::ids;
use tf2tools_app::picker::ScriptedPicker;
use tf2tools_app::shell::toast::toast_count;

use crate::support;

const HOOK: &str = "tf/custom/comfig-custom/cfg/app/setup_hook.cfg";

fn boot(env: &support::Env) -> Boot {
    Boot {
        data: env.data.clone(),
        assets: Ok(support::repo_asset_root()),
        picker: Arc::new(ScriptedPicker::empty()),
        detector: support::detection_forbidden(),
        audio: Arc::new(RecordingAudio::new()),
        endpoints: tf2tools_core::Endpoints::default(),
        auto_sync: false,
        offline: false,
        font_host: None,
        updates: tf2tools_app::updater::UpdateBoot::Off,
    }
}

// A managed install on `preset` with the game path saved, as a previous start left it.
fn managed(env: &support::Env, preset: &str) {
    let services = support::services(env, support::no_steam());
    services.db.set_tf2_path(Some(&env.game)).unwrap();
    support::seed_managed_mastercomfig(env, "9.100.1", preset);
}

fn started(cx: &mut TestAppContext, env: &support::Env) -> AnyWindowHandle {
    support::init(cx);
    cx.update(|cx| start(boot(env), cx)).unwrap()
}

fn connected(cx: &mut TestAppContext, win: AnyWindowHandle) {
    support::wait_real(cx, "the badge", |cx| {
        support::label_of(cx, win, ids::TF2_BADGE).as_deref() == Some("TF2 Connected")
    });
}

#[gpui_kit::test]
fn start_writes_back_a_cleared_preset_file(cx: &mut TestAppContext) {
    let env = support::env();
    managed(&env, "ultra");
    assert!(!env.game.join(HOOK).exists());
    let win = started(cx, &env);
    // Detection waits for the first frame, so the wait draws the window each round.
    support::wait_real(cx, "the restore", |cx| {
        support::settle(cx, win);
        env.game.join(HOOK).is_file()
    });
    assert_eq!(
        std::fs::read_to_string(env.game.join(HOOK)).unwrap(),
        "preset=ultra\n"
    );
}

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

// The failure's only trace is the developer log, so the test waits for that line before it
// counts toasts; without the wait a restore that never ran would pass too.
#[gpui_kit::test]
fn a_failed_restore_shows_no_toast(cx: &mut TestAppContext) {
    // It sets the global tracing subscriber, so it runs alone in a child process.
    if !support::in_own_process("mastercomfig_restore::a_failed_restore_shows_no_toast") {
        return;
    }
    let buf = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&buf);
    tracing_subscriber::fmt()
        .with_ansi(false)
        .with_writer(move || Capture(Arc::clone(&sink)))
        .try_init()
        .expect("the only subscriber in this process");
    let env = support::env();
    managed(&env, "high");
    // A file where the folder must go, so the write back fails.
    std::fs::write(env.game.join("tf/custom/comfig-custom"), b"").unwrap();
    let win = started(cx, &env);
    connected(cx, win);
    let logged = || String::from_utf8(buf.lock().unwrap().clone()).unwrap();
    support::wait_real(cx, "the restore's failure", |cx| {
        support::settle(cx, win);
        logged().contains("mastercomfig restore at start")
    });
    assert!(logged().contains("WARN"), "{}", logged());
    support::settle(cx, win);
    assert_eq!(
        cx.update_window(win, |_, w, cx| toast_count(w, cx))
            .unwrap(),
        0
    );
    assert!(env.game.join("tf/custom/comfig-custom").is_file());
}
