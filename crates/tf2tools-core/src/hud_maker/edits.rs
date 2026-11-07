//! Player actions as changes. Each builder reads the current model and returns one `Set` per
//! line of every key it touches, keeping each value's written form where it can.

use super::document::writable;
use super::expr::{atof, atoi};
use super::model::minmode_base;
use super::probe;
use super::{
    ALIGNMENTS, Align, Change, ChangeTarget, ClassSchema, HideTechnique, HudModel, KeyLine,
    PanelRule, Part, PartKey, PinCorner, PosExpr, PosUnit, PropertyKind, RemoveKey, Screen,
    ScreenScale, SetKey, SizeExpr, SizeForm, TermOp,
};

/// Why a builder made no changes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum EditRefused {
    /// No part has this key.
    UnknownPart,
    /// The part's files did not load, its geometry is not well formed, or the HUD hides this
    /// scene.
    ReadOnly,
    /// The game's code owns what the edit would change.
    NotOffered,
    /// The value is not one the maker writes.
    BadValue,
    /// The animation runs console commands, so it is shown but never changed. The player
    /// reads: "This animation runs game commands, so it can't be changed here."
    RunsGameCommands,
}

/// Which edits a part accepts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub struct PartCaps {
    move_x: bool,
    move_y: bool,
    resize_wide: bool,
    resize_tall: bool,
    hide: bool,
    recolour: bool,
    mirror: bool,
}

/// Why Mirror is not offered for a part that moves sideways.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum NoMirror {
    /// Its width follows the screen or parent in a way no anchor can mirror (`p`, `s`, `o`,
    /// or `f` under an `r` or `c` anchor).
    Width,
    /// Its place is set another way: the game's code, a pin, or a `p` or mixed-anchor position.
    Place,
}

impl PartCaps {
    /// Whether the part moves sideways.
    pub fn move_x(self) -> bool {
        self.move_x
    }

    /// Whether the part moves up and down.
    pub fn move_y(self) -> bool {
        self.move_y
    }

    /// Whether the part's width changes.
    pub fn resize_wide(self) -> bool {
        self.resize_wide
    }

    /// Whether the part's height changes.
    pub fn resize_tall(self) -> bool {
        self.resize_tall
    }

    /// Whether the part can be hidden.
    pub fn hide(self) -> bool {
        self.hide
    }

    /// Whether the part can be recoloured.
    pub fn recolour(self) -> bool {
        self.recolour
    }

    /// Whether Mirror applies: the part moves sideways and `why_not_mirrored` is `None`.
    pub fn mirror(self) -> bool {
        self.mirror
    }
}

const GEOMETRY: [&str; 4] = ["xpos", "ypos", "wide", "tall"];

/// The largest whole number `set_number` writes: four 480-unit screen heights, so no player
/// edit can push the game's scaled spacing maths anywhere near overflow.
const MAX_NUMBER: i32 = 4 * 480;

impl HudModel {
    /// The edits `part` accepts, or `None` when no part has that key.
    pub fn capabilities(&self, part: &PartKey) -> Option<PartCaps> {
        let part = self.part(part)?;
        let geometry = geometry_ok(part);
        let free = |key: &str| geometry && !code_owns(part, key);
        Some(PartCaps {
            mirror: free("xpos") && self.mirrored_lines(part).is_ok(),
            move_x: free("xpos"),
            move_y: free("ypos"),
            resize_wide: free("wide"),
            resize_tall: free("tall"),
            hide: part.is_readable() && hide_of(part) != HideTechnique::NotOffered,
            recolour: part.is_readable()
                && colour_slot(part).is_some_and(|slot| !code_owns(part, slot)),
        })
    }

    /// Pixel deltas at `screen`; an ignored axis is skipped.
    pub fn move_part(
        &self,
        part: &PartKey,
        dx: i32,
        dy: i32,
        screen: Screen,
        group: u32,
    ) -> Result<Vec<Change>, EditRefused> {
        let scale = ScreenScale::new(screen.tall());
        self.shift_part(part, scale.normalise(dx), scale.normalise(dy), group)
    }

    /// Moves `part` by whole HUD units on each axis, keeping each line's form; the pin sign
    /// applies and a code-owned axis is skipped. Refusals as `move_part`.
    pub fn nudge_part(
        &self,
        part: &PartKey,
        dx: i32,
        dy: i32,
        group: u32,
    ) -> Result<Vec<Change>, EditRefused> {
        self.shift_part(part, dx, dy, group)
    }

