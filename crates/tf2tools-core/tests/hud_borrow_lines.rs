mod common;

use common::hud_borrow::source;
use common::hud_stock::stock_files;
use tf2tools_core::{ChangeTarget, HudModel, PartKey, Platform};

const ROOT: &str = "HudPlayerStatus/HudPlayerHealth";
const HEALTH_RES: &str = "resource/ui/hudplayerhealth.res";
const MINE: &str = r#""Resource/UI/HudPlayerHealth.res"
{
  HudPlayerHealth { ControlName EditablePanel xpos 4 ypos r100 zpos 2 wide 200 tall 100 visible 1 }
  PlayerStatusHealthValue { ControlName CExLabel xpos 90 ypos 40 zpos 5 wide 60 tall 30 labelText %Health% }
  MineBadge { ControlName ImagePanel xpos 10 ypos 10 wide 20 tall 20 }
}
"#;

fn k(t: &str) -> PartKey {
    PartKey::new(t)
}
fn target() -> HudModel {
    HudModel::load(&stock_files(), Platform::Linux).unwrap()
}
#[test]
fn every_conditional_branch_of_a_key_is_copied_with_its_condition() {
    let cond = MINE.replace("xpos 4 ypos", "xpos 4 [$WIN32] xpos 40 [$X360] ypos");
    let t = target();
    let b = t
        .borrow_part(&source(&[(HEALTH_RES, &cond)]), &stock_files(), &k(ROOT), 1)
        .unwrap();
    let sets: Vec<(Option<String>, String)> = b
        .changes()
        .iter()
        .filter_map(|c| match c.target() {
            ChangeTarget::Set(s)
                if s.key() == "xpos"
                    && s.block().last().is_some_and(|n| n == "HudPlayerHealth") =>
            {
                Some((s.cond().map(str::to_owned), s.value().to_owned()))
            }
            _ => None,
        })
        .collect();
    assert!(
        sets.contains(&(Some("$WIN32".into()), "4".into())),
        "{sets:?}"
    );
    assert!(
        sets.contains(&(Some("$X360".into()), "40".into())),
        "{sets:?}"
    );
}

fn lines_of(m: &HudModel, part: &str, key: &str) -> Vec<(Option<String>, String)> {
    m.part(&k(part))
        .unwrap()
        .lines(key)
        .iter()
        .map(|l| {
            (
                l.cond()
                    .map(|c| c.trim_matches(|ch| ch == '[' || ch == ']').to_owned()),
                l.text().to_owned(),
            )
        })
        .collect()
}

#[test]
fn an_unconditional_source_line_replaces_the_targets_conditional_pair() {
    let t = target();
    let b = t
        .borrow_part(&source(&[(HEALTH_RES, MINE)]), &stock_files(), &k(ROOT), 1)
        .unwrap();
    let m = t.with_changes(b.changes()).into_model();
    assert_eq!(
        lines_of(&m, ROOT, "xpos"),
        [(None, "4".to_owned())],
        "the [$WIN32] and [$X360] lines are gone"
    );
}

#[test]
fn a_conditional_source_line_replaces_the_targets_unconditional_one() {
    let src = MINE.replace("wide 200", "wide 200 [$WIN32]");
    let t = target();
    let b = t
        .borrow_part(&source(&[(HEALTH_RES, &src)]), &stock_files(), &k(ROOT), 1)
        .unwrap();
    let m = t.with_changes(b.changes()).into_model();
    assert_eq!(
        lines_of(&m, ROOT, "wide"),
        [(Some("$WIN32".to_owned()), "200".to_owned())]
    );
}

#[test]
fn duplicate_source_lines_are_kept_in_order() {
    let src = MINE.replace("ypos r100", "ypos r100 ypos 7");
    let t = target();
    let b = t
        .borrow_part(&source(&[(HEALTH_RES, &src)]), &stock_files(), &k(ROOT), 1)
        .unwrap();
    let m = t.with_changes(b.changes()).into_model();
    assert_eq!(
        lines_of(&m, ROOT, "ypos"),
        [(None, "r100".to_owned()), (None, "7".to_owned())]
    );
}

#[test]
fn an_appending_set_round_trips_through_maker_json() {
    use std::collections::BTreeMap;
    use tf2tools_core::{Change, MakerFile, SetKey};
    let set = SetKey::new("resource/ui/a.res", &["R", "P"], "ypos", None, None, "7").appended();
    let file = MakerFile::new(BTreeMap::new(), vec![Change::set(1, set)]);
    let json = String::from_utf8(file.to_json()).unwrap();
    // A distinct kind, so a build that predates it refuses the file as newer.
    assert!(
        json.contains("\"set_append\"") && !json.contains("\"set\""),
        "{json}"
    );
    let (back, dropped) = MakerFile::parse(json.as_bytes()).unwrap();
    assert_eq!((back, dropped), (file, 0));
}
