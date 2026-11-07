//! HUD option schemas: the bundled set, read leniently (comments, a BOM) within caps, in
//! the file's own order, and matched to a copy by its GitHub repo.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::sync::LazyLock;

use serde::de::{Deserialize, Deserializer, MapAccess, SeqAccess, Visitor};

use super::changes::check_switch_path;
use super::document::KvDocument;
use super::files::HudFiles;
use super::hud_options::{active_bases, base_target, base_targets};
use super::model::{live_blocks, root_block};
use super::options::{
    Choice, Derived, OptionAction, OptionKind, OptionSource, OptionSpec, OptionValue, SwitchAction,
    ValueMap, is_item_colour,
};
use crate::error::SchemaError;
use crate::fonts::Platform;

/// A schema file is read only up to this size.
const MAX_SCHEMA_BYTES: usize = 1024 * 1024;
/// At most this many controls across all groups.
const MAX_CONTROLS: usize = 2_000;
/// At most this many options in one ComboBox.
const MAX_COMBO_OPTIONS: usize = 64;
/// Any string, key or value, is at most this long.
const MAX_STRING_BYTES: usize = 4 * 1024;

/// The schemas shipped with the app, copied from their upstream project (see `licenses/`).
const BUNDLED: [(&str, &str); 15] = [
    (
        "berryhud",
        include_str!("../../data/hud-schemas/berryhud.json"),
    ),
    ("budhud", include_str!("../../data/hud-schemas/budhud.json")),
    ("darhud", include_str!("../../data/hud-schemas/darhud.json")),
    (
        "eve-plus",
        include_str!("../../data/hud-schemas/eve-plus.json"),
    ),
    (
        "flawhud",
        include_str!("../../data/hud-schemas/flawhud.json"),
    ),
    ("hexhud", include_str!("../../data/hud-schemas/hexhud.json")),
    (
        "hud-fixes",
        include_str!("../../data/hud-schemas/hud-fixes.json"),
    ),
    (
        "hypnotize-hud",
        include_str!("../../data/hud-schemas/hypnotize-hud.json"),
    ),
    ("idhud", include_str!("../../data/hud-schemas/idhud.json")),
    ("kbnhud", include_str!("../../data/hud-schemas/kbnhud.json")),
    (
        "m0rehud-classic",
        include_str!("../../data/hud-schemas/m0rehud-classic.json"),
    ),
    (
        "middle-mann",
        include_str!("../../data/hud-schemas/middle-mann.json"),
    ),
    (
        "rayshud",
        include_str!("../../data/hud-schemas/rayshud.json"),
    ),
    (
        "sunsethud",
        include_str!("../../data/hud-schemas/sunsethud.json"),
    ),
    (
        "zeeshud",
        include_str!("../../data/hud-schemas/zeeshud.json"),
    ),
];

/// A JSON value that keeps object keys in file order; a repeated key keeps its first value.
#[derive(Debug, Clone, PartialEq)]
pub(super) enum SchemaValue {
    Null,
    Bool(bool),
    Number(f64),
    String(String),
    Array(Vec<SchemaValue>),
    Object(Vec<(String, SchemaValue)>),
}

impl SchemaValue {
    /// The value under `key` (exact spelling) when this is an object.
    pub(super) fn get(&self, key: &str) -> Option<&SchemaValue> {
        match self {
            SchemaValue::Object(fields) => fields.iter().find(|(k, _)| k == key).map(|(_, v)| v),
            _ => None,
        }
    }

    pub(super) fn as_str(&self) -> Option<&str> {
        match self {
            SchemaValue::String(s) => Some(s),
            _ => None,
        }
    }

    pub(super) fn as_object(&self) -> Option<&[(String, SchemaValue)]> {
        match self {
            SchemaValue::Object(fields) => Some(fields),
            _ => None,
        }
    }

    pub(super) fn as_array(&self) -> Option<&[SchemaValue]> {
        match self {
            SchemaValue::Array(items) => Some(items),
            _ => None,
        }
    }

    /// Whether any string or key in the value is past the string cap.
    fn too_long(&self) -> bool {
        match self {
            SchemaValue::String(s) => s.len() > MAX_STRING_BYTES,
            SchemaValue::Array(items) => items.iter().any(SchemaValue::too_long),
            SchemaValue::Object(fields) => {
                (fields.iter()).any(|(k, v)| k.len() > MAX_STRING_BYTES || v.too_long())
            }
            SchemaValue::Null | SchemaValue::Bool(_) | SchemaValue::Number(_) => false,
        }
    }
}

