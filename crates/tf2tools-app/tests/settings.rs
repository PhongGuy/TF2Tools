use std::sync::Arc;

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{AppContext as _, Entity, ScrollDelta, TestAppContext, point, px};
use tf2tools_app::ids;
use tf2tools_app::pages::settings::{SettingsPage, format_hours};
use tf2tools_app::picker::{PickKind, ScriptedPicker};
use tf2tools_app::services::{Tf2State, require_tf2};
use tf2tools_app::shell::CONTENT_PADDING;
use tf2tools_app::shell::route::Page;
use tf2tools_core::{Tf2Install, Volume};

mod support;
use support::ShellWindow;

/// Opens Settings through the sidebar and waits for its first load.
fn open_settings(cx: &mut TestAppContext, s: &ShellWindow) -> Entity<SettingsPage> {
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

fn path_value(cx: &mut TestAppContext, page: &Entity<SettingsPage>) -> String {
    cx.update(|cx| page.read(cx).path_input().read(cx).value().to_string())
}

fn set_path(cx: &mut TestAppContext, s: &ShellWindow, page: &Entity<SettingsPage>, path: &str) {
    let input = cx.update(|cx| page.read(cx).path_input().clone());
    cx.update_window(s.win, |_, window, cx| {
        input.update(cx, |state, cx| state.set_value(path, window, cx))
    })
    .unwrap();
    support::settle(cx, s.win);
}

fn set_slider(
    cx: &mut TestAppContext,
    s: &ShellWindow,
    slider: impl Fn(&SettingsPage) -> Entity<gpui_kit::component::slider::SliderState>,
    page: &Entity<SettingsPage>,
    value: f32,
) {
    let slider = cx.update(|cx| slider(page.read(cx)));
    cx.update_window(s.win, |_, window, cx| {
        slider.update(cx, |state, cx| state.set_value(value, window, cx))
    })
    .unwrap();
    support::settle(cx, s.win);
}

/// Save sits below the fold in the 900 px test window, as in v5, so wheel the page down first.
fn click_save(cx: &mut TestAppContext, s: &ShellWindow) {
    support::settle(cx, s.win);
    cx.update_window(s.win, |_, w, cx| {
        let down = ScrollDelta::Pixels(point(px(0.), px(-2000.)));
        w.scroll(ids::page(Page::Settings), down, cx)
    })
    .unwrap();
    support::click_id(cx, s.win, ids::SETTINGS_SAVE);
}

#[test]
fn hours_format_like_v5() {
    assert_eq!(format_hours(1), "1 hour");
    assert_eq!(format_hours(5), "5 hours");
    assert_eq!(format_hours(24), "1 day");
    assert_eq!(format_hours(48), "2 days");
    assert_eq!(format_hours(168), "7 days");
    assert_eq!(format_hours(25), "1d 1h");
    assert_eq!(format_hours(720), "30 days");
}

#[gpui_kit::test]
fn settings_draws_every_id_and_loads_the_saved_values(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(
        cx,
        &env,
        Tf2State::Found(Tf2Install::from_path(&env.game).unwrap()),
    );
    s.services.db.set_tf2_path(Some(&env.game)).unwrap();
    s.services.db.set_volume(Volume::new(70).unwrap()).unwrap();
    s.services.db.set_sync_cache_hours(25).unwrap();
    assert!(!Page::Settings.is_placeholder());
    let page = open_settings(cx, &s);
    for id in [
        ids::SETTINGS_VERSION,
        ids::SETTINGS_PATH,
        ids::SETTINGS_BROWSE,
        ids::SETTINGS_DETECT,
        ids::SETTINGS_VOLUME,
        ids::SETTINGS_VOLUME_LABEL,
        ids::SETTINGS_CACHE,
        ids::SETTINGS_CACHE_LABEL,
        ids::SETTINGS_SAVE,
    ] {
        assert!(support::present(cx, s.win, id), "missing {id}");
    }
    assert!(support::present(cx, s.win, ids::page(Page::Settings)));
    assert!(!support::present(cx, s.win, ids::PLACEHOLDER_MESSAGE));
    assert_eq!(path_value(cx, &page), env.game.display().to_string());
    assert_eq!(
        support::label_of(cx, s.win, ids::SETTINGS_VOLUME_LABEL).as_deref(),
        Some("Volume: 70%")
    );
    assert_eq!(
        support::label_of(cx, s.win, ids::SETTINGS_CACHE_LABEL).as_deref(),
        Some("Cache duration: 1d 1h")
    );
    assert_eq!(
        support::label_of(cx, s.win, ids::SETTINGS_VERSION),
        Some(format!("Current version: v{}", env!("CARGO_PKG_VERSION")))
    );
}

#[gpui_kit::test]
fn a_saved_path_that_no_longer_works_is_shown_for_correction(cx: &mut TestAppContext) {
    let env = support::env();
    let gone = env.tmp.path().join("gone");
    let s = support::shell_window(
        cx,
        &env,
        Tf2State::Missing {
            saved: Some(gone.clone()),
        },
    );
    s.services.db.set_tf2_path(Some(&gone)).unwrap();
    let page = open_settings(cx, &s);
    assert_eq!(path_value(cx, &page), gone.display().to_string());
}

#[gpui_kit::test]
fn moving_a_slider_updates_its_label(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, Tf2State::Missing { saved: None });
    let page = open_settings(cx, &s);
    set_slider(cx, &s, |p| p.volume().clone(), &page, 30.);
    set_slider(cx, &s, |p| p.cache_hours().clone(), &page, 48.);
    assert_eq!(
        support::label_of(cx, s.win, ids::SETTINGS_VOLUME_LABEL).as_deref(),
        Some("Volume: 30%")
    );
    assert_eq!(
        support::label_of(cx, s.win, ids::SETTINGS_CACHE_LABEL).as_deref(),
        Some("Cache duration: 2 days")
    );
}

