//! The HUD's own options: switches a copy's files ship (commented `#base` and manifest lines,
//! renamed, `_enabled` and `_disabled` folders), read as options and applied as a switch layer
//! beneath the model.

use std::collections::{BTreeMap, BTreeSet};

use super::HudModel;
use super::changes::{Change, ChangeTarget, OptionChange, SetKey, check_switch_path};
use super::document::KvDocument;
use super::edits::EditRefused;
use super::expr::atoi;
use super::files::{CfgFile, HudFiles};
use super::hud_schema::HudSchema;
use super::model::{BaseChains, PartKey, apply_set, live_blocks, root_block};
use super::options::{
    CfgPlace, Choice, Derived, OptionAction, OptionKind, OptionSource, OptionSpec, OptionValue,
    SwitchAction, ValueMap, derived_colour, rgba_text, template_refs, template_text, text_suits,
    value_suits,
};
use super::scheme::Scheme;
use crate::error::HudFilesError;
use crate::fonts::Platform;

/// Detection reads at most this many copy files and bytes.
const MAX_SCAN_FILES: usize = 8_192;
const MAX_SCAN_BYTES: usize = 64 * 1024 * 1024;
/// Detection keeps at most this many options.
const MAX_OPTIONS: usize = 512;
/// The switch layer holds at most this many written files and bytes.
const MAX_LAYER_FILES: usize = 2_048;
const MAX_LAYER_BYTES: usize = 64 * 1024 * 1024;

const MANIFEST: &str = "scripts/hudanimations_manifest.txt";
/// The menu art a shipped background becomes, the folder it ships in, and the chapter list
/// that points the menu at it.
const MENU_ART: &str = "materials/console/background_upward.vtf";
const MENU_ART_WIDE: &str = "materials/console/background_upward_widescreen.vtf";
pub(super) const SHIPPED_ART: &str = "materials/console/_disabled";
const CHAPTERS: &str = "scripts/chapterbackgrounds.txt";
const CHAPTERS_TEXT: &str = "\"chapters\"\n{\n\t1\t\"background_upward\"\n\t2\t\"background_upward\"\n\t3\t\"background_upward\"\n\t4\t\"background_upward\"\n}\n\n\"BackgroundMaps\"\n{\n\t1\t\"background_upward\"\n\t2\t\"background_upward\"\n\t3\t\"background_upward\"\n\t4\t\"background_upward\"\n}\n";
/// The group every switch but an F file's shares.
const SWITCHES: &str = "Switches";

/// A HUD copy's own options, read from its pristine files.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct HudOptions {
    specs: Vec<OptionSpec>,
    refused: usize,
    misfits: usize,
    left_out: usize,
    /// The values schema options start from, read from the copy, by id.
    pristine: BTreeMap<String, OptionValue>,
    platform: Platform,
}

/// No options, as for files that are no copy.
impl Default for HudOptions {
    fn default() -> HudOptions {
        HudOptions {
            specs: Vec::new(),
            refused: 0,
            misfits: 0,
            left_out: 0,
            pristine: BTreeMap::new(),
            platform: Platform::Linux,
        }
    }
}

/// A copy's files with the stored choices applied as a top switch layer.
#[derive(Debug)]
#[non_exhaustive]
pub struct Switched {
    files: HudFiles,
    /// Indexes into the changes given to `switch` of the choices it left out.
    left_out: Vec<usize>,
}

impl Switched {
    /// The switched view.
    pub fn files(&self) -> &HudFiles {
        &self.files
    }

    /// The switched view, owned.
    pub fn into_files(self) -> HudFiles {
        self.files
    }

    /// How many stored choices were left out: unknown, refused or past a cap.
    pub fn dropped(&self) -> usize {
        self.left_out.len()
    }

    /// `changes` (as given to `switch`) without the choices it left out.
    pub(crate) fn kept(&self, changes: &[Change]) -> Vec<Change> {
        (changes.iter().enumerate())
            .filter(|(i, _)| !self.left_out.contains(i))
            .map(|(_, c)| c.clone())
            .collect()
    }

    /// The changes of `changes` (as given to `switch`) that it left out.
    pub(crate) fn left_out<'a>(&self, changes: &'a [Change]) -> Vec<&'a Change> {
        self.left_out
            .iter()
            .filter_map(|&i| changes.get(i))
            .collect()
    }
}

impl HudOptions {
    /// The switches a copy's own files hold (empty for files that are no copy), in path order.
    pub fn detected(files: &HudFiles, platform: Platform) -> HudOptions {
        let mut found = Detector {
            copy: files.copy_paths().into_iter().collect(),
            specs: Vec::new(),
            refused: 0,
        };
        let (texts, skipped) = scan(files, &found.copy);
        found.refused = skipped;
        for (path, text) in texts {
            found.read_file(&path, &text);
        }
        HudOptions {
            specs: found.specs,
            refused: found.refused,
            platform,
            ..HudOptions::default()
        }
    }

    /// Schema options (when `schema` is given) then detected switches, dropping a detected
    /// switch that touches a path a schema option touches.
    pub fn read(files: &HudFiles, schema: Option<&HudSchema>, platform: Platform) -> HudOptions {
        let detected = HudOptions::detected(files, platform);
        let Some(schema) = schema else {
            return detected;
        };
        let compiled = schema.options(files, platform);
        let taken: Vec<String> = (compiled.specs().iter()).flat_map(spec_touched).collect();
        // A path overlaps when it is the same, or one is a folder holding the other.
        let overlaps = |a: &str, b: &str| {
            a == b
                || a.strip_prefix(b).is_some_and(|r| r.starts_with('/'))
                || b.strip_prefix(a).is_some_and(|r| r.starts_with('/'))
        };
        let kept = (detected.specs.into_iter()).filter(|spec| {
            !spec_touched(spec)
                .iter()
                .any(|p| taken.iter().any(|t| overlaps(p, t)))
        });
        let mut specs = compiled.specs().to_vec();
        let pristine = pristine_values(files, &specs, platform);
        specs.extend(kept);
        HudOptions {
            specs,
            refused: detected.refused,
            misfits: compiled.misfits(),
            left_out: compiled.left_out(),
            pristine,
            platform,
        }
    }

    /// Schema controls that name a file, block or folder the copy lacks.
    pub fn misfits(&self) -> usize {
        self.misfits
    }

    /// Schema controls this build does not support or whose paths break the path rule.
    pub fn left_out(&self) -> usize {
        self.left_out
    }

    /// The options, in the order the panel lists them.
    pub fn specs(&self) -> &[OptionSpec] {
        &self.specs
    }

    /// What was left out past a cap: options past the option cap, and files detection did
    /// not read past its file or byte cap.
    pub fn refused(&self) -> usize {
        self.refused
    }

