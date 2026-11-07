//! Background style: draw it or not, its colour, square or rounded corners.

use crate::common;

use common::hud_stock::{HEALTH, block, stock_files_with, with_blocks};
use tf2tools_core::{
    ClassSchema, EditRefused, HudModel, PartKey, Platform, PropertyKind, Screen,
    background_corners, corner_radius, paints_background,
};

fn model_with(class: &str, keys: &[(&str, &str)]) -> (HudModel, PartKey) {
    let mut all = vec![("wide", "40"), ("tall", "20")];
    all.extend_from_slice(keys);
    let health = with_blocks(HEALTH, &block("Box", class, &all));
    let files = stock_files_with(&[("resource/ui/hudplayerhealth.res", Some(&health))]);
    (
        HudModel::load(&files, Platform::Linux).unwrap(),
        PartKey::new("HudPlayerStatus/HudPlayerHealth/Box"),
    )
}

fn model(keys: &[(&str, &str)]) -> (HudModel, PartKey) {
    model_with("EditablePanel", keys)
}

#[test]
fn panels_offer_corners_as_choices() {
    let (m, key) = model(&[]);
    let schema = ClassSchema::for_part(m.part(&key).unwrap());
    let corners = schema.get("PaintBackgroundType").unwrap();
    assert_eq!(corners.kind(), PropertyKind::Choice);
    assert_eq!(corners.choices(), &[("Square", "0"), ("Rounded", "2")]);
    let which = schema.get("RoundedCorners").unwrap();
    assert_eq!(which.default(), Some("15"));
    assert_eq!(which.choices().len(), 5);
}

#[test]
fn set_property_takes_stage_5_background_rows() {
    let (m, key) = model(&[]);
    assert!(m.set_property(&key, "PaintBackgroundType", "2", 1).is_ok());
    assert!(m.set_property(&key, "RoundedCorners", "3", 1).is_ok());
    assert!(m.set_property(&key, "paintbackground", "0", 1).is_ok());
    assert!(
        m.set_property(&key, "bgcolor_override", "10 20 30 200", 1)
            .is_ok()
    );
    assert_eq!(
        m.set_property(&key, "PaintBackgroundType", "3", 1),
        Err(EditRefused::BadValue)
    );
    assert_eq!(
        m.set_property(&key, "RoundedCorners", "7", 1),
        Err(EditRefused::BadValue)
    );
}

#[test]
fn image_rows_stay_with_the_picture_path() {
    let (m, key) = model_with("ImagePanel", &[]);
    assert_eq!(
        m.set_property(&key, "image", "x", 1),
        Err(EditRefused::NotOffered)
    );
    assert!(m.set_property(&key, "scaleImage", "1", 1).is_ok());
}

#[test]
fn corners_round_only_when_painted_and_rounded() {
    let corners = |keys: &[(&str, &str)]| {
        let (m, key) = model(keys);
        background_corners(m.part(&key).unwrap())
    };
    assert_eq!(corners(&[]), None);
    assert_eq!(
        corners(&[("PaintBackgroundType", "3")]),
        None,
        "a fade the HUD set draws square"
    );
    assert_eq!(corners(&[("PaintBackgroundType", "2")]), Some(15));
    assert_eq!(
        corners(&[("PaintBackgroundType", "2"), ("RoundedCorners", "3")]),
        Some(3)
    );
    assert_eq!(
        corners(&[("PaintBackgroundType", "2"), ("RoundedCorners", "31")]),
        Some(15)
    );
    assert_eq!(
        corners(&[("PaintBackgroundType", "2"), ("paintbackground", "0")]),
        None
    );
}

#[test]
fn a_background_paints_unless_switched_off() {
    let paints = |keys: &[(&str, &str)]| {
        let (m, key) = model(keys);
        paints_background(m.part(&key).unwrap())
    };
    assert!(paints(&[]));
    assert!(paints(&[("paintbackground", "1")]));
    assert!(!paints(&[("paintbackground", "0")]));
}

#[test]
fn the_corner_radius_follows_the_engine_s_corner_size() {
    assert_eq!(corner_radius(Screen::new(1920, 1080)), 9);
    assert_eq!(corner_radius(Screen::new(1280, 720)), 8);
    assert_eq!(corner_radius(Screen::new(3840, 2160)), 18);
}