    /// Writes `source`'s size at `screen` onto `part` as plain units: the source's own text
    /// when it is plain units, else the fewest units reaching its pixel size. A code-owned or
    /// already equal axis writes nothing; a source axis under 1 px is `BadValue`.
    pub fn match_size(
        &self,
        part: &PartKey,
        source: &PartKey,
        screen: Screen,
        group: u32,
    ) -> Result<Vec<Change>, EditRefused> {
        let target = self.geometry_part(part, ["wide", "tall"])?;
        let from = self.part(source).ok_or(EditRefused::UnknownPart)?;
        let layout = self.layout(screen);
        let size = layout.get(source).ok_or(EditRefused::UnknownPart)?;
        let mut changes = Vec::new();
        for (key, px) in [("wide", size.wide()), ("tall", size.tall())] {
            if code_owns(target, key) {
                continue;
            }
            if px < 1 {
                return Err(EditRefused::BadValue);
            }
            // Plain text carries over as written, so matching never loses a fraction.
            let plain = (from.value(key).map(KeyLine::text))
                .filter(|_| !code_owns(from, key))
                .filter(|t| {
                    let e = SizeExpr::parse(t);
                    e.form() == SizeForm::Units && e.is_well_formed()
                });
            let text = plain.map_or_else(|| units_reaching(px, screen).to_string(), str::to_owned);
            changes.extend(changed(set_lines(target, key, group, |_| text.clone())));
        }
        Ok(changes)
    }

    /// Why `part` cannot be mirrored; `None` when it can or no part has that key.
    pub fn why_not_mirrored(&self, part: &PartKey) -> Option<NoMirror> {
        let part = self.part(part)?;
        if !geometry_ok(part) {
            return Some(NoMirror::Place);
        }
        self.mirrored_lines(part).err()
    }

    /// Mirrors `part` left to right, across the screen or, with `proportionaltoparent`, inside
    /// its parent, by rewriting its `xpos` lines with the opposite anchor; the width is never
    /// rewritten and an unchanged line writes nothing.
    pub fn mirror_part(&self, part: &PartKey, group: u32) -> Result<Vec<Change>, EditRefused> {
        let part = self.part(part).ok_or(EditRefused::UnknownPart)?;
        if code_owns(part, "xpos") {
            return Err(EditRefused::NotOffered);
        }
        if !geometry_ok(part) {
            return Err(EditRefused::ReadOnly);
        }
        let lines = self
            .mirrored_lines(part)
            .map_err(|_| EditRefused::NotOffered)?;
        // A copy's base line is written to the own block, as `set_lines` does.
        let set = |line: Option<&KeyLine>, new: &str| match line {
            Some(l) => SetKey::new(
                l.through().map_or(l.file(), |(file, _)| file),
                &refs(l.through().map_or(l.block(), |(_, block)| block)),
                "xpos",
                l.cond(),
                Some(l.text()),
                new,
            ),
            None => {
                let (file, block) = part.own_block();
                SetKey::new(file, &refs(block), "xpos", None, None, new)
            }
        };
        // A missing `xpos` sits at plain 0, so mirroring onto 0 writes nothing either.
        Ok(lines
            .into_iter()
            .filter(|(line, new)| line.map_or("0", KeyLine::text) != new)
            .map(|(line, new)| Change::set(group, set(line, &new)))
            .collect())
    }

