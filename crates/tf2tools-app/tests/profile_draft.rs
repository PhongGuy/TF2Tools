//! The editor's draft: slot modes, the new-draft fill, the Add-ons rule, dirty and Save checks,
//! and the core draft it saves.

use crate::support;

use support::profiles::{FLAT, PYRO, addon, fixture, label, low_fps60, on_scattergun};
use tf2tools_app::pages::profiles::editor_model::{
    Draft, Mode, Picked, SaveBlock, SlotKind, save_block,
};
use tf2tools_app::pages::profiles::model::taken_message;
use tf2tools_core::{Choices, DraftItem, Preset, ProfileDraft, SlotMode};

fn modes(d: &Draft) -> [Mode; 5] {
    SlotKind::ALL.map(|s| d.mode(s))
}

#[test]
fn slots_and_modes_have_stable_keys_and_labels() {
    assert_eq!(
        SlotKind::ALL.map(SlotKind::key),
        ["hud", "sounds", "crosshairs", "graphics", "addons"]
    );
    let labels: Vec<String> = SlotKind::ALL
        .iter()
        .map(|s| s.label().to_string())
        .collect();
    assert_eq!(
        labels,
        ["HUD", "Sounds", "Crosshairs", "Graphics", "Add-ons"]
    );
    assert_eq!(Mode::ALL.map(Mode::key), ["set", "none", "leave"]);
    assert_eq!(Mode::of(&SlotMode::<u8>::Leave), Mode::Leave);
    assert_eq!(Mode::of(&SlotMode::<u8>::None), Mode::None);
    assert_eq!(Mode::of(&SlotMode::Set(1u8)), Mode::Set);
}

#[test]
fn a_saved_profile_opens_with_its_modes_and_content() {
    let f = fixture();
    let p = f.full("Comp");
    let d = Draft::from_profile(&p, &label);
    assert_eq!(d.name, "Comp");
    assert_eq!(modes(&d), [Mode::Set; 5]);
    assert_eq!(
        d.hud.as_ref().map(|h| h.folder.as_str()),
        Some(f.rayshud.folder_name())
    );
    assert_eq!(d.hit.as_ref().map(|h| h.name.as_str()), Some("Ding"));
    assert_eq!(d.kill.as_ref().map(|h| h.name.as_str()), Some("Bell"));
    assert_eq!(d.crosshairs.len(), 1);
    assert_eq!(d.crosshairs[0].script, "tf_weapon_scattergun");
    assert_eq!(d.graphics, low_fps60());
    let names: Vec<&str> = d.addons.iter().map(|a| a.name.as_str()).collect();
    assert_eq!(names, ["no-pyroland"]);
    assert!(d.only_these);
    assert!(!d.differs_from(&Draft::from_profile(&p, &label)));
}

#[test]
fn saving_an_unchanged_draft_writes_the_profile_back_as_it_was() {
    let f = fixture();
    let p = f.full("Comp");
    let draft = Draft::from_profile(&p, &label).to_profile_draft();
    let done = f.s.library.update_profile(p.id(), &draft).unwrap();
    let saved = done.profile();
    assert_eq!(saved.hud(), p.hud());
    assert_eq!(saved.sounds(), p.sounds());
    assert_eq!(saved.crosshairs(), p.crosshairs());
    assert_eq!(saved.graphics(), p.graphics());
    assert_eq!(saved.addons(), p.addons());
    assert!(!done.deactivated());
}

#[test]
fn none_and_leave_slots_open_empty_and_save_their_mode() {
    let f = fixture();
    let draft = ProfileDraft::new("Mixed")
        .with_hud(SlotMode::Leave)
        .with_sounds(SlotMode::None)
        .with_crosshairs(SlotMode::Leave)
        .with_graphics(SlotMode::None)
        .with_addons(SlotMode::Leave);
    let p = f.create(draft.clone());
    let d = Draft::from_profile(&p, &label);
    assert_eq!(
        modes(&d),
        [
            Mode::Leave,
            Mode::None,
            Mode::Leave,
            Mode::None,
            Mode::Leave
        ]
    );
    assert!(d.hud.is_none() && d.hit.is_none() && d.kill.is_none());
    assert!(d.crosshairs.is_empty() && d.addons.is_empty() && !d.only_these);
    assert_eq!(d.graphics, Choices::new(Preset::Medium));
    assert_eq!(d.to_profile_draft(), draft);
}

