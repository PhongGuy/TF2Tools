//! The HUD Maker's canvas: the screen fitted into the free area, its parts, the selection,
//! pin markers and the crosshair marker.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::sync::Arc;

use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::{ActiveTheme as _, Sizable as _, h_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, Bounds, Context, CursorStyle, DispatchPhase, Hsla, InteractiveElement as _,
    IntoElement, MouseButton, MouseDownEvent, MouseExitEvent, MouseMoveEvent, MouseUpEvent,
    ObjectFit, ParentElement as _, Pixels, Point, RenderImage, SharedString, Size,
    StatefulInteractiveElement as _, Styled as _, StyledImage as _, TestSupportExt as _,
    TextSystem, Window, canvas, div, font as font_named, img, point, px, rgb,
};
use tf2tools_core::{
    ALIGNMENTS, AnimFrame, Animated, ChatSample, ColumnKind, FontVariant, GameSituation, GameText,
    GuideAxis, HiddenBy, HudLayout, HudModel, ImageBorder, ImageCell, Part, PartBox, PartKey, Pick,
    PlayerListLayout, Rect, Rgba, SamplePlayer, SampleSet, Scene, Screen, ScreenScale, Team,
    Vision, background_corners, corner_radius, image_cells, nine_slice, paints_background,
    rank_medal, rank_pieces, scene_image, stand_in_families, wrap_lines, wrapped_text_left,
};

use super::editor::{Guide, Handle};
use super::rank_paint::rank_paint;
use super::view::{CanvasFit, list_font_px};
use super::{HudMakerPage, Images};
use crate::ids;
use crate::pages::crosshair_maker::model::hsla_of_rgba;
use crate::widgets::labelled;

/// The preview's local player: the default situation's RED Soldier, whose team picks a
/// team-coloured panel's background.
pub(super) const LOCAL_TEAM: Team = Team::Red;

/// The decoded image key of the killfeed icon `name`; no part key starts with "killicon:".
pub(super) fn kill_icon_key(name: &str) -> PartKey {
    PartKey::new(format!("killicon:{name}"))
}

/// The 3D class model panel, drawn as the class portrait the game ships.
const MODEL_PANEL: &str = "CTFPlayerModelPanel";
/// The portrait a class model draws: the default situation's Soldier.
pub(super) const MODEL_STAND_IN: &str = "../hud/class_soldierred";
/// The main menu's rank panel, whose medal, name and level bar the game's code draws.
const RANK_PANEL: &str = "CPvPRankPanel";
/// How near, in window pixels, a dragged part's edge or centre must come to snap.
const SNAP_WINDOW_PX: f32 = 6.;
/// A resize handle's side, in window pixels.
const HANDLE: Pixels = px(8.);
/// A selection shorter than this on an axis drops the side handles on that axis, which would
/// crowd its corners.
const SIDE_HANDLES_FROM: Pixels = px(24.);
/// The text size of a placeholder's name.
const PLACEHOLDER_TEXT: Pixels = px(9.);
/// The class the game's code fills with player rows.
const SECTIONED_LIST: &str = "SectionedListPanel";
/// The killfeed part, which draws sample notice lines.
const KILLFEED: &str = "HudDeathNotice";
/// What a scoreboard list the game cannot fill with rows says in place of its name.
const ROWS_IN_GAME: &str = "Player rows draw in game";

/// The eight resize handles, each with where it sits on the selection (0, half or 1 of each
/// side) and the cursor it shows.
const HANDLES: [(Handle, f32, f32, CursorStyle); 8] = [
    (Handle::TopLeft, 0., 0., CursorStyle::ResizeUpLeftDownRight),
    (Handle::Top, 0.5, 0., CursorStyle::ResizeUpDown),
    (Handle::TopRight, 1., 0., CursorStyle::ResizeUpRightDownLeft),
    (Handle::Right, 1., 0.5, CursorStyle::ResizeLeftRight),
    (
        Handle::BottomRight,
        1.,
        1.,
        CursorStyle::ResizeUpLeftDownRight,
    ),
    (Handle::Bottom, 0.5, 1., CursorStyle::ResizeUpDown),
    (
        Handle::BottomLeft,
        0.,
        1.,
        CursorStyle::ResizeUpRightDownLeft,
    ),
    (Handle::Left, 0., 0.5, CursorStyle::ResizeLeftRight),
];

/// A drag of a part or handle on the canvas: where the pointer went down, at what scale, and
/// the latest screen-pixel delta not yet previewed, and what the last preview showed.
#[derive(Debug, Clone)]
pub(super) struct PartDrag {
    from: Point<Pixels>,
    handle: Handle,
    scale: f32,
    pending: Option<(i32, i32)>,
    // Ctrl (Cmd) held on the latest move, which turns snapping off.
    free: bool,
    shown: Option<(i32, i32)>,
    guides: Vec<Guide>,
}

/// A box drawn by dragging on empty canvas, in window pixels; on release it selects every
/// part inside it, added to the selection when Shift began it.
#[derive(Debug, Clone)]
pub(super) struct Marquee {
    from: Point<Pixels>,
    to: Point<Pixels>,
    adding: bool,
}

/// A canvas press: the screen pixel, the part it selected and the layout it picked on.
#[derive(Debug, Clone)]
pub(super) struct LastPress {
    at: tf2tools_core::Point,
    picked: PartKey,
    generation: u64,
}

