//! The HUD Maker's checking tools, headless: every screen at once, the backdrop, hard-to-read
//! flags and colour-blind views.

use crate::support;

use std::path::PathBuf;
use std::sync::Arc;

use gpui_kit::{AppContext as _, Entity, TestAppContext};
use support::hud_stock::{HEALTH, label, with_blocks};
use tf2tools_app::ids;
use tf2tools_app::pages::hud_maker::HudMakerPage;
use tf2tools_app::picker::ScriptedPicker;
use tf2tools_app::services::Tf2State;
use tf2tools_app::shell::route::Page;
use tf2tools_core::{PartKey, Scene, Screen, Tf2Install};

const UNDER: &str = "HudPlayerStatus/HudPlayerHealth/";
const FIVE_FOUR: Screen = Screen::new(1280, 1024);
const HD: Screen = Screen::new(1920, 1080);

fn k(name: &str) -> PartKey {
    PartKey::new(format!("{UNDER}{name}"))
}

/// Two labels that meet only at 1280x1024.
fn meeting() -> String {
    label("Right", "r200", "100", &[("labelText", "R")])
        + &label("Left", "400", "100", &[("labelText", "L")])
}

/// The same two labels apart; moving Left 200 px right makes them meet at 1280x1024 only.
fn apart() -> String {
    label("Right", "r200", "100", &[("labelText", "R")])
        + &label("Left", "312", "100", &[("labelText", "L")])
}

/// Moves Left 200 px right at 1920x1080, as a player's drag would.
fn bring_left_over(cx: &mut TestAppContext, page: &Entity<HudMakerPage>) {
    cx.update(|cx| {
        page.update(cx, |p, cx| {
            let e = p.editor_mut().unwrap();
            e.select(Some(k("Left")));
            e.move_selected(200, 0).unwrap();
            cx.notify();
        })
    });
}

fn change_count(cx: &mut TestAppContext, page: &Entity<HudMakerPage>) -> usize {
    cx.update(|cx| page.read(cx).editor().unwrap().changes().len())
}

/// Opens the HUD Maker on the stock game with `overrides` and binary `extra` files, answering
/// file picks with `picks` in order; `before` runs on the services before the page opens.
fn open_with(
    cx: &mut TestAppContext,
    overrides: &[(&str, Option<&str>)],
    extra: &[(&str, &[u8])],
    picks: Vec<PathBuf>,
    before: impl FnOnce(&support::ShellWindow),
) -> (
    support::Env,
    support::ShellWindow,
    Entity<HudMakerPage>,
    Arc<ScriptedPicker>,
) {
    let env = support::env();
    support::seed_stock_game_with_bytes(&env.game, overrides, extra);
    let state = Tf2State::Found(Tf2Install::from_path(&env.game).unwrap());
    let picker = Arc::new(ScriptedPicker::new(picks));
    let s = support::shell_window_with(cx, &env, state, Arc::clone(&picker), support::no_steam());
    before(&s);
    support::click_nav(cx, s.win, Page::HudMaker);
    let page = cx
        .update(|cx| s.shell.read(cx).hud_maker_page())
        .expect("a HUD Maker page");
    support::wait_real(cx, "the HUD to load", |cx| {
        cx.update(|cx| page.read(cx).is_loaded())
    });
    support::settle(cx, s.win);
    (env, s, page, picker)
}

/// The stock HUD with `blocks` inside HudPlayerHealth.
fn opened(
    cx: &mut TestAppContext,
    blocks: &str,
    picks: Vec<PathBuf>,
) -> (
    support::Env,
    support::ShellWindow,
    Entity<HudMakerPage>,
    Arc<ScriptedPicker>,
) {
    let health = with_blocks(HEALTH, blocks);
    open_with(
        cx,
        &[("resource/ui/hudplayerhealth.res", Some(&health))],
        &[],
        picks,
        |_| {},
    )
}

/// Opens the More menu and presses `id` in it.
fn more(cx: &mut TestAppContext, s: &support::ShellWindow, id: impl Into<gpui_kit::ElementId>) {
    support::click_id(cx, s.win, ids::HUD_MAKER_MORE);
    support::click_id(cx, s.win, id);
}

/// The HUD has no changes and nothing to save.
fn untouched(cx: &mut TestAppContext, page: &Entity<HudMakerPage>) -> bool {
    cx.update(|cx| {
        let e = page.read(cx).editor().unwrap();
        e.changes().is_empty() && !e.is_dirty() && !e.can_undo()
    })
}

#[gpui_kit::test]
fn every_screen_lays_out_six_shapes_and_flags_the_meeting_parts(cx: &mut TestAppContext) {
    let (_env, s, page, _) = opened(cx, &apart(), Vec::new());
    bring_left_over(cx, &page);
    let changes = change_count(cx, &page);
    more(cx, &s, ids::HUD_MAKER_EVERY_SCREEN);
    assert!(support::present(cx, s.win, ids::HUD_MAKER_SCREENS));
    assert!(!support::present(cx, s.win, ids::HUD_MAKER_CANVAS));
    for shape in Screen::PRESETS {
        assert!(support::present(
            cx,
            s.win,
            ids::hud_maker_screen_tile(shape)
        ));
    }
    assert_eq!(
        support::label_of(cx, s.win, ids::hud_maker_screen_tile(HD)).as_deref(),
        Some("16:9, 1920x1080: No problems")
    );
    assert_eq!(
        support::label_of(cx, s.win, ids::hud_maker_screen_tile(FIVE_FOUR)).as_deref(),
        Some("5:4, 1280x1024: 2 parts have problems")
    );
    assert_eq!(
        support::label_of(cx, s.win, ids::hud_maker_flag(FIVE_FOUR, &k("Right"))).as_deref(),
        Some("Right overlaps Left")
    );
    assert_eq!(
        support::label_of(cx, s.win, ids::hud_maker_flag(FIVE_FOUR, &k("Left"))).as_deref(),
        Some("Left overlaps Right")
    );
    assert!(!support::present(
        cx,
        s.win,
        ids::hud_maker_flag(HD, &k("Right"))
    ));
    // Frames come and go; the six layouts were made once.
    for _ in 0..3 {
        support::next_repaint(cx);
        support::settle(cx, s.win);
    }
    assert_eq!(cx.update(|cx| page.read(cx).checks().screen_runs()), 1);
    // The view records nothing of its own.
    assert_eq!(change_count(cx, &page), changes);
}

