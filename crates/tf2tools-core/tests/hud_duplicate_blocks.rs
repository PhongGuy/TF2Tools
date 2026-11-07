//! Duplicate sibling blocks for one panel: the game applies them all in file order, so reads
//! merge them key by key (the later wins) and edits land where they take effect.

use crate::common;

use common::hud_stock::{HEALTH, hudlayout_with_killfeed, stock_files_with, with_blocks};
use tf2tools_core::{
    AddBlock, BlockLine, Change, HudFiles, HudModel, OptionChange, OptionValue, PartKey, Platform,
    Screen,
};

const KF: &str = "HudDeathNotice";
const HV: &str = "HudPlayerStatus/HudPlayerHealth/PlayerStatusHealthValue";
const FIRST: &str = "  HudDeathNotice { fieldName HudDeathNotice xpos r640 ypos 18 wide 628 tall 468 MaxDeathNotices 4 }\n";

fn killfeed_files(blocks: &str) -> HudFiles {
    let layout = hudlayout_with_killfeed(blocks);
    stock_files_with(&[("scripts/hudlayout.res", Some(layout.as_str()))])
}

fn load(files: &HudFiles) -> HudModel {
    HudModel::load(files, Platform::Linux).unwrap()
}

fn value(m: &HudModel, part: &str, key: &str) -> Option<String> {
    m.part(&PartKey::new(part))?
        .value(key)
        .map(|l| l.text().to_owned())
}

fn rows(n: i32) -> Change {
    let choice = OptionChange::new(OptionChange::BUILTIN, "killfeed_rows", OptionValue::Int(n));
    Change::option(1, choice)
}

#[test]
fn a_later_duplicate_block_overrides_key_by_key() {
    let m = load(&killfeed_files(&format!(
        "{FIRST}  HudDeathNotice {{ MaxDeathNotices 7 LineHeight 20 }}\n"
    )));
    assert_eq!(value(&m, KF, "MaxDeathNotices").as_deref(), Some("7"));
    assert_eq!(value(&m, KF, "xpos").as_deref(), Some("r640"));
    assert_eq!(value(&m, KF, "LineHeight").as_deref(), Some("20"));
}

#[test]
fn an_option_on_a_duplicated_element_reads_back_in_the_part() {
    // The second block lacks the key, so the pristine value comes from the first.
    let m = load(&killfeed_files(&format!(
        "{FIRST}  HudDeathNotice {{ LineHeight 20 }}\n"
    )));
    assert_eq!(
        m.option_value("killfeed_rows", &[]),
        Some(OptionValue::Int(4))
    );
    let e = m.with_changes(&[rows(6)]);
    assert_eq!((e.applied().len(), e.dropped()), (1, 0));
    assert_eq!(
        value(e.model(), KF, "MaxDeathNotices").as_deref(),
        Some("6")
    );
    assert_eq!(
        e.model().option_value("killfeed_rows", &[rows(6)]),
        Some(OptionValue::Int(6))
    );
}

#[test]
fn the_pristine_option_value_is_the_later_block() {
    let m = load(&killfeed_files(&format!(
        "{FIRST}  HudDeathNotice {{ MaxDeathNotices 7 }}\n"
    )));
    assert_eq!(
        m.option_value("killfeed_rows", &[]),
        Some(OptionValue::Int(7))
    );
}

#[test]
fn a_set_and_a_remove_on_a_duplicated_element_read_back() {
    let m = load(&killfeed_files(&format!(
        "{FIRST}  HudDeathNotice {{ LineHeight 20 }}\n"
    )));
    let set = m
        .set_key_text(&PartKey::new(KF), "MaxDeathNotices", "9", 1)
        .unwrap();
    let e = m.with_changes(&set);
    assert_eq!(
        value(e.model(), KF, "MaxDeathNotices").as_deref(),
        Some("9")
    );

    // Removing clears the key from every block, so no earlier block shows through.
    let m = load(&killfeed_files(&format!(
        "{FIRST}  HudDeathNotice {{ MaxDeathNotices 7 }}\n"
    )));
    let removed = m
        .remove_key(&PartKey::new(KF), "MaxDeathNotices", 1)
        .unwrap();
    let e = m.with_changes(&removed);
    assert_eq!(e.dropped(), 0);
    assert_eq!(value(e.model(), KF, "MaxDeathNotices"), None);
}