#[gpui_kit::test]
fn browse_fills_the_path_without_saving(cx: &mut TestAppContext) {
    let env = support::env();
    let picker = Arc::new(ScriptedPicker::new([env.game.clone()]));
    let s = support::shell_window_with(
        cx,
        &env,
        Tf2State::Missing { saved: None },
        picker.clone(),
        support::no_steam(),
    );
    let page = open_settings(cx, &s);
    support::click_id(cx, s.win, ids::SETTINGS_BROWSE);
    support::wait_real(cx, "the picked path", |cx| {
        path_value(cx, &page) == env.game.display().to_string()
    });
    assert_eq!(picker.asked(), vec![PickKind::Tf2Folder]);
    assert_eq!(s.services.db.settings().unwrap().tf2_path(), None);
}

#[gpui_kit::test]
fn a_cancelled_browse_keeps_the_path(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, Tf2State::Missing { saved: None });
    let page = open_settings(cx, &s);
    set_path(cx, &s, &page, "/typed/by/hand");
    support::click_id(cx, s.win, ids::SETTINGS_BROWSE);
    support::settle(cx, s.win);
    assert_eq!(path_value(cx, &page), "/typed/by/hand");
}

#[gpui_kit::test]
fn revisiting_settings_reloads_and_drops_unsaved_edits(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, Tf2State::Missing { saved: None });
    let page = open_settings(cx, &s);
    set_path(cx, &s, &page, "/unsaved");
    support::click_id(cx, s.win, ids::nav(Page::Huds));
    let page = open_settings(cx, &s);
    assert_eq!(path_value(cx, &page), "");
}

#[gpui_kit::test]
fn save_persists_everything_and_connects_tf2(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, Tf2State::Missing { saved: None });
    let page = open_settings(cx, &s);
    set_path(cx, &s, &page, &env.game.display().to_string());
    set_slider(cx, &s, |p| p.volume().clone(), &page, 30.);
    set_slider(cx, &s, |p| p.cache_hours().clone(), &page, 48.);
    click_save(cx, &s);
    support::wait_real(cx, "the saved path", |_| {
        s.services.db.settings().unwrap().tf2_path() == Some(env.game.as_path())
    });
    support::wait_real(cx, "the success toast", |cx| {
        support::present(cx, s.win, ids::TOAST)
    });
    let settings = s.services.db.settings().unwrap();
    assert_eq!(settings.volume().get(), 30);
    assert_eq!(settings.sync_cache_hours(), 48);
    assert_eq!(
        support::label_of(cx, s.win, ids::TOAST).as_deref(),
        Some("Settings saved")
    );
    assert_eq!(
        support::label_of(cx, s.win, ids::TF2_BADGE).as_deref(),
        Some("TF2 Connected")
    );
    let install = cx.update(|cx| require_tf2(cx)).unwrap();
    assert_eq!(install.game_dir(), env.game);
}

