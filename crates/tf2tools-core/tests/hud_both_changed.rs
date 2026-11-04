//! Which kept changes the base update also touched, for every change kind.

mod common;

use common::hud_anims::{ANIMS, ANIMS_PATH, anim_files};
use common::hud_stock::{HEALTH, stock_files_with};
use tf2tools_core::{
    AddBlock, AnimationAppend, AnimationChange, AssetFile, BlockLine, Change, CreditNote, HudFiles,
    HudModel, OptionChange, OptionValue, Platform, RemoveKey, SetKey, both_changed,
};

const H: &str = "resource/ui/hudplayerhealth.res";
const ROOT: &str = "Resource/UI/HudPlayerHealth.res";
const HV: &str = "PlayerStatusHealthValue";

fn files(health: &str) -> HudFiles {
    stock_files_with(&[(H, Some(health))])
}

fn notes(old: &HudFiles, new: &HudFiles, applied: &[Change]) -> Vec<(String, Option<String>)> {
    let (om, nm) = (
        HudModel::load(old, Platform::Linux).unwrap(),
        HudModel::load(new, Platform::Linux).unwrap(),
    );
    both_changed(old, &om, new, &nm, applied)
        .iter()
        .map(|n| (n.part().to_owned(), n.key().map(str::to_owned)))
        .collect()
}

fn set(block: &str, key: &str, cond: Option<&str>, old: Option<&str>) -> Change {
    Change::set(1, SetKey::new(H, &[ROOT, block], key, cond, old, "9"))
}

#[test]
fn a_set_is_listed_only_when_the_base_changed_that_key() {
    let new = files(&HEALTH.replace("fgcolor TanDark", "fgcolor TanLight"));
    let got = notes(
        &files(HEALTH),
        &new,
        &[
            set(HV, "fgcolor", None, Some("TanDark")),
            set(HV, "xpos", None, Some("76")),
        ],
    );
    assert_eq!(got, [(HV.to_owned(), Some("fgcolor".to_owned()))]);
}

#[test]
fn a_conditional_set_follows_its_own_condition() {
    let win = set("HudPlayerHealth", "xpos", Some("$WIN32"), Some("0"));
    let x360 = files(&HEALTH.replace("xpos 32 [$X360]", "xpos 40 [$X360]"));
    assert!(notes(&files(HEALTH), &x360, std::slice::from_ref(&win)).is_empty());
    let win32 = files(&HEALTH.replace("xpos 0 [$WIN32]", "xpos 4 [$WIN32]"));
    assert_eq!(notes(&files(HEALTH), &win32, &[win]).len(), 1);
}

#[test]
fn an_insert_is_listed_when_the_base_added_that_key() {
    let new = files(&HEALTH.replace(
        "    labelText %Health%\n",
        "    labelText %Health%\n    allcaps 1\n",
    ));
    assert_eq!(
        notes(&files(HEALTH), &new, &[set(HV, "allcaps", None, None)]).len(),
        1
    );
    assert!(
        notes(
            &files(HEALTH),
            &files(HEALTH),
            &[set(HV, "allcaps", None, None)]
        )
        .is_empty()
    );
}

#[test]
fn a_remove_is_listed_when_the_base_changed_or_removed_that_key() {
    let remove = Change::remove(1, RemoveKey::new(H, &[ROOT, HV], "zpos", None, Some("5")));
    for new in [
        HEALTH.replace("    zpos 5\n", "    zpos 6\n"),
        HEALTH.replace("    zpos 5\n", ""),
    ] {
        assert_eq!(
            notes(&files(HEALTH), &files(&new), std::slice::from_ref(&remove)).len(),
            1
        );
    }
    assert!(notes(&files(HEALTH), &files(HEALTH), &[remove]).is_empty());
}

#[test]
fn an_added_block_is_listed_when_the_base_added_one_of_that_name() {
    let add = Change::add_block(
        1,
        AddBlock::new(
            H,
            &[ROOT],
            "MyLabel",
            None,
            vec![BlockLine::new("xpos", "1", None)],
        ),
    );
    let new = files(&HEALTH.replace(
        "  HealthBonusSettings",
        "  MyLabel { ControlName CExLabel }\n  HealthBonusSettings",
    ));
    assert_eq!(
        notes(&files(HEALTH), &new, &[add]),
        [("MyLabel".to_owned(), None)]
    );
}

