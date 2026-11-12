//! The rank panel's pieces follow the HUD's own pvprankpanel.res.

use crate::common;

use common::hud_stock::{RANK_PANEL, stock_files_with};
use tf2tools_core::{HudModel, PartKey, Platform, RankPieces, Rect, Scene, Screen, rank_pieces};

const MENU_FILE: &str = "resource/ui/mainmenuoverride.res";
const RANK_FILE: &str = "resource/ui/pvprankpanel.res";
const MENU: &str = r##""Resource/UI/MainMenuOverride.res"
{
  MainMenuOverride { fieldName MainMenuOverride xpos 0 ypos 0 wide f0 tall 480 }
  RankPanel { ControlName CPvPRankPanel xpos c-350 ypos 72 wide 320 tall 100 visible 0 show_model 0 show_type 1 }
}
"##;
const SD: Screen = Screen::new(640, 480);
const X2: Screen = Screen::new(1280, 960);

fn model(rank: Option<&str>) -> HudModel {
    let files = stock_files_with(&[(MENU_FILE, Some(MENU)), (RANK_FILE, rank)]);
    HudModel::load_scene(&files, Platform::Linux, Scene::MainMenu).unwrap()
}

fn pieces(rank: Option<&str>, wide: i32, tall: i32, screen: Screen) -> RankPieces {
    let m = model(rank);
    let part = m.part(&PartKey::new("MainMenuOverride/RankPanel")).unwrap();
    rank_pieces(&m, part, wide, tall, screen)
}

/// RANK_PANEL with `from` replaced by `to`; `from` must be there.
fn with(from: &str, to: &str) -> String {
    assert!(RANK_PANEL.contains(from), "{from}");
    RANK_PANEL.replace(from, to)
}

/// The labels' (field, text, visible box) in the order the pieces report them.
fn labels(p: &RankPieces) -> Vec<(&str, &str, Rect)> {
    (p.labels().iter())
        .map(|l| (l.field(), l.text(), l.shown()))
        .collect()
}

#[test]
fn stock_places_the_name_and_the_track() {
    // Built in and from a file alike; the track is the container (f-2 clipped to it).
    for rank in [None, Some(RANK_PANEL)] {
        let p = pieces(rank, 320, 100, SD);
        assert_eq!(p.name(), Some(Rect::new(125, 25, 195, 20)));
        assert_eq!(
            p.name_box(),
            Some(Rect::new(125, 25, 260, 20)),
            "the label reaches past BGPanel"
        );
        assert_eq!(p.bar(), Some(Rect::new(126, 60, 188, 7)));
        assert_eq!(p.name_font(), Some("HudFontSmallishBold"));
        assert_eq!(p.name_colour(), Some("TanLight"));
        assert_eq!(p.name_alignment(), "north-west");
        assert!(!p.name_all_caps());
        assert_eq!(p.track_colour(), Some("20 20 20 180"));
    }
}

#[test]
fn three_of_the_ten_segments_are_filled_in_the_fill_colour() {
    // The fill control sits a unit inside the track; its own ten segments, three of them shown.
    for (screen, wide, tall, gap) in [(SD, 320, 100, 1), (X2, 640, 200, 2)] {
        let p = pieces(Some(RANK_PANEL), wide, tall, screen);
        let f = p.fill_segments();
        assert_eq!(f.len(), 3, "{f:?}");
        let bar = p.bar().unwrap();
        assert!(f[0].x > bar.x && f[0].y > bar.y && f[0].tall < bar.tall);
        for (a, b) in f.iter().zip(&f[1..]) {
            assert_eq!(b.x - (a.x + a.wide), gap, "{f:?}");
        }
        assert_eq!(p.fill_colour(), Some("CreditsGreen"));
    }
    assert_eq!(
        pieces(Some(RANK_PANEL), 320, 100, SD).fill_segments()[0],
        Rect::new(127, 61, 17, 5)
    );
}

