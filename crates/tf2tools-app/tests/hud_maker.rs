//! The HUD Maker page through its pilot ids.

mod support;

use gpui_kit::component::color_picker::ColorPickerEvent;
use gpui_kit::component::input::InputEvent;
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{AppContext as _, Bounds, Entity, Pixels, TestAppContext, point, px, size};
use tf2tools_app::ids;
use tf2tools_app::pages::crosshair_maker::model::hsla_of;
use tf2tools_app::pages::hud_maker::editor::Handle;
use tf2tools_app::pages::hud_maker::view::{Panel, PanelMode, PanelPlace, Places};
use tf2tools_app::pages::hud_maker::{Field, HudMakerPage};
use tf2tools_app::services::Tf2State;
use tf2tools_app::shell::route::Page;
use tf2tools_core::{PanelRule, PartId, PartKey, Screen, Tf2Install};

const HV: &str = "HudPlayerStatus/HudPlayerHealth/PlayerStatusHealthValue";

fn found(env: &support::Env) -> Tf2State {
    Tf2State::Found(Tf2Install::from_path(&env.game).unwrap())
}

fn stock_shell(cx: &mut TestAppContext, env: &support::Env) -> support::ShellWindow {
    support::seed_stock_game(&env.game);
    support::shell_window(cx, env, found(env))
}

fn open_maker(cx: &mut TestAppContext, s: &support::ShellWindow) -> Entity<HudMakerPage> {
    support::click_nav(cx, s.win, Page::HudMaker);
    let page = cx
        .update(|cx| s.shell.read(cx).hud_maker_page())
        .expect("a HUD Maker page");
    support::wait_real(cx, "the HUD to load", |cx| {
        cx.update(|cx| page.read(cx).is_loaded())
    });
    support::settle(cx, s.win);
    page
}

fn page_now(cx: &mut TestAppContext, s: &support::ShellWindow) -> Page {
    cx.update(|cx| s.shell.read(cx).page())
}

#[gpui_kit::test]
fn the_hud_maker_sits_after_the_crosshair_maker(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, Tf2State::Missing { saved: None });
    support::settle(cx, s.win);
    let b = cx
        .update_window(s.win, |_, w, _| {
            [Page::CrosshairMaker, Page::HudMaker, Page::Settings]
                .map(|p| w.find(ids::nav(p)).bounds())
        })
        .unwrap();
    assert!(b[1].top() - b[0].bottom() < px(16.), "{b:?}");
    assert!(b[2].top() - b[1].bottom() > px(16.), "{b:?}");
    assert_eq!(
        support::label_of(cx, s.win, ids::nav(Page::HudMaker)).as_deref(),
        Some("HUD Maker")
    );
}

#[gpui_kit::test]
fn without_tf2_the_page_says_where_to_look(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, Tf2State::Missing { saved: None });
    let page = open_maker(cx, &s);
    assert!(cx.update(|cx| page.read(cx).editor().is_none()));
    assert_eq!(
        support::label_of(cx, s.win, ids::HUD_MAKER_NO_GAME).as_deref(),
        Some("TF2Tools can't find TF2's HUD files. Check the TF2 folder in Settings.")
    );
    support::click_id(cx, s.win, ids::HUD_MAKER_OPEN_SETTINGS);
    assert_eq!(page_now(cx, &s), Page::Settings);
}

#[gpui_kit::test]
fn a_tf2_folder_without_hud_files_says_the_same(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let _page = open_maker(cx, &s);
    assert!(support::present(cx, s.win, ids::HUD_MAKER_NO_GAME));
}

#[gpui_kit::test]
fn the_default_hud_loads_under_the_top_bar(cx: &mut TestAppContext) {
    let env = support::env();
    let s = stock_shell(cx, &env);
    let page = open_maker(cx, &s);
    let parts = cx.update(|cx| page.read(cx).editor().map(|e| e.model().parts().len()));
    assert_eq!(parts, Some(support::stock_model().parts().len()));
    // Only the scene tabs sit above the canvas, starting at the page's top.
    let top = bounds(cx, &s, ids::page(Page::HudMaker)).top();
    let tabs = bounds(cx, &s, ids::HUD_MAKER_SCENES);
    let area = bounds(cx, &s, ids::HUD_MAKER_CANVAS_AREA);
    assert!(f(tabs.top() - top).abs() <= 1., "{tabs:?} under {top:?}");
    assert!(
        f(area.top() - tabs.bottom()).abs() <= 1.,
        "{area:?} under {tabs:?}"
    );
    assert_eq!(
        support::label_of(cx, s.win, ids::HEADER_TITLE).as_deref(),
        Some("HUD Maker")
    );
    assert!(!support::present(cx, s.win, ids::HUD_MAKER_NO_GAME));
}

#[gpui_kit::test]
fn leaving_before_the_load_lands_is_harmless(cx: &mut TestAppContext) {
    let env = support::env();
    let s = stock_shell(cx, &env);
    // Both clicks land in one window update, so the load's result has no turn to arrive between.
    let mid_load = cx
        .update_window(s.win, |_, w, cx| {
            w.click(ids::nav(Page::HudMaker), cx);
            let page = s.shell.read(cx).hud_maker_page().expect("a HUD Maker page");
            let mid_load = !page.read(cx).is_loaded();
            w.click(ids::nav(Page::Huds), cx);
            mid_load
        })
        .unwrap();
    assert!(mid_load, "the load landed before the page was left");
    support::settle(cx, s.win);
    assert_eq!(page_now(cx, &s), Page::Huds);
    assert!(cx.update(|cx| s.shell.read(cx).hud_maker_page()).is_none());
}

#[gpui_kit::test]
fn leaving_the_maker_frees_its_drawn_images(cx: &mut TestAppContext) {
    let env = support::env();
    let health = support::hud_stock::HEALTH.replace(
        "    wide 51\n    tall 51\n",
        "    wide 51\n    tall 51\n    image ../hud/health_bg\n",
    );
    let vmt: &[u8] = br#""UnlitGeneric" { "$baseTexture" "hud/health_bg" }"#;
    let red = tf2tools_core::RgbaImage::from_pixels(2, 2, [255, 0, 0, 255].repeat(4)).unwrap();
    let vtf = tf2tools_core::rgba_to_vtf(&red).unwrap();
    support::seed_stock_game_with_bytes(
        &env.game,
        &[("resource/ui/hudplayerhealth.res", Some(&health))],
        &[
            ("materials/hud/health_bg.vmt", vmt),
            ("materials/hud/health_bg.vtf", &vtf),
        ],
    );
    let s = support::shell_window(cx, &env, found(&env));
    let page = open_maker(cx, &s);
    let key = PartKey::new("HudPlayerStatus/HudPlayerHealth/PlayerStatusHealthImage");
    let image = cx
        .update(|cx| page.read(cx).image(&key))
        .expect("the health image decoded");
    let drawn = |cx: &mut TestAppContext| {
        cx.update_window(s.win, |_, w, _| w.has_image_atlas_entry(&image))
            .unwrap()
    };
    assert!(drawn(cx), "the image was never drawn");
    drop(page);
    support::click_id(cx, s.win, ids::nav(Page::Huds));
    support::settle(cx, s.win);
    assert!(!drawn(cx), "the image stayed in GPU memory after leaving");
}

#[gpui_kit::test]
fn a_clean_maker_leaves_without_asking(cx: &mut TestAppContext) {
    let env = support::env();
    let s = stock_shell(cx, &env);
    let _page = open_maker(cx, &s);
    support::click_id(cx, s.win, ids::nav(Page::Huds));
    assert!(!support::present(cx, s.win, ids::CONFIRM_DIALOG));
    assert_eq!(page_now(cx, &s), Page::Huds);
}

#[gpui_kit::test]
fn leaving_with_unsaved_changes_asks_first(cx: &mut TestAppContext) {
    let env = support::env();
    let s = stock_shell(cx, &env);
    let page = open_maker(cx, &s);
    cx.update(|cx| {
        page.update(cx, |p, cx| {
            let e = p.editor_mut().unwrap();
            e.select(Some(PartKey::new(HV)));
            e.move_selected(45, 0).unwrap();
            cx.notify();
        })
    });
    support::click_id(cx, s.win, ids::nav(Page::Huds));
    assert!(support::present(cx, s.win, ids::CONFIRM_DIALOG));
    assert_eq!(page_now(cx, &s), Page::HudMaker);
    support::click_id(cx, s.win, ids::CONFIRM_OK);
    assert_eq!(page_now(cx, &s), Page::Huds);
}

// Keep editing, by button or Esc, hands the keys back to the maker without a click on it.
#[gpui_kit::test]
fn keep_editing_gives_the_keys_back_to_the_maker(cx: &mut TestAppContext) {
    let env = support::env();
    let s = stock_shell(cx, &env);
    let page = open_maker(cx, &s);
    let from = hv_box(cx, &page).0;
    for dismiss in ["button", "escape"] {
        let at = bounds(cx, &s, ids::hud_maker_part(&PartKey::new(HV))).center();
        let k = scale(cx, &s);
        drag_window(cx, &s, at, (45. * k, 0.));
        assert_eq!(hv_box(cx, &page).0, from + 45);
        support::click_id(cx, s.win, ids::nav(Page::Huds));
        assert!(support::present(cx, s.win, ids::CONFIRM_DIALOG));
        match dismiss {
            "button" => support::click_id(cx, s.win, ids::CONFIRM_CANCEL),
            _ => support::press(cx, s.win, "escape"),
        }
        support::settle(cx, s.win);
        assert!(!support::present(cx, s.win, ids::CONFIRM_DIALOG));
        assert_eq!(page_now(cx, &s), Page::HudMaker);
        support::press(cx, s.win, "ctrl-z");
        assert_eq!(hv_box(cx, &page).0, from, "{dismiss}: Ctrl+Z was lost");
    }
}

fn bounds(
    cx: &mut TestAppContext,
    s: &support::ShellWindow,
    id: impl Into<gpui_kit::ElementId>,
) -> Bounds<Pixels> {
    let id = id.into();
    support::bounds_of(cx, s.win, id.clone()).unwrap_or_else(|| panic!("no {id:?}"))
}

// Where the selected HV sits on the canvas. Moved out of its panel it paints nothing, as in the
// game, but the selection outline keeps its whole box.
fn hv_outline(cx: &mut TestAppContext, s: &support::ShellWindow) -> Bounds<Pixels> {
    bounds(cx, s, ids::HUD_MAKER_SELECTION)
}

fn f(p: Pixels) -> f32 {
    f32::from(p)
}

fn selection(cx: &mut TestAppContext, page: &Entity<HudMakerPage>) -> Option<String> {
    cx.update(|cx| {
        page.read(cx)
            .editor()?
            .selected()
            .map(|k| k.as_str().to_owned())
    })
}

/// Clicks the canvas at screen pixel (x, y).
fn click_screen(cx: &mut TestAppContext, s: &support::ShellWindow, x: f32, y: f32) {
    let canvas = bounds(cx, s, ids::HUD_MAKER_CANVAS);
    let scale = f(canvas.size.width) / 1920.;
    let at = point(px(x * scale), px(y * scale));
    cx.update_window(s.win, |_, w, cx| w.click_at(ids::HUD_MAKER_CANVAS, at, cx))
        .unwrap();
    support::settle(cx, s.win);
}

#[gpui_kit::test]
fn the_canvas_keeps_the_screen_shape_and_places_parts_in_scale(cx: &mut TestAppContext) {
    let env = support::env();
    let s = stock_shell(cx, &env);
    let _page = open_maker(cx, &s);
    let canvas = bounds(cx, &s, ids::HUD_MAKER_CANVAS);
    let area = bounds(cx, &s, ids::HUD_MAKER_CANVAS_AREA);
    assert!(canvas.size.width <= area.size.width && canvas.size.height <= area.size.height);
    assert!(
        (f(canvas.size.height) / f(canvas.size.width) - 1080. / 1920.).abs() < 0.01,
        "{canvas:?}"
    );
    let scale = f(canvas.size.width) / 1920.;
    let part = bounds(cx, &s, ids::hud_maker_part(&PartKey::new(HV)));
    assert!(
        (f(part.origin.x - canvas.origin.x) - 171. * scale).abs() <= 1.,
        "{part:?}"
    );
    assert!(
        (f(part.origin.y - canvas.origin.y) - 927. * scale).abs() <= 1.,
        "{part:?}"
    );
    assert!((f(part.size.width) - 99. * scale).abs() <= 1., "{part:?}");
    let mark = bounds(cx, &s, ids::HUD_MAKER_CROSSHAIR);
    assert!(f(mark.center().x - canvas.center().x).abs() <= 1.);
    assert!(f(mark.center().y - canvas.center().y).abs() <= 1.);
}

#[gpui_kit::test]
fn a_click_selects_the_part_under_the_pointer_and_a_miss_clears(cx: &mut TestAppContext) {
    let env = support::env();
    let s = stock_shell(cx, &env);
    let page = open_maker(cx, &s);
    click_screen(cx, &s, 220., 947.);
    assert_eq!(
        selection(cx, &page).as_deref(),
        Some(HH),
        "the part's group first"
    );
    press_screen(cx, &s, (220., 947.), alt());
    assert_eq!(selection(cx, &page).as_deref(), Some(HV));
    show_parts(cx, &s);
    assert_eq!(
        support::selected_of(cx, s.win, ids::hud_maker_row(&PartKey::new(HV))),
        Some(true)
    );
    hide_parts(cx, &s);
    let sel = bounds(cx, &s, ids::HUD_MAKER_SELECTION);
    let part = bounds(cx, &s, ids::hud_maker_part(&PartKey::new(HV)));
    assert!(
        f(sel.origin.x - part.origin.x).abs() <= 1.
            && f(sel.size.width - part.size.width).abs() <= 1.
    );
    click_screen(cx, &s, 1900., 20.);
    assert_eq!(selection(cx, &page), None);
    assert!(!support::present(cx, s.win, ids::HUD_MAKER_SELECTION));
}

#[gpui_kit::test]
fn rows_name_the_parts_and_select_them(cx: &mut TestAppContext) {
    let env = support::env();
    let s = stock_shell(cx, &env);
    let page = open_maker(cx, &s);
    show_parts(cx, &s);
    let health = PartKey::new("HudPlayerStatus/HudPlayerHealth");
    assert_eq!(
        support::label_of(cx, s.win, ids::hud_maker_row(&health)).as_deref(),
        Some(PanelRule::get(PartId::Health).label())
    );
    assert_eq!(
        support::label_of(cx, s.win, ids::hud_maker_row(&PartKey::new(HV))).as_deref(),
        Some("Health value")
    );
    support::click_part_row(cx, s.win, &PartKey::new(HV));
    assert_eq!(selection(cx, &page).as_deref(), Some(HV));
}

#[gpui_kit::test]
fn the_screen_buttons_change_the_shape(cx: &mut TestAppContext) {
    let env = support::env();
    let s = stock_shell(cx, &env);
    let page = open_maker(cx, &s);
    assert_eq!(
        support::label_of(cx, s.win, ids::HUD_MAKER_SCREEN_MENU).as_deref(),
        Some("16:9")
    );
    support::click_id(cx, s.win, ids::HUD_MAKER_SCREEN_MENU);
    let labels: Vec<_> = Screen::PRESETS
        .iter()
        .map(|&p| support::label_of(cx, s.win, ids::hud_maker_screen(p)).unwrap())
        .collect();
    assert_eq!(labels, ["16:9", "16:10", "4:3", "5:4", "21:9", "32:9"]);
    support::click_id(cx, s.win, ids::hud_maker_screen(Screen::new(1440, 1080)));
    assert!(!support::present(
        cx,
        s.win,
        ids::hud_maker_screen(Screen::PRESETS[0])
    ));
    assert_eq!(
        support::label_of(cx, s.win, ids::HUD_MAKER_SCREEN_MENU).as_deref(),
        Some("4:3")
    );
    assert_eq!(
        cx.update(|cx| page.read(cx).editor().unwrap().screen()),
        Screen::new(1440, 1080)
    );
    let canvas = bounds(cx, &s, ids::HUD_MAKER_CANVAS);
    assert!(
        (f(canvas.size.height) / f(canvas.size.width) - 0.75).abs() < 0.01,
        "{canvas:?}"
    );
}

#[gpui_kit::test]
fn a_hidden_part_leaves_the_canvas_but_keeps_its_row(cx: &mut TestAppContext) {
    let env = support::env();
    let s = stock_shell(cx, &env);
    let page = open_maker(cx, &s);
    cx.update(|cx| {
        page.update(cx, |p, cx| {
            let e = p.editor_mut().unwrap();
            e.select(Some(PartKey::new(HV)));
            e.hide_selected().unwrap();
            cx.notify();
        })
    });
    assert!(!support::present(
        cx,
        s.win,
        ids::hud_maker_part(&PartKey::new(HV))
    ));
    show_parts(cx, &s);
    assert_eq!(
        support::label_of(cx, s.win, ids::hud_maker_row(&PartKey::new(HV))).as_deref(),
        Some("Health value (hidden)")
    );
}

#[gpui_kit::test]
fn pinned_parts_show_a_pin(cx: &mut TestAppContext) {
    use support::hud_stock::{HEALTH, label, with_blocks};
    let env = support::env();
    let pinned = with_blocks(
        HEALTH,
        &label(
            "Pinned",
            "0",
            "0",
            &[("pin_to_sibling", "PlayerStatusHealthValue")],
        ),
    );
    support::seed_stock_game_with(
        &env.game,
        &[("resource/ui/hudplayerhealth.res", Some(pinned.as_str()))],
    );
    let s = support::shell_window(cx, &env, found(&env));
    let _page = open_maker(cx, &s);
    assert!(support::present(
        cx,
        s.win,
        ids::hud_maker_pin(&PartKey::new("HudPlayerStatus/HudPlayerHealth/Pinned"))
    ));
    assert!(!support::present(
        cx,
        s.win,
        ids::hud_maker_pin(&PartKey::new(HV))
    ));
}

fn saved_panel(s: &support::ShellWindow) -> Option<String> {
    s.services
        .db
        .settings()
        .unwrap()
        .hud_maker_panel()
        .map(str::to_owned)
}

fn saved_places(s: &support::ShellWindow) -> Option<Places> {
    saved_panel(s).as_deref().and_then(Places::from_setting)
}

#[gpui_kit::test]
fn the_panel_floats_at_its_default_place(cx: &mut TestAppContext) {
    let env = support::env();
    let s = stock_shell(cx, &env);
    let page = open_maker(cx, &s);
    support::maker_page::float_panels(cx, &s, &page);
    let panel = bounds(cx, &s, ids::HUD_MAKER_PANEL);
    let area = bounds(cx, &s, ids::HUD_MAKER_CANVAS_AREA);
    assert_eq!(panel.origin, area.origin + point(px(16.), px(16.)));
}

#[gpui_kit::test]
fn docking_right_puts_the_panel_beside_the_canvas_and_is_remembered(cx: &mut TestAppContext) {
    let env = support::env();
    let s = stock_shell(cx, &env);
    let _page = open_maker(cx, &s);
    support::click_id(cx, s.win, ids::HUD_MAKER_PANEL_DOCK_RIGHT);
    let beside = |cx: &mut TestAppContext| {
        let panel = bounds(cx, &s, ids::HUD_MAKER_PANEL);
        let area = bounds(cx, &s, ids::HUD_MAKER_CANVAS_AREA);
        let canvas = bounds(cx, &s, ids::HUD_MAKER_CANVAS);
        f(panel.right() - area.right()).abs() <= 1. && canvas.right() <= panel.left()
    };
    assert!(beside(cx));
    support::wait_real(cx, "the setting", |_| {
        saved_panel(&s).is_some_and(|p| p.starts_with("right "))
    });
    support::click_id(cx, s.win, ids::nav(Page::Huds));
    let _page = open_maker(cx, &s);
    assert!(beside(cx));
}

#[gpui_kit::test]
fn collapsing_keeps_only_the_title_bar(cx: &mut TestAppContext) {
    let env = support::env();
    let s = stock_shell(cx, &env);
    let page = open_maker(cx, &s);
    support::maker_page::float_panels(cx, &s, &page);
    assert!(support::present(cx, s.win, ids::HUD_MAKER_PANEL_BODY));
    support::click_id(cx, s.win, ids::HUD_MAKER_PANEL_COLLAPSE);
    assert!(!support::present(cx, s.win, ids::HUD_MAKER_PANEL_BODY));
    assert!(support::present(cx, s.win, ids::HUD_MAKER_PANEL_TITLE));
    support::wait_real(cx, "the setting", |_| {
        saved_places(&s).is_some_and(|p| p.options.collapsed)
    });
    support::click_id(cx, s.win, ids::HUD_MAKER_PANEL_COLLAPSE);
    assert!(support::present(cx, s.win, ids::HUD_MAKER_PANEL_BODY));
}

#[gpui_kit::test]
fn dragging_the_title_bar_moves_a_floating_panel(cx: &mut TestAppContext) {
    let env = support::env();
    let s = stock_shell(cx, &env);
    let page = open_maker(cx, &s);
    support::maker_page::float_panels(cx, &s, &page);
    let from = bounds(cx, &s, ids::HUD_MAKER_PANEL_TITLE).center();
    let to = from + point(px(200.), px(100.));
    cx.update_window(s.win, |_, w, cx| w.drag(from, to, cx))
        .unwrap();
    support::settle(cx, s.win);
    let panel = bounds(cx, &s, ids::HUD_MAKER_PANEL);
    let area = bounds(cx, &s, ids::HUD_MAKER_CANVAS_AREA);
    assert!(
        f(panel.left() - area.left() - px(216.)).abs() <= 1.,
        "{panel:?}"
    );
    assert!(
        f(panel.top() - area.top() - px(116.)).abs() <= 1.,
        "{panel:?}"
    );
    support::wait_real(cx, "the setting", |_| {
        saved_places(&s).is_some_and(|p| p.options.to_setting() == "floating 216 116 open")
    });
}

#[gpui_kit::test]
fn the_panel_moves_off_the_selected_part_without_saving_that(cx: &mut TestAppContext) {
    let env = support::env();
    let s = stock_shell(cx, &env);
    let page = open_maker(cx, &s);
    support::maker_page::float_panels(cx, &s, &page);
    let hv = bounds(cx, &s, ids::hud_maker_part(&PartKey::new(HV)));
    let area = bounds(cx, &s, ids::HUD_MAKER_CANVAS_AREA);
    let over = PanelPlace {
        mode: PanelMode::Floating,
        x: f(hv.left() - area.left()),
        y: f(hv.top() - area.top()) - 10.,
        collapsed: false,
    };
    cx.update(|cx| page.update(cx, |p, cx| p.set_panel_place(over, cx)));
    support::settle(cx, s.win);
    let placed = cx.update(|cx| page.read(cx).panel_place());
    click_row(cx, &s, &PartKey::new(HV));
    let panel = bounds(cx, &s, ids::HUD_MAKER_PANEL);
    assert!(!panel.intersects(&hv), "{panel:?} still covers {hv:?}");
    support::wait_real(cx, "the setting", |_| {
        saved_places(&s).is_some_and(|p| p.options.to_setting() == placed.to_setting())
    });
}

#[gpui_kit::test]
fn pressing_the_title_without_moving_keeps_the_players_place(cx: &mut TestAppContext) {
    let env = support::env();
    let s = stock_shell(cx, &env);
    let page = open_maker(cx, &s);
    let hv = bounds(cx, &s, ids::hud_maker_part(&PartKey::new(HV)));
    let area = bounds(cx, &s, ids::HUD_MAKER_CANVAS_AREA);
    let over = PanelPlace {
        mode: PanelMode::Floating,
        x: f(hv.left() - area.left()),
        y: f(hv.top() - area.top()) - 10.,
        collapsed: false,
    };
    cx.update(|cx| page.update(cx, |p, cx| p.set_panel_place(over, cx)));
    support::settle(cx, s.win);
    let placed = cx.update(|cx| page.read(cx).panel_place());
    click_row(cx, &s, &PartKey::new(HV));
    let moved_off = bounds(cx, &s, ids::HUD_MAKER_PANEL);
    support::click_id(cx, s.win, ids::HUD_MAKER_PANEL_TITLE);
    support::settle(cx, s.win);
    assert_eq!(cx.update(|cx| page.read(cx).panel_place()), placed);
    assert_eq!(bounds(cx, &s, ids::HUD_MAKER_PANEL), moved_off);
}