impl<'de> Deserialize<'de> for SchemaValue {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<SchemaValue, D::Error> {
        deserializer.deserialize_any(ValueVisitor)
    }
}

/// Builds a [`SchemaValue`], keeping object order without serde_json's `preserve_order`.
struct ValueVisitor;

impl<'de> Visitor<'de> for ValueVisitor {
    type Value = SchemaValue;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a JSON value")
    }
    fn visit_unit<E>(self) -> Result<SchemaValue, E> {
        Ok(SchemaValue::Null)
    }
    fn visit_bool<E>(self, v: bool) -> Result<SchemaValue, E> {
        Ok(SchemaValue::Bool(v))
    }
    fn visit_i64<E>(self, v: i64) -> Result<SchemaValue, E> {
        Ok(SchemaValue::Number(v as f64))
    }
    fn visit_u64<E>(self, v: u64) -> Result<SchemaValue, E> {
        Ok(SchemaValue::Number(v as f64))
    }
    fn visit_f64<E>(self, v: f64) -> Result<SchemaValue, E> {
        Ok(SchemaValue::Number(v))
    }
    fn visit_str<E>(self, v: &str) -> Result<SchemaValue, E> {
        Ok(SchemaValue::String(v.to_owned()))
    }
    fn visit_string<E>(self, v: String) -> Result<SchemaValue, E> {
        Ok(SchemaValue::String(v))
    }
    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<SchemaValue, A::Error> {
        let mut items = Vec::new();
        while let Some(item) = seq.next_element()? {
            items.push(item);
        }
        Ok(SchemaValue::Array(items))
    }
    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<SchemaValue, A::Error> {
        let mut fields: Vec<(String, SchemaValue)> = Vec::new();
        while let Some((key, value)) = map.next_entry::<String, SchemaValue>()? {
            if !fields.iter().any(|(k, _)| *k == key) {
                fields.push((key, value));
            }
        }
        Ok(SchemaValue::Object(fields))
    }
}

/// One control: its group and its fields in file order.
#[derive(Debug, Clone)]
pub(super) struct Control {
    pub(super) group: String,
    pub(super) fields: SchemaValue,
}

/// One HUD option schema: the HUD's repo, its option folders and its grouped controls.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct HudSchema {
    name: String,
    repo: Option<String>,
    customizations: Option<String>,
    enabled: Option<String>,
    groups: Vec<String>,
    controls: Vec<Control>,
}

impl HudSchema {
    /// Strips `//` and `/* */` comments outside strings and a UTF-8 BOM, then parses.
    ///
    /// # Errors
    /// TooLarge past the byte, string or control cap; Malformed for anything that is no JSON
    /// object holding a `Controls` map of control lists.
    pub fn parse(name: &str, bytes: &[u8]) -> Result<HudSchema, SchemaError> {
        if bytes.len() > MAX_SCHEMA_BYTES {
            return Err(SchemaError::TooLarge);
        }
        let malformed = |reason: &str| SchemaError::Malformed {
            reason: reason.to_owned(),
        };
        let text = std::str::from_utf8(bytes).map_err(|_| malformed("not UTF-8"))?;
        let text = strip_comments(text.strip_prefix('\u{feff}').unwrap_or(text));
        let mut de = serde_json::Deserializer::from_str(&text);
        let root = SchemaValue::deserialize(&mut de)
            .and_then(|v| de.end().map(|()| v))
            .map_err(|e| malformed(&e.to_string()))?;
        if root.as_object().is_none() {
            return Err(malformed("the top level is no object"));
        }
        if root.too_long() {
            return Err(SchemaError::TooLarge);
        }
        let Some(groups) = root.get("Controls").and_then(SchemaValue::as_object) else {
            return Err(malformed("no Controls map"));
        };
        let mut names = Vec::new();
        let mut controls = Vec::new();
        for (group, list) in groups {
            let Some(list) = list.as_array() else {
                return Err(malformed("a control group is no list"));
            };
            for control in list {
                if control.as_object().is_none() {
                    return Err(malformed("a control is no object"));
                }
                if controls.len() >= MAX_CONTROLS {
                    return Err(SchemaError::TooLarge);
                }
                let combo = control.get("Options").and_then(SchemaValue::as_array);
                if combo.is_some_and(|o| o.len() > MAX_COMBO_OPTIONS) {
                    return Err(SchemaError::TooLarge);
                }
                controls.push(Control {
                    group: group.clone(),
                    fields: control.clone(),
                });
            }
            names.push(group.clone());
        }
        let text_at = |key: &str| {
            root.get(key)
                .and_then(SchemaValue::as_str)
                .map(str::to_owned)
        };
        let repo = (root.get("Links").and_then(|l| l.get("GitHub")))
            .and_then(SchemaValue::as_str)
            .and_then(normalise_repo);
        Ok(HudSchema {
            name: name.to_owned(),
            repo,
            customizations: text_at("CustomizationsFolder"),
            enabled: text_at("EnabledFolder"),
            groups: names,
            controls,
        })
    }

