//! The Main menu tab's rank pieces follow the HUD's own pvprankpanel.res.

use crate::support;

use gpui_kit::{Bounds, Entity, Pixels, TestAppContext};
use tf2tools_app::ids;
use tf2tools_app::pages::hud_maker::HudMakerPage;
use tf2tools_app::services::Tf2State;
use tf2tools_core::{PartKey, Scene, Tf2Install};

const MENU: &str = r#""Resource/UI/MainMenuOverride.res"
{
  MainMenuOverride { ControlName EditablePanel xpos 0 ypos 0 wide f0 tall f0 }
  RankPanel { ControlName CPvPRankPanel xpos c-350 ypos 72 zpos -52 wide 320 tall 100 visible 0 proportionaltoparent 1 show_model 0 show_type 1 }
}
"#;

fn rank_file(name: &str, track: &str) -> String {
    format!(
        r#""Resource/UI/PvPRankPanel.res" {{ BGPanel {{ xpos rs1 ypos 20 wide 260 tall f40
  NameLabel {{ xpos 65 ypos 5 wide f0 tall 20 {name} font HudFontSmallishBold fgcolor_override TanLight textAlignment north-west }}
  StatsContainer {{ xpos rs1-5 ypos 0 wide f70 tall f0
    XPBar {{ xpos cs-0.5 ypos rs1-3 wide p1 tall 30
      CurrentXPLabel {{ xpos 0 ypos rs1 wide 100 tall 20 textAlignment south-west }}
      NextLevelXPLabel {{ xpos rs1 ypos rs1 wide 100 tall 20 font HudFontSmall fgcolor_override "1 2 3 255" textAlignment south-east }}
      ProgressBarsContainer {{ xpos 0 ypos rs1-10 wide p1 tall 7
        ProgressBar {{ xpos 0 ypos cs-0.5 {track} fgcolor_override "20 20 20 180" }}
        ContinuousProgressBar {{ xpos cs-0.5 ypos cs-0.5 wide f2 tall f2 fgcolor_override CreditsGreen }} }} }} }} }} }}"#
    )
}

fn open_rank(
    cx: &mut TestAppContext,
    rank: &str,
) -> (support::Env, support::ShellWindow, Entity<HudMakerPage>) {
    let env = support::env();
    support::seed_stock_game_with(
        &env.game,
        &[
            ("resource/ui/mainmenuoverride.res", Some(MENU)),
            ("resource/ui/pvprankpanel.res", Some(rank)),
        ],
    );
    let s = support::shell_window(
        cx,
        &env,
        Tf2State::Found(Tf2Install::from_path(&env.game).unwrap()),
    );
    support::click_nav(cx, s.win, tf2tools_app::shell::route::Page::HudMaker);
    let page = cx.update(|cx| s.shell.read(cx).hud_maker_page()).unwrap();
    support::wait_real(cx, "the HUD to load", |cx| {
        cx.update(|cx| page.read(cx).is_loaded())
    });
    support::click_scene(cx, s.win, Scene::MainMenu);
    (env, s, page)
}

fn bounds(
    cx: &mut TestAppContext,
    s: &support::ShellWindow,
    id: impl Into<gpui_kit::ElementId>,
) -> Option<Bounds<Pixels>> {
    support::bounds_of(cx, s.win, id.into())
}

#[gpui_kit::test]
fn a_hud_that_hides_the_name_still_draws_the_bar(cx: &mut TestAppContext) {
    let key = PartKey::new("MainMenuOverride/RankPanel");
    let (_env, s, _page) = open_rank(cx, &rank_file("visible 0", "wide f0 tall f-2"));
    assert!(!support::present(cx, s.win, ids::hud_maker_rank_name(&key)));
    assert!(support::present(cx, s.win, ids::hud_maker_rank_bar(&key)));
    assert!(support::present(
        cx,
        s.win,
        ids::hud_maker_rank_segment(&key, 9)
    ));
}

#[gpui_kit::test]
fn a_hud_that_hides_the_track_keeps_the_xp_text(cx: &mut TestAppContext) {
    let key = PartKey::new("MainMenuOverride/RankPanel");
    let (_env, s, _page) = open_rank(cx, &rank_file("visible 1", "wide 0 tall f-2"));
    assert!(!support::present(cx, s.win, ids::hud_maker_rank_bar(&key)));
    assert!(!support::present(
        cx,
        s.win,
        ids::hud_maker_rank_segment(&key, 0)
    ));
    let xp = ids::hud_maker_rank_label(&key, "NextLevelXPLabel");
    assert_eq!(
        support::label_of(cx, s.win, xp.clone()).as_deref(),
        Some("750 xp")
    );
    let xp = bounds(cx, &s, xp).expect("the xp text stays");
    assert!(f32::from(xp.size.width) > 0. && f32::from(xp.size.height) > 0.);
}