    /// The stored HUD and editor choices in `changes`, applied in order over `files`' pristine view.
    ///
    /// # Errors
    /// When a copy file a choice reads cannot be read.
    pub fn switch(&self, files: &HudFiles, changes: &[Change]) -> Result<Switched, HudFilesError> {
        let mut layer = Layer {
            files,
            copy: files.copy_paths().into_iter().collect(),
            written: BTreeMap::new(),
            removed: BTreeSet::new(),
            cfg: BTreeMap::new(),
        };
        // A text naming another schema switch (`{$name ? a : b}`) reads it as the changes
        // leave it, whatever order they came in.
        let is_on = |name: &str| {
            let id = format!("e:{name}");
            self.value(&id, changes) == Some(OptionValue::Bool(true))
        };
        let mut left_out = Vec::new();
        let mut chosen = BTreeSet::new();
        for (i, change) in changes.iter().enumerate() {
            let ChangeTarget::Option(choice) = change.target() else {
                continue;
            };
            if ![OptionChange::HUD, OptionChange::SCHEMA].contains(&choice.source()) {
                continue;
            }
            if !self.apply_choice(&mut layer, choice, &is_on)? {
                left_out.push(i);
            } else if choice.source() == OptionChange::SCHEMA {
                chosen.insert(choice.id().to_owned());
            }
        }
        // An option whose text names a switch the player changed is written again at its
        // start value, since its file holds the switch's old word.
        for spec in &self.specs {
            let names = spec_refs(spec);
            let named_changed = names.iter().any(|n| chosen.contains(&format!("e:{n}")));
            if named_changed && !chosen.contains(spec.id()) {
                let start = OptionChange::new(OptionChange::SCHEMA, spec.id(), self.start(spec));
                self.apply_choice(&mut layer, &start, &is_on)?;
            }
        }
        // A path back at its pristine bytes needs no entry in the layer.
        let mut written = Vec::new();
        for (path, bytes) in std::mem::take(&mut layer.written) {
            let pristine = files.read_pristine(&path)?.map(|f| f.into_bytes());
            if layer.copy.contains(&path) && pristine.as_ref() == Some(&bytes) {
                continue;
            }
            written.push((path, bytes));
        }
        let removed = layer.removed.into_iter().collect();
        let cfg = layer.cfg.into_values().collect();
        Ok(Switched {
            files: files.with_switches(written, removed).with_switched_cfg(cfg),
            left_out,
        })
    }

    /// Applies one stored choice to `layer`, whole or not at all, so a refused action leaves
    /// no half switch; false when it was refused.
    fn apply_choice(
        &self,
        layer: &mut Layer<'_>,
        choice: &OptionChange,
        is_on: &dyn Fn(&str) -> bool,
    ) -> Result<bool, HudFilesError> {
        let Some(sides) = self.sides(choice) else {
            return Ok(false);
        };
        let saved = (
            layer.written.clone(),
            layer.removed.clone(),
            layer.cfg.clone(),
        );
        let derived = match self.offered(choice.id()).map(OptionSpec::kind) {
            Some(OptionKind::Colour { derived }) => derived.as_slice(),
            _ => &[],
        };
        for (action, on) in sides {
            let ctx = Applying {
                on,
                value: choice.value(),
                derived,
                is_on,
            };
            if !layer.apply(action, &ctx, self.platform)? {
                (layer.written, layer.removed, layer.cfg) = saved;
                return Ok(false);
            }
        }
        if !layer.within_caps() {
            (layer.written, layer.removed, layer.cfg) = saved;
            return Ok(false);
        }
        Ok(true)
    }

    /// The last stored choice for `id` that applies, else the pristine value; None when not offered.
    pub fn value(&self, id: &str, changes: &[Change]) -> Option<OptionValue> {
        let spec = self.offered(id)?;
        let stored = changes
            .iter()
            .rev()
            .find_map(|change| match change.target() {
                ChangeTarget::Option(o)
                    if o.source() == source_of(spec)
                        && o.id() == id
                        && value_suits(spec.kind(), o.value()) =>
                {
                    Some(o.value().clone())
                }
                _ => None,
            });
        Some(stored.unwrap_or_else(|| self.start(spec)))
    }

    /// The value an option has with no stored choice: read from the copy, else its default.
    fn start(&self, spec: &OptionSpec) -> OptionValue {
        (self.pristine.get(spec.id()).cloned()).unwrap_or_else(|| spec.default().clone())
    }

    /// Earlier choices for `id` removed and one appended unless `value` is the pristine value.
    ///
    /// # Errors
    /// NotOffered for an unknown id; BadValue for a wrong kind, range, step or unknown key.
    pub fn set(
        &self,
        id: &str,
        value: OptionValue,
        changes: &[Change],
        group: u32,
    ) -> Result<Vec<Change>, EditRefused> {
        let spec = self.offered(id).ok_or(EditRefused::NotOffered)?;
        if !value_suits(spec.kind(), &value) {
            return Err(EditRefused::BadValue);
        }
        let mut next = without(changes, |source, choice| {
            source == source_of(spec) && choice == id
        });
        if value != self.start(spec) {
            let choice = OptionChange::new(source_of(spec), id, value);
            next.push(Change::option(group, choice));
        }
        Ok(next)
    }

    /// `changes` without the choices of every option in `group` (an option group name).
    pub fn reset_group(&self, group: &str, changes: &[Change]) -> Vec<Change> {
        without(changes, |source, id| {
            (self.specs.iter())
                .any(|s| s.group() == group && s.id() == id && source_of(s) == source)
        })
    }

    /// `changes` without the choices of every one of these options, all groups at once.
    pub fn reset_all(&self, changes: &[Change]) -> Vec<Change> {
        without(changes, |source, id| {
            (self.specs.iter()).any(|s| s.id() == id && source_of(s) == source)
        })
    }

    /// Lowercase paths option `id` touches; empty for an unknown id.
    pub fn touched(&self, id: &str) -> Vec<String> {
        self.offered(id).map(spec_touched).unwrap_or_default()
    }

    /// The options tied to `part`, in spec order: those touching a file the part reads or a
    /// `#base` path its chains name, or a folder holding one.
    pub fn for_part(&self, model: &HudModel, part: &PartKey) -> Vec<&OptionSpec> {
        let files = model.part_files(part);
        if files.is_empty() {
            return Vec::new();
        }
        let ties = |touched: &str| {
            let folder = format!("{touched}/");
            files.iter().any(|f| f == touched || f.starts_with(&folder))
        };
        (self.specs.iter())
            .filter(|spec| self.touched(spec.id()).iter().any(|t| ties(t)))
            .collect()
    }

    /// The option `id`, labelled for the player.
    pub(crate) fn label(&self, id: &str) -> Option<&str> {
        self.offered(id).map(OptionSpec::label)
    }

    fn offered(&self, id: &str) -> Option<&OptionSpec> {
        self.specs.iter().find(|s| s.id() == id)
    }

    /// Each action of a stored choice with the side it applies (true for on), or `None` when the
    /// choice is not one these options take.
    fn sides(&self, choice: &OptionChange) -> Option<Vec<(&OptionAction, bool)>> {
        let spec = self
            .offered(choice.id())
            .filter(|s| source_of(s) == choice.source())?;
        if !value_suits(spec.kind(), choice.value()) {
            return None;
        }
        match (spec.kind(), choice.value()) {
            // Every other choice turns off first, then the picked one turns on.
            (OptionKind::Choice(choices), OptionValue::Pick(key)) => {
                let off = (choices.iter().filter(|c| c.key() != key))
                    .flat_map(|c| c.actions().iter().map(|a| (a, false)));
                let on = (choices.iter().filter(|c| c.key() == key))
                    .flat_map(|c| c.actions().iter().map(|a| (a, true)));
                let own = spec.actions().iter().map(|a| (a, true));
                Some(off.chain(on).chain(own).collect())
            }
            (_, OptionValue::Bool(on)) => Some(spec.actions().iter().map(|a| (a, *on)).collect()),
            _ => Some(spec.actions().iter().map(|a| (a, true)).collect()),
        }
    }
}