    /// The file name without `.json`.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The HUD's GitHub repo as a normalised lowercase `owner/name`.
    pub fn repo(&self) -> Option<&str> {
        self.repo.as_deref()
    }

    /// How many controls the schema holds, supported or not.
    pub fn control_count(&self) -> usize {
        self.controls.len()
    }

    /// The control group names, in file order.
    pub fn groups(&self) -> Vec<&str> {
        self.groups.iter().map(String::as_str).collect()
    }
}

/// `text` without `//` line and `/* */` block comments, string contents left whole; a block
/// comment becomes a space so the tokens either side stay apart.
fn strip_comments(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    let mut in_string = false;
    while let Some(c) = chars.next() {
        if in_string {
            out.push(c);
            match c {
                '\\' => out.extend(chars.next()),
                '"' => in_string = false,
                _ => {}
            }
            continue;
        }
        match (c, chars.peek()) {
            ('"', _) => {
                in_string = true;
                out.push(c);
            }
            ('/', Some('/')) => {
                // The newline stays, so line numbers in parse errors hold.
                while chars.peek().is_some_and(|&n| n != '\n') {
                    chars.next();
                }
            }
            ('/', Some('*')) => {
                chars.next();
                let mut last = '\0';
                for n in chars.by_ref() {
                    if last == '*' && n == '/' {
                        break;
                    }
                    last = n;
                }
                out.push(' ');
            }
            _ => out.push(c),
        }
    }
    out
}

/// `owner/name` lowercase from a GitHub URL or `owner/name`; None otherwise.
pub fn normalise_repo(repo: &str) -> Option<String> {
    let lower = repo.trim().to_ascii_lowercase();
    let github = |rest: &str| {
        let rest = rest.strip_prefix("www.").unwrap_or(rest);
        rest.strip_prefix("github.com/").map(str::to_owned)
    };
    // A URL must name GitHub; text with no scheme is a GitHub URL without one or a bare repo.
    let (path, bare) = match ["https://", "http://"]
        .iter()
        .find_map(|s| lower.strip_prefix(s))
    {
        Some(rest) => (github(rest)?, false),
        None => match github(&lower) {
            Some(path) => (path, false),
            None => (lower.clone(), true),
        },
    };
    let mut segments = path.trim_end_matches('/').split('/');
    let (owner, name) = (segments.next()?, segments.next()?);
    let name = name.strip_suffix(".git").unwrap_or(name);
    let ok = |s: &str| {
        !s.is_empty()
            && (s.chars()).all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
    };
    // A bare form is exactly `owner/name`, and an owner with a dot reads as a host.
    let bare_ok = !bare || (segments.next().is_none() && !owner.contains('.'));
    (ok(owner) && ok(name) && bare_ok).then(|| format!("{owner}/{name}"))
}

/// A set of schemas by name.
#[derive(Debug, Clone, Default)]
#[non_exhaustive]
pub struct HudSchemas {
    schemas: BTreeMap<String, HudSchema>,
}

/// The bundled set, parsed once.
static BUNDLED_SET: LazyLock<HudSchemas> = LazyLock::new(|| {
    let schemas = (BUNDLED.iter())
        .filter_map(|(name, text)| HudSchema::parse(name, text.as_bytes()).ok())
        .map(|s| (s.name.clone(), s))
        .collect();
    HudSchemas { schemas }
});

impl HudSchemas {
    /// The schemas shipped with the app.
    pub fn bundled() -> HudSchemas {
        BUNDLED_SET.clone()
    }

    /// The schema names, in name order.
    pub fn names(&self) -> Vec<&str> {
        self.schemas.keys().map(String::as_str).collect()
    }

    /// The schema called `name`.
    pub fn for_name(&self, name: &str) -> Option<&HudSchema> {
        self.schemas.get(name)
    }

