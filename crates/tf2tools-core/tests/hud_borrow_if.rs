//! A borrowed part keeps its `if_*` sub-blocks as sub-blocks, so it behaves in every mode as it
//! did in its own HUD.

use crate::common;

use common::hud_borrow::source;
use common::hud_stock::{HEALTH, stock_files, stock_files_with};
use tf2tools_core::{HudFiles, HudModel, PartKey, Platform};

const ROOT: &str = "HudPlayerStatus/HudPlayerHealth";
const BADGE: &str = "HudPlayerStatus/HudPlayerHealth/MineBadge";
const HEALTH_RES: &str = "resource/ui/hudplayerhealth.res";

fn k(t: &str) -> PartKey {
    PartKey::new(t)
}

fn mine(root_extra: &str, badge_extra: &str) -> String {
    format!(
        "\"Resource/UI/HudPlayerHealth.res\"\n{{\n  HudPlayerHealth {{ ControlName EditablePanel xpos 4 {root_extra} wide 200 tall 100 visible 1 }}\n  MineBadge {{ ControlName ImagePanel xpos 10 ypos 10 {badge_extra} }}\n}}\n"
    )
}

/// The text of `key` inside the `if_` block `sub` of `part`, one entry per line found.
fn in_if(m: &HudModel, part: &str, sub: &str, key: &str) -> Vec<String> {
    let (file, block) = m.part(&k(part)).unwrap().own_block();
    let mut path: Vec<&str> = block.iter().map(String::as_str).collect();
    path.extend([sub, key]);
    m.document(file)
        .unwrap()
        .find(&path)
        .iter()
        .map(|e| e.text().unwrap_or_default().to_owned())
        .collect()
}

fn borrowed(files: &HudFiles, source_text: &str, part: &str) -> HudModel {
    let t = HudModel::load(files, Platform::Linux).unwrap();
    let b = t
        .borrow_part(&source(&[(HEALTH_RES, source_text)]), files, &k(part), 1)
        .unwrap();
    let e = t.with_changes(b.changes());
    assert_eq!(e.dropped(), 0, "borrowing {part} should drop no change");
    e.into_model()
}

#[test]
fn an_if_block_the_target_lacks_is_added_and_does_not_leak_into_the_plain_lines() {
    let src = mine("if_mvm { ypos 0 } ypos r100", "");
    let m = borrowed(&stock_files(), &src, ROOT);
    // Outside MvM the part sits where its own HUD put it; the MvM line only applies in MvM.
    assert_eq!(
        m.part(&k(ROOT)).unwrap().value("ypos").unwrap().text(),
        "r100"
    );
    assert_eq!(in_if(&m, ROOT, "if_mvm", "ypos"), ["0"]);
}

#[test]
fn an_if_block_the_target_has_gets_the_borrowed_lines_for_those_keys_only() {
    let health = HEALTH.replacen(
        "    zpos 2\n",
        "    zpos 2\n    if_mvm { ypos 5 alpha 9 }\n",
        1,
    );
    let files = stock_files_with(&[(HEALTH_RES, Some(&health))]);
    let src = mine("if_mvm { ypos 0 }", "");
    let m = borrowed(&files, &src, ROOT);
    assert_eq!(
        in_if(&m, ROOT, "if_mvm", "ypos"),
        ["0"],
        "replaced, not doubled"
    );
    assert_eq!(in_if(&m, ROOT, "if_mvm", "alpha"), ["9"], "other keys stay");
}

#[test]
fn a_new_child_brings_its_if_blocks() {
    let src = mine("", "if_mvm { xpos 99 }");
    let m = borrowed(&stock_files(), &src, BADGE);
    assert_eq!(
        m.part(&k(BADGE)).unwrap().value("xpos").unwrap().text(),
        "10"
    );
    assert_eq!(in_if(&m, BADGE, "if_mvm", "xpos"), ["99"]);
}
