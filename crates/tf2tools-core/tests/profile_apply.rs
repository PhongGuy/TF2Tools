//! Applying a profile: preflight, best-effort steps, the report and the active id.

mod common;

#[cfg(unix)]
use std::{fs, os::unix::fs::PermissionsExt};

use common::{
    LibraryEnv, builtin_catalog, import_hud_as, import_png_as, import_wav_as, library_env,
    set_setting, setting, sql, tree, write_file,
};
use tf2tools_core::{
    ApplyProfileError, ApplyStep, CrosshairRef, DraftCrosshair, DraftSlot, HudId, MissingKind,
    Profile, ProfileDraft, ProfileError, ProfileId, Provenance, SoundKind,
};

const UI: &str = "tf2tools-sounds/sound/ui";
const ALL: [ApplyStep; 4] = [
    ApplyStep::Hud,
    ApplyStep::Hitsound,
    ApplyStep::Killsound,
    ApplyStep::Crosshairs,
];

fn builtin(weapon_script: &str, name: &str) -> DraftCrosshair {
    DraftCrosshair::Set {
        weapon_script: weapon_script.into(),
        crosshair: CrosshairRef::Builtin(name.into()),
    }
}

/// A profile with a HUD, both sounds (WAV), a builtin and a library crosshair.
fn full_profile() -> (LibraryEnv, Profile) {
    let env = library_env();
    let hud = import_hud_as(&env, "rayshud", Provenance::Local);
    let hit = import_wav_as(&env, "Ding", Provenance::Local);
    let kill = import_wav_as(&env, "Bell", Provenance::Local);
    let dot = import_png_as(&env, "Dot", Provenance::Local);
    let draft = ProfileDraft::new("Full")
        .with_hud(DraftSlot::Item(hud.id().clone()))
        .with_hitsound(DraftSlot::Item(hit.id().clone()))
        .with_killsound(DraftSlot::Item(kill.id().clone()))
        .with_crosshairs(vec![
            builtin("tf_weapon_scattergun", "cpma_9"),
            DraftCrosshair::Set {
                weapon_script: "tf_weapon_rocketlauncher".into(),
                crosshair: CrosshairRef::Library(dot.id().clone()),
            },
        ]);
    let profile = env.lib.create_profile(&draft).unwrap();
    (env, profile)
}

// Read-only directories make a step fail; Windows has no mode bits to set.
#[cfg(unix)]
fn chmod(path: &std::path::Path, mode: u32) {
    fs::set_permissions(path, fs::Permissions::from_mode(mode)).unwrap();
}

fn assigned(env: &LibraryEnv) -> Vec<String> {
    env.lib
        .assignments()
        .unwrap()
        .iter()
        .map(|a| a.weapon_script().to_owned())
        .collect()
}

#[test]
fn applies_every_step_and_marks_the_profile_active() {
    let (env, p) = full_profile();
    let custom = env.tf2.custom_dir();
    write_file(&custom.join("oldhud/info.vdf"), b"x");
    env.lib
        .assign("tf_weapon_minigun", &CrosshairRef::Builtin("cpma_9".into()))
        .unwrap();
    let named = env.lib.external_huds(&env.tf2).unwrap();
    let report = env
        .lib
        .apply_profile_discarding(p.id(), &env.tf2, &builtin_catalog(), false, &named)
        .unwrap();
    assert_eq!(report.applied(), ALL);
    assert!(report.skipped().is_empty());
    assert!(report.failed().is_empty());
    assert!(custom.join("rayshud/info.vdf").is_file());
    assert!(!custom.join("oldhud").exists());
    assert!(custom.join(UI).join("hitsound.wav").is_file());
    assert!(custom.join(UI).join("killsound.wav").is_file());
    assert!(custom.join("tf2tools-crosshairs").is_dir());
    assert_eq!(
        assigned(&env),
        ["tf_weapon_rocketlauncher", "tf_weapon_scattergun"]
    );
    assert_eq!(
        setting(&env.dir, "active_profile_id").as_deref(),
        Some(p.id().as_str())
    );
    assert_eq!(env.lib.active_profile().unwrap().as_ref(), Some(p.id()));
}

