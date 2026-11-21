//! Applying the Graphics slot: a preset over a managed mastercomfig, its removal, the skip
//! reasons, and the module choices left to the app.

use crate::common;

use std::fs;
use std::path::PathBuf;

use common::{
    LibraryEnv, builtin_catalog, library_env, low_choices, place_mastercomfig, write_file,
};
use tf2tools_core::{
    ApplyProfileReport, ApplySkipReason, ApplyStep, Choices, MastercomfigStatus, Preset, Profile,
    ProfileDraft, ProfileId, SlotMode,
};

const STOCK: &str = "unbindall\r\nbind \"a\" \"+moveleft\"\r\nbind \"d\" \"+moveright\"\r\n\
                     bind \"e\" \"+use\"\r\nbind \"s\" \"+back\"\r\nbind \"w\" \"+forward\"\r\n";
const SAVED: &str = "unbindall\r\nbind \"a\" \"+ml\"\r\nbind \"d\" \"+mr\"\r\n\
                     bind \"e\" \"+use\"\r\nbind \"s\" \"+mb\"\r\nbind \"w\" \"+mf\"\r\n";

fn config(env: &LibraryEnv) -> PathBuf {
    env.tf2.tf_dir().join("cfg/config.cfg")
}

/// A profile that leaves every slot but Graphics.
fn graphics_only(env: &LibraryEnv, name: &str, slot: SlotMode<Choices>) -> Profile {
    let draft = ProfileDraft::new(name)
        .with_hud(SlotMode::Leave)
        .with_sounds(SlotMode::Leave)
        .with_crosshairs(SlotMode::Leave)
        .with_graphics(slot);
    env.lib.create_profile(&draft).unwrap()
}

fn apply(env: &LibraryEnv, p: &Profile) -> ApplyProfileReport {
    env.lib
        .apply_profile(p.id(), &env.tf2, &builtin_catalog(), false)
        .unwrap()
}

fn active(env: &LibraryEnv) -> Option<ProfileId> {
    env.lib.active_profile().unwrap()
}

#[test]
fn graphics_set_writes_the_preset_and_hands_the_choices_to_the_app() {
    let env = library_env();
    place_mastercomfig(&env, Some("high"));
    let p = graphics_only(&env, "Low", SlotMode::Set(low_choices()));
    let report = apply(&env, &p);
    assert_eq!(report.applied(), [ApplyStep::Graphics]);
    assert_eq!(report.pending_modules(), Some(&low_choices()));
    assert!(report.skipped_steps().is_empty(), "{report:?}");
    assert!(matches!(
        env.lib.mastercomfig_status(&env.tf2).unwrap(),
        MastercomfigStatus::Managed {
            preset: Preset::Low,
            ..
        }
    ));
    assert_eq!(
        active(&env),
        None,
        "the app marks it after the modules phase"
    );
}

#[test]
fn graphics_set_as_is_sends_no_module_choices() {
    let env = library_env();
    place_mastercomfig(&env, Some("high"));
    let p = graphics_only(&env, "Ultra", SlotMode::Set(Choices::new(Preset::Ultra)));
    let report = apply(&env, &p);
    assert_eq!(
        report
            .pending_modules()
            .map(|c| (c.preset(), c.modules().len())),
        Some((Preset::Ultra, 0))
    );
}

#[test]
fn graphics_set_without_mastercomfig_is_skipped_and_writes_nothing() {
    let env = library_env();
    let p = graphics_only(&env, "Low", SlotMode::Set(low_choices()));
    let report = apply(&env, &p);
    assert_eq!(
        report.skipped_steps(),
        [(
            ApplyStep::Graphics,
            ApplySkipReason::MastercomfigNotInstalled
        )]
    );
    assert!(
        report.applied().is_empty() && report.failed().is_empty(),
        "{report:?}"
    );
    assert_eq!(report.pending_modules(), None);
    assert!(!env.tf2.custom_dir().join("comfig-custom").exists());
    assert_eq!(active(&env), None);
}

#[test]
fn graphics_over_a_mastercomfig_tf2tools_does_not_manage_is_skipped() {
    for (name, slot) in [
        ("Set", SlotMode::Set(low_choices())),
        ("None", SlotMode::None),
    ] {
        let env = library_env();
        place_mastercomfig(&env, None);
        let p = graphics_only(&env, name, slot);
        let report = apply(&env, &p);
        assert_eq!(
            report.skipped_steps(),
            [(ApplyStep::Graphics, ApplySkipReason::MastercomfigNotManaged)],
            "{name}"
        );
        assert!(
            env.tf2.custom_dir().join("mastercomfig-base.vpk").is_file(),
            "{name}"
        );
        assert_eq!(active(&env), None, "{name}");
    }
}

#[test]
fn graphics_none_removes_a_managed_mastercomfig_and_reports_the_restored_binds() {
    let env = library_env();
    write_file(&config(&env), SAVED.as_bytes());
    place_mastercomfig(&env, Some("medium"));
    let p = graphics_only(&env, "Stock", SlotMode::None);
    let report = apply(&env, &p);
    assert_eq!(report.applied(), [ApplyStep::Graphics]);
    assert!(report.binds_restored());
    assert_eq!(
        env.lib.mastercomfig_status(&env.tf2).unwrap(),
        MastercomfigStatus::NotInstalled
    );
    assert_eq!(fs::read_to_string(config(&env)).unwrap(), STOCK);
    assert_eq!(active(&env).as_ref(), Some(p.id()));
}

#[test]
fn graphics_none_without_mastercomfig_has_nothing_to_do() {
    let env = library_env();
    let p = graphics_only(&env, "Stock", SlotMode::None);
    let report = apply(&env, &p);
    assert!(
        report.applied().is_empty() && report.skipped_steps().is_empty(),
        "{report:?}"
    );
    assert!(!report.binds_restored());
    assert_eq!(active(&env).as_ref(), Some(p.id()));
}

#[test]
fn a_finished_modules_phase_marks_a_graphics_profile_active() {
    let env = library_env();
    place_mastercomfig(&env, Some("high"));
    let p = graphics_only(&env, "Medium", SlotMode::Set(Choices::new(Preset::Medium)));
    let report = apply(&env, &p);
    assert!(report.pending_modules().is_some());
    assert_eq!(active(&env), None);
    assert!(env.lib.mark_profile_active(report.profile()).unwrap());
    assert_eq!(active(&env).as_ref(), Some(p.id()));
}
