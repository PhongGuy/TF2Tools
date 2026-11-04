//! `maker.json`: a made HUD's changes and the hashes of the pristine files they touch.
//! The file is a trust boundary, so loading refuses what it cannot read and drops paths
//! that leave the HUD's own files.

use std::collections::BTreeMap;

use serde_json::{Map, Value, json};

use super::{
    AddBlock, AnimationAppend, AnimationChange, AssetFile, BlockLine, Change, ChangeTarget,
    CreditNote, OptionChange, OptionValue, RemoveKey, SetKey, check_hud_path,
};
use crate::error::HudLoadError;

/// The only `maker.json` version this build reads and writes.
const VERSION: u64 = 1;

/// The longest picked key a file may store, in bytes.
const MAX_PICK: usize = 4096;

/// The option sources written under `hud_option`.
const HUD_SOURCES: [&str; 2] = [OptionChange::HUD, OptionChange::EDITOR];

/// A made HUD's `maker.json`: source hashes by normalised game path, and the changes.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct MakerFile {
    base: MakerBase,
    sources: BTreeMap<String, String>,
    changes: Vec<Change>,
}

/// What a made HUD starts from: the default HUD, or a copy of a library HUD.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
#[non_exhaustive]
#[expect(
    clippy::large_enum_variant,
    reason = "one per maker.json; the contract matches on Copy(CopyBase) unboxed"
)]
pub enum MakerBase {
    /// The stock HUD from the game's files.
    #[default]
    Default,
    /// A whole-folder copy of a library HUD.
    Copy(CopyBase),
}

/// The library HUD a copy was made from, as `maker.json` records it.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct CopyBase {
    hud_id: String,
    name: String,
    author: String,
    origin: String,
    store_id: String,
    repo: String,
    hash: String,
    tree: String,
    base_folder: String,
    base_updated_at: String,
    install_folder: Option<String>,
}

/// Generates a builder and a getter for each text field of `CopyBase`.
macro_rules! copy_texts {
    ($($field:ident, $with:ident, $doc:literal;)*) => {
        impl CopyBase {
            $(
                #[doc = concat!("The base's ", $doc, ".")]
                pub fn $field(&self) -> &str {
                    &self.$field
                }

                #[doc = concat!("This record with the base's ", $doc, ".")]
                pub fn $with(mut self, value: &str) -> CopyBase {
                    value.clone_into(&mut self.$field);
                    self
                }
            )*
        }
    };
}

copy_texts! {
    author, with_author, "author";
    origin, with_origin, "origin in Stage 8's words";
    store_id, with_store_id, "store id (`hud-db:<id>`, `gb:<id>` or empty)";
    repo, with_repo, "repository";
    hash, with_hash, "commit, for a curated HUD";
    tree, with_tree, "tree hash when copied";
    base_folder, with_base_folder, "library folder";
    base_updated_at, with_base_updated_at, "row's `updated_at` when copied";
}

impl CopyBase {
    /// A record of library HUD `hud_id` named `name`; every other text empty, no folder.
    pub fn new(hud_id: &str, name: &str) -> CopyBase {
        CopyBase {
            hud_id: hud_id.to_owned(),
            name: name.to_owned(),
            author: String::new(),
            origin: String::new(),
            store_id: String::new(),
            repo: String::new(),
            hash: String::new(),
            tree: String::new(),
            base_folder: String::new(),
            base_updated_at: String::new(),
            install_folder: None,
        }
    }

    /// The base's library id.
    pub fn hud_id(&self) -> &str {
        &self.hud_id
    }

    /// The base's name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The `tf/custom` folder the copy installs under, when one was worked out.
    pub fn install_folder(&self) -> Option<&str> {
        self.install_folder.as_deref()
    }

    /// This record with `folder` as its install folder.
    pub fn with_install_folder(mut self, folder: Option<&str>) -> CopyBase {
        self.install_folder = folder.map(str::to_owned);
        self
    }

