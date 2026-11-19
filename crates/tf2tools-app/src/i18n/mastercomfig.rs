//! mastercomfig's modules, values and categories in the current language. Keys are built at
//! runtime from catalogue names, and a word the catalogue lacks falls back to mastercomfig's
//! English, so a module a later release adds still shows.

use gpui_kit::SharedString;
use tf2tools_core::{ComfigModule, ModuleCategory, ModuleChoice, ModuleValue, readable_name};

use super::names::lookup;
use super::tr;

// Built with format!, so the key scans never read a literal under these runtime prefixes.

/// `graphics.module.{module}.{field}`, such as a module's label or description.
pub fn module_key(module: &str, field: &str) -> String {
    format!("graphics.module.{module}.{field}")
}

/// `graphics.value.{value}`: the wording every module shares for a value id.
pub fn value_key(value: &str) -> String {
    format!("graphics.value.{value}")
}

/// `graphics.module.{module}.value.{value}`: one module's own wording for a value id.
pub fn value_override_key(module: &str, value: &str) -> String {
    format!("graphics.module.{module}.value.{value}")
}

/// `graphics.category.{key}`: a category's heading.
pub fn category_key(key: &str) -> String {
    format!("graphics.category.{key}")
}

// The first of `keys` the current language has, or `fallback`.
fn first_of(keys: [String; 2], fallback: &str) -> SharedString {
    let locale = super::current();
    let [first, second] = keys;
    match crate::_rust_i18n_try_translate(locale, &first) {
        Some(text) => text.into_owned().into(),
        None => lookup(locale, Some(second), fallback),
    }
}

/// The module's label; mastercomfig's own for a module we have no words for.
pub fn module_label(module: &ComfigModule) -> SharedString {
    lookup(
        super::current(),
        Some(module_key(module.name(), "label")),
        module.label(),
    )
}

/// The module's description, or `None` for a module we have no words for, since
/// mastercomfig's own is written for config authors rather than players.
pub fn module_description(module: &ComfigModule) -> Option<SharedString> {
    crate::_rust_i18n_try_translate(super::current(), module_key(module.name(), "description"))
        .map(|text| text.into_owned().into())
}

/// The value's label: a number such as a frame cap shows as it is; otherwise the module's own
/// wording, the shared wording, then mastercomfig's label.
pub fn value_label(module: &ComfigModule, value: &ModuleValue) -> SharedString {
    value_words(module.name(), value.value(), value.label())
}

// The keyed wording for `module`'s value `id`, or `fallback`; digit-led ids are never keyed.
fn value_words(module: &str, id: &str, fallback: &str) -> SharedString {
    if id.starts_with(|c: char| c.is_ascii_digit()) {
        return id.to_owned().into();
    }
    first_of([value_override_key(module, id), value_key(id)], fallback)
}

/// The choice's label: "Let TF2 decide" for no choice, a value of the module by its label, and
/// an id the module does not offer as the id itself.
pub fn choice_label(module: &ComfigModule, choice: &ModuleChoice) -> SharedString {
    match choice {
        ModuleChoice::Value(id) => module
            .values()
            .iter()
            .find(|v| v.value() == id)
            .map_or_else(|| id.clone().into(), |v| value_label(module, v)),
        ModuleChoice::NotSet | _ => tr!("graphics.not_set"),
    }
}

/// The category's heading; mastercomfig's own for a category we have no words for.
pub fn category_label(category: &ModuleCategory) -> SharedString {
    lookup(
        super::current(),
        Some(category_key(category.key())),
        category.label(),
    )
}

/// A module's label without a catalogue: its keyed label, else its name made readable.
pub fn module_label_by_name(module: &str) -> SharedString {
    lookup(
        super::current(),
        Some(module_key(module, "label")),
        &readable_name(module),
    )
}

