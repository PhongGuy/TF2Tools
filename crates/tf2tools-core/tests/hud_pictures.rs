//! Replacing a part's picture: by its keys, or by the materials the game's code names.

mod common;

use common::hud_stock::{HEALTH, block, stock_files, stock_files_with, with_blocks};
use tf2tools_core::{
    ChangeTarget, EditRefused, HudFiles, HudImage, HudModel, PartKey, PictureTarget, Platform,
    RgbaImage, Scene, hud_texture_vtf, picture_id, picture_material,
};

const UNDER: &str = "HudPlayerStatus/HudPlayerHealth/";

fn k(name: &str) -> PartKey {
    PartKey::new(format!("{UNDER}{name}"))
}

fn model() -> HudModel {
    let size = [("wide", "20"), ("tall", "20")];
    let with = |extra: &[(&'static str, &'static str)]| [&size[..], extra].concat();
    let blocks = [
        block(
            "Badge",
            "ImagePanel",
            &with(&[("image", "replay/thumbnails/old")]),
        ),
        block(
            "Scaled",
            "ImagePanel",
            &with(&[("image", "x"), ("scaleImage", "1")]),
        ),
        block(
            "Team",
            "CTFImagePanel",
            &with(&[("image", "x"), ("teambg_2", "a"), ("teambg_3", "b")]),
        ),
        block(
            "Bare",
            "CTFImagePanel",
            &with(&[("teambg_2", "a"), ("teambg_3", "b")]),
        ),
    ]
    .concat();
    let health = with_blocks(HEALTH, &blocks);
    HudModel::load(
        &stock_files_with(&[("resource/ui/hudplayerhealth.res", Some(&health))]),
        Platform::Linux,
    )
    .unwrap()
}

fn vtf() -> Vec<u8> {
    hud_texture_vtf(&RgbaImage::from_pixels(2, 2, vec![255; 16]).unwrap()).unwrap()
}

fn value(m: &HudModel, part: &PartKey, key: &str) -> Option<String> {
    m.part(part)?.value(key).map(|l| l.text().to_owned())
}

#[test]
fn the_target_is_the_part_s_keys_or_the_code_materials() {
    let m = model();
    let code = PictureTarget::Code {
        materials: vec!["materials/hud/health_color.vmt".into()],
    };
    assert_eq!(m.picture_target(&k("PlayerStatusHealthImage")), Some(code));
    assert_eq!(
        m.picture_target(&k("Badge")),
        Some(PictureTarget::Keys(vec!["image"]))
    );
    assert_eq!(
        m.picture_target(&k("Team")),
        Some(PictureTarget::Keys(vec!["teambg_2", "teambg_3", "image"]))
    );
    assert_eq!(
        m.picture_target(&k("Bare")),
        Some(PictureTarget::Keys(vec!["teambg_2", "teambg_3"]))
    );
    assert_eq!(m.picture_target(&k("PlayerStatusHealthValue")), None);
}

#[test]
fn the_freeze_panel_overrides_both_team_panels_and_the_class_menu_offers_none() {
    let freeze = HudModel::load_scene(&stock_files(), Platform::Linux, Scene::FreezeCam).unwrap();
    let bg = freeze
        .parts()
        .iter()
        .find(|p| p.name().eq_ignore_ascii_case("FreezePanelBG"))
        .unwrap();
    let materials = vec![
        "materials/hud/color_panel_red.vmt".to_owned(),
        "materials/hud/color_panel_blu.vmt".to_owned(),
    ];
    assert_eq!(
        freeze.picture_target(bg.key()),
        Some(PictureTarget::Code { materials })
    );
    let class = HudModel::load_scene(&stock_files(), Platform::Linux, Scene::ClassSelect).unwrap();
    assert_eq!(
        class.picture_target(&PartKey::new("class/localPlayerImage")),
        None
    );
    assert_eq!(
        class.picture_target(&PartKey::new("class/scout/SubImage")),
        None
    );
}

#[test]
fn picture_material_and_id_are_stable() {
    assert_eq!(
        picture_material("../hud/health_color").as_deref(),
        Some("materials/hud/health_color.vmt")
    );
    assert_eq!(
        picture_material("Replay/Thumbnails/A.vmt").as_deref(),
        Some("materials/vgui/replay/thumbnails/a.vmt")
    );
    assert_eq!(picture_material(""), None);
    assert_eq!(picture_material("../../../cfg/x"), None);
    assert_eq!(picture_id(b"abc"), "ba7816bf8f01cfea");
    assert_eq!(picture_id(b"abc"), picture_id(b"abc"));
}

