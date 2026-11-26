use std::sync::Arc;

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{AppContext as _, ElementId, Entity, ScrollDelta, TestAppContext, point, px};
use tf2tools_app::ids;
use tf2tools_app::pages::settings::{SettingsPage, format_hours};
use tf2tools_app::picker::{PickKind, ScriptedPicker};
use tf2tools_app::services::{Tf2State, require_tf2};
use tf2tools_app::shell::CONTENT_PADDING;
use tf2tools_app::shell::route::Page;
use tf2tools_app::widgets::scroll::BAR_WIDTH;
use tf2tools_core::{Tf2Install, Volume};

use crate::support;
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
        let down = ScrollDelta::Pixels(point(px(0.), px(-4000.)));
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
        ids::SETTINGS_STORAGE,
        ids::SETTINGS_RESYNC_ALL,
        ids::SETTINGS_MATURE_ADDONS,
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
        Arc::clone(&picker),
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
    // Only the titles in view are laid out, so scroll the box a few rows at a time until each
    // title has shown with its label.
    let mut unseen: Vec<_> = notices.iter().collect();
    for _ in 0..400 {
        unseen.retain(|notice| {
            support::label_of(cx, s.win, ids::settings_notice(notice.file())).as_deref()
                != Some(notice.title())
        });
        if unseen.is_empty() {
            break;
        }
        cx.update_window(s.win, |_, w, cx| {
            let down = ScrollDelta::Pixels(point(px(0.), px(-48.)));
            w.scroll(ids::SETTINGS_NOTICES_VIEW, down, cx)
        })
        .unwrap();
    }
    let unseen: Vec<_> = unseen.iter().map(|n| n.file()).collect();
    assert!(unseen.is_empty(), "never shown: {unseen:?}");
    support::click_id(cx, s.win, ids::SETTINGS_NOTICES);
    assert!(!support::present(cx, s.win, ids::SETTINGS_NOTICES_VIEW));
}

/// The notices run to thousands of lines, so opening them lays out only the rows in view, the
/// short credits' lines and titles as well as the crate notices'.
#[gpui_kit::test]
fn the_notices_lay_out_only_the_visible_lines(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, Tf2State::Missing { saved: None });
    let page = open_settings(cx, &s);
    let notices = tf2tools_app::notices::all();
    assert!(
        notices.iter().any(|n| n.is_long()),
        "the crate notices are long"
    );
    cx.update_window(s.win, |_, w, cx| {
        let down = ScrollDelta::Pixels(point(px(0.), px(-4000.)));
        w.scroll(ids::page(Page::Settings), down, cx)
    })
    .unwrap();
    support::click_id(cx, s.win, ids::SETTINGS_NOTICES);
    support::settle(cx, s.win);
    let rows = cx.update(|cx| page.read(cx).notice_lines());
    let lines: Vec<_> = (notices.iter())
        .map(|n| rows.iter().filter(|(file, _)| file == n.file()).count())
        .collect();
    let total: usize = lines.iter().sum();
    assert!(total > 5_000, "{total} lines");
    let (rendered, titles, first) = cx
        .update_window(s.win, |_, w, _| {
            let shown = |notice: &tf2tools_app::notices::Notice, count: usize| {
                (0..count)
                    .filter(|&i| {
                        w.try_find(ids::settings_notice_line(notice.file(), i))
                            .is_some()
                    })
                    .count()
            };
            let rendered: usize = notices.iter().zip(&lines).map(|(n, &c)| shown(n, c)).sum();
            let titles = (notices.iter())
                .filter(|n| w.try_find(ids::settings_notice(n.file())).is_some())
                .count();
            let first = w.try_find(ids::settings_notice_line(notices[0].file(), 0));
            (rendered, titles, first.is_some())
        })
        .unwrap();
    assert!(
        first,
        "the first line of {} is not a row",
        notices[0].file()
    );
    assert!(rendered < 100, "{rendered} of {total} lines laid out");
    assert!(titles < notices.len(), "all {titles} titles laid out");
}

