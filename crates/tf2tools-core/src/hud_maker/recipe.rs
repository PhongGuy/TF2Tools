//! Recipe files: a shareable list of HUD changes plus the font bytes they need.
//! A recipe comes from the internet, so reading is strict about the file's shape, lenient
//! per change, bounded in size and count, and puts every path through the path rule.

use std::collections::{BTreeMap, BTreeSet};

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use serde_json::{Map, Value, json};

use super::animations::ConsoleReach;
use super::fonts::{
    CLONE_PREFIX, KEPT_DIR, addable_family, clone_number, is_clone, next_clone_number,
};
use super::maker_file::{change_json, parse_change};
use super::model::SCHEME_FILE;
use super::{
    AddBlock, BlockLine, Change, ChangeTarget, HudModel, RemoveKey, SetKey, check_hud_path,
    is_picture_path,
};
use crate::error::RecipeError;
use crate::library::HudAssets;

/// The only recipe version this build reads and writes.
const VERSION: u64 = 1;
/// More changes than this are skipped and counted, unparsed.
const MAX_CHANGES: usize = 10_000;
/// A carried font larger than this is dropped.
const MAX_ASSET_BYTES: usize = 16 * 1024 * 1024;
/// The longest recipe name kept, in characters.
const MAX_NAME_CHARS: usize = 80;
/// The most fonts one apply adds; each further one is skipped and counted.
const MAX_FONTS: usize = 32;
/// A font file the apply added, as (source, bytes).
type AddedFont = (String, Vec<u8>);
/// The scheme block that lists font files by slot.
const SLOTS: &str = "CustomFontFiles";

/// A recipe: a name, the HUD it was made for, changes and the font bytes they carry.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct Recipe {
    name: String,
    made_for: Option<(String, String)>,
    changes: Vec<Change>,
    assets: BTreeMap<String, Vec<u8>>,
    left_out: usize,
    pictures_left_out: usize,
    parts_left_out_for_pictures: usize,
}

impl Recipe {
    /// Larger files are refused unread.
    pub const MAX_BYTES: usize = 32 * 1024 * 1024;

    /// A recipe of `changes` named `name` (cleaned as when read), each change as the reader
    /// would read it back, carrying the bytes of each `Asset` change's source from `assets`. A
    /// change the reader would refuse (such as one on a path the path rule refuses) is left
    /// out, and so is an `Asset` whose bytes are missing, over 16 MiB, or kept outside
    /// `.tf2tools/fonts/`; a left-out picture takes every change of its group. `fonts_left_out`
    /// counts the `Asset` changes left out.
    pub fn new(name: &str, changes: &[Change], assets: &HudAssets) -> Recipe {
        // Only what the reader would keep is written, so an exported recipe always reads back.
        let fonts = format!("{KEPT_DIR}/");
        let mut carried = BTreeMap::new();
        let mut keep = |change: &Change| {
            let kept = parse_change(&change_json(change)).ok().flatten()?;
            let ChangeTarget::Asset(asset) = kept.target() else {
                return Some(kept);
            };
            let bytes = (assets.get(asset_source(change)?))
                .filter(|bytes| bytes.len() <= MAX_ASSET_BYTES)
                .filter(|_| asset.source().starts_with(&fonts))?;
            carried.insert(asset.source().to_owned(), bytes.to_vec());
            Some(kept)
        };
        let (mut kept, mut lost) = (Vec::new(), Vec::new());
        for change in changes {
            match (keep(change), change.target()) {
                (Some(change), _) => kept.push(change),
                (None, ChangeTarget::Asset(asset)) => lost.push((change.group(), asset.path())),
                (None, _) => {}
            }
        }
        drop_dependents(&mut kept, &lost);
        // A picture left out takes its whole group with it, credit and fonts included, so no
        // key names a missing texture and no credit names a part the recipe does not hold.
        let picture_groups: BTreeSet<u32> = (lost.iter())
            .filter(|(_, path)| check_hud_path(path).is_some_and(|p| is_picture_path(&p)))
            .map(|(group, _)| *group)
            .collect();
        let before = kept.len();
        let mut assets_dropped = 0;
        kept.retain(|c| {
            let stays = !picture_groups.contains(&c.group());
            if !stays && matches!(c.target(), ChangeTarget::Asset(_)) {
                assets_dropped += 1;
            }
            stays
        });
        // Bytes travel only for an asset the recipe still holds.
        if kept.len() != before {
            carried.retain(|source, _| kept.iter().any(|c| asset_source(c) == Some(source)));
        }
        let pictures_left_out = (lost.iter())
            .filter(|(_, path)| check_hud_path(path).is_some_and(|p| is_picture_path(&p)))
            .count();
        Recipe {
            name: clean_name(name),
            made_for: None,
            changes: kept,
            assets: carried,
            left_out: lost.len() + assets_dropped,
            pictures_left_out,
            parts_left_out_for_pictures: picture_groups.len(),
        }
    }

