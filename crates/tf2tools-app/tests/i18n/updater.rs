use gpui_kit::{AppContext as _, TestAppContext};
use tf2tools_app::ids;
use tf2tools_app::services::Tf2State;
use tf2tools_app::updater::backend::{describe_apply, describe_check, describe_download};
use tf2tools_core::{UpdateApplyError, UpdateCheckError, UpdateDownloadError};

use crate::LocaleGuard;
use crate::messages::assert_russian;
use crate::settings::open_settings;
use crate::support::{
    self,
    update_fake::{
        act, fake_updater, offer, offer_with_notes, reach_available, reach_ready, sample_notes,
        update_rows, wait_state, with_services,
    },
};
use tf2tools_app::updater::{AfterInstall, NotesState, UpdateState, whats_new};

#[gpui_kit::test]
fn the_app_updates_card_speaks_russian(cx: &mut TestAppContext) {
    let _guard = LocaleGuard::switch("ru");
    let env = support::env();
    let (_updater, _fake) = fake_updater(cx, true);
    let s = support::shell_window(cx, &env, Tf2State::Missing { saved: None });
    open_settings(cx, &s);
    assert_eq!(
        support::label_of(cx, s.win, ids::SETTINGS_VERSION),
        Some(format!("Текущая версия: v{}", env!("CARGO_PKG_VERSION")))
    );
    assert_eq!(
        support::label_of(cx, s.win, ids::SETTINGS_UPDATE_STATUS).as_deref(),
        Some("Установлена последняя версия")
    );
    assert_eq!(
        support::label_of(cx, s.win, ids::SETTINGS_UPDATE_CHECK).as_deref(),
        Some("Проверить обновления")
    );
}

#[gpui_kit::test]
fn an_unavailable_build_says_so_in_russian(cx: &mut TestAppContext) {
    let _guard = LocaleGuard::switch("ru");
    let env = support::env();
    let (_updater, _fake) = fake_updater(cx, false);
    let s = support::shell_window(cx, &env, Tf2State::Missing { saved: None });
    open_settings(cx, &s);
    assert_eq!(
        support::label_of(cx, s.win, ids::SETTINGS_UPDATE_STATUS).as_deref(),
        Some(
            "В этой версии автоматические обновления недоступны. Новые версии есть на странице релизов."
        )
    );
    assert_eq!(
        support::label_of(cx, s.win, ids::SETTINGS_UPDATE_INSTALL).as_deref(),
        Some("Открыть страницу релиза")
    );
}

#[test]
fn update_failures_read_in_russian() {
    let _guard = LocaleGuard::switch("ru");
    let path = std::path::PathBuf::from("/tmp/x");
    let failures = [
        describe_check(&UpdateCheckError::BadSignature),
        describe_check(&UpdateCheckError::BadKey {
            message: "x".into(),
        }),
        describe_download(&UpdateDownloadError::Status {
            url: "u".into(),
            status: 503,
        }),
        describe_apply(&UpdateApplyError::Changed { path }),
    ];
    for failure in &failures {
        assert_russian(&failure.message.text());
    }
    assert_russian(&tf2tools_app::updater::reopen());
}

#[gpui_kit::test]
fn the_update_banner_speaks_russian(cx: &mut TestAppContext) {
    let _guard = LocaleGuard::switch("ru");
    let env = support::env();
    let (u, fake) = fake_updater(cx, true);
    let s = support::shell_window(cx, &env, Tf2State::Missing { saved: None });
    reach_available(cx, &u, &fake, offer("6.0.1", true));
    let text = support::label_of(cx, s.win, ids::UPDATE_BANNER_TEXT).unwrap();
    assert_russian(&text);
    assert!(text.contains("6.0.1"), "{text}");
}

// Restarts into a staged 6.0.1 update and returns the settings rows once both are written.
fn rows_after_restart(cx: &mut TestAppContext, after: AfterInstall) -> Vec<String> {
    let env = support::env();
    let services = with_services(cx, &env);
    let (u, fake) = fake_updater(cx, true);
    *fake.apply_result.lock().unwrap() = Ok(after);
    reach_ready(cx, &u, &fake, offer("6.0.1", true));
    assert!(act(cx, &u, |u, cx| u.restart(cx)), "the restart starts");
    support::wait_real(cx, "both rows", |_| update_rows(&services).len() == 2);
    update_rows(&services)
        .into_iter()
        .map(|(_, _, text)| text)
        .collect()
}

