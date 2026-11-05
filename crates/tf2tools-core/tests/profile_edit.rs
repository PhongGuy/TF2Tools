//! Creating and editing profiles from drafts, writing v5's columns.

mod common;

use common::{
    LibraryEnv, V5_PROFILE_ID as V5_ID, count, gb, import_hud_as, import_png_as, import_wav_as,
    library_env, seed_v5_profile, set_setting, sql,
};
use tf2tools_core::{
    CrosshairRef, DraftCrosshair, DraftSlot, HudId, ProfileCrosshairRef, ProfileDraft,
    ProfileError, ProfileId, Provenance,
};

/// Every v5 column but name and updated_at, plus the crosshair rows, as one JSON text.
fn v5_columns(env: &LibraryEnv, id: &str) -> String {
    sql(&env.dir)
        .query_row(
            "SELECT json_array(hud_folder_name, hud_name, hud_source, hud_store_gb_id,
                 hud_store_hud_id, hitsound_folder_name, hitsound_name, hitsound_source,
                 hitsound_store_gb_id, killsound_folder_name, killsound_name, killsound_source,
                 killsound_store_gb_id, created_at,
                 (SELECT json_group_array(json_array(weapon_script, crosshair_source,
                      crosshair_ref, crosshair_store_gb_id))
                  FROM (SELECT * FROM profile_crosshair_assignments
                        WHERE profile_id = ?1 ORDER BY weapon_script)))
             FROM profiles WHERE id = ?1",
            [id],
            |r| r.get(0),
        )
        .unwrap()
}

fn set(weapon_script: &str, crosshair: CrosshairRef) -> DraftCrosshair {
    DraftCrosshair::Set {
        weapon_script: weapon_script.into(),
        crosshair,
    }
}

#[test]
fn editing_with_every_slot_kept_round_trips_the_v5_columns() {
    let env = library_env();
    seed_v5_profile(&env);
    let before = v5_columns(&env, V5_ID);
    let id = ProfileId::new(V5_ID);
    let p = env.lib.profile(&id).unwrap();
    let updated = env
        .lib
        .update_profile(&id, &ProfileDraft::from_profile(&p).with_name("Comp 2"))
        .unwrap();
    let updated = updated.profile();
    assert_eq!(updated.name(), "Comp 2");
    assert_eq!(v5_columns(&env, V5_ID), before);
    assert_ne!(updated.updated_at(), "2025-03-01T10:00:00+00:00");
    assert_eq!(updated.hud(), p.hud());
    assert_eq!(updated.crosshairs(), p.crosshairs());
}

#[test]
fn create_writes_folder_names_names_and_provenance() {
    let env = library_env();
    let hud = import_hud_as(&env, "budhud", gb(42));
    let hit = import_wav_as(&env, "Ding", Provenance::Local);
    let kill = import_wav_as(&env, "Bell", gb(77));
    let dot = import_png_as(&env, "Dot", gb(91));
    let draft = ProfileDraft::new("  Main  ")
        .with_hud(DraftSlot::Item(hud.id().clone()))
        .with_hitsound(DraftSlot::Item(hit.id().clone()))
        .with_killsound(DraftSlot::Item(kill.id().clone()))
        .with_crosshairs(vec![
            set(
                "tf_weapon_scattergun",
                CrosshairRef::Builtin("cpma_9".into()),
            ),
            set(
                "tf_weapon_rocketlauncher",
                CrosshairRef::Library(dot.id().clone()),
            ),
        ]);
    let p = env.lib.create_profile(&draft).unwrap();
    assert_eq!(p.name(), "Main");
    assert_eq!(p.id().as_str().len(), 36);
    assert_eq!(p.created_at().len(), 32);
    assert_eq!(p.hud().unwrap().folder_name(), hud.folder_name());
    assert_eq!(p.hud().unwrap().provenance(), &gb(42));
    assert_eq!(p.hitsound().unwrap().provenance(), &Provenance::Local);
    assert_eq!(p.killsound().unwrap().name(), "Bell");
    assert_eq!(env.lib.profile(p.id()).unwrap(), p);

    let conn = sql(&env.dir);
    let row: (
        String,
        String,
        Option<i64>,
        String,
        Option<i64>,
        Option<i64>,
    ) = conn
        .query_row(
            "SELECT hud_folder_name, hud_name, hud_store_gb_id, hitsound_folder_name,
                 hitsound_store_gb_id, killsound_store_gb_id
             FROM profiles WHERE id = ?1",
            [p.id().as_str()],
            |r| {
                Ok((
                    r.get(0)?,
                    r.get(1)?,
                    r.get(2)?,
                    r.get(3)?,
                    r.get(4)?,
                    r.get(5)?,
                ))
            },
        )
        .unwrap();
    assert_eq!(
        row,
        (
            hud.folder_name().to_owned(),
            hud.name().to_owned(),
            Some(42),
            hit.folder_name().to_owned(),
            None,
            Some(77)
        )
    );
    let same_source: bool = conn
        .query_row(
            "SELECT p.hud_source = h.source FROM profiles p, library_huds h
             WHERE p.id = ?1 AND h.id = ?2",
            [p.id().as_str(), hud.id().as_str()],
            |r| r.get(0),
        )
        .unwrap();
    assert!(same_source);
    let mut stmt = conn
        .prepare(
            "SELECT weapon_script, crosshair_source, crosshair_ref, crosshair_store_gb_id
             FROM profile_crosshair_assignments WHERE profile_id = ?1 ORDER BY weapon_script",
        )
        .unwrap();
    let rows: Vec<(String, String, String, Option<i64>)> = stmt
        .query_map([p.id().as_str()], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))
        })
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    assert_eq!(
        rows,
        [
            (
                "tf_weapon_rocketlauncher".to_owned(),
                "library".to_owned(),
                dot.folder_name().to_owned(),
                Some(91)
            ),
            (
                "tf_weapon_scattergun".to_owned(),
                "builtin".to_owned(),
                "cpma_9".to_owned(),
                None
            ),
        ]
    );
}

