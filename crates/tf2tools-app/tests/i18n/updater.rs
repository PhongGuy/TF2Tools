use gpui_kit::TestAppContext;
use tf2tools_app::ids;
use tf2tools_app::services::Tf2State;
use tf2tools_app::updater::backend::{describe_apply, describe_check, describe_download};
use tf2tools_core::{UpdateApplyError, UpdateCheckError, UpdateDownloadError};

use crate::LocaleGuard;
use crate::messages::assert_russian;
use crate::settings::open_settings;
use crate::support::{
    self,
    update_fake::{fake_updater, offer, reach_available},
};

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
        assert_russian(&failure.message);
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
