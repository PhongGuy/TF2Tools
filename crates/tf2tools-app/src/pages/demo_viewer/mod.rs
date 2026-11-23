//! The Demo Viewer page: opens a local TF2 demo and shows its map in 3D.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::Arc;
use std::time::Instant;

use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::{ActiveTheme as _, Icon, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AppContext as _, Context, CursorStyle, DispatchPhase, Entity, Hsla, InteractiveElement as _,
    IntoElement, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, ParentElement as _,
    Pixels, Point, Render, SharedString, Size, StatefulInteractiveElement as _, Styled as _,
    Subscription, TestSupportExt as _, Window, canvas, div, px,
};

use crate::i18n::tr;
use crate::ids;
use crate::local_messages::describe_settings;
use crate::picker::PickKind;
use crate::services::{Services, run_blocking, tf2_state};
use crate::shell::route::Page;
use crate::shell::toast::toast_error;
use crate::viewer::load::{LoadError, LoadedDemo, load_demo, snapshot_time};
use crate::viewer::viewport::DemoViewport;
use crate::widgets::Named as _;

mod bar;

pub use bar::BarPlace;

/// The bar's frame is the page width less this at each side; a bar dragged to an edge can
/// still touch it.
const BAR_MARGIN: Pixels = px(8.);

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

/// A grip drag under way: where the pointer and the bar's centre were when it began, and the
/// place it began from, so a press that moves nothing saves nothing.
#[derive(Clone, Copy)]
struct BarDrag {
    pointer: Point<Pixels>,
    centre: Point<Pixels>,
    start: BarPlace,
}

/// The newest bar setting waiting to be written, and whether a write is running.
#[derive(Default)]
struct BarSaves {
    latest: Option<String>,
    writing: bool,
}

/// The Demo Viewer page: the chosen demo's map over the whole page, under a floating control bar.
pub struct DemoViewerPage {
    view: View,
    /// Bumped by every resolved pick, so a slower earlier load cannot land over a later one.
    generation: u64,
    /// The page's size from the last frame, which places and caps the bar.
    area: Rc<Cell<Size<Pixels>>>,
    /// The bar's own size from the last frame, which the clamp keeps inside the page.
    bar_size: Rc<Cell<Size<Pixels>>>,
    /// The player's place for the bar, as stored; drawing clamps it but never writes back.
    bar: BarPlace,
    /// Set once the player moves or minimises the bar, so a slow stored place cannot undo it.
    bar_touched: bool,
    bar_drag: Option<BarDrag>,
    bar_saves: Rc<RefCell<BarSaves>>,
}

impl DemoViewerPage {
    pub fn new(_window: &mut Window, cx: &mut Context<Self>) -> Self {
        // A missing, damaged or unreadable place leaves the bar at its default.
        let stored = run_blocking(cx, |s| {
            s.db.settings()
                .map(|st| st.demo_viewer_bar().and_then(BarPlace::from_setting))
                .map_err(|e| describe_settings(&e).into_op(tr!("messages.action.load_bar_place")))
        });
        cx.spawn(async move |page, cx| {
            if let Ok(Some(place)) = stored.await {
                let _ = page.update(cx, |page, cx| {
                    if !page.bar_touched {
                        page.bar = place;
                        cx.notify();
                    }
                });
            }
        })
        .detach();
        Self {
            view: View::Empty,
            generation: 0,
            area: Rc::default(),
            bar_size: Rc::default(),
            bar: BarPlace::DEFAULT,
            bar_touched: false,
            bar_drag: None,
            bar_saves: Rc::default(),
        }
    }

