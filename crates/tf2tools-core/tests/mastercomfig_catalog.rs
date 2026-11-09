//! mastercomfig's module catalog: categories, labels, preset values and the player's choices.

use crate::common;

use common::{MC_MODULES, module_catalog};
use tf2tools_core::{
    Choices, ModuleCatalog, ModuleCatalogError, ModuleChoice, Preset, readable_name,
};

fn value(v: &str) -> ModuleChoice {
    ModuleChoice::Value(v.to_owned())
}

#[test]
fn the_catalog_keeps_mastercomfig_s_categories_and_module_order() {
    let catalog = module_catalog();
    assert_eq!(catalog.tag(), "9.100.1");
    let categories: Vec<(&str, &str)> = catalog
        .categories()
        .iter()
        .map(|c| (c.key(), c.label()))
        .collect();
    assert_eq!(
        categories,
        [("graphics", "Graphics"), ("hud", "HUD"), ("sound", "Sound")]
    );
    let graphics: Vec<&str> = catalog.categories()[0]
        .modules()
        .iter()
        .map(|m| m.name())
        .collect();
    assert_eq!(
        graphics,
        ["lod", "phong", "motion_blur", "decals", "fpscap"]
    );
}

#[test]
fn labels_use_display_names_or_a_readable_name() {
    let catalog = module_catalog();
    let lod = catalog.module("lod").unwrap();
    assert_eq!(lod.label(), "Model Quality");
    assert_eq!(lod.description(), Some("Controls how detailed models are."));
    let blur = catalog.module("motion_blur").unwrap();
    assert_eq!((blur.label(), blur.description()), ("Motion blur", None));
    let phong: Vec<(&str, &str)> = catalog
        .module("phong")
        .unwrap()
        .values()
        .iter()
        .map(|v| (v.value(), v.label()))
        .collect();
    assert_eq!(
        phong,
        [("off", "Off"), ("on", "On"), ("rim", "On + Rimlighting")]
    );
    let sound = catalog.module("sound").unwrap();
    assert_eq!(
        (sound.label(), sound.values()[3].label()),
        ("Sound", "Very high")
    );
    let fps = catalog.module("fpscap").unwrap();
    assert_eq!(
        (fps.values()[4].value(), fps.values()[4].label()),
        ("120", "120")
    );
    assert!(fps.has_value("400") && !fps.has_value("401"));
    assert!(catalog.module("nope").is_none());
    assert_eq!(readable_name("hud_player_model"), "Hud player model");
}

#[test]
fn a_preset_value_falls_back_to_default_and_must_be_one_of_the_values() {
    let catalog = module_catalog();
    assert_eq!(catalog.resolve("lod", Preset::Medium), Some("medium"));
    assert_eq!(catalog.resolve("lod", Preset::Ultra), Some("ultra"));
    assert_eq!(
        catalog.resolve("decals", Preset::Low),
        Some("off"),
        "no low entry: default"
    );
    assert_eq!(catalog.resolve("phong", Preset::Ultra), Some("rim"));
    assert_eq!(catalog.resolve("phong", Preset::Low), Some("off"));
    assert_eq!(catalog.resolve("fpscap", Preset::High), Some("400"));
    assert_eq!(
        catalog.resolve("lod", Preset::Custom),
        None,
        "\"\" is not one of lod's values"
    );
    assert_eq!(catalog.resolve("sound", Preset::Custom), None);
    assert_eq!(catalog.resolve("nope", Preset::Medium), None);
}