#[test]
fn missing_items_stop_the_apply_and_touch_nothing() {
    let (env, p) = full_profile();
    let hud_id = env.lib.list_huds().unwrap()[0].id().clone();
    env.lib.delete_hud(&hud_id).unwrap();
    let custom = env.tf2.custom_dir();
    write_file(&custom.join("oldhud/info.vdf"), b"x");
    set_setting(&env.dir, "active_profile_id", "other");
    let before = tree(&custom);
    match env
        .lib
        .apply_profile(p.id(), &env.tf2, &builtin_catalog(), false)
    {
        Err(ApplyProfileError::Missing(items)) => {
            assert_eq!(items.len(), 1);
            assert_eq!(items[0].kind(), &MissingKind::Hud);
        }
        other => panic!("expected Missing, got {other:?}"),
    }
    assert_eq!(tree(&custom), before);
    assert_eq!(
        setting(&env.dir, "active_profile_id").as_deref(),
        Some("other")
    );
}

#[test]
fn skip_missing_leaves_the_installed_hud_and_does_not_activate() {
    let (env, p) = full_profile();
    let hud_id = env.lib.list_huds().unwrap()[0].id().clone();
    env.lib.delete_hud(&hud_id).unwrap();
    let other = import_hud_as(&env, "otherhud", Provenance::Local);
    env.lib.install_hud(other.id(), &env.tf2).unwrap();
    set_setting(&env.dir, "active_profile_id", "other");
    let custom = env.tf2.custom_dir();
    let report = env
        .lib
        .apply_profile(p.id(), &env.tf2, &builtin_catalog(), true)
        .unwrap();
    assert_eq!(report.skipped().len(), 1);
    assert_eq!(report.skipped()[0].kind(), &MissingKind::Hud);
    assert_eq!(
        report.applied(),
        [
            ApplyStep::Hitsound,
            ApplyStep::Killsound,
            ApplyStep::Crosshairs
        ]
    );
    assert!(report.failed().is_empty());
    assert!(custom.join("otherhud/info.vdf").is_file());
    assert!(custom.join(UI).join("hitsound.wav").is_file());
    assert_eq!(setting(&env.dir, "active_profile_id").as_deref(), Some(""));
    assert_eq!(env.lib.active_profile().unwrap(), None);
}

#[test]
fn a_none_hud_still_asks_about_a_hand_placed_hud_when_another_item_is_skipped() {
    let env = library_env();
    let hit = import_wav_as(&env, "Ding", Provenance::Local);
    let draft = ProfileDraft::new("P").with_hitsound(DraftSlot::Item(hit.id().clone()));
    let p = env.lib.create_profile(&draft).unwrap();
    env.lib.delete_sound(hit.id(), None).unwrap();
    let custom = env.tf2.custom_dir();
    write_file(&custom.join("manual/info.vdf"), b"x");
    match env
        .lib
        .apply_profile(p.id(), &env.tf2, &builtin_catalog(), true)
    {
        Err(ApplyProfileError::ExternalHuds(huds)) => assert_eq!(huds.len(), 1),
        other => panic!("expected ExternalHuds, got {other:?}"),
    }
    let named = env.lib.external_huds(&env.tf2).unwrap();
    env.lib
        .apply_profile_discarding(p.id(), &env.tf2, &builtin_catalog(), true, &named)
        .unwrap();
    assert!(!custom.join("manual").exists());
}