    /// A recipe of only the `Option` changes in `changes`, with no assets.
    pub fn preset(name: &str, changes: &[Change]) -> Recipe {
        let options: Vec<Change> = changes
            .iter()
            .filter(|c| matches!(c.target(), ChangeTarget::Option(_)))
            .cloned()
            .collect();
        Recipe::new(name, &options, &HudAssets::new())
    }

    /// The recipe and how many changes were skipped.
    ///
    /// # Errors
    /// `Damaged` when the bytes are not a recipe file; `Newer` when its version is above 1.
    pub fn parse(bytes: &[u8]) -> Result<(Recipe, usize), RecipeError> {
        if bytes.len() > Self::MAX_BYTES {
            return Err(damaged("over 32 MB"));
        }
        // serde_json's recursion limit turns deep nesting into an error, not a stack overflow.
        let value: Value =
            serde_json::from_slice(bytes).map_err(|e| damaged(&format!("not JSON: {e}")))?;
        let root = value.as_object().ok_or_else(|| damaged("not an object"))?;
        match root.get("version").and_then(Value::as_u64) {
            Some(0) | None => return Err(damaged("no positive version")),
            Some(v) if v > VERSION => return Err(RecipeError::Newer),
            Some(_) => {}
        }
        if root.get("kind").and_then(Value::as_str) != Some("recipe") {
            return Err(damaged("kind is not recipe"));
        }
        let name = root
            .get("name")
            .and_then(Value::as_str)
            .ok_or_else(|| damaged("name is not text"))?;
        let entries = root
            .get("changes")
            .and_then(Value::as_array)
            .ok_or_else(|| damaged("changes is not a list"))?;
        let mut assets = match root.get("assets") {
            None => BTreeMap::new(),
            Some(assets) => read_assets(assets.as_object().ok_or_else(|| damaged("bad assets"))?),
        };

        // Any change this build cannot use is skipped and counted, never fatal; an asset
        // change also needs its bytes in the file.
        let mut changes = Vec::new();
        let mut skipped = entries.len().saturating_sub(MAX_CHANGES);
        for entry in entries.iter().take(MAX_CHANGES) {
            match parse_change(entry) {
                Ok(Some(change)) if has_bytes(&change, &assets) => changes.push(change),
                _ => skipped += 1,
            }
        }
        // Bytes no kept change installs would only be carried along and written out again.
        assets.retain(|source, _| changes.iter().any(|c| asset_source(c) == Some(source)));
        let recipe = Recipe {
            name: clean_name(name),
            made_for: read_made_for(root.get("made_for")),
            changes,
            assets,
            left_out: 0,
            pictures_left_out: 0,
            parts_left_out_for_pictures: 0,
        };
        Ok((recipe, skipped))
    }