#[gpui_kit::test]
fn an_overlap_the_hud_shipped_is_not_a_problem(cx: &mut TestAppContext) {
    let (_env, s, page, _) = opened(cx, &meeting(), Vec::new());
    more(cx, &s, ids::HUD_MAKER_EVERY_SCREEN);
    assert_eq!(
        support::label_of(cx, s.win, ids::hud_maker_screen_tile(FIVE_FOUR)).as_deref(),
        Some("5:4, 1280x1024: No problems")
    );
    assert!(untouched(cx, &page));
}

#[gpui_kit::test]
fn a_pile_of_labels_is_too_crowded_to_list(cx: &mut TestAppContext) {
    // A hundred labels on one spot meet in 4950 pairs, past what a tile lists.
    let pile: String = (0..100)
        .map(|i| label(&format!("P{i}"), "10", "10", &[("labelText", "P")]))
        .collect();
    let (_env, s, _page, _) = opened(cx, &pile, Vec::new());
    more(cx, &s, ids::HUD_MAKER_EVERY_SCREEN);
    assert_eq!(
        support::label_of(cx, s.win, ids::hud_maker_screen_tile(HD)).as_deref(),
        Some("16:9, 1920x1080: Too many overlaps to list here")
    );
}

#[gpui_kit::test]
fn the_toolbar_and_panels_come_back_where_they_were(cx: &mut TestAppContext) {
    let (_env, s, _page, _) = opened(cx, "", Vec::new());
    let places = |cx: &mut TestAppContext| {
        [
            ids::HUD_MAKER_TOOLBAR,
            ids::HUD_MAKER_PANEL,
            ids::HUD_MAKER_PARTS_PANEL,
        ]
        .map(|id| support::bounds_of(cx, s.win, id))
    };
    let before = places(cx);
    assert!(before.iter().all(Option::is_some), "{before:?}");
    more(cx, &s, ids::HUD_MAKER_EVERY_SCREEN);
    support::click_id(cx, s.win, ids::HUD_MAKER_SCREENS_BACK);
    for _ in 0..3 {
        support::next_repaint(cx);
        support::settle(cx, s.win);
    }
    assert_eq!(places(cx), before);
}

#[gpui_kit::test]
fn an_edit_while_the_view_shows_updates_the_tiles_once(cx: &mut TestAppContext) {
    let (_env, s, page, _) = opened(cx, &apart(), Vec::new());
    more(cx, &s, ids::HUD_MAKER_EVERY_SCREEN);
    bring_left_over(cx, &page);
    assert_eq!(
        support::label_of(cx, s.win, ids::hud_maker_screen_tile(FIVE_FOUR)).as_deref(),
        Some("5:4, 1280x1024: 2 parts have problems")
    );
    assert_eq!(cx.update(|cx| page.read(cx).checks().screen_runs()), 2);
}

#[gpui_kit::test]
fn a_tile_opens_its_shape(cx: &mut TestAppContext) {
    let (_env, s, page, _) = opened(cx, &meeting(), Vec::new());
    more(cx, &s, ids::HUD_MAKER_EVERY_SCREEN);
    support::click_id(cx, s.win, ids::hud_maker_screen_tile(FIVE_FOUR));
    assert!(!support::present(cx, s.win, ids::HUD_MAKER_SCREENS));
    assert!(support::present(cx, s.win, ids::HUD_MAKER_CANVAS));
    let (on, shape) = cx.update(|cx| {
        let p = page.read(cx);
        (p.every_screen(), p.editor().unwrap().screen())
    });
    assert_eq!((on, shape), (false, FIVE_FOUR));
}

#[gpui_kit::test]
fn back_and_escape_leave_the_view_at_the_same_shape(cx: &mut TestAppContext) {
    let (_env, s, page, _) = opened(cx, &meeting(), Vec::new());
    more(cx, &s, ids::HUD_MAKER_EVERY_SCREEN);
    assert_eq!(
        support::label_of(cx, s.win, ids::HUD_MAKER_SCREENS_BACK).as_deref(),
        Some("Back to one screen")
    );
    support::click_id(cx, s.win, ids::HUD_MAKER_SCREENS_BACK);
    assert!(support::present(cx, s.win, ids::HUD_MAKER_CANVAS));
    more(cx, &s, ids::HUD_MAKER_EVERY_SCREEN);
    support::press(cx, s.win, "escape");
    assert!(support::present(cx, s.win, ids::HUD_MAKER_CANVAS));
    let shape = cx.update(|cx| page.read(cx).editor().unwrap().screen());
    assert_eq!(shape, HD);
}

#[gpui_kit::test]
fn the_view_takes_the_whole_area_with_back_at_the_top_left(cx: &mut TestAppContext) {
    let (_env, s, _page, _) = opened(cx, "", Vec::new());
    assert!(support::present(cx, s.win, ids::HUD_MAKER_TOOLBAR));
    more(cx, &s, ids::HUD_MAKER_EVERY_SCREEN);
    for gone in [
        ids::HUD_MAKER_TOOLBAR,
        ids::HUD_MAKER_PANEL,
        ids::HUD_MAKER_PARTS,
    ] {
        assert!(!support::present(cx, s.win, gone), "{gone}");
    }
    let area = support::bounds_of(cx, s.win, ids::HUD_MAKER_CANVAS_AREA).unwrap();
    let back = support::bounds_of(cx, s.win, ids::HUD_MAKER_SCREENS_BACK).unwrap();
    let grid = support::bounds_of(cx, s.win, ids::HUD_MAKER_SCREENS).unwrap();
    // The back button sits in the area's top-left quarter, above the grid.
    assert!(
        back.left() < area.center().x && back.top() < area.center().y,
        "{back:?} in {area:?}"
    );
    assert!(back.bottom() <= grid.top(), "{back:?} over {grid:?}");
    support::click_id(cx, s.win, ids::HUD_MAKER_SCREENS_BACK);
    assert!(support::present(cx, s.win, ids::HUD_MAKER_TOOLBAR));
}

use std::path::Path;
use tf2tools_app::picker::PickKind;
use tf2tools_app::services::Services;

