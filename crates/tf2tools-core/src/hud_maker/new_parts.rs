//! Parts the player adds: a shadow, a background box, an anchor or a crosshair, each one
//! `AddBlock` beside an existing part, in the file the game already loads for its parent.

use super::borrow::{READ_AT, px_to_units};
use super::expr::atoi;
use super::model::{live_blocks, root_block};
use super::{
    AddBlock, AnimCommand, AnimationAppend, AnimationChange, BlockLine, Change, ChangeTarget,
    EditRefused, HudModel, Interp, KeyLine, Part, PartKey, check_hud_path,
};

/// A kind of part the player can add.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum NewPart {
    /// A pinned copy of a label, one unit down and right, one layer below.
    Shadow,
    /// A translucent panel pinned behind a part, a little larger.
    Box,
    /// An unpainted 8x8 panel at its parent's centre, for pinning to.
    Anchor,
    /// A `+` label at the screen's centre, at most one per HUD.
    Crosshair,
}

/// The file the game reads the class panel's children from, where the crosshair goes.
const CLASS_FILE: &str = "resource/ui/hudplayerclass.res";
const CROSSHAIR: &str = "TF2ToolsCrosshair";
/// The event the hit flash fills and `DamagedPlayer` restarts.
const HIT_MARKER: &str = "TF2ToolsHitMarker";
const DAMAGED: &str = "DamagedPlayer";
/// How long each colour step of the flash takes: next to nothing, so it snaps.
const SNAP: f32 = 0.00001;

/// Why a part could not be deleted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum RemoveRefused {
    /// No part has this key.
    UnknownPart,
    /// The part came with the HUD; only parts the player added can be deleted.
    NotAdded,
    /// Another part is attached to it. The player reads: "Detach the parts attached to it
    /// first."
    Attached,
}

/// Where a new block goes: its file, the parent block's path and the parent part.
struct Home {
    file: &'static str,
    parent: Vec<String>,
    parent_key: Option<PartKey>,
    /// The block sits inside `near`, which is an element rather than a sibling of it.
    inside: bool,
}

const PIN_KEYS: [&str; 3] = [
    "pin_to_sibling",
    "pin_corner_to_sibling",
    "pin_to_sibling_corner",
];

impl HudModel {
    /// One AddBlock in `group`, placed in `near`'s home under a free name; returns the new key.
    /// UnknownPart for `near`; NotOffered for a Shadow of a non-label; ReadOnly when unreadable.
    pub fn add_part(
        &self,
        kind: NewPart,
        near: &PartKey,
        group: u32,
    ) -> Result<(PartKey, Vec<Change>), EditRefused> {
        if kind == NewPart::Crosshair {
            return self.add_crosshair(group);
        }
        let part = self.part(near).ok_or(EditRefused::UnknownPart)?;
        let element = part.is_root() || part.parent().is_none();
        if kind == NewPart::Shadow && (!is_label(part) || element) {
            return Err(EditRefused::NotOffered);
        }
        if !part.is_readable() {
            return Err(EditRefused::ReadOnly);
        }
        let home = self.home_of(part, element).ok_or(EditRefused::NotOffered)?;
        // The crosshair returned above; it shares the anchor's arms only to keep the match whole.
        let base = match kind {
            NewPart::Shadow => format!("{}Shadow", part.name()),
            NewPart::Box => format!("{}Box", part.name()),
            NewPart::Anchor | NewPart::Crosshair => "TF2ToolsAnchor".to_owned(),
        };
        let name = self.free_name(&home, &base);
        let lines = match kind {
            NewPart::Shadow => self.shadow_lines(part, &name),
            NewPart::Box => self.box_lines(part, &name, home.inside),
            NewPart::Anchor | NewPart::Crosshair => anchor_lines(&name),
        };
        Ok(self.added(&home, &name, lines, group))
    }