#[test]
fn the_sample_is_a_level_one_casual_rank_with_its_xp() {
    // As the game shows a new player: the type and level under the name, and the xp so far
    // and needed under the bar's two ends.
    for rank in [None, Some(RANK_PANEL)] {
        let p = pieces(rank, 320, 100, SD);
        assert_eq!(
            labels(&p),
            [
                ("DescLine1", "Casual", Rect::new(125, 39, 195, 20)),
                ("DescLine2", "Level: 1", Rect::new(125, 49, 195, 20)),
                ("CurrentXPLabel", "225 xp", Rect::new(126, 57, 100, 20)),
                ("NextLevelXPLabel", "750 xp", Rect::new(215, 57, 100, 20)),
            ]
        );
        let xp = &p.labels()[2];
        assert_eq!(xp.font(), Some("ItemFontAttribSmall"));
        assert_eq!(xp.colour(), Some("TanLight"));
        assert_eq!(xp.alignment(), "south-west");
        assert_eq!(p.labels()[3].alignment(), "south-east");
        assert_eq!(p.labels()[0].font(), Some("HudFontSmallestBold"));
        assert!(!xp.all_caps());
    }
    assert_ne!(RankPieces::SAMPLE_NAME, "Level 42");
}

#[test]
fn the_bar_is_ten_segments_across_the_track() {
    for (screen, wide, tall, gap) in [(SD, 320, 100, 1), (X2, 640, 200, 2)] {
        let p = pieces(Some(RANK_PANEL), wide, tall, screen);
        let bar = p.bar().unwrap();
        let s = p.segments();
        assert_eq!(s.len(), 10);
        assert_eq!(s[0].x, bar.x);
        assert_eq!(s[9].x + s[9].wide, bar.x + bar.wide);
        for (a, b) in s.iter().zip(&s[1..]) {
            assert_eq!(b.x - (a.x + a.wide), gap, "{s:?}");
            assert!((a.wide - b.wide).abs() <= gap + 1, "{s:?}");
        }
        assert!(s.iter().all(|r| r.y == bar.y && r.tall == bar.tall));
    }
}

#[test]
fn the_pieces_scale_with_the_screen() {
    let p = pieces(Some(RANK_PANEL), 640, 200, X2);
    assert_eq!(p.name(), Some(Rect::new(250, 50, 390, 40)));
    assert_eq!(p.bar(), Some(Rect::new(251, 120, 376, 14)));
}

#[test]
fn a_moved_and_restyled_name_is_reported_as_the_file_sets_it() {
    let own = with(
        "ControlName Label xpos 65 ypos 5 wide f0 tall 20 visible 1 font HudFontSmallishBold fgcolor_override TanLight textAlignment north-west",
        "ControlName CExLabel xpos 0 ypos 0 wide 125 tall 20 font aRegular14 fgcolor White textAlignment center allcaps 1",
    );
    let p = pieces(Some(&own), 320, 100, SD);
    assert_eq!(p.name(), Some(Rect::new(60, 20, 125, 20)));
    assert_eq!(p.name_box(), p.name());
    assert_eq!(p.name_font(), Some("aRegular14"));
    assert_eq!(
        p.name_colour(),
        Some("White"),
        "fgcolor when there is no override"
    );
    assert_eq!(p.name_alignment(), "center");
    assert!(p.name_all_caps());
    assert_eq!(
        p.bar(),
        Some(Rect::new(126, 60, 188, 7)),
        "the bar is unmoved"
    );
}

#[test]
fn a_hidden_name_leaves_the_bar_alone() {
    for own in [
        with("tall 20 visible 1 font", "tall 20 visible 0 font"),
        with("xpos 65 ypos 5", "xpos 9999 ypos 5"),
        with("xpos 65 ypos 5 wide f0", "xpos 65 ypos 5 wide 0"),
    ] {
        let p = pieces(Some(&own), 320, 100, SD);
        assert_eq!((p.name(), p.name_box()), (None, None), "{own}");
        assert_eq!(p.bar(), Some(Rect::new(126, 60, 188, 7)));
    }
}

#[test]
fn a_hidden_bgpanel_hides_everything_inside_it() {
    let own = with("tall f40 visible 1", "tall f40 visible 0");
    let p = pieces(Some(&own), 320, 100, SD);
    assert_eq!((p.name(), p.bar()), (None, None));
    assert!(p.segments().is_empty() && p.labels().is_empty());
}

#[test]
fn a_hidden_track_draws_no_segments_but_keeps_the_xp_text() {
    // Seven corpus HUDs do this with wide 0.
    let own = with("wide f0 tall f-2", "wide 0 tall f-2");
    let p = pieces(Some(&own), 320, 100, SD);
    assert_eq!(p.bar(), None);
    assert!(p.segments().is_empty());
    assert_eq!(p.fill_segments().len(), 3, "the fill stays");
    assert_eq!(p.labels().len(), 4);
}