/// The source string stored for a spec's choices.
fn source_of(spec: &OptionSpec) -> &'static str {
    match spec.source() {
        OptionSource::Detected => OptionChange::HUD,
        OptionSource::BuiltIn => OptionChange::BUILTIN,
        OptionSource::HudSchema { .. } => OptionChange::SCHEMA,
    }
}

/// Lowercase paths `spec` touches, sorted.
fn spec_touched(spec: &OptionSpec) -> Vec<String> {
    let mut paths = Vec::new();
    for action in all_actions(spec) {
        match action {
            OptionAction::SetKey { file, .. } => paths.extend(check_switch_path(file)),
            OptionAction::Switch(SwitchAction::CommentLine { file, target }) => {
                paths.extend([file.clone(), target.clone()]);
            }
            OptionAction::Switch(SwitchAction::Move { from, to }) => {
                paths.extend([from.clone(), to.clone()]);
            }
            OptionAction::Switch(SwitchAction::BaseLines { file, on, off }) => {
                paths.push(file.clone());
                paths.extend(on.iter().chain(off).filter_map(|t| resolve(file, t)));
            }
            OptionAction::Switch(
                SwitchAction::Lines { file, .. } | SwitchAction::WriteText { file, .. },
            ) => paths.extend(check_switch_path(file)),
            OptionAction::Switch(SwitchAction::CopyFolder { from, .. }) => {
                paths.extend(check_switch_path(from));
            }
            OptionAction::Switch(SwitchAction::StockBackgrounds { folder }) => {
                paths.extend(check_switch_path(folder));
            }
            OptionAction::Switch(SwitchAction::MenuBackground { .. }) => {
                paths.extend([MENU_ART.to_owned(), CHAPTERS.to_owned()]);
            }
            OptionAction::WriteCfg {
                place: CfgPlace::Hud(file),
                ..
            } => paths.push(file.clone()),
            // A file under `tf/cfg` is no path of the copy.
            OptionAction::WriteCfg { .. } => {}
        }
    }
    paths.sort();
    paths.dedup();
    paths
}

/// The values `specs` start from in the copy, read at each one's first key edit: a colour
/// from the merged scheme, any other kind from the line in force.
fn pristine_values(
    files: &HudFiles,
    specs: &[OptionSpec],
    platform: Platform,
) -> BTreeMap<String, OptionValue> {
    let mut scheme = None;
    let mut out = BTreeMap::new();
    for spec in specs {
        if let Some(found) = lines_value(files, spec) {
            out.insert(spec.id().to_owned(), found);
            continue;
        }
        let Some(OptionAction::SetKey {
            file,
            block,
            key,
            cond,
            value,
        }) = spec.actions().first()
        else {
            continue;
        };
        let found = match spec.kind() {
            OptionKind::Colour { .. } => {
                let scheme = scheme.get_or_insert_with(|| Scheme::load(files, platform).ok());
                (scheme.as_ref().and_then(|s| s.named_colour(key)))
                    .map(|c| OptionValue::Rgba(c.to_array()))
            }
            kind => line_in_force(files, file, block, key, cond.as_deref(), platform)
                .and_then(|text| read_as(kind, value, text.trim())),
        };
        if let Some(found) = found {
            out.insert(spec.id().to_owned(), found);
        }
    }
    out
}

/// The value a switching option has in the copy: a switch whose line edits all stand one
/// way, or the first choice whose switches (lines, menu art) all stand as it leaves them.
/// Options that also set keys are read from their keys instead.
fn lines_value(files: &HudFiles, spec: &OptionSpec) -> Option<OptionValue> {
    let state = |actions: &[OptionAction]| -> Option<bool> {
        let mut out = None;
        for action in actions {
            let read = |path: &str| files.read_pristine(path).ok().flatten();
            let now = match action {
                OptionAction::Switch(SwitchAction::Lines {
                    file,
                    comment,
                    uncomment,
                    ..
                }) => {
                    let text = String::from_utf8(read(file)?.into_bytes()).ok()?;
                    lines_state(&text, comment, uncomment)?
                }
                // A shipped background is in force when the menu art holds its bytes.
                OptionAction::Switch(SwitchAction::MenuBackground { image }) => {
                    let art = read(MENU_ART)?;
                    art.bytes() == read(&format!("{SHIPPED_ART}/{image}.vtf"))?.bytes()
                }
                OptionAction::Switch(SwitchAction::StockBackgrounds { folder }) => {
                    menu_art(files.copy_paths(), folder).is_empty()
                }
                _ => return None,
            };
            if out.is_some_and(|o| o != now) {
                return None;
            }
            out = Some(now);
        }
        out
    };
    match spec.kind() {
        OptionKind::Toggle => state(spec.actions()).map(OptionValue::Bool),
        OptionKind::Choice(choices) if spec.actions().is_empty() => (choices.iter())
            .find(|c| state(c.actions()) == Some(true))
            .map(|c| OptionValue::Pick(c.key().to_owned())),
        _ => None,
    }
}

/// The text of `key` (same conditional, any case) in the last live block on `block` under
/// the root of copy file `file` that has it.
fn line_in_force(
    files: &HudFiles,
    file: &str,
    block: &[String],
    key: &str,
    cond: Option<&str>,
    platform: Platform,
) -> Option<String> {
    let bytes = files.read_pristine(&check_switch_path(file)?).ok()??;
    let doc = KvDocument::parse(bytes.bytes()).ok()?;
    let root = root_block(&doc)?;
    let path: Vec<&str> = std::iter::once(root.key())
        .chain(block.iter().map(String::as_str))
        .collect();
    let same_cond = |c: Option<&str>| match (c, cond) {
        (Some(a), Some(b)) => a.eq_ignore_ascii_case(b),
        (a, b) => a == b,
    };
    (live_blocks(&doc, &path, platform).into_iter().rev())
        .find_map(|b| {
            b.children().iter().find(|e| {
                e.text().is_some() && e.key().eq_ignore_ascii_case(key) && same_cond(e.cond())
            })
        })
        .and_then(|e| e.text())
        .map(str::to_owned)
}

/// A key's text read as an option of `kind` whose first edit writes `map`.
fn read_as(kind: &OptionKind, map: &ValueMap, text: &str) -> Option<OptionValue> {
    match (map, kind) {
        (ValueMap::Value, OptionKind::Number { .. }) => {
            Some(OptionValue::Int(atoi(text.as_bytes())))
        }
        (ValueMap::OnOff { on, .. }, OptionKind::Toggle) => Some(OptionValue::Bool(text == on)),
        (ValueMap::Value, OptionKind::Choice(choices)) => (choices.iter())
            .find(|c| c.key() == text)
            .map(|c| OptionValue::Pick(c.key().to_owned())),
        (ValueMap::PerChoice(pairs), OptionKind::Choice(_)) => (pairs.iter())
            .find(|(_, literal)| literal == text)
            .map(|(key, _)| OptionValue::Pick(key.clone())),
        (ValueMap::Value, OptionKind::Text) => {
            text_suits(text).then(|| OptionValue::Text(text.to_owned()))
        }
        (ValueMap::Template(template), OptionKind::Text | OptionKind::Number { .. }) => {
            let value = unfill(template, text)?;
            match kind {
                OptionKind::Text => text_suits(value).then(|| OptionValue::Text(value.to_owned())),
                _ => value.parse().ok().map(OptionValue::Int),
            }
        }
        _ => None,
    }
}

