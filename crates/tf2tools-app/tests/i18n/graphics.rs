//! The Graphics page's settings note comes from the catalogue.

use gpui_kit::TestAppContext;
use tf2tools_app::ids;
use tf2tools_app::pages::graphics::model::SettingsNote;

use crate::LocaleGuard;
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
