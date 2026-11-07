//! Options: the change kind, the built-ins, replay, values and the sample killfeed rows.

use crate::common;

use std::collections::BTreeMap;

use common::hud_stock::{hudlayout_with_killfeed, stock_files_with};
use tf2tools_core::{
    Change, ChangeTarget, Derived, HudFiles, HudLoadError, HudModel, MakerFile, OptionChange,
    OptionKind, OptionSource, OptionValue, PartKey, Platform, RemoveKey, Rgba, SetKey,
};

fn option(group: u32, id: &str, value: OptionValue) -> Change {
    Change::option(group, OptionChange::new(OptionChange::BUILTIN, id, value))
}

/// The stock files without the chat and main menu files the options write.
fn without_option_files() -> HudFiles {
    stock_files_with(&[
        ("resource/ui/basechat.res", None),
        ("resource/ui/mainmenuoverride.res", None),
    ])
}

fn maker_with(change: &str) -> String {
    format!(r#"{{ "version": 1, "base": {{ "kind": "default" }}, "changes": [ {change} ] }}"#)
}

#[test]
fn an_option_change_round_trips_through_maker_json() {
    let changes = vec![
        option(1, "chat_top", OptionValue::Bool(true)),
        option(2, "killfeed_rows", OptionValue::Int(6)),
        option(3, "item_unique", OptionValue::Rgba([1, 2, 3, 255])),
    ];
    let file = MakerFile::new(BTreeMap::new(), changes.clone());
    let (back, dropped) = MakerFile::parse(&file.to_json()).unwrap();
    assert_eq!((back.changes(), dropped), (changes.as_slice(), 0));
}

#[test]
fn the_spec_example_option_entry_reads() {
    let text = maker_with(
        r#"{ "group": 2, "option": { "source": "builtin", "id": "killfeed_rows", "value": { "int": 6 } } }"#,
    );
    let (file, dropped) = MakerFile::parse(text.as_bytes()).unwrap();
    assert_eq!(dropped, 0);
    let ChangeTarget::Option(o) = file.changes()[0].target() else {
        panic!("not an option")
    };
    assert_eq!(
        (file.changes()[0].group(), o.source(), o.id(), o.value()),
        (2, "builtin", "killfeed_rows", &OptionValue::Int(6))
    );
}

#[test]
fn hostile_option_entries_never_panic() {
    let damaged = [
        r#"{ "source": "builtin", "id": "x" }"#,
        r#"{ "source": "builtin", "id": "x", "value": 6 }"#,
        r#"{ "source": "builtin", "id": "x", "value": { "rgba": [1, 2, 3] } }"#,
        r#"{ "source": "builtin", "id": "x", "value": { "rgba": [1, 2, 3, 300] } }"#,
        r#"{ "source": "builtin", "id": "x", "value": { "int": 1e99 } }"#,
        r#"{ "source": "builtin", "id": "x", "value": { "int": 4294967296 } }"#,
        r#"{ "source": "builtin", "id": "x", "value": { "bool": "yes" } }"#,
        r#"{ "source": "builtin", "id": 7, "value": { "int": 1 } }"#,
        r#"{ "id": "x", "value": { "int": 1 } }"#,
        r#"[]"#,
    ];
    for body in damaged {
        let text = maker_with(&format!(r#"{{ "group": 1, "option": {body} }}"#));
        assert!(
            matches!(
                MakerFile::parse(text.as_bytes()),
                Err(HudLoadError::Damaged { .. })
            ),
            "{body}"
        );
    }
    // A value kind from a later stage is dropped and counted, like a path the rule refuses.
    let later = maker_with(
        r#"{ "group": 1, "option": { "source": "builtin", "id": "x", "value": { "index": 2 } } }"#,
    );
    let (file, dropped) = MakerFile::parse(later.as_bytes()).unwrap();
    assert_eq!((file.changes().len(), dropped), (0, 1));
}

const SCHEME: &str = r#""Scheme" { Colors { TanDark "117 107 94 255" TanLight "235 226 202 255"
  QualityColorNormal "178 178 178 255" QualityColorNormal_GreyedOut "44 44 44 255"
  QualityColorUnique "255 215 0 255" DimmQualityColorUnique "255 215 0 100" } }"#;
const BASECHAT: &str = "\"Resource/UI/BaseChat.res\"\n{\n\tHudChat\n\t{\n\t\tControlName EditablePanel\n\t\txpos 10\n\t\typos 275\n\t}\n}\n";
const MAINMENU: &str = "\"Resource/UI/MainMenuOverride.res\"\n{\n\tFriendsContainer\n\t{\n\t\tControlName EditablePanel\n\t\tvisible 1\n\t}\n}\n";
const ITEM_IDS: [&str; 16] = [
    "item_normal",
    "item_unique",
    "item_strange",
    "item_vintage",
    "item_haunted",
    "item_genuine",
    "item_collectors",
    "item_unusual",
    "item_community",
    "item_valve",
    "item_civilian",
    "item_freelance",
    "item_mercenary",
    "item_commando",
    "item_assassin",
    "item_elite",
];

fn hudlayout() -> String {
    hudlayout_with_killfeed(
        "  HudDeathNotice { fieldName HudDeathNotice xpos r640 ypos 18 wide 628 tall 468 MaxDeathNotices 4 }\n",
    )
}

/// The stock files with the item colours, the killfeed and both option files.
fn option_files_with(basechat: &str) -> HudFiles {
    let layout = hudlayout();
    stock_files_with(&[
        ("resource/clientscheme.res", Some(SCHEME)),
        ("resource/ui/basechat.res", Some(basechat)),
        ("resource/ui/mainmenuoverride.res", Some(MAINMENU)),
        ("scripts/hudlayout.res", Some(layout.as_str())),
    ])
}

fn model() -> HudModel {
    HudModel::load(&option_files_with(BASECHAT), Platform::Linux).unwrap()
}

fn ids(m: &HudModel) -> Vec<&str> {
    m.options().iter().map(|o| o.id()).collect()
}

#[test]
fn the_default_hud_offers_every_builtin() {
    let m = model();
    let mut want = vec!["killfeed_rows", "chat_top", "hide_friends"];
    want.extend(ITEM_IDS);
    assert_eq!(ids(&m), want);
    let groups: Vec<&str> = m.options().iter().map(|o| o.group()).collect();
    assert_eq!(&groups[..4], ["HUD", "HUD", "Main menu", "Item colours"]);
}

#[test]
fn builtin_specs_follow_the_spec_table() {
    let m = model();
    let get = |id: &str| *m.options().iter().find(|o| o.id() == id).unwrap();
    let rows = get("killfeed_rows");
    assert_eq!(rows.label(), "Killfeed rows");
    assert!(matches!(
        rows.kind(),
        OptionKind::Number {
            min: 0,
            max: 20,
            step: 1
        }
    ));
    assert_eq!(rows.default(), &OptionValue::Int(4));
    let chat = get("chat_top");
    assert!(matches!(chat.kind(), OptionKind::Toggle));
    assert_eq!(chat.tooltip(), None);
    assert!(!chat.restart());
    assert!(get("hide_friends").restart());
    let unique = get("item_unique");
    assert_eq!((unique.label(), unique.restart()), ("Unique", true));
    assert_eq!(unique.default(), &OptionValue::Rgba([255, 215, 0, 255]));
    let OptionKind::Colour { derived } = unique.kind() else {
        panic!("not a colour")
    };
    assert_eq!(derived, &[Derived::Dimm, Derived::GreyedOut]);
    assert_eq!(unique.source(), &OptionSource::BuiltIn);
    assert_eq!(get("item_collectors").label(), "Collector's");
    // The killfeed rows and the chat are drawn on the canvas; every other option shows only in TF2.
    let drawn: Vec<&str> = (m.options().iter())
        .filter(|o| o.drawn())
        .map(|o| o.id())
        .collect();
    assert_eq!(drawn, ["killfeed_rows", "chat_top"]);
}

#[test]
fn an_option_whose_file_or_block_is_missing_is_not_offered() {
    // The stock files without their killfeed block have Colors but no killfeed and no option files.
    let layout = hudlayout_with_killfeed("");
    let files = stock_files_with(&[
        ("scripts/hudlayout.res", Some(layout.as_str())),
        ("resource/ui/basechat.res", None),
        ("resource/ui/mainmenuoverride.res", None),
    ]);
    let m = HudModel::load(&files, Platform::Linux).unwrap();
    assert_eq!(ids(&m), ITEM_IDS);
    // A basechat.res without a HudChat block hides only Chat at the top.
    let no_chat = "\"Resource/UI/BaseChat.res\" { ChatInputLine { xpos 3 } }";
    let m = HudModel::load(&option_files_with(no_chat), Platform::Linux).unwrap();
    assert!(!ids(&m).contains(&"chat_top"));
    assert!(ids(&m).contains(&"hide_friends"));
}

#[test]
fn option_files_load_as_documents() {
    // basechat.res is a HUD scene file now (hud_chat.rs covers it missing); the main menu's
    // file still loads as a document the options write.
    let m = model();
    assert!(m.document("resource/ui/basechat.res").is_some());
    assert!(m.document("resource/ui/mainmenuoverride.res").is_some());
}

fn kf_value(m: &HudModel) -> Option<String> {
    m.part(&PartKey::new("HudDeathNotice"))?
        .value("MaxDeathNotices")
        .map(|l| l.text().to_owned())
}

#[test]
fn killfeed_rows_writes_max_death_notices() {
    let m = model();
    let e = m.with_changes(&[option(1, "killfeed_rows", OptionValue::Int(6))]);
    assert_eq!(kf_value(e.model()).as_deref(), Some("6"));
    assert_eq!((e.applied().len(), e.dropped()), (1, 0));
    assert_eq!(e.touched(), ["scripts/hudlayout.res"]);
}

#[test]
fn an_option_turned_off_leaves_pristine_bytes() {
    let m = model();
    let on = m.with_changes(&[option(1, "chat_top", OptionValue::Bool(true))]);
    let chat = on.model().document("resource/ui/basechat.res").unwrap();
    let ypos = chat.find(&["Resource/UI/BaseChat.res", "HudChat", "ypos"]);
    assert_eq!(
        ypos.iter().map(|e| e.text()).collect::<Vec<_>>(),
        [Some("0")]
    );
    for id in ["chat_top", "hide_friends"] {
        let off = m.with_changes(&[option(1, id, OptionValue::Bool(false))]);
        assert_eq!(
            (off.applied().len(), off.dropped(), off.touched().len()),
            (1, 0, 0),
            "{id}"
        );
    }
    let off = m.with_changes(&[option(1, "chat_top", OptionValue::Bool(false))]);
    let chat = off.model().document("resource/ui/basechat.res").unwrap();
    assert_eq!(chat.as_bytes(), BASECHAT.as_bytes());
}

#[test]
fn an_item_colour_writes_its_derived_colours_only_where_defined() {
    let m = model();
    let e = m.with_changes(&[
        option(1, "item_normal", OptionValue::Rgba([178, 18, 6, 255])),
        option(2, "item_unique", OptionValue::Rgba([10, 20, 30, 255])),
    ]);
    let s = e.model().scheme();
    assert_eq!(
        s.named_colour("QualityColorNormal"),
        Some(Rgba::new(178, 18, 6, 255))
    );
    // 44.5, 4.5 and 1.5 round half to even.
    assert_eq!(
        s.named_colour("QualityColorNormal_GreyedOut"),
        Some(Rgba::new(44, 4, 2, 255))
    );
    assert_eq!(s.named_colour("DimmQualityColorNormal"), None);
    assert_eq!(
        s.named_colour("DimmQualityColorUnique"),
        Some(Rgba::new(10, 20, 30, 100))
    );
    assert_eq!(s.named_colour("QualityColorUnique_GreyedOut"), None);
    assert_eq!(e.dropped(), 0);
}

#[test]
fn a_canvas_edit_to_the_same_key_wins_over_the_option() {
    let m = model();
    let canvas = Change::set(
        1,
        SetKey::new(
            "scripts/hudlayout.res",
            &["Resource/HudLayout.res", "HudDeathNotice"],
            "MaxDeathNotices",
            None,
            Some("4"),
            "9",
        ),
    );
    let changes = vec![canvas, option(2, "killfeed_rows", OptionValue::Int(6))];
    let e = m.with_changes(&changes);
    assert_eq!(kf_value(e.model()).as_deref(), Some("9"));
    assert_eq!(e.applied(), changes.as_slice());
}

fn max_death_notices(edit: &str) -> SetKey {
    SetKey::new(
        "scripts/hudlayout.res",
        &["Resource/HudLayout.res", "HudDeathNotice"],
        "MaxDeathNotices",
        None,
        Some("4"),
        edit,
    )
}

#[test]
fn a_canvas_edit_after_the_option_still_wins() {
    let m = model();
    let changes = vec![
        option(1, "killfeed_rows", OptionValue::Int(6)),
        Change::set(2, max_death_notices("9")),
    ];
    let e = m.with_changes(&changes);
    assert_eq!(kf_value(e.model()).as_deref(), Some("9"));
    assert_eq!(e.applied(), changes.as_slice());
}

#[test]
fn a_canvas_remove_of_the_key_wins_over_the_option() {
    let m = model();
    let remove = Change::remove(
        2,
        RemoveKey::new(
            "scripts/hudlayout.res",
            &["Resource/HudLayout.res", "HudDeathNotice"],
            "MaxDeathNotices",
            None,
            None,
        ),
    );
    let changes = vec![option(1, "killfeed_rows", OptionValue::Int(6)), remove];
    let e = m.with_changes(&changes);
    assert_eq!(kf_value(e.model()), None);
    assert_eq!((e.applied(), e.dropped()), (changes.as_slice(), 0));
}

#[test]
fn a_later_choice_for_the_same_option_wins() {
    let m = model();
    let e = m.with_changes(&[
        option(1, "killfeed_rows", OptionValue::Int(6)),
        option(2, "killfeed_rows", OptionValue::Int(8)),
    ]);
    assert_eq!(kf_value(e.model()).as_deref(), Some("8"));
}

#[test]
fn bad_option_choices_drop_and_count() {
    let m = model();
    let bad = [
        option(1, "killfeed_rows", OptionValue::Int(21)),
        option(2, "killfeed_rows", OptionValue::Int(-1)),
        option(3, "killfeed_rows", OptionValue::Bool(true)),
        option(4, "chat_top", OptionValue::Rgba([1, 2, 3, 4])),
        option(5, "nope", OptionValue::Bool(true)),
        Change::option(
            6,
            OptionChange::new("hud_schema", "killfeed_rows", OptionValue::Int(6)),
        ),
        option(7, "killfeed_rows", OptionValue::Int(5)),
    ];
    let e = m.with_changes(&bad);
    assert_eq!((e.applied().len(), e.dropped()), (1, 6));
    assert_eq!(kf_value(e.model()).as_deref(), Some("5"));
    // An option the HUD cannot take (no basechat.res) drops too.
    let plain = HudModel::load(&without_option_files(), Platform::Linux).unwrap();
    let e = plain.with_changes(&[option(1, "chat_top", OptionValue::Bool(true))]);
    assert_eq!((e.applied().len(), e.dropped()), (0, 1));
}

use tf2tools_core::EditRefused;

#[test]
fn values_come_from_the_hud_when_nothing_is_chosen() {
    let m = model();
    assert_eq!(
        m.option_value("killfeed_rows", &[]),
        Some(OptionValue::Int(4))
    );
    assert_eq!(
        m.option_value("chat_top", &[]),
        Some(OptionValue::Bool(false))
    );
    assert_eq!(
        m.option_value("hide_friends", &[]),
        Some(OptionValue::Bool(false))
    );
    assert_eq!(
        m.option_value("item_normal", &[]),
        Some(OptionValue::Rgba([178, 178, 178, 255]))
    );
    assert_eq!(m.option_value("nope", &[]), None);
    // A HUD whose chat already sits at the top reads as on.
    let top = BASECHAT.replace("ypos 275", "ypos 0");
    let m = HudModel::load(&option_files_with(&top), Platform::Linux).unwrap();
    assert_eq!(
        m.option_value("chat_top", &[]),
        Some(OptionValue::Bool(true))
    );
}

#[test]
fn a_stored_choice_wins() {
    let m = model();
    let changes = [option(1, "killfeed_rows", OptionValue::Int(6))];
    assert_eq!(
        m.option_value("killfeed_rows", &changes),
        Some(OptionValue::Int(6))
    );
}

#[test]
fn a_refused_later_choice_leaves_the_earlier_one_in_force() {
    let m = model();
    let changes = [
        option(1, "killfeed_rows", OptionValue::Int(6)),
        option(2, "killfeed_rows", OptionValue::Int(99)),
    ];
    assert_eq!(
        kf_value(m.with_changes(&changes).model()).as_deref(),
        Some("6")
    );
    assert_eq!(
        m.option_value("killfeed_rows", &changes),
        Some(OptionValue::Int(6))
    );
}

#[test]
fn a_later_switch_off_that_writes_nothing_leaves_on_in_force() {
    let m = model();
    let changes = [
        option(1, "chat_top", OptionValue::Bool(true)),
        option(2, "chat_top", OptionValue::Bool(false)),
    ];
    assert_eq!(
        m.option_value("chat_top", &changes),
        Some(OptionValue::Bool(true))
    );
}

#[test]
fn setting_an_option_replaces_its_earlier_choice() {
    let m = model();
    let hide = m.hide_part(&PartKey::new("HudWeaponAmmo"), 1).unwrap();
    let mut changes = vec![option(0, "killfeed_rows", OptionValue::Int(6))];
    changes.extend(hide.clone());
    let next = m
        .set_option("killfeed_rows", OptionValue::Int(7), &changes, 2)
        .unwrap();
    let mut want = hide;
    want.push(option(2, "killfeed_rows", OptionValue::Int(7)));
    assert_eq!(next, want);
}

#[test]
fn choosing_the_hud_value_is_a_reset() {
    let m = model();
    let changes = vec![option(0, "killfeed_rows", OptionValue::Int(6))];
    let next = m
        .set_option("killfeed_rows", OptionValue::Int(4), &changes, 1)
        .unwrap();
    assert!(next.is_empty());
}

#[test]
fn set_option_refuses_what_it_cannot_write() {
    let m = model();
    let set = |id: &str, v: OptionValue| m.set_option(id, v, &[], 1);
    assert_eq!(
        set("nope", OptionValue::Bool(true)),
        Err(EditRefused::NotOffered)
    );
    assert_eq!(
        set("killfeed_rows", OptionValue::Int(25)),
        Err(EditRefused::BadValue)
    );
    assert_eq!(
        set("killfeed_rows", OptionValue::Bool(true)),
        Err(EditRefused::BadValue)
    );
    assert_eq!(
        set("chat_top", OptionValue::Rgba([0, 0, 0, 0])),
        Err(EditRefused::BadValue)
    );
    let plain = HudModel::load(&without_option_files(), Platform::Linux).unwrap();
    assert_eq!(
        plain.set_option("chat_top", OptionValue::Bool(true), &[], 1),
        Err(EditRefused::NotOffered)
    );
}

#[test]
fn reset_drops_one_group_of_options() {
    let m = model();
    let changes = vec![
        option(0, "item_normal", OptionValue::Rgba([1, 2, 3, 255])),
        option(1, "killfeed_rows", OptionValue::Int(6)),
        option(2, "item_elite", OptionValue::Rgba([4, 5, 6, 255])),
    ];
    assert_eq!(
        m.reset_option_group("Item colours", &changes),
        [changes[1].clone()]
    );
    assert_eq!(m.reset_option_group("Main menu", &changes), changes);
}

#[test]
fn reset_all_drops_every_option_choice_and_keeps_other_edits() {
    let m = model();
    let edit = Change::set(
        3,
        SetKey::new(
            "scripts/hudlayout.res",
            &["HudLayout", "HudDeathNotice"],
            "xpos",
            None,
            None,
            "5",
        ),
    );
    let changes = vec![
        option(0, "item_normal", OptionValue::Rgba([1, 2, 3, 255])),
        option(1, "killfeed_rows", OptionValue::Int(6)),
        edit.clone(),
    ];
    assert_eq!(m.reset_all_options(&changes), [edit]);
}

#[test]
fn an_item_colour_from_a_base_scheme_file_reads_as_the_hud_value() {
    let scheme =
        "#base \"clientscheme_colors.res\"\n\"Scheme\" { Colors { TanDark \"117 107 94 255\" } }";
    let colours = r#""Scheme" { Colors { QualityColorUnique "1 2 3 255" } }"#;
    let layout = hudlayout();
    let files = stock_files_with(&[
        ("resource/clientscheme.res", Some(scheme)),
        ("resource/clientscheme_colors.res", Some(colours)),
        ("resource/ui/basechat.res", Some(BASECHAT)),
        ("resource/ui/mainmenuoverride.res", Some(MAINMENU)),
        ("scripts/hudlayout.res", Some(layout.as_str())),
    ]);
    let m = HudModel::load(&files, Platform::Linux).unwrap();
    assert_eq!(
        m.option_value("item_unique", &[]),
        Some(OptionValue::Rgba([1, 2, 3, 255]))
    );
    // The stock colour differs from the HUD's own, so choosing it is kept.
    let stock = OptionValue::Rgba([255, 215, 0, 255]);
    let next = m.set_option("item_unique", stock.clone(), &[], 1).unwrap();
    assert_eq!(next, [option(1, "item_unique", stock)]);
}

#[test]
fn a_switch_that_cannot_turn_off_reads_as_the_hud_value() {
    let top = BASECHAT.replace("ypos 275", "ypos 0");
    let m = HudModel::load(&option_files_with(&top), Platform::Linux).unwrap();
    let off = [option(1, "chat_top", OptionValue::Bool(false))];
    assert_eq!(
        m.option_value("chat_top", &off),
        Some(OptionValue::Bool(true))
    );
    assert!(!m.option_can_turn_off("chat_top"));
    assert!(model().option_can_turn_off("chat_top"));
    assert!(!model().option_can_turn_off("killfeed_rows"));
    assert!(!model().option_can_turn_off("nope"));
}

#[test]
fn the_hud_value_comes_from_the_block_an_option_writes() {
    // With duplicate blocks the option writes into the last one, so it is the one read.
    let twice = BASECHAT.replace("\n}\n", "\n\tHudChat\n\t{\n\t\typos 0\n\t}\n}\n");
    let m = HudModel::load(&option_files_with(&twice), Platform::Linux).unwrap();
    assert_eq!(
        m.option_value("chat_top", &[]),
        Some(OptionValue::Bool(true))
    );
}