    /// Each `xpos` line (or `None` for a missing key) with its mirrored text, or why the part
    /// cannot be mirrored on every screen shape.
    fn mirrored_lines<'a>(
        &self,
        part: &'a Part,
    ) -> Result<Vec<(Option<&'a KeyLine>, String)>, NoMirror> {
        // Without `proportionaltoparent` the anchors measure the screen, which only matches the
        // parent's frame when every ancestor spans the screen from x 0.
        let to_parent = part
            .value("proportionaltoparent")
            .is_some_and(|l| atoi(l.text().as_bytes()) != 0);
        if code_owns(part, "xpos")
            || self.is_pinned(part)
            || (!to_parent && !self.parents_span_screen(part))
        {
            return Err(NoMirror::Place);
        }
        let lines = part.lines("xpos");
        let texts: Vec<(Option<&KeyLine>, &str)> = match lines.is_empty() {
            true => vec![(None, "")],
            false => lines.into_iter().map(|l| (Some(l), l.text())).collect(),
        };
        let linear = |text: &str| {
            (PosExpr::parse(text).terms().iter().enumerate()).all(|(i, t)| {
                t.unit() != PosUnit::ParentSize && (i == 0 || t.align() == Align::Start)
            })
        };
        if !texts.iter().all(|(_, t)| linear(t)) {
            return Err(NoMirror::Place);
        }
        texts
            .into_iter()
            .map(|(line, text)| {
                let new = mirror_pos(text, mirror_width(part, line))?;
                Ok((line, new))
            })
            .collect()
    }

    /// The shared body of moves and nudges: whole units per axis, the pin sign applied.
    fn shift_part(
        &self,
        part: &PartKey,
        ux: i32,
        uy: i32,
        group: u32,
    ) -> Result<Vec<Change>, EditRefused> {
        let part = self.geometry_part(part, ["xpos", "ypos"])?;
        let (sign_x, sign_y) = self.pin_sign(part);
        let mut changes = Vec::new();
        for (key, units, sign) in [("xpos", ux, sign_x), ("ypos", uy, sign_y)] {
            let units = units.saturating_mul(sign);
            if units != 0 && !code_owns(part, key) {
                changes.extend(set_lines(part, key, group, |text| shift_pos(text, units)));
            }
        }
        Ok(changes)
    }

    /// New sizes in pixels at `screen`; `None` leaves that axis.
    pub fn resize_part(
        &self,
        part: &PartKey,
        wide: Option<i32>,
        tall: Option<i32>,
        screen: Screen,
        group: u32,
    ) -> Result<Vec<Change>, EditRefused> {
        let part = self.geometry_part(part, ["wide", "tall"])?;
        let scale = ScreenScale::new(screen.tall());
        // `f` sizes measure against the screen, or the parent with `proportionaltoparent`.
        let to_parent = part
            .value("proportionaltoparent")
            .is_some_and(|l| atoi(l.text().as_bytes()) != 0);
        let (w, t) = part
            .parent()
            .filter(|_| to_parent)
            .and_then(|p| self.layout(screen).get(p).map(|b| (b.wide(), b.tall())))
            .unwrap_or((clamp(screen.wide()), clamp(screen.tall())));
        let along = |horizontal: bool| if horizontal { w } else { t };
        let mut changes = Vec::new();
        for (key, px, horizontal) in [("wide", wide, true), ("tall", tall, false)] {
            let Some(px) = px.filter(|_| !code_owns(part, key)) else {
                continue;
            };
            let align = along(horizontal);
            let new = |text: &str| resize_size(text, px, align, scale);
            changes.extend(set_lines(part, key, group, new));
        }
        Ok(changes)
    }

    /// Hides the part the way the probe tables say holds for it.
    pub fn hide_part(&self, part: &PartKey, group: u32) -> Result<Vec<Change>, EditRefused> {
        let part = self.part(part).ok_or(EditRefused::UnknownPart)?;
        let technique = hide_of(part);
        if technique == HideTechnique::NotOffered {
            return Err(EditRefused::NotOffered);
        }
        if !part.is_readable() {
            return Err(EditRefused::ReadOnly);
        }
        Ok(technique
            .writes()
            .iter()
            .flat_map(|(key, value)| set_lines(part, key, group, |_| (*value).to_owned()))
            .collect())
    }

    /// Sets the part's `zpos`.
    pub fn set_layer(
        &self,
        part: &PartKey,
        zpos: i32,
        group: u32,
    ) -> Result<Vec<Change>, EditRefused> {
        let part = self.part(part).ok_or(EditRefused::UnknownPart)?;
        if !part.is_readable() {
            return Err(EditRefused::ReadOnly);
        }
        Ok(set_lines(part, "zpos", group, |_| zpos.to_string()))
    }

    /// Sets a whole-number key the part's schema offers as a Number row at Stage 1, written as
    /// the row spells the key. Refuses an unknown part, any other key, an unreadable part and a
    /// value below 0 or above `MAX_NUMBER` (four screen heights), in that order.
    pub fn set_number(
        &self,
        part: &PartKey,
        key: &str,
        value: i32,
        group: u32,
    ) -> Result<Vec<Change>, EditRefused> {
        let part = self.part(part).ok_or(EditRefused::UnknownPart)?;
        let spec = ClassSchema::for_part(part)
            .get(key)
            .filter(|s| s.kind() == PropertyKind::Number && s.stage() <= 1)
            .ok_or(EditRefused::NotOffered)?;
        if !part.is_readable() {
            return Err(EditRefused::ReadOnly);
        }
        if !(0..=MAX_NUMBER).contains(&value) {
            return Err(EditRefused::BadValue);
        }
        Ok(set_lines(part, spec.key(), group, |_| value.to_string()))
    }

    /// `colour` is literal `r g b a` text or a scheme name; the slot follows the Decisions.
    pub fn recolour_part(
        &self,
        part: &PartKey,
        colour: &str,
        group: u32,
    ) -> Result<Vec<Change>, EditRefused> {
        let part = self.part(part).ok_or(EditRefused::UnknownPart)?;
        let slot = colour_slot(part)
            .filter(|slot| !code_owns(part, slot))
            .ok_or(EditRefused::NotOffered)?;
        if !part.is_readable() {
            return Err(EditRefused::ReadOnly);
        }
        let value = self.colour_text(colour).ok_or(EditRefused::BadValue)?;
        Ok(set_lines(part, slot, group, |_| value.clone()))
    }

    /// Writes `value` to a row: the part's schema row for `key` (ASCII case-insensitive) of kind
    /// Text, Alignment, Number or Font at stage 2 or earlier, or Toggle, Colour or Choice at
    /// stage 5 or earlier, written on every line of the row's key or inserted into the own
    /// block. A Choice value must be one of the row's values. Refuses,
    /// in order: UnknownPart; NotOffered for no such row or a key the game's code owns;
    /// ReadOnly; BadValue unless Text is writable, Alignment is one of `ALIGNMENTS` ignoring
    /// case (written as listed), Toggle is "0" or "1", Number is 0 to 1920 with at most four
    /// whole digits and two decimals (none at stage 1), Colour passes the recolour rule, and
    /// Font is a writable scheme font name ignoring case. The Stage 1 Colour row writes the
    /// slot `recolour_part` picks.
    pub fn set_property(
        &self,
        part: &PartKey,
        key: &str,
        value: &str,
        group: u32,
    ) -> Result<Vec<Change>, EditRefused> {
        use PropertyKind::{
            Alignment, Choice, Colour, Font, Hide, Image, Layer, Number, Position, Size, Text,
            Toggle,
        };
        let part = self.part(part).ok_or(EditRefused::UnknownPart)?;
        let spec = ClassSchema::for_part(part)
            .get(key)
            .filter(|s| !code_owns(part, s.key()))
            .filter(|s| match s.kind() {
                Text | Alignment | Number | Font => s.stage() <= 2,
                Toggle | Colour | Choice => s.stage() <= 5,
                Position | Size | Layer | Hide | Image => false,
            })
            .ok_or(EditRefused::NotOffered)?;
        if !part.is_readable() {
            return Err(EditRefused::ReadOnly);
        }
        let text = match spec.kind() {
            Text => writable(spec.key(), value, None).then(|| value.to_owned()),
            Alignment => ALIGNMENTS
                .iter()
                .find(|a| a.eq_ignore_ascii_case(value))
                .map(|a| (*a).to_owned()),
            Toggle => matches!(value, "0" | "1").then(|| value.to_owned()),
            // Stage 1 Number rows (the player list spacing) are read as whole numbers.
            Number => (number_ok(value) && (spec.stage() > 1 || !value.contains('.')))
                .then(|| value.to_owned()),
            Colour => self.colour_text(value),
            Choice => (spec.choices().iter())
                .find(|(_, v)| *v == value)
                .map(|(_, v)| (*v).to_owned()),
            // Written as picked: the engine looks fonts up ignoring case.
            Font => (self.scheme().font_names().iter())
                .any(|f| f.eq_ignore_ascii_case(value) && writable(spec.key(), value, None))
                .then(|| value.to_owned()),
            Position | Size | Layer | Hide | Image => None,
        }
        .ok_or(EditRefused::BadValue)?;
        // The Stage 1 Colour row writes the slot `recolour_part` picks, so a set slot such as
        // fgcolor_override cannot shadow the edit.
        let key = match spec.kind() {
            Colour if spec.stage() <= 1 => colour_slot(part).unwrap_or(spec.key()),
            Position | Size | Layer | Hide | Colour | Text | Alignment | Toggle | Font | Image
            | Number | Choice => spec.key(),
        };
        Ok(set_lines(part, key, group, |_| text.clone()))
    }

    /// Sets `key` to `text` on every line `Part::lines` gives, or inserts it into the own
    /// block. Refuses, in order: UnknownPart; NotOffered for `ControlName`, `fieldName` or a
    /// key the game's code owns on a rule's own panel; ReadOnly when unreadable; BadValue for a
    /// blank key or when the key or `text` holds `"`, CR or LF.
    pub fn set_key_text(
        &self,
        part: &PartKey,
        key: &str,
        text: &str,
        group: u32,
    ) -> Result<Vec<Change>, EditRefused> {
        let part = self.text_part(part, key)?;
        if key.trim().is_empty() || !writable(key, text, None) {
            return Err(EditRefused::BadValue);
        }
        Ok(set_lines(part, key, group, |_| text.to_owned()))
    }

    /// One `Remove` (cond `None`, `old` the winning text) per block of the chain holding a
    /// line of `key`, a line in an `if_*` sub-block counting for its parent block. The same
    /// refusals as `set_key_text` except BadValue. An absent key gives no changes. In a copy a
    /// `#base` file is never edited: only the own lines go, and a key only a base sets is
    /// written as the class default in the own block (NotOffered when it has none).
    pub fn remove_key(
        &self,
        part: &PartKey,
        key: &str,
        group: u32,
    ) -> Result<Vec<Change>, EditRefused> {
        let part = self.text_part(part, key)?;
        let (lines, base): (Vec<&KeyLine>, Vec<&KeyLine>) =
            (part.chain_lines(key).into_iter()).partition(|line| line.through().is_none());
        let old = part.value(key).map(KeyLine::text);
        if lines.is_empty() && !base.is_empty() {
            let default = (ClassSchema::for_part(part).get(key))
                .and_then(|spec| spec.default())
                .ok_or(EditRefused::NotOffered)?;
            let (file, block) = part.own_block();
            let set = SetKey::new(file, &refs(block), key, None, old, default);
            return Ok(vec![Change::set(group, set)]);
        }
        // A line sits in one of the part's blocks or in an `if_*` sub-block directly under it,
        // so matching against the blocks in load order also deduplicates them.
        let holds = |(file, block): &&(String, Vec<String>)| {
            lines.iter().any(|line| {
                let path = line.block();
                line.file() == file
                    && (path == block.as_slice()
                        || path
                            .split_last()
                            .is_some_and(|(_, up)| up == block.as_slice()))
            })
        };
        Ok((part.blocks().iter().filter(holds))
            .map(|(file, block)| {
                Change::remove(group, RemoveKey::new(file, &refs(block), key, None, old))
            })
            .collect())
    }

    /// The part for a text edit or removal of `key`: refused when unknown, when the key names
    /// the panel's identity or the game's code owns it, then when the part is unreadable.
    fn text_part(&self, key: &PartKey, setting: &str) -> Result<&Part, EditRefused> {
        let part = self.part(key).ok_or(EditRefused::UnknownPart)?;
        let identity = ["ControlName", "fieldName"]
            .iter()
            .any(|k| k.eq_ignore_ascii_case(setting));
        if identity || code_owns(part, setting) {
            return Err(EditRefused::NotOffered);
        }
        if !part.is_readable() {
            return Err(EditRefused::ReadOnly);
        }
        Ok(part)
    }

    /// The part for a move or resize over `keys`: refused when the game owns both, or when the
    /// part is unreadable or any geometry line is not well formed.
    fn geometry_part(&self, key: &PartKey, keys: [&str; 2]) -> Result<&Part, EditRefused> {
        let part = self.part(key).ok_or(EditRefused::UnknownPart)?;
        if keys.iter().all(|k| code_owns(part, k)) {
            return Err(EditRefused::NotOffered);
        }
        if !geometry_ok(part) {
            return Err(EditRefused::ReadOnly);
        }
        Ok(part)
    }

    /// Whether the part names a sibling that exists in `pin_to_sibling`.
    pub(super) fn is_pinned(&self, part: &Part) -> bool {
        let Some(name) = part.value("pin_to_sibling").map(KeyLine::text) else {
            return false;
        };
        self.parts().iter().any(|p| {
            p.parent() == part.parent()
                && p.key() != part.key()
                && p.name().eq_ignore_ascii_case(name)
        })
    }

    /// The sign the engine applies to a pinned part's offsets; `(1, 1)` when it is not pinned.
    fn pin_sign(&self, part: &Part) -> (i32, i32) {
        if !self.is_pinned(part) {
            return (1, 1);
        }
        let corner = |key: &str| PinCorner::parse(part.value(key).map(KeyLine::text));
        probe::pin_offset_sign(
            corner("pin_corner_to_sibling"),
            corner("pin_to_sibling_corner"),
        )
    }

    /// The text written for `colour`: four whole numbers 0 to 255 joined by single spaces, or a
    /// name this HUD's scheme defines; `None` for anything else.
    pub(super) fn colour_text(&self, colour: &str) -> Option<String> {
        if colour.contains(['"', '\r', '\n']) {
            return None;
        }
        // Anything that starts like a number must be a full literal, so the scheme's lenient
        // literal reader never accepts three numbers or an out-of-range one.
        let numeric = colour
            .trim_start()
            .starts_with(|c: char| c.is_ascii_digit() || matches!(c, '+' | '-' | '.'));
        if numeric {
            let parts: Vec<&str> = colour.split_whitespace().collect();
            let byte = |p: &&str| p.bytes().all(|b| b.is_ascii_digit()) && p.parse::<u8>().is_ok();
            return (parts.len() == 4 && parts.iter().all(byte)).then(|| parts.join(" "));
        }
        self.scheme()
            .named_colour(colour)
            .map(|_| colour.to_owned())
    }
}

