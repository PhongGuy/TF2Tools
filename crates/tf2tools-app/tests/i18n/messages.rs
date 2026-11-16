use gpui_kit::SharedString;
use tf2tools_app::i18n::{self, list};
use tf2tools_app::local_messages::{self as local, LocalFailure};
use tf2tools_app::pages::graphics::jobs;
use tf2tools_app::services::OpError;

use crate::LocaleGuard;

/// Russian text: some Cyrillic, every variable filled, and no catalogue key showing.
pub fn assert_russian(text: &str) {
    assert!(
        text.chars().any(|c| matches!(c, 'А'..='я' | 'Ё' | 'ё')),
        "not Russian: {text:?}"
    );
    assert!(!text.contains("%{"), "an unfilled variable: {text:?}");
    for prefix in ["messages.", "updater.", "common."] {
        assert!(!text.contains(prefix), "a raw key: {text:?}");
    }
}

fn items(n: usize) -> Vec<String> {
    (1..=n).map(|i| format!("i{i}")).collect()
}

#[test]
fn lists_join_in_both_languages() {
    let _guard = LocaleGuard::hold();
    assert_eq!(list(&items(1)), "i1");
    assert_eq!(list(&items(2)), "i1 and i2");
    assert_eq!(list(&items(5)), "i1, i2, i3, i4 and i5");
    let long = list(&items(21));
    assert!(
        long.starts_with("i1, i2, ") && long.ends_with(", i20 and i21"),
        "{long}"
    );
    i18n::apply("ru");
    assert_eq!(list(&items(1)), "i1");
    assert_eq!(list(&items(2)), "i1 и i2");
    assert_russian(&list(&items(2)));
    assert_eq!(list(&items(5)), "i1, i2, i3, i4 и i5");
    let long = list(&items(21));
    assert!(
        long.starts_with("i1, i2, ") && long.ends_with(", i20 и i21"),
        "{long}"
    );
}

#[test]
fn shared_texts_read_in_russian() {
    let _guard = LocaleGuard::switch("ru");
    let shared = [
        local::internal(),
        local::hud_files(),
        local::hud_base_changed(),
        local::hud_options_unreadable(),
        local::map_restart(),
        local::map_skipped(),
        jobs::close_tf2(),
        jobs::close_tf2_text(),
    ];
    for text in &shared {
        assert_russian(text);
    }
}

#[test]
fn op_errors_read_in_russian() {
    let _guard = LocaleGuard::switch("ru");
    for err in [OpError::NoTf2, OpError::Internal("boom".into())] {
        assert_russian(&err.title());
        assert_russian(&err.description());
        assert_russian(&err.to_string());
    }
    let op = LocalFailure::new(local::hud_files(), "x").into_op(SharedString::from("Худ"));
    assert_russian(&op.description());
    assert_russian(&op.to_string());
}

#[test]
fn a_failure_keeps_the_language_it_was_built_in() {
    let _guard = LocaleGuard::switch("ru");
    let failure = LocalFailure::new(local::hud_files(), "x");
    let op = LocalFailure::new(local::hud_files(), "x").into_op(SharedString::from("Сохранение"));
    i18n::apply("en");
    assert_russian(&failure.message);
    assert_russian(&op.description());
    // NoTf2 and Internal carry no text, so they read in the language of the moment.
    assert_eq!(OpError::NoTf2.title(), "TF2 not found");
}