    /// Pretty JSON, version 1, assets in standard base64.
    pub fn to_json(&self) -> Vec<u8> {
        let made_for = match &self.made_for {
            Some((repo, hash)) => json!({ "repo": repo, "hash": hash }),
            None => Value::Null,
        };
        let assets: Map<String, Value> = self
            .assets
            .iter()
            .map(|(source, bytes)| (source.clone(), Value::String(STANDARD.encode(bytes))))
            .collect();
        let file = json!({
            "version": VERSION,
            "kind": "recipe",
            "name": self.name,
            "made_for": made_for,
            "changes": self.changes.iter().map(change_json).collect::<Vec<_>>(),
            "assets": assets,
        });
        format!("{file:#}\n").into_bytes()
    }

    /// The cleaned name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The repository and hash of the HUD the recipe was made for, if it says.
    pub fn made_for(&self) -> Option<(&str, &str)> {
        self.made_for
            .as_ref()
            .map(|(repo, hash)| (repo.as_str(), hash.as_str()))
    }

    /// The changes in the order they were made.
    pub fn changes(&self) -> &[Change] {
        &self.changes
    }

    /// The bytes carried for an asset source.
    pub fn asset(&self, source: &str) -> Option<&[u8]> {
        self.assets.get(source).map(Vec::as_slice)
    }

    /// How many fonts `new` left out, for the player to be told; 0 for a read recipe, whose
    /// left-out changes are in `parse`'s count.
    pub fn fonts_left_out(&self) -> usize {
        self.left_out
    }

    /// How many `Asset` changes of pictures `new` left out, for the player to be told; 0 for a
    /// read recipe.
    pub fn pictures_left_out(&self) -> usize {
        self.pictures_left_out
    }

    /// How many change groups (picked pictures or borrowed parts) `new` left out whole because
    /// they hold a picture; 0 for a read recipe.
    pub fn parts_left_out_for_pictures(&self) -> usize {
        self.parts_left_out_for_pictures
    }

    /// True when the recipe holds changes and every one is an `Option`.
    pub fn is_preset(&self) -> bool {
        !self.changes.is_empty()
            && self
                .changes
                .iter()
                .all(|c| matches!(c.target(), ChangeTarget::Option(_)))
    }
}

fn damaged(reason: &str) -> RecipeError {
    RecipeError::Damaged {
        reason: reason.to_owned(),
    }
}

/// Keeps a carried font only when its source is a safe path under the kept fonts folder
/// and its base64 decodes to at most 16 MiB; anything else is dropped silently.
fn read_assets(assets: &Map<String, Value>) -> BTreeMap<String, Vec<u8>> {
    let fonts = format!("{KEPT_DIR}/");
    assets
        .iter()
        .filter_map(|(source, data)| {
            let source = check_hud_path(source).filter(|s| s.starts_with(&fonts))?;
            // Padded base64 of at most 16 MiB is never longer than this, so nothing longer is decoded.
            let data = data
                .as_str()
                .filter(|d| d.len() <= MAX_ASSET_BYTES.div_ceil(3) * 4)?;
            let bytes = STANDARD.decode(data).ok()?;
            (bytes.len() <= MAX_ASSET_BYTES).then_some((source, bytes))
        })
        .collect()
}

/// The kept copy an `Asset` change installs from.
fn asset_source(change: &Change) -> Option<&str> {
    match change.target() {
        ChangeTarget::Asset(asset) => Some(asset.source()),
        ChangeTarget::Set(_)
        | ChangeTarget::Remove(_)
        | ChangeTarget::AddBlock(_)
        | ChangeTarget::Option(_)
        | ChangeTarget::Credit(_)
        | ChangeTarget::Animation(_)
        | ChangeTarget::AnimationAppend(_) => None,
    }
}

fn has_bytes(change: &Change, assets: &BTreeMap<String, Vec<u8>>) -> bool {
    asset_source(change).is_none_or(|source| assets.contains_key(source))
}