#[gpui_kit::test]
fn a_panel_placed_past_the_edge_stays_inside_the_area(cx: &mut TestAppContext) {
    let env = support::env();
    let s = stock_shell(cx, &env);
    let page = open_maker(cx, &s);
    let far = PanelPlace {
        mode: PanelMode::Floating,
        x: 10_000.,
        y: 0.,
        collapsed: true,
    };
    cx.update(|cx| page.update(cx, |p, cx| p.set_panel_place(far, cx)));
    support::settle(cx, s.win);
    let panel = bounds(cx, &s, ids::HUD_MAKER_PANEL);
    let area = bounds(cx, &s, ids::HUD_MAKER_CANVAS_AREA);
    assert!(panel.right() <= area.right(), "{panel:?} past {area:?}");
}

#[gpui_kit::test]
fn clicks_on_the_panel_keep_the_selection(cx: &mut TestAppContext) {
    let env = support::env();
    let s = stock_shell(cx, &env);
    let page = open_maker(cx, &s);
    click_row(cx, &s, &PartKey::new(HV));
    let area = bounds(cx, &s, ids::HUD_MAKER_CANVAS_AREA);
    let middle = PanelPlace {
        mode: PanelMode::Floating,
        x: f(area.size.width) / 2. - 100.,
        y: f(area.size.height) / 2. - 100.,
        collapsed: false,
    };
    cx.update(|cx| page.update(cx, |p, cx| p.set_panel_place(middle, cx)));
    support::settle(cx, s.win);
    let canvas = bounds(cx, &s, ids::HUD_MAKER_CANVAS);
    assert!(bounds(cx, &s, ids::HUD_MAKER_PANEL).intersects(&canvas));
    support::click_id(cx, s.win, ids::HUD_MAKER_PANEL_BODY);
    assert_eq!(selection(cx, &page).as_deref(), Some(HV));
    support::click_id(cx, s.win, ids::HUD_MAKER_PANEL_COLLAPSE);
    assert_eq!(selection(cx, &page).as_deref(), Some(HV));
}

fn hv_box(cx: &mut TestAppContext, page: &Entity<HudMakerPage>) -> (i32, i32, i32, i32) {
    cx.update(|cx| {
        let b = page
            .read(cx)
            .editor()
            .unwrap()
            .layout()
            .get(&PartKey::new(HV))
            .unwrap()
            .clone();
        (b.x(), b.y(), b.wide(), b.tall())
    })
}

fn hv_value(cx: &mut TestAppContext, page: &Entity<HudMakerPage>, key: &str) -> Option<String> {
    cx.update(|cx| {
        let e = page.read(cx).editor().unwrap();
        e.model()
            .part(&PartKey::new(HV))?
            .value(key)
            .map(|l| l.text().to_owned())
    })
}

fn commit(
    cx: &mut TestAppContext,
    s: &support::ShellWindow,
    page: &Entity<HudMakerPage>,
    field: Field,
    text: &str,
) {
    let input = cx.update(|cx| page.read(cx).field(field).clone());
    cx.update_window(s.win, |_, window, cx| {
        input.update(cx, |st, cx| {
            st.set_value(text, window, cx);
            cx.emit(InputEvent::PressEnter {
                secondary: false,
                shift: false,
            });
        })
    })
    .unwrap();
    support::settle(cx, s.win);
}

fn show_parts(cx: &mut TestAppContext, s: &support::ShellWindow) {
    if !support::present(cx, s.win, ids::HUD_MAKER_PARTS) {
        support::click_id(cx, s.win, ids::HUD_MAKER_PARTS_PANEL_COLLAPSE);
    }
}

fn hide_parts(cx: &mut TestAppContext, s: &support::ShellWindow) {
    if support::present(cx, s.win, ids::HUD_MAKER_PARTS) {
        support::click_id(cx, s.win, ids::HUD_MAKER_PARTS_PANEL_COLLAPSE);
    }
}

/// Clicks `key`'s row with the parts panel opened for the click and closed after it.
fn click_row(cx: &mut TestAppContext, s: &support::ShellWindow, key: &PartKey) {
    show_parts(cx, s);
    support::click_part_row(cx, s.win, key);
    hide_parts(cx, s);
}

fn select_hv(cx: &mut TestAppContext, s: &support::ShellWindow) {
    click_row(cx, s, &PartKey::new(HV));
}

#[gpui_kit::test]
fn the_panel_names_the_part_and_when_it_shows(cx: &mut TestAppContext) {
    let env = support::env();
    let s = stock_shell(cx, &env);
    let page = open_maker(cx, &s);
    assert_eq!(
        support::label_of(cx, s.win, ids::HUD_MAKER_PANEL_EMPTY).as_deref(),
        Some("Click a part of the HUD to change it.")
    );
    select_hv(cx, &s);
    assert_eq!(
        support::label_of(cx, s.win, ids::HUD_MAKER_PANEL_PART).as_deref(),
        Some("Health value")
    );
    assert_eq!(
        support::label_of(cx, s.win, ids::HUD_MAKER_PANEL_SHOWS).as_deref(),
        Some(PanelRule::get(PartId::Health).shows_when())
    );
    let x = cx.update(|cx| page.read(cx).field(Field::X).read(cx).value().to_string());
    assert_eq!(x, "171");
}

#[gpui_kit::test]
fn typing_a_position_or_size_moves_and_resizes_the_part(cx: &mut TestAppContext) {
    let env = support::env();
    let s = stock_shell(cx, &env);
    let page = open_maker(cx, &s);
    select_hv(cx, &s);
    commit(cx, &s, &page, Field::X, "216");
    assert_eq!(hv_box(cx, &page).0, 216);
    commit(cx, &s, &page, Field::Wide, "135");
    assert_eq!(hv_box(cx, &page).2, 135);
    commit(cx, &s, &page, Field::Y, "not a number");
    assert_eq!(hv_box(cx, &page).1, 927);
}

#[gpui_kit::test]
fn a_typed_value_goes_to_its_part_when_the_selection_moves_on(cx: &mut TestAppContext) {
    let env = support::env();
    let s = stock_shell(cx, &env);
    let page = open_maker(cx, &s);
    select_hv(cx, &s);
    show_parts(cx, &s);
    let clip = PartKey::new("HudWeaponAmmo/AmmoInClip");
    let before = cx.update(|cx| {
        page.read(cx)
            .editor()
            .unwrap()
            .layout()
            .get(&clip)
            .unwrap()
            .clone()
    });
    let input = cx.update(|cx| page.read(cx).field(Field::X).clone());
    cx.update_window(s.win, |_, window, cx| {
        input.update(cx, |st, cx| st.set_value("216", window, cx))
    })
    .unwrap();
    support::click_part_row(cx, s.win, &clip);
    assert_eq!(hv_box(cx, &page).0, 216);
    let after = cx.update(|cx| {
        page.read(cx)
            .editor()
            .unwrap()
            .layout()
            .get(&clip)
            .unwrap()
            .clone()
    });
    assert_eq!(after, before);
    let x = cx.update(|cx| page.read(cx).field(Field::X).read(cx).value().to_string());
    assert_eq!(x, before.x().to_string());
}

#[gpui_kit::test]
fn the_panel_keeps_clear_after_a_typed_move(cx: &mut TestAppContext) {
    let env = support::env();
    let s = stock_shell(cx, &env);
    let page = open_maker(cx, &s);
    select_hv(cx, &s);
    let canvas = bounds(cx, &s, ids::HUD_MAKER_CANVAS);
    let area = bounds(cx, &s, ids::HUD_MAKER_CANVAS_AREA);
    let k = f(canvas.size.width) / 1920.;
    let hv = bounds(cx, &s, ids::hud_maker_part(&PartKey::new(HV)));
    let target = Bounds {
        origin: point(canvas.origin.x + px(600. * k), hv.origin.y),
        size: hv.size,
    };
    let over = PanelPlace {
        mode: PanelMode::Floating,
        x: f(target.left() - area.left()) - 10.,
        y: f(target.top() - area.top()) - 10.,
        collapsed: false,
    };
    cx.update(|cx| page.update(cx, |p, cx| p.set_panel_place(over, cx)));
    support::settle(cx, s.win);
    assert!(
        bounds(cx, &s, ids::HUD_MAKER_PANEL).intersects(&target),
        "the setup must cover the target"
    );
    commit(cx, &s, &page, Field::X, "600");
    let moved = hv_outline(cx, &s);
    assert!(!bounds(cx, &s, ids::HUD_MAKER_PANEL).intersects(&moved));
}

#[gpui_kit::test]
fn layer_and_colour_write_their_keys(cx: &mut TestAppContext) {
    let env = support::env();
    let s = stock_shell(cx, &env);
    let page = open_maker(cx, &s);
    select_hv(cx, &s);
    commit(cx, &s, &page, Field::Layer, "9");
    assert_eq!(hv_value(cx, &page, "zpos").as_deref(), Some("9"));
    let state = cx.update(|cx| page.read(cx).colour_state().clone());
    cx.update(|cx| {
        state.update(cx, |_, cx| {
            cx.emit(ColorPickerEvent::Change(Some(hsla_of([255, 80, 80]))))
        })
    });
    support::settle(cx, s.win);
    assert_eq!(
        hv_value(cx, &page, "fgcolor").as_deref(),
        Some("255 80 80 255")
    );
    support::click_id(cx, s.win, ids::HUD_MAKER_COLOUR_NAME);
    support::click_id(cx, s.win, ids::hud_maker_colour_name("TanLight"));
    assert_eq!(hv_value(cx, &page, "fgcolor").as_deref(), Some("TanLight"));
}

#[gpui_kit::test]
fn hide_toggles_the_part(cx: &mut TestAppContext) {
    let env = support::env();
    let s = stock_shell(cx, &env);
    let page = open_maker(cx, &s);
    select_hv(cx, &s);
    support::click_id(cx, s.win, ids::HUD_MAKER_HIDE);
    assert!(!support::present(
        cx,
        s.win,
        ids::hud_maker_part(&PartKey::new(HV))
    ));
    support::click_id(cx, s.win, ids::HUD_MAKER_HIDE);
    assert!(support::present(
        cx,
        s.win,
        ids::hud_maker_part(&PartKey::new(HV))
    ));
    assert!(cx.update(|cx| page.read(cx).editor().unwrap().changes().is_empty()));
}

#[gpui_kit::test]
fn reset_drops_that_propertys_changes(cx: &mut TestAppContext) {
    let env = support::env();
    let s = stock_shell(cx, &env);
    let page = open_maker(cx, &s);
    select_hv(cx, &s);
    let reset = ids::hud_maker_reset("xpos");
    assert!(support::disabled_of(cx, s.win, reset.clone()));
    commit(cx, &s, &page, Field::X, "216");
    assert!(!support::disabled_of(cx, s.win, reset.clone()));
    support::click_id(cx, s.win, reset);
    assert_eq!(hv_box(cx, &page).0, 171);
}

#[gpui_kit::test]
fn rows_follow_the_class(cx: &mut TestAppContext) {
    let env = support::env();
    let s = stock_shell(cx, &env);
    let _page = open_maker(cx, &s);
    select_hv(cx, &s);
    assert!(support::present(cx, s.win, ids::HUD_MAKER_COLOUR));
    click_row(cx, &s, &PartKey::new("HudWeaponAmmo"));
    assert!(!support::present(cx, s.win, ids::HUD_MAKER_COLOUR));
    assert!(support::present(cx, s.win, ids::HUD_MAKER_LAYER));
}

#[gpui_kit::test]
fn an_unreadable_part_says_so_and_offers_no_rows(cx: &mut TestAppContext) {
    let env = support::env();
    support::seed_stock_game_with(&env.game, &[("resource/ui/hudammoweapons.res", None)]);
    let s = support::shell_window(cx, &env, found(&env));
    let _page = open_maker(cx, &s);
    click_row(cx, &s, &PartKey::new("HudWeaponAmmo"));
    assert_eq!(
        support::label_of(cx, s.win, ids::HUD_MAKER_PANEL_UNREADABLE).as_deref(),
        Some("Can't show this part of the HUD")
    );
    assert!(!support::present(cx, s.win, ids::HUD_MAKER_LAYER));
}

/// Types `text` into `field` without committing it.
fn type_only(
    cx: &mut TestAppContext,
    s: &support::ShellWindow,
    page: &Entity<HudMakerPage>,
    field: Field,
    text: &str,
) {
    let input = cx.update(|cx| page.read(cx).field(field).clone());
    cx.update_window(s.win, |_, window, cx| {
        input.update(cx, |st, cx| st.set_value(text, window, cx))
    })
    .unwrap();
}

fn field_text(cx: &mut TestAppContext, page: &Entity<HudMakerPage>, field: Field) -> String {
    cx.update(|cx| page.read(cx).field(field).read(cx).value().to_string())
}

fn undo(cx: &mut TestAppContext, page: &Entity<HudMakerPage>) -> bool {
    cx.update(|cx| page.update(cx, |p, _| p.editor_mut().unwrap().undo()))
}

#[gpui_kit::test]
fn reset_discards_a_value_still_being_typed(cx: &mut TestAppContext) {
    let env = support::env();
    let s = stock_shell(cx, &env);
    let page = open_maker(cx, &s);
    select_hv(cx, &s);
    commit(cx, &s, &page, Field::X, "216");
    type_only(cx, &s, &page, Field::X, "300");
    support::click_id(cx, s.win, ids::hud_maker_reset("xpos"));
    assert_eq!(hv_box(cx, &page).0, 171);
    assert_eq!(field_text(cx, &page, Field::X), "171");
}

#[gpui_kit::test]
fn a_typed_value_commits_once_across_a_screen_change(cx: &mut TestAppContext) {
    let env = support::env();
    let s = stock_shell(cx, &env);
    let page = open_maker(cx, &s);
    let clip = PartKey::new("HudWeaponAmmo/AmmoInClip");
    click_row(cx, &s, &clip);
    let clip_x = |cx: &mut TestAppContext| {
        cx.update(|cx| {
            page.read(cx)
                .editor()
                .unwrap()
                .layout()
                .get(&clip)
                .unwrap()
                .x()
        })
    };
    let x0 = clip_x(cx);
    type_only(cx, &s, &page, Field::X, &(x0 - 54).to_string());
    pick_screen(cx, &s, Screen::PRESETS[2]);
    pick_screen(cx, &s, Screen::PRESETS[0]);
    // One file unit is 2.25 px at 1080 tall, so the part lands within a unit of the typed x.
    let x = clip_x(cx);
    assert!((x - (x0 - 54)).abs() <= 2, "{x} for {}", x0 - 54);
    assert!(undo(cx, &page));
    assert!(!cx.update(|cx| page.read(cx).editor().unwrap().can_undo()));
}

#[gpui_kit::test]
fn a_typed_value_commits_once_when_the_canvas_is_clicked(cx: &mut TestAppContext) {
    let env = support::env();
    let s = stock_shell(cx, &env);
    let page = open_maker(cx, &s);
    select_hv(cx, &s);
    type_only(cx, &s, &page, Field::X, "216");
    click_screen(cx, &s, 1900., 20.);
    assert_eq!(hv_box(cx, &page).0, 216);
    assert!(undo(cx, &page));
    assert!(!cx.update(|cx| page.read(cx).editor().unwrap().can_undo()));
    assert_eq!(hv_box(cx, &page).0, 171);
}

#[gpui_kit::test]
fn a_picker_drag_is_one_undo_step(cx: &mut TestAppContext) {
    let env = support::env();
    let s = stock_shell(cx, &env);
    let page = open_maker(cx, &s);
    select_hv(cx, &s);
    let before = hv_value(cx, &page, "fgcolor");
    let state = cx.update(|cx| page.read(cx).colour_state().clone());
    for rgb in [[255, 80, 80], [200, 80, 80], [150, 80, 80]] {
        cx.update(|cx| {
            state.update(cx, |_, cx| {
                cx.emit(ColorPickerEvent::Change(Some(hsla_of(rgb))))
            })
        });
        support::settle(cx, s.win);
    }
    assert_eq!(
        hv_value(cx, &page, "fgcolor").as_deref(),
        Some("150 80 80 255")
    );
    assert!(undo(cx, &page));
    assert!(!cx.update(|cx| page.read(cx).editor().unwrap().can_undo()));
    assert_eq!(hv_value(cx, &page, "fgcolor"), before);
}

#[gpui_kit::test]
fn out_of_range_numbers_are_refused_and_the_field_restored(cx: &mut TestAppContext) {
    let env = support::env();
    let s = stock_shell(cx, &env);
    let page = open_maker(cx, &s);
    select_hv(cx, &s);
    let start = hv_box(cx, &page);
    for (field, text) in [
        (Field::X, "-2147483648"),
        (Field::Y, "2147483647"),
        (Field::X, "7681"),
        (Field::X, "-7681"),
        (Field::Y, "4321"),
        (Field::Wide, "0"),
        (Field::Wide, "-5"),
        (Field::Wide, "7681"),
        (Field::Tall, "4321"),
        (Field::Wide, "1"),
        (Field::Tall, "1"),
        (Field::Layer, "99999999999"),
    ] {
        let shown = field_text(cx, &page, field);
        commit(cx, &s, &page, field, text);
        assert_eq!(hv_box(cx, &page), start, "{field:?} {text}");
        assert_eq!(field_text(cx, &page, field), shown, "{field:?} {text}");
    }
    assert!(cx.update(|cx| page.read(cx).editor().unwrap().changes().is_empty()));
    commit(cx, &s, &page, Field::Wide, "7677");
    assert_eq!(hv_box(cx, &page).2, 7677);
    commit(cx, &s, &page, Field::Wide, "7680");
    assert_eq!(
        hv_box(cx, &page).2,
        7679,
        "the edge itself is accepted, to the nearest unit"
    );
}

fn note_under(
    cx: &mut TestAppContext,
    s: &support::ShellWindow,
    of: &'static str,
) -> Option<String> {
    support::label_of(cx, s.win, ids::hud_maker_note_for(&of.into()))
}

#[gpui_kit::test]
fn a_rounded_number_says_where_it_landed_until_the_next_good_one(cx: &mut TestAppContext) {
    let env = support::env();
    let s = stock_shell(cx, &env);
    let page = open_maker(cx, &s);
    select_hv(cx, &s);
    commit(cx, &s, &page, Field::X, "301");
    let x = hv_box(cx, &page).0;
    assert_ne!(x, 301, "the file holds coarser steps than one pixel");
    assert_eq!(
        note_under(cx, &s, ids::HUD_MAKER_X),
        Some(format!(
            "Rounded to {x}, the nearest step the HUD file can hold"
        ))
    );
    // A run of typed values hits steps and misses them; only a miss leaves a note.
    let (mut hits, mut misses) = (0, 0);
    for typed in 200..=210 {
        commit(cx, &s, &page, Field::X, &typed.to_string());
        let x = hv_box(cx, &page).0;
        let note = note_under(cx, &s, ids::HUD_MAKER_X);
        if x == typed {
            hits += 1;
            assert_eq!(note, None, "{typed}");
        } else {
            misses += 1;
            assert!(note.is_some_and(|n| n.starts_with(&format!("Rounded to {x},"))));
        }
    }
    assert!(hits > 0 && misses > 0, "{hits} hits, {misses} misses");
}

#[gpui_kit::test]
fn a_refused_number_says_why_until_the_next_good_one(cx: &mut TestAppContext) {
    let env = support::env();
    let s = stock_shell(cx, &env);
    let page = open_maker(cx, &s);
    select_hv(cx, &s);
    for (field, id, text, said) in [
        (
            Field::Y,
            ids::HUD_MAKER_Y,
            "5000",
            "Must be between -4320 and 4320",
        ),
        (Field::X, ids::HUD_MAKER_X, "abc", "Whole numbers only"),
        (Field::X, ids::HUD_MAKER_X, "1.5", "Whole numbers only"),
        (
            Field::X,
            ids::HUD_MAKER_X,
            "99999999999999999999999",
            "Must be between -7680 and 7680",
        ),
        (
            Field::Wide,
            ids::HUD_MAKER_WIDE,
            "0",
            "Must be between 1 and 7680",
        ),
        (
            Field::Layer,
            ids::HUD_MAKER_LAYER,
            "99999999999",
            "Must be between -2147483648 and 2147483647",
        ),
    ] {
        commit(cx, &s, &page, field, text);
        assert_eq!(note_under(cx, &s, id).as_deref(), Some(said), "{text}");
    }
    commit(cx, &s, &page, Field::Y, "900");
    assert_eq!(note_under(cx, &s, ids::HUD_MAKER_Y), None);
    assert!(
        note_under(cx, &s, ids::HUD_MAKER_X).is_some(),
        "each box keeps its own note"
    );
    // Another part starts with clean boxes.
    click_row(cx, &s, &PartKey::new("HudWeaponAmmo/AmmoInClip"));
    assert_eq!(note_under(cx, &s, ids::HUD_MAKER_X), None);
}

/// Drags from `from` by `by` window pixels in eight moves with Ctrl held, so snapping stays
/// off and the part lands where the pointer says.
fn drag_window(
    cx: &mut TestAppContext,
    s: &support::ShellWindow,
    from: gpui_kit::Point<Pixels>,
    by: (f32, f32),
) {
    let left = gpui_kit::MouseButton::Left;
    let free = gpui_kit::Modifiers::secondary_key();
    let at = |t: f32| from + point(px(by.0 * t), px(by.1 * t));
    send(
        cx,
        s,
        gpui_kit::MouseMoveEvent {
            position: from,
            pressed_button: None,
            modifiers: free,
        },
    );
    send(
        cx,
        s,
        gpui_kit::MouseDownEvent {
            button: left,
            position: from,
            modifiers: free,
            click_count: 1,
            first_mouse: false,
        },
    );
    for step in 1..=8 {
        let position = at(step as f32 / 8.);
        send(
            cx,
            s,
            gpui_kit::MouseMoveEvent {
                position,
                pressed_button: Some(left),
                modifiers: free,
            },
        );
    }
    send(
        cx,
        s,
        gpui_kit::MouseUpEvent {
            button: left,
            position: at(1.),
            modifiers: free,
            click_count: 1,
        },
    );
}

fn scale(cx: &mut TestAppContext, s: &support::ShellWindow) -> f32 {
    f(bounds(cx, s, ids::HUD_MAKER_CANVAS).size.width) / 1920.
}

#[gpui_kit::test]
fn dragging_a_part_selects_and_moves_it_as_one_undo_step(cx: &mut TestAppContext) {
    let env = support::env();
    let s = stock_shell(cx, &env);
    let page = open_maker(cx, &s);
    assert!(support::disabled_of(cx, s.win, ids::HUD_MAKER_UNDO));
    let k = scale(cx, &s);
    let from = bounds(cx, &s, ids::hud_maker_part(&PartKey::new(HV))).center();
    drag_window(cx, &s, from, (45. * k, 0.));
    assert_eq!(selection(cx, &page).as_deref(), Some(HH));
    assert_eq!(hv_box(cx, &page).0, 216);
    support::click_id(cx, s.win, ids::HUD_MAKER_UNDO);
    assert_eq!(hv_box(cx, &page).0, 171);
    support::click_id(cx, s.win, ids::HUD_MAKER_REDO);
    assert_eq!(hv_box(cx, &page).0, 216);
    support::press(cx, s.win, "ctrl-z");
    assert_eq!(hv_box(cx, &page).0, 171);
    support::press(cx, s.win, "ctrl-shift-z");
    assert_eq!(hv_box(cx, &page).0, 216);
}

