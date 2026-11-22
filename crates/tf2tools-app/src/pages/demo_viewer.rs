//! The Demo Viewer page: opens a local TF2 demo and shows its map in 3D.

use std::path::PathBuf;

use gpui_kit::component::button::Button;
use gpui_kit::component::{ActiveTheme as _, Sizable as _, h_flex, v_flex};
use gpui_kit::{
    Context, InteractiveElement as _, IntoElement, ParentElement as _, Render,
    StatefulInteractiveElement as _, Styled as _, TestSupportExt as _, Window, div,
};

use crate::i18n::tr;
use crate::ids;
use crate::picker::PickKind;
use crate::services::Services;
use crate::shell::CONTENT_PADDING;
use crate::shell::route::Page;

/// The Demo Viewer page; until it loads demos it shows the empty prompt and its Open button.
pub struct DemoViewerPage {
    /// The demo the player last chose, waiting for the loader.
    picked: Option<PathBuf>,
}

impl DemoViewerPage {
    pub fn new(_window: &mut Window, _cx: &mut Context<Self>) -> Self {
        Self { picked: None }
    }

    /// Asks the player for a demo. A cancelled pick changes nothing; a path is recorded for
    /// the loader.
    pub fn open_demo(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let picking = cx.global::<Services>().picker.pick(PickKind::OpenDemo, cx);
        cx.spawn(async move |page, cx| {
            let picked = picking.await;
            let _ = page.update(cx, |page, cx| {
                if let Some(path) = picked {
                    page.picked = Some(path);
                    cx.notify();
                }
            });
        })
        .detach();
    }

    /// The demo the player last chose, if any.
    pub fn picked(&self) -> Option<&std::path::Path> {
        self.picked.as_deref()
    }
}

impl Render for DemoViewerPage {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let muted = cx.theme().muted_foreground;
        let status = tr!("demo_viewer.empty");
        v_flex()
            .id(ids::page(Page::DemoViewer))
            .test_support()
            .size_full()
            .p(CONTENT_PADDING)
            .gap_4()
            // The caption row keeps the small Open button at the right, where the loaded view
            // will name its demo on the left.
            .child(
                h_flex()
                    .id(ids::DEMO_VIEWER_CAPTION)
                    .test_support()
                    .justify_end()
                    .child(
                        Button::new(ids::DEMO_VIEWER_OPEN)
                            .outline()
                            .small()
                            .label(tr!("demo_viewer.open"))
                            .on_click(
                                cx.listener(|page, _, window, cx| page.open_demo(window, cx)),
                            ),
                    ),
            )
            .child(
                div().flex_1().flex().items_center().justify_center().child(
                    div()
                        .id(ids::DEMO_VIEWER_STATUS)
                        .aria_label(status.clone())
                        .test_support()
                        .text_sm()
                        .text_color(muted)
                        .child(status),
                ),
            )
    }
}
