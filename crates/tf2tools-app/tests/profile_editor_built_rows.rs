//! The profile editor's Graphics rows are built and cut into sections once per change, and
//! every frame after it only reads them.

use crate::support;

use gpui_kit::prelude::*;
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{AnyWindowHandle, Entity, TestAppContext};
use support::graphics::found;
use support::profiles::{edit, low_fps60, page};
use tf2tools_app::ids::{self, IdScope};
use tf2tools_app::pages::profiles::draft_editor::ProfileDraftEditor;
use tf2tools_core::{Preset, ProfileDraft, SlotMode};

// Draws a fresh frame of the editor, as a scroll or a hover does.
fn redraw(cx: &mut TestAppContext, win: AnyWindowHandle, editor: &Entity<ProfileDraftEditor>) {
    cx.update(|cx| editor.update(cx, |_, cx| cx.notify()));
    cx.update_window(win, |_, w, cx| {
        w.refresh();
        w.render_frame(cx);
    })
    .unwrap();
}

#[gpui_kit::test]
fn the_editor_builds_its_graphics_rows_once_per_change(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    support::seed_managed_mastercomfig(&env, "9.100.1", "medium");
    support::seed_mastercomfig_catalog(&env, "9.100.1");
    let draft = ProfileDraft::new("Potato").with_graphics(SlotMode::Set(low_fps60()));
    let p = s.services.library.create_profile(&draft).unwrap();
    let page = page(cx, &s);
    let e = edit(cx, &s, &page, p.id());
    let gfx = IdScope::PROFILE_GRAPHICS;
    support::wait_real(cx, "the rows", |cx| {
        support::present(cx, s.win, gfx.id(ids::graphics_row("fpscap")))
    });
    support::settle(cx, s.win);
    let counts = |cx: &mut TestAppContext| {
        cx.update(|cx| {
            let editor = e.read(cx);
            let rows = editor.graphics().read(cx).rows_generation();
            (rows, editor.graphics_sections_cut())
        })
    };
    let before = counts(cx);
    for _ in 0..5 {
        redraw(cx, s.win, &e);
    }
    assert_eq!(counts(cx), before, "redraws rebuilt or re-cut the rows");
    support::click_id(cx, s.win, ids::profile_rail("graphics"));
    support::click_id(cx, s.win, gfx.id(ids::graphics_preset(Preset::High)));
    support::settle(cx, s.win);
    let preset = cx.update(|cx| e.read(cx).draft().graphics.preset());
    assert_eq!(preset, Preset::High, "the preset click landed");
    let after = counts(cx);
    assert!(after.0 > before.0, "a preset pick rebuilt the rows");
    assert!(after.1 > before.1, "a preset pick re-cut the sections");
    for _ in 0..5 {
        redraw(cx, s.win, &e);
    }
    assert_eq!(counts(cx), after, "redraws rebuilt or re-cut the rows");
}
