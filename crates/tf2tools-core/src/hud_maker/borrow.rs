//! Borrowing a part from another HUD: the part's keys and children become ordinary changes on
//! the player's own HUD, plus a credit for the author. Nothing is written to the source.

mod animations;
mod images;
mod scheme;

use std::collections::{HashMap, HashSet};

use super::edits::code_owns;
use super::{
    AddBlock, BlockLine, Change, ChangeTarget, CreditNote, HudFiles, HudModel, KeyLine, Part,
    PartKey, RemoveKey, Screen, SetKey,
};
use crate::library::HudAssets;

/// Keys that name a panel rather than describe it; a borrow never rewrites them on a part the
/// player's HUD already has.
const IDENTITY: [&str; 2] = ["ControlName", "fieldName"];
/// The keys that pin a part to a sibling.
const PIN_KEYS: [&str; 3] = [
    "pin_to_sibling",
    "pin_corner_to_sibling",
    "pin_to_sibling_corner",
];
/// The screen a pinned part's position is read at when the pin has to become plain numbers.
pub(super) const READ_AT: Screen = Screen::new(1920, 1080);

/// Another HUD, loaded and ready to borrow from: its model, its files over the game's, and who
/// to credit.
#[derive(Debug)]
pub struct BorrowSource {
    model: HudModel,
    files: HudFiles,
    hud: String,
    author: String,
    origin: String,
}

impl BorrowSource {
    /// A source HUD named `hud` by `author` (may be empty), found at `origin`. `files` are the
    /// HUD's folder over the game's files and `model` is loaded from them.
    pub fn new(
        model: HudModel,
        files: HudFiles,
        hud: &str,
        author: &str,
        origin: &str,
    ) -> BorrowSource {
        BorrowSource {
            model,
            files,
            hud: hud.to_owned(),
            author: author.to_owned(),
            origin: origin.to_owned(),
        }
    }

    /// The source HUD's parts.
    pub fn model(&self) -> &HudModel {
        &self.model
    }

    /// The source HUD's name.
    pub fn hud(&self) -> &str {
        &self.hud
    }
}

/// What a borrow brings along besides the part's own keys.
#[derive(Debug, PartialEq)]
#[non_exhaustive]
pub enum Brings {
    /// A child part, added or replaced.
    Child(String),
    /// A font definition.
    Font(String),
    /// A font file shipped with the HUD, by family.
    FontFile(String),
    /// A colour.
    Colour(String),
    /// A border.
    Border(String),
    /// A picture the part names.
    Image(String),
    /// A picture the game draws by a fixed name, replaced with the source's.
    CodeImage(String),
    /// An animation event the borrow writes or adds lines to.
    Animation(String),
}

/// What a borrow could not bring.
#[derive(Debug, PartialEq)]
#[non_exhaustive]
pub enum LeftBehind {
    /// Console command lines in the part's animations, never borrowed.
    Animations(usize),
    /// A font family no file was found for.
    MissingFont(String),
    /// A picture or material the source does not have.
    MissingFile(String),
    /// Settings the game's code owns.
    CodeKeys(usize),
    /// Pins to parts that are not coming along and could not be turned into positions.
    UnresolvedPins(usize),
}

/// Why a part cannot be borrowed.
#[derive(Debug, PartialEq)]
#[non_exhaustive]
pub enum BorrowRefused {
    /// The source has no such part.
    UnknownPart,
    /// The part, or the one it replaces, could not be read.
    Unreadable,
    /// The two HUDs are showing different screens.
    SceneMismatch,
    /// There is nowhere in the player's HUD to put it.
    NoRoom,
    /// More pictures than a HUD may hold.
    TooMuch,
}

/// A borrow worked out: the changes to apply as one group and what comes with them.
#[derive(Debug)]
pub struct Borrowed {
    changes: Vec<Change>,
    assets: HudAssets,
    brings: Vec<Brings>,
    left_behind: Vec<LeftBehind>,
    renamed: Vec<(String, String)>,
}

impl Borrowed {
    /// The changes, all in the group asked for, ending with the credit.
    pub fn changes(&self) -> &[Change] {
        &self.changes
    }

    /// The bytes of the files the changes add, by their kept copy.
    pub fn assets(&self) -> &HudAssets {
        &self.assets
    }

    /// What comes along with the part.
    pub fn brings(&self) -> &[Brings] {
        &self.brings
    }

    /// What was left behind.
    pub fn left_behind(&self) -> &[LeftBehind] {
        &self.left_behind
    }

    /// Names that were changed to avoid clashing, as (source name, new name).
    pub fn renamed(&self) -> &[(String, String)] {
        &self.renamed
    }
}