/// A `wide` x `tall` PNG of one colour, written to `dir`.
fn png(dir: &Path, name: &str, wide: u32, tall: u32, rgba: [u8; 4]) -> PathBuf {
    let path = dir.join(name);
    image::RgbaImage::from_pixel(wide, tall, image::Rgba(rgba))
        .save(&path)
        .unwrap();
    path
}

/// A `wide` x `tall` JPEG of one colour, written to `dir`.
fn jpeg(dir: &Path, name: &str, wide: u32, tall: u32, rgb: [u8; 3]) -> PathBuf {
    let path = dir.join(name);
    image::RgbImage::from_pixel(wide, tall, image::Rgb(rgb))
        .save(&path)
        .unwrap();
    path
}

fn saved_backdrop(cx: &mut TestAppContext) -> Option<PathBuf> {
    cx.update(|cx| {
        let settings = cx.global::<Services>().db.settings().unwrap();
        settings.hud_maker_backdrop().map(Path::to_path_buf)
    })
}

fn has_backdrop(cx: &mut TestAppContext, page: &Entity<HudMakerPage>) -> bool {
    cx.update(|cx| page.read(cx).backdrop().is_some())
}

/// Picks the next scripted file through the More menu and waits for it to show.
fn pick_backdrop(cx: &mut TestAppContext, s: &support::ShellWindow, page: &Entity<HudMakerPage>) {
    let before = cx.update(|cx| page.read(cx).backdrop().cloned());
    more(cx, s, ids::HUD_MAKER_BACKDROP);
    support::wait_real(cx, "the backdrop", |cx| {
        cx.update(|cx| page.read(cx).backdrop().cloned()) != before
    });
    support::settle(cx, s.win);
}

#[gpui_kit::test]
fn a_picked_screenshot_draws_behind_the_hud_and_is_remembered(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let shot = png(dir.path(), "shot.png", 64, 36, [0, 0, 0, 255]);
    let (_env, s, page, picker) = opened(cx, "", vec![shot.clone()]);
    assert!(!support::present(cx, s.win, ids::HUD_MAKER_BACKDROP_IMAGE));
    pick_backdrop(cx, &s, &page);
    assert_eq!(picker.asked(), [PickKind::Backdrop]);
    // The entry closed the menu.
    assert!(!support::present(cx, s.win, ids::HUD_MAKER_BACKDROP));
    let size = cx.update(|cx| page.read(cx).backdrop().map(|b| (b.wide(), b.tall())));
    assert_eq!(size, Some((64, 36)));
    // Stretched over the whole screen box, under the parts.
    assert_eq!(
        support::bounds_of(cx, s.win, ids::HUD_MAKER_BACKDROP_IMAGE),
        support::bounds_of(cx, s.win, ids::HUD_MAKER_CANVAS)
    );
    assert_eq!(saved_backdrop(cx), Some(shot));
    assert!(untouched(cx, &page));
}

#[gpui_kit::test]
fn a_saved_screenshot_loads_with_the_page_shrunk_to_fit(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let wide = jpeg(dir.path(), "wide.jpg", 4000, 1000, [30, 60, 90]);
    let saved = wide.clone();
    let (_env, s, page, _) = open_with(cx, &[], &[], Vec::new(), move |s| {
        s.services.db.set_hud_maker_backdrop(Some(&saved)).unwrap();
    });
    support::wait_real(cx, "the saved backdrop", |cx| has_backdrop(cx, &page));
    let size = cx.update(|cx| page.read(cx).backdrop().map(|b| (b.wide(), b.tall())));
    assert_eq!(size, Some((1920, 480)));
    assert!(support::present(cx, s.win, ids::HUD_MAKER_BACKDROP_IMAGE));
}

#[gpui_kit::test]
fn remove_clears_the_screenshot(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let shot = png(dir.path(), "shot.png", 8, 8, [0, 0, 0, 255]);
    let (_env, s, page, _) = opened(cx, "", vec![shot]);
    pick_backdrop(cx, &s, &page);
    more(cx, &s, ids::HUD_MAKER_BACKDROP_CLEAR);
    assert!(
        !support::present(cx, s.win, ids::HUD_MAKER_BACKDROP_CLEAR),
        "the menu closed"
    );
    assert!(!has_backdrop(cx, &page));
    assert!(!support::present(cx, s.win, ids::HUD_MAKER_BACKDROP_IMAGE));
    assert_eq!(saved_backdrop(cx), None);
    // With nothing set, the menu offers no Remove.
    support::click_id(cx, s.win, ids::HUD_MAKER_MORE);
    assert!(support::present(cx, s.win, ids::HUD_MAKER_BACKDROP));
    assert!(!support::present(cx, s.win, ids::HUD_MAKER_BACKDROP_CLEAR));
}

#[gpui_kit::test]
fn a_file_that_is_not_a_picture_toasts_and_changes_nothing(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let junk = dir.path().join("junk.png");
    std::fs::write(&junk, b"not a picture").unwrap();
    let (_env, s, page, _) = opened(cx, "", vec![junk]);
    more(cx, &s, ids::HUD_MAKER_BACKDROP);
    support::wait_real(cx, "a toast", |cx| {
        !support::toast_labels(cx, s.win).is_empty()
    });
    assert!(
        support::toast_labels(cx, s.win)
            .iter()
            .any(|t| t.contains("That file isn't a picture TF2Tools can read."))
    );
    assert!(!has_backdrop(cx, &page));
    assert_eq!(saved_backdrop(cx), None);
}

#[gpui_kit::test]
fn a_saved_screenshot_that_is_gone_is_skipped_quietly(cx: &mut TestAppContext) {
    let gone = PathBuf::from("/nonexistent/tf2tools-test/shot.png");
    let saved = gone.clone();
    let (_env, s, page, _) = open_with(cx, &[], &[], Vec::new(), move |s| {
        s.services.db.set_hud_maker_backdrop(Some(&saved)).unwrap();
    });
    support::wait_real(cx, "the saved backdrop's read", |cx| {
        cx.update(|cx| page.read(cx).backdrop_settled())
    });
    support::settle(cx, s.win);
    assert!(!has_backdrop(cx, &page));
    assert!(support::toast_labels(cx, s.win).is_empty());
    assert_eq!(saved_backdrop(cx), Some(gone));
}