#[gpui_kit::test]
fn the_right_handle_resizes_the_selected_part(cx: &mut TestAppContext) {
    let env = support::env();
    let s = stock_shell(cx, &env);
    let page = open_maker(cx, &s);
    assert!(!support::present(
        cx,
        s.win,
        ids::hud_maker_handle(Handle::Right)
    ));
    select_hv(cx, &s);
    // HV is too short on the canvas for side handles, so it grows tall enough first.
    commit(cx, &s, &page, Field::Tall, "120");
    let k = scale(cx, &s);
    let from = bounds(cx, &s, ids::hud_maker_handle(Handle::Right)).center();
    drag_window(cx, &s, from, (36. * k, 0.));
    assert_eq!(hv_box(cx, &page).2, 135);
}

#[gpui_kit::test]
fn a_drag_released_outside_the_canvas_still_commits(cx: &mut TestAppContext) {
    let env = support::env();
    let s = stock_shell(cx, &env);
    let page = open_maker(cx, &s);
    let canvas = bounds(cx, &s, ids::HUD_MAKER_CANVAS);
    let from = bounds(cx, &s, ids::hud_maker_part(&PartKey::new(HV))).center();
    drag_window(cx, &s, from, (0., f(canvas.bottom() - from.y) + 40.));
    let e = cx.update(|cx| {
        let e = page.read(cx).editor().unwrap();
        (e.is_dragging(), e.can_undo())
    });
    assert_eq!(e, (false, true));
}

/// Opens the toolbar's screen shape menu and picks `preset`.
fn pick_screen(cx: &mut TestAppContext, s: &support::ShellWindow, preset: Screen) {
    support::click_id(cx, s.win, ids::HUD_MAKER_SCREEN_MENU);
    support::click_id(cx, s.win, ids::hud_maker_screen(preset));
}

/// Dispatches one pointer event to the shell window, then draws a frame.
fn send(cx: &mut TestAppContext, s: &support::ShellWindow, event: impl gpui_kit::InputEvent) {
    cx.update_window(s.win, |_, w, cx| {
        let _ = w.dispatch_event(event.to_platform_input(), cx);
        w.render_frame(cx);
    })
    .unwrap();
    support::settle(cx, s.win);
}

/// Presses at `from` and moves by `by` with the button held, without releasing, with Ctrl
/// held, so snapping stays off.
fn hold_drag(
    cx: &mut TestAppContext,
    s: &support::ShellWindow,
    from: gpui_kit::Point<Pixels>,
    by: (f32, f32),
) {
    let left = gpui_kit::MouseButton::Left;
    send(
        cx,
        s,
        gpui_kit::MouseDownEvent {
            button: left,
            position: from,
            modifiers: gpui_kit::Modifiers::secondary_key(),
            click_count: 1,
            first_mouse: false,
        },
    );
    send(
        cx,
        s,
        gpui_kit::MouseMoveEvent {
            position: from + point(px(by.0), px(by.1)),
            pressed_button: Some(left),
            modifiers: gpui_kit::Modifiers::secondary_key(),
        },
    );
}

fn release(cx: &mut TestAppContext, s: &support::ShellWindow, at: gpui_kit::Point<Pixels>) {
    send(
        cx,
        s,
        gpui_kit::MouseUpEvent {
            button: gpui_kit::MouseButton::Left,
            position: at,
            modifiers: Default::default(),
            click_count: 1,
        },
    );
}

#[gpui_kit::test]
fn a_click_whose_release_lands_in_the_same_frame_leaves_no_drag(cx: &mut TestAppContext) {
    let env = support::env();
    let s = stock_shell(cx, &env);
    let page = open_maker(cx, &s);
    let at = bounds(cx, &s, ids::hud_maker_part(&PartKey::new(HV))).center();
    let left = gpui_kit::MouseButton::Left;
    // A tap: the press and the release reach the window with no frame drawn between them.
    cx.update_window(s.win, |_, w, cx| {
        let down = gpui_kit::MouseDownEvent {
            button: left,
            position: at,
            modifiers: Default::default(),
            click_count: 1,
            first_mouse: false,
        };
        let up = gpui_kit::MouseUpEvent {
            button: left,
            position: at,
            modifiers: Default::default(),
            click_count: 1,
        };
        let _ = w.dispatch_event(gpui_kit::InputEvent::to_platform_input(down), cx);
        let _ = w.dispatch_event(gpui_kit::InputEvent::to_platform_input(up), cx);
    })
    .unwrap();
    support::settle(cx, s.win);
    assert_eq!(selection(cx, &page).as_deref(), Some(HH));
    let x = hv_box(cx, &page).0;
    support::press(cx, s.win, "right");
    assert!(hv_box(cx, &page).0 > x, "the first arrow press nudges");
    support::press(cx, s.win, "escape");
    assert_eq!(selection(cx, &page), None, "the first Escape deselects");
}

#[gpui_kit::test]
fn escape_closes_the_more_menu_before_it_deselects(cx: &mut TestAppContext) {
    let env = support::env();
    let s = stock_shell(cx, &env);
    let page = open_maker(cx, &s);
    select_hv(cx, &s);
    support::click_id(cx, s.win, ids::HUD_MAKER_MORE);
    assert!(support::present(cx, s.win, ids::HUD_MAKER_START));
    support::press(cx, s.win, "escape");
    assert!(!support::present(cx, s.win, ids::HUD_MAKER_START));
    assert_eq!(selection(cx, &page).as_deref(), Some(HV));
    support::press(cx, s.win, "escape");
    assert_eq!(selection(cx, &page), None);
}

#[gpui_kit::test]
fn escape_closes_the_colour_picker_before_it_deselects(cx: &mut TestAppContext) {
    let env = support::env();
    let s = stock_shell(cx, &env);
    let page = open_maker(cx, &s);
    select_hv(cx, &s);
    support::click_id(cx, s.win, ids::HUD_MAKER_COLOUR);
    let state = cx.update(|cx| page.read(cx).colour_state().clone());
    assert!(cx.update(|cx| state.read(cx).is_open()));
    support::press(cx, s.win, "escape");
    assert!(!cx.update(|cx| state.read(cx).is_open()));
    assert_eq!(selection(cx, &page).as_deref(), Some(HV));
    support::press(cx, s.win, "escape");
    assert_eq!(selection(cx, &page), None);
}

#[gpui_kit::test]
fn escape_cancels_a_running_drag(cx: &mut TestAppContext) {
    let env = support::env();
    let s = stock_shell(cx, &env);
    let page = open_maker(cx, &s);
    let k = scale(cx, &s);
    let from = bounds(cx, &s, ids::hud_maker_part(&PartKey::new(HV))).center();
    hold_drag(cx, &s, from, (45. * k, 0.));
    assert_eq!(hv_box(cx, &page).0, 216, "the drag previews");
    support::press(cx, s.win, "escape");
    release(cx, &s, from + point(px(45. * k), px(0.)));
    let after = cx.update(|cx| {
        let e = page.read(cx).editor().unwrap();
        (e.is_dragging(), e.can_undo())
    });
    assert_eq!((hv_box(cx, &page).0, after), (171, (false, false)));
}

#[gpui_kit::test]
fn a_new_drag_ends_one_left_running(cx: &mut TestAppContext) {
    let env = support::env();
    let s = stock_shell(cx, &env);
    let page = open_maker(cx, &s);
    select_hv(cx, &s);
    cx.update(|cx| {
        page.update(cx, |p, _| {
            p.editor_mut().unwrap().drag(Handle::Bottom, 0, 45)
        })
    })
    .unwrap();
    let k = scale(cx, &s);
    let from = bounds(cx, &s, ids::hud_maker_part(&PartKey::new(HV))).center();
    drag_window(cx, &s, from, (45. * k, 0.));
    assert_eq!(hv_box(cx, &page).0, 216);
    assert!(undo(cx, &page));
    assert_eq!(hv_box(cx, &page).0, 171, "the new drag is its own step");
    assert!(
        cx.update(|cx| page.read(cx).editor().unwrap().can_undo()),
        "the old drag was kept"
    );
}

#[gpui_kit::test]
fn a_collapsed_docked_panel_gives_its_column_back(cx: &mut TestAppContext) {
    let env = support::env();
    let s = stock_shell(cx, &env);
    let page = open_maker(cx, &s);
    support::maker_page::float_panels(cx, &s, &page);
    support::click_id(cx, s.win, ids::HUD_MAKER_PANEL_DOCK_RIGHT);
    let open = bounds(cx, &s, ids::HUD_MAKER_CANVAS);
    support::click_id(cx, s.win, ids::HUD_MAKER_PANEL_COLLAPSE);
    let area = bounds(cx, &s, ids::HUD_MAKER_CANVAS_AREA);
    let canvas = bounds(cx, &s, ids::HUD_MAKER_CANVAS);
    let rail = bounds(cx, &s, ids::HUD_MAKER_PANEL);
    // Collapsed, it folds to a slim rail at the right edge, and the canvas takes the rest.
    assert!(
        canvas.size.width > open.size.width && canvas.right() <= rail.left(),
        "canvas {canvas:?} beside the rail {rail:?}, was {open:?}"
    );
    assert!(
        f(rail.right() - area.right()).abs() <= 1. && rail.size.width < px(48.),
        "the rail {rail:?} stays at the right edge of {area:?}"
    );
}

const ALL_HANDLES: [Handle; 8] = [
    Handle::TopLeft,
    Handle::Top,
    Handle::TopRight,
    Handle::Right,
    Handle::BottomRight,
    Handle::Bottom,
    Handle::BottomLeft,
    Handle::Left,
];

#[gpui_kit::test]
fn handles_sit_outside_the_part_and_a_short_part_drops_its_side_ones(cx: &mut TestAppContext) {
    let env = support::env();
    let s = stock_shell(cx, &env);
    let page = open_maker(cx, &s);
    support::maker_page::float_panels(cx, &s, &page);
    select_hv(cx, &s);
    let part = bounds(cx, &s, ids::hud_maker_part(&PartKey::new(HV)));
    assert!(
        f(part.size.height) < 24. && f(part.size.width) >= 24.,
        "HV is short and wide enough: {part:?}"
    );
    let drawn: Vec<Handle> = ALL_HANDLES
        .into_iter()
        .filter(|&h| {
            support::bounds_of(cx, s.win, ids::hud_maker_handle(h)).is_some_and(|b| {
                assert!(!b.intersects(&part), "{h:?} {b:?} covers the part {part:?}");
                true
            })
        })
        .collect();
    assert!(!drawn.contains(&Handle::Left) && !drawn.contains(&Handle::Right));
    assert_eq!(drawn.len(), 6, "{drawn:?}");
    commit(cx, &s, &page, Field::Tall, "120");
    assert!(support::present(
        cx,
        s.win,
        ids::hud_maker_handle(Handle::Left)
    ));
}

#[gpui_kit::test]
fn a_part_hidden_by_size_zero_shows_its_size_as_hidden(cx: &mut TestAppContext) {
    let env = support::env();
    let s = stock_shell(cx, &env);
    let _page = open_maker(cx, &s);
    select_hv(cx, &s);
    support::click_id(cx, s.win, ids::HUD_MAKER_HIDE);
    assert_eq!(
        support::label_of(cx, s.win, ids::HUD_MAKER_SIZE_HIDDEN).as_deref(),
        Some("Hidden")
    );
    assert!(!support::present(cx, s.win, ids::HUD_MAKER_WIDE));
    assert!(support::disabled_of(
        cx,
        s.win,
        ids::hud_maker_reset("wide")
    ));
    assert!(!support::disabled_of(
        cx,
        s.win,
        ids::hud_maker_reset("visible")
    ));
}

#[gpui_kit::test]
fn the_hide_switch_says_what_it_does(cx: &mut TestAppContext) {
    let env = support::env();
    let s = stock_shell(cx, &env);
    let _page = open_maker(cx, &s);
    select_hv(cx, &s);
    assert_eq!(
        support::label_of(cx, s.win, ids::HUD_MAKER_HIDE).as_deref(),
        Some("Hide this part")
    );
}

#[gpui_kit::test]
fn escape_without_a_drag_goes_on_to_whatever_else_wants_it(cx: &mut TestAppContext) {
    use std::cell::Cell;
    use std::rc::Rc;
    use tf2tools_app::keymap::CancelHudDrag;
    let env = support::env();
    let s = stock_shell(cx, &env);
    let _page = open_maker(cx, &s);
    let count = Rc::new(Cell::new(0));
    let seen = count.clone();
    cx.update(|cx| {
        cx.on_action(move |_: &CancelHudDrag, _| seen.set(seen.get() + 1));
    });
    support::press(cx, s.win, "escape");
    assert_eq!(count.get(), 1);
}

#[gpui_kit::test]
fn a_side_panel_place_saved_before_the_parts_panel_still_loads(cx: &mut TestAppContext) {
    let env = support::env();
    let s = stock_shell(cx, &env);
    s.services
        .db
        .set_hud_maker_panel("right 40 24 collapsed")
        .unwrap();
    let page = open_maker(cx, &s);
    let (options, parts) = cx.update(|cx| {
        let p = page.read(cx);
        (p.panel_place(), p.place_of(Panel::Parts))
    });
    assert_eq!(
        options,
        PanelPlace {
            mode: PanelMode::DockRight,
            x: 40.,
            y: 24.,
            collapsed: true
        }
    );
    assert_eq!(parts, PanelPlace::PARTS_DEFAULT);
}

#[gpui_kit::test]
fn every_place_is_saved_in_the_one_setting(cx: &mut TestAppContext) {
    let env = support::env();
    let s = stock_shell(cx, &env);
    let page = open_maker(cx, &s);
    let parts = PanelPlace {
        mode: PanelMode::DockLeft,
        x: 0.,
        y: 0.,
        collapsed: false,
    };
    cx.update(|cx| page.update(cx, |p, cx| p.set_place_of(Panel::Parts, parts, cx)));
    let want = Places {
        parts,
        ..Places::DEFAULT
    }
    .to_setting();
    support::wait_real(cx, "the setting", |_| {
        saved_panel(&s).as_deref() == Some(want.as_str())
    });
}

#[gpui_kit::test]
fn the_parts_panel_starts_collapsed_at_the_top_right(cx: &mut TestAppContext) {
    let env = support::env();
    let s = stock_shell(cx, &env);
    let page = open_maker(cx, &s);
    support::maker_page::float_panels(cx, &s, &page);
    assert!(support::present(
        cx,
        s.win,
        ids::HUD_MAKER_PARTS_PANEL_TITLE
    ));
    assert!(!support::present(cx, s.win, ids::HUD_MAKER_PARTS));
    let parts = bounds(cx, &s, ids::HUD_MAKER_PARTS_PANEL);
    let area = bounds(cx, &s, ids::HUD_MAKER_CANVAS_AREA);
    assert!(
        f(area.right() - parts.right() - px(16.)).abs() <= 1.,
        "{parts:?} in {area:?}"
    );
    assert!(
        f(parts.top() - area.top() - px(16.)).abs() <= 1.,
        "{parts:?}"
    );
    assert!(!parts.intersects(&bounds(cx, &s, ids::HUD_MAKER_PANEL)));
}

#[gpui_kit::test]
fn opening_the_parts_panel_lists_the_parts_and_is_remembered(cx: &mut TestAppContext) {
    let env = support::env();
    let s = stock_shell(cx, &env);
    let _page = open_maker(cx, &s);
    show_parts(cx, &s);
    assert_eq!(
        support::label_of(cx, s.win, ids::hud_maker_row(&PartKey::new(HV))).as_deref(),
        Some("Health value")
    );
    support::wait_real(cx, "the setting", |_| {
        saved_places(&s).is_some_and(|p| !p.parts.collapsed)
    });
    support::click_id(cx, s.win, ids::nav(Page::Huds));
    let _page = open_maker(cx, &s);
    assert!(support::present(cx, s.win, ids::HUD_MAKER_PARTS));
}

#[gpui_kit::test]
fn dragging_the_parts_title_moves_the_parts_panel(cx: &mut TestAppContext) {
    let env = support::env();
    let s = stock_shell(cx, &env);
    let page = open_maker(cx, &s);
    support::maker_page::float_panels(cx, &s, &page);
    let before = bounds(cx, &s, ids::HUD_MAKER_PARTS_PANEL);
    let from = bounds(cx, &s, ids::HUD_MAKER_PARTS_PANEL_TITLE).center();
    cx.update_window(s.win, |_, w, cx| {
        w.drag(from, from + point(px(-300.), px(60.)), cx)
    })
    .unwrap();
    support::settle(cx, s.win);
    let after = bounds(cx, &s, ids::HUD_MAKER_PARTS_PANEL);
    assert!(
        f(after.left() - before.left() + px(300.)).abs() <= 1.,
        "{after:?}"
    );
    assert!(
        f(after.top() - before.top() - px(60.)).abs() <= 1.,
        "{after:?}"
    );
    let area = bounds(cx, &s, ids::HUD_MAKER_CANVAS_AREA);
    let placed = cx.update(|cx| page.read(cx).place_of(Panel::Parts));
    assert!(
        f(px(placed.x) - (after.left() - area.left())).abs() <= 1.,
        "{placed:?}"
    );
}

#[gpui_kit::test]
fn two_panels_docked_left_stack_in_one_column(cx: &mut TestAppContext) {
    let env = support::env();
    let s = stock_shell(cx, &env);
    let _page = open_maker(cx, &s);
    show_parts(cx, &s);
    support::click_id(cx, s.win, ids::HUD_MAKER_PANEL_DOCK_LEFT);
    support::click_id(cx, s.win, ids::HUD_MAKER_PARTS_PANEL_DOCK_LEFT);
    let options = bounds(cx, &s, ids::HUD_MAKER_PANEL);
    let parts = bounds(cx, &s, ids::HUD_MAKER_PARTS_PANEL);
    let area = bounds(cx, &s, ids::HUD_MAKER_CANVAS_AREA);
    let canvas = bounds(cx, &s, ids::HUD_MAKER_CANVAS);
    assert!(
        f(options.left() - area.left()).abs() <= 1. && f(parts.left() - area.left()).abs() <= 1.
    );
    assert!(
        parts.top() >= options.bottom() + px(7.),
        "{parts:?} under {options:?}"
    );
    assert!(
        canvas.left() >= options.right() && canvas.left() >= parts.right(),
        "{canvas:?}"
    );
}

#[gpui_kit::test]
fn the_parts_panel_keeps_off_the_selection_and_the_side_panel(cx: &mut TestAppContext) {
    for (w, h) in [(1280., 800.), (1024., 768.), (800., 600.)] {
        parts_panel_keeps_off_at(cx, w, h);
    }
}

// The open parts panel over HV, the side panel at the area's top right, then HV is selected:
// whatever each panel shows, open or only its header, stays off HV, and nothing is saved.
fn parts_panel_keeps_off_at(cx: &mut TestAppContext, w: f32, h: f32) {
    let label = format!("{w}x{h}");
    let env = support::env();
    let s = stock_shell(cx, &env);
    resize(cx, &s, w, h);
    let page = open_maker(cx, &s);
    support::maker_page::float_panels(cx, &s, &page);
    let hv = bounds(cx, &s, ids::hud_maker_part(&PartKey::new(HV)));
    let area = bounds(cx, &s, ids::HUD_MAKER_CANVAS_AREA);
    // The side panel starts at the area's top right, out of HV's column, so the setup below
    // holds however tall the HUD's options make it.
    let right = PanelPlace {
        mode: PanelMode::Floating,
        x: f(area.size.width) - 296.,
        y: 16.,
        collapsed: false,
    };
    cx.update(|cx| page.update(cx, |p, cx| p.set_place_of(Panel::Options, right, cx)));
    support::settle(cx, s.win);
    let over = PanelPlace {
        mode: PanelMode::Floating,
        x: f(hv.left() - area.left()) - 10.,
        y: (f(hv.top() - area.top()) - 100.).max(0.),
        collapsed: false,
    };
    cx.update(|cx| page.update(cx, |p, cx| p.set_place_of(Panel::Parts, over, cx)));
    support::settle(cx, s.win);
    // The list keeps off what the canvas draws, so it is the player's place that covers HV.
    let size = bounds(cx, &s, ids::HUD_MAKER_PARTS_PANEL).size;
    let placed = Bounds::new(area.origin + point(px(over.x), px(over.y)), size);
    assert!(placed.intersects(&hv), "{label}: setup must cover HV");
    let stored = Panel::ALL.map(|p| cx.update(|cx| page.read(cx).place_of(p)));
    support::click_part_row(cx, s.win, &PartKey::new(HV));
    assert_eq!(selection(cx, &page).as_deref(), Some(HV), "{label}");
    let parts = bounds(cx, &s, ids::HUD_MAKER_PARTS_PANEL);
    let options = bounds(cx, &s, ids::HUD_MAKER_PANEL);
    assert!(!parts.intersects(&hv), "{label}: {parts:?} covers {hv:?}");
    assert!(
        !options.intersects(&hv),
        "{label}: {options:?} covers {hv:?}"
    );
    // The widest area still has room for both panels apart.
    if w >= 1280. {
        assert!(
            !parts.intersects(&options),
            "{label}: {parts:?} covers {options:?}"
        );
    }
    assert_eq!(
        Panel::ALL.map(|p| cx.update(|cx| page.read(cx).place_of(p))),
        stored,
        "{label}"
    );
}

// Selects HV and makes it a band across the screen leaving 236 px above it and above the
// toolbar: under the side panel's 240 px least height and its 8 px gap, so only a header fits.
fn band_hv_without_room(
    cx: &mut TestAppContext,
    s: &support::ShellWindow,
    page: &Entity<HudMakerPage>,
) {
    select_hv(cx, s);
    let area = bounds(cx, s, ids::HUD_MAKER_CANVAS_AREA);
    let bar = bounds(cx, s, ids::HUD_MAKER_TOOLBAR);
    let canvas = bounds(cx, s, ids::HUD_MAKER_CANVAS);
    let (top, bottom) = (area.top() + px(236.), bar.top() - px(236.));
    assert!(bottom - top > px(40.), "setup: no band fits in {area:?}");
    let screen = cx.update(|cx| page.read(cx).editor().unwrap().screen());
    let scale = f(canvas.size.width) / screen.wide() as f32;
    let y = (f(top - canvas.top()) / scale).ceil() as i32;
    let tall = (f(bottom - top) / scale).floor() as i32;
    commit(cx, s, page, Field::X, "0");
    commit(cx, s, page, Field::Y, &y.to_string());
    commit(cx, s, page, Field::Wide, &screen.wide().to_string());
    commit(cx, s, page, Field::Tall, &tall.to_string());
}

// Selects HV and stretches it into a thin band across the screen's middle: a small part, yet
// no open side panel fits above or below it, while a header does.
fn stretch_hv(cx: &mut TestAppContext, s: &support::ShellWindow, page: &Entity<HudMakerPage>) {
    select_hv(cx, s);
    let screen = cx.update(|cx| page.read(cx).editor().unwrap().screen());
    let tall = screen.tall() / 6;
    commit(cx, s, page, Field::X, "0");
    commit(
        cx,
        s,
        page,
        Field::Y,
        &((screen.tall() - tall) / 2).to_string(),
    );
    commit(cx, s, page, Field::Wide, &screen.wide().to_string());
    commit(cx, s, page, Field::Tall, &tall.to_string());
}

#[gpui_kit::test]
fn a_large_selection_keeps_the_side_panel_open(cx: &mut TestAppContext) {
    let env = support::env();
    let s = stock_shell(cx, &env);
    resize(cx, &s, 800., 600.);
    let page = open_maker(cx, &s);
    support::maker_page::float_panels(cx, &s, &page);
    select_hv(cx, &s);
    let screen = cx.update(|cx| page.read(cx).editor().unwrap().screen());
    commit(cx, &s, &page, Field::X, "0");
    commit(cx, &s, &page, Field::Y, "0");
    commit(cx, &s, &page, Field::Wide, &screen.wide().to_string());
    commit(cx, &s, &page, Field::Tall, &screen.tall().to_string());
    // The narrow area closed the side panel when the part list opened; its stored place opens
    // it again without the hold an Expand click would give.
    let open = PanelPlace {
        collapsed: false,
        ..cx.update(|cx| page.read(cx).place_of(Panel::Options))
    };
    cx.update(|cx| page.update(cx, |p, cx| p.set_place_of(Panel::Options, open, cx)));
    support::settle(cx, s.win);
    assert!(!cx.update(|cx| page.read(cx).is_folded(Panel::Options)));
    assert!(support::present(cx, s.win, ids::HUD_MAKER_PANEL_BODY));
}