/// The value `text` holds where `template` has its one `$value`; `None` when the text does
/// not fit the template's other words.
fn unfill<'a>(template: &str, text: &'a str) -> Option<&'a str> {
    let (before, after) = template.split_once("$value")?;
    if after.contains("$value") {
        return None;
    }
    text.strip_prefix(before)?.strip_suffix(after)
}

/// A spec's own actions and its choices'.
fn all_actions(spec: &OptionSpec) -> impl Iterator<Item = &OptionAction> {
    let choices = match spec.kind() {
        OptionKind::Choice(choices) => choices.as_slice(),
        _ => &[],
    };
    (spec.actions().iter()).chain(choices.iter().flat_map(|c| c.actions()))
}

/// `changes` without the HUD and editor choices `(source, id)` that `wanted` passes.
fn without(changes: &[Change], wanted: impl Fn(&str, &str) -> bool) -> Vec<Change> {
    (changes.iter())
        .filter(|change| match change.target() {
            ChangeTarget::Option(o) => !wanted(o.source(), o.id()),
            _ => true,
        })
        .cloned()
        .collect()
}

/// The copy's `.res` and `.txt` files as text, in path order, within the scan caps, and how
/// many the caps left unread; files that fail the switch path rule or are no UTF-8 are skipped.
pub(super) fn scan(files: &HudFiles, copy: &BTreeSet<String>) -> (Vec<(String, String)>, usize) {
    let mut out = Vec::new();
    let mut bytes = 0usize;
    let readable: Vec<&String> = (copy.iter())
        .filter(|p| p.ends_with(".res") || p.ends_with(".txt"))
        .filter(|p| check_switch_path(p).is_some())
        .collect();
    for (n, path) in readable.iter().enumerate() {
        // The file at the cap and every one after it are refused and counted.
        if n >= MAX_SCAN_FILES {
            return (out, readable.len() - n);
        }
        let Ok(Some(file)) = files.read_pristine(path) else {
            continue;
        };
        bytes = bytes.saturating_add(file.bytes().len());
        if bytes > MAX_SCAN_BYTES {
            return (out, readable.len() - n);
        }
        if let Ok(text) = String::from_utf8(file.into_bytes()) {
            out.push(((*path).clone(), text));
        }
    }
    (out, 0)
}

/// A `#base` target `rel` named in `file`, resolved to a copy path; `None` when it leaves the
/// copy or fails the switch path rule.
fn resolve(file: &str, rel: &str) -> Option<String> {
    match file.rsplit_once('/') {
        Some((dir, _)) => check_switch_path(&format!("{dir}/{rel}")),
        None => check_switch_path(rel),
    }
}

/// Where `#base` target `rel` of copy file `file` lands in the copy: inside by the copy's own
/// folders, or through `custom/<install folder>/`; `None` when it leaves the copy or fails the
/// switch path rule.
pub(super) fn base_target(files: &HudFiles, file: &str, rel: &str) -> Option<String> {
    let landing = resolve(file, rel).or_else(|| files.copy_target(file, rel))?;
    check_switch_path(&landing)
}

/// The targets of `text`'s active `#base` lines, as written.
pub(super) fn active_bases(text: &str) -> Vec<String> {
    (text.lines())
        .filter_map(|line| match classify(line, false) {
            Line::Switch {
                commented: false,
                target,
                ..
            } => Some(target.to_owned()),
            _ => None,
        })
        .collect()
}

/// The targets of `text` when it is nothing but active `#base` lines (quoted or bare) and
/// blank lines; `None` for any other line.
pub(super) fn base_targets(text: &str) -> Option<Vec<String>> {
    let mut targets = Vec::new();
    for line in text.lines() {
        match classify(line, false) {
            Line::Blank => {}
            Line::Switch {
                commented: false,
                target,
                trailing: None,
            } => targets.push(target.to_owned()),
            _ => return None,
        }
    }
    Some(targets)
}

/// One line of a switch file.
enum Line<'a> {
    Blank,
    /// A `//` comment that is no switch line, as its words.
    Comment(&'a str),
    /// A `#base` (or manifest `file`) line: commented or not, its target as written, and its
    /// trailing comment.
    Switch {
        commented: bool,
        target: &'a str,
        trailing: Option<&'a str>,
    },
    Other,
}

/// The line's kind; `manifest` reads `"file" "<path>"` lines instead of `#base` ones.
fn classify(line: &str, manifest: bool) -> Line<'_> {
    let text = line.trim();
    if text.is_empty() {
        return Line::Blank;
    }
    let commented = text.starts_with("//");
    let body = text.trim_start_matches('/').trim_start();
    let rest = if manifest {
        let lower = body.get(..6).map(str::to_ascii_lowercase);
        (lower.as_deref() == Some("\"file\"")).then(|| &body[6..])
    } else {
        let lower = body.get(..5).map(str::to_ascii_lowercase);
        (lower.as_deref() == Some("#base"))
            .then(|| &body[5..])
            .filter(|r| r.starts_with([' ', '\t', '"']))
    };
    let Some(rest) = rest else {
        return match commented {
            true => Line::Comment(body),
            false => Line::Other,
        };
    };
    let Some((target, after)) = token(rest) else {
        return Line::Other;
    };
    let trailing = after
        .find("//")
        .map(|at| clean_label(&after[at..]))
        .filter(|t| !t.is_empty());
    Line::Switch {
        commented,
        target,
        trailing,
    }
}

/// The first quoted or bare token of `text` and what follows it.
fn token(text: &str) -> Option<(&str, &str)> {
    let text = text.trim_start();
    if let Some(quoted) = text.strip_prefix('"') {
        let end = quoted.find('"')?;
        return Some((&quoted[..end], &quoted[end + 1..]));
    }
    let end = text.find(char::is_whitespace).unwrap_or(text.len());
    (end > 0).then(|| (&text[..end], &text[end..]))
}

/// A comment's words: trimmed of `/`, `*`, `=` and spaces.
fn clean_label(text: &str) -> &str {
    text.trim_matches(|c: char| matches!(c, '/' | '*' | '=') || c.is_whitespace())
}

/// A label as shown: an all-caps comment in sentence case, anything else as written.
fn label_of(text: &str) -> String {
    let text = clean_label(text);
    let shouting = text.chars().any(char::is_uppercase) && !text.chars().any(char::is_lowercase);
    match shouting {
        true => sentence(text),
        false => text.to_owned(),
    }
}

/// A path's last segment without its extension, `_` and `-` as spaces, in sentence case.
fn stem_label(path: &str) -> String {
    let name = path.rsplit('/').next().unwrap_or(path);
    let stem = name.rsplit_once('.').map_or(name, |(stem, _)| stem);
    sentence(stem.replace(['_', '-'], " ").trim())
}

