//! Presets, recipes and starting points in the HUD Maker, headless.

mod support;

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{AppContext as _, Entity, ScrollDelta, TestAppContext, point};
use std::path::PathBuf;
use std::sync::Arc;
use tf2tools_app::ids;
use tf2tools_app::pages::hud_maker::HudMakerPage;
use tf2tools_app::pages::hud_maker::editor::HudEditor;
use tf2tools_app::pages::hud_maker::view::PanelPlace;
use tf2tools_app::picker::{PickKind, ScriptedPicker};
use tf2tools_app::services::{Services, Tf2State};
use tf2tools_app::shell::route::Page;
use tf2tools_core::{
    HudAssets, HudModel, MakeTarget, OptionValue, PartKey, Platform, Recipe, Tf2Install,
};

const SCHEME: &str = r#""Scheme" { Colors { TanDark "117 107 94 255" TanLight "235 226 202 255"
  QualityColorNormal "178 178 178 255" } }"#;
const BASECHAT: &str = r#""Resource/UI/BaseChat.res" { HudChat { xpos 10 ypos 275 } }"#;
const HV: &str = "HudPlayerStatus/HudPlayerHealth/PlayerStatusHealthValue";
const MAINMENU: &str = r#""Resource/UI/MainMenuOverride.res" { FriendsContainer { visible 1 } }"#;

fn opened_with(
    cx: &mut TestAppContext,
    picks: Vec<PathBuf>,
) -> (
    support::Env,
    support::ShellWindow,
    Entity<HudMakerPage>,
    Arc<ScriptedPicker>,
) {
    opened_picking(cx, Arc::new(ScriptedPicker::new(picks)))
}

