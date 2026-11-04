//! Stage 2 settings: replaying removes, added blocks and assets; every resolved setting;
//! text edits; Stage 2 rows.

mod common;

use common::hud_stock::stock_files;
use tf2tools_core::{
    AddBlock, AssetFile, BlockLine, Change, ChangeTarget, HudModel, PartKey, Platform, RemoveKey,
    SetKey,
};

const HP: &str = "resource/ui/hudplayerhealth.res";
const HP_ROOT: &str = "Resource/UI/HudPlayerHealth.res";
const HEALTH: &str = "HudPlayerStatus/HudPlayerHealth";

fn model() -> HudModel {
    HudModel::load(&stock_files(), Platform::Linux).unwrap()
}

fn k(text: &str) -> PartKey {
    PartKey::new(text)
}

fn remove(group: u32, block: &[&str], key: &str, cond: Option<&str>) -> Change {
    Change::remove(group, RemoveKey::new(HP, block, key, cond, None))
}

fn hp_text(m: &HudModel) -> String {
    String::from_utf8(m.document(HP).unwrap().as_bytes().to_vec()).unwrap()
}

#[test]
fn a_remove_deletes_every_line_of_the_key() {
    let edited = model().with_changes(&[remove(1, &[HP_ROOT, "HudPlayerHealth"], "xpos", None)]);
    assert_eq!((edited.applied().len(), edited.dropped()), (1, 0));
    assert_eq!(edited.touched(), [HP]);
    let m = edited.model();
    assert!(m.part(&k(HEALTH)).unwrap().value("xpos").is_none());
    let text = hp_text(m);
    assert!(!text.contains("xpos 0 [$WIN32]") && !text.contains("xpos 32 [$X360]"));
    assert!(text.contains("xpos_minmode -5"), "another key stays");
}

#[test]
fn a_cond_narrows_a_remove_to_its_line() {
    let edited = model().with_changes(&[remove(
        1,
        &[HP_ROOT, "HudPlayerHealth"],
        "xpos",
        Some("$x360"),
    )]);
    let m = edited.model();
    assert_eq!(
        m.part(&k(HEALTH)).unwrap().value("xpos").map(|l| l.text()),
        Some("0")
    );
    assert!(!hp_text(m).contains("xpos 32"));
}

#[test]
fn a_remove_of_an_absent_key_applies_and_changes_nothing() {
    let edited =
        model().with_changes(&[remove(1, &[HP_ROOT, "HudPlayerHealth"], "labelText", None)]);
    assert_eq!((edited.applied().len(), edited.dropped()), (1, 0));
    assert!(edited.touched().is_empty());
    assert_eq!(hp_text(edited.model()), hp_text(&model()));
}

#[test]
fn removes_into_missing_blocks_or_refused_paths_drop() {
    let edited = model().with_changes(&[
        remove(1, &[HP_ROOT, "NoSuchBlock"], "xpos", None),
        Change::remove(2, RemoveKey::new("cfg/a.res", &["R"], "k", None, None)),
    ]);
    assert_eq!((edited.applied().len(), edited.dropped()), (0, 2));
}

#[test]
fn an_added_block_becomes_a_part_a_later_set_can_edit() {
    let add = AddBlock::new(
        HP,
        &[HP_ROOT],
        "TF2ToolsLabel",
        None,
        vec![
            BlockLine::new("ControlName", "CExLabel", None),
            BlockLine::new("xpos", "10", None),
            BlockLine::new("wide", "40", None),
            BlockLine::new("tall", "10", None),
        ],
    );
    let set = SetKey::new(
        HP,
        &[HP_ROOT, "TF2ToolsLabel"],
        "fgcolor",
        None,
        None,
        "TanLight",
    );
    let edited = model().with_changes(&[Change::add_block(1, add), Change::set(2, set)]);
    assert_eq!((edited.applied().len(), edited.dropped()), (2, 0));
    let part = edited
        .model()
        .part(&k("HudPlayerStatus/HudPlayerHealth/TF2ToolsLabel"))
        .expect("the new block is a part");
    assert_eq!(part.class(), Some("CExLabel"));
    assert_eq!(part.value("fgcolor").map(|l| l.text()), Some("TanLight"));
}

