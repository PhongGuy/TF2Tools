use std::sync::Arc;

use gpui_kit::{AnyWindowHandle, Entity, TestAppContext};
use tf2tools_app::app::{Boot, start};
use tf2tools_app::audio::RecordingAudio;
use tf2tools_app::i18n;
use tf2tools_app::ids;
use tf2tools_app::pages::settings::SettingsPage;
use tf2tools_app::picker::ScriptedPicker;
use tf2tools_app::services::Tf2State;
use tf2tools_app::shell::route::Page;

use crate::LocaleGuard;
use crate::support::{self, ShellWindow};

pub const NOTE_EN: &str = "Parts of TF2Tools not yet translated stay in English.";
pub const NOTE_RU: &str = "Ещё не переведённые части TF2Tools остаются на английском.";

/// Opens Settings through the sidebar and waits for its first load.
pub fn open_settings(cx: &mut TestAppContext, s: &ShellWindow) -> Entity<SettingsPage> {
    support::click_id(cx, s.win, ids::nav(Page::Settings));
    let page = cx
        .update(|cx| s.shell.read(cx).settings_page())
        .expect("a settings page");
    support::wait_real(cx, "settings to load", |cx| {
        cx.update(|cx| page.read(cx).is_loaded())
    });
    support::settle(cx, s.win);
    page
}

// Opens the Language select and confirms the row for `code`.
fn pick(cx: &mut TestAppContext, win: AnyWindowHandle, code: &str) {
    support::click_id(cx, win, ids::SETTINGS_LANGUAGE);
    support::click_id(cx, win, ids::settings_language(code));
    support::settle(cx, win);
}

fn note(cx: &mut TestAppContext, win: AnyWindowHandle) -> Option<String> {
    support::label_of(cx, win, ids::SETTINGS_LANGUAGE_NOTE)
}

fn boot(env: &support::Env) -> Boot {
    Boot {
        data: env.data.clone(),
        assets: Ok(support::repo_asset_root()),
        picker: Arc::new(ScriptedPicker::empty()),
        detector: support::no_steam(),
        audio: Arc::new(RecordingAudio::new()),
        endpoints: tf2tools_core::Endpoints::default(),
        auto_sync: false,
        offline: false,
        font_host: None,
        updates: tf2tools_app::updater::UpdateBoot::Off,
    }
}

#[gpui_kit::test]
fn picking_russian_switches_settings_at_once_and_saves_it(cx: &mut TestAppContext) {
    let _guard = LocaleGuard::hold();
    let env = support::env();
    let s = support::shell_window(cx, &env, Tf2State::Missing { saved: None });
    open_settings(cx, &s);
    assert_eq!(note(cx, s.win).as_deref(), Some(NOTE_EN));

    pick(cx, s.win, "ru");
    assert_eq!(note(cx, s.win).as_deref(), Some(NOTE_RU));
    assert_eq!(i18n::current(), "ru");
    support::wait_real(cx, "Russian saved", |_| {
        s.services.db.settings().unwrap().language() == "ru"
    });

    pick(cx, s.win, "en");
    assert_eq!(note(cx, s.win).as_deref(), Some(NOTE_EN));
    support::wait_real(cx, "English saved", |_| {
        s.services.db.settings().unwrap().language() == "en"
    });
}

#[gpui_kit::test]
fn a_language_saved_last_time_applies_at_startup(cx: &mut TestAppContext) {
    let _guard = LocaleGuard::hold();
    let env = support::env();
    support::services(&env, support::no_steam())
        .db
        .set_language("ru")
        .unwrap();
    support::init(cx);
    let win = cx.update(|cx| start(boot(&env), cx)).unwrap();
    support::wait_real(cx, "the shell", |cx| {
        support::present(cx, win, ids::page(Page::Huds))
    });
    assert_eq!(i18n::current(), "ru");
    support::click_id(cx, win, ids::nav(Page::Settings));
    support::wait_real(cx, "the Russian note", |cx| {
        note(cx, win).as_deref() == Some(NOTE_RU)
    });
}
