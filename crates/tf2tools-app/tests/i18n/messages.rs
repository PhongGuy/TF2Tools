use tf2tools_app::i18n::{self, list};

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
