//! A profile's GraphicsChoicesEditor beside the Graphics page: its pictures open, zoom and
//! wipe on their own, under its own ids. Built on the page's picture fixture (the full
//! catalogue and the repo's setting pictures, where lod has crops: lod/*.zoom.webp).

use crate::support;

use std::sync::Arc;

use gpui_kit::component::v_flex;
use gpui_kit::prelude::*;
use gpui_kit::{AnyWindowHandle, Context, Entity, Subscription, TestAppContext, Window};
use support::graphics::{picture_page, toggle_pictures};
use tf2tools_app::ids::{self, IdScope};
use tf2tools_app::pages::graphics::GraphicsChoicesEditor;
use tf2tools_app::pages::graphics::pictures::PictureIndex;
use tf2tools_core::{Choices, Preset};

const SCOPE: IdScope = IdScope::PROFILE_GRAPHICS;

struct Host {
    editor: Entity<GraphicsChoicesEditor>,
    _redraw: Subscription,
}

impl Render for Host {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let row = self.editor.update(cx, |e, cx| {
            e.want_images(cx);
            e.settings_row("lod", true, false, cx)
        });
        v_flex().size_full().children(row)
    }
}

/// A second window holding a profile-scoped editor on the page's catalogue and pictures.
fn profile_editor(
    cx: &mut TestAppContext,
    s: &support::ShellWindow,
) -> (AnyWindowHandle, Entity<GraphicsChoicesEditor>) {
    let catalog = Arc::new(support::full_module_catalog());
    let dir = s.services.assets.setting_pictures_dir();
    let pictures = Arc::new(PictureIndex::open(&dir));
    let (win, host) = cx
        .update(|cx| {
            gpui_kit::open_window(support::window_options(), cx, |window, cx| {
                let editor = cx.new(|cx| {
                    let medium = Choices::new(Preset::Medium);
                    let mut e = GraphicsChoicesEditor::new(SCOPE, medium, window, cx);
                    e.set_catalog(Some(catalog), window, cx);
                    e.set_pictures(Some(pictures), cx);
                    e
                });
                cx.new(|cx| {
                    let _redraw = cx.observe(&editor, |_, _, cx| cx.notify());
                    Host { editor, _redraw }
                })
            })
        })
        .unwrap();
    support::settle(cx, win);
    (win, cx.update(|cx| host.read(cx).editor.clone()))
}

#[gpui_kit::test]
fn a_second_editor_keeps_its_own_pictures_state(cx: &mut TestAppContext) {
    let env = support::env();
    let (s, page) = picture_page(cx, &env, "medium", r#"{"lod":"high"}"#);
    let (win, editor) = profile_editor(cx, &s);
    let thumb = SCOPE.id(ids::graphics_thumb("lod"));
    assert!(support::present(cx, win, thumb.clone()), "lod has pictures");
    assert!(!support::present(cx, win, ids::graphics_thumb("lod")));
    support::click_id(cx, win, thumb);
    let open = cx.update(|cx| editor.read(cx).open_row().map(str::to_owned));
    assert_eq!(open, Some("lod".to_owned()));
    assert_eq!(
        cx.update(|cx| page.read(cx).open_row().map(str::to_owned)),
        None
    );
    assert!(support::present(
        cx,
        win,
        SCOPE.id(ids::graphics_pictures("lod"))
    ));
    let zoom = SCOPE.id(ids::graphics_zoom("lod"));
    assert!(support::present(cx, win, zoom.clone()), "lod has crops");
    support::click_id(cx, win, zoom);
    assert!(cx.update(|cx| editor.read(cx).zoom()));
    assert!(!cx.update(|cx| page.read(cx).zoom()));
    assert!(!cx.update(|cx| editor.read(cx).wanted_images().is_empty()));
}

#[gpui_kit::test]
fn the_page_s_pictures_state_still_reads_through_its_accessors(cx: &mut TestAppContext) {
    let env = support::env();
    let (s, page) = picture_page(cx, &env, "medium", r#"{"lod":"high"}"#);
    toggle_pictures(cx, &s, "lod");
    let editor = cx.update(|cx| page.read(cx).choices_editor().clone());
    assert_eq!(
        cx.update(|cx| page.read(cx).open_row().map(str::to_owned)),
        Some("lod".to_owned())
    );
    assert_eq!(
        cx.update(|cx| editor.read(cx).open_row().map(str::to_owned)),
        Some("lod".to_owned())
    );
    let page_wants = cx.update(|cx| page.read(cx).wanted_images().to_vec());
    let editor_wants = cx.update(|cx| editor.read(cx).wanted_images().to_vec());
    assert_eq!(page_wants, editor_wants);
    assert_eq!(cx.update(|cx| page.read(cx).wipe(cx)), 50.);
}
