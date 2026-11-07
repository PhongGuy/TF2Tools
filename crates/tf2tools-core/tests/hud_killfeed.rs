//! The killfeed part and the two sample lines drawn from HudDeathNotice's settings.

use crate::common;

use common::hud_stock::{hudlayout_with_killfeed, stock_files, stock_files_with};
use tf2tools_core::{HudModel, PanelRule, PartId, PartKey, Platform, Rgba, Scene, Screen};

const HD: Screen = Screen::new(1920, 1080);
const KF: &str = "HudDeathNotice";

fn with_killfeed(block: &str) -> HudModel {
    let text = hudlayout_with_killfeed(&format!("  {block}\n"));
    let files = stock_files_with(&[("scripts/hudlayout.res", Some(text.as_str()))]);
    HudModel::load(&files, Platform::Linux).unwrap()
}

fn stock_killfeed() -> HudModel {
    with_killfeed(
        "HudDeathNotice { fieldName HudDeathNotice xpos r640 ypos 18 wide 628 tall 468 \
         LineHeight 16 LineSpacing 4 CornerRadius 3 RightJustify 1 TextFont Default \
         TeamBlue \"153 204 255 255\" TeamRed \"255 64 64 255\" IconColor TanLight \
         BaseBackgroundColor \"46 43 42 220\" LocalBackgroundColor \"245 229 196 200\" }",
    )
}

#[test]
fn the_killfeed_is_a_childless_hud_part() {
    let rule = PanelRule::get(PartId::Killfeed);
    assert_eq!(
        (rule.label(), rule.panel(), rule.parent()),
        ("Killfeed", KF, None)
    );
    assert!(!rule.has_children() && !rule.clickable());
    let m = stock_killfeed();
    let part = m.part(&PartKey::new(KF)).unwrap();
    assert!(part.is_root() && part.is_readable());
    assert!(
        m.parts().iter().all(|p| p.parent() != Some(part.key())),
        "no children"
    );
    assert_eq!(part.value("LineHeight").map(|l| l.text()), Some("16"));
}

#[test]
fn the_killfeed_box_never_takes_a_click() {
    let m = stock_killfeed();
    let l = m.layout(HD);
    let b = l.get(&PartKey::new(KF)).unwrap();
    assert_eq!((b.x(), b.y(), b.wide(), b.tall()), (480, 40, 1413, 1053));
    assert!(!b.clickable());
    assert_eq!(l.hit(1800, 100), None);
}

#[test]
fn two_sample_lines_follow_the_settings() {
    let m = stock_killfeed();
    let d = m.death_notices(HD).unwrap();
    assert!(d.right_justify());
    assert_eq!(
        (d.margin(), d.corner_radius(), d.font()),
        (30, 6, "Default")
    );
    assert_eq!(d.icon_colour(), Rgba::new(235, 226, 202, 255));
    let red = Rgba::new(255, 64, 64, 255);
    let blue = Rgba::new(153, 204, 255, 255);
    let rows: Vec<_> = d
        .lines()
        .iter()
        .map(|l| {
            (
                l.y(),
                l.tall(),
                l.icon(),
                l.killer(),
                l.killer_colour(),
                l.victim(),
                l.victim_colour(),
                l.background(),
            )
        })
        .collect();
    assert_eq!(
        rows,
        [
            (
                0,
                36,
                31,
                "You",
                red,
                "Blue Rocket",
                blue,
                Rgba::new(245, 229, 196, 200)
            ),
            (
                45,
                36,
                31,
                "Market Garden",
                blue,
                "Rocket Jumper",
                red,
                Rgba::new(46, 43, 42, 220)
            ),
        ]
    );
}

#[test]
fn each_line_names_its_killers_weapon_icon() {
    let d = with_killfeed(
        "HudDeathNotice { fieldName HudDeathNotice xpos r640 ypos 18 wide 628 tall 468 \
         MaxDeathNotices 3 LineHeight 16 LineSpacing 4 }",
    )
    .death_notices(HD)
    .unwrap();
    let names: Vec<(&str, &str)> = d
        .lines()
        .iter()
        .map(|l| (l.killer(), l.icon_name()))
        .collect();
    assert_eq!(
        names,
        [
            ("You", "dneg_tf_projectile_rocket"),
            ("Market Garden", "d_tf_projectile_rocket"),
            ("Sticky Situation", "d_tf_projectile_pipe"),
        ],
        "the local player's line takes the inverted icon"
    );
}