#[test]
fn an_add_block_of_an_existing_name_drops() {
    let add = AddBlock::new(HP, &[HP_ROOT], "playerstatushealthvalue", None, Vec::new());
    let missing = AddBlock::new(HP, &[HP_ROOT, "Nope"], "B", None, Vec::new());
    let edited = model().with_changes(&[Change::add_block(1, add), Change::add_block(2, missing)]);
    assert_eq!((edited.applied().len(), edited.dropped()), (0, 2));
    assert!(edited.touched().is_empty());
}

#[test]
fn assets_apply_by_path_and_refuse_outside_sources() {
    let font = "resource/fonts/tf2tools/a.ttf";
    let edited = model().with_changes(&[
        Change::asset(1, AssetFile::new(font, ".tf2tools/fonts/a.ttf")),
        Change::asset(2, AssetFile::new(font, ".tf2tools/fonts/a-2.ttf")),
        Change::asset(
            3,
            AssetFile::new("resource/fonts/tf2tools/b.ttf", "fonts/b.ttf"),
        ),
        Change::asset(4, AssetFile::new("cfg/b.ttf", ".tf2tools/fonts/b.ttf")),
    ]);
    assert_eq!((edited.applied().len(), edited.dropped()), (2, 2));
    assert!(edited.touched().is_empty());
    assert_eq!(
        edited.model().assets(),
        [(font.to_owned(), ".tf2tools/fonts/a-2.ttf".to_owned())]
    );
    assert!(model().assets().is_empty());
}

#[test]
fn reset_drops_removes_and_ignores_other_kinds() {
    let m = model();
    let changes = vec![
        remove(1, &[HP_ROOT, "HudPlayerHealth"], "xpos", None),
        Change::asset(
            2,
            AssetFile::new("resource/fonts/tf2tools/a.ttf", ".tf2tools/fonts/a.ttf"),
        ),
    ];
    let kept = m.reset_property(&k(HEALTH), &["xpos", "ypos"], &changes);
    assert_eq!(kept.len(), 1);
    assert!(matches!(kept[0].target(), ChangeTarget::Asset(_)));
    assert!(m.hide_groups(&k(HEALTH), &changes).is_empty());
}

#[test]
fn reset_undoes_a_set_later_removed_from_an_earlier_block() {
    let block = ["Resource/HudLayout.res", "HudItemEffectMeter"];
    let file = "scripts/hudlayout.res";
    let changes = vec![
        Change::set(1, SetKey::new(file, &block, "wide", None, None, "77")),
        Change::remove(2, RemoveKey::new(file, &block, "wide", None, None)),
    ];
    let edited = model().with_changes(&changes).into_model();
    assert!(
        edited
            .reset_property(&k("HudItemEffectMeter"), &["wide"], &changes)
            .is_empty()
    );
}

#[test]
fn reset_undoes_a_remove_from_an_earlier_block_on_the_edited_model() {
    let m = model();
    let changes = vec![Change::remove(
        1,
        RemoveKey::new(
            "scripts/hudlayout.res",
            &["Resource/HudLayout.res", "HudItemEffectMeter"],
            "wide",
            None,
            None,
        ),
    )];
    let edited = m.with_changes(&changes).into_model();
    let meter = k("HudItemEffectMeter");
    assert!(edited.part(&meter).unwrap().value("wide").is_none());
    assert_eq!(
        edited.part(&meter).unwrap().blocks(),
        [
            (
                "scripts/hudlayout.res".to_owned(),
                vec![
                    "Resource/HudLayout.res".to_owned(),
                    "HudItemEffectMeter".to_owned()
                ]
            ),
            (
                "resource/ui/huditemeffectmeter.res".to_owned(),
                vec![
                    "Resource/UI/HudItemEffectMeter.res".to_owned(),
                    "HudItemEffectMeter".to_owned()
                ]
            ),
        ]
    );
    assert!(
        edited
            .reset_property(&meter, &["wide", "tall"], &changes)
            .is_empty()
    );
    assert_eq!(
        edited.reset_property(&meter, &["xpos"], &changes),
        changes,
        "another key's remove stays"
    );
}

