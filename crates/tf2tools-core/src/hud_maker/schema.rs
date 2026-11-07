//! The side panel schema: which rows each HUD class offers and at which stage, plus the
//! change-list helpers behind a row's reset and the panel's unhide.

use super::fonts::is_clone;
use super::model::SCHEME_FILE;
use super::{Change, ChangeTarget, HudModel, Part, PartKey, SetKey};
use PropertyKind::{
    Alignment, Choice, Colour, Font, Hide, Image, Layer, Number, Position, Size, Text, Toggle,
};
use std::collections::BTreeMap;

/// The nine `textAlignment` values the engine reads, as the maker writes them.
pub const ALIGNMENTS: [&str; 9] = [
    "north-west",
    "north",
    "north-east",
    "west",
    "center",
    "east",
    "south-west",
    "south",
    "south-east",
];

/// What a side panel row edits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum PropertyKind {
    /// `xpos` and `ypos`.
    Position,
    /// `wide` and `tall`.
    Size,
    /// `zpos`.
    Layer,
    /// Whether the part shows.
    Hide,
    /// A colour slot.
    Colour,
    /// Label text.
    Text,
    /// Text alignment.
    Alignment,
    /// An on/off switch.
    Toggle,
    /// A scheme font.
    Font,
    /// An image path.
    Image,
    /// A plain number.
    Number,
    /// One of a fixed list of values, given by `PropertySpec::choices`.
    Choice,
}

/// One row of a class's side panel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub struct PropertySpec {
    key: &'static str,
    label: &'static str,
    keys: &'static [&'static str],
    kind: PropertyKind,
    stage: u8,
    default: Option<&'static str>,
    choices: &'static [(&'static str, &'static str)],
}

impl PropertySpec {
    /// The row's key as the files spell it.
    pub fn key(&self) -> &'static str {
        self.key
    }

    /// The row's player-facing label.
    pub fn label(&self) -> &'static str {
        self.label
    }

    /// What the row edits.
    pub fn kind(&self) -> PropertyKind {
        self.kind
    }

    /// The stage that first offers the row.
    pub fn stage(&self) -> u8 {
        self.stage
    }

    /// The engine's default value, when known.
    pub fn default(&self) -> Option<&'static str> {
        self.default
    }

    /// A Choice row's (label, value) pairs in display order; empty for any other row.
    pub fn choices(&self) -> &'static [(&'static str, &'static str)] {
        self.choices
    }

    /// The keys the row writes: Position `["xpos","ypos"]`, Size `["wide","tall"]`, else `[key]`.
    pub fn keys(&self) -> &'static [&'static str] {
        self.keys
    }
}

/// The side panel rows of one HUD class, in display order.
#[derive(Debug)]
#[non_exhaustive]
pub struct ClassSchema {
    class: &'static str,
    properties: &'static [PropertySpec],
}

impl ClassSchema {
    /// Every class's schema.
    pub fn all() -> &'static [ClassSchema] {
        &SCHEMAS
    }

    /// By element name for HudDeathNotice and CDamageAccountPanel, else by `class()`, both
    /// ignoring ASCII case; anything else gets the schema whose class is "common".
    pub fn for_part(part: &Part) -> &'static ClassSchema {
        let named = |name: &str| {
            SCHEMAS
                .iter()
                .find(|s| s.class != COMMON && s.class.eq_ignore_ascii_case(name))
        };
        const BY_NAME: [&str; 2] = ["HudDeathNotice", "CDamageAccountPanel"];
        BY_NAME
            .iter()
            .find(|n| n.eq_ignore_ascii_case(part.name()))
            .and_then(|n| named(n))
            .or_else(|| part.class().and_then(named))
            .unwrap_or(&SCHEMAS[0])
    }

    /// The class name, or "common".
    pub fn class(&self) -> &'static str {
        self.class
    }

    /// Every row, in display order.
    pub fn properties(&self) -> &'static [PropertySpec] {
        self.properties
    }

    /// The rows offered at or before `stage`, in table order.
    pub fn up_to(&self, stage: u8) -> impl Iterator<Item = &'static PropertySpec> {
        let properties: &'static [PropertySpec] = self.properties;
        properties.iter().filter(move |p| p.stage <= stage)
    }

    /// The row for `key`, ignoring ASCII case.
    pub fn get(&self, key: &str) -> Option<&'static PropertySpec> {
        let properties: &'static [PropertySpec] = self.properties;
        properties.iter().find(|p| p.key.eq_ignore_ascii_case(key))
    }
}

