//! The side panel schema and the change-list helpers the panel's reset and unhide use.

use crate::common;

use common::hud_stock::{HEALTH, block, stock_files, stock_files_with, with_blocks};
use tf2tools_core::{ClassSchema, HudModel, PartKey, Platform, PropertyKind, Screen};

const HV: &str = "HudPlayerStatus/HudPlayerHealth/PlayerStatusHealthValue";
const HD: Screen = Screen::new(1920, 1080);

fn model() -> HudModel {
    HudModel::load(&stock_files(), Platform::Linux).unwrap()
}

fn hv() -> PartKey {
    PartKey::new(HV)
}

fn stage1(schema: &ClassSchema) -> Vec<(&'static str, PropertyKind)> {
    schema.up_to(1).map(|p| (p.label(), p.kind())).collect()
}

fn class(name: &str) -> &'static ClassSchema {
    ClassSchema::all()
        .iter()
        .find(|s| s.class() == name)
        .unwrap()
}

#[test]
fn a_label_offers_layout_and_its_colour_slot_in_stage_1() {
    let m = model();
    let schema = ClassSchema::for_part(m.part(&hv()).unwrap());
    assert_eq!(schema.class(), "CExLabel");
    assert_eq!(
        stage1(schema),
        [
            ("Position", PropertyKind::Position),
            ("Size", PropertyKind::Size),
            ("Layer", PropertyKind::Layer),
            ("Hide", PropertyKind::Hide),
            ("Colour", PropertyKind::Colour),
        ]
    );
    assert_eq!(schema.get("FGCOLOR").map(|p| p.key()), Some("fgcolor"));
    assert_eq!(schema.get("labelText").map(|p| p.stage()), Some(2));
}

#[test]
fn position_and_size_rows_write_both_axes() {
    let schema = class("common");
    let keys: Vec<_> = schema.up_to(1).map(|p| p.keys()).collect();
    assert_eq!(
        keys,
        [
            &["xpos", "ypos"][..],
            &["wide", "tall"],
            &["zpos"],
            &["visible"]
        ]
    );
}

#[test]
fn colour_slots_follow_the_class() {
    let slot = |name: &str| {
        class(name)
            .up_to(1)
            .find(|p| p.kind() == PropertyKind::Colour)
            .map(|p| p.key())
    };
    assert_eq!(slot("CExLabel"), Some("fgcolor"));
    assert_eq!(slot("Label"), Some("fgcolor_override"));
    assert_eq!(slot("ImagePanel"), Some("drawcolor"));
    assert_eq!(slot("CTFImagePanel"), Some("drawcolor"));
    assert_eq!(slot("EditablePanel"), None);
}

#[test]
fn classless_and_named_elements_get_their_schema() {
    let m = model();
    let of = |k: &str| ClassSchema::for_part(m.part(&PartKey::new(k)).unwrap()).class();
    assert_eq!(of("HudWeaponAmmo"), "common");
    assert_eq!(of("HudPlayerStatus/HudPlayerHealth"), "EditablePanel");
    assert_eq!(of("CDamageAccountPanel"), "CDamageAccountPanel");
    assert_eq!(stage1(class("common")).len(), 4);
}

#[test]
fn reset_drops_only_that_propertys_changes_on_that_part() {
    let m = model();
    let mut changes = m.move_part(&hv(), 45, 0, HD, 1).unwrap();
    changes.extend(m.recolour_part(&hv(), "255 0 0 255", 2).unwrap());
    let clip = PartKey::new("HudWeaponAmmo/AmmoInClip");
    changes.extend(m.move_part(&clip, 0, 45, HD, 3).unwrap());
    let kept = m.reset_property(&hv(), &["XPOS", "ypos"], &changes);
    let groups: Vec<u32> = kept.iter().map(|c| c.group()).collect();
    assert_eq!(groups, [2, 3]);
    assert_eq!(m.reset_property(&hv(), &["labelText"], &changes), changes);
}

#[test]
fn hide_groups_find_the_hide_and_nothing_else() {
    let m = model();
    let mut changes = m.move_part(&hv(), 45, 0, HD, 1).unwrap();
    changes.extend(m.hide_part(&hv(), 2).unwrap());
    changes.extend(m.recolour_part(&hv(), "TanLight", 3).unwrap());
    assert_eq!(m.hide_groups(&hv(), &changes), [2]);
    assert!(
        m.hide_groups(&PartKey::new("HudWeaponAmmo/AmmoInClip"), &changes)
            .is_empty()
    );
}

