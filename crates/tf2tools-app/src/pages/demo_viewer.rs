//! The Demo Viewer page: opens a local TF2 demo and shows its map in 3D.

use std::sync::Arc;
use std::time::Instant;

use gpui_kit::component::button::Button;
use gpui_kit::component::{ActiveTheme as _, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AppContext as _, Context, Entity, InteractiveElement as _, IntoElement, ParentElement as _,
    Render, SharedString, StatefulInteractiveElement as _, Styled as _, Subscription,
    TestSupportExt as _, Window, div,
};

use crate::i18n::tr;
use crate::ids;
use crate::picker::PickKind;
use crate::services::{Services, tf2_state};
use crate::shell::CONTENT_PADDING;
use crate::shell::route::Page;
use crate::viewer::load::{LoadError, LoadedDemo, load_demo, snapshot_time};
use crate::viewer::viewport::DemoViewport;

/// What the page shows below its caption row.
enum View {
    Empty,
    Loading,
    Failed(SharedString),
    Loaded {
        /// The map's name, with the players' count and moment when they were read.
        caption: SharedString,
        /// Whether the players could not be read, so a note says so.
        players_note: bool,
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
        let snapshots = Arc::clone(&cx.global::<Services>().demo_snapshots);
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
                    crate::crash::catching(|| load_demo(&path, install.as_ref(), &snapshots))
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
        loaded: Result<LoadedDemo, LoadError>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.view = match loaded {
            Err(error) => View::Failed(error.message()),
            Ok(loaded) => {
                // A snapshot with nobody in it counts as no players read.
                let players = loaded.snapshot.as_ref().filter(|s| !s.players().is_empty());
                let caption = match players {
                    Some(snapshot) => tr!(
                        "demo_viewer.caption_players",
                        map = loaded.map,
                        count = snapshot.players().len(),
                        time = snapshot_time(snapshot.tick(), loaded.seconds_per_tick)
                    ),
                    None => tr!("demo_viewer.caption", map = loaded.map),
                };
                let players_note = players.is_none();
                let renderer = (cx.global::<Services>().renderers)();
                let lock = (cx.global::<Services>().pointer_lock)();
                let viewport = cx.new(|cx| {
                    DemoViewport::new(
                        Arc::new(loaded.mesh),
                        loaded.snapshot.as_ref(),
                        renderer,
                        lock,
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
                    caption,
                    players_note,
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
            // Loaded: the caption row names the map beside the controls hint, with a note when
            // the players could not be read, then the viewport fills the rest.
            View::Loaded {
                caption,
                players_note,
                viewport,
                ..
            } => {
                let caption = caption.clone();
                let hint = tr!("demo_viewer.hint");
                let line = h_flex()
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
                    );
                // The note goes under the caption line, so it never pushes Open off the row.
                let text = v_flex().gap_1().child(line).when(*players_note, |text| {
                    let note = tr!("demo_viewer.players_note");
                    text.child(
                        div()
                            .id(ids::DEMO_VIEWER_PLAYERS_NOTE)
                            .aria_label(note.clone())
                            .test_support()
                            .text_sm()
                            .text_color(muted)
                            .child(note),
                    )
                });
                let row = h_flex()
                    .gap_3()
                    .child(text)
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