    /// The schema whose GitHub link names `repo` (any URL or `owner/name` form).
    pub fn for_repo(&self, repo: &str) -> Option<&HudSchema> {
        let repo = normalise_repo(repo)?;
        self.schemas
            .values()
            .find(|s| s.repo() == Some(repo.as_str()))
    }

    /// This set with `schema` added, replacing one of the same name.
    pub fn with(mut self, schema: HudSchema) -> HudSchemas {
        self.schemas.insert(schema.name.clone(), schema);
        self
    }
}

/// Any of these on a control asks for something this build does not do.
const UNSUPPORTED: [&str; 6] = [
    "WriteCfg",
    "Special",
    "SpecialParameters",
    "ComboDirectories",
    "ComboFiles",
    "Command",
];

/// The controls of one schema compiled against one copy.
#[derive(Debug, Clone, Default)]
#[non_exhaustive]
pub struct SchemaOptions {
    specs: Vec<OptionSpec>,
    left_out: usize,
    misfits: usize,
}

impl SchemaOptions {
    /// The options, in schema order.
    pub fn specs(&self) -> &[OptionSpec] {
        &self.specs
    }

    /// Controls this build does not support, or whose paths break the path rule.
    pub fn left_out(&self) -> usize {
        self.left_out
    }

    /// Supported controls naming a file, block or folder the copy lacks.
    pub fn misfits(&self) -> usize {
        self.misfits
    }
}

/// Why a control or one of its choices gives no option.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Refused {
    /// Unsupported, or a path the path rule refuses.
    LeftOut,
    /// Supported, but the copy lacks what it names.
    Misfit,
}

/// How a `Files` leaf value becomes a key's text, by where it stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Leaves {
    /// A CheckBox: `{"true": a, "false": b}` only.
    Toggle,
    /// A colour or number: `$value` only.
    Value,
    /// A ComboBox option: a literal only.
    Literal,
}

/// Where a key edit lands: file, block path, key and conditional.
type KeyAt = (String, Vec<String>, String, Option<String>);
/// One ComboBox literal: the key it lands on and the text a pick writes.
type Literal = (KeyAt, String);
/// A literal with the key of the choice that writes it.
type ChoiceLiteral = (String, Literal);

impl HudSchema {
    /// The controls this build supports (the mapping table) whose every path fits the copy.
    pub fn options(&self, files: &HudFiles, platform: Platform) -> SchemaOptions {
        let mut fit = Fit {
            files,
            copy: files.copy_paths().into_iter().collect(),
            platform,
            customizations: self.customizations.as_deref(),
            enabled: self.enabled.as_deref(),
            docs: BTreeMap::new(),
        };
        let mut out = SchemaOptions::default();
        let mut seen = BTreeSet::new();
        for control in &self.controls {
            let name = control.fields.get("Name").and_then(SchemaValue::as_str);
            // A nameless control cannot be stored; a repeated name keeps the first.
            let Some(name) = name.filter(|n| !n.is_empty() && seen.insert(n.to_owned())) else {
                out.left_out += 1;
                continue;
            };
            match fit.control(&self.name, name, control) {
                Ok(spec) => out.specs.push(spec),
                Err(Refused::LeftOut) => out.left_out += 1,
                Err(Refused::Misfit) => out.misfits += 1,
            }
        }
        out
    }
}

/// A truthy schema flag: `true` or the text `"true"` in any case.
fn flag(value: Option<&SchemaValue>) -> bool {
    match value {
        Some(SchemaValue::Bool(b)) => *b,
        Some(SchemaValue::String(s)) => s.eq_ignore_ascii_case("true"),
        _ => false,
    }
}

/// A number given as JSON or as text.
fn int(value: Option<&SchemaValue>) -> Option<i32> {
    match value? {
        SchemaValue::Number(n)
            if n.fract() == 0.0 && (f64::from(i32::MIN)..=f64::from(i32::MAX)).contains(n) =>
        {
            Some(*n as i32)
        }
        SchemaValue::String(s) => s.trim().parse().ok(),
        _ => None,
    }
}

/// A scalar as text: a string as is, a number without a fraction, a bool as `true`/`false`.
fn text(value: Option<&SchemaValue>) -> Option<String> {
    match value? {
        SchemaValue::String(s) => Some(s.clone()),
        SchemaValue::Number(n) => Some(n.to_string()),
        SchemaValue::Bool(b) => Some(b.to_string()),
        _ => None,
    }
}

