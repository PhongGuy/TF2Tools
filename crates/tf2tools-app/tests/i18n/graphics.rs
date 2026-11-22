//! The Graphics page's settings note comes from the catalogue.

use gpui_kit::TestAppContext;
use tf2tools_app::ids;
use tf2tools_app::pages::graphics::model::SettingsNote;
use tf2tools_core::Preset;

use crate::LocaleGuard;
use crate::library::shown_text;
use crate::support::{self, graphics};

#[gpui_kit::test]
fn the_graphics_settings_note_reads_in_russian(cx: &mut TestAppContext) {
    let _guard = LocaleGuard::switch("ru");
    let env = support::env();
    let s = support::shell_window(cx, &env, graphics::found(&env));
    support::seed_managed_mastercomfig(&env, "9.100.1", "medium");
    let page = graphics::open(cx, &s);
    support::wait_real(cx, "the offline note", |cx| {
        cx.update(|cx| page.read(cx).settings_note()) == Some(SettingsNote::Offline)
    });
    support::settle(cx, s.win);
    assert_eq!(
        support::label_of(cx, s.win, ids::GRAPHICS_SETTINGS_NOTE).as_deref(),
        Some(
            "В первый раз списку настроек нужно подключение к интернету. Подключитесь и откройте эту страницу снова."
        )
    );
}

#[gpui_kit::test]
fn a_russian_search_finds_the_translated_name_in_any_case(cx: &mut TestAppContext) {
    let _guard = LocaleGuard::switch("ru");
    let env = support::env();
    let s = support::shell_window(cx, &env, graphics::found(&env));
    support::seed_managed_mastercomfig(&env, "9.100.1", "medium");
    support::seed_full_mastercomfig_catalog(&env, "9.100.1");
    let page = graphics::open(cx, &s);
    graphics::wait_for_list(cx, &s, &page);
    let name = support::label_of(cx, s.win, ids::graphics_module("shadows")).unwrap();
    assert_ne!(name, "Shadows", "the row reads in Russian");
    graphics::search(cx, &s, &page, &name.to_uppercase());
    assert!(support::present(cx, s.win, ids::graphics_row("shadows")));
    assert!(!support::present(cx, s.win, ids::graphics_row("lod")));
}

#[gpui_kit::test]
fn every_russian_control_fits_its_row_at_800_and_1280_px(cx: &mut TestAppContext) {
    let _guard = LocaleGuard::switch("ru");
    let env = support::env();
    let s = support::shell_window(cx, &env, graphics::found(&env));
    support::seed_managed_mastercomfig(&env, "9.100.1", "medium");
    support::seed_full_mastercomfig_catalog(&env, "9.100.1");
    let page = graphics::open(cx, &s);
    graphics::wait_for_list(cx, &s, &page);
    graphics::assert_controls_fit(cx, &s, 800.);
    graphics::assert_controls_fit(cx, &s, 1280.);
}

fn cyrillic(text: &str) -> bool {
    text.chars().any(|c| ('\u{0400}'..='\u{04FF}').contains(&c))
}

#[gpui_kit::test]
fn the_status_words_and_a_preset_toast_read_in_russian(cx: &mut TestAppContext) {
    let _guard = LocaleGuard::switch("ru");
    let env = support::env();
    let s = support::shell_window(cx, &env, graphics::found(&env));
    support::seed_managed_mastercomfig(&env, "9.100.1", "medium");
    let page = graphics::open(cx, &s);
    for id in [ids::GRAPHICS_HEADLINE, ids::GRAPHICS_DETAIL] {
        let text = support::label_of(cx, s.win, id).unwrap_or_default();
        assert!(cyrillic(&text), "{id}: {text:?}");
    }
    support::click_id(cx, s.win, ids::graphics_preset(Preset::High));
    support::wait_real(cx, "the pick", |cx| {
        cx.update(|cx| page.read(cx).preset()) == Some(Preset::High)
    });
    support::wait_real(cx, "a Russian toast", |cx| {
        support::toast_labels(cx, s.win).iter().any(|t| cyrillic(t))
    });
    assert!(
        support::toast_labels(cx, s.win)
            .iter()
            .all(|t| !t.contains("Preset applied"))
    );
}

// The page is kept while other pages show, so its search box retitles after a language switch.
#[gpui_kit::test]
fn the_settings_search_follows_a_language_switch(cx: &mut TestAppContext) {
    let _guard = LocaleGuard::hold();
    let env = support::env();
    let s = support::shell_window(cx, &env, graphics::found(&env));
    support::seed_managed_mastercomfig(&env, "9.100.1", "medium");
    support::seed_full_mastercomfig_catalog(&env, "9.100.1");
    let page = graphics::open(cx, &s);
    graphics::wait_for_list(cx, &s, &page);
    assert_eq!(
        shown_text(cx, s.win, ids::GRAPHICS_SEARCH).as_deref(),
        Some("Search settings")
    );
    tf2tools_app::i18n::apply("ru");
    cx.update(|cx| cx.refresh_windows());
    assert_eq!(
        shown_text(cx, s.win, ids::GRAPHICS_SEARCH).as_deref(),
        Some("Поиск настроек")
    );
}

// Russian and French have the longest "as is" text; it still wraps inside the reserved 90 px.
#[gpui_kit::test]
fn the_russian_as_is_text_fits_its_box_at_800_px(cx: &mut TestAppContext) {
    as_is_text_fits_at_800_px(cx, "ru");
}

#[gpui_kit::test]
fn the_french_as_is_text_fits_its_box_at_800_px(cx: &mut TestAppContext) {
    as_is_text_fits_at_800_px(cx, "fr");
}

fn as_is_text_fits_at_800_px(cx: &mut TestAppContext, code: &str) {
    let _guard = LocaleGuard::switch(code);
    let env = support::env();
    let s = support::shell_window(cx, &env, graphics::found(&env));
    support::seed_managed_mastercomfig(&env, "9.100.1", "high");
    support::seed_mastercomfig_catalog(&env, "9.100.1");
    support::seed_mastercomfig_choices(&env, "{}");
    let page = graphics::open(cx, &s);
    graphics::wait_for_list(cx, &s, &page);
    graphics::assert_as_is_text_fits(cx, &s, 800., 90.);
}

#[gpui_kit::test]
fn changed_russian_five_value_rows_fit_at_800_px(cx: &mut TestAppContext) {
    let _guard = LocaleGuard::switch("ru");
    let env = support::env();
    let s = support::shell_window(cx, &env, graphics::found(&env));
    support::seed_managed_mastercomfig(&env, "9.100.1", "medium");
    support::seed_full_mastercomfig_catalog(&env, "9.100.1");
    support::seed_mastercomfig_choices(
        &env,
        r#"{"shadows":"high","texture_quality":"very_high","color_filter":"grayscale","water":"ultra"}"#,
    );
    let page = graphics::open(cx, &s);
    graphics::wait_for_list(cx, &s, &page);
    graphics::assert_controls_fit(cx, &s, 800.);
    graphics::assert_changed_rows_fit(
        cx,
        &s,
        &["shadows", "texture_quality", "color_filter", "water"],
        800.,
    );
}