#[gpui_kit::test]
fn update_rows_are_written_in_russian(cx: &mut TestAppContext) {
    let _guard = LocaleGuard::switch("ru");
    let rows = rows_after_restart(cx, AfterInstall::Relaunch("/tmp/TF2Tools.AppImage".into()));
    for text in &rows {
        assert_russian(text);
        assert!(text.contains("v6.0.1"), "{text}");
    }
}

#[gpui_kit::test]
fn an_installer_row_is_written_in_russian(cx: &mut TestAppContext) {
    let _guard = LocaleGuard::switch("ru");
    let rows = rows_after_restart(cx, AfterInstall::Quit);
    assert!(rows.iter().any(|t| t.contains("установщик")), "{rows:?}");
    for text in &rows {
        assert_russian(text);
        assert!(text.contains("v6.0.1"), "{text}");
    }
}

// A failed check stays on the App Updates card until the next check, so its line must be worded
// when it is drawn and follow a switch made on the same page.
#[gpui_kit::test]
fn a_failed_update_check_follows_a_language_switch(cx: &mut TestAppContext) {
    let _guard = LocaleGuard::hold();
    let env = support::env();
    let (u, fake) = fake_updater(cx, true);
    let s = support::shell_window(cx, &env, Tf2State::Missing { saved: None });
    open_settings(cx, &s);
    act(cx, &u, |u, cx| u.check(cx));
    let offline = UpdateCheckError::Network {
        url: "https://example.invalid/latest.json".into(),
        message: "connection refused".into(),
    };
    fake.next_check(cx)
        .send(Err(describe_check(&offline)))
        .unwrap();
    wait_state(cx, &u, "Failed", |s| matches!(s, UpdateState::Failed(_)));
    support::settle(cx, s.win);
    assert_eq!(
        support::label_of(cx, s.win, ids::SETTINGS_UPDATE_STATUS).as_deref(),
        Some("Couldn't reach the update server. Check your internet connection and try again.")
    );

    crate::settings::pick(cx, s.win, "ru");
    assert_eq!(
        support::label_of(cx, s.win, ids::SETTINGS_UPDATE_STATUS).as_deref(),
        Some(
            "Не удалось связаться с сервером обновлений. Проверьте подключение к интернету и попробуйте снова."
        )
    );
}

#[gpui_kit::test]
fn the_dialog_says_the_notes_are_in_english_in_russian(cx: &mut TestAppContext) {
    let _guard = LocaleGuard::switch("ru");
    let env = support::env();
    let (u, fake) = fake_updater(cx, true);
    let s = support::shell_window(cx, &env, Tf2State::Missing { saved: None });
    let offer = offer_with_notes("6.0.2", true, sample_notes(NotesState::Complete));
    reach_available(cx, &u, &fake, offer.clone());
    cx.update_window(s.win, |_, window, cx| whats_new::open(offer, window, cx))
        .unwrap();
    support::settle(cx, s.win);
    assert_eq!(
        support::label_of(cx, s.win, ids::UPDATE_NOTES_ENGLISH).as_deref(),
        Some("Примечания к выпуску на английском.")
    );
    assert_eq!(
        support::label_of(cx, s.win, ids::update_notes_group(0, 0)).as_deref(),
        Some("Новые функции")
    );
    assert_eq!(
        support::label_of(cx, s.win, ids::update_notes_group(0, 2)).as_deref(),
        Some("Odds and ends"),
        "an Other group keeps its published title"
    );
    // These kinds' sample titles equal their English text, so only a translation proves the arm.
    for (r, g, want) in [
        (0, 1, "Производительность"),
        (1, 0, "Исправления"),
        (1, 1, "Удобство"),
        (1, 2, "Thanks"),
    ] {
        assert_eq!(
            support::label_of(cx, s.win, ids::update_notes_group(r, g)).as_deref(),
            Some(want),
            "group {r}.{g}"
        );
    }
    assert_russian(&support::label_of(cx, s.win, ids::UPDATE_NOTES_TITLE).unwrap());
}