/// At the smallest window the notices rows wrap to the box: no line runs under the bar, and
/// every word of each short notice is still there in order.
#[gpui_kit::test]
fn the_notices_wrap_to_the_box_at_the_smallest_window(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, Tf2State::Missing { saved: None });
    cx.simulate_window_resize(s.win, gpui_kit::size(px(800.), px(600.)));
    let page = open_settings(cx, &s);
    cx.update_window(s.win, |_, w, cx| {
        let down = ScrollDelta::Pixels(point(px(0.), px(-4000.)));
        w.scroll(ids::page(Page::Settings), down, cx)
    })
    .unwrap();
    support::click_id(cx, s.win, ids::SETTINGS_NOTICES);
    support::settle(cx, s.win);
    let rows = cx.update(|cx| page.read(cx).notice_lines());
    let notices = tf2tools_app::notices::all();
    for notice in notices.iter().filter(|n| !n.is_long()) {
        // A word wider than a row is cut across rows, so compare the text without its spaces.
        let shown: String = (rows.iter())
            .filter(|(file, _)| file == notice.file())
            .flat_map(|(_, text)| text.split_whitespace())
            .collect();
        let words: String = notice.text().split_whitespace().collect();
        assert_eq!(shown, words, "{} lost words", notice.file());
    }
    // Walk the box a few rows at a time so every short notice's rows are laid out once, each
    // ending left of the bar's track.
    let mut counts = std::collections::HashMap::<&str, usize>::new();
    let short: Vec<_> = (rows.iter())
        .take_while(|(file, _)| notices.iter().any(|n| !n.is_long() && n.file() == file))
        .map(|(file, _)| {
            let row = counts.entry(file.as_str()).or_default();
            *row += 1;
            ids::settings_notice_line(file, *row - 1)
        })
        .collect();
    let mut seen = vec![false; short.len()];
    for _ in 0..400 {
        support::settle(cx, s.win);
        let (bar, found) = cx
            .update_window(s.win, |_, w, _| {
                let bar = w
                    .try_find(ids::SETTINGS_NOTICES_SCROLLBAR)
                    .unwrap()
                    .bounds();
                let found: Vec<_> = (short.iter())
                    .map(|id| w.try_find(id.clone()).map(|e| e.bounds()))
                    .collect();
                (bar, found)
            })
            .unwrap();
        for (i, line) in found.iter().enumerate() {
            if let Some(line) = line {
                assert!(
                    line.right() <= bar.right() - BAR_WIDTH + px(1.),
                    "{:?} runs under the bar: {line:?} vs {bar:?}",
                    short[i]
                );
                seen[i] = true;
            }
        }
        if seen.iter().all(|&s| s) {
            break;
        }
        cx.update_window(s.win, |_, w, cx| {
            let down = ScrollDelta::Pixels(point(px(0.), px(-160.)));
            w.scroll(ids::SETTINGS_NOTICES_VIEW, down, cx)
        })
        .unwrap();
    }
    let missed = seen.iter().filter(|&&s| !s).count();
    assert_eq!(missed, 0, "{missed} short notice rows were never laid out");
}

#[gpui_kit::test]
fn the_add_ons_card_toggles_mature_add_ons_and_saves_at_once(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(
        cx,
        &env,
        Tf2State::Found(Tf2Install::from_path(&env.game).unwrap()),
    );
    let _page = open_settings(cx, &s);
    assert_eq!(
        support::checked_of(cx, s.win, ids::SETTINGS_MATURE_ADDONS),
        Some(false)
    );
    scroll_settings_to(cx, &s, ids::SETTINGS_MATURE_ADDONS.into());
    support::click_id(cx, s.win, ids::SETTINGS_MATURE_ADDONS);
    support::wait_real(cx, "the save", |_| {
        s.services.db.settings().unwrap().show_mature_addons()
    });
    assert_eq!(
        support::checked_of(cx, s.win, ids::SETTINGS_MATURE_ADDONS),
        Some(true)
    );
}

#[gpui_kit::test]
fn the_logs_tf_card_toggles_nicknames_in_match_logs_and_saves_at_once(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(
        cx,
        &env,
        Tf2State::Found(Tf2Install::from_path(&env.game).unwrap()),
    );
    let _page = open_settings(cx, &s);
    support::wait_real(cx, "the saved value", |cx| {
        cx.update(|cx| {
            let nicknames = tf2tools_app::log_nicknames::LogNicknames::global(cx);
            nicknames.read(cx).known()
        })
    });
    assert_eq!(
        support::checked_of(cx, s.win, ids::SETTINGS_LOG_NICKNAMES),
        Some(true)
    );
    scroll_settings_to(cx, &s, ids::SETTINGS_LOG_NICKNAMES.into());
    support::click_id(cx, s.win, ids::SETTINGS_LOG_NICKNAMES);
    support::wait_real(cx, "the save", |_| {
        !s.services.db.settings().unwrap().show_log_nicknames()
    });
    assert_eq!(
        support::checked_of(cx, s.win, ids::SETTINGS_LOG_NICKNAMES),
        Some(false)
    );
}

