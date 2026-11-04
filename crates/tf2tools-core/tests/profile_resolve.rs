//! Resolving profile references by folder, then by store id, and reporting the rest missing.

mod common;

use std::fs;

use common::{
    LibraryEnv, count, gb, import_hud_as, import_png_as, import_wav_as, library_env, sql,
};
use tf2tools_core::{
    CrosshairRef, DraftCrosshair, DraftSlot, LibraryKind, MissingKind, Profile,
    ProfileCrosshairRef, ProfileDraft, ProfileError, ProfileId, Provenance,
};

fn hud_profile(env: &LibraryEnv, folder: &str, provenance: Provenance) -> Profile {
    let hud = import_hud_as(env, folder, provenance);
    let draft = ProfileDraft::new("P").with_hud(DraftSlot::Item(hud.id().clone()));
    let p = env.lib.create_profile(&draft).unwrap();
    env.lib.delete_hud(hud.id()).unwrap();
    p
}

fn hud_dir(env: &LibraryEnv, folder: &str) -> std::path::PathBuf {
    env.dir.library_dir(LibraryKind::Huds).join(folder)
}

#[test]
fn a_profile_whose_items_exist_has_nothing_missing() {
    let env = library_env();
    let hud = import_hud_as(&env, "budhud", Provenance::Local);
    let hit = import_wav_as(&env, "Ding", Provenance::Local);
    let dot = import_png_as(&env, "Dot", Provenance::Local);
    let draft = ProfileDraft::new("P")
        .with_hud(DraftSlot::Item(hud.id().clone()))
        .with_hitsound(DraftSlot::Item(hit.id().clone()))
        .with_crosshairs(vec![DraftCrosshair::Set {
            weapon_script: "tf_weapon_scattergun".into(),
            crosshair: CrosshairRef::Library(dot.id().clone()),
        }]);
    let p = env.lib.create_profile(&draft).unwrap();
    assert!(env.lib.missing_items(p.id()).unwrap().is_empty());
}

#[test]
fn a_deleted_local_item_is_missing() {
    let env = library_env();
    let p = hud_profile(&env, "budhud", Provenance::Local);
    let missing = env.lib.missing_items(p.id()).unwrap();
    assert_eq!(missing.len(), 1);
    assert_eq!(missing[0].kind(), &MissingKind::Hud);
    assert_eq!(missing[0].name(), p.hud().unwrap().name());
    assert_eq!(missing[0].provenance(), &Provenance::Local);
}

#[test]
fn a_row_whose_folder_vanished_is_missing() {
    let env = library_env();
    let hud = import_hud_as(&env, "budhud", Provenance::Local);
    let draft = ProfileDraft::new("P").with_hud(DraftSlot::Item(hud.id().clone()));
    let p = env.lib.create_profile(&draft).unwrap();
    fs::remove_dir_all(hud_dir(&env, "budhud")).unwrap();
    let missing = env.lib.missing_items(p.id()).unwrap();
    assert_eq!(missing.len(), 1);
    assert_eq!(missing[0].kind(), &MissingKind::Hud);
}

#[test]
fn a_redownload_with_the_same_gamebanana_id_rejoins_and_rewrites_the_folder() {
    let env = library_env();
    let p = hud_profile(&env, "budhud", gb(42));
    import_hud_as(&env, "budhud-v2", gb(42));
    assert!(env.lib.missing_items(p.id()).unwrap().is_empty());
    let after = env.lib.profile(p.id()).unwrap();
    assert_eq!(after.hud().unwrap().folder_name(), "budhud-v2");
    assert_eq!(after.hud().unwrap().name(), p.hud().unwrap().name());
}

#[test]
fn a_curated_hud_rejoins_by_its_store_hud_id() {
    let env = library_env();
    let curated = Provenance::Curated {
        hud_id: "rayshud".into(),
    };
    let p = hud_profile(&env, "rayshud", curated.clone());
    import_hud_as(&env, "rayshud-main", curated);
    assert!(env.lib.missing_items(p.id()).unwrap().is_empty());
    let after = env.lib.profile(p.id()).unwrap();
    assert_eq!(after.hud().unwrap().folder_name(), "rayshud-main");
}

#[test]
fn duplicate_gamebanana_ids_pick_the_newest_row_whose_folder_exists() {
    let env = library_env();
    let p = hud_profile(&env, "budhud", gb(42));
    import_hud_as(&env, "budhud-old", gb(42));
    import_hud_as(&env, "budhud-new", gb(42));
    assert!(env.lib.missing_items(p.id()).unwrap().is_empty());
    let folder = |env: &LibraryEnv| {
        env.lib
            .profile(p.id())
            .unwrap()
            .hud()
            .unwrap()
            .folder_name()
            .to_owned()
    };
    assert_eq!(folder(&env), "budhud-new");
    fs::remove_dir_all(hud_dir(&env, "budhud-new")).unwrap();
    assert!(env.lib.missing_items(p.id()).unwrap().is_empty());
    assert_eq!(folder(&env), "budhud-old");
}

