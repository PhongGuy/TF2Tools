mod common;

use common::hud_borrow::{files_over_stock_bytes, source_over};
use common::hud_stock::{stock, stock_files};
use tf2tools_core::{Brings, HudFiles, HudModel, LeftBehind, PartKey, Platform};

const ROOT: &str = "HudPlayerStatus/HudPlayerHealth";
const BADGE: &str = "HudPlayerStatus/HudPlayerHealth/MineBadge";
const HEALTH_RES: &str = "resource/ui/hudplayerhealth.res";
const VMT: &str = "materials/vgui/replay/thumbnails/badge.vmt";
const VTF: &str = "materials/vgui/replay/thumbnails/badge.vtf";
const KEPT: &str = ".tf2tools/borrowed/materials/vgui/replay/thumbnails/badge";

fn health(image: &str) -> Vec<u8> {
    format!(
        r#""Resource/UI/HudPlayerHealth.res" {{
  HudPlayerHealth {{ ControlName EditablePanel xpos 0 ypos r120 wide 250 tall 120 visible 1 }}
  MineBadge {{ ControlName ImagePanel xpos 10 ypos 10 wide 20 tall 20 image {image} }}
}}"#
    )
    .into_bytes()
}

fn vmt(base: &str) -> Vec<u8> {
    format!("\"UnlitGeneric\"\n{{\n\t\"$basetexture\" \"{base}\"\n}}\n").into_bytes()
}

fn badge_source(extra: Vec<(&str, Vec<u8>)>) -> tf2tools_core::BorrowSource {
    let mut files = vec![
        (HEALTH_RES, health("replay/thumbnails/badge")),
        (VMT, vmt("vgui/replay/thumbnails/badge")),
        (VTF, b"VTF-SOURCE".to_vec()),
    ];
    files.extend(extra);
    source_over(files_over_stock_bytes(&files))
}

fn target() -> HudModel {
    HudModel::load(&stock_files(), Platform::Linux).unwrap()
}

#[test]
fn an_image_comes_with_its_material_and_texture() {
    let t = target();
    let b = t
        .borrow_part(
            &badge_source(vec![]),
            &stock_files(),
            &PartKey::new(ROOT),
            1,
        )
        .unwrap();
    let m = t.with_changes(b.changes()).into_model();
    let paths: Vec<&str> = m.assets().iter().map(|(p, _)| p.as_str()).collect();
    assert!(paths.contains(&VMT) && paths.contains(&VTF), "{paths:?}");
    assert_eq!(
        b.assets().get(&format!("{KEPT}.vtf")),
        Some(b"VTF-SOURCE".as_slice())
    );
    assert!(
        b.brings()
            .contains(&Brings::Image("replay/thumbnails/badge".into()))
    );
}

fn target_files_with(vtf: &[u8]) -> HudFiles {
    let mut layer: Vec<(String, Vec<u8>)> = stock()
        .into_iter()
        .map(|(p, t)| (p, t.into_bytes()))
        .collect();
    // The player's HUD has the picture's material too; only the texture's bytes vary.
    layer.push((VMT.to_owned(), vmt("vgui/replay/thumbnails/badge")));
    layer.push((VTF.to_owned(), vtf.to_vec()));
    HudFiles::from_memory(vec![layer])
}

#[test]
fn a_file_the_target_already_has_is_reused_when_identical_and_renamed_when_not() {
    let t = target();
    let same = t
        .borrow_part(
            &badge_source(vec![]),
            &target_files_with(b"VTF-SOURCE"),
            &PartKey::new(ROOT),
            1,
        )
        .unwrap();
    assert!(same.assets().is_empty() && same.renamed().is_empty());
    let other = t
        .borrow_part(
            &badge_source(vec![]),
            &target_files_with(b"DIFFERENT"),
            &PartKey::new(ROOT),
            1,
        )
        .unwrap();
    assert_eq!(other.renamed().len(), 2, "the material and the texture");
    let m = t.with_changes(other.changes()).into_model();
    assert_eq!(
        m.part(&PartKey::new(BADGE))
            .unwrap()
            .value("image")
            .unwrap()
            .text(),
        "replay/thumbnails/badge-b2"
    );
    let vmt_source = format!("{KEPT}-b2.vmt");
    assert!(String::from_utf8_lossy(other.assets().get(&vmt_source).unwrap()).contains("badge-b2"));
}

#[test]
fn a_missing_image_is_listed_and_the_key_kept() {
    let t = target();
    let src = source_over(files_over_stock_bytes(&[(
        HEALTH_RES,
        health("replay/thumbnails/nope"),
    )]));
    let b = t
        .borrow_part(&src, &stock_files(), &PartKey::new(ROOT), 1)
        .unwrap();
    assert!(
        b.left_behind()
            .contains(&LeftBehind::MissingFile("replay/thumbnails/nope".into()))
    );
    assert!(b.assets().is_empty());
    let m = t.with_changes(b.changes()).into_model();
    assert_eq!(
        m.part(&PartKey::new(BADGE))
            .unwrap()
            .value("image")
            .unwrap()
            .text(),
        "replay/thumbnails/nope"
    );
}