impl HudModel {
    /// `changes` without the `Set`s that write any of `keys` (ASCII case-insensitive) on `part`'s
    /// blocks, and without the `Remove`s of any of `keys` on one of [`Part::blocks`].
    pub fn reset_property(&self, part: &PartKey, keys: &[&str], changes: &[Change]) -> Vec<Change> {
        let Some(part) = self.part(part) else {
            return changes.to_vec();
        };
        // A `Set` may land in any of the part's blocks or their `if_*` sub-blocks; a `Remove`
        // names one of the part's blocks. The blocks are listed whatever keys they hold, so a
        // change still resets after a later `Remove` emptied its block.
        let (file, block) = part.own_block();
        let mut blocks = vec![(file, block)];
        blocks.extend(
            part.blocks()
                .iter()
                .map(|(f, b)| (f.as_str(), b.as_slice())),
        );
        for key in keys {
            blocks.extend(part.lines(key).into_iter().map(|l| (l.file(), l.block())));
        }
        let names_key = |key: &str| keys.iter().any(|k| k.eq_ignore_ascii_case(key));
        let keeps = |change: &&Change| match change.target() {
            ChangeTarget::Set(set) => {
                !(names_key(set.key())
                    && (blocks
                        .iter()
                        .any(|(f, b)| same_block(set.file(), set.block(), f, b))
                        || in_if_block_of(set, part.blocks())))
            }
            ChangeTarget::Remove(remove) => {
                !(names_key(remove.key())
                    && part
                        .blocks()
                        .iter()
                        .any(|(f, b)| same_block(remove.file(), remove.block(), f, b)))
            }
            ChangeTarget::AddBlock(_)
            | ChangeTarget::Asset(_)
            | ChangeTarget::Option(_)
            | ChangeTarget::Credit(_)
            | ChangeTarget::Animation(_)
            | ChangeTarget::AnimationAppend(_) => true,
        };
        let (kept, dropped): (Vec<&Change>, Vec<&Change>) = changes.iter().partition(keeps);
        // A clone a dropped `Set` named goes with it rather than linger as an orphan, unless a
        // kept `Set` outside the fonts themselves still names it: the Font row offers clones too.
        let in_use = |clone: &str| {
            kept.iter()
                .filter(|change| font_of(change).is_none())
                .filter_map(|change| set_of(change))
                .any(|set| set.value().eq_ignore_ascii_case(clone))
        };
        let clones: Vec<&str> = dropped
            .iter()
            .filter_map(|change| set_of(change))
            .map(SetKey::value)
            .filter(|value| is_clone(value) && !in_use(value))
            .collect();
        kept.into_iter()
            .filter(|change| {
                font_of(change)
                    .is_none_or(|font| !clones.iter().any(|c| c.eq_ignore_ascii_case(font)))
            })
            .cloned()
            .collect()
    }

    /// The groups in `changes` that are wholly a hide of `part`, ascending.
    pub fn hide_groups(&self, part: &PartKey, changes: &[Change]) -> Vec<u32> {
        // Compare against a fresh hide's writes, ignoring `old` and the group, so the hide
        // technique stays owned by `hide_part`.
        let Ok(hide) = self.hide_part(part, 0) else {
            return Vec::new();
        };
        let is_hide = |set: &SetKey| {
            hide.iter().filter_map(set_of).any(|h| {
                same_block(set.file(), set.block(), h.file(), h.block())
                    && h.key().eq_ignore_ascii_case(set.key())
                    && h.value() == set.value()
            })
        };
        let mut groups: BTreeMap<u32, bool> = BTreeMap::new();
        // A group holding any other kind is a player's edit, never a hide.
        for change in changes {
            let all = groups.entry(change.group()).or_insert(true);
            *all &= set_of(change).is_some_and(is_hide);
        }
        groups
            .into_iter()
            .filter(|(_, all)| *all)
            .map(|(g, _)| g)
            .collect()
    }
}