#[gpui_kit::test]
fn the_name_follows_the_files_box_and_case(cx: &mut TestAppContext) {
    let key = PartKey::new("MainMenuOverride/RankPanel");
    let (_env, s, _page) = open_rank(cx, &rank_file("allcaps 1", "wide f0 tall f-2"));
    assert_eq!(
        support::label_of(cx, s.win, ids::hud_maker_rank_name(&key)).as_deref(),
        Some("PLAYER")
    );
    // The name starts 65 units in and ends at the level area's right edge; the bar starts
    // at the same x, so they share a left edge.
    let name = bounds(cx, &s, ids::hud_maker_rank_name(&key)).unwrap();
    let bar = bounds(cx, &s, ids::hud_maker_rank_bar(&key)).unwrap();
    assert!((f32::from(name.left()) - f32::from(bar.left())).abs() < 1.);
    assert!(f32::from(name.right()) >= f32::from(bar.right()) - 1.);
}

/// The paint the canvas gives a rank file's name, track and fill, read on the stock scheme.
fn paint_of(rank: &str) -> tf2tools_app::pages::hud_maker::rank_paint::RankPaint {
    use support::hud_stock::stock_files_with;
    use tf2tools_core::{HudModel, Platform, Screen, rank_pieces};
    let files = stock_files_with(&[
        ("resource/ui/mainmenuoverride.res", Some(MENU)),
        ("resource/ui/pvprankpanel.res", Some(rank)),
    ]);
    let m = HudModel::load_scene(&files, Platform::Linux, Scene::MainMenu).unwrap();
    let part = m.part(&PartKey::new("MainMenuOverride/RankPanel")).unwrap();
    let pieces = rank_pieces(&m, part, 320, 100, Screen::new(640, 480));
    tf2tools_app::pages::hud_maker::rank_paint::rank_paint(&pieces, m.scheme(), 480)
}

#[test]
fn the_name_is_drawn_in_the_files_font_colour_alignment_and_case() {
    let rank = rank_file("allcaps 1", "wide f0 tall f-2")
        .replace("font HudFontSmallishBold", "font HudFontSmall")
        .replace(
            "fgcolor_override TanLight",
            "fgcolor_override \"10 20 30 255\"",
        )
        .replace("north-west", "south-east");
    let p = paint_of(&rank);
    assert_eq!(p.text(), "PLAYER");
    assert_eq!(p.font().map(|f| f.family()), Some("TF2 Secondary"));
    assert_eq!(p.colour(), Some(tf2tools_core::Rgba::new(10, 20, 30, 255)));
    assert_eq!(p.alignment(), (2, 2), "east, south");
}

#[test]
fn an_unset_name_draws_the_canvas_defaults() {
    let rank = rank_file("visible 1", "wide f0 tall f-2").replace(
        "font HudFontSmallishBold fgcolor_override TanLight textAlignment north-west",
        "",
    );
    let p = paint_of(&rank);
    assert_eq!(p.text(), "Player");
    assert!(p.font().is_none());
    assert_eq!(p.colour(), None, "the canvas then draws white");
    assert_eq!(p.alignment(), (0, 1), "west, centred");
}

#[test]
fn the_track_takes_the_files_colour_else_the_stock_one() {
    let rank = rank_file("visible 1", "wide f0 tall f-2").replace(
        "fgcolor_override \"20 20 20 180\"",
        "fgcolor_override \"1 2 3 4\"",
    );
    let p = paint_of(&rank);
    assert_eq!(p.track(), tf2tools_core::Rgba::new(1, 2, 3, 4));
    let bare =
        rank_file("visible 1", "wide f0 tall f-2").replace("fgcolor_override \"20 20 20 180\"", "");
    let p = paint_of(&bare);
    assert_eq!(p.track(), tf2tools_core::Rgba::new(20, 20, 20, 180));
}

#[test]
fn the_xp_text_is_drawn_in_its_labels_font_colour_and_alignment() {
    use support::hud_stock::stock_files_with;
    use tf2tools_app::pages::hud_maker::rank_paint::rank_text;
    use tf2tools_core::{HudModel, Platform, Screen, rank_pieces};
    let rank = rank_file("visible 1", "wide f0 tall f-2");
    let files = stock_files_with(&[
        ("resource/ui/mainmenuoverride.res", Some(MENU)),
        ("resource/ui/pvprankpanel.res", Some(&rank)),
    ]);
    let m = HudModel::load_scene(&files, Platform::Linux, Scene::MainMenu).unwrap();
    let part = m.part(&PartKey::new("MainMenuOverride/RankPanel")).unwrap();
    let pieces = rank_pieces(&m, part, 320, 100, Screen::new(640, 480));
    let next = (pieces.labels().iter())
        .find(|l| l.field() == "NextLevelXPLabel")
        .unwrap();
    let t = rank_text(next, m.scheme(), 480);
    assert_eq!(t.text(), "750 xp");
    assert_eq!(t.font().map(|f| f.family()), Some("TF2 Secondary"));
    assert_eq!(t.colour(), Some(tf2tools_core::Rgba::new(1, 2, 3, 255)));
    assert_eq!(t.alignment(), (2, 2), "east, south");
}