/// Whether `text` is a Stage 2 number: one to four digits, optionally `.` and one or two
/// more, and no more than `MAX_NUMBER`.
fn number_ok(text: &str) -> bool {
    let (whole, decimals) = text.split_once('.').unwrap_or((text, "0"));
    let digits =
        |s: &str, max: usize| (1..=max).contains(&s.len()) && s.bytes().all(|b| b.is_ascii_digit());
    digits(whole, 4)
        && digits(decimals, 2)
        && whole.parse::<i32>().is_ok_and(|w| {
            w < MAX_NUMBER || (w == MAX_NUMBER && decimals.bytes().all(|b| b == b'0'))
        })
}

fn clamp(px: u32) -> i32 {
    i32::try_from(px).unwrap_or(i32::MAX)
}

/// Whether the game's code owns `key` for this part: a rule's own panel ignores it, or the
/// scene's code-toggled entry for the part claims it.
pub(super) fn code_owns(part: &Part, key: &str) -> bool {
    (part.is_root()
        && part
            .owner()
            .is_some_and(|id| PanelRule::get(id).ignores(key)))
        || part.code_toggled().is_some_and(|t| t.owns(key))
}

/// Whether the part loaded and every line of each geometry key it may edit is well formed.
pub(super) fn geometry_ok(part: &Part) -> bool {
    part.is_readable()
        && GEOMETRY.iter().filter(|k| !code_owns(part, k)).all(|key| {
            part.lines(key)
                .iter()
                .all(|line| match key.ends_with("pos") {
                    true => PosExpr::parse(line.text()).is_well_formed(),
                    false => SizeExpr::parse(line.text()).is_well_formed(),
                })
        })
}

