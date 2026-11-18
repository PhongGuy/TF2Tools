//! Names a history step in player words for the Undo and Redo tooltips, from the changes it
//! adds and removes: "move Health value", "hide Ammo", "add a part".

use std::cmp::Ordering;

use tf2tools_core::{Change, ChangeTarget, HudModel, Part, PartKey, SetKey};

use super::parts::row_label;
use crate::i18n::tr;

/// What a step does to its one part, which names it.
#[derive(Clone, Copy)]
enum Verb {
    Hide,
    Show,
    Move,
    Resize,
    Recolour,
    Font,
    Text,
    Layer,
    Picture,
    Change,
}

// The one-part step in player words, each verb a whole phrase around the part's name.
fn part_step(verb: Verb, name: &str) -> String {
    match verb {
        Verb::Hide => tr!("hud_maker.step.part.hide", name = name),
        Verb::Show => tr!("hud_maker.step.part.show", name = name),
        Verb::Move => tr!("hud_maker.step.part.move", name = name),
        Verb::Resize => tr!("hud_maker.step.part.resize", name = name),
        Verb::Recolour => tr!("hud_maker.step.part.recolour", name = name),
        Verb::Font => tr!("hud_maker.step.part.font", name = name),
        Verb::Text => tr!("hud_maker.step.part.text", name = name),
        Verb::Layer => tr!("hud_maker.step.part.layer", name = name),
        Verb::Picture => tr!("hud_maker.step.part.picture", name = name),
        Verb::Change => tr!("hud_maker.step.part.change", name = name),
    }
    .to_string()
}

/// The edit from `before` to `after` in player words; `None` when it has no short name.
/// `model` is the HUD before the edit, `pristine` the starting HUD, which holds removed parts.
pub(super) fn name(
    pristine: &HudModel,
    model: &HudModel,
    before: &[Change],
    after: &[Change],
) -> Option<String> {
    let (removed, added) = differing(before, after);
    let all = || removed.iter().chain(&added).map(|c| c.target());
    // Steps that bring whole parts are named for that, whatever settings came along.
    if all().any(|t| matches!(t, ChangeTarget::Credit(_))) {
        return Some(tr!("hud_maker.step.borrow").to_string());
    }
    let blocks = |list: &[&Change]| {
        list.iter()
            .any(|c| matches!(c.target(), ChangeTarget::AddBlock(_)))
    };
    if blocks(&added) {
        return Some(tr!("hud_maker.step.add").to_string());
    }
    if blocks(&removed) {
        return Some(tr!("hud_maker.step.delete").to_string());
    }
    let sets: Vec<&SetKey> = all()
        .filter_map(|t| match t {
            ChangeTarget::Set(set) => Some(set),
            ChangeTarget::Remove(_)
            | ChangeTarget::AddBlock(_)
            | ChangeTarget::Asset(_)
            | ChangeTarget::Option(_)
            | ChangeTarget::Credit(_)
            | ChangeTarget::Animation(_)
            | ChangeTarget::AnimationAppend(_)
            | _ => None,
        })
        .collect();
    match owners(model, pristine, &sets).as_slice() {
        [] => {}
        [part] => {
            let verb = verb(model, part.key(), &sets, before, after);
            return Some(part_step(verb, &row_label(part)));
        }
        _ => return Some(tr!("hud_maker.step.several").to_string()),
    }
    if all().any(|t| matches!(t, ChangeTarget::Option(_))) {
        return Some(tr!("hud_maker.step.hud_option").to_string());
    }
    if all().any(|t| {
        matches!(
            t,
            ChangeTarget::Animation(_) | ChangeTarget::AnimationAppend(_)
        )
    }) {
        return Some(tr!("hud_maker.step.animation").to_string());
    }
    None
}

// The changes only `before` holds and those only `after` holds. A step mostly appends or drops
// one group, so the shared ends are trimmed before the quadratic compare of the middle.
fn differing<'a>(before: &'a [Change], after: &'a [Change]) -> (Vec<&'a Change>, Vec<&'a Change>) {
    let head = before.iter().zip(after).take_while(|(a, b)| a == b).count();
    let (b, a) = (&before[head..], &after[head..]);
    let tail = (b.iter().rev().zip(a.iter().rev()))
        .take_while(|(x, y)| x == y)
        .count();
    let (b, a) = (&b[..b.len() - tail], &a[..a.len() - tail]);
    let only =
        |from: &'a [Change], to: &[Change]| from.iter().filter(|c| !to.contains(c)).collect();
    (only(b, a), only(a, b))
}

// The parts the sets write, at most two since more are only "several"; the deepest part whose
// block holds a set owns it, so a child's line never counts for its parent.
fn owners<'a>(model: &'a HudModel, pristine: &'a HudModel, sets: &[&SetKey]) -> Vec<&'a Part> {
    let mut found: Vec<&Part> = Vec::new();
    for set in sets {
        let Some(part) = owner(model, set).or_else(|| owner(pristine, set)) else {
            continue;
        };
        if !found.iter().any(|p| p.key() == part.key()) {
            found.push(part);
            if found.len() > 1 {
                break;
            }
        }
    }
    found
}

fn owner<'a>(model: &'a HudModel, set: &SetKey) -> Option<&'a Part> {
    (model.parts().iter())
        .filter_map(|part| {
            let depth = (part.blocks().iter())
                .filter(|(file, block)| holds(file, block, set))
                .map(|(_, block)| block.len())
                .max()?;
            Some((depth, part))
        })
        .max_by_key(|&(depth, _)| depth)
        .map(|(_, part)| part)
}

// Whether a set lands in `block` of `file`, or in an `if_*` sub-block of it.
fn holds(file: &str, block: &[String], set: &SetKey) -> bool {
    let path = set.block();
    let extra = match path.len().checked_sub(block.len()) {
        Some(0) => true,
        Some(1) => path[block.len()].to_ascii_lowercase().starts_with("if_"),
        _ => false,
    };
    extra
        && file.eq_ignore_ascii_case(set.file())
        && block
            .iter()
            .zip(path)
            .all(|(a, b)| a.eq_ignore_ascii_case(b))
}

// What the sets do to `part`: a hide or show by the eye's switch first, then by the keys written.
fn verb(
    model: &HudModel,
    part: &PartKey,
    sets: &[&SetKey],
    before: &[Change],
    after: &[Change],
) -> Verb {
    let hides = |changes: &[Change]| model.hide_groups(part, changes).len();
    match hides(after).cmp(&hides(before)) {
        Ordering::Greater => return Verb::Hide,
        Ordering::Less => return Verb::Show,
        Ordering::Equal => {}
    }
    let keys: Vec<String> = sets.iter().map(|s| s.key().to_ascii_lowercase()).collect();
    let any = |names: &[&str]| keys.iter().any(|k| names.contains(&k.as_str()));
    if keys.iter().all(|k| k == "xpos" || k == "ypos") {
        Verb::Move
    } else if any(&["wide", "tall"]) {
        Verb::Resize
    } else if keys.iter().any(|k| k.contains("color")) {
        Verb::Recolour
    } else if any(&["font"]) {
        Verb::Font
    } else if any(&["labeltext"]) {
        Verb::Text
    } else if any(&["zpos"]) {
        Verb::Layer
    } else if any(&["image"]) {
        Verb::Picture
    } else {
        Verb::Change
    }
}