// Once settled, further frames move nothing and settle nothing again.
fn assert_at_rest(
    cx: &mut TestAppContext,
    s: &support::ShellWindow,
    page: &Entity<HudMakerPage>,
    label: &str,
) {
    support::settle(cx, s.win);
    let before = cx.update(|cx| page.read(cx).relayouts());
    let panel = bounds(cx, s, ids::HUD_MAKER_PANEL);
    for frame in 0..3 {
        support::settle(cx, s.win);
        assert_eq!(
            bounds(cx, s, ids::HUD_MAKER_PANEL),
            panel,
            "{label}: frame {frame}"
        );
    }
    assert_eq!(
        cx.update(|cx| page.read(cx).relayouts()),
        before,
        "{label}: relaid out"
    );
}

// A band the side panel is held open over, then a full-screen part it spills onto.
fn comes_to_rest_at(cx: &mut TestAppContext, w: f32, h: f32) {
    let env = support::env();
    let s = stock_shell(cx, &env);
    resize(cx, &s, w, h);
    let page = open_maker(cx, &s);
    support::maker_page::float_panels(cx, &s, &page);
    stretch_hv(cx, &s, &page);
    if cx.update(|cx| page.read(cx).is_folded(Panel::Options)) {
        support::click_id(cx, s.win, ids::HUD_MAKER_PANEL_COLLAPSE);
    }
    assert_at_rest(cx, &s, &page, &format!("{w}x{h} band"));
    let screen = cx.update(|cx| page.read(cx).editor().unwrap().screen());
    commit(cx, &s, &page, Field::Y, "0");
    commit(cx, &s, &page, Field::Tall, &screen.tall().to_string());
    assert_at_rest(cx, &s, &page, &format!("{w}x{h} full screen"));
}

#[gpui_kit::test]
fn the_toolbar_fits_one_row_at_1024x768(cx: &mut TestAppContext) {
    let env = support::env();
    let s = stock_shell(cx, &env);
    resize(cx, &s, 1024., 768.);
    let page = open_maker(cx, &s);
    support::maker_page::float_panels(cx, &s, &page);
    let bar = bounds(cx, &s, ids::HUD_MAKER_TOOLBAR);
    let button = bounds(cx, &s, ids::HUD_MAKER_UNDO);
    // One row of buttons, with the bar's padding and border round it.
    assert!(
        bar.size.height < button.size.height * 1.5,
        "the toolbar is {bar:?} for a {button:?} button"
    );
}

/// At 800x600, places the Medic meter high on the right, then clicks Mirror with the pointer
/// on it, which moves the meter under the side panel; returns the panel's bounds before.
fn mirror_under_the_panel(
    cx: &mut TestAppContext,
) -> (
    support::Env,
    support::ShellWindow,
    Entity<HudMakerPage>,
    Bounds<Pixels>,
) {
    let env = support::env();
    let s = stock_shell(cx, &env);
    resize(cx, &s, 800., 600.);
    let page = open_maker(cx, &s);
    support::maker_page::float_panels(cx, &s, &page);
    click_row(cx, &s, &PartKey::new(MEDIC));
    // The narrow window closed the side panel while the part list was open.
    if !support::present(cx, s.win, ids::HUD_MAKER_PANEL_BODY) {
        support::click_id(cx, s.win, ids::HUD_MAKER_PANEL_COLLAPSE);
    }
    // High on the right, so Mirror takes it under the side panel's top-left home.
    commit(cx, &s, &page, Field::X, "1500");
    commit(cx, &s, &page, Field::Y, "300");
    scroll_panel_to(cx, &s, ids::HUD_MAKER_MIRROR);
    assert_at_rest(cx, &s, &page, "before Mirror");
    let panel = bounds(cx, &s, ids::HUD_MAKER_PANEL);
    let mirror = bounds(cx, &s, ids::HUD_MAKER_MIRROR).center();
    send(cx, &s, pointer_at(mirror));
    support::click_id(cx, s.win, ids::HUD_MAKER_MIRROR);
    let part = medic_box(cx, &s);
    assert!(
        panel.intersects(&part),
        "setup: Mirror must move {part:?} under {panel:?}"
    );
    (env, s, page, panel)
}

const MEDIC: &str = "HudMedicCharge";

fn medic_box(cx: &mut TestAppContext, s: &support::ShellWindow) -> Bounds<Pixels> {
    bounds(cx, s, ids::hud_maker_part(&PartKey::new(MEDIC)))
}

fn pointer_at(position: gpui_kit::Point<Pixels>) -> gpui_kit::MouseMoveEvent {
    gpui_kit::MouseMoveEvent {
        position,
        pressed_button: None,
        modifiers: Default::default(),
    }
}

#[gpui_kit::test]
fn a_panel_under_the_pointer_waits_for_it_to_leave_before_it_moves(cx: &mut TestAppContext) {
    let (_env, s, page, panel) = mirror_under_the_panel(cx);
    assert_at_rest(cx, &s, &page, "pointer on Mirror");
    assert_eq!(bounds(cx, &s, ids::HUD_MAKER_PANEL), panel);
    // Once the pointer leaves the panel, it settles clear of the part.
    let canvas = bounds(cx, &s, ids::HUD_MAKER_CANVAS);
    let away = if panel.left() - canvas.left() > px(40.) {
        point(panel.left() - px(20.), panel.top() + px(10.))
    } else {
        point(panel.right() + px(20.), panel.top() + px(10.))
    };
    send(cx, &s, pointer_at(away));
    let (moved, part) = (bounds(cx, &s, ids::HUD_MAKER_PANEL), medic_box(cx, &s));
    assert!(!moved.intersects(&part), "{moved:?} still covers {part:?}");
}

#[gpui_kit::test]
fn a_keyboard_nudge_never_holds_the_panel(cx: &mut TestAppContext) {
    let (_env, s, _page, _) = mirror_under_the_panel(cx);
    // The pointer stays on the panel; the nudge is the keyboard's, so the panel moves off.
    support::press(cx, s.win, "left");
    let (moved, part) = (bounds(cx, &s, ids::HUD_MAKER_PANEL), medic_box(cx, &s));
    assert!(!moved.intersects(&part), "{moved:?} still covers {part:?}");
}

#[gpui_kit::test]
fn the_pointer_leaving_the_window_lets_a_held_panel_go(cx: &mut TestAppContext) {
    let (_env, s, _page, panel) = mirror_under_the_panel(cx);
    assert_eq!(bounds(cx, &s, ids::HUD_MAKER_PANEL), panel);
    send(
        cx,
        &s,
        gpui_kit::MouseExitEvent {
            position: panel.center(),
            pressed_button: None,
            modifiers: Default::default(),
        },
    );
    let (moved, part) = (bounds(cx, &s, ids::HUD_MAKER_PANEL), medic_box(cx, &s));
    assert!(!moved.intersects(&part), "{moved:?} still covers {part:?}");
}

/// Wheels the side panel's body down until `id` shows whole inside it.
fn scroll_panel_to(cx: &mut TestAppContext, s: &support::ShellWindow, id: &'static str) {
    for _ in 0..40 {
        let seen = support::present(cx, s.win, id)
            && cx
                .update_window(s.win, |_, w, _| {
                    let body = w.try_find(ids::HUD_MAKER_PANEL_BODY).map(|b| b.bounds());
                    w.try_find(id).is_some_and(|e| {
                        let b = e.bounds();
                        e.visible() && body.is_none_or(|body| b.bottom() < body.bottom())
                    })
                })
                .unwrap();
        if seen {
            return;
        }
        let down = gpui_kit::ScrollDelta::Lines(point(0., -3.));
        cx.update_window(s.win, |_, w, cx| {
            w.scroll(ids::HUD_MAKER_PANEL_BODY, down, cx)
        })
        .unwrap();
        support::settle(cx, s.win);
    }
    panic!("{id} never showed in the panel");
}

#[gpui_kit::test]
fn the_panels_come_to_rest_at_1280x800(cx: &mut TestAppContext) {
    comes_to_rest_at(cx, 1280., 800.);
}

#[gpui_kit::test]
fn the_panels_come_to_rest_at_1024x768(cx: &mut TestAppContext) {
    comes_to_rest_at(cx, 1024., 768.);
}

#[gpui_kit::test]
fn the_panels_come_to_rest_at_800x600(cx: &mut TestAppContext) {
    comes_to_rest_at(cx, 800., 600.);
}

#[gpui_kit::test]
fn a_panel_with_no_room_beside_the_selection_shows_only_its_header_for_now(
    cx: &mut TestAppContext,
) {
    let env = support::env();
    let s = stock_shell(cx, &env);
    resize(cx, &s, 1024., 768.);
    let page = open_maker(cx, &s);
    support::maker_page::float_panels(cx, &s, &page);
    band_hv_without_room(cx, &s, &page);
    let hv = hv_outline(cx, &s);
    assert!(!support::present(cx, s.win, ids::HUD_MAKER_PANEL_BODY));
    let header = bounds(cx, &s, ids::HUD_MAKER_PANEL);
    assert!(!header.intersects(&hv), "{header:?} covers {hv:?}");
    assert!(
        !cx.update(|cx| page.read(cx).place_of(Panel::Options))
            .collapsed
    );
    // A save of every place made now stores the side panel open.
    support::click_id(cx, s.win, ids::HUD_MAKER_TOOLBAR_COLLAPSE);
    support::wait_real(cx, "the setting", |_| {
        saved_places(&s).is_some_and(|p| p.toolbar_collapsed)
    });
    assert!(!saved_places(&s).unwrap().options.collapsed);
    support::click_id(cx, s.win, ids::HUD_MAKER_TOOLBAR_EXPAND);
    // Clearing the selection reopens it.
    support::press(cx, s.win, "escape");
    assert_eq!(selection(cx, &page), None);
    assert!(support::present(cx, s.win, ids::HUD_MAKER_PANEL_BODY));
}

#[gpui_kit::test]
fn expanding_a_folded_panel_holds_it_open_until_the_selection_changes(cx: &mut TestAppContext) {
    let env = support::env();
    let s = stock_shell(cx, &env);
    resize(cx, &s, 1024., 768.);
    let page = open_maker(cx, &s);
    support::maker_page::float_panels(cx, &s, &page);
    band_hv_without_room(cx, &s, &page);
    assert!(!support::present(cx, s.win, ids::HUD_MAKER_PANEL_BODY));
    support::click_id(cx, s.win, ids::HUD_MAKER_PANEL_COLLAPSE);
    assert!(support::present(cx, s.win, ids::HUD_MAKER_PANEL_BODY));
    assert!(
        !cx.update(|cx| page.read(cx).place_of(Panel::Options))
            .collapsed
    );
    // Collapsed by the player and expanded again, it opens and stays open too.
    support::click_id(cx, s.win, ids::HUD_MAKER_PANEL_COLLAPSE);
    assert!(
        cx.update(|cx| page.read(cx).place_of(Panel::Options))
            .collapsed
    );
    support::click_id(cx, s.win, ids::HUD_MAKER_PANEL_COLLAPSE);
    assert!(support::present(cx, s.win, ids::HUD_MAKER_PANEL_BODY));
    support::press(cx, s.win, "escape");
    select_hv(cx, &s);
    assert!(!support::present(cx, s.win, ids::HUD_MAKER_PANEL_BODY));
}

#[gpui_kit::test]
fn clicks_on_the_parts_panel_keep_the_selection(cx: &mut TestAppContext) {
    let env = support::env();
    let s = stock_shell(cx, &env);
    let page = open_maker(cx, &s);
    click_row(cx, &s, &PartKey::new(HV));
    let area = bounds(cx, &s, ids::HUD_MAKER_CANVAS_AREA);
    let middle = PanelPlace {
        mode: PanelMode::Floating,
        x: f(area.size.width) / 2. - 100.,
        y: f(area.size.height) / 2. - 20.,
        collapsed: true,
    };
    cx.update(|cx| page.update(cx, |p, cx| p.set_place_of(Panel::Parts, middle, cx)));
    support::settle(cx, s.win);
    let canvas = bounds(cx, &s, ids::HUD_MAKER_CANVAS);
    assert!(bounds(cx, &s, ids::HUD_MAKER_PARTS_PANEL).intersects(&canvas));
    support::click_id(cx, s.win, ids::HUD_MAKER_PARTS_PANEL_TITLE);
    assert_eq!(selection(cx, &page).as_deref(), Some(HV));
}

fn hover(cx: &mut TestAppContext, s: &support::ShellWindow, id: gpui_kit::ElementId) {
    cx.update_window(s.win, |_, w, cx| w.hover(id, cx)).unwrap();
    support::settle(cx, s.win);
}

#[gpui_kit::test]
fn hovering_a_row_outlines_its_part(cx: &mut TestAppContext) {
    let env = support::env();
    let s = stock_shell(cx, &env);
    let page = open_maker(cx, &s);
    show_parts(cx, &s);
    assert!(!support::present(cx, s.win, ids::HUD_MAKER_HOVER));
    hover(cx, &s, ids::hud_maker_row(&PartKey::new(HV)));
    let outline = bounds(cx, &s, ids::HUD_MAKER_HOVER);
    let part = bounds(cx, &s, ids::hud_maker_part(&PartKey::new(HV)));
    assert!(
        f(outline.origin.x - part.origin.x).abs() <= 1.
            && f(outline.origin.y - part.origin.y).abs() <= 1.
            && f(outline.size.width - part.size.width).abs() <= 1.
            && f(outline.size.height - part.size.height).abs() <= 1.,
        "{outline:?} vs {part:?}"
    );
    // Off the row, the outline leaves HV; on the canvas it follows what the pointer is over.
    hover(cx, &s, ids::HUD_MAKER_CANVAS.into());
    assert_ne!(hovered(cx, &page).as_deref(), Some(HV));
}

#[gpui_kit::test]
fn collapsing_the_parts_panel_drops_the_outline(cx: &mut TestAppContext) {
    let env = support::env();
    let s = stock_shell(cx, &env);
    let page = open_maker(cx, &s);
    show_parts(cx, &s);
    hover(cx, &s, ids::hud_maker_row(&PartKey::new(HV)));
    assert!(support::present(cx, s.win, ids::HUD_MAKER_HOVER));
    // Collapse without moving the pointer, so no hover-out reaches the row.
    cx.update(|cx| {
        page.update(cx, |p, cx| {
            let place = PanelPlace {
                collapsed: true,
                ..p.place_of(Panel::Parts)
            };
            p.set_place_of(Panel::Parts, place, cx)
        })
    });
    assert!(!support::present(cx, s.win, ids::HUD_MAKER_HOVER));
}

#[gpui_kit::test]
fn scrolling_the_parts_list_takes_the_outline_off_the_row(cx: &mut TestAppContext) {
    let env = support::env();
    let s = stock_shell(cx, &env);
    let _page = open_maker(cx, &s);
    show_parts(cx, &s);
    let row = bounds(cx, &s, ids::hud_maker_row(&PartKey::new(HV)));
    hover(cx, &s, ids::hud_maker_row(&PartKey::new(HV)));
    assert!(support::present(cx, s.win, ids::HUD_MAKER_HOVER));
    // The wheel turns under a still pointer, so the row moves off it without a hover-out.
    send(
        cx,
        &s,
        gpui_kit::ScrollWheelEvent {
            position: row.center(),
            delta: gpui_kit::ScrollDelta::Lines(point(0., -400.)),
            modifiers: gpui_kit::Modifiers::default(),
            touch_phase: gpui_kit::TouchPhase::Moved,
        },
    );
    let part = bounds(cx, &s, ids::hud_maker_part(&PartKey::new(HV)));
    let stale = support::present(cx, s.win, ids::HUD_MAKER_HOVER) && {
        let outline = bounds(cx, &s, ids::HUD_MAKER_HOVER);
        f(outline.origin.x - part.origin.x).abs() <= 1.
            && f(outline.origin.y - part.origin.y).abs() <= 1.
    };
    assert!(!stale, "the scrolled-away row keeps its outline");
}

/// Moves the pointer to `at` with no button held, as a player's plain hover does.
fn move_to(cx: &mut TestAppContext, s: &support::ShellWindow, at: gpui_kit::Point<Pixels>) {
    let moved = gpui_kit::MouseMoveEvent {
        position: at,
        pressed_button: None,
        modifiers: Default::default(),
    };
    send(cx, s, moved);
}

fn hovered(cx: &mut TestAppContext, page: &Entity<HudMakerPage>) -> Option<String> {
    cx.update(|cx| page.read(cx).hovered().map(|k| k.as_str().to_owned()))
}

const IMAGE: &str = "HudPlayerStatus/HudPlayerHealth/PlayerStatusHealthImage";
const NOTHING: &str = "draws nothing in this preview";

#[gpui_kit::test]
fn hovering_a_part_on_the_canvas_outlines_and_names_it(cx: &mut TestAppContext) {
    let env = support::env();
    let s = stock_shell(cx, &env);
    let page = open_maker(cx, &s);
    support::put_start_card_away(cx, s.win);
    show_parts(cx, &s);
    let hv = PartKey::new(HV);
    let part = bounds(cx, &s, ids::hud_maker_part(&hv));
    move_to(cx, &s, part.center());
    // Over the health value, the outline takes what a click would: the health group.
    assert_eq!(hovered(cx, &page).as_deref(), Some(HH));
    let outline = bounds(cx, &s, ids::HUD_MAKER_HOVER);
    assert!(
        outline.contains(&part.origin) && outline.contains(&part.bottom_right()),
        "{outline:?} around {part:?}"
    );
    // The tag reads as the group's row does, so the canvas and the list name it alike.
    let name = support::label_of(cx, s.win, ids::hud_maker_row(&PartKey::new(HH))).unwrap();
    assert_eq!(
        support::label_of(cx, s.win, ids::HUD_MAKER_HOVER_NAME),
        Some(name)
    );
    // Leaving the screen takes the outline and its name away.
    let canvas = bounds(cx, &s, ids::HUD_MAKER_CANVAS);
    move_to(
        cx,
        &s,
        point(canvas.right() + px(4.), canvas.bottom() + px(4.)),
    );
    assert_eq!(hovered(cx, &page), None);
    assert!(!support::present(cx, s.win, ids::HUD_MAKER_HOVER));
    assert!(!support::present(cx, s.win, ids::HUD_MAKER_HOVER_NAME));
}

#[gpui_kit::test]
fn a_canvas_hover_marks_the_row_a_click_there_would_select(cx: &mut TestAppContext) {
    let env = support::env();
    let s = stock_shell(cx, &env);
    let page = open_maker(cx, &s);
    support::put_start_card_away(cx, s.win);
    show_parts(cx, &s);
    let part = bounds(cx, &s, ids::hud_maker_part(&PartKey::new(HV)));
    move_to(cx, &s, part.center());
    let canvas_hover = hovered(cx, &page);
    assert!(canvas_hover.is_some());
    let canvas = bounds(cx, &s, ids::HUD_MAKER_CANVAS);
    let at = part.center() - canvas.origin;
    cx.update_window(s.win, |_, w, cx| w.click_at(ids::HUD_MAKER_CANVAS, at, cx))
        .unwrap();
    support::settle(cx, s.win);
    assert_eq!(selection(cx, &page), canvas_hover);
}

#[gpui_kit::test]
fn hovering_a_row_names_its_part_on_the_canvas(cx: &mut TestAppContext) {
    let env = support::env();
    let s = stock_shell(cx, &env);
    let _page = open_maker(cx, &s);
    show_parts(cx, &s);
    let hv = PartKey::new(HV);
    hover(cx, &s, ids::hud_maker_row(&hv));
    let name = support::label_of(cx, s.win, ids::hud_maker_row(&hv)).unwrap();
    assert_eq!(
        support::label_of(cx, s.win, ids::HUD_MAKER_HOVER_NAME),
        Some(name)
    );
    let tag = bounds(cx, &s, ids::HUD_MAKER_HOVER_NAME);
    assert!(tag.intersects(&bounds(cx, &s, ids::HUD_MAKER_CANVAS)));
}

#[gpui_kit::test]
fn a_part_with_no_artwork_is_outlined_dashed_and_says_so(cx: &mut TestAppContext) {
    let env = support::env();
    let s = stock_shell(cx, &env);
    let _page = open_maker(cx, &s);
    show_parts(cx, &s);
    let image = PartKey::new(IMAGE);
    // A part that draws something keeps a plain outline.
    hover(cx, &s, ids::hud_maker_row(&PartKey::new(HV)));
    let plain = support::label_of(cx, s.win, ids::HUD_MAKER_HOVER).unwrap();
    assert!(!plain.contains(NOTHING), "{plain}");
    // The stock health image paints nothing in the preview, so its outline is dashed.
    hover(cx, &s, ids::hud_maker_row(&image));
    let hover_label = support::label_of(cx, s.win, ids::HUD_MAKER_HOVER).unwrap();
    assert!(hover_label.ends_with(NOTHING), "{hover_label}");
    let tag = support::label_of(cx, s.win, ids::HUD_MAKER_HOVER_NAME).unwrap();
    assert!(tag.contains(NOTHING), "{tag}");
    support::click_part_row(cx, s.win, &image);
    let selected = support::label_of(cx, s.win, ids::HUD_MAKER_SELECTION).unwrap();
    assert!(selected.ends_with(NOTHING), "{selected}");
}

#[gpui_kit::test]
fn a_hidden_parts_row_outlines_nothing(cx: &mut TestAppContext) {
    let env = support::env();
    let s = stock_shell(cx, &env);
    let page = open_maker(cx, &s);
    cx.update(|cx| {
        page.update(cx, |p, cx| {
            let e = p.editor_mut().unwrap();
            e.select(Some(PartKey::new(HV)));
            e.hide_selected().unwrap();
            cx.notify();
        })
    });
    show_parts(cx, &s);
    hover(cx, &s, ids::hud_maker_row(&PartKey::new(HV)));
    assert!(!support::present(cx, s.win, ids::HUD_MAKER_HOVER));
}

#[gpui_kit::test]
fn two_quick_place_changes_leave_the_later_one_saved(cx: &mut TestAppContext) {
    // Writes once raced on the background pool; several rounds make an out-of-order land show.
    let env = support::env();
    let s = stock_shell(cx, &env);
    let page = open_maker(cx, &s);
    for round in 0..10 {
        let first = PanelPlace {
            x: 40. + round as f32,
            ..PanelPlace::PARTS_DEFAULT
        };
        let later = PanelPlace {
            mode: PanelMode::DockLeft,
            ..first
        };
        cx.update(|cx| {
            page.update(cx, |p, cx| {
                p.set_place_of(Panel::Parts, first, cx);
                p.set_place_of(Panel::Parts, later, cx);
            })
        });
        cx.run_until_parked();
        assert_eq!(
            saved_places(&s).map(|p| p.parts),
            Some(later),
            "round {round}"
        );
    }
}

fn resize(cx: &mut TestAppContext, s: &support::ShellWindow, w: f32, h: f32) {
    cx.simulate_window_resize(s.win, size(px(w), px(h)));
    support::settle(cx, s.win);
}