/// The hide for a part: its rule's for a rule's own panel, else the scene's for a code-toggled
/// part, else by whether it is a hudlayout element (a top-level part).
fn hide_of(part: &Part) -> HideTechnique {
    match (part.owner().filter(|_| part.is_root()), part.code_toggled()) {
        (Some(id), _) => PanelRule::get(id).hide(),
        (None, Some(toggled)) => toggled.hide(),
        (None, None) => probe::unclassified_hide(part.parent().is_none()),
    }
}

/// The key a recolour writes: none for a player list, whose row colours are code, else the
/// first colour key the part sets, else one by class.
fn colour_slot(part: &Part) -> Option<&'static str> {
    const SLOTS: [&str; 3] = ["fgcolor", "fgcolor_override", "drawcolor"];
    if part
        .class()
        .is_some_and(|c| c.eq_ignore_ascii_case("SectionedListPanel"))
    {
        return None;
    }
    if let Some(slot) = SLOTS.into_iter().find(|k| part.value(k).is_some()) {
        return Some(slot);
    }
    let class = part.class()?;
    let is = |name: &str| class.eq_ignore_ascii_case(name);
    if is("CExLabel") {
        Some("fgcolor")
    } else if is("Label") {
        Some("fgcolor_override")
    } else if is("ImagePanel") || is("CTFImagePanel") {
        Some("drawcolor")
    } else {
        None
    }
}

