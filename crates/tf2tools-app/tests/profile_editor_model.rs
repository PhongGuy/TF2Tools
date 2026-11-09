//! The profile editor's pure rules: the next free name, each edit's draft, slot texts and
//! crosshair labels.

use crate::support;

use tf2tools_app::pages::profiles::editor_model::{
    Edit, Slot, SlotState, crosshair_labels, keep_label, next_profile_name, slot_state, slot_text,
    weapon_count,
};
use tf2tools_app::services::Services;
use tf2tools_core::{
    CrosshairRef, DraftCrosshair, DraftSlot, HudEntry, Profile, ProfileDraft, Provenance,
    SoundEntry, SoundKind,
};

/// 441 frames of 44.1 kHz stereo 16-bit silence, a file `import_sound` accepts.
fn wav() -> Vec<u8> {
    let data_len: u32 = 441 * 4;
    let mut out = Vec::new();
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data_len).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&2u16.to_le_bytes());
    out.extend_from_slice(&44_100u32.to_le_bytes());
    out.extend_from_slice(&(44_100u32 * 4).to_le_bytes());
    out.extend_from_slice(&4u16.to_le_bytes());
    out.extend_from_slice(&16u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_len.to_le_bytes());
    out.resize(out.len() + data_len as usize, 0);
    out
}

/// Two HUDs, two sounds and "Comp": rayshud, Ding as hitsound, Bell as killsound, the first
/// builtin on the scattergun.
struct Fixture {
    _env: support::Env,
    s: Services,
    budhud: HudEntry,
    ding: SoundEntry,
    bell: SoundEntry,
    comp: Profile,
}

fn fixture() -> Fixture {
    let env = support::env();
    let s = support::services(&env, support::no_steam());
    let hud = |name: &str| {
        let path = env.tmp.path().join(format!("{name}.zip"));
        std::fs::write(&path, support::hud_zip_bytes(name)).unwrap();
        s.library.import_hud(&path, Provenance::Local).unwrap()
    };
    let sound = |name: &str| {
        let path = env.tmp.path().join(format!("{name}.wav"));
        std::fs::write(&path, wav()).unwrap();
        s.library
            .import_sound(&path, name, Provenance::Local)
            .unwrap()
    };
    let (rayshud, budhud, ding, bell) =
        (hud("rayshud"), hud("budhud"), sound("Ding"), sound("Bell"));
    let first = s.builtins.all()[0].name().to_owned();
    let draft = ProfileDraft::new("Comp")
        .with_hud(DraftSlot::Item(rayshud.id().clone()))
        .with_hitsound(DraftSlot::Item(ding.id().clone()))
        .with_killsound(DraftSlot::Item(bell.id().clone()))
        .with_crosshairs(vec![DraftCrosshair::Set {
            weapon_script: "tf_weapon_scattergun".into(),
            crosshair: CrosshairRef::Builtin(first),
        }]);
    let comp = s.library.create_profile(&draft).unwrap();
    Fixture {
        _env: env,
        s,
        budhud,
        ding,
        bell,
        comp,
    }
}

#[test]
fn the_next_name_is_the_lowest_free_number_in_any_case() {
    let f = fixture();
    let all = |f: &Fixture| f.s.library.profiles().unwrap();
    assert_eq!(next_profile_name(&[]), "Profile 1");
    assert_eq!(next_profile_name(&all(&f)), "Profile 1");
    for name in ["profile 1", "PROFILE 3", "Profile 03"] {
        f.s.library
            .create_profile(&ProfileDraft::new(name))
            .unwrap();
    }
    assert_eq!(next_profile_name(&all(&f)), "Profile 2");
    f.s.library
        .create_profile(&ProfileDraft::new("Profile 2"))
        .unwrap();
    assert_eq!(next_profile_name(&all(&f)), "Profile 4");
}

#[test]
fn each_slot_edit_changes_only_its_slot() {
    let f = fixture();
    let keep = || ProfileDraft::from_profile(&f.comp);
    let bud = f.budhud.id().clone();
    assert_eq!(
        Edit::SetHud(bud.clone()).draft(&f.comp),
        keep().with_hud(DraftSlot::Item(bud))
    );
    assert_eq!(
        Edit::SetSound(SoundKind::Hitsound, f.bell.id().clone()).draft(&f.comp),
        keep().with_hitsound(DraftSlot::Item(f.bell.id().clone()))
    );
    assert_eq!(
        Edit::SetSound(SoundKind::Killsound, f.ding.id().clone()).draft(&f.comp),
        keep().with_killsound(DraftSlot::Item(f.ding.id().clone()))
    );
    assert_eq!(
        Edit::ClearSlot(Slot::Hud).draft(&f.comp),
        keep().with_hud(DraftSlot::Empty)
    );
    assert_eq!(
        Edit::ClearSlot(Slot::Hitsound).draft(&f.comp),
        keep().with_hitsound(DraftSlot::Empty)
    );
    assert_eq!(
        Edit::ClearSlot(Slot::Killsound).draft(&f.comp),
        keep().with_killsound(DraftSlot::Empty)
    );
    assert_eq!(
        Edit::ClearSlot(Slot::Crosshairs).draft(&f.comp),
        keep().with_crosshairs(Vec::new())
    );
    assert_eq!(
        Edit::ClearCrosshairs.draft(&f.comp),
        keep().with_crosshairs(Vec::new())
    );
    assert_eq!(
        Edit::Rename("Pubs".into()).draft(&f.comp),
        keep().with_name("Pubs")
    );
}

