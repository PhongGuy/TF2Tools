//! A player's edits as semantic changes, kept apart from the bytes they produce so they can be
//! saved, replayed onto pristine files and refused when they leave the HUD's own files.

use super::animations::checked_lines;
use super::{AnimCommand, OptionValue, normalise_game_path};
use crate::fonts::Platform;

/// Sets one key in one block of one file; `old` records the value it replaced, never checked.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct SetKey {
    file: String,
    block: Vec<String>,
    key: String,
    cond: Option<String>,
    old: Option<String>,
    new: String,
    append: bool,
}

impl SetKey {
    /// A set of `key` (with conditional `cond`) to `new` in the block at `block` of `file`.
    pub fn new(
        file: &str,
        block: &[&str],
        key: &str,
        cond: Option<&str>,
        old: Option<&str>,
        new: &str,
    ) -> SetKey {
        SetKey {
            file: file.to_owned(),
            block: owned(block),
            key: key.to_owned(),
            cond: cond.map(str::to_owned),
            old: old.map(str::to_owned),
            new: new.to_owned(),
            append: false,
        }
    }

    /// The same set, but always adding a new line instead of setting the first line of the same
    /// key and conditional, so a key written twice keeps both lines.
    #[must_use]
    pub fn appended(mut self) -> SetKey {
        self.append = true;
        self
    }

    /// Whether the set adds a new line even when the key and conditional already have one.
    pub fn appends(&self) -> bool {
        self.append
    }

    /// The game path as given.
    pub fn file(&self) -> &str {
        &self.file
    }

    /// The block path, the file's root block first.
    pub fn block(&self) -> &[String] {
        &self.block
    }

    /// The key.
    pub fn key(&self) -> &str {
        &self.key
    }

    /// The conditional the set line carries, such as `$WIN32`.
    pub fn cond(&self) -> Option<&str> {
        self.cond.as_deref()
    }

    /// The value before the change, when it was known.
    pub fn old(&self) -> Option<&str> {
        self.old.as_deref()
    }

    /// The new value.
    pub fn value(&self) -> &str {
        &self.new
    }
}

/// Removes one key's lines from one block of one file; `old` records the value it removed.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct RemoveKey {
    file: String,
    block: Vec<String>,
    key: String,
    cond: Option<String>,
    old: Option<String>,
}

impl RemoveKey {
    /// A removal of `key` from the block at `block` of `file`; `cond: None` removes every line
    /// of the key, `Some(c)` only the lines carrying conditional `c`.
    pub fn new(
        file: &str,
        block: &[&str],
        key: &str,
        cond: Option<&str>,
        old: Option<&str>,
    ) -> RemoveKey {
        RemoveKey {
            file: file.to_owned(),
            block: owned(block),
            key: key.to_owned(),
            cond: cond.map(str::to_owned),
            old: old.map(str::to_owned),
        }
    }

    /// The game path as given.
    pub fn file(&self) -> &str {
        &self.file
    }

    /// The block path, the file's root block first.
    pub fn block(&self) -> &[String] {
        &self.block
    }

    /// The key.
    pub fn key(&self) -> &str {
        &self.key
    }

    /// The conditional whose lines are removed, or `None` for every line.
    pub fn cond(&self) -> Option<&str> {
        self.cond.as_deref()
    }

    /// The value before the change, when it was known.
    pub fn old(&self) -> Option<&str> {
        self.old.as_deref()
    }
}

/// One `key value [cond]` line of an added block.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct BlockLine {
    key: String,
    value: String,
    cond: Option<String>,
}

impl BlockLine {
    /// The line `key value`, with conditional `cond` when given.
    pub fn new(key: &str, value: &str, cond: Option<&str>) -> BlockLine {
        BlockLine {
            key: key.to_owned(),
            value: value.to_owned(),
            cond: cond.map(str::to_owned),
        }
    }

    /// The key.
    pub fn key(&self) -> &str {
        &self.key
    }

    /// The value.
    pub fn value(&self) -> &str {
        &self.value
    }

    /// The conditional the line carries, such as `$LINUX`.
    pub fn cond(&self) -> Option<&str> {
        self.cond.as_deref()
    }
}

/// Adds a named block of lines inside the block at `parent` of one file.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct AddBlock {
    file: String,
    parent: Vec<String>,
    name: String,
    cond: Option<String>,
    lines: Vec<BlockLine>,
}

impl AddBlock {
    /// A block `name` (with conditional `cond`) holding `lines`, added to `parent` of `file`.
    pub fn new(
        file: &str,
        parent: &[&str],
        name: &str,
        cond: Option<&str>,
        lines: Vec<BlockLine>,
    ) -> AddBlock {
        AddBlock {
            file: file.to_owned(),
            parent: owned(parent),
            name: name.to_owned(),
            cond: cond.map(str::to_owned),
            lines,
        }
    }

