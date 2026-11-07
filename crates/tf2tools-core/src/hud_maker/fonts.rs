//! Per-part fonts and added font files. A part's font edit goes into a clone of the font it
//! uses, `TF2Tools_<n>`, so no other part that shares the original font changes with it.

use super::document::{KvDocument, KvEntry, writable};
use super::edits::{code_owns, set_lines};
use super::expr::atoi;
use super::model::{SCHEME_FILE, root_block};
use super::{
    AddBlock, AssetFile, BlockLine, Change, ClassSchema, EditRefused, HudModel, KeyLine, PartKey,
    PropertyKind, SetKey, check_hud_path,
};
use crate::fonts::names;

/// The prefix of the fonts the maker clones for one part.
pub const CLONE_PREFIX: &str = "TF2Tools_";

/// The largest font height the maker writes, in scheme units.
const MAX_SIZE: i32 = 256;

/// Where an added font file installs, and where its kept copy sits in the HUD folder.
pub(crate) const FONT_DIR: &str = "resource/fonts/tf2tools";
pub(crate) const KEPT_DIR: &str = ".tf2tools/fonts";
/// Where a made HUD keeps the images it borrowed, at the same path they have in the game.
pub(crate) const BORROWED_DIR: &str = ".tf2tools/borrowed";
/// Where a made HUD keeps the pictures the player picked, one folder per content id.
pub(crate) const IMAGES_DIR: &str = ".tf2tools/images";

/// The longest file name stem the maker writes for an added font.
const MAX_STEM: usize = 64;

/// One change to a part's font.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum FontEdit {
    /// The font family (`name`): any non-empty text `writable` accepts, so the family of a font
    /// file added in the same action needs no scheme entry first.
    Family(String),
    /// The height in scheme units (`tall`) for the variant `screen_tall` uses, 1 to 256.
    Size(i32),
    /// `outline` on or off.
    Outline(bool),
    /// `dropshadow` on or off.
    Shadow(bool),
}

impl FontEdit {
    /// The variant key the edit writes.
    fn key(&self) -> &'static str {
        match self {
            FontEdit::Family(_) => "name",
            FontEdit::Size(_) => "tall",
            FontEdit::Outline(_) => "outline",
            FontEdit::Shadow(_) => "dropshadow",
        }
    }

    fn is_valid(&self) -> bool {
        match self {
            FontEdit::Family(family) => !family.is_empty() && writable("name", family, None),
            FontEdit::Size(size) => (1..=MAX_SIZE).contains(size),
            FontEdit::Outline(_) | FontEdit::Shadow(_) => true,
        }
    }

    /// The text for one line of the edited key (`None` when the variant lacks it). A size
    /// scales by the ratio of the new size to `active`, the height the current screen uses, so
    /// a font's `yres` steps stay in proportion; without an active height it is written as is.
    fn text(&self, old: Option<&str>, active: i32) -> String {
        match self {
            FontEdit::Family(family) => family.clone(),
            FontEdit::Size(size) => match old.filter(|_| active > 0) {
                Some(old) => scale(atoi(old.as_bytes()), *size, active).to_string(),
                None => size.to_string(),
            },
            FontEdit::Outline(on) | FontEdit::Shadow(on) => if *on { "1" } else { "0" }.to_owned(),
        }
    }
}