    fn to_json(&self) -> Value {
        json!({
            "kind": "copy",
            "hud_id": self.hud_id,
            "name": self.name,
            "author": self.author,
            "origin": self.origin,
            "store_id": self.store_id,
            "repo": self.repo,
            "hash": self.hash,
            "tree": self.tree,
            "base_folder": self.base_folder,
            "base_updated_at": self.base_updated_at,
            "install_folder": self.install_folder,
        })
    }

    /// Reads a `"kind": "copy"` base: absent texts are empty, each text is capped like a
    /// credit's, and an install folder that is not one safe folder name reads as `None`.
    fn parse(base: &Map<String, Value>) -> Result<CopyBase, HudLoadError> {
        let text = |field: &str| -> Result<String, HudLoadError> {
            match base.get(field) {
                None | Some(Value::Null) => Ok(String::new()),
                Some(Value::String(s))
                    if s.chars().count() <= 200 && !s.chars().any(char::is_control) =>
                {
                    Ok(s.clone())
                }
                Some(_) => Err(damaged(&format!("the base's {field} is malformed"))),
            }
        };
        let folder = text("install_folder")?;
        Ok(CopyBase {
            hud_id: text("hud_id")?,
            name: text("name")?,
            author: text("author")?,
            origin: text("origin")?,
            store_id: text("store_id")?,
            repo: text("repo")?,
            hash: text("hash")?,
            tree: text("tree")?,
            base_folder: text("base_folder")?,
            base_updated_at: text("base_updated_at")?,
            install_folder: crate::library::is_safe_folder(&folder).then_some(folder),
        })
    }
}

impl MakerFile {
    /// Larger files are refused unread; a real one holds a few hundred changes.
    pub(crate) const MAX_BYTES: usize = 8 * 1024 * 1024;

    /// A file holding `sources` (game path to SHA-256 hex) and `changes`.
    pub fn new(sources: BTreeMap<String, String>, changes: Vec<Change>) -> MakerFile {
        MakerFile {
            base: MakerBase::Default,
            sources,
            changes,
        }
    }

    /// This file with `base` as what the HUD starts from.
    pub fn with_base(mut self, base: MakerBase) -> MakerFile {
        self.base = base;
        self
    }

    /// What the HUD starts from.
    pub fn base(&self) -> &MakerBase {
        &self.base
    }

    /// The pristine-file hashes by game path.
    pub fn sources(&self) -> &BTreeMap<String, String> {
        &self.sources
    }

    /// The changes in the order they were made.
    pub fn changes(&self) -> &[Change] {
        &self.changes
    }

    /// The file and how many changes the path rule dropped.
    ///
    /// # Errors
    /// `Damaged` when the bytes are not a `maker.json`; `Newer` when they use a version, base
    /// or change kind this build does not know.
    pub fn parse(bytes: &[u8]) -> Result<(MakerFile, usize), HudLoadError> {
        if bytes.len() > Self::MAX_BYTES {
            return Err(damaged("over 8 MB"));
        }
        let value: Value =
            serde_json::from_slice(bytes).map_err(|e| damaged(&format!("not JSON: {e}")))?;
        let root = value.as_object().ok_or_else(|| damaged("not an object"))?;
        match root.get("version").and_then(Value::as_u64) {
            Some(0) | None => return Err(damaged("no positive version")),
            Some(v) if v > VERSION => return Err(HudLoadError::Newer),
            Some(_) => {}
        }
        let base_object = root.get("base").and_then(Value::as_object);
        let kind = base_object
            .and_then(|b| b.get("kind"))
            .and_then(Value::as_str);
        let base = match (kind, base_object) {
            (Some("default"), _) => MakerBase::Default,
            (Some("copy"), Some(object)) => MakerBase::Copy(CopyBase::parse(object)?),
            (Some(_), _) => return Err(HudLoadError::Newer),
            (None, _) => return Err(damaged("no base kind")),
        };

        // Sources failing the path rule are dropped silently: a hash alone changes nothing.
        let mut sources = BTreeMap::new();
        for (path, hash) in optional(root, "sources", Value::as_object)?
            .into_iter()
            .flatten()
        {
            let hash = hash
                .as_str()
                .ok_or_else(|| damaged("a source hash is not text"))?;
            if let Some(path) = check_hud_path(path) {
                sources.insert(path, hash.to_owned());
            }
        }

        let mut changes = Vec::new();
        let mut dropped = 0;
        for entry in optional(root, "changes", Value::as_array)?
            .into_iter()
            .flatten()
        {
            match parse_change(entry)? {
                Some(change) => changes.push(change),
                None => dropped += 1,
            }
        }
        Ok((
            MakerFile {
                base,
                sources,
                changes,
            },
            dropped,
        ))
    }