fn opened_picking(
    cx: &mut TestAppContext,
    picker: Arc<ScriptedPicker>,
) -> (
    support::Env,
    support::ShellWindow,
    Entity<HudMakerPage>,
    Arc<ScriptedPicker>,
) {
    let env = support::env();
    support::seed_stock_game_with(
        &env.game,
        &[
            ("resource/clientscheme.res", Some(SCHEME)),
            ("resource/ui/basechat.res", Some(BASECHAT)),
            ("resource/ui/mainmenuoverride.res", Some(MAINMENU)),
        ],
    );
    let state = Tf2State::Found(Tf2Install::from_path(&env.game).unwrap());
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

fn opened(cx: &mut TestAppContext) -> (support::Env, support::ShellWindow, Entity<HudMakerPage>) {
    let (env, s, page, _) = opened_with(cx, Vec::new());
    (env, s, page)
}

/// Clicks `id` in the side panel, scrolling the panel down until it is wholly in view, since
/// a click lands on the element's centre.
fn click_in_panel(cx: &mut TestAppContext, s: &support::ShellWindow, id: gpui_kit::ElementId) {
    scroll_into_panel(cx, s, &id);
    support::click_id(cx, s.win, id);
}

/// Scrolls the side panel down until `id` is wholly in view.
fn scroll_into_panel(cx: &mut TestAppContext, s: &support::ShellWindow, id: &gpui_kit::ElementId) {
    for _ in 0..60 {
        let body = support::bounds_of(cx, s.win, ids::HUD_MAKER_PANEL_BODY);
        let seen = cx
            .update_window(s.win, |_, w, _| {
                w.try_find(id.clone()).is_some_and(|e| {
                    let b = e.bounds();
                    e.visible()
                        && body.is_some_and(|body| {
                            b.top() >= body.top() && b.bottom() <= body.bottom()
                        })
                })
            })
            .unwrap();
        if seen {
            break;
        }
        let down = ScrollDelta::Lines(point(0., -3.));
        cx.update_window(s.win, |_, w, cx| {
            w.scroll(ids::HUD_MAKER_PANEL_BODY, down, cx)
        })
        .unwrap();
    }
}

fn type_preset_name(
    cx: &mut TestAppContext,
    s: &support::ShellWindow,
    page: &Entity<HudMakerPage>,
    name: &str,
) {
    let input = cx.update(|cx| page.read(cx).preset_name_input().clone());
    cx.update_window(s.win, |_, window, cx| {
        input.update(cx, |st, cx| st.set_value(name, window, cx))
    })
    .unwrap();
    support::settle(cx, s.win);
}

/// Saves the HUD's option choices as preset `name` and returns its file once it is listed.
fn save_preset(
    cx: &mut TestAppContext,
    s: &support::ShellWindow,
    page: &Entity<HudMakerPage>,
    name: &str,
) -> String {
    type_preset_name(cx, s, page, name);
    click_in_panel(cx, s, ids::HUD_MAKER_PRESET_SAVE.into());
    support::wait_real(cx, "the preset to list", |cx| {
        cx.update(|cx| {
            let presets = cx.global::<Services>().library.list_presets();
            presets.iter().any(|p| p.name() == name)
        })
    });
    let file = cx.update(|cx| {
        let presets = cx.global::<Services>().library.list_presets();
        presets
            .iter()
            .find(|p| p.name() == name)
            .unwrap()
            .file()
            .to_owned()
    });
    support::wait_real(cx, "the preset row", |cx| {
        support::present(cx, s.win, ids::hud_maker_preset_apply(&file))
    });
    file
}

#[gpui_kit::test]
fn presets_save_list_and_delete(cx: &mut TestAppContext) {
    let (_env, s, page) = opened(cx);
    click_in_panel(cx, &s, ids::hud_maker_option("chat_top"));
    let file = save_preset(cx, &s, &page, "Top chat");
    assert_eq!(
        support::label_of(cx, s.win, ids::hud_maker_preset_apply(&file)).as_deref(),
        Some("Top chat")
    );
    let changes = cx.update(|cx| page.read(cx).editor().unwrap().changes().to_vec());
    click_in_panel(cx, &s, ids::hud_maker_preset_delete(&file));
    support::wait_real(cx, "the preset row to go", |cx| {
        !support::present(cx, s.win, ids::hud_maker_preset_apply(&file))
    });
    assert!(cx.update(|cx| cx.global::<Services>().library.list_presets().is_empty()));
    assert_eq!(
        cx.update(|cx| page.read(cx).editor().unwrap().changes().to_vec()),
        changes
    );
}

#[gpui_kit::test]
fn a_preset_needs_a_name(cx: &mut TestAppContext) {
    let (_env, s, _page) = opened(cx);
    click_in_panel(cx, &s, ids::hud_maker_option("chat_top"));
    click_in_panel(cx, &s, ids::HUD_MAKER_PRESET_SAVE.into());
    support::wait_real(cx, "a toast", |cx| {
        !support::toast_labels(cx, s.win).is_empty()
    });
    assert!(
        support::toast_labels(cx, s.win)
            .iter()
            .any(|t| t.contains("Give the preset a name."))
    );
    assert!(cx.update(|cx| cx.global::<Services>().library.list_presets().is_empty()));
}

#[gpui_kit::test]
fn a_preset_needs_some_options(cx: &mut TestAppContext) {
    let (_env, s, page) = opened(cx);
    type_preset_name(cx, &s, &page, "Nothing");
    click_in_panel(cx, &s, ids::HUD_MAKER_PRESET_SAVE.into());
    support::wait_real(cx, "a toast", |cx| {
        !support::toast_labels(cx, s.win).is_empty()
    });
    assert!(
        support::toast_labels(cx, s.win)
            .iter()
            .any(|t| t.contains("Choose some options first, then save them as a preset."))
    );
}

fn previewing(cx: &mut TestAppContext, page: &Entity<HudMakerPage>) -> bool {
    cx.update(|cx| page.read(cx).editor().unwrap().previewing())
}

fn option(cx: &mut TestAppContext, page: &Entity<HudMakerPage>, id: &str) -> Option<OptionValue> {
    cx.update(|cx| page.read(cx).editor().unwrap().option_value(id))
}

fn undo(cx: &mut TestAppContext, s: &support::ShellWindow, page: &Entity<HudMakerPage>) {
    cx.update_window(s.win, |_, window, cx| {
        page.update(cx, |page, cx| page.undo(window, cx))
    })
    .unwrap();
    support::settle(cx, s.win);
}

/// Shows `recipe` through the page's one preview entry, as a file read with `skipped` would.
fn start_preview(
    cx: &mut TestAppContext,
    s: &support::ShellWindow,
    page: &Entity<HudMakerPage>,
    recipe: Recipe,
    skipped: usize,
) {
    cx.update_window(s.win, |_, window, cx| {
        page.update(cx, |page, cx| {
            page.start_preview(recipe, skipped, window, cx)
        })
    })
    .unwrap();
    support::settle(cx, s.win);
}

#[gpui_kit::test]
fn a_preset_previews_then_applies_as_one_step(cx: &mut TestAppContext) {
    let (_env, s, page) = opened(cx);
    click_in_panel(cx, &s, ids::hud_maker_option("chat_top"));
    let file = save_preset(cx, &s, &page, "Top chat");
    undo(cx, &s, &page);
    assert_eq!(
        option(cx, &page, "chat_top"),
        Some(OptionValue::Bool(false))
    );
    click_in_panel(cx, &s, ids::hud_maker_preset_apply(&file));
    support::wait_real(cx, "the preview", |cx| previewing(cx, &page));
    assert_eq!(
        support::label_of(cx, s.win, ids::HUD_MAKER_PREVIEW).as_deref(),
        // The canvas draws the chat, so moving it shows in the preview.
        Some("Previewing Top chat.")
    );
    click_in_panel(cx, &s, ids::HUD_MAKER_PREVIEW_APPLY.into());
    assert_eq!(option(cx, &page, "chat_top"), Some(OptionValue::Bool(true)));
    assert!(!support::present(cx, s.win, ids::HUD_MAKER_PREVIEW));
    undo(cx, &s, &page);
    assert_eq!(
        option(cx, &page, "chat_top"),
        Some(OptionValue::Bool(false))
    );
}

#[gpui_kit::test]
fn cancel_leaves_the_hud_as_it_was(cx: &mut TestAppContext) {
    let (_env, s, page) = opened(cx);
    click_in_panel(cx, &s, ids::hud_maker_option("hide_friends"));
    let file = save_preset(cx, &s, &page, "No friends");
    undo(cx, &s, &page);
    click_in_panel(cx, &s, ids::hud_maker_preset_apply(&file));
    support::wait_real(cx, "the preview", |cx| previewing(cx, &page));
    click_in_panel(cx, &s, ids::HUD_MAKER_PREVIEW_CANCEL.into());
    assert_eq!(
        option(cx, &page, "hide_friends"),
        Some(OptionValue::Bool(false))
    );
    assert!(!support::present(cx, s.win, ids::HUD_MAKER_PREVIEW));
}

#[gpui_kit::test]
fn the_bar_goes_when_the_editor_drops_the_preview(cx: &mut TestAppContext) {
    let (_env, s, page) = opened(cx);
    click_in_panel(cx, &s, ids::hud_maker_option("hide_friends"));
    let file = save_preset(cx, &s, &page, "No friends");
    undo(cx, &s, &page);
    click_in_panel(cx, &s, ids::hud_maker_preset_apply(&file));
    support::wait_real(cx, "the preview", |cx| previewing(cx, &page));
    // Undo cancels the preview inside the editor; the bar must follow.
    undo(cx, &s, &page);
    assert!(!support::present(cx, s.win, ids::HUD_MAKER_PREVIEW));
}

#[gpui_kit::test]
fn a_preset_that_cant_be_read_toasts(cx: &mut TestAppContext) {
    let (env, s, page) = opened(cx);
    click_in_panel(cx, &s, ids::hud_maker_option("chat_top"));
    let file = save_preset(cx, &s, &page, "Top chat");
    std::fs::write(env.data.hud_presets_dir().join(&file), b"not a preset").unwrap();
    click_in_panel(cx, &s, ids::hud_maker_preset_apply(&file));
    support::wait_real(cx, "a toast", |cx| {
        !support::toast_labels(cx, s.win).is_empty()
    });
    assert!(
        support::toast_labels(cx, s.win)
            .iter()
            .any(|t| t.contains("That file isn't a preset TF2Tools can read."))
    );
    assert!(!previewing(cx, &page));
}

#[gpui_kit::test]
fn the_bar_counts_one_change_that_doesnt_fit(cx: &mut TestAppContext) {
    let (_env, s, page) = opened(cx);
    start_preview(cx, &s, &page, Recipe::preset("Empty", &[]), 1);
    assert_eq!(
        support::label_of(cx, s.win, ids::HUD_MAKER_PREVIEW).as_deref(),
        Some("Previewing Empty. 1 change doesn't fit this HUD.")
    );
}

#[gpui_kit::test]
fn the_bar_and_apply_say_what_was_skipped_and_which_fonts_were_left_out(cx: &mut TestAppContext) {
    let (_env, s, page) = opened(cx);
    // A font change whose bytes are missing is left out when the recipe is made.
    let stock = HudModel::load(&support::hud_stock::stock_files(), Platform::Linux).unwrap();
    let mut maker = HudEditor::new(stock, Vec::new());
    maker.select(Some(PartKey::new(HV)));
    let bytes = support::fonts::sfnt_bytes(&[(1, "Big Numbers"), (2, "Regular")]);
    maker
        .add_font_file_selected("font", "Big Numbers.ttf", bytes)
        .unwrap();
    let font = maker.changes().to_vec();
    let recipe = Recipe::new("Fonts", &font, &HudAssets::new());
    assert_eq!(recipe.fonts_left_out(), 1);
    start_preview(cx, &s, &page, recipe, 2);
    assert_eq!(
        support::label_of(cx, s.win, ids::HUD_MAKER_PREVIEW).as_deref(),
        Some("Previewing Fonts. 2 changes don't fit this HUD. 1 font was left out.")
    );
    click_in_panel(cx, &s, ids::HUD_MAKER_PREVIEW_APPLY.into());
    support::wait_real(cx, "a toast", |cx| {
        !support::toast_labels(cx, s.win).is_empty()
    });
    let toasts = support::toast_labels(cx, s.win);
    assert!(
        toasts.iter().any(|t| t
            .contains("2 of your changes no longer fit and were left out. 1 font was left out.")),
        "{toasts:?}"
    );
    assert!(!support::present(cx, s.win, ids::HUD_MAKER_PREVIEW));
}

#[gpui_kit::test]
fn a_previewed_recipe_draws_with_its_own_fonts(cx: &mut TestAppContext) {
    let (_env, s, page) = opened(cx);
    let stock = HudModel::load(&support::hud_stock::stock_files(), Platform::Linux).unwrap();
    let mut maker = HudEditor::new(stock, Vec::new());
    maker.select(Some(PartKey::new(HV)));
    let bytes = support::fonts::sfnt_bytes(&[(1, "Preview Numbers"), (2, "Regular")]);
    maker
        .add_font_file_selected("font", "Preview Numbers.ttf", bytes)
        .unwrap();
    let recipe = Recipe::new("Fonts", maker.changes(), maker.assets());
    assert!(!cx.update(|cx| page.read(cx).knows_font("Preview Numbers")));
    start_preview(cx, &s, &page, recipe, 0);
    assert!(previewing(cx, &page));
    assert!(cx.update(|cx| page.read(cx).knows_font("Preview Numbers")));
}

#[gpui_kit::test]
fn the_bar_keeps_clear_at_800x600(cx: &mut TestAppContext) {
    let (_env, s, page) = opened(cx);
    cx.simulate_window_resize(
        s.win,
        gpui_kit::size(gpui_kit::px(800.), gpui_kit::px(600.)),
    );
    support::settle(cx, s.win);
    click_in_panel(cx, &s, ids::hud_maker_option("chat_top"));
    let changes = cx.update(|cx| page.read(cx).editor().unwrap().changes().to_vec());
    undo(cx, &s, &page);
    // Starting a preview from the foot of the panel brings the bar into view.
    start_preview(cx, &s, &page, Recipe::preset("Top chat", &changes), 3);
    let body = support::bounds_of(cx, s.win, ids::HUD_MAKER_PANEL_BODY).unwrap();
    let toolbar = support::bounds_of(cx, s.win, ids::HUD_MAKER_TOOLBAR).unwrap();
    let view = support::viewport(cx, s.win);
    for id in [
        ids::HUD_MAKER_PREVIEW,
        ids::HUD_MAKER_PREVIEW_APPLY,
        ids::HUD_MAKER_PREVIEW_CANCEL,
    ] {
        let b = support::bounds_of(cx, s.win, id).unwrap_or_else(|| panic!("no {id}"));
        assert!(
            b.top() >= body.top() && b.bottom() <= body.bottom(),
            "{id} {b:?} scrolled out of {body:?}"
        );
        assert!(
            b.right() <= view.width && b.bottom() <= view.height,
            "{id} {b:?} off {view:?}"
        );
        assert!(
            !b.intersects(&toolbar),
            "{id} {b:?} under the toolbar {toolbar:?}"
        );
    }
    support::click_id(cx, s.win, ids::HUD_MAKER_PREVIEW_APPLY);
    assert_eq!(option(cx, &page, "chat_top"), Some(OptionValue::Bool(true)));
}

#[gpui_kit::test]
fn a_preview_opens_a_collapsed_panel_so_the_bar_shows(cx: &mut TestAppContext) {
    let (_env, s, page) = opened(cx);
    cx.update(|cx| {
        page.update(cx, |page, cx| {
            let place = PanelPlace {
                collapsed: true,
                ..page.panel_place()
            };
            page.set_panel_place(place, cx)
        })
    });
    support::settle(cx, s.win);
    assert!(!support::present(cx, s.win, ids::HUD_MAKER_PANEL_BODY));
    start_preview(cx, &s, &page, Recipe::preset("Empty", &[]), 1);
    assert!(!cx.update(|cx| page.read(cx).panel_place().collapsed));
    assert!(support::present(cx, s.win, ids::HUD_MAKER_PREVIEW));
}

#[gpui_kit::test]
fn a_preset_read_for_a_hud_since_replaced_never_lands(cx: &mut TestAppContext) {
    let (_env, s, page) = opened(cx);
    let made = s
        .services
        .library
        .save_made_hud(
            &MakeTarget::New,
            "Made",
            &support::hud_stock::stock_files(),
            Platform::Linux,
            &[],
        )
        .unwrap();
    click_in_panel(cx, &s, ids::hud_maker_option("hide_friends"));
    let file = save_preset(cx, &s, &page, "No friends");
    undo(cx, &s, &page);
    let apply = ids::hud_maker_preset_apply(&file);
    scroll_into_panel(cx, &s, &apply);
    support::settle(cx, s.win);
    // The click starts the preset read and, before it lands, another HUD starts loading.
    cx.update_window(s.win, |_, w, cx| {
        w.click(apply, cx);
        page.update(cx, |p, cx| p.open(made.entry().id().clone(), cx));
    })
    .unwrap();
    support::wait_real(cx, "the made HUD to load", |cx| {
        cx.update(|cx| page.read(cx).saved_hud().is_some())
    });
    support::settle(cx, s.win);
    assert!(!previewing(cx, &page));
    assert!(!support::present(cx, s.win, ids::HUD_MAKER_PREVIEW));
}

#[gpui_kit::test]
fn a_shared_recipe_holds_the_hud_changes(cx: &mut TestAppContext) {
    let env_dir = tempfile::tempdir().unwrap();
    let target = env_dir.path().join("mine.json");
    let (_env, s, _page, picker) = opened_with(cx, vec![target.clone()]);
    click_in_panel(cx, &s, ids::hud_maker_option("chat_top"));
    click_in_panel(cx, &s, ids::HUD_MAKER_RECIPE_EXPORT.into());
    support::wait_real(cx, "the recipe file", |_| {
        std::fs::read(&target).is_ok_and(|b| Recipe::parse(&b).is_ok())
    });
    assert_eq!(picker.asked(), [PickKind::RecipeExport]);
    let (recipe, skipped) = Recipe::parse(&std::fs::read(&target).unwrap()).unwrap();
    assert_eq!((recipe.changes().len(), skipped), (1, 0));
}

#[gpui_kit::test]
fn a_recipe_file_previews_then_applies(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("friends.json");
    std::fs::write(
        &file,
        r#"{ "version": 1, "kind": "recipe", "name": "No friends", "made_for": null,
            "changes": [ { "group": 4, "option": { "source": "builtin", "id": "hide_friends",
                "value": { "bool": true } } },
              { "group": 4, "x-not-a-kind": {} } ] }"#,
    )
    .unwrap();
    let (_env, s, page, _) = opened_with(cx, vec![file]);
    click_in_panel(cx, &s, ids::HUD_MAKER_RECIPE_IMPORT.into());
    support::wait_real(cx, "the preview", |cx| {
        cx.update(|cx| page.read(cx).editor().unwrap().previewing())
    });
    assert_eq!(
        support::label_of(cx, s.win, ids::HUD_MAKER_PREVIEW).as_deref(),
        Some(
            "Previewing No friends. 1 change doesn't fit this HUD. Some of these changes only show in TF2."
        )
    );
    click_in_panel(cx, &s, ids::HUD_MAKER_PREVIEW_APPLY.into());
    assert_eq!(
        option(cx, &page, "hide_friends"),
        Some(OptionValue::Bool(true))
    );
    assert!(
        support::toast_labels(cx, s.win)
            .iter()
            .any(|t| t.contains("1 of your changes no longer fit and were left out."))
    );
    undo(cx, &s, &page);
    assert_eq!(
        option(cx, &page, "hide_friends"),
        Some(OptionValue::Bool(false))
    );
}