/// Drops the edits that name a left-out font, given as (group, path): its `CustomFontFiles`
/// line, the slot and font-clone edits of its own action (which give a part its family), and
/// any edit of a clone that action defined.
fn drop_dependents(changes: &mut Vec<Change>, lost: &[(u32, &str)]) {
    let in_lost_action = |c: &Change| lost.iter().any(|(group, _)| *group == c.group());
    let names_path = |c: &Change| {
        let texts = named_texts(c);
        lost.iter()
            .any(|(_, path)| texts.iter().any(|t| t.eq_ignore_ascii_case(path)))
    };
    let clones: Vec<String> = (changes.iter())
        .filter(|c| in_lost_action(c))
        .filter_map(|c| match c.target() {
            ChangeTarget::AddBlock(add) if defines_font(add) && is_clone(add.name()) => {
                Some(add.name().to_owned())
            }
            ChangeTarget::Set(_)
            | ChangeTarget::Remove(_)
            | ChangeTarget::AddBlock(_)
            | ChangeTarget::Asset(_)
            | ChangeTarget::Option(_)
            | ChangeTarget::Credit(_)
            | ChangeTarget::Animation(_)
            | ChangeTarget::AnimationAppend(_) => None,
        })
        .collect();
    let depends = |c: &Change| {
        let texts = named_texts(c);
        (writes_font_slots(c) && (names_path(c) || in_lost_action(c)))
            || (in_lost_action(c) && texts.iter().any(|t| is_clone(t)))
            || (texts.iter()).any(|t| clones.iter().any(|n| n.eq_ignore_ascii_case(t)))
    };
    changes.retain(|c| !depends(c));
}

/// `made_for` counts only as an object of two strings; anything else reads as unknown.
fn read_made_for(value: Option<&Value>) -> Option<(String, String)> {
    let made_for = value?.as_object()?;
    let repo = made_for.get("repo")?.as_str()?;
    let hash = made_for.get("hash")?.as_str()?;
    Some((repo.to_owned(), hash.to_owned()))
}

/// Control and invisible formatting characters out, trimmed, at most 80 characters; nothing
/// left reads as "Recipe".
fn clean_name(name: &str) -> String {
    let kept: String = name
        .chars()
        .filter(|c| !c.is_control() && !is_invisible(*c))
        .collect();
    let cut: String = kept.trim().chars().take(MAX_NAME_CHARS).collect();
    match cut.trim_end() {
        "" => "Recipe".to_owned(),
        name => name.to_owned(),
    }
}

/// Bidi controls, zero-width marks and the byte order mark, which could make a shared name
/// display as something other than its text.
pub(super) fn is_invisible(c: char) -> bool {
    matches!(c, '\u{200B}'..='\u{200F}' | '\u{202A}'..='\u{202E}' | '\u{2066}'..='\u{2069}' | '\u{FEFF}')
}

/// What applying a recipe gives: the next change list and what was left out.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct RecipeApplied {
    changes: Vec<Change>,
    skipped: usize,
    added: Vec<(String, Vec<u8>)>,
}

impl RecipeApplied {
    /// The whole next change list.
    pub fn changes(&self) -> &[Change] {
        &self.changes
    }

    /// Recipe changes left out: those that did not resolve, and fonts that were refused or
    /// over the limit of 32 per apply.
    pub fn skipped(&self) -> usize {
        self.skipped
    }

    /// Font files the list now needs, as (source, bytes), for the editor to hold.
    pub fn added(&self) -> &[(String, Vec<u8>)] {
        &self.added
    }
}

