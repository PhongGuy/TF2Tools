//! The profile editor's Graphics rows and section headings reword when the app's language
//! changes.

use gpui_kit::TestAppContext;
use tf2tools_app::ids::{self, IdScope};
use tf2tools_core::{ProfileDraft, SlotMode};

use crate::LocaleGuard;
use crate::support;
use support::graphics::found;
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
    cx.update(|cx| cx.refresh_windows());
    support::settle(cx, s.win);
    let english = support::label_of(cx, s.win, label).unwrap();
    let heading_en = support::label_of(cx, s.win, heading);
    assert_eq!(
        (heading_de.as_deref(), heading_en.as_deref()),
        (Some("Grafik"), Some("Graphics")),
        "the section heading follows the language"
    );
    assert_ne!(german, english, "the row kept its German label");
    assert_eq!(english, "Frame rate cap");
}