#[gpui_kit::test]
fn a_file_that_is_not_a_recipe_toasts_and_changes_nothing(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let junk = dir.path().join("junk.json");
    std::fs::write(&junk, b"\x00\x01 not json").unwrap();
    let huge = dir.path().join("huge.json");
    std::fs::write(&huge, vec![b' '; Recipe::MAX_BYTES + 2]).unwrap();
    let (_env, s, page, _) = opened_with(cx, vec![junk, huge]);
    for _ in 0..2 {
        click_in_panel(cx, &s, ids::HUD_MAKER_RECIPE_IMPORT.into());
        support::wait_real(cx, "a toast", |cx| {
            !support::toast_labels(cx, s.win).is_empty()
        });
        assert!(
            support::toast_labels(cx, s.win)
                .iter()
                .any(|t| t.contains("That file isn't a recipe TF2Tools can read."))
        );
        cx.update(|cx| {
            let e = page.read(cx).editor().unwrap();
            assert!(!e.previewing() && e.changes().is_empty());
        });
    }
}

#[gpui_kit::test]
fn sharing_says_how_many_fonts_were_left_out(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("fonts.json");
    let (_env, s, page, _) = opened_with(cx, vec![target.clone()]);
    // A font over the recipe's 16 MiB cap for one file is left out of the recipe.
    let stock = HudModel::load(&support::hud_stock::stock_files(), Platform::Linux).unwrap();
    let mut maker = HudEditor::new(stock, Vec::new());
    maker.select(Some(PartKey::new(HV)));
    let mut bytes = support::fonts::sfnt_bytes(&[(1, "Big Numbers"), (2, "Regular")]);
    bytes.resize(16 * 1024 * 1024 + 1, 0);
    maker
        .add_font_file_selected("font", "Big Numbers.ttf", bytes)
        .unwrap();
    // With nothing selected the side panel shows the options and the Recipes section.
    maker.select(None);
    cx.update(|cx| page.update(cx, |page, _| *page.editor_mut().unwrap() = maker));
    support::settle(cx, s.win);
    click_in_panel(cx, &s, ids::HUD_MAKER_RECIPE_EXPORT.into());
    support::wait_real(cx, "a toast", |cx| {
        !support::toast_labels(cx, s.win).is_empty()
    });
    let toasts = support::toast_labels(cx, s.win);
    assert!(
        toasts
            .iter()
            .any(|t| t.contains("Recipe saved. 1 font was left out.")),
        "{toasts:?}"
    );
    assert!(Recipe::parse(&std::fs::read(&target).unwrap()).is_ok());
}