#[test]
fn log_space_reads_in_kilobytes_or_megabytes() {
    use tf2tools_app::pages::settings::{keep_text, size_text, usage_text};
    use tf2tools_core::KeepFullLogs;
    assert_eq!(size_text(0), "0 B");
    assert_eq!(size_text(10_300), "10.1 KB");
    assert_eq!(size_text(640_499), "625.5 KB");
    assert_eq!(size_text(1_048_576), "1.0 MB");
    assert_eq!(size_text(12_345_678), "11.8 MB");
    assert_eq!(keep_text(KeepFullLogs::Day1, 0), "1 day");
    assert_eq!(keep_text(KeepFullLogs::Days7, 0), "7 days");
    assert_eq!(
        keep_text(KeepFullLogs::Days3, 12_345_678),
        "3 days, frees about 11.8 MB"
    );
    assert_eq!(keep_text(KeepFullLogs::Forever, 0), "Forever");
    assert_eq!(
        usage_text(25_000_000, 1, 10_300),
        "The logs cache uses about 23.8 MB; 1 full log uses about 10.1 KB."
    );
    assert_eq!(
        usage_text(25_000_000, 21, 216_300),
        "The logs cache uses about 23.8 MB; 21 full logs use about 211.2 KB."
    );
}

// With no full log kept, the line says so instead of counting zero logs and their empty table.
#[test]
fn an_empty_logs_cache_says_nothing_is_cached() {
    use tf2tools_app::pages::settings::usage_text;
    assert_eq!(
        usage_text(40_960, 0, 4_096),
        "No full logs are cached yet. The logs cache uses about 40.0 KB."
    );
}

/// Waits until the library and trim jobs both rest, then returns the library job's passes.
fn jobs_idle(cx: &mut TestAppContext) -> u64 {
    use tf2tools_app::library_pack::LibraryPack;
    use tf2tools_app::logs_trim::LogsTrim;
    support::wait_real(cx, "the library and trim jobs to rest", |cx| {
        cx.update(|cx| {
            let (pack, trim) = (LibraryPack::global(cx), LogsTrim::global(cx));
            let (pack, trim) = (pack.read(cx), trim.read(cx));
            pack.passes() > 0 && !pack.is_running() && trim.passes() > 0 && !trim.is_running()
        })
    });
    cx.update(|cx| {
        tf2tools_app::library_pack::LibraryPack::global(cx)
            .read(cx)
            .passes()
    })
}

fn pack_passes(cx: &mut TestAppContext) -> u64 {
    cx.update(|cx| {
        tf2tools_app::library_pack::LibraryPack::global(cx)
            .read(cx)
            .passes()
    })
}

fn request_trim(cx: &mut TestAppContext) -> u64 {
    use tf2tools_app::logs_trim::LogsTrim;
    cx.update(|cx| {
        let trim = LogsTrim::global(cx);
        let before = trim.read(cx).passes();
        trim.update(cx, |t, cx| t.request(cx));
        before
    })
}

#[gpui_kit::test]
fn storage_is_measured_again_when_a_trim_lands_or_a_sync_ends(cx: &mut TestAppContext) {
    use tf2tools_app::sync::SyncSummary;
    let env = support::env();
    let (hub, runs) = support::fake_hub(cx);
    let s = support::shell_window(cx, &env, Tf2State::Missing { saved: None });
    let _page = open_settings(cx, &s);
    let before = jobs_idle(cx);
    request_trim(cx);
    support::wait_real(cx, "a measure after the trim", |cx| {
        pack_passes(cx) > before
    });
    let before = jobs_idle(cx);
    assert!(cx.update(|cx| hub.update(cx, |h, cx| h.start(
        tf2tools_core::StoreKind::Sounds,
        false,
        cx
    ))));
    runs.try_recv().unwrap().finish(Ok(SyncSummary::default()));
    support::wait_hub(cx, "a measure after the sync", |cx| {
        pack_passes(cx) > before
    });
}

