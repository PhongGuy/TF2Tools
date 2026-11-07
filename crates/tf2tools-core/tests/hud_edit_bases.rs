//! In a copy, edits to parts drawn through #base write the HUD's own file, never the base.

use crate::common;

use common::hud_borrow::files_over_stock;
use common::hud_stock::{FREEZE_PANEL, HEALTH, freeze_stub, stock_files_with};
use tf2tools_core::{
    BorrowSource, Change, ChangeTarget, EditRefused, HudModel, NewPart, PartKey, Platform,
    RemoveKey, Scene, Screen, SetKey,
};

const HEALTH_PATH: &str = "resource/ui/hudplayerhealth.res";
const BASE_PATH: &str = "resource/ui/health_base.res";
const HV: &str = "HudPlayerStatus/HudPlayerHealth/PlayerStatusHealthValue";
const HV_NAME: &str = "PlayerStatusHealthValue";
const KILLER: &str = "FreezePanel/FreezePanelBase/FreezeLabelKiller";
const BASIC: &str = "resource/ui/freezepanel_basic.res";
const SD: Screen = Screen::new(640, 480);

fn health_over(base: &str) -> HudModel {
    let own = format!(
        "#base \"health_base.res\"\n{}",
        HEALTH.replace("    fgcolor TanDark\n", "")
    );
    let files = stock_files_with(&[(HEALTH_PATH, Some(&own)), (BASE_PATH, Some(base))]);
    HudModel::load(&files.with_install_folder("myhud"), Platform::Linux).unwrap()
}

fn sets(changes: &[Change]) -> Vec<&SetKey> {
    changes
        .iter()
        .filter_map(|c| match c.target() {
            ChangeTarget::Set(s) => Some(s),
            _ => None,
        })
        .collect()
}

fn text(m: &HudModel, path: &str) -> String {
    String::from_utf8(m.document(path).unwrap().as_bytes().to_vec()).unwrap()
}