#[gpui_kit::test]
fn sharing_never_replaces_a_file_the_json_name_lands_on(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let taken = dir.path().join("mine.json");
    std::fs::write(&taken, b"keep me").unwrap();
    let (_env, s, _page, picker) = opened_with(cx, vec![dir.path().join("mine")]);
    click_in_panel(cx, &s, ids::hud_maker_option("chat_top"));
    click_in_panel(cx, &s, ids::HUD_MAKER_RECIPE_EXPORT.into());
    support::wait_real(cx, "a toast", |cx| {
        !support::toast_labels(cx, s.win).is_empty()
    });
    let toasts = support::toast_labels(cx, s.win);
    assert!(
        toasts
            .iter()
            .any(|t| t.contains("A file named mine.json is already there. Pick another name.")),
        "{toasts:?}"
    );
    assert_eq!(std::fs::read(&taken).unwrap(), b"keep me");
    let mut left: Vec<_> = std::fs::read_dir(dir.path())
        .unwrap()
        .map(|e| e.unwrap().file_name())
        .collect();
    left.sort();
    assert_eq!(left, ["mine.json"]);
    assert!(picker.suggested().iter().all(|n| n.ends_with(".json")));
}

#[gpui_kit::test]
fn a_recipe_read_for_a_hud_since_replaced_never_lands(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("empty.json");
    std::fs::write(&file, Recipe::preset("Empty", &[]).to_json()).unwrap();
    let (picker, release) = ScriptedPicker::held(vec![file]);
    let (_env, s, page, _) = opened_picking(cx, Arc::new(picker));
    let made = s
        .services
        .library
        .save_made_hud(
            &MakeTarget::New,
            "Made",
            &support::hud_stock::stock_files(),
            Platform::Linux,
            &[],
        )
        .unwrap();
    // The dialog stays open while another HUD loads, then answers.
    click_in_panel(cx, &s, ids::HUD_MAKER_RECIPE_IMPORT.into());
    cx.update(|cx| page.update(cx, |p, cx| p.open(made.entry().id().clone(), cx)));
    support::wait_real(cx, "the made HUD to load", |cx| {
        cx.update(|cx| page.read(cx).saved_hud().is_some())
    });
    release.release();
    support::settle(cx, s.win);
    assert!(!previewing(cx, &page));
    assert!(!support::present(cx, s.win, ids::HUD_MAKER_PREVIEW));
}

