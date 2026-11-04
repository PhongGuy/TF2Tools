//! Paint order among siblings with equal `zpos`: one whose block sets `zpos` paints beneath
//! the earlier siblings that set the same value, so a label listed before its shadow stays on
//! top; siblings that never set `zpos` keep file order.

mod common;

use common::hud_stock::{HEALTH, label, stock_files_with, with_blocks};
use tf2tools_core::{HudLayout, HudModel, PartKey, Platform, Screen};

const HD: Screen = Screen::new(1920, 1080);
const PANEL: &str = "HudPlayerStatus/HudPlayerHealth";

fn layout(blocks: &str) -> HudLayout {
    let health = with_blocks(HEALTH, blocks);
    let files = stock_files_with(&[("resource/ui/hudplayerhealth.res", Some(&health))]);
    HudModel::load(&files, Platform::Linux).unwrap().layout(HD)
}

fn rank(l: &HudLayout, name: &str) -> usize {
    let key = format!("{PANEL}/{name}");
    l.paint_order()
        .position(|b| b.key().as_str() == key)
        .unwrap_or_else(|| panic!("{key}"))
}

#[test]
fn a_label_listed_before_its_shadow_paints_over_it() {
    let l = layout(
        &(label("Fill", "10", "50", &[("zpos", "5")])
            + &label("FillShadow", "11", "51", &[("zpos", "5")])),
    );
    assert!(rank(&l, "FillShadow") < rank(&l, "Fill"));
    // Unit (15, 55) lies in both boxes: x 15 x 2.25 = 33, y r120 + 123 = 933, below the stock chat.
    assert_eq!(l.hit(33, 933), Some(&PartKey::new(format!("{PANEL}/Fill"))));
}

#[test]
fn siblings_that_never_set_zpos_keep_file_order() {
    let l = layout(&(label("First", "10", "50", &[]) + &label("Second", "11", "51", &[])));
    assert!(rank(&l, "First") < rank(&l, "Second"));
    assert_eq!(
        l.hit(33, 933),
        Some(&PartKey::new(format!("{PANEL}/Second")))
    );
}

#[test]
fn a_higher_zpos_still_paints_over_an_earlier_lower_one() {
    let l = layout(
        &(label("Top", "10", "10", &[("zpos", "6")])
            + &label("Under", "11", "11", &[("zpos", "5")])),
    );
    assert!(rank(&l, "Under") < rank(&l, "Top"));
}
#[test]
fn an_explicit_zpos_beside_none_keeps_todays_order() {
    // Unchanged on purpose: no stock file shows how the game orders a mix of set and unset.
    let l =
        layout(&(label("Set", "10", "10", &[("zpos", "0")]) + &label("Unset", "11", "11", &[])));
    assert!(rank(&l, "Set") < rank(&l, "Unset"));
    let l =
        layout(&(label("Unset", "10", "10", &[]) + &label("Set", "11", "11", &[("zpos", "0")])));
    assert!(rank(&l, "Unset") < rank(&l, "Set"));
}