use tf2tools_core::EditRefused;

const METER: &str = "HudItemEffectMeter";
const HV: &str = "HudPlayerStatus/HudPlayerHealth/PlayerStatusHealthValue";

fn origin(line: &tf2tools_core::KeyLine) -> (String, String, String, u32) {
    (
        line.key().to_owned(),
        line.text().to_owned(),
        line.file().to_owned(),
        line.line(),
    )
}

#[test]
fn settings_list_each_winning_line_with_its_origin() {
    let m = model();
    let rows: Vec<_> = m
        .part(&k(METER))
        .unwrap()
        .settings()
        .into_iter()
        .map(origin)
        .collect();
    let row = |key: &str, text: &str, file: &str, line: u32| {
        (key.to_owned(), text.to_owned(), file.to_owned(), line)
    };
    assert_eq!(
        rows,
        [
            row("tall", "50", "scripts/hudlayout.res", 9),
            row("wide", "100", "scripts/hudlayout.res", 9),
            row("xpos", "r174", "resource/ui/huditemeffectmeter.res", 1),
            row("ypos", "r62", "resource/ui/huditemeffectmeter.res", 1),
        ]
    );
}

#[test]
fn chain_lines_cover_every_block_in_load_order() {
    let m = model();
    let part = m.part(&k(METER)).unwrap();
    let texts: Vec<&str> = part.chain_lines("XPOS").iter().map(|l| l.text()).collect();
    assert_eq!(texts, ["r162", "r174"]);
    assert_eq!(
        part.lines("xpos").len(),
        1,
        "lines stays the winning block's"
    );
    let timer = m
        .part(&k("HudMatchStatus/ObjectiveStatusTimePanel"))
        .unwrap();
    let wide: Vec<&str> = timer.chain_lines("wide").iter().map(|l| l.text()).collect();
    assert_eq!(wide, ["110", "130"], "an if_match line counts");
}

#[test]
fn remove_key_clears_every_block_of_the_chain() {
    let m = model();
    let changes = m.remove_key(&k(METER), "xpos", 7).unwrap();
    let blocks: Vec<(String, Vec<String>, Option<String>)> = changes
        .iter()
        .map(|c| {
            assert_eq!(c.group(), 7);
            let ChangeTarget::Remove(r) = c.target() else {
                panic!("not a remove")
            };
            assert_eq!((r.key(), r.cond()), ("xpos", None));
            (
                r.file().to_owned(),
                r.block().to_vec(),
                r.old().map(str::to_owned),
            )
        })
        .collect();
    assert_eq!(
        blocks,
        [
            (
                "scripts/hudlayout.res".to_owned(),
                vec!["Resource/HudLayout.res".to_owned(), METER.to_owned()],
                Some("r174".to_owned())
            ),
            (
                "resource/ui/huditemeffectmeter.res".to_owned(),
                vec![
                    "Resource/UI/HudItemEffectMeter.res".to_owned(),
                    METER.to_owned()
                ],
                Some("r174".to_owned())
            ),
        ]
    );
    let after = m.with_changes(&changes);
    assert!(
        after
            .model()
            .part(&k(METER))
            .unwrap()
            .value("xpos")
            .is_none()
    );
    assert!(m.remove_key(&k(METER), "labelText", 1).unwrap().is_empty());
}

#[test]
fn an_if_block_line_is_removed_through_its_parent() {
    let m = model();
    let changes = m
        .remove_key(&k("HudMatchStatus/ObjectiveStatusTimePanel"), "wide", 1)
        .unwrap();
    assert_eq!(
        changes.len(),
        1,
        "the if_match line belongs to the same block"
    );
    let after = m.with_changes(&changes);
    let timer = after
        .model()
        .part(&k("HudMatchStatus/ObjectiveStatusTimePanel"))
        .unwrap();
    assert!(timer.chain_lines("wide").is_empty());
}

