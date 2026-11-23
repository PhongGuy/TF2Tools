//! The Demo Viewer page: opens a local TF2 demo and shows its map in 3D.

use std::sync::Arc;
use std::time::Instant;

use gpui_kit::component::button::Button;
use gpui_kit::component::{ActiveTheme as _, Sizable as _, h_flex, v_flex};
use gpui_kit::{
    AppContext as _, Context, Entity, InteractiveElement as _, IntoElement, ParentElement as _,
    Render, SharedString, StatefulInteractiveElement as _, Styled as _, Subscription,
    TestSupportExt as _, Window, div,
};
use tf2tools_core::MapMesh;

use crate::i18n::tr;
use crate::ids;
use crate::picker::PickKind;
use crate::services::{Services, tf2_state};
use crate::shell::CONTENT_PADDING;
use crate::shell::route::Page;
use crate::viewer::load::{LoadError, load_demo_map};
use crate::viewer::viewport::DemoViewport;

/// What the page shows below its caption row.
enum View {
    Empty,
    Loading,
    Failed(SharedString),
    Loaded {
        map: String,
        viewport: Entity<DemoViewport>,
        _failure_watch: Subscription,
    },
}

/// The Demo Viewer page: an Open button, and the chosen demo's map once it loads.
pub struct DemoViewerPage {
    view: View,
    /// Bumped by every resolved pick, so a slower earlier load cannot land over a later one.
    generation: u64,
}

impl DemoViewerPage {
    pub fn new(_window: &mut Window, _cx: &mut Context<Self>) -> Self {
        Self {
            view: View::Empty,
            generation: 0,
        }
    }

    /// Asks the player for a demo and loads its map off the UI thread. A cancelled pick
    /// changes nothing.
    pub fn open_demo(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let picking = cx.global::<Services>().picker.pick(PickKind::OpenDemo, cx);
        cx.spawn_in(window, async move |page, cx| {
            let Some(path) = picking.await else { return };
            let Ok((generation, install)) = page.update(cx, |page, cx| {
                page.generation += 1;
                page.view = View::Loading;
                cx.notify();
                (page.generation, tf2_state(cx).read(cx).install().cloned())
            }) else {
                return;
            };
            // A panic in the BSP reader is a map that cannot be shown, not a dead app.
            let loaded = cx
                .background_spawn(async move {
                    crate::crash::catching(|| load_demo_map(&path, install.as_ref()))
                        .unwrap_or_else(|payload| {
                            let message = crate::crash::panic_text(&*payload);
                            tracing::warn!("the demo viewer's map loader panicked: {message}");
                            Err(LoadError::MapUnreadable)
                        })
                })
                .await;
            let _ = page.update_in(cx, |page, window, cx| {
                if page.generation == generation {
                    page.show(loaded, window, cx);
                }
            });
        })
        .detach();
    }

    // Puts a loaded map in a fresh viewport; the old one drops with its renderer.
    fn show(
        &mut self,
        loaded: Result<(String, MapMesh), LoadError>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.view = match loaded {
            Err(error) => View::Failed(error.message()),
            Ok((map, mesh)) => {
                let renderer = (cx.global::<Services>().renderers)();
                let viewport = cx.new(|cx| {
                    DemoViewport::new(
                        Arc::new(mesh),
                        None,
                        renderer,
                        Arc::new(Instant::now),
                        window,
                        cx,
                    )
                });
                // A renderer that stops leaves nothing to look at, so its viewport goes too.
                let watch = cx.observe(&viewport, |page, viewport, cx| {
                    if viewport.read(cx).failure().is_some() {
                        page.view = View::Failed(tr!("demo_viewer.error.unsupported"));
                        cx.notify();
                    }
                });
                View::Loaded {
                    map,
                    viewport,
                    _failure_watch: watch,
                }
            }
        };
        cx.notify();
    }
}

impl Render for DemoViewerPage {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let muted = cx.theme().muted_foreground;
        let open = Button::new(ids::DEMO_VIEWER_OPEN)
            .outline()
            .small()
            .label(tr!("demo_viewer.open"))
            .on_click(cx.listener(|page, _, window, cx| page.open_demo(window, cx)));
        let page = v_flex()
            .id(ids::page(Page::DemoViewer))
            .test_support()
            .size_full()
            .p(CONTENT_PADDING)
            .gap_4();
        let status = match &self.view {
            // Loaded: the caption row names the map beside the controls hint, then the
            // viewport fills the rest.
            View::Loaded { map, viewport, .. } => {
                let caption = tr!("demo_viewer.caption", map = map);
                let hint = tr!("demo_viewer.hint");
                let row = h_flex()
                    .gap_3()
                    .child(
                        div()
                            .id(ids::DEMO_VIEWER_CAPTION)
                            .aria_label(caption.clone())
                            .test_support()
                            .text_sm()
                            .child(caption),
                    )
                    .child(
                        div()
                            .id(ids::DEMO_VIEWER_HINT)
                            .aria_label(hint.clone())
                            .test_support()
                            .text_sm()
                            .text_color(muted)
                            .child(hint),
                    )
                    .child(div().flex_1())
                    .child(open);
                return page
                    .child(row)
                    .child(div().flex_1().min_h_0().child(viewport.clone()));
            }
            View::Empty => tr!("demo_viewer.empty"),
            View::Loading => tr!("demo_viewer.loading"),
            View::Failed(message) => message.clone(),
        };
        page.child(h_flex().justify_end().child(open)).child(
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