impl HudMakerPage {
    // The free area measures itself with a `canvas` watcher, as the Crosshair Maker's grid does;
    // the screen and every part are absolutely placed divs inside it, fitted from last frame's
    // bounds. Parts carry no handlers: a press on the screen goes through `HudLayout::pick`.
    // The watcher always listens window-wide for a release, so one anywhere commits a drag,
    // even a drag whose press and release arrive before the next frame; it follows moves only
    // while a drag runs or a held panel waits for the pointer to leave it.
    pub(super) fn canvas_area(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let area = Rc::clone(&self.area);
        let page = cx.entity().downgrade();
        let dragging = self.part_drag.is_some() || self.marquee.is_some();
        let dividing = self.compare_drag;
        let waiting = self.held.is_some();
        let panning = self.is_panning();
        let watch = canvas(
            move |bounds, window, _| {
                if area.get() != bounds {
                    area.set(bounds);
                    window.request_animation_frame();
                }
            },
            move |_, _, window, _| {
                let released = page.clone();
                window.on_mouse_event(move |_: &MouseUpEvent, phase, window, cx| {
                    if phase == DispatchPhase::Capture {
                        let _ = released.update(cx, |page, cx| {
                            page.compare_drag = false;
                            page.end_canvas_pan(cx);
                            page.end_marquee(window, cx);
                            page.end_part_drag(window, cx)
                        });
                    }
                });
                // The latest press makes an edit the pointer's, which alone may hold a panel;
                // the pointer leaving the window lets go, as a key in the page does.
                let pressed = page.clone();
                window.on_mouse_event(move |ev: &MouseDownEvent, phase, _, cx| {
                    if phase == DispatchPhase::Capture {
                        let _ = pressed.update(cx, |page, _| page.pointer_pressed(ev.position));
                    }
                });
                let exited = page.clone();
                window.on_mouse_event(move |_: &MouseExitEvent, phase, _, cx| {
                    if phase == DispatchPhase::Capture {
                        let _ = exited.update(cx, |page, cx| page.let_go(cx));
                    }
                });
                if waiting {
                    let left = page.clone();
                    window.on_mouse_event(move |ev: &MouseMoveEvent, phase, _, cx| {
                        if phase == DispatchPhase::Capture {
                            let _ =
                                left.update(cx, |page, cx| page.settle_when_left(ev.position, cx));
                        }
                    });
                }
                if dividing {
                    let slid = page.clone();
                    window.on_mouse_event(move |ev: &MouseMoveEvent, phase, _, cx| {
                        if phase == DispatchPhase::Capture {
                            let _ = slid.update(cx, |page, cx| page.drag_compare(ev.position, cx));
                        }
                    });
                }
                if panning {
                    let panned = page.clone();
                    window.on_mouse_event(move |ev: &MouseMoveEvent, phase, _, cx| {
                        if phase == DispatchPhase::Capture {
                            let _ = panned.update(cx, |page, cx| page.pan_canvas(ev, cx));
                        }
                    });
                }
                if !dragging {
                    return;
                }
                let moved = page.clone();
                window.on_mouse_event(move |ev: &MouseMoveEvent, phase, window, cx| {
                    if phase == DispatchPhase::Capture {
                        let _ = moved.update(cx, |page, cx| {
                            page.drag_marquee(ev, cx);
                            page.drag_part(ev, window, cx)
                        });
                    }
                });
            },
        )
        .absolute()
        .size_full();
        let frame = div()
            .id(ids::HUD_MAKER_CANVAS_AREA)
            .test_support()
            .relative()
            .flex_1()
            .min_h_0()
            .w_full()
            .child(watch);
        if self.every_screen && matches!(self.load, super::Load::Ready(_)) {
            return frame.children(self.screens_view(cx)).into_any_element();
        }
        let hard = self.hard_to_read();
        self.forget_stale_text_bounds();
        let area = self.area.get();
        let super::Load::Ready(loaded) = &self.load else {
            return frame.into_any_element();
        };
        let editor = &loaded.editor;
        let fit = self.canvas_fit(editor.screen());
        let screen = fit.screen_bounds();
        let room = self.screen_area();
        let theme = cx.theme();
        let (primary, border) = (theme.primary, theme.border);
        let (warning, background) = (theme.warning, theme.background);
        let tag_colours = (
            theme.popover,
            theme.popover_foreground,
            theme.muted_foreground,
            theme.radius,
        );
        let local = |r: Bounds<Pixels>| Bounds {
            origin: r.origin - screen.origin,
            size: r.size,
        };

        let model = editor.model();
        let shown_layout = editor.shown_layout();
        let layout: &HudLayout = &shown_layout;
        let played_frame = (editor.moment() > 0.).then(|| editor.frame());
        let rule = model.scene_rule();
        let mut look = self.look(loaded, layout, fit.scale(), border, primary);
        look.frame = played_frame.as_ref();
        // The chat lines record where they draw for the part list, as the killfeed's do.
        match look.chat {
            Some(_) => look.chat_measure = Some(Rc::clone(&self.chat_bounds)),
            None => self.chat_bounds.borrow_mut().clear(),
        }
        let measured_text = Rc::clone(&self.text_bounds);
        if hard.as_ref().is_some_and(|h| !h.is_empty()) {
            look.measure = Some(Rc::clone(&measured_text));
        } else {
            measured_text.borrow_mut().clear();
        }
        // While the selection is a part the game hides, its whole toggled panel draws faintly so
        // the player sees what they edit; it goes again once the selection leaves the panel.
        let ghost = editor
            .selected()
            .and_then(|key| game_hidden_root(model, layout, key));
        let drawn = |b: &PartBox| {
            b.hidden().is_none()
                || ghost.is_some_and(|root| game_hidden_root(model, layout, b.key()) == Some(root))
        };
        // The window boxes of what the game shows here, cut as the game clips them, each marked
        // when it is the selection: the part list moves off these. Faint ghosts of a hidden
        // panel are out.
        let selected_key = editor.selected();
        // A new scene's parts change what the list is weighed against, so ask for one more
        // frame to weigh it; it fires only when the recorded scene changes.
        if self.drawn_scene.replace(Some(editor.scene())) != Some(editor.scene()) {
            cx.notify();
        }
        *self.drawn_bounds.borrow_mut() = (draw_order(model, layout).into_iter())
            .filter(|part| draws(part, &look))
            .filter(|part| layout.get(part.key()).is_some_and(|b| b.hidden().is_none()))
            .filter_map(|part| {
                let s = layout.shown(part.key())?;
                let r = fit.rect(s.x, s.y, s.wide, s.tall).intersect(&room);
                Some((r, Some(part.key()) == selected_key))
            })
            .collect();
        // Each part paints only what the game shows of it, cut at its parents' edges. A part cut
        // off whole paints no content, unless it is the selection: then it and everything in it
        // ghost, each cut only at the parents inside the selection.
        let cut_off = editor.selected().filter(|key| cut_off_whole(layout, key));
        let parts = part_elements(model, layout, &look, &fit, screen.origin, editor.selected());
        // The HUD's own crosshair label stands in for the plain marker while the HUD shows it.
        let hud_crosshair = (!rule.hides_hud())
            .then(|| hud_crosshair_element(&look, |x, y, w, h| local(fit.rect(x, y, w, h))))
            .flatten();
        let measured = Rc::clone(&self.killfeed_bounds);
        let killfeed = layout.get(&PartKey::new(KILLFEED)).and_then(|b| {
            let r = local(fit.rect(b.x(), b.y(), b.wide(), b.tall()));
            killfeed_lines(&look, r, &measured, cx)
        });
        if killfeed.is_none() {
            measured.borrow_mut().clear();
        }
        // While comparing, the snapshot's parts and the current ones each fill the whole screen
        // but show only their side of the split, so a backdrop shows through both.
        let mut now_content = parts;
        now_content.extend(killfeed);
        // The screen's place in the view, and the part of it the view shows, view-local.
        let shift = screen.origin - room.origin;
        let visible = Bounds::new(shift, screen.size)
            .intersect(&Bounds::new(point(px(0.), px(0.)), room.size));
        let (under, (compare_chrome, compare_notes)) = match editor.compared() {
            None => (now_content, (Vec::new(), Vec::new())),
            Some(shown) => {
                let (wide, tall) = (screen.size.width, screen.size.height);
                let at = wide * self.compare_split;
                // The snapshot's own look, apart from the current one, so it records into none
                // of the current look's measuring cells.
                let before = self.look_for(
                    loaded,
                    shown.model(),
                    shown.layout(),
                    fit.scale(),
                    border,
                    primary,
                );
                let mut before_content = part_elements(
                    shown.model(),
                    shown.layout(),
                    &before,
                    &fit,
                    screen.origin,
                    None,
                );
                // The snapshot's killfeed draws into a throwaway bounds cell for the same reason.
                let throwaway = Rc::new(RefCell::new(Vec::new()));
                before_content.extend(shown.layout().get(&PartKey::new(KILLFEED)).and_then(|b| {
                    let r = local(fit.rect(b.x(), b.y(), b.wide(), b.tall()));
                    killfeed_lines(&before, r, &throwaway, cx)
                }));
                let layer = |id: &'static str,
                             left: Pixels,
                             width: Pixels,
                             inner: Pixels,
                             content: Vec<AnyElement>| {
                    let content = div()
                        .absolute()
                        .left(inner)
                        .top_0()
                        .w(wide)
                        .h(tall)
                        .children(content);
                    div()
                        .id(id)
                        .test_support()
                        .absolute()
                        .left(left)
                        .top_0()
                        .w(width)
                        .h(tall)
                        .overflow_hidden()
                        .child(content)
                        .into_any_element()
                };
                let layers = vec![
                    layer(
                        ids::HUD_MAKER_COMPARE_BEFORE,
                        px(0.),
                        at,
                        px(0.),
                        before_content,
                    ),
                    layer(ids::HUD_MAKER_COMPARE_NOW, at, wide - at, -at, now_content),
                ];
                let name = self.comparing.as_ref().map(|(_, name)| name.clone());
                let chrome = self.compare_chrome(at, shift, visible, room.size, name, cx);
                (layers, chrome)
            }
        };
        // Each hard-to-read label gets an outline around the text it draws and a badge beside
        // it, kept on the canvas and off every visible label's text. The text is as drawn last
        // frame, clipped to its box, else core's estimate until it has drawn once.
        // A part core judges by its whole box, such as a label painting a background, keeps it.
        let extent = |b: &PartBox| {
            // The chat history's text is the lines it drew last frame, not a label's estimate.
            let chat = (look.chat.as_ref())
                .filter(|c| c.history() == b.key())
                .and_then(|_| {
                    (self.chat_bounds.borrow().iter())
                        .filter(|l| l.size.width > px(0.) && l.size.height > px(0.))
                        .map(|l| local(*l))
                        .reduce(|all, l| all.union(&l))
                });
            if chat.is_some() {
                return chat;
            }
            let core = model.text_extent(layout, b.key(), look.text)?;
            let estimate = local(fit.rect(core.x, core.y, core.wide, core.tall));
            if core == b.rect() {
                return Some(estimate);
            }
            let boxed = local(fit.rect(b.x(), b.y(), b.wide(), b.tall()));
            let drawn = measured_text.borrow().get(b.key()).copied();
            Some(drawn.map_or(estimate, |d| local(d).intersect(&boxed)))
        };
        let shown = |b: &&PartBox| b.hidden().is_none();
        let flagged: Vec<_> = (hard.iter())
            .flat_map(|hard| {
                (layout.boxes().iter()).filter(move |b| shown(b) && hard.contains(b.key()))
            })
            .filter_map(|b| Some((b.key().clone(), extent(b)?)))
            .collect();
        let texts: Vec<_> = if flagged.is_empty() {
            Vec::new()
        } else {
            (layout.boxes().iter())
                .filter(shown)
                .filter(|b| (model.part(b.key())).is_some_and(|p| p.value("labelText").is_some()))
                .filter_map(extent)
                // The chat lines are text too, as drawn last frame.
                .chain(
                    (self.chat_bounds.borrow().iter())
                        .filter(|b| b.size.width > px(0.) && b.size.height > px(0.))
                        .map(|b| local(*b)),
                )
                .collect()
        };
        let markers = super::markers::markers(
            &flagged,
            &texts,
            screen.size,
            &super::markers::MarkerLook {
                warning,
                background,
            },
        );
        // Both outlines cover only what the game can draw of the part, inside its parents.
        let outline = |b: &PartBox| {
            let v = layout.visible(b.key()).unwrap_or(b.rect());
            local(fit.rect(v.x, v.y, v.wide, v.tall))
        };
        let selected = editor
            .selected()
            .and_then(|key| layout.get(key))
            .filter(|b| drawn(b));
        // Sides a parent cuts off, as (left, top, right, bottom): a handle there would sit off
        // the box's real edge, so those sides offer none.
        let cut = selected.map_or((false, false, false, false), |b| {
            let Rect {
                x,
                y,
                wide: w,
                tall: h,
                ..
            } = (layout.visible(b.key())).unwrap_or(b.rect());
            let end = |at: i32, len: i32| i64::from(at) + i64::from(len);
            (
                x != b.x(),
                y != b.y(),
                end(x, w) != end(b.x(), b.wide()),
                end(y, h) != end(b.y(), b.tall()),
            )
        });
        let selected = selected.map(outline);
        // A part that paints nothing here, itself or through a part inside it, is outlined dashed
        // and says so, since the outline is all the player sees of it.
        let named = |key: &PartKey| {
            let part = model.part(key)?;
            let blank = !draws(part, &look) && !paints_inside(model, layout, key, &look);
            let name = super::parts::row_label(part);
            let label = match blank {
                true => format!("{name}, {NOTHING_SHOWN}"),
                false => name.clone(),
            };
            Some((name, blank, label))
        };
        let selected_name = editor.selected().and_then(named);
        // Lighter than the selection and faintly filled, so it reads as "this one" without
        // looking selected, with the part's name on a tag beside it. Neither has handlers, so
        // presses still reach the canvas.
        let hovered = (self.hovered.as_ref())
            .and_then(|key| Some((layout.get(key).filter(|b| drawn(b))?, named(key)?)));
        let (hover, hover_tag) = match hovered {
            Some((b, (name, blank, label))) => {
                let r = outline(b);
                let el = place(div().id(ids::HUD_MAKER_HOVER).test_support(), r)
                    .aria_label(label.clone())
                    .border_1()
                    .border_color(primary.opacity(0.7))
                    .bg(primary.opacity(0.12))
                    .when(blank, |el| el.border_dashed());
                let tag = hover_tag(r, screen.size, name, blank, label, tag_colours);
                (Some(el), Some(tag))
            }
            None => (None, None),
        };
        // Guides span the whole screen, one window pixel wide, at the line the drag snapped to.
        let guides = (self.part_drag.iter())
            .flat_map(|d| d.guides.iter())
            .map(|g| {
                let at = local(fit.rect(g.at(), g.at(), 0, 0)).origin;
                let line = match g.axis() {
                    GuideAxis::Vertical => div()
                        .id(ids::HUD_MAKER_GUIDE_X)
                        .left(at.x)
                        .top_0()
                        .w(px(1.))
                        .h(screen.size.height),
                    GuideAxis::Horizontal | _ => div()
                        .id(ids::HUD_MAKER_GUIDE_Y)
                        .left_0()
                        .top(at.y)
                        .w(screen.size.width)
                        .h(px(1.)),
                };
                line.test_support().absolute().bg(primary)
            });
        let selection = selected.map(|r| {
            let blank = selected_name.as_ref().is_some_and(|(_, blank, _)| *blank);
            place(div().id(ids::HUD_MAKER_SELECTION).test_support(), r)
                .when_some(selected_name.map(|(_, _, label)| label), |el, label| {
                    el.aria_label(label)
                })
                .border_2()
                .border_color(primary)
                .when(ghost.is_some() || cut_off.is_some() || blank, |el| {
                    el.border_dashed()
                })
        });
        // Several selected parts each get the selection's outline and no handles, since they
        // only move together.
        let several: &[PartKey] = match editor.several() {
            true => editor.selection(),
            false => &[],
        };
        let several = (several.iter())
            .filter_map(|key| {
                let b = layout.get(key).filter(|b| drawn(b))?;
                let label = named(key).map(|(_, _, label)| label)?;
                let el = place(
                    div().id(ids::hud_maker_selected(key)).test_support(),
                    outline(b),
                )
                .aria_label(label)
                .border_2()
                .border_color(primary);
                Some(el)
            })
            .collect::<Vec<_>>();
        // A selected part poking out of its parent shows its whole box dashed in the warning
        // colour, so the player sees what the game cuts off at the parent's edges.
        let chosen: &[PartKey] = match editor.several() {
            true => editor.selection(),
            false => editor.selection().get(..1).unwrap_or(&[]),
        };
        let cut_offs = (chosen.iter())
            .filter_map(|key| {
                let parent = super::editor::outside_parent(model, layout, key)?;
                let b = layout.get(key).filter(|b| drawn(b))?;
                let (name, parent) = (named(key)?.0, named(parent)?.0);
                let el = place(
                    div().id(ids::hud_maker_cut_off(key)).test_support(),
                    local(fit.rect(b.x(), b.y(), b.wide(), b.tall())),
                )
                .aria_label(format!("{name}, cut off by {parent}"))
                .border_1()
                .border_dashed()
                .border_color(warning);
                Some(el)
            })
            .collect::<Vec<_>>();
        // The box being drawn, faintly filled, from where the press began to the pointer.
        let marquee = self.marquee.as_ref().map(|m| {
            let (from, to) = (m.from - screen.origin, m.to - screen.origin);
            div()
                .id(ids::HUD_MAKER_MARQUEE)
                .test_support()
                .absolute()
                .left(from.x.min(to.x))
                .top(from.y.min(to.y))
                .w((to.x - from.x).abs())
                .h((to.y - from.y).abs())
                .border_1()
                .border_color(primary)
                .bg(primary.opacity(0.08))
        });
        // Handles sit just outside the selection, so they never cover a small part's content,
        // and an edge too short for a mid-edge handle shows only its corners. A handle press
        // starts its own drag and stops there, so the canvas never re-selects.
        let handles = selected.into_iter().flat_map(|r| {
            let short = (
                r.size.width < SIDE_HANDLES_FROM,
                r.size.height < SIDE_HANDLES_FROM,
            );
            HANDLES
                .into_iter()
                .filter(|&(_, fx, fy, _)| !((fx == 0.5 && short.0) || (fy == 0.5 && short.1)))
                .filter(|&(_, fx, fy, _)| {
                    !((fx == 0. && cut.0)
                        || (fy == 0. && cut.1)
                        || (fx == 1. && cut.2)
                        || (fy == 1. && cut.3))
                })
                .map(|(handle, fx, fy, cursor)| {
                    // 0 puts the handle before the edge, 1 after it, a half centres it.
                    let at =
                        |start: Pixels, len: Pixels, f: f32| start + len * f - HANDLE * (1. - f);
                    div()
                        .id(ids::hud_maker_handle(handle))
                        .test_support()
                        .absolute()
                        .left(at(r.origin.x, r.size.width, fx))
                        .top(at(r.origin.y, r.size.height, fy))
                        .size(HANDLE)
                        .bg(primary)
                        .border_1()
                        .border_color(gpui_kit::white())
                        .cursor(cursor)
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |page, ev: &MouseDownEvent, window, cx| {
                                cx.stop_propagation();
                                if page.stop_matching_size(cx) {
                                    return;
                                }
                                page.start_part_drag(handle, ev.position, window, cx);
                            }),
                        )
                })
                .collect::<Vec<_>>()
        });

        // The screenshot fills the screen box, stretched as the game would show it, under every
        // part; the main menu draws the game's own menu picture there instead.
        let (backdrop_id, backdrop) = match model.scene() {
            Scene::MainMenu => (ids::HUD_MAKER_MENU_BACKGROUND, self.menu_background()),
            Scene::Hud
            | Scene::Scoreboard
            | Scene::FreezeCam
            | Scene::WinPanel
            | Scene::ClassSelect
            | Scene::Spectator
            | _ => (ids::HUD_MAKER_BACKDROP_IMAGE, self.backdrop_image()),
        };
        let backdrop = backdrop.map(|image| {
            div()
                .id(backdrop_id)
                .test_support()
                .absolute()
                .inset_0()
                .size_full()
                .child(img(image).size_full().object_fit(ObjectFit::Fill))
        });
        // While the menu picture is being read, a plain line stands in for the blank screen.
        let loading = self.menu_art_loading().then(|| {
            div()
                .id(ids::HUD_MAKER_MENU_LOADING)
                .aria_label("Loading the menu picture\u{2026}")
                .test_support()
                .absolute()
                .inset_0()
                .flex()
                .items_center()
                .justify_center()
                .text_sm()
                .text_color(cx.theme().muted_foreground)
                .child("Loading the menu picture\u{2026}")
        });
        let screen_div = div()
            .id(ids::HUD_MAKER_CANVAS)
            .test_support()
            .absolute()
            .left(screen.origin.x - room.origin.x)
            .top(screen.origin.y - room.origin.y)
            .w(screen.size.width)
            .h(screen.size.height)
            .overflow_hidden()
            .bg(rgb(0x101010))
            .children(backdrop)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|page, ev: &MouseDownEvent, window, cx| {
                    page.press_canvas(ev, window, cx)
                }),
            )
            .on_mouse_move(
                cx.listener(|page, ev: &MouseMoveEvent, _, cx| page.hover_canvas(ev.position, cx)),
            )
            .on_hover(cx.listener(|page, inside: &bool, _, cx| {
                if !*inside {
                    page.leave_canvas(cx)
                }
            }))
            .children(under)
            .children(markers)
            .children(hover)
            .children(cut_offs)
            .children(selection)
            .children(several)
            .children(marquee)
            .children(handles)
            .children(guides)
            .children(hover_tag)
            .children(hud_crosshair.or_else(|| {
                (!rule.hides_hud()).then(|| {
                    crosshair(screen.size.width / 2., screen.size.height / 2.).into_any_element()
                })
            }))
            .children(compare_chrome);
        // The view's name over the canvas, so the player knows the preview is altered.
        let badge = super::colour_view::badge(self.vision()).map(|text| {
            let at = self.screen_area().origin - area.origin;
            div()
                .id(ids::HUD_MAKER_VISION_BADGE)
                .aria_label(text.clone())
                .test_support()
                .absolute()
                .left(at.x + px(8.))
                .top(at.y + px(8.))
                .px_2()
                .py_1()
                .rounded_md()
                .text_xs()
                .bg(background)
                .border_1()
                .border_color(border)
                .child(text)
        });
        // The screen sits in a view the size of the room it is fitted into, which clips it when
        // zoomed. The view takes the wheel and the presses that pan, over its letterbox too.
        let grab = match (self.is_panning(), self.space_held) {
            (true, _) => Some(CursorStyle::ClosedHand),
            (false, true) => Some(CursorStyle::OpenHand),
            _ => None,
        };
        let view = div()
            .id(ids::HUD_MAKER_CANVAS_VIEW)
            .test_support()
            .absolute()
            .left(room.origin.x - area.origin.x)
            .top(room.origin.y - area.origin.y)
            .w(room.size.width)
            .h(room.size.height)
            .overflow_hidden()
            .when_some(grab, |el, cursor| el.cursor(cursor))
            .on_scroll(cx.listener(|page, ev, window, cx| page.wheel_canvas(ev, window, cx)))
            .on_mouse_down(
                MouseButton::Middle,
                cx.listener(|page, ev: &MouseDownEvent, window, cx| {
                    page.start_canvas_pan(ev.position, window, cx)
                }),
            )
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|page, ev: &MouseDownEvent, window, cx| {
                    if page.space_held {
                        page.start_canvas_pan(ev.position, window, cx);
                    }
                }),
            )
            .child(screen_div)
            // Notes over the canvas keep to the view, so no zoom or pan takes them away.
            .children(loading)
            .children(compare_notes);
        let screen_shape = editor.screen();
        frame
            .child(view)
            .children(badge)
            .children(self.start_card(cx))
            .child(self.floating_toolbar(screen_shape, cx))
            .children(self.panels(cx))
            .into_any_element()
    }

    // The comparison's divider and grip, local to the screen, and its captions and Stop chip,
    // local to the canvas view and kept on the part of the screen it `shown`s, so no zoom or pan
    // takes them away. `shift` is the screen's place in the view. The grip's press hands the keys to the page, so Escape reaches it, and stops
    // there so the canvas never selects.
    fn compare_chrome(
        &self,
        at: Pixels,
        shift: Point<Pixels>,
        shown: Bounds<Pixels>,
        view: Size<Pixels>,
        name: Option<String>,
        cx: &mut Context<Self>,
    ) -> (Vec<AnyElement>, Vec<AnyElement>) {
        let theme = cx.theme();
        let (primary, background) = (theme.primary, theme.background.opacity(0.8));
        let divider = div()
            .id(ids::HUD_MAKER_COMPARE_DIVIDER)
            .test_support()
            .absolute()
            .left(at - px(1.))
            .top(shown.top() - shift.y)
            .w(px(2.))
            .h(shown.size.height)
            .bg(primary);
        let grip = div()
            .id(ids::HUD_MAKER_COMPARE_HANDLE)
            .test_support()
            .absolute()
            .left(at - px(8.))
            // Mid-height of what the view shows of the screen, so a zoom keeps it in reach.
            .top(shown.center().y - shift.y - px(16.))
            .w(px(16.))
            .h(px(32.))
            .rounded_md()
            .bg(primary)
            .border_1()
            .border_color(gpui_kit::white())
            .cursor(CursorStyle::ResizeLeftRight)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|page, _: &MouseDownEvent, window, cx| {
                    cx.stop_propagation();
                    page.compare_drag = true;
                    page.focus.focus(window, cx);
                    cx.notify();
                }),
            );
        let caption =
            |base: gpui_kit::Div| base.px_2().py_1().rounded_md().text_xs().bg(background);
        // Both captions hang from the divider, below the colour-blind badge: the snapshot's
        // name ends just left of it, "Now" starts just right of it. Near an edge the caption on
        // the empty side stays on the screen, and a long name is cut with an ellipsis.
        let (gap, top) = (px(6.), shown.top() + px(40.));
        let (left, at, wide) = (shown.left(), at + shift.x - shown.left(), shown.size.width);
        let name_box = CAPTION_MAX.min(f32::from(wide)).max(0.);
        let room = f32::from(at - gap);
        let before_left = (room - name_box).max(0.);
        let before = div()
            .absolute()
            .left(left + px(before_left))
            .top(top)
            .w(px(name_box))
            .flex()
            .when(room >= name_box, |d| d.justify_end())
            .child(labelled(
                caption(div().max_w_full().truncate()),
                ids::HUD_MAKER_COMPARE_LABEL_BEFORE,
                name.unwrap_or_default(),
            ));
        let now_left = f32::from(at + gap).min((f32::from(wide) - NOW_BOX).max(0.));
        let now = div()
            .absolute()
            .left(left + px(now_left))
            .top(top)
            .child(labelled(
                caption(div()),
                ids::HUD_MAKER_COMPARE_LABEL_NOW,
                "Now",
            ));
        // The chip stays at the shown screen's top right, clear of the panel on the left. It occludes, so a
        // press on it never reaches the canvas.
        let stop = div().occlude().child(
            Button::new(ids::HUD_MAKER_COMPARE_STOP)
                .outline()
                .small()
                .label("Stop comparing")
                .on_click(cx.listener(|page, _, _, cx| page.stop_comparing(cx))),
        );
        let top_right = h_flex()
            .absolute()
            .right(view.width - shown.right() + px(8.))
            .top(shown.top() + px(8.))
            .child(stop);
        let on_screen = vec![divider.into_any_element(), grip.into_any_element()];
        let on_view = vec![
            before.into_any_element(),
            now.into_any_element(),
            top_right.into_any_element(),
        ];
        (on_screen, on_view)
    }

    // Moves the divider to the pointer's place across the screen; nothing is laid out.
    fn drag_compare(&mut self, at: Point<Pixels>, cx: &mut Context<Self>) {
        let Some(editor) = self.editor() else {
            return;
        };
        let shown = self.canvas_fit(editor.screen()).screen_bounds();
        let wide = f32::from(shown.size.width);
        if wide > 0. {
            self.set_compare_split(f32::from(at.x - shown.origin.x) / wide, cx);
        }
    }

    // The toolbar floats where `toolbar_shown` says, at most the screen area's width less its
    // margins, wrapping when narrower. It occludes, so a press on it never reaches the canvas,
    // and a watcher measures it so the next frame can centre and settle it.
    fn floating_toolbar(&mut self, screen: Screen, cx: &mut Context<Self>) -> AnyElement {
        let measured = Rc::clone(&self.toolbar_bounds);
        let watch = canvas(
            move |inner, window, _| {
                // Pinned to the padding box's corner, the watcher fills the inside of the 1 px
                // border; the bar is that much bigger.
                let bounds = inner.dilate(px(1.));
                if measured.get() != bounds {
                    window.request_animation_frame();
                }
                measured.set(bounds);
            },
            |_, _, _, _| {},
        )
        .absolute()
        .top_0()
        .left_0()
        .size_full();
        let content = if self.toolbar_collapsed {
            Button::new(ids::HUD_MAKER_TOOLBAR_EXPAND)
                .ghost()
                .icon(IconName::ChevronUp)
                .label("Show toolbar")
                .tooltip("Show toolbar")
                .on_click(cx.listener(|page, _, _, cx| page.set_toolbar_collapsed(false, cx)))
                .into_any_element()
        } else {
            self.toolbar(screen, cx)
        };
        let at = self.toolbar_shown();
        let widest = (self.screen_area().size.width - super::TOOLBAR_GAP * 2.).max(px(0.));
        let theme = cx.theme();
        div()
            .id(ids::HUD_MAKER_TOOLBAR)
            .test_support()
            .occlude()
            .absolute()
            .left(at.x)
            .top(at.y)
            .max_w(widest)
            .p_1()
            .rounded_md()
            .border_1()
            .border_color(theme.border)
            .bg(theme.background)
            .shadow_md()
            .child(watch)
            .child(content)
            .into_any_element()
    }

    // Selects the group of the best part under the press (see `grouped`), fitted from the same
    // bounds the frame was drawn with, and starts moving it; the press also hands the keys to
    // the page. Alt, a second press on the same spot or a double-click goes inside a selection
    // around that part, and otherwise Alt or the second press steps on to the next part under
    // the pointer. A press the selection holds (see `holds_press`) keeps it, so it drags.
    fn press_canvas(&mut self, ev: &MouseDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        // With Space held the press pans instead; the canvas view's own handler starts it.
        if self.space_held {
            return;
        }
        if self.matching_size {
            return self.pick_size_source(ev, window, cx);
        }
        let Some(editor) = self.editor() else {
            return;
        };
        let at = self.canvas_fit(editor.screen()).to_screen(ev.position);
        let selected = editor.selected().cloned();
        let picks = at.map_or_else(Vec::new, |p| self.picks_at(p.x, p.y));
        let held = at.is_some_and(|p| self.holds_press(p.x, p.y, &picks));
        let picks: Vec<PartKey> = picks.into_iter().map(|p| p.key().clone()).collect();
        // Shift adds what a press picks to the selection or takes it out, and starts a box on
        // empty canvas; a press on any of several selected parts drags them all.
        if ev.modifiers.shift {
            return self.press_adding(ev, at.is_some(), &picks, window, cx);
        }
        let model = editor.model();
        let on_several = editor.several()
            && (picks.iter()).any(|p| editor.selection().iter().any(|s| is_within(model, p, s)));
        if on_several {
            self.focus.focus(window, cx);
            return self.start_part_drag(Handle::Move, ev.position, window, cx);
        }
        // The same spot steps on only while nothing was laid out again since that press; the
        // second half of a double-click goes inside what its first half picked, or keeps it.
        let generation = editor.layout().generation();
        let same_spot = matches!(&self.last_press, Some(last)
            if Some(last.at) == at && selected.as_ref() == Some(&last.picked)
                && last.generation == generation);
        let steps = ev.modifiers.alt || (same_spot && ev.click_count < 2);
        let top = picks.first();
        let inside = (selected.as_ref().zip(top)).and_then(|(s, t)| self.drilled(s, t));
        let next = match inside.filter(|_| steps || ev.click_count >= 2) {
            Some(inner) => Some(inner),
            None if steps => (selected.as_ref())
                .and_then(|current| picks.iter().position(|k| k == current))
                .map(|i| picks[(i + 1) % picks.len()].clone())
                .or_else(|| top.cloned()),
            None if same_spot || held => selected.clone(),
            None => top.map(|t| self.grouped(t)),
        };
        let hit = next.is_some();
        let Some(editor) = self.editor_mut() else {
            return;
        };
        // A press on the screen ends a recipe preview even when it hits nothing.
        match at {
            Some(_) => editor.select_pick(next.clone()),
            None => editor.select(None),
        }
        self.refresh(window, cx);
        self.focus.focus(window, cx);
        if hit {
            self.start_part_drag(Handle::Move, ev.position, window, cx);
        } else if at.is_some() {
            self.start_marquee(ev.position, false, cx);
        }
        // Read after the press's own flush, which may have committed a typed value.
        let generation = self.editor().map(|e| e.layout().generation());
        self.last_press = at
            .zip(next)
            .zip(generation)
            .map(|((at, picked), generation)| LastPress {
                at,
                picked,
                generation,
            });
    }

    // A Shift press: the part a plain press would pick (Alt: the part itself) goes in or out of
    // the selection, or on empty screen a box starts that adds what it covers.
    fn press_adding(
        &mut self,
        ev: &MouseDownEvent,
        on_screen: bool,
        picks: &[PartKey],
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(top) = picks.first() else {
            if on_screen {
                self.start_marquee(ev.position, true, cx);
            }
            return;
        };
        let part = match ev.modifiers.alt {
            true => top.clone(),
            false => self.grouped(top),
        };
        self.last_press = None;
        if let Some(editor) = self.editor_mut() {
            editor.toggle_select(part);
        }
        self.refresh(window, cx);
        self.focus.focus(window, cx);
    }

    fn start_marquee(&mut self, at: Point<Pixels>, adding: bool, cx: &mut Context<Self>) {
        self.marquee = Some(Marquee {
            from: at,
            to: at,
            adding,
        });
        cx.notify();
    }

    fn drag_marquee(&mut self, ev: &MouseMoveEvent, cx: &mut Context<Self>) {
        if let Some(m) = &mut self.marquee {
            m.to = ev.position;
            cx.notify();
        }
    }

    // The release selects every part the box covers whole, each taken as its group when the
    // group fits too, and none inside another; a box too small to see selects nothing new.
    pub(super) fn end_marquee(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(m) = self.marquee.take() else {
            return;
        };
        let covered = self.covered_by(&m);
        let Some(editor) = self.editor_mut() else {
            return;
        };
        let mut parts = match m.adding {
            true => editor.selection().to_vec(),
            false => Vec::new(),
        };
        for key in covered {
            if !parts.contains(&key) {
                parts.push(key);
            }
        }
        editor.select_all(parts);
        self.refresh(window, cx);
    }

    // The parts marquee `m` covers whole, in screen pixels, as the selection it makes.
    fn covered_by(&self, m: &Marquee) -> Vec<PartKey> {
        let super::Load::Ready(loaded) = &self.load else {
            return Vec::new();
        };
        let editor = &loaded.editor;
        let fit = self.canvas_fit(editor.screen());
        let (origin, k) = (fit.screen_bounds().origin, fit.scale());
        if k <= 0. {
            return Vec::new();
        }
        let px_of = |v: Pixels, o: Pixels| f32::from(v - o) / k;
        let (x0, x1) = (px_of(m.from.x, origin.x), px_of(m.to.x, origin.x));
        let (y0, y1) = (px_of(m.from.y, origin.y), px_of(m.to.y, origin.y));
        let (left, right) = (x0.min(x1), x0.max(x1));
        let (top, bottom) = (y0.min(y1), y0.max(y1));
        if right - left < 2. && bottom - top < 2. {
            return Vec::new();
        }
        let model = editor.model();
        let shown_layout = editor.shown_layout();
        let layout: &HudLayout = &shown_layout;
        let inside = |key: &PartKey| {
            layout.visible(key).is_some_and(|v| {
                v.x as f32 >= left
                    && v.y as f32 >= top
                    && (v.x + v.wide) as f32 <= right
                    && (v.y + v.tall) as f32 <= bottom
            })
        };
        let shown = takes_clicks(model, layout, None);
        let mut found: Vec<PartKey> = (layout.boxes().iter())
            .filter(|b| b.clickable() && b.hidden().is_none() && inside(b.key()))
            .map(|b| match group_of(model, layout, b.key(), &shown) {
                Some(group) if inside(&group) => group,
                _ => b.key().clone(),
            })
            .collect();
        found.dedup();
        let all = found.clone();
        found.retain(|k| !all.iter().any(|o| o != k && is_within(model, k, o)));
        let mut unique = Vec::new();
        for k in found {
            if !unique.contains(&k) {
                unique.push(k);
            }
        }
        unique
    }

    // Hovers what a press at window point `at` would select. Only a change redraws, so a
    // pointer moving within one part costs a pick and nothing more; a drag or pan keeps it.
    fn hover_canvas(&mut self, at: Point<Pixels>, cx: &mut Context<Self>) {
        if self.part_drag.is_some() || self.compare_drag || self.is_panning() {
            return;
        }
        let Some(editor) = self.editor() else {
            return;
        };
        let next = (self.canvas_fit(editor.screen()).to_screen(at)).and_then(|p| {
            let picks = self.picks_at(p.x, p.y);
            match self.holds_press(p.x, p.y, &picks) {
                true => editor.selected().cloned(),
                false => picks.first().map(|p| self.grouped(p.key())),
            }
        });
        self.hover_on_canvas = true;
        if self.hovered != next {
            self.hovered = next;
            cx.notify();
        }
    }

    // The pointer left the screen: a hover the canvas set goes, one a list row set stays.
    fn leave_canvas(&mut self, cx: &mut Context<Self>) {
        if std::mem::take(&mut self.hover_on_canvas) && self.hovered.take().is_some() {
            cx.notify();
        }
    }

    // Every part under screen pixel (x, y) a click may select, best first: the parts on the
    // canvas, plus the faint panel of a selected part the game hides.
    fn picks_at(&self, x: i32, y: i32) -> Vec<Pick> {
        let super::Load::Ready(loaded) = &self.load else {
            return Vec::new();
        };
        let model = loaded.editor.model();
        let shown_layout = loaded.editor.shown_layout();
        let layout: &HudLayout = &shown_layout;
        // Chrome colours do not change what a part draws.
        let look = self.look(loaded, layout, 1., Hsla::default(), Hsla::default());
        let shown = takes_clicks(model, layout, loaded.editor.selected());
        let draws = |key: &PartKey| model.part(key).is_some_and(|p| draws(p, &look));
        layout.pick_ranked(x, y, shown, draws)
    }

    // What a plain click on `top` selects: its group (see `group_of`), so a player takes their
    // health whole; once the selection is inside that group, `top` itself.
    fn grouped(&self, top: &PartKey) -> PartKey {
        let super::Load::Ready(loaded) = &self.load else {
            return top.clone();
        };
        let model = loaded.editor.model();
        let layout = loaded.editor.shown_layout();
        let selected = loaded.editor.selected();
        let shown = takes_clicks(model, &layout, selected);
        match group_of(model, &layout, top, &shown) {
            Some(group)
                if !selected.is_some_and(|s| *s != group && is_within(model, s, &group)) =>
            {
                group
            }
            _ => top.clone(),
        }
    }

    // Going inside `selected` towards `top`: the outermost group between them, or `top` when
    // none is; `None` unless `selected` encloses `top`.
    fn drilled(&self, selected: &PartKey, top: &PartKey) -> Option<PartKey> {
        let super::Load::Ready(loaded) = &self.load else {
            return None;
        };
        let model = loaded.editor.model();
        if selected == top || !is_within(model, top, selected) {
            return None;
        }
        let layout = loaded.editor.shown_layout();
        let shown = takes_clicks(model, &layout, Some(selected));
        let mut inner = top.clone();
        let mut at = model.part(top).and_then(|p| p.parent());
        while let Some(key) = at.filter(|k| *k != selected) {
            if is_group(model, &layout, key, &shown) {
                inner = key.clone();
            }
            at = model.part(key).and_then(|p| p.parent());
        }
        Some(inner)
    }

    // Whether a press at screen pixel (x, y) stays on the selection: inside its outline, unless
    // the best pick there draws and paints over it. A ghost the game hides or its parents cut
    // off holds every press in its whole box, since nothing it shows can be pressed.
    fn holds_press(&self, x: i32, y: i32, picks: &[Pick]) -> bool {
        let super::Load::Ready(loaded) = &self.load else {
            return false;
        };
        let model = loaded.editor.model();
        let shown_layout = loaded.editor.shown_layout();
        let layout: &HudLayout = &shown_layout;
        let Some(key) = loaded.editor.selected() else {
            return false;
        };
        if on_selected_ghost(model, layout, Some(key), x, y) {
            return true;
        }
        // A box that takes no clicks itself, such as the killfeed's, holds none either.
        let outlined = layout
            .get(key)
            .is_some_and(|b| b.hidden().is_none() && b.clickable());
        let inside = layout.visible(key).is_some_and(|v| {
            (v.x..v.x.saturating_add(v.wide)).contains(&x)
                && (v.y..v.y.saturating_add(v.tall)).contains(&y)
        });
        if !(outlined && inside) {
            return false;
        }
        let Some(top) = picks.first().filter(|top| top.key() != key) else {
            return true;
        };
        let rank = (picks.iter().find(|p| p.key() == key))
            .map(Pick::rank)
            .or_else(|| layout.paint_rank(key));
        !(top.draws() && Some(top.rank()) > rank)
    }

    // Drops the labels' recorded text bounds when the HUD, scene, screen or shown changes differ
    // from what they were measured for, so no marker uses a stale measurement.
    fn forget_stale_text_bounds(&mut self) {
        let super::Load::Ready(loaded) = &self.load else {
            self.text_bounds.borrow_mut().clear();
            self.text_bounds_for = None;
            return;
        };
        let e = &loaded.editor;
        let current =
            self.text_bounds_for
                .as_ref()
                .is_some_and(|(files, scene, screen, changes)| {
                    Arc::ptr_eq(files, &loaded.files)
                        && (*scene, *screen) == (e.scene(), e.screen())
                        && changes.as_slice() == e.shown_changes()
                });
        if !current {
            self.text_bounds.borrow_mut().clear();
            let stamp = (
                Arc::clone(&loaded.files),
                e.scene(),
                e.screen(),
                e.shown_changes().to_vec(),
            );
            self.text_bounds_for = Some(stamp);
        }
    }

    // What drawing needs to know about the loaded HUD, at canvas `scale` in the chrome's
    // `border` and `pin` colours. Each list is laid out once, since `player_list` lays out the
    // whole scene.
    fn look<'a>(
        &'a self,
        loaded: &'a super::Loaded,
        layout: &'a HudLayout,
        scale: f32,
        border: Hsla,
        pin: Hsla,
    ) -> Look<'a> {
        self.look_for(loaded, loaded.editor.model(), layout, scale, border, pin)
    }

    // The same, for any model and layout laid out at the editor's screen: the fields derived
    // from the model are its own, the rest (images, text, fonts, view) are shared with the HUD.
    // A snapshot that changes an image draws the current image or a missing-image outline.
    fn look_for<'a>(
        &'a self,
        loaded: &'a super::Loaded,
        model: &'a HudModel,
        layout: &'a HudLayout,
        scale: f32,
        border: Hsla,
        pin: Hsla,
    ) -> Look<'a> {
        let screen = loaded.editor.screen();
        let rule = model.scene_rule();
        let list_font = list_font_px(model.scheme(), screen);
        let lists = model
            .parts()
            .iter()
            .filter(|p| {
                p.class()
                    .is_some_and(|c| c.eq_ignore_ascii_case(SECTIONED_LIST))
            })
            .filter_map(|p| {
                let list = model.player_list(p.key(), screen, list_font)?;
                Some((p.key().clone(), list))
            })
            .collect();
        let shown_changes = loaded.editor.shown_changes();
        let anchors = (model.parts().iter())
            .filter(|p| p.name().starts_with(ANCHOR_NAME))
            .filter(|p| model.is_added(shown_changes, p.key()))
            .map(|p| p.key().clone())
            .collect();
        Look {
            model,
            layout,
            screen,
            scale,
            sample: rule.sample(),
            situation: loaded.editor.situation(),
            frame: None,
            anchors,
            images: self.seen_images().unwrap_or(&loaded.images),
            tints: &self.tints,
            text: &loaded.text,
            fonts: &loaded.font_names,
            text_system: &self.text_system,
            ui_family: &self.ui_family,
            root: rule.root(),
            lists,
            list_font,
            border,
            pin,
            vision: self.vision(),
            measure: None,
            chat: model.chat_sample(screen),
            chat_measure: None,
        }
    }

    // While Match size picks, the press names the part whose size the selection copies; it never
    // selects or drags, and a miss or the selection itself ends picking with no change.
    fn pick_size_source(
        &mut self,
        ev: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.matching_size = false;
        self.focus.focus(window, cx);
        let source = self.editor().and_then(|editor| {
            let at = self.canvas_fit(editor.screen()).to_screen(ev.position)?;
            let hit = self.picks_at(at.x, at.y).into_iter().next()?.key().clone();
            (Some(&hit) != editor.selected()).then_some(hit)
        });
        match source {
            Some(source) => self.edit_selected(window, cx, |e| {
                if let Err(err) = e.match_size_selected(&source) {
                    tracing::debug!("the HUD Maker refused a size match: {err:?}");
                }
            }),
            None => cx.notify(),
        }
    }

    // Ends Match size picking, reporting whether it was running, so a press that only stops it
    // does nothing else.
    fn stop_matching_size(&mut self, cx: &mut Context<Self>) -> bool {
        let was = std::mem::take(&mut self.matching_size);
        if was {
            cx.notify();
        }
        was
    }

    // A press on a sample killfeed line selects the killfeed, whose own box takes no clicks,
    // and starts moving it as a press on any other part does.
    fn press_killfeed(&mut self, ev: &MouseDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        if self.space_held || self.stop_matching_size(cx) {
            return;
        }
        let Some(editor) = self.editor_mut() else {
            return;
        };
        editor.select(Some(PartKey::new(KILLFEED)));
        self.refresh(window, cx);
        self.focus.focus(window, cx);
        self.start_part_drag(Handle::Move, ev.position, window, cx);
    }

    // A drag measures from where it began at the scale it began at, so rounding never adds up.
    // A preview some earlier drag left running is kept as its own step first.
    fn start_part_drag(
        &mut self,
        handle: Handle,
        from: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let scale = (self.editor()).map_or(0., |e| self.canvas_fit(e.screen()).scale());
        // A drag measures the real layout, so playback stops and the moment returns to 0.
        self.stop_playing();
        let Some(editor) = self.editor_mut() else {
            return;
        };
        editor.end_drag();
        editor.set_moment(0.);
        self.flush(cx);
        self.focus.focus(window, cx);
        self.part_drag = Some(PartDrag {
            from,
            handle,
            scale,
            pending: None,
            free: false,
            shown: None,
            guides: Vec::new(),
        });
        cx.notify();
    }

    // A move only records the delta; the next frame previews it, so a burst of pointer moves
    // costs one replay. A move with no button held ends the drag, in case the release was lost.
    fn drag_part(&mut self, ev: &MouseMoveEvent, window: &mut Window, cx: &mut Context<Self>) {
        let Some(drag) = &mut self.part_drag else {
            return;
        };
        if ev.pressed_button.is_none() {
            self.end_part_drag(window, cx);
            return;
        }
        let by = ev.position - drag.from;
        let screen_px = |d: Pixels| (f32::from(d) / drag.scale).round() as i32;
        drag.pending = Some((screen_px(by.x), screen_px(by.y)));
        drag.free = ev.modifiers.secondary(cx);
        cx.notify();
    }

    /// Previews the drag's latest delta, if a move left one, snapped unless Ctrl is held.
    pub(super) fn preview_part_drag(&mut self) {
        let Some(drag) = &mut self.part_drag else {
            return;
        };
        let (Some((dx, dy)), handle) = (drag.pending.take(), drag.handle) else {
            return;
        };
        // The threshold is a fixed reach in window pixels, so snapping feels the same at any zoom.
        let snaps = !drag.free && drag.scale > 0.;
        let threshold = (SNAP_WINDOW_PX / drag.scale).round() as i32;
        let snap = (snaps)
            .then(|| {
                self.editor().and_then(|e| match handle {
                    Handle::Move => e.snap_move(dx, dy, threshold),
                    Handle::Left
                    | Handle::Right
                    | Handle::Top
                    | Handle::Bottom
                    | Handle::TopLeft
                    | Handle::TopRight
                    | Handle::BottomLeft
                    | Handle::BottomRight => e.snap_resize(handle, dx, dy, threshold),
                })
            })
            .flatten();
        let (dx, dy, guides) = match snap {
            Some(s) => (s.dx(), s.dy(), s.guides().to_vec()),
            None => (dx, dy, Vec::new()),
        };
        let Some(drag) = &mut self.part_drag else {
            return;
        };
        drag.guides = guides;
        // Pointer moves inside one snap land on the same delta, which needs no second replay.
        if drag.shown == Some((dx, dy)) {
            return;
        }
        drag.shown = Some((dx, dy));
        if let Some(Err(err)) = self.editor_mut().map(|e| e.drag(handle, dx, dy)) {
            tracing::debug!("the HUD Maker refused a drag preview: {err:?}");
        }
    }

    // The release keeps the preview as one undo step and shows the part's new values.
    pub(super) fn end_part_drag(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.part_drag.is_none() {
            return;
        }
        self.preview_part_drag();
        self.part_drag = None;
        if let Some(editor) = self.editor_mut() {
            editor.end_drag();
        }
        self.refresh(window, cx);
    }
}

