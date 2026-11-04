mod common;

use common::hud_borrow::{files_over_stock_bytes, source_over};
use common::hud_stock::{HUDLAYOUT, stock_files};
use tf2tools_core::{HudModel, LeftBehind, LibraryKind, MakeTarget, PartKey, Platform};

const ROOT: &str = "HudPlayerStatus/HudPlayerHealth";
const HEALTH_RES: &str = "resource/ui/hudplayerhealth.res";
const VMT: &str = "materials/vgui/replay/thumbnails/badge.vmt";
const VTF: &str = "materials/vgui/replay/thumbnails/badge.vtf";

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
fn keys_the_game_owns_are_counted_not_copied() {
    let layout = HUDLAYOUT.replacen("xpos c-126", "xpos c-100", 1);
    let src = source_over(files_over_stock_bytes(&[(
        "scripts/hudlayout.res",
        layout.into_bytes(),
    )]));
    let t = target();
    let b = t
        .borrow_part(&src, &stock_files(), &PartKey::new("CMainTargetID"), 1)
        .unwrap();
    assert!(
        b.left_behind()
            .iter()
            .any(|l| matches!(l, LeftBehind::CodeKeys(n) if *n >= 1))
    );
}

#[test]
fn animations_that_name_the_part_are_carried() {
    let anim = "event HealthPulse\n{\n\tAnimate MineBadge FgColor \"255 0 0 255\" Linear 0.0 0.1\n}\nevent Other\n{\n\tAnimate Elsewhere FgColor \"0 0 0 255\" Linear 0.0 0.1\n}\n";
    let manifest = "hudanimations_manifest { file scripts/hudanimations_tf.txt }";
    let src = badge_source(vec![
        ("scripts/hudanimations_tf.txt", anim.as_bytes().to_vec()),
        (
            "scripts/hudanimations_manifest.txt",
            manifest.as_bytes().to_vec(),
        ),
    ]);
    let b = target()
        .borrow_part(&src, &stock_files(), &PartKey::new(ROOT), 1)
        .unwrap();
    assert!(
        b.brings()
            .contains(&tf2tools_core::Brings::Animation("HealthPulse".into()))
    );
    assert!(
        !b.brings()
            .contains(&tf2tools_core::Brings::Animation("Other".into()))
    );
    assert!(
        !b.left_behind()
            .iter()
            .any(|l| matches!(l, LeftBehind::Animations(_)))
    );
}

#[test]
fn a_borrow_survives_save_and_reopen() {
    let env = common::library_env();
    let t = target();
    let b = t
        .borrow_part(
            &badge_source(vec![]),
            &stock_files(),
            &PartKey::new(ROOT),
            1,
        )
        .unwrap();
    let saved = env
        .lib
        .save_made_hud_with_assets(
            &MakeTarget::New,
            "Mixed",
            &stock_files(),
            Platform::Linux,
            b.changes(),
            b.assets(),
        )
        .unwrap();
    assert_eq!(saved.dropped(), 0);
    let dir = env
        .dir
        .library_dir(LibraryKind::Huds)
        .join(saved.entry().folder_name());
    assert_eq!(std::fs::read(dir.join(VTF)).unwrap(), b"VTF-SOURCE");
    assert!(dir.join("TF2Tools-credits.txt").is_file());
    let made = env
        .lib
        .open_made_hud(saved.entry().id(), &stock_files(), Platform::Linux)
        .unwrap();
    assert_eq!(made.dropped(), 0);
}

#[test]
fn a_child_whose_block_cannot_be_added_refuses_the_borrow() {
    use tf2tools_core::BorrowRefused;
    // The player's file already has a block of this name, though it is not a control, so the
    // part is absent but the block cannot be added again: the replay drops the change.
    const NAME: &str = "HealthBonusSettings";
    let res = format!(
        "\"Resource/UI/HudPlayerHealth.res\"\n{{\n  HudPlayerHealth {{ ControlName EditablePanel wide 250 tall 120 }}\n  {NAME} {{ ControlName ImagePanel xpos 1 ypos 1 wide 5 tall 5 }}\n}}\n"
    );
    let src = source_over(files_over_stock_bytes(&[(
        "resource/ui/hudplayerhealth.res",
        res.into_bytes(),
    )]));
    let t = target();
    let key = PartKey::new(format!("{ROOT}/{NAME}"));
    assert!(t.part(&key).is_none() && src.model().part(&key).is_some());
    let err = t.borrow_part(&src, &stock_files(), &key, 1).unwrap_err();
    assert_eq!(err, BorrowRefused::NoRoom);
}