    /// The file as pretty JSON, version 1.
    pub fn to_json(&self) -> Vec<u8> {
        let changes: Vec<Value> = self.changes.iter().map(change_json).collect();
        let base = match &self.base {
            MakerBase::Default => json!({ "kind": "default" }),
            MakerBase::Copy(copy) => copy.to_json(),
        };
        let file = json!({
            "version": VERSION,
            "base": base,
            "sources": self.sources,
            "changes": changes,
        });
        format!("{file:#}\n").into_bytes()
    }
}

/// One change as its `maker.json` entry; recipe files share the format.
pub(super) fn change_json(change: &Change) -> Value {
    match change.target() {
        ChangeTarget::Set(set) => json!({
            "group": change.group(),
            // An appending set is its own kind, so an older build reads the file as newer
            // instead of replacing a line it should have added.
            (if set.appends() { "set_append" } else { "set" }): {
                "file": set.file(),
                "block": set.block(),
                "key": set.key(),
                "cond": set.cond(),
                "old": set.old(),
                "new": set.value(),
            },
        }),
        ChangeTarget::Remove(remove) => json!({
            "group": change.group(),
            "remove": {
                "file": remove.file(),
                "block": remove.block(),
                "key": remove.key(),
                "cond": remove.cond(),
                "old": remove.old(),
            },
        }),
        ChangeTarget::AddBlock(add) => json!({
            "group": change.group(),
            "add_block": {
                "file": add.file(),
                "parent": add.parent(),
                "name": add.name(),
                "cond": add.cond(),
                "lines": add.lines().iter().map(|line| json!({
                    "key": line.key(),
                    "value": line.value(),
                    "cond": line.cond(),
                })).collect::<Vec<_>>(),
            },
        }),
        ChangeTarget::Asset(asset) => json!({
            "group": change.group(),
            "asset": { "path": asset.path(), "source": asset.source() },
        }),
        ChangeTarget::Credit(note) => json!({
            "group": change.group(),
            "credit": {
                "hud": note.hud(),
                "author": note.author(),
                "origin": note.origin(),
                "part": note.part(),
            },
        }),
        ChangeTarget::Animation(a) => json!({
            "group": change.group(),
            "animation": { "event": a.event(), "body": a.body() },
        }),
        ChangeTarget::AnimationAppend(a) => json!({
            "group": change.group(),
            "animation_append": { "event": a.event(), "lines": a.lines() },
        }),
        ChangeTarget::Option(option) => {
            // The HUD's own choices take their own kind key, so an older build reads the file
            // as newer instead of dropping them and losing them on its next save.
            let kind = if HUD_SOURCES.contains(&option.source()) {
                "hud_option"
            } else {
                "option"
            };
            json!({
                "group": change.group(),
                kind: {
                    "source": option.source(),
                    "id": option.id(),
                    "value": option_value_json(option.value()),
                },
            })
        }
    }
}

fn damaged(reason: &str) -> HudLoadError {
    HudLoadError::Damaged {
        reason: reason.to_owned(),
    }
}

/// A field that may be absent but, when present, must have the shape `cast` accepts.
fn optional<'a, T: ?Sized>(
    root: &'a Map<String, Value>,
    field: &str,
    cast: fn(&'a Value) -> Option<&'a T>,
) -> Result<Option<&'a T>, HudLoadError> {
    match root.get(field) {
        None => Ok(None),
        Some(value) => cast(value)
            .map(Some)
            .ok_or_else(|| damaged(&format!("{field} has the wrong shape"))),
    }
}