// What the hover tag and outline labels add for a part that paints nothing in the preview.
const NOTHING_SHOWN: &str = "draws nothing in this preview";

// The hovered part's name on a small tag just above its outline (below it at the screen's top),
// kept on the screen's side the part sits nearer, with a note for a part that paints nothing.
fn hover_tag(
    r: Bounds<Pixels>,
    screen: Size<Pixels>,
    name: String,
    blank: bool,
    label: String,
    (bg, fg, muted, radius): (Hsla, Hsla, Hsla, Pixels),
) -> AnyElement {
    let tall = if blank { HOVER_TAG * 2. } else { HOVER_TAG };
    let tag = div()
        .id(ids::HUD_MAKER_HOVER_NAME)
        .aria_label(label)
        .test_support()
        .absolute()
        .px_1p5()
        .py_0p5()
        .rounded(radius)
        .bg(bg.opacity(0.92))
        .text_color(fg)
        .text_xs()
        .whitespace_nowrap()
        .child(name)
        .when(blank, |tag| {
            tag.child(div().text_color(muted).child(capitalised(NOTHING_SHOWN)))
        });
    let tag = match r.origin.y >= tall + px(2.) {
        true => tag.bottom(screen.height - r.origin.y + px(2.)),
        false => tag.top(r.bottom() + px(2.)),
    };
    match r.center().x > screen.width / 2. {
        true => tag.right((screen.width - r.right()).max(px(0.))),
        false => tag.left(r.origin.x.max(px(0.))),
    }
    .into_any_element()
}