#[test]
fn set_key_text_writes_any_key_as_text() {
    let m = model();
    let changes = m
        .set_key_text(&k(HV), "labelText", "%Health% HP", 3)
        .unwrap();
    assert_eq!(changes.len(), 1);
    let after = m.with_changes(&changes);
    let hv = after.model().part(&k(HV)).unwrap();
    assert_eq!(hv.value("labeltext").map(|l| l.text()), Some("%Health% HP"));
    let added = m.set_key_text(&k(HV), "allcaps", "1", 4).unwrap();
    let after = m.with_changes(&added);
    assert_eq!(
        after
            .model()
            .part(&k(HV))
            .unwrap()
            .value("allcaps")
            .map(|l| l.text()),
        Some("1")
    );
}

#[test]
fn text_edits_refuse_identity_code_owned_and_unwritable() {
    let m = model();
    assert_eq!(
        m.set_key_text(&k("Nope"), "xpos", "1", 1),
        Err(EditRefused::UnknownPart)
    );
    assert_eq!(
        m.set_key_text(&k(HV), "ControlName", "Label", 1),
        Err(EditRefused::NotOffered)
    );
    assert_eq!(
        m.remove_key(&k(HV), "controlname", 1),
        Err(EditRefused::NotOffered)
    );
    assert_eq!(
        m.set_key_text(&k(METER), "visible", "0", 1),
        Err(EditRefused::NotOffered)
    );
    assert_eq!(
        m.set_key_text(&k(HV), "labelText", "say \"hi\"", 1),
        Err(EditRefused::BadValue)
    );
    assert_eq!(
        m.set_key_text(&k(HV), "labelText", "two\nlines", 1),
        Err(EditRefused::BadValue)
    );
}

#[test]
fn set_key_text_refuses_an_empty_key() {
    let m = model();
    for key in ["", " ", "\t"] {
        assert_eq!(
            m.set_key_text(&k(HV), key, "1", 1),
            Err(EditRefused::BadValue),
            "{key:?}"
        );
    }
}

#[test]
fn an_unreadable_part_is_read_only() {
    let files = common::hud_stock::stock_files_with(&[(HP, None)]);
    let m = HudModel::load(&files, Platform::Linux).unwrap();
    assert_eq!(
        m.set_key_text(&k(HEALTH), "zpos", "1", 1),
        Err(EditRefused::ReadOnly)
    );
    assert_eq!(
        m.remove_key(&k(HEALTH), "zpos", 1),
        Err(EditRefused::ReadOnly)
    );
}

use tf2tools_core::ALIGNMENTS;

const DAMAGE: &str = "CDamageAccountPanel";

const SCHEME_WITH_FONTS: &str = r#""Scheme" { Colors { TanDark "117 107 94 255" TanLight "235 226 202 255" }
  Fonts { Default { "1" { name Verdana tall 12 } } HudFontSmall { "1" { name "TF2 Build" tall 14 } } } }"#;

fn value_after(m: &HudModel, changes: &[Change], part: &str, key: &str) -> Option<String> {
    let after = m.with_changes(changes);
    after
        .model()
        .part(&k(part))
        .unwrap()
        .value(key)
        .map(|l| l.text().to_owned())
}

#[test]
fn text_alignment_and_switches_write_their_rows() {
    let m = model();
    let text = m
        .set_property(&k(HV), "labeltext", "HP %Health%", 1)
        .unwrap();
    assert_eq!(
        value_after(&m, &text, HV, "labelText").as_deref(),
        Some("HP %Health%")
    );
    let align = m
        .set_property(&k(HV), "textAlignment", "North-East", 2)
        .unwrap();
    assert_eq!(
        value_after(&m, &align, HV, "textAlignment").as_deref(),
        Some("north-east")
    );
    let caps = m.set_property(&k(HV), "allcaps", "1", 3).unwrap();
    assert_eq!(value_after(&m, &caps, HV, "allcaps").as_deref(), Some("1"));
    assert_eq!(ALIGNMENTS.len(), 9);
}