/// `"r g b a"` (alpha optional) as a colour.
fn rgba(text: &str) -> Option<[u8; 4]> {
    let parts: Vec<u8> = (text.split_whitespace())
        .map(|p| p.parse().ok())
        .collect::<Option<_>>()?;
    match parts.as_slice() {
        [r, g, b] => Some([*r, *g, *b, 255]),
        [r, g, b, a] => Some([*r, *g, *b, *a]),
        _ => None,
    }
}

/// A leaf key `key^[$X]` split into the key and its conditional's inner text.
fn split_cond(key: &str) -> (String, Option<String>) {
    match key.split_once("^[") {
        Some((key, cond)) => (key.to_owned(), Some(cond.trim_end_matches(']').to_owned())),
        None => (key.to_owned(), None),
    }
}

/// The copy one schema is compiled against.
struct Fit<'a> {
    files: &'a HudFiles,
    copy: BTreeSet<String>,
    platform: Platform,
    customizations: Option<&'a str>,
    enabled: Option<&'a str>,
    /// Parsed copy files by path; `None` for one missing or unreadable.
    docs: BTreeMap<String, Option<KvDocument>>,
}

impl Fit<'_> {
    /// Whether the copy holds `path` as a file or a folder.
    fn has(&self, path: &str) -> bool {
        let folder = format!("{path}/");
        self.copy.contains(path)
            || (self.copy.range(folder.clone()..).next()).is_some_and(|p| p.starts_with(&folder))
    }

    /// The copy's pristine document at `path`, read once.
    fn doc(&mut self, path: &str) -> Option<&KvDocument> {
        if !self.docs.contains_key(path) {
            let doc = (self.copy.contains(path))
                .then(|| self.files.read_pristine(path).ok().flatten())
                .flatten()
                .and_then(|f| KvDocument::parse(f.bytes()).ok());
            self.docs.insert(path.to_owned(), doc);
        }
        self.docs.get(path).and_then(Option::as_ref)
    }

    fn control(
        &mut self,
        schema: &str,
        name: &str,
        control: &Control,
    ) -> Result<OptionSpec, Refused> {
        let fields = &control.fields;
        if UNSUPPORTED.iter().any(|k| fields.get(k).is_some()) {
            return Err(Refused::LeftOut);
        }
        let kind = fields
            .get("Type")
            .and_then(SchemaValue::as_str)
            .unwrap_or("")
            .to_ascii_lowercase();
        let value = text(fields.get("Value"));
        let (kind, default, actions) = match kind.as_str() {
            "checkbox" => {
                let on = value
                    .as_deref()
                    .is_some_and(|v| v.trim().eq_ignore_ascii_case("true"));
                (
                    OptionKind::Toggle,
                    OptionValue::Bool(on),
                    self.actions(fields, Leaves::Toggle, &mut Vec::new())?,
                )
            }
            "colorpicker" | "color" | "colour" | "colourpicker" => {
                let actions = self.actions(fields, Leaves::Value, &mut Vec::new())?;
                let colour = value
                    .as_deref()
                    .and_then(rgba)
                    .unwrap_or([255, 255, 255, 255]);
                (
                    OptionKind::Colour {
                        derived: derived(fields, &actions),
                    },
                    OptionValue::Rgba(colour),
                    actions,
                )
            }
            "integerupdown" | "number" | "integer" => {
                let min = int(fields.get("Minimum")).unwrap_or(10);
                let max = int(fields.get("Maximum")).unwrap_or(30);
                let step = int(fields.get("Increment")).unwrap_or(2);
                let start = value
                    .as_deref()
                    .and_then(|v| v.trim().parse().ok())
                    .unwrap_or(min);
                let actions = self.actions(fields, Leaves::Value, &mut Vec::new())?;
                (
                    OptionKind::Number { min, max, step },
                    OptionValue::Int(start),
                    actions,
                )
            }
            "combobox" | "dropdown" | "select" => {
                // The schema format ignores a ComboBox's own Files, so such a control is left out.
                if fields.get("Files").is_some() {
                    return Err(Refused::LeftOut);
                }
                let mut actions = self.actions(fields, Leaves::Value, &mut Vec::new())?;
                let (choices, literals) = self.choices(fields)?;
                actions.extend(per_choice(literals));
                if actions.is_empty() && choices.iter().all(|c| c.actions().is_empty()) {
                    return Err(Refused::LeftOut);
                }
                let key = value.filter(|v| choices.iter().any(|c| c.key() == v));
                let key = key.or_else(|| choices.first().map(|c| c.key().to_owned()));
                (
                    OptionKind::Choice(choices),
                    OptionValue::Pick(key.unwrap_or_default()),
                    actions,
                )
            }
            _ => return Err(Refused::LeftOut),
        };
        // An option that writes nothing is no option; a ComboBox was checked with its choices.
        if actions.is_empty() && !matches!(kind, OptionKind::Choice(_)) {
            return Err(Refused::LeftOut);
        }
        let label = fields
            .get("Label")
            .and_then(SchemaValue::as_str)
            .unwrap_or(name);
        let tooltip = (fields.get("ToolTip").or_else(|| fields.get("Tooltip")))
            .and_then(SchemaValue::as_str)
            .map(str::to_owned);
        let spec = OptionSpec::hud(
            &format!("e:{name}"),
            label,
            &control.group,
            kind,
            default,
            actions,
            flag(fields.get("Restart")),
            OptionSource::HudSchema {
                schema: schema.to_owned(),
            },
        );
        Ok(spec.with_tooltip(tooltip))
    }

    /// A ComboBox's choices with the literals each writes; refused choices are dropped, and
    /// none left refuses the control.
    fn choices(
        &mut self,
        fields: &SchemaValue,
    ) -> Result<(Vec<Choice>, Vec<ChoiceLiteral>), Refused> {
        let mut choices: Vec<Choice> = Vec::new();
        let mut literals = Vec::new();
        let mut worst = Refused::LeftOut;
        for option in fields
            .get("Options")
            .and_then(SchemaValue::as_array)
            .unwrap_or_default()
        {
            let Some(key) = text(option.get("Value")) else {
                continue;
            };
            if choices.iter().any(|c| c.key() == key) {
                continue;
            }
            let mut own = Vec::new();
            match self.actions(option, Leaves::Literal, &mut own) {
                Ok(actions) => {
                    let label = option
                        .get("Label")
                        .and_then(SchemaValue::as_str)
                        .unwrap_or(&key);
                    choices.push(Choice::new(&key, label, actions));
                    literals.extend(own.into_iter().map(|l| (key.clone(), l)));
                }
                Err(Refused::Misfit) => worst = Refused::Misfit,
                Err(Refused::LeftOut) => {}
            }
        }
        if choices.is_empty() {
            return Err(worst);
        }
        Ok((choices, literals))
    }

    /// The actions of a control or ComboBox option, in field order; literals go to `literals`.
    /// A path the rule refuses leaves the whole thing out, before any misfit counts.
    fn actions(
        &mut self,
        fields: &SchemaValue,
        leaves: Leaves,
        literals: &mut Vec<Literal>,
    ) -> Result<Vec<OptionAction>, Refused> {
        let mut actions = Vec::new();
        let mut misfit = false;
        for (key, value) in fields.as_object().unwrap_or_default() {
            let got = match key.as_str() {
                "Files" => self.files_actions(value, leaves, literals),
                "FileName" => self.file_name(value),
                "RenameFile" => self.rename(value),
                "WriteFile" => self.write_file(value),
                _ => continue,
            };
            match got {
                Ok(more) => actions.extend(more),
                Err(Refused::Misfit) => misfit = true,
                Err(Refused::LeftOut) => return Err(Refused::LeftOut),
            }
        }
        match misfit {
            true => Err(Refused::Misfit),
            false => Ok(actions),
        }
    }

    /// `Files`: per `.res` file, a key edit per leaf (or a `#base` swap).
    fn files_actions(
        &mut self,
        files: &SchemaValue,
        leaves: Leaves,
        literals: &mut Vec<Literal>,
    ) -> Result<Vec<OptionAction>, Refused> {
        let Some(files) = files.as_object() else {
            return Err(Refused::LeftOut);
        };
        let mut actions = Vec::new();
        let mut misfit = false;
        for (path, content) in files {
            let file = check_switch_path(path).ok_or(Refused::LeftOut)?;
            let Some(content) = content.as_object() else {
                return Err(Refused::LeftOut);
            };
            if let Some((_, targets)) = content
                .iter()
                .find(|(k, _)| k.eq_ignore_ascii_case("#base"))
            {
                match self.base_action(&file, targets) {
                    Ok(action) => actions.push(action),
                    Err(Refused::Misfit) => misfit = true,
                    Err(Refused::LeftOut) => return Err(Refused::LeftOut),
                }
                continue;
            }
            if !file.ends_with(".res") {
                return Err(Refused::LeftOut);
            }
            let mut sets = Vec::new();
            for (k, v) in content {
                collect_leaves(k, v, &mut Vec::new(), leaves, &mut sets)?;
            }
            let Some(root) = self
                .doc(&file)
                .and_then(root_block)
                .map(|r| r.key().to_owned())
            else {
                misfit = true;
                continue;
            };
            for (mut block, key, cond, value) in sets {
                // A first level naming the root block is the root itself.
                if block.first().is_some_and(|b| b.eq_ignore_ascii_case(&root)) {
                    block.remove(0);
                }
                let path: Vec<&str> = std::iter::once(root.as_str())
                    .chain(block.iter().map(String::as_str))
                    .collect();
                let platform = self.platform;
                if self
                    .doc(&file)
                    .is_none_or(|doc| live_blocks(doc, &path, platform).is_empty())
                {
                    misfit = true;
                    continue;
                }
                let value = match value {
                    Leaf::Map(map) => map,
                    Leaf::Literal(text) => {
                        literals.push(((file.clone(), block, key, cond), text));
                        continue;
                    }
                };
                actions.push(OptionAction::SetKey {
                    file: file.clone(),
                    block,
                    key,
                    cond,
                    value,
                });
            }
        }
        match misfit {
            true => Err(Refused::Misfit),
            false => Ok(actions),
        }
    }

    /// `FileName`: the file moves from the customizations folder to the enabled one.
    fn file_name(&self, value: &SchemaValue) -> Result<Vec<OptionAction>, Refused> {
        let name = value.as_str().ok_or(Refused::LeftOut)?;
        let (Some(from), Some(to)) = (self.customizations, self.enabled) else {
            return Err(Refused::LeftOut);
        };
        self.moved(&format!("{from}/{name}"), &format!("{to}/{name}"))
    }

    /// `RenameFile`: `OldName` moves to `NewName`, a trailing `/` naming a folder.
    fn rename(&self, value: &SchemaValue) -> Result<Vec<OptionAction>, Refused> {
        let side = |k: &str| {
            value
                .get(k)
                .and_then(SchemaValue::as_str)
                .ok_or(Refused::LeftOut)
        };
        self.moved(side("OldName")?, side("NewName")?)
    }

    /// A move both of whose paths pass the rule, fitting when either end is in the copy.
    fn moved(&self, from: &str, to: &str) -> Result<Vec<OptionAction>, Refused> {
        let from = check_switch_path(from).ok_or(Refused::LeftOut)?;
        let to = check_switch_path(to).ok_or(Refused::LeftOut)?;
        if !self.has(&from) && !self.has(&to) {
            return Err(Refused::Misfit);
        }
        Ok(vec![OptionAction::Switch(SwitchAction::Move { from, to })])
    }

    /// `#base` in `Files`: a `[{"true": a, "false": b}]` list swaps a for b; a plain string
    /// turns its line on and the file's own lines back on when off.
    fn base_action(&mut self, file: &str, targets: &SchemaValue) -> Result<OptionAction, Refused> {
        let (on, off) = match targets {
            SchemaValue::String(target) => {
                let own = self.own_bases(file).ok_or(Refused::Misfit)?;
                (vec![target.clone()], own)
            }
            SchemaValue::Array(items) => {
                let (mut on, mut off) = (Vec::new(), Vec::new());
                for item in items {
                    let side =
                        |k: &str| item.get(k).and_then(SchemaValue::as_str).map(str::to_owned);
                    let (Some(a), Some(b)) = (side("true"), side("false")) else {
                        return Err(Refused::LeftOut);
                    };
                    on.push(a);
                    off.push(b);
                }
                (on, off)
            }
            _ => return Err(Refused::LeftOut),
        };
        self.base_lines(file, on, off)
    }

    /// `WriteFile` whose texts are nothing but `#base` lines, as a `#base` swap.
    fn write_file(&mut self, value: &SchemaValue) -> Result<Vec<OptionAction>, Refused> {
        let side = |k: &str| value.get(k).and_then(SchemaValue::as_str);
        let file = side("FileName")
            .and_then(check_switch_path)
            .ok_or(Refused::LeftOut)?;
        let on = side("TrueText")
            .and_then(base_targets)
            .ok_or(Refused::LeftOut)?;
        let off = side("FalseText")
            .and_then(base_targets)
            .ok_or(Refused::LeftOut)?;
        Ok(vec![self.base_lines(&file, on, off)?])
    }

    /// A `#base` swap on copy file `file`, every target passing the path rule and in the copy.
    fn base_lines(
        &mut self,
        file: &str,
        on: Vec<String>,
        off: Vec<String>,
    ) -> Result<OptionAction, Refused> {
        let mut misfit = !self.copy.contains(file);
        for target in on.iter().chain(&off) {
            let landing = base_target(self.files, file, target).ok_or(Refused::LeftOut)?;
            misfit |= !self.copy.contains(&landing);
        }
        if misfit {
            return Err(Refused::Misfit);
        }
        let file = file.to_owned();
        Ok(OptionAction::Switch(SwitchAction::BaseLines {
            file,
            on,
            off,
        }))
    }

    /// The active `#base` targets of copy file `file` as written; `None` when it is not there.
    fn own_bases(&self, file: &str) -> Option<Vec<String>> {
        let bytes = (self.copy.contains(file))
            .then(|| self.files.read_pristine(file).ok().flatten())
            .flatten()?;
        Some(active_bases(&String::from_utf8_lossy(bytes.bytes())))
    }
}