// The height of one line of the hover tag, used to tell whether it fits above the outline.
const HOVER_TAG: Pixels = px(20.);

fn capitalised(text: &str) -> String {
    let mut chars = text.chars();
    chars
        .next()
        .map_or_else(String::new, |c| c.to_uppercase().chain(chars).collect())
}

// Every part the game shows (or, for a selected hidden panel, ghosts) as elements local to the
// screen at `screen_origin`. `selected` decides the ghosts and cut-off roots; a snapshot layer
// passes none, so it draws only what the game would.
fn part_elements(
    model: &HudModel,
    layout: &HudLayout,
    look: &Look,
    fit: &CanvasFit,
    screen_origin: Point<Pixels>,
    selected: Option<&PartKey>,
) -> Vec<AnyElement> {
    let local = |r: Bounds<Pixels>| Bounds {
        origin: r.origin - screen_origin,
        size: r.size,
    };
    // While the selection is a part the game hides, its whole toggled panel draws faintly.
    let ghost = selected.and_then(|key| game_hidden_root(model, layout, key));
    let drawn = |b: &PartBox| {
        b.hidden().is_none()
            || ghost.is_some_and(|root| game_hidden_root(model, layout, b.key()) == Some(root))
    };
    // Each part paints only what the game shows of it, cut at its parents' edges. A part cut
    // off whole paints no content, unless it is the selection: then it and everything in it
    // ghost, each cut only at the parents inside the selection.
    let cut_off = selected.filter(|key| cut_off_whole(layout, key));
    draw_order(model, layout)
        .into_iter()
        .filter_map(|part| {
            let b = layout.get(part.key()).filter(|b| drawn(b))?;
            let r = local(fit.rect(b.x(), b.y(), b.wide(), b.tall()));
            let ghosted = cut_off.is_some_and(|root| is_within(model, part.key(), root));
            let shown = match cut_off {
                Some(root) if ghosted => shown_within(model, layout, part.key(), root),
                _ => layout.shown(part.key()),
            };
            let clip = shown.map(|s| {
                let shown = local(fit.rect(s.x, s.y, s.wide, s.tall));
                Bounds {
                    origin: shown.origin - r.origin,
                    size: shown.size,
                }
            });
            let faint = b.hidden().is_some() || ghosted;
            Some(part_element(part, b, r, clip, look, faint))
        })
        .collect()
}