/// One `Set` per line of `key` with `new(old text)`, or one insert into the part's own block.
/// A line of a copy's `#base` file is written to the own block instead, once per conditional,
/// and not at all when the own block already sets that key and conditional.
pub(super) fn set_lines(
    part: &Part,
    key: &str,
    group: u32,
    new: impl Fn(&str) -> String,
) -> Vec<Change> {
    let lines = part.lines(key);
    if lines.is_empty() {
        let (file, block) = part.own_block();
        let set = SetKey::new(file, &refs(block), key, None, None, &new(""));
        return vec![Change::set(group, set)];
    }
    let same_cond = |a: Option<&str>, b: Option<&str>| match (a, b) {
        (Some(a), Some(b)) => a.eq_ignore_ascii_case(b),
        (a, b) => a == b,
    };
    let same_block = |a: &[String], b: &[String]| {
        a.len() == b.len() && a.iter().zip(b).all(|(x, y)| x.eq_ignore_ascii_case(y))
    };
    // When a base line wins, the own block's lines of the key (an `if_*` sub-block's too) are
    // read from the whole chain, since one inactive on this platform is no winner but is still
    // the line an edit of that conditional writes.
    let own_lines: Vec<&KeyLine> = match lines.iter().find_map(|l| l.through()) {
        Some((file, block)) => (part.chain_lines(key).into_iter())
            .filter(|l| {
                let path = l.block();
                l.through().is_none()
                    && l.file() == file
                    && (same_block(path, block)
                        || path
                            .split_last()
                            .is_some_and(|(_, up)| same_block(up, block)))
            })
            .collect(),
        None => Vec::new(),
    };
    let own_sets = |cond: Option<&str>| own_lines.iter().any(|l| same_cond(l.cond(), cond));
    let mut sets: Vec<SetKey> = Vec::new();
    for line in own_lines.iter().copied().chain(lines.iter().copied()) {
        let old = line.text();
        let (file, block) = line.through().unwrap_or((line.file(), line.block()));
        if line.through().is_some() && own_sets(line.cond()) {
            continue;
        }
        // A minimal-HUD line is written under its own name, so the normal HUD keeps its line.
        let key = match minmode_base(line.key()) {
            Some(_) => line.key(),
            None => key,
        };
        let set = SetKey::new(file, &refs(block), key, line.cond(), Some(old), &new(old));
        // Two base lines can land on one own line; it is written once.
        let twin = sets.iter().any(|s| {
            s.file() == set.file()
                && same_block(s.block(), set.block())
                && same_cond(s.cond(), set.cond())
        });
        if !twin {
            sets.push(set);
        }
    }
    sets.into_iter()
        .map(|set| Change::set(group, set))
        .collect()
}

/// Drops the sets whose new text equals the old, so an unchanged line writes nothing.
fn changed(changes: Vec<Change>) -> impl Iterator<Item = Change> {
    changes.into_iter().filter(|c| match c.target() {
        ChangeTarget::Set(s) => s.old() != Some(s.value()),
        ChangeTarget::Remove(_)
        | ChangeTarget::AddBlock(_)
        | ChangeTarget::Asset(_)
        | ChangeTarget::Option(_)
        | ChangeTarget::Credit(_)
        | ChangeTarget::Animation(_)
        | ChangeTarget::AnimationAppend(_) => true,
    })
}

/// The fewest whole units reaching `px` pixels on `screen`.
fn units_reaching(px: i32, screen: Screen) -> i32 {
    let tall = i64::from(screen.tall().max(1));
    clamp_i64((i64::from(px) * 480 + tall - 1).div_euclid(tall))
}

