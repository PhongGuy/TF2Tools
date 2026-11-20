//! What a card says about each slot, the new-draft note, and the Apply summary's rows.

use crate::support;

use support::profiles::{FLAT, NULL, PYRO, addon, fixture, label, low_fps60, on_scattergun};
use tf2tools_app::pages::profiles::editor_model::{
    CardLine, Draft, SlotKind, Summary, card_lines, graphics_text, left_line, new_draft_note,
    summary, weapon_crosshairs,
};
use tf2tools_core::{
    Choices, DraftCrosshair, DraftItem, ModuleChoice, Preset, ProfileDraft, SlotMode,
};

fn texts(lines: &[CardLine; 5]) -> Vec<(&'static str, &str, bool)> {
    lines
        .iter()
        .map(|l| (l.slot.key(), l.text.as_str(), l.left))
        .collect()
}

fn rows(s: &Summary) -> Vec<(&str, String, &str, &str)> {
    s.rows
        .iter()
        .map(|r| (r.key, r.label.to_string(), r.from.as_str(), r.to.as_str()))
        .collect()
}

#[test]
fn graphics_reads_its_preset_and_change_count() {
    assert_eq!(graphics_text(&Choices::new(Preset::Ultra)), "Ultra as is");
    assert_eq!(graphics_text(&low_fps60()), "Low, 1 change");
    let mut two = low_fps60();
    two.set(
        &support::module_catalog(),
        "decals",
        ModuleChoice::Value("high".into()),
    );
    assert_eq!(graphics_text(&two), "Low, 2 changes");
    assert_eq!(weapon_crosshairs(1), "1 weapon crosshair");
    assert_eq!(weapon_crosshairs(14), "14 weapon crosshairs");
}

#[test]
fn a_full_card_names_each_slot() {
    let f = fixture();
    let p = f.full("Comp");
    assert_eq!(
        texts(&card_lines(&p, &label)),
        [
            ("hud", "rayshud", false),
            ("sounds", "Hit: Ding, kill: Bell", false),
            ("crosshairs", "1 weapon crosshair", false),
            ("graphics", "Low, 1 change", false),
            ("addons", "Only no-pyroland", false),
        ]
    );
}

#[test]
fn none_and_leave_cards_read_their_mode() {
    let f = fixture();
    assert_eq!(
        texts(&card_lines(&f.every("Vanilla", true), &label)),
        [
            ("hud", "TF2's own HUD", false),
            ("sounds", "TF2 default sounds", false),
            ("crosshairs", "TF2's default crosshairs", false),
            ("graphics", "mastercomfig off", false),
            ("addons", "All add-ons off", false),
        ]
    );
    let leave = card_lines(&f.every("Untouched", false), &label);
    assert!(leave.iter().all(|l| l.text == "Left as is" && l.left));
}

#[test]
fn set_slots_with_little_in_them_still_read_plainly() {
    let f = fixture();
    let p = f.create(
        ProfileDraft::new("Mixed")
            .with_hud(SlotMode::Leave)
            .with_sounds(SlotMode::Set((
                Some(DraftItem::Pick(f.ding.id().clone())),
                None,
            )))
            .with_crosshairs(SlotMode::Set(Vec::new()))
            .with_graphics(SlotMode::Set(Choices::new(Preset::Ultra)))
            .with_addons(SlotMode::Set((
                vec![DraftItem::Pick(addon(PYRO)), DraftItem::Pick(addon(FLAT))],
                false,
            ))),
    );
    let lines = card_lines(&p, &label);
    assert_eq!(lines[1].text, "Hit: Ding, kill: TF2 default");
    assert_eq!(lines[2].text, "TF2's default crosshairs");
    assert_eq!(lines[3].text, "Ultra as is");
    assert_eq!(lines[4].text, "Turns on flat-mouse, no-pyroland");
    let only = f.create(ProfileDraft::new("Only").with_addons(SlotMode::Set((
        vec![DraftItem::Pick(addon(PYRO)), DraftItem::Pick(addon(FLAT))],
        true,
    ))));
    assert_eq!(
        card_lines(&only, &label)[4].text,
        "Only these 2: flat-mouse, no-pyroland"
    );
    let empty_only =
        f.create(ProfileDraft::new("Off").with_addons(SlotMode::Set((Vec::new(), true))));
    assert_eq!(card_lines(&empty_only, &label)[4].text, "All add-ons off");
    let empty =
        f.create(ProfileDraft::new("Nothing").with_addons(SlotMode::Set((Vec::new(), false))));
    assert_eq!(card_lines(&empty, &label)[4].text, "Turns no add-ons on");
}