const HV: &str = "HudPlayerStatus/HudPlayerHealth/PlayerStatusHealthValue";

fn hard(cx: &mut TestAppContext, s: &support::ShellWindow, key: &PartKey) -> bool {
    support::present(cx, s.win, ids::hud_maker_hard_to_read(key))
}

#[gpui_kit::test]
fn text_too_close_to_the_screenshot_is_flagged(cx: &mut TestAppContext) {
    // Tan is TanDark (117 107 94): 1.04 to 1 on 120 110 95, 4.02 on black.
    let dir = tempfile::tempdir().unwrap();
    let beige = png(dir.path(), "beige.png", 16, 9, [120, 110, 95, 255]);
    let black = png(dir.path(), "black.png", 16, 9, [0, 0, 0, 255]);
    let (_env, s, page, _) = opened(cx, &bare_tan(), vec![beige, black]);
    let tan = k("Tan");
    assert!(!hard(cx, &s, &tan), "no screenshot, no check");
    pick_backdrop(cx, &s, &page);
    assert!(hard(cx, &s, &tan));
    assert_eq!(
        support::label_of(cx, s.win, ids::hud_maker_hard_to_read(&tan)).as_deref(),
        Some("Hard to read here")
    );
    pick_backdrop(cx, &s, &page);
    assert!(!hard(cx, &s, &tan));
    assert!(untouched(cx, &page));
}

#[gpui_kit::test]
fn the_contrast_check_runs_once_per_change(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let beige = png(dir.path(), "beige.png", 16, 9, [120, 110, 95, 255]);
    let (_env, s, page, _) = opened(cx, "", vec![beige]);
    pick_backdrop(cx, &s, &page);
    let runs = cx.update(|cx| page.read(cx).checks().hard_runs());
    for _ in 0..3 {
        support::next_repaint(cx);
        support::settle(cx, s.win);
    }
    assert_eq!(cx.update(|cx| page.read(cx).checks().hard_runs()), runs);
}

use tf2tools_core::{Rgba, Vision};

const BLUE_IMAGE: &str = "scores/BlueTeamImage";

/// The first pixel of `image` as RGBA (gpui keeps BGRA).
fn first_pixel(image: Option<Arc<gpui_kit::RenderImage>>) -> Option<[u8; 4]> {
    let bytes = image?.as_bytes(0)?.to_vec();
    Some([bytes[2], bytes[1], bytes[0], bytes[3]])
}