#[gpui_kit::test]
fn a_hidden_settings_page_measures_nothing_on_a_trim_or_a_sync(cx: &mut TestAppContext) {
    use tf2tools_app::logs_trim::LogsTrim;
    use tf2tools_app::sync::SyncSummary;
    let env = support::env();
    let (hub, runs) = support::fake_hub(cx);
    let s = support::shell_window(cx, &env, Tf2State::Missing { saved: None });
    let _page = open_settings(cx, &s);
    support::click_id(cx, s.win, ids::nav(Page::Huds));
    let before = jobs_idle(cx);
    let trims = request_trim(cx);
    support::wait_real(cx, "the trim to land", |cx| {
        cx.update(|cx| {
            let trim = LogsTrim::global(cx);
            let trim = trim.read(cx);
            trim.passes() > trims && !trim.is_running()
        })
    });
    assert!(cx.update(|cx| hub.update(cx, |h, cx| h.start(
        tf2tools_core::StoreKind::Sounds,
        false,
        cx
    ))));
    runs.try_recv().unwrap().finish(Ok(SyncSummary::default()));
    support::wait_hub(cx, "the sync to end", |cx| {
        cx.update(|cx| !hub.read(cx).is_running(tf2tools_core::StoreKind::Sounds))
    });
    support::settle(cx, s.win);
    assert_eq!(pack_passes(cx), before, "a hidden page asks for no measure");
}

// Wheels the Settings page a notch at a time until all of `id` lies inside it.
fn scroll_settings_to(cx: &mut TestAppContext, s: &support::ShellWindow, id: ElementId) {
    for _ in 0..120 {
        let page = support::bounds_of(cx, s.win, ids::page(Page::Settings)).unwrap();
        let b = support::bounds_of(cx, s.win, id.clone()).unwrap();
        if b.top() >= page.top() && b.bottom() <= page.bottom() {
            return;
        }
        let step = if b.top() < page.top() { 1. } else { -1. };
        let turn = ScrollDelta::Lines(point(0., step));
        cx.update_window(s.win, |_, w, cx| {
            w.scroll(ids::page(Page::Settings), turn, cx)
        })
        .unwrap();
        support::settle(cx, s.win);
    }
    panic!("{id:?} never scrolled into the Settings page");
}

#[gpui_kit::test]
fn the_logs_tf_card_keeps_full_logs_for_the_chosen_days_and_saves_at_once(cx: &mut TestAppContext) {
    use tf2tools_app::logs_trim::LogsTrim;
    use tf2tools_core::{KeepFullLogs, LogsCache};
    let env = support::env();
    let s = support::shell_window(
        cx,
        &env,
        Tf2State::Found(Tf2Install::from_path(&env.game).unwrap()),
    );
    let _page = open_settings(cx, &s);
    support::wait_real(cx, "the logs cache figures", |cx| {
        cx.update(|cx| LogsTrim::global(cx).read(cx).space().is_some())
    });
    support::settle(cx, s.win);
    scroll_settings_to(cx, &s, ids::SETTINGS_LOG_KEEP.into());
    scroll_settings_to(cx, &s, ids::SETTINGS_LOG_SPACE.into());
    let usage = support::label_of(cx, s.win, ids::SETTINGS_LOG_SPACE).unwrap_or_default();
    assert!(
        usage.starts_with("No full logs are cached yet. The logs cache uses about "),
        "{usage}"
    );
    support::click_id(cx, s.win, ids::SETTINGS_LOG_KEEP);
    for keep in KeepFullLogs::ALL {
        assert!(
            support::present(cx, s.win, ids::settings_log_keep(keep.key())),
            "{keep:?} row"
        );
    }
    support::click_id(cx, s.win, ids::settings_log_keep("30d"));
    support::wait_real(cx, "the save", |_| {
        LogsCache::new(s.services.db.clone())
            .keep_full_logs()
            .unwrap()
            == KeepFullLogs::Days30
    });
}

#[gpui_kit::test]
fn the_logs_tf_card_says_when_the_cache_size_could_not_be_measured(cx: &mut TestAppContext) {
    use tf2tools_app::logs_trim::LogsTrim;
    let env = support::env();
    let s = support::shell_window(cx, &env, Tf2State::Missing { saved: None });
    // A plain body the pass must recompress, and a full disk that refuses the write.
    support::sql(&env)
        .execute_batch(
            "INSERT INTO logstf_logs (id, title, map, date, body) VALUES (1, 't', 'm', 0, 'plain');
             CREATE TRIGGER no_write BEFORE UPDATE ON logstf_logs
             BEGIN SELECT RAISE(ABORT, 'disk full'); END;",
        )
        .unwrap();
    let _page = open_settings(cx, &s);
    support::wait_real(cx, "the failed pass", |cx| {
        cx.update(|cx| {
            let trim = LogsTrim::global(cx).read(cx);
            trim.passes() > 0 && !trim.is_running()
        })
    });
    support::settle(cx, s.win);
    scroll_settings_to(cx, &s, ids::SETTINGS_LOG_SPACE.into());
    assert_eq!(
        support::label_of(cx, s.win, ids::SETTINGS_LOG_SPACE).as_deref(),
        Some("The logs cache size could not be measured.")
    );
}