// The widest the snapshot's caption grows, and the room "Now" needs, in window pixels.
const CAPTION_MAX: f32 = 220.;
const NOW_BOX: f32 = 56.;

// What every part's drawing needs besides the part itself.
struct Look<'a> {
    model: &'a HudModel,
    layout: &'a HudLayout,
    screen: Screen,
    scale: f32,
    sample: SampleSet,
    /// The game moment shown, which fills the health, ammo and charge numbers.
    situation: &'a GameSituation,
    /// What playing animations change at the moment shown; `None` at moment 0.
    frame: Option<&'a AnimFrame>,
    /// The anchors the player added, which draw as a small cross.
    anchors: HashSet<PartKey>,
    images: &'a Images,
    /// Image textures tinted by their draw colour.
    tints: &'a RefCell<super::tint::Tints>,
    /// The game's English text, which a `#token` label draws in place of its token.
    text: &'a GameText,
    /// The text system's family names, lowercased, so a label draws its font only when known.
    fonts: &'a HashSet<String>,
    /// Measures the glyphs of a wrapped label's text.
    text_system: &'a TextSystem,
    /// The family a label without a known font draws in.
    ui_family: &'a SharedString,
    /// The scene's root part, whose player lists draw rows.
    root: Option<&'static str>,
    lists: HashMap<PartKey, PlayerListLayout>,
    list_font: i32,
    border: Hsla,
    pin: Hsla,
    /// The colour-blind view every HUD colour is drawn through.
    vision: Vision,
    /// Where each label records its drawn text's window bounds, while markers need them.
    measure: Option<TextBounds>,
    /// The sample chat lines the chat history draws, when the chat shows.
    chat: Option<ChatSample>,
    /// Where each chat line records its window bounds, while the canvas draws.
    chat_measure: Option<Rc<RefCell<Vec<Bounds<Pixels>>>>>,
}

/// Each label's drawn text in window bounds, by part.
type TextBounds = Rc<RefCell<HashMap<PartKey, Bounds<Pixels>>>>;

impl Look<'_> {
    /// A HUD colour as the view shows it; app chrome never goes through here.
    fn shade(&self, c: Rgba) -> Hsla {
        hsla_of_rgba(self.vision.simulate(c).to_array())
    }
}

// The parts in core's paint order, so what paints on top is also what a click picks first.
fn draw_order<'a>(model: &'a HudModel, layout: &HudLayout) -> Vec<&'a Part> {
    layout
        .paint_order()
        .filter_map(|b| model.part(b.key()))
        .collect()
}

// What a part draws over its colour; `part_element` draws it and a click counts it.
enum Kind<'l> {
    /// The class portrait.
    Portrait,
    /// A rank panel's sample medal, name and level bar.
    Rank,
    /// The chat history's sample lines.
    Chat(&'l ChatSample),
    /// A player list's header and sample rows.
    List(&'l PlayerListLayout),
    /// A scoreboard list the game fills, outlined with a note.
    RowsInGame,
    /// A box the game fills with content the preview cannot draw, outlined with the sample's note.
    GameFilled(&'static str),
    /// A game-filled image's plain square.
    StandIn,
    /// A decoded texture.
    Image(&'l Arc<RenderImage>),
    /// A texture that did not decode, outlined with the part's name.
    MissingImage,
    /// A label's text after the game's words, the sample values and `allcaps`.
    Label(String),
    Nothing,
}

fn look_kind<'l>(part: &Part, look: &'l Look) -> Kind<'l> {
    // Ahead of the label branch, which the history's empty labelText would otherwise take.
    if let Some(chat) = look.chat.as_ref().filter(|c| c.history() == part.key()) {
        return Kind::Chat(chat);
    }
    if part
        .class()
        .is_some_and(|c| c.eq_ignore_ascii_case(MODEL_PANEL))
    {
        return Kind::Portrait;
    }
    if part
        .class()
        .is_some_and(|c| c.eq_ignore_ascii_case(RANK_PANEL))
    {
        return Kind::Rank;
    }
    if let Some(list) = look.lists.get(part.key()) {
        return Kind::List(list);
    }
    if not_a_list(part, look) {
        return Kind::RowsInGame;
    }
    if let Some(note) = look.sample.game_filled(part.name()) {
        return Kind::GameFilled(note);
    }
    if let Some(image) = scene_image(look.model, part, LOCAL_TEAM) {
        // A texture only the game's code names draws nothing when it is absent: no outline.
        return match look.images.get(part.key()) {
            _ if image.trim().is_empty() => Kind::StandIn,
            Some(image) => Kind::Image(image),
            None if part.value("image").is_none() => Kind::Nothing,
            None => Kind::MissingImage,
        };
    }
    // A picture the game's code puts in a part, such as the main menu's class picture.
    if let Some(image) = look.images.get(part.key()) {
        return Kind::Image(image);
    }
    // The game's code sets some labels' text itself, over what the file's labelText says.
    let text = match look.sample.code_label(part.name()) {
        Some(label) => label.text(),
        None => match look.sample.code_text(part.name()) {
            Some(text) => text,
            None => match part.value("labelText") {
                Some(text) => text.text(),
                None => return Kind::Nothing,
            },
        },
    };
    let shown = (look.sample).fill_in(&look.text.show(text), look.situation);
    Kind::Label(match switched_on(part, "allcaps") {
        true => shown.to_uppercase(),
        false => shown,
    })
}

// Whether a part puts anything the player can see on the canvas: a colour, or content that
// is not blank text or a portrait that did not decode.
fn draws(part: &Part, look: &Look) -> bool {
    look.model.fill_colour(part).is_some()
        || border_image(part, look).is_some()
        || match look_kind(part, look) {
            Kind::Portrait => look.images.contains_key(&PartKey::new(MODEL_STAND_IN)),
            Kind::Label(text) => !text.trim().is_empty(),
            Kind::Nothing => false,
            Kind::Rank
            | Kind::Chat(_)
            | Kind::List(_)
            | Kind::RowsInGame
            | Kind::GameFilled(_)
            | Kind::StandIn
            | Kind::Image(_)
            | Kind::MissingImage => true,
        }
}

// What the animations playing change on `part` at the moment shown.
fn played<'a>(look: &Look<'a>, part: &Part) -> Option<&'a Animated> {
    look.frame?.get(part.key())
}

// The name every added anchor starts with, and its cross's size in window pixels.
const ANCHOR_NAME: &str = "TF2ToolsAnchor";
const ANCHOR_CROSS: f32 = 9.;

// A label's text laid out in its box: its alignment, font, size and colour.
fn label_element(
    el: gpui_kit::Div,
    part: &Part,
    b: &PartBox,
    wide: Pixels,
    shown: String,
    look: &Look,
    drawn: &mut Option<SharedString>,
) -> gpui_kit::Div {
    let font = part_font(part, look);
    let size = font_px(font, look, b.tall());
    let shown = drawn.insert(SharedString::from(shown)).clone();
    let (column, row) = alignment(part.value("textAlignment").map_or("", |l| l.text()));
    let el = match row {
        0 => el.flex().items_start(),
        2 => el.flex().items_end(),
        _ => el.flex().items_center(),
    };
    // Core's text inset, placed as the game places it: the text draws at most the box's width
    // less the x inset, then the x inset moves it in from a west or east edge (out past it when
    // negative) and the y inset moves it down. The box clips what the insets push out.
    let inset = look.model.text_inset(part.key(), look.screen);
    let (ix, iy) = (inset.x as f32 * look.scale, inset.y as f32 * look.scale);
    // A label the game widens to its text has no width to cut it at.
    let grows = look.sample.sizes_to_text(part.name());
    let room = match grows {
        true => px(look.screen.wide() as f32 * look.scale),
        false => px((f32::from(wide) - ix).max(0.)),
    };
    let wrapped = wrapped_lines(part, b, &shown, font, look);
    let el = el
        .when(!grows, |el| el.overflow_hidden())
        .whitespace_nowrap()
        .text_size(px(size))
        .line_height(px(size))
        .text_color(
            look.shade(
                played(look, part)
                    .and_then(Animated::fg)
                    .unwrap_or_else(|| {
                        (look.sample.code_label(part.name()))
                            .map_or_else(|| look.model.text_colour(part), |label| label.colour())
                    }),
            ),
        );
    let el = match font.and_then(|f| drawn_family(f.family(), look.fonts)) {
        Some(family) => el.font_family(family),
        None => el,
    };
    let shift = match column {
        _ if wrapped.is_some() => wrapped_text_left(column, inset.x) as f32 * look.scale,
        0 => ix,
        2 => -ix,
        _ => 0.,
    };
    el.child(label_text(
        part.key(),
        shown,
        font,
        TextPlace {
            column,
            shift: point(px(shift), px(iy)),
            // The width the game draws the text in, and a line of room above and below for ink.
            room: gpui_kit::size(room, px(size)),
        },
        wrapped.as_ref(),
        look.measure.as_ref(),
    ))
}

// The lines of a label with `wrap` or `centerwrap` on, `None` for any other. Glyphs measure at
// the screen's own size, not the canvas's, so zooming the canvas never moves a break.
fn wrapped_lines(
    part: &Part,
    b: &PartBox,
    shown: &str,
    font: Option<&FontVariant>,
    look: &Look,
) -> Option<WrappedText> {
    let centred = switched_on(part, "centerwrap");
    if !centred && !switched_on(part, "wrap") {
        return None;
    }
    let family = font.and_then(|f| drawn_family(f.family(), look.fonts));
    let id = (look.text_system).resolve_font(&font_named(family.unwrap_or(look.ui_family.clone())));
    let tall = font_tall(font, look, b.tall());
    let advance = |ch| {
        let glyph = look.text_system.advance(id, px(tall), ch);
        glyph.map_or(tall / 2., |size| f32::from(size.width))
    };
    let room = b
        .wide()
        .saturating_sub(look.model.text_inset(part.key(), look.screen).x);
    let text = shown.replace("\\n", "\n");
    let lines = wrap_lines(&text, room as f32, advance);
    Some(WrappedText {
        lines: lines.into_iter().map(|l| l.to_owned().into()).collect(),
        centred,
    })
}

// A wrapped label's lines, and whether `centerwrap` centres each one.
struct WrappedText {
    lines: Vec<SharedString>,
    centred: bool,
}

// Where a label's text goes: its `textAlignment` column, what moves it from there, and the
// width it draws in with the room left above and below for ink.
struct TextPlace {
    column: usize,
    shift: Point<Pixels>,
    room: Size<Pixels>,
}

fn place<E: gpui_kit::Styled>(el: E, r: Bounds<Pixels>) -> E {
    el.absolute()
        .left(r.origin.x)
        .top(r.origin.y)
        .w(r.size.width)
        .h(r.size.height)
}

