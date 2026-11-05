mod support;

use std::collections::BTreeMap;

use tf2tools_app::pages::profiles::wizard_model::*;
use tf2tools_core::{CrosshairRef, DraftCrosshair, DraftSlot, LogLevel, ProfileDraft, Provenance};

#[test]
fn the_steps_are_v5s() {
    let labels: Vec<_> = WizardStep::ALL
        .iter()
        .map(|s| (s.number(), s.label()))
        .collect();
    assert_eq!(
        labels,
        [(1, "Name"), (2, "HUD"), (3, "Sounds"), (4, "Crosshairs")]
    );
    assert_eq!(
        WizardStep::Name.description(),
        "Choose a name for your profile."
    );
    assert_eq!(
        WizardStep::Hud.description(),
        "Select a HUD from your library (optional)."
    );
    assert_eq!(
        WizardStep::Sounds.description(),
        "Select hitsound and killsound (optional)."
    );
    assert_eq!(
        WizardStep::Crosshairs.description(),
        "Configure crosshair assignments (optional)."
    );
    assert_eq!(WizardStep::Name.next(), Some(WizardStep::Hud));
    assert_eq!(WizardStep::Crosshairs.next(), None);
    assert_eq!(WizardStep::Name.back(), None);
    assert_eq!(WizardStep::Sounds.back(), Some(WizardStep::Hud));
}

#[test]
fn a_name_must_have_text() {
    assert!(!can_proceed(""));
    assert!(!can_proceed("  \t "));
    assert!(can_proceed(" Comp "));
}

#[test]
fn labels_follow_create_or_edit() {
    assert_eq!(title(false), "Create New Profile");
    assert_eq!(title(true), "Edit Profile");
    assert_eq!(save_label(false, false), "Create Profile");
    assert_eq!(save_label(false, true), "Creating...");
    assert_eq!(save_label(true, false), "Save Changes");
    assert_eq!(save_label(true, true), "Saving...");
    assert_eq!(
        saved_toast(false, "Comp"),
        (
            "Profile created",
            "\"Comp\" is ready to activate".to_owned()
        )
    );
    assert_eq!(
        saved_toast(true, "Comp"),
        ("Profile updated", "\"Comp\" has been saved".to_owned())
    );
    assert_eq!(
        deactivated_toast("Comp"),
        (
            "Profile updated",
            "\"Comp\" was saved. Activate it to use the changes in game".to_owned()
        )
    );
    assert_eq!(failed_title(false), "Failed to create profile");
    assert_eq!(failed_title(true), "Failed to update profile");
    assert_eq!(copy_label(3), "Copy current (3)");
    assert_eq!(
        copy_note(1),
        "Will copy your 1 current crosshair assignment into this profile."
    );
    assert_eq!(
        copy_note(2),
        "Will copy your 2 current crosshair assignments into this profile."
    );
    assert_eq!(assigned_count(1), "1 weapon assigned");
    assert_eq!(assigned_count(4), "4 weapons assigned");
}

#[test]
fn hud_options_mark_a_missing_saved_hud() {
    let env = support::env();
    let s = support::services(&env, support::no_steam());
    let path = env.tmp.path().join("rayshud.zip");
    std::fs::write(&path, support::hud_zip_bytes("rayshud")).unwrap();
    let hud = s.library.import_hud(&path, Provenance::Local).unwrap();
    let library = s.library.list_huds().unwrap();

    let (options, initial) = hud_options(&library, None);
    let titles: Vec<_> = options
        .iter()
        .map(|o| (o.title.to_string(), o.key.clone()))
        .collect();
    assert_eq!(
        titles,
        [
            ("None (no HUD)".into(), "none".into()),
            ("rayshud".into(), "rayshud".into())
        ]
    );
    assert_eq!(options[0].value, DraftSlot::Empty);
    assert_eq!(initial, DraftSlot::Empty);

    let profile = s
        .library
        .create_profile(&ProfileDraft::new("P").with_hud(DraftSlot::Item(hud.id().clone())))
        .unwrap();
    let (_, initial) = hud_options(&library, profile.hud());
    assert_eq!(initial, DraftSlot::Item(hud.id().clone()));

    s.library.delete_hud(hud.id()).unwrap();
    let library = s.library.list_huds().unwrap();
    let (options, initial) = hud_options(&library, profile.hud());
    assert_eq!(initial, DraftSlot::Keep);
    let last = options.last().unwrap();
    assert_eq!(
        (last.title.as_ref(), last.key.as_str()),
        ("rayshud (missing)", "missing")
    );
    assert_eq!(last.value, DraftSlot::Keep);
}

