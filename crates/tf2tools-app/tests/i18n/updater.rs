use gpui_kit::TestAppContext;
use tf2tools_app::ids;
use tf2tools_app::services::Tf2State;

use crate::LocaleGuard;
use crate::settings::open_settings;
use crate::support::{self, update_fake::fake_updater};

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