/// `text` lowercased with its first letter capitalised.
fn sentence(text: &str) -> String {
    let lower = text.to_lowercase();
    let mut chars = lower.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

/// Whether a folder name is a customizations folder (`#?customi[sz]ations?`, any case).
fn is_customization(segment: &str) -> bool {
    let lower = segment.to_ascii_lowercase();
    let name = lower.strip_prefix('#').unwrap_or(&lower);
    let rest = (name.strip_prefix("customi"))
        .and_then(|r| r.strip_prefix(['s', 'z']))
        .and_then(|r| r.strip_prefix("ation"));
    matches!(rest, Some("" | "s"))
}

/// Joins a folder and a name, an empty folder giving the name.
fn join(dir: &str, name: &str) -> String {
    match dir.is_empty() {
        true => name.to_owned(),
        false => format!("{dir}/{name}"),
    }
}

/// Whether an option touching `paths` needs TF2 restarted to show.
fn needs_restart<'a>(paths: impl IntoIterator<Item = &'a String>) -> bool {
    paths.into_iter().any(|p| {
        p.rsplit('/')
            .next()
            .is_some_and(|name| name.contains("scheme"))
            || p.starts_with("resource/fonts/")
    })
}

/// The options found so far over one copy.
struct Detector {
    copy: BTreeSet<String>,
    specs: Vec<OptionSpec>,
    refused: usize,
}

impl Detector {
    /// Whether the copy holds `path` as a file or a folder.
    fn has(&self, path: &str) -> bool {
        self.copy.contains(path) || self.has_folder(path)
    }

    fn has_folder(&self, path: &str) -> bool {
        let folder = format!("{path}/");
        (self.copy.range(folder.clone()..).next()).is_some_and(|p| p.starts_with(&folder))
    }

    /// Adds `spec` unless its id is taken; past the cap it is refused and counted.
    fn add(&mut self, spec: OptionSpec) {
        if self.specs.iter().any(|s| s.id() == spec.id()) {
            return;
        }
        if self.specs.len() >= MAX_OPTIONS {
            self.refused += 1;
            return;
        }
        self.specs.push(spec);
    }

    fn toggle(&mut self, id: &str, label: &str, group: &str, on: bool, action: SwitchAction) {
        let restart = needs_restart(&switch_paths(&action));
        self.add(OptionSpec::hud(
            id,
            label,
            group,
            OptionKind::Toggle,
            OptionValue::Bool(on),
            vec![OptionAction::Switch(action)],
            restart,
            OptionSource::Detected,
        ));
    }

    fn read_file(&mut self, path: &str, text: &str) {
        let dir = path.rsplit_once('/').map_or("", |(dir, _)| dir);
        if dir.split('/').any(is_customization) {
            self.read_groups(path, text);
        }
        if path == MANIFEST {
            self.read_manifest(text);
        }
        for line in text.lines() {
            if let Line::Switch {
                commented: false,
                target,
                ..
            } = classify(line, false)
                && let Some(target) = resolve(path, target)
            {
                self.read_target(&target);
            }
        }
    }

    /// F: runs of `#base` lines in a customizations file, one-of when the comments above say so.
    fn read_groups(&mut self, path: &str, text: &str) {
        let mut comments: Vec<&str> = Vec::new();
        let mut run: Vec<(bool, String, Option<&str>)> = Vec::new();
        let mut n = 0;
        let lines: Vec<&str> = text.lines().collect();
        for line in lines.iter().copied().chain([""]) {
            match classify(line, false) {
                Line::Switch {
                    commented,
                    target,
                    trailing,
                } => {
                    // A target that leaves the copy or is missing from it is no switch.
                    if let Some(target) = resolve(path, target).filter(|t| self.copy.contains(t)) {
                        run.push((!commented, target, trailing));
                    }
                    continue;
                }
                Line::Comment(words) => {
                    if !run.is_empty() {
                        self.add_group(path, n, &comments, &run);
                        n += 1;
                        comments.clear();
                        run.clear();
                    }
                    comments.push(words);
                    continue;
                }
                Line::Blank | Line::Other => {}
            }
            if !run.is_empty() {
                self.add_group(path, n, &comments, &run);
                n += 1;
                comments.clear();
                run.clear();
            }
        }
    }

    fn add_group(
        &mut self,
        path: &str,
        n: usize,
        comments: &[&str],
        run: &[(bool, String, Option<&str>)],
    ) {
        let group = stem_label(path);
        let one_of = comments.iter().any(|c| {
            let c = c.to_lowercase();
            c.contains("only one") || c.contains("only 1")
        });
        let label = |target: &str, trailing: Option<&str>| match trailing {
            Some(t) => label_of(t),
            None => stem_label(target),
        };
        let action = |target: &str| SwitchAction::CommentLine {
            file: path.to_owned(),
            target: target.to_owned(),
        };
        if !one_of {
            for (on, target, trailing) in run {
                let id = format!("f:{path}:{target}");
                self.toggle(&id, &label(target, *trailing), &group, *on, action(target));
            }
            return;
        }
        let title = (comments.iter())
            .find(|c| c.chars().any(char::is_alphabetic))
            .map_or_else(|| group.clone(), |c| label_of(c));
        let mut choices = Vec::new();
        let active = run.iter().find(|(on, ..)| *on).map(|(_, t, _)| t.clone());
        if active.is_none() {
            choices.push(Choice::new("", "None", Vec::new()));
        }
        let mut paths = vec![path.to_owned()];
        for (_, target, trailing) in run {
            if choices.iter().any(|c| c.key() == target) {
                continue;
            }
            let actions = vec![OptionAction::Switch(action(target))];
            choices.push(Choice::new(target, &label(target, *trailing), actions));
            paths.push(target.clone());
        }
        self.add(OptionSpec::hud(
            &format!("f:{path}:{n}"),
            &title,
            &group,
            OptionKind::Choice(choices),
            OptionValue::Pick(active.unwrap_or_default()),
            Vec::new(),
            needs_restart(&paths),
            OptionSource::Detected,
        ));
    }

    /// Commented `file` lines of the animation manifest whose file exists.
    fn read_manifest(&mut self, text: &str) {
        for line in text.lines() {
            let Line::Switch {
                commented: true,
                target,
                ..
            } = classify(line, true)
            else {
                continue;
            };
            let Some(target) = check_switch_path(target).filter(|t| self.copy.contains(t)) else {
                continue;
            };
            let action = SwitchAction::CommentLine {
                file: MANIFEST.to_owned(),
                target: target.clone(),
            };
            self.toggle(
                &format!("m:{target}"),
                &stem_label(&target),
                SWITCHES,
                false,
                action,
            );
        }
    }