    /// The game path as given.
    pub fn file(&self) -> &str {
        &self.file
    }

    /// The parent block path, the file's root block first.
    pub fn parent(&self) -> &[String] {
        &self.parent
    }

    /// The new block's name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The conditional the new block carries.
    pub fn cond(&self) -> Option<&str> {
        self.cond.as_deref()
    }

    /// The new block's lines in order.
    pub fn lines(&self) -> &[BlockLine] {
        &self.lines
    }
}

/// A file the made HUD gains: `source` (under `.tf2tools/`) is installed at game path `path`.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct AssetFile {
    path: String,
    source: String,
}

impl AssetFile {
    /// The asset copied from `source` to `path`.
    pub fn new(path: &str, source: &str) -> AssetFile {
        AssetFile {
            path: path.to_owned(),
            source: source.to_owned(),
        }
    }

    /// The game path the file lands at.
    pub fn path(&self) -> &str {
        &self.path
    }

    /// Where the file is kept inside the made HUD.
    pub fn source(&self) -> &str {
        &self.source
    }
}

/// One option choice: which option (`source`, `id`) and its value.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct OptionChange {
    source: String,
    id: String,
    value: OptionValue,
}

impl OptionChange {
    /// The source string for the built-in options.
    pub const BUILTIN: &'static str = "builtin";
    /// The source string for switches read from the HUD's own files.
    pub const HUD: &'static str = "hud";
    /// The stored source string for options read from the HUD's option schema.
    pub const SCHEMA: &'static str = "editor";

    /// The choice of `value` for option `id` from `source`.
    pub fn new(source: &str, id: &str, value: OptionValue) -> OptionChange {
        OptionChange {
            source: source.to_owned(),
            id: id.to_owned(),
            value,
        }
    }

    /// Where the option is defined, such as [`OptionChange::BUILTIN`].
    pub fn source(&self) -> &str {
        &self.source
    }

    /// The option's id within its source.
    pub fn id(&self) -> &str {
        &self.id
    }

    /// The chosen value.
    pub fn value(&self) -> &OptionValue {
        &self.value
    }
}

/// Who a borrowed part came from: the HUD, its author, where it came from and the part.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct CreditNote {
    hud: String,
    author: String,
    origin: String,
    part: String,
}

impl CreditNote {
    /// A credit for `part`, borrowed from `hud` by `author` (may be empty), found at `origin`.
    pub fn new(hud: &str, author: &str, origin: &str, part: &str) -> CreditNote {
        CreditNote {
            hud: hud.to_owned(),
            author: author.to_owned(),
            origin: origin.to_owned(),
            part: part.to_owned(),
        }
    }

    /// The HUD the part came from.
    pub fn hud(&self) -> &str {
        &self.hud
    }

    /// The HUD's author; empty when unknown.
    pub fn author(&self) -> &str {
        &self.author
    }

    /// Where the HUD came from, such as a store item or a file the player imported.
    pub fn origin(&self) -> &str {
        &self.origin
    }

    /// The borrowed part's name.
    pub fn part(&self) -> &str {
        &self.part
    }
}

/// The player's whole event: `body` replaces the event's lines.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct AnimationChange {
    event: String,
    body: String,
}

impl AnimationChange {
    /// Event `event` with exactly the lines of `body`.
    pub fn new(event: &str, body: &str) -> AnimationChange {
        AnimationChange {
            event: event.to_owned(),
            body: body.to_owned(),
        }
    }

    /// The event's name.
    pub fn event(&self) -> &str {
        &self.event
    }

    /// The event's lines, one command per line.
    pub fn body(&self) -> &str {
        &self.body
    }

    /// The body's commands when the change is valid: a name with no whitespace, quote,
    /// brace or `//`, text that parses whole within the caps, and no FireCommand.
    pub fn check(&self, platform: Platform) -> Option<Vec<AnimCommand>> {
        checked_lines(&self.event, &self.body, platform)
    }
}

/// Lines a feature adds after an event's body, removable by dropping the feature's group.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct AnimationAppend {
    event: String,
    lines: String,
}

impl AnimationAppend {
    /// `lines` to add after event `event`'s body.
    pub fn new(event: &str, lines: &str) -> AnimationAppend {
        AnimationAppend {
            event: event.to_owned(),
            lines: lines.to_owned(),
        }
    }

    /// The event's name.
    pub fn event(&self) -> &str {
        &self.event
    }

    /// The added lines, one command per line.
    pub fn lines(&self) -> &str {
        &self.lines
    }

    /// The lines' commands when the change is valid, by the same rules as
    /// [`AnimationChange::check`].
    pub fn check(&self, platform: Platform) -> Option<Vec<AnimCommand>> {
        checked_lines(&self.event, &self.lines, platform)
    }
}

fn owned(path: &[&str]) -> Vec<String> {
    path.iter().map(|s| (*s).to_owned()).collect()
}

