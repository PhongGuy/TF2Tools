//! The HUD Maker's Stage 2 side panel and canvas, headless.

mod support;

use gpui_kit::component::color_picker::ColorPickerEvent;
use gpui_kit::component::input::InputEvent;
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{AppContext as _, Entity, ScrollDelta, TestAppContext, point};
use std::sync::Arc;
use tf2tools_app::ids;
use tf2tools_app::pages::hud_maker::view::Panel;
use tf2tools_app::pages::hud_maker::{Field, HudMakerPage};
use tf2tools_app::picker::{PickKind, ScriptedPicker};
use tf2tools_app::services::Tf2State;
use tf2tools_app::shell::route::Page;
use tf2tools_core::{PartKey, Tf2Install};

const HV: &str = "HudPlayerStatus/HudPlayerHealth/PlayerStatusHealthValue";
const DAMAGE: &str = "CDamageAccountPanel";
const KF: &str = "HudDeathNotice";

const FONTS: &str = r#""Scheme" { "Colors" { "TanDark" "117 107 94 255" "TanLight" "235 226 202 255" }
  "Fonts" { "Default" { "1" { "name" "Verdana" "tall" "12" } }
            "HudFontSmall" { "1" { "name" "TF2 Build" "tall" "14" "dropshadow" "1" } }
            "HudFontMedium" { "1" { "name" "TF2 Build" "tall" "16" } }
            "TF2Tools_9" { "1" { "name" "Verdana" "tall" "9" } } } }"#;

/// The stock hudlayout with a bare killfeed block, so its colours are the engine defaults.
fn hudlayout_with_killfeed() -> String {
    support::hud_stock::hudlayout_with_killfeed(
        "  HudDeathNotice { fieldName HudDeathNotice xpos r640 ypos 18 wide 628 tall 468 }\n",
    )
}

fn shell(
    cx: &mut TestAppContext,
    env: &support::Env,
    picker: Arc<ScriptedPicker>,
) -> support::ShellWindow {
    shell_with_scheme(cx, env, picker, FONTS)
}