#[test]
fn the_storage_rows_read_their_size_and_any_unpacked_size() {
    use tf2tools_app::pages::settings::{
        size_text, storage_name, storage_row_label, storage_total_label,
    };
    use tf2tools_core::StorageKind;
    assert_eq!(size_text(1_073_689_395), "1023.9 MB");
    assert_eq!(size_text(1_073_689_396), "1.0 GB");
    let names: Vec<String> = StorageKind::ALL
        .iter()
        .map(|k| storage_name(*k).to_string())
        .collect();
    assert_eq!(
        names,
        [
            "HUDs",
            "Sounds",
            "Crosshairs",
            "Add-ons",
            "logs.tf cache",
            "Store data",
            "Image cache",
            "Demo cache",
            "Temporary files"
        ]
    );
    assert_eq!(
        storage_row_label("HUDs", 412_000_000, 2_100_000_000),
        "HUDs: 392.9 MB. 2.0 GB unpacked"
    );
    assert_eq!(
        storage_row_label("Sounds", 1_048_576, 1_048_576),
        "Sounds: 1.0 MB"
    );
    assert_eq!(storage_total_label(2_100_000_000), "Total: 2.0 GB");
}

#[test]
fn every_storage_kind_has_its_own_colour() {
    use tf2tools_app::pages::settings::STORAGE_ROWS;
    let kinds: Vec<_> = STORAGE_ROWS.iter().map(|r| r.0).collect();
    assert_eq!(kinds, tf2tools_core::StorageKind::ALL);
    let mut colours: Vec<u32> = STORAGE_ROWS.iter().map(|r| r.2).collect();
    colours.sort_unstable();
    colours.dedup();
    assert_eq!(colours.len(), 9, "{colours:x?}");
}

#[test]
fn settings_sizes_read_like_store_sizes() {
    use tf2tools_app::pages::settings::size_text;
    use tf2tools_app::store_download::format_bytes;
    let mut bytes = 0_u64;
    while bytes < 1 << 42 {
        assert_eq!(
            size_text(bytes),
            format_bytes(bytes).as_str(),
            "{bytes} bytes"
        );
        bytes = bytes * 3 / 2 + 7;
    }
}

#[gpui_kit::test]
fn the_storage_card_lists_every_kind_with_its_size(cx: &mut TestAppContext) {
    use tf2tools_app::library_pack::LibraryPack;
    use tf2tools_app::pages::settings::{storage_name, storage_row_label, storage_total_label};
    use tf2tools_core::StorageKind;
    let env = support::env();
    let s = support::shell_window(cx, &env, Tf2State::Missing { saved: None });
    let archive = env.tmp.path().join("rayshud.zip");
    std::fs::write(&archive, support::hud_zip_bytes("rayshud")).unwrap();
    s.services
        .library
        .import_hud(&archive, tf2tools_core::Provenance::Local)
        .unwrap();
    let _page = open_settings(cx, &s);
    jobs_idle(cx);
    support::settle(cx, s.win);
    scroll_settings_to(cx, &s, ids::SETTINGS_STORAGE.into());
    let landed = cx
        .update(|cx| LibraryPack::global(cx).read(cx).space())
        .expect("figures");
    for kind in StorageKind::ALL {
        let want = storage_row_label(
            &storage_name(kind),
            landed.bytes(kind),
            landed.unpacked(kind),
        );
        assert_eq!(
            support::label_of(cx, s.win, ids::settings_storage(kind)).as_deref(),
            Some(want.as_str()),
            "{kind:?}"
        );
    }
    let total = storage_total_label(landed.total());
    assert_eq!(
        support::label_of(cx, s.win, ids::SETTINGS_STORAGE_TOTAL).as_deref(),
        Some(total.as_str())
    );
    assert_eq!(
        support::label_of(cx, s.win, ids::SETTINGS_STORAGE).as_deref(),
        Some(total.as_str())
    );
    let huds = support::label_of(cx, s.win, ids::settings_storage(StorageKind::Huds)).unwrap();
    assert!(
        huds.ends_with(" unpacked"),
        "an imported HUD is packed: {huds}"
    );
    assert_eq!(
        landed.bytes(StorageKind::Huds),
        s.services
            .library
            .storage()
            .unwrap()
            .bytes(StorageKind::Huds)
    );
}

