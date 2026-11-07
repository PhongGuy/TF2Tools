//! The banner offering to remove TF2Tools 4: it shows once, either answer is remembered, and
//! Remove runs v4's own uninstaller through the injected runner.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{AppContext as _, TestAppContext};
use tf2tools_app::ids;
use tf2tools_app::old_v4::{NOTICE, OldV4Notice, UninstallRunner, pending};
use tf2tools_app::services::Tf2State;
use tf2tools_core::{OldV4, UninstallEntry, find_old_v4};

use crate::support;

const V4_EXE: &str = r"C:\Users\pyro\AppData\Local\Programs\tf2tools\Uninstall TF2Tools.exe";

fn v4() -> OldV4 {
    let entry = UninstallEntry::new("v4")
        .display_name("TF2Tools 4.8.0")
        .display_version("4.8.0")
        .uninstall_string(format!(r#""{V4_EXE}" /currentuser"#));
    find_old_v4(&[entry], |_| true).unwrap()
}

type Calls = Arc<Mutex<Vec<(PathBuf, Vec<String>)>>>;

fn recording_runner() -> (UninstallRunner, Calls) {
    let calls: Calls = Arc::default();
    let seen = Arc::clone(&calls);
    let runner: UninstallRunner = Arc::new(move |exe: &Path, args: &[String]| {
        seen.lock()
            .unwrap()
            .push((exe.to_path_buf(), args.to_vec()));
        Ok(())
    });
    (runner, calls)
}

fn shell_with(
    cx: &mut TestAppContext,
    found: Option<OldV4>,
) -> (support::Env, support::ShellWindow, Calls) {
    let env = support::env();
    let (runner, calls) = recording_runner();
    cx.update(|cx| {
        OldV4Notice::install(cx, found, runner);
    });
    let s = support::shell_window(cx, &env, Tf2State::Missing { saved: None });
    (env, s, calls)
}

fn asked(s: &support::ShellWindow) -> bool {
    s.services.db.settings().unwrap().old_v4_asked()
}

#[gpui_kit::test]
fn no_banner_without_v4(cx: &mut TestAppContext) {
    let (_env, s, _) = shell_with(cx, None);
    assert!(!support::present(cx, s.win, ids::OLD_V4_BANNER));
}

#[gpui_kit::test]
fn the_banner_asks_in_plain_words(cx: &mut TestAppContext) {
    let (_env, s, _) = shell_with(cx, Some(v4()));
    let text = support::label_of(cx, s.win, ids::OLD_V4_BANNER_TEXT).unwrap();
    assert_eq!(text, NOTICE);
    assert_eq!(text, "The old version of TF2Tools is still installed.");
    support::assert_plain(&text);
    assert_eq!(
        support::label_of(cx, s.win, ids::OLD_V4_BANNER_REMOVE).as_deref(),
        Some("Remove it")
    );
    assert_eq!(
        support::label_of(cx, s.win, ids::OLD_V4_BANNER_LATER).as_deref(),
        Some("Not now")
    );
}

fn open(cx: &mut TestAppContext, s: &support::ShellWindow) -> bool {
    cx.update(|cx| cx.windows().contains(&s.win))
}

#[gpui_kit::test]
fn remove_first_says_tf2tools_will_close(cx: &mut TestAppContext) {
    let (_env, s, calls) = shell_with(cx, Some(v4()));
    support::click_id(cx, s.win, ids::OLD_V4_BANNER_REMOVE);
    assert!(support::present(cx, s.win, ids::CONFIRM_DIALOG));
    let text = support::label_of(cx, s.win, ids::CONFIRM_DESCRIPTION).unwrap();
    assert_eq!(
        text,
        "TF2Tools closes while the old version is removed. Open it again when it's done."
    );
    support::assert_plain(&text);
    assert_eq!(
        support::label_of(cx, s.win, ids::CONFIRM_OK).as_deref(),
        Some("Remove and close")
    );
    assert_eq!(
        support::label_of(cx, s.win, ids::CONFIRM_CANCEL).as_deref(),
        Some("Cancel")
    );
    assert!(calls.lock().unwrap().is_empty(), "nothing runs before OK");
}

#[gpui_kit::test]
fn cancel_leaves_everything_as_it_was(cx: &mut TestAppContext) {
    let (_env, s, calls) = shell_with(cx, Some(v4()));
    support::click_id(cx, s.win, ids::OLD_V4_BANNER_REMOVE);
    support::click_id(cx, s.win, ids::CONFIRM_CANCEL);
    cx.run_until_parked();
    assert!(calls.lock().unwrap().is_empty());
    assert!(open(cx, &s));
    assert!(support::present(cx, s.win, ids::OLD_V4_BANNER));
    assert!(!asked(&s), "Cancel is not an answer");
}

#[gpui_kit::test]
fn remove_and_close_starts_v4s_uninstaller_then_closes(cx: &mut TestAppContext) {
    let (_env, s, calls) = shell_with(cx, Some(v4()));
    // A second window keeps the test app alive when the shell's window goes.
    let _other = support::blank_window(cx);
    support::click_id(cx, s.win, ids::OLD_V4_BANNER_REMOVE);
    // A plain click, since the helper's settle afterwards would look for the closed window.
    support::settle(cx, s.win);
    cx.update_window(s.win, |_, w, cx| w.click(ids::CONFIRM_OK, cx))
        .unwrap();
    cx.run_until_parked();
    assert_eq!(
        *calls.lock().unwrap(),
        [(PathBuf::from(V4_EXE), vec!["/currentuser".to_owned()])]
    );
    assert!(asked(&s), "Remove is remembered");
    assert!(!open(cx, &s), "TF2Tools closes so the uninstaller can run");
}

#[gpui_kit::test]
fn not_now_hides_it_for_good_without_running_anything(cx: &mut TestAppContext) {
    let (_env, s, calls) = shell_with(cx, Some(v4()));
    support::click_id(cx, s.win, ids::OLD_V4_BANNER_LATER);
    cx.run_until_parked();
    assert!(calls.lock().unwrap().is_empty());
    assert!(!support::present(cx, s.win, ids::OLD_V4_BANNER));
    assert!(asked(&s), "Not now is remembered");
}

#[gpui_kit::test]
fn a_remembered_answer_skips_detection(cx: &mut TestAppContext) {
    let (_env, s, _) = shell_with(cx, None);
    let settings = s.services.db.settings().unwrap();
    assert_eq!(pending(&settings, || Some(v4())), Some(v4()));
    s.services.db.set_old_v4_asked().unwrap();
    let settings = s.services.db.settings().unwrap();
    assert_eq!(
        pending(&settings, || panic!("detection ran after an answer")),
        None
    );
}

#[gpui_kit::test]
fn a_failed_start_keeps_the_app_open_and_says_where_to_remove_it(cx: &mut TestAppContext) {
    let env = support::env();
    let runner: UninstallRunner =
        Arc::new(|_: &Path, _: &[String]| Err(std::io::Error::other("blocked")));
    cx.update(|cx| {
        OldV4Notice::install(cx, Some(v4()), runner);
    });
    let s = support::shell_window(cx, &env, Tf2State::Missing { saved: None });
    support::click_id(cx, s.win, ids::OLD_V4_BANNER_REMOVE);
    support::click_id(cx, s.win, ids::CONFIRM_OK);
    cx.run_until_parked();
    assert!(open(cx, &s), "a failed start keeps TF2Tools open");
    let toasts = support::toast_labels(cx, s.win);
    assert!(
        toasts.contains(
            &"Couldn't remove the old TF2Tools. Remove it in Windows Settings > Apps instead."
                .to_owned()
        ),
        "{toasts:?}"
    );
}

#[cfg(not(windows))]
#[gpui_kit::test]
fn nothing_shows_off_windows(cx: &mut TestAppContext) {
    let env = support::env();
    let settings = tf2tools_core::Database::open(&env.data)
        .unwrap()
        .settings()
        .unwrap();
    let found = pending(&settings, tf2tools_core::detect_old_v4);
    let (_env, s, _) = shell_with(cx, found);
    assert!(!support::present(cx, s.win, ids::OLD_V4_BANNER));
}