/// A `Files` leaf: a value map, or a ComboBox option's literal.
enum Leaf {
    Map(ValueMap),
    Literal(String),
}

/// One key edit read from a `Files` map: block path, key, conditional and value.
type LeafSet = (Vec<String>, String, Option<String>, Leaf);

/// The leaves under `key: value` at `block`, as `leaves` allows; anything else is left out.
fn collect_leaves(
    key: &str,
    value: &SchemaValue,
    block: &mut Vec<String>,
    leaves: Leaves,
    out: &mut Vec<LeafSet>,
) -> Result<(), Refused> {
    let is_switch = |fields: &[(String, SchemaValue)]| {
        !fields.is_empty() && fields.iter().all(|(k, _)| k == "true" || k == "false")
    };
    let leaf = match (value, leaves) {
        (SchemaValue::Object(fields), _) if !is_switch(fields) => {
            block.push(key.to_owned());
            for (k, v) in fields {
                collect_leaves(k, v, block, leaves, out)?;
            }
            block.pop();
            return Ok(());
        }
        (SchemaValue::Object(_), Leaves::Toggle) => {
            let side = |k: &str| text(value.get(k));
            let on = side("true").ok_or(Refused::LeftOut)?;
            Leaf::Map(ValueMap::OnOff {
                on,
                off: side("false"),
            })
        }
        (SchemaValue::String(s), Leaves::Value) if s == "$value" => Leaf::Map(ValueMap::Value),
        (SchemaValue::String(s), Leaves::Literal) if !s.contains('$') => Leaf::Literal(s.clone()),
        _ => return Err(Refused::LeftOut),
    };
    let (key, cond) = split_cond(key);
    out.push((block.clone(), key, cond, leaf));
    Ok(())
}