#[test]
fn sounds_and_crosshairs_rejoin_by_gamebanana_id_and_the_rest_are_missing_in_order() {
    let env = library_env();
    let hit = import_wav_as(&env, "Ding", gb(7));
    let kill = import_wav_as(&env, "Bell", Provenance::Local);
    let dot = import_png_as(&env, "Dot", gb(91));
    let ring = import_png_as(&env, "Ring", Provenance::Local);
    let draft = ProfileDraft::new("P")
        .with_hitsound(DraftSlot::Item(hit.id().clone()))
        .with_killsound(DraftSlot::Item(kill.id().clone()))
        .with_crosshairs(vec![
            DraftCrosshair::Set {
                weapon_script: "tf_weapon_scattergun".into(),
                crosshair: CrosshairRef::Library(ring.id().clone()),
            },
            DraftCrosshair::Set {
                weapon_script: "tf_weapon_rocketlauncher".into(),
                crosshair: CrosshairRef::Library(dot.id().clone()),
            },
            DraftCrosshair::Set {
                weapon_script: "tf_weapon_minigun".into(),
                crosshair: CrosshairRef::Builtin("no_such_builtin".into()),
            },
        ]);
    let p = env.lib.create_profile(&draft).unwrap();
    env.lib.delete_sound(hit.id(), None).unwrap();
    env.lib.delete_sound(kill.id(), None).unwrap();
    env.lib.delete_crosshair(dot.id()).unwrap();
    env.lib.delete_crosshair(ring.id()).unwrap();
    let hit2 = import_wav_as(&env, "Ding", gb(7));
    let dot2 = import_png_as(&env, "Dot", gb(91));

    let missing = env.lib.missing_items(p.id()).unwrap();
    let kinds: Vec<_> = missing.iter().map(|m| m.kind().clone()).collect();
    assert_eq!(
        kinds,
        [
            MissingKind::Killsound,
            MissingKind::Crosshair {
                weapon_script: "tf_weapon_scattergun".into()
            },
        ]
    );
    assert_eq!(missing[1].name(), ring.folder_name());
    let after = env.lib.profile(p.id()).unwrap();
    assert_eq!(after.hitsound().unwrap().folder_name(), hit2.folder_name());
    let rocket = after
        .crosshairs()
        .iter()
        .find(|c| c.weapon_script() == "tf_weapon_rocketlauncher")
        .unwrap();
    assert_eq!(
        rocket.crosshair(),
        &ProfileCrosshairRef::Library {
            folder_name: dot2.folder_name().to_owned()
        }
    );
}

#[test]
fn a_resolve_that_rewrites_several_items_stamps_the_profile_once() {
    let env = library_env();
    let hud = import_hud_as(&env, "budhud", gb(42));
    let hit = import_wav_as(&env, "Ding", gb(7));
    let dot = import_png_as(&env, "Dot", gb(91));
    let ring = import_png_as(&env, "Ring", gb(92));
    let draft = ProfileDraft::new("P")
        .with_hud(DraftSlot::Item(hud.id().clone()))
        .with_hitsound(DraftSlot::Item(hit.id().clone()))
        .with_crosshairs(vec![
            DraftCrosshair::Set {
                weapon_script: "tf_weapon_scattergun".into(),
                crosshair: CrosshairRef::Library(dot.id().clone()),
            },
            DraftCrosshair::Set {
                weapon_script: "tf_weapon_rocketlauncher".into(),
                crosshair: CrosshairRef::Library(ring.id().clone()),
            },
        ]);
    let p = env.lib.create_profile(&draft).unwrap();
    env.lib.delete_hud(hud.id()).unwrap();
    env.lib.delete_sound(hit.id(), None).unwrap();
    env.lib.delete_crosshair(dot.id()).unwrap();
    env.lib.delete_crosshair(ring.id()).unwrap();
    import_hud_as(&env, "budhud-v2", gb(42));
    let hit2 = import_wav_as(&env, "Ding", gb(7));
    let dot2 = import_png_as(&env, "Dot", gb(91));
    let ring2 = import_png_as(&env, "Ring", gb(92));
    // Every statement that stamps the profile leaves one row behind.
    sql(&env.dir)
        .execute_batch(
            "CREATE TABLE stamps (profile_id TEXT);
             CREATE TRIGGER count_stamps AFTER UPDATE OF updated_at ON profiles
             BEGIN INSERT INTO stamps VALUES (NEW.id); END;",
        )
        .unwrap();

    assert!(env.lib.missing_items(p.id()).unwrap().is_empty());
    assert_eq!(count(&env.dir, "stamps"), 1);
    let after = env.lib.profile(p.id()).unwrap();
    assert_eq!(after.hud().unwrap().folder_name(), "budhud-v2");
    assert_eq!(after.hitsound().unwrap().folder_name(), hit2.folder_name());
    let folder = |script: &str| {
        after
            .crosshairs()
            .iter()
            .find(|c| c.weapon_script() == script)
            .unwrap()
            .crosshair()
            .clone()
    };
    assert_eq!(
        folder("tf_weapon_scattergun"),
        ProfileCrosshairRef::Library {
            folder_name: dot2.folder_name().to_owned()
        }
    );
    assert_eq!(
        folder("tf_weapon_rocketlauncher"),
        ProfileCrosshairRef::Library {
            folder_name: ring2.folder_name().to_owned()
        }
    );

    // Nothing left to rewrite, so a second resolve does not stamp.
    assert!(env.lib.missing_items(p.id()).unwrap().is_empty());
    assert_eq!(count(&env.dir, "stamps"), 1);
}

#[test]
fn missing_items_of_an_unknown_profile_is_not_found() {
    let env = library_env();
    assert!(matches!(
        env.lib.missing_items(&ProfileId::new("gone")),
        Err(ProfileError::NotFound { .. })
    ));
}