/// Moves a position by whole `units`, keeping its form: a trailing plain-unit number changes in
/// place, anything else gets `+N`/`-N` appended.
pub(crate) fn shift_pos(text: &str, units: i32) -> String {
    let expr = PosExpr::parse(text);
    let append = || format!("{text}{units:+}");
    let Some(last) = expr.terms().last() else {
        return append();
    };
    if last.unit() != PosUnit::Units || !text.ends_with(last.number()) {
        return append();
    }
    let start = text.len() - last.number().len();
    let old = i64::from(atoi(last.number().as_bytes()));
    let units = i64::from(units);
    match last.op() {
        // The first term sets the position; `r` measures back from the far edge.
        TermOp::Set => {
            let new = clamp_i64(match last.align() {
                Align::End => old - units,
                Align::Start | Align::Centre => old + units,
            });
            let plus = if new >= 0 && last.number().starts_with('+') {
                "+"
            } else {
                ""
            };
            format!("{}{plus}{new}", &text[..start])
        }
        // A later plain term is a signed offset whose operator follows the new sign.
        TermOp::Add | TermOp::Sub if last.align() == Align::Start => {
            let signed = if last.op() == TermOp::Sub { -old } else { old };
            let new = clamp_i64(signed + units);
            let op = if new < 0 { '-' } else { '+' };
            format!("{}{op}{}", &text[..start - 1], i64::from(new).abs())
        }
        TermOp::Add | TermOp::Sub => append(),
    }
}

/// The width text an `xpos` line mirrors with: the `wide` line of the same block (same
/// condition first), else the part's winning `wide`; `None` when the part sets no width.
fn mirror_width<'a>(part: &'a Part, line: Option<&KeyLine>) -> Option<&'a str> {
    let widths = part.lines("wide");
    let same = |l: &&&KeyLine| line.is_some_and(|x| l.file() == x.file() && l.block() == x.block());
    let in_block: Vec<&&KeyLine> = widths.iter().filter(same).collect();
    (in_block
        .iter()
        .find(|l| line.is_some_and(|x| l.cond() == x.cond())))
    .or(in_block.first())
    .map(|l| l.text())
    .or_else(|| part.value("wide").map(KeyLine::text))
}

/// The mirrored text of one position for a part whose width text is `wide` (`None` when it
/// sets none), or why Mirror refuses it: a width it cannot keep, or a position it cannot take
/// or whose result leaves the file's number range. No terms counts as plain 0.
pub(crate) fn mirror_pos(text: &str, wide: Option<&str>) -> Result<String, NoMirror> {
    // Read the position as `a·P + b·own + u`: the anchor fraction, own-size fraction and units.
    let (mut a, mut b, mut u) = (0.0_f64, 0.0_f64, 0_i64);
    for (i, term) in PosExpr::parse(text).terms().iter().enumerate() {
        let number = term.number().as_bytes();
        let sign: i32 = match (i, term.align(), term.op()) {
            (0, Align::Start, _) => 1,
            (0, Align::Centre, _) => {
                a = 0.5;
                1
            }
            (0, Align::End, _) => {
                a = 1.0;
                -1
            }
            (_, Align::Start, TermOp::Sub) => -1,
            (_, Align::Start, _) => 1,
            _ => return Err(NoMirror::Place),
        };
        match term.unit() {
            PosUnit::Units => u += i64::from(sign) * i64::from(atoi(number)),
            PosUnit::OwnSize => b += f64::from(sign) * f64::from(atof(number)),
            PosUnit::ParentSize => return Err(NoMirror::Place),
        }
    }
    let round = |x: f64| (x * 1e4).round() / 1e4;
    let b = round(b);
    // With an own-size term the size stays symbolic: `P - own - x` flips every coefficient.
    if b != 0.0 {
        let (a, b, u) = (1.0 - a, round(-1.0 - b), -u);
        if b == 0.0 {
            return anchored(a, u);
        }
        in_range(u)?;
        let head = match a {
            0.0 => format!("s{b}"),
            0.5 => format!("cs{b}"),
            _ => format!("rs{}", -b),
        };
        let tail = if u == 0 {
            String::new()
        } else {
            format!("{u:+}")
        };
        return Ok(head + &tail);
    }
    let size = SizeExpr::parse(wide.ok_or(NoMirror::Width)?);
    let n = i64::from(atoi(size.number().as_bytes()));
    match size.form() {
        SizeForm::Units => anchored(1.0 - a, -n - u),
        // An `f` width spans to N short of the far edge, so only a plain start mirrors.
        SizeForm::Full if a == 0.0 => anchored(0.0, n - u),
        SizeForm::Full | SizeForm::ParentFrac | SizeForm::OwnFrac | SizeForm::OtherAxis => {
            Err(NoMirror::Width)
        }
    }
}

/// A plain-units position at anchor fraction `a` (0, ½ or 1) offset by `u` units.
fn anchored(a: f64, u: i64) -> Result<String, NoMirror> {
    in_range(u)?;
    Ok(match a {
        0.0 => u.to_string(),
        0.5 => format!("c{u}"),
        _ => format!("r{}", -u),
    })
}

