//! Applying the Add-ons slot: turning add-ons on and off, Only these, the mastercomfig base it
//! needs, missing add-ons, movement keys and the full step order.

use crate::common;

use common::{
    FLAT_ADDON, LibraryEnv, NULL_ADDON, PYRO_ADDON, builtin_catalog, crosshair_env,
    import_addon_as, import_hud_as, import_wav_as, library_env, place_mastercomfig, write_file,
};
use tf2tools_core::{
    AddonName, ApplyProfileError, ApplyProfileReport, ApplySkipReason, ApplyStep, Choices,
    CrosshairRef, DraftCrosshair, DraftItem, MissingKind, MoveDirection, Preset, Profile,
    ProfileDraft, ProfileId, Provenance, SlotMode,
};

const STOCK: &str = "unbindall\r\nbind \"a\" \"+moveleft\"\r\nbind \"d\" \"+moveright\"\r\n\
                     bind \"e\" \"+use\"\r\nbind \"s\" \"+back\"\r\nbind \"w\" \"+forward\"\r\n";

type AddonSlot = SlotMode<(Vec<DraftItem<AddonName>>, bool)>;

fn ticked(names: &[&AddonName], only_these: bool) -> AddonSlot {
    let items = names
        .iter()
        .map(|n| DraftItem::Pick((*n).clone()))
        .collect();
    SlotMode::Set((items, only_these))
}

/// A profile that leaves every slot but Graphics (by `graphics`) and Add-ons.
fn addons_profile(
    env: &LibraryEnv,
    name: &str,
    graphics: SlotMode<Choices>,
    addons: AddonSlot,
) -> Profile {
    let draft = ProfileDraft::new(name)
        .with_hud(SlotMode::Leave)
        .with_sounds(SlotMode::Leave)
        .with_crosshairs(SlotMode::Leave)
        .with_graphics(graphics)
        .with_addons(addons);
    env.lib.create_profile(&draft).unwrap()
}

fn apply(env: &LibraryEnv, p: &Profile) -> ApplyProfileReport {
    env.lib
        .apply_profile(p.id(), &env.tf2, &builtin_catalog(), false)
        .unwrap()
}

fn installed(env: &LibraryEnv, name: &AddonName) -> bool {
    env.tf2.custom_dir().join(name.as_str()).is_file()
}

fn active(env: &LibraryEnv) -> Option<ProfileId> {
    env.lib.active_profile().unwrap()
}

#[test]
fn addons_set_turns_the_ticked_ones_on_and_leaves_the_rest() {
    let env = library_env();
    place_mastercomfig(&env, Some("medium"));
    let flat = import_addon_as(&env, FLAT_ADDON);
    let pyro = import_addon_as(&env, PYRO_ADDON);
    env.lib.install_addon(&pyro, &env.tf2).unwrap();
    let p = addons_profile(&env, "Flat", SlotMode::Leave, ticked(&[&flat], false));
    let report = apply(&env, &p);
    assert_eq!(report.applied(), [ApplyStep::Addons]);
    assert!(installed(&env, &flat) && installed(&env, &pyro));
    assert_eq!(report.movement(), None);
    assert_eq!(active(&env).as_ref(), Some(p.id()));
}

#[test]
fn only_these_turns_every_other_library_addon_off() {
    let env = library_env();
    place_mastercomfig(&env, Some("medium"));
    let flat = import_addon_as(&env, FLAT_ADDON);
    let pyro = import_addon_as(&env, PYRO_ADDON);
    env.lib.install_addon(&pyro, &env.tf2).unwrap();
    let p = addons_profile(&env, "Flat", SlotMode::Leave, ticked(&[&flat], true));
    assert_eq!(apply(&env, &p).applied(), [ApplyStep::Addons]);
    assert!(installed(&env, &flat));
    assert!(!installed(&env, &pyro));
}

#[test]
fn addons_none_turns_every_library_addon_off_even_without_mastercomfig() {
    let env = library_env();
    let flat = import_addon_as(&env, FLAT_ADDON);
    env.lib.install_addon(&flat, &env.tf2).unwrap();
    let p = addons_profile(&env, "Off", SlotMode::Leave, SlotMode::None);
    let report = apply(&env, &p);
    assert_eq!(report.applied(), [ApplyStep::Addons]);
    assert!(report.skipped_steps().is_empty(), "{report:?}");
    assert!(!installed(&env, &flat));
    assert_eq!(active(&env).as_ref(), Some(p.id()));
}

#[test]
fn without_mastercomfig_the_turn_ons_are_skipped_and_the_turn_offs_still_run() {
    let env = library_env();
    let flat = import_addon_as(&env, FLAT_ADDON);
    let pyro = import_addon_as(&env, PYRO_ADDON);
    env.lib.install_addon(&flat, &env.tf2).unwrap();
    let p = addons_profile(&env, "Pyro", SlotMode::Leave, ticked(&[&pyro], true));
    let report = apply(&env, &p);
    assert_eq!(
        report.skipped_steps(),
        [(ApplyStep::Addons, ApplySkipReason::MastercomfigNotInstalled)]
    );
    assert!(report.failed().is_empty(), "{report:?}");
    assert!(!report.applied().contains(&ApplyStep::Addons));
    assert!(!installed(&env, &flat));
    assert!(!installed(&env, &pyro));
    assert_eq!(active(&env), None);
}

