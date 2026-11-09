//! mastercomfig's module catalog for one release, read from its `modules.json` and
//! `preset_modules.json`, and the player's module choices over a preset.

use std::collections::BTreeMap;

use serde::Deserialize;

use super::Preset;
use crate::error::ModuleCatalogError;

/// mastercomfig's own category order; any other category follows alphabetically.
const CATEGORY_ORDER: [&str; 5] = ["graphics", "hud", "sound", "misc", "networking"];

/// The modules of one mastercomfig release and the value each preset gives them.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct ModuleCatalog {
    tag: String,
    categories: Vec<ModuleCategory>,
    /// `preset_modules.json`: module name to preset name (or "default") to value.
    presets: BTreeMap<String, BTreeMap<String, String>>,
}

/// One group of modules, such as Graphics or HUD.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct ModuleCategory {
    key: String,
    label: String,
    modules: Vec<ComfigModule>,
}

/// One mastercomfig module and the values it can take.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct ComfigModule {
    name: String,
    label: String,
    description: Option<String>,
    values: Vec<ModuleValue>,
}

/// One value a module can take.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct ModuleValue {
    value: String,
    label: String,
}

/// The player's choice for one module.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ModuleChoice {
    /// One of the module's values.
    Value(String),
    /// The module is left unset, so mastercomfig's own default applies.
    NotSet,
}

/// The player's preset and the modules they set away from it.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct Choices {
    preset: Preset,
    modules: BTreeMap<String, ModuleChoice>,
}

#[derive(Deserialize)]
struct RawCategory {
    display: Option<String>,
    modules: Vec<RawModule>,
}

#[derive(Deserialize)]
struct RawModule {
    name: String,
    display: Option<String>,
    description: Option<String>,
    values: Vec<RawValue>,
}

/// A value is either a bare string or an object with its own display name.
#[derive(Deserialize)]
#[serde(untagged)]
enum RawValue {
    Plain(String),
    Full {
        value: String,
        display: Option<String>,
    },
}

fn format_error(e: serde_json::Error) -> ModuleCatalogError {
    ModuleCatalogError::Format {
        reason: e.to_string(),
    }
}

impl ModuleCatalog {
    /// Reads a release's catalog from the bytes of `modules.json` and `preset_modules.json`.
    /// Only a module's `name` and `values` are required; anything else malformed is `Format`.
    pub fn parse(
        tag: &str,
        modules: &[u8],
        preset_modules: &[u8],
    ) -> Result<Self, ModuleCatalogError> {
        let raw: BTreeMap<String, RawCategory> =
            serde_json::from_slice(modules).map_err(format_error)?;
        let presets = serde_json::from_slice(preset_modules).map_err(format_error)?;
        let mut categories: Vec<ModuleCategory> = raw
            .into_iter()
            .map(|(key, category)| ModuleCategory {
                label: category.display.unwrap_or_else(|| readable_name(&key)),
                modules: category
                    .modules
                    .into_iter()
                    .map(ComfigModule::from)
                    .collect(),
                key,
            })
            .collect();
        // The map already sorts keys, so a stable sort on the known rank keeps the rest
        // alphabetical.
        categories.sort_by_key(|c| {
            CATEGORY_ORDER
                .iter()
                .position(|k| *k == c.key)
                .unwrap_or(CATEGORY_ORDER.len())
        });
        Ok(Self {
            tag: tag.to_owned(),
            categories,
            presets,
        })
    }

    /// The release tag this catalog was read for.
    pub fn tag(&self) -> &str {
        &self.tag
    }

    /// The categories in mastercomfig's order.
    pub fn categories(&self) -> &[ModuleCategory] {
        &self.categories
    }

    /// The module named `name`, in any category.
    pub fn module(&self, name: &str) -> Option<&ComfigModule> {
        self.categories
            .iter()
            .flat_map(|c| c.modules.iter())
            .find(|m| m.name == name)
    }

    /// The value `preset` gives `module`: its own entry, else the module's "default";
    /// None when neither exists or the value is not one of the module's values.
    pub fn resolve(&self, module: &str, preset: Preset) -> Option<&str> {
        let levels = self.presets.get(module)?;
        let value = levels
            .get(preset.name())
            .or_else(|| levels.get("default"))?;
        self.module(module)?
            .values
            .iter()
            .find(|v| v.value == *value)
            .map(|v| v.value.as_str())
    }
}

impl ModuleCategory {
    /// mastercomfig's key, such as "graphics".
    pub fn key(&self) -> &str {
        &self.key
    }