    /// B, B' and C: an active `#base` target that an enabled slot, a disabled folder or a
    /// renamed folder would supply.
    fn read_target(&mut self, target: &str) {
        let segments: Vec<&str> = target.split('/').collect();
        // B: `<dir>/_enabled/<option>`, the option shipped in the slot (on) or beside it (off).
        if let Some(i) = (segments.iter()).position(|s| matches!(*s, "_enabled" | "#enabled"))
            && i + 1 < segments.len()
        {
            let dir = segments[..i].join("/");
            let option = segments[i + 1];
            let from = join(&dir, option);
            let to = join(&join(&dir, segments[i]), option);
            let on = self.has(&to);
            if check_switch_path(&from).is_some()
                && check_switch_path(&to).is_some()
                && on != self.has(&from)
            {
                let action = SwitchAction::Move {
                    from: from.clone(),
                    to,
                };
                self.toggle(
                    &format!("b:{from}"),
                    &stem_label(option),
                    SWITCHES,
                    on,
                    action,
                );
                return;
            }
        }
        if self.copy.contains(target) {
            return;
        }
        for i in 0..segments.len().saturating_sub(1) {
            let dir = segments[..i].join("/");
            let option = segments[i];
            let to = join(&dir, option);
            if self.has(&to) {
                continue;
            }
            let disabled = join(&dir, "_disabled");
            // B': `<dir>/_disabled/<option>` moves up to `<dir>/<option>`.
            let from = join(&disabled, option);
            if self.has(&from) && check_switch_path(&from).is_some() {
                let action = SwitchAction::Move {
                    from: from.clone(),
                    to,
                };
                self.toggle(
                    &format!("b:{from}"),
                    &stem_label(option),
                    SWITCHES,
                    false,
                    action,
                );
                return;
            }
            // B' one-of: `<dir>/_disabled/<! group>/<option>`.
            if let Some(group) = self.bang_group(&disabled, option) {
                self.add_bang_choice(&disabled, &group, option, &to);
                return;
            }
            // C: a folder shipped as `s_`, `_s` or `_s_` renames to `s`.
            for renamed in [
                format!("{option}_"),
                format!("_{option}"),
                format!("_{option}_"),
            ] {
                let from = join(&dir, &renamed);
                if self.has_folder(&from)
                    && check_switch_path(&from).is_some()
                    && check_switch_path(&to).is_some()
                {
                    let action = SwitchAction::Move {
                        from: from.clone(),
                        to,
                    };
                    self.toggle(
                        &format!("c:{from}"),
                        &stem_label(option),
                        SWITCHES,
                        false,
                        action,
                    );
                    return;
                }
            }
        }
    }

    /// The `!` group folder under `disabled` that holds `option`, if any.
    fn bang_group(&self, disabled: &str, option: &str) -> Option<String> {
        let prefix = format!("{disabled}/");
        (self.copy.range(prefix.clone()..))
            .take_while(|p| p.starts_with(&prefix))
            .filter_map(|p| {
                let mut rest = p[prefix.len()..].split('/');
                let group = rest.next()?;
                (group.starts_with('!') && rest.next() == Some(option)).then(|| group.to_owned())
            })
            .next()
    }

    /// Adds `option` as a choice of the `!` group's one-of option, creating it on first sight.
    fn add_bang_choice(&mut self, disabled: &str, group: &str, option: &str, to: &str) {
        let folder = join(disabled, group);
        let from = join(&folder, option);
        if check_switch_path(&from).is_none() || check_switch_path(to).is_none() {
            return;
        }
        let id = format!("b:{folder}");
        let action = OptionAction::Switch(SwitchAction::Move {
            from: from.clone(),
            to: to.to_owned(),
        });
        let choice = Choice::new(&from, &stem_label(option), vec![action]);
        let restart = needs_restart(&[from.clone(), to.to_owned()]);
        if let Some(spec) = self.specs.iter_mut().find(|s| s.id() == id) {
            spec.add_choice(choice, restart);
            return;
        }
        let label = sentence(group.trim_start_matches('!').trim());
        let none = Choice::new("", "None", Vec::new());
        self.add(OptionSpec::hud(
            &id,
            &label,
            SWITCHES,
            OptionKind::Choice(vec![none, choice]),
            OptionValue::Pick(String::new()),
            Vec::new(),
            restart,
            OptionSource::Detected,
        ));
    }
}

/// The paths a switch action names.
fn switch_paths(action: &SwitchAction) -> Vec<String> {
    match action {
        SwitchAction::CommentLine { file, target } => vec![file.clone(), target.clone()],
        SwitchAction::Move { from, to } => vec![from.clone(), to.clone()],
        SwitchAction::BaseLines { file, .. }
        | SwitchAction::Lines { file, .. }
        | SwitchAction::WriteText { file, .. } => vec![file.clone()],
        SwitchAction::CopyFolder { from, to } => vec![from.clone(), to.clone()],
        SwitchAction::StockBackgrounds { folder } => vec![folder.clone()],
        SwitchAction::MenuBackground { .. } => vec![MENU_ART.to_owned(), CHAPTERS.to_owned()],
    }
}

/// One side of a stored choice being applied: on or off, its value, the derived colours it
/// writes and how to read a switch its texts name.
#[derive(Clone, Copy)]
struct Applying<'a> {
    on: bool,
    value: &'a OptionValue,
    derived: &'a [Derived],
    is_on: &'a dyn Fn(&str) -> bool,
}

/// The switch options `spec`'s texts name through `{$name ? a : b}`, by schema name.
fn spec_refs(spec: &OptionSpec) -> Vec<String> {
    (all_actions(spec))
        .filter_map(|a| match a {
            OptionAction::SetKey {
                value: ValueMap::Template(t),
                ..
            } => template_refs(t),
            _ => None,
        })
        .flatten()
        .collect()
}

/// The switch layer being built over a copy's pristine files.
struct Layer<'a> {
    files: &'a HudFiles,
    copy: BTreeSet<String>,
    written: BTreeMap<String, Vec<u8>>,
    removed: BTreeSet<String>,
    /// Settings files under `tf/cfg`, by lowercase path.
    cfg: BTreeMap<String, CfgFile>,
}

