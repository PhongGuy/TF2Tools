//! A copy's switches save over base bytes, reopen in force, and turn off back to the base.

mod common;

use common::hud_anims::ANIMS;
use common::hud_copy::{game_for, snapshot};
use common::hud_switches::{
    ADDED, DAMAGE_SWITCH, SLOT, choices, copy_with, dir, save, sha, switch_layer,
};
use common::library_env;
use tf2tools_core::{AnimationChange, Change, OptionChange, OptionValue, Platform, SetKey};

const MANIFEST: &str = "hudanimations_manifest\n{\n\t\"file\"\t\"scripts/hudanimations_tf.txt\"\n\t//\"file\"\t\"scripts/hudanimations_hitmarker.txt\"\n}\n";
const MANIFEST_PATH: &str = "scripts/hudanimations_manifest.txt";

fn sources(d: &std::path::Path) -> serde_json::Value {
    serde_json::from_slice::<serde_json::Value>(
        &std::fs::read(d.join(".tf2tools/maker.json")).unwrap(),
    )
    .unwrap()["sources"]
        .clone()
}

#[test]
fn switches_save_into_the_copy_and_reopen_in_force() {
    let env = library_env();
    let tf = game_for(&env, &[]);
    let copy = copy_with(&env, &switch_layer(&[]));
    save(&env, &tf, &copy, &choices());
    let d = dir(&env, &copy);
    assert!(
        std::fs::read_to_string(d.join("customizations/damage.res"))
            .unwrap()
            .contains("\n#base \"../resource/ui/damage_big.res\"")
    );
    assert_eq!(
        std::fs::read_to_string(d.join(".tf2tools/original/customizations/damage.res")).unwrap(),
        DAMAGE_SWITCH
    );
    assert!(
        d.join(ADDED).is_file() && !d.join("#customization/bh_player_largehealth.res").exists()
    );
    let files = env.lib.made_hud_files(copy.id(), &tf).unwrap();
    let made = env
        .lib
        .open_made_hud(copy.id(), &files, Platform::Linux)
        .unwrap();
    assert_eq!((made.changes().len(), made.dropped()), (2, 0));
    assert!(made.files().contains(ADDED));
    let align = made
        .pristine()
        .parts()
        .iter()
        .find(|p| p.name() == "PlayerStatusHealthValue")
        .and_then(|p| p.value("textAlignment"))
        .map(|l| l.text().to_owned());
    assert_eq!(align.as_deref(), Some("east"));
}

#[test]
fn an_added_path_is_recorded_and_hidden_from_the_base_on_reopen() {
    let env = library_env();
    let tf = game_for(&env, &[]);
    let copy = copy_with(&env, &switch_layer(&[]));
    save(&env, &tf, &copy, &choices());
    let d = dir(&env, &copy);
    assert_eq!(
        sources(&d)[ADDED],
        sha(&std::fs::read(d.join(ADDED)).unwrap())
    );
    let base = env.lib.made_hud_files(copy.id(), &tf).unwrap();
    assert!(!base.contains(ADDED));
    assert!(base.contains("#customization/bh_player_largehealth.res"));
    let made = env
        .lib
        .open_made_hud(copy.id(), &base, Platform::Linux)
        .unwrap();
    assert_eq!(
        made.options().value(SLOT, &[]),
        Some(OptionValue::Bool(false))
    );
}

#[test]
fn sources_hash_the_base_bytes_and_never_read_as_a_game_update() {
    let env = library_env();
    let tf = game_for(&env, &[]);
    let copy = copy_with(&env, &switch_layer(&[]));
    save(&env, &tf, &copy, &choices());
    let d = dir(&env, &copy);
    assert_eq!(
        sources(&d)["customizations/damage.res"],
        sha(DAMAGE_SWITCH.as_bytes())
    );
    let files = env.lib.made_hud_files(copy.id(), &tf).unwrap();
    assert!(
        !env.lib
            .open_made_hud(copy.id(), &files, Platform::Linux)
            .unwrap()
            .tf2_updated()
    );
}