    /// The viewport over the loaded map, if one shows.
    pub fn viewport(&self) -> Option<&Entity<DemoViewport>> {
        match &self.view {
            View::Loaded { viewport, .. } => Some(viewport),
            View::Empty | View::Loading | View::Failed(_) => None,
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

    // The drag starts from the bar's centre as drawn, so a bar pulled inside the page by the
    // clamp does not jump when grabbed.
    fn start_bar_drag(&mut self, ev: &MouseDownEvent, cx: &mut Context<Self>) {
        let size = self.bar_size.get();
        let at = self.bar.clamp(self.area.get(), size);
        let half = Point::new(size.width / 2., size.height / 2.);
        self.bar_touched = true;
        self.bar_drag = Some(BarDrag {
            pointer: ev.position,
            centre: at + half,
            start: self.bar,
        });
        cx.notify();
    }

    // A move with no button held ends the drag, in case its release never reached the window.
    fn drag_bar(&mut self, ev: &MouseMoveEvent, window: &mut Window, cx: &mut Context<Self>) {
        let Some(drag) = self.bar_drag else { return };
        if ev.pressed_button.is_none() {
            self.end_bar_drag(window, cx);
            return;
        }
        let by = ev.position - drag.pointer;
        self.bar = drag.start.dragged(drag.centre, by, self.area.get());
        cx.notify();
    }

    fn end_bar_drag(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(drag) = self.bar_drag.take() else {
            return;
        };
        if self.bar != drag.start {
            self.save_bar(window, cx);
        }
        cx.notify();
    }

    fn toggle_bar(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.bar_touched = true;
        self.bar.collapsed = !self.bar.collapsed;
        self.save_bar(window, cx);
        cx.notify();
    }

    // One writer at a time, each taking the newest text, so quick toggles never land out of order.
    fn save_bar(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let mut saves = self.bar_saves.borrow_mut();
        saves.latest = Some(self.bar.to_setting());
        if std::mem::replace(&mut saves.writing, true) {
            return;
        }
        drop(saves);
        let saves = Rc::clone(&self.bar_saves);
        cx.spawn_in(window, async move |_, cx| {
            loop {
                let next = {
                    let mut saves = saves.borrow_mut();
                    let next = saves.latest.take();
                    saves.writing = next.is_some();
                    next
                };
                let Some(text) = next else { break };
                let Ok(save) = cx.update(|_, cx| {
                    run_blocking(cx, move |s| {
                        s.db.set_demo_viewer_bar(&text).map_err(|e| {
                            describe_settings(&e).into_op(tr!("messages.action.save_bar_place"))
                        })
                    })
                }) else {
                    break;
                };
                if let Err(err) = save.await {
                    let _ = cx.update(|window, cx| toast_error(window, cx, &err));
                }
            }
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
        // A line of the bar's text column wraps; the status stays on one line.
        let line = |id: &'static str, words: SharedString, muted: Option<Hsla>, wraps: bool| {
            div()
                .id(id)
                .aria_label(words.clone())
                .test_support()
                .flex_none()
                .when(wraps, |el| el.min_w_0())
                .when(!wraps, |el| el.whitespace_nowrap())
                .text_sm()
                .when_some(muted, |el, colour| el.text_color(colour))
                .child(words)
        };
        // The viewport fills the whole page; without one, the status sits in its centre.
        let status = |words: SharedString| {
            let centre = div().size_full().flex().items_center().justify_center();
            let status = line(ids::DEMO_VIEWER_STATUS, words, Some(muted), false);
            (centre.child(status).into_any_element(), None)
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
        let collapsed = self.bar.collapsed;
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
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|page, ev: &MouseDownEvent, _, cx| page.start_bar_drag(ev, cx)),
            )
            .child(Icon::new(IconName::GripVertical).small());
        let (icon, tip) = match collapsed {
            true => (IconName::ChevronUp, tr!("demo_viewer.bar_expand")),
            false => (IconName::ChevronDown, tr!("demo_viewer.bar_collapse")),
        };
        let collapse = Button::new(ids::DEMO_VIEWER_BAR_COLLAPSE)
            .ghost()
            .small()
            .icon(icon)
            .named(tip)
            .on_click(cx.listener(|page, _, window, cx| page.toggle_bar(window, cx)));

        // Both watchers keep last frame's sizes and ask for one more frame when either moves,
        // so the bar settles at its place a frame after it first draws.
        let area = Rc::clone(&self.area);
        // While the grip is held, moves and the release anywhere in the window reach the page
        // first, so the bar follows a fast pointer that outruns it.
        let page = cx.entity().downgrade();
        let dragging = self.bar_drag.is_some();
        let area_watch = canvas(
            move |bounds, window, _| {
                if area.get() != bounds.size {
                    area.set(bounds.size);
                    window.request_animation_frame();
                }
            },
            move |_, _, window, _| {
                if !dragging {
                    return;
                }
                window.set_window_cursor_style(CursorStyle::ClosedHand);
                let moved = page.clone();
                window.on_mouse_event(move |ev: &MouseMoveEvent, phase, window, cx| {
                    if phase == DispatchPhase::Capture {
                        let _ = moved.update(cx, |page, cx| page.drag_bar(ev, window, cx));
                    }
                });
                window.on_mouse_event(move |_: &MouseUpEvent, phase, window, cx| {
                    if phase == DispatchPhase::Capture {
                        let _ = page.update(cx, |page, cx| page.end_bar_drag(window, cx));
                    }
                });
            },
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
        let at = self.bar.clamp(self.area.get(), self.bar_size.get());
        // The bar's room is a fixed-width frame, so where it sits never changes how it wraps.
        let room = (self.area.get().width - BAR_MARGIN * 2.).max(px(0.));
        let theme = cx.theme();
        let bar = h_flex()
            .id(ids::DEMO_VIEWER_BAR)
            .test_support()
            .occlude()
            .max_w_full()
            .min_w_0()
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
            // Minimised, only the grip and the expand button stay. The caption, note and hint
            // share one column, the only part that shrinks and wraps on a narrow page.
            .when(!collapsed, |bar| {
                bar.when_some(caption.clone(), |bar, (caption, note)| {
                    let words = v_flex()
                        .min_w_0()
                        .flex_shrink(1.)
                        .child(line(ids::DEMO_VIEWER_CAPTION, caption, None, true))
                        .when(note, |words| {
                            let note = tr!("demo_viewer.players_note");
                            words.child(line(
                                ids::DEMO_VIEWER_PLAYERS_NOTE,
                                note,
                                Some(muted),
                                true,
                            ))
                        })
                        .child(line(
                            ids::DEMO_VIEWER_HINT,
                            tr!("demo_viewer.hint"),
                            Some(muted),
                            true,
                        ));
                    bar.child(words)
                })
                .child(div().flex_none().child(open))
            })
            .child(div().flex_none().child(collapse));
        let frame = div()
            .absolute()
            .left(at.x)
            .top(at.y)
            .w(room)
            .flex()
            .items_start()
            .child(bar);
        div()
            .id(ids::page(Page::DemoViewer))
            .test_support()
            .relative()
            .size_full()
            .child(area_watch)
            .child(body)
            .child(frame)
    }
}
