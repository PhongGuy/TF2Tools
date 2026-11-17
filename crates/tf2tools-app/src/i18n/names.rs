//! The names of the game's classes, weapon slots and sound kinds in the current language.

use gpui_kit::SharedString;
use tf2tools_core::{SoundKind, TfClass, WeaponSlot};

use super::tr;

/// The class's full name, such as "Heavy".
pub fn class_name(class: TfClass) -> SharedString {
    // A class core adds later keeps its English name until the catalogue names it.
    match class {
        TfClass::Scout => tr!("common.class.scout"),
        TfClass::Soldier => tr!("common.class.soldier"),
        TfClass::Pyro => tr!("common.class.pyro"),
        TfClass::Demoman => tr!("common.class.demoman"),
        TfClass::Heavy => tr!("common.class.heavy"),
        TfClass::Engineer => tr!("common.class.engineer"),
        TfClass::Medic => tr!("common.class.medic"),
        TfClass::Sniper => tr!("common.class.sniper"),
        TfClass::Spy => tr!("common.class.spy"),
        _ => class.name().into(),
    }
}

/// The class's name for a header or tab about as wide as the English: English keeps the full
/// name, and a language whose full name is longer shortens it.
pub fn class_compact_name(class: TfClass) -> SharedString {
    match class {
        TfClass::Scout => tr!("common.class_compact.scout"),
        TfClass::Soldier => tr!("common.class_compact.soldier"),
        TfClass::Pyro => tr!("common.class_compact.pyro"),
        TfClass::Demoman => tr!("common.class_compact.demoman"),
        TfClass::Heavy => tr!("common.class_compact.heavy"),
        TfClass::Engineer => tr!("common.class_compact.engineer"),
        TfClass::Medic => tr!("common.class_compact.medic"),
        TfClass::Sniper => tr!("common.class_compact.sniper"),
        TfClass::Spy => tr!("common.class_compact.spy"),
        _ => class.name().into(),
    }
}

/// The weapon slot's name, such as "Melee".
pub fn slot_name(slot: WeaponSlot) -> SharedString {
    match slot {
        WeaponSlot::Primary => tr!("common.slot.primary"),
        WeaponSlot::Secondary => tr!("common.slot.secondary"),
        WeaponSlot::Melee => tr!("common.slot.melee"),
        _ => slot.name().into(),
    }
}

/// The weapon slot's name for a narrow header; only the longer Russian names shorten.
pub fn slot_short_name(slot: WeaponSlot) -> SharedString {
    match slot {
        WeaponSlot::Secondary => tr!("common.slot_short.secondary"),
        WeaponSlot::Melee => tr!("common.slot_short.melee"),
        WeaponSlot::Primary | _ => slot_name(slot),
    }
}

/// The sound kind's name, such as "Killsound".
pub fn sound_kind_name(kind: SoundKind) -> SharedString {
    match kind {
        SoundKind::Hitsound => tr!("common.sound_kind.hitsound"),
        SoundKind::Killsound => tr!("common.sound_kind.killsound"),
        _ => kind.name().into(),
    }
}