impl HudModel {
    /// Edits the font the part's Font row `key` names (its value, else the row's default).
    /// A `TF2Tools_*` font with a block in clientscheme.res gets one `Set` per line in each
    /// variant (appended where the variant lacks the key). Any other font is cloned: an
    /// `AddBlock` of `TF2Tools_<n>` under the own Fonts (created in a copy when only a `#base`
    /// file has one) holding the font's own lines, one `AddBlock` per variant block with every
    /// line copied from the definition merged over clientscheme.res and its `#base` chain
    /// (conditionals kept, the edit applied, the edited key appended where missing), then the
    /// part's `key` set to the clone. Refuses, in order: UnknownPart; NotOffered for no Font row
    /// `key` at stage 2 or earlier; ReadOnly when the part is unreadable or neither
    /// clientscheme.res nor (in a copy) its `#base` chain defines the font;
    /// BadValue for an empty or unwritable family or a size outside 1..=256; ReadOnly when a
    /// clone would need an unwritable line or a variant holds a nested block.
    pub fn edit_font(
        &self,
        part: &PartKey,
        key: &str,
        edit: FontEdit,
        screen_tall: u32,
        group: u32,
    ) -> Result<Vec<Change>, EditRefused> {
        let part = self.part(part).ok_or(EditRefused::UnknownPart)?;
        let spec = ClassSchema::for_part(part)
            .get(key)
            .filter(|s| s.kind() == PropertyKind::Font && s.stage() <= 2)
            .filter(|s| !code_owns(part, s.key()))
            .ok_or(EditRefused::NotOffered)?;
        if !part.is_readable() {
            return Err(EditRefused::ReadOnly);
        }
        let name = (part.value(spec.key()).map(KeyLine::text))
            .or(spec.default())
            .ok_or(EditRefused::ReadOnly)?;
        let doc = self.document(SCHEME_FILE).ok_or(EditRefused::ReadOnly)?;
        let merged = MergedFont::find(self, doc, name).ok_or(EditRefused::ReadOnly)?;
        if !edit.is_valid() {
            return Err(EditRefused::BadValue);
        }
        let active = self
            .scheme()
            .font(name, screen_tall)
            .map_or(0, |f| f.tall());
        if is_clone(name)
            && let Some(own) = OwnFont::find(doc, name)
        {
            return Ok(own.edit_in_place(&edit, active, group));
        }
        let n = next_clone_number(self, doc).ok_or(EditRefused::ReadOnly)?;
        let clone = format!("{CLONE_PREFIX}{n}");
        let mut changes = merged.clone_as(&clone, &edit, active, group)?;
        changes.extend(set_lines(part, spec.key(), group, |_| clone.clone()));
        Ok(changes)
    }
}

impl HudModel {
    /// The changes that make font `own` a copy of font `name` at `size` on a `screen_tall`
    /// screen, its other heights in proportion, or that resize `own` in place when the HUD
    /// already defines it; `None` when no scheme file defines `name` or a copy is refused.
    pub(super) fn sized_font(
        &self,
        name: &str,
        own: &str,
        size: i32,
        screen_tall: u32,
    ) -> Option<Vec<Change>> {
        let edit = FontEdit::Size(size);
        let doc = self.document(SCHEME_FILE)?;
        if !edit.is_valid() {
            return None;
        }
        if let Some(found) = OwnFont::find(doc, own) {
            let active = self.scheme().font(own, screen_tall).map_or(0, |f| f.tall());
            return Some(found.edit_in_place(&edit, active, 0));
        }
        let active = self
            .scheme()
            .font(name, screen_tall)
            .map_or(0, |f| f.tall());
        let merged = MergedFont::find(self, doc, name)?;
        merged.clone_as(own, &edit, active, 0).ok()
    }
}

/// The primary family (name ID 1, a Windows-platform record first) of the first face of a
/// TrueType or OpenType file, at most 64 MiB; `None` for anything else. Never panics.
pub fn font_family(bytes: &[u8]) -> Option<String> {
    names::family_in(bytes)
}

// The Windows families the stock scheme names, each with the open families that stand in for
// it where it is not installed, in the order the game's own Linux font list suggests.
const STAND_INS: [(&str, &[&str]); 6] = [
    (
        "Verdana",
        &["DejaVu Sans", "Bitstream Vera Sans", "Noto Sans"],
    ),
    ("Tahoma", &["DejaVu Sans", "Noto Sans"]),
    ("Trebuchet MS", &["DejaVu Sans", "Noto Sans"]),
    ("Arial", &["Liberation Sans", "Arimo", "Noto Sans"]),
    ("Arial Black", &["DejaVu Sans", "Liberation Sans"]),
    (
        "Courier New",
        &["Liberation Mono", "Cousine", "DejaVu Sans Mono"],
    ),
];

/// The open families that draw `family` (matched ignoring ASCII case) where it is not
/// installed, best first; empty for a family with no stand-in.
pub fn stand_in_families(family: &str) -> &'static [&'static str] {
    STAND_INS
        .iter()
        .find(|(windows, _)| windows.eq_ignore_ascii_case(family))
        .map_or(&[], |&(_, open)| open)
}