#[gpui_kit::test]
fn the_default_panels_keep_apart_in_the_smallest_area(cx: &mut TestAppContext) {
    // The 800x600 window leaves a 544 px area: room for both default panels 8 px apart, but
    // not for the parts panel's 16 px inset too. The window is sized to give that area.
    let env = support::env();
    let s = stock_shell(cx, &env);
    let page = open_maker(cx, &s);
    support::maker_page::float_panels(cx, &s, &page);
    resize(cx, &s, 800., 600.);
    let wide = f(bounds(cx, &s, ids::HUD_MAKER_CANVAS_AREA).size.width);
    resize(cx, &s, 800. + 544. - wide, 600.);
    let area = bounds(cx, &s, ids::HUD_MAKER_CANVAS_AREA);
    assert!(f(area.size.width - px(544.)).abs() <= 1., "{area:?}");
    show_parts(cx, &s);
    let options = bounds(cx, &s, ids::HUD_MAKER_PANEL);
    let parts = bounds(cx, &s, ids::HUD_MAKER_PARTS_PANEL);
    assert!(
        parts.left() >= options.right() + px(7.),
        "{parts:?} near {options:?}"
    );
    assert!(
        parts.right() <= area.right() + px(1.),
        "{parts:?} outside {area:?}"
    );
    let dock = bounds(cx, &s, ids::HUD_MAKER_PANEL_DOCK_LEFT);
    assert!(!dock.intersects(&parts), "{dock:?} under {parts:?}");
    support::click_id(cx, s.win, ids::HUD_MAKER_PANEL_DOCK_LEFT);
    assert_eq!(
        cx.update(|cx| page.read(cx).place_of(Panel::Options)).mode,
        PanelMode::DockLeft
    );
}

#[gpui_kit::test]
fn a_wide_area_keeps_the_parts_panel_16_px_in(cx: &mut TestAppContext) {
    let env = support::env();
    let s = stock_shell(cx, &env);
    let page = open_maker(cx, &s);
    support::maker_page::float_panels(cx, &s, &page);
    resize(cx, &s, 1920., 1080.);
    show_parts(cx, &s);
    let area = bounds(cx, &s, ids::HUD_MAKER_CANVAS_AREA);
    let parts = bounds(cx, &s, ids::HUD_MAKER_PARTS_PANEL);
    assert!(
        f(area.right() - parts.right() - px(16.)).abs() <= 1.,
        "{parts:?} in {area:?}"
    );
}

#[gpui_kit::test]
fn the_canvas_fills_the_page(cx: &mut TestAppContext) {
    let env = support::env();
    let s = stock_shell(cx, &env);
    let _page = open_maker(cx, &s);
    let page = bounds(cx, &s, ids::page(Page::HudMaker));
    let area = bounds(cx, &s, ids::HUD_MAKER_CANVAS_AREA);
    let tabs = bounds(cx, &s, ids::HUD_MAKER_SCENES);
    assert!(
        f(area.left() - page.left()).abs() <= 1.
            && f(area.top() - tabs.bottom()).abs() <= 1.
            && f(area.right() - page.right()).abs() <= 1.
            && f(area.bottom() - page.bottom()).abs() <= 1.,
        "{area:?} in {page:?}"
    );
}

#[gpui_kit::test]
fn the_toolbar_floats_at_the_bottom_centre_and_collapses(cx: &mut TestAppContext) {
    let env = support::env();
    let s = stock_shell(cx, &env);
    let _page = open_maker(cx, &s);
    let bar = bounds(cx, &s, ids::HUD_MAKER_TOOLBAR);
    let area = bounds(cx, &s, ids::HUD_MAKER_CANVAS_AREA);
    let canvas = bounds(cx, &s, ids::HUD_MAKER_CANVAS);
    assert!(f(bar.center().x - canvas.center().x).abs() <= 1., "{bar:?}");
    assert!(
        f(area.bottom() - bar.bottom() - px(8.)).abs() <= 1.,
        "{bar:?}"
    );
    for id in [
        ids::HUD_MAKER_SAVE,
        ids::HUD_MAKER_UNDO,
        ids::HUD_MAKER_NAME,
    ] {
        let b = bounds(cx, &s, id);
        assert!(bar.contains(&b.center()), "{id} {b:?} outside {bar:?}");
    }
    support::click_id(cx, s.win, ids::HUD_MAKER_TOOLBAR_COLLAPSE);
    assert!(!support::present(cx, s.win, ids::HUD_MAKER_SAVE));
    assert!(support::present(cx, s.win, ids::HUD_MAKER_TOOLBAR_EXPAND));
    support::wait_real(cx, "the setting", |_| {
        saved_places(&s).is_some_and(|p| p.toolbar_collapsed)
    });
    support::click_id(cx, s.win, ids::nav(Page::Huds));
    let _page = open_maker(cx, &s);
    assert!(!support::present(cx, s.win, ids::HUD_MAKER_SAVE));
    support::click_id(cx, s.win, ids::HUD_MAKER_TOOLBAR_EXPAND);
    assert!(support::present(cx, s.win, ids::HUD_MAKER_SAVE));
}

#[gpui_kit::test]
fn the_toolbar_stands_below_the_canvas_and_occludes(cx: &mut TestAppContext) {
    // At 1920x1080 a 4:3 screen would fill the area's height; it leaves the toolbar's band.
    let env = support::env();
    let s = stock_shell(cx, &env);
    resize(cx, &s, 1920., 1080.);
    let page = open_maker(cx, &s);
    pick_screen(cx, &s, Screen::new(1440, 1080));
    let home = bounds(cx, &s, ids::HUD_MAKER_TOOLBAR);
    let canvas = bounds(cx, &s, ids::HUD_MAKER_CANVAS);
    assert!(
        home.top() >= canvas.bottom() + px(3.),
        "{home:?} must stand below {canvas:?}"
    );
    select_hv(cx, &s);
    // A press on the toolbar's own padding never reaches the canvas.
    let at = point(px(2.), home.size.height / 2.);
    cx.update_window(s.win, |_, w, cx| w.click_at(ids::HUD_MAKER_TOOLBAR, at, cx))
        .unwrap();
    support::settle(cx, s.win);
    assert_eq!(selection(cx, &page).as_deref(), Some(HV));
    // HV along the screen's bottom edge leaves the toolbar home.
    let (_, _, wide, _) = hv_box(cx, &page);
    commit(cx, &s, &page, Field::Y, &(1080 - 40).to_string());
    commit(cx, &s, &page, Field::X, &((1440 - wide) / 2).to_string());
    let hv = hv_outline(cx, &s);
    let bar = bounds(cx, &s, ids::HUD_MAKER_TOOLBAR);
    assert!(!bar.intersects(&hv), "{bar:?} covers {hv:?}");
    assert_eq!(bar, home, "the toolbar moved");
}

#[gpui_kit::test]
fn the_side_panel_leaves_the_lower_hud_clickable(cx: &mut TestAppContext) {
    let env = support::env();
    let s = stock_shell(cx, &env);
    let page = open_maker(cx, &s);
    support::maker_page::float_panels(cx, &s, &page);
    for (w, h) in [
        (1920., 1080.),
        (1280., 800.),
        (1200., 900.),
        (1024., 768.),
        (800., 600.),
    ] {
        resize(cx, &s, w, h);
        let area = bounds(cx, &s, ids::HUD_MAKER_CANVAS_AREA);
        let panel = bounds(cx, &s, ids::HUD_MAKER_PANEL);
        let cap = f(area.top()) + 0.6 * f(area.size.height);
        assert!(
            f(panel.bottom()) <= cap + 1.,
            "{w}x{h}: side panel {panel:?} reaches past {cap} in {area:?}"
        );
        click_screen(cx, &s, 220., 947.);
        assert_eq!(selection(cx, &page).as_deref(), Some(HH), "{w}x{h}");
        // Bottom right lies below the side panel at every size, where the canvas is empty.
        click_screen(cx, &s, 1910., 1070.);
        assert_eq!(selection(cx, &page), None, "{w}x{h}");
    }
}

#[gpui_kit::test]
fn a_docked_side_panel_takes_the_whole_height_of_the_area(cx: &mut TestAppContext) {
    // A docked panel no longer keeps the floating 60% cap: the canvas is narrowed beside it, so
    // it runs down to the column's 16 px margin at every size.
    let env = support::env();
    let s = stock_shell(cx, &env);
    let page = open_maker(cx, &s);
    support::maker_page::float_panels(cx, &s, &page);
    support::click_id(cx, s.win, ids::HUD_MAKER_PANEL_DOCK_RIGHT);
    for (w, h) in [(1920., 1080.), (1280., 800.), (1024., 768.), (800., 600.)] {
        resize(cx, &s, w, h);
        assert_reaches_bottom(cx, &s, &page, (w, h), Panel::Options);
    }
}

/// A docked panel's column runs to 16 px above the area's bottom. Where the dock is floored
/// (800x600) the panel folds to a rail instead, a slim tab at the area's top; a floating one
/// keeps its cap: 60% of the area for the side panel, 75% for the parts panel.
fn assert_reaches_bottom(
    cx: &mut TestAppContext,
    s: &support::ShellWindow,
    page: &Entity<HudMakerPage>,
    (w, h): (f32, f32),
    which: Panel,
) {
    let (id, share) = match which {
        Panel::Options => (ids::HUD_MAKER_PANEL, 0.6),
        Panel::Parts => (ids::HUD_MAKER_PARTS_PANEL, 0.75),
    };
    let area = bounds(cx, s, ids::HUD_MAKER_CANVAS_AREA);
    let panel = bounds(cx, s, id);
    let (mode, railed) = cx.update(|cx| {
        let page = page.read(cx);
        (page.mode_shown(which), page.is_railed(which))
    });
    if railed {
        assert!(
            panel.size.width < px(48.) && panel.top() == area.top(),
            "{w}x{h}: rail {panel:?} in {area:?}"
        );
    } else if mode != PanelMode::Floating {
        let foot = f(area.bottom()) - 16.;
        assert!(
            (f(panel.bottom()) - foot).abs() <= 1.,
            "{w}x{h}: docked {panel:?} should end at {foot} in {area:?}"
        );
    } else {
        let cap = f(area.top()) + share * f(area.size.height);
        assert!(
            f(panel.bottom()) <= cap + 1.,
            "{w}x{h}: floating {panel:?} reaches past {cap} in {area:?}"
        );
    }
}

/// Puts each panel in `mode` (left, right or floating) and opens or collapses it.
fn arrange(
    cx: &mut TestAppContext,
    s: &support::ShellWindow,
    options: (PanelMode, bool),
    parts: (PanelMode, bool),
) {
    let dock = |mode: PanelMode, left, float, right| match mode {
        PanelMode::DockLeft => left,
        PanelMode::DockRight => right,
        _ => float,
    };
    // A railed panel has no dock buttons until it opens.
    let open_rail = |cx: &mut TestAppContext, railed: &'static str, collapse: &'static str| {
        let rail = support::bounds_of(cx, s.win, railed).is_some_and(|b| b.size.width < px(48.));
        if rail {
            support::click_id(cx, s.win, collapse);
        }
    };
    open_rail(cx, ids::HUD_MAKER_PANEL, ids::HUD_MAKER_PANEL_COLLAPSE);
    support::click_id(
        cx,
        s.win,
        dock(
            options.0,
            ids::HUD_MAKER_PANEL_DOCK_LEFT,
            ids::HUD_MAKER_PANEL_FLOAT,
            ids::HUD_MAKER_PANEL_DOCK_RIGHT,
        ),
    );
    open_rail(
        cx,
        ids::HUD_MAKER_PARTS_PANEL,
        ids::HUD_MAKER_PARTS_PANEL_COLLAPSE,
    );
    support::click_id(
        cx,
        s.win,
        dock(
            parts.0,
            ids::HUD_MAKER_PARTS_PANEL_DOCK_LEFT,
            ids::HUD_MAKER_PARTS_PANEL_FLOAT,
            ids::HUD_MAKER_PARTS_PANEL_DOCK_RIGHT,
        ),
    );
    if support::present(cx, s.win, ids::HUD_MAKER_PANEL_BODY) != options.1 {
        support::click_id(cx, s.win, ids::HUD_MAKER_PANEL_COLLAPSE);
    }
    if parts.1 {
        show_parts(cx, s);
    } else {
        hide_parts(cx, s);
    }
}

#[gpui_kit::test]
fn a_docked_panel_reaches_the_area_bottom(cx: &mut TestAppContext) {
    let env = support::env();
    let s = stock_shell(cx, &env);
    let page = open_maker(cx, &s);
    support::maker_page::float_panels(cx, &s, &page);
    let (left, right, float) = (
        PanelMode::DockLeft,
        PanelMode::DockRight,
        PanelMode::Floating,
    );
    // Each panel docked alone, then both stacked with either open; the column's last panel,
    // open or a collapsed header under an open one, is the one that must reach the bottom.
    let cases = [
        ((left, true), (float, false), Panel::Options),
        ((right, true), (float, false), Panel::Options),
        ((float, true), (left, true), Panel::Parts),
        ((float, true), (right, true), Panel::Parts),
        ((left, true), (left, true), Panel::Parts),
        ((right, true), (right, true), Panel::Parts),
        ((left, false), (left, true), Panel::Parts),
        ((right, true), (right, false), Panel::Parts),
    ];
    for (w, h) in [(1920., 1080.), (1024., 768.), (800., 600.)] {
        resize(cx, &s, w, h);
        for (options, parts, last) in cases {
            arrange(cx, &s, options, parts);
            assert_reaches_bottom(cx, &s, &page, (w, h), last);
            // Two open panels in one column share it without overlapping; a narrow window shows
            // rails instead.
            let railed = cx.update(|cx| Panel::ALL.map(|p| page.read(cx).is_railed(p)));
            if options.0 == parts.0 && options.1 && parts.1 && railed == [false; 2] {
                let top = bounds(cx, &s, ids::HUD_MAKER_PANEL);
                let under = bounds(cx, &s, ids::HUD_MAKER_PARTS_PANEL);
                assert!(
                    top.bottom() <= under.top(),
                    "{w}x{h}: {top:?} over {under:?}"
                );
            }
        }
    }
}

/// Clicks the foot of the vertical scrollbar track in `list` and checks the element `last`,
/// first below the list's bottom, ends inside it after.
fn scrollbar_reaches(
    cx: &mut TestAppContext,
    s: &support::ShellWindow,
    list: &'static str,
    last: gpui_kit::ElementId,
) {
    // A list builds only its visible rows, so a last row past the bottom may not exist yet.
    let body = bounds(cx, s, list);
    let before = support::bounds_of(cx, s.win, last.clone());
    assert!(
        before.is_none_or(|b| b.bottom() > body.bottom() + px(1.)),
        "{list} should overflow: {before:?} in {body:?}"
    );
    cx.update_window(s.win, |_, w, cx| {
        w.click_at(
            list,
            point(body.size.width - px(6.), body.size.height - px(6.)),
            cx,
        )
    })
    .unwrap();
    support::settle(cx, s.win);
    let after = bounds(cx, s, last);
    assert!(
        after.bottom() <= body.bottom() + px(1.) && after.top() >= body.top(),
        "no scrollbar took the click in {list}: {before:?} -> {after:?} in {body:?}"
    );
}

fn last_part_row(cx: &mut TestAppContext, page: &Entity<HudMakerPage>) -> gpui_kit::ElementId {
    cx.update(|cx| {
        let parts = page.read(cx).editor().unwrap().model().parts();
        ids::hud_maker_row(parts.last().unwrap().key())
    })
}

fn shown_mode(cx: &mut TestAppContext, page: &Entity<HudMakerPage>, panel: Panel) -> PanelMode {
    cx.update(|cx| page.read(cx).mode_shown(panel))
}

#[gpui_kit::test]
fn a_docked_panel_body_shows_a_scrollbar(cx: &mut TestAppContext) {
    // 1024x768 is the smallest listed size where a dock holds. Both panels open in one column
    // halve it, so each body is shorter than its content.
    let env = support::env();
    let s = stock_shell(cx, &env);
    let page = open_maker(cx, &s);
    resize(cx, &s, 1024., 768.);
    let right = PanelMode::DockRight;
    arrange(cx, &s, (right, true), (right, true));
    for panel in Panel::ALL {
        assert_eq!(shown_mode(cx, &page, panel), right, "{panel:?} floats");
    }
    let last = last_part_row(cx, &page);
    scrollbar_reaches(cx, &s, ids::HUD_MAKER_PARTS, last);
    select_hv(cx, &s);
    arrange(cx, &s, (right, true), (right, true));
    scrollbar_reaches(
        cx,
        &s,
        ids::HUD_MAKER_PANEL_BODY,
        ids::HUD_MAKER_ALL_SETTINGS.into(),
    );
}

#[gpui_kit::test]
fn a_floating_panel_body_shows_a_scrollbar(cx: &mut TestAppContext) {
    let env = support::env();
    let s = stock_shell(cx, &env);
    let page = open_maker(cx, &s);
    support::maker_page::float_panels(cx, &s, &page);
    resize(cx, &s, 800., 600.);
    let float = PanelMode::Floating;
    // The part list first, with nothing selected for a floating panel to keep off.
    arrange(cx, &s, (float, false), (float, true));
    let last = last_part_row(cx, &page);
    scrollbar_reaches(cx, &s, ids::HUD_MAKER_PARTS, last);
    // HV's rows overflow the side panel at the smallest window.
    select_hv(cx, &s);
    arrange(cx, &s, (float, true), (float, false));
    scrollbar_reaches(
        cx,
        &s,
        ids::HUD_MAKER_PANEL_BODY,
        ids::HUD_MAKER_ALL_SETTINGS.into(),
    );
}

#[gpui_kit::test]
fn a_docked_part_list_fills_its_panel(cx: &mut TestAppContext) {
    let env = support::env();
    let s = stock_shell(cx, &env);
    let page = open_maker(cx, &s);
    support::maker_page::float_panels(cx, &s, &page);
    resize(cx, &s, 1920., 1080.);
    arrange(
        cx,
        &s,
        (PanelMode::Floating, true),
        (PanelMode::DockLeft, true),
    );
    let list = bounds(cx, &s, ids::HUD_MAKER_PARTS);
    let panel = bounds(cx, &s, ids::HUD_MAKER_PARTS_PANEL);
    // Only the list's 4 px inset and the panel's border lie below it.
    assert!(
        f(panel.bottom() - list.bottom()) <= 6.,
        "{list:?} stops short of {panel:?}"
    );
}

#[gpui_kit::test]
fn a_collapsed_docked_panel_stays_its_header(cx: &mut TestAppContext) {
    let env = support::env();
    let s = stock_shell(cx, &env);
    let page = open_maker(cx, &s);
    support::maker_page::float_panels(cx, &s, &page);
    resize(cx, &s, 1920., 1080.);
    let left = PanelMode::DockLeft;
    // Collapsed over an open panel, then under one.
    let cases = [
        (
            (left, false),
            (left, true),
            ids::HUD_MAKER_PANEL,
            ids::HUD_MAKER_PANEL_TITLE,
        ),
        (
            (left, true),
            (left, false),
            ids::HUD_MAKER_PARTS_PANEL,
            ids::HUD_MAKER_PARTS_PANEL_TITLE,
        ),
    ];
    for (options, parts, frame, title) in cases {
        arrange(cx, &s, options, parts);
        let frame = bounds(cx, &s, frame);
        let title = bounds(cx, &s, title);
        assert!(
            f(frame.size.height - title.size.height) <= 4.,
            "{options:?} {parts:?}: collapsed {frame:?} grew past its title {title:?}"
        );
    }
}

#[gpui_kit::test]
fn a_floating_panel_keeps_off_a_tall_docked_panel(cx: &mut TestAppContext) {
    // The docked part list is drawn far taller than the list's own 480 px, so a floating panel
    // placed over its lower half must still move off it.
    let env = support::env();
    let s = stock_shell(cx, &env);
    let page = open_maker(cx, &s);
    support::maker_page::float_panels(cx, &s, &page);
    resize(cx, &s, 1920., 1080.);
    arrange(
        cx,
        &s,
        (PanelMode::Floating, true),
        (PanelMode::DockLeft, true),
    );
    let place = PanelPlace {
        mode: PanelMode::Floating,
        x: 0.,
        y: 700.,
        collapsed: false,
    };
    cx.update(|cx| page.update(cx, |p, cx| p.set_place_of(Panel::Options, place, cx)));
    support::settle(cx, s.win);
    let docked = bounds(cx, &s, ids::HUD_MAKER_PARTS_PANEL);
    let floating = bounds(cx, &s, ids::HUD_MAKER_PANEL);
    assert!(
        !floating.intersects(&docked),
        "{floating:?} settled onto {docked:?}"
    );
}

#[gpui_kit::test]
fn part_row_text_ends_left_of_the_scrollbar(cx: &mut TestAppContext) {
    // The bar is 16 px wide over the list's right edge; a row's name and state stay clear of it.
    let env = support::env();
    let s = stock_shell(cx, &env);
    let page = open_maker(cx, &s);
    support::maker_page::float_panels(cx, &s, &page);
    resize(cx, &s, 1920., 1080.);
    arrange(
        cx,
        &s,
        (PanelMode::Floating, false),
        (PanelMode::DockLeft, true),
    );
    let list = bounds(cx, &s, ids::HUD_MAKER_PARTS);
    let keys: Vec<PartKey> = cx.update(|cx| {
        let parts = page.read(cx).editor().unwrap().model().parts();
        parts.iter().take(20).map(|p| p.key().clone()).collect()
    });
    let bar = list.right() - px(16.);
    let mut states = 0;
    for key in &keys {
        let name = bounds(cx, &s, ids::hud_maker_row_name(key));
        assert!(
            name.right() <= bar,
            "{key:?}: {name:?} runs under the bar at {bar:?}"
        );
        if let Some(state) = support::bounds_of(cx, s.win, ids::hud_maker_row_state(key)) {
            states += 1;
            assert!(
                state.right() <= bar,
                "{key:?}: {state:?} runs under the bar at {bar:?}"
            );
        }
    }
    assert!(states > 0, "no row among the first 20 shows a state tag");
}

#[gpui_kit::test]
fn everything_fits_from_the_smallest_window_to_full_hd(cx: &mut TestAppContext) {
    let env = support::env();
    let s = stock_shell(cx, &env);
    let page = open_maker(cx, &s);
    support::maker_page::float_panels(cx, &s, &page);
    for (w, h) in [(800., 600.), (1024., 768.), (1920., 1080.)] {
        resize(cx, &s, w, h);
        let area = bounds(cx, &s, ids::HUD_MAKER_CANVAS_AREA);
        let canvas = bounds(cx, &s, ids::HUD_MAKER_CANVAS);
        let inside = |b: Bounds<Pixels>| {
            b.left() >= area.left() - px(1.)
                && b.top() >= area.top() - px(1.)
                && b.right() <= area.right() + px(1.)
                && b.bottom() <= area.bottom() + px(1.)
        };
        assert!(
            inside(canvas),
            "{w}x{h}: canvas {canvas:?} outside {area:?}"
        );
        // The screen fills the area's width, its height, or the height above the toolbar's band.
        let bar = bounds(cx, &s, ids::HUD_MAKER_TOOLBAR);
        let above = area.size.height - bar.size.height - px(12.);
        assert!(
            f(canvas.size.width - area.size.width).abs() <= 1.
                || f(canvas.size.height - area.size.height).abs() <= 1.
                || f(canvas.size.height - above).abs() <= 1.,
            "{w}x{h}: {canvas:?} does not fill {area:?} on either axis"
        );
        let boxes = [
            ("toolbar", bounds(cx, &s, ids::HUD_MAKER_TOOLBAR)),
            ("side panel", bounds(cx, &s, ids::HUD_MAKER_PANEL)),
            ("parts panel", bounds(cx, &s, ids::HUD_MAKER_PARTS_PANEL)),
        ];
        for (name, b) in boxes {
            assert!(inside(b), "{w}x{h}: {name} {b:?} outside {area:?}");
        }
        for i in 0..boxes.len() {
            for j in i + 1..boxes.len() {
                assert!(
                    !boxes[i].1.intersects(&boxes[j].1),
                    "{w}x{h}: {} {:?} covers {} {:?}",
                    boxes[i].0,
                    boxes[i].1,
                    boxes[j].0,
                    boxes[j].1
                );
            }
        }
    }
}

