//! Schema controls as options: kinds, actions, fit against the copy, refusals.

mod common;

use common::hud_schema::schema;
use common::hud_switches::switch_files;
use tf2tools_core::{
    Derived, OptionAction, OptionKind, OptionSource, OptionSpec, OptionValue, Platform,
    SwitchAction, ValueMap,
};

fn spec<'a>(specs: &'a [OptionSpec], id: &str) -> &'a OptionSpec {
    specs
        .iter()
        .find(|s| s.id() == id)
        .unwrap_or_else(|| panic!("no {id}"))
}

#[test]
fn each_supported_type_becomes_its_option_kind() {
    let got = schema().options(&switch_files(&[]), Platform::Linux);
    let ids: Vec<&str> = got.specs().iter().map(|s| s.id()).collect();
    assert_eq!(
        ids,
        [
            "e:sh_tan",
            "e:sh_chat_top",
            "e:sh_rows",
            "e:sh_big",
            "e:sh_style"
        ]
    );
    let tan = spec(got.specs(), "e:sh_tan");
    assert_eq!(
        tan.kind(),
        &OptionKind::Colour {
            derived: vec![Derived::Pulse]
        }
    );
    assert_eq!((tan.group(), tan.restart()), ("Colours", true));
    assert_eq!(
        tan.source(),
        &OptionSource::HudSchema {
            schema: "somehud".into()
        }
    );
    assert_eq!(
        spec(got.specs(), "e:sh_rows").kind(),
        &OptionKind::Number {
            min: 10,
            max: 30,
            step: 2
        }
    );
    let OptionAction::SetKey {
        file,
        block,
        key,
        value,
        ..
    } = &spec(got.specs(), "e:sh_chat_top").actions()[0]
    else {
        panic!("not a key")
    };
    assert_eq!(
        (file.as_str(), block.as_slice(), key.as_str()),
        (
            "resource/ui/basechat.res",
            &["HudChat".to_owned()][..],
            "ypos"
        )
    );
    assert_eq!(
        value,
        &ValueMap::OnOff {
            on: "0".into(),
            off: Some("275".into())
        }
    );
}

#[test]
fn file_name_and_rename_become_moves() {
    let got = schema().options(&switch_files(&[]), Platform::Linux);
    assert_eq!(
        spec(got.specs(), "e:sh_big").actions(),
        [OptionAction::Switch(SwitchAction::Move {
            from: "#customization/bh_player_largehealth.res".into(),
            to: "#customization/_enabled/bh_player_largehealth.res".into(),
        })]
    );
    let OptionKind::Choice(choices) = spec(got.specs(), "e:sh_style").kind() else {
        panic!("not a choice")
    };
    let keys: Vec<(&str, &str)> = choices.iter().map(|c| (c.key(), c.label())).collect();
    assert_eq!(keys, [("0", "Plain"), ("1", "Streamer")]);
    assert_eq!(
        choices[1].actions(),
        [OptionAction::Switch(SwitchAction::Move {
            from: "customizations/streamermode_".into(),
            to: "customizations/streamermode".into(),
        })]
    );
    assert_eq!(
        spec(got.specs(), "e:sh_style").default(),
        &OptionValue::Pick("0".into())
    );
}

#[test]
fn refused_controls_are_counted_and_misfits_hidden() {
    let got = schema().options(&switch_files(&[]), Platform::Linux);
    assert_eq!((got.left_out(), got.misfits()), (6, 2));
    assert!(
        got.specs()
            .iter()
            .all(|s| !["e:sh_gone", "e:sh_noblock", "e:sh_cfg"].contains(&s.id()))
    );
}

#[test]
fn files_keys_keep_their_order() {
    let text = r##"{ "Controls": { "A": [ { "Name": "k", "Label": "K", "Type": "CheckBox",
      "Files": { "resource/ui/basechat.res": { "HudChat": { "ypos": { "true": "0", "false": "275" }, "xpos": { "true": "0", "false": "10" } } } } } ] } }"##;
    let got = tf2tools_core::HudSchema::parse("x", text.as_bytes())
        .unwrap()
        .options(&switch_files(&[]), Platform::Linux);
    let keys: Vec<&str> = spec(got.specs(), "e:k")
        .actions()
        .iter()
        .filter_map(|a| match a {
            OptionAction::SetKey { key, .. } => Some(key.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(keys, ["ypos", "xpos"]);
}

#[test]
fn combo_literals_map_per_choice() {
    let text = r##"{ "Controls": { "A": [ { "Name": "c", "Label": "C", "Type": "ComboBox", "Value": "0", "Options": [
      { "Label": "Low", "Value": "0", "Files": { "resource/ui/basechat.res": { "HudChat": { "ypos": "275" } } } },
      { "Label": "Top", "Value": "1", "Files": { "resource/ui/basechat.res": { "HudChat": { "ypos": "0" } } } },
      { "Label": "Mid", "Value": "2", "Files": { "resource/ui/basechat.res": { "HudChat": { "ypos": "100" } } } } ] } ] } }"##;
    let got = tf2tools_core::HudSchema::parse("x", text.as_bytes())
        .unwrap()
        .options(&switch_files(&[]), Platform::Linux);
    let OptionAction::SetKey {
        file, key, value, ..
    } = &spec(got.specs(), "e:c").actions()[0]
    else {
        panic!("not a key")
    };
    assert_eq!(
        (file.as_str(), key.as_str()),
        ("resource/ui/basechat.res", "ypos")
    );
    let pairs = [("0", "275"), ("1", "0"), ("2", "100")]
        .map(|(k, v)| (k.to_owned(), v.to_owned()))
        .to_vec();
    assert_eq!(value, &ValueMap::PerChoice(pairs));
}

