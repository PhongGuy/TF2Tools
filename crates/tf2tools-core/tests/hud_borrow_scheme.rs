use crate::common;

use common::hud_borrow::source;
use common::hud_stock::stock_files;
use common::sfnt::sfnt_bytes;
use tf2tools_core::{Brings, ChangeTarget, HudModel, LeftBehind, PartKey, Platform, Rgba};

const ROOT: &str = "HudPlayerStatus/HudPlayerHealth";
const VALUE: &str = "HudPlayerStatus/HudPlayerHealth/PlayerStatusHealthValue";
const CS: &str = "resource/clientscheme.res";
const HEALTH_RES: &str = "resource/ui/hudplayerhealth.res";

fn health(fgcolor: &str, font: &str) -> String {
    format!(
        r#""Resource/UI/HudPlayerHealth.res" {{
  HudPlayerHealth {{ ControlName EditablePanel xpos 0 ypos r120 wide 250 tall 120 visible 1 }}
  PlayerStatusHealthValue {{ ControlName CExLabel xpos 76 ypos 52 wide 44 tall 18 fgcolor {fgcolor} font {font} labelText %Health% }}
}}"#
    )
}

fn scheme(colors: &str, fonts: &str, extra: &str) -> String {
    format!(r#""Scheme" {{ Colors {{ {colors} }} Fonts {{ {fonts} }} {extra} }}"#)
}

fn borrow(top: &[(&str, &str)]) -> (HudModel, tf2tools_core::Borrowed) {
    let t = HudModel::load(&stock_files(), Platform::Linux).unwrap();
    let b = t
        .borrow_part(&source(top), &stock_files(), &PartKey::new(ROOT), 1)
        .unwrap();
    (t, b)
}

#[test]
fn a_colour_and_font_the_target_lacks_come_across_and_keep_their_names() {
    let sch = scheme(
        r#"MineRed "200 10 10 255""#,
        r#"MineBig { "1" { name "TF2 Build" tall 30 weight 500 } }"#,
        "",
    );
    let (t, b) = borrow(&[(HEALTH_RES, &health("MineRed", "MineBig")), (CS, &sch)]);
    let m = t.with_changes(b.changes()).into_model();
    assert_eq!(
        m.scheme().named_colour("MineRed"),
        Some(Rgba::new(200, 10, 10, 255))
    );
    assert_eq!(m.scheme().font("MineBig", 1080).unwrap().tall(), 30);
    assert_eq!(
        m.part(&PartKey::new(VALUE))
            .unwrap()
            .value("fgcolor")
            .unwrap()
            .text(),
        "MineRed"
    );
    assert!(b.brings().contains(&Brings::Colour("MineRed".into())));
    assert!(b.brings().contains(&Brings::Font("MineBig".into())));
}

#[test]
fn a_colliding_name_is_renamed_and_the_copied_value_follows_it() {
    let sch = scheme(r#"TanDark "1 2 3 255""#, "", "");
    let (t, b) = borrow(&[
        (HEALTH_RES, &health("TanDark", "HudClassHealth")),
        (CS, &sch),
    ]);
    let m = t.with_changes(b.changes()).into_model();
    assert_eq!(
        b.renamed(),
        [("TanDark".to_owned(), "TanDark_2".to_owned())]
    );
    assert_eq!(
        m.scheme().named_colour("TanDark_2"),
        Some(Rgba::new(1, 2, 3, 255))
    );
    assert_eq!(
        m.scheme().named_colour("TanDark"),
        Some(Rgba::new(117, 107, 94, 255))
    );
    assert_eq!(
        m.part(&PartKey::new(VALUE))
            .unwrap()
            .value("fgcolor")
            .unwrap()
            .text(),
        "TanDark_2"
    );
}

#[test]
fn an_identical_definition_is_reused_and_writes_nothing_to_the_scheme() {
    let sch = scheme(r#"TanDark "117 107 94 255""#, "", "");
    let (_t, b) = borrow(&[
        (HEALTH_RES, &health("TanDark", "HudClassHealth")),
        (CS, &sch),
    ]);
    assert!(b.renamed().is_empty());
    assert!(!b.changes().iter().any(
        |c| matches!(c.target(), ChangeTarget::Set(s) if s.file() == CS)
            || matches!(c.target(), ChangeTarget::AddBlock(a) if a.file() == CS)
    ));
}

#[test]
fn a_font_file_the_source_ships_comes_with_its_family() {
    let sch = scheme(
        "",
        r#"MineBig { "1" { name "Big Numbers" tall 30 } }"#,
        r#"CustomFontFiles { "1" "resource/fonts/bignum.ttf" }"#,
    );
    let ttf = sfnt_bytes(&[(1, "Big Numbers")], false);
    let files = common::hud_borrow::files_over_stock_bytes(&[
        (HEALTH_RES, health("TanDark", "MineBig").into_bytes()),
        (CS, sch.into_bytes()),
        ("resource/fonts/bignum.ttf", ttf.clone()),
    ]);
    let t = HudModel::load(&stock_files(), Platform::Linux).unwrap();
    let src = common::hud_borrow::source_over(files);
    let b = t
        .borrow_part(&src, &stock_files(), &PartKey::new(ROOT), 1)
        .unwrap();
    let m = t.with_changes(b.changes()).into_model();
    assert_eq!(m.assets().len(), 1);
    assert!(m.assets()[0].0.starts_with("resource/fonts/tf2tools/"));
    assert_eq!(b.assets().get(&m.assets()[0].1), Some(ttf.as_slice()));
    assert!(b.brings().contains(&Brings::FontFile("Big Numbers".into())));
}

#[test]
fn a_family_nobody_ships_is_listed_and_the_borrow_still_applies() {
    let sch = scheme("", r#"MineBig { "1" { name "Mystery Sans" tall 30 } }"#, "");
    let (t, b) = borrow(&[(HEALTH_RES, &health("TanDark", "MineBig")), (CS, &sch)]);
    assert!(
        b.left_behind()
            .contains(&LeftBehind::MissingFont("Mystery Sans".into()))
    );
    assert_eq!(t.with_changes(b.changes()).dropped(), 0);
}

#[test]
fn a_font_path_that_climbs_out_of_resource_is_never_read() {
    let sch = scheme(
        "",
        r#"MineBig { "1" { name "Big Numbers" tall 30 } }"#,
        r#"CustomFontFiles { "1" "../fonts/bignum.ttf" }"#,
    );
    let ttf = sfnt_bytes(&[(1, "Big Numbers")], false);
    let files = common::hud_borrow::files_over_stock_bytes(&[
        (HEALTH_RES, health("TanDark", "MineBig").into_bytes()),
        (CS, sch.into_bytes()),
        ("fonts/bignum.ttf", ttf),
    ]);
    let t = HudModel::load(&stock_files(), Platform::Linux).unwrap();
    let b = t
        .borrow_part(
            &common::hud_borrow::source_over(files),
            &stock_files(),
            &PartKey::new(ROOT),
            1,
        )
        .unwrap();
    assert!(b.assets().is_empty());
    assert!(
        b.left_behind()
            .contains(&LeftBehind::MissingFont("Big Numbers".into()))
    );
}

#[test]
fn a_protected_family_skips_only_that_font_and_a_full_rename_pool_refuses_the_borrow() {
    let ttf = sfnt_bytes(&[(1, "Segoe UI")], false);
    let sch = scheme(
        "",
        r#"MineBig { "1" { name "Segoe UI" tall 30 } }"#,
        r#"CustomFontFiles { "1" "resource/fonts/b.ttf" }"#,
    );
    let files = common::hud_borrow::files_over_stock_bytes(&[
        (HEALTH_RES, health("TanDark", "MineBig").into_bytes()),
        (CS, sch.into_bytes()),
        ("resource/fonts/b.ttf", ttf),
    ]);
    let t = HudModel::load(&stock_files(), Platform::Linux).unwrap();
    let b = t
        .borrow_part(
            &common::hud_borrow::source_over(files),
            &stock_files(),
            &PartKey::new(ROOT),
            1,
        )
        .unwrap();
    assert!(t.with_changes(b.changes()).model().assets().is_empty());
    assert!(
        b.left_behind()
            .contains(&LeftBehind::MissingFont("Segoe UI".into())),
        "the target scheme has no Segoe UI"
    );
    assert!(
        b.brings().contains(&Brings::Font("MineBig".into())),
        "only the file is skipped"
    );
    // 98 colours TanDark_2..TanDark_99 already taken in the target: the 99th clash has no name left.
    let taken: String = (2..=99)
        .map(|n| format!("TanDark_{n} \"1 1 1 255\" "))
        .collect();
    let target_cs = format!("\"Scheme\" {{ Colors {{ TanDark \"9 9 9 255\" {taken} }} }}");
    let tf = common::hud_stock::stock_files_with(&[(CS, Some(&target_cs))]);
    let t = HudModel::load(&tf, Platform::Linux).unwrap();
    let sch = scheme(r#"TanDark "1 2 3 255""#, "", "");
    let src = source(&[
        (HEALTH_RES, &health("TanDark", "HudClassHealth")),
        (CS, &sch),
    ]);
    assert_eq!(
        t.borrow_part(&src, &tf, &PartKey::new(ROOT), 1)
            .unwrap_err(),
        tf2tools_core::BorrowRefused::NoRoom
    );
}

#[test]
fn a_border_comes_with_its_nested_definition() {
    let sch = scheme(
        "",
        "",
        r#"Borders { MineBorder { inset "0 0 1 0" Left { color "1 1 1 255" offset "0 0" } } }"#,
    );
    let res =
        health("TanDark", "HudClassHealth").replace("labelText", "border MineBorder labelText");
    let (t, b) = borrow(&[(HEALTH_RES, &res), (CS, &sch)]);
    let m = t.with_changes(b.changes()).into_model();
    let doc = m.document(CS).unwrap();
    assert!(
        !doc.find(&["Scheme", "Borders", "MineBorder", "Left"])
            .is_empty()
    );
    assert!(b.brings().contains(&Brings::Border("MineBorder".into())));
}
