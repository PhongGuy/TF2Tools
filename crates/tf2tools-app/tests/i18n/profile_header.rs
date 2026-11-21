//! The profile editor's header crumb in the languages with the widest Cancel and Save, in the
//! slot Windows leaves at 800x600.

use gpui_kit::TestAppContext;
use tf2tools_app::ids;
use tf2tools_core::{DraftItem, ProfileDraft, SlotMode};

use crate::LocaleGuard;
use crate::support;
use support::graphics::found;
use support::profiles::{assert_crumb_fits, at_windows_800x600, edit, page, seed};

// `cancel` is the language's Cancel, which the button keeps as its name even when icon-only.
fn fits_in(lang: &str, cancel: &str, cx: &mut TestAppContext) {
    let _guard = LocaleGuard::switch(lang);
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let seeded = seed(&s, &env);
    let draft = ProfileDraft::new("A rather long profile name for the header")
        .with_hud(SlotMode::Set(DraftItem::Pick(seeded.rayshud.id().clone())));
    let comp = s.services.library.create_profile(&draft).unwrap();
    // The editor opens first: French cards stand taller than a 600 px window shows.
    let page = page(cx, &s);
    edit(cx, &s, &page, comp.id());
    at_windows_800x600(cx, &s);
    assert_crumb_fits(cx, &s);
    assert_eq!(
        support::label_of(cx, s.win, ids::PROFILE_EDITOR_CANCEL).as_deref(),
        Some(cancel),
        "{lang}: Cancel keeps its name"
    );
}

#[gpui_kit::test]
fn cancel_and_save_fit_the_windows_header_in_german(cx: &mut TestAppContext) {
    fits_in("de", "Abbrechen", cx);
}

#[gpui_kit::test]
fn cancel_and_save_fit_the_windows_header_in_french(cx: &mut TestAppContext) {
    fits_in("fr", "Annuler", cx);
}

#[gpui_kit::test]
fn cancel_and_save_fit_the_windows_header_in_russian(cx: &mut TestAppContext) {
    fits_in("ru", "Отмена", cx);
}