#[test]
fn skip_missing_keeps_a_missing_sound_and_still_clears_a_none_slot() {
    let env = library_env();
    let hit = import_wav_as(&env, "Ding", Provenance::Local);
    let old_hit = import_wav_as(&env, "OldDing", Provenance::Local);
    let old_kill = import_wav_as(&env, "OldBell", Provenance::Local);
    let hud = import_hud_as(&env, "rayshud", Provenance::Local);
    env.lib.install_hud(hud.id(), &env.tf2).unwrap();
    // The profile wants "Ding" as hitsound and no killsound and no HUD.
    let draft = ProfileDraft::new("P").with_hitsound(DraftSlot::Item(hit.id().clone()));
    let p = env.lib.create_profile(&draft).unwrap();
    env.lib.delete_sound(hit.id(), None).unwrap();
    env.lib
        .install_sound(old_hit.id(), SoundKind::Hitsound, &env.tf2)
        .unwrap();
    env.lib
        .install_sound(old_kill.id(), SoundKind::Killsound, &env.tf2)
        .unwrap();
    let custom = env.tf2.custom_dir();
    let hitsound = custom.join(UI).join("hitsound.wav");
    let kept = std::fs::read(&hitsound).unwrap();
    let report = env
        .lib
        .apply_profile(p.id(), &env.tf2, &builtin_catalog(), true)
        .unwrap();
    assert_eq!(report.skipped()[0].kind(), &MissingKind::Hitsound);
    assert_eq!(
        report.applied(),
        [ApplyStep::Hud, ApplyStep::Killsound, ApplyStep::Crosshairs]
    );
    assert_eq!(std::fs::read(&hitsound).unwrap(), kept);
    assert!(!custom.join(UI).join("killsound.wav").exists());
    assert!(!custom.join("rayshud").exists());
    assert_eq!(env.lib.active_profile().unwrap(), None);
}

#[test]
fn a_skipped_crosshair_keeps_that_weapons_assignment_and_others_follow_the_profile() {
    let env = library_env();
    let draft = ProfileDraft::new("X").with_crosshairs(vec![
        builtin("tf_weapon_scattergun", "no_such_builtin"),
        builtin("tf_weapon_rocketlauncher", "cpma_9"),
        builtin("tf_weapon_minigun", "no_such_builtin"),
    ]);
    let p = env.lib.create_profile(&draft).unwrap();
    let kept = CrosshairRef::Builtin("cpma_9".into());
    env.lib.assign("tf_weapon_scattergun", &kept).unwrap();
    env.lib.assign("tf_weapon_sniperrifle", &kept).unwrap();
    let catalog = builtin_catalog();
    match env.lib.apply_profile(p.id(), &env.tf2, &catalog, false) {
        Err(ApplyProfileError::Missing(items)) => assert_eq!(
            items[0].kind(),
            &MissingKind::Crosshair {
                weapon_script: "tf_weapon_minigun".into()
            }
        ),
        other => panic!("expected Missing, got {other:?}"),
    }
    let report = env
        .lib
        .apply_profile(p.id(), &env.tf2, &catalog, true)
        .unwrap();
    assert_eq!(report.skipped().len(), 2);
    assert!(report.applied().contains(&ApplyStep::Crosshairs));
    // The skipped scattergun keeps its crosshair, the sniper rifle (not in the profile)
    // is cleared and the skipped minigun, never assigned, stays unassigned.
    let assignments = env.lib.assignments().unwrap();
    let scattergun = assignments
        .iter()
        .find(|a| a.weapon_script() == "tf_weapon_scattergun")
        .unwrap();
    assert_eq!(scattergun.crosshair(), &kept);
    assert_eq!(
        assigned(&env),
        ["tf_weapon_rocketlauncher", "tf_weapon_scattergun"]
    );
    assert_eq!(env.lib.active_profile().unwrap(), None);
}

// Needs a read-only directory, which `chmod` only makes on unix.
#[cfg(unix)]
#[test]
fn one_failing_step_is_reported_and_the_rest_still_apply() {
    let env = library_env();
    let hud = import_hud_as(&env, "rayshud", Provenance::Local);
    let hit = import_wav_as(&env, "Ding", Provenance::Local);
    let draft = ProfileDraft::new("P")
        .with_hud(DraftSlot::Item(hud.id().clone()))
        .with_hitsound(DraftSlot::Item(hit.id().clone()))
        .with_crosshairs(vec![builtin("tf_weapon_scattergun", "cpma_9")]);
    let p = env.lib.create_profile(&draft).unwrap();
    set_setting(&env.dir, "active_profile_id", "other");
    let custom = env.tf2.custom_dir();
    let ui = custom.join(UI);
    fs::create_dir_all(&ui).unwrap();
    chmod(&ui, 0o555);
    let report = env
        .lib
        .apply_profile(p.id(), &env.tf2, &builtin_catalog(), false);
    chmod(&ui, 0o755);
    let report = report.unwrap();
    assert_eq!(
        report.applied(),
        [ApplyStep::Hud, ApplyStep::Killsound, ApplyStep::Crosshairs]
    );
    assert_eq!(report.failed().len(), 1);
    assert_eq!(report.failed()[0].0, ApplyStep::Hitsound);
    assert!(!report.failed()[0].1.is_empty());
    assert!(custom.join("rayshud/info.vdf").is_file());
    assert_eq!(setting(&env.dir, "active_profile_id").as_deref(), Some(""));
}

