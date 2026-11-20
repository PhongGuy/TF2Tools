//! The five profile slots and their modes: drafts, storage and reading back.

use crate::common;

use common::{
    FLAT_ADDON as FLAT, PYRO_ADDON as PYRO, count, import_addon_as as addon, import_hud_as,
    library_env, low_choices, module_catalog, set_setting, setting, sql,
};
use tf2tools_core::{
    AddonName, Choices, CrosshairRef, DraftCrosshair, DraftItem, ModuleChoice, Preset, Profile,
    ProfileDraft, ProfileError, ProfileItem, Provenance, SlotMode,
};

const ACTIVE: &str = "active_profile_id";

fn mode<T>(slot: &SlotMode<T>) -> &'static str {
    match slot {
        SlotMode::Leave => "leave",
        SlotMode::None => "none",
        SlotMode::Set(_) => "set",
        _ => "unknown",
    }
}

fn modes(p: &Profile) -> [&'static str; 5] {
    [
        mode(p.hud()),
        mode(p.sounds()),
        mode(p.crosshairs()),
        mode(p.graphics()),
        mode(p.addons()),
    ]
}

fn scattergun() -> DraftCrosshair {
    DraftCrosshair::Set {
        weapon_script: "tf_weapon_scattergun".into(),
        crosshair: CrosshairRef::Builtin("cpma_9".into()),
    }
}

#[test]
fn a_new_draft_takes_items_out_and_leaves_graphics_and_addons() {
    let env = library_env();
    let p = env.lib.create_profile(&ProfileDraft::new("Bare")).unwrap();
    assert_eq!(modes(&p), ["none", "none", "none", "leave", "leave"]);
    assert_eq!(env.lib.profile(p.id()).unwrap(), p);
}

#[test]
fn every_hud_mode_round_trips() {
    let env = library_env();
    let hud = import_hud_as(&env, "budhud", Provenance::Local);
    for (slot, want) in [
        (SlotMode::Leave, "leave"),
        (SlotMode::None, "none"),
        (SlotMode::Set(DraftItem::Pick(hud.id().clone())), "set"),
    ] {
        let p = env
            .lib
            .create_profile(&ProfileDraft::new(want).with_hud(slot))
            .unwrap();
        assert_eq!(mode(p.hud()), want);
        assert_eq!(env.lib.profile(p.id()).unwrap(), p);
        if want == "set" {
            assert_eq!(
                p.hud().as_set().map(ProfileItem::folder_name),
                Some(hud.folder_name())
            );
        }
    }
}

#[test]
fn sounds_are_one_slot_whose_set_holds_each_sound_or_tf2_default() {
    let env = library_env();
    let hit = common::import_wav_as(&env, "Ding", Provenance::Local);
    let draft = ProfileDraft::new("Hit").with_sounds(SlotMode::Set((
        Some(DraftItem::Pick(hit.id().clone())),
        None,
    )));
    let p = env.lib.create_profile(&draft).unwrap();
    let SlotMode::Set(sounds) = p.sounds() else {
        panic!("{:?}", p.sounds())
    };
    assert_eq!(
        sounds.hit.as_ref().map(ProfileItem::folder_name),
        Some(hit.folder_name())
    );
    assert_eq!(sounds.kill, None);
    let quiet = env
        .lib
        .create_profile(&ProfileDraft::new("Quiet").with_sounds(SlotMode::Set((None, None))))
        .unwrap();
    assert_eq!(mode(quiet.sounds()), "set");
    let left = env
        .lib
        .create_profile(&ProfileDraft::new("Left").with_sounds(SlotMode::Leave))
        .unwrap();
    assert_eq!(mode(left.sounds()), "leave");
    for p in [p, quiet, left] {
        assert_eq!(env.lib.profile(p.id()).unwrap(), p);
    }
}

#[test]
fn crosshairs_set_keeps_its_list_even_when_empty() {
    let env = library_env();
    let one = env
        .lib
        .create_profile(
            &ProfileDraft::new("One").with_crosshairs(SlotMode::Set(vec![scattergun()])),
        )
        .unwrap();
    assert_eq!(one.crosshairs().as_set().map(Vec::len), Some(1));
    let empty = env
        .lib
        .create_profile(&ProfileDraft::new("Empty").with_crosshairs(SlotMode::Set(Vec::new())))
        .unwrap();
    assert_eq!(empty.crosshairs(), &SlotMode::Set(Vec::new()));
    let left = env
        .lib
        .create_profile(&ProfileDraft::new("Left").with_crosshairs(SlotMode::Leave))
        .unwrap();
    assert_eq!(left.crosshairs(), &SlotMode::Leave);
    for p in [one, empty, left] {
        assert_eq!(env.lib.profile(p.id()).unwrap(), p);
    }
}