#[test]
fn an_asset_is_listed_when_the_base_added_removed_or_changed_that_file() {
    let path = "materials/vgui/replay/thumbnails/tf2tools/x.vtf";
    let asset = Change::asset(
        1,
        AssetFile::new(
            path,
            ".tf2tools/borrowed/materials/vgui/replay/thumbnails/tf2tools/x.vtf",
        ),
    );
    let with = |text: Option<&str>| stock_files_with(&[(path, text)]);
    let listed = [("x.vtf".to_owned(), None)];
    assert_eq!(
        notes(&with(None), &with(Some("b")), std::slice::from_ref(&asset)),
        listed
    );
    assert_eq!(
        notes(&with(Some("a")), &with(None), std::slice::from_ref(&asset)),
        listed
    );
    assert_eq!(
        notes(
            &with(Some("a")),
            &with(Some("b")),
            std::slice::from_ref(&asset)
        ),
        listed
    );
    assert!(notes(&with(Some("a")), &with(Some("a")), &[asset]).is_empty());
}

#[test]
fn an_animation_or_append_is_listed_when_the_base_changed_that_event() {
    let changed = ANIMS.replace(
        "\"255 0 0 255\" Linear 0.0 0.1",
        "\"0 255 0 255\" Linear 0.0 0.2",
    );
    let (old, new) = (anim_files(&[]), anim_files(&[(ANIMS_PATH, Some(&changed))]));
    let whole = Change::animation(
        1,
        AnimationChange::new(
            "HudLowAmmoPulse",
            "Animate AmmoInClip Alpha \"0\" Linear 0.0 0.1",
        ),
    );
    let append = Change::animation_append(
        2,
        AnimationAppend::new(
            "HudLowAmmoPulse",
            "Animate AmmoInClip Alpha \"9\" Linear 0.0 0.1",
        ),
    );
    let other = Change::animation(
        3,
        AnimationChange::new(
            "HudMedicCharged",
            "Animate ChargeLabel Alpha \"0\" Linear 0.0 0.1",
        ),
    );
    let event = ("HudLowAmmoPulse".to_owned(), None);
    assert_eq!(
        notes(&old, &new, &[whole, append, other]),
        [event.clone(), event]
    );
}

#[test]
fn options_and_credits_are_never_listed() {
    let option = Change::option(
        1,
        OptionChange::new("builtin", "killfeed_rows", OptionValue::Int(6)),
    );
    let credit = Change::credit(
        2,
        CreditNote::new("rayshud", "raysfire", "hud-db rayshud", "Health"),
    );
    let new = files(&HEALTH.replace("fgcolor TanDark", "fgcolor TanLight"));
    assert!(notes(&files(HEALTH), &new, &[option, credit]).is_empty());
}

const BASE: &str = "resource/ui/health_base.res";

/// The health file as a copy's stub over `health_base.res`, which holds `base`.
fn over_base(base: &str) -> HudFiles {
    let own = format!(
        "#base \"health_base.res\"\n{}",
        HEALTH.replace("    fgcolor TanDark\n", "")
    );
    stock_files_with(&[(H, Some(&own)), (BASE, Some(base))]).with_install_folder("myhud")
}

#[test]
fn a_change_the_update_makes_in_a_base_file_is_listed() {
    let old = over_base(r#""Base" { PlayerStatusHealthValue { fgcolor "1 2 3 255" } }"#);
    let new = over_base(r#""Base" { PlayerStatusHealthValue { fgcolor "4 5 6 255" } }"#);
    let same = over_base(r#""Base" { PlayerStatusHealthValue { fgcolor "1 2 3 255" } }"#);
    let change = set(HV, "fgcolor", None, Some("1 2 3 255"));
    assert_eq!(
        notes(&old, &new, std::slice::from_ref(&change)),
        [(HV.to_owned(), Some("fgcolor".to_owned()))]
    );
    assert!(notes(&old, &same, &[change]).is_empty());
}

#[test]
fn a_block_the_update_adds_in_a_base_file_is_listed() {
    let add = Change::add_block(
        1,
        AddBlock::new(
            H,
            &[ROOT],
            "MyLabel",
            None,
            vec![BlockLine::new("xpos", "1", None)],
        ),
    );
    let old = over_base(r#""Base" { PlayerStatusHealthValue { fgcolor "1 2 3 255" } }"#);
    let new = over_base(
        r#""Base" { PlayerStatusHealthValue { fgcolor "1 2 3 255" } MyLabel { ControlName CExLabel } }"#,
    );
    assert_eq!(notes(&old, &new, &[add]), [("MyLabel".to_owned(), None)]);
}