#[gpui_kit::test]
fn the_storage_card_counts_every_file_in_the_demo_folder(cx: &mut TestAppContext) {
    use tf2tools_app::library_pack::LibraryPack;
    use tf2tools_app::pages::settings::{storage_name, storage_row_label};
    use tf2tools_core::StorageKind;
    let env = support::env();
    let demos = env.data.demo_cache_dir();
    std::fs::create_dir_all(&demos).unwrap();
    std::fs::write(demos.join("7.dem"), [1u8; 3000]).unwrap();
    std::fs::write(demos.join(".8.dem.abc123.part"), [2u8; 500]).unwrap();
    let s = support::shell_window(cx, &env, Tf2State::Missing { saved: None });
    let _page = open_settings(cx, &s);
    jobs_idle(cx);
    support::settle(cx, s.win);
    scroll_settings_to(cx, &s, ids::settings_storage(StorageKind::DemoCache));
    let landed = cx
        .update(|cx| LibraryPack::global(cx).read(cx).space())
        .expect("figures");
    assert_eq!(landed.bytes(StorageKind::DemoCache), 3500);
    let want = storage_row_label(&storage_name(StorageKind::DemoCache), 3500, 3500);
    assert_eq!(
        support::label_of(cx, s.win, ids::settings_storage(StorageKind::DemoCache)).as_deref(),
        Some(want.as_str())
    );
}

#[gpui_kit::test]
fn a_kind_that_takes_nothing_keeps_its_row_but_gets_no_slice(cx: &mut TestAppContext) {
    use tf2tools_app::library_pack::LibraryPack;
    use tf2tools_app::pages::settings::{storage_name, storage_row_label, storage_slices};
    use tf2tools_core::StorageKind;
    let env = support::env();
    let s = support::shell_window(cx, &env, Tf2State::Missing { saved: None });
    let _page = open_settings(cx, &s);
    jobs_idle(cx);
    support::settle(cx, s.win);
    scroll_settings_to(cx, &s, ids::SETTINGS_STORAGE.into());
    let landed = cx
        .update(|cx| LibraryPack::global(cx).read(cx).space())
        .expect("figures");
    // A fresh data dir holds no sounds, so that kind measures nothing.
    assert_eq!(landed.bytes(StorageKind::Sounds), 0);
    let want = storage_row_label(&storage_name(StorageKind::Sounds), 0, 0);
    assert_eq!(
        support::label_of(cx, s.win, ids::settings_storage(StorageKind::Sounds)).as_deref(),
        Some(want.as_str())
    );
    let sliced: Vec<StorageKind> = storage_slices(&landed).iter().map(|d| d.0).collect();
    assert!(!sliced.contains(&StorageKind::Sounds), "{sliced:?}");
    let nonzero: Vec<StorageKind> = StorageKind::ALL
        .into_iter()
        .filter(|k| landed.bytes(*k) > 0)
        .collect();
    assert_eq!(sliced, nonzero);
}

// Folder permissions that refuse a listing are a Unix feature.
#[cfg(unix)]
#[gpui_kit::test]
fn the_storage_card_says_when_storage_could_not_be_measured(cx: &mut TestAppContext) {
    use std::os::unix::fs::PermissionsExt as _;
    use tf2tools_app::library_pack::LibraryPack;
    let env = support::env();
    let s = support::shell_window(cx, &env, Tf2State::Missing { saved: None });
    let locked = env.data.library_dir(tf2tools_core::LibraryKind::Huds);
    std::fs::create_dir_all(locked.join("x")).unwrap();
    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o000)).unwrap();
    let unlock = Unlock(locked.clone());
    // Root reads any folder, so there is nothing to refuse; the test cannot run that way.
    if std::fs::read_dir(&locked).is_ok() {
        eprintln!("skipped: this user can list a folder with no permissions");
        return;
    }
    let _page = open_settings(cx, &s);
    jobs_idle(cx);
    drop(unlock);
    assert!(cx.update(|cx| LibraryPack::global(cx).read(cx).failed()));
    support::settle(cx, s.win);
    scroll_settings_to(cx, &s, ids::SETTINGS_STORAGE.into());
    assert_eq!(
        support::label_of(cx, s.win, ids::SETTINGS_STORAGE).as_deref(),
        Some("Storage use could not be measured.")
    );
    assert!(!support::present(cx, s.win, ids::SETTINGS_STORAGE_TOTAL));
}