#[test]
fn type_names_match_in_any_case() {
    let text = r##"{ "Controls": { "A": [
      { "Name": "a", "Label": "A", "Type": "checkbox", "Files": { "resource/ui/basechat.res": { "HudChat": { "ypos": { "true": "0", "false": "275" } } } } },
      { "Name": "b", "Label": "B", "Type": "COLOURPICKER", "Files": { "resource/clientscheme.res": { "Scheme": { "Colors": { "TanDark": "$value" } } } } } ] } }"##;
    let got = tf2tools_core::HudSchema::parse("x", text.as_bytes())
        .unwrap()
        .options(&switch_files(&[]), Platform::Linux);
    let kinds: Vec<(&str, bool)> = got
        .specs()
        .iter()
        .map(|s| (s.id(), matches!(s.kind(), OptionKind::Colour { .. })))
        .collect();
    assert_eq!(kinds, [("e:a", false), ("e:b", true)]);
}

#[test]
fn a_colour_with_both_flags_derives_only_its_pulse_as_the_editor_does() {
    let text = r##"{ "Controls": { "A": [
      { "Name": "both", "Label": "Both", "Type": "ColorPicker", "Value": "1 2 3 255", "Shadow": true, "Pulse": true,
        "Files": { "resource/clientscheme.res": { "Scheme": { "Colors": { "TanLight": "$value" } } } } },
      { "Name": "shadow", "Label": "Shadow", "Type": "ColorPicker", "Value": "1 2 3 255", "Shadow": true,
        "Files": { "resource/clientscheme.res": { "Scheme": { "Colors": { "TanDark": "$value" } } } } } ] } }"##;
    let got = tf2tools_core::HudSchema::parse("x", text.as_bytes())
        .unwrap()
        .options(&switch_files(&[]), Platform::Linux);
    assert_eq!(
        spec(got.specs(), "e:both").kind(),
        &OptionKind::Colour {
            derived: vec![Derived::Pulse]
        }
    );
    assert_eq!(
        spec(got.specs(), "e:shadow").kind(),
        &OptionKind::Colour {
            derived: vec![Derived::Shadow]
        }
    );
}

#[test]
fn a_combo_with_control_level_files_is_left_out() {
    let text = r##"{ "Controls": { "A": [ { "Name": "c", "Label": "C", "Type": "ComboBox", "Value": "0",
      "Files": { "resource/ui/basechat.res": { "HudChat": { "ypos": "$value" } } },
      "Options": [ { "Label": "Low", "Value": "0" }, { "Label": "Top", "Value": "1" } ] } ] } }"##;
    let got = tf2tools_core::HudSchema::parse("x", text.as_bytes())
        .unwrap()
        .options(&switch_files(&[]), Platform::Linux);
    assert!(got.specs().is_empty());
    assert_eq!((got.left_out(), got.misfits()), (1, 0));
}

#[test]
fn a_combo_whose_choices_all_write_nothing_is_left_out() {
    let text = r##"{ "Controls": { "A": [ { "Name": "c", "Label": "C", "Type": "ComboBox", "Value": "0",
      "Options": [ { "Label": "Low", "Value": "0" }, { "Label": "Top", "Value": "1" } ] } ] } }"##;
    let got = tf2tools_core::HudSchema::parse("x", text.as_bytes())
        .unwrap()
        .options(&switch_files(&[]), Platform::Linux);
    assert!(got.specs().is_empty());
    assert_eq!((got.left_out(), got.misfits()), (1, 0));
}

#[test]
fn a_path_climbing_out_of_the_copy_is_refused_by_the_path_rule() {
    // Each leaf and move is a shape the compiler accepts; only the climbing path refuses it.
    let text = r##"{ "Controls": { "A": [
      { "Name": "f", "Label": "F", "Type": "CheckBox",
        "Files": { "../../resource/ui/basechat.res": { "HudChat": { "ypos": { "true": "0", "false": "275" } } } } },
      { "Name": "m", "Label": "M", "Type": "CheckBox",
        "RenameFile": { "OldName": "customizations/streamermode_/", "NewName": "../../streamermode/" } } ] } }"##;
    let got = tf2tools_core::HudSchema::parse("x", text.as_bytes())
        .unwrap()
        .options(&switch_files(&[]), Platform::Linux);
    assert!(
        got.specs().is_empty(),
        "{:?}",
        got.specs().iter().map(|s| s.id()).collect::<Vec<_>>()
    );
    assert_eq!((got.left_out(), got.misfits()), (2, 0));
}