#[test]
fn a_code_set_image_replaces_the_stock_material() {
    let res = r#""Resource/UI/HudPlayerHealth.res" { HudPlayerHealth { ControlName EditablePanel xpos 0 ypos r120 wide 250 tall 120 visible 1 }
      PlayerStatusHealthImage { ControlName ImagePanel xpos 73 ypos 33 wide 51 tall 51 } }"#;
    let src = source_over(files_over_stock_bytes(&[
        (HEALTH_RES, res.as_bytes().to_vec()),
        (
            "materials/hud/health_color.vmt",
            vmt("hud/health_color_mine"),
        ),
        ("materials/hud/health_color_mine.vtf", b"MINE".to_vec()),
    ]));
    let t = target();
    let b = t
        .borrow_part(&src, &stock_files(), &PartKey::new(ROOT), 1)
        .unwrap();
    assert!(
        b.brings()
            .contains(&Brings::CodeImage("health_color".into()))
    );
    assert!(
        t.with_changes(b.changes())
            .model()
            .assets()
            .iter()
            .any(|(p, _)| p == "materials/hud/health_color.vmt")
    );
}

#[test]
fn a_path_that_climbs_out_of_the_hud_is_treated_as_missing() {
    let t = target();
    let src = source_over(files_over_stock_bytes(&[(
        HEALTH_RES,
        health("../../secret"),
    )]));
    let b = t
        .borrow_part(&src, &stock_files(), &PartKey::new(ROOT), 1)
        .unwrap();
    assert!(b.assets().is_empty());
    assert!(
        b.left_behind()
            .iter()
            .any(|l| matches!(l, LeftBehind::MissingFile(_)))
    );
}

#[test]
fn a_basetexture_that_climbs_out_is_missing_too() {
    let src = source_over(files_over_stock_bytes(&[
        (HEALTH_RES, health("replay/thumbnails/badge")),
        (VMT, vmt("../../secret")),
    ]));
    let b = target()
        .borrow_part(&src, &stock_files(), &PartKey::new(ROOT), 1)
        .unwrap();
    assert!(b.assets().is_empty());
    assert!(
        b.left_behind()
            .iter()
            .any(|l| matches!(l, LeftBehind::MissingFile(_)))
    );
}

/// A source whose health file has `n` image panels, each with a material and a 1-byte texture.
fn many_images(n: usize) -> tf2tools_core::BorrowSource {
    let names: Vec<String> = (0..n).map(|i| format!("replay/thumbnails/p{i}")).collect();
    let panels: String = names
        .iter()
        .enumerate()
        .map(|(i, name)| {
            format!("Img{i} {{ ControlName ImagePanel xpos 0 ypos 0 wide 5 tall 5 image {name} }} ")
        })
        .collect();
    let res = format!(
        "\"Resource/UI/HudPlayerHealth.res\" {{ HudPlayerHealth {{ ControlName EditablePanel wide 250 tall 120 }} {panels} }}"
    );
    let mut files: Vec<(String, Vec<u8>)> = vec![(HEALTH_RES.to_owned(), res.into_bytes())];
    for name in &names {
        files.push((
            format!("materials/vgui/{name}.vmt"),
            vmt(&format!("vgui/{name}")),
        ));
        files.push((format!("materials/vgui/{name}.vtf"), b"V".to_vec()));
    }
    let refs: Vec<(&str, Vec<u8>)> = files.iter().map(|(p, b)| (p.as_str(), b.clone())).collect();
    source_over(files_over_stock_bytes(&refs))
}

#[test]
fn thirty_two_images_are_exactly_the_cap_and_thirty_three_refuse_the_borrow() {
    let t = target();
    // Every borrowed file counts: 32 materials + 32 textures is 64, 33 images is 66.
    let ok = t
        .borrow_part(&many_images(32), &stock_files(), &PartKey::new(ROOT), 1)
        .unwrap();
    assert_eq!(ok.assets().len(), 64);
    let err = t
        .borrow_part(&many_images(33), &stock_files(), &PartKey::new(ROOT), 1)
        .unwrap_err();
    assert_eq!(err, tf2tools_core::BorrowRefused::TooMuch);
}

#[test]
fn one_texture_over_16_mib_refuses_the_borrow() {
    let src = source_over(files_over_stock_bytes(&[
        (HEALTH_RES, health("replay/thumbnails/badge")),
        (VMT, vmt("vgui/replay/thumbnails/badge")),
        (VTF, vec![0u8; 16 * 1024 * 1024 + 1]),
    ]));
    let err = target()
        .borrow_part(&src, &stock_files(), &PartKey::new(ROOT), 1)
        .unwrap_err();
    assert_eq!(err, tf2tools_core::BorrowRefused::TooMuch);
}