#[test]
fn graphics_runs_before_addons_so_removing_mastercomfig_skips_the_turn_ons() {
    let env = library_env();
    place_mastercomfig(&env, Some("medium"));
    let flat = import_addon_as(&env, FLAT_ADDON);
    let p = addons_profile(&env, "Odd", SlotMode::None, ticked(&[&flat], false));
    let report = apply(&env, &p);
    assert_eq!(report.applied(), [ApplyStep::Graphics]);
    assert_eq!(
        report.skipped_steps(),
        [(ApplyStep::Addons, ApplySkipReason::MastercomfigNotInstalled)]
    );
    assert!(!installed(&env, &flat));
}

#[test]
fn a_deleted_addon_is_a_missing_item() {
    let env = library_env();
    place_mastercomfig(&env, Some("medium"));
    let flat = import_addon_as(&env, FLAT_ADDON);
    let pyro = import_addon_as(&env, PYRO_ADDON);
    let p = addons_profile(
        &env,
        "Both",
        SlotMode::Leave,
        ticked(&[&flat, &pyro], false),
    );
    env.lib.delete_addon(&flat, Some(&env.tf2)).unwrap();
    let err = env
        .lib
        .apply_profile(p.id(), &env.tf2, &builtin_catalog(), false)
        .unwrap_err();
    let ApplyProfileError::Missing(missing) = err else {
        panic!("{err:?}")
    };
    assert_eq!(missing.len(), 1, "{missing:?}");
    assert_eq!(
        missing[0].kind(),
        &MissingKind::Addon { name: flat.clone() }
    );
    assert_eq!(env.lib.missing_items(p.id()).unwrap(), missing);
    assert!(!installed(&env, &pyro), "a refused apply changes nothing");

    let report = env
        .lib
        .apply_profile(p.id(), &env.tf2, &builtin_catalog(), true)
        .unwrap();
    assert_eq!(report.skipped(), missing.as_slice());
    assert!(installed(&env, &pyro));
    assert_eq!(active(&env), None);
}

#[test]
fn turning_on_null_canceling_movement_reports_the_keys_it_binds() {
    let env = library_env();
    write_file(&env.tf2.tf_dir().join("cfg/config.cfg"), STOCK.as_bytes());
    place_mastercomfig(&env, Some("medium"));
    let null = import_addon_as(&env, NULL_ADDON);
    let p = addons_profile(&env, "Null", SlotMode::Leave, ticked(&[&null], false));
    let report = apply(&env, &p);
    let keys = report
        .movement()
        .expect("Null-Canceling Movement reports its keys");
    assert_eq!(keys.keys(MoveDirection::Forward), ["w"]);
}

#[test]
fn turning_off_null_canceling_movement_is_reported() {
    let env = library_env();
    write_file(&env.tf2.tf_dir().join("cfg/config.cfg"), STOCK.as_bytes());
    place_mastercomfig(&env, Some("medium"));
    let null = import_addon_as(&env, NULL_ADDON);
    env.lib.install_addon(&null, &env.tf2).unwrap();
    let p = addons_profile(&env, "Off", SlotMode::Leave, SlotMode::None);
    let report = apply(&env, &p);
    assert!(!installed(&env, &null));
    assert!(report.movement_off());
}

#[test]
fn turning_off_another_addon_reports_no_movement_change() {
    let env = library_env();
    let flat = import_addon_as(&env, FLAT_ADDON);
    env.lib.install_addon(&flat, &env.tf2).unwrap();
    let p = addons_profile(&env, "Off", SlotMode::Leave, SlotMode::None);
    let report = apply(&env, &p);
    assert!(!installed(&env, &flat));
    assert!(!report.movement_off());
}

#[test]
fn every_slot_runs_in_order() {
    let env = crosshair_env();
    place_mastercomfig(&env, Some("high"));
    let hud = import_hud_as(&env, "budhud", Provenance::Local);
    let hit = import_wav_as(&env, "Ding", Provenance::Local);
    let flat = import_addon_as(&env, FLAT_ADDON);
    let draft = ProfileDraft::new("Full")
        .with_hud(SlotMode::Set(DraftItem::Pick(hud.id().clone())))
        .with_sounds(SlotMode::Set((
            Some(DraftItem::Pick(hit.id().clone())),
            None,
        )))
        .with_crosshairs(SlotMode::Set(vec![DraftCrosshair::Set {
            weapon_script: "tf_weapon_scattergun".into(),
            crosshair: CrosshairRef::Builtin("cpma_9".into()),
        }]))
        .with_graphics(SlotMode::Set(Choices::new(Preset::Medium)))
        .with_addons(ticked(&[&flat], true));
    let p = env.lib.create_profile(&draft).unwrap();
    let report = apply(&env, &p);
    assert_eq!(
        report.applied(),
        [
            ApplyStep::Hud,
            ApplyStep::Hitsound,
            ApplyStep::Killsound,
            ApplyStep::Crosshairs,
            ApplyStep::Graphics,
            ApplyStep::Addons
        ]
    );
    assert!(
        report.failed().is_empty() && report.skipped_steps().is_empty(),
        "{report:?}"
    );
}