impl HudModel {
    /// `changes` plus `recipe`'s changes in `group`. Fonts are added again on a free file
    /// name and `CustomFontFiles` slot (the recipe's own slot edits are left out), and a font
    /// clone the HUD already has is renamed above every clone number in play. Recipe changes
    /// that do not apply after a replay from pristine, and fonts refused or past 32, are left
    /// out and counted; then each applied recipe `Option` removes the list's earlier choice
    /// for the same (source, id).
    pub fn apply_recipe(&self, recipe: &Recipe, changes: &[Change], group: u32) -> RecipeApplied {
        let (tail, added, refused) = self.recipe_tail(recipe, changes, group);
        let fitted = tail.len();
        let kept = self.resolved_tail(changes, tail);
        let skipped = refused.saturating_add(fitted.saturating_sub(kept.len()));

        // A recipe's choice replaces the player's only once it has applied, so a skipped
        // option never costs the player the choice they had.
        let chosen: Vec<(&str, &str)> = kept
            .iter()
            .filter_map(|c| match c.target() {
                ChangeTarget::Option(o) => Some((o.source(), o.id())),
                ChangeTarget::Set(_)
                | ChangeTarget::Remove(_)
                | ChangeTarget::AddBlock(_)
                | ChangeTarget::Asset(_)
                | ChangeTarget::Credit(_)
                | ChangeTarget::Animation(_)
                | ChangeTarget::AnimationAppend(_) => None,
            })
            .collect();
        let mut next: Vec<Change> = changes
            .iter()
            .filter(|c| match c.target() {
                ChangeTarget::Option(o) => !chosen.contains(&(o.source(), o.id())),
                ChangeTarget::Set(_)
                | ChangeTarget::Remove(_)
                | ChangeTarget::AddBlock(_)
                | ChangeTarget::Asset(_)
                | ChangeTarget::Credit(_)
                | ChangeTarget::Animation(_)
                | ChangeTarget::AnimationAppend(_) => true,
            })
            .cloned()
            .collect();
        next.extend(kept);
        RecipeApplied {
            changes: next,
            skipped,
            added,
        }
    }

    /// The recipe's changes in `group`, fitted onto the HUD as `head` leaves it, with the
    /// fonts added as (source, bytes) and how many changes were refused.
    fn recipe_tail(
        &self,
        recipe: &Recipe,
        head: &[Change],
        group: u32,
    ) -> (Vec<Change>, Vec<AddedFont>, usize) {
        let renames = self.clone_renames(recipe.changes(), head);
        let (mut tail, mut added, mut refused) = (Vec::new(), Vec::new(), 0usize);
        let mut reach: Option<ConsoleReach> = None;
        let mut attempts = 0usize;
        for change in recipe.changes() {
            if writes_font_slots(change) {
                continue;
            }
            let Some(change) = renamed_change(change, &renames) else {
                refused += 1;
                continue;
            };
            // An animation line that could start a console command is cut and counted.
            let change = if is_animation(&change) {
                let reach = reach.get_or_insert_with(|| recipe_reach(self, recipe, head));
                let (change, cut) = reach.cut(&change, self.platform());
                refused = refused.saturating_add(cut);
                change
            } else {
                change
            };
            let ChangeTarget::Asset(asset) = change.target() else {
                tail.push(change.in_group(group));
                continue;
            };
            // Every font counts toward the cap, refused or not, and bytes that are no usable
            // font are refused before the replay; each font is added on the list so far, so
            // two fonts in one recipe (or one already in the HUD) never share a name or slot.
            attempts = attempts.saturating_add(1);
            let font = (recipe.asset(asset.source()))
                .filter(|bytes| attempts <= MAX_FONTS && addable_family(bytes).is_some())
                .and_then(|bytes| {
                    let so_far: Vec<Change> = head.iter().chain(&tail).cloned().collect();
                    let name = asset.path().rsplit('/').next().unwrap_or_default();
                    let model = self.with_changes(&so_far).into_model();
                    let (file, changes) = model.add_font_file(name, bytes, group).ok()?;
                    Some((file.source().to_owned(), bytes.to_vec(), changes))
                });
            match font {
                Some((source, bytes, changes)) => {
                    tail.extend(changes);
                    added.push((source, bytes));
                }
                None => refused += 1,
            }
        }
        (tail, added, refused)
    }