// Needs a read-only directory, which `chmod` only makes on unix.
#[cfg(unix)]
#[test]
fn every_failing_step_leaves_no_active_profile() {
    let (env, p) = full_profile();
    set_setting(&env.dir, "active_profile_id", "other");
    let custom = env.tf2.custom_dir();
    chmod(&custom, 0o555);
    let report = env
        .lib
        .apply_profile(p.id(), &env.tf2, &builtin_catalog(), false);
    chmod(&custom, 0o755);
    let report = report.unwrap();
    assert!(report.applied().is_empty());
    let failed: Vec<ApplyStep> = report.failed().iter().map(|(s, _)| *s).collect();
    assert_eq!(failed, ALL);
    assert_eq!(setting(&env.dir, "active_profile_id").as_deref(), Some(""));
    assert_eq!(env.lib.active_profile().unwrap(), None);
}

#[test]
fn an_empty_profile_uninstalls_everything() {
    let env = library_env();
    let custom = env.tf2.custom_dir();
    write_file(&custom.join("oldhud/info.vdf"), b"x");
    write_file(&custom.join(UI).join("hitsound.wav"), b"x");
    write_file(&custom.join("tf2tools-killsound.vpk"), b"x");
    write_file(&custom.join("tf2tools-crosshairs/scripts/x.txt"), b"x");
    env.lib
        .assign("tf_weapon_minigun", &CrosshairRef::Builtin("cpma_9".into()))
        .unwrap();
    let p = env.lib.create_profile(&ProfileDraft::new("Bare")).unwrap();
    let named = env.lib.external_huds(&env.tf2).unwrap();
    let report = env
        .lib
        .apply_profile_discarding(p.id(), &env.tf2, &builtin_catalog(), false, &named)
        .unwrap();
    assert_eq!(report.applied(), ALL);
    assert_eq!(tree(&custom), Vec::<String>::new());
    assert!(assigned(&env).is_empty());
}

#[test]
fn applying_an_unknown_profile_is_not_found() {
    let env = library_env();
    assert!(matches!(
        env.lib
            .apply_profile(&ProfileId::new("gone"), &env.tf2, &builtin_catalog(), false),
        Err(ApplyProfileError::NotFound { .. })
    ));
}

#[test]
fn a_database_failure_while_resolving_is_a_database_error() {
    let (env, p) = full_profile();
    sql(&env.dir)
        .execute_batch("ALTER TABLE profile_crosshair_assignments RENAME TO moved_away")
        .unwrap();
    assert!(matches!(
        env.lib
            .apply_profile(p.id(), &env.tf2, &builtin_catalog(), false),
        Err(ApplyProfileError::Database { .. })
    ));
    assert!(matches!(
        env.lib.missing_items(p.id()),
        Err(ProfileError::Database { .. })
    ));
}

#[test]
fn a_crosshair_table_failure_names_the_assignments() {
    let (env, p) = full_profile();
    sql(&env.dir)
        .execute_batch(
            "CREATE TRIGGER no_assignments BEFORE INSERT ON crosshair_assignments
             BEGIN SELECT RAISE(ABORT, 'disk full'); END;",
        )
        .unwrap();
    let report = env
        .lib
        .apply_profile(p.id(), &env.tf2, &builtin_catalog(), false)
        .unwrap();
    assert_eq!(
        report.applied(),
        [ApplyStep::Hud, ApplyStep::Hitsound, ApplyStep::Killsound]
    );
    assert_eq!(report.failed().len(), 1);
    let (step, message) = &report.failed()[0];
    assert_eq!(*step, ApplyStep::Crosshairs);
    assert!(message.starts_with("crosshair assignments: "), "{message}");
    assert!(message.contains("disk full"), "{message}");
}