// A label draws its text, an image part its texture in the game's pieces, the class model the
// class portrait, and a game-filled image a plain square; a missing texture draws as an outline
// named after the part. Other parts are invisible boxes. The look sits in an
// inner box that clips, so the pin dot on the outer corner is never clipped with it.
fn part_element(
    part: &Part,
    b: &PartBox,
    r: Bounds<Pixels>,
    clip: Option<Bounds<Pixels>>,
    look: &Look,
    faint: bool,
) -> AnyElement {
    let el = div().size_full();
    // A label's drawn text doubles as its accessibility label.
    let mut drawn: Option<SharedString> = None;
    let pin = b.pinned().then(|| {
        div()
            .id(ids::hud_maker_pin(part.key()))
            .test_support()
            .absolute()
            .left(px(-3.))
            .top(px(-3.))
            .size(px(6.))
            .rounded_full()
            .bg(look.pin)
    });
    // The name stays on one line and ends in an ellipsis, small enough to read at 32:9.
    let outline = |el: gpui_kit::Div, text: SharedString| {
        el.border_1()
            .border_color(look.border)
            .overflow_hidden()
            .px_0p5()
            .text_size(PLACEHOLDER_TEXT)
            .line_height(PLACEHOLDER_TEXT)
            .text_color(look.border)
            .child(div().truncate().child(text))
    };
    let name = || SharedString::from(part.name().to_owned());
    // The border's pieces lie under the colour, which sits under the content, filling the box.
    let el = el.children(border_pieces(part, b, look));
    // A part told not to paint its background draws no fill; a rounded one rounds the corners
    // its mask names, at the engine's radius in screen pixels.
    let corners = background_corners(part).unwrap_or(0);
    let radius = px(corner_radius(look.screen) as f32 * look.scale);
    let fill = (played(look, part).and_then(Animated::bg))
        .filter(|c| c.to_array()[3] > 0)
        .or_else(|| look.model.fill_colour(part))
        .filter(|_| paints_background(part));
    let el = el.children(fill.map(|c| {
        div()
            .id(ids::hud_maker_fill(part.key()))
            .test_support()
            .absolute()
            .inset_0()
            .when(corners & 1 != 0, |el| el.rounded_tl(radius))
            .when(corners & 2 != 0, |el| el.rounded_tr(radius))
            .when(corners & 4 != 0, |el| el.rounded_bl(radius))
            .when(corners & 8 != 0, |el| el.rounded_br(radius))
            .bg(look.shade(c))
    }));
    let el = match look_kind(part, look) {
        Kind::Portrait => el.overflow_hidden().children(portrait(part, b, look)),
        Kind::Rank => el.overflow_hidden().children(rank(part, b, look)),
        Kind::Chat(chat) => el.overflow_hidden().children(chat_lines(chat, look)),
        Kind::List(list) => el.child(player_rows(part.key(), list, b.tall(), look)),
        Kind::RowsInGame => outline(el, drawn.insert(ROWS_IN_GAME.into()).clone()),
        Kind::GameFilled(note) => outline(el, drawn.insert(note.into()).clone()),
        Kind::StandIn => el.child(
            div()
                .id(ids::hud_maker_stand_in(part.key()))
                .test_support()
                .size_full()
                .bg(gpui_kit::hsla(0., 0., 0.45, 1.)),
        ),
        Kind::Image(image) => el
            .overflow_hidden()
            .children(image_pieces(part, b, image, look)),
        Kind::MissingImage => outline(el, name()),
        Kind::Label(shown) => label_element(el, part, b, r.size.width, shown, look, &mut drawn),
        Kind::Nothing => el,
    };
    // What the part draws is cut to `clip`, the shown area relative to its box, and left out
    // without one, while the box and its pin keep their whole size.
    let el = clip.map(|clip| {
        let el = el
            .absolute()
            .left(-clip.origin.x)
            .top(-clip.origin.y)
            .w(r.size.width)
            .h(r.size.height);
        place(
            div()
                .id(ids::hud_maker_part_clip(part.key()))
                .test_support(),
            clip,
        )
        .overflow_hidden()
        .child(el)
    });
    // An added anchor paints nothing in game, so the canvas marks its centre with a cross.
    let cross = look.anchors.contains(part.key()).then(|| {
        let (half, arm) = (ANCHOR_CROSS / 2., px(ANCHOR_CROSS));
        let at = point(r.size.width / 2. - px(half), r.size.height / 2. - px(half));
        div()
            .id(ids::hud_maker_anchor(part.key()))
            .test_support()
            .absolute()
            .left(at.x)
            .top(at.y)
            .size(arm)
            .child(
                div()
                    .absolute()
                    .left_0()
                    .top(px(half - 0.5))
                    .w(arm)
                    .h(px(1.))
                    .bg(look.pin),
            )
            .child(
                div()
                    .absolute()
                    .top_0()
                    .left(px(half - 0.5))
                    .h(arm)
                    .w(px(1.))
                    .bg(look.pin),
            )
    });
    let outer = place(div().id(ids::hud_maker_part(part.key())), r);
    let outer = match drawn {
        Some(text) => outer.aria_label(text),
        None => outer,
    };
    let outer = if faint { outer.opacity(0.6) } else { outer };
    // A played alpha fades the part; a played SetVisible hides it.
    let outer = match played(look, part) {
        Some(a) if a.visible() == Some(false) => outer.opacity(0.),
        Some(a) => match a.alpha() {
            Some(alpha) => outer.opacity(f32::from(alpha) / 255.),
            None => outer,
        },
        None => outer,
    };
    outer
        .children(el)
        .children(cross)
        .children(pin)
        .test_support()
        .into_any_element()
}

/// The game-toggled panel that hides `key` in the preview's situation: the part itself when the
/// game hides it, else the nearest ancestor whose hiding by the game passes down to it. `None`
/// for a shown part or one the player or its file hides.
pub(super) fn game_hidden_root<'a>(
    model: &'a HudModel,
    layout: &HudLayout,
    key: &'a PartKey,
) -> Option<&'a PartKey> {
    let mut at = key;
    loop {
        match layout.get(at)?.hidden()? {
            HiddenBy::Game => return Some(at),
            HiddenBy::Parent => at = model.part(at)?.parent()?,
            HiddenBy::SizeZero | HiddenBy::OffScreen | HiddenBy::Visible | HiddenBy::Alpha | _ => {
                return None;
            }
        }
    }
}

// Whether `key` has a box of which nothing would show, cut off whole by its parents or the screen.
fn cut_off_whole(layout: &HudLayout, key: &PartKey) -> bool {
    layout.get(key).is_some() && layout.shown(key).is_none()
}

// Which boxes a click may land on: those the game shows, plus the faint panel of a selected
// part the game hides.
fn takes_clicks<'a>(
    model: &'a HudModel,
    layout: &'a HudLayout,
    selected: Option<&'a PartKey>,
) -> impl Fn(&PartBox) -> bool + 'a {
    let ghost = selected.and_then(|key| game_hidden_root(model, layout, key));
    move |b: &PartBox| {
        b.hidden().is_none()
            || ghost.is_some_and(|root| game_hidden_root(model, layout, b.key()) == Some(root))
    }
}

/// `key`'s group: its nearest enclosing panel that a click may select and that holds two or
/// more parts, as the health panel holds the health number and cross; `None` when none does.
pub(super) fn group_of(
    model: &HudModel,
    layout: &HudLayout,
    key: &PartKey,
    shown: &dyn Fn(&PartBox) -> bool,
) -> Option<PartKey> {
    let mut at = model.part(key)?.parent();
    while let Some(parent) = at {
        if is_group(model, layout, parent, shown) {
            return Some(parent.clone());
        }
        at = model.part(parent)?.parent();
    }
    None
}

fn is_group(
    model: &HudModel,
    layout: &HudLayout,
    key: &PartKey,
    shown: &dyn Fn(&PartBox) -> bool,
) -> bool {
    let takes = layout.get(key).is_some_and(|b| b.clickable() && shown(b));
    let parts = (model.parts().iter())
        .filter(|p| p.parent() == Some(key) && layout.get(p.key()).is_some())
        .take(2)
        .count();
    takes && parts >= 2
}

// Whether a part the game shows inside `key`, at any depth, draws something.
fn paints_inside(model: &HudModel, layout: &HudLayout, key: &PartKey, look: &Look) -> bool {
    (model.parts().iter()).any(|p| {
        p.key() != key
            && layout.get(p.key()).is_some_and(|b| b.hidden().is_none())
            && draws(p, look)
            && is_within(model, p.key(), key)
    })
}

// Whether `key` is `root` or lies anywhere under it.
fn is_within(model: &HudModel, key: &PartKey, root: &PartKey) -> bool {
    let mut at = Some(key);
    while let Some(k) = at {
        if k == root {
            return true;
        }
        at = model.part(k).and_then(|p| p.parent());
    }
    false
}

// `key`'s box cut to each enclosing box up to and including `root`, as the game would clip it
// were `root` on screen; `None` when nothing of it is left.
fn shown_within(
    model: &HudModel,
    layout: &HudLayout,
    key: &PartKey,
    root: &PartKey,
) -> Option<Rect> {
    let edges = |b: &PartBox| {
        let (x, y) = (i64::from(b.x()), i64::from(b.y()));
        (x, y, x + i64::from(b.wide()), y + i64::from(b.tall()))
    };
    let mut cut = edges(layout.get(key)?);
    let mut at = key;
    while at != root {
        at = model.part(at)?.parent()?;
        let e = edges(layout.get(at)?);
        cut = (
            cut.0.max(e.0),
            cut.1.max(e.1),
            cut.2.min(e.2),
            cut.3.min(e.3),
        );
    }
    // Inside the part's own box on every side, so each value fits an i32 as the box's own do.
    if cut.0 >= cut.2 || cut.1 >= cut.3 {
        return None;
    }
    let fit = |v: i64| i32::try_from(v).ok();
    Some(Rect::new(
        fit(cut.0)?,
        fit(cut.1)?,
        fit(cut.2 - cut.0)?,
        fit(cut.3 - cut.1)?,
    ))
}

// Whether screen point (x, y) falls in the selected part's whole box while it paints only as a
// ghost: hidden by the game, or cut off whole by its parents.
fn on_selected_ghost(
    model: &HudModel,
    layout: &HudLayout,
    selected: Option<&PartKey>,
    x: i32,
    y: i32,
) -> bool {
    selected
        .filter(|key| game_hidden_root(model, layout, key).is_some() || cut_off_whole(layout, key))
        .and_then(|key| layout.get(key))
        .is_some_and(|b| {
            (b.x()..b.x().saturating_add(b.wide())).contains(&x)
                && (b.y()..b.y().saturating_add(b.tall())).contains(&y)
        })
}

fn image_pieces(
    part: &Part,
    b: &PartBox,
    image: &Arc<RenderImage>,
    look: &Look,
) -> Vec<AnyElement> {
    // The texture modulated by the colour the game draws it in, such as the dying pulse's red.
    let colour = look.model.draw_colour(part, look.situation);
    let image = &super::tint::drawn(look.tints, image, colour, look.vision);
    let cells = image_cells(part, b.wide(), b.tall(), texture_size(image), look.screen);
    pieces(&cells, image, look, |i| {
        ids::hud_maker_image_cell(part.key(), i)
    })
}

// A decoded texture's size in texels.
fn texture_size(image: &Arc<RenderImage>) -> tf2tools_core::Size<u32> {
    let s = image.size(0);
    tf2tools_core::Size::new(s.width.0.max(0) as u32, s.height.0.max(0) as u32)
}

// Each piece clips a copy of the whole texture, sized and shifted so the piece's texels fill it.
// Explicit sizes keep gpui from giving the image the texture's own aspect ratio.
fn pieces(
    cells: &[ImageCell],
    image: &Arc<RenderImage>,
    look: &Look,
    id: impl Fn(usize) -> gpui_kit::ElementId,
) -> Vec<AnyElement> {
    let texture = texture_size(image);
    let at = |v: i32| px(v as f32 * look.scale);
    cells
        .iter()
        .enumerate()
        .map(|(i, c)| {
            let fx = c.wide() as f32 / c.src_wide() as f32 * look.scale;
            let fy = c.tall() as f32 / c.src_tall() as f32 * look.scale;
            let whole = img(Arc::clone(image))
                .object_fit(ObjectFit::Fill)
                .absolute()
                .left(px(-(c.src_x() as f32) * fx))
                .top(px(-(c.src_y() as f32) * fy))
                .w(px(texture.wide as f32 * fx))
                .h(px(texture.tall as f32 * fy));
            div()
                .id(id(i))
                .test_support()
                .absolute()
                .left(at(c.x()))
                .top(at(c.y()))
                .w(at(c.wide()))
                .h(at(c.tall()))
                .overflow_hidden()
                .child(whole)
                .into_any_element()
        })
        .collect()
}

// The scheme border a part names, when it is a scalable image whose texture decoded.
fn border_image<'l>(
    part: &Part,
    look: &'l Look,
) -> Option<(&'l ImageBorder, &'l Arc<RenderImage>)> {
    let border = look.model.picture_border(part)?;
    let image = look.images.get(&PartKey::new(border.image()))?;
    Some((border, image))
}

// A scalable border's nine pieces over the part's whole box; none for any other border.
fn border_pieces(part: &Part, b: &PartBox, look: &Look) -> Vec<AnyElement> {
    let Some((border, image)) = border_image(part, look) else {
        return Vec::new();
    };
    let cells = nine_slice(
        b.wide(),
        b.tall(),
        texture_size(image),
        border.draw_corner(),
        border.src_corner(),
        look.screen,
    );
    pieces(&cells, image, look, |i| {
        ids::hud_maker_border_cell(part.key(), i)
    })
}

// The class model as the shipped portrait: a square as wide or tall as the box allows, standing
// on the box's bottom edge, centred. It never reaches up under a shown sibling with a higher
// zpos that overlaps it, as the game's camera frames the model below the player lists. The
// class menu's model stands where its own camera puts it. Nothing when the portrait did not decode.
fn portrait(part: &Part, b: &PartBox, look: &Look) -> Option<AnyElement> {
    let image = look.images.get(&PartKey::new(MODEL_STAND_IN))?;
    if look.model.scene() == Scene::ClassSelect {
        return Some(framed_portrait(part, b, image, look));
    }
    let foot = b.y().saturating_add(b.tall());
    let right = b.x().saturating_add(b.wide());
    let ceiling = look
        .model
        .parts()
        .iter()
        .filter(|p| p.parent() == part.parent() && p.key() != part.key())
        .filter_map(|p| look.layout.get(p.key()))
        .filter(|s| s.hidden().is_none() && s.z() > b.z())
        .filter(|s| s.x() < right && b.x() < s.x().saturating_add(s.wide()))
        .map(|s| s.y().saturating_add(s.tall()))
        .filter(|&bottom| bottom > b.y() && bottom < foot)
        .max();
    let room = ceiling.map_or(i32::MAX, |c| foot.saturating_sub(c));
    let side = b.wide().min(b.tall()).min(room).max(0);
    let at = |v: i32| px(v as f32 * look.scale);
    Some(
        div()
            .id(ids::hud_maker_portrait(part.key()))
            .test_support()
            .absolute()
            .bottom_0()
            .left(at((b.wide() - side) / 2))
            .size(at(side))
            .child(
                img(Arc::clone(image))
                    .object_fit(ObjectFit::Fill)
                    .size(at(side)),
            )
            .into_any_element(),
    )
}