#[test]
fn the_new_draft_note_lists_what_was_copied() {
    let f = fixture();
    let d = Draft::from_profile(&f.full("Comp"), &label);
    assert_eq!(
        new_draft_note(&d),
        "rayshud, Ding and Bell, 1 weapon crosshair, Low with 1 change, 1 add-on. Name it and save, or change anything that should be different."
    );
    let bare = Draft::from_profile(&f.every("Vanilla", true), &label);
    assert_eq!(
        new_draft_note(&bare),
        "Name it and save, or change anything that should be different."
    );
}

#[test]
fn the_installed_setup_differs_from_nothing() {
    let f = fixture();
    let current = f.current_after_base();
    let s = summary(&f.full("Comp"), &current, &label);
    assert!(s.rows.is_empty(), "{:?}", s.rows);
    assert!(s.left.is_empty());
    assert_eq!(left_line(&s.left), None);
}

#[test]
fn a_switch_lists_each_part_that_differs() {
    let f = fixture();
    let current = f.current_after_base();
    let casual = f.create(
        ProfileDraft::new("Casual")
            .with_hud(SlotMode::Set(DraftItem::Pick(f.budhud.id().clone())))
            .with_sounds(SlotMode::Set((
                Some(DraftItem::Pick(f.bell.id().clone())),
                None,
            )))
            .with_crosshairs(SlotMode::Leave)
            .with_graphics(SlotMode::Set(Choices::new(Preset::High)))
            .with_addons(SlotMode::Set((vec![DraftItem::Pick(addon(FLAT))], true))),
    );
    let s = summary(&casual, &current, &label);
    assert_eq!(
        rows(&s),
        [
            ("hud", "HUD".into(), "rayshud", "budhud"),
            ("hitsound", "Hitsound".into(), "Ding", "Bell"),
            ("killsound", "Killsound".into(), "Bell", "TF2 default"),
            ("graphics", "Graphics".into(), "Low, 1 change", "High as is"),
            (
                "addons",
                "Add-ons".into(),
                "1 on",
                "Turns on flat-mouse; turns off no-pyroland"
            ),
        ]
    );
    assert_eq!(s.left, [SlotKind::Crosshairs]);
    assert_eq!(
        left_line(&s.left).as_deref(),
        Some("Stays as it is: Crosshairs.")
    );
}

#[test]
fn none_everywhere_takes_each_part_out() {
    let f = fixture();
    let current = f.current_after_base();
    let s = summary(&f.every("Vanilla", true), &current, &label);
    assert_eq!(
        rows(&s),
        [
            ("hud", "HUD".into(), "rayshud", "TF2's own HUD"),
            ("sounds", "Sounds".into(), "Ding, Bell", "TF2 default"),
            (
                "crosshairs",
                "Crosshairs".into(),
                "1 weapon",
                "TF2's default"
            ),
            (
                "graphics",
                "Graphics".into(),
                "Low, 1 change",
                "mastercomfig removed"
            ),
            ("addons", "Add-ons".into(), "1 on", "Turns off no-pyroland"),
        ]
    );
    assert!(s.left.is_empty());
}

#[test]
fn leave_everywhere_changes_nothing_and_says_so() {
    let f = fixture();
    let current = f.current_after_base();
    let s = summary(&f.every("Untouched", false), &current, &label);
    assert!(s.rows.is_empty());
    assert_eq!(s.left, SlotKind::ALL);
    assert_eq!(
        left_line(&s.left).as_deref(),
        Some("Stays as it is: HUD, Sounds, Crosshairs, Graphics, Add-ons.")
    );
}

#[test]
fn other_crosshairs_count_their_weapons() {
    let f = fixture();
    let current = f.current_after_base();
    let pistol = DraftCrosshair::Set {
        weapon_script: "tf_weapon_pistol".into(),
        crosshair: f.builtin(0),
    };
    let p = f.create(
        ProfileDraft::new("Two")
            .with_hud(SlotMode::Leave)
            .with_sounds(SlotMode::Leave)
            .with_crosshairs(SlotMode::Set(vec![on_scattergun(f.builtin(1)), pistol])),
    );
    let s = summary(&p, &current, &label);
    assert_eq!(
        rows(&s),
        [("crosshairs", "Crosshairs".into(), "1 weapon", "2 weapons")]
    );
}

#[test]
fn null_movement_says_it_changes_the_movement_keys() {
    let f = fixture();
    let current = f.current_after_base();
    let p = f.create(
        ProfileDraft::new("Movement")
            .with_hud(SlotMode::Leave)
            .with_sounds(SlotMode::Leave)
            .with_crosshairs(SlotMode::Leave)
            .with_addons(SlotMode::Set((vec![DraftItem::Pick(addon(NULL))], false))),
    );
    let s = summary(&p, &current, &label);
    assert_eq!(
        rows(&s),
        [(
            "addons",
            "Add-ons".into(),
            "1 on",
            "Turns on null-canceling-movement, changes your movement keys"
        )]
    );
}
