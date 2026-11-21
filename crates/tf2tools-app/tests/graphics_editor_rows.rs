//! GraphicsChoicesEditor's settings rows: controls propose edits, Selects follow the host's
//! choices, and the Graphics page hands out the editor's live Selects.

use crate::support;

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::Arc;

use gpui_kit::component::select::SelectEvent;
use gpui_kit::component::v_flex;
use gpui_kit::prelude::*;
use gpui_kit::{
    AnyElement, AnyWindowHandle, Context, Entity, Subscription, TestAppContext, Window,
};
use support::graphics::{found, idle, open as open_page, pick, value, wait_for_list};
use tf2tools_app::ids::{self, IdScope};
use tf2tools_app::pages::graphics::model::ValueOption;
use tf2tools_app::pages::graphics::{ChoicesEdit, GraphicsChoicesEditor};
use tf2tools_core::{Choices, Preset};

const SCOPE: IdScope = IdScope::PROFILE_GRAPHICS;
const MODULES: [&str; 3] = ["lod", "phong", "fpscap"];

type Events = Rc<RefCell<Vec<ChoicesEdit>>>;

struct Host {
    editor: Entity<GraphicsChoicesEditor>,
    _subscriptions: Vec<Subscription>,
}

impl Render for Host {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let rows: Vec<AnyElement> = self.editor.update(cx, |e, cx| {
            MODULES
                .iter()
                .enumerate()
                .filter_map(|(i, m)| e.settings_row(m, i == 0, false, cx))
                .collect()
        });
        v_flex().size_full().children(rows)
    }
}