// An offset written either way round must read back as the same i32, so `-u` must fit too.
fn in_range(u: i64) -> Result<(), NoMirror> {
    match u > i64::from(i32::MIN) && u <= i64::from(i32::MAX) {
        true => Ok(()),
        false => Err(NoMirror::Place),
    }
}

/// Borrows a block path for `SetKey::new`.
fn refs(block: &[String]) -> Vec<&str> {
    block.iter().map(String::as_str).collect()
}

fn clamp_i64(value: i64) -> i32 {
    value.clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32
}

/// A size of `new_px` pixels as text: plain and `f` keep their form (`f` against `align_px`),
/// `p`, `s` and `o` become plain units.
pub(crate) fn resize_size(text: &str, new_px: i32, align_px: i32, scale: ScreenScale) -> String {
    match SizeExpr::parse(text).form() {
        SizeForm::Full => {
            let units = scale.normalise(align_px.saturating_sub(new_px));
            format!("{}{units}", &text[..1])
        }
        SizeForm::Units | SizeForm::ParentFrac | SizeForm::OwnFrac | SizeForm::OtherAxis => {
            scale.normalise(new_px).to_string()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hud_maker::ScreenScale;

    #[test]
    fn moves_keep_the_form() {
        let cases = [
            ("0", 20, "20"),
            ("r95", 20, "r75"),
            ("r95", -20, "r115"),
            ("r-5", 20, "r-25"),
            ("c-100", 20, "c-80"),
            ("c+10", 20, "c+30"),
            ("c-10", 20, "c10"),
            ("cs-0.5", 20, "cs-0.5+20"),
            ("cs-0.5", -20, "cs-0.5-20"),
            ("p0.1", 3, "p0.1+3"),
            ("rs1-10", 20, "rs1+10"),
            ("rs1-10", -5, "rs1-15"),
            ("rs1+3", -5, "rs1-2"),
            ("12.5", 2, "14"),
        ];
        for (text, units, want) in cases {
            assert_eq!(shift_pos(text, units), want, "{text} {units:+}");
        }
    }

    #[test]
    fn resizes_keep_plain_and_f_and_flatten_the_rest() {
        let s = ScreenScale::new(1080);
        let cases = [
            ("250", 450, "200"),
            ("f0", 1800, "f53"),
            ("f10", 1920, "f0"),
            ("f0", 2000, "f-35"),
            ("p0.5", 450, "200"),
            ("s2", 450, "200"),
            ("o1", 450, "200"),
        ];
        for (text, px, want) in cases {
            assert_eq!(resize_size(text, px, 1920, s), want, "{text} to {px}px");
        }
    }

    #[test]
    fn mirrored_positions_swap_the_anchor() {
        let cases = [
            // Plain widths: plain and `r` swap, `c` stays.
            ("76", "44", Some("r120")),
            ("r95", "94", Some("1")),
            ("r131", "94", Some("37")),
            ("c-38", "100", Some("c-62")),
            ("c-126", "252", Some("c-126")),
            ("r-5", "10", Some("-15")),
            ("r0", "30", Some("-30")),
            ("", "20", Some("r20")),
            ("10+5", "20", Some("r35")),
            ("12.5", "2", Some("r14")),
            // `f` widths: only a plain position, which stays plain.
            ("0", "f0", Some("0")),
            ("5", "f200", Some("195")),
            ("10", "f20", Some("10")),
            ("r10", "f20", None),
            ("c-10", "f20", None),
            // Other widths: only through an `s` position.
            ("10", "p0.5", None),
            ("10", "s1", None),
            ("10", "o2", None),
            // `s` positions mirror at any width.
            ("cs-0.5", "44", Some("cs-0.5")),
            ("cs-0.5+20", "p0.5", Some("cs-0.5-20")),
            ("cs-0.25", "f20", Some("cs-0.75")),
            ("rs1-10", "o2", Some("10")),
            ("s-1", "44", Some("r0")),
            ("rs0.5", "44", Some("s-0.5")),
            ("s-0.5+10", "s1", Some("rs0.5-10")),
            // Positions Mirror never takes.
            ("p0.1", "20", None),
            ("c10-r5", "20", None),
            ("c-10+p0.5", "20", None),
        ];
        for (text, wide, want) in cases {
            assert_eq!(
                mirror_pos(text, Some(wide)).ok().as_deref(),
                want,
                "{text} at width {wide}"
            );
        }
    }

    #[test]
    fn mirroring_a_position_twice_gives_it_back() {
        let cases = [
            ("76", "44"),
            ("r95", "94"),
            ("c-38", "100"),
            ("5", "f200"),
            ("cs-0.5+20", "p0.5"),
            ("rs0.5-10", "f0"),
        ];
        for (text, wide) in cases {
            let once = mirror_pos(text, Some(wide)).unwrap();
            assert_eq!(
                mirror_pos(&once, Some(wide)).ok().as_deref(),
                Some(text),
                "{text} via {once}"
            );
        }
    }
}
