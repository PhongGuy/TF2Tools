//! The Graphics page's view-model over the full 9.100.1 catalogue.

use crate::support;

use tf2tools_app::pages::graphics::view::{
    ControlKind, EmptyState, Filter, changes, control_kind, empty_state, row, sections,
};
use tf2tools_core::{Choices, ModuleChoice, Preset};

fn value(v: &str) -> ModuleChoice {
    ModuleChoice::Value(v.to_owned())
}

fn filter(query: &str, changed_only: bool) -> Filter {
    Filter {
        query: query.to_owned(),
        changed_only,
    }
}

#[test]
fn each_module_gets_the_control_that_fits_its_values() {
    let catalog = support::full_module_catalog();
    let modules: Vec<_> = catalog
        .categories()
        .iter()
        .flat_map(|c| c.modules())
        .collect();
    assert_eq!(modules.len(), 56);
    let count = |kind: ControlKind| modules.iter().filter(|m| control_kind(m) == kind).count();
    assert_eq!(
        (
            count(ControlKind::Switch),
            count(ControlKind::Segmented),
            count(ControlKind::Select)
        ),
        (13, 38, 5)
    );
    let kind = |name: &str| control_kind(catalog.module(name).unwrap());
    assert_eq!(kind("vsync"), ControlKind::Switch);
    assert_eq!(kind("flashlight"), ControlKind::Switch);
    assert_eq!(
        kind("sillygibs"),
        ControlKind::Segmented,
        "auto, off, on is no plain switch"
    );
    assert_eq!(kind("phong"), ControlKind::Segmented);
    assert_eq!(kind("lod"), ControlKind::Segmented, "four values still fit");
    assert_eq!(kind("shadows"), ControlKind::Segmented, "five values fit");
    assert_eq!(kind("texture_quality"), ControlKind::Segmented);
    assert_eq!(
        kind("texture_filter"),
        ControlKind::Select,
        "six values do not"
    );
    assert_eq!(kind("fpscap"), ControlKind::Select);
}

#[test]
fn a_row_shows_the_saved_choice_against_the_preset_s_value() {
    let catalog = support::full_module_catalog();
    let mut choices = Choices::new(Preset::Medium);
    choices.set(&catalog, "fpscap", value("120"));
    choices.set(&catalog, "lod", ModuleChoice::NotSet);
    let at = |name: &str| row(&catalog, &choices, catalog.module(name).unwrap());
    let fps = at("fpscap");
    assert_eq!(
        (fps.shown, fps.preset_value, fps.changed),
        (value("120"), value("400"), true)
    );
    assert_eq!(fps.preset_text.as_ref(), "Medium: 400");
    let lod = at("lod");
    assert_eq!((lod.shown, lod.changed), (ModuleChoice::NotSet, true));
    assert_eq!(lod.preset_text.as_ref(), "Medium: Medium");
    let decals = at("decals");
    assert_eq!((decals.shown, decals.changed), (value("low"), false));
    let shadows = at("shadows");
    assert_eq!(
        (shadows.shown, shadows.preset_value, shadows.changed),
        (value("medium"), value("medium"), false),
        "an unsaved row shows the preset's value"
    );
}

#[test]
fn under_custom_most_modules_are_left_to_tf2() {
    let catalog = support::full_module_catalog();
    let mut choices = Choices::new(Preset::Custom);
    let lod = row(&catalog, &choices, catalog.module("lod").unwrap());
    assert_eq!(lod.preset_value, ModuleChoice::NotSet);
    assert_eq!(lod.preset_text.as_ref(), "Custom: Let TF2 decide");
    assert!(!lod.changed);
    let fps = row(&catalog, &choices, catalog.module("fpscap").unwrap());
    assert_eq!(
        fps.preset_value,
        value("400"),
        "fpscap's default applies under Custom"
    );
    choices.set(&catalog, "lod", value("high"));
    assert!(row(&catalog, &choices, catalog.module("lod").unwrap()).changed);
    choices.set(&catalog, "lod", ModuleChoice::NotSet);
    assert_eq!(choices.get("lod"), None, "NotSet is Custom's own value");
    assert!(!row(&catalog, &choices, catalog.module("lod").unwrap()).changed);
}

#[test]
fn segments_and_selects_lead_with_let_tf2_decide_and_switches_do_not() {
    let catalog = support::full_module_catalog();
    let choices = Choices::new(Preset::Medium);
    let options = |name: &str| -> Vec<(ModuleChoice, String)> {
        row(&catalog, &choices, catalog.module(name).unwrap())
            .options
            .into_iter()
            .map(|o| (o.choice, o.label.to_string()))
            .collect()
    };
    assert_eq!(
        options("vsync"),
        [
            (value("off"), "Off".to_owned()),
            (value("on"), "On".to_owned())
        ]
    );
    let filter = options("texture_filter");
    assert_eq!(
        filter[0],
        (ModuleChoice::NotSet, "Let TF2 decide".to_owned())
    );
    let rest: Vec<ModuleChoice> = filter[1..].iter().map(|(c, _)| c.clone()).collect();
    assert_eq!(
        rest,
        [
            "blocky",
            "trilinear",
            "aniso2x",
            "aniso4x",
            "aniso8x",
            "aniso16x"
        ]
        .map(value)
    );
    for module in ["decals", "hud_avatars"] {
        let segments = options(module);
        assert_eq!(
            segments[0],
            (ModuleChoice::NotSet, "Let TF2 decide".to_owned()),
            "segments lead with Let TF2 decide, as a Select does"
        );
        assert!(
            segments[1..]
                .iter()
                .all(|(c, _)| *c != ModuleChoice::NotSet)
        );
    }
}