/// A saved choice's label without a catalogue: keyed wording by id, else the id made readable.
pub fn choice_label_by_id(module: &str, choice: &ModuleChoice) -> SharedString {
    match choice {
        ModuleChoice::Value(id) => value_words(module, id, &readable_name(id)),
        ModuleChoice::NotSet | _ => tr!("graphics.not_set"),
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use tf2tools_core::ModuleCatalog;

    use super::{category_key, module_key, value_key, value_override_key};

    fn key(parts: &[&str]) -> String {
        parts.join(".")
    }

    #[test]
    fn keys_sit_under_the_graphics_surface() {
        assert_eq!(
            module_key("lod", "label"),
            key(&["graphics", "module", "lod", "label"])
        );
        assert_eq!(
            value_key("very_low"),
            key(&["graphics", "value", "very_low"])
        );
        assert_eq!(
            value_override_key("post_processing", "low"),
            key(&["graphics", "module", "post_processing", "value", "low"])
        );
        assert_eq!(category_key("hud"), key(&["graphics", "category", "hud"]));
    }

    const MODULES: &[u8] =
        include_bytes!("../../../tf2tools-core/tests/fixtures/mastercomfig/modules-9.100.1.json");

    fn catalog() -> ModuleCatalog {
        ModuleCatalog::parse("9.100.1", MODULES, b"{}").unwrap()
    }

    fn messages(code: &str) -> BTreeMap<String, String> {
        crate::_rust_i18n_backend()
            .messages_for_locale(code)
            .unwrap_or_default()
            .into_iter()
            .map(|(k, v)| (k.into_owned(), v.into_owned()))
            .collect()
    }

    // The modules of the categories `wanted` admits that lack an English label or description.
    fn unworded(wanted: impl Fn(&str) -> bool) -> Vec<String> {
        let en = messages("en");
        catalog()
            .categories()
            .iter()
            .filter(|c| wanted(c.key()))
            .flat_map(|c| c.modules())
            .flat_map(|m| {
                [
                    module_key(m.name(), "label"),
                    module_key(m.name(), "description"),
                ]
            })
            .filter(|k| !en.contains_key(k))
            .collect()
    }

    #[test]
    fn every_graphics_module_of_9_100_1_has_an_english_label_and_description() {
        let missing = unworded(|key| key == "graphics");
        assert!(missing.is_empty(), "no English for {missing:?}");
    }

    #[test]
    fn every_other_module_of_9_100_1_has_an_english_label_and_description() {
        let missing = unworded(|key| key != "graphics");
        assert!(missing.is_empty(), "no English for {missing:?}");
    }

    #[test]
    fn a_saved_choice_is_named_without_a_catalogue() {
        use tf2tools_core::ModuleChoice;
        let _lock = crate::i18n::test_lock();
        crate::i18n::apply("en");
        assert_eq!(
            super::module_label_by_name("fpscap").as_ref(),
            "Frame rate cap"
        );
        assert_eq!(super::module_label_by_name("made_up").as_ref(), "Made up");
        let named = |m: &str, c: ModuleChoice| super::choice_label_by_id(m, &c).to_string();
        assert_eq!(named("fpscap", ModuleChoice::Value("120".into())), "120");
        assert_eq!(named("lod", ModuleChoice::NotSet), "Let TF2 decide");
        assert_eq!(named("vsync", ModuleChoice::Value("on".into())), "On");
    }

    #[test]
    fn a_module_reads_our_label_and_a_future_one_mastercomfig_s() {
        let _lock = crate::i18n::test_lock();
        crate::i18n::apply("en");
        let catalog = catalog();
        let lod = catalog.module("lod").unwrap();
        assert_eq!(super::module_label(lod).as_ref(), "Model quality");
        assert_eq!(
            super::module_description(lod).as_deref(),
            Some("How detailed players, weapons and props are up close.")
        );
        let future = ModuleCatalog::parse(
            "9.200.0",
            br#"{"graphics":{"modules":[{"name":"new_thing","display":"New Thing","description":"Theirs.","values":["off","on"]}]}}"#,
            b"{}",
        )
        .unwrap();
        let new = future.module("new_thing").unwrap();
        assert_eq!(super::module_label(new).as_ref(), "New Thing");
        assert_eq!(super::module_description(new), None);
    }

    #[test]
    fn every_word_value_and_category_of_9_100_1_has_english() {
        let en = messages("en");
        let catalog = catalog();
        let mut missing: Vec<String> = Vec::new();
        for category in catalog.categories() {
            if !en.contains_key(&category_key(category.key())) {
                missing.push(category.key().to_owned());
            }
            for module in category.modules() {
                for v in module.values() {
                    if v.value().starts_with(|c: char| c.is_ascii_digit()) {
                        continue;
                    }
                    let keys = [
                        value_override_key(module.name(), v.value()),
                        value_key(v.value()),
                    ];
                    if !keys.iter().any(|k| en.contains_key(k)) {
                        missing.push(format!("{}={}", module.name(), v.value()));
                    }
                }
            }
        }
        assert!(missing.is_empty(), "no English for {missing:?}");
    }

    #[test]
    fn an_override_beats_the_shared_word_and_numbers_show_as_is() {
        let _lock = crate::i18n::test_lock();
        crate::i18n::apply("en");
        let catalog = catalog();
        let label = |m: &str, v: &str| {
            let module = catalog.module(m).unwrap();
            let value = module.values().iter().find(|x| x.value() == v).unwrap();
            super::value_label(module, value).to_string()
        };
        assert_eq!(label("post_processing", "low"), "Bloom only");
        assert_eq!(label("lod", "low"), "Low");
        assert_eq!(label("phong", "rim"), "Rim light");
        assert_eq!(label("fpscap", "120"), "120");
        assert_eq!(label("bandwidth", "1.0Mbps"), "1.0Mbps");
        let filter = catalog.module("texture_filter").unwrap();
        let shown = super::choice_label(
            filter,
            &tf2tools_core::ModuleChoice::Value("aniso16x".into()),
        );
        assert_eq!(shown.as_ref(), "Anisotropic 16x");
        let hud = &catalog.categories()[1];
        assert_eq!(super::category_label(hud).as_ref(), "HUD");
    }
}
