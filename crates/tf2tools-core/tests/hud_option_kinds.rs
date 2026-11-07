//! The HUD's own option choices: stored as `hud_option`, picked by key, left to the switch layer.

mod common;

use std::collections::BTreeMap;

use common::hud_stock::stock_files;
use tf2tools_core::{
    Change, HudLoadError, HudModel, MakerFile, OptionChange, OptionValue, Platform,
};

fn hud(group: u32, id: &str, value: OptionValue) -> Change {
    Change::option(group, OptionChange::new(OptionChange::HUD, id, value))
}

fn maker_with(change: &str) -> String {
    format!(r#"{{ "version": 1, "base": {{ "kind": "default" }}, "changes": [ {change} ] }}"#)
}

#[test]
fn hud_and_editor_choices_round_trip_under_their_own_kind() {
    let changes = vec![
        hud(
            1,
            "f:customizations/damage.res:0",
            OptionValue::Pick("resource/ui/damage_big.res".into()),
        ),
        Change::option(
            2,
            OptionChange::new(
                OptionChange::SCHEMA,
                "e:bh_player_boxes",
                OptionValue::Bool(true),
            ),
        ),
    ];
    let file = MakerFile::new(BTreeMap::new(), changes.clone());
    let json: serde_json::Value = serde_json::from_slice(&file.to_json()).unwrap();
    let entries = json["changes"].as_array().unwrap();
    assert!(
        entries
            .iter()
            .all(|e| e.get("hud_option").is_some() && e.get("option").is_none())
    );
    assert_eq!(
        entries[0]["hud_option"]["value"]["pick"],
        "resource/ui/damage_big.res"
    );
    let (back, dropped) = MakerFile::parse(&file.to_json()).unwrap();
    assert_eq!((back.changes(), dropped), (changes.as_slice(), 0));
}

#[test]
fn a_builtin_choice_is_still_written_as_option() {
    let change = Change::option(
        1,
        OptionChange::new(OptionChange::BUILTIN, "chat_top", OptionValue::Bool(true)),
    );
    let json: serde_json::Value =
        serde_json::from_slice(&MakerFile::new(BTreeMap::new(), vec![change]).to_json()).unwrap();
    assert!(json["changes"][0].get("option").is_some());
    assert!(json["changes"][0].get("hud_option").is_none());
}

#[test]
fn hostile_hud_option_entries_are_damaged() {
    let long = "a".repeat(4097);
    let bodies = [
        r#"{ "source": "builtin", "id": "chat_top", "value": { "bool": true } }"#.to_owned(),
        r#"{ "source": "hud", "id": "c:x", "value": { "pick": 7 } }"#.to_owned(),
        format!(r#"{{ "source": "hud", "id": "c:x", "value": {{ "pick": "{long}" }} }}"#),
        r#"{ "source": "editor", "value": { "bool": true } }"#.to_owned(),
    ];
    for body in bodies {
        let text = maker_with(&format!(r#"{{ "group": 1, "hud_option": {body} }}"#));
        assert!(
            matches!(
                MakerFile::parse(text.as_bytes()),
                Err(HudLoadError::Damaged { .. })
            ),
            "{body}"
        );
    }
}

#[test]
fn an_unknown_kind_key_still_reads_as_newer() {
    let text = maker_with(r#"{ "group": 1, "hud_option_v2": { "id": "x" } }"#);
    assert!(matches!(
        MakerFile::parse(text.as_bytes()),
        Err(HudLoadError::Newer)
    ));
}

#[test]
fn the_model_leaves_hud_choices_to_the_switch_layer() {
    let model = HudModel::load(&stock_files(), Platform::Linux).unwrap();
    let choices = [
        hud(1, "c:customizations/streamermode_", OptionValue::Bool(true)),
        Change::option(
            2,
            OptionChange::new(OptionChange::SCHEMA, "e:x", OptionValue::Pick("1".into())),
        ),
    ];
    let edited = model.with_changes(&choices);
    assert_eq!((edited.dropped(), edited.applied().len()), (0, 2));
}
