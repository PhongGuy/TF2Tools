//! The HUD Maker's Options panel, headless.

use crate::support;

use gpui_kit::component::color_picker::ColorPickerEvent;
use gpui_kit::component::slider::{SliderEvent, SliderValue};
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{
    AppContext as _, Entity, InputEvent as _, Pixels, ScrollDelta, TestAppContext, point, px,
};
use std::cell::Cell;
use std::rc::Rc;
use std::sync::Arc;
use tf2tools_app::ids;
use tf2tools_app::library_events::LibraryEvents;
use tf2tools_app::pages::crosshair_maker::model::hsla_of;
use tf2tools_app::pages::hud_maker::HudMakerPage;
use tf2tools_app::picker::ScriptedPicker;
use tf2tools_app::services::{Services, Tf2State, tf2_state};
use tf2tools_app::shell::route::Page;
use tf2tools_core::{
    MakeTarget, OptionValue, PartKey, Platform, Screen, StartingPoint, Tf2Install,
};

const SCHEME: &str = r#""Scheme" { Colors { TanDark "117 107 94 255" TanLight "235 226 202 255"
  QualityColorNormal "178 178 178 255" QualityColorUnique "255 215 0 255" } }"#;
const BASECHAT: &str = r#""Resource/UI/BaseChat.res" { HudChat { xpos 10 ypos 275 } }"#;
const MAINMENU: &str = r#""Resource/UI/MainMenuOverride.res" { FriendsContainer { visible 1 } }"#;
const HV: &str = "HudPlayerStatus/HudPlayerHealth/PlayerStatusHealthValue";
const VIEWMODELS: &str = "mastercomfig-addon-transparent-viewmodels.vpk";

fn opened(cx: &mut TestAppContext) -> (support::Env, support::ShellWindow, Entity<HudMakerPage>) {
    opened_after(cx, |_, _| {})
}

/// Opens the maker after `seed` ran on the services; the page is built, and the HUD lands,
/// only at the first visit.
fn opened_after(
    cx: &mut TestAppContext,
    seed: impl FnOnce(&mut TestAppContext, &support::Env),
) -> (support::Env, support::ShellWindow, Entity<HudMakerPage>) {
    opened_with_chat(cx, BASECHAT, seed)
}

/// `opened_after` on a HUD whose chat file is `basechat`.
fn opened_with_chat(
    cx: &mut TestAppContext,
    basechat: &str,
    seed: impl FnOnce(&mut TestAppContext, &support::Env),
) -> (support::Env, support::ShellWindow, Entity<HudMakerPage>) {
    let env = support::env();
    support::seed_stock_game_with(
        &env.game,
        &[
            ("resource/clientscheme.res", Some(SCHEME)),
            ("scripts/hudlayout.res", Some(support::hud_stock::HUDLAYOUT)),
            ("resource/ui/basechat.res", Some(basechat)),
            ("resource/ui/mainmenuoverride.res", Some(MAINMENU)),
        ],
    );
    let state = Tf2State::Found(Tf2Install::from_path(&env.game).unwrap());
    let picker = Arc::new(ScriptedPicker::empty());
    let s = support::shell_window_with(cx, &env, state, picker, support::no_steam());
    seed(cx, &env);
    support::click_nav(cx, s.win, Page::HudMaker);
    let page = cx
        .update(|cx| s.shell.read(cx).hud_maker_page())
        .expect("a HUD Maker page");
    support::wait_real(cx, "the HUD to load", |cx| {
        cx.update(|cx| page.read(cx).is_loaded())
    });
    support::settle(cx, s.win);
    (env, s, page)
}