    /// The crosshair label inside the class panel; the existing one's key and no changes
    /// when the HUD already has it.
    fn add_crosshair(&self, group: u32) -> Result<(PartKey, Vec<Change>), EditRefused> {
        let panel = (self.parts().iter())
            .find(|p| {
                p.is_root()
                    && (self.scene_rule().rules().iter())
                        .any(|r| Some(r.id()) == p.owner() && r.children_file() == CLASS_FILE)
            })
            .ok_or(EditRefused::NotOffered)?;
        let existing = (self.parts().iter())
            .find(|p| p.parent() == Some(panel.key()) && p.name().eq_ignore_ascii_case(CROSSHAIR));
        if let Some(existing) = existing {
            return Ok((existing.key().clone(), Vec::new()));
        }
        if !panel.is_readable() {
            return Err(EditRefused::ReadOnly);
        }
        let home = self.home_of(panel, true).ok_or(EditRefused::NotOffered)?;
        let lines = [
            ("ControlName", "CExLabel"),
            ("fieldName", CROSSHAIR),
            ("xpos", "cs-0.5"),
            ("ypos", "cs-0.5"),
            ("wide", "40"),
            ("tall", "40"),
            ("labelText", "+"),
            ("font", "Default"),
            ("textAlignment", "center"),
            ("fgcolor", "255 255 255 255"),
            ("visible", "1"),
            ("enabled", "1"),
        ];
        let lines = (lines.into_iter())
            .map(|(k, v)| (k.to_owned(), v.to_owned()))
            .collect();
        Ok(self.added(&home, CROSSHAIR, lines, group))
    }

    /// In `group`, two AnimationAppend changes and never a whole-event change:
    /// - to TF2ToolsHitMarker, the part to `colour` at 0 and back to its own colour text at 0.1,
    ///   each over 0.00001 s;
    /// - to DamagedPlayer, `StopEvent TF2ToolsHitMarker 0` and `RunEvent TF2ToolsHitMarker 0.00001`.
    ///
    /// Ok(empty) when TF2ToolsHitMarker already names the part. Refuses UnknownPart;
    /// NotOffered for a non-label; BadValue for a colour the recolour rule refuses;
    /// RunsGameCommands when the HUD's own copy of either event runs a console command,
    /// since replay never appends to such an event.
    pub fn hit_flash(
        &self,
        part: &PartKey,
        colour: &str,
        group: u32,
    ) -> Result<Vec<Change>, EditRefused> {
        let part = self.part(part).ok_or(EditRefused::UnknownPart)?;
        if !is_label(part) {
            return Err(EditRefused::NotOffered);
        }
        let colour = self.colour_text(colour).ok_or(EditRefused::BadValue)?;
        self.can_change_animation(HIT_MARKER)?;
        self.can_change_animation(DAMAGED)?;
        let flashing = (self.animations().event(HIT_MARKER)).is_some_and(|e| {
            (e.commands().iter()).any(|c| {
                c.panel()
                    .is_some_and(|p| p.eq_ignore_ascii_case(part.name()))
            })
        });
        if flashing {
            return Ok(Vec::new());
        }
        let own = (part
            .value("fgcolor")
            .or_else(|| part.value("fgcolor_override")))
        .map_or("255 255 255 255", KeyLine::text);
        let animate = |value: &str, start: f32| AnimCommand::Animate {
            panel: part.name().to_owned(),
            field: "FgColor".to_owned(),
            value: value.to_owned(),
            interp: Interp::Linear,
            start,
            duration: SNAP,
        };
        let flash = [animate(&colour, 0.0), animate(own, 0.1)]
            .map(|c| c.to_line())
            .join("\n");
        let trigger = trigger_lines();
        Ok(vec![
            Change::animation_append(group, AnimationAppend::new(HIT_MARKER, &flash)),
            Change::animation_append(group, AnimationAppend::new(DAMAGED, &trigger)),
        ])
    }

    /// Whether an AddBlock in `changes` made `part` (its file and block path match).
    pub fn is_added(&self, changes: &[Change], part: &PartKey) -> bool {
        self.part(part)
            .is_some_and(|p| added_at(changes, p).is_some())
    }