fn open(
    cx: &mut TestAppContext,
    env: &support::Env,
) -> (
    AnyWindowHandle,
    Entity<GraphicsChoicesEditor>,
    Events,
    Rc<Cell<usize>>,
) {
    support::init(cx);
    support::install_services(cx, support::services(env, support::no_steam()));
    let catalog = Arc::new(support::module_catalog());
    let events: Events = Rc::new(RefCell::new(Vec::new()));
    let notifies = Rc::new(Cell::new(0));
    let (sink, count) = (Rc::clone(&events), Rc::clone(&notifies));
    let (win, host) = cx
        .update(|cx| {
            gpui_kit::open_window(support::window_options(), cx, |window, cx| {
                let editor = cx.new(|cx| {
                    let medium = Choices::new(Preset::Medium);
                    let mut e = GraphicsChoicesEditor::new(SCOPE, medium, window, cx);
                    e.set_catalog(Some(catalog), window, cx);
                    e
                });
                cx.new(|cx| {
                    let edits = cx.subscribe(&editor, move |_, _, e: &ChoicesEdit, _| {
                        sink.borrow_mut().push(e.clone())
                    });
                    let redraw = cx.observe(&editor, move |_, _, cx| {
                        count.set(count.get() + 1);
                        cx.notify()
                    });
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
    (win, editor, events, notifies)
}

fn medium_with(module: &str, v: &str) -> Choices {
    let mut choices = Choices::new(Preset::Medium);
    choices.set(&support::module_catalog(), module, value(v));
    choices
}

#[gpui_kit::test]
fn a_segment_click_proposes_the_choice_and_changes_nothing_itself(cx: &mut TestAppContext) {
    let env = support::env();
    let (win, editor, events, _) = open(cx, &env);
    let before = std::fs::read_dir(env.game.join("tf/custom"))
        .unwrap()
        .count();
    support::click_id(cx, win, SCOPE.id(ids::graphics_segment("phong", "off")));
    let proposed = ChoicesEdit::Modules {
        from: Choices::new(Preset::Medium),
        to: medium_with("phong", "off"),
    };
    assert_eq!(*events.borrow(), vec![proposed]);
    let shown = cx.update(|cx| editor.read(cx).choices().clone());
    assert_eq!(shown, Choices::new(Preset::Medium));
    assert_eq!(
        std::fs::read_dir(env.game.join("tf/custom"))
            .unwrap()
            .count(),
        before
    );
}

#[gpui_kit::test]
fn a_select_pick_proposes_the_choice(cx: &mut TestAppContext) {
    let env = support::env();
    let (_, editor, events, _) = open(cx, &env);
    let select = cx
        .update(|cx| editor.read(cx).module_select("fpscap"))
        .expect("fpscap is a Select row");
    cx.update(|cx| {
        select.update(cx, |_, cx| {
            cx.emit(SelectEvent::<Vec<ValueOption>>::Confirm(Some(value("60"))))
        })
    });
    let proposed = ChoicesEdit::Modules {
        from: Choices::new(Preset::Medium),
        to: medium_with("fpscap", "60"),
    };
    assert_eq!(*events.borrow(), vec![proposed]);
}

#[gpui_kit::test]
fn set_choices_moves_a_select_back_even_when_unchanged(cx: &mut TestAppContext) {
    let env = support::env();
    let (win, editor, _, _) = open(cx, &env);
    let select = cx
        .update(|cx| editor.read(cx).module_select("fpscap"))
        .unwrap();
    cx.update_window(win, |_, window, cx| {
        select.update(cx, |s, cx| s.set_selected_value(&value("60"), window, cx))
    })
    .unwrap();
    cx.update_window(win, |_, window, cx| {
        editor.update(cx, |e, cx| {
            e.set_choices(Choices::new(Preset::Medium), window, cx)
        })
    })
    .unwrap();
    let shown = cx.update(|cx| select.read(cx).selected_value().cloned());
    assert_eq!(shown, Some(value("400")));
}

#[gpui_kit::test]
fn set_catalog_rebuilds_the_selects_and_notifies(cx: &mut TestAppContext) {
    let env = support::env();
    let (win, editor, _, notifies) = open(cx, &env);
    let old = cx
        .update(|cx| editor.read(cx).module_select("fpscap"))
        .unwrap();
    let before = notifies.get();
    cx.update_window(win, |_, window, cx| {
        let full = Arc::new(support::full_module_catalog());
        editor.update(cx, |e, cx| e.set_catalog(Some(full), window, cx))
    })
    .unwrap();
    support::settle(cx, win);
    assert!(notifies.get() > before);
    let new = cx
        .update(|cx| editor.read(cx).module_select("fpscap"))
        .unwrap();
    assert_ne!(new.entity_id(), old.entity_id());
}

#[gpui_kit::test]
fn a_disabled_row_proposes_nothing(cx: &mut TestAppContext) {
    let env = support::env();
    let (win, editor, events, _) = open(cx, &env);
    cx.update(|cx| editor.update(cx, |e, cx| e.set_disabled(true, cx)));
    support::click_id(cx, win, SCOPE.id(ids::graphics_segment("phong", "off")));
    assert!(events.borrow().is_empty());
}

#[gpui_kit::test]
fn the_page_hands_out_the_editor_s_live_selects(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    support::seed_managed_mastercomfig(&env, "9.100.1", "medium");
    support::seed_mastercomfig_catalog(&env, "9.100.1");
    let page = open_page(cx, &s);
    wait_for_list(cx, &s, &page);
    let live = |cx: &mut TestAppContext| {
        cx.update(|cx| {
            let editor = page.read(cx).choices_editor().clone();
            let from_page = page.read(cx).module_select("fpscap").map(|e| e.entity_id());
            let from_editor = editor
                .read(cx)
                .module_select("fpscap")
                .map(|e| e.entity_id());
            (from_page, from_editor)
        })
    };
    let (from_page, from_editor) = live(cx);
    assert!(from_page.is_some());
    assert_eq!(from_page, from_editor);
    pick(cx, &s, &page, "fpscap", value("120"));
    idle(cx, &page);
    support::settle(cx, s.win);
    let (from_page, from_editor) = live(cx);
    assert_eq!(from_page, from_editor);
    assert_eq!(
        support::graphics::shown(cx, &page, "fpscap"),
        Some(value("120"))
    );
}