    /// For each clone the recipe defines that the HUD after `head` already has, its new name:
    /// counting up from one above every clone number in the HUD and the recipe, or `None`
    /// once the numbers run out.
    fn clone_renames(&self, recipe: &[Change], head: &[Change]) -> Vec<(String, Option<String>)> {
        let mut defined: Vec<&str> = Vec::new();
        for change in recipe {
            if let ChangeTarget::AddBlock(add) = change.target()
                && defines_font(add)
                && is_clone(add.name())
                && !defined.iter().any(|d| d.eq_ignore_ascii_case(add.name()))
            {
                defined.push(add.name());
            }
        }
        if defined.is_empty() {
            return Vec::new();
        }
        let model = self.with_changes(head).into_model();
        let Some(doc) = model.document(SCHEME_FILE) else {
            return Vec::new();
        };
        let named = recipe.iter().flat_map(named_texts).filter_map(clone_number);
        let mut next = named
            .map(|n| n.checked_add(1))
            .chain([next_clone_number(&model, doc)])
            .try_fold(1u64, |max, n| Some(max.max(n?)));
        let in_use = |name: &str| {
            (model.scheme().font_names().iter()).any(|f| f.eq_ignore_ascii_case(name))
                || (doc.entries().iter().filter(|e| e.text().is_none()))
                    .any(|root| !doc.find(&[root.key(), "Fonts", name]).is_empty())
        };
        let mut renames = Vec::new();
        for name in defined.into_iter().filter(|name| in_use(name)) {
            let new = next.map(|n| format!("{CLONE_PREFIX}{n}"));
            next = next.and_then(|n| n.checked_add(1));
            renames.push((name.to_owned(), new));
        }
        renames
    }

    /// The changes of `tail` that resolve when replayed after `head`. `Edited::applied` keeps
    /// input order, so a two-pointer walk tells applied changes from dropped ones; changes
    /// already in `head` stay as they are whether or not they resolve.
    fn resolved_tail(&self, head: &[Change], tail: Vec<Change>) -> Vec<Change> {
        let all: Vec<Change> = head.iter().chain(&tail).cloned().collect();
        let edited = self.with_changes(&all);
        let mut applied = edited.applied().iter().peekable();
        let mut resolved = Vec::new();
        for (i, change) in all.into_iter().enumerate() {
            let hit = applied.next_if(|a| **a == change).is_some();
            if hit && i >= head.len() {
                resolved.push(change);
            }
        }
        resolved
    }
}

/// The receiving HUD's events after `head`, with the recipe's own whole events laid over
/// them.
fn recipe_reach(model: &HudModel, recipe: &Recipe, head: &[Change]) -> ConsoleReach {
    let edited = model.with_changes(head);
    let mut reach = ConsoleReach::over(edited.model().animations());
    reach.lay_over(recipe.changes(), model.platform());
    reach
}

fn is_animation(change: &Change) -> bool {
    matches!(
        change.target(),
        ChangeTarget::Animation(_) | ChangeTarget::AnimationAppend(_)
    )
}

/// Whether a change lies in clientscheme.res's `CustomFontFiles`, which the apply rewrites
/// itself when it adds a recipe's fonts. Lookups are length-checked for hostile paths.
fn writes_font_slots(change: &Change) -> bool {
    let slots = |p: &[String]| p.get(1).is_some_and(|b| b.eq_ignore_ascii_case(SLOTS));
    match change.target() {
        ChangeTarget::Set(set) => is_scheme(set.file()) && slots(set.block()),
        ChangeTarget::Remove(remove) => is_scheme(remove.file()) && slots(remove.block()),
        ChangeTarget::AddBlock(add) => {
            is_scheme(add.file())
                && (slots(add.parent())
                    || (add.parent().len() == 1 && add.name().eq_ignore_ascii_case(SLOTS)))
        }
        ChangeTarget::Asset(_)
        | ChangeTarget::Option(_)
        | ChangeTarget::Credit(_)
        | ChangeTarget::Animation(_)
        | ChangeTarget::AnimationAppend(_) => false,
    }
}

fn is_scheme(file: &str) -> bool {
    check_hud_path(file).as_deref() == Some(SCHEME_FILE)
}