    /// The name shown to the player.
    pub fn label(&self) -> &str {
        &self.label
    }

    /// The category's modules in file order.
    pub fn modules(&self) -> &[ComfigModule] {
        &self.modules
    }
}

impl From<RawModule> for ComfigModule {
    fn from(raw: RawModule) -> Self {
        Self {
            label: raw.display.unwrap_or_else(|| readable_name(&raw.name)),
            name: raw.name,
            description: raw.description,
            values: raw
                .values
                .into_iter()
                .map(|v| {
                    let (value, display) = match v {
                        RawValue::Plain(value) => (value, None),
                        RawValue::Full { value, display } => (value, display),
                    };
                    ModuleValue {
                        label: display.unwrap_or_else(|| readable_name(&value)),
                        value,
                    }
                })
                .collect(),
        }
    }
}

impl ComfigModule {
    /// mastercomfig's name, as written into `modules.cfg`.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The name shown to the player.
    pub fn label(&self) -> &str {
        &self.label
    }

    /// mastercomfig's description, if it has one.
    pub fn description(&self) -> Option<&str> {
        self.description.as_deref()
    }

    /// The values in mastercomfig's order.
    pub fn values(&self) -> &[ModuleValue] {
        &self.values
    }

    /// True when `value` is one of this module's values.
    pub fn has_value(&self, value: &str) -> bool {
        self.values.iter().any(|v| v.value == value)
    }
}

impl ModuleValue {
    /// The value as written into `modules.cfg`.
    pub fn value(&self) -> &str {
        &self.value
    }

    /// The name shown to the player.
    pub fn label(&self) -> &str {
        &self.label
    }
}

impl Choices {
    /// No module choices over `preset`.
    pub fn new(preset: Preset) -> Self {
        Self {
            preset,
            modules: BTreeMap::new(),
        }
    }

    /// Choices exactly as saved, without dropping any that equal the preset's value.
    pub(crate) fn from_saved(preset: Preset, modules: BTreeMap<String, ModuleChoice>) -> Self {
        Self { preset, modules }
    }

    /// The preset the choices sit on.
    pub fn preset(&self) -> Preset {
        self.preset
    }

    /// Every module the player set away from the preset.
    pub fn modules(&self) -> &BTreeMap<String, ModuleChoice> {
        &self.modules
    }

    /// The player's choice for `module`, if they made one.
    pub fn get(&self, module: &str) -> Option<&ModuleChoice> {
        self.modules.get(module)
    }

    /// Records `choice`; one equal to the preset's resolved value (NotSet equals None) removes
    /// the entry.
    pub fn set(&mut self, catalog: &ModuleCatalog, module: &str, choice: ModuleChoice) {
        let same = match (&choice, catalog.resolve(module, self.preset)) {
            (ModuleChoice::Value(v), Some(preset)) => v == preset,
            (ModuleChoice::NotSet, None) => true,
            (ModuleChoice::Value(_), None) | (ModuleChoice::NotSet, Some(_)) => false,
        };
        if same {
            self.modules.remove(module);
        } else {
            self.modules.insert(module.to_owned(), choice);
        }
    }

    /// Drops the choice for `module`, so the preset's value applies.
    pub fn reset(&mut self, module: &str) {
        self.modules.remove(module);
    }

    /// Drops every choice and keeps the preset.
    pub fn clear(&mut self) {
        self.modules.clear();
    }

    /// Drops entries whose module is not in `catalog` or whose Value is not among its values;
    /// returns their names in order.
    pub fn retain_known(&mut self, catalog: &ModuleCatalog) -> Vec<String> {
        let mut dropped = Vec::new();
        self.modules.retain(|name, choice| {
            let known = catalog.module(name).is_some_and(|m| match choice {
                ModuleChoice::Value(v) => m.has_value(v),
                ModuleChoice::NotSet => true,
            });
            if !known {
                dropped.push(name.clone());
            }
            known
        });
        dropped
    }

    /// Bit 1 for a saved sound=low, bit 2 for a saved decals=off; 0 for neither.
    pub fn override_mask(&self) -> u8 {
        let chose = |module: &str, value: &str| {
            self.get(module) == Some(&ModuleChoice::Value(value.to_owned()))
        };
        u8::from(chose("sound", "low")) | (u8::from(chose("decals", "off")) << 1)
    }
}

/// "motion_blur" -> "Motion blur": underscores as spaces, first letter upper case.
pub fn readable_name(name: &str) -> String {
    let spaced = name.replace('_', " ");
    let mut chars = spaced.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}
