//! The Graphics rows are built once per state change and read by every frame after it.

use crate::support;

use gpui_kit::prelude::*;
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{AnyWindowHandle, Entity, TestAppContext};
use support::graphics::{idle, picture_page, saved, value};
use tf2tools_app::pages::graphics::GraphicsPage;
use tf2tools_app::pages::graphics::view::{self, BuiltRows, Filter};
use tf2tools_core::{Choices, Preset};

fn some_choices(catalog: &tf2tools_core::ModuleCatalog) -> Choices {
    let mut choices = Choices::new(Preset::Medium);
    choices.set(catalog, "lod", value("low"));
    choices.set(catalog, "fpscap", value("120"));
    choices
}

// Draws a fresh frame of the page, as a scroll or a hover does.
fn redraw(cx: &mut TestAppContext, win: AnyWindowHandle, page: &Entity<GraphicsPage>) {
    cx.update(|cx| page.update(cx, |_, cx| cx.notify()));
    cx.update_window(win, |_, w, cx| {
        w.refresh();
        w.render_frame(cx);
    })
    .unwrap();
}

#[test]
fn built_rows_read_the_same_as_the_view_functions() {
    let catalog = support::full_module_catalog();
    let choices = some_choices(&catalog);
    let built = BuiltRows::build(Some(&catalog), &choices);
    for filter in [
        Filter::default(),
        Filter {
            query: "detail".into(),
            changed_only: false,
        },
        Filter {
            query: String::new(),
            changed_only: true,
        },
    ] {
        assert_eq!(
            built.sections(&filter),
            view::sections(&catalog, &choices, &filter)
        );
    }
    assert_eq!(built.changes(), view::changes(Some(&catalog), &choices));
    let lod = catalog.module("lod").unwrap();
    assert_eq!(built.row("lod"), Some(&view::row(&catalog, &choices, lod)));
    assert_eq!(built.row("no-such-module"), None);
    let bare = BuiltRows::build(None, &choices);
    assert_eq!(bare.changes(), view::changes(None, &choices));
    assert!(bare.sections(&Filter::default()).is_empty());
}

#[gpui_kit::test]
fn the_page_builds_its_rows_once_per_state_change(cx: &mut TestAppContext) {
    let env = support::env();
    let (s, page) = picture_page(cx, &env, "medium", r#"{"lod":"high"}"#);
    let editor = cx.update(|cx| page.read(cx).choices_editor().clone());
    let generation = |cx: &mut TestAppContext| cx.update(|cx| editor.read(cx).rows_generation());
    let before = generation(cx);
    for _ in 0..5 {
        redraw(cx, s.win, &page);
    }
    assert_eq!(generation(cx), before, "redraws rebuilt the rows");
    support::graphics::pick(cx, &s, &page, "fpscap", value("120"));
    idle(cx, &page);
    support::settle(cx, s.win);
    assert_eq!(saved(cx, &page, "fpscap"), Some(value("120")));
    let after = generation(cx);
    assert!(after > before, "a saved pick rebuilt the rows");
    for _ in 0..5 {
        redraw(cx, s.win, &page);
    }
    assert_eq!(generation(cx), after, "redraws rebuilt the rows");
}