#[test]
fn sound_options_start_with_their_none_label() {
    let (options, initial) = sound_options(&[], None, "killsound");
    assert_eq!(options.len(), 1);
    assert_eq!(options[0].title.as_ref(), "None (no killsound)");
    assert_eq!(
        (options[0].key.as_str(), options[0].field),
        ("none", "killsound")
    );
    assert_eq!(initial, DraftSlot::Empty);
}

#[test]
fn the_log_rows_are_v5s() {
    let env = support::env();
    let s = support::services(&env, support::no_steam());
    let profile = s
        .library
        .create_profile(&ProfileDraft::new("Comp"))
        .unwrap();
    s.activity.append(&log_entry(false, &profile)).unwrap();
    let row = &support::log_rows(&s)[0];
    assert_eq!(
        (
            row.level(),
            row.category(),
            row.action(),
            row.message(),
            row.details()
        ),
        (
            LogLevel::Info,
            "profile",
            "create",
            "Created profile: Comp",
            None
        )
    );
    s.activity.append(&log_entry(true, &profile)).unwrap();
    let row = &support::log_rows(&s)[0];
    // The profile's id is internal, so the row names it and carries no details.
    assert_eq!(
        (row.action(), row.message(), row.details()),
        ("update", "Updated profile: Comp", None)
    );
}

#[test]
fn edit_rules_hold_kept_crosshairs() {
    let env = support::env();
    let s = support::services(&env, support::no_steam());
    let first = s.builtins.all()[0].clone();
    let png = env.tmp.path().join("z.png");
    std::fs::write(&png, support::crosshair_png([255, 0, 0, 255])).unwrap();
    let zebra = s
        .library
        .import_crosshair(&png, "Zebra", Provenance::Local)
        .unwrap();
    let draft = ProfileDraft::new("P").with_crosshairs(vec![
        DraftCrosshair::Set {
            weapon_script: "tf_weapon_scattergun".into(),
            crosshair: CrosshairRef::Builtin(first.name().into()),
        },
        DraftCrosshair::Set {
            weapon_script: "tf_weapon_rocketlauncher".into(),
            crosshair: CrosshairRef::Library(zebra.id().clone()),
        },
    ]);
    let profile = s.library.create_profile(&draft).unwrap();
    assert_eq!(initial_mode(None), CrosshairMode::None);
    assert_eq!(initial_mode(Some(&profile)), CrosshairMode::Customize);
    let custom = custom_from_profile(&profile);
    assert_eq!(custom.len(), 2);
    assert!(matches!(
        custom["tf_weapon_scattergun"],
        CustomCrosshair::Keep(_)
    ));
    let library = s.library.list_crosshairs().unwrap();
    let labels: Vec<_> = profile
        .crosshairs()
        .iter()
        .map(|c| keep_label(c, &s.builtins, &library))
        .collect();
    assert_eq!(
        labels,
        ["Zebra".to_owned(), first.display_name().to_owned()]
    );
    s.library.delete_crosshair(zebra.id()).unwrap();
    let library = s.library.list_crosshairs().unwrap();
    let rocket = &profile.crosshairs()[0];
    assert_eq!(rocket.weapon_script(), "tf_weapon_rocketlauncher");
    assert!(keep_label(rocket, &s.builtins, &library).ends_with(" (missing)"));
    let kept = draft_crosshairs(CrosshairMode::Customize, &[], &custom);
    assert_eq!(
        kept,
        profile
            .crosshairs()
            .iter()
            .cloned()
            .map(DraftCrosshair::Keep)
            .collect::<Vec<_>>()
    );
    assert!(draft_crosshairs(CrosshairMode::None, &[], &custom).is_empty());
}

#[test]
fn copy_turns_live_assignments_into_sets() {
    let env = support::env();
    let s = support::services(&env, support::no_steam());
    let first = s.builtins.all()[0].name().to_owned();
    s.library
        .assign("tf_weapon_bat", &CrosshairRef::Builtin(first.clone()))
        .unwrap();
    let live = s.library.assignments().unwrap();
    assert_eq!(
        draft_crosshairs(CrosshairMode::Copy, &live, &BTreeMap::new()),
        [DraftCrosshair::Set {
            weapon_script: "tf_weapon_bat".into(),
            crosshair: CrosshairRef::Builtin(first)
        }]
    );
}