#[test]
fn a_saved_draft_keeps_the_other_slots() {
    let f = fixture();
    let draft = Edit::SetHud(f.budhud.id().clone()).draft(&f.comp);
    let done = f.s.library.update_profile(f.comp.id(), &draft).unwrap();
    let saved = done.profile();
    assert_eq!(
        saved.hud().map(|h| h.folder_name()),
        Some(f.budhud.folder_name())
    );
    assert_eq!(saved.name(), "Comp");
    assert_eq!(saved.hitsound(), f.comp.hitsound());
    assert_eq!(saved.killsound(), f.comp.killsound());
    assert_eq!(saved.crosshairs(), f.comp.crosshairs());
}

#[test]
fn assign_replaces_targets_in_place_and_appends_new_ones() {
    let f = fixture();
    let second = CrosshairRef::Builtin(f.s.builtins.all()[1].name().to_owned());
    let set = |script: &str| DraftCrosshair::Set {
        weapon_script: script.into(),
        crosshair: second.clone(),
    };
    let keep = || ProfileDraft::from_profile(&f.comp);
    let bulk = Edit::Assign {
        scripts: vec![
            "tf_weapon_scattergun".into(),
            "tf_weapon_pistol_scout".into(),
        ],
        crosshair: second.clone(),
    };
    assert_eq!(
        bulk.draft(&f.comp),
        keep().with_crosshairs(vec![
            set("tf_weapon_scattergun"),
            set("tf_weapon_pistol_scout")
        ])
    );
    let one = Edit::Assign {
        scripts: vec!["tf_weapon_bat".into()],
        crosshair: second.clone(),
    };
    let saved = DraftCrosshair::Keep(f.comp.crosshairs()[0].clone());
    assert_eq!(
        one.draft(&f.comp),
        keep().with_crosshairs(vec![saved, set("tf_weapon_bat")])
    );
}

#[test]
fn clearing_a_weapon_drops_only_that_entry() {
    let f = fixture();
    let keep = || ProfileDraft::from_profile(&f.comp);
    assert_eq!(
        Edit::ClearWeapon("tf_weapon_scattergun".into()).draft(&f.comp),
        keep().with_crosshairs(Vec::new())
    );
    assert_eq!(
        Edit::ClearWeapon("tf_weapon_bat".into()).draft(&f.comp),
        keep()
    );
}

#[test]
fn slots_have_stable_keys_and_labels() {
    let keys: Vec<_> = Slot::ALL.iter().map(|s| (s.key(), s.label())).collect();
    assert_eq!(
        keys,
        [
            ("hud", "HUD"),
            ("hitsound", "Hitsound"),
            ("killsound", "Killsound"),
            ("crosshairs", "Crosshairs")
        ]
    );
    assert_eq!(weapon_count(1), "1 weapon");
    assert_eq!(weapon_count(2), "2 weapons");
}

#[test]
fn slot_texts_name_the_item_say_none_or_say_missing() {
    let f = fixture();
    let texts = |p: &Profile, missing| -> Vec<String> {
        Slot::ALL
            .iter()
            .map(|&slot| slot_text(&slot_state(p, slot, missing), slot))
            .collect()
    };
    let hud_name = f.comp.hud().unwrap().name().to_owned();
    assert_eq!(
        texts(&f.comp, &[]),
        [
            hud_name.clone(),
            "Ding".into(),
            "Bell".into(),
            "1 weapon".into()
        ]
    );
    let vanilla =
        f.s.library
            .create_profile(&ProfileDraft::new("Vanilla"))
            .unwrap();
    assert_eq!(
        texts(&vanilla, &[]),
        ["None", "None", "None", "Default crosshairs"]
    );

    let rayshud =
        f.s.library
            .list_huds()
            .unwrap()
            .into_iter()
            .find(|h| h.folder_name() == f.comp.hud().unwrap().folder_name())
            .unwrap();
    f.s.library.delete_hud(rayshud.id()).unwrap();
    let missing = f.s.library.missing_items(f.comp.id()).unwrap();
    assert_eq!(
        slot_state(&f.comp, Slot::Hud, &missing),
        SlotState::Missing(hud_name.clone())
    );
    assert_eq!(texts(&f.comp, &missing)[0], format!("Missing: {hud_name}"));
    assert_eq!(
        slot_state(&f.comp, Slot::Hitsound, &missing),
        SlotState::Set("Ding".into())
    );
}

#[test]
fn crosshair_labels_name_builtins_library_and_missing() {
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
    let library = s.library.list_crosshairs().unwrap();
    let mut labels = crosshair_labels(&profile, &s.builtins, &library);
    labels.sort();
    assert_eq!(
        labels,
        [
            ("tf_weapon_rocketlauncher".to_owned(), "Zebra".to_owned()),
            (
                "tf_weapon_scattergun".to_owned(),
                first.display_name().to_owned()
            ),
        ]
    );
    s.library.delete_crosshair(zebra.id()).unwrap();
    let library = s.library.list_crosshairs().unwrap();
    let rocket = profile
        .crosshairs()
        .iter()
        .find(|c| c.weapon_script() == "tf_weapon_rocketlauncher")
        .unwrap();
    assert!(keep_label(rocket, &s.builtins, &library).ends_with(" (missing)"));
}