#[gpui_kit::test]
fn save_with_an_invalid_path_writes_nothing(cx: &mut TestAppContext) {
    let env = support::env();
    let found = Tf2State::Found(Tf2Install::from_path(&env.game).unwrap());
    let s = support::shell_window(cx, &env, found.clone());
    s.services.db.set_tf2_path(Some(&env.game)).unwrap();
    let page = open_settings(cx, &s);
    let nowhere = env.tmp.path().join("nowhere");
    set_path(cx, &s, &page, &nowhere.display().to_string());
    set_slider(cx, &s, |p| p.volume().clone(), &page, 10.);
    click_save(cx, &s);
    support::wait_real(cx, "the error toast", |cx| {
        support::present(cx, s.win, ids::TOAST)
    });
    let toast = support::label_of(cx, s.win, ids::TOAST).unwrap();
    assert_eq!(
        toast,
        "Failed to save. This folder isn't a TF2 install. Pick the folder TF2 is installed in, then try again."
    );
    let settings = s.services.db.settings().unwrap();
    assert_eq!(settings.tf2_path(), Some(env.game.as_path()));
    assert_eq!(settings.volume().get(), 50);
    assert_eq!(cx.update(|cx| s.tf2.read(cx).clone()), found);
}

#[gpui_kit::test]
fn save_with_an_empty_path_keeps_the_saved_path(cx: &mut TestAppContext) {
    let env = support::env();
    let found = Tf2State::Found(Tf2Install::from_path(&env.game).unwrap());
    let s = support::shell_window(cx, &env, found.clone());
    s.services.db.set_tf2_path(Some(&env.game)).unwrap();
    let page = open_settings(cx, &s);
    set_path(cx, &s, &page, "");
    set_slider(cx, &s, |p| p.volume().clone(), &page, 80.);
    click_save(cx, &s);
    support::wait_real(cx, "the saved volume", |_| {
        s.services.db.settings().unwrap().volume().get() == 80
    });
    assert_eq!(
        s.services.db.settings().unwrap().tf2_path(),
        Some(env.game.as_path())
    );
    assert_eq!(cx.update(|cx| s.tf2.read(cx).clone()), found);
}

#[gpui_kit::test]
fn detect_with_no_install_warns_and_keeps_the_path(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, Tf2State::Missing { saved: None });
    let page = open_settings(cx, &s);
    support::click_id(cx, s.win, ids::SETTINGS_DETECT);
    support::wait_real(cx, "the warning", |cx| {
        support::present(cx, s.win, ids::TOAST)
    });
    assert_eq!(
        support::label_of(cx, s.win, ids::TOAST).as_deref(),
        Some("TF2 not found. Select your TF2 folder manually")
    );
    assert_eq!(path_value(cx, &page), "");
    assert!(!support::present(cx, s.win, ids::SETTINGS_CANDIDATES));
}

#[gpui_kit::test]
fn detect_with_one_install_fills_the_path_without_saving(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window_with(
        cx,
        &env,
        Tf2State::Missing { saved: None },
        Arc::new(ScriptedPicker::empty()),
        support::steam_finds(env.game.clone()),
    );
    let page = open_settings(cx, &s);
    support::click_id(cx, s.win, ids::SETTINGS_DETECT);
    support::wait_real(cx, "the success toast", |cx| {
        support::present(cx, s.win, ids::TOAST)
    });
    assert_eq!(
        support::label_of(cx, s.win, ids::TOAST),
        Some(format!("TF2 found. {}", env.game.display()))
    );
    assert_eq!(path_value(cx, &page), env.game.display().to_string());
    assert!(!support::present(cx, s.win, ids::SETTINGS_CANDIDATES));
    assert_eq!(s.services.db.settings().unwrap().tf2_path(), None);
}

#[gpui_kit::test]
fn detect_with_several_installs_offers_a_choice(cx: &mut TestAppContext) {
    let env = support::env();
    let flatpak = env.tmp.path().join("Flatpak TF2");
    std::fs::create_dir_all(flatpak.join("tf/custom")).unwrap();
    let s = support::shell_window_with(
        cx,
        &env,
        Tf2State::Missing { saved: None },
        Arc::new(ScriptedPicker::empty()),
        support::steam_finds_all(vec![flatpak.clone(), env.game.clone()]),
    );
    let page = open_settings(cx, &s);
    support::click_id(cx, s.win, ids::SETTINGS_DETECT);
    support::wait_real(cx, "the success toast", |cx| {
        support::present(cx, s.win, ids::TOAST)
    });
    assert_eq!(
        support::label_of(cx, s.win, ids::TOAST).as_deref(),
        Some("TF2 found. 2 installs found; choose one below")
    );
    assert_eq!(path_value(cx, &page), flatpak.display().to_string());
    assert!(support::present(cx, s.win, ids::SETTINGS_CANDIDATES));
    support::click_id(cx, s.win, ids::SETTINGS_CANDIDATES);
    support::click_id(cx, s.win, ids::settings_candidate(1));
    assert_eq!(path_value(cx, &page), env.game.display().to_string());
    click_save(cx, &s);
    support::wait_real(cx, "the chosen path saved", |_| {
        s.services.db.settings().unwrap().tf2_path() == Some(env.game.as_path())
    });
}

