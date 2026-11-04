use std::collections::BTreeSet;

use tf2tools_app::pages::hud_store::curated::{
    CuratedFilter, CuratedHud, Facet, Os, count_label, facets,
};

const RAYSHUD: &str = r#"{"name":"rayshud","author":"raysfire","contributors":["CriticalFlaw"],
  "social":{"steam_group":"rayshud","discord":"/hTdtK9vBhE"},"releaseDate":"2015-04-01",
  "flags":["menus","scoreboards","crosshairs","minmode"],
  "traits":{"os":["windows","mac","linux"],"gamemode":["tournament","comp"],"res":["4/3","16/9"]},
  "tags":["minimal"],"verified":true,"resources":["banner","menu"]}"#;

fn hud(id: &str, author: &str, json: &str) -> CuratedHud {
    CuratedHud::parse(id, id, author, Some("https://github.com/x/y"), vec![], json)
}

fn sample() -> Vec<CuratedHud> {
    vec![
        hud(
            "budhud",
            "Whisker",
            r#"{"flags":["menus"],"tags":["colorful"],"traits":{"os":["windows"],"gamemode":["casual"],"res":["16/9"]}}"#,
        ),
        hud(
            "prehud",
            "Pre",
            r#"{"prerelease":true,"flags":["menus","minmode"],"traits":{"os":["linux","windows"]}}"#,
        ),
        hud("rayshud", "raysfire", RAYSHUD),
    ]
}

fn shown(filter: &CuratedFilter, huds: &[CuratedHud]) -> Vec<String> {
    filter
        .apply(huds)
        .into_iter()
        .map(|i| huds[i].hud_id.clone())
        .collect()
}

#[test]
fn parses_hud_db_data() {
    let h = hud("rayshud", "raysfire", RAYSHUD);
    assert!(h.verified && !h.prerelease);
    assert_eq!(h.flags, ["menus", "scoreboards", "crosshairs", "minmode"]);
    assert_eq!(h.os, ["windows", "mac", "linux"]);
    assert_eq!(h.gamemodes, ["tournament", "comp"]);
    assert_eq!(h.resolutions, ["4/3", "16/9"]);
    assert_eq!(h.tags, ["minimal"]);
    assert_eq!(h.contributors, ["CriticalFlaw"]);
    assert_eq!(h.release_date.as_deref(), Some("2015-04-01"));
    assert_eq!(
        h.socials,
        [
            ("discord".to_string(), "/hTdtK9vBhE".to_string()),
            ("steam_group".to_string(), "rayshud".to_string())
        ]
    );
    assert_eq!(h.values(Facet::Resolution), ["4/3", "16/9"]);
    let broken = hud("x", "y", "not json");
    assert_eq!(
        (
            broken.hud_id.as_str(),
            broken.flags.len(),
            broken.prerelease
        ),
        ("x", 0, false)
    );
}

#[test]
fn defaults_hide_prereleases_and_preset_the_os() {
    let huds = sample();
    let f = CuratedFilter::new(Os::Linux);
    assert_eq!(f.os, BTreeSet::from([Os::Linux]));
    assert!(f.hide_prerelease);
    assert_eq!(shown(&f, &huds), ["rayshud"]);
    let f = CuratedFilter::new(Os::Windows);
    assert_eq!(shown(&f, &huds), ["budhud", "rayshud"]);
    assert_eq!(Os::ALL.map(Os::key), ["windows", "mac", "linux"]);
    assert!(Os::ALL.contains(&Os::current()));
}

#[test]
fn search_matches_name_or_author_ignoring_case() {
    let huds = sample();
    let mut f = CuratedFilter::new(Os::Windows);
    f.search = "BUD".into();
    assert_eq!(shown(&f, &huds), ["budhud"]);
    f.search = "Raysf".into();
    assert_eq!(shown(&f, &huds), ["rayshud"]);
}

#[test]
fn flags_need_all_and_other_facets_need_any() {
    let huds = sample();
    let mut f = CuratedFilter::new(Os::Windows);
    f.hide_prerelease = false;
    f.toggle(Facet::Flag, "menus");
    assert_eq!(shown(&f, &huds), ["budhud", "prehud", "rayshud"]);
    f.toggle(Facet::Flag, "minmode");
    assert_eq!(shown(&f, &huds), ["prehud", "rayshud"]);
    f.toggle(Facet::Flag, "menus");
    f.toggle(Facet::Flag, "minmode");
    assert!(
        f.selected(Facet::Flag).is_empty(),
        "a second toggle removes"
    );
    f.toggle(Facet::Gamemode, "casual");
    f.toggle(Facet::Gamemode, "comp");
    assert_eq!(shown(&f, &huds), ["budhud", "rayshud"]);
    f.toggle(Facet::Tag, "minimal");
    assert_eq!(shown(&f, &huds), ["rayshud"]);
    f.toggle(Facet::Resolution, "4/3");
    assert_eq!(shown(&f, &huds), ["rayshud"]);
}

#[test]
fn os_is_any_of_and_clear_restores_v5_defaults() {
    let huds = sample();
    let mut f = CuratedFilter::new(Os::Mac);
    assert_eq!(shown(&f, &huds), ["rayshud"]);
    f.toggle_os(Os::Windows);
    assert_eq!(shown(&f, &huds), ["budhud", "rayshud"]);
    f.search = "zzz".into();
    f.hide_prerelease = false;
    f.toggle(Facet::Tag, "minimal");
    f.clear();
    assert_eq!(f.search, "");
    assert!(f.os.is_empty(), "clear empties the OS filter");
    assert!(f.hide_prerelease);
    assert!(f.selected(Facet::Tag).is_empty());
    assert_eq!(shown(&f, &huds), ["budhud", "rayshud"]);
}

#[test]
fn facets_are_sorted_and_unique_and_the_count_reads_as_v5() {
    let huds = sample();
    assert_eq!(
        facets(&huds, Facet::Flag),
        ["crosshairs", "menus", "minmode", "scoreboards"]
    );
    assert_eq!(facets(&huds, Facet::Resolution), ["16/9", "4/3"]);
    assert_eq!(count_label(2, 304), "2 of 304 HUDs");
}

#[test]
fn a_filter_is_active_like_v5_s_has_active_filters() {
    let mut first_load = CuratedFilter::new(Os::Linux);
    assert!(first_load.is_active(), "the OS preset is a filter");
    first_load.clear();
    assert!(!first_load.is_active(), "cleared");
    let edits: [fn(&mut CuratedFilter); 7] = [
        |f| f.search = "bud".into(),
        |f| f.toggle_os(Os::Mac),
        |f| f.hide_prerelease = false,
        |f| f.toggle(Facet::Flag, "menus"),
        |f| f.toggle(Facet::Tag, "minimal"),
        |f| f.toggle(Facet::Gamemode, "casual"),
        |f| f.toggle(Facet::Resolution, "16/9"),
    ];
    for (ix, edit) in edits.into_iter().enumerate() {
        let mut f = CuratedFilter::new(Os::Linux);
        f.clear();
        edit(&mut f);
        assert!(f.is_active(), "edit {ix}");
    }
}
