//! The Maps page's pure parts: search, the three sorts and the row texts.

use tf2tools_app::pages::maps::model::{
    MapRow, MapSort, SORTS, count_label, last_played_label, visible,
};
use tf2tools_core::MapState;

fn row(name: &str, times: u64, last: Option<&str>) -> MapRow {
    MapRow {
        name: name.to_owned(),
        times_played: times,
        last_played: last.map(str::to_owned),
        size: 1,
        state: MapState::Available,
    }
}

fn rows() -> Vec<MapRow> {
    vec![
        row("koth_product_final", 900, Some("2026-10-01")),
        row("cp_process_f12", 5000, Some("2026-11-01")),
        row("Ultiduo_Baloo", 0, None),
        row("cp_gullywash_f9", 5000, Some("2025-01-01")),
    ]
}

fn names(rows: &[MapRow], query: &str, sort: MapSort) -> Vec<String> {
    visible(rows, query, sort)
        .into_iter()
        .map(|i| rows[i].name.clone())
        .collect()
}

#[test]
fn name_sort_ignores_case() {
    assert_eq!(
        names(&rows(), "", MapSort::Name),
        [
            "cp_gullywash_f9",
            "cp_process_f12",
            "koth_product_final",
            "Ultiduo_Baloo"
        ]
    );
}

#[test]
fn most_played_breaks_ties_by_name() {
    assert_eq!(
        names(&rows(), "", MapSort::MostPlayed),
        [
            "cp_gullywash_f9",
            "cp_process_f12",
            "koth_product_final",
            "Ultiduo_Baloo"
        ]
    );
}

#[test]
fn recently_played_puts_never_played_maps_last() {
    assert_eq!(
        names(&rows(), "", MapSort::LastPlayed),
        [
            "cp_process_f12",
            "koth_product_final",
            "cp_gullywash_f9",
            "Ultiduo_Baloo"
        ]
    );
}

#[test]
fn search_matches_any_part_of_the_name_without_case() {
    assert_eq!(names(&rows(), "  ULTI ", MapSort::Name), ["Ultiduo_Baloo"]);
    assert_eq!(
        names(&rows(), "cp_", MapSort::Name),
        ["cp_gullywash_f9", "cp_process_f12"]
    );
    assert!(names(&rows(), "nothing", MapSort::Name).is_empty());
}

#[test]
fn the_sort_menu_lists_every_sort_once() {
    let labels: Vec<_> = SORTS.iter().map(|(label, _)| *label).collect();
    assert_eq!(labels, ["Name", "Most played", "Recently played"]);
}

#[test]
fn row_texts_read_as_plain_words() {
    assert_eq!(last_played_label(None), "Never");
    assert_eq!(last_played_label(Some("2026-11-01")), "2026-11-01");
    assert_eq!(count_label(4368, 4368), "4368 maps");
    assert_eq!(count_label(1, 1), "1 map");
    assert_eq!(count_label(2, 4368), "2 of 4368 maps");
}
