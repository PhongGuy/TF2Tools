//! Applying the HUD, Sounds and Crosshairs slots in each mode, and marking a profile active.

use crate::common;

use common::{
    LibraryEnv, builtin_catalog, crosshair_env, gb, import_hud_as, import_wav_as, library_env,
    set_setting, sql, write_file,
};
use tf2tools_core::{
    ApplyProfileReport, ApplyStep, CrosshairRef, Database, DraftCrosshair, DraftItem, Library,
    Profile, ProfileDraft, ProfileError, ProfileItem, Provenance, SlotMode, SoundId, SoundKind,
};

fn leave_all(name: &str) -> ProfileDraft {
    ProfileDraft::new(name)
        .with_hud(SlotMode::Leave)
        .with_sounds(SlotMode::Leave)
        .with_crosshairs(SlotMode::Leave)
}

fn apply(lib: &Library, env: &LibraryEnv, p: &Profile) -> ApplyProfileReport {
    lib.apply_profile(p.id(), &env.tf2, &builtin_catalog(), false)
        .unwrap()
}

fn sound(lib: &Library, env: &LibraryEnv, kind: SoundKind) -> Option<SoundId> {
    lib.installed_sounds(&env.tf2)
        .unwrap()
        .slot(kind)
        .managed()
        .cloned()
}

fn scattergun() -> DraftCrosshair {
    DraftCrosshair::Set {
        weapon_script: "tf_weapon_scattergun".into(),
        crosshair: CrosshairRef::Builtin("cpma_9".into()),
    }
}

#[test]
fn a_hud_left_as_is_keeps_every_hud_and_asks_nothing_about_external_ones() {
    let env = library_env();
    let hud = import_hud_as(&env, "budhud", Provenance::Local);
    env.lib.install_hud(hud.id(), &env.tf2).unwrap();
    write_file(&env.tf2.custom_dir().join("oldhud/info.vdf"), b"x");
    let p = env.lib.create_profile(&leave_all("Keep")).unwrap();
    let report = apply(&env.lib, &env, &p);
    assert!(report.applied().is_empty(), "{report:?}");
    assert!(report.failed().is_empty(), "{report:?}");
    assert!(env.tf2.custom_dir().join(hud.folder_name()).is_dir());
    assert!(env.tf2.custom_dir().join("oldhud").is_dir());
    assert_eq!(env.lib.active_profile().unwrap().as_ref(), Some(p.id()));
}

#[test]
fn a_hud_set_to_none_is_uninstalled() {
    let env = library_env();
    let hud = import_hud_as(&env, "budhud", Provenance::Local);
    env.lib.install_hud(hud.id(), &env.tf2).unwrap();
    let p = env
        .lib
        .create_profile(&leave_all("Plain").with_hud(SlotMode::None))
        .unwrap();
    let report = apply(&env.lib, &env, &p);
    assert_eq!(report.applied(), [ApplyStep::Hud]);
    assert!(!env.tf2.custom_dir().join(hud.folder_name()).exists());
}

#[test]
fn sounds_left_as_is_keep_both_and_none_takes_both_out() {
    let env = library_env();
    let hit = import_wav_as(&env, "Ding", Provenance::Local);
    let kill = import_wav_as(&env, "Bell", Provenance::Local);
    env.lib
        .install_sound(hit.id(), SoundKind::Hitsound, &env.tf2)
        .unwrap();
    env.lib
        .install_sound(kill.id(), SoundKind::Killsound, &env.tf2)
        .unwrap();
    let left = env.lib.create_profile(&leave_all("Left")).unwrap();
    assert!(apply(&env.lib, &env, &left).applied().is_empty());
    assert_eq!(
        sound(&env.lib, &env, SoundKind::Hitsound),
        Some(hit.id().clone())
    );
    assert_eq!(
        sound(&env.lib, &env, SoundKind::Killsound),
        Some(kill.id().clone())
    );

    let none = env
        .lib
        .create_profile(&leave_all("None").with_sounds(SlotMode::None))
        .unwrap();
    let report = apply(&env.lib, &env, &none);
    assert_eq!(
        report.applied(),
        [ApplyStep::Hitsound, ApplyStep::Killsound]
    );
    assert_eq!(sound(&env.lib, &env, SoundKind::Hitsound), None);
    assert_eq!(sound(&env.lib, &env, SoundKind::Killsound), None);
}

#[test]
fn a_tf2_default_side_of_set_sounds_is_uninstalled() {
    let env = library_env();
    let old = import_wav_as(&env, "Ding", Provenance::Local);
    let new = import_wav_as(&env, "Beep", Provenance::Local);
    let kill = import_wav_as(&env, "Bell", Provenance::Local);
    env.lib
        .install_sound(old.id(), SoundKind::Hitsound, &env.tf2)
        .unwrap();
    env.lib
        .install_sound(kill.id(), SoundKind::Killsound, &env.tf2)
        .unwrap();
    let draft = leave_all("Beep").with_sounds(SlotMode::Set((
        Some(DraftItem::Pick(new.id().clone())),
        None,
    )));
    let p = env.lib.create_profile(&draft).unwrap();
    let report = apply(&env.lib, &env, &p);
    assert_eq!(
        report.applied(),
        [ApplyStep::Hitsound, ApplyStep::Killsound]
    );
    assert_eq!(
        sound(&env.lib, &env, SoundKind::Hitsound),
        Some(new.id().clone())
    );
    assert_eq!(sound(&env.lib, &env, SoundKind::Killsound), None);
}