    /// The next change list. It drops that AddBlock, every later Set, Remove or AddBlock in
    /// the same file whose block path lies under it, every AnimationAppend whose lines name
    /// the part, and the DamagedPlayer trigger of each hit flash so dropped. It keeps every whole-event Animation, so no edit of the player's is lost. For
    /// each event whose own body names the part, it appends a new whole-event Animation in
    /// `group`: the current own body minus the lines naming the part. Undo is the old list.
    /// Attached while another part pins to it.
    pub fn remove_added(
        &self,
        changes: &[Change],
        part: &PartKey,
        group: u32,
    ) -> Result<Vec<Change>, RemoveRefused> {
        let p = self.part(part).ok_or(RemoveRefused::UnknownPart)?;
        let (at, block) = added_at(changes, p).ok_or(RemoveRefused::NotAdded)?;
        let attached = (self.parts().iter()).any(|s| {
            s.parent() == p.parent()
                && s.key() != p.key()
                && (s.value("pin_to_sibling"))
                    .is_some_and(|l| l.text().eq_ignore_ascii_case(p.name()))
        });
        if attached {
            return Err(RemoveRefused::Attached);
        }
        let names = |c: &AnimCommand| c.panel().is_some_and(|n| n.eq_ignore_ascii_case(p.name()));
        let platform = self.platform();
        // Only the AddBlock's own file counts: a block of the same path in another file is
        // another panel.
        let under = |file: &str, path: &[String]| {
            same_file(file, &block.0)
                && path.len() >= block.1.len()
                && (path.iter().zip(&block.1)).all(|(a, b)| a.eq_ignore_ascii_case(b))
        };
        let drops_part = |i: usize, change: &Change| match change.target() {
            ChangeTarget::AddBlock(_) if i == at => true,
            ChangeTarget::AddBlock(a) => i > at && under(a.file(), a.parent()),
            ChangeTarget::Set(s) => i > at && under(s.file(), s.block()),
            ChangeTarget::Remove(r) => i > at && under(r.file(), r.block()),
            ChangeTarget::AnimationAppend(a) => {
                (a.check(platform)).is_some_and(|c| c.iter().any(names))
            }
            ChangeTarget::Asset(_)
            | ChangeTarget::Option(_)
            | ChangeTarget::Credit(_)
            | ChangeTarget::Animation(_) => false,
        };
        // A flash's DamagedPlayer trigger goes with its marker lines, matched by group. Every
        // other flash carries its own copy, so the ones left keep firing.
        let flash_groups: Vec<u32> = (changes.iter().enumerate())
            .filter(|(i, c)| {
                matches!(c.target(), ChangeTarget::AnimationAppend(a) if a.event() == HIT_MARKER)
                    && drops_part(*i, c)
            })
            .map(|(_, c)| c.group())
            .collect();
        let trigger = trigger_lines();
        let dropped = |i: usize, change: &Change| {
            drops_part(i, change)
                || matches!(change.target(), ChangeTarget::AnimationAppend(a)
                    if flash_groups.contains(&change.group())
                        && a.event() == DAMAGED
                        && a.lines() == trigger)
        };
        let mut next: Vec<Change> = (changes.iter().enumerate())
            .filter(|(i, c)| !dropped(*i, c))
            .map(|(_, c)| c.clone())
            .collect();
        // A HUD's own events cannot name a part the player added, so only events a change
        // wrote are rewritten, from their own lines as they stand without the dropped appends.
        let left = self.with_changes(&next).into_model();
        for event in left
            .animations()
            .events()
            .iter()
            .filter(|e| e.layer().is_none())
        {
            let own = &event.commands()[..event.own_len().min(event.commands().len())];
            if !own.iter().any(names) {
                continue;
            }
            let body: Vec<String> = (own.iter())
                .filter(|c| !names(c))
                .map(AnimCommand::to_line)
                .collect();
            let change = AnimationChange::new(event.name(), &body.join("\n"));
            next.push(Change::animation(group, change));
        }
        Ok(next)
    }

    /// The new part's key and its one AddBlock change.
    fn added(
        &self,
        home: &Home,
        name: &str,
        lines: Vec<(String, String)>,
        group: u32,
    ) -> (PartKey, Vec<Change>) {
        let parent: Vec<&str> = home.parent.iter().map(String::as_str).collect();
        let lines = (lines.iter())
            .map(|(k, v)| BlockLine::new(k, v, None))
            .collect();
        let add = AddBlock::new(home.file, &parent, name, None, lines);
        let key = match &home.parent_key {
            Some(p) => PartKey::new(format!("{}/{name}", p.as_str())),
            None => PartKey::new(name),
        };
        (key, vec![Change::add_block(group, add)])
    }

