//! Picking pictures and backgrounds in the HUD Maker, headless.

mod support;

use std::path::{Path, PathBuf};
use std::sync::Arc;

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{AppContext as _, Entity, ScrollDelta, TestAppContext, point};
use tf2tools_app::ids;
use tf2tools_app::pages::hud_maker::HudMakerPage;
use tf2tools_app::pages::hud_maker::view::Panel;
use tf2tools_app::picker::{PickKind, ScriptedPicker};
use tf2tools_app::services::Tf2State;
use tf2tools_app::shell::route::Page;
use tf2tools_core::{PartKey, RgbaImage, Scene, Tf2Install, decode_vtf, rgba_to_vtf};

const IMAGE: &str = "HudPlayerStatus/HudPlayerHealth/PlayerStatusHealthImage";
const VALUE: &str = "HudPlayerStatus/HudPlayerHealth/PlayerStatusHealthValue";
const BOX: &str = "HudPlayerStatus/HudPlayerHealth/Box";
const HEALTH_VMT: &str = r#""UnlitGeneric" { "$basetexture" "hud/health_color" }"#;

fn solid_vtf(w: u32, h: u32) -> Vec<u8> {
    rgba_to_vtf(&RgbaImage::from_pixels(w, h, vec![200; (w * h * 4) as usize]).unwrap()).unwrap()
}