// Folder permissions that refuse a listing are a Unix feature.
#[cfg(unix)]
#[gpui_kit::test]
fn a_failed_later_measure_keeps_the_last_figures_on_the_storage_rows(cx: &mut TestAppContext) {
    use std::os::unix::fs::PermissionsExt as _;
    use tf2tools_app::library_pack::LibraryPack;
    use tf2tools_app::pages::settings::{storage_name, storage_row_label, storage_total_label};
    use tf2tools_core::StorageKind;
    let env = support::env();
    let s = support::shell_window(cx, &env, Tf2State::Missing { saved: None });
    let _page = open_settings(cx, &s);
    let before = jobs_idle(cx);
    let landed = cx
        .update(|cx| LibraryPack::global(cx).read(cx).space())
        .expect("figures");
    let locked = env.data.library_dir(tf2tools_core::LibraryKind::Huds);
    std::fs::create_dir_all(locked.join("x")).unwrap();
    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o000)).unwrap();
    let _unlock = Unlock(locked.clone());
    // Root reads any folder, so there is nothing to refuse; the test cannot run that way.
    if std::fs::read_dir(&locked).is_ok() {
        eprintln!("skipped: this user can list a folder with no permissions");
        return;
    }
    cx.update(|cx| LibraryPack::global(cx).update(cx, |job, cx| job.measure(cx)));
    support::wait_real(cx, "the failed re-measure", |cx| {
        cx.update(|cx| {
            let pack = LibraryPack::global(cx).read(cx);
            pack.passes() > before && !pack.is_running()
        })
    });
    assert!(cx.update(|cx| LibraryPack::global(cx).read(cx).failed()));
    support::settle(cx, s.win);
    scroll_settings_to(cx, &s, ids::SETTINGS_STORAGE.into());
    for kind in StorageKind::ALL {
        let want = storage_row_label(
            &storage_name(kind),
            landed.bytes(kind),
            landed.unpacked(kind),
        );
        assert_eq!(
            support::label_of(cx, s.win, ids::settings_storage(kind)).as_deref(),
            Some(want.as_str()),
            "{kind:?}"
        );
    }
    let total = storage_total_label(landed.total());
    assert_eq!(
        support::label_of(cx, s.win, ids::SETTINGS_STORAGE).as_deref(),
        Some(total.as_str())
    );
}

// Gives a locked folder its permissions back however a test ends, so the temp dir can go.
#[cfg(unix)]
struct Unlock(std::path::PathBuf);

#[cfg(unix)]
impl Drop for Unlock {
    fn drop(&mut self) {
        use std::os::unix::fs::PermissionsExt as _;
        let _ = std::fs::set_permissions(&self.0, std::fs::Permissions::from_mode(0o755));
    }
}

#[gpui_kit::test]
fn storage_logs_tf_and_stores_follow_audio_in_that_order(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, Tf2State::Missing { saved: None });
    let page = open_settings(cx, &s);
    support::wait_real(cx, "the store status", |cx| {
        cx.update(|cx| page.read(cx).is_sync_loaded())
    });
    jobs_idle(cx);
    support::settle(cx, s.win);
    let order: [ElementId; 7] = [
        ids::SETTINGS_VOLUME.into(),
        ids::SETTINGS_STORAGE.into(),
        ids::SETTINGS_LOG_NICKNAMES.into(),
        ids::SETTINGS_RESYNC_ALL.into(),
        ids::SETTINGS_MATURE_ADDONS.into(),
        ids::SETTINGS_SAVE.into(),
        ids::SETTINGS_NOTICES.into(),
    ];
    let tops: Vec<_> = order
        .iter()
        .map(|id| {
            support::bounds_of(cx, s.win, id.clone())
                .expect("drawn")
                .top()
        })
        .collect();
    assert!(
        tops.windows(2).all(|w| w[0] < w[1]),
        "{order:?} at {tops:?}"
    );
}