#[test]
fn the_changes_list_names_was_and_now_in_catalogue_order() {
    let catalog = support::full_module_catalog();
    let mut choices = Choices::new(Preset::Medium);
    choices.set(&catalog, "hud_avatars", value("off"));
    choices.set(&catalog, "fpscap", value("120"));
    choices.set(&catalog, "lod", ModuleChoice::NotSet);
    let list = |catalog| -> Vec<(String, Option<String>, String)> {
        changes(catalog, &choices)
            .into_iter()
            .map(|c| (c.module, c.was.map(|w| w.to_string()), c.now.to_string()))
            .collect()
    };
    let want = [
        ("lod", Some("Medium"), "Let TF2 decide"),
        ("fpscap", Some("400"), "120"),
        ("hud_avatars", Some("Everyone"), "Off"),
    ]
    .map(|(m, w, n)| (m.to_owned(), w.map(str::to_owned), n.to_owned()));
    assert_eq!(list(Some(&catalog)), want);
    let offline = [
        ("fpscap", None, "120"),
        ("hud_avatars", None, "Off"),
        ("lod", None, "Let TF2 decide"),
    ]
    .map(|(m, w, n): (&str, Option<&str>, &str)| {
        (m.to_owned(), w.map(str::to_owned), n.to_owned())
    });
    assert_eq!(
        list(None),
        offline,
        "without a catalogue every saved choice shows, by name"
    );
    assert!(changes(Some(&catalog), &Choices::new(Preset::Medium)).is_empty());
}

#[test]
fn sections_keep_every_category_with_its_count_and_change_dot() {
    let catalog = support::full_module_catalog();
    let mut choices = Choices::new(Preset::Medium);
    choices.set(&catalog, "hud_avatars", value("off"));
    let shape: Vec<(String, usize, bool, usize)> = sections(&catalog, &choices, &Filter::default())
        .iter()
        .map(|s| (s.key.clone(), s.count, s.has_changes, s.rows.len()))
        .collect();
    let want = [
        ("graphics", 33, false, 33),
        ("hud", 14, true, 14),
        ("sound", 2, false, 2),
        ("misc", 2, false, 2),
        ("networking", 5, false, 5),
    ]
    .map(|(k, c, d, r)| (k.to_owned(), c, d, r));
    assert_eq!(shape, want);
    let only = sections(&catalog, &choices, &filter("", true));
    let rows: Vec<(&str, Vec<&str>)> = only
        .iter()
        .map(|s| {
            (
                s.key.as_str(),
                s.rows.iter().map(|r| r.module.as_str()).collect(),
            )
        })
        .collect();
    assert_eq!(
        rows,
        [
            ("graphics", vec![]),
            ("hud", vec!["hud_avatars"]),
            ("sound", vec![]),
            ("misc", vec![]),
            ("networking", vec![]),
        ]
    );
    assert_eq!(
        only.iter().map(|s| s.count).sum::<usize>(),
        56,
        "counts ignore the filter"
    );
}

#[test]
fn search_matches_the_label_ignoring_case_and_spaces() {
    let catalog = support::full_module_catalog();
    let choices = Choices::new(Preset::Medium);
    let hits = |q: &str| -> Vec<String> {
        sections(&catalog, &choices, &filter(q, false))
            .into_iter()
            .flat_map(|s| s.rows)
            .map(|r| r.module)
            .collect()
    };
    assert_eq!(hits("  SHADOWS "), ["shadows"]);
    assert_eq!(hits("").len(), 56);
    assert_eq!(hits("   ").len(), 56);
    assert!(hits("zzz").is_empty());
}

#[test]
fn an_empty_result_says_why() {
    let catalog = support::full_module_catalog();
    let choices = Choices::new(Preset::Medium);
    let empty = |q: &str, only: bool| {
        let f = filter(q, only);
        empty_state(&sections(&catalog, &choices, &f), &f)
    };
    assert_eq!(empty("", false), None);
    assert_eq!(empty("", true), Some(EmptyState::NothingChanged));
    assert_eq!(
        empty(" zzz ", false),
        Some(EmptyState::NoMatch("zzz".to_owned()))
    );
    assert_eq!(
        empty("zzz", true),
        Some(EmptyState::NoMatch("zzz".to_owned()))
    );
    assert_eq!(
        empty("shadows", true),
        Some(EmptyState::NoMatch("shadows".to_owned()))
    );
}