#[gpui_kit::test]
fn a_docked_column_fits_the_area_and_its_last_row_can_be_reached(cx: &mut TestAppContext) {
    let env = support::env();
    let s = stock_shell(cx, &env);
    resize(cx, &s, 1024., 768.);
    let page = open_maker(cx, &s);
    support::maker_page::float_panels(cx, &s, &page);
    show_parts(cx, &s);
    click_row(cx, &s, &PartKey::new("HudWeaponAmmo"));
    show_parts(cx, &s);
    support::click_id(cx, s.win, ids::HUD_MAKER_PANEL_DOCK_LEFT);
    support::click_id(cx, s.win, ids::HUD_MAKER_PARTS_PANEL_DOCK_LEFT);
    let area = bounds(cx, &s, ids::HUD_MAKER_CANVAS_AREA);
    let parts = bounds(cx, &s, ids::HUD_MAKER_PARTS_PANEL);
    // The column stops 16 px short of the area's bottom, so its bottom border shows.
    assert!(
        parts.bottom() <= area.bottom() - px(15.),
        "{parts:?} past {area:?}"
    );
    let list = bounds(cx, &s, ids::HUD_MAKER_PARTS);
    let last = cx.update(|cx| {
        let parts = page.read(cx).editor().unwrap().model().parts();
        parts.last().unwrap().key().clone()
    });
    send(
        cx,
        &s,
        gpui_kit::ScrollWheelEvent {
            position: list.center(),
            delta: gpui_kit::ScrollDelta::Lines(point(0., -400.)),
            modifiers: gpui_kit::Modifiers::default(),
            touch_phase: gpui_kit::TouchPhase::Moved,
        },
    );
    let row = bounds(cx, &s, ids::hud_maker_row(&last));
    assert!(
        row.top() >= list.top() - px(1.) && row.bottom() <= list.bottom() + px(1.),
        "{row:?} outside {list:?}"
    );
    assert!(
        row.bottom() <= area.bottom() + px(1.),
        "{row:?} past {area:?}"
    );
    support::click_part_row(cx, s.win, &last);
    assert_eq!(selection(cx, &page).as_deref(), Some(last.as_str()));
}

fn collapsed(cx: &mut TestAppContext, page: &Entity<HudMakerPage>, panel: Panel) -> bool {
    cx.update(|cx| page.read(cx).place_of(panel).collapsed)
}

#[gpui_kit::test]
fn a_narrow_area_keeps_one_floating_panel_open(cx: &mut TestAppContext) {
    let env = support::env();
    let s = stock_shell(cx, &env);
    resize(cx, &s, 800., 600.);
    let page = open_maker(cx, &s);
    support::maker_page::float_panels(cx, &s, &page);
    assert!(!collapsed(cx, &page, Panel::Options));
    support::click_id(cx, s.win, ids::HUD_MAKER_PARTS_PANEL_COLLAPSE);
    assert!(!collapsed(cx, &page, Panel::Parts));
    assert!(
        collapsed(cx, &page, Panel::Options),
        "opening the parts panel closes the side panel"
    );
    support::click_id(cx, s.win, ids::HUD_MAKER_PANEL_COLLAPSE);
    assert!(!collapsed(cx, &page, Panel::Options));
    assert!(
        collapsed(cx, &page, Panel::Parts),
        "opening the side panel closes the parts panel"
    );
    support::wait_real(cx, "the setting", |_| {
        saved_places(&s).is_some_and(|p| !p.options.collapsed && p.parts.collapsed)
    });
}

#[gpui_kit::test]
fn a_wide_area_keeps_both_floating_panels_open(cx: &mut TestAppContext) {
    let env = support::env();
    let s = stock_shell(cx, &env);
    resize(cx, &s, 1024., 768.);
    let page = open_maker(cx, &s);
    show_parts(cx, &s);
    assert!(!collapsed(cx, &page, Panel::Parts));
    assert!(!collapsed(cx, &page, Panel::Options));
}

const FLOATING: [(&str, &str); 3] = [
    ("toolbar", ids::HUD_MAKER_TOOLBAR),
    ("side panel", ids::HUD_MAKER_PANEL),
    ("parts panel", ids::HUD_MAKER_PARTS_PANEL),
];

// No two floating things overlap, all lie inside the area, and nothing covers the middle of a
// toolbar control.
fn assert_apart(cx: &mut TestAppContext, s: &support::ShellWindow, label: &str) {
    let area = bounds(cx, s, ids::HUD_MAKER_CANVAS_AREA);
    let boxes = FLOATING.map(|(name, id)| (name, bounds(cx, s, id)));
    for (i, (name, b)) in boxes.iter().enumerate() {
        assert!(
            b.left() >= area.left() - px(1.)
                && b.top() >= area.top() - px(1.)
                && b.right() <= area.right() + px(1.)
                && b.bottom() <= area.bottom() + px(1.),
            "{label}: {name} {b:?} outside {area:?}"
        );
        for (other, o) in &boxes[i + 1..] {
            assert!(!b.intersects(o), "{label}: {name} {b:?} on {other} {o:?}");
        }
    }
    let controls = [
        ids::HUD_MAKER_SCREEN_MENU,
        ids::HUD_MAKER_UNDO,
        ids::HUD_MAKER_REDO,
        ids::HUD_MAKER_NAME,
        ids::HUD_MAKER_SAVE,
        ids::HUD_MAKER_INSTALL,
    ]
    .map(gpui_kit::ElementId::from);
    for id in controls {
        let at = support::bounds_of(cx, s.win, id.clone()).unwrap().center();
        for (name, b) in &boxes[1..] {
            assert!(!b.contains(&at), "{label}: {name} covers {id:?}");
        }
    }
}

// Every toolbar and panel button does its job when clicked where it is drawn; the places are
// put back afterwards.
fn assert_buttons_work(
    cx: &mut TestAppContext,
    s: &support::ShellWindow,
    page: &Entity<HudMakerPage>,
    label: &str,
) {
    let selected = selection(cx, page);
    support::click_id(cx, s.win, ids::HUD_MAKER_TOOLBAR_COLLAPSE);
    assert!(
        cx.update(|cx| page.read(cx).toolbar_collapsed()),
        "{label}: hide toolbar"
    );
    support::click_id(cx, s.win, ids::HUD_MAKER_TOOLBAR_EXPAND);
    assert!(
        !cx.update(|cx| page.read(cx).toolbar_collapsed()),
        "{label}: show toolbar"
    );
    let screen = cx.update(|cx| page.read(cx).editor().unwrap().screen());
    let other = *Screen::PRESETS.iter().find(|&&p| p != screen).unwrap();
    for preset in [other, screen] {
        pick_screen(cx, s, preset);
        let now = cx.update(|cx| page.read(cx).editor().unwrap().screen());
        assert_eq!(now, preset, "{label}: screen button {preset:?}");
    }
    let places = Panel::ALL.map(|p| cx.update(|cx| page.read(cx).place_of(p)));
    let panel_ids = [
        (
            Panel::Options,
            ids::HUD_MAKER_PANEL_COLLAPSE,
            ids::HUD_MAKER_PANEL_DOCK_LEFT,
            ids::HUD_MAKER_PANEL_FLOAT,
        ),
        (
            Panel::Parts,
            ids::HUD_MAKER_PARTS_PANEL_COLLAPSE,
            ids::HUD_MAKER_PARTS_PANEL_DOCK_LEFT,
            ids::HUD_MAKER_PARTS_PANEL_FLOAT,
        ),
    ];
    for (panel, collapse, dock, float) in panel_ids {
        let place = |cx: &mut TestAppContext| cx.update(|cx| page.read(cx).place_of(panel));
        // Shown collapsed: by the player's place, or folded off the selection for now.
        let shut = |cx: &mut TestAppContext| {
            cx.update(|cx| {
                let page = page.read(cx);
                page.place_of(panel).collapsed || page.is_folded(panel)
            })
        };
        let was = shut(cx);
        support::click_id(cx, s.win, collapse);
        assert_eq!(shut(cx), !was, "{label}: {panel:?} collapse");
        support::click_id(cx, s.win, collapse);
        assert_eq!(shut(cx), was, "{label}: {panel:?} collapse back");
        support::click_id(cx, s.win, dock);
        assert_eq!(
            place(cx).mode,
            PanelMode::DockLeft,
            "{label}: {panel:?} dock"
        );
        // A docked panel may fold to a rail, which opens it to show its Float button.
        if cx.update(|cx| page.read(cx).is_railed(panel)) {
            support::click_id(cx, s.win, collapse);
        }
        support::click_id(cx, s.win, float);
        assert_eq!(
            place(cx).mode,
            PanelMode::Floating,
            "{label}: {panel:?} float"
        );
        // Collapsed first, so restoring the open one never closes it again.
        let mut order = Panel::ALL;
        order.sort_by_key(|&p| !places[p as usize].collapsed);
        for p in order {
            cx.update(|cx| page.update(cx, |pg, cx| pg.set_place_of(p, places[p as usize], cx)));
        }
        support::settle(cx, s.win);
    }
    assert_eq!(
        selection(cx, page),
        selected,
        "{label}: a click reached the canvas"
    );
}

// With the parts panel open, nothing selected or Health or Ammo selected at 16:9 and 4:3.
fn keep_apart_at(cx: &mut TestAppContext, w: f32, h: f32) {
    let scenes: [(Option<&str>, Screen); 5] = [
        (None, Screen::new(1920, 1080)),
        (Some(HV), Screen::new(1920, 1080)),
        (Some(HV), Screen::new(1440, 1080)),
        (Some("HudWeaponAmmo"), Screen::new(1920, 1080)),
        (Some("HudWeaponAmmo"), Screen::new(1440, 1080)),
    ];
    for (part, screen) in scenes {
        let label = format!("{w}x{h} {part:?} {}x{}", screen.wide(), screen.tall());
        let env = support::env();
        let s = stock_shell(cx, &env);
        resize(cx, &s, w, h);
        let page = open_maker(cx, &s);
        support::maker_page::float_panels(cx, &s, &page);
        pick_screen(cx, &s, screen);
        show_parts(cx, &s);
        if let Some(key) = part {
            support::click_part_row(cx, s.win, &PartKey::new(key));
            assert_eq!(
                selection(cx, &page).as_deref(),
                Some(key),
                "{label}: select"
            );
        }
        assert_apart(cx, &s, &label);
        assert_buttons_work(cx, &s, &page, &label);
        assert_apart(cx, &s, &format!("{label} after the clicks"));
    }
}

#[gpui_kit::test]
fn floating_things_keep_apart_and_every_button_works_at_800x600(cx: &mut TestAppContext) {
    keep_apart_at(cx, 800., 600.);
}

#[gpui_kit::test]
fn floating_things_keep_apart_and_every_button_works_at_1024x768(cx: &mut TestAppContext) {
    keep_apart_at(cx, 1024., 768.);
}

#[gpui_kit::test]
fn floating_things_keep_apart_and_every_button_works_at_1280x800(cx: &mut TestAppContext) {
    keep_apart_at(cx, 1280., 800.);
}

#[gpui_kit::test]
fn floating_things_keep_apart_and_every_button_works_at_1920x1080(cx: &mut TestAppContext) {
    keep_apart_at(cx, 1920., 1080.);
}

#[gpui_kit::test]
fn toasts_stay_at_the_bottom_above_the_toolbar(cx: &mut TestAppContext) {
    // A 4:3 screen that would fill a 1920x1080 area's height, with HV stretched over its lower
    // part: the toolbar keeps its band under the screen and the toasts stand at the bottom.
    let env = support::env();
    let s = stock_shell(cx, &env);
    resize(cx, &s, 1920., 1080.);
    let page = open_maker(cx, &s);
    pick_screen(cx, &s, Screen::new(1440, 1080));
    select_hv(cx, &s);
    commit(cx, &s, &page, Field::X, "0");
    commit(cx, &s, &page, Field::Y, "200");
    commit(cx, &s, &page, Field::Wide, "1440");
    commit(cx, &s, &page, Field::Tall, "880");
    let canvas = bounds(cx, &s, ids::HUD_MAKER_CANVAS);
    let bar = bounds(cx, &s, ids::HUD_MAKER_TOOLBAR);
    assert!(
        bar.top() > canvas.bottom(),
        "{bar:?} left its band under {canvas:?}"
    );
    cx.update_window(s.win, |_, window, cx| {
        tf2tools_app::shell::toast::toast(
            window,
            cx,
            tf2tools_app::shell::toast::ToastKind::Info,
            "Hello",
            "",
        )
    })
    .unwrap();
    support::settle(cx, s.win);
    let window = cx
        .update_window(s.win, |_, w, _| w.viewport_size())
        .unwrap();
    let toast = gpui_kit::ElementId::from(ids::TOAST);
    let toasts: Vec<Bounds<Pixels>> = cx
        .update_window(s.win, |_, w, _| {
            gpui_kit::base::test_support::snapshots(w)
                .iter()
                .filter(|s| s.path().last() == Some(&toast))
                .map(|s| s.bounds())
                .collect()
        })
        .unwrap();
    assert!(!toasts.is_empty());
    for t in toasts {
        assert!(
            t.top() >= px(0.) && t.top() > window.height / 2.,
            "{t:?} left the bottom of {window:?}"
        );
        assert!(t.bottom() <= bar.top(), "{t:?} covers the toolbar {bar:?}");
    }
}

#[gpui_kit::test]
fn at_800x600_both_default_panels_keep_a_margin_from_the_area_edges(cx: &mut TestAppContext) {
    let env = support::env();
    let s = stock_shell(cx, &env);
    let page = open_maker(cx, &s);
    support::maker_page::float_panels(cx, &s, &page);
    resize(cx, &s, 800., 600.);
    let area = bounds(cx, &s, ids::HUD_MAKER_CANVAS_AREA);
    let options = bounds(cx, &s, ids::HUD_MAKER_PANEL);
    let parts = bounds(cx, &s, ids::HUD_MAKER_PARTS_PANEL);
    assert!(
        options.left() >= area.left() + px(7.),
        "{options:?} in {area:?}"
    );
    assert!(
        parts.right() <= area.right() - px(7.),
        "{parts:?} in {area:?}"
    );
    assert!(
        parts.left() >= options.right() + px(7.),
        "{parts:?} near {options:?}"
    );
}

const MEDIC_LABEL: &str = "HudMedicCharge/ChargeLabel";
const MEDI_GUN: &str = "Shows when you hold a Medi Gun";

#[gpui_kit::test]
fn a_part_the_game_hides_draws_while_it_is_selected(cx: &mut TestAppContext) {
    let env = support::env();
    let s = stock_shell(cx, &env);
    let page = open_maker(cx, &s);
    let meter = PartKey::new(MEDIC);
    assert!(!support::present(cx, s.win, ids::hud_maker_part(&meter)));
    click_row(cx, &s, &meter);
    let before = bounds(cx, &s, ids::hud_maker_part(&meter));
    assert_eq!(bounds(cx, &s, ids::HUD_MAKER_SELECTION), before);
    assert_eq!(
        support::label_of(cx, s.win, ids::HUD_MAKER_PANEL_SHOWS).as_deref(),
        Some(MEDI_GUN)
    );
    support::press(cx, s.win, "left");
    let after = bounds(cx, &s, ids::hud_maker_part(&meter));
    assert!(
        after.left() < before.left(),
        "{after:?} did not move left of {before:?}"
    );
    support::press(cx, s.win, "escape");
    assert_eq!(selection(cx, &page), None);
    assert!(!support::present(cx, s.win, ids::hud_maker_part(&meter)));
}

#[gpui_kit::test]
fn a_child_of_a_hidden_panel_draws_the_whole_panel_until_the_selection_leaves(
    cx: &mut TestAppContext,
) {
    let env = support::env();
    let s = stock_shell(cx, &env);
    let _page = open_maker(cx, &s);
    let (meter, label) = (PartKey::new(MEDIC), PartKey::new(MEDIC_LABEL));
    click_row(cx, &s, &label);
    assert!(support::present(cx, s.win, ids::hud_maker_part(&meter)));
    assert_eq!(
        bounds(cx, &s, ids::HUD_MAKER_SELECTION),
        bounds(cx, &s, ids::hud_maker_part(&label))
    );
    assert_eq!(
        support::label_of(cx, s.win, ids::HUD_MAKER_PANEL_SHOWS).as_deref(),
        Some(MEDI_GUN)
    );
    select_hv(cx, &s);
    assert!(!support::present(cx, s.win, ids::hud_maker_part(&meter)));
    assert!(!support::present(cx, s.win, ids::hud_maker_part(&label)));
}

#[gpui_kit::test]
fn the_parts_list_tells_game_hidden_from_player_hidden(cx: &mut TestAppContext) {
    let env = support::env();
    let s = stock_shell(cx, &env);
    let _page = open_maker(cx, &s);
    show_parts(cx, &s);
    for key in [MEDIC, MEDIC_LABEL] {
        let label = support::label_of(cx, s.win, ids::hud_maker_row(&PartKey::new(key))).unwrap();
        assert!(label.ends_with(" (shows in game)"), "{label}");
    }
}

const HH: &str = "HudPlayerStatus/HudPlayerHealth";

/// The stock HUD with Heal numbers moved over the health value and an empty label on it, both
/// above it and neither drawing anything; screen x 235 is clear of the class model.
fn covered_shell(cx: &mut TestAppContext, env: &support::Env) -> support::ShellWindow {
    covered_shell_with(cx, env, "")
}

/// As `covered_shell`, with `extra` keys on Heal numbers.
fn covered_shell_with(
    cx: &mut TestAppContext,
    env: &support::Env,
    extra: &str,
) -> support::ShellWindow {
    use support::hud_stock::{HEALTH, HUDLAYOUT, label, with_blocks};
    let stock = "CHealthAccountPanel { xpos c-60 ypos c-8 wide 120 tall 70 }";
    assert!(HUDLAYOUT.contains(stock));
    let layout = HUDLAYOUT.replace(
        stock,
        &format!("CHealthAccountPanel {{ xpos 60 ypos r90 zpos 9 wide 100 tall 60 {extra} }}"),
    );
    let blank = label("Blank", "90", "52", &[("zpos", "9"), ("labelText", "\"\"")]);
    let health = with_blocks(HEALTH, &blank);
    support::seed_stock_game_with(
        &env.game,
        &[
            ("scripts/hudlayout.res", Some(layout.as_str())),
            ("resource/ui/hudplayerhealth.res", Some(health.as_str())),
        ],
    );
    support::shell_window(cx, env, found(env))
}

/// Presses and releases the left button on screen pixel (x, y) with `modifiers` held.
fn press_screen(
    cx: &mut TestAppContext,
    s: &support::ShellWindow,
    at: (f32, f32),
    modifiers: gpui_kit::Modifiers,
) {
    press_screen_n(cx, s, at, modifiers, 1);
}

/// As `press_screen`, as the `click_count`th click of a quick run.
fn press_screen_n(
    cx: &mut TestAppContext,
    s: &support::ShellWindow,
    (x, y): (f32, f32),
    modifiers: gpui_kit::Modifiers,
    click_count: usize,
) {
    let canvas = bounds(cx, s, ids::HUD_MAKER_CANVAS);
    let k = scale(cx, s);
    let position = canvas.origin + point(px(x * k), px(y * k));
    let button = gpui_kit::MouseButton::Left;
    send(
        cx,
        s,
        gpui_kit::MouseDownEvent {
            button,
            position,
            modifiers,
            click_count,
            first_mouse: false,
        },
    );
    send(
        cx,
        s,
        gpui_kit::MouseUpEvent {
            button,
            position,
            modifiers,
            click_count,
        },
    );
}

#[gpui_kit::test]
fn a_click_picks_the_drawn_health_value_under_invisible_parts(cx: &mut TestAppContext) {
    let env = support::env();
    let s = covered_shell(cx, &env);
    let page = open_maker(cx, &s);
    press_screen(cx, &s, (235., 940.), alt());
    assert_eq!(selection(cx, &page).as_deref(), Some(HV));
}

#[gpui_kit::test]
fn alt_or_a_second_click_on_the_spot_steps_through_the_parts_under_it(cx: &mut TestAppContext) {
    let env = support::env();
    let s = covered_shell(cx, &env);
    let page = open_maker(cx, &s);
    let none = gpui_kit::Modifiers::default();
    let alt = gpui_kit::Modifiers {
        alt: true,
        ..Default::default()
    };
    press_screen(cx, &s, (235., 940.), none);
    assert_eq!(selection(cx, &page).as_deref(), Some(HH), "the group first");
    press_screen(cx, &s, (235., 940.), alt);
    assert_eq!(selection(cx, &page).as_deref(), Some(HV), "Alt goes inside");
    press_screen(cx, &s, (235., 940.), alt);
    let blank = format!("{HH}/Blank");
    assert_eq!(
        selection(cx, &page),
        Some(blank.clone()),
        "smallest invisible next"
    );
    press_screen(cx, &s, (235., 940.), none);
    assert_eq!(
        selection(cx, &page),
        Some(format!("{HH}/PlayerStatusHealthImage")),
        "a second press on the spot steps on"
    );
    press_screen(cx, &s, (235., 940.), alt);
    assert_eq!(selection(cx, &page).as_deref(), Some("CHealthAccountPanel"));
    press_screen(cx, &s, (235., 940.), alt);
    assert_eq!(selection(cx, &page).as_deref(), Some(HH));
    press_screen(cx, &s, (235., 940.), alt);
    assert_eq!(selection(cx, &page).as_deref(), Some(HV), "and wraps round");
    press_screen(cx, &s, (250., 945.), none);
    assert_eq!(
        selection(cx, &page).as_deref(),
        Some(HV),
        "a new spot picks afresh"
    );
}

#[gpui_kit::test]
fn dragging_over_invisible_parts_moves_the_picked_part(cx: &mut TestAppContext) {
    let env = support::env();
    let s = covered_shell(cx, &env);
    let page = open_maker(cx, &s);
    let k = scale(cx, &s);
    let from = bounds(cx, &s, ids::hud_maker_part(&PartKey::new(HV))).center();
    drag_window(cx, &s, from, (45. * k, 0.));
    assert_eq!(selection(cx, &page).as_deref(), Some(HH));
    assert_eq!(hv_box(cx, &page).0, 216);
}

#[gpui_kit::test]
fn a_double_click_or_an_edit_between_clicks_keeps_the_pick(cx: &mut TestAppContext) {
    let env = support::env();
    let s = covered_shell(cx, &env);
    let page = open_maker(cx, &s);
    let none = gpui_kit::Modifiers::default();
    press_screen_n(cx, &s, (235., 940.), none, 1);
    press_screen_n(cx, &s, (235., 940.), none, 2);
    assert_eq!(
        selection(cx, &page).as_deref(),
        Some(HV),
        "the second half of a double-click"
    );
    support::press(cx, s.win, "right");
    press_screen(cx, &s, (235., 940.), none);
    assert_eq!(selection(cx, &page).as_deref(), Some(HV), "after a nudge");
    support::press(cx, s.win, "ctrl-z");
    press_screen(cx, &s, (235., 940.), none);
    assert_eq!(selection(cx, &page).as_deref(), Some(HV), "after an undo");
    press_screen(cx, &s, (235., 940.), none);
    assert_ne!(
        selection(cx, &page).as_deref(),
        Some(HV),
        "a plain second click steps on"
    );
}

#[gpui_kit::test]
fn a_clear_colour_paints_nothing_and_takes_no_click(cx: &mut TestAppContext) {
    let env = support::env();
    let s = covered_shell_with(cx, &env, "bgcolor_override \"0 0 0 0\"");
    let page = open_maker(cx, &s);
    let heal = PartKey::new("CHealthAccountPanel");
    assert!(!support::present(cx, s.win, ids::hud_maker_fill(&heal)));
    press_screen(cx, &s, (235., 940.), alt());
    assert_eq!(selection(cx, &page).as_deref(), Some(HV));
}

