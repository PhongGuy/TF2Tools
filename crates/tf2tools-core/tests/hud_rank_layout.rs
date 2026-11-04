//! The rank panel's file: the HUD's own pvprankpanel.res, else the built-in stock tree.

mod common;

use common::hud_stock::{RANK_PANEL, stock_files, stock_files_with};
use tf2tools_core::{HudModel, Platform, RankLayout, Scene, SceneRule};

const RANK_FILE: &str = "resource/ui/pvprankpanel.res";
const NAME: &str = "BGPanel/NameLabel";
const TRACK: &str = "BGPanel/StatsContainer/XPBar/ProgressBarsContainer/ProgressBar";
const FILL: &str = "BGPanel/StatsContainer/XPBar/ProgressBarsContainer/ContinuousProgressBar";

fn read(rank: Option<&str>) -> RankLayout {
    RankLayout::read(&stock_files_with(&[(RANK_FILE, rank)]), Platform::Linux)
}

fn value<'a>(l: &'a RankLayout, path: &str, key: &str) -> Option<&'a str> {
    l.control(path)?.value(key)
}

#[test]
fn the_main_menu_reads_the_rank_file_as_a_read_only_file() {
    let menu = SceneRule::get(Scene::MainMenu);
    assert_eq!(menu.read_only_files(), [RANK_FILE]);
    assert_eq!(menu.files(), ["resource/ui/mainmenuoverride.res"]);
}

#[test]
fn a_game_without_the_file_gets_the_built_in_stock_tree() {
    let stock = RankLayout::stock();
    assert_eq!(RankLayout::read(&stock_files(), Platform::Linux), stock);
    assert_eq!(value(&stock, NAME, "xpos"), Some("65"));
    assert_eq!(value(&stock, NAME, "font"), Some("HudFontSmallishBold"));
    assert_eq!(value(&stock, TRACK, "tall"), Some("f-2"));
    assert_eq!(value(&stock, FILL, "wide"), Some("f2"));
    assert_eq!(
        value(&stock, FILL, "fgcolor_override"),
        Some("CreditsGreen")
    );
    assert_eq!(
        value(&stock, "bgpanel/NAMELABEL", "XPOS"),
        Some("65"),
        "ignoring case"
    );
    assert!(stock.control("BGPanel/Nope").is_none());
}

#[test]
fn the_shipped_stock_shaped_file_reads_like_the_built_in_tree() {
    assert_eq!(read(Some(RANK_PANEL)), RankLayout::stock());
}

#[test]
fn the_huds_file_replaces_the_stock_tree() {
    let own = RANK_PANEL.replace("xpos 65 ypos 5", "xpos 10 ypos 5");
    let l = read(Some(&own));
    assert_eq!(value(&l, NAME, "xpos"), Some("10"));
    assert_ne!(l, RankLayout::stock());
}

#[test]
fn a_base_chain_is_merged_with_the_files_own_keys_first() {
    // The way budhud, cosmoshud, jofrehud, center-hud and peachhud ship it.
    let own = "#base \"pvprankpanel_base.res\"\n\"Resource/UI/PvPRankPanel.res\" { BGPanel { NameLabel { xpos 12 } } }";
    let files = stock_files_with(&[
        (RANK_FILE, Some(own)),
        ("resource/ui/pvprankpanel_base.res", Some(RANK_PANEL)),
    ]);
    let l = RankLayout::read(&files, Platform::Linux);
    assert_eq!(
        value(&l, NAME, "xpos"),
        Some("12"),
        "the file's own key wins"
    );
    assert_eq!(
        value(&l, NAME, "ypos"),
        Some("5"),
        "the base fills the rest"
    );
    assert_eq!(value(&l, TRACK, "tall"), Some("f-2"));
}

#[test]
fn an_empty_or_broken_file_falls_back_to_stock() {
    for text in [
        "",
        "   \n",
        "not keyvalues {{{",
        "\"Resource/UI/PvPRankPanel.res\"",
    ] {
        assert_eq!(read(Some(text)), RankLayout::stock(), "{text:?}");
    }
}

#[test]
fn a_file_that_lacks_a_block_has_no_such_control() {
    let partial = r#""Resource/UI/PvPRankPanel.res" { BGPanel { NameLabel { xpos 3 ypos 4 wide 50 tall 20 } } }"#;
    let l = read(Some(partial));
    assert_eq!(value(&l, NAME, "xpos"), Some("3"));
    assert!(l.control(TRACK).is_none() && l.control(FILL).is_none());
    assert_eq!(value(&l, NAME, "font"), None, "an unset key stays unset");
}

#[test]
fn the_first_copy_of_a_key_wins() {
    let dup = RANK_PANEL.replace("xpos 65 ypos 5", "xpos 1 xpos 2 ypos 5");
    assert_eq!(value(&read(Some(&dup)), NAME, "xpos"), Some("1"));
}

#[test]
fn the_model_carries_the_layout_and_a_bad_file_is_no_problem() {
    let own = RANK_PANEL.replace("xpos 65 ypos 5", "xpos 10 ypos 5");
    let files = stock_files_with(&[(RANK_FILE, Some(&own))]);
    let m = HudModel::load_scene(&files, Platform::Linux, Scene::MainMenu).unwrap();
    assert_eq!(value(m.rank_layout(), NAME, "xpos"), Some("10"));
    assert_eq!(
        value(m.with_scene(Scene::Hud).rank_layout(), NAME, "xpos"),
        Some("10")
    );
    assert_eq!(
        value(m.with_changes(&[]).model().rank_layout(), NAME, "xpos"),
        Some("10")
    );
    let bad = stock_files_with(&[(RANK_FILE, Some("{{{ garbage"))]);
    let m = HudModel::load_scene(&bad, Platform::Linux, Scene::MainMenu).unwrap();
    assert!(m.problems().is_empty());
    assert_eq!(m.rank_layout(), &RankLayout::stock());
}