/// `full_profile`, applied, so its id is the active one.
fn active_profile() -> (LibraryEnv, Profile) {
    let (env, p) = full_profile();
    env.lib
        .apply_profile(p.id(), &env.tf2, &builtin_catalog(), false)
        .unwrap();
    assert_eq!(env.lib.active_profile().unwrap().as_ref(), Some(p.id()));
    (env, p)
}

/// Saves `edit` of the active profile and pins that it ended the active state without
/// touching a game file.
fn assert_edit_deactivates(env: &LibraryEnv, p: &Profile, edit: ProfileDraft) {
    let custom = env.tf2.custom_dir();
    let before = tree(&custom);
    let update = env.lib.update_profile(p.id(), &edit).unwrap();
    assert!(update.deactivated());
    assert_eq!(env.lib.active_profile().unwrap(), None);
    assert_eq!(tree(&custom), before);
}

#[test]
fn changing_the_hud_of_the_active_profile_ends_its_active_state() {
    let (env, p) = active_profile();
    let other = import_hud_as(&env, "budhud", Provenance::Local);
    let draft = ProfileDraft::from_profile(&p).with_hud(DraftSlot::Item(other.id().clone()));
    assert_edit_deactivates(&env, &p, draft);
    assert!(env.tf2.custom_dir().join("rayshud/info.vdf").is_file());
}

#[test]
fn emptying_the_hud_of_the_active_profile_ends_its_active_state() {
    let (env, p) = active_profile();
    let draft = ProfileDraft::from_profile(&p).with_hud(DraftSlot::Empty);
    assert_edit_deactivates(&env, &p, draft);
}

#[test]
fn changing_the_hitsound_of_the_active_profile_ends_its_active_state() {
    let (env, p) = active_profile();
    let other = import_wav_as(&env, "Pop", Provenance::Local);
    let draft = ProfileDraft::from_profile(&p).with_hitsound(DraftSlot::Item(other.id().clone()));
    assert_edit_deactivates(&env, &p, draft);
}

#[test]
fn changing_the_killsound_of_the_active_profile_ends_its_active_state() {
    let (env, p) = active_profile();
    let other = import_wav_as(&env, "Pop", Provenance::Local);
    let draft = ProfileDraft::from_profile(&p).with_killsound(DraftSlot::Item(other.id().clone()));
    assert_edit_deactivates(&env, &p, draft);
}

#[test]
fn changing_a_crosshair_of_the_active_profile_ends_its_active_state() {
    let (env, p) = active_profile();
    // The same two weapons, so only the scattergun's crosshair can tell the setups apart.
    let draft = ProfileDraft::from_profile(&p).with_crosshairs(vec![
        DraftCrosshair::Keep(p.crosshairs()[0].clone()),
        builtin("tf_weapon_scattergun", "cpma_1"),
    ]);
    assert_edit_deactivates(&env, &p, draft);
}

#[test]
fn moving_a_crosshair_to_another_weapon_ends_the_active_state() {
    let (env, p) = active_profile();
    // The sniper rifle sorts where the scattergun did, so only the weapon name differs.
    let draft = ProfileDraft::from_profile(&p).with_crosshairs(vec![
        DraftCrosshair::Keep(p.crosshairs()[0].clone()),
        builtin("tf_weapon_sniperrifle", "cpma_9"),
    ]);
    assert_edit_deactivates(&env, &p, draft);
}

#[test]
fn renaming_the_active_profile_keeps_it_active() {
    let (env, p) = active_profile();
    let draft = ProfileDraft::from_profile(&p).with_name("Renamed");
    let update = env.lib.update_profile(p.id(), &draft).unwrap();
    assert!(!update.deactivated());
    assert_eq!(update.profile().name(), "Renamed");
    assert_eq!(env.lib.active_profile().unwrap().as_ref(), Some(p.id()));
}