/// As `shell`, with `scheme` as the game's clientscheme.
fn shell_with_scheme(
    cx: &mut TestAppContext,
    env: &support::Env,
    picker: Arc<ScriptedPicker>,
    scheme: &str,
) -> support::ShellWindow {
    let layout = hudlayout_with_killfeed();
    // The health value keeps no `font` line, so its font is the row's default.
    let health = support::hud_stock::HEALTH.replace("    font HudClassHealth\n", "");
    support::seed_stock_game_with(
        &env.game,
        &[
            ("resource/clientscheme.res", Some(scheme)),
            ("scripts/hudlayout.res", Some(layout.as_str())),
            ("resource/ui/hudplayerhealth.res", Some(health.as_str())),
        ],
    );
    let state = Tf2State::Found(Tf2Install::from_path(&env.game).unwrap());
    support::shell_window_with(cx, env, state, picker, support::no_steam())
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

fn opened(cx: &mut TestAppContext) -> (support::Env, support::ShellWindow, Entity<HudMakerPage>) {
    let env = support::env();
    let s = shell(cx, &env, Arc::new(ScriptedPicker::empty()));
    let page = open_maker(cx, &s);
    (env, s, page)
}

fn click_row(cx: &mut TestAppContext, s: &support::ShellWindow, key: &str) {
    if !support::present(cx, s.win, ids::HUD_MAKER_PARTS) {
        support::click_id(cx, s.win, ids::HUD_MAKER_PARTS_PANEL_COLLAPSE);
    }
    // The part list draws only the rows in view, so it scrolls until the row is wholly shown.
    support::click_part_row(cx, s.win, &PartKey::new(key));
    // A panel folded off a large selection expands first, as a player would; the list then
    // closes and the side panel shows its body.
    if folded(cx, s, Panel::Parts) {
        support::click_id(cx, s.win, ids::HUD_MAKER_PARTS_PANEL_COLLAPSE);
    }
    if support::present(cx, s.win, ids::HUD_MAKER_PARTS) {
        support::click_id(cx, s.win, ids::HUD_MAKER_PARTS_PANEL_COLLAPSE);
    }
    if folded(cx, s, Panel::Options) {
        support::click_id(cx, s.win, ids::HUD_MAKER_PANEL_COLLAPSE);
    }
}

fn folded(cx: &mut TestAppContext, s: &support::ShellWindow, panel: Panel) -> bool {
    cx.update(|cx| {
        let page = s.shell.read(cx).hud_maker_page().expect("a HUD Maker page");
        page.read(cx).is_folded(panel)
    })
}

/// Scrolls the side panel until all of `id` lies inside its body, up when it sits above.
fn scroll_panel_to(cx: &mut TestAppContext, s: &support::ShellWindow, id: gpui_kit::ElementId) {
    for _ in 0..80 {
        let body = support::bounds_of(cx, s.win, ids::HUD_MAKER_PANEL_BODY).unwrap();
        let b = support::bounds_of(cx, s.win, id.clone()).unwrap();
        if b.top() >= body.top() && b.bottom() <= body.bottom() {
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

/// Clicks `id` in the side panel, scrolling the panel down until all of it is in view.
fn click_in_panel(cx: &mut TestAppContext, s: &support::ShellWindow, id: gpui_kit::ElementId) {
    scroll_panel_to(cx, s, id.clone());
    support::click_id(cx, s.win, id);
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

fn value(
    cx: &mut TestAppContext,
    page: &Entity<HudMakerPage>,
    part: &str,
    key: &str,
) -> Option<String> {
    cx.update(|cx| {
        let e = page.read(cx).editor()?;
        e.model()
            .part(&PartKey::new(part))?
            .value(key)
            .map(|l| l.text().to_owned())
    })
}

#[gpui_kit::test]
fn the_text_field_keeps_a_hotkey_marker_the_canvas_hides(cx: &mut TestAppContext) {
    let (_env, s, page) = opened(cx);
    click_row(cx, &s, HV);
    commit(cx, &s, &page, Field::Text, "&Salt && Pepper");
    let typed = cx.update(|cx| {
        let p = page.read(cx);
        p.field(Field::Text).read(cx).value().to_string()
    });
    assert_eq!(typed, "&Salt && Pepper");
    assert_eq!(
        value(cx, &page, HV, "labelText").as_deref(),
        Some("&Salt && Pepper")
    );
    assert_eq!(
        support::label_of(cx, s.win, ids::hud_maker_part(&PartKey::new(HV))).as_deref(),
        Some("Salt & Pepper")
    );
}

#[gpui_kit::test]
fn a_label_offers_text_alignment_and_switches(cx: &mut TestAppContext) {
    let (_env, s, page) = opened(cx);
    click_row(cx, &s, HV);
    assert!(support::present(cx, s.win, ids::HUD_MAKER_TEXT));
    let text = cx.update(|cx| {
        page.read(cx)
            .field(Field::Text)
            .read(cx)
            .value()
            .to_string()
    });
    assert_eq!(text, "%Health%");
    assert_eq!(
        support::selected_of(cx, s.win, ids::hud_maker_align("west")),
        Some(true)
    );
    click_in_panel(cx, &s, ids::hud_maker_align("north-east"));
    assert_eq!(
        value(cx, &page, HV, "textAlignment").as_deref(),
        Some("north-east")
    );
    assert_eq!(
        support::selected_of(cx, s.win, ids::hud_maker_align("north-east")),
        Some(true)
    );
    click_in_panel(cx, &s, ids::hud_maker_toggle("allcaps"));
    assert_eq!(value(cx, &page, HV, "allcaps").as_deref(), Some("1"));
    commit(cx, &s, &page, Field::Text, "HP %Health%");
    assert_eq!(
        value(cx, &page, HV, "labelText").as_deref(),
        Some("HP %Health%")
    );
}

#[gpui_kit::test]
fn typed_text_commits_to_the_part_it_was_typed_for(cx: &mut TestAppContext) {
    let (_env, s, page) = opened(cx);
    click_row(cx, &s, HV);
    let input = cx.update(|cx| page.read(cx).field(Field::Text).clone());
    cx.update_window(s.win, |_, window, cx| {
        input.update(cx, |st, cx| st.set_value("Typed", window, cx))
    })
    .unwrap();
    click_row(cx, &s, "HudWeaponAmmo/AmmoInClip");
    assert_eq!(value(cx, &page, HV, "labelText").as_deref(), Some("Typed"));
    assert_eq!(
        value(cx, &page, "HudWeaponAmmo/AmmoInClip", "labelText").as_deref(),
        Some("%Ammo%")
    );
}

#[gpui_kit::test]
fn damage_numbers_take_a_decimal_time_on_screen(cx: &mut TestAppContext) {
    let (_env, s, page) = opened(cx);
    click_row(cx, &s, DAMAGE);
    assert!(support::present(
        cx,
        s.win,
        ids::hud_maker_number("delta_lifetime")
    ));
    commit(cx, &s, &page, Field::Lifetime, "1.5");
    assert_eq!(
        value(cx, &page, DAMAGE, "delta_lifetime").as_deref(),
        Some("1.5")
    );
    commit(cx, &s, &page, Field::Lifetime, "1.555");
    assert_eq!(
        value(cx, &page, DAMAGE, "delta_lifetime").as_deref(),
        Some("1.5"),
        "a bad value commits nothing"
    );
}

#[gpui_kit::test]
fn the_killfeed_offers_its_line_numbers(cx: &mut TestAppContext) {
    let (_env, s, page) = opened(cx);
    click_row(cx, &s, KF);
    commit(cx, &s, &page, Field::LineHeight, "20");
    assert_eq!(value(cx, &page, KF, "LineHeight").as_deref(), Some("20"));
    commit(cx, &s, &page, Field::LineSpacing, "6");
    assert_eq!(value(cx, &page, KF, "LineSpacing").as_deref(), Some("6"));
}

fn pick(
    cx: &mut TestAppContext,
    s: &support::ShellWindow,
    page: &Entity<HudMakerPage>,
    rgb: [u8; 3],
) {
    let picker = cx.update(|cx| page.read(cx).colour_state().clone());
    cx.update_window(s.win, |_, _, cx| {
        picker.update(cx, |_, cx| {
            cx.emit(ColorPickerEvent::Change(Some(
                tf2tools_app::pages::crosshair_maker::model::hsla_of(rgb),
            )))
        })
    })
    .unwrap();
    support::settle(cx, s.win);
}

#[gpui_kit::test]
fn a_named_colour_goes_to_the_active_row(cx: &mut TestAppContext) {
    let (_env, s, page) = opened(cx);
    click_row(cx, &s, KF);
    click_in_panel(cx, &s, ids::hud_maker_colour_row("TeamRed"));
    click_in_panel(cx, &s, ids::HUD_MAKER_COLOUR_NAME.into());
    click_in_panel(cx, &s, ids::hud_maker_colour_name("TanLight"));
    assert_eq!(value(cx, &page, KF, "TeamRed").as_deref(), Some("TanLight"));
    assert_eq!(value(cx, &page, KF, "TeamBlue"), None);
}

#[gpui_kit::test]
fn a_picker_drag_on_another_colour_row_is_one_undo_step(cx: &mut TestAppContext) {
    let (_env, s, page) = opened(cx);
    click_row(cx, &s, KF);
    click_in_panel(cx, &s, ids::hud_maker_colour_row("IconColor"));
    for rgb in [[10, 0, 0], [20, 0, 0], [30, 0, 0]] {
        pick(cx, &s, &page, rgb);
    }
    assert_eq!(
        value(cx, &page, KF, "IconColor").as_deref(),
        Some("30 0 0 255")
    );
    support::click_id(cx, s.win, ids::HUD_MAKER_UNDO);
    assert_eq!(value(cx, &page, KF, "IconColor"), None);
}

#[gpui_kit::test]
fn the_font_row_picks_a_font_and_hides_clones(cx: &mut TestAppContext) {
    let (_env, s, page) = opened(cx);
    click_row(cx, &s, DAMAGE);
    assert_eq!(
        support::label_of(cx, s.win, ids::hud_maker_font("delta_item_font")).as_deref(),
        Some("HudFontMedium")
    );
    click_in_panel(cx, &s, ids::hud_maker_font_list("delta_item_font"));
    assert!(support::present(
        cx,
        s.win,
        ids::hud_maker_font_name("HudFontSmall")
    ));
    assert!(!support::present(
        cx,
        s.win,
        ids::hud_maker_font_name("TF2Tools_9")
    ));
    support::click_id(cx, s.win, ids::hud_maker_font_name("HudFontSmall"));
    assert_eq!(
        value(cx, &page, DAMAGE, "delta_item_font").as_deref(),
        Some("HudFontSmall")
    );
}

#[gpui_kit::test]
fn size_outline_and_shadow_give_the_part_its_own_font(cx: &mut TestAppContext) {
    let (_env, s, page) = opened(cx);
    click_row(cx, &s, HV);
    assert_eq!(
        support::selected_of(cx, s.win, ids::hud_maker_font_shadow("font")),
        Some(false)
    );
    click_in_panel(cx, &s, ids::hud_maker_font_outline("font"));
    let font = value(cx, &page, HV, "font").unwrap();
    assert_eq!(font, "TF2Tools_10", "numbered above the HUD's TF2Tools_9");
    click_in_panel(cx, &s, ids::hud_maker_font_size("font"));
    commit(cx, &s, &page, Field::FontSize, "24");
    let tall = cx.update(|cx| {
        let e = page.read(cx).editor().unwrap();
        e.model()
            .scheme()
            .font(&font, e.screen().tall())
            .map(|f| (f.tall(), f.outline()))
    });
    assert_eq!(tall, Some((24, true)));
    assert_eq!(
        value(cx, &page, HV, "font").as_deref(),
        Some("TF2Tools_10"),
        "edited in place"
    );
}

/// True when `field`'s input shows its whole text, unscrolled.
fn shows_whole(cx: &mut TestAppContext, page: &Entity<HudMakerPage>, field: Field) -> bool {
    cx.update(|cx| page.read(cx).field(field).read(cx).scroll_offset().x == gpui_kit::px(0.))
}

#[gpui_kit::test]
fn a_three_digit_size_shows_whole_in_its_box(cx: &mut TestAppContext) {
    let (_env, s, page) = opened(cx);
    click_row(cx, &s, HV);
    click_in_panel(cx, &s, ids::hud_maker_font_size("font"));
    // Typed as a player types, so the caret ends after the last digit.
    let input = cx.update(|cx| page.read(cx).field(Field::FontSize).clone());
    cx.update_window(s.win, |_, window, cx| {
        input.update(cx, |st, cx| st.focus(window, cx))
    })
    .unwrap();
    support::press(cx, s.win, "ctrl-a");
    cx.update_window(s.win, |_, w, cx| w.input("140", cx))
        .unwrap();
    support::press(cx, s.win, "enter");
    assert!(shows_whole(cx, &page, Field::FontSize), "after Enter");
    cx.update_window(s.win, |_, window, cx| window.blur(cx))
        .unwrap();
    support::settle(cx, s.win);
    assert!(
        shows_whole(cx, &page, Field::FontSize),
        "after focus leaves"
    );
    // The row still fits the panel: nothing reaches past the font name's line.
    let edge = support::bounds_of(cx, s.win, ids::hud_maker_font("font"))
        .unwrap()
        .right();
    for id in [
        ids::hud_maker_font_size("font"),
        ids::hud_maker_font_outline("font"),
        ids::hud_maker_font_shadow("font"),
    ] {
        let b = support::bounds_of(cx, s.win, id.clone()).unwrap();
        assert!(b.right() <= edge, "{id:?} clips at {b:?}");
    }
}

#[gpui_kit::test]
fn a_size_typed_for_one_font_row_commits_there(cx: &mut TestAppContext) {
    let (_env, s, page) = opened(cx);
    click_row(cx, &s, DAMAGE);
    click_in_panel(cx, &s, ids::hud_maker_font_size("delta_item_font"));
    let input = cx.update(|cx| page.read(cx).field(Field::FontSize).clone());
    cx.update_window(s.win, |_, window, cx| {
        input.update(cx, |st, cx| st.set_value("30", window, cx))
    })
    .unwrap();
    click_in_panel(cx, &s, ids::hud_maker_font_size("delta_item_font_big"));
    let small = value(cx, &page, DAMAGE, "delta_item_font").unwrap();
    assert!(
        small.starts_with("TF2Tools_"),
        "the first row got its own font"
    );
    assert_eq!(
        value(cx, &page, DAMAGE, "delta_item_font_big"),
        None,
        "the second row is untouched"
    );
}

#[gpui_kit::test]
fn a_family_comes_from_the_huds_fonts(cx: &mut TestAppContext) {
    let (_env, s, page) = opened(cx);
    click_row(cx, &s, HV);
    click_in_panel(cx, &s, ids::hud_maker_font_family_list("font"));
    support::click_id(cx, s.win, ids::hud_maker_font_family("TF2 Build"));
    let font = value(cx, &page, HV, "font").unwrap();
    let family = cx.update(|cx| {
        let e = page.read(cx).editor().unwrap();
        e.model()
            .scheme()
            .font(&font, 1080)
            .map(|f| f.family().to_owned())
    });
    assert_eq!(family.as_deref(), Some("TF2 Build"));
}

#[gpui_kit::test]
fn the_font_name_gets_the_whole_row(cx: &mut TestAppContext) {
    let (_env, s, page) = opened(cx);
    click_row(cx, &s, HV);
    click_in_panel(cx, &s, ids::hud_maker_font_family_list("font"));
    support::click_id(cx, s.win, ids::hud_maker_font_family("TF2 Build"));
    let font = value(cx, &page, HV, "font").unwrap();
    assert_eq!(
        support::label_of(cx, s.win, ids::hud_maker_font("font")).as_deref(),
        Some(font.as_str())
    );
    scroll_panel_to(cx, &s, ids::hud_maker_font_add("font"));
    let name = support::bounds_of(cx, s.win, ids::hud_maker_font("font")).unwrap();
    let add = support::bounds_of(cx, s.win, ids::hud_maker_font_add("font")).unwrap();
    assert!(
        name.size.width >= add.size.width - gpui_kit::px(1.),
        "{name:?} is narrower than the row {add:?}"
    );
}

#[gpui_kit::test]
fn an_opened_font_list_scrolls_into_view(cx: &mut TestAppContext) {
    let (_env, s, _page) = opened(cx);
    cx.simulate_window_resize(
        s.win,
        gpui_kit::size(gpui_kit::px(1280.), gpui_kit::px(800.)),
    );
    support::settle(cx, s.win);
    click_row(cx, &s, HV);
    for list in [
        ids::hud_maker_font_list("font"),
        ids::hud_maker_font_family_list("font"),
    ] {
        // The button sits at the panel's bottom edge, so the list opens below the fold.
        scroll_panel_to(cx, &s, list.clone());
        support::click_id(cx, s.win, list.clone());
        let body = support::bounds_of(cx, s.win, ids::HUD_MAKER_PANEL_BODY).unwrap();
        let choices = support::bounds_of(cx, s.win, ids::hud_maker_font_choices("font")).unwrap();
        assert!(
            choices.top() >= body.top() && choices.bottom() <= body.bottom() + gpui_kit::px(1.),
            "{list:?}: {choices:?} outside {body:?}"
        );
        support::click_id(cx, s.win, list);
    }
}

fn font_file(env: &support::Env, name: &str, bytes: &[u8]) -> std::path::PathBuf {
    let path = env.tmp.path().join(name);
    std::fs::write(&path, bytes).unwrap();
    path
}

fn made_folder(env: &support::Env) -> std::path::PathBuf {
    let huds = env.data.library_dir(tf2tools_core::LibraryKind::Huds);
    let mut dirs: Vec<_> = std::fs::read_dir(&huds)
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect();
    dirs.retain(|d| d.join(".tf2tools/maker.json").is_file());
    assert_eq!(dirs.len(), 1, "one made HUD");
    dirs.remove(0)
}

#[gpui_kit::test]
fn an_added_font_file_saves_with_the_hud(cx: &mut TestAppContext) {
    let env = support::env();
    let bytes = support::fonts::sfnt_bytes(&[(1, "Big Numbers")]);
    let path = font_file(&env, "Big Numbers.ttf", &bytes);
    let picker = Arc::new(ScriptedPicker::new([path]));
    let s = shell(cx, &env, picker.clone());
    let page = open_maker(cx, &s);
    click_row(cx, &s, HV);
    click_in_panel(cx, &s, ids::hud_maker_font_add("font"));
    support::wait_real(cx, "the font file to be added", |cx| {
        cx.update(|cx| {
            page.read(cx)
                .editor()
                .is_some_and(|e| !e.assets().is_empty())
        })
    });
    assert_eq!(picker.asked(), [PickKind::FontFile]);
    let input = cx.update(|cx| page.read(cx).name_input().clone());
    cx.update_window(s.win, |_, window, cx| {
        input.update(cx, |st, cx| st.set_value("Fonty", window, cx))
    })
    .unwrap();
    support::click_id(cx, s.win, ids::HUD_MAKER_SAVE);
    support::wait_real(cx, "the save", |cx| {
        cx.update(|cx| page.read(cx).editor().is_some_and(|e| !e.is_dirty()))
    });
    let dir = made_folder(&env);
    assert_eq!(
        std::fs::read(dir.join("resource/fonts/tf2tools/big_numbers.ttf")).unwrap(),
        bytes
    );
    assert_eq!(
        std::fs::read(dir.join(".tf2tools/fonts/big_numbers.ttf")).unwrap(),
        bytes
    );
}

#[gpui_kit::test]
fn a_file_that_is_not_a_font_toasts_and_changes_nothing(cx: &mut TestAppContext) {
    let env = support::env();
    let path = font_file(&env, "notes.ttf", b"not a font at all");
    let s = shell(cx, &env, Arc::new(ScriptedPicker::new([path])));
    let page = open_maker(cx, &s);
    click_row(cx, &s, HV);
    click_in_panel(cx, &s, ids::hud_maker_font_add("font"));
    support::wait_real(cx, "the toast", |cx| {
        support::toast_labels(cx, s.win)
            .iter()
            .any(|t| t == "That file isn't a font TF2Tools can read.")
    });
    let changed = cx.update(|cx| page.read(cx).editor().map(|e| e.changes().len()));
    assert_eq!(changed, Some(0));
}

#[gpui_kit::test]
fn a_good_font_file_on_a_part_whose_font_the_hud_lacks_never_blames_the_file(
    cx: &mut TestAppContext,
) {
    let env = support::env();
    let bytes = support::fonts::sfnt_bytes(&[(1, "Big Numbers")]);
    let path = font_file(&env, "Big Numbers.ttf", &bytes);
    let no_fonts = r#""Scheme" { "Colors" { "TanDark" "117 107 94 255" } }"#;
    let s = shell_with_scheme(cx, &env, Arc::new(ScriptedPicker::new([path])), no_fonts);
    let page = open_maker(cx, &s);
    click_row(cx, &s, HV);
    click_in_panel(cx, &s, ids::hud_maker_font_add("font"));
    support::wait_real(cx, "the toast", |cx| {
        !support::toast_labels(cx, s.win).is_empty()
    });
    assert_eq!(
        support::toast_labels(cx, s.win),
        ["This part's font can't be changed in this HUD, so the font wasn't added."]
    );
    let changed = cx.update(|cx| page.read(cx).editor().map(|e| e.changes().len()));
    assert_eq!(changed, Some(0));
}

#[gpui_kit::test]
fn a_cancelled_pick_changes_nothing(cx: &mut TestAppContext) {
    let env = support::env();
    let s = shell(cx, &env, Arc::new(ScriptedPicker::empty()));
    let page = open_maker(cx, &s);
    click_row(cx, &s, HV);
    click_in_panel(cx, &s, ids::hud_maker_font_add("font"));
    support::settle(cx, s.win);
    let changed = cx.update(|cx| page.read(cx).editor().map(|e| e.changes().len()));
    assert_eq!(changed, Some(0));
    assert!(support::toast_labels(cx, s.win).is_empty());
}

#[gpui_kit::test]
fn a_picked_font_draws_under_every_family_it_carries(cx: &mut TestAppContext) {
    let env = support::env();
    let bytes = support::fonts::sfnt_bytes(&[(1, "Team Fortress"), (1, "Counter-Strike")]);
    let path = font_file(&env, "tf.ttf", &bytes);
    let s = shell(cx, &env, Arc::new(ScriptedPicker::new([path])));
    let page = open_maker(cx, &s);
    click_row(cx, &s, HV);
    click_in_panel(cx, &s, ids::hud_maker_font_add("font"));
    support::wait_real(cx, "the font file to be added", |cx| {
        cx.update(|cx| page.read(cx).knows_font("Team Fortress"))
    });
    assert!(cx.update(|cx| page.read(cx).knows_font("COUNTER-STRIKE")));
}

#[gpui_kit::test]
fn a_reopened_hud_keeps_its_added_font(cx: &mut TestAppContext) {
    let env = support::env();
    let bytes = support::fonts::sfnt_bytes(&[(1, "Big Numbers")]);
    let path = font_file(&env, "Big Numbers.ttf", &bytes);
    let s = shell(cx, &env, Arc::new(ScriptedPicker::new([path])));
    let page = open_maker(cx, &s);
    click_row(cx, &s, HV);
    click_in_panel(cx, &s, ids::hud_maker_font_add("font"));
    support::wait_real(cx, "the font file to be added", |cx| {
        cx.update(|cx| page.read(cx).knows_font("big numbers"))
    });
    // Save makes the HUD, then Save as new copies it, so both writes carry the font.
    let input = cx.update(|cx| page.read(cx).name_input().clone());
    let mut ids_seen = Vec::new();
    for (name, button) in [
        ("Fonty", ids::HUD_MAKER_SAVE),
        ("Fonty two", ids::HUD_MAKER_SAVE_AS_NEW),
    ] {
        cx.update_window(s.win, |_, window, cx| {
            input.update(cx, |st, cx| st.set_value(name, window, cx))
        })
        .unwrap();
        if button == ids::HUD_MAKER_SAVE_AS_NEW {
            support::click_id(cx, s.win, ids::HUD_MAKER_MORE);
        }
        support::click_id(cx, s.win, button);
        support::wait_real(cx, "the save", |cx| {
            cx.update(|cx| {
                let p = page.read(cx);
                p.saved_hud().is_some_and(|id| !ids_seen.contains(id))
                    && p.editor().is_some_and(|e| !e.is_dirty())
            })
        });
        ids_seen.push(cx.update(|cx| page.read(cx).saved_hud().cloned()).unwrap());
    }
    let id = ids_seen.pop().unwrap();
    cx.update(|cx| page.update(cx, |p, cx| p.open(id, cx)));
    support::wait_real(cx, "the made HUD to load", |cx| {
        cx.update(|cx| page.read(cx).is_loaded())
    });
    let families = cx.update(|cx| page.read(cx).editor().map(|e| e.font_families()));
    assert!(families.unwrap().iter().any(|f| f == "Big Numbers"));
    assert!(cx.update(|cx| page.read(cx).knows_font("Big Numbers")));
}

const METER: &str = "HudItemEffectMeter";

#[gpui_kit::test]
fn all_settings_list_every_key_with_where_it_came_from(cx: &mut TestAppContext) {
    let (_env, s, _page) = opened(cx);
    click_row(cx, &s, METER);
    assert!(
        !support::present(cx, s.win, ids::hud_maker_setting("xpos")),
        "starts closed"
    );
    click_in_panel(cx, &s, ids::HUD_MAKER_ALL_SETTINGS.into());
    for key in ["tall", "wide", "xpos", "ypos"] {
        assert!(
            support::present(cx, s.win, ids::hud_maker_setting(key)),
            "{key}"
        );
    }
    assert_eq!(
        support::label_of(cx, s.win, ids::hud_maker_setting_value("xpos")).as_deref(),
        Some("r174")
    );
    assert_eq!(
        support::label_of(cx, s.win, ids::hud_maker_setting_origin("wide")).as_deref(),
        Some("From scripts/hudlayout.res, line 9")
    );
}

#[gpui_kit::test]
fn a_long_origin_keeps_its_line_number_in_view(cx: &mut TestAppContext) {
    let (_env, s, _page) = opened(cx);
    for (w, h) in [(1280., 800.), (800., 600.)] {
        cx.simulate_window_resize(s.win, gpui_kit::size(gpui_kit::px(w), gpui_kit::px(h)));
        support::settle(cx, s.win);
        click_row(cx, &s, HV);
        // In a narrow area the open part list closes the side panel; swap them back.
        if !support::present(cx, s.win, ids::HUD_MAKER_PANEL_BODY) {
            support::click_id(cx, s.win, ids::HUD_MAKER_PARTS_PANEL_COLLAPSE);
            support::click_id(cx, s.win, ids::HUD_MAKER_PANEL_COLLAPSE);
        }
        if !support::present(cx, s.win, ids::hud_maker_setting("fgcolor")) {
            scroll_panel_to(cx, &s, ids::HUD_MAKER_ALL_SETTINGS.into());
            click_in_panel(cx, &s, ids::HUD_MAKER_ALL_SETTINGS.into());
        }
        scroll_panel_to(cx, &s, ids::hud_maker_setting_origin("fgcolor"));
        assert_eq!(
            support::label_of(cx, s.win, ids::hud_maker_setting_origin("fgcolor")).as_deref(),
            Some("From resource/ui/hudplayerhealth.res, line 32")
        );
        let row = support::bounds_of(cx, s.win, ids::hud_maker_setting("fgcolor")).unwrap();
        let line = support::bounds_of(cx, s.win, ids::hud_maker_setting_line("fgcolor"))
            .unwrap_or_else(|| panic!("{w}x{h}: no line number"));
        assert_eq!(
            support::label_of(cx, s.win, ids::hud_maker_setting_line("fgcolor")).as_deref(),
            Some("line 32")
        );
        assert!(
            line.left() >= row.left() && line.right() <= row.right(),
            "{w}x{h}: {line:?} outside {row:?}"
        );
    }
}

#[gpui_kit::test]
fn a_setting_edits_as_text_and_removes(cx: &mut TestAppContext) {
    let (_env, s, page) = opened(cx);
    click_row(cx, &s, METER);
    click_in_panel(cx, &s, ids::HUD_MAKER_ALL_SETTINGS.into());
    click_in_panel(cx, &s, ids::hud_maker_setting_value("tall"));
    assert!(support::present(cx, s.win, ids::HUD_MAKER_SETTING_INPUT));
    commit(cx, &s, &page, Field::Setting, "60");
    assert_eq!(value(cx, &page, METER, "tall").as_deref(), Some("60"));
    click_in_panel(cx, &s, ids::hud_maker_setting_remove("xpos"));
    assert_eq!(value(cx, &page, METER, "xpos"), None);
    assert!(
        !support::present(cx, s.win, ids::hud_maker_setting("xpos")),
        "a removed key leaves the list"
    );
}

#[gpui_kit::test]
fn identity_keys_are_listed_but_not_editable(cx: &mut TestAppContext) {
    let (_env, s, _page) = opened(cx);
    click_row(cx, &s, HV);
    // The label's Stage 2 rows push the section below the panel's view, so it scrolls there.
    click_in_panel(cx, &s, ids::HUD_MAKER_ALL_SETTINGS.into());
    assert!(support::present(
        cx,
        s.win,
        ids::hud_maker_setting("ControlName")
    ));
    assert!(!support::present(
        cx,
        s.win,
        ids::hud_maker_setting_value("ControlName")
    ));
    assert!(!support::present(
        cx,
        s.win,
        ids::hud_maker_setting_remove("ControlName")
    ));
    click_row(cx, &s, "HudWeaponAmmo");
    assert!(
        support::present(cx, s.win, ids::hud_maker_setting("visible")),
        "the game sets the ammo's visibility"
    );
    assert!(!support::present(
        cx,
        s.win,
        ids::hud_maker_setting_remove("visible")
    ));
    assert!(support::present(
        cx,
        s.win,
        ids::hud_maker_setting_remove("xpos")
    ));
}

#[gpui_kit::test]
fn a_setting_typed_then_another_pressed_commits_the_first(cx: &mut TestAppContext) {
    let (_env, s, page) = opened(cx);
    click_row(cx, &s, METER);
    click_in_panel(cx, &s, ids::HUD_MAKER_ALL_SETTINGS.into());
    click_in_panel(cx, &s, ids::hud_maker_setting_value("tall"));
    let input = cx.update(|cx| page.read(cx).field(Field::Setting).clone());
    cx.update_window(s.win, |_, window, cx| {
        input.update(cx, |st, cx| st.set_value("70", window, cx))
    })
    .unwrap();
    click_in_panel(cx, &s, ids::hud_maker_setting_value("wide"));
    assert_eq!(value(cx, &page, METER, "tall").as_deref(), Some("70"));
    assert_eq!(value(cx, &page, METER, "wide").as_deref(), Some("100"));
    let shown = cx.update(|cx| {
        page.read(cx)
            .field(Field::Setting)
            .read(cx)
            .value()
            .to_string()
    });
    assert_eq!(shown, "100");
}

fn hv_key() -> PartKey {
    PartKey::new(HV)
}

#[gpui_kit::test]
fn upper_case_and_alignment_show_on_the_canvas(cx: &mut TestAppContext) {
    let (_env, s, _page) = opened(cx);
    click_row(cx, &s, HV);
    click_in_panel(cx, &s, ids::hud_maker_toggle("allcaps"));
    click_in_panel(cx, &s, ids::hud_maker_align("east"));
    assert_eq!(
        support::label_of(cx, s.win, ids::hud_maker_text(&hv_key())).as_deref(),
        Some("150")
    );
    let part = support::bounds_of(cx, s.win, ids::hud_maker_part(&hv_key())).unwrap();
    let text = support::bounds_of(cx, s.win, ids::hud_maker_text(&hv_key())).unwrap();
    let gap = f32::from(part.right() - text.right()).abs();
    assert!(
        gap <= 2.0,
        "east puts the text at the right edge, {gap} px away"
    );
    click_in_panel(cx, &s, ids::hud_maker_align("west"));
    let text = support::bounds_of(cx, s.win, ids::hud_maker_text(&hv_key())).unwrap();
    assert!(f32::from(text.left() - part.left()).abs() <= 2.0);
}

#[gpui_kit::test]
fn allcaps_upper_cases_letters(cx: &mut TestAppContext) {
    let (_env, s, page) = opened(cx);
    click_row(cx, &s, HV);
    commit(cx, &s, &page, Field::Text, "hp %Health%");
    assert_eq!(
        support::label_of(cx, s.win, ids::hud_maker_text(&hv_key())).as_deref(),
        Some("hp 150")
    );
    click_in_panel(cx, &s, ids::hud_maker_toggle("allcaps"));
    assert_eq!(
        support::label_of(cx, s.win, ids::hud_maker_text(&hv_key())).as_deref(),
        Some("HP 150")
    );
}

#[gpui_kit::test]
fn a_shadowed_font_draws_its_shadow(cx: &mut TestAppContext) {
    let (_env, s, _page) = opened(cx);
    click_row(cx, &s, HV);
    assert!(!support::present(
        cx,
        s.win,
        ids::hud_maker_text_shadow(&hv_key())
    ));
    click_in_panel(cx, &s, ids::hud_maker_font_list("font"));
    support::click_id(cx, s.win, ids::hud_maker_font_name("HudFontSmall"));
    assert!(support::present(
        cx,
        s.win,
        ids::hud_maker_text_shadow(&hv_key())
    ));
    assert!(!support::present(
        cx,
        s.win,
        ids::hud_maker_text_outline(&hv_key())
    ));
}

fn kf_key() -> PartKey {
    PartKey::new(KF)
}

fn selection(cx: &mut TestAppContext, page: &Entity<HudMakerPage>) -> Option<String> {
    cx.update(|cx| {
        page.read(cx)
            .editor()?
            .selected()
            .map(|k| k.as_str().to_owned())
    })
}

#[gpui_kit::test]
fn two_killfeed_lines_draw_right_justified(cx: &mut TestAppContext) {
    let (_env, s, _page) = opened(cx);
    assert_eq!(
        support::label_of(cx, s.win, ids::hud_maker_killfeed_line(0)).as_deref(),
        Some("You killed Blue Rocket")
    );
    assert_eq!(
        support::label_of(cx, s.win, ids::hud_maker_killfeed_line(1)).as_deref(),
        Some("Market Garden killed Rocket Jumper")
    );
    let part = support::bounds_of(cx, s.win, ids::hud_maker_part(&kf_key())).unwrap();
    let first = support::bounds_of(cx, s.win, ids::hud_maker_killfeed_line(0)).unwrap();
    let second = support::bounds_of(cx, s.win, ids::hud_maker_killfeed_line(1)).unwrap();
    assert!(f32::from(part.right() - first.right()).abs() <= 1.0);
    assert!(f32::from(first.top() - part.top()).abs() <= 1.0);
    assert!(
        second.top() > first.bottom(),
        "the second line sits below the first"
    );
}

#[gpui_kit::test]
fn pressing_a_line_selects_the_killfeed(cx: &mut TestAppContext) {
    let (_env, s, page) = opened(cx);
    support::click_id(cx, s.win, ids::hud_maker_killfeed_line(1));
    assert_eq!(selection(cx, &page).as_deref(), Some(KF));
}

#[gpui_kit::test]
fn a_click_beside_the_lines_still_reaches_other_parts(cx: &mut TestAppContext) {
    let (_env, s, page) = opened(cx);
    support::click_id(
        cx,
        s.win,
        ids::hud_maker_part(&PartKey::new("HudWeaponAmmo/AmmoInClip")),
    );
    assert_eq!(
        selection(cx, &page).as_deref(),
        Some("HudWeaponAmmo/AmmoInClip")
    );
}

#[gpui_kit::test]
fn no_lines_in_the_scoreboard_or_when_hidden(cx: &mut TestAppContext) {
    let (_env, s, _page) = opened(cx);
    click_row(cx, &s, KF);
    // The side panel spills below the killfeed, so its body may need scrolling.
    click_in_panel(cx, &s, ids::HUD_MAKER_HIDE.into());
    assert!(!support::present(
        cx,
        s.win,
        ids::hud_maker_killfeed_line(0)
    ));
    support::click_id(cx, s.win, ids::HUD_MAKER_UNDO);
    assert!(support::present(cx, s.win, ids::hud_maker_killfeed_line(0)));
    support::click_id(
        cx,
        s.win,
        ids::hud_maker_scene(tf2tools_core::Scene::Scoreboard),
    );
    assert!(!support::present(
        cx,
        s.win,
        ids::hud_maker_killfeed_line(0)
    ));
}

#[gpui_kit::test]
fn the_remove_button_is_keyed_by_its_setting_and_named(cx: &mut TestAppContext) {
    let (_env, s, _page) = opened(cx);
    click_row(cx, &s, METER);
    click_in_panel(cx, &s, ids::HUD_MAKER_ALL_SETTINGS.into());
    assert_eq!(
        support::label_of(cx, s.win, ids::hud_maker_setting_remove_button("xpos")).as_deref(),
        Some("Remove this setting")
    );
}

#[gpui_kit::test]
fn undo_after_remove_brings_the_setting_back(cx: &mut TestAppContext) {
    let (_env, s, page) = opened(cx);
    click_row(cx, &s, METER);
    click_in_panel(cx, &s, ids::HUD_MAKER_ALL_SETTINGS.into());
    click_in_panel(cx, &s, ids::hud_maker_setting_remove("xpos"));
    assert!(!support::present(cx, s.win, ids::hud_maker_setting("xpos")));
    support::click_id(cx, s.win, ids::HUD_MAKER_UNDO);
    assert!(support::present(cx, s.win, ids::hud_maker_setting("xpos")));
    assert_eq!(value(cx, &page, METER, "xpos").as_deref(), Some("r174"));
}

#[gpui_kit::test]
fn a_font_list_and_the_named_colours_never_show_together(cx: &mut TestAppContext) {
    let (_env, s, _page) = opened(cx);
    click_row(cx, &s, KF);
    click_in_panel(cx, &s, ids::hud_maker_colour_row("TeamRed"));
    click_in_panel(cx, &s, ids::HUD_MAKER_COLOUR_NAME.into());
    assert!(support::present(cx, s.win, ids::HUD_MAKER_COLOUR_NAMES));
    click_in_panel(cx, &s, ids::hud_maker_font_list("TextFont"));
    assert!(support::present(
        cx,
        s.win,
        ids::hud_maker_font_name("HudFontSmall")
    ));
    assert!(
        !support::present(cx, s.win, ids::HUD_MAKER_COLOUR_NAMES),
        "a font list closes the named colours"
    );
    // The open font list takes wheel turns under it, so the panel turns from its bottom edge.
    let body = support::bounds_of(cx, s.win, ids::HUD_MAKER_PANEL_BODY).unwrap();
    send(
        cx,
        &s,
        gpui_kit::ScrollWheelEvent {
            position: point(
                body.left() + gpui_kit::px(8.),
                body.bottom() - gpui_kit::px(8.),
            ),
            delta: ScrollDelta::Lines(point(0., -40.)),
            modifiers: Default::default(),
            touch_phase: gpui_kit::TouchPhase::Moved,
        },
    );
    support::click_id(cx, s.win, ids::HUD_MAKER_COLOUR_NAME);
    assert!(support::present(cx, s.win, ids::HUD_MAKER_COLOUR_NAMES));
    assert!(
        !support::present(cx, s.win, ids::hud_maker_font_name("HudFontSmall")),
        "the named colours close a font list"
    );
}

#[gpui_kit::test]
fn an_outline_or_shadow_press_is_one_undo_step(cx: &mut TestAppContext) {
    let (_env, s, page) = opened(cx);
    click_row(cx, &s, HV);
    let before = value(cx, &page, HV, "font");
    for press in [
        ids::hud_maker_font_outline("font"),
        ids::hud_maker_font_shadow("font"),
    ] {
        click_in_panel(cx, &s, press);
        assert_ne!(value(cx, &page, HV, "font"), before);
        support::click_id(cx, s.win, ids::HUD_MAKER_UNDO);
        assert_eq!(value(cx, &page, HV, "font"), before);
        let changes = cx.update(|cx| page.read(cx).editor().unwrap().changes().len());
        assert_eq!(changes, 0, "one undo takes the whole press back");
    }
}

#[gpui_kit::test]
fn an_active_colour_row_lapses_when_the_canvas_selects_another_part(cx: &mut TestAppContext) {
    let (_env, s, page) = opened(cx);
    click_row(cx, &s, KF);
    click_in_panel(cx, &s, ids::hud_maker_colour_row("TeamRed"));
    assert!(support::present(cx, s.win, ids::HUD_MAKER_COLOUR));
    let ammo = "HudWeaponAmmo/AmmoInClip";
    support::click_id(cx, s.win, ids::hud_maker_part(&PartKey::new(ammo)));
    assert_eq!(selection(cx, &page).as_deref(), Some(ammo));
    support::click_id(cx, s.win, ids::hud_maker_killfeed_line(0));
    assert_eq!(selection(cx, &page).as_deref(), Some(KF));
    assert!(
        !support::present(cx, s.win, ids::HUD_MAKER_COLOUR),
        "the killfeed starts again with no row active"
    );
}

#[gpui_kit::test]
fn an_active_colour_row_lapses_when_undo_leaves_the_part(cx: &mut TestAppContext) {
    let (_env, s, page) = opened(cx);
    // An edit on the scoreboard, so the undo below switches scene and clears the selection.
    cx.update(|cx| {
        page.update(cx, |p, _| {
            let e = p.editor_mut().unwrap();
            e.set_scene(tf2tools_core::Scene::Scoreboard);
            e.select(Some(PartKey::new("scores/BluePlayerList")));
            e.move_selected(50, 0).unwrap();
            assert!(e.can_undo());
            e.set_scene(tf2tools_core::Scene::Hud);
        })
    });
    click_row(cx, &s, KF);
    click_in_panel(cx, &s, ids::hud_maker_colour_row("TeamRed"));
    assert!(support::present(cx, s.win, ids::HUD_MAKER_COLOUR));
    support::click_id(cx, s.win, ids::HUD_MAKER_UNDO);
    assert_eq!(selection(cx, &page), None);
    // Back on the killfeed without the page's own select, which would clear the row itself.
    cx.update(|cx| {
        page.update(cx, |p, cx| {
            let e = p.editor_mut().unwrap();
            e.set_scene(tf2tools_core::Scene::Hud);
            e.select(Some(kf_key()));
            cx.notify();
        })
    });
    assert!(!support::present(cx, s.win, ids::HUD_MAKER_COLOUR));
    assert!(support::present(
        cx,
        s.win,
        ids::hud_maker_colour_row("TeamRed")
    ));
}

#[gpui_kit::test]
fn each_alignment_button_has_its_own_name(cx: &mut TestAppContext) {
    let (_env, s, _page) = opened(cx);
    click_row(cx, &s, HV);
    for (value, word) in [("north-west", "Top left"), ("east", "Right")] {
        assert_eq!(
            support::label_of(cx, s.win, ids::hud_maker_align_button(value)).as_deref(),
            Some(word)
        );
    }
}

/// Writes `text` to the health value's `key` as any setting, then waits for the canvas.
fn set_hv(
    cx: &mut TestAppContext,
    s: &support::ShellWindow,
    page: &Entity<HudMakerPage>,
    key: &str,
    text: &str,
) {
    cx.update(|cx| {
        page.update(cx, |p, cx| {
            let e = p.editor_mut().unwrap();
            e.select(Some(hv_key()));
            e.set_key_text_selected(key, text).unwrap();
            cx.notify();
        })
    });
    support::settle(cx, s.win);
}

#[gpui_kit::test]
fn an_alignment_the_game_does_not_know_draws_west(cx: &mut TestAppContext) {
    let (_env, s, page) = opened(cx);
    for unknown in ["northeast", "eastern"] {
        set_hv(cx, &s, &page, "textAlignment", unknown);
        let part = support::bounds_of(cx, s.win, ids::hud_maker_part(&hv_key())).unwrap();
        let text = support::bounds_of(cx, s.win, ids::hud_maker_text(&hv_key())).unwrap();
        let gap = f32::from(text.left() - part.left()).abs();
        assert!(
            gap <= 2.0,
            "{unknown} draws west, {gap} px from the left edge"
        );
        let middle =
            f32::from((text.top() + text.bottom()) / 2. - (part.top() + part.bottom()) / 2.);
        assert!(middle.abs() <= 2.0, "{unknown} is centred, {middle} px off");
    }
}

#[gpui_kit::test]
fn text_wider_than_its_box_draws_west(cx: &mut TestAppContext) {
    let (_env, s, page) = opened(cx);
    set_hv(
        cx,
        &s,
        &page,
        "labelText",
        "%Health% and many more words than the box holds",
    );
    for alignment in ["east", "center"] {
        set_hv(cx, &s, &page, "textAlignment", alignment);
        let part = support::bounds_of(cx, s.win, ids::hud_maker_part(&hv_key())).unwrap();
        let text = support::bounds_of(cx, s.win, ids::hud_maker_text(&hv_key())).unwrap();
        // The game narrows text to its box, so it fills the box and is cut at its east edge.
        let fill = f32::from(text.size.width - part.size.width).abs();
        assert!(fill <= 1.0, "the text fills its box, {fill} px off");
        let gap = f32::from(text.left() - part.left()).abs();
        assert!(gap <= 2.0, "{alignment} falls back to west, {gap} px away");
    }
}

#[gpui_kit::test]
fn allcaps_below_one_is_off(cx: &mut TestAppContext) {
    let (_env, s, page) = opened(cx);
    set_hv(cx, &s, &page, "labelText", "hp %Health%");
    set_hv(cx, &s, &page, "allcaps", "-1");
    assert_eq!(
        support::label_of(cx, s.win, ids::hud_maker_text(&hv_key())).as_deref(),
        Some("hp 150")
    );
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

#[gpui_kit::test]
fn a_drag_from_a_killfeed_line_moves_the_killfeed_as_one_undo_step(cx: &mut TestAppContext) {
    let (_env, s, page) = opened(cx);
    let left = gpui_kit::MouseButton::Left;
    let from = support::bounds_of(cx, s.win, ids::hud_maker_killfeed_line(0))
        .unwrap()
        .center();
    let to = from + point(gpui_kit::px(-60.), gpui_kit::px(40.));
    let before = value(cx, &page, KF, "xpos");
    send(
        cx,
        &s,
        gpui_kit::MouseDownEvent {
            button: left,
            position: from,
            modifiers: Default::default(),
            click_count: 1,
            first_mouse: false,
        },
    );
    send(
        cx,
        &s,
        gpui_kit::MouseMoveEvent {
            position: to,
            pressed_button: Some(left),
            modifiers: Default::default(),
        },
    );
    let held = cx.update(|cx| page.read(cx).editor().unwrap().can_undo());
    assert!(!held, "a running drag commits nothing");
    send(
        cx,
        &s,
        gpui_kit::MouseUpEvent {
            button: left,
            position: to,
            modifiers: Default::default(),
            click_count: 1,
        },
    );
    assert_eq!(selection(cx, &page).as_deref(), Some(KF));
    assert_ne!(value(cx, &page, KF, "xpos"), before, "the release moved it");
    support::click_id(cx, s.win, ids::HUD_MAKER_UNDO);
    assert_eq!(value(cx, &page, KF, "xpos"), before);
    assert!(support::disabled_of(cx, s.win, ids::HUD_MAKER_UNDO));
}

#[gpui_kit::test]
fn a_missing_windows_family_draws_in_its_open_stand_in(cx: &mut TestAppContext) {
    let env = support::env();
    let bytes = support::fonts::sfnt_bytes(&[(1, "DejaVu Sans")]);
    let path = font_file(&env, "DejaVuSans.ttf", &bytes);
    let s = shell(cx, &env, Arc::new(ScriptedPicker::new([path])));
    let page = open_maker(cx, &s);
    let drawn =
        |cx: &mut TestAppContext, family: &str| cx.update(|cx| page.read(cx).drawn_family(family));
    assert_eq!(drawn(cx, "Verdana"), None, "no stand-in is known yet");
    click_row(cx, &s, HV);
    click_in_panel(cx, &s, ids::hud_maker_font_add("font"));
    support::wait_real(cx, "the font file to be added", |cx| {
        cx.update(|cx| page.read(cx).knows_font("DejaVu Sans"))
    });
    assert_eq!(drawn(cx, "Verdana").as_deref(), Some("DejaVu Sans"));
    assert_eq!(
        drawn(cx, "dejavu sans").as_deref(),
        Some("dejavu sans"),
        "a known family draws itself"
    );
    assert_eq!(
        drawn(cx, "TF2 Build"),
        None,
        "no stand-in for the game's own fonts"
    );
}

#[gpui_kit::test]
fn a_text_inset_moves_the_text_in_from_its_aligned_edge(cx: &mut TestAppContext) {
    let (_env, s, page) = opened(cx);
    // The gap from the aligned edge grows with the inset: none at 0, twice as wide at 10 as at 5.
    let mut gaps = Vec::new();
    for alignment in ["west", "east"] {
        set_hv(cx, &s, &page, "textAlignment", alignment);
        for inset in ["0", "5", "10"] {
            set_hv(cx, &s, &page, "textinsetx", inset);
            let part = support::bounds_of(cx, s.win, ids::hud_maker_part(&hv_key())).unwrap();
            let text = support::bounds_of(cx, s.win, ids::hud_maker_text(&hv_key())).unwrap();
            let gap = match alignment {
                "west" => text.left() - part.left(),
                _ => part.right() - text.right(),
            };
            gaps.push(f32::from(gap));
        }
    }
    for (side, gap) in [("west", &gaps[..3]), ("east", &gaps[3..])] {
        assert!(
            gap[0].abs() <= 2.0,
            "{side} with no inset sits at its edge: {gaps:?}"
        );
        assert!(gap[1] > 2.0, "{side} moves in: {gaps:?}");
        assert!(
            (gap[2] - 2. * gap[1]).abs() <= 2.0,
            "{side} scales: {gaps:?}"
        );
    }
}

#[gpui_kit::test]
fn a_negative_or_centred_x_inset_places_the_text_as_the_game_does(cx: &mut TestAppContext) {
    let (_env, s, page) = opened(cx);
    // (left gap, width) of the text against its box.
    let place = |cx: &mut TestAppContext| {
        let part = support::bounds_of(cx, s.win, ids::hud_maker_part(&hv_key())).unwrap();
        let text = support::bounds_of(cx, s.win, ids::hud_maker_text(&hv_key())).unwrap();
        (
            f32::from(text.left() - part.left()),
            f32::from(text.size.width),
            f32::from(part.size.width),
        )
    };
    set_hv(cx, &s, &page, "textAlignment", "west");
    set_hv(cx, &s, &page, "textinsetx", "10");
    let (inset, ..) = place(cx);
    assert!(inset > 2.0, "a west inset moves the text in: {inset}");
    // A negative inset pulls the text out past the west edge, where the box clips it.
    set_hv(cx, &s, &page, "textinsetx", "-10");
    let (gap, ..) = place(cx);
    assert!(
        (gap + inset).abs() <= 1.0,
        "pulled out by {gap}, not -{inset}"
    );
    // Centred text that fits ignores the inset.
    set_hv(cx, &s, &page, "textAlignment", "center");
    set_hv(cx, &s, &page, "textinsetx", "0");
    let fits = place(cx);
    set_hv(cx, &s, &page, "textinsetx", "10");
    assert_eq!(place(cx), fits, "a centred label that fits keeps its place");
    // Centred text too wide for the box narrows to the box less the inset, centred in it.
    set_hv(cx, &s, &page, "labelText", &"8".repeat(60));
    let (gap, wide, box_wide) = place(cx);
    assert!(
        (wide - (box_wide - inset)).abs() <= 1.0,
        "draws {wide} wide in a {box_wide} box less {inset}"
    );
    assert!((gap - inset / 2.).abs() <= 1.0, "centred at {gap}");
}

#[gpui_kit::test]
fn text_wider_than_its_room_starts_west_then_takes_an_east_inset(cx: &mut TestAppContext) {
    let (_env, s, page) = opened(cx);
    let place = |cx: &mut TestAppContext| {
        let part = support::bounds_of(cx, s.win, ids::hud_maker_part(&hv_key())).unwrap();
        let text = support::bounds_of(cx, s.win, ids::hud_maker_text(&hv_key())).unwrap();
        (
            f32::from(text.left() - part.left()),
            f32::from(text.size.width - part.size.width),
        )
    };
    set_hv(cx, &s, &page, "textAlignment", "west");
    set_hv(cx, &s, &page, "textinsetx", "10");
    let (inset, _) = place(cx);
    set_hv(cx, &s, &page, "labelText", &"8".repeat(60));
    set_hv(cx, &s, &page, "textinsetx", "-10");
    // A negative inset widens the room past the box, so the text no longer fits: it starts
    // at the west edge, and an east label is then moved by its inset, back in by 10.
    for (alignment, start) in [("center", 0.), ("east", inset)] {
        set_hv(cx, &s, &page, "textAlignment", alignment);
        let (gap, over) = place(cx);
        assert!(
            (over - inset).abs() <= 1.0,
            "{alignment}: {over} wider than its box"
        );
        assert!(
            (gap - start).abs() <= 1.0,
            "{alignment}: starts at {gap}, not {start}"
        );
    }
}

#[gpui_kit::test]
fn the_text_clip_cuts_only_across_and_leaves_room_for_ink(cx: &mut TestAppContext) {
    let (_env, s, page) = opened(cx);
    set_hv(cx, &s, &page, "labelText", "gyp");
    set_hv(cx, &s, &page, "textinsetx", "10");
    for alignment in ["west", "center", "east"] {
        set_hv(cx, &s, &page, "textAlignment", alignment);
        let text = support::bounds_of(cx, s.win, ids::hud_maker_text(&hv_key())).unwrap();
        let clip = support::bounds_of(cx, s.win, ids::hud_maker_text_clip(&hv_key())).unwrap();
        // Across, the clip is the text's drawn width; up and down it leaves the font's ink,
        // outline and shadow copies a whole line of room.
        assert_eq!((clip.left(), clip.right()), (text.left(), text.right()));
        let tall = f32::from(text.size.height);
        assert!(
            f32::from(text.top() - clip.top()) >= tall,
            "{alignment}: room above"
        );
        assert!(
            f32::from(clip.bottom() - text.bottom()) >= tall,
            "{alignment}: room below"
        );
    }
}

#[gpui_kit::test]
fn a_y_inset_moves_the_text_down_whatever_its_alignment(cx: &mut TestAppContext) {
    let (_env, s, page) = opened(cx);
    // How far the text drops from where it sits with no inset: twice as far at 6 as at 3.
    for alignment in ["north-west", "west", "south-west"] {
        set_hv(cx, &s, &page, "textAlignment", alignment);
        let mut tops = Vec::new();
        for inset in ["0", "3", "6"] {
            set_hv(cx, &s, &page, "textinsety", inset);
            let text = support::bounds_of(cx, s.win, ids::hud_maker_text(&hv_key())).unwrap();
            tops.push(f32::from(text.top()));
        }
        let (d3, d6) = (tops[1] - tops[0], tops[2] - tops[0]);
        assert!(d3 > 1.0, "{alignment} moves down: {tops:?}");
        assert!((d6 - 2. * d3).abs() <= 1.0, "{alignment} scales: {tops:?}");
    }
}