#[gpui_kit::test]
fn a_half_clear_colour_paints_and_takes_the_click(cx: &mut TestAppContext) {
    let env = support::env();
    let s = covered_shell_with(cx, &env, "bgcolor_override \"0 0 0 128\"");
    let page = open_maker(cx, &s);
    let heal = PartKey::new("CHealthAccountPanel");
    let fill = bounds(cx, &s, ids::hud_maker_fill(&heal));
    let part = bounds(cx, &s, ids::hud_maker_part(&heal));
    assert!(
        f(fill.size.width - part.size.width).abs() <= 1.,
        "{fill:?} vs {part:?}"
    );
    click_screen(cx, &s, 235., 940.);
    assert_eq!(selection(cx, &page).as_deref(), Some("CHealthAccountPanel"));
}

// How the toolbar test sets the page up: the panels as they first open, docked; floating with
// the sidebar shut (shot 43), the side panel docked left (shot 42), or neither.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Setup {
    Default,
    Plain,
    NarrowSidebar,
    DockedLeft,
    DockedRight,
}

// The toolbar never covers the canvas and every panel stays inside the area: at rest, with HV
// selected, and with HV stretched along the screen's bottom under the toolbar (shot 06).
fn toolbar_keeps_off_the_canvas_at(cx: &mut TestAppContext, w: f32, h: f32, setup: Setup) {
    let label = format!("{w}x{h} {setup:?}");
    let env = support::env();
    let s = stock_shell(cx, &env);
    resize(cx, &s, w, h);
    if setup == Setup::NarrowSidebar {
        support::click_id(cx, s.win, ids::SIDEBAR_TOGGLE);
    }
    let page = open_maker(cx, &s);
    if setup != Setup::Default {
        support::maker_page::float_panels(cx, &s, &page);
    }
    if let Some(mode) = match setup {
        Setup::DockedLeft => Some(PanelMode::DockLeft),
        Setup::DockedRight => Some(PanelMode::DockRight),
        _ => None,
    } {
        let place = PanelPlace {
            mode,
            collapsed: false,
            ..cx.update(|cx| page.read(cx).place_of(Panel::Options))
        };
        cx.update(|cx| page.update(cx, |p, cx| p.set_place_of(Panel::Options, place, cx)));
        support::settle(cx, s.win);
    }
    let check = |cx: &mut TestAppContext, when: &str| {
        let area = bounds(cx, &s, ids::HUD_MAKER_CANVAS_AREA);
        let canvas = bounds(cx, &s, ids::HUD_MAKER_CANVAS);
        let bar = bounds(cx, &s, ids::HUD_MAKER_TOOLBAR);
        assert!(
            !bar.intersects(&canvas),
            "{label} {when}: toolbar {bar:?} covers the canvas {canvas:?}"
        );
        assert!(canvas.size.height > px(100.), "{label} {when}: {canvas:?}");
        // Two rows of buttons at most: each row is a button tall, 8 px apart, inside the
        // bar's padding and border.
        let button = bounds(cx, &s, ids::HUD_MAKER_UNDO);
        assert!(
            bar.size.height < button.size.height * 2. + px(24.),
            "{label} {when}: toolbar {bar:?} takes more than two rows"
        );
        let inside = |b: Bounds<Pixels>| {
            b.left() >= area.left() - px(1.)
                && b.top() >= area.top() - px(1.)
                && b.right() <= area.right() + px(1.)
                && b.bottom() <= area.bottom() + px(1.)
        };
        for (name, id) in [
            ("toolbar", ids::HUD_MAKER_TOOLBAR),
            ("side panel", ids::HUD_MAKER_PANEL),
            ("parts panel", ids::HUD_MAKER_PARTS_PANEL),
        ] {
            let b = bounds(cx, &s, id);
            assert!(inside(b), "{label} {when}: {name} {b:?} outside {area:?}");
        }
    };
    check(cx, "at rest");
    select_hv(cx, &s);
    assert_eq!(selection(cx, &page).as_deref(), Some(HV), "{label}");
    check(cx, "with HV selected");
    let screen = cx.update(|cx| page.read(cx).editor().unwrap().screen());
    commit(cx, &s, &page, Field::X, "0");
    commit(cx, &s, &page, Field::Wide, &screen.wide().to_string());
    commit(cx, &s, &page, Field::Tall, "60");
    commit(cx, &s, &page, Field::Y, &(screen.tall() - 70).to_string());
    check(cx, "with HV along the bottom");
}

// One window per test: an edited HUD Maker in an earlier window would hold the next one's
// sidebar click behind the leave check.
#[gpui_kit::test]
fn at_800x600_the_toolbar_keeps_off_the_docked_default_canvas(cx: &mut TestAppContext) {
    toolbar_keeps_off_the_canvas_at(cx, 800., 600., Setup::Default);
}

#[gpui_kit::test]
fn at_1280x800_the_toolbar_keeps_off_the_docked_default_canvas(cx: &mut TestAppContext) {
    toolbar_keeps_off_the_canvas_at(cx, 1280., 800., Setup::Default);
}

#[gpui_kit::test]
fn at_1440x900_the_toolbar_keeps_off_the_docked_default_canvas(cx: &mut TestAppContext) {
    toolbar_keeps_off_the_canvas_at(cx, 1440., 900., Setup::Default);
}

#[gpui_kit::test]
fn at_800x600_the_toolbar_keeps_off_the_canvas(cx: &mut TestAppContext) {
    toolbar_keeps_off_the_canvas_at(cx, 800., 600., Setup::Plain);
}

#[gpui_kit::test]
fn at_800x600_the_toolbar_keeps_off_the_canvas_with_the_sidebar_shut(cx: &mut TestAppContext) {
    toolbar_keeps_off_the_canvas_at(cx, 800., 600., Setup::NarrowSidebar);
}

#[gpui_kit::test]
fn at_800x600_the_toolbar_keeps_off_the_canvas_beside_a_docked_panel(cx: &mut TestAppContext) {
    toolbar_keeps_off_the_canvas_at(cx, 800., 600., Setup::DockedLeft);
}

#[gpui_kit::test]
fn at_1280x800_the_toolbar_keeps_off_the_canvas(cx: &mut TestAppContext) {
    toolbar_keeps_off_the_canvas_at(cx, 1280., 800., Setup::Plain);
}

// Shot 10: with the side panel docked left the open part list stands over the canvas; a part
// moved under the list folds it where it stands instead of sending it across the canvas.
fn parts_panel_folds_in_place_at(cx: &mut TestAppContext, w: f32, h: f32) {
    let label = format!("{w}x{h}");
    let env = support::env();
    // No killfeed, so the open list stands at its home over the screen's right middle.
    let layout = support::hud_stock::hudlayout_with_killfeed("");
    support::seed_stock_game_with(&env.game, &[("scripts/hudlayout.res", Some(&layout))]);
    let s = support::shell_window(cx, &env, found(&env));
    resize(cx, &s, w, h);
    let page = open_maker(cx, &s);
    support::maker_page::float_panels(cx, &s, &page);
    let place = PanelPlace {
        mode: PanelMode::DockLeft,
        collapsed: false,
        ..cx.update(|cx| page.read(cx).place_of(Panel::Options))
    };
    cx.update(|cx| page.update(cx, |p, cx| p.set_place_of(Panel::Options, place, cx)));
    show_parts(cx, &s);
    let before = bounds(cx, &s, ids::HUD_MAKER_PARTS_PANEL);
    support::click_part_row(cx, s.win, &PartKey::new(HV));
    assert_eq!(selection(cx, &page).as_deref(), Some(HV), "{label}");
    // HV goes to the screen's right, level with the list's middle: the list is capped to a
    // share of the screen, so how far down it reaches follows the window.
    let screen = cx.update(|cx| page.read(cx).editor().unwrap().screen());
    let canvas = bounds(cx, &s, ids::HUD_MAKER_CANVAS);
    let scale = f(canvas.size.height) / screen.tall() as f32;
    let tall = screen.tall() as i32;
    let middle = ((f(before.center().y - canvas.top()) / scale) as i32 - 30).clamp(0, tall - 60);
    commit(cx, &s, &page, Field::Wide, "200");
    commit(cx, &s, &page, Field::Tall, "60");
    commit(cx, &s, &page, Field::X, &(screen.wide() - 260).to_string());
    commit(cx, &s, &page, Field::Y, &middle.to_string());
    // The row click left the pointer on the list, which holds it still until the pointer leaves.
    send(
        cx,
        &s,
        pointer_at(point(before.left() - px(20.), before.top() + px(10.))),
    );
    let hv = hv_outline(cx, &s);
    let open = Bounds::new(before.origin, before.size);
    assert!(
        open.intersects(&hv),
        "{label} setup: the list must cover HV"
    );
    let after = bounds(cx, &s, ids::HUD_MAKER_PARTS_PANEL);
    assert_eq!(after.origin, before.origin, "{label}: the list moved");
    assert!(
        cx.update(|cx| page.read(cx).is_folded(Panel::Parts)),
        "{label}"
    );
    assert!(!after.intersects(&hv), "{label}: {after:?} covers {hv:?}");
}

#[gpui_kit::test]
fn at_800x600_the_parts_panel_folds_in_place_rather_than_jump(cx: &mut TestAppContext) {
    parts_panel_folds_in_place_at(cx, 800., 600.);
}

#[gpui_kit::test]
fn at_1280x800_the_parts_panel_folds_in_place_rather_than_jump(cx: &mut TestAppContext) {
    parts_panel_folds_in_place_at(cx, 1280., 800.);
}

// How much shorter the drawn screen is than the largest fit of the whole canvas area.
fn preview_loss(
    cx: &mut TestAppContext,
    s: &support::ShellWindow,
    page: &Entity<HudMakerPage>,
) -> f32 {
    let area = bounds(cx, s, ids::HUD_MAKER_CANVAS_AREA);
    let canvas = bounds(cx, s, ids::HUD_MAKER_CANVAS);
    let screen = cx.update(|cx| page.read(cx).editor().unwrap().screen());
    let full = tf2tools_app::pages::hud_maker::view::CanvasFit::new(area, screen).screen_bounds();
    f(full.size.height - canvas.size.height)
}

#[gpui_kit::test]
fn at_1920x1080_the_open_toolbar_costs_a_16_9_screen_nothing(cx: &mut TestAppContext) {
    let env = support::env();
    let s = stock_shell(cx, &env);
    resize(cx, &s, 1920., 1080.);
    let page = open_maker(cx, &s);
    support::maker_page::float_panels(cx, &s, &page);
    let loss = preview_loss(cx, &s, &page);
    assert!(loss <= 0.5, "the screen lost {loss} px");
    let (bar, canvas) = (
        bounds(cx, &s, ids::HUD_MAKER_TOOLBAR),
        bounds(cx, &s, ids::HUD_MAKER_CANVAS),
    );
    assert!(!bar.intersects(&canvas), "{bar:?} covers {canvas:?}");
}

#[gpui_kit::test]
fn at_1920x1080_the_shut_toolbar_sits_beside_a_4_3_screen(cx: &mut TestAppContext) {
    let env = support::env();
    let s = stock_shell(cx, &env);
    resize(cx, &s, 1920., 1080.);
    let page = open_maker(cx, &s);
    support::maker_page::float_panels(cx, &s, &page);
    pick_screen(cx, &s, Screen::new(1440, 1080));
    support::click_id(cx, s.win, ids::HUD_MAKER_TOOLBAR_COLLAPSE);
    let loss = preview_loss(cx, &s, &page);
    assert!(loss <= 0.5, "the screen lost {loss} px");
    let area = bounds(cx, &s, ids::HUD_MAKER_CANVAS_AREA);
    let (bar, canvas) = (
        bounds(cx, &s, ids::HUD_MAKER_TOOLBAR),
        bounds(cx, &s, ids::HUD_MAKER_CANVAS),
    );
    assert!(!bar.intersects(&canvas), "{bar:?} covers {canvas:?}");
    assert!(
        bar.right() <= area.right() && bar.bottom() <= area.bottom(),
        "{bar:?} outside {area:?}"
    );
}

#[gpui_kit::test]
fn the_open_parts_list_keeps_off_the_killfeed(cx: &mut TestAppContext) {
    for (w, h) in [(1280., 800.), (1920., 1080.)] {
        let env = support::env();
        let s = stock_shell(cx, &env);
        resize(cx, &s, w, h);
        let _page = open_maker(cx, &s);
        show_parts(cx, &s);
        support::settle(cx, s.win);
        let parts = bounds(cx, &s, ids::HUD_MAKER_PARTS_PANEL);
        for line in 0..4 {
            let kill = bounds(cx, &s, ids::hud_maker_killfeed_line(line));
            assert!(
                !parts.intersects(&kill),
                "{w}x{h}: {parts:?} over line {line} {kill:?}"
            );
        }
    }
}

#[gpui_kit::test]
fn a_long_part_name_is_cut_short_of_its_state_tag(cx: &mut TestAppContext) {
    let env = support::env();
    let name = "PlayerStatus_MedicSmallBulletResistImageWithALongerName";
    let health = support::hud_stock::with_blocks(
        support::hud_stock::HEALTH,
        &support::hud_stock::label(name, "10", "10", &[("visible", "0")]),
    );
    support::seed_stock_game_with(
        &env.game,
        &[("resource/ui/hudplayerhealth.res", Some(&health))],
    );
    let s = support::shell_window(
        cx,
        &env,
        Tf2State::Found(Tf2Install::from_path(&env.game).unwrap()),
    );
    let _page = open_maker(cx, &s);
    show_parts(cx, &s);
    support::settle(cx, s.win);
    let key = PartKey::new(format!("HudPlayerStatus/HudPlayerHealth/{name}"));
    assert_eq!(
        support::label_of(cx, s.win, ids::hud_maker_row(&key)).as_deref(),
        Some("Medic small bullet resist image with a longer name (hidden)")
    );
    // The tag stays pinned inside the panel and the name ends before it.
    let panel = bounds(cx, &s, ids::HUD_MAKER_PARTS_PANEL);
    let name_box = bounds(cx, &s, ids::hud_maker_row_name(&key));
    let tag = bounds(cx, &s, ids::hud_maker_row_state(&key));
    assert!(
        tag.left() >= panel.left() && tag.right() <= panel.right(),
        "{tag:?} outside {panel:?}"
    );
    assert!(tag.size.width > px(0.), "{tag:?} squeezed away");
    assert!(
        !name_box.intersects(&tag) && name_box.right() <= tag.left(),
        "{name_box:?} over {tag:?}"
    );
    // The cut name shows whole in its tooltip, over the element name it stands for.
    assert_eq!(
        row_tip(cx, &s, &key),
        Some(format!(
            "Medic small bullet resist image with a longer name\n{name}"
        ))
    );
}

#[gpui_kit::test]
fn a_row_click_gives_the_canvas_the_keys(cx: &mut TestAppContext) {
    let env = support::env();
    let s = stock_shell(cx, &env);
    let page = open_maker(cx, &s);
    // The name field has the keys: what is typed lands in it.
    support::click_id(cx, s.win, ids::HUD_MAKER_NAME);
    cx.update_window(s.win, |_, w, cx| w.input("x", cx))
        .unwrap();
    let name = cx.update(|cx| page.read(cx).name_input().read(cx).value().to_string());
    assert!(name.ends_with('x'), "{name}");
    show_parts(cx, &s);
    support::click_part_row(cx, s.win, &PartKey::new(HV));
    assert_eq!(selection(cx, &page).as_deref(), Some(HV));
    let x = hv_box(cx, &page).0;
    support::press(cx, s.win, "right");
    assert!(hv_box(cx, &page).0 > x, "the arrow nudged nothing");
    assert_eq!(
        cx.update(|cx| page.read(cx).name_input().read(cx).value().to_string()),
        name
    );
}

// Whether `key`'s row lies whole inside the part list's view.
fn row_in_view(cx: &mut TestAppContext, s: &support::ShellWindow, key: &PartKey) -> bool {
    support::settle(cx, s.win);
    cx.update_window(s.win, |_, w, _| {
        let list = w.try_find(ids::HUD_MAKER_PARTS).map(|l| l.bounds());
        let row = w.try_find(ids::hud_maker_row(key)).map(|r| r.bounds());
        list.zip(row)
            .is_some_and(|(l, r)| r.top() >= l.top() && r.bottom() <= l.bottom())
    })
    .unwrap()
}

fn wheel_parts_to_the_end(cx: &mut TestAppContext, s: &support::ShellWindow) {
    let down = gpui_kit::ScrollDelta::Lines(point(0., -1000.));
    cx.update_window(s.win, |_, w, cx| w.scroll(ids::HUD_MAKER_PARTS, down, cx))
        .unwrap();
    support::settle(cx, s.win);
}

#[gpui_kit::test]
fn a_pick_on_the_canvas_scrolls_its_row_into_view_once(cx: &mut TestAppContext) {
    let env = support::env();
    let s = stock_shell(cx, &env);
    let page = open_maker(cx, &s);
    support::maker_page::float_panels(cx, &s, &page);
    show_parts(cx, &s);
    let hv = PartKey::new(HV);
    wheel_parts_to_the_end(cx, &s);
    assert!(!row_in_view(cx, &s, &hv), "the list is too short to test");
    press_screen(cx, &s, (220., 947.), alt());
    assert_eq!(selection(cx, &page).as_deref(), Some(HV));
    assert!(
        row_in_view(cx, &s, &hv),
        "the picked row stayed out of view"
    );
    // The player's own scroll stands while the selection stays the same.
    wheel_parts_to_the_end(cx, &s);
    support::press(cx, s.win, "right");
    assert!(!row_in_view(cx, &s, &hv), "a nudge scrolled the list back");
}

/// Hovers `key`'s row name until its tooltip would show, and returns the tooltip's text.
fn row_tip(cx: &mut TestAppContext, s: &support::ShellWindow, key: &PartKey) -> Option<String> {
    support::reveal_part_row(cx, s.win, key);
    cx.update_window(s.win, |_, w, cx| w.hover(ids::hud_maker_row_name(key), cx))
        .unwrap();
    support::settle(cx, s.win);
    cx.executor()
        .advance_clock(std::time::Duration::from_millis(800));
    support::settle(cx, s.win);
    support::label_of(cx, s.win, ids::HUD_MAKER_ROW_TIP)
}

#[gpui_kit::test]
fn a_child_row_shows_a_plain_name_and_keeps_the_file_name_in_its_tooltip(cx: &mut TestAppContext) {
    let env = support::env();
    let s = stock_shell(cx, &env);
    let _page = open_maker(cx, &s);
    show_parts(cx, &s);
    let hv = PartKey::new(HV);
    assert_eq!(
        support::label_of(cx, s.win, ids::hud_maker_row(&hv)).as_deref(),
        Some("Health value")
    );
    assert_eq!(
        row_tip(cx, &s, &hv).as_deref(),
        Some("PlayerStatusHealthValue")
    );
}

/// The stock HUD with a label named `name` in the health panel, opened with the parts showing.
fn shell_with_label(
    cx: &mut TestAppContext,
    env: &support::Env,
    name: &str,
) -> (support::ShellWindow, PartKey) {
    let health = support::hud_stock::with_blocks(
        support::hud_stock::HEALTH,
        &support::hud_stock::label(name, "10", "10", &[]),
    );
    support::seed_stock_game_with(
        &env.game,
        &[("resource/ui/hudplayerhealth.res", Some(&health))],
    );
    let s = support::shell_window(cx, env, found(env));
    let _page = open_maker(cx, &s);
    show_parts(cx, &s);
    support::settle(cx, s.win);
    (
        s,
        PartKey::new(format!("HudPlayerStatus/HudPlayerHealth/{name}")),
    )
}

#[gpui_kit::test]
fn a_whole_name_that_is_its_file_name_has_no_tooltip(cx: &mut TestAppContext) {
    let env = support::env();
    let (s, key) = shell_with_label(cx, &env, "Medal");
    assert_eq!(
        support::label_of(cx, s.win, ids::hud_maker_row(&key)).as_deref(),
        Some("Medal")
    );
    assert_eq!(row_tip(cx, &s, &key), None);
    // A renamed row in the same list does show one, so the hover reaches the tooltips.
    assert_eq!(
        row_tip(cx, &s, &PartKey::new(HV)).as_deref(),
        Some("PlayerStatusHealthValue")
    );
}

#[gpui_kit::test]
fn a_cut_name_shows_whole_in_its_tooltip(cx: &mut TestAppContext) {
    let env = support::env();
    let name = "medalmedalmedalmedalmedalmedalmedalmedalmedalmedalmedal";
    let (s, key) = shell_with_label(cx, &env, name);
    let plain = "Medalmedalmedalmedalmedalmedalmedalmedalmedalmedalmedal";
    assert_eq!(
        support::label_of(cx, s.win, ids::hud_maker_row(&key)).as_deref(),
        Some(plain)
    );
    assert_eq!(row_tip(cx, &s, &key).as_deref(), Some(plain));
}

// The open part list stops short of the screen's bottom, where the ammo sits: with no killfeed
// it stands at home, and at 1280x720 its full height would reach the ammo.
#[gpui_kit::test]
fn the_open_parts_list_stops_above_the_ammo(cx: &mut TestAppContext) {
    for (w, h, killfeed) in [(1280., 720., false), (1280., 800., true)] {
        let env = support::env();
        let layout = match killfeed {
            true => support::hud_stock::HUDLAYOUT.to_owned(),
            false => support::hud_stock::hudlayout_with_killfeed(""),
        };
        support::seed_stock_game_with(&env.game, &[("scripts/hudlayout.res", Some(&layout))]);
        let s = support::shell_window(cx, &env, found(&env));
        resize(cx, &s, w, h);
        let page = open_maker(cx, &s);
        show_parts(cx, &s);
        assert_at_rest(cx, &s, &page, &format!("{w}x{h}"));
        let parts = bounds(cx, &s, ids::HUD_MAKER_PARTS_PANEL);
        let ammo = bounds(cx, &s, ids::hud_maker_part(&PartKey::new("HudWeaponAmmo")));
        assert!(
            !parts.intersects(&ammo),
            "{w}x{h}: the list {parts:?} covers the ammo {ammo:?}"
        );
        // The list still shows rows: it is capped, not folded.
        assert!(support::present(cx, s.win, ids::HUD_MAKER_PARTS), "{w}x{h}");
    }
}
// A dock that would shrink the screen below its floor folds to a slim rail for now, so the
// screen keeps nearly all its room; the dock stays the player's choice and returns in a larger
// window.
#[gpui_kit::test]
fn a_dock_that_would_shrink_the_screen_too_far_folds_to_a_rail_for_now(cx: &mut TestAppContext) {
    let env = support::env();
    let s = stock_shell(cx, &env);
    resize(cx, &s, 800., 600.);
    let page = open_maker(cx, &s);
    support::maker_page::float_panels(cx, &s, &page);
    let before = bounds(cx, &s, ids::HUD_MAKER_CANVAS);
    support::click_id(cx, s.win, ids::HUD_MAKER_PANEL_DOCK_RIGHT);
    assert_at_rest(cx, &s, &page, "docked right at 800x600");
    let canvas = bounds(cx, &s, ids::HUD_MAKER_CANVAS);
    assert!(
        canvas.size.height >= before.size.height * 0.8,
        "the screen shrank from {before:?} to {canvas:?}"
    );
    assert_eq!(
        cx.update(|cx| page.read(cx).place_of(Panel::Options)).mode,
        PanelMode::DockRight
    );
    assert!(cx.update(|cx| page.read(cx).is_railed(Panel::Options)));
    let rail = bounds(cx, &s, ids::HUD_MAKER_PANEL);
    let area = bounds(cx, &s, ids::HUD_MAKER_CANVAS_AREA);
    assert!(
        f(rail.right() - area.right()).abs() <= 1. && canvas.right() <= rail.left(),
        "a rail {rail:?} beside {canvas:?}"
    );
    support::wait_real(cx, "the setting", |_| {
        saved_panel(&s).is_some_and(|p| p.starts_with("right "))
    });
    resize(cx, &s, 1280., 800.);
    let panel = bounds(cx, &s, ids::HUD_MAKER_PANEL);
    let area = bounds(cx, &s, ids::HUD_MAKER_CANVAS_AREA);
    let canvas = bounds(cx, &s, ids::HUD_MAKER_CANVAS);
    assert!(support::present(cx, s.win, ids::HUD_MAKER_PANEL_BODY));
    assert!(
        f(panel.right() - area.right()).abs() <= 1. && canvas.right() <= panel.left(),
        "docked again: {panel:?} beside {canvas:?}"
    );
}

