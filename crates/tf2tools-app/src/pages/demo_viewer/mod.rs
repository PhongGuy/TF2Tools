//! The Demo Viewer page: opens a local TF2 demo and shows its map in 3D.

use std::cell::Cell;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Instant;

use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::{ActiveTheme as _, Icon, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AppContext as _, Context, CursorStyle, Entity, Hsla, InteractiveElement as _, IntoElement,
    ParentElement as _, Pixels, Render, SharedString, Size, StatefulInteractiveElement as _,
    Styled as _, Subscription, TestSupportExt as _, Window, canvas, div, px,
};

use crate::i18n::tr;
use crate::ids;
use crate::picker::PickKind;
use crate::services::{Services, tf2_state};
use crate::shell::route::Page;
use crate::viewer::load::{LoadError, LoadedDemo, load_demo, snapshot_time};
use crate::viewer::viewport::DemoViewport;
use crate::widgets::Named as _;

mod bar;

pub use bar::BarPlace;

/// What the page shows under its control bar.
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

/// The Demo Viewer page: the chosen demo's map over the whole page, under a floating control bar.
pub struct DemoViewerPage {
    view: View,
    /// Bumped by every resolved pick, so a slower earlier load cannot land over a later one.
    generation: u64,
    /// The page's and the bar's sizes from the last frame, which place the bar.
    area: Rc<Cell<Size<Pixels>>>,
    bar_size: Rc<Cell<Size<Pixels>>>,
}

impl DemoViewerPage {
    pub fn new(_window: &mut Window, _cx: &mut Context<Self>) -> Self {
        Self {
            view: View::Empty,
            generation: 0,
            area: Rc::default(),
            bar_size: Rc::default(),
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
                let viewport = cx.new(|cx| {
                    DemoViewport::new(
                        Arc::new(loaded.mesh),
                        loaded.snapshot.as_ref(),
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
        let text = |id: &'static str, words: SharedString, muted: Option<Hsla>| {
            div()
                .id(id)
                .aria_label(words.clone())
                .test_support()
                .flex_none()
                .whitespace_nowrap()
                .text_sm()
                .when_some(muted, |el, colour| el.text_color(colour))
                .child(words)
        };
        // The viewport fills the whole page; without one, the status sits in its centre.
        let status = |words: SharedString| {
            let centre = div().size_full().flex().items_center().justify_center();
            let line = text(ids::DEMO_VIEWER_STATUS, words, Some(muted));
            (centre.child(line).into_any_element(), None)
        };
        let (body, caption) = match &self.view {
            View::Loaded {
                caption,
                players_note,
                viewport,
                ..
            } => (
                viewport.clone().into_any_element(),
                Some((caption.clone(), *players_note)),
            ),
            View::Empty => status(tr!("demo_viewer.empty")),
            View::Loading => status(tr!("demo_viewer.loading")),
            View::Failed(message) => status(message.clone()),
        };
        let open = Button::new(ids::DEMO_VIEWER_OPEN)
            .outline()
            .small()
            .label(tr!("demo_viewer.open"))
            .on_click(cx.listener(|page, _, window, cx| page.open_demo(window, cx)));
        let grip = div()
            .id(ids::DEMO_VIEWER_BAR_GRIP)
            .aria_label(tr!("demo_viewer.bar_grip"))
            .test_support()
            .flex_none()
            .cursor(CursorStyle::OpenHand)
            .text_color(muted)
            .child(Icon::new(IconName::GripVertical).small());
        let collapse = Button::new(ids::DEMO_VIEWER_BAR_COLLAPSE)
            .ghost()
            .small()
            .icon(IconName::ChevronDown)
            .named(tr!("demo_viewer.bar_collapse"));

        // Both watchers keep last frame's sizes and ask for one more frame when either moves,
        // so the bar settles at its place a frame after it first draws.
        let area = Rc::clone(&self.area);
        let area_watch = canvas(
            move |bounds, window, _| {
                if area.get() != bounds.size {
                    area.set(bounds.size);
                    window.request_animation_frame();
                }
            },
            |_, _, _, _| {},
        )
        .absolute()
        .size_full();
        let bar_size = Rc::clone(&self.bar_size);
        let bar_watch = canvas(
            move |inner, window, _| {
                // The watcher fills the inside of the 1 px border; the bar is that much bigger.
                let size = inner.dilate(px(1.)).size;
                if bar_size.get() != size {
                    bar_size.set(size);
                    window.request_animation_frame();
                }
            },
            |_, _, _, _| {},
        )
        .absolute()
        .size_full();
        let at = BarPlace::DEFAULT.clamp(self.area.get(), self.bar_size.get());
        let theme = cx.theme();
        let bar = h_flex()
            .id(ids::DEMO_VIEWER_BAR)
            .test_support()
            .occlude()
            .absolute()
            .left(at.x)
            .top(at.y)
            .flex_none()
            .items_center()
            .gap_3()
            .pl_2()
            .pr_1()
            .py_1()
            .rounded_md()
            .border_1()
            .border_color(theme.border)
            .bg(theme.background)
            .shadow_md()
            .child(bar_watch)
            .child(grip)
            // The note stands under the caption, so the row stays narrow enough for the page.
            .when_some(caption.clone(), |bar, (caption, note)| {
                let words = v_flex()
                    .flex_none()
                    .child(text(ids::DEMO_VIEWER_CAPTION, caption, None))
                    .when(note, |words| {
                        let note = tr!("demo_viewer.players_note");
                        words.child(text(ids::DEMO_VIEWER_PLAYERS_NOTE, note, Some(muted)))
                    });
                bar.child(words)
            })
            .child(open)
            .when(caption.is_some(), |bar| {
                bar.child(text(
                    ids::DEMO_VIEWER_HINT,
                    tr!("demo_viewer.hint"),
                    Some(muted),
                ))
            })
            .child(collapse);
        div()
            .id(ids::page(Page::DemoViewer))
            .test_support()
            .relative()
            .size_full()
            .child(area_watch)
            .child(body)
            .child(bar)
    }
}
