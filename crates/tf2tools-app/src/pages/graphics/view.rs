//! The Graphics page's view-model: the catalogue and the saved choices turned into rows,
//! changes, sections and an empty state, so rendering only reads plain data.

use std::collections::BTreeMap;

use gpui_kit::SharedString;
use tf2tools_core::{Choices, ComfigModule, ModuleCatalog, ModuleChoice};

use super::model::{preset_choice, preset_label, select_choices, shown_choice, value_choices};
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
pub const SEGMENTED_MOST: usize = 5;

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
    /// A saved choice exists, whether or not it equals the preset's value.
    pub changed: bool,
    /// "Preset: value", naming what the preset would set.
    pub preset_text: SharedString,
    /// The preset's own name, for labels that already show the value.
    pub preset_name: SharedString,
    /// A Select's entries start with NotSet; switches and segments offer only values.
    pub options: Vec<ChoiceView>,
}

/// The row for `module` under the saved `choices`.
pub fn row(catalog: &ModuleCatalog, choices: &Choices, module: &ComfigModule) -> RowView {
    let name = module.name();
    let kind = control_kind(module);
    let preset_value = preset_choice(catalog, choices, name);
    // Any saved choice counts, even one a later preset switch made equal to the preset: it
    // still pins the value in modules.cfg across switches, so it keeps its Reset.
    let changed = choices.get(name).is_some();
    let preset_name = preset_label(choices.preset());
    let preset_text = tr!(
        "graphics.row.preset_value",
        preset = preset_name.clone(),
        value = choice_label(module, &preset_value)
    );
    let offered = match kind {
        ControlKind::Select => select_choices(module),
        ControlKind::Switch | ControlKind::Segmented => value_choices(module).collect(),
    };
    let options = offered
        .into_iter()
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
        preset_name,
        options,
    }
}

/// One line of the changes strip; `was` is None when there is no catalogue to resolve it.
/// `gone` marks a saved choice for a setting the catalogue no longer has.
#[derive(Debug, Clone, PartialEq)]
pub struct ChangeView {
    pub module: String,
    pub label: SharedString,
    pub was: Option<SharedString>,
    pub now: SharedString,
    /// The saved value equals the preset's, as after a preset switch.
    pub same: bool,
    pub gone: bool,
}

/// The player's changes from the preset: changed rows in catalogue order, then saved choices
/// for settings the catalogue dropped (`gone`), or without a catalogue every saved choice by
/// module name, worded from the keys alone.
pub fn changes(catalog: Option<&ModuleCatalog>, choices: &Choices) -> Vec<ChangeView> {
    BuiltRows::build(catalog, choices).changes
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
    BuiltRows::build(Some(catalog), choices).sections(filter)
}

/// Every row of a catalogue under one set of choices, built once so the sections, the changes
/// strip and the pictures all read the same rows instead of each building their own.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct BuiltRows {
    /// Every category with all its rows, unfiltered.
    all: Vec<SectionView>,
    changes: Vec<ChangeView>,
    /// Each module's section and row index in `all`.
    at: BTreeMap<String, (usize, usize)>,
}

impl BuiltRows {
    /// The rows of `catalog` under `choices`; without a catalogue only the changes, by name.
    pub fn build(catalog: Option<&ModuleCatalog>, choices: &Choices) -> Self {
        let by_name = |(name, choice): (&String, &ModuleChoice)| ChangeView {
            module: name.clone(),
            label: module_label_by_name(name),
            was: None,
            now: choice_label_by_id(name, choice),
            same: false,
            gone: catalog.is_some(),
        };
        let Some(catalog) = catalog else {
            return Self {
                changes: choices.modules().iter().map(by_name).collect(),
                ..Self::default()
            };
        };
        let mut built = Self::default();
        for (s, category) in catalog.categories().iter().enumerate() {
            let mut rows = Vec::with_capacity(category.modules().len());
            for (r, module) in category.modules().iter().enumerate() {
                let row = row(catalog, choices, module);
                if row.changed {
                    built.changes.push(ChangeView {
                        module: row.module.clone(),
                        label: row.label.clone(),
                        was: Some(choice_label(module, &row.preset_value)),
                        now: choice_label(module, &row.shown),
                        same: row.shown == row.preset_value,
                        gone: false,
                    });
                }
                built.at.insert(row.module.clone(), (s, r));
                rows.push(row);
            }
            built.all.push(SectionView {
                key: category.key().to_owned(),
                label: category_label(category),
                count: rows.len(),
                has_changes: rows.iter().any(|r| r.changed),
                rows,
            });
        }
        // A newer mastercomfig can drop a saved setting; it stays listed so it can be removed.
        let dropped = choices
            .modules()
            .iter()
            .filter(|(name, _)| catalog.module(name).is_none())
            .map(by_name);
        built.changes.extend(dropped);
        built
    }

    /// `module`'s row; None for a module the catalogue lacks.
    pub fn row(&self, module: &str) -> Option<&RowView> {
        let &(s, r) = self.at.get(module)?;
        Some(&self.all[s].rows[r])
    }

    /// Every row in catalogue order.
    pub fn rows(&self) -> impl Iterator<Item = &RowView> {
        self.all.iter().flat_map(|s| &s.rows)
    }

    /// The changes strip's lines, as `changes` words them.
    pub fn changes(&self) -> &[ChangeView] {
        &self.changes
    }

    /// Every category, its rows cut down to the ones `filter` admits.
    pub fn sections(&self, filter: &Filter) -> Vec<SectionView> {
        self.all
            .iter()
            .map(|section| SectionView {
                key: section.key.clone(),
                label: section.label.clone(),
                count: section.count,
                has_changes: section.has_changes,
                rows: section
                    .rows
                    .iter()
                    .filter(|r| filter.admits(r))
                    .cloned()
                    .collect(),
            })
            .collect()
    }
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