// The class menu's model, framed as its camera frames it: the panel's `fov` spans its width at
// the model's distance, and the model stands at the `model` block's offset from the eye line.
// Paint order stays the file's zpos (lower behind; equal zpos, the later block behind), so with
// the stock zpos 6 for both, the buttons paint over the model, which stands below them anyway.
fn framed_portrait(part: &Part, b: &PartBox, image: &Arc<RenderImage>, look: &Look) -> AnyElement {
    let camera = |key: &str, stock: f32| {
        model_block_value(part, look.model, key)
            .or_else(|| part.value(key).and_then(|l| l.text().trim().parse().ok()))
            .unwrap_or(stock)
    };
    let fov = camera("fov", 25.).clamp(1., 179.);
    let ahead = camera("origin_x", 320.).max(1.);
    let units = 2. * ahead * (fov / 2.).to_radians().tan();
    let per_unit = b.wide() as f32 / units;
    let feet = b.tall() as f32 / 2. - camera("origin_z", -49.) * per_unit;
    let side = CLASS_HEIGHT * per_unit;
    // Positive y is the camera's left.
    let centre = b.wide() as f32 / 2. - camera("origin_y", 10.) * per_unit;
    let at = |v: f32| px(v * look.scale);
    div()
        .id(ids::hud_maker_portrait(part.key()))
        .test_support()
        .absolute()
        .left(at(centre - side / 2.))
        .top(at(feet - side))
        .size(at(side))
        .child(
            img(Arc::clone(image))
                .object_fit(ObjectFit::Fill)
                .size(at(side)),
        )
        .into_any_element()
}

/// A standing class's height in world units: the player hull.
const CLASS_HEIGHT: f32 = 82.;

// A number from the model panel's `model` block, the last block in load order that sets it.
fn model_block_value(part: &Part, model: &HudModel, key: &str) -> Option<f32> {
    (part.blocks().iter().rev()).find_map(|(file, path)| {
        let mut path: Vec<&str> = path.iter().map(String::as_str).collect();
        path.push("model");
        let doc = model.document(file)?;
        (doc.find(&path).into_iter())
            .flat_map(|block| block.children())
            .filter(|e| e.key().eq_ignore_ascii_case(key))
            .filter_map(|e| e.text()?.trim().parse().ok())
            .next_back()
    })
}

// The sample rank where the game draws the player's: the medal, a 3D model whose flat picture
// the game does not ship, as the plain stand-in; the name in the game's font and colour; and the
// level bar, a dark track filled green part of the way.
fn rank(part: &Part, b: &PartBox, look: &Look) -> Vec<AnyElement> {
    let pieces = rank_pieces(look.model, part, b.wide(), b.tall(), look.screen);
    // The game draws its medal left of the name, so the stand-in keeps off the text.
    let medal_at = rank_medal(look.model, look.layout, part.key(), look.screen);
    let at = |v: i32| px(v as f32 * look.scale);
    let piece = |id: gpui_kit::ElementId, r: Rect| {
        div()
            .id(id)
            .test_support()
            .absolute()
            .left(at(r.x))
            .top(at(r.y))
            .w(at(r.wide.max(0)))
            .h(at(r.tall.max(0)))
    };
    let scheme = look.model.scheme();
    let medal = pieces.medal().and(medal_at).map(|r| {
        piece(ids::hud_maker_stand_in(part.key()), r)
            .bg(gpui_kit::hsla(0., 0., 0.45, 1.))
            .into_any_element()
    });
    let paint = rank_paint(&pieces, scheme, look.screen.tall());
    // The label's own box holds the aligned text; the outer element shows only the visible part.
    let name = pieces.name().zip(pieces.name_box()).map(|(shown, boxed)| {
        let (col, row) = paint.alignment();
        let font = paint.font();
        let mut inner = div()
            .absolute()
            .left(at(boxed.x - shown.x))
            .top(at(boxed.y - shown.y))
            .w(at(boxed.wide.max(0)))
            .h(at(boxed.tall.max(0)))
            .flex()
            .whitespace_nowrap()
            .text_size(px(font_px(font, look, boxed.tall)))
            .text_color(paint.colour().map_or(gpui_kit::white(), |c| look.shade(c)));
        inner = match col {
            0 => inner.justify_start(),
            1 => inner.justify_center(),
            _ => inner.justify_end(),
        };
        inner = match row {
            0 => inner.items_start(),
            1 => inner.items_center(),
            _ => inner.items_end(),
        };
        if let Some(family) = font.and_then(|f| drawn_family(f.family(), look.fonts)) {
            inner = inner.font_family(family);
        }
        let inner = inner.child(paint.text().to_owned());
        piece(ids::hud_maker_rank_name(part.key()), shown)
            .overflow_hidden()
            .aria_label(paint.text().to_owned())
            .child(inner)
            .into_any_element()
    });
    let bar = pieces.bar().map(|r| {
        piece(ids::hud_maker_rank_bar(part.key()), r)
            .bg(look.shade(paint.track()))
            .into_any_element()
    });
    let fill = pieces.fill().map(|r| {
        piece(ids::hud_maker_rank_fill(part.key()), r)
            .bg(look.shade(paint.fill()))
            .into_any_element()
    });
    [medal, name, bar, fill].into_iter().flatten().collect()
}

// The sample chat inside the history part at core's offsets times the canvas scale, each line
// as wide as its text: the name in its team colour, then the message. Clicks pass to the part.
fn chat_lines(chat: &ChatSample, look: &Look) -> Vec<AnyElement> {
    let at = |v: i32| px(v.max(0) as f32 * look.scale);
    let size = px((chat.line_tall() as f32 * look.scale).max(1.));
    let family = (chat.font()).and_then(|f| drawn_family(f.family(), look.fonts));
    let span =
        |text: &'static str, c: Rgba| div().flex_none().text_color(look.shade(c)).child(text);
    if let Some(measured) = &look.chat_measure {
        (measured.borrow_mut()).resize(chat.lines().len(), Bounds::default());
    }
    (chat.lines().iter().enumerate())
        .map(|(i, line)| {
            let el = div()
                .id(ids::hud_maker_chat_line(i))
                .test_support()
                .aria_label(format!("{}{}", line.name(), line.text()))
                .absolute()
                .left(at(chat.x()))
                .top(at(line.y()))
                .h(at(chat.line_tall()))
                .flex()
                .items_center()
                .whitespace_nowrap()
                .text_size(size)
                .line_height(size)
                .children((look.chat_measure.clone()).map(|m| line_watcher(m, i)))
                .child(span(line.name(), line.name_colour()))
                .child(span(line.text(), line.text_colour()));
            match &family {
                Some(f) => el.font_family(f.clone()),
                None => el,
            }
            .into_any_element()
        })
        .collect()
}

// A `BluePlayerList` or `RedPlayerList` under the scene's root that core lays out no rows for,
// because its class is not the one the game fills.
fn not_a_list(part: &Part, look: &Look) -> bool {
    let named = ["BluePlayerList", "RedPlayerList"]
        .iter()
        .any(|n| part.name().eq_ignore_ascii_case(n));
    named
        && look
            .root
            .is_some_and(|root| part.parent() == Some(&PartKey::new(root)))
        && !look.lists.contains_key(part.key())
}

// The column header and one row per sample player of the list's team, at core's list-relative
// offsets times the canvas scale. A row of no height, or one starting at or past the list's
// bottom, is never built, so a hostile spacing costs nothing; the box clips the rest.
fn player_rows(key: &PartKey, list: &PlayerListLayout, list_tall: i32, look: &Look) -> AnyElement {
    let at = |v: i32| px(v as f32 * look.scale);
    let (x, wide) = (at(list.row_x()), at(list.row_wide().max(0)));
    let font = |tall: i32| px((look.list_font.min(tall) as f32 * look.scale).max(1.));
    let column = |kind: ColumnKind| list.columns().iter().find(|c| c.kind() == kind);
    // A cell spans its column on the row; text in it stays on one line.
    let cell = |kind: ColumnKind| {
        column(kind).map(|c| {
            div()
                .absolute()
                .top_0()
                .h_full()
                .left(at(c.x()))
                .w(at(c.wide().max(0)))
                .flex()
                .items_center()
                .overflow_hidden()
                .whitespace_nowrap()
        })
    };
    let (hy, htall) = list.header();
    // Each title stays whole as in game: Name from its column's left, Score up to its column's
    // right, running left over the empty columns when the column is narrower than the word.
    // Titles come from the game's scoreboard tokens, keeping the plain word when the game lacks
    // one or leaves it empty.
    // The MvM table (the one with a Tour column) has its own columns; the stat titles run up to
    // their column's right edge like Score does.
    let mvm = column(ColumnKind::Tour).is_some();
    let casual_titles = [
        (ColumnKind::Name, "TF_Scoreboard_Name", "Name", "name"),
        (ColumnKind::Score, "TF_Scoreboard_Score", "Score", "score"),
    ];
    let mvm_titles = [
        (ColumnKind::Name, "TF_Scoreboard_Name", "Name", "name"),
        (ColumnKind::Tour, "TF_MvMScoreboard_Tour", "Tour", "tour"),
        (ColumnKind::Score, "TF_Scoreboard_Score", "Score", "score"),
        (
            ColumnKind::Damage,
            "TF_MvMScoreboard_Damage",
            "Damage",
            "damage",
        ),
        (ColumnKind::Tank, "TF_MvMScoreboard_Tank", "Tank", "tank"),
        (
            ColumnKind::Healing,
            "TF_MvMScoreboard_Healing",
            "Healing",
            "healing",
        ),
        (
            ColumnKind::Support,
            "TF_MvMScoreboard_Support",
            "Support",
            "support",
        ),
        (
            ColumnKind::Credits,
            "TF_MvMScoreboard_Money",
            "Money",
            "credits",
        ),
        (ColumnKind::Ping, "TF_Scoreboard_Ping", "Ping", "ping"),
    ];
    let titles = (if mvm {
        &mvm_titles[..]
    } else {
        &casual_titles[..]
    })
    .iter()
    .copied()
    .filter_map(|(kind, token, fallback, name)| {
        let c = column(kind)?;
        // Shown through the token, not the token's value, so a value starting `#` stays literal.
        let title = (look.text.get(token))
            .map(|_| look.text.show(&format!("#{token}")).into_owned())
            .filter(|text| !text.is_empty())
            .unwrap_or_else(|| fallback.to_owned());
        let left_aligned = matches!(kind, ColumnKind::Name | ColumnKind::Class);
        let at_edge = if !left_aligned {
            div().right(wide - at(c.x().saturating_add(c.wide().max(0))))
        } else {
            div().left(at(c.x()))
        };
        Some(
            at_edge
                .id(ids::hud_maker_list_title(key, name))
                .test_support()
                .absolute()
                .top_0()
                .h_full()
                .flex()
                .items_center()
                .whitespace_nowrap()
                .aria_label(title.clone())
                .child(title),
        )
    });
    let header = div()
        .id(ids::hud_maker_list_header(key))
        .test_support()
        .absolute()
        .left(x)
        .top(at(hy))
        .w(wide)
        .h(at(htall.max(0)))
        .border_b_1()
        .border_color(look.shade(PlayerListLayout::HEADER_COLOUR))
        .text_size(font(htall))
        .text_color(look.shade(PlayerListLayout::HEADER_COLOUR))
        .children(titles);

    let players = look.sample.players(list.team());
    let background = look.shade(if mvm {
        PlayerListLayout::MVM_ROW_BACKGROUND
    } else {
        PlayerListLayout::ROW_BACKGROUND
    });
    let class_side = at(ScreenScale::new(look.screen.tall()).scale(13));
    // A dead defender draws grey on a grey row; everyone else in the team's row colour.
    let row_background = |player: &SamplePlayer| match (mvm, player.alive()) {
        (true, false) => look.shade(PlayerListLayout::MVM_DEAD_BACKGROUND),
        _ => background,
    };
    let row_text = |player: &SamplePlayer| match (mvm, player.alive()) {
        (true, false) => look.shade(PlayerListLayout::MVM_DEAD_COLOUR),
        (true, true) => look.shade(Team::Red.row_colour(true, false)),
        _ => look.shade(player.row_colour()),
    };
    let rows = players
        .iter()
        .zip(list.rows(players.len()))
        .enumerate()
        .filter(|&(_, (_, (y, tall)))| tall > 0 && y < list_tall)
        .map(|(i, (player, (y, tall)))| {
            let ping = player
                .ping_image()
                .and_then(|name| look.images.get(&PartKey::new(name)))
                .and_then(|image| {
                    let bar = img(Arc::clone(image)).size_full();
                    let id = ids::hud_maker_list_ping(key, i);
                    Some(
                        cell(ColumnKind::Ping)?
                            .child(div().id(id).test_support().size_full().child(bar)),
                    )
                });
            // A square inset 1 px, as tall as the row or as wide as the column, whichever is less.
            let avatar = column(ColumnKind::Avatar).and_then(|c| {
                let side = (at(tall.min(c.wide())) - px(2.)).max(px(0.));
                let square = div()
                    .id(ids::hud_maker_list_avatar(key, i))
                    .test_support()
                    .flex_none()
                    .size(side)
                    .rounded_sm()
                    .bg(look.border);
                Some(cell(ColumnKind::Avatar)?.justify_center().child(square))
            });
            let name = cell(ColumnKind::Name).map(|c| c.child(player.name()));
            let score = cell(ColumnKind::Score).map(|c| {
                let text = match mvm {
                    true => thousands(i64::from(player.score())),
                    false => player.score().to_string(),
                };
                c.justify_end().child(SharedString::from(text))
            });
            // The MvM stat columns: right-aligned numbers, the surplus as a marker, ping as text.
            let stats = player.mvm().into_iter().flat_map(|m| {
                let text = |v: u32| thousands(i64::from(v));
                let columns = [
                    (ColumnKind::Tour, text(m.tour())),
                    (ColumnKind::Damage, text(m.damage())),
                    (ColumnKind::Tank, text(m.tank())),
                    (ColumnKind::Healing, text(m.healing())),
                    (ColumnKind::Support, text(m.support())),
                    (ColumnKind::Credits, text(m.credits())),
                    (
                        ColumnKind::SquadSurplus,
                        if m.squad_surplus() { "+" } else { "" }.to_owned(),
                    ),
                    (ColumnKind::Ping, player.ping().to_string()),
                ];
                columns.into_iter().filter_map(|(kind, text)| {
                    Some(cell(kind)?.justify_end().child(SharedString::from(text)))
                })
            });
            // Your own team's class icons, a 13-unit square at the column's right edge.
            let class = player
                .class_image(LOCAL_TEAM)
                .and_then(|name| look.images.get(&PartKey::new(name)))
                .and_then(|image| {
                    let icon = div()
                        .id(ids::hud_maker_list_class(key, i))
                        .test_support()
                        .flex_none()
                        .size(class_side)
                        .child(
                            img(Arc::clone(image))
                                .object_fit(ObjectFit::Fill)
                                .size_full(),
                        );
                    Some(cell(ColumnKind::Class)?.justify_end().child(icon))
                });
            div()
                .id(ids::hud_maker_list_row(key, i))
                .aria_label(player.name())
                .test_support()
                .absolute()
                .left(x)
                .top(at(y))
                .w(wide)
                .h(at(tall))
                .bg(row_background(player))
                .text_size(font(tall))
                .line_height(font(tall))
                .text_color(row_text(player))
                .children(avatar)
                .children(name)
                .children(score)
                .children(class)
                .children(ping)
                .children(stats)
        });
    // The rows draw in the scheme's `Default` font, as the game's player list does.
    let family = look
        .model
        .scheme()
        .font(
            if mvm {
                PlayerListLayout::MVM_FONT
            } else {
                PlayerListLayout::ROW_FONT
            },
            look.screen.tall(),
        )
        .and_then(|f| drawn_family(f.family(), look.fonts));
    let lists = div()
        .relative()
        .overflow_hidden()
        .size_full()
        .child(header)
        .children(rows);
    match family {
        Some(family) => lists.font_family(family),
        None => lists,
    }
    .into_any_element()
}

