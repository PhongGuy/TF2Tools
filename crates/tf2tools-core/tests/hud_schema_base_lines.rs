//! Schema #base swaps in every form, and no schema operation reaching `.tf2tools/`.

mod common;

use common::hud_switches::switch_files;
use tf2tools_core::{HudSchema, OptionAction, OptionSpec, Platform, SwitchAction};

const FILE: &str = "customizations/damage.res";

fn spec<'a>(specs: &'a [OptionSpec], id: &str) -> &'a OptionSpec {
    specs
        .iter()
        .find(|s| s.id() == id)
        .unwrap_or_else(|| panic!("no {id}"))
}
fn base_lines(on: &[&str], off: &[&str]) -> [OptionAction; 1] {
    let owned = |v: &[&str]| v.iter().map(|s| (*s).to_owned()).collect();
    [OptionAction::Switch(SwitchAction::BaseLines {
        file: FILE.into(),
        on: owned(on),
        off: owned(off),
    })]
}

#[test]
fn every_base_form_becomes_base_lines() {
    let text = r##"{ "Controls": { "A": [
      { "Name": "b1", "Label": "B1", "Type": "CheckBox", "Files": { "customizations/damage.res": { "#base": [ { "true": "../resource/ui/damage_big.res", "false": "../resource/ui/damage_small.res" } ] } } },
      { "Name": "b2", "Label": "B2", "Type": "CheckBox", "Files": { "customizations/damage.res": { "#base": "../resource/ui/damage_big.res" } } },
      { "Name": "w1", "Label": "W1", "Type": "CheckBox", "WriteFile": { "FileName": "customizations/damage.res", "TrueText": "#base \"../resource/ui/damage_big.res\"", "FalseText": "#base \"../resource/ui/damage_small.res\"" } },
      { "Name": "w2", "Label": "W2", "Type": "CheckBox", "WriteFile": { "FileName": "customizations/damage.res", "TrueText": "#base ../resource/ui/damage_big.res", "FalseText": "#base ../resource/ui/damage_small.res" } },
      { "Name": "w3", "Label": "W3", "Type": "CheckBox", "WriteFile": { "FileName": "customizations/damage.res", "TrueText": "#base ../../../custom/somehud/resource/ui/damage_big.res", "FalseText": "" } },
      { "Name": "w4", "Label": "W4", "Type": "CheckBox", "WriteFile": { "FileName": "customizations/damage.res", "TrueText": "anything else", "FalseText": "" } } ] } }"##;
    let got = HudSchema::parse("x", text.as_bytes())
        .unwrap()
        .options(&switch_files(&[]), Platform::Linux);
    let swap = base_lines(
        &["../resource/ui/damage_big.res"],
        &["../resource/ui/damage_small.res"],
    );
    assert_eq!(spec(got.specs(), "e:b1").actions(), swap);
    assert_eq!(spec(got.specs(), "e:w1").actions(), swap);
    assert_eq!(spec(got.specs(), "e:w2").actions(), swap);
    // A plain string turns its line on; off keeps the file's own #base lines.
    assert_eq!(
        spec(got.specs(), "e:b2").actions(),
        base_lines(
            &["../resource/ui/damage_big.res"],
            &["../resource/ui/damage_medium.res"]
        )
    );
    assert_eq!(
        spec(got.specs(), "e:w3").actions(),
        base_lines(&["../../../custom/somehud/resource/ui/damage_big.res"], &[])
    );
    // Any other text writes the whole file.
    assert_eq!(
        spec(got.specs(), "e:w4").actions(),
        [OptionAction::Switch(SwitchAction::WriteText {
            file: FILE.into(),
            on: "anything else".into(),
            off: Some(String::new()),
        })]
    );
    assert_eq!(got.left_out(), 0);
}

#[test]
fn a_schema_never_reaches_the_metadata_folder() {
    let text = r##"{ "CustomizationsFolder": "#customization", "EnabledFolder": ".TF2Tools", "Controls": { "A": [
      { "Name": "m1", "Label": "M1", "Type": "CheckBox", "Files": { ".tf2tools/maker.json": { "version": { "true": "9", "false": "1" } } } },
      { "Name": "m2", "Label": "M2", "Type": "CheckBox", "RenameFile": { "OldName": "customizations/streamermode_/", "NewName": ".tf2tools/" } },
      { "Name": "m3", "Label": "M3", "Type": "CheckBox", "RenameFile": { "OldName": ".tf2tools/maker.json", "NewName": "customizations/x.res" } },
      { "Name": "m4", "Label": "M4", "Type": "CheckBox", "FileName": "bh_player_largehealth.res" },
      { "Name": "m5", "Label": "M5", "Type": "CheckBox", "WriteFile": { "FileName": "customizations/damage.res", "TrueText": "#base \"../.tf2tools/maker.json\"", "FalseText": "" } },
      { "Name": "m6", "Label": "M6", "Type": "CheckBox", "Files": { "customizations/../.tf2tools/maker.res": { "R": { "a": { "true": "1", "false": "0" } } } } },
      { "Name": "m7", "Label": "M7", "Type": "CheckBox", "RenameFile": { "OldName": "customizations\\streamermode_\\", "NewName": "customizations\\..\\.TF2Tools\\" } } ] } }"##;
    let got = HudSchema::parse("x", text.as_bytes())
        .unwrap()
        .options(&switch_files(&[]), Platform::Linux);
    assert!(got.specs().is_empty());
    // Every one fails the path rule, so all seven are left out; none is a misfit.
    assert_eq!((got.left_out(), got.misfits()), (7, 0));
}