/// What a change does.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum ChangeTarget {
    /// Set one key's value.
    Set(SetKey),
    /// Remove one key's lines.
    Remove(RemoveKey),
    /// Add a block of lines.
    AddBlock(AddBlock),
    /// Add a file to the HUD.
    Asset(AssetFile),
    /// Choose an option's value.
    Option(OptionChange),
    /// Credit the author of a borrowed part; changes no file but the credits file.
    Credit(CreditNote),
    /// Replace an animation event's lines.
    Animation(AnimationChange),
    /// Add lines after an animation event's body.
    AnimationAppend(AnimationAppend),
}

/// One semantic change; changes made by one player action share a group.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct Change {
    group: u32,
    target: ChangeTarget,
}

impl Change {
    /// A key set in action group `group`.
    pub fn set(group: u32, set: SetKey) -> Change {
        Change {
            group,
            target: ChangeTarget::Set(set),
        }
    }

    /// A key removal in action group `group`.
    pub fn remove(group: u32, remove: RemoveKey) -> Change {
        Change {
            group,
            target: ChangeTarget::Remove(remove),
        }
    }

    /// A block addition in action group `group`.
    pub fn add_block(group: u32, add: AddBlock) -> Change {
        Change {
            group,
            target: ChangeTarget::AddBlock(add),
        }
    }

    /// An added file in action group `group`.
    pub fn asset(group: u32, asset: AssetFile) -> Change {
        Change {
            group,
            target: ChangeTarget::Asset(asset),
        }
    }

    /// An option choice in action group `group`.
    pub fn option(group: u32, option: OptionChange) -> Change {
        Change {
            group,
            target: ChangeTarget::Option(option),
        }
    }

    /// A credit for a borrowed part in action group `group`.
    pub fn credit(group: u32, note: CreditNote) -> Change {
        Change {
            group,
            target: ChangeTarget::Credit(note),
        }
    }

    /// A whole-event animation change in action group `group`.
    pub fn animation(group: u32, change: AnimationChange) -> Change {
        Change {
            group,
            target: ChangeTarget::Animation(change),
        }
    }

    /// Animation lines a feature appends, in action group `group`.
    pub fn animation_append(group: u32, append: AnimationAppend) -> Change {
        Change {
            group,
            target: ChangeTarget::AnimationAppend(append),
        }
    }

    /// The action group.
    pub fn group(&self) -> u32 {
        self.group
    }

    /// What the change does.
    pub fn target(&self) -> &ChangeTarget {
        &self.target
    }

    /// The same change in action group `group`.
    pub fn in_group(&self, group: u32) -> Change {
        Change {
            group,
            target: self.target.clone(),
        }
    }
}

/// The rule for a path inside a HUD: `Some(normalised lowercase path)` when allowed.
pub fn check_hud_path(path: &str) -> Option<String> {
    // Absolute, drive or NUL-bearing paths are refused before normalising could hide them.
    if path.is_empty() || path.starts_with(['/', '\\']) || path.contains([':', '\0']) {
        return None;
    }
    let path = normalise_game_path(path)?.to_ascii_lowercase();
    // Windows trims trailing dots and spaces and maps device names, so such a segment could
    // land on a refused name or a device; refuse it rather than reason about the result.
    if path
        .split('/')
        .any(|segment| segment.ends_with(['.', ' ']) || is_device_name(segment))
    {
        return None;
    }
    const REFUSED: [&str; 6] = [".cfg", ".bat", ".ps1", ".exe", ".dll", ".vpk"];
    let config_dir = path.split('/').next() == Some("cfg");
    (!config_dir && !REFUSED.iter().any(|ext| path.ends_with(ext))).then_some(path)
}

/// The path rule for the HUD's own switches: [`check_hud_path`], and no segment may be the
/// `.tf2tools` metadata folder in any case.
pub(crate) fn check_switch_path(path: &str) -> Option<String> {
    check_hud_path(path).filter(|p| !p.split('/').any(|s| s.eq_ignore_ascii_case(".tf2tools")))
}

/// Whether a lowercase segment names a Windows device, with or without an extension.
pub(crate) fn is_device_name(segment: &str) -> bool {
    let stem = segment
        .split('.')
        .next()
        .unwrap_or(segment)
        .trim_end_matches(' ');
    // Windows maps COM and LPT followed by any digit or superscript 1-3, not only 1-9.
    let port_suffix = stem
        .strip_prefix("com")
        .or_else(|| stem.strip_prefix("lpt"));
    matches!(
        stem,
        "con" | "prn" | "aux" | "nul" | "conin$" | "conout$" | "clock$"
    ) || port_suffix.is_some_and(|rest| {
        let mut chars = rest.chars();
        matches!(
            chars.next(),
            Some('0'..='9' | '\u{b9}' | '\u{b2}' | '\u{b3}')
        ) && chars.next().is_none()
    })
}
