//! The Stage 2 change kinds and their maker.json round trip.

use std::collections::BTreeMap;
use tf2tools_core::{
    AddBlock, AssetFile, BlockLine, Change, ChangeTarget, HudLoadError, MakerFile, RemoveKey,
    SetKey,
};

fn with_change(change: &str) -> String {
    format!(
        r#"{{ "version": 1, "base": {{ "kind": "default" }}, "sources": {{}}, "changes": [ {change} ] }}"#
    )
}

fn every_kind() -> Vec<Change> {
    vec![
        Change::set(
            1,
            SetKey::new(
                "resource/ui/a.res",
                &["R", "P"],
                "xpos",
                None,
                Some("1"),
                "2",
            ),
        ),
        Change::remove(
            2,
            RemoveKey::new(
                "resource/ui/a.res",
                &["R", "P"],
                "xpos",
                Some("$WIN32"),
                Some("0"),
            ),
        ),
        Change::add_block(
            3,
            AddBlock::new(
                "resource/clientscheme.res",
                &["Scheme", "Fonts"],
                "TF2Tools_1",
                None,
                vec![
                    BlockLine::new("name", "TF2", None),
                    BlockLine::new("tall", "12", Some("$LINUX")),
                ],
            ),
        ),
        Change::asset(
            4,
            AssetFile::new("resource/fonts/tf2tools/a.ttf", ".tf2tools/fonts/a.ttf"),
        ),
    ]
}

#[test]
fn every_kind_round_trips() {
    let mut sources = BTreeMap::new();
    sources.insert("resource/ui/a.res".to_owned(), "ab12".to_owned());
    let file = MakerFile::new(sources, every_kind());
    let (back, dropped) = MakerFile::parse(&file.to_json()).unwrap();
    assert_eq!(dropped, 0);
    assert_eq!(back, file);
}

#[test]
fn the_json_uses_the_documented_names() {
    let json: serde_json::Value =
        serde_json::from_slice(&MakerFile::new(BTreeMap::new(), every_kind()).to_json()).unwrap();
    let changes = &json["changes"];
    assert_eq!(changes[1]["remove"]["cond"], "$WIN32");
    assert_eq!(
        changes[2]["add_block"]["parent"],
        serde_json::json!(["Scheme", "Fonts"])
    );
    assert_eq!(changes[2]["add_block"]["lines"][1]["cond"], "$LINUX");
    assert!(changes[2]["add_block"]["cond"].is_null());
    assert_eq!(changes[3]["asset"]["source"], ".tf2tools/fonts/a.ttf");
}

#[test]
fn accessors_read_back() {
    let kinds = every_kind();
    let ChangeTarget::Remove(r) = kinds[1].target() else {
        panic!("not a remove")
    };
    assert_eq!(
        (r.file(), r.key(), r.cond(), r.old()),
        ("resource/ui/a.res", "xpos", Some("$WIN32"), Some("0"))
    );
    assert_eq!(r.block(), ["R", "P"]);
    let ChangeTarget::AddBlock(a) = kinds[2].target() else {
        panic!("not an add")
    };
    assert_eq!(
        (a.name(), a.cond(), a.lines().len()),
        ("TF2Tools_1", None, 2)
    );
    assert_eq!(
        (
            a.lines()[1].key(),
            a.lines()[1].value(),
            a.lines()[1].cond()
        ),
        ("tall", "12", Some("$LINUX"))
    );
    let ChangeTarget::Asset(f) = kinds[3].target() else {
        panic!("not an asset")
    };
    assert_eq!(f.path(), "resource/fonts/tf2tools/a.ttf");
}

#[test]
fn paths_outside_the_hud_drop_and_count() {
    let text = with_change(
        r#"{ "group": 1, "remove": { "file": "cfg/a.res", "block": ["R"], "key": "k" } },
           { "group": 2, "add_block": { "file": "../a.res", "parent": ["R"], "name": "B", "lines": [] } },
           { "group": 3, "asset": { "path": "resource/fonts/tf2tools/a.ttf", "source": "fonts/evil.exe" } },
           { "group": 4, "asset": { "path": "cfg/autoexec.cfg", "source": ".tf2tools/fonts/a.ttf" } },
           { "group": 5, "remove": { "file": "resource/ui/a.res", "block": ["R"], "key": "k" } }"#,
    );
    let (file, dropped) = MakerFile::parse(text.as_bytes()).unwrap();
    assert_eq!(dropped, 4);
    assert_eq!(file.changes().len(), 1);
    assert_eq!(file.changes()[0].group(), 5);
}

#[test]
fn malformed_new_kinds_are_damaged() {
    let damaged = [
        r#"{ "group": 1, "remove": { "file": "a.res", "block": [], "key": "k" } }"#,
        r#"{ "group": 1, "remove": { "file": "a.res", "block": ["R"], "key": 5 } }"#,
        r#"{ "group": 1, "add_block": { "file": "a.res", "parent": [], "name": "B", "lines": [] } }"#,
        r#"{ "group": 1, "add_block": { "file": "a.res", "parent": ["R"], "name": "", "lines": [] } }"#,
        r#"{ "group": 1, "add_block": { "file": "a.res", "parent": ["R"], "name": "B", "lines": [ { "key": "k" } ] } }"#,
        r#"{ "group": 1, "add_block": { "file": "a.res", "parent": ["R"], "name": "B", "lines": "no" } }"#,
        r#"{ "group": 1, "asset": { "path": "a.ttf" } }"#,
    ];
    for change in damaged {
        let text = with_change(change);
        assert!(
            matches!(
                MakerFile::parse(text.as_bytes()),
                Err(HudLoadError::Damaged { .. })
            ),
            "{change}"
        );
    }
}

#[test]
fn a_malformed_change_names_its_kind() {
    let text = with_change(r#"{ "group": 1, "asset": { "path": "a.ttf" } }"#);
    let Err(HudLoadError::Damaged { reason }) = MakerFile::parse(text.as_bytes()) else {
        panic!("damaged");
    };
    assert_eq!(reason, "the asset is malformed");
}