/// Every family name (IDs 1 and 16, all faces, platforms and languages) the file answers to,
/// deduplicated ignoring case, [`font_family`] first; empty for anything else. A scheme name
/// finds the file when it equals any of them ignoring case. Never panics.
pub fn font_families(bytes: &[u8]) -> Vec<String> {
    names::families_in(bytes)
}

/// The family of `bytes` when they are a font a HUD may add: readable, every name decodable
/// and none protected, the rule font install applies. Cheap next to a replay, so callers can
/// refuse garbage early.
pub(super) fn addable_family(bytes: &[u8]) -> Option<String> {
    let family = font_family(bytes)?;
    let found = names::names_in(bytes).ok()?;
    (!found.undecoded && !names::is_protected(&found.names)).then_some(family)
}

/// A font file the player added: its family and where it goes.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct FontFile {
    family: String,
    path: String,
    source: String,
}

impl FontFile {
    /// The family name the file's first face declares.
    pub fn family(&self) -> &str {
        &self.family
    }

    /// The game path it installs at, `resource/fonts/tf2tools/<name>`.
    pub fn path(&self) -> &str {
        &self.path
    }

    /// The kept copy's path in the HUD folder, `.tf2tools/fonts/<name>`, which is the asset's
    /// `source` and the key its bytes travel under.
    pub fn source(&self) -> &str {
        &self.source
    }
}

impl HudModel {
    /// The changes that add `bytes` (picked as `file_name`) as a HUD font: an `Asset`, then the
    /// `CustomFontFiles` entry (a `Set` of slot `next_font_slot`, or an `AddBlock` of the block
    /// holding it), in clientscheme.res itself even when a `#base` file holds the font list.
    /// Refuses with BadValue for a name not ending `.ttf`/`.otf`, bytes with no first-face
    /// family or over 64 MiB, or a protected family; ReadOnly when clientscheme.res did not
    /// load, neither it nor its `#base` chain has a root block, or no slot is left.
    pub fn add_font_file(
        &self,
        file_name: &str,
        bytes: &[u8],
        group: u32,
    ) -> Result<(FontFile, Vec<Change>), EditRefused> {
        let last = file_name.rsplit(['/', '\\']).next().unwrap_or_default();
        let (stem, ext) = split_font_ext(last).ok_or(EditRefused::BadValue)?;
        let family = addable_family(bytes).ok_or(EditRefused::BadValue)?;
        let doc = self.document(SCHEME_FILE).ok_or(EditRefused::ReadOnly)?;
        // A stub names its root after its base's, the block a set there creates first.
        let root = self.merged_roots(SCHEME_FILE).first().map(|r| r.key());
        let root = root.ok_or(EditRefused::ReadOnly)?;
        let slot = match self.scheme().max_font_slot() {
            Some(max) => max.checked_add(1).ok_or(EditRefused::ReadOnly)?,
            None => 1,
        };
        let file = self.unused_font_name(&slug(stem), ext);
        let path = format!("{FONT_DIR}/{file}");
        let source = format!("{KEPT_DIR}/{file}");
        // The slug leaves only safe characters, so this guards the rule rather than the input.
        if check_hud_path(&path).is_none() || check_hud_path(&source).is_none() {
            return Err(EditRefused::BadValue);
        }
        let slot = slot.to_string();
        let entry = match (doc.find(&[root, "CustomFontFiles"]).into_iter())
            .rfind(|e| e.text().is_none())
        {
            Some(block) => Change::set(
                group,
                SetKey::new(SCHEME_FILE, &[root, block.key()], &slot, None, None, &path),
            ),
            None => Change::add_block(
                group,
                AddBlock::new(
                    SCHEME_FILE,
                    &[root],
                    "CustomFontFiles",
                    None,
                    vec![BlockLine::new(&slot, &path, None)],
                ),
            ),
        };
        let changes = vec![Change::asset(group, AssetFile::new(&path, &source)), entry];
        Ok((
            FontFile {
                family,
                path,
                source,
            },
            changes,
        ))
    }