// The issue's case: with the sidebar expanded the 800x600 screen is 544x306, and the open
// list may cover at most a quarter of it, opened and with a part selected from it.
#[gpui_kit::test]
fn at_800x600_the_open_parts_list_covers_at_most_a_quarter_of_the_screen(cx: &mut TestAppContext) {
    let env = support::env();
    let s = stock_shell(cx, &env);
    resize(cx, &s, 800., 600.);
    let page = open_maker(cx, &s);
    support::maker_page::float_panels(cx, &s, &page);
    show_parts(cx, &s);
    let check = |cx: &mut TestAppContext, when: &str| {
        assert_at_rest(cx, &s, &page, when);
        let canvas = bounds(cx, &s, ids::HUD_MAKER_CANVAS);
        let parts = bounds(cx, &s, ids::HUD_MAKER_PARTS_PANEL);
        let over = parts.intersect(&canvas);
        let share = (f(over.size.width).max(0.) * f(over.size.height).max(0.))
            / (f(canvas.size.width) * f(canvas.size.height));
        assert!(
            share <= 0.25 + 0.005,
            "{when}: {parts:?} covers {share} of {canvas:?}"
        );
    };
    check(cx, "opened");
    assert!(
        support::present(cx, s.win, ids::HUD_MAKER_PARTS),
        "the list shows its rows"
    );
    support::click_part_row(cx, s.win, &PartKey::new(HV));
    assert_eq!(selection(cx, &page).as_deref(), Some(HV));
    check(cx, "with HV selected");
}

// A part selected under the open list, held open, moves the list off the part, and never onto
// the killfeed above it: the killfeed is an obstacle to every settling step, not only a cost.
fn a_selection_keeps_the_list_off_the_killfeed_at(cx: &mut TestAppContext, w: f32, h: f32) {
    {
        let label = format!("{w}x{h}");
        let env = support::env();
        let s = stock_shell(cx, &env);
        resize(cx, &s, w, h);
        let page = open_maker(cx, &s);
        support::maker_page::float_panels(cx, &s, &page);
        show_parts(cx, &s);
        let list = bounds(cx, &s, ids::HUD_MAKER_PARTS_PANEL);
        support::click_part_row(cx, s.win, &PartKey::new(HV));
        // HV goes three quarters down the list, from a list's width left of it to its middle,
        // so the list can only clear it by moving up, where the killfeed is.
        let screen = cx.update(|cx| page.read(cx).editor().unwrap().screen());
        let canvas = bounds(cx, &s, ids::HUD_MAKER_CANVAS);
        let scale = f(canvas.size.width) / screen.wide() as f32;
        let to_screen = |v: Pixels| (f(v) / scale) as i32;
        let wide = to_screen(list.size.width * 1.5);
        let left = list.left() - list.size.width - canvas.left();
        let x = to_screen(left).clamp(0, screen.wide() as i32 - wide);
        let low = list.top() + list.size.height * 0.75 - canvas.top();
        let y = to_screen(low).clamp(0, screen.tall() as i32 - 40);
        commit(cx, &s, &page, Field::Wide, &wide.to_string());
        commit(cx, &s, &page, Field::Tall, "40");
        commit(cx, &s, &page, Field::X, &x.to_string());
        commit(cx, &s, &page, Field::Y, &y.to_string());
        // The row click left the pointer on the list, which holds it until the pointer leaves.
        let away = pointer_at(point(canvas.left() + px(4.), canvas.bottom() - px(4.)));
        send(cx, &s, away.clone());
        // The list folds where it stands; expanding it holds it open, so settling moves it.
        if cx.update(|cx| page.read(cx).is_folded(Panel::Parts)) {
            support::click_id(cx, s.win, ids::HUD_MAKER_PARTS_PANEL_COLLAPSE);
            send(cx, &s, away);
        }
        assert!(
            support::present(cx, s.win, ids::HUD_MAKER_PARTS),
            "{label}: held open"
        );
        assert_at_rest(cx, &s, &page, &label);
        let parts = bounds(cx, &s, ids::HUD_MAKER_PARTS_PANEL);
        let hv = hv_outline(cx, &s);
        assert!(
            !parts.intersects(&hv),
            "{label}: {parts:?} covers HV {hv:?}"
        );
        for line in 0..4 {
            let kill = bounds(cx, &s, ids::hud_maker_killfeed_line(line));
            assert!(
                !parts.intersects(&kill),
                "{label}: {parts:?} over killfeed line {line} {kill:?}"
            );
        }
    }
}

// One window per test: an edited HUD Maker holds the next window's sidebar click.
#[gpui_kit::test]
fn at_1280x800_a_selection_never_pushes_the_open_list_onto_the_killfeed(cx: &mut TestAppContext) {
    a_selection_keeps_the_list_off_the_killfeed_at(cx, 1280., 800.);
}

#[gpui_kit::test]
fn at_800x600_a_selection_never_pushes_the_open_list_onto_the_killfeed(cx: &mut TestAppContext) {
    a_selection_keeps_the_list_off_the_killfeed_at(cx, 800., 600.);
}

#[gpui_kit::test]
fn at_800x600_the_toolbar_keeps_off_the_canvas_docked_right(cx: &mut TestAppContext) {
    toolbar_keeps_off_the_canvas_at(cx, 800., 600., Setup::DockedRight);
}

#[gpui_kit::test]
fn at_940x768_the_toolbar_keeps_off_the_canvas_docked_right(cx: &mut TestAppContext) {
    toolbar_keeps_off_the_canvas_at(cx, 940., 768., Setup::DockedRight);
}

#[gpui_kit::test]
fn at_1024x768_the_toolbar_keeps_off_the_canvas_docked_right(cx: &mut TestAppContext) {
    toolbar_keeps_off_the_canvas_at(cx, 1024., 768., Setup::DockedRight);
}

// At 940x768 docked right the screen keeps 388 px, too narrow for the toolbar in two rows: the
// name and save row wait at the top of the More menu, and come back in a wider window.
#[gpui_kit::test]
fn a_narrow_toolbar_keeps_the_save_row_in_the_more_menu(cx: &mut TestAppContext) {
    let env = support::env();
    let s = stock_shell(cx, &env);
    resize(cx, &s, 940., 768.);
    let page = open_maker(cx, &s);
    support::maker_page::float_panels(cx, &s, &page);
    support::click_id(cx, s.win, ids::HUD_MAKER_PANEL_DOCK_RIGHT);
    for id in [
        ids::HUD_MAKER_NAME,
        ids::HUD_MAKER_SAVE,
        ids::HUD_MAKER_INSTALL,
    ] {
        assert!(
            !support::present(cx, s.win, id),
            "{id} still in the toolbar"
        );
    }
    support::click_id(cx, s.win, ids::HUD_MAKER_MORE);
    let bar = bounds(cx, &s, ids::HUD_MAKER_TOOLBAR);
    for id in [
        ids::HUD_MAKER_NAME,
        ids::HUD_MAKER_SAVE,
        ids::HUD_MAKER_INSTALL,
    ] {
        let b = bounds(cx, &s, id);
        assert!(
            !bar.contains(&b.center()),
            "{id} {b:?} in the toolbar {bar:?}"
        );
    }
    support::press(cx, s.win, "escape");
    resize(cx, &s, 1280., 800.);
    let bar = bounds(cx, &s, ids::HUD_MAKER_TOOLBAR);
    for id in [
        ids::HUD_MAKER_NAME,
        ids::HUD_MAKER_SAVE,
        ids::HUD_MAKER_INSTALL,
    ] {
        let b = bounds(cx, &s, id);
        assert!(bar.contains(&b.center()), "{id} {b:?} not back in {bar:?}");
    }
}

/// The stock ammo panel's background and reserve count, trimmed to their 640x480 geometry.
const STOCK_RESERVE: &str = r#""Resource/UI/HudAmmoWeapons.res"
{
  HudWeaponAmmoBG { ControlName CTFImagePanel xpos 4 ypos 0 zpos 1 wide 90 tall 45 image ../hud/ammo_blue_bg }
  AmmoInReserve { ControlName CExLabel font HudFontMediumSmall xpos 59 ypos 8 zpos 7 wide 40 tall 27 textAlignment south-west labelText %AmmoInReserve% }
}"#;

#[gpui_kit::test]
fn the_reserve_ammo_outlines_end_where_its_panel_cuts_it_off(cx: &mut TestAppContext) {
    let env = support::env();
    support::seed_stock_game_with(
        &env.game,
        &[("resource/ui/hudammoweapons.res", Some(STOCK_RESERVE))],
    );
    let s = support::shell_window(cx, &env, found(&env));
    let page = open_maker(cx, &s);
    support::maker_page::float_panels(cx, &s, &page);
    let reserve = PartKey::new("HudWeaponAmmo/AmmoInReserve");
    let bg = bounds(
        cx,
        &s,
        ids::hud_maker_part(&PartKey::new("HudWeaponAmmo/HudWeaponAmmoBG")),
    );
    let label = bounds(cx, &s, ids::hud_maker_part(&reserve));
    assert!(
        f(label.right() - bg.right()) > 1.,
        "the file's box runs past the panel"
    );
    click_row(cx, &s, &reserve);
    let sel = bounds(cx, &s, ids::HUD_MAKER_SELECTION);
    assert!(f(sel.right() - bg.right()).abs() <= 1., "{sel:?} vs {bg:?}");
    assert!(
        f(sel.origin.x - label.origin.x).abs() <= 1.,
        "{sel:?} vs {label:?}"
    );
    show_parts(cx, &s);
    hover(cx, &s, ids::hud_maker_row(&reserve));
    let outline = bounds(cx, &s, ids::HUD_MAKER_HOVER);
    assert!(
        f(outline.right() - bg.right()).abs() <= 1.,
        "{outline:?} vs {bg:?}"
    );
}

#[gpui_kit::test]
fn the_reserve_ammo_paints_only_inside_its_panel(cx: &mut TestAppContext) {
    let env = support::env();
    support::seed_stock_game_with(
        &env.game,
        &[("resource/ui/hudammoweapons.res", Some(STOCK_RESERVE))],
    );
    let s = support::shell_window(cx, &env, found(&env));
    let _page = open_maker(cx, &s);
    let reserve = PartKey::new("HudWeaponAmmo/AmmoInReserve");
    let bg = bounds(
        cx,
        &s,
        ids::hud_maker_part(&PartKey::new("HudWeaponAmmo/HudWeaponAmmoBG")),
    );
    let label = bounds(cx, &s, ids::hud_maker_part(&reserve));
    assert!(
        f(label.right() - bg.right()) > 1.,
        "the file's box runs past"
    );
    // The label paints inside a clip that ends where its panel does, so nothing shows past it.
    let clip = bounds(cx, &s, ids::hud_maker_part_clip(&reserve));
    assert!(
        f(clip.right() - bg.right()).abs() <= 1.,
        "{clip:?} vs {bg:?}"
    );
    assert!(f(clip.origin.x - label.origin.x).abs() <= 1., "{clip:?}");
    let inside = cx
        .update_window(s.win, |_, w, _| {
            let text = w.try_find(ids::hud_maker_text(&reserve)).unwrap();
            text.path().contains(&ids::hud_maker_part_clip(&reserve))
        })
        .unwrap();
    assert!(inside, "the label's text paints inside its clip");
}

#[gpui_kit::test]
fn a_part_its_panel_cuts_off_whole_paints_nothing(cx: &mut TestAppContext) {
    // Above the panel, so on the screen but wholly outside its parent.
    let off = r#""Resource/UI/HudAmmoWeapons.res" { Far { ControlName CExLabel xpos 10 ypos -30 wide 20 tall 10 labelText 1 } }"#;
    let env = support::env();
    support::seed_stock_game_with(&env.game, &[("resource/ui/hudammoweapons.res", Some(off))]);
    let s = support::shell_window(cx, &env, found(&env));
    let _page = open_maker(cx, &s);
    let far = PartKey::new("HudWeaponAmmo/Far");
    let panel = PartKey::new("HudWeaponAmmo");
    assert!(support::present(cx, s.win, ids::hud_maker_part(&panel)));
    // The part keeps its box, and so any pin dot, but its content paints nothing.
    assert!(support::present(cx, s.win, ids::hud_maker_part(&far)));
    assert!(!support::present(cx, s.win, ids::hud_maker_part_clip(&far)));
}

#[gpui_kit::test]
fn a_selected_panel_cut_off_whole_ghosts_with_its_children(cx: &mut TestAppContext) {
    // A content-free panel above its parent, so cut off whole, holding one label.
    let off = r#""Resource/UI/HudAmmoWeapons.res" { Box { ControlName EditablePanel xpos 10 ypos -60 wide 40 tall 30
  Kid { ControlName CExLabel xpos 2 ypos 2 wide 20 tall 10 labelText 1 } } }"#;
    let env = support::env();
    support::seed_stock_game_with(&env.game, &[("resource/ui/hudammoweapons.res", Some(off))]);
    let s = support::shell_window(cx, &env, found(&env));
    let _page = open_maker(cx, &s);
    let panel = PartKey::new("HudWeaponAmmo/Box");
    let kid = PartKey::new("HudWeaponAmmo/Box/Kid");
    assert!(!support::present(cx, s.win, ids::hud_maker_part_clip(&kid)));
    click_row(cx, &s, &panel);
    // Selected, the panel ghosts with everything in it, each child inside the panel's box.
    let kid_clip = bounds(cx, &s, ids::hud_maker_part_clip(&kid));
    let panel_box = bounds(cx, &s, ids::hud_maker_part(&panel));
    assert!(
        f(kid_clip.origin.x - panel_box.origin.x) > 1. && kid_clip.size.width > px(1.),
        "{kid_clip:?} vs {panel_box:?}"
    );
    assert!(support::present(cx, s.win, ids::hud_maker_text(&kid)));
    // Selecting something else puts the subtree away again.
    select_hv(cx, &s);
    assert!(!support::present(cx, s.win, ids::hud_maker_part_clip(&kid)));
}

#[gpui_kit::test]
fn a_selected_part_moved_out_of_its_panel_stays_ghosted_and_drags_back(cx: &mut TestAppContext) {
    let env = support::env();
    let s = stock_shell(cx, &env);
    let page = open_maker(cx, &s);
    select_hv(cx, &s);
    let (x, y, _, _) = hv_box(cx, &page);
    let hv = PartKey::new(HV);
    commit(cx, &s, &page, Field::X, "600");
    // Cut off whole, the selection still paints faintly in its whole box.
    let clip = bounds(cx, &s, ids::hud_maker_part_clip(&hv));
    let outline = hv_outline(cx, &s);
    assert!(
        f(clip.origin.x - outline.origin.x).abs() <= 1.,
        "{clip:?} vs {outline:?}"
    );
    assert!(
        f(clip.size.width - outline.size.width).abs() <= 1.,
        "{clip:?} vs {outline:?}"
    );
    // A press inside its outline keeps it and drags it back into its panel.
    let k = scale(cx, &s);
    drag_window(cx, &s, outline.center(), ((x - 600) as f32 * k, 0.));
    assert_eq!(selection(cx, &page).as_deref(), Some(HV));
    let back = hv_box(cx, &page);
    assert!(
        (back.0 - x).abs() <= 1 && back.1 == y,
        "{back:?} vs ({x}, {y})"
    );
    let shown = cx.update(|cx| page.read(cx).editor().unwrap().layout().shown(&hv));
    assert!(shown.is_some(), "back inside its panel");
    assert!(support::present(cx, s.win, ids::hud_maker_part_clip(&hv)));
}

#[gpui_kit::test]
fn a_side_its_panel_cuts_off_has_no_handles(cx: &mut TestAppContext) {
    let env = support::env();
    support::seed_stock_game_with(
        &env.game,
        &[("resource/ui/hudammoweapons.res", Some(STOCK_RESERVE))],
    );
    let s = support::shell_window(cx, &env, found(&env));
    let page = open_maker(cx, &s);
    support::maker_page::float_panels(cx, &s, &page);
    let reserve = PartKey::new("HudWeaponAmmo/AmmoInReserve");
    click_row(cx, &s, &reserve);
    let cut = [Handle::Right, Handle::TopRight, Handle::BottomRight];
    for handle in cut {
        assert!(
            !support::present(cx, s.win, ids::hud_maker_handle(handle)),
            "{handle:?}"
        );
    }
    for handle in [Handle::Left, Handle::TopLeft, Handle::BottomLeft] {
        assert!(
            support::present(cx, s.win, ids::hud_maker_handle(handle)),
            "{handle:?}"
        );
    }
    let reserve_box = |cx: &mut TestAppContext| {
        cx.update(|cx| {
            let e = page.read(cx).editor().unwrap();
            let b = e.layout().get(&reserve).unwrap();
            (b.x(), b.wide())
        })
    };
    let (x, wide) = reserve_box(cx);
    let k = scale(cx, &s);
    let from = bounds(cx, &s, ids::hud_maker_handle(Handle::Left)).center();
    drag_window(cx, &s, from, (-18. * k, 0.));
    assert_eq!(
        reserve_box(cx),
        (x - 18, wide + 18),
        "the left side still resizes"
    );
}

#[gpui_kit::test]
fn a_docked_panel_leaves_the_screen_the_same_gutter_on_both_sides(cx: &mut TestAppContext) {
    let env = support::env();
    let s = stock_shell(cx, &env);
    resize(cx, &s, 1280., 800.);
    let page = open_maker(cx, &s);
    support::maker_page::float_panels(cx, &s, &page);
    support::click_id(cx, s.win, ids::HUD_MAKER_PANEL_DOCK_LEFT);
    let panel = bounds(cx, &s, ids::HUD_MAKER_PANEL);
    let area = bounds(cx, &s, ids::HUD_MAKER_CANVAS_AREA);
    let canvas = bounds(cx, &s, ids::HUD_MAKER_CANVAS);
    let (near, far) = (canvas.left() - panel.right(), area.right() - canvas.right());
    assert!(
        f(near) > 0. && f(near - far).abs() <= 1.,
        "gutters {near:?} and {far:?}: {canvas:?} in {area:?} beside {panel:?}"
    );
}

const STATUS: &str = "HudPlayerStatus";

fn alt() -> gpui_kit::Modifiers {
    gpui_kit::Modifiers {
        alt: true,
        ..Default::default()
    }
}

/// The screen pixel at the middle of `key`'s box.
fn middle_of(cx: &mut TestAppContext, page: &Entity<HudMakerPage>, key: &str) -> (f32, f32) {
    cx.update(|cx| {
        let editor = page.read(cx).editor().unwrap();
        let b = editor.layout().get(&PartKey::new(key)).unwrap();
        ((b.x() + b.wide() / 2) as f32, (b.y() + b.tall() / 2) as f32)
    })
}

fn x_of(cx: &mut TestAppContext, page: &Entity<HudMakerPage>, key: &str) -> i32 {
    cx.update(|cx| {
        let editor = page.read(cx).editor().unwrap();
        editor.layout().get(&PartKey::new(key)).unwrap().x()
    })
}

#[gpui_kit::test]
fn a_click_selects_the_parts_group_and_a_double_click_goes_inside(cx: &mut TestAppContext) {
    let env = support::env();
    let s = stock_shell(cx, &env);
    let page = open_maker(cx, &s);
    let none = gpui_kit::Modifiers::default();
    let at = middle_of(cx, &page, HV);
    // The outline shows what a click would take: the whole health group.
    let canvas = bounds(cx, &s, ids::HUD_MAKER_CANVAS);
    let k = scale(cx, &s);
    move_to(cx, &s, canvas.origin + point(px(at.0 * k), px(at.1 * k)));
    assert_eq!(hovered(cx, &page).as_deref(), Some(HH));
    press_screen_n(cx, &s, at, none, 1);
    assert_eq!(selection(cx, &page).as_deref(), Some(HH), "the group first");
    press_screen_n(cx, &s, at, none, 2);
    assert_eq!(selection(cx, &page).as_deref(), Some(HV), "then inside it");
    // Inside the group, a click on another of its parts takes that part.
    let (ix, iy, _, _) = cx.update(|cx| {
        let b = page.read(cx).editor().unwrap().layout();
        let b = b.get(&PartKey::new(IMAGE)).unwrap();
        (b.x(), b.y(), b.wide(), b.tall())
    });
    press_screen(cx, &s, ((ix + 3) as f32, (iy + 3) as f32), none);
    assert_eq!(selection(cx, &page).as_deref(), Some(IMAGE), "a sibling");
}

#[gpui_kit::test]
fn alt_click_goes_straight_to_the_part(cx: &mut TestAppContext) {
    let env = support::env();
    let s = stock_shell(cx, &env);
    let page = open_maker(cx, &s);
    let alt = gpui_kit::Modifiers {
        alt: true,
        ..Default::default()
    };
    let at = middle_of(cx, &page, HV);
    press_screen(cx, &s, at, alt);
    assert_eq!(selection(cx, &page).as_deref(), Some(HV));
}

#[gpui_kit::test]
fn dragging_a_group_moves_all_its_parts_as_one_undo_step(cx: &mut TestAppContext) {
    let env = support::env();
    let s = stock_shell(cx, &env);
    let page = open_maker(cx, &s);
    let before = [HH, HV, IMAGE].map(|key| x_of(cx, &page, key));
    let k = scale(cx, &s);
    let from = bounds(cx, &s, ids::hud_maker_part(&PartKey::new(HV))).center();
    drag_window(cx, &s, from, (45. * k, 0.));
    assert_eq!(selection(cx, &page).as_deref(), Some(HH));
    let after = [HH, HV, IMAGE].map(|key| x_of(cx, &page, key));
    assert_eq!(after, before.map(|x| x + 45), "every part moves together");
    support::press(cx, s.win, "ctrl-z");
    assert_eq!([HH, HV, IMAGE].map(|key| x_of(cx, &page, key)), before);
}

#[gpui_kit::test]
fn a_parts_row_still_selects_exactly_its_part(cx: &mut TestAppContext) {
    let env = support::env();
    let s = stock_shell(cx, &env);
    let page = open_maker(cx, &s);
    select_hv(cx, &s);
    assert_eq!(selection(cx, &page).as_deref(), Some(HV));
    let at = middle_of(cx, &page, HV);
    press_screen(cx, &s, at, gpui_kit::Modifiers::default());
    assert_eq!(
        selection(cx, &page).as_deref(),
        Some(HV),
        "a click on the selected part keeps it"
    );
}

#[gpui_kit::test]
fn the_panel_title_shows_the_way_down_and_each_step_selects(cx: &mut TestAppContext) {
    let env = support::env();
    let s = stock_shell(cx, &env);
    let page = open_maker(cx, &s);
    select_hv(cx, &s);
    let crumb = |key: &str| ids::hud_maker_crumb(&PartKey::new(key));
    assert_eq!(
        support::label_of(cx, s.win, crumb(STATUS)).as_deref(),
        Some("Player status")
    );
    assert_eq!(
        support::label_of(cx, s.win, crumb(HH)).as_deref(),
        Some("Health")
    );
    assert!(
        !support::present(cx, s.win, crumb(HV)),
        "the part is the title"
    );
    support::click_id(cx, s.win, crumb(HH));
    assert_eq!(selection(cx, &page).as_deref(), Some(HH));
    assert!(!support::present(cx, s.win, crumb(HH)));
    support::click_id(cx, s.win, crumb(STATUS));
    assert_eq!(selection(cx, &page).as_deref(), Some(STATUS));
}