#[test]
fn the_model_wins_on_a_path_both_write() {
    let env = library_env();
    let tf = game_for(&env, &[]);
    let mut layer = switch_layer(&[]);
    layer.push((MANIFEST_PATH.into(), MANIFEST.into()));
    layer.push(("scripts/hudanimations_tf.txt".into(), ANIMS.into()));
    layer.push((
        "scripts/hudanimations_hitmarker.txt".into(),
        "event HitMarker\n{\n}\n".into(),
    ));
    let copy = copy_with(&env, &layer);
    let changes = [
        Change::option(
            1,
            OptionChange::new(
                OptionChange::HUD,
                "m:scripts/hudanimations_hitmarker.txt",
                OptionValue::Bool(true),
            ),
        ),
        Change::animation(2, AnimationChange::new("HudLowAmmoPulse", "")),
    ];
    save(&env, &tf, &copy, &changes);
    let d = dir(&env, &copy);
    let manifest = std::fs::read_to_string(d.join(MANIFEST_PATH)).unwrap();
    assert!(manifest.contains("\t\"file\"\t\"scripts/hudanimations_tf2tools.txt\"\n"));
    assert!(manifest.contains("\n\t\"file\"\t\"scripts/hudanimations_hitmarker.txt\""));
    assert_eq!(sources(&d)[MANIFEST_PATH], sha(MANIFEST.as_bytes()));
}

#[test]
fn a_moved_path_stays_moved_on_later_saves_and_comes_back_when_off() {
    let env = library_env();
    let tf = game_for(&env, &[]);
    let copy = copy_with(&env, &switch_layer(&[]));
    let d = dir(&env, &copy);
    let before: Vec<(String, Vec<u8>)> = snapshot(&d)
        .into_iter()
        .filter(|(r, _)| !r.starts_with(".tf2tools/"))
        .collect();
    let mut more = choices();
    more.push(Change::set(
        3,
        SetKey::new(
            "resource/ui/hudammoweapons.res",
            &["Resource/UI/HudAmmoWeapons.res", "AmmoInClip"],
            "xpos",
            None,
            Some("10"),
            "20",
        ),
    ));
    for changes in [choices(), more.clone(), more] {
        save(&env, &tf, &copy, &changes);
        assert!(
            !d.join("#customization/bh_player_largehealth.res").exists() && d.join(ADDED).is_file()
        );
        assert!(
            d.join(".tf2tools/original/#customization/bh_player_largehealth.res")
                .is_file()
        );
    }
    save(&env, &tf, &copy, &[]);
    // Every save of a copy writes its credits file (Stage 7), so it is left out here.
    let after: Vec<(String, Vec<u8>)> = snapshot(&d)
        .into_iter()
        .filter(|(r, _)| !r.starts_with(".tf2tools/") && r != "TF2Tools-credits.txt")
        .collect();
    assert_eq!(after, before);
    assert!(
        !d.join(".tf2tools/original/customizations/damage.res")
            .exists()
    );
}

const SPEC_HEALTH: &str = "resource/ui/spectatorguihealth.res";
const SPEC_SLOT: &str = "resource/ui/_enabled/spectatorguihealth.res";

/// A copy whose scene file `spectatorguihealth.res` ships in an enabled slot, so turning the
/// slot off moves it onto the scene path, where the model reads it as a document.
fn scene_slot_copy(env: &common::LibraryEnv) -> tf2tools_core::HudEntry {
    let mut layer = switch_layer(&[]);
    layer.push((
        SPEC_SLOT.into(),
        "\"Resource/UI/SpectatorGUIHealth.res\"\n{\n\tPlayerStatusHealthValue\n\t{\n\t\txpos 10\n\t}\n}\n".into(),
    ));
    layer.push((
        "resource/ui/slots.res".into(),
        "#base \"_enabled/spectatorguihealth.res\"\n".into(),
    ));
    copy_with(env, &layer)
}

#[test]
fn the_model_wins_on_a_switched_scene_file_and_reopens_without_a_game_update() {
    let env = library_env();
    let tf = game_for(&env, &[]);
    let copy = scene_slot_copy(&env);
    let off = OptionChange::new(
        OptionChange::HUD,
        "b:resource/ui/spectatorguihealth.res",
        OptionValue::Bool(false),
    );
    let block = [
        "Resource/UI/SpectatorGUIHealth.res",
        "PlayerStatusHealthValue",
    ];
    let changes = [
        Change::option(1, off),
        Change::set(
            2,
            SetKey::new(SPEC_HEALTH, &block, "xpos", None, Some("10"), "20"),
        ),
    ];
    save(&env, &tf, &copy, &changes);
    let d = dir(&env, &copy);
    // The switch moved the file onto the scene path; the model's edit to it is what is written.
    let written = std::fs::read_to_string(d.join(SPEC_HEALTH)).unwrap();
    assert!(
        written.contains("xpos 20") && !written.contains("xpos 10"),
        "{written}"
    );
    assert!(!d.join(SPEC_SLOT).exists());
    let files = env.lib.made_hud_files(copy.id(), &tf).unwrap();
    let made = env
        .lib
        .open_made_hud(copy.id(), &files, Platform::Linux)
        .unwrap();
    assert_eq!((made.changes().len(), made.dropped()), (2, 0));
    assert!(!made.tf2_updated());
}
