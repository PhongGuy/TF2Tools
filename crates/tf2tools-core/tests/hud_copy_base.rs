//! maker.json's base: the default HUD or a copy of a library HUD.

use std::collections::BTreeMap;

use tf2tools_core::{CopyBase, HudLoadError, MakerBase, MakerFile};

fn rays() -> CopyBase {
    CopyBase::new("h1", "rayshud")
        .with_author("raysfire")
        .with_origin("hud-db rayshud")
        .with_store_id("hud-db:rayshud")
        .with_repo("raysfire/rayshud")
        .with_hash("a22ede014e4f3b1260aa8fd657ff0757d9d006b8")
        .with_tree(&"ab".repeat(32))
        .with_base_folder("rayshud-a22ede014e4f3b1260aa8fd657ff0757d9d006b8")
        .with_base_updated_at("2026-07-01T10:00:00.000000Z")
        .with_install_folder(Some("rayshud"))
}

#[test]
fn a_copy_base_round_trips_and_the_default_is_unchanged() {
    let file = MakerFile::new(BTreeMap::new(), Vec::new()).with_base(MakerBase::Copy(rays()));
    let json: serde_json::Value = serde_json::from_slice(&file.to_json()).unwrap();
    assert_eq!(
        (json["version"].clone(), json["base"]["kind"].clone()),
        (1.into(), "copy".into())
    );
    let (back, dropped) = MakerFile::parse(&file.to_json()).unwrap();
    assert_eq!((back.base(), dropped), (&MakerBase::Copy(rays()), 0));
    let plain = MakerFile::new(BTreeMap::new(), Vec::new());
    let json: serde_json::Value = serde_json::from_slice(&plain.to_json()).unwrap();
    assert_eq!(json["base"], serde_json::json!({ "kind": "default" }));
}

#[test]
fn an_unsafe_install_folder_reads_as_none() {
    let bytes = br#"{"version":1,"base":{"kind":"copy","hud_id":"h1","name":"x","install_folder":"../evil"}}"#;
    let (file, _) = MakerFile::parse(bytes).unwrap();
    let MakerBase::Copy(base) = file.base() else {
        panic!("a copy base")
    };
    assert_eq!((base.install_folder(), base.author()), (None, ""));
}

#[test]
fn an_overlong_field_is_damaged_and_an_unknown_kind_is_newer() {
    let long = "n".repeat(201);
    let bytes =
        format!(r#"{{"version":1,"base":{{"kind":"copy","hud_id":"h1","name":"{long}"}}}}"#);
    assert!(matches!(
        MakerFile::parse(bytes.as_bytes()),
        Err(HudLoadError::Damaged { .. })
    ));
    assert!(matches!(
        MakerFile::parse(br#"{"version":1,"base":{"kind":"sparse"}}"#),
        Err(HudLoadError::Newer)
    ));
}
