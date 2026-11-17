//! The local developer log, alone in its binary: tracing caches a callsite's interest the
//! first time any thread hits it, so a sibling test without a subscriber would silence this one.

use crate::support;
use crate::support::update_fake::{act, failure, fake_updater, offer, reach_available, wait_state};

use std::sync::{Arc, Mutex};

use gpui_kit::TestAppContext;
use tf2tools_app::local_messages::{LocalFailure, gb_install_op, hud_files, internal};
use tf2tools_app::pages::graphics::jobs::save_failed;
use tf2tools_app::services::{OpError, run_blocking};
use tf2tools_app::store_messages::no_tf2;
use tf2tools_app::updater::UpdateState;
use tf2tools_core::GbAddonInstallError;

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

// Installs the only subscriber in this process and hands back what it writes.
fn capture_log() -> Arc<Mutex<Vec<u8>>> {
    let buf = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&buf);
    tracing_subscriber::fmt()
        .with_ansi(false)
        .with_writer(move || Capture(Arc::clone(&sink)))
        .try_init()
        .expect("the only subscriber in this binary");
    buf
}

fn logged(buf: &Mutex<Vec<u8>>) -> String {
    String::from_utf8(buf.lock().unwrap().clone()).unwrap()
}

fn cyrillic(c: char) -> bool {
    matches!(c, 'А'..='я' | 'Ё' | 'ё')
}

// The raw error is for developers: a failed action and a caught panic both write it to the
// log, and neither shows it to the player.
#[gpui_kit::test]
async fn raw_details_go_to_the_developer_log(cx: &mut TestAppContext) {
    // It sets process-wide state, so it runs alone in a child process.
    if !support::in_own_process("local_log::raw_details_go_to_the_developer_log") {
        return;
    }
    let buf = capture_log();
    let failure = LocalFailure::new(
        "Try again.",
        "library database error: no such table: library_huds",
    );
    let line = line!() + 1;
    let shown = failure.into_op("Failed to load HUDs".into());
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
    assert_eq!(panicked.description(), internal());
    let log = logged(&buf);
    let want = format!(
        "{}:{line}: library database error: no such table: library_huds",
        file!()
    );
    assert!(log.contains(&want), "{log}");
    assert!(!log.contains("Failed to load HUDs"), "{log}");
    assert!(log.contains("WARN"), "{log}");
    assert!(log.contains("ERROR"), "{log}");
    assert!(log.contains("boom at the bottom"), "{log}");
}

// A Russian player's failures still log in English: the place in the code and the raw
// detail, never the translated title or message.
#[test]
fn the_developer_log_stays_english_for_a_russian_player() {
    if !support::in_own_process("local_log::the_developer_log_stays_english_for_a_russian_player") {
        return;
    }
    let buf = capture_log();
    tf2tools_app::i18n::apply("ru");
    let failure = LocalFailure::new(hud_files(), "gameinfo.txt is missing");
    let line = line!() + 1;
    let op = failure.into_op(save_failed());
    let helper_line = line!() + 1;
    let _ = gb_install_op(&GbAddonInstallError::NothingChosen, save_failed());
    tracing::warn!("{}", OpError::NoTf2);
    tracing::warn!("{}", OpError::Internal("boom".into()));
    no_tf2().log("store download");
    assert!(
        op.title().chars().any(cyrillic) && op.description().chars().any(cyrillic),
        "the player still reads Russian: {op:?}"
    );
    let log = logged(&buf);
    assert!(!log.chars().any(cyrillic), "{log}");
    let want = format!("{}:{line}: gameinfo.txt is missing", file!());
    assert!(log.contains(&want), "{log}");
    assert!(
        log.contains(&format!("{}:{helper_line}: ", file!())),
        "{log}"
    );
    assert!(log.contains("WARN"), "{log}");
    assert!(log.contains("unexpected error: boom"), "{log}");
    assert!(
        log.contains("store download failed: no TF2 folder is set"),
        "{log}"
    );
}

// A Russian player's failed update writes the player's message to the activity row only;
// the developer warning names the action in English.
#[gpui_kit::test]
fn the_update_failure_warning_stays_english(cx: &mut TestAppContext) {
    if !support::in_own_process("local_log::the_update_failure_warning_stays_english") {
        return;
    }
    let buf = capture_log();
    tf2tools_app::i18n::apply("ru");
    let (u, fake) = fake_updater(cx, true);
    reach_available(cx, &u, &fake, offer("6.0.1", true));
    act(cx, &u, |u, cx| u.download(cx));
    let (sink, reply) = fake.next_download(cx);
    drop(sink);
    reply
        .send(Err(failure("Загруженный файл повреждён.", false)))
        .unwrap();
    wait_state(cx, &u, "Available again", |s| {
        matches!(s, UpdateState::Available(_))
    });
    let log = logged(&buf);
    assert!(log.contains("update failed"), "{log}");
    assert!(log.contains("update-download"), "{log}");
    assert!(!log.chars().any(cyrillic), "{log}");
}

// A Russian player's failed check shows Russian in Settings but logs only English.
#[gpui_kit::test]
fn the_failed_check_warning_stays_english(cx: &mut TestAppContext) {
    if !support::in_own_process("local_log::the_failed_check_warning_stays_english") {
        return;
    }
    let buf = capture_log();
    tf2tools_app::i18n::apply("ru");
    let (u, fake) = fake_updater(cx, true);
    assert!(act(cx, &u, |u, cx| u.check(cx)), "the check starts");
    fake.next_check(cx)
        .send(Err(failure("Не удалось связаться с сервером.", false)))
        .unwrap();
    wait_state(cx, &u, "Failed", |s| matches!(s, UpdateState::Failed(_)));
    let log = logged(&buf);
    assert!(log.contains("update check failed"), "{log}");
    assert!(!log.chars().any(cyrillic), "{log}");
}