#[test]
fn a_team_picture_sets_every_key_stretches_and_adds_two_files() {
    let m = model();
    let id = picture_id(&vtf());
    let new = m.replace_picture(&k("Team"), vtf(), 7).unwrap();
    assert!(new.changes().iter().all(|c| c.group() == 7));
    let edited = m.with_changes(new.changes()).into_model();
    let want = format!("replay/thumbnails/tf2tools/{id}");
    for key in ["teambg_2", "teambg_3", "image"] {
        assert_eq!(
            value(&edited, &k("Team"), key).as_deref(),
            Some(want.as_str()),
            "{key}"
        );
    }
    assert_eq!(
        value(&edited, &k("Team"), "scaleImage").as_deref(),
        Some("1")
    );
    let layer = new
        .files()
        .iter()
        .map(|(s, b)| {
            (
                s.trim_start_matches(&format!(".tf2tools/images/{id}/"))
                    .to_owned(),
                b.clone(),
            )
        })
        .collect();
    let top = HudFiles::from_memory(vec![layer]);
    assert!(
        matches!(top.image(&want, Platform::Linux).unwrap(), HudImage::Found(t) if t.image().width() == 2)
    );
}

#[test]
fn an_image_already_stretched_gets_no_second_scale_line() {
    let new = model().replace_picture(&k("Scaled"), vtf(), 1).unwrap();
    let scale_sets = new.changes().iter()
        .filter(|c| matches!(c.target(), ChangeTarget::Set(s) if s.key().eq_ignore_ascii_case("scaleImage")))
        .count();
    assert_eq!(scale_sets, 0);
}

#[test]
fn a_code_picture_overrides_the_stock_material_and_sets_no_key() {
    let m = model();
    let new = m
        .replace_picture(&k("PlayerStatusHealthImage"), vtf(), 1)
        .unwrap();
    assert!(
        new.changes()
            .iter()
            .all(|c| matches!(c.target(), ChangeTarget::Asset(_)))
    );
    let edited = m.with_changes(new.changes()).into_model();
    assert!(
        edited
            .assets()
            .iter()
            .any(|(p, _)| p == "materials/hud/health_color.vmt")
    );
    let vmt = &new
        .files()
        .iter()
        .find(|(s, _)| s.ends_with("health_color.vmt"))
        .unwrap()
        .1;
    let base = format!(
        "\"$basetexture\" \"vgui/replay/thumbnails/tf2tools/{}\"",
        picture_id(&vtf())
    );
    assert!(String::from_utf8_lossy(vmt).contains(&base));
}

#[test]
fn refusals_name_what_was_wrong() {
    let m = model();
    let r = |key: &PartKey, bytes: Vec<u8>| m.replace_picture(key, bytes, 1).err();
    assert_eq!(
        r(&PartKey::new("nope"), vtf()),
        Some(EditRefused::UnknownPart)
    );
    assert_eq!(
        r(&k("PlayerStatusHealthValue"), vtf()),
        Some(EditRefused::NotOffered)
    );
    assert_eq!(
        r(&k("Badge"), b"not a vtf".to_vec()),
        Some(EditRefused::BadValue)
    );
}

#[test]
fn reset_drops_every_pick_of_the_part_and_nothing_else() {
    let m = model();
    let mut changes = m
        .recolour_part(&k("PlayerStatusHealthValue"), "255 0 0 255", 1)
        .unwrap();
    for (g, part) in [
        (2, "Badge"),
        (3, "Badge"),
        (4, "PlayerStatusHealthImage"),
        (5, "PlayerStatusHealthImage"),
    ] {
        changes.extend(
            m.replace_picture(&k(part), vtf(), g)
                .unwrap()
                .into_parts()
                .0,
        );
    }
    let kept = m.reset_picture(&k("Badge"), &changes);
    assert!(kept.iter().all(|c| c.group() != 2 && c.group() != 3));
    let kept = m.reset_picture(&k("PlayerStatusHealthImage"), &kept);
    assert!(
        kept.iter().all(|c| c.group() == 1),
        "only the recolour is left"
    );
}