/// Whether `set` wrote into an `if_*` sub-block of one of `blocks`, which a later `Remove`
/// may have emptied so that no line of the key remains to find it by.
fn in_if_block_of(set: &SetKey, blocks: &[(String, Vec<String>)]) -> bool {
    let Some((last, parent)) = set.block().split_last() else {
        return false;
    };
    last.get(..3).is_some_and(|p| p.eq_ignore_ascii_case("if_"))
        && blocks
            .iter()
            .any(|(f, b)| same_block(set.file(), parent, f, b))
}

/// The key set a change makes, when it is a `Set`.
fn set_of(change: &Change) -> Option<&SetKey> {
    match change.target() {
        ChangeTarget::Set(set) => Some(set),
        ChangeTarget::Remove(_)
        | ChangeTarget::AddBlock(_)
        | ChangeTarget::Asset(_)
        | ChangeTarget::Option(_)
        | ChangeTarget::Credit(_)
        | ChangeTarget::Animation(_)
        | ChangeTarget::AnimationAppend(_) => None,
    }
}

/// The font a `clientscheme.res` change builds or edits: the path segment after `Fonts`.
fn font_of(change: &Change) -> Option<&str> {
    let (file, path, name) = match change.target() {
        ChangeTarget::AddBlock(add) => (add.file(), add.parent(), Some(add.name())),
        ChangeTarget::Set(set) => (set.file(), set.block(), None),
        ChangeTarget::Remove(remove) => (remove.file(), remove.block(), None),
        ChangeTarget::Asset(_)
        | ChangeTarget::Option(_)
        | ChangeTarget::Credit(_)
        | ChangeTarget::Animation(_)
        | ChangeTarget::AnimationAppend(_) => return None,
    };
    if !file.eq_ignore_ascii_case(SCHEME_FILE) {
        return None;
    }
    let mut segments = path.iter().map(String::as_str).chain(name);
    segments.find(|s| s.eq_ignore_ascii_case("Fonts"))?;
    segments.next()
}

/// Whether a change's (file, block) is `block` of `file`, both compared ASCII
/// case-insensitively as a change is resolved.
fn same_block(file: &str, block: &[String], other_file: &str, other_block: &[String]) -> bool {
    file.eq_ignore_ascii_case(other_file)
        && block.len() == other_block.len()
        && block
            .iter()
            .zip(other_block)
            .all(|(a, b)| a.eq_ignore_ascii_case(b))
}

const COMMON: &str = "common";

/// A row writing its own key, with the engine default when the Source SDK sets one.
macro_rules! row {
    ($key:literal, $label:literal, $kind:expr, $stage:literal) => {
        PropertySpec {
            key: $key,
            label: $label,
            keys: &[$key],
            kind: $kind,
            stage: $stage,
            default: None,
            choices: &[],
        }
    };
    ($key:literal, $label:literal, $kind:expr, $stage:literal, $default:literal) => {
        PropertySpec {
            default: Some($default),
            ..row!($key, $label, $kind, $stage)
        }
    };
    ($key:literal, $label:literal, $kind:expr, $stage:literal, $default:literal, $choices:expr) => {
        PropertySpec {
            choices: $choices,
            ..row!($key, $label, $kind, $stage, $default)
        }
    };
}

