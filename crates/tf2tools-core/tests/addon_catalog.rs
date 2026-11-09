//! Browsing GameBanana add-ons live: sections, steps, search, categories and profiles.

use tf2tools_core::AddonSection;

#[test]
fn the_sections_table_matches_the_spec() {
    let rows: Vec<_> = AddonSection::ALL
        .iter()
        .map(|s| (s.key(), s.title(), s.root_category(), s.exclude().to_vec()))
        .collect();
    assert_eq!(
        rows,
        [
            ("skins", "Skins", 7951, vec![]),
            ("effects", "Effects", 1090, vec![1105]),
            ("guis", "GUIs", 1644, vec![1649]),
            ("textures", "Textures", 587, vec![]),
            ("gamefiles", "Game files", 2774, vec![]),
        ]
    );
    assert_eq!(
        AddonSection::by_key("guis").map(AddonSection::root_category),
        Some(1644)
    );
    assert_eq!(AddonSection::by_key("maps"), None);
    assert_eq!(
        AddonSection::ALL[0].weapon_parents(),
        [7962, 7960, 7956, 7968, 7954, 7958, 7964, 7966, 7952]
    );
    assert!(
        AddonSection::ALL[1..]
            .iter()
            .all(|s| s.weapon_parents().is_empty())
    );
}
