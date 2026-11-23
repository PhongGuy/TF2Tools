//! The committed setting pictures manifest against mastercomfig 9.100.1's own module list.

use std::collections::BTreeMap;
use std::path::PathBuf;

use tf2tools_core::{ComfigModule, ModuleCatalog, REASON_KEYS, SettingPictures};

const MODULES: &[u8] = include_bytes!("fixtures/mastercomfig/modules-9.100.1.json");
const PRESETS: &[u8] = include_bytes!("fixtures/mastercomfig/preset_modules.json");

fn pictures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets/setting-pictures")
}

/// Pictured modules from outside the graphics category, as (category, module), in manifest order.
const EXTRA_PICTURED: &[(&str, &str)] = &[("hud", "hud_player_model")];

/// mastercomfig's graphics modules in its order, then each extra pictured module, with their values.
fn pictured() -> Vec<(String, Vec<String>)> {
    let catalog = ModuleCatalog::parse("9.100.1", MODULES, PRESETS).unwrap();
    let category = |key: &str| {
        catalog
            .categories()
            .iter()
            .find(|c| c.key() == key)
            .unwrap_or_else(|| panic!("no {key} category"))
    };
    let named = |m: &ComfigModule| {
        (
            m.name().to_owned(),
            m.values().iter().map(|v| v.value().to_owned()).collect(),
        )
    };
    let mut modules: Vec<(String, Vec<String>)> =
        category("graphics").modules().iter().map(named).collect();
    for (key, name) in EXTRA_PICTURED {
        let module = category(key).modules().iter().find(|m| m.name() == *name);
        modules.push(named(module.unwrap_or_else(|| panic!("no {name} in {key}"))));
    }
    modules
}

#[test]
fn the_manifest_lists_every_graphics_module_then_the_extras_in_order() {
    let pics = SettingPictures::open(&pictures_dir()).unwrap();
    let ours: Vec<(String, Vec<String>)> = pics
        .manifest()
        .settings
        .iter()
        .map(|s| {
            (
                s.module.clone(),
                s.values.iter().map(|v| v.value.clone()).collect(),
            )
        })
        .collect();
    assert_eq!(ours, pictured());
    assert_eq!(pics.manifest().mastercomfig, "9.100.1");
    assert_eq!(pics.manifest().size, [960, 540]);
}

#[test]
fn the_base_sets_every_visual_module_to_one_of_its_values() {
    let pics = SettingPictures::open(&pictures_dir()).unwrap();
    let modules: BTreeMap<String, Vec<String>> = pictured().into_iter().collect();
    for setting in &pics.manifest().settings {
        if setting.no_picture.is_some() {
            assert!(
                !pics.manifest().base.contains_key(&setting.module),
                "{}",
                setting.module
            );
            continue;
        }
        let base = pics.manifest().base.get(&setting.module);
        let base = base.unwrap_or_else(|| panic!("{} has no base value", setting.module));
        assert!(
            modules[&setting.module].contains(base),
            "{}={base}",
            setting.module
        );
    }
    assert!(pics.manifest().base.keys().all(|k| modules.contains_key(k)));
}

#[test]
fn reasons_are_known_keys_and_same_look_values_point_at_a_shot_value() {
    let pics = SettingPictures::open(&pictures_dir()).unwrap();
    for setting in &pics.manifest().settings {
        if let Some(reason) = &setting.no_picture {
            assert!(
                REASON_KEYS.contains(&reason.as_str()),
                "{}: {reason}",
                setting.module
            );
        }
        for value in &setting.values {
            if let Some(reason) = &value.reason {
                assert!(
                    REASON_KEYS.contains(&reason.as_str()),
                    "{}={}: {reason}",
                    setting.module,
                    value.value
                );
            }
            if let Some(target) = &value.same_as {
                let other = setting.values.iter().find(|v| &v.value == target);
                let other = other
                    .unwrap_or_else(|| panic!("{}={} -> {target}", setting.module, value.value));
                assert!(
                    other.same_as.is_none(),
                    "{}={target} is itself a same_as",
                    setting.module
                );
                assert!(
                    value.reason.is_some(),
                    "{}={} has no reason",
                    setting.module,
                    value.value
                );
            }
        }
    }
}
