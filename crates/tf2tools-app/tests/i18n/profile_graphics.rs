//! The profile editor's Graphics rows and section headings reword when the app's language
//! changes, and its German changes strip holds still on the first change.

use gpui_kit::component::select::SelectEvent;
use gpui_kit::{TestAppContext, px, size};
use tf2tools_app::ids::{self, IdScope};
use tf2tools_app::pages::graphics::model::ValueOption;
use tf2tools_core::{Choices, ModuleChoice, Preset, ProfileDraft, SlotMode};

use crate::LocaleGuard;
use crate::support;
use support::graphics::{found, value};
use support::profiles::{edit, low_fps60, page};

#[gpui_kit::test]
fn a_language_switch_rewords_the_editor_rows(cx: &mut TestAppContext) {
    let _guard = LocaleGuard::switch("de");
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    support::seed_managed_mastercomfig(&env, "9.100.1", "medium");
    support::seed_mastercomfig_catalog(&env, "9.100.1");
    let draft = ProfileDraft::new("Potato").with_graphics(SlotMode::Set(low_fps60()));
    let p = s.services.library.create_profile(&draft).unwrap();
    let page = page(cx, &s);
    edit(cx, &s, &page, p.id());
    let gfx = IdScope::PROFILE_GRAPHICS;
    let label = gfx.id(ids::graphics_module("fpscap"));
    // The headings come from the editor's cached sections, so they show the cache follows.
    let heading = gfx.id(ids::graphics_category("graphics"));
    support::wait_real(cx, "the rows", |cx| {
        support::present(cx, s.win, label.clone())
    });
    support::settle(cx, s.win);
    let german = support::label_of(cx, s.win, label.clone()).unwrap();
    let heading_de = support::label_of(cx, s.win, heading.clone());
    tf2tools_app::i18n::apply("en");
    // The heading must reword on the first frame after the switch. No refresh beforehand:
    // its flush draws a frame of its own, which would hide a one-frame lag.
    let heading_en = support::label_after_one_frame(cx, s.win, heading);
    let english = support::label_of(cx, s.win, label).unwrap();
    assert_eq!(
        (heading_de.as_deref(), heading_en.as_deref()),
        (Some("Grafik"), Some("Graphics")),
        "the section heading follows the language"
    );
    assert_ne!(german, english, "the row kept its German label");
    assert_eq!(english, "Frame rate cap");
}

// At 1300 px the editor's strip is narrower than the Graphics page's, so it stacks by its own
// width; the long German chip would otherwise wrap on the first change and move the rows.
#[gpui_kit::test]
fn the_first_german_change_keeps_the_editor_rows_still_at_1300_px(cx: &mut TestAppContext) {
    let _guard = LocaleGuard::switch("de");
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    support::seed_managed_mastercomfig(&env, "9.100.1", "medium");
    support::seed_mastercomfig_catalog(&env, "9.100.1");
    let draft =
        ProfileDraft::new("Potato").with_graphics(SlotMode::Set(Choices::new(Preset::Medium)));
    let p = s.services.library.create_profile(&draft).unwrap();
    let page = page(cx, &s);
    let e = edit(cx, &s, &page, p.id());
    let gfx = IdScope::PROFILE_GRAPHICS;
    cx.simulate_window_resize(s.win, size(px(1300.), px(900.)));
    support::wait_real(cx, "the rows", |cx| {
        support::present(cx, s.win, gfx.id(ids::graphics_row("fpscap")))
    });
    support::click_id(cx, s.win, ids::profile_rail("graphics"));
    support::settle(cx, s.win);
    let empty = support::bounds_of(cx, s.win, gfx.id(ids::GRAPHICS_NO_CHANGES)).unwrap();
    let row = |cx: &mut TestAppContext| {
        support::bounds_of(cx, s.win, gfx.id(ids::graphics_row("fpscap"))).unwrap()
    };
    let row_before = row(cx);
    let select = cx
        .update(|cx| e.read(cx).graphics().read(cx).module_select("fpscap"))
        .expect("the fpscap Select");
    cx.update(|cx| {
        select.update(cx, |_, cx| {
            cx.emit(SelectEvent::<Vec<ValueOption>>::Confirm(Some(value("120"))))
        })
    });
    support::wait_real(cx, "the first change", |cx| {
        support::present(cx, s.win, gfx.id(ids::GRAPHICS_CHANGES))
    });
    support::settle(cx, s.win);
    let strip = support::bounds_of(cx, s.win, gfx.id(ids::GRAPHICS_CHANGES)).unwrap();
    assert_eq!(
        empty.size.height, strip.size.height,
        "the strip keeps its height: {empty:?} {strip:?}"
    );
    assert_eq!(row_before.top(), row(cx).top(), "the first row moved");
}

// The editor's rows are narrower than the page's at the same window width, so a changed row's
// controls must leave its text room there too.
#[gpui_kit::test]
fn changed_english_editor_rows_keep_their_text_readable(cx: &mut TestAppContext) {
    editor_rows_stay_readable(cx, "en");
}

#[gpui_kit::test]
fn changed_german_editor_rows_keep_their_text_readable(cx: &mut TestAppContext) {
    editor_rows_stay_readable(cx, "de");
}

#[gpui_kit::test]
fn changed_russian_editor_rows_keep_their_text_readable(cx: &mut TestAppContext) {
    editor_rows_stay_readable(cx, "ru");
}

fn editor_rows_stay_readable(cx: &mut TestAppContext, code: &str) {
    let _guard = LocaleGuard::switch(code);
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    support::seed_managed_mastercomfig(&env, "9.100.1", "medium");
    support::seed_full_mastercomfig_catalog(&env, "9.100.1");
    let catalog = support::full_module_catalog();
    let mut choices = Choices::new(Preset::Medium);
    choices.set(&catalog, "shadows", value("high"));
    choices.set(&catalog, "texture_quality", value("very_high"));
    choices.set(&catalog, "color_filter", ModuleChoice::NotSet);
    let draft = ProfileDraft::new("Potato").with_graphics(SlotMode::Set(choices));
    let p = s.services.library.create_profile(&draft).unwrap();
    let page = page(cx, &s);
    edit(cx, &s, &page, p.id());
    let gfx = IdScope::PROFILE_GRAPHICS;
    support::wait_real(cx, "the rows", |cx| {
        support::present(cx, s.win, gfx.id(ids::graphics_segments("shadows")))
    });
    support::click_id(cx, s.win, ids::profile_rail("graphics"));
    for width in [1280., 1100.] {
        support::graphics::assert_rows_readable(cx, &s, gfx, &support::graphics::WIDE, width);
    }
}