#[test]
fn graphics_stores_its_preset_and_module_choices() {
    let env = library_env();
    let draft = ProfileDraft::new("Potato").with_graphics(SlotMode::Set(low_choices()));
    let p = env.lib.create_profile(&draft).unwrap();
    let SlotMode::Set(graphics) = p.graphics() else {
        panic!("{:?}", p.graphics())
    };
    assert_eq!(graphics.choices, low_choices());
    let (preset, modules): (String, String) = sql(&env.dir)
        .query_row(
            "SELECT graphics_preset, graphics_modules FROM profiles WHERE id = ?1",
            [p.id().as_str()],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!(preset, "low");
    assert_eq!(modules, r#"{"fpscap":"144","motion_blur":"high"}"#);
    assert_eq!(env.lib.profile(p.id()).unwrap(), p);
}

#[test]
fn graphics_keeps_a_module_the_catalog_lacks_and_an_as_is_preset() {
    let env = library_env();
    let mut odd = Choices::new(Preset::High);
    odd.set(
        &module_catalog(),
        "not_a_module",
        ModuleChoice::Value("on".into()),
    );
    for choices in [odd, Choices::new(Preset::Ultra)] {
        let p = env
            .lib
            .create_profile(
                &ProfileDraft::new(choices.preset().name())
                    .with_graphics(SlotMode::Set(choices.clone())),
            )
            .unwrap();
        let read = env.lib.profile(p.id()).unwrap();
        assert_eq!(read.graphics().as_set().map(|g| &g.choices), Some(&choices));
    }
}

#[test]
fn addons_store_the_ticked_names_sorted_once_each_with_only_these() {
    let env = library_env();
    let flat = addon(&env, FLAT);
    let pyro = addon(&env, PYRO);
    let ticked = vec![
        DraftItem::Pick(pyro.clone()),
        DraftItem::Pick(flat.clone()),
        DraftItem::Pick(flat.clone()),
    ];
    let p = env
        .lib
        .create_profile(&ProfileDraft::new("Mine").with_addons(SlotMode::Set((ticked, true))))
        .unwrap();
    let SlotMode::Set(addons) = p.addons() else {
        panic!("{:?}", p.addons())
    };
    assert_eq!(addons.on, [flat, pyro]);
    assert!(addons.only_these);
    assert_eq!(count(&env.dir, "profile_addons"), 2);
    assert_eq!(env.lib.profile(p.id()).unwrap(), p);
}

#[test]
fn an_addon_the_library_lacks_is_refused_before_writing() {
    let env = library_env();
    let name = AddonName::new(FLAT).unwrap();
    let draft =
        ProfileDraft::new("X").with_addons(SlotMode::Set((vec![DraftItem::Pick(name)], false)));
    let err = env.lib.create_profile(&draft).unwrap_err();
    assert!(
        matches!(&err, ProfileError::ItemNotFound { what } if *what == format!("addon {FLAT}")),
        "{err:?}"
    );
    assert_eq!(count(&env.dir, "profiles"), 0);
}

#[test]
fn a_kept_addon_survives_its_library_copy_being_deleted() {
    let env = library_env();
    let flat = addon(&env, FLAT);
    let draft = ProfileDraft::new("Mine")
        .with_addons(SlotMode::Set((vec![DraftItem::Pick(flat.clone())], true)));
    let p = env.lib.create_profile(&draft).unwrap();
    env.lib.delete_addon(&flat, None).unwrap();
    let renamed = env
        .lib
        .update_profile(p.id(), &ProfileDraft::from_profile(&p).with_name("Renamed"))
        .unwrap();
    assert_eq!(renamed.profile().addons(), p.addons());
    let kept = ProfileDraft::from_profile(&p).with_addons(SlotMode::Set((
        vec![DraftItem::keep_addon(flat.clone())],
        false,
    )));
    let update = env.lib.update_profile(p.id(), &kept).unwrap();
    let SlotMode::Set(addons) = update.profile().addons() else {
        panic!("{:?}", update.profile().addons())
    };
    assert_eq!(addons.on, [flat]);
    assert!(!addons.only_these);
}

#[test]
fn a_slot_that_is_not_set_stores_no_content() {
    let env = library_env();
    let hud = import_hud_as(&env, "budhud", Provenance::Local);
    let flat = addon(&env, FLAT);
    let draft = ProfileDraft::new("Full")
        .with_hud(SlotMode::Set(DraftItem::Pick(hud.id().clone())))
        .with_crosshairs(SlotMode::Set(vec![scattergun()]))
        .with_graphics(SlotMode::Set(low_choices()))
        .with_addons(SlotMode::Set((vec![DraftItem::Pick(flat)], true)));
    let p = env.lib.create_profile(&draft).unwrap();
    let cleared = ProfileDraft::from_profile(&p)
        .with_hud(SlotMode::Leave)
        .with_crosshairs(SlotMode::None)
        .with_graphics(SlotMode::None)
        .with_addons(SlotMode::Leave);
    let q = env.lib.update_profile(p.id(), &cleared).unwrap();
    assert_eq!(
        modes(q.profile()),
        ["leave", "none", "none", "none", "leave"]
    );
    let row: (Option<String>, Option<String>, Option<String>, i64) = sql(&env.dir)
        .query_row(
            "SELECT hud_folder_name, graphics_preset, graphics_modules, addons_only
             FROM profiles WHERE id = ?1",
            [p.id().as_str()],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )
        .unwrap();
    assert_eq!(row, (None, None, None, 0));
    assert_eq!(count(&env.dir, "profile_crosshair_assignments"), 0);
    assert_eq!(count(&env.dir, "profile_addons"), 0);
}

#[test]
fn from_profile_keeps_all_five_slots() {
    let env = library_env();
    let hud = import_hud_as(&env, "budhud", Provenance::Local);
    let kill = common::import_wav_as(&env, "Bell", Provenance::Local);
    let flat = addon(&env, FLAT);
    let draft = ProfileDraft::new("Full")
        .with_hud(SlotMode::Set(DraftItem::Pick(hud.id().clone())))
        .with_sounds(SlotMode::Set((
            None,
            Some(DraftItem::Pick(kill.id().clone())),
        )))
        .with_crosshairs(SlotMode::Set(vec![scattergun()]))
        .with_graphics(SlotMode::Set(low_choices()))
        .with_addons(SlotMode::Set((vec![DraftItem::Pick(flat)], true)));
    let p = env.lib.create_profile(&draft).unwrap();
    let q = env
        .lib
        .update_profile(p.id(), &ProfileDraft::from_profile(&p))
        .unwrap();
    let q = q.profile();
    assert_eq!(q.hud(), p.hud());
    assert_eq!(q.sounds(), p.sounds());
    assert_eq!(q.crosshairs(), p.crosshairs());
    assert_eq!(q.graphics(), p.graphics());
    assert_eq!(q.addons(), p.addons());
}

#[test]
fn a_mode_this_build_does_not_know_reads_as_leave() {
    let env = library_env();
    let p = env.lib.create_profile(&ProfileDraft::new("P")).unwrap();
    sql(&env.dir)
        .execute(
            "UPDATE profiles SET hud_mode = 'later' WHERE id = ?1",
            [p.id().as_str()],
        )
        .unwrap();
    assert_eq!(env.lib.profile(p.id()).unwrap().hud(), &SlotMode::Leave);
}

#[test]
fn deleting_a_profile_drops_its_addon_rows() {
    let env = library_env();
    let flat = addon(&env, FLAT);
    let draft =
        ProfileDraft::new("P").with_addons(SlotMode::Set((vec![DraftItem::Pick(flat)], false)));
    let p = env.lib.create_profile(&draft).unwrap();
    env.lib.delete_profile(p.id()).unwrap();
    assert_eq!(count(&env.dir, "profile_addons"), 0);
}

#[test]
fn changing_any_slot_ends_the_active_state_and_a_rename_does_not() {
    let env = library_env();
    let flat = addon(&env, FLAT);
    let p = env.lib.create_profile(&ProfileDraft::new("Main")).unwrap();
    let activate = || set_setting(&env.dir, ACTIVE, p.id().as_str());
    activate();
    let renamed = env
        .lib
        .update_profile(p.id(), &ProfileDraft::from_profile(&p).with_name("Main 2"))
        .unwrap();
    assert!(!renamed.deactivated());
    let mut saved = renamed.profile().clone();
    let edits: [fn(ProfileDraft, &AddonName) -> ProfileDraft; 4] = [
        |d, _| d.with_graphics(SlotMode::Set(Choices::new(Preset::High))),
        |d, a| d.with_addons(SlotMode::Set((vec![DraftItem::Pick(a.clone())], false))),
        |d, a| d.with_addons(SlotMode::Set((vec![DraftItem::Pick(a.clone())], true))),
        |d, _| d.with_hud(SlotMode::Leave),
    ];
    for (i, edit) in edits.into_iter().enumerate() {
        activate();
        let update = env
            .lib
            .update_profile(p.id(), &edit(ProfileDraft::from_profile(&saved), &flat))
            .unwrap();
        assert!(update.deactivated(), "edit {i}");
        assert_eq!(setting(&env.dir, ACTIVE).as_deref(), Some(""), "edit {i}");
        saved = update.profile().clone();
    }
}