#[test]
fn absent_keys_take_the_engine_defaults() {
    let m = with_killfeed("HudDeathNotice { xpos r640 ypos 18 wide 628 tall 468 }");
    let d = m.death_notices(HD).unwrap();
    assert!(d.right_justify());
    assert_eq!(d.lines()[1].y(), 45);
    assert_eq!(d.lines()[0].killer_colour(), Rgba::new(255, 64, 64, 255));
    assert_eq!(d.icon_colour(), Rgba::new(255, 80, 0, 255));
    assert_eq!(d.font(), "Default");
}

#[test]
fn no_sample_without_a_shown_killfeed() {
    let stock = HudModel::load(&stock_files(), Platform::Linux).unwrap();
    assert!(
        stock.death_notices(HD).is_some(),
        "the stock killfeed shows"
    );
    let none = hudlayout_with_killfeed("");
    let files = stock_files_with(&[("scripts/hudlayout.res", Some(none.as_str()))]);
    assert!(
        HudModel::load(&files, Platform::Linux)
            .unwrap()
            .death_notices(HD)
            .is_none()
    );
    let m = stock_killfeed();
    let hide = m.hide_part(&PartKey::new(KF), 1).unwrap();
    assert!(m.with_changes(&hide).model().death_notices(HD).is_none());
    assert!(m.with_scene(Scene::Scoreboard).death_notices(HD).is_none());
}

#[test]
fn the_stock_killfeed_box_is_where_the_game_puts_it() {
    // Proportional at 1080 tall scales by 1080 / 480: r640 is 1920 - 1440, 628 wide is 1413,
    // 468 tall is 1053 and ypos 18 truncates to 40; the [$X360] lines are skipped.
    let m = HudModel::load(&stock_files(), Platform::Linux).unwrap();
    let l = m.layout(HD);
    let b = l.get(&PartKey::new(KF)).unwrap();
    assert_eq!((b.x(), b.y(), b.wide(), b.tall()), (480, 40, 1413, 1053));
}

use tf2tools_core::{Change, OptionChange, OptionValue, SampleSet, Team};

fn killfeed_rows(rows: &str) -> HudModel {
    with_killfeed(&format!(
        "HudDeathNotice {{ fieldName HudDeathNotice xpos r640 ypos 18 wide 628 tall 468 \
         LineHeight 16 LineSpacing 4 MaxDeathNotices {rows} }}"
    ))
}

#[test]
fn the_sample_shows_as_many_lines_as_the_killfeed_keeps() {
    let d = killfeed_rows("5").death_notices(HD).unwrap();
    let ys: Vec<i32> = d.lines().iter().map(|l| l.y()).collect();
    assert_eq!(ys, [0, 45, 90, 135, 180]);
    let casual = SampleSet::Casual;
    let line = &d.lines()[2];
    assert_eq!(
        (line.killer(), line.victim()),
        (
            casual.players(Team::Red)[2].name(),
            casual.players(Team::Blue)[2].name()
        )
    );
    assert_eq!(d.lines()[1].killer(), "Market Garden");
}

#[test]
fn the_line_count_is_kept_in_range() {
    assert_eq!(
        killfeed_rows("0").death_notices(HD).unwrap().lines().len(),
        0
    );
    assert_eq!(
        killfeed_rows("99").death_notices(HD).unwrap().lines().len(),
        20
    );
    assert_eq!(
        killfeed_rows("-3").death_notices(HD).unwrap().lines().len(),
        0
    );
}

#[test]
fn the_rows_option_redraws_the_sample() {
    let m = killfeed_rows("4");
    let option = Change::option(
        1,
        OptionChange::new(OptionChange::BUILTIN, "killfeed_rows", OptionValue::Int(6)),
    );
    let d = m.with_changes(&[option]).model().death_notices(HD).unwrap();
    assert_eq!(d.lines().len(), 6);
}