#[test]
fn a_mode_switch_keeps_the_content_for_switching_back() {
    let f = fixture();
    let base = Draft::from_profile(&f.full("Comp"), &label);
    let mut d = base.clone();
    for slot in SlotKind::ALL {
        assert!(d.set_mode(slot, Mode::Leave), "{slot:?}");
    }
    assert_eq!(modes(&d), [Mode::Leave; 5]);
    assert!(d.differs_from(&base));
    for slot in SlotKind::ALL {
        assert!(d.set_mode(slot, Mode::Set), "{slot:?}");
    }
    assert!(!d.differs_from(&base), "every slot's content came back");
}

#[test]
fn graphics_none_moves_addons_to_none_and_blocks_addons_set() {
    let f = fixture();
    let mut d = Draft::from_profile(&f.full("Comp"), &label);
    assert!(!d.addons_need_graphics());
    assert!(d.set_mode(SlotKind::Graphics, Mode::None));
    assert_eq!(d.mode(SlotKind::Addons), Mode::None);
    assert!(d.addons_need_graphics());
    assert!(!d.set_mode(SlotKind::Addons, Mode::Set));
    assert_eq!(d.mode(SlotKind::Addons), Mode::None);
    assert!(d.set_mode(SlotKind::Addons, Mode::Leave));
    assert!(d.set_mode(SlotKind::Graphics, Mode::Set));
    assert!(d.set_mode(SlotKind::Graphics, Mode::None));
    assert_eq!(
        d.mode(SlotKind::Addons),
        Mode::Leave,
        "only a Set slot moves"
    );
    assert!(d.set_mode(SlotKind::Graphics, Mode::Leave));
    assert!(d.set_mode(SlotKind::Addons, Mode::Set));
    assert_eq!(d.addons.len(), 1, "the ticked add-ons came back");
}

#[test]
fn dirty_trims_the_name_and_ignores_content_that_is_not_set() {
    let f = fixture();
    let base = Draft::from_profile(&f.full("Comp"), &label);
    let mut d = base.clone();
    d.name = "  Comp ".into();
    assert!(!d.differs_from(&base));
    d.name = "Pubs".into();
    assert!(d.differs_from(&base));

    let mut d = base.clone();
    d.hud = Some(Picked::hud(&f.rayshud));
    assert!(
        !d.differs_from(&base),
        "picking the saved HUD again changes nothing"
    );
    d.hud = Some(Picked::hud(&f.budhud));
    assert!(d.differs_from(&base));

    let mut none = base.clone();
    none.set_mode(SlotKind::Hud, Mode::None);
    let mut d = none.clone();
    d.hud = Some(Picked::hud(&f.budhud));
    assert!(!d.differs_from(&none), "a None slot's content is not saved");

    let mut d = base.clone();
    d.only_these = false;
    assert!(d.differs_from(&base));
    let mut d = base.clone();
    d.graphics = Choices::new(Preset::Low);
    assert!(d.differs_from(&base));
    let mut d = base.clone();
    d.crosshairs.clear();
    assert!(d.differs_from(&base));
}

#[test]
fn save_is_blocked_by_a_blank_or_taken_name_a_missing_hud_or_no_change() {
    let f = fixture();
    let base = Draft::from_profile(&f.full("Comp"), &label);
    assert_eq!(
        save_block(&base, &base, true, None),
        Some(SaveBlock::Unchanged)
    );
    let mut d = base.clone();
    d.name = "Pubs".into();
    assert_eq!(save_block(&d, &base, true, None), None);
    d.name = "   ".into();
    assert_eq!(
        save_block(&d, &base, true, None),
        Some(SaveBlock::BlankName)
    );
    d.name = "Casual".into();
    assert_eq!(
        save_block(&d, &base, true, Some("Casual".into())),
        Some(SaveBlock::NameTaken("Casual".into()))
    );
    d.hud = None;
    assert_eq!(save_block(&d, &base, true, None), Some(SaveBlock::NoHud));
    d.set_mode(SlotKind::Hud, Mode::None);
    assert_eq!(save_block(&d, &base, true, None), None);

    let mut start = base.clone();
    start.name = String::new();
    assert_eq!(
        save_block(&start, &start, false, None),
        Some(SaveBlock::BlankName)
    );
    let mut named = start.clone();
    named.name = "New".into();
    assert_eq!(
        save_block(&named, &start, false, None),
        None,
        "a new draft saves unchanged"
    );

    assert_eq!(
        SaveBlock::BlankName.message().as_deref(),
        Some("Give this profile a name")
    );
    assert_eq!(
        SaveBlock::NameTaken("Casual".into()).message().as_deref(),
        Some(taken_message("Casual").as_str())
    );
    assert_eq!(
        SaveBlock::NoHud.message().as_deref(),
        Some("Pick a HUD, or choose None")
    );
    assert_eq!(SaveBlock::Unchanged.message(), None);
}