/// The stock HUD with a 64x64 health cross texture and a filled `Box` panel.
fn opened(
    cx: &mut TestAppContext,
    picks: Vec<PathBuf>,
) -> (
    support::Env,
    support::ShellWindow,
    Entity<HudMakerPage>,
    Arc<ScriptedPicker>,
) {
    use support::hud_stock::{HEALTH, block, with_blocks};
    let env = support::env();
    let keys = [
        ("xpos", "10"),
        ("ypos", "10"),
        ("wide", "40"),
        ("tall", "20"),
        ("bgcolor_override", r#""0 0 0 200""#),
    ];
    let health = with_blocks(HEALTH, &block("Box", "EditablePanel", &keys));
    let texture = solid_vtf(64, 64);
    support::seed_stock_game_with_bytes(
        &env.game,
        &[("resource/ui/hudplayerhealth.res", Some(&health))],
        &[
            ("materials/hud/health_color.vmt", HEALTH_VMT.as_bytes()),
            ("materials/hud/health_color.vtf", &texture),
        ],
    );
    let state = Tf2State::Found(Tf2Install::from_path(&env.game).unwrap());
    let picker = Arc::new(ScriptedPicker::new(picks));
    let s = support::shell_window_with(cx, &env, state, picker.clone(), support::no_steam());
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

fn size_of(image: Option<Arc<gpui_kit::RenderImage>>) -> Option<(i32, i32)> {
    let size = image?.size(0);
    Some((i32::from(size.width), i32::from(size.height)))
}

fn shown_size(
    cx: &mut TestAppContext,
    page: &Entity<HudMakerPage>,
    key: &str,
) -> Option<(i32, i32)> {
    size_of(cx.update(|cx| page.read(cx).image(&PartKey::new(key))))
}

fn select(cx: &mut TestAppContext, page: &Entity<HudMakerPage>, key: &str) {
    cx.update(|cx| {
        page.update(cx, |p, cx| {
            p.editor_mut().unwrap().select(Some(PartKey::new(key)));
            cx.notify();
        })
    });
}

fn land(
    cx: &mut TestAppContext,
    s: &support::ShellWindow,
    page: &Entity<HudMakerPage>,
    key: &str,
    vtf: Vec<u8>,
) {
    cx.update_window(s.win, |_, window, cx| {
        page.update(cx, |p, cx| {
            p.apply_picture(&PartKey::new(key), vtf, window, cx)
        })
    })
    .unwrap();
    support::settle(cx, s.win);
}

fn undo(cx: &mut TestAppContext, s: &support::ShellWindow, page: &Entity<HudMakerPage>) {
    cx.update_window(s.win, |_, window, cx| {
        page.update(cx, |p, cx| p.undo(window, cx))
    })
    .unwrap();
    support::settle(cx, s.win);
}

#[gpui_kit::test]
fn a_code_set_picture_redraws_under_the_same_name_and_back_on_undo(cx: &mut TestAppContext) {
    let (_env, s, page, _) = opened(cx, vec![]);
    support::wait_real(cx, "the stock cross", |cx| {
        shown_size(cx, &page, IMAGE) == Some((64, 64))
    });
    select(cx, &page, IMAGE);
    land(cx, &s, &page, IMAGE, solid_vtf(8, 8));
    support::wait_real(cx, "the new cross", |cx| {
        shown_size(cx, &page, IMAGE) == Some((8, 8))
    });
    undo(cx, &s, &page);
    support::wait_real(cx, "the stock cross again", |cx| {
        shown_size(cx, &page, IMAGE) == Some((64, 64))
    });
}

#[gpui_kit::test]
fn a_picture_for_a_part_no_longer_selected_is_dropped(cx: &mut TestAppContext) {
    let (_env, s, page, _) = opened(cx, vec![]);
    select(cx, &page, VALUE);
    land(cx, &s, &page, IMAGE, solid_vtf(8, 8));
    assert_eq!(
        cx.update(|cx| page.read(cx).editor().unwrap().changes().len()),
        0
    );
}

#[gpui_kit::test]
fn a_menu_background_shows_on_the_main_menu_and_goes_on_undo(cx: &mut TestAppContext) {
    let (_env, s, page, _) = opened(cx, vec![]);
    support::click_scene(cx, s.win, Scene::MainMenu);
    support::settle(cx, s.win);
    assert_eq!(
        size_of(cx.update(|cx| page.read(cx).menu_background())),
        None
    );
    cx.update_window(s.win, |_, window, cx| {
        page.update(cx, |p, cx| {
            p.apply_menu_background(solid_vtf(1024, 1024), solid_vtf(2048, 1024), window, cx)
        })
    })
    .unwrap();
    support::wait_real(cx, "the wide menu picture", |cx| {
        size_of(cx.update(|cx| page.read(cx).menu_background())) == Some((2048, 1024))
    });
    undo(cx, &s, &page);
    support::wait_real(cx, "no menu picture", |cx| {
        cx.update(|cx| page.read(cx).menu_background()).is_none()
    });
}

fn png(dir: &Path, name: &str, w: u32, h: u32) -> PathBuf {
    let path = dir.join(name);
    image::RgbaImage::from_pixel(w, h, image::Rgba([10, 200, 10, 255]))
        .save(&path)
        .unwrap();
    path
}

fn folded(cx: &mut TestAppContext, s: &support::ShellWindow, panel: Panel) -> bool {
    cx.update(|cx| {
        let page = s.shell.read(cx).hud_maker_page().expect("a HUD Maker page");
        page.read(cx).is_folded(panel)
    })
}

fn click_row(cx: &mut TestAppContext, s: &support::ShellWindow, key: &str) {
    if !support::present(cx, s.win, ids::HUD_MAKER_PARTS) {
        support::click_id(cx, s.win, ids::HUD_MAKER_PARTS_PANEL_COLLAPSE);
    }
    support::click_part_row(cx, s.win, &PartKey::new(key));
    if support::present(cx, s.win, ids::HUD_MAKER_PARTS) {
        support::click_id(cx, s.win, ids::HUD_MAKER_PARTS_PANEL_COLLAPSE);
    }
    if folded(cx, s, Panel::Options) {
        support::click_id(cx, s.win, ids::HUD_MAKER_PANEL_COLLAPSE);
    }
}

fn click_in_panel(cx: &mut TestAppContext, s: &support::ShellWindow, id: gpui_kit::ElementId) {
    for _ in 0..80 {
        let body = support::bounds_of(cx, s.win, ids::HUD_MAKER_PANEL_BODY).unwrap();
        let b = support::bounds_of(cx, s.win, id.clone()).unwrap();
        if b.top() >= body.top() && b.bottom() <= body.bottom() {
            support::click_id(cx, s.win, id);
            return;
        }
        let step = if b.top() < body.top() { 1. } else { -1. };
        let turn = ScrollDelta::Lines(point(0., step));
        cx.update_window(s.win, |_, w, cx| {
            w.scroll(ids::HUD_MAKER_PANEL_BODY, turn, cx)
        })
        .unwrap();
    }
    panic!("{id:?} never scrolled into the side panel");
}

fn changes(cx: &mut TestAppContext, page: &Entity<HudMakerPage>) -> usize {
    cx.update(|cx| page.read(cx).editor().map_or(0, |e| e.changes().len()))
}

fn wait_toast(cx: &mut TestAppContext, s: &support::ShellWindow, text: &str) {
    support::wait_real(cx, "the toast", |cx| {
        support::toast_labels(cx, s.win).iter().any(|t| t == text)
    });
}

#[gpui_kit::test]
fn a_picked_png_replaces_the_cross_at_the_old_texture_s_size(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (_env, s, page, picker) = opened(cx, vec![png(dir.path(), "cross.png", 30, 30)]);
    click_row(cx, &s, IMAGE);
    assert!(support::present(cx, s.win, ids::HUD_MAKER_PICTURE_SHARED));
    click_in_panel(cx, &s, ids::HUD_MAKER_PICTURE_PICK.into());
    support::wait_real(cx, "the pick", |cx| changes(cx, &page) > 0);
    assert_eq!(picker.asked(), [PickKind::HudPicture]);
    // The new texture is the one applied asset that is a .vtf; its bytes are kept by source.
    let bytes = cx.update(|cx| {
        let e = page.read(cx).editor().unwrap();
        let (_, source) = e
            .model()
            .assets()
            .iter()
            .find(|(p, _)| p.ends_with(".vtf"))
            .cloned()?;
        e.assets().get(&source).map(<[u8]>::to_vec)
    });
    let texture = decode_vtf(&bytes.expect("the kept texture")).unwrap();
    assert_eq!((texture.width(), texture.height()), (64, 64));
    click_in_panel(cx, &s, ids::HUD_MAKER_PICTURE_RESET.into());
    support::wait_real(cx, "the reset", |cx| changes(cx, &page) == 0);
}

#[gpui_kit::test]
fn a_file_that_is_not_a_picture_toasts_and_changes_nothing(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let notes = dir.path().join("notes.png");
    std::fs::write(&notes, b"not a picture").unwrap();
    let (_env, s, page, _) = opened(cx, vec![notes]);
    click_row(cx, &s, IMAGE);
    click_in_panel(cx, &s, ids::HUD_MAKER_PICTURE_PICK.into());
    wait_toast(cx, &s, "That file isn't a picture TF2Tools can read.");
    assert_eq!(changes(cx, &page), 0);
}

#[gpui_kit::test]
fn a_picture_too_wide_toasts_in_plain_words(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (_env, s, page, _) = opened(cx, vec![png(dir.path(), "wide.png", 4097, 1)]);
    click_row(cx, &s, IMAGE);
    click_in_panel(cx, &s, ids::HUD_MAKER_PICTURE_PICK.into());
    wait_toast(
        cx,
        &s,
        "That picture is too big. Pick one under 4096 pixels on each side.",
    );
    assert_eq!(changes(cx, &page), 0);
}

#[gpui_kit::test]
fn a_label_has_no_picture_row(cx: &mut TestAppContext) {
    let (_env, s, _page, _) = opened(cx, vec![]);
    click_row(cx, &s, VALUE);
    assert!(!support::present(cx, s.win, ids::HUD_MAKER_PICTURE_PICK));
}

fn box_value(cx: &mut TestAppContext, page: &Entity<HudMakerPage>, key: &str) -> Option<String> {
    cx.update(|cx| {
        let e = page.read(cx).editor().unwrap();
        e.model()
            .part(&PartKey::new(BOX))?
            .value(key)
            .map(|l| l.text().to_owned())
    })
}

#[gpui_kit::test]
fn switching_the_background_off_hides_the_fill(cx: &mut TestAppContext) {
    let (_env, s, page, _) = opened(cx, vec![]);
    let fill = ids::hud_maker_fill(&PartKey::new(BOX));
    assert!(support::present(cx, s.win, fill.clone()));
    click_row(cx, &s, BOX);
    click_in_panel(cx, &s, ids::hud_maker_toggle("paintbackground"));
    support::settle(cx, s.win);
    assert_eq!(
        box_value(cx, &page, "paintbackground").as_deref(),
        Some("0")
    );
    assert!(!support::present(cx, s.win, fill));
}

#[gpui_kit::test]
fn corners_are_picked_by_name_and_undo_one_at_a_time(cx: &mut TestAppContext) {
    let (_env, s, page, _) = opened(cx, vec![]);
    click_row(cx, &s, BOX);
    click_in_panel(cx, &s, ids::hud_maker_choice("PaintBackgroundType", "2"));
    click_in_panel(cx, &s, ids::hud_maker_choice("RoundedCorners", "3"));
    support::settle(cx, s.win);
    assert_eq!(
        box_value(cx, &page, "PaintBackgroundType").as_deref(),
        Some("2")
    );
    assert_eq!(box_value(cx, &page, "RoundedCorners").as_deref(), Some("3"));
    assert!(support::present(
        cx,
        s.win,
        ids::hud_maker_fill(&PartKey::new(BOX))
    ));
    undo(cx, &s, &page);
    assert_eq!(box_value(cx, &page, "RoundedCorners"), None);
    assert_eq!(
        box_value(cx, &page, "PaintBackgroundType").as_deref(),
        Some("2")
    );
}

fn open_options(cx: &mut TestAppContext, s: &support::ShellWindow) {
    if folded(cx, s, Panel::Options) {
        support::click_id(cx, s.win, ids::HUD_MAKER_PANEL_COLLAPSE);
    }
}

#[gpui_kit::test]
fn the_menu_row_shows_only_on_the_main_menu(cx: &mut TestAppContext) {
    let (_env, s, _page, _) = opened(cx, vec![]);
    open_options(cx, &s);
    assert!(!support::present(
        cx,
        s.win,
        ids::HUD_MAKER_MENU_PICTURE_PICK
    ));
    support::click_scene(cx, s.win, Scene::MainMenu);
    open_options(cx, &s);
    assert!(support::present(
        cx,
        s.win,
        ids::HUD_MAKER_MENU_PICTURE_PICK
    ));
}

#[gpui_kit::test]
fn a_picked_menu_background_previews_and_resets(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (_env, s, page, picker) = opened(cx, vec![png(dir.path(), "menu.png", 64, 36)]);
    support::click_scene(cx, s.win, Scene::MainMenu);
    open_options(cx, &s);
    click_in_panel(cx, &s, ids::HUD_MAKER_MENU_PICTURE_PICK.into());
    support::wait_real(cx, "the menu picture", |cx| {
        size_of(cx.update(|cx| page.read(cx).menu_background())) == Some((2048, 1024))
    });
    assert_eq!(picker.asked(), [PickKind::HudPicture]);
    click_in_panel(cx, &s, ids::HUD_MAKER_MENU_PICTURE_RESET.into());
    support::wait_real(cx, "no menu picture", |cx| {
        cx.update(|cx| page.read(cx).menu_background()).is_none()
    });
}

#[gpui_kit::test]
fn a_shared_recipe_says_its_pictures_stay_behind(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("mine.json");
    let (_env, s, page, picker) = opened(cx, vec![target.clone()]);
    select(cx, &page, IMAGE);
    land(cx, &s, &page, IMAGE, solid_vtf(8, 8));
    cx.update(|cx| {
        page.update(cx, |p, cx| {
            p.editor_mut().unwrap().select(None);
            cx.notify();
        })
    });
    support::settle(cx, s.win);
    open_options(cx, &s);
    click_in_panel(cx, &s, ids::HUD_MAKER_RECIPE_EXPORT.into());
    support::wait_real(cx, "the toast", |cx| {
        support::toast_labels(cx, s.win)
            .iter()
            .any(|t| t.contains("Pictures aren't included in shared recipes yet, so parts that use them were left out."))
    });
    assert_eq!(picker.asked(), [PickKind::RecipeExport]);
    assert!(
        !support::toast_labels(cx, s.win)
            .iter()
            .any(|t| t.contains("font")),
        "pictures are not counted as fonts"
    );
    let (recipe, _) = tf2tools_core::Recipe::parse(&std::fs::read(&target).unwrap()).unwrap();
    assert!(recipe.changes().is_empty());
}

/// Answers each pick with the next queued path; a gated answer waits until its gate closes,
/// like a slow file read, so a later pick can resolve first.
struct Ordered(
    std::sync::Mutex<std::collections::VecDeque<(PathBuf, Option<async_channel::Receiver<()>>)>>,
);

impl tf2tools_app::picker::PathPicker for Ordered {
    fn pick(&self, _: PickKind, cx: &gpui_kit::App) -> gpui_kit::Task<Option<PathBuf>> {
        let Some((path, gate)) = self.0.lock().unwrap().pop_front() else {
            return gpui_kit::Task::ready(None);
        };
        match gate {
            None => gpui_kit::Task::ready(Some(path)),
            Some(gate) => cx.background_executor().spawn(async move {
                let _ = gate.recv().await;
                Some(path)
            }),
        }
    }

    fn save_to(&self, _: PickKind, _: &str, _: &gpui_kit::App) -> gpui_kit::Task<Option<PathBuf>> {
        gpui_kit::Task::ready(None)
    }
}

fn first_pixel_of_kept_texture(cx: &mut TestAppContext, page: &Entity<HudMakerPage>) -> [u8; 4] {
    let bytes = cx.update(|cx| {
        let e = page.read(cx).editor().unwrap();
        let (_, source) = (e.model().assets().iter())
            .find(|(p, _)| p.ends_with(".vtf"))
            .cloned()?;
        e.assets().get(&source).map(<[u8]>::to_vec)
    });
    let texture = decode_vtf(&bytes.expect("the kept texture")).unwrap();
    let px = &texture.pixels()[..4];
    [px[0], px[1], px[2], px[3]]
}

#[gpui_kit::test]
fn of_two_picture_picks_only_the_latest_lands_even_when_the_first_resolves_last(
    cx: &mut TestAppContext,
) {
    let dir = tempfile::tempdir().unwrap();
    let first = dir.path().join("red.png");
    image::RgbaImage::from_pixel(30, 30, image::Rgba([250, 0, 0, 255]))
        .save(&first)
        .unwrap();
    let second = dir.path().join("blue.png");
    image::RgbaImage::from_pixel(30, 30, image::Rgba([0, 0, 250, 255]))
        .save(&second)
        .unwrap();
    let (_env, s, page, _) = opened(cx, vec![]);
    let (slow, gate) = async_channel::bounded::<()>(1);
    let answers = [(first, Some(gate)), (second, None)];
    cx.update(|cx| {
        let mut services = cx.global::<tf2tools_app::services::Services>().clone();
        services.picker = Arc::new(Ordered(std::sync::Mutex::new(answers.into())));
        cx.set_global(services);
    });
    click_row(cx, &s, IMAGE);
    click_in_panel(cx, &s, ids::HUD_MAKER_PICTURE_PICK.into());
    click_in_panel(cx, &s, ids::HUD_MAKER_PICTURE_PICK.into());
    support::settle(cx, s.win);
    let landed = changes(cx, &page);
    assert!(landed > 0, "the second pick landed");
    assert_eq!(first_pixel_of_kept_texture(cx, &page)[2], 250);
    // The first pick's answer comes last and is dropped.
    slow.close();
    support::settle(cx, s.win);
    assert_eq!(
        changes(cx, &page),
        landed,
        "the older pick landed after the newer"
    );
    assert_eq!(first_pixel_of_kept_texture(cx, &page)[2], 250);
}

#[gpui_kit::test]
fn without_a_background_the_corner_rows_are_disabled_and_say_why(cx: &mut TestAppContext) {
    let (_env, s, page, _) = opened(cx, vec![]);
    click_row(cx, &s, BOX);
    let corner = ids::hud_maker_choice_button("PaintBackgroundType", "2");
    let rounded = ids::hud_maker_choice_button("RoundedCorners", "3");
    assert!(!support::disabled_of(cx, s.win, corner.clone()));
    click_in_panel(cx, &s, ids::hud_maker_toggle("paintbackground"));
    support::settle(cx, s.win);
    assert_eq!(
        box_value(cx, &page, "paintbackground").as_deref(),
        Some("0")
    );
    for id in [corner.clone(), rounded.clone()] {
        assert!(
            support::disabled_of(cx, s.win, id.clone()),
            "{id:?} is enabled"
        );
    }
    // A press on a disabled corner changes nothing.
    click_in_panel(cx, &s, ids::hud_maker_choice("PaintBackgroundType", "2"));
    support::settle(cx, s.win);
    assert_eq!(box_value(cx, &page, "PaintBackgroundType"), None);
    cx.update_window(s.win, |_, w, cx| {
        w.hover(ids::hud_maker_tip_for(&corner), cx)
    })
    .unwrap();
    support::settle(cx, s.win);
    cx.executor()
        .advance_clock(std::time::Duration::from_millis(800));
    assert_eq!(
        support::label_of(cx, s.win, ids::HUD_MAKER_TIP).as_deref(),
        Some("Turn on Draw background to use corners")
    );
}