fn red_label() -> String {
    label(
        "Red",
        "10",
        "10",
        &[("labelText", "R"), ("fgcolor", r#""255 0 0 255""#)],
    )
}

#[gpui_kit::test]
fn a_colour_blind_view_recolours_text_and_says_so(cx: &mut TestAppContext) {
    let (_env, s, page, _) = opened(cx, &red_label(), Vec::new());
    let red = k("Red");
    let colour = |cx: &mut TestAppContext| cx.update(|cx| page.read(cx).drawn_colour(&red));
    assert_eq!(colour(cx), Some(Rgba::new(255, 0, 0, 255)));
    assert!(!support::present(cx, s.win, ids::HUD_MAKER_VISION_BADGE));
    more(cx, &s, ids::hud_maker_vision("Red-blind"));
    assert_eq!(cx.update(|cx| page.read(cx).vision()), Vision::Protanopia);
    assert_eq!(colour(cx), Some(Rgba::new(109, 95, 0, 255)));
    assert_eq!(
        support::label_of(cx, s.win, ids::HUD_MAKER_VISION_BADGE).as_deref(),
        Some("Red-blind view (protanopia)")
    );
    more(cx, &s, ids::hud_maker_vision("Green-blind"));
    assert_eq!(colour(cx), Some(Rgba::new(163, 144, 0, 255)));
    more(cx, &s, ids::hud_maker_vision("Normal"));
    assert_eq!(colour(cx), Some(Rgba::new(255, 0, 0, 255)));
    assert!(!support::present(cx, s.win, ids::HUD_MAKER_VISION_BADGE));
    assert!(untouched(cx, &page));
}

#[gpui_kit::test]
fn images_and_the_screenshot_are_seen_through_the_view(cx: &mut TestAppContext) {
    let board = support::hud_stock::SCOREBOARD.replace(
        "BlueTeamImage { ControlName ImagePanel",
        "BlueTeamImage { ControlName ImagePanel image ../hud/health_bg",
    );
    let red = tf2tools_core::RgbaImage::from_pixels(2, 2, [255, 0, 0, 255].repeat(4)).unwrap();
    let vtf = tf2tools_core::rgba_to_vtf(&red).unwrap();
    let vmt = r#""UnlitGeneric" { "$baseTexture" "hud/health_bg" }"#;
    let dir = tempfile::tempdir().unwrap();
    let shot = png(dir.path(), "red.png", 8, 8, [255, 0, 0, 255]);
    let (_env, s, page, _) = open_with(
        cx,
        &[("resource/ui/scoreboard.res", Some(&board))],
        &[
            ("materials/hud/health_bg.vmt", vmt.as_bytes()),
            ("materials/hud/health_bg.vtf", &vtf),
        ],
        vec![shot],
        |_| {},
    );
    pick_backdrop(cx, &s, &page);
    let key = PartKey::new(BLUE_IMAGE);
    let seen = |cx: &mut TestAppContext| {
        cx.update(|cx| {
            let p = page.read(cx);
            (first_pixel(p.image(&key)), first_pixel(p.backdrop_image()))
        })
    };
    assert_eq!(seen(cx), (Some([255, 0, 0, 255]), Some([255, 0, 0, 255])));
    more(cx, &s, ids::hud_maker_vision("Red-blind"));
    support::wait_real(cx, "the simulated images", |cx| {
        seen(cx) == (Some([109, 95, 0, 255]), Some([109, 95, 0, 255]))
    });
    more(cx, &s, ids::hud_maker_vision("Normal"));
    assert_eq!(seen(cx), (Some([255, 0, 0, 255]), Some([255, 0, 0, 255])));
}

#[gpui_kit::test]
fn hard_to_read_follows_the_view(cx: &mut TestAppContext) {
    // Dark grey on red, picture or panel: 3.69 to 1 normally, 2.31 red-blind, 4.60 green-blind.
    let dir = tempfile::tempdir().unwrap();
    let red = png(dir.path(), "red.png", 16, 9, [255, 0, 0, 255]);
    let grey_text = |name: &str, xpos: &str| {
        let keys = [
            ("zpos", "3"),
            ("labelText", "G"),
            ("fgcolor", r#""40 40 40 255""#),
        ];
        label(name, xpos, "10", &keys)
    };
    let panel = support::hud_stock::block(
        "Panel",
        "EditablePanel",
        &[
            ("xpos", "55"),
            ("ypos", "5"),
            ("zpos", "2"),
            ("wide", "40"),
            ("tall", "20"),
            ("bgcolor_override", r#""255 0 0 255""#),
        ],
    );
    let blocks = grey_text("Bare", "10") + &panel + &grey_text("OnPanel", "60");
    let (_env, s, page, _) = opened(cx, &blocks, vec![red]);
    pick_backdrop(cx, &s, &page);
    let (bare, on_panel) = (k("Bare"), k("OnPanel"));
    assert!(!hard(cx, &s, &bare) && !hard(cx, &s, &on_panel));
    more(cx, &s, ids::hud_maker_vision("Red-blind"));
    assert!(hard(cx, &s, &bare) && hard(cx, &s, &on_panel));
    more(cx, &s, ids::hud_maker_vision("Green-blind"));
    assert!(!hard(cx, &s, &bare) && !hard(cx, &s, &on_panel));
}

/// A label in the health number's colour, so a beige screenshot makes it hard to read.
fn tan_label(name: &str, xpos: &str, ypos: &str) -> String {
    label(
        name,
        xpos,
        ypos,
        &[("labelText", name), ("fgcolor", r#""117 107 94 255""#)],
    )
}

/// Tan at the health number's row, painted before the cross, so only the backdrop is behind it.
fn bare_tan() -> String {
    tan_label("Tan", "100", "52")
}

#[gpui_kit::test]
fn hard_to_read_badges_sit_beside_their_labels_inside_the_canvas(cx: &mut TestAppContext) {
    // Corner sits in the screen's bottom-left corner and Next just right of Tan, so the first
    // spot for each badge is off the canvas or on another flagged label.
    let dir = tempfile::tempdir().unwrap();
    let beige = png(dir.path(), "beige.png", 16, 9, [120, 110, 95, 255]);
    let blocks = bare_tan() + &tan_label("Corner", "0", "110") + &tan_label("Next", "122", "52");
    let (_env, s, page, _) = opened(cx, &blocks, vec![beige]);
    cx.simulate_window_resize(
        s.win,
        gpui_kit::size(gpui_kit::px(800.), gpui_kit::px(600.)),
    );
    support::settle(cx, s.win);
    pick_backdrop(cx, &s, &page);
    let keys = [k("Tan"), k("Corner"), k("Next")];
    let canvas = support::bounds_of(cx, s.win, ids::HUD_MAKER_CANVAS).unwrap();
    let marked: Vec<_> = (keys.iter())
        .map(|key| support::bounds_of(cx, s.win, ids::hud_maker_hard_to_read(key)).unwrap())
        .collect();
    let badges: Vec<_> = (keys.iter())
        .map(|key| support::bounds_of(cx, s.win, ids::hud_maker_hard_to_read_badge(key)).unwrap())
        .collect();
    let meets = |a: &gpui_kit::Bounds<gpui_kit::Pixels>, b: &gpui_kit::Bounds<gpui_kit::Pixels>| {
        a.origin.x < b.origin.x + b.size.width
            && b.origin.x < a.origin.x + a.size.width
            && a.origin.y < b.origin.y + b.size.height
            && b.origin.y < a.origin.y + a.size.height
    };
    for (i, badge) in badges.iter().enumerate() {
        assert!(
            badge.origin.x >= canvas.origin.x
                && badge.origin.y >= canvas.origin.y
                && badge.origin.x + badge.size.width <= canvas.origin.x + canvas.size.width
                && badge.origin.y + badge.size.height <= canvas.origin.y + canvas.size.height,
            "badge {i} {badge:?} leaves the canvas {canvas:?}"
        );
        for (j, label) in marked.iter().enumerate() {
            assert!(!meets(badge, label), "badge {i} covers label {j}");
        }
        for (j, other) in badges.iter().enumerate().filter(|(j, _)| *j != i) {
            assert!(!meets(badge, other), "badge {i} covers badge {j}");
        }
    }
    assert_eq!(
        support::label_of(cx, s.win, ids::hud_maker_hard_to_read_badge(&keys[1])).as_deref(),
        Some("Hard to read here")
    );
    assert!(untouched(cx, &page));
}

/// A stats row as the stock scoreboard lays it out: an east-aligned name in the health number's tan and its
/// value in black 5 units right, in 20-unit boxes in the 20 px `Default` font.
fn stats_row(name: &str, value: &str, ypos: &str) -> String {
    let block = |block: &str, x: &str, wide: &str, align: &str, text: &str, colour: &str| {
        let text = format!("\"{text}\"");
        let keys = [
            ("xpos", x),
            ("ypos", ypos),
            ("wide", wide),
            ("tall", "20"),
            ("font", "Default"),
            ("textAlignment", align),
            ("labelText", text.as_str()),
            ("fgcolor", colour),
        ];
        support::hud_stock::block(block, "CExLabel", &keys)
    };
    let (tan, black) = (r#""117 107 94 255""#, r#""0 0 0 255""#);
    block(
        &format!("{name}Label"),
        "130",
        "60",
        "east",
        &format!("{name}:"),
        tan,
    ) + &block(name, "195", "35", "west", value, black)
}

#[gpui_kit::test]
fn hard_to_read_markers_keep_off_every_label_text(cx: &mut TestAppContext) {
    // As on the stock scoreboard, rows 10 units apart in 20-unit boxes: a name's box edge runs
    // through the next row's text, and its readable value sits 5 units right of it.
    let dir = tempfile::tempdir().unwrap();
    let beige = png(dir.path(), "beige.png", 16, 9, [120, 110, 95, 255]);
    let rows = bare_tan() + &stats_row("Caps", "7", "30") + &stats_row("Defs", "3", "40");
    let (_env, s, page, _) = opened(cx, &rows, vec![beige]);
    cx.simulate_window_resize(
        s.win,
        gpui_kit::size(gpui_kit::px(1280.), gpui_kit::px(800.)),
    );
    support::settle(cx, s.win);
    pick_backdrop(cx, &s, &page);
    let flagged = [k("Tan"), k("CapsLabel"), k("DefsLabel")];
    let labels = [flagged.as_slice(), &[k("Caps"), k("Defs")]].concat();
    assert!(hard(cx, &s, &flagged[1]) && !hard(cx, &s, &labels[3]));
    let bounds =
        |cx: &mut TestAppContext, keys: &[PartKey], id: fn(&PartKey) -> gpui_kit::ElementId| {
            (keys.iter())
                .map(|key| support::bounds_of(cx, s.win, id(key)).unwrap())
                .collect::<Vec<_>>()
        };
    let texts = bounds(cx, &labels, ids::hud_maker_text);
    let outlines = bounds(cx, &flagged, ids::hud_maker_hard_to_read);
    let badges = bounds(cx, &flagged, ids::hud_maker_hard_to_read_badge);
    let meets = |a: &gpui_kit::Bounds<gpui_kit::Pixels>, b: &gpui_kit::Bounds<gpui_kit::Pixels>| {
        a.origin.x < b.origin.x + b.size.width
            && b.origin.x < a.origin.x + a.size.width
            && a.origin.y < b.origin.y + b.size.height
            && b.origin.y < a.origin.y + a.size.height
    };
    // Every way a marker hides text, so a failure names each symptom at once.
    let mut hidden = Vec::new();
    for (i, outline) in outlines.iter().enumerate() {
        // Around its own text and clear of the rest; `labels` starts with the flagged ones.
        let own = texts[i];
        if !(outline.left() <= own.left()
            && outline.top() <= own.top()
            && outline.right() >= own.right()
            && outline.bottom() >= own.bottom())
        {
            hidden.push(format!(
                "outline {i} {outline:?} cuts through its text {own:?}"
            ));
        }
        for (j, text) in texts.iter().enumerate().filter(|(j, _)| *j != i) {
            if meets(outline, text) {
                hidden.push(format!("outline {i} {outline:?} crosses text {j} {text:?}"));
            }
        }
    }
    for (i, badge) in badges.iter().enumerate() {
        for (j, text) in texts.iter().enumerate() {
            if meets(badge, text) {
                hidden.push(format!("badge {i} {badge:?} covers text {j} {text:?}"));
            }
        }
        for (j, other) in badges.iter().enumerate().filter(|(j, _)| *j != i) {
            if meets(badge, other) {
                hidden.push(format!("badge {i} covers badge {j}"));
            }
        }
    }
    // A badge set apart draws a line back to its outline, which never runs over text.
    for (i, key) in flagged.iter().enumerate() {
        let pieces = (0..).map_while(|n| {
            support::bounds_of(cx, s.win, ids::hud_maker_hard_to_read_leader(key, n))
        });
        for piece in pieces {
            for (j, text) in texts.iter().enumerate() {
                if meets(&piece, text) {
                    hidden.push(format!("leader {i} {piece:?} covers text {j} {text:?}"));
                }
            }
        }
    }
    assert!(hidden.is_empty(), "{}", hidden.join("\n"));
    assert!(untouched(cx, &page));
}

#[gpui_kit::test]
fn a_flagged_part_off_its_tile_gets_an_arrow_at_the_edge(cx: &mut TestAppContext) {
    // Edge at 625 units is wholly past the 1280x1024 screen and only partly past 1440x1080.
    let edge = label("Edge", "625", "10", &[("labelText", r#""Edge label""#)]);
    let (_env, s, _page, _) = opened(cx, &edge, Vec::new());
    more(cx, &s, ids::HUD_MAKER_EVERY_SCREEN);
    let four_three = Screen::new(1440, 1080);
    assert!(support::present(
        cx,
        s.win,
        ids::hud_maker_flag(four_three, &k("Edge"))
    ));
    assert!(!support::present(
        cx,
        s.win,
        ids::hud_maker_edge_arrow(four_three, &k("Edge"))
    ));
    let tile = support::bounds_of(cx, s.win, ids::hud_maker_screen_tile(FIVE_FOUR)).unwrap();
    let arrow = support::bounds_of(cx, s.win, ids::hud_maker_edge_arrow(FIVE_FOUR, &k("Edge")))
        .expect("an arrow on the 5:4 tile");
    assert_eq!(
        support::label_of(cx, s.win, ids::hud_maker_edge_arrow(FIVE_FOUR, &k("Edge"))).as_deref(),
        Some("Edge goes off the screen")
    );
    // On the tile, at its right side, level with the part near the bottom of the screen.
    let (corner, tile_corner) = (arrow.bottom_right(), tile.bottom_right());
    assert!(
        arrow.origin.x >= tile.origin.x
            && arrow.origin.y >= tile.origin.y
            && corner.x <= tile_corner.x
            && corner.y <= tile_corner.y,
        "{arrow:?} in {tile:?}"
    );
    let tile_mid = tile.origin.x + tile.size.width / 2.;
    assert!(
        arrow.origin.x > tile_mid + tile.size.width / 4.,
        "{arrow:?} in {tile:?}"
    );
    assert!(
        arrow.origin.y > tile.origin.y + tile.size.height / 2.,
        "{arrow:?} in {tile:?}"
    );
}

#[gpui_kit::test]
fn switching_back_to_a_colour_blind_view_reuses_its_images(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let shot = png(dir.path(), "red.png", 8, 8, [255, 0, 0, 255]);
    let (_env, s, page, _) = opened(cx, "", vec![shot]);
    pick_backdrop(cx, &s, &page);
    let seen =
        |cx: &mut TestAppContext| cx.update(|cx| first_pixel(page.read(cx).backdrop_image()));
    let runs = |cx: &mut TestAppContext| cx.update(|cx| page.read(cx).simulation_runs());
    let before = runs(cx);
    more(cx, &s, ids::hud_maker_vision("Red-blind"));
    support::wait_real(cx, "red-blind images", |cx| {
        seen(cx) == Some([109, 95, 0, 255])
    });
    more(cx, &s, ids::hud_maker_vision("Green-blind"));
    support::wait_real(cx, "green-blind images", |cx| {
        seen(cx) == Some([163, 144, 0, 255])
    });
    more(cx, &s, ids::hud_maker_vision("Normal"));
    assert_eq!(seen(cx), Some([255, 0, 0, 255]));
    // Back to a view seen before: its images show at once, with no new simulation.
    more(cx, &s, ids::hud_maker_vision("Red-blind"));
    assert_eq!(seen(cx), Some([109, 95, 0, 255]));
    assert_eq!(runs(cx) - before, 2);
    assert!(untouched(cx, &page));
}

#[gpui_kit::test]
fn edge_arrows_point_off_each_side_of_a_tile(cx: &mut TestAppContext) {
    // Wide and Far, in a screen-wide panel, sit past the left and right of the narrow screens
    // only; Up and Down are dragged off the top and bottom of every screen.
    let damage = with_blocks(
        r#""Resource/UI/HudDamageAccount.res" { CDamageAccountPanel { delta_item_font HudFontMedium } }"#,
        &(label("Wide", "c-440", "100", &[("labelText", "W")])
            + &label("Far", "c420", "100", &[("labelText", "F")])),
    );
    let health = with_blocks(
        HEALTH,
        &(label("Up", "10", "10", &[("labelText", "U")])
            + &label("Down", "40", "10", &[("labelText", "D")])),
    );
    let (_env, s, page, _) = open_with(
        cx,
        &[
            ("resource/ui/huddamageaccount.res", Some(&damage)),
            ("resource/ui/hudplayerhealth.res", Some(&health)),
        ],
        &[],
        Vec::new(),
        |_| {},
    );
    for (key, dy) in [(k("Up"), -3000), (k("Down"), 3000)] {
        cx.update(|cx| {
            page.update(cx, |p, cx| {
                let e = p.editor_mut().unwrap();
                e.select(Some(key));
                e.move_selected(0, dy).unwrap();
                cx.notify();
            })
        });
    }
    more(cx, &s, ids::HUD_MAKER_EVERY_SCREEN);
    let tile = support::bounds_of(cx, s.win, ids::hud_maker_screen_tile(FIVE_FOUR)).unwrap();
    let arrow = |cx: &mut TestAppContext, key: PartKey| {
        let b = support::bounds_of(cx, s.win, ids::hud_maker_edge_arrow(FIVE_FOUR, &key));
        b.unwrap_or_else(|| panic!("an arrow for {key:?}")).center()
    };
    let (left, right) = (
        arrow(cx, PartKey::new("CDamageAccountPanel/Wide")),
        arrow(cx, PartKey::new("CDamageAccountPanel/Far")),
    );
    let (up, down) = (arrow(cx, k("Up")), arrow(cx, k("Down")));
    let mid = tile.center();
    assert!(
        left.x < mid.x && right.x > mid.x,
        "{left:?} {right:?} {tile:?}"
    );
    assert!(up.y < mid.y && down.y > up.y, "{up:?} {down:?} {tile:?}");
    assert!(down.y > mid.y, "{down:?} {tile:?}");
}

#[gpui_kit::test]
fn a_hud_that_lands_drops_every_kept_colour_blind_set(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let shot = png(dir.path(), "red.png", 8, 8, [255, 0, 0, 255]);
    let (_env, s, page, _) = opened(cx, "", vec![shot]);
    pick_backdrop(cx, &s, &page);
    let runs = |cx: &mut TestAppContext| cx.update(|cx| page.read(cx).simulation_runs());
    more(cx, &s, ids::hud_maker_vision("Red-blind"));
    support::wait_real(cx, "red-blind images", |cx| {
        cx.update(|cx| first_pixel(page.read(cx).backdrop_image())) == Some([109, 95, 0, 255])
    });
    let before = runs(cx);
    // Open a made HUD: the red-blind set was for the old HUD's images, so it is made again.
    let changes = support::stock_model()
        .move_part(&PartKey::new(HV), 45, 0, HD, 1)
        .unwrap();
    let saved = s
        .services
        .library
        .save_made_hud(
            &tf2tools_core::MakeTarget::New,
            "Made",
            &support::hud_stock::stock_files(),
            tf2tools_core::Platform::Linux,
            &changes,
        )
        .unwrap();
    let id = saved.entry().id().clone();
    cx.update(|cx| page.update(cx, |p, cx| p.open(id, cx)));
    support::wait_real(cx, "the made HUD", |cx| {
        cx.update(|cx| page.read(cx).saved_hud().is_some())
    });
    support::settle(cx, s.win);
    assert_eq!(runs(cx), before + 1);
    // Normal and back reuses the new set.
    more(cx, &s, ids::hud_maker_vision("Normal"));
    more(cx, &s, ids::hud_maker_vision("Red-blind"));
    assert_eq!(runs(cx), before + 1);
}

#[gpui_kit::test]
fn the_main_menu_draws_its_own_picture_instead_of_the_screenshot(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let shot = png(dir.path(), "shot.png", 64, 36, [0, 0, 0, 255]);
    let (_env, s, page, _) = open_with(cx, &[], &[], Vec::new(), move |s| {
        s.services.db.set_hud_maker_backdrop(Some(&shot)).unwrap();
    });
    support::wait_real(cx, "the saved backdrop", |cx| has_backdrop(cx, &page));
    assert!(support::present(cx, s.win, ids::HUD_MAKER_BACKDROP_IMAGE));
    cx.update_window(s.win, |_, window, cx| {
        page.update(cx, |p, cx| p.set_scene(Scene::MainMenu, window, cx))
    })
    .unwrap();
    support::settle(cx, s.win);
    assert!(!support::present(cx, s.win, ids::HUD_MAKER_BACKDROP_IMAGE));
    cx.update_window(s.win, |_, window, cx| {
        page.update(cx, |p, cx| p.set_scene(Scene::Hud, window, cx))
    })
    .unwrap();
    support::settle(cx, s.win);
    assert!(support::present(cx, s.win, ids::HUD_MAKER_BACKDROP_IMAGE));
}

/// A chat whose history starts at the screen's left edge level with a label at health-local
/// (10, 10), its history with no labelText, so only its drawn lines keep the badge off them.
const CHAT_BY_LABEL: &str = r##""Resource/UI/BaseChat.res"
{
  HudChat { ControlName EditablePanel fieldName HudChat xpos 0 ypos 352 wide 280 tall 120 }
  HudChatHistory { ControlName RichText fieldName HudChatHistory xpos 10 ypos 17 wide 260 tall 75 visible 1 }
}
"##;

#[gpui_kit::test]
fn hard_to_read_badges_keep_off_the_chat_lines(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let beige = png(dir.path(), "beige.png", 16, 9, [120, 110, 95, 255]);
    let health = with_blocks(HEALTH, &tan_label("Tan", "10", "10"));
    let (_env, s, page, _) = open_with(
        cx,
        &[
            ("resource/ui/hudplayerhealth.res", Some(&health)),
            ("resource/ui/basechat.res", Some(CHAT_BY_LABEL)),
        ],
        &[],
        vec![beige],
        |_| {},
    );
    cx.simulate_window_resize(
        s.win,
        gpui_kit::size(gpui_kit::px(1280.), gpui_kit::px(800.)),
    );
    support::settle(cx, s.win);
    pick_backdrop(cx, &s, &page);
    let tan = k("Tan");
    assert!(hard(cx, &s, &tan));
    let badge = support::bounds_of(cx, s.win, ids::hud_maker_hard_to_read_badge(&tan)).unwrap();
    for i in 0..3 {
        let line = support::bounds_of(cx, s.win, ids::hud_maker_chat_line(i)).unwrap();
        assert!(
            !badge.intersects(&line),
            "the badge {badge:?} covers chat line {i} {line:?}"
        );
    }
}

/// The chat history flagged hard to read: it has a labelText of its own and white text, while its drawn lines are the text the outline must hold.
const CHAT_FLAGGED: &str = r##""Resource/UI/BaseChat.res"
{
  HudChat { ControlName EditablePanel fieldName HudChat xpos 0 ypos 352 wide 280 tall 120 }
  HudChatHistory { ControlName RichText fieldName HudChatHistory xpos 10 ypos 17 wide 260 tall 75 visible 1 labelText "x" }
}
"##;

#[gpui_kit::test]
fn the_hard_to_read_outline_holds_the_whole_chat_text(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    // The history's text is white, so a white screenshot hides it.
    let white = png(dir.path(), "white.png", 16, 9, [255, 255, 255, 255]);
    let (_env, s, page, _) = open_with(
        cx,
        &[("resource/ui/basechat.res", Some(CHAT_FLAGGED))],
        &[],
        vec![white],
        |_| {},
    );
    cx.simulate_window_resize(
        s.win,
        gpui_kit::size(gpui_kit::px(1280.), gpui_kit::px(800.)),
    );
    support::settle(cx, s.win);
    pick_backdrop(cx, &s, &page);
    support::settle(cx, s.win);
    let history = PartKey::new("HudChat/HudChatHistory");
    assert!(hard(cx, &s, &history));
    let outline = support::bounds_of(cx, s.win, ids::hud_maker_hard_to_read(&history)).unwrap();
    for i in 0..3 {
        let line = support::bounds_of(cx, s.win, ids::hud_maker_chat_line(i)).unwrap();
        assert!(
            outline.left() < line.left(),
            "line {i}: {outline:?} {line:?}"
        );
        assert!(
            outline.right() > line.right(),
            "line {i}: {outline:?} {line:?}"
        );
    }
}

#[gpui_kit::test]
fn the_freeze_cam_tab_draws_the_screenshot_undimmed_behind_its_panel(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let shot = png(dir.path(), "shot.png", 64, 36, [0, 0, 0, 255]);
    let (_env, s, page, _picker) = opened(cx, "", vec![shot]);
    pick_backdrop(cx, &s, &page);
    support::click_scene(cx, s.win, Scene::FreezeCam);
    // The same picture, stretched over the screen box, under the panel and with no dimming layer.
    assert_eq!(
        support::bounds_of(cx, s.win, ids::HUD_MAKER_BACKDROP_IMAGE),
        support::bounds_of(cx, s.win, ids::HUD_MAKER_CANVAS)
    );
    // The main menu has its own picture; the freeze cam does not.
    support::click_scene(cx, s.win, Scene::MainMenu);
    assert!(!support::present(cx, s.win, ids::HUD_MAKER_BACKDROP_IMAGE));
}

#[gpui_kit::test]
fn the_win_panel_tab_draws_the_screenshot_undimmed_behind_its_panel(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let shot = png(dir.path(), "shot.png", 64, 36, [0, 0, 0, 255]);
    let (_env, s, page, _picker) = opened(cx, "", vec![shot]);
    pick_backdrop(cx, &s, &page);
    support::click_scene(cx, s.win, Scene::WinPanel);
    assert_eq!(
        support::bounds_of(cx, s.win, ids::HUD_MAKER_BACKDROP_IMAGE),
        support::bounds_of(cx, s.win, ids::HUD_MAKER_CANVAS)
    );
}

#[gpui_kit::test]
fn the_class_select_tab_draws_the_screenshot_behind_its_menu(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let shot = png(dir.path(), "shot.png", 64, 36, [0, 0, 0, 255]);
    let (_env, s, page, _picker) = opened(cx, "", vec![shot]);
    pick_backdrop(cx, &s, &page);
    support::click_scene(cx, s.win, Scene::ClassSelect);
    assert_eq!(
        support::bounds_of(cx, s.win, ids::HUD_MAKER_BACKDROP_IMAGE),
        support::bounds_of(cx, s.win, ids::HUD_MAKER_CANVAS)
    );
}

#[gpui_kit::test]
fn the_spectator_tab_draws_the_screenshot_behind_its_bars(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let shot = png(dir.path(), "shot.png", 64, 36, [0, 0, 0, 255]);
    let (_env, s, page, _picker) = opened(cx, "", vec![shot]);
    pick_backdrop(cx, &s, &page);
    support::click_scene(cx, s.win, Scene::Spectator);
    assert_eq!(
        support::bounds_of(cx, s.win, ids::HUD_MAKER_BACKDROP_IMAGE),
        support::bounds_of(cx, s.win, ids::HUD_MAKER_CANVAS)
    );
}