#[test]
fn create_refuses_a_blank_name_an_unknown_item_and_a_bad_weapon_script() {
    let env = library_env();
    assert!(matches!(
        env.lib.create_profile(&ProfileDraft::new("   ")),
        Err(ProfileError::EmptyName)
    ));
    let unknown = ProfileDraft::new("x").with_hud(DraftSlot::Item(HudId::new("nope")));
    assert!(matches!(
        env.lib.create_profile(&unknown),
        Err(ProfileError::ItemNotFound { .. })
    ));
    let bad = ProfileDraft::new("x")
        .with_crosshairs(vec![set("../evil", CrosshairRef::Builtin("cpma_9".into()))]);
    assert!(matches!(
        env.lib.create_profile(&bad),
        Err(ProfileError::InvalidWeaponScript { .. })
    ));
    assert_eq!(count(&env.dir, "profiles"), 0);
}

#[test]
fn keep_on_create_is_an_empty_slot() {
    let env = library_env();
    let p = env
        .lib
        .create_profile(&ProfileDraft::new("Bare").with_hud(DraftSlot::Keep))
        .unwrap();
    assert!(p.hud().is_none() && p.hitsound().is_none() && p.killsound().is_none());
    assert!(p.crosshairs().is_empty());
}

#[test]
fn update_replaces_slots_and_keep_preserves_a_dangling_reference() {
    let env = library_env();
    seed_v5_profile(&env);
    let kill = import_wav_as(&env, "Bell", Provenance::Local);
    let id = ProfileId::new(V5_ID);
    let p = env.lib.profile(&id).unwrap();
    let rocket = p.crosshairs()[0].clone();
    let draft = ProfileDraft::from_profile(&p)
        .with_hitsound(DraftSlot::Empty)
        .with_killsound(DraftSlot::Item(kill.id().clone()))
        .with_crosshairs(vec![
            DraftCrosshair::Keep(rocket.clone()),
            set(
                "tf_weapon_scattergun",
                CrosshairRef::Builtin("cpma_10".into()),
            ),
        ]);
    let update = env.lib.update_profile(&id, &draft).unwrap();
    let updated = update.profile();
    assert_eq!(updated.hud(), p.hud());
    assert!(updated.hitsound().is_none());
    assert_eq!(
        updated.killsound().unwrap().folder_name(),
        kill.folder_name()
    );
    assert_eq!(updated.crosshairs()[0], rocket);
    assert_eq!(
        updated.crosshairs()[1].crosshair(),
        &ProfileCrosshairRef::Builtin("cpma_10".into())
    );
    assert_eq!(updated.crosshairs().len(), 2);
    let hit_folder: Option<String> = sql(&env.dir)
        .query_row(
            "SELECT hitsound_folder_name FROM profiles WHERE id = ?1",
            [V5_ID],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(hit_folder, None);
}

#[test]
fn updating_an_unknown_profile_is_not_found() {
    let env = library_env();
    assert!(matches!(
        env.lib
            .update_profile(&ProfileId::new("gone"), &ProfileDraft::new("x")),
        Err(ProfileError::NotFound { .. })
    ));
}

// Another profile's name, in any case, is refused on create and rename; a profile keeps its
// own name, or changes only its case, freely.
#[test]
fn a_name_another_profile_has_is_refused() {
    let env = library_env();
    let comp = env
        .lib
        .create_profile(&ProfileDraft::new("Competitive"))
        .unwrap();
    let pub_ = env.lib.create_profile(&ProfileDraft::new("Pubs")).unwrap();
    let taken = |r: Result<_, ProfileError>| matches!(r, Err(ProfileError::NameTaken { name }) if name == "competitive");
    assert!(taken(
        env.lib.create_profile(&ProfileDraft::new(" competitive "))
    ));
    assert!(taken(
        env.lib
            .update_profile(pub_.id(), &ProfileDraft::new("competitive"))
            .map(|u| u.profile().clone())
    ));
    env.lib
        .update_profile(comp.id(), &ProfileDraft::new("COMPETITIVE"))
        .unwrap();
    assert_eq!(env.lib.profiles().unwrap().len(), 2);
}

#[test]
fn profiles_are_listed_by_name() {
    let env = library_env();
    env.lib.create_profile(&ProfileDraft::new("beta")).unwrap();
    env.lib.create_profile(&ProfileDraft::new("alpha")).unwrap();
    let names: Vec<String> = env
        .lib
        .profiles()
        .unwrap()
        .iter()
        .map(|p| p.name().to_owned())
        .collect();
    assert_eq!(names, ["alpha", "beta"]);
}

// Inserted as Beta, then Alpha twice with the higher id first, so only name-then-id gives
// this order.
#[test]
fn profiles_with_one_name_are_ordered_by_id() {
    let env = library_env();
    sql(&env.dir)
        .execute_batch(
            "INSERT INTO profiles (id, name, created_at, updated_at) VALUES
                 ('p2', 'Beta', '2025-01-01T00:00:00+00:00', '2025-01-01T00:00:00+00:00'),
                 ('p3', 'Alpha', '2025-01-01T00:00:00+00:00', '2025-01-01T00:00:00+00:00'),
                 ('p1', 'Alpha', '2025-01-01T00:00:00+00:00', '2025-01-01T00:00:00+00:00');",
        )
        .unwrap();
    let profiles = env.lib.profiles().unwrap();
    let listed: Vec<(&str, &str)> = profiles
        .iter()
        .map(|p| (p.name(), p.id().as_str()))
        .collect();
    assert_eq!(listed, [("Alpha", "p1"), ("Alpha", "p3"), ("Beta", "p2")]);
}

#[test]
fn a_weapon_set_twice_in_a_draft_keeps_the_last() {
    let env = library_env();
    let p = env
        .lib
        .create_profile(&ProfileDraft::new("Twice").with_crosshairs(vec![
            set(
                "tf_weapon_scattergun",
                CrosshairRef::Builtin("cpma_9".into()),
            ),
            set(
                "tf_weapon_scattergun",
                CrosshairRef::Builtin("cpma_1".into()),
            ),
        ]))
        .unwrap();
    let crosshairs: Vec<_> = p
        .crosshairs()
        .iter()
        .map(|c| (c.weapon_script(), c.crosshair().clone()))
        .collect();
    assert_eq!(
        crosshairs,
        [(
            "tf_weapon_scattergun",
            ProfileCrosshairRef::Builtin("cpma_1".into())
        )]
    );
    assert_eq!(count(&env.dir, "profile_crosshair_assignments"), 1);
}

#[test]
fn deleting_another_profile_keeps_the_active_id() {
    let env = library_env();
    let a = env.lib.create_profile(&ProfileDraft::new("a")).unwrap();
    let b = env.lib.create_profile(&ProfileDraft::new("b")).unwrap();
    set_setting(&env.dir, "active_profile_id", a.id().as_str());
    env.lib.delete_profile(b.id()).unwrap();
    assert_eq!(env.lib.active_profile().unwrap().as_ref(), Some(a.id()));
}

/// The profile's crosshair rows as (weapon script, source), by weapon script.
fn crosshair_sources(env: &LibraryEnv, id: &str) -> Vec<(String, String)> {
    let conn = sql(&env.dir);
    let mut stmt = conn
        .prepare(
            "SELECT weapon_script, crosshair_source FROM profile_crosshair_assignments
             WHERE profile_id = ?1 ORDER BY weapon_script",
        )
        .unwrap();
    stmt.query_map([id], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap()
        .map(Result::unwrap)
        .collect()
}

#[test]
fn an_update_keeps_crosshair_rows_of_an_unknown_source() {
    let env = library_env();
    seed_v5_profile(&env);
    sql(&env.dir)
        .execute(
            "INSERT INTO profile_crosshair_assignments
                 (profile_id, weapon_script, crosshair_source, crosshair_ref, crosshair_store_gb_id)
             VALUES (?1, 'tf_weapon_minigun', 'workshop', 'ring', NULL)",
            [V5_ID],
        )
        .unwrap();
    let id = ProfileId::new(V5_ID);
    let p = env.lib.profile(&id).unwrap();
    // v6 cannot read the unknown row, so the draft never mentions it.
    assert_eq!(p.crosshairs().len(), 2);
    env.lib
        .update_profile(&id, &ProfileDraft::from_profile(&p).with_crosshairs(vec![]))
        .unwrap();
    assert_eq!(
        crosshair_sources(&env, V5_ID),
        [("tf_weapon_minigun".to_owned(), "workshop".to_owned())]
    );
    // Setting that weapon explicitly replaces the unknown row.
    env.lib
        .update_profile(
            &id,
            &ProfileDraft::from_profile(&p).with_crosshairs(vec![set(
                "tf_weapon_minigun",
                CrosshairRef::Builtin("cpma_9".into()),
            )]),
        )
        .unwrap();
    assert_eq!(
        crosshair_sources(&env, V5_ID),
        [("tf_weapon_minigun".to_owned(), "builtin".to_owned())]
    );
}