    /// `<stem>.<ext>`, or `<stem>-<n>.<ext>` with the lowest `n` from 2 that no applied asset's
    /// path or kept copy, and no font file the loaded HUD ships, already ends in.
    fn unused_font_name(&self, stem: &str, ext: &str) -> String {
        let taken = |name: &str| {
            let shipped = self.shipped_fonts().iter().map(String::as_str);
            (self.assets().iter())
                .flat_map(|(path, source)| [path.as_str(), source.as_str()])
                .filter_map(|p| p.rsplit('/').next())
                .chain(shipped)
                .any(|last| last.eq_ignore_ascii_case(name))
        };
        // Each asset or shipped file takes one name, so a free one is found within that many + 1.
        std::iter::once(format!("{stem}.{ext}"))
            .chain((2u64..).map(|n| format!("{stem}-{n}.{ext}")))
            .find(|name| !taken(name))
            .unwrap_or_default()
    }
}

/// The stem and lowercase extension of a `.ttf` or `.otf` name, the suffix matched ignoring case.
fn split_font_ext(name: &str) -> Option<(&str, &'static str)> {
    let cut = name.len().checked_sub(4)?;
    let suffix = name.get(cut..)?;
    let ext = [".ttf", ".otf"]
        .into_iter()
        .find(|e| suffix.eq_ignore_ascii_case(e))?;
    Some((&name[..cut], &ext[1..]))
}

/// A stem the game path rule accepts: ASCII letters and digits lowercased, `-` and `_` kept,
/// any other char `_`, at most 64 chars, `font` when empty and a `_` after a device name.
fn slug(stem: &str) -> String {
    let mut out: String = (stem.chars())
        .map(|c| match c {
            'a'..='z' | '0'..='9' | '-' | '_' => c,
            'A'..='Z' => c.to_ascii_lowercase(),
            _ => '_',
        })
        .take(MAX_STEM)
        .collect();
    if out.is_empty() {
        out.push_str("font");
    }
    let device = matches!(out.as_str(), "con" | "prn" | "aux" | "nul")
        || (out.len() == 4
            && (out.starts_with("com") || out.starts_with("lpt"))
            && out.as_bytes()[3].is_ascii_digit());
    if device {
        out.push('_');
    }
    out
}

/// A font's block in clientscheme.res itself, with the spellings its paths are written in.
struct OwnFont<'a> {
    root: &'a str,
    fonts: &'a str,
    font: &'a KvEntry,
}

impl<'a> OwnFont<'a> {
    /// The last block of `name` under the root's Fonts, the one a `Set` on its path resolves to.
    fn find(doc: &'a KvDocument, name: &str) -> Option<Self> {
        let root = doc.entries().iter().find(|e| e.text().is_none())?.key();
        let is_block = |e: &&KvEntry| e.text().is_none();
        let fonts = doc.find(&[root, "Fonts"]).into_iter().rfind(is_block)?;
        let font = doc
            .find(&[root, "Fonts", name])
            .into_iter()
            .rfind(is_block)?;
        Some(OwnFont {
            root,
            fonts: fonts.key(),
            font,
        })
    }

    /// The variant blocks in document order; nested blocks inside a variant are not variants.
    fn variants(&self) -> impl Iterator<Item = &'a KvEntry> {
        self.font.children().iter().filter(|e| e.text().is_none())
    }

    /// One `Set` per line of the edited key in each variant, or one appended line.
    fn edit_in_place(&self, edit: &FontEdit, active: i32, group: u32) -> Vec<Change> {
        let mut changes = Vec::new();
        for variant in self.variants() {
            let path = [self.root, self.fonts, self.font.key(), variant.key()];
            let lines: Vec<&KvEntry> = (variant.children().iter())
                .filter(|l| l.text().is_some() && l.key().eq_ignore_ascii_case(edit.key()))
                .collect();
            if lines.is_empty() {
                let set = SetKey::new(
                    SCHEME_FILE,
                    &path,
                    edit.key(),
                    None,
                    None,
                    &edit.text(None, active),
                );
                changes.push(Change::set(group, set));
            }
            for line in lines {
                let old = line.text();
                let new = edit.text(old, active);
                let set = SetKey::new(SCHEME_FILE, &path, line.key(), line.cond(), old, &new);
                changes.push(Change::set(group, set));
            }
        }
        changes
    }
}

