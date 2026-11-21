//! The header slot: a page's own view in place of the title, only while that page shows.

use crate::support;

use gpui_kit::prelude::*;
use gpui_kit::{
    AnyView, Context, Entity, SharedString, TestAppContext, TestSupportExt as _, Window, div,
};
use support::graphics::found;
use tf2tools_app::ids;
use tf2tools_app::shell::header_slot::HeaderSlot;
use tf2tools_app::shell::route::Page;

const CRUMB: &str = "test-crumb";

struct Crumb {
    text: SharedString,
}

impl Render for Crumb {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .id(CRUMB)
            .aria_label(self.text.clone())
            .test_support()
            .child(self.text.clone())
    }
}

fn put(cx: &mut TestAppContext, page: Page, text: &str) -> Entity<Crumb> {
    let crumb = cx.new(|_| Crumb {
        text: SharedString::from(text.to_owned()),
    });
    let view: AnyView = crumb.clone().into();
    cx.update(|cx| {
        let slot = HeaderSlot::global(cx);
        slot.update(cx, |slot, cx| slot.set(page, view, cx));
    });
    crumb
}

fn clear(cx: &mut TestAppContext, page: Page) {
    cx.update(|cx| {
        let slot = HeaderSlot::global(cx);
        slot.update(cx, |slot, cx| slot.clear(page, cx));
    });
}

#[gpui_kit::test]
fn the_slot_replaces_the_title_and_keeps_the_rest(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    support::click_nav(cx, s.win, Page::Profiles);
    put(cx, Page::Profiles, "Profiles / Comp");
    assert_eq!(
        support::label_of(cx, s.win, CRUMB).as_deref(),
        Some("Profiles / Comp")
    );
    assert!(!support::present(cx, s.win, ids::HEADER_TITLE));
    assert!(support::present(cx, s.win, ids::SIDEBAR_TOGGLE));
    assert!(support::present(cx, s.win, ids::TF2_BADGE));
    let row = support::bounds_of(cx, s.win, ids::HEADER_ROW).unwrap();
    let slot = support::bounds_of(cx, s.win, ids::HEADER_SLOT).unwrap();
    let badge = support::bounds_of(cx, s.win, ids::TF2_BADGE).unwrap();
    assert!(row.contains(&slot.center()), "{slot:?} in {row:?}");
    assert!(slot.right() <= badge.left(), "{slot:?} before {badge:?}");
}

#[gpui_kit::test]
fn the_slot_shows_only_on_its_page(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    put(cx, Page::Profiles, "Profiles / Comp");
    assert!(!support::present(cx, s.win, CRUMB), "HUDs is showing");
    assert!(support::present(cx, s.win, ids::HEADER_TITLE));
    support::click_nav(cx, s.win, Page::Profiles);
    assert!(support::present(cx, s.win, CRUMB));
    support::click_nav(cx, s.win, Page::Huds);
    assert!(!support::present(cx, s.win, CRUMB));
}

#[gpui_kit::test]
fn the_view_redraws_itself_and_clear_brings_the_title_back(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    support::click_nav(cx, s.win, Page::Profiles);
    let crumb = put(cx, Page::Profiles, "Profiles / Comp");
    cx.update(|cx| {
        crumb.update(cx, |c, cx| {
            c.text = "Profiles / Casual".into();
            cx.notify();
        })
    });
    assert_eq!(
        support::label_of(cx, s.win, CRUMB).as_deref(),
        Some("Profiles / Casual")
    );
    clear(cx, Page::Huds);
    assert!(
        support::present(cx, s.win, CRUMB),
        "clearing another page leaves it"
    );
    clear(cx, Page::Profiles);
    assert!(!support::present(cx, s.win, CRUMB));
    assert!(support::present(cx, s.win, ids::HEADER_TITLE));
}