#[test]
fn a_recolour_of_a_base_colour_writes_the_own_file() {
    let m = health_over(r#""Base" { PlayerStatusHealthValue { fgcolor "1 2 3 255" } }"#);
    let changes = m
        .recolour_part(&PartKey::new(HV), "255 0 0 255", 1)
        .unwrap();
    let set = sets(&changes);
    assert_eq!(set.len(), 1);
    assert_eq!(
        (set[0].file(), set[0].old()),
        (HEALTH_PATH, Some("1 2 3 255"))
    );
    assert_eq!(
        set[0].block(),
        ["Resource/UI/HudPlayerHealth.res", "PlayerStatusHealthValue"]
    );
    let edited = m.with_changes(&changes);
    assert_eq!((edited.applied().len(), edited.dropped()), (1, 0));
    assert_eq!(edited.touched(), [HEALTH_PATH]);
    let own = text(edited.model(), HEALTH_PATH);
    assert!(own.starts_with("#base \"health_base.res\"") && own.contains("255 0 0 255"));
    let part = edited.model().part(&PartKey::new(HV)).unwrap();
    assert_eq!(part.value("fgcolor").unwrap().file(), HEALTH_PATH);
}

#[test]
fn a_move_writes_the_own_line_once_when_a_base_also_sets_it() {
    let m = health_over(r#""Base" { PlayerStatusHealthValue { xpos 99 } }"#);
    let changes = m.move_part(&PartKey::new(HV), 10, 0, SD, 1).unwrap();
    let xs: Vec<_> = sets(&changes)
        .into_iter()
        .filter(|s| s.key() == "xpos")
        .collect();
    assert_eq!(xs.len(), 1);
    assert_eq!(xs[0].file(), HEALTH_PATH);
}

#[test]
fn a_set_on_a_block_no_file_holds_still_drops() {
    let m = health_over(r#""Base" { PlayerStatusHealthValue { xpos 99 } }"#);
    let gone = Change::set(
        1,
        SetKey::new(
            HEALTH_PATH,
            &["Resource/UI/HudPlayerHealth.res", "Gone"],
            "xpos",
            None,
            None,
            "5",
        ),
    );
    let edited = m.with_changes(&[gone]);
    assert_eq!((edited.applied().len(), edited.dropped()), (0, 1));
}

#[test]
fn a_stub_part_in_a_copy_moves_by_giving_the_stub_its_own_block() {
    let files = freeze_stub().with_install_folder("myhud");
    let m = HudModel::load_scene(&files, Platform::Linux, Scene::FreezeCam).unwrap();
    let key = PartKey::new(KILLER);
    assert!(m.part(&key).unwrap().is_readable());
    let changes = m.move_part(&key, 20, 0, SD, 1).unwrap();
    assert!(sets(&changes).iter().all(|s| s.file() == BASIC));
    let edited = m.with_changes(&changes);
    assert_eq!(edited.dropped(), 0);
    let stub = text(edited.model(), BASIC);
    assert!(
        stub.starts_with("#base \"freezepanel_basic_variant.res\"")
            && stub.contains("FreezeLabelKiller")
    );
    assert!(stub.contains("Resource/UI/FreezePanel_Basic.res"));
    assert!(
        stub.contains("FreezePanelBase") && !stub.contains("[$"),
        "{stub}"
    );
    let again = stock_files_with(&[
        (
            "resource/ui/freezepanel_basic_variant.res",
            Some(FREEZE_PANEL),
        ),
        (BASIC, Some(&stub)),
    ])
    .with_install_folder("myhud");
    let reloaded = HudModel::load_scene(&again, Platform::Linux, Scene::FreezeCam).unwrap();
    assert_eq!(
        reloaded.layout(SD).get(&key).unwrap().x(),
        edited.model().layout(SD).get(&key).unwrap().x()
    );
}

#[test]
fn a_stock_stub_stays_read_only() {
    let m = HudModel::load_scene(&freeze_stub(), Platform::Linux, Scene::FreezeCam).unwrap();
    let part = m.part(&PartKey::new(KILLER)).unwrap();
    assert!(part.is_from_base() && !part.is_readable());
    assert_eq!(
        m.move_part(&PartKey::new(KILLER), 4, 0, SD, 1),
        Err(EditRefused::ReadOnly)
    );
}

#[test]
fn removing_a_key_both_set_removes_only_the_own_line() {
    let m = health_over(r#""Base" { PlayerStatusHealthValue { xpos 99 } }"#);
    let changes = m.remove_key(&PartKey::new(HV), "xpos", 1).unwrap();
    assert!(!changes.is_empty() && sets(&changes).is_empty());
    assert!(
        changes
            .iter()
            .all(|c| matches!(c.target(), ChangeTarget::Remove(r) if r.file() == HEALTH_PATH))
    );
    let edited = m.with_changes(&changes);
    let part = edited.model().part(&PartKey::new(HV)).unwrap();
    assert_eq!(
        (
            part.value("xpos").unwrap().text(),
            part.value("xpos").unwrap().file()
        ),
        ("99", BASE_PATH)
    );
}

#[test]
fn a_remove_aimed_at_a_base_file_is_dropped() {
    let m = health_over(r#""Base" { PlayerStatusHealthValue { allcaps 1 } }"#);
    let aimed = Change::remove(
        1,
        RemoveKey::new(BASE_PATH, &["Base", HV_NAME], "allcaps", None, Some("1")),
    );
    let edited = m.with_changes(&[aimed]);
    assert_eq!((edited.applied().len(), edited.dropped()), (0, 1));
    assert!(edited.touched().is_empty());
}

#[test]
fn removing_a_key_a_base_sets_writes_the_class_default() {
    let m = health_over(r#""Base" { PlayerStatusHealthValue { allcaps 1 } }"#);
    let changes = m.remove_key(&PartKey::new(HV), "allcaps", 1).unwrap();
    assert!(changes.iter().all(|c| match c.target() {
        ChangeTarget::Remove(r) => r.file() != BASE_PATH,
        ChangeTarget::Set(s) => s.file() == HEALTH_PATH && s.value() == "0",
        _ => false,
    }));
    assert_eq!(sets(&changes).len(), 1);
    let edited = m.with_changes(&changes);
    let part = edited.model().part(&PartKey::new(HV)).unwrap();
    assert_eq!(part.value("allcaps").unwrap().text(), "0");
}

#[test]
fn removing_a_base_key_with_no_default_is_refused() {
    let m = health_over(r#""Base" { PlayerStatusHealthValue { tf2tools_odd 1 } }"#);
    assert_eq!(
        m.remove_key(&PartKey::new(HV), "tf2tools_odd", 1)
            .unwrap_err(),
        EditRefused::NotOffered
    );
}

#[test]
fn an_own_line_inactive_here_is_written_beside_the_base_winner() {
    // `$WINDOWS` is false on Linux, so the base's xpos wins while the own block still has one.
    let own = HEALTH
        .replace("    fgcolor TanDark\n", "")
        .replace("    xpos 76\n", "    xpos 76 [$WINDOWS]\n");
    let files = stock_files_with(&[
        (
            HEALTH_PATH,
            Some(&format!("#base \"health_base.res\"\n{own}")),
        ),
        (
            BASE_PATH,
            Some(r#""Base" { PlayerStatusHealthValue { xpos 99 } }"#),
        ),
    ]);
    let m = HudModel::load(&files.with_install_folder("myhud"), Platform::Linux).unwrap();
    let changes = m.move_part(&PartKey::new(HV), 10, 0, SD, 1).unwrap();
    let mut xs: Vec<_> = sets(&changes)
        .into_iter()
        .filter(|s| s.key() == "xpos")
        .map(|s| (s.file(), s.cond(), s.old()))
        .collect();
    xs.sort();
    assert_eq!(
        xs,
        [
            (HEALTH_PATH, None, Some("99")),
            (HEALTH_PATH, Some("$WINDOWS"), Some("76"))
        ]
    );
    assert_eq!(m.with_changes(&changes).dropped(), 0);
}

#[test]
fn a_new_part_beside_a_stub_part_creates_its_parent_in_the_stub() {
    let files = freeze_stub().with_install_folder("myhud");
    let m = HudModel::load_scene(&files, Platform::Linux, Scene::FreezeCam).unwrap();
    let (key, changes) = m
        .add_part(NewPart::Shadow, &PartKey::new(KILLER), 1)
        .unwrap();
    let edited = m.with_changes(&changes);
    assert_eq!((edited.applied().len(), edited.dropped()), (1, 0));
    assert_eq!(edited.touched(), [BASIC]);
    assert!(edited.model().part(&key).is_some(), "{key:?} is drawn");
    let stub = text(edited.model(), BASIC);
    assert!(
        stub.starts_with("#base") && stub.contains("FreezeLabelKillerShadow"),
        "{stub}"
    );
}

#[test]
fn a_borrow_under_a_parent_only_a_base_defines_lands_in_the_stub() {
    let mine = FREEZE_PANEL.replace(
        "    AvatarImage {",
        "    MyLabel { ControlName CExLabel font HudFontSmall xpos 5 ypos 5 wide 50 tall 10 labelText \"hi\" }\n    AvatarImage {",
    );
    let source_files = files_over_stock(&[(BASIC, &mine)]);
    let source_model =
        HudModel::load_scene(&source_files, Platform::Linux, Scene::FreezeCam).unwrap();
    let source = BorrowSource::new(
        source_model,
        source_files,
        "Mine HUD",
        "Mia",
        "GameBanana item 7",
    );
    let files = freeze_stub().with_install_folder("myhud");
    let m = HudModel::load_scene(&files, Platform::Linux, Scene::FreezeCam).unwrap();
    let key = PartKey::new("FreezePanel/FreezePanelBase/MyLabel");
    assert!(m.part(&key).is_none());
    let borrowed = m.borrow_part(&source, &files, &key, 1).unwrap();
    let edited = m.with_changes(borrowed.changes());
    assert_eq!(edited.dropped(), 0);
    assert!(
        edited.touched().iter().all(|f| f == BASIC),
        "{:?}",
        edited.touched()
    );
    assert!(edited.model().part(&key).is_some(), "MyLabel is drawn");
    assert!(text(edited.model(), BASIC).contains("MyLabel"));
}
