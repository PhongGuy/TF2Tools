//! The Crosshairs page's loadout board model: one cell per class and slot, the weapons each
//! board click targets, and how many weapons differ from what is in the game. UI-free, so it tests alone.

use std::collections::{HashMap, HashSet};

use tf2tools_core::{Assignment, CrosshairRef, TfClass, Weapon, WeaponSlot, weapons};

/// The board's slot rows, in loadout order. Every class has a weapon in each.
pub const SLOTS: [WeaponSlot; 3] = [
    WeaponSlot::Primary,
    WeaponSlot::Secondary,
    WeaponSlot::Melee,
];

/// What one board click covers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BoardTarget {
    /// One class's weapons in one slot.
    Cell(TfClass, WeaponSlot),
    /// Every weapon of a class.
    Class(TfClass),
    /// One slot for every class.
    Slot(WeaponSlot),
    /// Every weapon.
    Everything,
}

impl BoardTarget {
    /// Whether `weapon` is one this target covers.
    pub fn covers(self, weapon: &Weapon) -> bool {
        match self {
            BoardTarget::Cell(class, slot) => weapon.class() == class && weapon.slot() == slot,
            BoardTarget::Class(class) => weapon.class() == class,
            BoardTarget::Slot(slot) => weapon.slot() == slot,
            BoardTarget::Everything => true,
        }
    }

    /// The covered weapon scripts, in `weapons()` order.
    pub fn scripts(self) -> Vec<&'static str> {
        weapons()
            .iter()
            .filter(|w| self.covers(w))
            .map(Weapon::script_name)
            .collect()
    }

    /// The covered weapons in player words, to follow "applied to" or "reset".
    pub fn describe(self) -> String {
        match self {
            BoardTarget::Cell(class, slot) => format!(
                "all {} {} weapons",
                class.name(),
                slot.name().to_lowercase()
            ),
            BoardTarget::Class(class) => format!("all {} weapons", class.name()),
            BoardTarget::Slot(slot) => {
                format!("every class's {} weapons", slot.name().to_lowercase())
            }
            BoardTarget::Everything => "every weapon".into(),
        }
    }
}

/// What a board cell shows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CellState {
    /// No weapon in the cell has a crosshair set, so TF2 draws its own.
    Default,
    /// Every weapon in the cell has this crosshair.
    Single(CrosshairRef),
    /// The weapons differ, or only some have one.
    Mixed,
}

/// The assignments by weapon script and the scripts an Install would change.
#[derive(Debug, Clone, Default)]
pub struct Loadout {
    assigned: HashMap<String, CrosshairRef>,
    waiting: HashSet<String>,
}

impl Loadout {
    pub fn new(
        assigned: impl IntoIterator<Item = (String, CrosshairRef)>,
        waiting: impl IntoIterator<Item = String>,
    ) -> Self {
        Self {
            assigned: assigned.into_iter().collect(),
            waiting: waiting.into_iter().collect(),
        }
    }

    pub fn from_assignments(assignments: &[Assignment], waiting: &[String]) -> Self {
        Self::new(
            assignments
                .iter()
                .map(|a| (a.weapon_script().to_owned(), a.crosshair().clone())),
            waiting.iter().cloned(),
        )
    }

    /// The cell for `class` in `slot`: one crosshair when every weapon there shares it.
    pub fn cell(&self, class: TfClass, slot: WeaponSlot) -> CellState {
        let mut shown = BoardTarget::Cell(class, slot)
            .scripts()
            .into_iter()
            .map(|s| self.assigned.get(s));
        let Some(first) = shown.next() else {
            return CellState::Default;
        };
        if shown.any(|other| other != first) {
            return CellState::Mixed;
        }
        first.map_or(CellState::Default, |c| CellState::Single(c.clone()))
    }

    /// Sets `crosshair` on every weapon `target` covers, as a finished assign leaves the table.
    pub fn assign(&mut self, target: BoardTarget, crosshair: &CrosshairRef) {
        for script in target.scripts() {
            self.assigned.insert(script.to_owned(), crosshair.clone());
        }
    }

    /// Takes every weapon `target` covers back to the game's own crosshair.
    pub fn clear(&mut self, target: BoardTarget) {
        for script in target.scripts() {
            self.assigned.remove(script);
        }
    }

    /// How many weapons an Install would change.
    pub fn waiting_count(&self) -> usize {
        self.waiting.len()
    }
}