#[test]
fn crosshairs_left_as_is_keep_the_assignments_and_none_clears_them() {
    let env = crosshair_env();
    env.lib
        .assign(
            "tf_weapon_scattergun",
            &CrosshairRef::Builtin("cpma_9".into()),
        )
        .unwrap();
    let left = env.lib.create_profile(&leave_all("Left")).unwrap();
    assert!(apply(&env.lib, &env, &left).applied().is_empty());
    assert_eq!(env.lib.assignments().unwrap().len(), 1);

    let none = env
        .lib
        .create_profile(&leave_all("None").with_crosshairs(SlotMode::None))
        .unwrap();
    assert_eq!(
        apply(&env.lib, &env, &none).applied(),
        [ApplyStep::Crosshairs]
    );
    assert!(env.lib.assignments().unwrap().is_empty());
}

#[test]
fn a_profile_saved_before_slot_modes_applies_as_it_did() {
    let env = crosshair_env();
    let hud = import_hud_as(&env, "budhud", Provenance::Local);
    let hit = import_wav_as(&env, "Ding", Provenance::Local);
    let kill = import_wav_as(&env, "Bell", Provenance::Local);
    env.lib
        .install_sound(kill.id(), SoundKind::Killsound, &env.tf2)
        .unwrap();
    let draft = ProfileDraft::new("Old")
        .with_hud(SlotMode::Set(DraftItem::Pick(hud.id().clone())))
        .with_sounds(SlotMode::Set((
            Some(DraftItem::Pick(hit.id().clone())),
            None,
        )))
        .with_crosshairs(SlotMode::Set(vec![scattergun()]));
    let p = env.lib.create_profile(&draft).unwrap();
    // Back to how a v22 build left the row, so migration 24 backfills it on the next open.
    sql(&env.dir)
        .execute_batch(
            "UPDATE profiles SET hud_mode = 'leave', sounds_mode = 'leave',
                 crosshairs_mode = 'leave';
             PRAGMA user_version = 22;",
        )
        .unwrap();
    let lib = Library::new(Database::open(&env.dir).unwrap(), env.dir.clone());
    let report = apply(&lib, &env, &p);
    assert_eq!(
        report.applied(),
        [
            ApplyStep::Hud,
            ApplyStep::Hitsound,
            ApplyStep::Killsound,
            ApplyStep::Crosshairs
        ]
    );
    assert!(env.tf2.custom_dir().join(hud.folder_name()).is_dir());
    assert_eq!(
        sound(&lib, &env, SoundKind::Hitsound),
        Some(hit.id().clone())
    );
    assert_eq!(sound(&lib, &env, SoundKind::Killsound), None);
    assert_eq!(lib.assignments().unwrap().len(), 1);
    assert_eq!(lib.active_profile().unwrap().as_ref(), Some(p.id()));
}

#[test]
fn the_report_carries_the_profile_as_its_references_were_rewritten() {
    let env = library_env();
    let hud = import_hud_as(&env, "budhud", gb(42));
    let draft = leave_all("P").with_hud(SlotMode::Set(DraftItem::Pick(hud.id().clone())));
    let p = env.lib.create_profile(&draft).unwrap();
    env.lib.delete_hud(hud.id()).unwrap();
    import_hud_as(&env, "budhud-v2", gb(42));
    let report = apply(&env.lib, &env, &p);
    let after = env.lib.profile(p.id()).unwrap();
    assert_ne!(after.updated_at(), p.updated_at());
    assert_eq!(report.profile(), &after);
    assert_eq!(
        report
            .profile()
            .hud()
            .as_set()
            .map(ProfileItem::folder_name),
        Some("budhud-v2")
    );
}

#[test]
fn marking_active_needs_the_profile_unchanged_since_it_was_read() {
    let env = library_env();
    let p = env.lib.create_profile(&leave_all("Main")).unwrap();
    let report = apply(&env.lib, &env, &p);
    assert_eq!(report.profile(), &env.lib.profile(p.id()).unwrap());
    set_setting(&env.dir, "active_profile_id", "");
    let edited = env
        .lib
        .update_profile(p.id(), &ProfileDraft::from_profile(&p).with_name("Main 2"))
        .unwrap();
    assert!(!env.lib.mark_profile_active(&p).unwrap());
    assert_eq!(env.lib.active_profile().unwrap(), None);
    assert!(env.lib.mark_profile_active(edited.profile()).unwrap());
    assert_eq!(env.lib.active_profile().unwrap().as_ref(), Some(p.id()));
}

#[test]
fn marking_a_deleted_profile_active_is_not_found() {
    let env = library_env();
    let p = env.lib.create_profile(&leave_all("Gone")).unwrap();
    env.lib.delete_profile(p.id()).unwrap();
    assert!(matches!(
        env.lib.mark_profile_active(&p),
        Err(ProfileError::NotFound { .. })
    ));
}