#[test]
fn only_name_and_values_are_required_and_no_default_means_not_set() {
    let modules =
        br#"{"misc":{"modules":[{"name":"x","values":["on","off"]},{"name":"y","values":["a"]}]}}"#;
    let catalog = ModuleCatalog::parse("1.0.0", modules, br#"{"x":{"high":"on"}}"#).unwrap();
    assert_eq!(catalog.resolve("x", Preset::High), Some("on"));
    assert_eq!(
        catalog.resolve("x", Preset::Medium),
        None,
        "no medium and no default"
    );
    assert_eq!(
        catalog.resolve("y", Preset::Medium),
        None,
        "no preset_modules entry"
    );
    assert_eq!(catalog.module("x").unwrap().label(), "X");
    assert_eq!(catalog.categories()[0].label(), "Misc");
    let odd = ModuleCatalog::parse("1.0.0", modules, br#"{"x":{"high":"zzz"}}"#).unwrap();
    assert_eq!(
        odd.resolve("x", Preset::High),
        None,
        "a non-empty value outside x's values"
    );
}

#[test]
fn a_module_without_a_name_or_values_is_a_format_error() {
    for modules in [
        br#"{"misc":{"modules":[{"values":["on"]}]}}"#.as_slice(),
        br#"{"misc":{"modules":[{"name":"x"}]}}"#.as_slice(),
        br#"{"misc":{"modules":[{"name":"x","values":[{"display":"On"}]}]}}"#.as_slice(),
        br#"{"misc":{}}"#.as_slice(),
        b"not json".as_slice(),
    ] {
        let err = ModuleCatalog::parse("1.0.0", modules, b"{}").unwrap_err();
        assert!(matches!(err, ModuleCatalogError::Format { .. }), "{err:?}");
    }
    let err = ModuleCatalog::parse("1.0.0", MC_MODULES, b"[1, 2]").unwrap_err();
    assert!(matches!(err, ModuleCatalogError::Format { .. }), "{err:?}");
}

#[test]
fn a_choice_equal_to_the_preset_value_is_not_kept() {
    let catalog = module_catalog();
    let mut choices = Choices::new(Preset::Medium);
    choices.set(&catalog, "fpscap", value("120"));
    choices.set(&catalog, "lod", ModuleChoice::NotSet);
    assert_eq!(choices.get("fpscap"), Some(&value("120")));
    assert_eq!(choices.get("lod"), Some(&ModuleChoice::NotSet));
    choices.set(&catalog, "fpscap", value("400"));
    choices.set(&catalog, "lod", value("medium"));
    assert!(choices.modules().is_empty(), "{choices:?}");
    let mut custom = Choices::new(Preset::Custom);
    custom.set(&catalog, "lod", ModuleChoice::NotSet);
    assert!(
        custom.modules().is_empty(),
        "Custom already leaves lod unset"
    );
}

#[test]
fn reset_and_clear_drop_choices_and_keep_the_preset() {
    let catalog = module_catalog();
    let mut choices = Choices::new(Preset::High);
    choices.set(&catalog, "fpscap", value("144"));
    choices.set(&catalog, "sound", value("low"));
    choices.reset("fpscap");
    assert_eq!(choices.modules().keys().collect::<Vec<_>>(), ["sound"]);
    choices.clear();
    assert!(choices.modules().is_empty());
    assert_eq!(choices.preset(), Preset::High);
}

#[test]
fn choices_the_catalog_no_longer_has_are_dropped_by_name() {
    let catalog = module_catalog();
    let mut choices = Choices::new(Preset::Medium);
    choices.set(&catalog, "fpscap", value("120"));
    choices.set(&catalog, "gone", value("on"));
    choices.set(&catalog, "lod", value("very_low"));
    choices.set(&catalog, "sound", ModuleChoice::NotSet);
    assert_eq!(choices.retain_known(&catalog), ["gone", "lod"]);
    let kept: Vec<&str> = choices.modules().keys().map(String::as_str).collect();
    assert_eq!(kept, ["fpscap", "sound"]);
}

#[test]
fn low_sound_and_no_decals_pick_the_override() {
    let catalog = module_catalog();
    let mask = |pairs: &[(&str, ModuleChoice)]| {
        let mut choices = Choices::new(Preset::Ultra);
        for (name, choice) in pairs {
            choices.set(&catalog, name, choice.clone());
        }
        choices.override_mask()
    };
    assert_eq!(mask(&[]), 0);
    assert_eq!(mask(&[("sound", value("low"))]), 1);
    assert_eq!(mask(&[("decals", value("off"))]), 2);
    assert_eq!(
        mask(&[("sound", value("low")), ("decals", value("off"))]),
        3
    );
    assert_eq!(
        mask(&[("sound", value("medium")), ("decals", ModuleChoice::NotSet)]),
        0
    );
}