    /// Where a part added near `part` goes: inside an element, else beside `part` in the
    /// owning rule's children file, never a `#base` file. `None` when the game reads no
    /// children for that rule or the file did not load.
    fn home_of(&self, part: &Part, element: bool) -> Option<Home> {
        let rule = (self.scene_rule().rules().iter())
            .find(|r| Some(r.id()) == part.owner())
            .filter(|r| r.has_children())?;
        let file = rule.children_file();
        let doc = self.document(file)?;
        // A part only a copy's `#base` chain defines is written where its edits go: the
        // children file, whose missing blocks the replay creates from the chain.
        let written = |p: &Part| {
            let (f, path) = p.own_block();
            f.eq_ignore_ascii_case(file).then(|| path.to_vec())
        };
        if element {
            let rule_root =
                (self.parts().iter()).find(|p| p.is_root() && p.owner() == part.owner());
            let root = match root_block(doc) {
                Some(root) => root.key().to_owned(),
                None => rule_root.and_then(written)?.first()?.clone(),
            };
            return Some(Home {
                file,
                parent: vec![root],
                parent_key: rule_root.map(|p| p.key().clone()),
                inside: true,
            });
        }
        // The sibling's own block in the children file names the parent block to add into.
        let path = match (part.blocks().iter()).rfind(|(f, _)| f.eq_ignore_ascii_case(file)) {
            Some((_, path)) => path.clone(),
            None => written(part)?,
        };
        let (_, parent) = path.split_last()?;
        Some(Home {
            file,
            parent: parent.to_vec(),
            parent_key: part.parent().cloned(),
            inside: false,
        })
    }

    /// `base`, or `base_2`, `base_3` and so on: the first name no part in the scene and no
    /// block in the parent uses, ignoring ASCII case.
    fn free_name(&self, home: &Home, base: &str) -> String {
        let parent: Vec<&str> = home.parent.iter().map(String::as_str).collect();
        let blocks = (self.document(home.file))
            .map(|doc| live_blocks(doc, &parent, self.platform()))
            .unwrap_or_default();
        let taken = |name: &str| {
            self.parts()
                .iter()
                .any(|p| p.name().eq_ignore_ascii_case(name))
                || (blocks.iter().flat_map(|b| b.children()))
                    .any(|e| e.key().eq_ignore_ascii_case(name))
        };
        (1u32..)
            .map(|n| match n {
                1 => base.to_owned(),
                n => format!("{base}_{n}"),
            })
            .find(|name| !taken(name))
            .unwrap_or_else(|| base.to_owned())
    }

    /// The label's resolved keys, renamed, pinned one unit down and right of the label, one
    /// layer below, in the scheme's `<colour>Shadow` or black.
    fn shadow_lines(&self, label: &Part, name: &str) -> Vec<(String, String)> {
        let slot = ["fgcolor", "fgcolor_override"]
            .into_iter()
            .find(|k| label.value(k).is_some())
            .unwrap_or(match label.class() {
                Some(c) if c.eq_ignore_ascii_case("Label") => "fgcolor_override",
                _ => "fgcolor",
            });
        let named = (label.value(slot).map(KeyLine::text))
            .filter(|t| !t.trim_start().starts_with(|c: char| c.is_ascii_digit()))
            .map(|t| format!("{t}Shadow"))
            .filter(|t| self.scheme().named_colour(t).is_some());
        let colour = named.unwrap_or_else(|| "0 0 0 255".to_owned());
        let replaced = ["fieldName", "xpos", "ypos", "zpos", "ControlName", slot];
        let mut lines = vec![
            (
                "ControlName".to_owned(),
                label.class().unwrap_or("CExLabel").to_owned(),
            ),
            ("fieldName".to_owned(), name.to_owned()),
        ];
        lines.extend(
            (label.settings().into_iter())
                .filter(|l| {
                    !(replaced.iter().chain(PIN_KEYS.iter()))
                        .any(|k| k.eq_ignore_ascii_case(l.key()))
                })
                .map(|l| (l.key().to_owned(), l.text().to_owned())),
        );
        lines.extend(pinned_to(label.name(), 1));
        lines.push(("zpos".to_owned(), layer_below(label).to_string()));
        lines.push((slot.to_owned(), colour));
        lines
    }