impl Layer<'_> {
    fn exists(&self, path: &str) -> bool {
        self.written.contains_key(path)
            || (self.copy.contains(path) && !self.removed.contains(path))
    }

    /// The files under folder `dir` in the current view.
    fn under(&self, dir: &str) -> Vec<String> {
        let folder = format!("{dir}/");
        let mut paths: Vec<String> = (self.copy.iter())
            .chain(self.written.keys())
            .filter(|p| p.starts_with(&folder) && self.exists(p))
            .cloned()
            .collect();
        paths.sort();
        paths.dedup();
        paths
    }

    fn read(&self, path: &str) -> Result<Option<Vec<u8>>, HudFilesError> {
        if let Some(bytes) = self.written.get(path) {
            return Ok(Some(bytes.clone()));
        }
        if !self.exists(path) {
            return Ok(None);
        }
        Ok(self.files.read_pristine(path)?.map(|f| f.into_bytes()))
    }

    fn put(&mut self, path: &str, bytes: Vec<u8>) {
        self.removed.remove(path);
        self.written.insert(path.to_owned(), bytes);
    }

    fn take(&mut self, path: &str) {
        self.written.remove(path);
        if self.copy.contains(path) {
            self.removed.insert(path.to_owned());
        }
    }

    fn within_caps(&self) -> bool {
        let cfg = self.cfg.values().map(|f| f.bytes());
        let bytes = (self.written.values().map(Vec::as_slice).chain(cfg))
            .fold(0usize, |n, b| n.saturating_add(b.len()));
        self.written.len() + self.cfg.len() <= MAX_LAYER_FILES && bytes <= MAX_LAYER_BYTES
    }

    /// Applies one side of `action`; false when it is refused (a missing file or source, a
    /// taken destination, a line that is not there).
    fn apply(
        &mut self,
        action: &OptionAction,
        ctx: &Applying<'_>,
        platform: Platform,
    ) -> Result<bool, HudFilesError> {
        let Applying {
            on,
            value,
            derived,
            is_on,
        } = *ctx;
        match action {
            OptionAction::Switch(SwitchAction::CommentLine { file, target }) => {
                let Some(text) = self.read_text(file)? else {
                    return Ok(false);
                };
                let Some(edited) = comment_lines(&text, file, target, on) else {
                    return Ok(false);
                };
                self.put(file, edited.into_bytes());
                Ok(true)
            }
            OptionAction::Switch(SwitchAction::BaseLines {
                file,
                on: a,
                off: b,
            }) => {
                let Some(text) = self.read_text(file)? else {
                    return Ok(false);
                };
                let targets = if on { a } else { b };
                if targets
                    .iter()
                    .any(|t| base_target(self.files, file, t).is_none())
                {
                    return Ok(false);
                }
                let Some(edited) = base_lines(&text, targets) else {
                    return Ok(false);
                };
                self.put(file, edited.into_bytes());
                Ok(true)
            }
            OptionAction::Switch(SwitchAction::Move { from, to }) => {
                let (source, dest) = if on { (from, to) } else { (to, from) };
                self.move_paths(source, dest)
            }
            OptionAction::Switch(SwitchAction::Lines {
                file,
                comment,
                uncomment,
                reverses,
            }) => {
                if !on && !reverses {
                    return Ok(true);
                }
                let Some(text) = self.read_text(file)? else {
                    return Ok(false);
                };
                let (comment, uncomment) = if on {
                    (comment, uncomment)
                } else {
                    (uncomment, comment)
                };
                let Some(edited) = toggle_lines(&text, comment, uncomment) else {
                    return Ok(false);
                };
                self.put(file, edited.into_bytes());
                Ok(true)
            }
            OptionAction::Switch(SwitchAction::CopyFolder { from, to }) => {
                if !on {
                    return Ok(true);
                }
                self.copy_folder(from, to)
            }
            OptionAction::Switch(SwitchAction::StockBackgrounds { folder }) => {
                if on {
                    for path in menu_art(self.under(folder), folder) {
                        self.take(&path);
                    }
                }
                Ok(true)
            }
            OptionAction::Switch(SwitchAction::MenuBackground { image }) => {
                if !on {
                    return Ok(true);
                }
                let Some(art) = self.read(&format!("{SHIPPED_ART}/{image}.vtf"))? else {
                    return Ok(false);
                };
                self.put(MENU_ART, art);
                if let Some(wide) = self.read(&format!("{SHIPPED_ART}/{image}_widescreen.vtf"))? {
                    self.put(MENU_ART_WIDE, wide);
                }
                self.put(CHAPTERS, CHAPTERS_TEXT.as_bytes().to_vec());
                Ok(true)
            }
            // An empty text for a file the copy lacks writes no file, so off leaves no stub.
            OptionAction::Switch(SwitchAction::WriteText {
                file,
                on: a,
                off: b,
            }) => {
                let Some(text) = (if on { Some(a) } else { b.as_ref() }) else {
                    return Ok(true);
                };
                let Some(path) = check_switch_path(file) else {
                    return Ok(false);
                };
                match text.is_empty() && !self.copy.contains(&path) {
                    true => drop(self.written.remove(&path)),
                    false => self.put(&path, text.clone().into_bytes()),
                }
                Ok(true)
            }
            // An empty side writes no file: one this layer wrote goes, and a copy file is
            // left as the HUD ships it.
            OptionAction::WriteCfg {
                place,
                on: a,
                off: b,
            } => {
                let text = if on { a } else { b };
                match place {
                    CfgPlace::Game(path) => {
                        let Some(file) = CfgFile::new(path, text.clone().into_bytes()) else {
                            return Ok(false);
                        };
                        match text.is_empty() {
                            true => self.cfg.remove(file.path()),
                            false => self.cfg.insert(file.path().to_owned(), file),
                        };
                    }
                    CfgPlace::Hud(path) => {
                        let Some(path) = check_switch_path(path) else {
                            return Ok(false);
                        };
                        match text.is_empty() {
                            true => drop(self.written.remove(&path)),
                            false => self.put(&path, text.clone().into_bytes()),
                        }
                    }
                }
                Ok(true)
            }
            OptionAction::SetKey {
                file,
                block,
                key,
                cond,
                value: map,
            } => {
                let text = match (map, value) {
                    (ValueMap::OnOff { on: text, .. }, _) if on => Some(text.clone()),
                    (ValueMap::OnOff { off, .. }, _) => off.clone(),
                    (ValueMap::Value, OptionValue::Int(v)) => Some(v.to_string()),
                    (ValueMap::Value, OptionValue::Rgba(c)) => Some(rgba_text(*c)),
                    (ValueMap::Value, OptionValue::Text(t)) => Some(t.clone()),
                    (ValueMap::Value, OptionValue::Pick(k)) => Some(k.clone()),
                    (ValueMap::Template(t), v) => template_text(t, v, is_on),
                    // A pick with no text of its own leaves the key as it is.
                    (ValueMap::PerChoice(pairs), OptionValue::Pick(k)) => {
                        (pairs.iter()).find(|(c, _)| c == k).map(|(_, t)| t.clone())
                    }
                    _ => return Ok(false),
                };
                // An off side that writes nothing leaves the file as it is.
                let Some(text) = text else {
                    return Ok(true);
                };
                let Some(file) = check_switch_path(file) else {
                    return Ok(false);
                };
                let Some(bytes) = self.read(&file)? else {
                    return Ok(false);
                };
                let Ok(doc) = KvDocument::parse(&bytes) else {
                    return Ok(false);
                };
                let Some(root) = root_block(&doc).map(|r| r.key().to_owned()) else {
                    return Ok(false);
                };
                let path: Vec<&str> = std::iter::once(root.as_str())
                    .chain(block.iter().map(String::as_str))
                    .collect();
                let mut sets = vec![SetKey::new(&file, &path, key, cond.as_deref(), None, &text)];
                // Derived colours go only where the switched scheme already names them.
                if let (false, OptionValue::Rgba(c)) = (derived.is_empty(), value) {
                    let view = (self.files).with_switches(
                        self.written.clone().into_iter().collect(),
                        self.removed.iter().cloned().collect(),
                    );
                    let scheme = Scheme::load(&view, platform)?;
                    for d in derived {
                        let (name, colour) = derived_colour(*d, key, *c);
                        if scheme.named_colour(&name).is_some() {
                            let text = rgba_text(colour);
                            sets.push(SetKey::new(
                                &file,
                                &path,
                                &name,
                                cond.as_deref(),
                                None,
                                &text,
                            ));
                        }
                    }
                }
                let mut docs = BTreeMap::from([(file.clone(), doc)]);
                for set in &sets {
                    if apply_set(&mut docs, &BaseChains::default(), set, platform).is_none() {
                        return Ok(false);
                    }
                }
                let bytes = docs.remove(&file).map(|d| d.as_bytes().to_vec());
                self.put(&file, bytes.unwrap_or_default());
                Ok(true)
            }
        }
    }

    fn read_text(&self, path: &str) -> Result<Option<String>, HudFilesError> {
        Ok(self.read(path)?.and_then(|b| String::from_utf8(b).ok()))
    }

    /// Copies every file under folder `from` into folder `to` by its path below `from`,
    /// leaving out the preview pictures option folders carry; refused when a landing fails the
    /// path rule or the folder holds nothing.
    fn copy_folder(&mut self, from: &str, to: &str) -> Result<bool, HudFilesError> {
        let files = self.under(from);
        let mut copies = Vec::new();
        for path in &files {
            if path.ends_with(".png") || path.ends_with(".jpg") {
                continue;
            }
            let rest = &path[from.len() + 1..];
            let dest = match to.is_empty() {
                true => rest.to_owned(),
                false => format!("{to}/{rest}"),
            };
            let Some(dest) = check_switch_path(&dest) else {
                return Ok(false);
            };
            copies.push((path.clone(), dest));
        }
        if copies.is_empty() {
            return Ok(false);
        }
        for (path, dest) in copies {
            let Some(bytes) = self.read(&path)? else {
                return Ok(false);
            };
            self.put(&dest, bytes);
        }
        Ok(true)
    }

    /// Moves the file or every file under the folder `source` to `dest`. Already in place
    /// (nothing at `source`, something at `dest`) is done; a taken destination is refused.
    fn move_paths(&mut self, source: &str, dest: &str) -> Result<bool, HudFilesError> {
        let moves: Vec<(String, String)> = match self.exists(source) {
            true => vec![(source.to_owned(), dest.to_owned())],
            false => (self.under(source).into_iter())
                .map(|p| {
                    let rest = &p[source.len()..];
                    (p.clone(), format!("{dest}{rest}"))
                })
                .collect(),
        };
        if moves.is_empty() {
            return Ok(self.exists(dest) || !self.under(dest).is_empty());
        }
        if moves.iter().any(|(_, to)| self.exists(to)) {
            return Ok(false);
        }
        for (from, to) in moves {
            let Some(bytes) = self.read(&from)? else {
                return Ok(false);
            };
            self.take(&from);
            self.put(&to, bytes);
        }
        Ok(true)
    }
}

