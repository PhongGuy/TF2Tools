//! Reading v5 profile rows, deleting a profile and the active profile id.

mod common;

use common::{
    V5_PROFILE_ID as V5_ID, count, gb, library_env, seed_v5_profile, set_setting, setting,
};
use tf2tools_core::{GbId, ProfileCrosshairRef, ProfileError, ProfileId, Provenance};

#[test]
fn a_v5_profile_reads_with_its_raw_references() {
    let env = library_env();
    seed_v5_profile(&env);
    let p = env.lib.profile(&ProfileId::new(V5_ID)).unwrap();
    assert_eq!(p.name(), "Comp");
    let hud = p.hud().unwrap();
    assert_eq!((hud.folder_name(), hud.name()), ("rayshud", "rayshud"));
    assert_eq!(
        hud.provenance(),
        &Provenance::Curated {
            hud_id: "rayshud".into()
        }
    );
    let hit = p.hitsound().unwrap();
    assert_eq!((hit.folder_name(), hit.name()), ("ding-1a2b3c4d", "Ding"));
    assert_eq!(hit.provenance(), &gb(7001));
    assert!(p.killsound().is_none());
    let crosshairs: Vec<_> = p
        .crosshairs()
        .iter()
        .map(|c| (c.weapon_script(), c.crosshair().clone(), c.store_gb_id()))
        .collect();
    assert_eq!(
        crosshairs,
        [
            (
                "tf_weapon_rocketlauncher",
                ProfileCrosshairRef::Library {
                    folder_name: "dot-5e6f7a8b".into()
                },
                Some(GbId::new(9001))
            ),
            (
                "tf_weapon_scattergun",
                ProfileCrosshairRef::Builtin("cpma_9".into()),
                None
            ),
        ]
    );
    assert_eq!(p.created_at(), "2025-03-01T10:00:00+00:00");
}

#[test]
fn an_unknown_profile_is_not_found() {
    let env = library_env();
    let id = ProfileId::new("gone");
    assert!(matches!(
        env.lib.profile(&id),
        Err(ProfileError::NotFound { .. })
    ));
    assert!(matches!(
        env.lib.delete_profile(&id),
        Err(ProfileError::NotFound { .. })
    ));
}

#[test]
fn deleting_the_active_profile_cascades_and_clears_the_active_id() {
    let env = library_env();
    seed_v5_profile(&env);
    let id = ProfileId::new(V5_ID);
    set_setting(&env.dir, "active_profile_id", V5_ID);
    assert_eq!(env.lib.active_profile().unwrap(), Some(id.clone()));
    env.lib.delete_profile(&id).unwrap();
    assert_eq!(count(&env.dir, "profiles"), 0, "the profile row is gone");
    assert!(matches!(
        env.lib.profile(&id),
        Err(ProfileError::NotFound { .. })
    ));
    assert_eq!(count(&env.dir, "profile_crosshair_assignments"), 0);
    assert_eq!(setting(&env.dir, "active_profile_id").as_deref(), Some(""));
    assert_eq!(env.lib.active_profile().unwrap(), None);
}

#[test]
fn an_empty_or_dangling_active_id_is_none() {
    let env = library_env();
    assert_eq!(env.lib.active_profile().unwrap(), None);
    set_setting(&env.dir, "active_profile_id", "");
    assert_eq!(env.lib.active_profile().unwrap(), None);
    set_setting(&env.dir, "active_profile_id", "gone");
    assert_eq!(env.lib.active_profile().unwrap(), None);
}