impl HudModel {
    /// Works out borrowing `part` of `source` into this HUD as one group of changes. `self` is
    /// the player's HUD as their changes leave it; `target_files` are its game files.
    ///
    /// # Errors
    /// `UnknownPart` when the source has no such part, `SceneMismatch` when the HUDs show
    /// different screens, `Unreadable` when the part or the one it replaces cannot be read,
    /// `NoRoom` when there is nowhere to put it.
    pub fn borrow_part(
        &self,
        source: &BorrowSource,
        target_files: &HudFiles,
        part: &PartKey,
        group: u32,
    ) -> Result<Borrowed, BorrowRefused> {
        let src = source.model.part(part).ok_or(BorrowRefused::UnknownPart)?;
        if source.model.scene() != self.scene() {
            return Err(BorrowRefused::SceneMismatch);
        }
        if !(src.is_readable() || src.is_from_base()) {
            return Err(BorrowRefused::Unreadable);
        }
        let prefix = format!("{}/", part.as_str());
        let set: Vec<&Part> = (source.model.parts().iter())
            .filter(|p| p.key() == part || p.key().as_str().starts_with(&prefix))
            .collect();
        let mut run = Run {
            target: self,
            source,
            root: part,
            group,
            changes: Vec::new(),
            brings: Vec::new(),
            code_keys: 0,
            unresolved_pins: 0,
            added: HashMap::new(),
            assets: HudAssets::new(),
            left_behind: Vec::new(),
            renamed: Vec::new(),
        };
        for s in &set {
            match self.part(s.key()) {
                Some(t) => run.replace(s, t)?,
                None => run.add(s)?,
            }
        }
        run.hide_missing(&set);
        run.carry_scheme()?;
        run.carry_images(target_files, &set)?;
        run.carry_animations(&set);
        let mut left_behind = std::mem::take(&mut run.left_behind);
        if run.code_keys > 0 {
            left_behind.push(LeftBehind::CodeKeys(run.code_keys));
        }
        if run.unresolved_pins > 0 {
            left_behind.push(LeftBehind::UnresolvedPins(run.unresolved_pins));
        }
        let note = CreditNote::new(&source.hud, &source.author, &source.origin, src.name());
        run.changes.push(Change::credit(group, note));
        // The borrow must replay whole after the changes that made `self`: nothing dropped,
        // and every block it adds a part.
        let all: Vec<Change> = (self.replayed().iter())
            .chain(&run.changes)
            .cloned()
            .collect();
        let replay = self.with_changes(&all);
        let all_added = run.added.keys().all(|k| replay.model().part(k).is_some());
        if replay.dropped() != 0 || !all_added {
            return Err(BorrowRefused::NoRoom);
        }
        Ok(Borrowed {
            changes: run.changes,
            assets: run.assets,
            brings: run.brings,
            left_behind,
            renamed: run.renamed,
        })
    }
}

/// One borrow in progress.
struct Run<'a> {
    target: &'a HudModel,
    source: &'a BorrowSource,
    root: &'a PartKey,
    group: u32,
    changes: Vec<Change>,
    brings: Vec<Brings>,
    code_keys: usize,
    unresolved_pins: usize,
    /// Blocks this borrow adds, by part, as (file, block path), so a child lands inside them.
    added: HashMap<PartKey, (String, Vec<String>)>,
    /// Bytes of the files the changes add, by kept copy.
    assets: HudAssets,
    left_behind: Vec<LeftBehind>,
    /// Names changed to avoid a clash, as (source name, new name).
    renamed: Vec<(String, String)>,
}

/// An `if_*` sub-block: its name and its lines as (key, conditional, value).
type IfBlock = (String, Vec<(String, Option<String>, String)>);

/// Pixels on a screen `screen_tall` px high as 480-high units, rounded to the nearest.
pub(super) fn px_to_units(px: i32, screen_tall: u32) -> i32 {
    let tall = i64::from(screen_tall.max(1));
    i32::try_from((i64::from(px) * 480 * 2 + tall).div_euclid(tall * 2)).unwrap_or(0)
}

fn refs(path: &[String]) -> Vec<&str> {
    path.iter().map(String::as_str).collect()
}

fn is_identity(key: &str) -> bool {
    IDENTITY.iter().any(|k| k.eq_ignore_ascii_case(key))
}