/// Whether an `AddBlock` defines a font directly under the scheme's `Fonts`.
fn defines_font(add: &AddBlock) -> bool {
    let parent = add.parent();
    is_scheme(add.file())
        && parent.len() == 2
        && parent
            .get(1)
            .is_some_and(|b| b.eq_ignore_ascii_case("Fonts"))
}

/// Every text in a change that can name a font clone: values, keys, block paths and the
/// lines of an added block.
fn named_texts(change: &Change) -> Vec<&str> {
    let mut texts: Vec<&str> = Vec::new();
    match change.target() {
        ChangeTarget::Set(set) => {
            texts.extend(set.block().iter().map(String::as_str));
            texts.extend([set.key(), set.value()]);
        }
        ChangeTarget::Remove(remove) => {
            texts.extend(remove.block().iter().map(String::as_str));
            texts.push(remove.key());
        }
        ChangeTarget::AddBlock(add) => {
            texts.extend(add.parent().iter().map(String::as_str));
            texts.push(add.name());
            texts.extend(add.lines().iter().flat_map(|l| [l.key(), l.value()]));
        }
        ChangeTarget::Asset(_)
        | ChangeTarget::Option(_)
        | ChangeTarget::Credit(_)
        | ChangeTarget::Animation(_)
        | ChangeTarget::AnimationAppend(_) => {}
    }
    texts
}

/// `text` under its new name when it is a renamed clone; `None` when that clone found no name.
fn renamed<'a>(text: &'a str, renames: &'a [(String, Option<String>)]) -> Option<&'a str> {
    match renames
        .iter()
        .find(|(old, _)| old.eq_ignore_ascii_case(text))
    {
        Some((_, new)) => new.as_deref(),
        None => Some(text),
    }
}

/// The change with every renamed clone replaced in its value and block path; `None` when it
/// names a clone that could not be renamed.
fn renamed_change(change: &Change, renames: &[(String, Option<String>)]) -> Option<Change> {
    if renames.is_empty() {
        return Some(change.clone());
    }
    let path = |p: &'_ [String]| -> Option<Vec<String>> {
        p.iter()
            .map(|s| renamed(s, renames).map(str::to_owned))
            .collect()
    };
    let group = change.group();
    Some(match change.target() {
        ChangeTarget::Set(set) => {
            let block = path(set.block())?;
            let block: Vec<&str> = block.iter().map(String::as_str).collect();
            let (key, value) = (renamed(set.key(), renames)?, renamed(set.value(), renames)?);
            let key = SetKey::new(set.file(), &block, key, set.cond(), set.old(), value);
            Change::set(group, key)
        }
        ChangeTarget::Remove(remove) => {
            let block = path(remove.block())?;
            let block: Vec<&str> = block.iter().map(String::as_str).collect();
            let key = RemoveKey::new(
                remove.file(),
                &block,
                renamed(remove.key(), renames)?,
                remove.cond(),
                remove.old(),
            );
            Change::remove(group, key)
        }
        ChangeTarget::AddBlock(add) => {
            let name = match defines_font(add) {
                true => renamed(add.name(), renames)?,
                false => add.name(),
            };
            let parent = path(add.parent())?;
            let parent: Vec<&str> = parent.iter().map(String::as_str).collect();
            let lines = (add.lines().iter())
                .map(|l| {
                    let (key, value) = (renamed(l.key(), renames)?, renamed(l.value(), renames)?);
                    Some(BlockLine::new(key, value, l.cond()))
                })
                .collect::<Option<Vec<_>>>()?;
            let block = AddBlock::new(add.file(), &parent, name, add.cond(), lines);
            Change::add_block(group, block)
        }
        ChangeTarget::Asset(_)
        | ChangeTarget::Option(_)
        | ChangeTarget::Credit(_)
        | ChangeTarget::Animation(_)
        | ChangeTarget::AnimationAppend(_) => change.clone(),
    })
}