#[gpui_kit::test]
fn a_detector_error_is_toasted(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window_with(
        cx,
        &env,
        Tf2State::Missing { saved: None },
        Arc::new(ScriptedPicker::empty()),
        support::failing_steam(),
    );
    let _page = open_settings(cx, &s);
    support::click_id(cx, s.win, ids::SETTINGS_DETECT);
    support::wait_real(cx, "the error toast", |cx| {
        support::present(cx, s.win, ids::TOAST)
    });
    let toast = support::label_of(cx, s.win, ids::TOAST).unwrap();
    assert_eq!(
        toast,
        "Auto-detect failed. Couldn't search your Steam library for TF2. Select your TF2 folder manually."
    );
}

#[gpui_kit::test]
fn save_survives_leaving_the_page(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, Tf2State::Missing { saved: None });
    let page = open_settings(cx, &s);
    set_path(cx, &s, &page, &env.game.display().to_string());
    // Save and leave in one update, and drop the test's handle, so the page is released
    // before the save's task resumes.
    cx.update_window(s.win, |_, window, cx| {
        page.update(cx, |page, cx| page.save(window, cx));
        s.shell
            .update(cx, |shell, cx| shell.navigate(Page::Huds, cx));
    })
    .unwrap();
    drop(page);
    support::wait_real(cx, "the saved path", |_| {
        s.services.db.settings().unwrap().tf2_path() == Some(env.game.as_path())
    });
    support::wait_real(cx, "the badge to connect", |cx| {
        support::label_of(cx, s.win, ids::TF2_BADGE).as_deref() == Some("TF2 Connected")
    });
    assert!(cx.update(|cx| s.shell.read(cx).settings_page()).is_none());
    let install = cx.update(|cx| require_tf2(cx)).unwrap();
    assert_eq!(install.game_dir(), env.game);
}

#[gpui_kit::test]
fn settings_scrolls_to_the_window_edge(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, Tf2State::Missing { saved: None });
    let _page = open_settings(cx, &s);
    let root = support::bounds_of(cx, s.win, ids::page(Page::Settings)).unwrap();
    assert_eq!(root.origin.y + root.size.height, px(900.));
    let path = support::bounds_of(cx, s.win, ids::SETTINGS_PATH).unwrap();
    assert!(path.origin.y >= root.origin.y + CONTENT_PADDING);
}

#[gpui_kit::test]
fn save_writes_the_settings_row(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, Tf2State::Missing { saved: None });
    let _page = open_settings(cx, &s);
    click_save(cx, &s);
    support::wait_real(cx, "the row", |_| support::log_rows(&s.services).len() == 1);
    let row = &support::log_rows(&s.services)[0];
    assert_eq!(
        (row.category(), row.action(), row.message()),
        ("settings", "update", "Settings updated")
    );
}

#[gpui_kit::test]
fn third_party_notices_open_from_settings(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, Tf2State::Missing { saved: None });
    open_settings(cx, &s);
    let notices = tf2tools_app::notices::all();
    assert!(!notices.is_empty());
    assert!(!support::present(cx, s.win, ids::SETTINGS_NOTICES_VIEW));
    cx.update_window(s.win, |_, w, cx| {
        let down = ScrollDelta::Pixels(point(px(0.), px(-4000.)));
        w.scroll(ids::page(Page::Settings), down, cx)
    })
    .unwrap();
    support::click_id(cx, s.win, ids::SETTINGS_NOTICES);
    assert!(support::present(cx, s.win, ids::SETTINGS_NOTICES_VIEW));
    for notice in &notices {
        assert_eq!(
            support::label_of(cx, s.win, ids::settings_notice(notice.file())).as_deref(),
            Some(notice.title()),
            "{} is not shown",
            notice.file()
        );
    }
    support::click_id(cx, s.win, ids::SETTINGS_NOTICES);
    assert!(!support::present(cx, s.win, ids::SETTINGS_NOTICES_VIEW));
}