#[test]
fn a_new_draft_copies_what_is_installed() {
    let f = fixture();
    let tf2 = f.tf2();
    let base = f.create(
        ProfileDraft::new("Base")
            .with_hud(SlotMode::Set(DraftItem::Pick(f.rayshud.id().clone())))
            .with_sounds(SlotMode::Set((
                Some(DraftItem::Pick(f.ding.id().clone())),
                None,
            )))
            .with_crosshairs(SlotMode::Set(vec![on_scattergun(f.builtin(0))])),
    );
    f.s.library
        .apply_profile(base.id(), &tf2, &f.s.builtins, false)
        .unwrap();
    let current = f.s.library.current_setup(&tf2).unwrap();
    let d = Draft::from_current(&current, &label);
    assert_eq!(d.name, "");
    assert_eq!(
        modes(&d),
        [Mode::Set, Mode::Set, Mode::Set, Mode::None, Mode::Leave],
        "no mastercomfig: Graphics None and Add-ons Leave"
    );
    assert_eq!(
        d.hud.as_ref().map(|h| h.folder.as_str()),
        Some(f.rayshud.folder_name())
    );
    assert_eq!(d.hit.as_ref().map(|h| h.name.as_str()), Some("Ding"));
    assert!(d.kill.is_none());
    assert_eq!(d.crosshairs.len(), 1);
    let mut named = d.clone();
    named.name = "Copy".into();
    let made = f.create(named.to_profile_draft());
    assert_eq!(
        made.hud().as_set().map(|h| h.folder_name()),
        Some(f.rayshud.folder_name())
    );
    assert_eq!(made.crosshairs().as_set().map(Vec::len), Some(1));
}

#[test]
fn a_managed_mastercomfig_fills_graphics_and_addons() {
    let f = fixture();
    let tf2 = f.tf2();
    support::seed_managed_mastercomfig(&f.env, "9.100.1", "low");
    support::seed_mastercomfig_choices(&f.env, r#"{"fpscap":"60"}"#);
    f.s.library.install_addon(&addon(PYRO), &tf2).unwrap();
    let d = Draft::from_current(&f.s.library.current_setup(&tf2).unwrap(), &label);
    assert_eq!(d.mode(SlotKind::Hud), Mode::None, "no managed HUD");
    assert_eq!(d.mode(SlotKind::Graphics), Mode::Set);
    assert_eq!(d.graphics, low_fps60());
    assert_eq!(d.mode(SlotKind::Addons), Mode::Set);
    let names: Vec<&str> = d.addons.iter().map(|a| a.name.as_str()).collect();
    assert_eq!(names, ["no-pyroland"]);
    assert!(d.only_these);
}

#[test]
fn an_outside_mastercomfig_leaves_graphics_and_sets_addons() {
    let f = fixture();
    let base = f.env.game.join("tf/custom/mastercomfig-base.vpk");
    std::fs::write(base, support::mastercomfig_base("9.100.1")).unwrap();
    let d = Draft::from_current(&f.s.library.current_setup(&f.tf2()).unwrap(), &label);
    assert_eq!(d.mode(SlotKind::Graphics), Mode::Leave);
    assert_eq!(d.mode(SlotKind::Addons), Mode::Set);
    assert!(d.addons.is_empty());
}

#[test]
fn an_added_addon_carries_its_display_name() {
    let f = fixture();
    let entries = f.s.library.list_addons(None).unwrap();
    let flat = entries.iter().find(|e| e.name() == &addon(FLAT)).unwrap();
    let picked = Picked::addon(flat);
    assert_eq!(picked.name, "Flat Mouse");
    assert_eq!(picked.folder, FLAT);
    assert_eq!(picked.item, DraftItem::Pick(addon(FLAT)));
}
