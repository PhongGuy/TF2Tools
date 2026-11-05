//! Updating a copy with switches: dropped choices named from the old HUD, no false both-changed.

mod common;

use common::hud_copy::{game_for, import_stock_hud, store_item};
use common::hud_stock::HEALTH;
use common::hud_switches::{choices, copy_with, save, switch_layer};
use common::{LibraryEnv, library_env};
use tf2tools_core::{Change, CopyUpdated, HudEntry, Platform, SetKey};

const SLOT_FILE: &str = "#customization/bh_player_largehealth.res";
const HEALTH_FILE: &str = "resource/ui/hudplayerhealth.res";

fn update(
    env: &LibraryEnv,
    tf: &std::path::Path,
    copy: &HudEntry,
    layer: Vec<(String, String)>,
) -> CopyUpdated {
    let extra: Vec<(&str, &str)> = layer
        .iter()
        .map(|(p, t)| (p.as_str(), t.as_str()))
        .collect();
    let newer = import_stock_hud(env, "somehud", &extra, store_item());
    env.lib
        .update_copy(copy.id(), newer.id(), tf, Platform::Linux)
        .unwrap()
}
fn with_edit() -> Vec<Change> {
    let hv = ["Resource/UI/HudPlayerHealth.res", "PlayerStatusHealthValue"];
    let mut changes = choices();
    // The slot file (moved by a switch) gives textAlignment east; the player's edit overrides it.
    changes.push(Change::set(
        3,
        SetKey::new(
            HEALTH_FILE,
            &hv,
            "textAlignment",
            None,
            Some("east"),
            "west",
        ),
    ));
    changes
}
fn health_entry(health: &str) -> String {
    format!("#base \"../../#customization/_enabled/bh_player_largehealth.res\"\n{health}")
}

#[test]
fn a_dropped_choice_is_named_from_the_old_hud() {
    let env = library_env();
    let tf = game_for(&env, &[]);
    let copy = copy_with(&env, &switch_layer(&[]));
    save(&env, &tf, &copy, &choices());
    let mut layer = switch_layer(&[]);
    layer.retain(|(p, _)| p != SLOT_FILE);
    let updated = update(&env, &tf, &copy, layer);
    let left: Vec<(String, Option<String>)> = updated
        .left_out()
        .iter()
        .map(|n| (n.part().to_owned(), n.key().map(str::to_owned)))
        .collect();
    assert_eq!(left, [("Bh player largehealth".to_owned(), None)]);
}

#[test]
fn changes_elsewhere_in_switched_and_edited_files_are_not_both_changed() {
    let env = library_env();
    let tf = game_for(&env, &[]);
    let copy = copy_with(&env, &switch_layer(&[]));
    save(&env, &tf, &copy, &with_edit());
    // The new base changes the slot file (another key) and the edited file (another block).
    let slot = "\"Resource/UI/HudPlayerHealth.res\" { PlayerStatusHealthValue { textAlignment east wide 50 } }\n";
    let health = health_entry(&HEALTH.replace("    xpos 73", "    xpos 80"));
    let updated = update(
        &env,
        &tf,
        &copy,
        switch_layer(&[(SLOT_FILE, slot), (HEALTH_FILE, &health)]),
    );
    assert_eq!(
        (updated.both_changed().len(), updated.left_out().len()),
        (0, 0)
    );
}

#[test]
fn a_change_to_the_edited_key_in_a_switched_file_is_both_changed() {
    let env = library_env();
    let tf = game_for(&env, &[]);
    let copy = copy_with(&env, &switch_layer(&[]));
    save(&env, &tf, &copy, &with_edit());
    let slot = "\"Resource/UI/HudPlayerHealth.res\" { PlayerStatusHealthValue { textAlignment center } }\n";
    let updated = update(&env, &tf, &copy, switch_layer(&[(SLOT_FILE, slot)]));
    let both: Vec<Option<&str>> = updated.both_changed().iter().map(|n| n.key()).collect();
    assert_eq!(both, [Some("textAlignment")]);
}

#[test]
fn a_dropped_choice_neither_hud_offers_is_still_counted() {
    let env = library_env();
    let tf = game_for(&env, &[]);
    let copy = copy_with(&env, &switch_layer(&[]));
    save(&env, &tf, &copy, &[]);
    // A hand-edited maker.json holds a choice no version of the HUD offers.
    let maker = common::hud_switches::dir(&env, &copy).join(".tf2tools/maker.json");
    let mut json: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&maker).unwrap()).unwrap();
    let choice = serde_json::json!({
        "group": 1,
        "hud_option": { "source": "hud", "id": "c:customizations/gone_", "value": { "bool": true } },
    });
    json["changes"].as_array_mut().unwrap().push(choice);
    std::fs::write(&maker, serde_json::to_vec(&json).unwrap()).unwrap();
    // The newer base differs only in a file nothing touches.
    let newer = switch_layer(&[("resource/ui/newer.res", "\"R\" { }\n")]);
    let updated = update(&env, &tf, &copy, newer);
    assert_eq!(updated.left_out().len(), 1);
    assert_eq!(updated.left_out()[0].key(), None);
}