/// Opens the toolbar's overflow menu; true when its Start from entry is live.
fn open_more(cx: &mut TestAppContext, s: &support::ShellWindow) -> bool {
    support::click_id(cx, s.win, ids::HUD_MAKER_MORE);
    assert!(support::present(cx, s.win, ids::HUD_MAKER_START));
    !support::disabled_of(cx, s.win, ids::HUD_MAKER_START)
}

/// Picks starting point `label` through the overflow menu.
fn start_from(cx: &mut TestAppContext, s: &support::ShellWindow, label: &str) {
    assert!(open_more(cx, s), "Start from is off");
    support::click_id(cx, s.win, ids::HUD_MAKER_START);
    support::click_id(cx, s.win, ids::hud_maker_start(label));
}

#[gpui_kit::test]
fn a_new_hud_offers_the_starting_points_in_the_overflow_menu(cx: &mut TestAppContext) {
    let (_env, s, page) = opened(cx);
    // Nothing in the side panel: the starting points live in the menu only.
    assert!(!support::present(cx, s.win, ids::HUD_MAKER_START));
    assert!(open_more(cx, &s));
    support::click_id(cx, s.win, ids::HUD_MAKER_START);
    for label in [
        "Competitive minimal",
        "Big numbers",
        "Streamer clean",
        "Centre HUD",
    ] {
        assert!(
            support::present(cx, s.win, ids::hud_maker_start(label)),
            "{label}"
        );
    }
    support::click_id(cx, s.win, ids::hud_maker_start("Streamer clean"));
    assert!(previewing(cx, &page));
    assert!(
        !support::present(cx, s.win, ids::HUD_MAKER_START),
        "the menu closes"
    );
    // The next one replaces the preview, so trying them all takes no scrolling.
    start_from(cx, &s, "Competitive minimal");
    assert!(previewing(cx, &page));
    click_in_panel(cx, &s, ids::HUD_MAKER_PREVIEW_APPLY.into());
    assert_eq!(
        option(cx, &page, "killfeed_rows"),
        Some(OptionValue::Int(3))
    );
    assert_eq!(
        option(cx, &page, "chat_top"),
        Some(OptionValue::Bool(false))
    );
    assert!(!open_more(cx, &s), "an applied starting point is an edit");
}