// `n` with a comma between each group of three digits, as the game writes a scoreboard value.
fn thousands(n: i64) -> String {
    let digits = n.unsigned_abs().to_string();
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    if n < 0 {
        out.insert(0, '-');
    }
    out
}

// The sample killfeed over its part's box at core's box-relative screen pixels times the canvas
// scale, each line as wide as its text. Only the lines take presses, so a click on the rest of
// the box still reaches the parts under it. A hidden killfeed or another scene draws none.
fn killfeed_lines(
    look: &Look,
    r: Bounds<Pixels>,
    measured: &Rc<RefCell<Vec<Bounds<Pixels>>>>,
    cx: &Context<HudMakerPage>,
) -> Option<AnyElement> {
    let notices = look.model.death_notices(look.screen)?;
    measured
        .borrow_mut()
        .resize(notices.lines().len(), Bounds::default());
    let at = |v: i32| px(v.max(0) as f32 * look.scale);
    let font = look.model.scheme().font(notices.font(), look.screen.tall());
    let family = font.and_then(|f| drawn_family(f.family(), look.fonts));
    let icon_colour = look.shade(notices.icon_colour());
    let lines = notices.lines().iter().enumerate().map(|(i, line)| {
        let size = px((font
            .map(|f| f.pixel_tall(look.screen.tall()) as f32)
            .unwrap_or(line.tall() as f32 * 0.6)
            * look.scale)
            .max(1.));
        let name =
            |text: &'static str, c: Rgba| div().flex_none().text_color(look.shade(c)).child(text);
        // The game's icon at the line's icon height, its width scaled alike and truncated as
        // the game does; without one, an outline twice as wide as tall.
        let icon = div()
            .id(ids::hud_maker_killfeed_icon(i))
            .test_support()
            .flex_none()
            .h(at(line.icon()));
        let icon = match look.images.get(&kill_icon_key(line.icon_name())) {
            Some(image) => {
                let t = texture_size(image);
                let wide = (t.wide as f32 * line.icon() as f32 / t.tall.max(1) as f32) as i32;
                icon.w(at(wide)).child(
                    img(Arc::clone(image))
                        .object_fit(ObjectFit::Fill)
                        .size_full(),
                )
            }
            None => icon
                .w(at(line.icon().saturating_mul(2)))
                .border_1()
                .border_color(icon_colour),
        };
        let el = div()
            .id(ids::hud_maker_killfeed_line(i))
            .aria_label(format!("{} killed {}", line.killer(), line.victim()))
            .test_support()
            .absolute()
            .top(at(line.y()))
            .h(at(line.tall()))
            .px(at(notices.margin()))
            .flex()
            .items_center()
            .whitespace_nowrap()
            .rounded(at(notices.corner_radius()))
            .bg(look.shade(line.background()))
            .text_size(size)
            .line_height(size)
            .child(line_watcher(Rc::clone(measured), i))
            .child(name(line.killer(), line.killer_colour()))
            .child(icon)
            .child(name(line.victim(), line.victim_colour()))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|page, ev: &MouseDownEvent, window, cx| {
                    cx.stop_propagation();
                    page.press_killfeed(ev, window, cx);
                }),
            );
        let el = match &family {
            Some(f) => el.font_family(f.clone()),
            None => el,
        };
        if notices.right_justify() {
            el.right_0()
        } else {
            el.left_0()
        }
    });
    Some(
        place(div(), r)
            .overflow_hidden()
            .children(lines)
            .into_any_element(),
    )
}

// Records killfeed or chat line `i`'s window bounds each frame, asking for another frame when they
// change, so the panels can keep off the lines. Only the part a clipping box leaves visible counts.
fn line_watcher(measured: Rc<RefCell<Vec<Bounds<Pixels>>>>, i: usize) -> impl IntoElement {
    canvas(
        move |bounds, window, _| {
            let bounds = bounds.intersect(&window.content_mask().bounds);
            let mut lines = measured.borrow_mut();
            if let Some(line) = lines.get_mut(i)
                && *line != bounds
            {
                *line = bounds;
                window.request_animation_frame();
            }
        },
        |_, _, _, _| {},
    )
    .absolute()
    .top_0()
    .left_0()
    .size_full()
}

// `family` when the text system knows it, else its first known stand-in; `known` is lowercased.
pub(super) fn drawn_family(family: &str, known: &HashSet<String>) -> Option<SharedString> {
    std::iter::once(family)
        .chain(stand_in_families(family).iter().copied())
        .find(|f| known.contains(&f.to_lowercase()))
        .map(|f| SharedString::from(f.to_owned()))
}

// The class colour slot (the first Stage 1 colour row) through the scheme; white when unset.
pub(super) fn colour(part: &Part, model: &HudModel) -> Hsla {
    hsla_of_rgba(model.text_colour(part).to_array())
}

// The scheme font variant a part's `font` names at this screen height, if the scheme has it.
fn part_font<'a>(part: &Part, look: &Look<'a>) -> Option<&'a FontVariant> {
    let name = part.value("font")?;
    look.model.scheme().font(name.text(), look.screen.tall())
}

// The font's height on the canvas.
fn font_px(font: Option<&FontVariant>, look: &Look, box_tall: i32) -> f32 {
    (font_tall(font, look, box_tall) * look.scale).max(1.)
}

// The font's height in screen pixels when the font is known, else 60 % of the box.
fn font_tall(font: Option<&FontVariant>, look: &Look, box_tall: i32) -> f32 {
    font.map(|font| font.pixel_tall(look.screen.tall()) as f32)
        .unwrap_or(box_tall as f32 * 0.6)
}

// Whether a 0/1 key is on, read as the label reads it: a number above zero.
fn switched_on(part: &Part, key: &str) -> bool {
    part.value(key)
        .and_then(|l| l.text().trim().parse::<i64>().ok())
        .is_some_and(|v| v > 0)
}

// The (column, row) of `textAlignment` in the 3x3 grid, west and north first, matched whole and
// ignoring case as the game's Label does; unset or unknown is west, centred.
pub(super) fn alignment(text: &str) -> (usize, usize) {
    ALIGNMENTS
        .iter()
        .position(|a| a.eq_ignore_ascii_case(text))
        .map_or((0, 1), |i| (i % 3, i / 3))
}

// The label's text, sized to itself so the alignment places it. A shadow or outline font gets
// black copies offset by one canvas pixel, painted under the text; blur and additive are not shown.
// It sits `down` below where its row puts it. Auto margins place it across `column`; they
// shrink to nothing when the text is wider than its box, so overflowing text starts at the
// west edge as the game's does. Wrapped lines fill the width instead, so `shift` alone places
// them.
fn label_text(
    key: &PartKey,
    shown: SharedString,
    font: Option<&FontVariant>,
    TextPlace {
        column,
        shift,
        room,
    }: TextPlace,
    wrapped: Option<&WrappedText>,
    measure: Option<&TextBounds>,
) -> AnyElement {
    let copy = |dx: f32, dy: f32| {
        let copy = div()
            .absolute()
            .left(px(dx))
            .top(px(dy))
            .text_color(gpui_kit::black());
        text_content(copy, &shown, wrapped, room, None)
    };
    let shadow = font.filter(|f| f.dropshadow()).map(|_| {
        copy(1., 1.)
            .id(ids::hud_maker_text_shadow(key))
            .test_support()
    });
    let outline = font.filter(|f| f.outline()).map(|_| {
        div()
            .id(ids::hud_maker_text_outline(key))
            .test_support()
            .absolute()
            .inset_0()
            .children([(-1., 0.), (1., 0.), (0., -1.), (0., 1.)].map(|(x, y)| copy(x, y)))
    });
    // The text shrinks to its room while its glyphs spill on; the clip around it cuts them at
    // the room's width only, padded a line up and down (and pulled back by as much) so ink
    // above the ascent, descenders and the shadow and outline copies are never cut.
    let watcher = measure.map(|m| text_watcher(Rc::clone(m), key.clone()));
    let text = div()
        .relative()
        .min_w(px(0.))
        .children(outline)
        .children(shadow);
    let text = text_content(text, &shown, wrapped, room, Some(key))
        .id(ids::hud_maker_text(key))
        .aria_label(shown.clone())
        .children(watcher)
        .test_support();
    let clip = div()
        .id(ids::hud_maker_text_clip(key))
        .flex()
        .relative()
        .left(shift.x)
        .top(shift.y)
        .max_w(room.width)
        .py(room.height)
        .my(-room.height)
        .overflow_hidden()
        .flex_none();
    // Text wider than its box loses its auto margins and starts at the west edge, as the
    // game's does.
    let clip = match column {
        _ if wrapped.is_some() => clip,
        1 => clip.mx_auto(),
        2 => clip.ml_auto(),
        _ => clip,
    };
    clip.child(text).test_support().into_any_element()
}

// `el` showing a label's text: the one string, or wrapped lines stacked a font height apart.
// `centerwrap` needs the whole width to centre in. Only the text itself names its lines (`key`),
// not its shadow and outline copies.
fn text_content(
    el: gpui_kit::Div,
    shown: &SharedString,
    wrapped: Option<&WrappedText>,
    room: Size<Pixels>,
    key: Option<&PartKey>,
) -> gpui_kit::Div {
    let Some(wrapped) = wrapped else {
        return el.child(shown.clone());
    };
    let lines = wrapped.lines.iter().enumerate().map(|(i, line)| {
        let drawn = div().flex_none().h(room.height).child(line.clone());
        match key {
            Some(key) => drawn
                .id(ids::hud_maker_text_line(key, i))
                .aria_label(line.clone())
                .test_support()
                .into_any_element(),
            None => drawn.into_any_element(),
        }
    });
    el.flex()
        .flex_col()
        .map(|el| match wrapped.centred {
            true => el.w(room.width).items_center(),
            false => el.items_start(),
        })
        .children(lines)
}

// Records a label's drawn text bounds each frame, asking for another frame when they change,
// so the hard-to-read markers keep off the glyphs.
fn text_watcher(measured: TextBounds, key: PartKey) -> impl IntoElement {
    canvas(
        move |bounds, window, _| {
            let mut texts = measured.borrow_mut();
            if texts.get(&key) != Some(&bounds) {
                texts.insert(key.clone(), bounds);
                window.request_animation_frame();
            }
        },
        |_, _, _, _| {},
    )
    .absolute()
    .top_0()
    .left_0()
    .size_full()
}

/// "Can't show the <scene>" or "This HUD hides the <scene>" for a scene with a root; else None.
/// Problems come first, since a missing file also leaves the root unsized.
pub(super) fn scene_note(model: &HudModel) -> Option<String> {
    let rule = model.scene_rule();
    rule.root()?;
    let scene = rule.label().to_lowercase();
    if !model.problems().is_empty() {
        Some(format!("Can't show the {scene}"))
    } else {
        model
            .hidden_by_hud()
            .map(|_| format!("This HUD hides the {scene}"))
    }
}

// The HUD's own crosshair label when it shows: its symbol in its font, size and colour, in
// its box; `rect` turns a screen box into the canvas's.
fn hud_crosshair_element(
    look: &Look,
    rect: impl Fn(i32, i32, i32, i32) -> Bounds<Pixels>,
) -> Option<AnyElement> {
    let crosshair = look.model.hud_crosshair(look.screen)?;
    if !crosshair.shown() || crosshair.symbol().is_empty() {
        return None;
    }
    let font = look
        .model
        .scheme()
        .font(crosshair.font(), look.screen.tall());
    let size = font_px(font, look, crosshair.tall());
    let symbol = SharedString::from(crosshair.symbol().to_owned());
    let el = place(
        div(),
        rect(
            crosshair.x(),
            crosshair.y(),
            crosshair.wide(),
            crosshair.tall(),
        ),
    )
    .id(ids::HUD_MAKER_HUD_CROSSHAIR)
    .aria_label(symbol.clone())
    .test_support()
    .flex()
    .items_center()
    .justify_center()
    .whitespace_nowrap()
    .text_size(px(size))
    .line_height(px(size))
    .text_color(look.shade(crosshair.colour()));
    let el = match font.and_then(|f| drawn_family(f.family(), look.fonts)) {
        Some(family) => el.font_family(family),
        None => el,
    };
    Some(el.child(symbol).into_any_element())
}

// A small white plus at the screen's middle, where the game draws the crosshair.
fn crosshair(cx: Pixels, cy: Pixels) -> impl IntoElement {
    let arm = px(12.);
    let bar = |w: Pixels, h: Pixels| {
        div()
            .absolute()
            .left((arm - w) / 2.)
            .top((arm - h) / 2.)
            .w(w)
            .h(h)
            .bg(gpui_kit::white())
    };
    div()
        .id(ids::HUD_MAKER_CROSSHAIR)
        .test_support()
        .absolute()
        .left(cx - arm / 2.)
        .top(cy - arm / 2.)
        .size(arm)
        .child(bar(arm, px(2.)))
        .child(bar(px(2.), arm))
}
