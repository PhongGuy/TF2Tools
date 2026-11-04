//! The weapon table ported from v5's `src/lib/data/weapons.ts`.

use std::collections::HashSet;
use tf2tools_core::{TfClass, WeaponSlot, weapons};

fn fnv1a(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |hash, &b| {
        (hash ^ u64::from(b)).wrapping_mul(0x0000_0100_0000_01b3)
    })
}

#[test]
fn the_table_has_v5s_64_weapons_in_order() {
    let all = weapons();
    assert_eq!(all.len(), 64);
    let first = &all[0];
    assert_eq!(first.class(), TfClass::Scout);
    assert_eq!(first.script_name(), "tf_weapon_pep_brawler_blaster");
    assert_eq!(first.display_name(), "Baby Face's Blaster");
    assert_eq!(first.slot(), WeaponSlot::Primary);
    assert_eq!(first.weapons_affected(), ["Baby Face's Blaster"]);
    let last = &all[63];
    assert_eq!(last.class(), TfClass::Spy);
    assert_eq!(last.script_name(), "tf_weapon_builder");
    assert_eq!(last.display_name(), "Stock Sapper");
    assert_eq!(last.slot(), WeaponSlot::Secondary);
    assert_eq!(last.weapons_affected(), ["Sapper"]);
}

#[test]
fn script_names_are_unique() {
    let names: HashSet<&str> = weapons().iter().map(|w| w.script_name()).collect();
    assert_eq!(names.len(), 64);
}

#[test]
fn class_and_slot_counts_match_v5() {
    let per_class: Vec<usize> = TfClass::ALL
        .iter()
        .map(|c| weapons().iter().filter(|w| w.class() == *c).count())
        .collect();
    assert_eq!(per_class, [13, 8, 8, 7, 4, 8, 4, 8, 4]);
    let per_slot: Vec<usize> = [
        WeaponSlot::Primary,
        WeaponSlot::Secondary,
        WeaponSlot::Melee,
    ]
    .iter()
    .map(|s| weapons().iter().filter(|w| w.slot() == *s).count())
    .collect();
    assert_eq!(per_slot, [23, 24, 17]);
    let affected: usize = weapons().iter().map(|w| w.weapons_affected().len()).sum();
    assert_eq!(affected, 269);
}

#[test]
fn every_field_matches_v5() {
    let text: String = weapons()
        .iter()
        .map(|w| {
            format!(
                "{}|{}|{}|{}|{}\n",
                w.class().name(),
                w.script_name(),
                w.display_name(),
                w.slot().name(),
                w.weapons_affected().join(";")
            )
        })
        .collect();
    assert_eq!(
        fnv1a(text.as_bytes()),
        0xf54e_f907_e056_dbfd,
        "table differs from weapons.ts"
    );
}

#[test]
fn class_names_follow_v5s_order() {
    let names: Vec<&str> = TfClass::ALL.iter().map(|c| c.name()).collect();
    assert_eq!(
        names,
        [
            "Scout", "Soldier", "Pyro", "Demoman", "Heavy", "Engineer", "Medic", "Sniper", "Spy"
        ]
    );
}