#[gpui_kit::test]
fn start_from_is_off_once_the_player_edits(cx: &mut TestAppContext) {
    let (_env, s, _page) = opened(cx);
    click_in_panel(cx, &s, ids::hud_maker_option("chat_top"));
    assert!(!open_more(cx, &s));
    // A press on the off entry opens no choices.
    support::click_id(cx, s.win, ids::HUD_MAKER_START);
    assert!(!support::present(
        cx,
        s.win,
        ids::hud_maker_start("Big numbers")
    ));
}

#[gpui_kit::test]
fn cancelling_a_starting_point_keeps_start_from_on(cx: &mut TestAppContext) {
    let (_env, s, page) = opened(cx);
    start_from(cx, &s, "Streamer clean");
    click_in_panel(cx, &s, ids::HUD_MAKER_PREVIEW_CANCEL.into());
    assert!(!previewing(cx, &page));
    assert!(open_more(cx, &s));
}

#[gpui_kit::test]
fn undoing_an_applied_starting_point_turns_start_from_back_on(cx: &mut TestAppContext) {
    let (_env, s, page) = opened(cx);
    start_from(cx, &s, "Streamer clean");
    click_in_panel(cx, &s, ids::HUD_MAKER_PREVIEW_APPLY.into());
    undo(cx, &s, &page);
    assert!(cx.update(|cx| page.read(cx).editor().unwrap().changes().is_empty()));
    assert!(open_more(cx, &s));
}

#[gpui_kit::test]
fn the_bar_says_when_some_changes_only_show_in_tf2(cx: &mut TestAppContext) {
    const TF2_ONLY: &str = "Some of these changes only show in TF2.";
    let (_env, s, _page) = opened(cx);
    start_from(cx, &s, "Streamer clean");
    let bar = support::label_of(cx, s.win, ids::HUD_MAKER_PREVIEW).unwrap();
    assert!(bar.ends_with(TF2_ONLY), "{bar}");
    // The killfeed rows and a hidden portrait both draw on the canvas.
    start_from(cx, &s, "Competitive minimal");
    let bar = support::label_of(cx, s.win, ids::HUD_MAKER_PREVIEW).unwrap();
    assert!(!bar.contains(TF2_ONLY), "{bar}");
}