#[test]
fn a_hidden_xp_label_is_left_out() {
    let own = with(
        "NextLevelXPLabel { ControlName Label",
        "NextLevelXPLabel { visible 0 ControlName Label",
    );
    let p = pieces(Some(&own), 320, 100, SD);
    let fields: Vec<_> = p.labels().iter().map(|l| l.field()).collect();
    assert_eq!(fields, ["DescLine1", "DescLine2", "CurrentXPLabel"]);
}

#[test]
fn a_file_missing_the_bar_blocks_draws_no_bar() {
    let partial = r#""Resource/UI/PvPRankPanel.res" { BGPanel { xpos rs1 ypos 20 wide 260 tall f40 NameLabel { xpos 65 ypos 5 wide f0 tall 20 } } }"#;
    let p = pieces(Some(partial), 320, 100, SD);
    assert_eq!(p.name(), Some(Rect::new(125, 25, 195, 20)));
    assert_eq!(p.bar(), None);
    assert!(p.segments().is_empty() && p.labels().is_empty());
    assert_eq!(p.name_font(), None);
}

#[test]
fn a_broken_file_draws_stock() {
    let p = pieces(Some("{{{ not a rank file"), 320, 100, SD);
    assert_eq!(p.bar(), Some(Rect::new(126, 60, 188, 7)));
    assert_eq!(p.segments().len(), 10);
}

#[test]
fn extreme_boxes_never_overflow() {
    let tiny = pieces(None, 10, 10, SD);
    assert_eq!(
        (tiny.name(), tiny.bar(), tiny.segments().len()),
        (None, None, 0),
        "BGPanel has no height"
    );
    // RankPanel has its medal off, so the medal check uses a panel with it on.
    let menu = MENU.replace("visible 0 show_model 0", "visible 0");
    let files = stock_files_with(&[(MENU_FILE, Some(&menu))]);
    let m = HudModel::load_scene(&files, Platform::Linux, Scene::MainMenu).unwrap();
    let part = m.part(&PartKey::new("MainMenuOverride/RankPanel")).unwrap();
    let screen = Screen::new(7680, 4320);
    assert!(
        rank_pieces(&m, part, i32::MAX, i32::MAX, screen)
            .medal()
            .is_some()
    );
}

#[test]
fn only_a_cexlabel_reads_fgcolor() {
    // A plain Label has no fgcolor key in the game, so the unset colour stays unset.
    let own = with("TanLight", "TanLight").replace("fgcolor_override TanLight", "fgcolor White");
    let p = pieces(Some(&own), 320, 100, SD);
    assert_eq!(p.name_colour(), None);
    let ex = own.replace("ControlName Label", "ControlName CExLabel");
    assert_eq!(pieces(Some(&ex), 320, 100, SD).name_colour(), Some("White"));
}

#[test]
fn the_medal_stops_at_the_names_whole_box_not_its_clipped_part() {
    // BGPanel starts at x 200 and the label 40 units into the panel from the left of it, so
    // the box starts at 160 but only shows from 200. The 8 px medal at 156 yields to the box.
    let own =
        with("xpos rs1 ypos 20", "xpos 200 ypos 20").replace("xpos 65 ypos 5", "xpos -40 ypos 20");
    let menu = MENU.replace("visible 0 show_model 0 show_type 1", "show_type 1");
    let files = stock_files_with(&[(MENU_FILE, Some(&menu)), (RANK_FILE, Some(&own))]);
    let m = HudModel::load_scene(&files, Platform::Linux, Scene::MainMenu).unwrap();
    let key = PartKey::new("MainMenuOverride/RankPanel");
    let p = rank_pieces(&m, m.part(&key).unwrap(), 320, 100, SD);
    assert_eq!(p.name_box(), Some(Rect::new(160, 40, 260, 20)));
    assert_eq!(p.name(), Some(Rect::new(200, 40, 220, 20)));
    let layout = m.layout(SD);
    let Rect { x, wide, .. } = tf2tools_core::rank_medal(&m, &layout, &key, SD).unwrap();
    assert_eq!(x + wide, 156, "ends a gap short of the box at 160");
}