/// A font's definition as the game merges it: the last block of the name in clientscheme.res,
/// then in each `#base` file in read order, where a key or variant an earlier file gave wins
/// and the rest is appended. Own and base entries keep their conditionals.
struct MergedFont<'a> {
    root: &'a str,
    fonts: &'a str,
    lines: Vec<&'a KvEntry>,
    variants: Vec<Variant<'a>>,
}

/// One variant block of a merged font: the block that named it first and its merged entries.
struct Variant<'a> {
    head: &'a KvEntry,
    entries: Vec<&'a KvEntry>,
}

impl<'a> MergedFont<'a> {
    /// The merged definition of `name`, written under `doc`'s root (else the first base's) and
    /// its last Fonts block (else a `Fonts` an edit creates); `None` when no file defines it.
    fn find(model: &'a HudModel, doc: &'a KvDocument, name: &str) -> Option<Self> {
        // Merged roots list the bases lowest rank first; the merge wants the winner first.
        let mut roots = model.merged_roots(SCHEME_FILE);
        let root = roots.first()?.key();
        let own = usize::from(root_block(doc).is_some());
        roots[own..].reverse();
        let is_block = |e: &&KvEntry| e.text().is_none();
        let fonts =
            (doc.find(&[root, "Fonts"]).into_iter().rfind(is_block)).map_or("Fonts", KvEntry::key);
        let blocks = roots.into_iter().filter_map(|root| {
            (root.children().iter())
                .filter(|e| is_block(e) && e.key().eq_ignore_ascii_case("Fonts"))
                .flat_map(KvEntry::children)
                .rfind(|e| is_block(e) && e.key().eq_ignore_ascii_case(name))
        });
        let mut font = MergedFont {
            root,
            fonts,
            lines: Vec::new(),
            variants: Vec::new(),
        };
        let mut found = false;
        for block in blocks {
            found = true;
            // Only what earlier files gave outranks this file; its own duplicates all stay.
            let (lines, variants) = (font.lines.len(), font.variants.len());
            for entry in block.children() {
                let same = |e: &KvEntry| e.key().eq_ignore_ascii_case(entry.key());
                if entry.text().is_some() {
                    if !font.lines[..lines].iter().any(|l| same(l)) {
                        font.lines.push(entry);
                    }
                    continue;
                }
                match font.variants[..variants].iter().rposition(|v| same(v.head)) {
                    Some(at) => font.variants[at].merge(entry),
                    None => font.variants.push(Variant {
                        head: entry,
                        entries: entry.children().iter().collect(),
                    }),
                }
            }
        }
        found.then_some(font)
    }

    /// The `AddBlock`s that copy this font as `clone` with the edit applied, the lines directly
    /// under the font included. ReadOnly when a copied line could not be written back or a
    /// variant holds a nested block (which an `AddBlock` cannot carry), so a clone is never
    /// partly made.
    fn clone_as(
        &self,
        clone: &str,
        edit: &FontEdit,
        active: i32,
        group: u32,
    ) -> Result<Vec<Change>, EditRefused> {
        let mut own_lines = Vec::new();
        for &line in &self.lines {
            let Some(value) = line.text() else { continue };
            if !writable(line.key(), value, line.cond()) {
                return Err(EditRefused::ReadOnly);
            }
            own_lines.push(BlockLine::new(line.key(), value, line.cond()));
        }
        let parent = [self.root, self.fonts];
        let font = AddBlock::new(SCHEME_FILE, &parent, clone, None, own_lines);
        let mut changes = vec![Change::add_block(group, font)];
        let path = [self.root, self.fonts, clone];
        for Variant {
            head: variant,
            entries,
        } in &self.variants
        {
            let nested = entries.iter().any(|l| l.text().is_none());
            if nested || !writable(variant.key(), "", variant.cond()) {
                return Err(EditRefused::ReadOnly);
            }
            let mut lines = Vec::new();
            let mut edited = false;
            for line in entries {
                let Some(old) = line.text() else { continue };
                let value = match line.key().eq_ignore_ascii_case(edit.key()) {
                    true => {
                        edited = true;
                        edit.text(Some(old), active)
                    }
                    false => old.to_owned(),
                };
                if !writable(line.key(), &value, line.cond()) {
                    return Err(EditRefused::ReadOnly);
                }
                lines.push(BlockLine::new(line.key(), &value, line.cond()));
            }
            if !edited {
                lines.push(BlockLine::new(edit.key(), &edit.text(None, active), None));
            }
            let add = AddBlock::new(SCHEME_FILE, &path, variant.key(), variant.cond(), lines);
            changes.push(Change::add_block(group, add));
        }
        Ok(changes)
    }
}