// Naming the saved items by id instead of `Keep` is the same setup, so it stays active.
#[test]
fn saving_the_same_items_again_keeps_the_profile_active() {
    let (env, p) = active_profile();
    let huds = env.lib.list_huds().unwrap();
    let sound = |folder: &str| {
        let sounds = env.lib.list_sounds().unwrap();
        let found = sounds.iter().find(|s| s.folder_name() == folder).unwrap();
        DraftSlot::Item(found.id().clone())
    };
    let library = env.lib.list_crosshairs().unwrap();
    let draft = ProfileDraft::from_profile(&p)
        .with_hud(DraftSlot::Item(huds[0].id().clone()))
        .with_hitsound(sound(p.hitsound().unwrap().folder_name()))
        .with_killsound(sound(p.killsound().unwrap().folder_name()))
        .with_crosshairs(vec![
            builtin("tf_weapon_scattergun", "cpma_9"),
            DraftCrosshair::Set {
                weapon_script: "tf_weapon_rocketlauncher".into(),
                crosshair: CrosshairRef::Library(library[0].id().clone()),
            },
        ]);
    let update = env.lib.update_profile(p.id(), &draft).unwrap();
    assert!(!update.deactivated());
    assert_eq!(env.lib.active_profile().unwrap().as_ref(), Some(p.id()));
}

#[test]
fn a_refused_edit_keeps_the_profile_active() {
    let (env, p) = active_profile();
    let draft = ProfileDraft::from_profile(&p).with_hud(DraftSlot::Item(HudId::new("gone")));
    assert!(matches!(
        env.lib.update_profile(p.id(), &draft),
        Err(ProfileError::ItemNotFound { .. })
    ));
    assert_eq!(env.lib.active_profile().unwrap().as_ref(), Some(p.id()));
}

/// Fails every insert into `table`, then pins that an edit of the HUD and crosshairs leaves
/// the profile and its active state as they were. The same edit saves once the trigger is gone.
fn assert_failed_write_rolls_back(table: &str) {
    let (env, p) = active_profile();
    let other = import_hud_as(&env, "budhud", Provenance::Local);
    let draft = ProfileDraft::from_profile(&p)
        .with_hud(DraftSlot::Item(other.id().clone()))
        .with_crosshairs(vec![builtin("tf_weapon_minigun", "cpma_1")]);
    sql(&env.dir)
        .execute_batch(&format!(
            "CREATE TRIGGER no_inserts BEFORE INSERT ON {table}
             BEGIN SELECT RAISE(ABORT, 'no'); END;"
        ))
        .unwrap();
    assert!(matches!(
        env.lib.update_profile(p.id(), &draft),
        Err(ProfileError::Database { .. })
    ));
    assert_eq!(env.lib.profile(p.id()).unwrap(), p);
    assert_eq!(env.lib.active_profile().unwrap().as_ref(), Some(p.id()));

    sql(&env.dir)
        .execute_batch("DROP TRIGGER no_inserts;")
        .unwrap();
    assert_edit_deactivates(&env, &p, draft);
}

#[test]
fn a_failed_active_clear_saves_nothing() {
    assert_failed_write_rolls_back("settings");
}

#[test]
fn a_failed_crosshair_write_saves_nothing() {
    assert_failed_write_rolls_back("profile_crosshair_assignments");
}

#[test]
fn editing_another_profile_leaves_the_active_one_alone() {
    let (env, p) = active_profile();
    let other = env.lib.create_profile(&ProfileDraft::new("Other")).unwrap();
    let hud = env.lib.list_huds().unwrap()[0].id().clone();
    let draft = ProfileDraft::from_profile(&other).with_hud(DraftSlot::Item(hud));
    let update = env.lib.update_profile(other.id(), &draft).unwrap();
    assert!(!update.deactivated());
    assert_eq!(env.lib.active_profile().unwrap().as_ref(), Some(p.id()));
}

#[test]
fn an_edited_profile_activates_again_with_the_new_hud() {
    let (env, p) = active_profile();
    let other = import_hud_as(&env, "budhud", Provenance::Local);
    let draft = ProfileDraft::from_profile(&p).with_hud(DraftSlot::Item(other.id().clone()));
    assert_edit_deactivates(&env, &p, draft);
    let report = env
        .lib
        .apply_profile(p.id(), &env.tf2, &builtin_catalog(), false)
        .unwrap();
    assert_eq!(report.applied(), ALL);
    let custom = env.tf2.custom_dir();
    assert!(custom.join("budhud/info.vdf").is_file());
    assert!(!custom.join("rayshud").exists());
    assert_eq!(env.lib.active_profile().unwrap().as_ref(), Some(p.id()));
}
