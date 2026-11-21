//! GraphicsChoicesEditor's presets card and changes strip: edits go out as events, the editor
//! keeps showing what its host gave it, and nothing reaches the game.

use crate::support;

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use gpui_kit::component::v_flex;
use gpui_kit::prelude::*;
use gpui_kit::{AnyWindowHandle, Context, Entity, Subscription, TestAppContext, Window};
use tf2tools_app::ids::{self, IdScope};
use tf2tools_app::pages::graphics::{ChoicesEdit, GraphicsChoicesEditor};
use tf2tools_app::services::Services;
use tf2tools_core::{Choices, ModuleCatalog, ModuleChoice, Preset};

const SCOPE: IdScope = IdScope::PROFILE_GRAPHICS;

type Events = Rc<RefCell<Vec<ChoicesEdit>>>;

struct Host {
    editor: Entity<GraphicsChoicesEditor>,
    _subscriptions: Vec<Subscription>,
}

impl Render for Host {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let (presets, changes) = self
            .editor
            .update(cx, |e, cx| (e.presets_card(false, cx), e.changes_strip(cx)));
        v_flex().size_full().gap_4().child(presets).child(changes)
    }
}

fn phong_off(catalog: &ModuleCatalog) -> Choices {
    let mut choices = Choices::new(Preset::Medium);
    choices.set(catalog, "phong", ModuleChoice::Value("off".to_owned()));
    choices
}

fn open(
    cx: &mut TestAppContext,
    env: &support::Env,
    choices: impl FnOnce(&ModuleCatalog) -> Choices,
) -> (
    AnyWindowHandle,
    Entity<GraphicsChoicesEditor>,
    Events,
    Services,
) {
    support::init(cx);
    let services = support::services(env, support::no_steam());
    support::install_services(cx, services.clone());
    let catalog = Arc::new(support::module_catalog());
    let choices = choices(&catalog);
    let events: Events = Rc::new(RefCell::new(Vec::new()));
    let sink = Rc::clone(&events);
    let (win, host) = cx
        .update(|cx| {
            gpui_kit::open_window(support::window_options(), cx, |window, cx| {
                let editor = cx.new(|cx| {
                    let mut editor = GraphicsChoicesEditor::new(SCOPE, choices, window, cx);
                    editor.set_catalog(Some(catalog), window, cx);
                    editor
                });
                cx.new(|cx| {
                    let edits = cx.subscribe(&editor, move |_, _, e: &ChoicesEdit, _| {
                        sink.borrow_mut().push(e.clone())
                    });
                    let redraw = cx.observe(&editor, |_, _, cx| cx.notify());
                    Host {
                        editor,
                        _subscriptions: vec![edits, redraw],
                    }
                })
            })
        })
        .unwrap();
    let editor = cx.update(|cx| host.read(cx).editor.clone());
    support::settle(cx, win);
    (win, editor, events, services)
}

fn custom_entries(env: &support::Env) -> usize {
    std::fs::read_dir(env.game.join("tf/custom"))
        .unwrap()
        .count()
}

#[gpui_kit::test]
fn picking_another_preset_proposes_it_and_changes_nothing(cx: &mut TestAppContext) {
    let env = support::env();
    let (win, editor, events, services) = open(cx, &env, |_| Choices::new(Preset::Medium));
    let before = custom_entries(&env);
    support::click_id(cx, win, SCOPE.id(ids::graphics_preset(Preset::Low)));
    support::click_id(cx, win, SCOPE.id(ids::graphics_preset(Preset::Medium)));
    assert_eq!(*events.borrow(), vec![ChoicesEdit::Preset(Preset::Low)]);
    assert_eq!(
        cx.update(|cx| editor.read(cx).choices().preset()),
        Preset::Medium,
        "the editor shows what its host gave it until told otherwise"
    );
    assert!(services.library.mastercomfig_choices().unwrap().is_none());
    assert_eq!(custom_entries(&env), before);
}

#[gpui_kit::test]
fn undo_and_reset_all_propose_the_choices_without_the_change(cx: &mut TestAppContext) {
    let env = support::env();
    let (win, _, events, _) = open(cx, &env, phong_off);
    let catalog = support::module_catalog();
    assert_eq!(
        support::label_of(cx, win, SCOPE.id(ids::GRAPHICS_CHANGES)).as_deref(),
        Some("1 change from Medium")
    );
    support::click_id(cx, win, SCOPE.id(ids::graphics_change_undo("phong")));
    support::click_id(cx, win, SCOPE.id(ids::GRAPHICS_RESET_ALL));
    let edit = ChoicesEdit::Modules {
        from: phong_off(&catalog),
        to: Choices::new(Preset::Medium),
    };
    assert_eq!(*events.borrow(), vec![edit.clone(), edit]);
}

#[gpui_kit::test]
fn set_choices_redraws_the_strip(cx: &mut TestAppContext) {
    let env = support::env();
    let (win, editor, _, _) = open(cx, &env, phong_off);
    assert!(support::present(
        cx,
        win,
        SCOPE.id(ids::graphics_change("phong"))
    ));
    cx.update_window(win, |_, window, cx| {
        editor.update(cx, |e, cx| {
            e.set_choices(Choices::new(Preset::High), window, cx)
        })
    })
    .unwrap();
    assert!(!support::present(
        cx,
        win,
        SCOPE.id(ids::graphics_change("phong"))
    ));
    assert!(support::present(
        cx,
        win,
        SCOPE.id(ids::GRAPHICS_NO_CHANGES)
    ));
}

#[gpui_kit::test]
fn a_disabled_editor_proposes_nothing(cx: &mut TestAppContext) {
    let env = support::env();
    let (win, editor, events, _) = open(cx, &env, phong_off);
    cx.update(|cx| editor.update(cx, |e, cx| e.set_disabled(true, cx)));
    support::click_id(cx, win, SCOPE.id(ids::graphics_preset(Preset::High)));
    support::click_id(cx, win, SCOPE.id(ids::GRAPHICS_RESET_ALL));
    assert!(events.borrow().is_empty());
}

#[gpui_kit::test]
fn the_editor_draws_none_of_the_page_ids(cx: &mut TestAppContext) {
    let env = support::env();
    let (win, _, _, _) = open(cx, &env, phong_off);
    assert!(support::present(cx, win, SCOPE.id(ids::GRAPHICS_PRESETS)));
    assert!(!support::present(cx, win, ids::GRAPHICS_PRESETS));
    assert!(!support::present(
        cx,
        win,
        ids::graphics_preset(Preset::Low)
    ));
    assert!(!support::present(cx, win, ids::GRAPHICS_CHANGES));
}