    /// A translucent panel behind `part`: pinned two units up and left and four units larger,
    /// or filling an element from inside.
    fn box_lines(&self, part: &Part, name: &str, inside: bool) -> Vec<(String, String)> {
        let mut lines = vec![
            ("ControlName".to_owned(), "EditablePanel".to_owned()),
            ("fieldName".to_owned(), name.to_owned()),
        ];
        let own = |pairs: &[(&str, &str)]| -> Vec<(String, String)> {
            (pairs.iter())
                .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
                .collect()
        };
        if inside {
            // `f0` measures the parent only with `proportionaltoparent`.
            lines.extend(own(&[
                ("xpos", "0"),
                ("ypos", "0"),
                ("wide", "f0"),
                ("tall", "f0"),
                ("proportionaltoparent", "1"),
                ("zpos", "-10"),
            ]));
        } else {
            let layout = self.layout(READ_AT);
            let (wide, tall) = (layout.get(part.key())).map_or((0, 0), |b| (b.wide(), b.tall()));
            let size = |px: i32| {
                px_to_units(px, READ_AT.tall())
                    .saturating_add(4)
                    .to_string()
            };
            lines.extend(pinned_to(part.name(), -2));
            lines.push(("wide".to_owned(), size(wide)));
            lines.push(("tall".to_owned(), size(tall)));
            lines.push(("zpos".to_owned(), layer_below(part).to_string()));
        }
        lines.extend(own(&[
            ("bgcolor_override", "0 0 0 120"),
            ("PaintBackgroundType", "2"),
            ("paintbackground", "1"),
        ]));
        lines
    }
}

/// An unpainted 8x8 panel centred in its parent.
fn anchor_lines(name: &str) -> Vec<(String, String)> {
    [
        ("ControlName", "EditablePanel"),
        ("fieldName", name),
        ("xpos", "cs-0.5"),
        ("ypos", "cs-0.5"),
        ("wide", "8"),
        ("tall", "8"),
        // `cs` measures the parent only with `proportionaltoparent`.
        ("proportionaltoparent", "1"),
        ("visible", "1"),
    ]
    .into_iter()
    .map(|(k, v)| (k.to_owned(), v.to_owned()))
    .collect()
}

/// Pin keys to `sibling` by both top-left corners, offset `units` on each axis.
fn pinned_to(sibling: &str, units: i32) -> Vec<(String, String)> {
    let units = units.to_string();
    [
        ("pin_to_sibling", sibling),
        ("pin_corner_to_sibling", "PIN_TOPLEFT"),
        ("pin_to_sibling_corner", "PIN_TOPLEFT"),
        ("xpos", &units),
        ("ypos", &units),
    ]
    .into_iter()
    .map(|(k, v)| (k.to_owned(), v.to_owned()))
    .collect()
}

/// One layer below the part; an unset layer counts as 0.
fn layer_below(part: &Part) -> i32 {
    part.value("zpos")
        .map_or(0, |l| atoi(l.text().as_bytes()))
        .saturating_sub(1)
}

/// The index of the AddBlock that made `part` and the new block as (file, path).
fn added_at(changes: &[Change], part: &Part) -> Option<(usize, (String, Vec<String>))> {
    changes.iter().enumerate().find_map(|(i, change)| {
        let ChangeTarget::AddBlock(add) = change.target() else {
            return None;
        };
        let mut path = add.parent().to_vec();
        path.push(add.name().to_owned());
        let made = (part.blocks().iter()).any(|(file, block)| {
            same_file(file, add.file())
                && block.len() == path.len()
                && (block.iter().zip(&path)).all(|(a, b)| a.eq_ignore_ascii_case(b))
        });
        made.then(|| (i, (add.file().to_owned(), path)))
    })
}

/// The DamagedPlayer lines that stop and rerun the hit flash, as every flash writes them.
fn trigger_lines() -> String {
    let event = || HIT_MARKER.to_owned();
    [
        AnimCommand::StopEvent {
            event: event(),
            delay: 0.0,
        },
        AnimCommand::RunEvent {
            event: event(),
            delay: SNAP,
        },
    ]
    .map(|c| c.to_line())
    .join("\n")
}

/// Whether two game paths name the same file once normalised.
fn same_file(a: &str, b: &str) -> bool {
    match (check_hud_path(a), check_hud_path(b)) {
        (Some(a), Some(b)) => a == b,
        _ => false,
    }
}

/// Whether the part draws text the game can recolour: a `CExLabel` or a `Label`.
pub(super) fn is_label(part: &Part) -> bool {
    part.class()
        .is_some_and(|c| c.eq_ignore_ascii_case("CExLabel") || c.eq_ignore_ascii_case("Label"))
}