/// One change entry: `None` when a path fails the path rule.
pub(super) fn parse_change(entry: &Value) -> Result<Option<Change>, HudLoadError> {
    let entry = entry
        .as_object()
        .ok_or_else(|| damaged("a change is not an object"))?;
    let group = entry
        .get("group")
        .and_then(Value::as_u64)
        .and_then(|g| u32::try_from(g).ok())
        .ok_or_else(|| damaged("a change has no numeric group"))?;
    // The first known kind key decides the change; any other kind belongs to a later stage.
    let known = [
        "set",
        "remove",
        "add_block",
        "set_append",
        "asset",
        "credit",
        "option",
        "hud_option",
        "animation",
        "animation_append",
    ];
    let Some((kind, body)) = known
        .iter()
        .find_map(|k| entry.get(*k).map(|body| (*k, body)))
    else {
        return Err(if entry.keys().any(|k| k != "group") {
            HudLoadError::Newer
        } else {
            damaged("a change has no kind")
        });
    };
    let malformed = || damaged(&format!("the {kind} is malformed"));
    let fields = Fields(body.as_object().ok_or_else(malformed)?);
    let change = match kind {
        "set" | "set_append" => parse_set(&fields).ok_or_else(malformed)?.map(|set| {
            Change::set(
                group,
                if kind == "set_append" {
                    set.appended()
                } else {
                    set
                },
            )
        }),
        "remove" => parse_remove(&fields)
            .ok_or_else(malformed)?
            .map(|remove| Change::remove(group, remove)),
        "add_block" => parse_add_block(&fields)
            .ok_or_else(malformed)?
            .map(|add| Change::add_block(group, add)),
        "asset" => parse_asset(&fields)
            .ok_or_else(malformed)?
            .map(|asset| Change::asset(group, asset)),
        "credit" => parse_credit(&fields)
            .ok_or_else(malformed)?
            .map(|note| Change::credit(group, note)),
        // Text that fails the parser still loads; the replay drops and counts it.
        "animation" => {
            let (event, body) = (fields.text("event"), fields.text("body"));
            let (event, body) = event.zip(body).ok_or_else(malformed)?;
            Some(Change::animation(group, AnimationChange::new(event, body)))
        }
        "animation_append" => {
            let (event, lines) = (fields.text("event"), fields.text("lines"));
            let (event, lines) = event.zip(lines).ok_or_else(malformed)?;
            Some(Change::animation_append(
                group,
                AnimationAppend::new(event, lines),
            ))
        }
        _ => {
            let option = parse_option(&fields).ok_or_else(malformed)?;
            // A `hud_option` holds only the HUD's own choices; anything else there is damage.
            let source = fields.text("source").unwrap_or_default();
            if kind == "hud_option" && !HUD_SOURCES.contains(&source) {
                return Err(malformed());
            }
            option.map(|option| Change::option(group, option))
        }
    };
    Ok(change)
}

/// A change body's fields. Each reader returns `None` when the field has the wrong shape.
struct Fields<'a>(&'a Map<String, Value>);

impl<'a> Fields<'a> {
    fn text(&self, field: &str) -> Option<&'a str> {
        self.0.get(field)?.as_str()
    }

    /// Absent or null is `Some(None)`; text is `Some(Some(_))`.
    fn maybe(&self, field: &str) -> Option<Option<&'a str>> {
        match self.0.get(field) {
            None | Some(Value::Null) => Some(None),
            Some(Value::String(s)) => Some(Some(s.as_str())),
            Some(_) => None,
        }
    }

    /// A non-empty array of text.
    fn path(&self, field: &str) -> Option<Vec<&'a str>> {
        let path: Vec<&str> = self
            .0
            .get(field)?
            .as_array()?
            .iter()
            .map(Value::as_str)
            .collect::<Option<_>>()?;
        (!path.is_empty()).then_some(path)
    }
}

// Each parser's outer `None` is a malformed body; the inner `None` is a path the path rule
// refuses, so the change is dropped and counted.

