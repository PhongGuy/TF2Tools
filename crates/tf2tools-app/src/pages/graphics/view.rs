//! The Graphics page's view-model: the catalogue and the saved choices turned into rows,
//! changes, sections and an empty state, so rendering only reads plain data.

use gpui_kit::SharedString;
use tf2tools_core::{Choices, ComfigModule, ModuleCatalog, ModuleChoice};

use super::model::{preset_label, shown_choice};
use crate::i18n::mastercomfig::{
    category_label, choice_label, choice_label_by_id, module_description, module_label,
    module_label_by_name,
};
use crate::i18n::tr;

/// The control a row edits its module with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ControlKind {
    Switch,
    Segmented,
    Select,
}

/// The most values a segmented control holds before a row falls back to a Select.
pub const SEGMENTED_MOST: usize = 4;

/// A switch for a plain off/on module, segments for a few values, a Select for the rest.
pub fn control_kind(module: &ComfigModule) -> ControlKind {
    let ids: Vec<&str> = module.values().iter().map(|v| v.value()).collect();
    match ids.as_slice() {
        ["off", "on"] => ControlKind::Switch,
        ids if (2..=SEGMENTED_MOST).contains(&ids.len()) => ControlKind::Segmented,
        _ => ControlKind::Select,
    }
}

/// One choice a row offers, with its words.
#[derive(Debug, Clone, PartialEq)]
pub struct ChoiceView {
    pub choice: ModuleChoice,
    pub label: SharedString,
}

/// One setting's row.
#[derive(Debug, Clone, PartialEq)]
pub struct RowView {
    pub module: String,
    pub label: SharedString,
    pub description: Option<SharedString>,
    pub kind: ControlKind,
    /// The saved choice, else the preset's value.
    pub shown: ModuleChoice,
    /// What the preset sets, or NotSet when it leaves the module to TF2.
    pub preset_value: ModuleChoice,
    /// A saved choice exists and differs from the preset's value.
    pub changed: bool,
    /// "Preset: value", naming what the preset would set.
    pub preset_text: SharedString,
    /// A Select's entries start with NotSet; switches and segments offer only values.
    pub options: Vec<ChoiceView>,
}

/// The row for `module` under the saved `choices`.
pub fn row(catalog: &ModuleCatalog, choices: &Choices, module: &ComfigModule) -> RowView {
    let name = module.name();
    let kind = control_kind(module);
    let preset_value = catalog
        .resolve(name, choices.preset())
        .map_or(ModuleChoice::NotSet, |v| ModuleChoice::Value(v.to_owned()));
    let changed = choices
        .get(name)
        .is_some_and(|saved| *saved != preset_value);
    let preset_text = tr!(
        "graphics.row.preset_value",
        preset = preset_label(choices.preset()),
        value = choice_label(module, &preset_value)
    );
    let unset = (kind == ControlKind::Select).then_some(ModuleChoice::NotSet);
    let options = unset
        .into_iter()
        .chain(
            module
                .values()
                .iter()
                .map(|v| ModuleChoice::Value(v.value().to_owned())),
        )
        .map(|choice| ChoiceView {
            label: choice_label(module, &choice),
            choice,
        })
        .collect();
    RowView {
        module: name.to_owned(),
        label: module_label(module),
        description: module_description(module),
        kind,
        shown: shown_choice(catalog, choices, name),
        preset_value,
        changed,
        preset_text,
        options,
    }
}

/// One line of the changes strip; `was` is None when there is no catalogue to resolve it.
#[derive(Debug, Clone, PartialEq)]
pub struct ChangeView {
    pub module: String,
    pub label: SharedString,
    pub was: Option<SharedString>,
    pub now: SharedString,
}

/// The player's changes from the preset: changed rows in catalogue order, or without a
/// catalogue every saved choice by module name, worded from the keys alone.
pub fn changes(catalog: Option<&ModuleCatalog>, choices: &Choices) -> Vec<ChangeView> {
    let Some(catalog) = catalog else {
        return choices
            .modules()
            .iter()
            .map(|(name, choice)| ChangeView {
                module: name.clone(),
                label: module_label_by_name(name),
                was: None,
                now: choice_label_by_id(name, choice),
            })
            .collect();
    };
    catalog
        .categories()
        .iter()
        .flat_map(|c| c.modules())
        .map(|module| (module, row(catalog, choices, module)))
        .filter(|(_, row)| row.changed)
        .map(|(module, row)| ChangeView {
            was: Some(choice_label(module, &row.preset_value)),
            now: choice_label(module, &row.shown),
            module: row.module,
            label: row.label,
        })
        .collect()
}

/// What the list shows: rows whose words hold the query, and only changed ones on request.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Filter {
    pub query: String,
    pub changed_only: bool,
}

impl Filter {
    // Unicode lowercasing on both sides, so Cyrillic labels match whatever case is typed.
    fn needle(&self) -> String {
        self.query.trim().to_lowercase()
    }

    /// Whether `row` passes: the trimmed query in its label or description, ignoring case.
    pub fn admits(&self, row: &RowView) -> bool {
        if self.changed_only && !row.changed {
            return false;
        }
        let needle = self.needle();
        let holds = |text: &SharedString| text.to_lowercase().contains(&needle);
        needle.is_empty() || holds(&row.label) || row.description.as_ref().is_some_and(holds)
    }
}

/// One category's section; `count` and `has_changes` describe all its modules, `rows` only
/// the ones the filter admits.
#[derive(Debug, Clone, PartialEq)]
pub struct SectionView {
    pub key: String,
    pub label: SharedString,
    pub count: usize,
    pub has_changes: bool,
    pub rows: Vec<RowView>,
}

/// Every category in catalogue order, each kept even when the filter leaves it empty.
pub fn sections(catalog: &ModuleCatalog, choices: &Choices, filter: &Filter) -> Vec<SectionView> {
    catalog
        .categories()
        .iter()
        .map(|category| {
            let all: Vec<RowView> = category
                .modules()
                .iter()
                .map(|module| row(catalog, choices, module))
                .collect();
            SectionView {
                key: category.key().to_owned(),
                label: category_label(category),
                count: all.len(),
                has_changes: all.iter().any(|r| r.changed),
                rows: all.into_iter().filter(|r| filter.admits(r)).collect(),
            }
        })
        .collect()
}

/// Why the list shows no rows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EmptyState {
    /// Nothing matches this trimmed query.
    NoMatch(String),
    /// "Changed only" is on and nothing differs from the preset.
    NothingChanged,
}

/// None while any row shows; otherwise the reason, blaming the query whenever there is one.
pub fn empty_state(sections: &[SectionView], filter: &Filter) -> Option<EmptyState> {
    if sections.iter().any(|s| !s.rows.is_empty()) {
        return None;
    }
    let query = filter.query.trim();
    if filter.changed_only && query.is_empty() {
        Some(EmptyState::NothingChanged)
    } else {
        Some(EmptyState::NoMatch(query.to_owned()))
    }
}