/// Clicks `id` in the side panel, scrolling the panel down until it is wholly in view, so the
/// click never lands on the panel's edge.
fn click_in_panel(cx: &mut TestAppContext, s: &support::ShellWindow, id: gpui_kit::ElementId) {
    for _ in 0..40 {
        let seen = support::present(cx, s.win, id.clone())
            && cx
                .update_window(s.win, |_, w, _| {
                    let body = w.try_find(ids::HUD_MAKER_PANEL_BODY).map(|b| b.bounds());
                    w.try_find(id.clone()).is_some_and(|e| {
                        let b = e.bounds();
                        e.visible() && body.is_none_or(|body| b.bottom() < body.bottom())
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
    support::click_id(cx, s.win, id);
}

fn option(cx: &mut TestAppContext, page: &Entity<HudMakerPage>, id: &str) -> Option<OptionValue> {
    cx.update(|cx| page.read(cx).editor().and_then(|e| e.option_value(id)))
}

#[gpui_kit::test]
fn the_hud_options_show_when_no_part_is_selected(cx: &mut TestAppContext) {
    let (_env, s, _page) = opened(cx);
    assert!(support::present(cx, s.win, ids::HUD_MAKER_OPTIONS));
    assert_eq!(
        support::label_of(cx, s.win, ids::hud_maker_option("chat_top")).as_deref(),
        Some("Chat at the top")
    );
    assert_eq!(
        support::label_of(cx, s.win, ids::hud_maker_option_note("hide_friends")).as_deref(),
        Some("Takes effect after TF2 restarts.")
    );
    assert!(!support::present(
        cx,
        s.win,
        ids::hud_maker_option_note("chat_top")
    ));
}

#[gpui_kit::test]
fn a_switch_turns_an_option_on_and_off(cx: &mut TestAppContext) {
    let (_env, s, page) = opened(cx);
    let chat = ids::hud_maker_option("chat_top");
    click_in_panel(cx, &s, chat.clone());
    assert_eq!(option(cx, &page, "chat_top"), Some(OptionValue::Bool(true)));
    assert_eq!(support::checked_of(cx, s.win, chat.clone()), Some(true));
    click_in_panel(cx, &s, chat.clone());
    assert_eq!(
        option(cx, &page, "chat_top"),
        Some(OptionValue::Bool(false))
    );
    assert!(cx.update(|cx| page.read(cx).editor().unwrap().changes().is_empty()));
}

#[gpui_kit::test]
fn reset_puts_a_group_back(cx: &mut TestAppContext) {
    let (_env, s, page) = opened(cx);
    let reset = ids::hud_maker_option_reset("Main menu");
    assert!(support::disabled_of(cx, s.win, reset.clone()));
    click_in_panel(cx, &s, ids::hud_maker_option("hide_friends"));
    assert!(!support::disabled_of(cx, s.win, reset.clone()));
    click_in_panel(cx, &s, reset);
    assert_eq!(
        option(cx, &page, "hide_friends"),
        Some(OptionValue::Bool(false))
    );
    cx.update_window(s.win, |_, window, cx| {
        page.update(cx, |p, cx| p.undo(window, cx))
    })
    .unwrap();
    support::settle(cx, s.win);
    assert_eq!(
        option(cx, &page, "hide_friends"),
        Some(OptionValue::Bool(true))
    );
    assert_eq!(
        support::checked_of(cx, s.win, ids::hud_maker_option("hide_friends")),
        Some(true)
    );
}

#[gpui_kit::test]
fn the_options_give_way_to_a_selected_part(cx: &mut TestAppContext) {
    let (_env, s, page) = opened(cx);
    cx.update(|cx| {
        page.update(cx, |p, _| {
            p.editor_mut().unwrap().select(Some(PartKey::new(HV)))
        })
    });
    support::settle(cx, s.win);
    assert!(!support::present(cx, s.win, ids::HUD_MAKER_OPTIONS));
}

#[gpui_kit::test]
fn a_switch_the_hud_already_has_on_stays_on(cx: &mut TestAppContext) {
    let top = r#""Resource/UI/BaseChat.res" { HudChat { xpos 10 ypos 0 } }"#;
    let (_env, s, page) = opened_with_chat(cx, top, |_, _| {});
    let chat = ids::hud_maker_option("chat_top");
    assert_eq!(support::checked_of(cx, s.win, chat.clone()), Some(true));
    assert!(support::disabled_of(cx, s.win, chat.clone()));
    click_in_panel(cx, &s, chat);
    assert_eq!(option(cx, &page, "chat_top"), Some(OptionValue::Bool(true)));
    assert!(cx.update(|cx| page.read(cx).editor().unwrap().changes().is_empty()));
}

fn slide(
    cx: &mut TestAppContext,
    s: &support::ShellWindow,
    page: &Entity<HudMakerPage>,
    id: &str,
    event: SliderEvent,
) {
    let slider = cx
        .update(|cx| page.read(cx).option_slider(id))
        .expect("a slider");
    cx.update_window(s.win, |_, window, cx| {
        slider.update(cx, |st, cx| {
            let (SliderEvent::Change(v) | SliderEvent::Release(v)) = event;
            st.set_value(v, window, cx);
            cx.emit(event);
        })
    })
    .unwrap();
    support::settle(cx, s.win);
}

fn sample_lines(cx: &mut TestAppContext, page: &Entity<HudMakerPage>) -> usize {
    cx.update(|cx| {
        let e = page.read(cx).editor().unwrap();
        e.model()
            .death_notices(Screen::new(1920, 1080))
            .map_or(0, |d| d.lines().len())
    })
}

#[gpui_kit::test]
fn a_slider_previews_and_commits_once(cx: &mut TestAppContext) {
    let (_env, s, page) = opened(cx);
    slide(
        cx,
        &s,
        &page,
        "killfeed_rows",
        SliderEvent::Change(SliderValue::Single(6.)),
    );
    slide(
        cx,
        &s,
        &page,
        "killfeed_rows",
        SliderEvent::Change(SliderValue::Single(7.)),
    );
    assert_eq!(
        option(cx, &page, "killfeed_rows"),
        Some(OptionValue::Int(7))
    );
    assert_eq!(sample_lines(cx, &page), 7);
    assert!(cx.update(|cx| page.read(cx).editor().unwrap().is_dragging()));
    assert_eq!(
        support::label_of(cx, s.win, ids::hud_maker_option_value("killfeed_rows")).as_deref(),
        Some("7")
    );
    slide(
        cx,
        &s,
        &page,
        "killfeed_rows",
        SliderEvent::Release(SliderValue::Single(7.)),
    );
    cx.update(|cx| {
        let e = page.read(cx).editor().unwrap();
        assert!(!e.is_dragging() && e.can_undo());
    });
}

#[gpui_kit::test]
fn undo_moves_the_slider_back(cx: &mut TestAppContext) {
    let (_env, s, page) = opened(cx);
    slide(
        cx,
        &s,
        &page,
        "killfeed_rows",
        SliderEvent::Change(SliderValue::Single(6.)),
    );
    slide(
        cx,
        &s,
        &page,
        "killfeed_rows",
        SliderEvent::Release(SliderValue::Single(6.)),
    );
    cx.update_window(s.win, |_, window, cx| {
        page.update(cx, |p, cx| p.undo(window, cx))
    })
    .unwrap();
    support::settle(cx, s.win);
    assert_eq!(
        option(cx, &page, "killfeed_rows"),
        Some(OptionValue::Int(4))
    );
    let slider = cx
        .update(|cx| page.read(cx).option_slider("killfeed_rows"))
        .unwrap();
    let at = cx.update(|cx| slider.read(cx).value());
    assert_eq!(at, SliderValue::Single(4.));
    assert!(!cx.update(|cx| page.read(cx).editor().unwrap().can_undo()));
}

// A replay builds a new change list, so the list's address shows whether one ran.
fn changes_at(cx: &mut TestAppContext, page: &Entity<HudMakerPage>) -> *const () {
    cx.update(|cx| page.read(cx).editor().unwrap().changes().as_ptr().cast())
}

#[gpui_kit::test]
fn a_slide_to_the_same_value_does_not_replay(cx: &mut TestAppContext) {
    let (_env, s, page) = opened(cx);
    let six = || SliderEvent::Change(SliderValue::Single(6.));
    slide(cx, &s, &page, "killfeed_rows", six());
    let before = changes_at(cx, &page);
    slide(cx, &s, &page, "killfeed_rows", six());
    assert_eq!(changes_at(cx, &page), before);
    assert_eq!(
        option(cx, &page, "killfeed_rows"),
        Some(OptionValue::Int(6))
    );
}

#[gpui_kit::test]
fn a_release_back_where_it_started_records_nothing(cx: &mut TestAppContext) {
    let (_env, s, page) = opened(cx);
    let at = |v| SliderValue::Single(v);
    slide(cx, &s, &page, "killfeed_rows", SliderEvent::Change(at(6.)));
    slide(cx, &s, &page, "killfeed_rows", SliderEvent::Change(at(4.)));
    slide(cx, &s, &page, "killfeed_rows", SliderEvent::Release(at(4.)));
    cx.update(|cx| {
        let e = page.read(cx).editor().unwrap();
        assert!(!e.is_dragging() && !e.can_undo() && e.changes().is_empty());
    });
}

#[gpui_kit::test]
fn a_group_reset_moves_the_slider_back(cx: &mut TestAppContext) {
    let (_env, s, page) = opened(cx);
    let at = |v| SliderValue::Single(v);
    slide(cx, &s, &page, "killfeed_rows", SliderEvent::Change(at(6.)));
    slide(cx, &s, &page, "killfeed_rows", SliderEvent::Release(at(6.)));
    click_in_panel(cx, &s, ids::hud_maker_option_reset("HUD"));
    assert_eq!(
        option(cx, &page, "killfeed_rows"),
        Some(OptionValue::Int(4))
    );
    let slider = cx
        .update(|cx| page.read(cx).option_slider("killfeed_rows"))
        .unwrap();
    assert_eq!(cx.update(|cx| slider.read(cx).value()), at(4.));
}

fn pick_option_colour(
    cx: &mut TestAppContext,
    s: &support::ShellWindow,
    page: &Entity<HudMakerPage>,
    rgb: [u8; 3],
) {
    let picker = cx.update(|cx| page.read(cx).option_colour_state().clone());
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
fn a_colour_drag_is_one_undo_step(cx: &mut TestAppContext) {
    let (_env, s, page) = opened(cx);
    assert_eq!(
        support::label_of(cx, s.win, ids::hud_maker_option("item_unique")).as_deref(),
        Some("Unique")
    );
    click_in_panel(cx, &s, ids::hud_maker_option("item_unique"));
    pick_option_colour(cx, &s, &page, [200, 0, 0]);
    pick_option_colour(cx, &s, &page, [0, 0, 200]);
    assert_eq!(
        option(cx, &page, "item_unique"),
        Some(OptionValue::Rgba([0, 0, 200, 255]))
    );
    let picker = cx.update(|cx| page.read(cx).option_colour_state().clone());
    cx.update_window(s.win, |_, _, cx| {
        picker.update(cx, |st, cx| st.set_open(false, cx))
    })
    .unwrap();
    support::settle(cx, s.win);
    cx.update_window(s.win, |_, window, cx| {
        page.update(cx, |p, cx| p.undo(window, cx))
    })
    .unwrap();
    support::settle(cx, s.win);
    assert_eq!(
        option(cx, &page, "item_unique"),
        Some(OptionValue::Rgba([255, 215, 0, 255]))
    );
    assert!(!cx.update(|cx| page.read(cx).editor().unwrap().can_undo()));
}

#[gpui_kit::test]
fn a_second_swatch_keeps_the_first_colour(cx: &mut TestAppContext) {
    let (_env, s, page) = opened(cx);
    click_in_panel(cx, &s, ids::hud_maker_option("item_unique"));
    pick_option_colour(cx, &s, &page, [1, 2, 3]);
    click_in_panel(cx, &s, ids::hud_maker_option("item_normal"));
    pick_option_colour(cx, &s, &page, [4, 5, 6]);
    assert_eq!(
        option(cx, &page, "item_unique"),
        Some(OptionValue::Rgba([1, 2, 3, 255]))
    );
    assert_eq!(
        option(cx, &page, "item_normal"),
        Some(OptionValue::Rgba([4, 5, 6, 255]))
    );
    // The first colour is its own undo step: undoing the second leaves it in place.
    undo(cx, &s, &page);
    assert_eq!(
        option(cx, &page, "item_normal"),
        Some(OptionValue::Rgba([178, 178, 178, 255]))
    );
    assert_eq!(
        option(cx, &page, "item_unique"),
        Some(OptionValue::Rgba([1, 2, 3, 255]))
    );
    undo(cx, &s, &page);
    assert_eq!(
        option(cx, &page, "item_unique"),
        Some(OptionValue::Rgba([255, 215, 0, 255]))
    );
}

fn undo(cx: &mut TestAppContext, s: &support::ShellWindow, page: &Entity<HudMakerPage>) {
    cx.update_window(s.win, |_, window, cx| {
        page.update(cx, |p, cx| p.undo(window, cx))
    })
    .unwrap();
    support::settle(cx, s.win);
}

#[gpui_kit::test]
fn undo_with_the_picker_open_shows_the_colour_back(cx: &mut TestAppContext) {
    let (_env, s, page) = opened(cx);
    click_in_panel(cx, &s, ids::hud_maker_option("item_unique"));
    let picker = cx.update(|cx| page.read(cx).option_colour_state().clone());
    cx.update_window(s.win, |_, window, cx| {
        picker.update(cx, |st, cx| {
            st.update_color(hsla_of([200, 0, 0]), window, cx)
        })
    })
    .unwrap();
    support::settle(cx, s.win);
    undo(cx, &s, &page);
    assert_eq!(
        option(cx, &page, "item_unique"),
        Some(OptionValue::Rgba([255, 215, 0, 255]))
    );
    let shown = cx.update(|cx| picker.read(cx).displayed_color());
    assert_eq!(shown, Some(hsla_of([255, 215, 0])));
}

#[gpui_kit::test]
fn a_hud_that_lands_closes_the_option_picker(cx: &mut TestAppContext) {
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
    click_in_panel(cx, &s, ids::hud_maker_option("item_unique"));
    let picker = cx.update(|cx| page.read(cx).option_colour_state().clone());
    assert!(cx.update(|cx| picker.read(cx).is_open()));
    cx.update(|cx| page.update(cx, |p, cx| p.open(made.entry().id().clone(), cx)));
    support::wait_real(cx, "the made HUD to load", |cx| {
        cx.update(|cx| page.read(cx).is_loaded())
    });
    support::settle(cx, s.win);
    assert!(!cx.update(|cx| picker.read(cx).is_open()));
    // A late pick from the old picker reaches no option of the new HUD.
    pick_option_colour(cx, &s, &page, [9, 9, 9]);
    assert_eq!(
        option(cx, &page, "item_unique"),
        Some(OptionValue::Rgba([255, 215, 0, 255]))
    );
}

#[gpui_kit::test]
fn the_item_colours_say_restart_once(cx: &mut TestAppContext) {
    let (_env, s, _page) = opened(cx);
    assert_eq!(
        support::label_of(cx, s.win, ids::hud_maker_option_group_note("Item colours")).as_deref(),
        Some("Takes effect after TF2 restarts.")
    );
    assert!(!support::present(
        cx,
        s.win,
        ids::hud_maker_option_note("item_unique")
    ));
}

#[gpui_kit::test]
fn without_the_addon_the_row_says_where_to_get_it(cx: &mut TestAppContext) {
    let (_env, s, _page) = opened(cx);
    assert_eq!(
        support::label_of(cx, s.win, ids::HUD_MAKER_VIEWMODELS).as_deref(),
        Some("Get Transparent Viewmodels from the Addon store first.")
    );
}

/// Opens the maker with the viewmodels addon in the library, not installed, once its switch shows.
fn opened_with_viewmodels(
    cx: &mut TestAppContext,
) -> (support::Env, support::ShellWindow, Entity<HudMakerPage>) {
    let (env, s, page) = opened_after(cx, |cx, env| {
        let vpk = support::vpk_bytes(&[("cfg/addons/x.cfg", b"v1".as_slice())]);
        cx.update(|cx| {
            support::seed_library_addon(
                cx.global::<Services>(),
                env,
                VIEWMODELS,
                "9.0.0",
                &vpk,
                false,
            )
        });
    });
    support::wait_real(cx, "the addon state", |cx| {
        support::checked_of(cx, s.win, ids::HUD_MAKER_VIEWMODELS).is_some()
    });
    (env, s, page)
}

#[gpui_kit::test]
fn viewmodels_act_at_once_and_are_not_stored(cx: &mut TestAppContext) {
    let (env, s, page) = opened_with_viewmodels(cx);
    assert_eq!(
        support::checked_of(cx, s.win, ids::HUD_MAKER_VIEWMODELS),
        Some(false)
    );
    click_in_panel(cx, &s, ids::HUD_MAKER_VIEWMODELS.into());
    let installed = env.game.join("tf/custom").join(VIEWMODELS);
    support::wait_real(cx, "the addon to install", |_| installed.is_file());
    support::settle(cx, s.win);
    assert_eq!(
        support::checked_of(cx, s.win, ids::HUD_MAKER_VIEWMODELS),
        Some(true)
    );
    assert!(cx.update(|cx| page.read(cx).editor().unwrap().changes().is_empty()));
    click_in_panel(cx, &s, ids::HUD_MAKER_VIEWMODELS.into());
    support::wait_real(cx, "the addon to uninstall", |_| !installed.is_file());
}

// A library change while the switch's job runs must not hand the switch back before that job
// lands, or a second click could start a second job. The seeds vary which job runs first.
#[gpui_kit::test(iterations = 8)]
fn a_library_change_during_a_toggle_keeps_the_switch_held(cx: &mut TestAppContext) {
    let (env, s, page) = opened_with_viewmodels(cx);
    let installed = env.game.join("tf/custom").join(VIEWMODELS);
    let released_early = Rc::new(Cell::new(false));
    let sent = Rc::new(Cell::new(false));
    let _watch = cx.update(|cx| {
        let (early, sent, installed) = (released_early.clone(), sent.clone(), installed.clone());
        cx.observe(&page, move |page, cx| {
            let held = page.read(cx).viewmodels_held();
            if held && !sent.replace(true) {
                LibraryEvents::changed(cx);
            }
            if sent.get() && !held && !installed.is_file() {
                early.set(true);
            }
        })
    });
    click_in_panel(cx, &s, ids::HUD_MAKER_VIEWMODELS.into());
    support::wait_real(cx, "the addon to install", |_| installed.is_file());
    support::settle(cx, s.win);
    assert!(sent.get(), "the toggle never held the switch");
    assert!(
        !released_early.get(),
        "the switch came back before the install landed"
    );
    assert_eq!(
        support::checked_of(cx, s.win, ids::HUD_MAKER_VIEWMODELS),
        Some(true)
    );
}

#[gpui_kit::test]
fn without_tf2_the_viewmodels_row_says_so(cx: &mut TestAppContext) {
    let (_env, s, _page) = opened_with_viewmodels(cx);
    cx.update(|cx| {
        tf2_state(cx).update(cx, |state, cx| {
            *state = Tf2State::Missing { saved: None };
            cx.notify();
        });
        LibraryEvents::changed(cx);
    });
    support::wait_real(cx, "the row to say TF2 is missing", |cx| {
        support::label_of(cx, s.win, ids::HUD_MAKER_VIEWMODELS).as_deref()
            == Some(
                "TF2 wasn't found. Set the TF2 folder in Settings to use Transparent viewmodels.",
            )
    });
}

/// Moves, presses and releases the pointer at `at`, as a player's click does.
fn click_point(cx: &mut TestAppContext, s: &support::ShellWindow, at: gpui_kit::Point<Pixels>) {
    let left = gpui_kit::MouseButton::Left;
    cx.update_window(s.win, |_, w, cx| {
        let events = [
            gpui_kit::MouseMoveEvent {
                position: at,
                pressed_button: None,
                modifiers: Default::default(),
            }
            .to_platform_input(),
            gpui_kit::MouseDownEvent {
                button: left,
                position: at,
                modifiers: Default::default(),
                click_count: 1,
                first_mouse: false,
            }
            .to_platform_input(),
            gpui_kit::MouseUpEvent {
                button: left,
                position: at,
                modifiers: Default::default(),
                click_count: 1,
            }
            .to_platform_input(),
        ];
        for event in events {
            let _ = w.dispatch_event(event, cx);
            w.render_frame(cx);
        }
    })
    .unwrap();
    support::settle(cx, s.win);
}

/// Clicks the open picker's first featured swatch, which sits under the Palette tab bar; the
/// kit gives swatches no findable id.
fn click_first_palette_swatch(cx: &mut TestAppContext, s: &support::ShellWindow) {
    support::settle(cx, s.win);
    let tab = cx
        .update_window(s.win, |_, w, _| {
            w.within("popover")
                .try_find(gpui_kit::ElementId::Integer(0))
                .map(|e| e.bounds())
        })
        .unwrap()
        .expect("the picker's Palette tab");
    click_point(
        cx,
        s,
        tab.origin + point(px(10.), tab.size.height + px(22.)),
    );
}

#[gpui_kit::test]
fn a_palette_pick_applies_the_option_colour(cx: &mut TestAppContext) {
    let (_env, s, page) = opened(cx);
    click_in_panel(cx, &s, ids::hud_maker_option("item_unique"));
    let picker = cx.update(|cx| page.read(cx).option_colour_state().clone());
    click_first_palette_swatch(cx, &s);
    assert!(!cx.update(|cx| picker.read(cx).is_open()));
    let picked = cx
        .update(|cx| picker.read(cx).value())
        .expect("a picked colour");
    let rgb = tf2tools_app::pages::crosshair_maker::model::rgb_of(picked);
    assert_ne!(rgb, [255, 215, 0]);
    assert_eq!(
        option(cx, &page, "item_unique"),
        Some(OptionValue::Rgba([rgb[0], rgb[1], rgb[2], 255]))
    );
    // The pick is one undo step of its own.
    undo(cx, &s, &page);
    assert_eq!(
        option(cx, &page, "item_unique"),
        Some(OptionValue::Rgba([255, 215, 0, 255]))
    );
}

#[gpui_kit::test]
fn a_typed_hex_applies_the_option_colour(cx: &mut TestAppContext) {
    let (_env, s, page) = opened(cx);
    click_in_panel(cx, &s, ids::hud_maker_option("item_unique"));
    let picker = cx.update(|cx| page.read(cx).option_colour_state().clone());
    // The player types into the picker's hex field and presses Enter.
    let hex = cx.update(|cx| picker.read(cx).hex_input().clone());
    cx.update_window(s.win, |_, window, cx| {
        hex.update(cx, |st, cx| {
            st.focus(window, cx);
            st.set_value("#0000c8", window, cx);
        })
    })
    .unwrap();
    support::press(cx, s.win, "enter");
    assert!(!cx.update(|cx| picker.read(cx).is_open()));
    assert_eq!(
        option(cx, &page, "item_unique"),
        Some(OptionValue::Rgba([0, 0, 200, 255]))
    );
}

/// Previews starting point `which` as the page does for a pick.
fn preview_start(
    cx: &mut TestAppContext,
    s: &support::ShellWindow,
    page: &Entity<HudMakerPage>,
    which: StartingPoint,
) {
    cx.update_window(s.win, |_, window, cx| {
        page.update(cx, |p, cx| {
            let recipe = p.editor().unwrap().pristine().starting_point(which);
            p.start_preview(recipe, 0, window, cx);
        })
    })
    .unwrap();
    support::settle(cx, s.win);
}

fn previewing(cx: &mut TestAppContext, page: &Entity<HudMakerPage>) -> bool {
    cx.update(|cx| page.read(cx).editor().unwrap().previewing())
}

#[gpui_kit::test]
fn escape_closes_an_open_option_picker(cx: &mut TestAppContext) {
    let (_env, s, page) = opened(cx);
    click_in_panel(cx, &s, ids::hud_maker_option("item_unique"));
    let picker = cx.update(|cx| page.read(cx).option_colour_state().clone());
    assert!(cx.update(|cx| picker.read(cx).is_open()));
    support::press(cx, s.win, "escape");
    assert!(!cx.update(|cx| picker.read(cx).is_open()));
}

#[gpui_kit::test]
fn an_open_option_picker_leaves_the_swatches_where_they_were_at_800x600(cx: &mut TestAppContext) {
    let (_env, s, page) = opened(cx);
    support::maker_page::float_panels(cx, &s, &page);
    cx.simulate_window_resize(s.win, gpui_kit::size(px(800.), px(600.)));
    support::settle(cx, s.win);
    // Brings the grid into view, then measures every swatch before the picker opens.
    click_in_panel(cx, &s, ids::hud_maker_option("item_strange"));
    let picker = cx.update(|cx| page.read(cx).option_colour_state().clone());
    cx.update(|cx| picker.update(cx, |st, cx| st.set_open(false, cx)));
    support::settle(cx, s.win);
    let swatches = ["item_normal", "item_unique", "item_strange", "item_vintage"];
    let at = |cx: &mut TestAppContext| {
        swatches.map(|id| support::bounds_of(cx, s.win, ids::hud_maker_option(id)).unwrap())
    };
    let before = at(cx);
    support::click_id(cx, s.win, ids::hud_maker_option("item_unique"));
    assert!(cx.update(|cx| picker.read(cx).is_open()));
    assert_eq!(at(cx), before, "the open picker moved the swatches");
}

#[gpui_kit::test]
fn a_click_on_an_open_swatchs_corner_keeps_its_picker(cx: &mut TestAppContext) {
    let (_env, s, page) = opened(cx);
    let unique = ids::hud_maker_option("item_unique");
    click_in_panel(cx, &s, unique.clone());
    let picker = cx.update(|cx| page.read(cx).option_colour_state().clone());
    assert!(cx.update(|cx| picker.read(cx).is_open()));
    // The picker's own trigger takes no room on the swatch.
    let holder = support::bounds_of(cx, s.win, ids::HUD_MAKER_OPTION_PICKER).unwrap();
    assert_eq!((holder.size.width, holder.size.height), (px(0.), px(0.)));
    // The lower-left corner is the swatch's too: it chooses the colour, as anywhere else on it.
    let b = support::bounds_of(cx, s.win, unique.clone()).unwrap();
    let corner = point(px(3.), b.size.height - px(3.));
    cx.update_window(s.win, |_, w, cx| w.click_at(unique, corner, cx))
        .unwrap();
    support::settle(cx, s.win);
    assert!(cx.update(|cx| picker.read(cx).is_open()));
}

#[gpui_kit::test]
fn the_switches_show_a_preview_and_wait_for_it(cx: &mut TestAppContext) {
    let (_env, s, page) = opened(cx);
    preview_start(cx, &s, &page, StartingPoint::StreamerClean);
    let chat = ids::hud_maker_option("chat_top");
    assert_eq!(support::checked_of(cx, s.win, chat.clone()), Some(true));
    assert!(support::disabled_of(cx, s.win, chat.clone()));
    click_in_panel(cx, &s, chat.clone());
    assert!(
        previewing(cx, &page),
        "a click on a held switch leaves the preview running"
    );
    // The colour swatches wait too: a press opens no picker.
    click_in_panel(cx, &s, ids::hud_maker_option("item_unique"));
    let picker = cx.update(|cx| page.read(cx).option_colour_state().clone());
    assert!(!cx.update(|cx| picker.read(cx).is_open()));
    assert!(previewing(cx, &page));
    support::click_id(cx, s.win, ids::HUD_MAKER_PREVIEW_APPLY);
    assert_eq!(support::checked_of(cx, s.win, chat.clone()), Some(true));
    assert!(!support::disabled_of(cx, s.win, chat));
}

#[gpui_kit::test]
fn the_slider_shows_a_preview_and_comes_back_on_cancel(cx: &mut TestAppContext) {
    let (_env, s, page) = opened(cx);
    let slider = cx
        .update(|cx| page.read(cx).option_slider("killfeed_rows"))
        .expect("a slider");
    let shown = |cx: &mut TestAppContext| {
        support::label_of(cx, s.win, ids::hud_maker_option_value("killfeed_rows"))
    };
    assert_eq!(shown(cx).as_deref(), Some("4"));
    // A choice in the group makes its Reset live, until a preview holds the controls.
    click_in_panel(cx, &s, ids::hud_maker_option("chat_top"));
    let reset = ids::hud_maker_option_reset("HUD");
    assert!(!support::disabled_of(cx, s.win, reset.clone()));
    preview_start(cx, &s, &page, StartingPoint::CompetitiveMinimal);
    assert_eq!(shown(cx).as_deref(), Some("3"));
    assert_eq!(
        cx.update(|cx| slider.read(cx).value()),
        SliderValue::from(3.)
    );
    assert!(support::disabled_of(cx, s.win, reset.clone()));
    support::click_id(cx, s.win, ids::HUD_MAKER_PREVIEW_CANCEL);
    assert_eq!(shown(cx).as_deref(), Some("4"));
    assert_eq!(
        cx.update(|cx| slider.read(cx).value()),
        SliderValue::from(4.)
    );
    assert!(!support::disabled_of(cx, s.win, reset));
}

/// Selects `key` from its row in the parts panel, opened for the click and closed after it.
fn select_row(cx: &mut TestAppContext, s: &support::ShellWindow, key: &str) {
    let toggle = || ids::HUD_MAKER_PARTS_PANEL_COLLAPSE;
    if !support::present(cx, s.win, ids::HUD_MAKER_PARTS) {
        support::click_id(cx, s.win, toggle());
    }
    support::click_part_row(cx, s.win, &PartKey::new(key));
    if support::present(cx, s.win, ids::HUD_MAKER_PARTS) {
        support::click_id(cx, s.win, toggle());
    }
}

fn selected(cx: &mut TestAppContext, page: &Entity<HudMakerPage>) -> Option<PartKey> {
    cx.update(|cx| page.read(cx).editor().unwrap().selected().cloned())
}

#[gpui_kit::test]
fn a_selected_part_opens_at_the_top_of_the_panel(cx: &mut TestAppContext) {
    let (_env, s, page) = opened(cx);
    let body = || ids::HUD_MAKER_PANEL_BODY;
    let down = ScrollDelta::Lines(point(0., -40.));
    cx.update_window(s.win, |_, w, cx| w.scroll(body(), down, cx))
        .unwrap();
    let top = support::bounds_of(cx, s.win, body()).unwrap().top();
    let note = support::bounds_of(cx, s.win, ids::HUD_MAKER_PANEL_EMPTY).unwrap();
    assert!(note.top() < top, "the options scrolled");
    select_row(cx, &s, HV);
    assert_eq!(selected(cx, &page), Some(PartKey::new(HV)));
    let x = support::bounds_of(cx, s.win, ids::HUD_MAKER_X).unwrap();
    assert!(x.top() >= top, "X at {:?}, the body at {top:?}", x.top());
}

#[gpui_kit::test]
fn escape_deselects_the_part(cx: &mut TestAppContext) {
    let (_env, s, page) = opened(cx);
    select_row(cx, &s, HV);
    assert!(selected(cx, &page).is_some());
    support::press(cx, s.win, "escape");
    assert_eq!(selected(cx, &page), None);
    assert!(support::present(cx, s.win, ids::HUD_MAKER_OPTIONS));
}
