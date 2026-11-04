//! maker.json: its shape, what is newer, what is damaged, and the path rule on load.

use std::collections::BTreeMap;
use tf2tools_core::{Change, ChangeTarget, HudLoadError, MakerFile, SetKey};

const EXAMPLE: &str = r#"{ "version": 1, "base": { "kind": "default" },
  "sources": { "resource/ui/hudplayerhealth.res": "ab12" },
  "changes": [ { "group": 1, "set": { "file": "resource/ui/hudplayerhealth.res",
      "block": ["Resource/UI/HudPlayerHealth.res", "PlayerStatusHealthValue"],
      "key": "fgcolor", "cond": null, "old": "TanDark", "new": "255 80 80 255" } } ] }"#;

fn with_change(change: &str) -> String {
    format!(
        r#"{{ "version": 1, "base": {{ "kind": "default" }}, "sources": {{}}, "changes": [ {change} ] }}"#
    )
}

#[test]
fn the_spec_example_reads() {
    let (file, dropped) = MakerFile::parse(EXAMPLE.as_bytes()).unwrap();
    assert_eq!(dropped, 0);
    assert_eq!(
        file.sources()
            .get("resource/ui/hudplayerhealth.res")
            .map(String::as_str),
        Some("ab12")
    );
    let ChangeTarget::Set(set) = file.changes()[0].target() else {
        panic!("not a set")
    };
    assert_eq!(
        (file.changes()[0].group(), set.key(), set.old(), set.value()),
        (1, "fgcolor", Some("TanDark"), "255 80 80 255")
    );
    assert_eq!(
        set.block(),
        ["Resource/UI/HudPlayerHealth.res", "PlayerStatusHealthValue"]
    );
}

#[test]
fn written_files_read_back_equal() {
    let changes = vec![
        Change::set(
            1,
            SetKey::new(
                "scripts/hudlayout.res",
                &["Resource/HudLayout.res", "HudWeaponAmmo"],
                "xpos",
                Some("$WIN32"),
                Some("r95"),
                "r75",
            ),
        ),
        Change::set(
            2,
            SetKey::new(
                "resource/ui/hudplayerhealth.res",
                &["R", "B"],
                "visible",
                None,
                None,
                "0",
            ),
        ),
    ];
    let file = MakerFile::new(
        BTreeMap::from([("scripts/hudlayout.res".to_owned(), "00ff".to_owned())]),
        changes,
    );
    let json = file.to_json();
    assert_eq!(MakerFile::parse(&json).unwrap(), (file, 0));
    let value: serde_json::Value = serde_json::from_slice(&json).unwrap();
    assert_eq!(
        (value["version"].as_u64(), value["base"]["kind"].as_str()),
        (Some(1), Some("default"))
    );
    assert!(value["changes"][1]["set"]["cond"].is_null());
}

#[test]
fn newer_files_are_refused() {
    let newer = [
        EXAMPLE.replace("\"version\": 1", "\"version\": 2"),
        EXAMPLE.replace("\"kind\": \"default\"", "\"kind\": \"sparse\""),
        with_change(r#"{ "group": 2, "set_prepend": { "file": "a" } }"#),
        with_change(r#"{ "group": 2, "x-not-a-kind": {} }"#),
    ];
    for text in newer {
        assert!(
            matches!(MakerFile::parse(text.as_bytes()), Err(HudLoadError::Newer)),
            "{text}"
        );
    }
}

#[test]
fn an_option_entry_is_no_longer_newer() {
    let text = with_change(
        r#"{ "group": 2, "option": { "source": "builtin", "id": "killfeed_rows", "value": { "int": 6 } } }"#,
    );
    let (file, dropped) = MakerFile::parse(text.as_bytes()).unwrap();
    assert_eq!((file.changes().len(), dropped), (1, 0));
}

#[test]
fn damaged_files_are_refused() {
    let damaged = [
        "{".to_owned(),
        "[]".to_owned(),
        EXAMPLE.replace("\"version\": 1,", ""),
        with_change(r#"{ "group": "one", "set": {} }"#),
        with_change(r#"{ "group": 1, "set": { "file": "a.res" } }"#),
        format!("{EXAMPLE}{}", " ".repeat(8 * 1024 * 1024)),
    ];
    for text in damaged {
        assert!(
            matches!(
                MakerFile::parse(text.as_bytes()),
                Err(HudLoadError::Damaged { .. })
            ),
            "{}",
            &text[..text.len().min(80)]
        );
    }
}

#[test]
fn paths_outside_the_hud_are_dropped_and_counted() {
    let text = EXAMPLE
        .replace(
            "\"file\": \"resource/ui/hudplayerhealth.res\"",
            "\"file\": \"../../cfg/autoexec.cfg\"",
        )
        .replace(
            "\"resource/ui/hudplayerhealth.res\": \"ab12\"",
            "\"/etc/passwd\": \"ab12\"",
        );
    let (file, dropped) = MakerFile::parse(text.as_bytes()).unwrap();
    assert_eq!(
        (dropped, file.changes().len(), file.sources().len()),
        (1, 0, 0)
    );
}