#[test]
fn stage_2_values_are_checked_by_kind() {
    let m = model();
    let bad = |key: &str, value: &str| m.set_property(&k(HV), key, value, 1);
    assert_eq!(bad("textAlignment", "middle"), Err(EditRefused::BadValue));
    assert_eq!(bad("allcaps", "yes"), Err(EditRefused::BadValue));
    assert_eq!(bad("labelText", "a\"b"), Err(EditRefused::BadValue));
    assert_eq!(bad("fgcolor", "1 2 3"), Err(EditRefused::BadValue));
    assert_eq!(bad("font", "NoSuchFont"), Err(EditRefused::BadValue));
    assert_eq!(
        bad("image", "replay/x"),
        Err(EditRefused::NotOffered),
        "no such row on a label"
    );
    assert_eq!(
        bad("zpos", "1"),
        Err(EditRefused::NotOffered),
        "a Layer row is not a stage 2 kind"
    );
    assert_eq!(
        m.set_property(&k("Nope"), "labelText", "x", 1),
        Err(EditRefused::UnknownPart)
    );
}

#[test]
fn numbers_take_two_decimals_within_range() {
    let m = model();
    let set = m
        .set_property(&k(DAMAGE), "delta_lifetime", "1.5", 1)
        .unwrap();
    assert_eq!(
        value_after(&m, &set, DAMAGE, "delta_lifetime").as_deref(),
        Some("1.5")
    );
    for bad in ["1.555", "-1", "1921", "abc", "", "1e3", ".5", "5.", " 1"] {
        assert_eq!(
            m.set_property(&k(DAMAGE), "delta_lifetime", bad, 1),
            Err(EditRefused::BadValue),
            "{bad}"
        );
    }
    assert!(
        m.set_property(&k(DAMAGE), "delta_lifetime", "1920", 1)
            .is_ok()
    );
    assert!(
        m.set_property(&k(DAMAGE), "delta_lifetime", "007", 1)
            .is_ok()
    );
}

#[test]
fn a_font_row_takes_any_scheme_font() {
    let files = common::hud_stock::stock_files_with(&[(
        "resource/clientscheme.res",
        Some(SCHEME_WITH_FONTS),
    )]);
    let m = HudModel::load(&files, Platform::Linux).unwrap();
    assert_eq!(m.scheme().font_names(), ["Default", "HudFontSmall"]);
    let set = m
        .set_property(&k(DAMAGE), "delta_item_font", "hudfontsmall", 1)
        .unwrap();
    assert_eq!(
        value_after(&m, &set, DAMAGE, "delta_item_font").as_deref(),
        Some("hudfontsmall")
    );
}

#[test]
fn dropped_rows_are_gone_from_the_schema() {
    let m = model();
    let damage = tf2tools_core::ClassSchema::for_part(m.part(&k(DAMAGE)).unwrap());
    assert!(damage.get("PositiveColor").is_none() && damage.get("NegativeColor").is_none());
    let notice = tf2tools_core::ClassSchema::all()
        .iter()
        .find(|s| s.class() == "HudDeathNotice")
        .unwrap();
    assert!(notice.get("LocalPlayerColor").is_none());
}

#[test]
fn reset_undoes_a_set_in_an_if_block_later_removed() {
    let file = "resource/ui/hudmatchstatus.res";
    let block = ["Resource/UI/HudMatchStatus.res", "ObjectiveStatusTimePanel"];
    let changes = vec![
        Change::set(
            1,
            SetKey::new(
                file,
                &[block[0], block[1], "IF_match"],
                "wide",
                None,
                None,
                "140",
            ),
        ),
        Change::remove(2, RemoveKey::new(file, &block, "wide", None, None)),
    ];
    let edited = model().with_changes(&changes).into_model();
    assert!(
        edited
            .reset_property(
                &k("HudMatchStatus/ObjectiveStatusTimePanel"),
                &["wide"],
                &changes
            )
            .is_empty()
    );
}

