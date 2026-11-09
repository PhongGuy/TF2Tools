//! The settings list's words: row options, shown values, ids and the notes in place of the list.

use crate::support;

use gpui_kit::ElementId;
use tf2tools_app::ids;
use tf2tools_app::pages::graphics::model::{
    GAME_DEFAULT, SettingsNote, catalog_note, dropped_note, settings_note_text, shown_choice,
    value_options,
};
use tf2tools_core::{Choices, ModuleCatalogError, ModuleChoice, Preset};

fn value(v: &str) -> ModuleChoice {
    ModuleChoice::Value(v.to_owned())
}

fn name(id: ElementId) -> String {
    match id {
        ElementId::Name(n) => n.to_string(),
        other => panic!("expected a named id, got {other:?}"),
    }
}

#[test]
fn a_row_offers_game_default_then_each_value_in_order() {
    let catalog = support::module_catalog();
    let options = value_options(catalog.module("phong").unwrap());
    let shown: Vec<(&str, ModuleChoice)> = options
        .iter()
        .map(|o| (o.label(), o.choice().clone()))
        .collect();
    assert_eq!(
        shown,
        [
            (GAME_DEFAULT, ModuleChoice::NotSet),
            ("Off", value("off")),
            ("On", value("on")),
            ("On + Rimlighting", value("rim")),
        ]
    );
    assert_eq!(GAME_DEFAULT, "Game default");
}

#[test]
fn a_row_shows_the_saved_choice_else_the_preset_s_value() {
    let catalog = support::module_catalog();
    let mut choices = Choices::new(Preset::Medium);
    choices.set(&catalog, "fpscap", value("120"));
    choices.set(&catalog, "lod", ModuleChoice::NotSet);
    assert_eq!(shown_choice(&catalog, &choices, "fpscap"), value("120"));
    assert_eq!(
        shown_choice(&catalog, &choices, "lod"),
        ModuleChoice::NotSet
    );
    assert_eq!(shown_choice(&catalog, &choices, "decals"), value("low"));
    let custom = Choices::new(Preset::Custom);
    assert_eq!(
        shown_choice(&catalog, &custom, "sound"),
        ModuleChoice::NotSet
    );
}

#[test]
fn a_catalog_failure_reads_as_offline_or_unsupported() {
    let url = || "https://raw.githubusercontent.com/x".to_owned();
    let cases = [
        (
            ModuleCatalogError::Network {
                url: url(),
                message: "refused".into(),
            },
            SettingsNote::Offline,
        ),
        (
            ModuleCatalogError::RateLimited { url: url() },
            SettingsNote::Offline,
        ),
        (
            ModuleCatalogError::Http {
                url: url(),
                status: 503,
            },
            SettingsNote::Offline,
        ),
        (
            ModuleCatalogError::Http {
                url: url(),
                status: 404,
            },
            SettingsNote::Unsupported,
        ),
        (
            ModuleCatalogError::Format { reason: "x".into() },
            SettingsNote::Unsupported,
        ),
        (
            ModuleCatalogError::UnsupportedTag { tag: "..".into() },
            SettingsNote::Unsupported,
        ),
        (
            ModuleCatalogError::TooLarge {
                url: url(),
                limit: 1,
            },
            SettingsNote::Unsupported,
        ),
    ];
    for (err, note) in cases {
        assert_eq!(catalog_note(&err), note, "{err:?}");
    }
    assert_eq!(
        settings_note_text(SettingsNote::Unsupported),
        "This mastercomfig version isn't supported yet. The presets above still work."
    );
    assert_eq!(
        settings_note_text(SettingsNote::Offline),
        "The settings list needs an internet connection the first time. Connect, then open this page again."
    );
}

#[test]
fn dropped_settings_are_named_in_plain_words() {
    assert_eq!(dropped_note(&[]), None);
    assert_eq!(
        dropped_note(&["motion_blur".to_owned(), "gone".to_owned()]).as_deref(),
        Some(
            "These settings aren't in this mastercomfig version and went back to the preset: Motion blur, Gone."
        )
    );
}

#[test]
fn row_ids_name_the_module_and_value() {
    assert_eq!(name(ids::graphics_module("lod")), "graphics-module:lod");
    assert_eq!(
        name(ids::graphics_module_reset("lod")),
        "graphics-module-reset:lod"
    );
    assert_eq!(
        name(ids::graphics_value("lod", &value("high"))),
        "graphics-value:lod=high"
    );
    assert_eq!(
        name(ids::graphics_value("lod", &ModuleChoice::NotSet)),
        "graphics-value:lod="
    );
}