impl<'a> Variant<'a> {
    /// Appends the entries of a later file's block of this variant whose keys it lacks.
    fn merge(&mut self, later: &'a KvEntry) {
        let had = self.entries.len();
        for entry in later.children() {
            let same = |e: &&KvEntry| e.key().eq_ignore_ascii_case(entry.key());
            if !self.entries[..had].iter().any(same) {
                self.entries.push(entry);
            }
        }
    }
}

/// Whether a font name is one of the maker's clones, its prefix compared ignoring ASCII case.
pub(super) fn is_clone(name: &str) -> bool {
    name.get(..CLONE_PREFIX.len())
        .is_some_and(|p| p.eq_ignore_ascii_case(CLONE_PREFIX))
}

/// One above the highest clone number in `model`'s scheme and in `doc`'s own Fonts blocks
/// (which keep other platforms' fonts the scheme drops); `None` when that overflows.
pub(super) fn next_clone_number(model: &HudModel, doc: &KvDocument) -> Option<u64> {
    let own_names = (doc.entries().iter().filter(|e| e.text().is_none()))
        .flat_map(|root| root.children())
        .filter(|e| e.text().is_none() && e.key().eq_ignore_ascii_case("Fonts"))
        .flat_map(|fonts| fonts.children())
        .map(KvEntry::key);
    (model.scheme().font_names().into_iter())
        .chain(own_names)
        .filter_map(clone_number)
        .max()
        .map_or(Some(1), |max| max.checked_add(1))
}

/// The `n` of a `TF2Tools_<digits>` name.
pub(super) fn clone_number(name: &str) -> Option<u64> {
    let digits = name.get(CLONE_PREFIX.len()..).filter(|_| is_clone(name))?;
    (!digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit()))
        .then(|| digits.parse().ok())
        .flatten()
}

/// `old * size / active` rounded half up, at least 1; only called with `active > 0`.
fn scale(old: i32, size: i32, active: i32) -> i32 {
    let (old, size, active) = (i64::from(old), i64::from(size), i64::from(active));
    let scaled = (2 * old * size + active).div_euclid(2 * active);
    scaled.clamp(1, i64::from(i32::MAX)) as i32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sizes_round_half_up_and_stay_in_range() {
        assert_eq!(scale(15, 1, 2), 8);
        assert_eq!(scale(14, 32, 16), 28);
        assert_eq!(scale(-5, 10, 3), 1);
        assert_eq!(scale(i32::MAX, MAX_SIZE, 1), i32::MAX);
    }

    #[test]
    fn slugs_are_safe_file_stems() {
        assert_eq!(slug("Big Numbers"), "big_numbers");
        assert_eq!(slug("LPT9"), "lpt9_");
        assert_eq!(slug("com"), "com");
        assert_eq!(slug("..\u{1f600}"), "___");
        assert_eq!(slug(&"x".repeat(100)).len(), MAX_STEM);
        assert_eq!(split_font_ext("a.OTF"), Some(("a", "otf")));
        assert_eq!(split_font_ext("\u{e9}ttf"), None);
        assert_eq!(split_font_ext("ttf"), None);
    }

    #[test]
    fn clone_numbers_need_digits_after_the_prefix() {
        assert_eq!(clone_number("tf2tools_12"), Some(12));
        assert_eq!(clone_number("TF2Tools_"), None);
        assert_eq!(clone_number("TF2Tools_1a"), None);
        assert_eq!(clone_number("TF2Tool\u{e9}_1"), None);
        assert_eq!(clone_number("TF2Tools_99999999999999999999999"), None);
        assert!(!is_clone("TF2Too\u{20ac}"));
    }
}