#[test]
fn the_colour_row_writes_the_slot_recolour_picks() {
    let files = common::hud_stock::stock_files_with(&[(
        "resource/ui/hudmediccharge.res",
        Some(
            r#""Resource/UI/HudMedicCharge.res" { ChargeLabel { ControlName CExLabel xpos 0 ypos 0 wide 100 tall 20 fgcolor_override TanDark } }"#,
        ),
    )]);
    let m = HudModel::load(&files, Platform::Linux).unwrap();
    let label = "HudMedicCharge/ChargeLabel";
    let row = m.set_property(&k(label), "fgcolor", "TanLight", 1).unwrap();
    assert_eq!(row, m.recolour_part(&k(label), "TanLight", 1).unwrap());
    assert_eq!(
        value_after(&m, &row, label, "fgcolor_override").as_deref(),
        Some("TanLight")
    );
}

#[test]
fn player_list_spacing_takes_whole_numbers_only() {
    let m = HudModel::load_scene(
        &stock_files(),
        Platform::Linux,
        tf2tools_core::Scene::Scoreboard,
    )
    .unwrap();
    let list = "scores/BluePlayerList";
    for key in ["linespacing", "linegap", "sectiongap"] {
        assert_eq!(
            m.set_property(&k(list), key, "18.5", 1),
            Err(EditRefused::BadValue),
            "{key}"
        );
        let set = m.set_property(&k(list), key, "22", 1).unwrap();
        assert_eq!(value_after(&m, &set, list, key).as_deref(), Some("22"));
    }
}

#[test]
fn a_font_name_that_cannot_be_written_is_refused() {
    let scheme = "\"Scheme\" { Fonts { \"Two\nLines\" { \"1\" { name Verdana tall 12 } } } }";
    let files = common::hud_stock::stock_files_with(&[("resource/clientscheme.res", Some(scheme))]);
    let m = HudModel::load(&files, Platform::Linux).unwrap();
    assert_eq!(m.scheme().font_names(), ["Two\nLines"]);
    assert_eq!(
        m.set_property(&k(DAMAGE), "delta_item_font", "two\nlines", 1),
        Err(EditRefused::BadValue)
    );
}

#[test]
fn reset_keeps_a_set_in_another_parts_if_block() {
    let file = "resource/ui/hudmatchstatus.res";
    let root = "Resource/UI/HudMatchStatus.res";
    let set = |group, block: &[&str]| {
        Change::set(group, SetKey::new(file, block, "wide", None, None, "140"))
    };
    let changes = vec![
        set(1, &[root, "RoundSignModel", "if_match"]),
        set(2, &[root, "if_match"]),
    ];
    let edited = model().with_changes(&changes).into_model();
    assert_eq!(
        edited.reset_property(
            &k("HudMatchStatus/ObjectiveStatusTimePanel"),
            &["wide"],
            &changes
        ),
        changes
    );
}

#[test]
fn not_offered_wins_over_read_only() {
    let files = common::hud_stock::stock_files_with(&[(HP, None)]);
    let m = HudModel::load(&files, Platform::Linux).unwrap();
    assert_eq!(
        m.set_key_text(&k(HEALTH), "ControlName", "Label", 1),
        Err(EditRefused::NotOffered)
    );
    assert_eq!(
        m.remove_key(&k(HEALTH), "ControlName", 1),
        Err(EditRefused::NotOffered)
    );
}

#[test]
fn field_name_is_not_offered() {
    let m = model();
    assert_eq!(
        m.set_key_text(&k(HV), "fieldName", "Other", 1),
        Err(EditRefused::NotOffered)
    );
    assert_eq!(
        m.remove_key(&k(HV), "FIELDNAME", 1),
        Err(EditRefused::NotOffered)
    );
}

#[test]
fn set_property_does_not_offer_a_key_the_code_owns() {
    let m = model();
    assert_eq!(
        m.set_property(&k(METER), "visible", "0", 1),
        Err(EditRefused::NotOffered)
    );
}