#[test]
fn reset_drops_every_line_of_the_key_including_if_blocks() {
    let m = model();
    let target = PartKey::new("CMainTargetID");
    let mut changes = m.move_part(&target, 0, 45, HD, 1).unwrap();
    let written = changes.len();
    assert!(
        written >= 2,
        "the base ypos and the if_vr ypos: {changes:?}"
    );
    changes.extend(m.move_part(&hv(), 45, 0, HD, 2).unwrap());
    let kept = m.reset_property(&target, &["xpos", "ypos"], &changes);
    assert!(kept.iter().all(|c| c.group() == 2), "{kept:?}");
    assert_eq!(kept.len(), changes.len() - written);
}

#[test]
fn reset_on_an_unknown_part_keeps_every_change() {
    let m = model();
    let changes = m.move_part(&hv(), 45, 0, HD, 1).unwrap();
    let kept = m.reset_property(&PartKey::new("NoSuchPart"), &["xpos"], &changes);
    assert_eq!(kept, changes);
}

#[test]
fn a_death_notice_is_found_by_its_name_over_its_class() {
    let health = with_blocks(HEALTH, &block("HudDeathNotice", "EditablePanel", &[]));
    let files = stock_files_with(&[("resource/ui/hudplayerhealth.res", Some(&health))]);
    let m = HudModel::load(&files, Platform::Linux).unwrap();
    let part = m
        .part(&PartKey::new(
            "HudPlayerStatus/HudPlayerHealth/HudDeathNotice",
        ))
        .expect("the death notice part");
    assert_eq!(ClassSchema::for_part(part).class(), "HudDeathNotice");
}

#[test]
fn rows_carry_the_engine_defaults_from_the_survey() {
    let by_class = |class: &str| {
        tf2tools_core::ClassSchema::all()
            .iter()
            .find(|s| s.class() == class)
            .unwrap()
    };
    let default = |class: &str, key: &str| by_class(class).get(key).and_then(|p| p.default());
    assert_eq!(default("common", "xpos"), Some("0"));
    assert_eq!(default("common", "wide"), None);
    assert_eq!(default("common", "zpos"), None);
    assert_eq!(default("common", "visible"), Some("1"));
    assert_eq!(default("CExLabel", "fgcolor"), Some("Label.TextColor"));
    assert_eq!(default("CExLabel", "labelText"), Some(""));
    assert_eq!(default("CExLabel", "textAlignment"), Some("west"));
    assert_eq!(default("CExLabel", "font"), Some("Default"));
    assert_eq!(default("CExLabel", "allcaps"), Some("0"));
    assert_eq!(
        default("Label", "fgcolor_override"),
        Some("Label.TextColor")
    );
    assert_eq!(default("ImagePanel", "drawcolor"), Some("255 255 255 255"));
    assert_eq!(default("CTFImagePanel", "fillcolor"), Some("0 0 0 0"));
    assert_eq!(default("EditablePanel", "paintbackground"), Some("1"));
    assert_eq!(default("CAvatarImagePanel", "color_outline"), Some("Black"));
    assert_eq!(default("HudDeathNotice", "LineHeight"), Some("16"));
    assert_eq!(default("HudDeathNotice", "LineSpacing"), Some("4"));
    assert_eq!(default("HudDeathNotice", "TeamRed"), Some("255 64 64 255"));
    assert_eq!(
        default("HudDeathNotice", "TeamBlue"),
        Some("153 204 255 255")
    );
    assert_eq!(
        default("HudDeathNotice", "BaseBackgroundColor"),
        Some("46 43 42 220")
    );
    assert_eq!(
        default("HudDeathNotice", "LocalBackgroundColor"),
        Some("245 229 196 200")
    );
    assert_eq!(default("HudDeathNotice", "IconColor"), Some("255 80 0 255"));
    assert_eq!(default("HudDeathNotice", "TextFont"), Some("Default"));
    assert_eq!(
        default("CDamageAccountPanel", "delta_lifetime"),
        Some("2.0")
    );
    assert_eq!(default("SectionedListPanel", "linespacing"), Some("20"));
    assert_eq!(default("SectionedListPanel", "sectiongap"), Some("8"));
}

#[test]
fn every_offered_default_is_text_the_maker_can_write() {
    for schema in tf2tools_core::ClassSchema::all() {
        for row in schema.properties() {
            if let Some(d) = row.default() {
                assert!(
                    !d.contains(['"', '\r', '\n']),
                    "{} {}",
                    schema.class(),
                    row.key()
                );
            }
        }
    }
}