#[test]
fn a_block_whose_conditional_fails_is_neither_read_nor_written() {
    let m = load(&killfeed_files(&format!(
        "{FIRST}  HudDeathNotice [$X360] {{ MaxDeathNotices 9 }}\n"
    )));
    assert_eq!(value(&m, KF, "MaxDeathNotices").as_deref(), Some("4"));
    let e = m.with_changes(&[rows(6)]);
    assert_eq!(
        value(e.model(), KF, "MaxDeathNotices").as_deref(),
        Some("6")
    );

    // A new sub-block lands in the live block, before the one Linux skips.
    let root = "Resource/HudLayout.res";
    let add = AddBlock::new(
        "scripts/hudlayout.res",
        &[root, KF],
        "Extra",
        None,
        vec![BlockLine::new("k", "v", None)],
    );
    let e = m.with_changes(&[Change::add_block(1, add)]);
    assert_eq!(e.applied().len(), 1);
    let text = String::from_utf8_lossy(
        e.model()
            .document("scripts/hudlayout.res")
            .unwrap()
            .as_bytes(),
    )
    .into_owned();
    let skipped = text.find("HudDeathNotice [$X360]").unwrap();
    assert!(text.find("Extra").unwrap() < skipped, "{text}");
}

fn health_with(blocks: &str) -> HudFiles {
    stock_files_with(&[(
        "resource/ui/hudplayerhealth.res",
        Some(with_blocks(HEALTH, blocks).as_str()),
    )])
}

#[test]
fn duplicate_control_blocks_are_one_part() {
    let m = load(&health_with(
        "  PlayerStatusHealthValue { ControlName CExLabel xpos 90 }\n",
    ));
    assert_eq!(value(&m, HV, "xpos").as_deref(), Some("90"));
    assert_eq!(value(&m, HV, "ypos").as_deref(), Some("52"));
    assert!(m.part(&PartKey::new(format!("{HV}#2"))).is_none());

    let set = m.set_key_text(&PartKey::new(HV), "ypos", "60", 1).unwrap();
    let e = m.with_changes(&set);
    assert_eq!(value(e.model(), HV, "ypos").as_deref(), Some("60"));
    let removed = m.remove_key(&PartKey::new(HV), "xpos", 1).unwrap();
    let e = m.with_changes(&removed);
    assert_eq!(value(e.model(), HV, "xpos"), None);
}

#[test]
fn layout_places_a_duplicated_control_by_the_merged_values() {
    let screen = Screen::new(1920, 1080);
    let twice = load(&health_with(
        "  PlayerStatusHealthValue { ControlName CExLabel xpos 90 }\n",
    ))
    .layout(screen);
    let once = load(&stock_files_with(&[(
        "resource/ui/hudplayerhealth.res",
        Some(HEALTH.replacen("xpos 76", "xpos 90", 1).as_str()),
    )]))
    .layout(screen);
    let rect = |b: &tf2tools_core::PartBox| (b.x(), b.y(), b.wide(), b.tall());
    let key = PartKey::new(HV);
    assert_eq!(twice.get(&key).map(rect), once.get(&key).map(rect));
}

const HEALTH_PART: &str = "HudPlayerStatus/HudPlayerHealth";
const HEALTH_AGAIN: &str = "  HudPlayerHealth { ControlName EditablePanel zpos 3 }\n";

fn hidden(m: &HudModel, part: &str) -> Option<tf2tools_core::HiddenBy> {
    let layout = m.layout(Screen::new(1920, 1080));
    layout.get(&PartKey::new(part)).unwrap().hidden()
}

#[test]
fn a_later_block_without_visible_shows_the_panel_again() {
    // The game resets `visible` to 1 on every block it applies, so block 1's 0 does not hold.
    let shown = HEALTH.replacen("    visible 1\n", "    visible 0\n", 1);
    let m = load(&stock_files_with(&[(
        "resource/ui/hudplayerhealth.res",
        Some(with_blocks(&shown, HEALTH_AGAIN).as_str()),
    )]));
    assert_eq!(value(&m, HEALTH_PART, "visible"), None);
    assert_eq!(hidden(&m, HEALTH_PART), None);
}

#[test]
fn a_hide_writes_visible_into_the_last_block() {
    let m = load(&health_with(HEALTH_AGAIN));
    let hide = m.hide_part(&PartKey::new(HEALTH_PART), 1).unwrap();
    let e = m.with_changes(&hide);
    assert_eq!(e.dropped(), 0);
    let doc = e
        .model()
        .document("resource/ui/hudplayerhealth.res")
        .unwrap();
    let blocks = doc.find(&["Resource/UI/HudPlayerHealth.res", "HudPlayerHealth"]);
    let visible = |b: &tf2tools_core::KvEntry| {
        (b.children().iter())
            .find(|e| e.key() == "visible")
            .and_then(|e| e.text().map(str::to_owned))
    };
    assert_eq!(blocks.last().and_then(|b| visible(b)).as_deref(), Some("0"));
    assert_eq!(
        hidden(e.model(), HEALTH_PART),
        Some(tf2tools_core::HiddenBy::Visible)
    );
}