/// `text` with every line of `file` naming `target` commented (off) or uncommented (on), each
/// line's indentation and ending kept; `None` when no line names it.
fn comment_lines(text: &str, file: &str, target: &str, on: bool) -> Option<String> {
    let manifest = file == MANIFEST;
    let mut found = false;
    let mut out = String::with_capacity(text.len() + 8);
    for line in text.split_inclusive('\n') {
        let body = line.trim_end_matches(['\n', '\r']);
        let ending = &line[body.len()..];
        let names = match classify(body, manifest) {
            Line::Switch { target: t, .. } if manifest => check_switch_path(t),
            Line::Switch { target: t, .. } => resolve(file, t),
            _ => None,
        };
        if names.as_deref() != Some(target) {
            out.push_str(line);
            continue;
        }
        found = true;
        let indent = &body[..body.len() - body.trim_start().len()];
        let rest = body.trim_start();
        match (on, rest.starts_with("//")) {
            (true, true) => {
                out.push_str(indent);
                out.push_str(rest.trim_start_matches('/').trim_start_matches([' ', '\t']));
            }
            (false, false) => {
                out.push_str(indent);
                out.push_str("//");
                out.push_str(rest);
            }
            _ => out.push_str(body),
        }
        out.push_str(ending);
    }
    found.then_some(out)
}

/// The menu art among `paths`: `.vtf` and `.vmt` files right in `folder`.
pub(super) fn menu_art(paths: impl IntoIterator<Item = String>, folder: &str) -> Vec<String> {
    let inside = |p: &str| {
        p.strip_prefix(folder)
            .and_then(|r| r.strip_prefix('/'))
            .is_some_and(|name| !name.contains('/'))
    };
    (paths.into_iter())
        .filter(|p| inside(p) && (p.ends_with(".vtf") || p.ends_with(".vmt")))
        .collect()
}

/// Whether `line` holds `needle`, any run of spaces and tabs in either read as one space, as
/// schemas write a tab-separated animation line with spaces.
fn holds(line: &str, needle: &str) -> bool {
    let squeeze = |s: &str| s.split_whitespace().collect::<Vec<_>>().join(" ");
    let needle = squeeze(needle);
    !needle.is_empty() && squeeze(line).contains(&needle)
}

/// Whether a line is commented out with `//`.
fn is_commented(line: &str) -> bool {
    line.trim_start().starts_with("//")
}

/// `text` with every line holding one of `comment` commented out and every line holding one
/// of `uncomment` uncommented, indentation and endings kept; `None` when some needle is on
/// no line, since the copy then differs from what the option was made for.
pub(super) fn toggle_lines(text: &str, comment: &[String], uncomment: &[String]) -> Option<String> {
    let mut found = vec![false; comment.len() + uncomment.len()];
    let mut out = String::with_capacity(text.len() + 16);
    for line in text.split_inclusive('\n') {
        let body = line.trim_end_matches(['\n', '\r']);
        let ending = &line[body.len()..];
        let mut hit = |list: &[String], offset: usize| {
            let mut any = false;
            for (i, needle) in list.iter().enumerate() {
                if holds(body, needle) {
                    found[offset + i] = true;
                    any = true;
                }
            }
            any
        };
        let shut = hit(comment, 0);
        let open = hit(uncomment, comment.len());
        let indent = &body[..body.len() - body.trim_start().len()];
        let rest = body.trim_start();
        match (open, shut, is_commented(body)) {
            (true, _, true) => {
                out.push_str(indent);
                out.push_str(rest.trim_start_matches('/'));
            }
            (false, true, false) => {
                out.push_str(indent);
                out.push_str("//");
                out.push_str(rest);
            }
            _ => out.push_str(body),
        }
        out.push_str(ending);
    }
    found.iter().all(|f| *f).then_some(out)
}

/// Whether `text` stands as [`toggle_lines`] leaves it (`Some(true)`), as its reverse leaves
/// it (`Some(false)`), or neither.
pub(super) fn lines_state(text: &str, comment: &[String], uncomment: &[String]) -> Option<bool> {
    let state = |shut: &[String], open: &[String]| {
        let lines: Vec<&str> = text.lines().collect();
        let all = |list: &[String], commented: bool| {
            list.iter().all(|n| {
                let mut hits = lines.iter().filter(|l| holds(l, n)).peekable();
                hits.peek().is_some() && hits.all(|l| is_commented(l) == commented)
            })
        };
        all(shut, true) && all(open, false)
    };
    match (state(comment, uncomment), state(uncomment, comment)) {
        (true, false) => Some(true),
        (false, true) => Some(false),
        _ => None,
    }
}

/// `text` with its active `#base` lines replaced by one per target, where the first stood (or
/// at the top); `None` when a target cannot be written as one line.
fn base_lines(text: &str, targets: &[String]) -> Option<String> {
    if targets.iter().any(|t| t.contains(['"', '\n', '\r'])) {
        return None;
    }
    let newline = if text.contains("\r\n") { "\r\n" } else { "\n" };
    let new: String = (targets.iter())
        .map(|t| format!("#base \"{t}\"{newline}"))
        .collect();
    let mut out = String::with_capacity(text.len() + new.len());
    let mut placed = false;
    for line in text.split_inclusive('\n') {
        let body = line.trim_end_matches(['\n', '\r']);
        if matches!(
            classify(body, false),
            Line::Switch {
                commented: false,
                ..
            }
        ) {
            if !placed {
                out.push_str(&new);
                placed = true;
            }
            continue;
        }
        out.push_str(line);
    }
    if !placed {
        out.insert_str(0, &new);
    }
    Some(out)
}
