//! In a copy, the model reads each file's #base chain as the game merges it.

mod common;

use common::hud_stock::{AMMO, HEALTH, stock_files_with};
use tf2tools_core::{HudFiles, HudModel, PartKey, Platform};

const HEALTH_PATH: &str = "resource/ui/hudplayerhealth.res";
const BASE_PATH: &str = "resource/ui/health_base.res";
const HV: &str = "HudPlayerStatus/HudPlayerHealth/PlayerStatusHealthValue";

fn health_over(base: &str) -> HudFiles {
    let own = format!(
        "#base \"health_base.res\"\n{}",
        HEALTH.replace("    fgcolor TanDark\n", "")
    );
    stock_files_with(&[(HEALTH_PATH, Some(&own)), (BASE_PATH, Some(base))])
}

fn copy_model(files: HudFiles) -> HudModel {
    HudModel::load(&files.with_install_folder("myhud"), Platform::Linux).unwrap()
}

#[test]
fn a_key_only_a_base_sets_resolves_from_the_base_and_own_keys_win() {
    let m = copy_model(health_over(
        r#""Base" { PlayerStatusHealthValue { fgcolor "1 2 3 255" xpos 99 } }"#,
    ));
    let part = m.part(&PartKey::new(HV)).unwrap();
    assert!(part.is_readable());
    let colour = part.value("fgcolor").unwrap();
    assert_eq!((colour.text(), colour.file()), ("1 2 3 255", BASE_PATH));
    let xpos = part.value("xpos").unwrap();
    assert_eq!((xpos.text(), xpos.file()), ("76", HEALTH_PATH));
    assert_eq!(part.chain_lines("xpos").len(), 2);
    assert_eq!(part.own_block().0, HEALTH_PATH);
    assert_eq!(m.base_files(), [BASE_PATH]);
    assert!(m.missing_bases().is_empty());
}

#[test]
fn an_earlier_base_beats_a_later_one() {
    let own = format!("#base \"first.res\"\n#base \"second.res\"\n{HEALTH}");
    let files = stock_files_with(&[
        (HEALTH_PATH, Some(&own)),
        (
            "resource/ui/first.res",
            Some(r#""A" { PlayerStatusHealthValue { allcaps 1 } }"#),
        ),
        (
            "resource/ui/second.res",
            Some(r#""B" { PlayerStatusHealthValue { allcaps 0 wrap 1 } }"#),
        ),
    ]);
    let m = copy_model(files);
    let part = m.part(&PartKey::new(HV)).unwrap();
    assert_eq!(
        part.value("allcaps").unwrap().file(),
        "resource/ui/first.res"
    );
    assert_eq!(part.value("allcaps").unwrap().text(), "1");
    assert_eq!(part.value("wrap").unwrap().file(), "resource/ui/second.res");
}

#[test]
fn a_control_only_a_base_defines_is_a_part() {
    let m = copy_model(health_over(
        r#""Base" { BaseOnlyLabel { ControlName CExLabel xpos 1 ypos 1 wide 10 tall 10 } }"#,
    ));
    let part = m
        .part(&PartKey::new(
            "HudPlayerStatus/HudPlayerHealth/BaseOnlyLabel",
        ))
        .expect("a part");
    assert!(part.is_readable());
    assert_eq!(part.own_block().0, HEALTH_PATH);
    assert_eq!(part.own_block().1[0], "Resource/UI/HudPlayerHealth.res");
}

#[test]
fn a_block_a_shared_base_injects_is_not_a_part() {
    let shared = r#""S" { StrayPanel { ControlName EditablePanel xpos 0 ypos 0 wide 5 tall 5 } }"#;
    let health = format!("#base \"shared.res\"\n{HEALTH}");
    let ammo = format!("#base \"shared.res\"\n{AMMO}");
    let m = copy_model(stock_files_with(&[
        (HEALTH_PATH, Some(&health)),
        ("resource/ui/hudammoweapons.res", Some(&ammo)),
        ("resource/ui/shared.res", Some(shared)),
    ]));
    assert!(
        !m.parts()
            .iter()
            .any(|p| p.key().as_str().ends_with("/StrayPanel"))
    );
}

#[test]
fn a_missing_base_in_a_copy_is_named() {
    let own = format!("#base \"gone.res\"\n{HEALTH}");
    let m = copy_model(stock_files_with(&[(HEALTH_PATH, Some(&own))]));
    assert_eq!(m.missing_bases(), ["custom/myhud/resource/ui/gone.res"]);
}

#[test]
fn a_stock_model_ignores_bases_as_before() {
    let m = HudModel::load(
        &health_over(r#""Base" { PlayerStatusHealthValue { fgcolor "1 2 3 255" } }"#),
        Platform::Linux,
    )
    .unwrap();
    assert!(m.base_files().is_empty());
    assert!(
        m.part(&PartKey::new(HV))
            .unwrap()
            .value("fgcolor")
            .is_none()
    );
}

#[test]
fn a_nested_base_ranks_after_its_includer_and_before_the_next_base() {
    let own = format!("#base \"a.res\"\n#base \"b.res\"\n{HEALTH}");
    let files = stock_files_with(&[
        (HEALTH_PATH, Some(&own)),
        (
            "resource/ui/a.res",
            Some("#base \"a_base.res\"\n\"A\" { PlayerStatusHealthValue { allcaps 1 } }"),
        ),
        (
            "resource/ui/a_base.res",
            Some(r#""AB" { PlayerStatusHealthValue { allcaps 2 wrap 2 xpos 2 } }"#),
        ),
        (
            "resource/ui/b.res",
            Some(r#""B" { PlayerStatusHealthValue { allcaps 3 wrap 3 centerwrap 3 xpos 3 } }"#),
        ),
    ]);
    let m = copy_model(files);
    let part = m.part(&PartKey::new(HV)).unwrap();
    let from = |key: &str| {
        let line = part.value(key).unwrap();
        (line.text(), line.file())
    };
    assert_eq!(from("xpos"), ("76", HEALTH_PATH));
    assert_eq!(from("allcaps"), ("1", "resource/ui/a.res"));
    assert_eq!(from("wrap"), ("2", "resource/ui/a_base.res"));
    assert_eq!(from("centerwrap"), ("3", "resource/ui/b.res"));
}