fn parse_set(f: &Fields) -> Option<Option<SetKey>> {
    let (block, key, cond) = (f.path("block")?, f.text("key")?, f.maybe("cond")?);
    let (old, new) = (f.maybe("old")?, f.text("new")?);
    let file = f.text("file")?;
    Some(check_hud_path(file).map(|file| SetKey::new(&file, &block, key, cond, old, new)))
}

fn parse_remove(f: &Fields) -> Option<Option<RemoveKey>> {
    let (block, key, cond) = (f.path("block")?, f.text("key")?, f.maybe("cond")?);
    let (old, file) = (f.maybe("old")?, f.text("file")?);
    Some(check_hud_path(file).map(|file| RemoveKey::new(&file, &block, key, cond, old)))
}

fn parse_add_block(f: &Fields) -> Option<Option<AddBlock>> {
    let (parent, name, cond) = (f.path("parent")?, f.text("name")?, f.maybe("cond")?);
    if name.is_empty() {
        return None;
    }
    let lines =
        f.0.get("lines")?
            .as_array()?
            .iter()
            .map(|line| {
                let line = Fields(line.as_object()?);
                Some(BlockLine::new(
                    line.text("key")?,
                    line.text("value")?,
                    line.maybe("cond")?,
                ))
            })
            .collect::<Option<Vec<_>>>()?;
    let file = f.text("file")?;
    Some(check_hud_path(file).map(|file| AddBlock::new(&file, &parent, name, cond, lines)))
}

/// An asset's source must also stay inside the made HUD's own `.tf2tools/` folder.
fn parse_asset(f: &Fields) -> Option<Option<AssetFile>> {
    let (path, source) = (f.text("path")?, f.text("source")?);
    let source = check_hud_path(source).filter(|s| s.starts_with(".tf2tools/"));
    Some(
        check_hud_path(path)
            .zip(source)
            .map(|(path, source)| AssetFile::new(&path, &source)),
    )
}

/// A credit's four fields are text of at most 200 characters with no control characters.
fn parse_credit(f: &Fields) -> Option<Option<CreditNote>> {
    let clean = |field: &str| {
        let text = f.text(field)?;
        (text.chars().count() <= 200 && !text.chars().any(char::is_control)).then_some(text)
    };
    let (hud, author) = (clean("hud")?, clean("author")?);
    let (origin, part) = (clean("origin")?, clean("part")?);
    Some(Some(CreditNote::new(hud, author, origin, part)))
}

/// An option body's value holds exactly one kind key; a kind this build does not know is the
/// inner `None`, so the change is dropped and counted.
fn parse_option(f: &Fields) -> Option<Option<OptionChange>> {
    let (source, id) = (f.text("source")?, f.text("id")?);
    let value = f.0.get("value")?.as_object()?;
    let mut kinds = value.iter();
    let (Some((kind, value)), None) = (kinds.next(), kinds.next()) else {
        return None;
    };
    let value = match kind.as_str() {
        "bool" => OptionValue::Bool(value.as_bool()?),
        "int" => OptionValue::Int(i32::try_from(value.as_i64()?).ok()?),
        "rgba" => {
            let channels = value
                .as_array()?
                .iter()
                .map(|c| u8::try_from(c.as_u64()?).ok())
                .collect::<Option<Vec<u8>>>()?;
            OptionValue::Rgba(<[u8; 4]>::try_from(channels).ok()?)
        }
        "pick" => OptionValue::Pick(value.as_str().filter(|k| k.len() <= MAX_PICK)?.to_owned()),
        _ => return Some(None),
    };
    Some(Some(OptionChange::new(source, id, value)))
}

fn option_value_json(value: &OptionValue) -> Value {
    match value {
        OptionValue::Bool(b) => json!({ "bool": b }),
        OptionValue::Int(i) => json!({ "int": i }),
        OptionValue::Rgba(rgba) => json!({ "rgba": rgba }),
        OptionValue::Pick(key) => json!({ "pick": key }),
    }
}