const POSITION: PropertySpec = PropertySpec {
    keys: &["xpos", "ypos"],
    ..row!("xpos", "Position", Position, 1, "0")
};
const SIZE: PropertySpec = PropertySpec {
    keys: &["wide", "tall"],
    ..row!("wide", "Size", Size, 1)
};
const LAYER: PropertySpec = row!("zpos", "Layer", Layer, 1);
const HIDE: PropertySpec = row!("visible", "Hide", Hide, 1, "1");
// Only the flat and rounded background types are offered: the preview cannot draw the others.
const CORNERS: PropertySpec = row!(
    "PaintBackgroundType",
    "Corners",
    Choice,
    5,
    "0",
    &[("Square", "0"), ("Rounded", "2")]
);
// A bitmask: top-left 1, top-right 2, bottom-left 4, bottom-right 8.
const ROUNDED_CORNERS: PropertySpec = row!(
    "RoundedCorners",
    "Rounded corners",
    Choice,
    5,
    "15",
    &[
        ("All", "15"),
        ("Top", "3"),
        ("Bottom", "12"),
        ("Left", "5"),
        ("Right", "10")
    ]
);

// The corpus survey's keys per class, Stage 1 rows first; "common" stays first so it is the
// fallback in `for_part`. Keys the game ignores in practice (damage `PositiveColor` and
// `NegativeColor`, killfeed `LocalPlayerColor`) get no row; "All settings" still edits them.
static SCHEMAS: [ClassSchema; 18] = [
    ClassSchema {
        class: COMMON,
        properties: &[POSITION, SIZE, LAYER, HIDE],
    },
    ClassSchema {
        class: "CExLabel",
        properties: &[
            POSITION,
            SIZE,
            LAYER,
            HIDE,
            row!("fgcolor", "Colour", Colour, 1, "Label.TextColor"),
            row!("labelText", "Text", Text, 2, ""),
            row!("textAlignment", "Alignment", Alignment, 2, "west"),
            row!("font", "Font", Font, 2, "Default"),
            row!("allcaps", "Upper case", Toggle, 2, "0"),
            row!("wrap", "Wrap", Toggle, 2, "0"),
            row!("centerwrap", "Centre each line", Toggle, 2, "0"),
            row!(
                "bgcolor_override",
                "Background colour",
                Colour,
                5,
                "Label.BgColor"
            ),
        ],
    },
    ClassSchema {
        class: "Label",
        properties: &[
            POSITION,
            SIZE,
            LAYER,
            HIDE,
            row!("fgcolor_override", "Colour", Colour, 1, "Label.TextColor"),
            row!("labelText", "Text", Text, 2, ""),
            row!("font", "Font", Font, 2, "Default"),
        ],
    },
    ClassSchema {
        class: "ImagePanel",
        properties: &[
            POSITION,
            SIZE,
            LAYER,
            HIDE,
            row!("drawcolor", "Colour", Colour, 1, "255 255 255 255"),
            row!("image", "Image", Image, 5, ""),
            row!("scaleImage", "Stretch image", Toggle, 5, "0"),
            row!("fillcolor", "Fill colour", Colour, 5, "0 0 0 0"),
            row!("tileImage", "Tile image", Toggle, 5, "0"),
        ],
    },
    ClassSchema {
        class: "CTFImagePanel",
        properties: &[
            POSITION,
            SIZE,
            LAYER,
            HIDE,
            row!("drawcolor", "Colour", Colour, 1, "255 255 255 255"),
            row!("image", "Image", Image, 5, ""),
            row!("scaleImage", "Stretch image", Toggle, 5, "0"),
            row!("fillcolor", "Fill colour", Colour, 5, "0 0 0 0"),
            row!("tileImage", "Tile image", Toggle, 5, "0"),
            row!("teambg_2", "RED image", Image, 5, ""),
            row!("teambg_3", "BLU image", Image, 5, ""),
        ],
    },
    ClassSchema {
        class: "ScalableImagePanel",
        properties: &[
            POSITION,
            SIZE,
            LAYER,
            HIDE,
            row!("drawcolor", "Colour", Colour, 1, "255 255 255 255"),
            row!("image", "Image", Image, 5, ""),
        ],
    },
    ClassSchema {
        class: "CIconPanel",
        properties: &[
            POSITION,
            SIZE,
            LAYER,
            HIDE,
            row!("icon", "Icon", Image, 5, ""),
            row!("iconcolor", "Icon colour", Colour, 2, "255 255 255 255"),
        ],
    },
    ClassSchema {
        class: "EditablePanel",
        properties: &[
            POSITION,
            SIZE,
            LAYER,
            HIDE,
            row!(
                "bgcolor_override",
                "Background colour",
                Colour,
                5,
                "Panel.BgColor"
            ),
            row!("paintbackground", "Draw background", Toggle, 5, "1"),
            CORNERS,
            ROUNDED_CORNERS,
        ],
    },
    ClassSchema {
        class: "Panel",
        properties: &[
            POSITION,
            SIZE,
            LAYER,
            HIDE,
            row!(
                "bgcolor_override",
                "Background colour",
                Colour,
                5,
                "Panel.BgColor"
            ),
            row!("paintbackground", "Draw background", Toggle, 5, "1"),
            CORNERS,
            ROUNDED_CORNERS,
        ],
    },
    ClassSchema {
        class: "ContinuousProgressBar",
        properties: &[
            POSITION,
            SIZE,
            LAYER,
            HIDE,
            row!(
                "fgcolor_override",
                "Bar colour",
                Colour,
                2,
                "ProgressBar.FgColor"
            ),
            row!(
                "bgcolor_override",
                "Background colour",
                Colour,
                2,
                "ProgressBar.BgColor"
            ),
        ],
    },
    ClassSchema {
        class: "CircularProgressBar",
        properties: &[
            POSITION,
            SIZE,
            LAYER,
            HIDE,
            row!("fg_image", "Bar image", Image, 5, ""),
            row!("bg_image", "Background image", Image, 5, ""),
        ],
    },
    ClassSchema {
        class: "CTFPlayerModelPanel",
        properties: &[POSITION, SIZE, LAYER, HIDE],
    },
    ClassSchema {
        class: "CExButton",
        properties: &[POSITION, SIZE, LAYER, HIDE],
    },
    ClassSchema {
        class: "CExImageButton",
        properties: &[POSITION, SIZE, LAYER, HIDE],
    },
    ClassSchema {
        class: "CAvatarImagePanel",
        properties: &[
            POSITION,
            SIZE,
            LAYER,
            HIDE,
            row!("color_outline", "Outline colour", Colour, 2, "Black"),
        ],
    },
    ClassSchema {
        class: "HudDeathNotice",
        properties: &[
            POSITION,
            SIZE,
            LAYER,
            HIDE,
            row!("TextFont", "Font", Font, 2, "Default"),
            row!("LineHeight", "Line height", Number, 2, "16"),
            row!("LineSpacing", "Line spacing", Number, 2, "4"),
            row!("CornerRadius", "Corner rounding", Number, 5, "3"),
            row!(
                "BaseBackgroundColor",
                "Background",
                Colour,
                2,
                "46 43 42 220"
            ),
            row!(
                "LocalBackgroundColor",
                "Your kills background",
                Colour,
                2,
                "245 229 196 200"
            ),
            row!("TeamRed", "RED names", Colour, 2, "255 64 64 255"),
            row!("TeamBlue", "BLU names", Colour, 2, "153 204 255 255"),
            row!("IconColor", "Icon colour", Colour, 2, "255 80 0 255"),
            row!("MaxDeathNotices", "Rows", Number, 3, "4"),
        ],
    },
    ClassSchema {
        class: "CDamageAccountPanel",
        properties: &[
            POSITION,
            SIZE,
            LAYER,
            HIDE,
            row!("delta_item_font", "Font", Font, 2, "Default"),
            row!("delta_item_font_big", "Big hit font", Font, 2, "Default"),
            row!("delta_lifetime", "Time on screen", Number, 2, "2.0"),
        ],
    },
    // Player lists offer no colour: their row colours are code.
    ClassSchema {
        class: "SectionedListPanel",
        properties: &[
            POSITION,
            SIZE,
            LAYER,
            HIDE,
            row!("linespacing", "Row height", Number, 1, "20"),
            row!("linegap", "Row gap", Number, 1, "0"),
            row!("sectiongap", "Section gap", Number, 1, "8"),
        ],
    },
];
