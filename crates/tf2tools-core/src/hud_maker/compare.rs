//! Notes on a copy's changes after its base updates: which no longer fit, and which the
//! update also touched.

use super::{Change, ChangeTarget, HudFile, HudFiles, HudModel, KvEntry};

/// One change named for the player: the block, event or file it touches, and its key.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct ChangeNote {
    part: String,
    key: Option<String>,
}

impl ChangeNote {
    /// The block, event or file name the change touches.
    pub fn part(&self) -> &str {
        &self.part
    }

    /// The key the change sets or removes, if it has one.
    pub fn key(&self) -> Option<&str> {
        self.key.as_deref()
    }

    /// The note for one of the HUD's own options, named by its label.
    pub(crate) fn option(label: &str) -> ChangeNote {
        ChangeNote {
            part: label.to_owned(),
            key: None,
        }
    }

    /// The note for `change`; `None` for an option or a credit, which are never reported.
    pub(crate) fn of(change: &Change) -> Option<ChangeNote> {
        let last = |path: &[String]| path.last().cloned().unwrap_or_default();
        let (part, key) = match change.target() {
            ChangeTarget::Set(set) => (last(set.block()), Some(set.key().to_owned())),
            ChangeTarget::Remove(remove) => (last(remove.block()), Some(remove.key().to_owned())),
            ChangeTarget::AddBlock(add) => (add.name().to_owned(), None),
            ChangeTarget::Asset(asset) => {
                let name = asset.path().rsplit('/').next().unwrap_or(asset.path());
                (name.to_owned(), None)
            }
            ChangeTarget::Animation(a) => (a.event().to_owned(), None),
            ChangeTarget::AnimationAppend(a) => (a.event().to_owned(), None),
            ChangeTarget::Option(_) | ChangeTarget::Credit(_) => return None,
        };
        Some(ChangeNote { part, key })
    }
}

/// The notes for `saved` changes that are not among `applied`, in saved order; each applied
/// change accounts for one equal saved change.
pub(crate) fn left_out(saved: &[Change], applied: &[Change]) -> Vec<ChangeNote> {
    let mut unmatched: Vec<&Change> = applied.iter().collect();
    let mut notes = Vec::new();
    for change in saved {
        match unmatched.iter().position(|a| *a == change) {
            Some(at) => {
                unmatched.remove(at);
            }
            None => notes.extend(ChangeNote::of(change)),
        }
    }
    notes
}

/// The applied changes whose target the base also changed between `old` and `new`: a key's
/// values in that block and condition, a block of that name added under that parent, the
/// file at an asset's path, or an animation event. In change order, one note per change;
/// `Option` and `Credit` never.
pub fn both_changed(
    old_files: &HudFiles,
    old: &HudModel,
    new_files: &HudFiles,
    new: &HudModel,
    applied: &[Change],
) -> Vec<ChangeNote> {
    applied
        .iter()
        .filter(|change| match change.target() {
            ChangeTarget::Set(set) => {
                let cond = set.cond().map_or(AnyCond::None, AnyCond::Exactly);
                let values = |m: &HudModel| key_values(m, set.file(), set.block(), set.key(), cond);
                values(old) != values(new)
            }
            ChangeTarget::Remove(remove) => {
                // A remove with no condition takes every line of the key.
                let cond = remove.cond().map_or(AnyCond::Any, AnyCond::Exactly);
                let values =
                    |m: &HudModel| key_values(m, remove.file(), remove.block(), remove.key(), cond);
                values(old) != values(new)
            }
            ChangeTarget::AddBlock(add) => {
                let count = |m: &HudModel| blocks_named(m, add.file(), add.parent(), add.name());
                count(new) > count(old)
            }
            ChangeTarget::Asset(asset) => {
                let bytes =
                    |f: &HudFiles| f.read(asset.path()).ok().flatten().map(HudFile::into_bytes);
                bytes(old_files) != bytes(new_files)
            }
            ChangeTarget::Animation(a) => {
                old.animations().event(a.event()) != new.animations().event(a.event())
            }
            ChangeTarget::AnimationAppend(a) => {
                old.animations().event(a.event()) != new.animations().event(a.event())
            }
            ChangeTarget::Option(_) | ChangeTarget::Credit(_) => false,
        })
        .filter_map(ChangeNote::of)
        .collect()
}

/// Every block on `path` in `file` and the `#base` files merged into it (names ignoring
/// ASCII case, any condition); the root segment names whichever root each file has.
fn blocks_on<'a>(model: &'a HudModel, file: &str, path: &[String]) -> Vec<&'a KvEntry> {
    let Some((_, below)) = path.split_first() else {
        return Vec::new();
    };
    let mut level = model.merged_roots(file);
    for name in below {
        level = (level.into_iter())
            .flat_map(|block| block.children())
            .filter(|e| e.text().is_none() && e.key().eq_ignore_ascii_case(name))
            .collect();
    }
    level
}

/// Which lines' conditional a key comparison takes.
#[derive(Clone, Copy)]
enum AnyCond<'a> {
    /// Lines without a conditional.
    None,
    /// Lines with this conditional, ignoring ASCII case.
    Exactly(&'a str),
    /// Every line.
    Any,
}

/// The values of `key` under `cond` in the blocks on `path`, in file order.
fn key_values(
    model: &HudModel,
    file: &str,
    path: &[String],
    key: &str,
    cond: AnyCond,
) -> Vec<String> {
    let same_cond = |c: Option<&str>| match (c, cond) {
        (_, AnyCond::Any) | (None, AnyCond::None) => true,
        (Some(a), AnyCond::Exactly(b)) => a.eq_ignore_ascii_case(b),
        _ => false,
    };
    blocks_on(model, file, path)
        .into_iter()
        .flat_map(KvEntry::children)
        .filter(|e| e.key().eq_ignore_ascii_case(key) && same_cond(e.cond()))
        .filter_map(|e| e.text().map(str::to_owned))
        .collect()
}

/// How many blocks named `name` sit under the blocks on `parent`.
fn blocks_named(model: &HudModel, file: &str, parent: &[String], name: &str) -> usize {
    blocks_on(model, file, parent)
        .into_iter()
        .flat_map(KvEntry::children)
        .filter(|e| e.text().is_none() && e.key().eq_ignore_ascii_case(name))
        .count()
}