impl Run<'_> {
    /// Rewrites every value in the changes so far where `swap(key, value)` gives a new one.
    fn rewrite_values(&mut self, swap: &dyn Fn(&str, &str) -> Option<String>) {
        let changes = std::mem::take(&mut self.changes);
        self.changes = changes
            .into_iter()
            .map(|change| match change.target() {
                ChangeTarget::Set(s) => match swap(s.key(), s.value()) {
                    Some(new) => {
                        let mut set = SetKey::new(
                            s.file(),
                            &refs(s.block()),
                            s.key(),
                            s.cond(),
                            s.old(),
                            &new,
                        );
                        if s.appends() {
                            set = set.appended();
                        }
                        Change::set(change.group(), set)
                    }
                    None => change,
                },
                ChangeTarget::AddBlock(a) => {
                    let lines = a
                        .lines()
                        .iter()
                        .map(|l| {
                            let value = swap(l.key(), l.value());
                            BlockLine::new(l.key(), value.as_deref().unwrap_or(l.value()), l.cond())
                        })
                        .collect();
                    let add = AddBlock::new(a.file(), &refs(a.parent()), a.name(), a.cond(), lines);
                    Change::add_block(change.group(), add)
                }
                ChangeTarget::Remove(_)
                | ChangeTarget::Asset(_)
                | ChangeTarget::Option(_)
                | ChangeTarget::Credit(_)
                | ChangeTarget::Animation(_)
                | ChangeTarget::AnimationAppend(_) => change,
            })
            .collect();
    }

    /// Replaces the keys of the player's part `t` with those of the source part `s`: every line
    /// of a key the source sets, exactly as written, and nothing of a key only `t` sets.
    fn replace(&mut self, s: &Part, t: &Part) -> Result<(), BorrowRefused> {
        if !t.is_readable() {
            return Err(BorrowRefused::Unreadable);
        }
        if s.key() != self.root {
            self.brings.push(Brings::Child(s.name().to_owned()));
        }
        let position = self.detached_position(s);
        let mut keys = self.source_keys(s, position.is_some());
        if let Some((x, y)) = position {
            for (key, value) in [("xpos", x), ("ypos", y)] {
                keys.retain(|(k, _)| !k.eq_ignore_ascii_case(key));
                keys.push((key.to_owned(), vec![Line::Plain(value.to_string())]));
            }
        }
        let wanted: HashSet<String> = keys.iter().map(|(k, _)| k.to_ascii_lowercase()).collect();
        // Keys only the player's part sets go; one the game owns just stays.
        for line in t.settings() {
            if !is_identity(line.key())
                && !wanted.contains(&line.key().to_ascii_lowercase())
                && let Ok(removed) = self.target.remove_key(t.key(), line.key(), self.group)
            {
                self.changes.extend(removed);
            }
        }
        let (file, block) = t.own_block();
        for (key, lines) in keys {
            let removed = match self.target.remove_key(t.key(), &key, self.group) {
                Ok(removed) => removed,
                Err(super::EditRefused::NotOffered) => {
                    self.code_keys += 1;
                    continue;
                }
                Err(_) => continue,
            };
            self.changes.extend(removed);
            // A second line of the same key and condition must not replace the first.
            let mut seen: HashSet<Option<String>> = HashSet::new();
            for line in lines {
                let (cond, text) = line.parts();
                let mut set = SetKey::new(file, &refs(block), &key, cond, None, text);
                if !seen.insert(cond.map(str::to_ascii_lowercase)) {
                    set = set.appended();
                }
                self.changes.push(Change::set(self.group, set));
            }
        }
        self.replace_if_blocks(s, t, file, block);
        Ok(())
    }

    /// Copies each `if_*` sub-block of the source part, active or not, into the player's part:
    /// the borrowed keys replace the player's lines of those keys inside the sub-block of the
    /// same name, and a sub-block the player's part lacks is added whole.
    fn replace_if_blocks(&mut self, s: &Part, t: &Part, file: &str, block: &[String]) {
        for (name, lines) in self.source_if_blocks(s) {
            let mut inside = block.to_vec();
            inside.push(name.clone());
            let exists = self
                .target
                .document(file)
                .is_some_and(|doc| !doc.find(&refs(&inside)).is_empty());
            if !exists {
                let lines = lines
                    .iter()
                    .map(|(k, c, v)| BlockLine::new(k, v, c.as_deref()))
                    .collect();
                let add = AddBlock::new(file, &refs(block), &name, None, lines);
                self.changes.push(Change::add_block(self.group, add));
                continue;
            }
            let mut keys: Vec<&str> = Vec::new();
            for (key, _, _) in &lines {
                if !keys.iter().any(|k| k.eq_ignore_ascii_case(key)) {
                    keys.push(key);
                }
            }
            for key in keys {
                if is_identity(key) || code_owns(t, key) {
                    continue;
                }
                let gone = RemoveKey::new(file, &refs(&inside), key, None, None);
                self.changes.push(Change::remove(self.group, gone));
                let mut seen: HashSet<Option<String>> = HashSet::new();
                for (_, cond, text) in lines.iter().filter(|(k, _, _)| k.eq_ignore_ascii_case(key))
                {
                    let mut set =
                        SetKey::new(file, &refs(&inside), key, cond.as_deref(), None, text);
                    if !seen.insert(cond.as_deref().map(str::to_ascii_lowercase)) {
                        set = set.appended();
                    }
                    self.changes.push(Change::set(self.group, set));
                }
            }
        }
    }

    /// The `if_*` sub-blocks of every block the source part reads, same-named ones joined in
    /// load order, each as (name, lines as (key, conditional, value)).
    fn source_if_blocks(&self, s: &Part) -> Vec<IfBlock> {
        let mut found: Vec<IfBlock> = Vec::new();
        let mut seen: Vec<&(String, Vec<String>)> = Vec::new();
        for place in s.blocks() {
            if seen.contains(&place) {
                continue;
            }
            seen.push(place);
            let (file, path) = place;
            let Some(doc) = self.source.model.document(file) else {
                continue;
            };
            for block in doc.find(&refs(path)) {
                for sub in block.children().iter().filter(|e| e.text().is_none()) {
                    if sub
                        .key()
                        .get(..3)
                        .is_none_or(|p| !p.eq_ignore_ascii_case("if_"))
                    {
                        continue;
                    }
                    let lines = sub.children().iter().filter_map(|l| {
                        let text = l.text()?;
                        Some((
                            l.key().to_owned(),
                            l.cond().map(str::to_owned),
                            text.to_owned(),
                        ))
                    });
                    match found
                        .iter_mut()
                        .find(|(n, _)| n.eq_ignore_ascii_case(sub.key()))
                    {
                        Some((_, existing)) => existing.extend(lines),
                        None => found.push((sub.key().to_owned(), lines.collect())),
                    }
                }
            }
        }
        found
    }

    /// Adds the source part `s` as a new block inside its parent, with every line it has.
    fn add(&mut self, s: &Part) -> Result<(), BorrowRefused> {
        let parent = s.parent().ok_or(BorrowRefused::NoRoom)?;
        let (file, path) = match self.added.get(parent) {
            Some(place) => place.clone(),
            None => {
                let up = self.target.part(parent).ok_or(BorrowRefused::NoRoom)?;
                if !up.is_readable() {
                    return Err(BorrowRefused::Unreadable);
                }
                self.container_of(up).ok_or(BorrowRefused::NoRoom)?
            }
        };
        if s.key() != self.root {
            self.brings.push(Brings::Child(s.name().to_owned()));
        }
        let position = self.detached_position(s);
        let mut keys = self.source_keys(s, position.is_some());
        if let Some((x, y)) = position {
            for (key, value) in [("xpos", x), ("ypos", y)] {
                keys.retain(|(k, _)| !k.eq_ignore_ascii_case(key));
                keys.push((key.to_owned(), vec![Line::Plain(value.to_string())]));
            }
        }
        let lines: Vec<BlockLine> = keys
            .iter()
            .flat_map(|(key, lines)| {
                lines.iter().map(move |l| {
                    let (cond, text) = l.parts();
                    BlockLine::new(key, text, cond)
                })
            })
            .collect();
        self.changes.push(Change::add_block(
            self.group,
            AddBlock::new(&file, &refs(&path), s.name(), None, lines),
        ));
        let mut inside = path;
        inside.push(s.name().to_owned());
        for (name, lines) in self.source_if_blocks(s) {
            let lines = (lines.iter())
                .map(|(k, c, v)| BlockLine::new(k, v, c.as_deref()))
                .collect();
            let add = AddBlock::new(&file, &refs(&inside), &name, None, lines);
            self.changes.push(Change::add_block(self.group, add));
        }
        self.added.insert(s.key().clone(), (file, inside));
        Ok(())
    }

    /// The block that holds the children of the player's part `up`: beside an existing child
    /// when it has one, else the file's top block for a rule's own panel (whose controls sit
    /// beside it), else the part's own block.
    fn container_of(&self, up: &Part) -> Option<(String, Vec<String>)> {
        let sibling = self
            .target
            .parts()
            .iter()
            .find(|p| p.parent() == Some(up.key()));
        if let Some(sibling) = sibling {
            let (file, block) = sibling.own_block();
            let (_, container) = block.split_last()?;
            return Some((file.to_owned(), container.to_vec()));
        }
        let (file, block) = up.own_block();
        if up.is_root() {
            let (_, container) = block.split_last()?;
            (!container.is_empty()).then(|| (file.to_owned(), container.to_vec()))
        } else {
            (!block.is_empty()).then(|| (file.to_owned(), block.to_vec()))
        }
    }

    /// The source part's keys with their lines in document order; the identity keys only when
    /// the part is being added, and the pin keys left out when `drop_pin`.
    fn source_keys(&self, s: &Part, drop_pin: bool) -> Vec<(String, Vec<Line>)> {
        let adding = self.target.part(s.key()).is_none();
        let mut keys: Vec<(u32, String, Vec<Line>)> = Vec::new();
        for winner in s.settings() {
            let key = winner.key();
            let identity = is_identity(key);
            let pin = PIN_KEYS.iter().any(|k| k.eq_ignore_ascii_case(key));
            if (identity && !adding) || (pin && drop_pin) {
                continue;
            }
            // Lines inside an `if_*` sub-block travel with that sub-block, not as plain lines.
            let found: Vec<&KeyLine> = (s.lines(key).into_iter())
                .filter(|l| !in_if_block(l))
                .collect();
            let lines: Vec<Line> = found.iter().map(|l| Line::of(l)).collect();
            let first = found
                .iter()
                .map(|l| l.line())
                .min()
                .unwrap_or(winner.line());
            keys.push((first, key.to_owned(), lines));
        }
        keys.sort_by_key(|(first, _, _)| *first);
        keys.into_iter().map(|(_, k, l)| (k, l)).collect()
    }

    /// For the borrowed part itself, when it is pinned to a sibling the player's HUD lacks: its
    /// position relative to its parent as the source draws it, in 480-high units. `None` when
    /// the pin can stay (or there is none); an unresolvable pin is counted and just dropped.
    fn detached_position(&mut self, s: &Part) -> Option<(i32, i32)> {
        if s.key() != self.root {
            return None;
        }
        let pinned_to = s.value("pin_to_sibling").map(KeyLine::text)?.trim();
        if pinned_to.is_empty() {
            return None;
        }
        let sibling_exists = s.parent().is_some_and(|parent| {
            self.target
                .parts()
                .iter()
                .any(|p| p.parent() == Some(parent) && p.name().eq_ignore_ascii_case(pinned_to))
        });
        if sibling_exists {
            return None;
        }
        let layout = self.source.model.layout(READ_AT);
        let here = layout.get(s.key());
        let up = s.parent().and_then(|p| layout.get(p));
        let Some(here) = here else {
            self.unresolved_pins += 1;
            return None;
        };
        let (px, py) = match up {
            Some(up) => (here.x() - up.x(), here.y() - up.y()),
            None => (here.x(), here.y()),
        };
        Some((
            px_to_units(px, READ_AT.tall()),
            px_to_units(py, READ_AT.tall()),
        ))
    }

    /// Hides each child the player's part has that the source part lacks, so the result looks
    /// like the borrowed part. A part that cannot be hidden is left as it is.
    fn hide_missing(&mut self, set: &[&Part]) {
        let theirs: HashSet<&PartKey> = set.iter().map(|p| p.key()).collect();
        let matched: HashSet<&PartKey> = set
            .iter()
            .filter(|p| self.target.part(p.key()).is_some())
            .map(|p| p.key())
            .collect();
        for p in self.target.parts() {
            let under_match = p.parent().is_some_and(|parent| matched.contains(parent));
            if under_match
                && !theirs.contains(p.key())
                && p.class().is_some()
                && let Ok(hidden) = self.target.hide_part(p.key(), self.group)
            {
                self.changes.extend(hidden);
            }
        }
    }
}

/// One line to write: a source line with its conditional, or a plain computed value.
enum Line {
    Source { cond: Option<String>, text: String },
    Plain(String),
}

impl Line {
    fn of(line: &KeyLine) -> Line {
        Line::Source {
            cond: line.cond().map(str::to_owned),
            text: line.text().to_owned(),
        }
    }

    fn parts(&self) -> (Option<&str>, &str) {
        match self {
            Line::Source { cond, text } => (cond.as_deref(), text),
            Line::Plain(text) => (None, text),
        }
    }
}

/// Whether a line sits in an `if_*` sub-block of its part's block.
fn in_if_block(line: &KeyLine) -> bool {
    line.block()
        .last()
        .and_then(|name| name.get(..3))
        .is_some_and(|p| p.eq_ignore_ascii_case("if_"))
}