// Fills the demo cache before the app starts: two finished demos, one in-flight part file.
fn seed_demos(env: &support::Env) -> std::path::PathBuf {
    let demos = env.data.demo_cache_dir();
    std::fs::create_dir_all(&demos).unwrap();
    std::fs::write(demos.join("1.dem"), [1u8; 3000]).unwrap();
    std::fs::write(demos.join("22.dem"), [1u8; 2000]).unwrap();
    std::fs::write(demos.join(".5.dem.abc123.part"), [2u8; 100]).unwrap();
    demos
}

fn demo_bytes(cx: &mut TestAppContext) -> u64 {
    use tf2tools_app::library_pack::LibraryPack;
    cx.update(|cx| LibraryPack::global(cx).read(cx).space())
        .expect("figures")
        .bytes(tf2tools_core::StorageKind::DemoCache)
}

#[gpui_kit::test]
fn clear_demo_cache_asks_then_deletes_finished_demos_and_measures_again(cx: &mut TestAppContext) {
    use tf2tools_app::pages::settings::size_text;
    use tf2tools_core::StorageKind;
    let env = support::env();
    let demos = seed_demos(&env);
    let s = support::shell_window(cx, &env, Tf2State::Missing { saved: None });
    let _page = open_settings(cx, &s);
    jobs_idle(cx);
    support::settle(cx, s.win);
    assert_eq!(demo_bytes(cx), 5100);
    let clear = ids::settings_storage_clear(StorageKind::DemoCache);
    scroll_settings_to(cx, &s, clear.clone());
    assert_eq!(
        support::label_of(cx, s.win, clear.clone()).as_deref(),
        Some("Clear demo cache")
    );
    assert!(!support::disabled_of(cx, s.win, clear.clone()));

    support::click_id(cx, s.win, clear.clone());
    support::settle(cx, s.win);
    assert_eq!(
        support::label_of(cx, s.win, ids::CONFIRM_TITLE).as_deref(),
        Some("Clear demo cache")
    );
    let body = format!(
        "Delete {} of downloaded demos? Demos still downloading are kept, and any demo can be downloaded again.",
        size_text(5100)
    );
    assert_eq!(
        support::label_of(cx, s.win, ids::CONFIRM_DESCRIPTION).as_deref(),
        Some(body.as_str())
    );
    support::click_id(cx, s.win, ids::CONFIRM_OK);
    support::wait_real(cx, "the demo cache to clear and measure", |cx| {
        demo_bytes(cx) == 100
    });
    assert!(!demos.join("1.dem").exists());
    assert!(!demos.join("22.dem").exists());
    assert!(demos.join(".5.dem.abc123.part").exists());
    let toasts = support::toast_labels(cx, s.win);
    assert!(
        toasts.iter().any(|t| t.contains("Demo cache cleared")
            && t.contains(&format!("{} freed", size_text(5000)))),
        "{toasts:?}"
    );
}

#[gpui_kit::test]
fn cancelling_clear_demo_cache_keeps_every_demo(cx: &mut TestAppContext) {
    use tf2tools_core::StorageKind;
    let env = support::env();
    let demos = seed_demos(&env);
    let s = support::shell_window(cx, &env, Tf2State::Missing { saved: None });
    let _page = open_settings(cx, &s);
    jobs_idle(cx);
    support::settle(cx, s.win);
    let clear = ids::settings_storage_clear(StorageKind::DemoCache);
    scroll_settings_to(cx, &s, clear.clone());
    support::click_id(cx, s.win, clear);
    support::settle(cx, s.win);
    support::click_id(cx, s.win, ids::CONFIRM_CANCEL);
    support::settle(cx, s.win);
    assert!(demos.join("1.dem").exists());
    assert!(demos.join("22.dem").exists());
    assert_eq!(demo_bytes(cx), 5100);
}

#[gpui_kit::test]
fn clear_demo_cache_is_off_while_the_cache_is_empty(cx: &mut TestAppContext) {
    use tf2tools_core::StorageKind;
    let env = support::env();
    let s = support::shell_window(cx, &env, Tf2State::Missing { saved: None });
    let _page = open_settings(cx, &s);
    jobs_idle(cx);
    support::settle(cx, s.win);
    assert_eq!(demo_bytes(cx), 0);
    let clear = ids::settings_storage_clear(StorageKind::DemoCache);
    scroll_settings_to(cx, &s, clear.clone());
    assert!(support::disabled_of(cx, s.win, clear));
}