/// The ComboBox literals gathered per (file, block, key, cond) into one per-choice edit each.
fn per_choice(literals: Vec<ChoiceLiteral>) -> Vec<OptionAction> {
    let mut grouped: Vec<(KeyAt, Vec<(String, String)>)> = Vec::new();
    for (choice, (at, text)) in literals {
        match grouped.iter_mut().find(|(g, _)| *g == at) {
            Some((_, pairs)) => pairs.push((choice, text)),
            None => grouped.push((at, vec![(choice, text)])),
        }
    }
    (grouped.into_iter())
        .map(|((file, block, key, cond), pairs)| OptionAction::SetKey {
            file,
            block,
            key,
            cond,
            value: ValueMap::PerChoice(pairs),
        })
        .collect()
}

/// The derived colours a ColorPicker writes: the item-rarity pair for an item key, then
/// `Pulse`, or else `Shadow`, as the schema format writes only the pulse when both are set.
fn derived(fields: &SchemaValue, actions: &[OptionAction]) -> Vec<Derived> {
    let item = actions.iter().any(|a| match a {
        OptionAction::SetKey { key, .. } => is_item_colour(key),
        OptionAction::Switch(_) => false,
    });
    let mut out = Vec::new();
    if item {
        out.extend([Derived::Dimm, Derived::GreyedOut]);
    }
    if flag(fields.get("Pulse")) {
        out.push(Derived::Pulse);
    } else if flag(fields.get("Shadow")) {
        out.push(Derived::Shadow);
    }
    out
}
