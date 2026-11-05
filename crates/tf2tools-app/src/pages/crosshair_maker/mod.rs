//! The Crosshair Maker page: paint a 64x64 or 32x32 crosshair, preview it and save it into
//! the library.

pub mod model;
pub mod starter;
pub mod starter_dialog;

use std::cell::Cell;
use std::rc::Rc;

use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::color_picker::{ColorPicker, ColorPickerEvent, ColorPickerState};
use gpui_kit::component::input::{Input, InputState};
use gpui_kit::component::select::{Select, SelectState};
use gpui_kit::component::slider::{Slider, SliderState};
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::component::{
    ActiveTheme as _, Disableable as _, IndexPath, Selectable as _, WindowExt as _, h_flex, v_flex,
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, AnyWindowHandle, AppContext as _, Context, CursorStyle, DispatchPhase, Entity,
    FocusHandle, FontWeight, Hsla, InteractiveElement as _, IntoElement, MouseButton,
    MouseDownEvent, MouseExitEvent, MouseMoveEvent, MouseUpEvent, ObjectFit, ParentElement as _,
    Pixels, Point, Render, ScrollHandle, ScrollWheelEvent, SharedString, Size,
    StatefulInteractiveElement as _, Styled as _, StyledImage as _, Subscription,
    TestSupportExt as _, WeakEntity, Window, canvas, div, fill, img, point, px, size,
};

use tf2tools_core::{
    CrosshairId, CrosshairRef, FitChange, LogLevel, NewLogEntry, PaintError, PaintTarget,
    RgbaImage, fit_to_maker, read_png,
};

use crate::activity;
use crate::assets::BACKGROUNDS;
use crate::ids;
use crate::keymap::{
    FitGrid, MAKER_CONTEXT, RedoDrawing, UndoDrawing, UseCircle, UseEraser, UseFill, UseLine,
    UsePencil, UseRectangle, ZoomInGrid, ZoomOutGrid,
};
use crate::library_events::LibraryEvents;
use crate::local_messages::{
    PaintStep, describe_library, describe_paint, describe_png_read, describe_settings,
};
use crate::picker::PickKind;
use crate::services::{OpError, Services, run_blocking};
use crate::shell::CONTENT_PADDING;
use crate::shell::confirm::{Confirm, confirm};
use crate::shell::route::Page;
use crate::shell::toast::{ToastKind, toast, toast_error};
use crate::widgets::scroll::scroll_area;
use model::{
    Editor, FIT_TIP, OPACITY_TIP, REDO_TIP, SWATCH_NAMES, SWATCHES, Tool, UNDO_TIP, Viewport,
    ZOOM_IN_TIP, ZOOM_OUT_TIP, alpha_of_percent, colour_hint, fit_note, hsla_of, hsla_of_rgba,
    percent_of_alpha, position_text, preview_texels, rgb_of, swatch_tooltip, wheel_scroll,
    wheel_zoom_steps,
};
use starter_dialog::StarterDialog;

/// The side panel's width; the grid takes the rest.
const PANEL_WIDTH: Pixels = px(280.);
/// The side panel's width in a window narrower than NARROW_WINDOW.
const NARROW_PANEL_WIDTH: Pixels = px(232.);
const NARROW_WINDOW: Pixels = px(1024.);
/// A toolbar group's lead: the divider's width with the space around it, 4 + 1 + 4 + 4.
const DIVIDER_LEAD: Pixels = px(13.);
/// The preview box, as on the Crosshairs page; the texture inside is drawn at its 64 px.
const PREVIEW_SIZE: Pixels = px(128.);
const SAVE_FAILED: &str = "Failed to save crosshair";
const OPEN_FAILED: &str = "Failed to open crosshair";
const COPY_FAILED: &str = "Failed to copy crosshair";
const PNG_IMPORT_FAILED: &str = "Failed to import PNG";
/// The toast for a picture (starter, copy or PNG) that arrives while a save or open runs.
pub const BUSY: &str = "Wait for the Crosshair Maker to finish, then try again.";
const RECOVERED: &str = "The saved drawing was damaged, so it was rebuilt from the crosshair file.";

/// How a save ended, when it did not fail with a toast.
enum Saved {
    /// Saved as `id` under `name`; `assigned` when a re-saved crosshair is on some weapon.
    Done {
        id: CrosshairId,
        name: String,
        assigned: bool,
    },
    /// Another crosshair has this trimmed name; the field says so.
    NameTaken(String),
    /// The crosshair being re-saved was deleted meanwhile; the next save makes a new one.
    Gone(OpError),
}

/// The Crosshair Maker page over one `Editor`.
pub struct CrosshairMakerPage {
    win: AnyWindowHandle,
    /// Tracked on the page root, so the maker's shortcuts work while the page has focus.
    focus: FocusHandle,
    editor: Editor,
    /// The free space beside the toolbar the grid is fitted into, as last painted.
    free: Rc<Cell<Size<Pixels>>>,
    /// The view's top-left in window coordinates, as last painted.
    view_origin: Rc<Cell<Point<Pixels>>>,
    /// The grid's zoom and scroll inside the view.
    view: Viewport,
    /// Touchpad travel gathered toward the next Ctrl+wheel zoom level.
    zoom_carry: Pixels,
    /// While a right or middle drag holds the grid, the pointer's last window position.
    pan: Option<Point<Pixels>>,
    colour: Entity<ColorPickerState>,
    /// The paint's opacity in whole percents; the editor keeps the exact alpha.
    opacity: Entity<SliderState>,
    preview_colour: Entity<ColorPickerState>,
    /// The tint the preview multiplies the drawing by, as `cl_crosshair_rgb` would.
    preview_rgb: [u8; 3],
    outline_colour: Entity<ColorPickerState>,
    /// The colour "Add outline" paints with.
    outline_rgb: [u8; 3],
    background: Entity<SelectState<Vec<&'static str>>>,
    name: Entity<InputState>,
    /// The trimmed name core last refused as taken, and the line shown under the field.
    name_error: Option<(String, String)>,
    saving: bool,
    // Whether the running save came from Save as new, for that button's label.
    saving_as_new: bool,
    /// True while `open` loads a drawing.
    opening: bool,
    /// True while Import PNG's file dialog is open, so a second click opens no second one.
    picking: bool,
    /// Whether the centre guides show over the grid.
    guides: bool,
    /// The cell under the pointer, for the position readout.
    hover: Option<(u32, u32)>,
    /// Whether lines show between the cells; remembered in settings.
    grid_lines: bool,
    /// Set once the player toggles the lines, so the stored choice arriving late never undoes it.
    grid_lines_touched: bool,
    /// The starter shapes dialog; the kit's dialog owns it, so it lapses once closed.
    starter: Option<WeakEntity<StarterDialog>>,
    panel_scroll: ScrollHandle,
    _subscriptions: Vec<Subscription>,
}

impl CrosshairMakerPage {
    /// A page with an empty 64x64 drawing.
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let editor = Editor::new();
        let editor_side = editor.size().side();
        let white = hsla_of([255, 255, 255]);
        let colour =
            cx.new(|cx| ColorPickerState::new(window, cx).default_value(hsla_of(editor.colour())));
        let preview_colour = cx.new(|cx| ColorPickerState::new(window, cx).default_value(white));
        let outline_colour =
            cx.new(|cx| ColorPickerState::new(window, cx).default_value(hsla_of([0, 0, 0])));
        let opacity = cx.new(|_| {
            SliderState::new()
                .min(1.)
                .max(100.)
                .step(1.)
                .default_value(100.)
        });
        // A cleared picker (Change(None)) leaves the colour as it was.
        let mut subscriptions = vec![
            cx.subscribe_in(&colour, window, |page, _, ev: &ColorPickerEvent, _, cx| {
                let ColorPickerEvent::Change(Some(c)) = ev else {
                    return;
                };
                page.editor.set_colour(rgb_of(*c));
                cx.notify();
            }),
            cx.subscribe_in(
                &preview_colour,
                window,
                |page, _, ev: &ColorPickerEvent, _, cx| {
                    let ColorPickerEvent::Change(Some(c)) = ev else {
                        return;
                    };
                    page.preview_rgb = rgb_of(*c);
                    cx.notify();
                },
            ),
            cx.subscribe_in(
                &outline_colour,
                window,
                |page, _, ev: &ColorPickerEvent, _, cx| {
                    let ColorPickerEvent::Change(Some(c)) = ev else {
                        return;
                    };
                    page.outline_rgb = rgb_of(*c);
                    cx.notify();
                },
            ),
        ];
        // The slider only notifies, so it is observed. A percent the editor's alpha already
        // shows keeps that alpha, so an exact picked alpha is not rounded away.
        subscriptions.push(cx.observe(&opacity, |page, slider, cx| {
            let percent = slider.read(cx).value().start().round().clamp(1., 100.) as u32;
            if percent != percent_of_alpha(page.editor.alpha()) {
                page.editor.set_alpha(alpha_of_percent(percent));
            }
            cx.notify();
        }));
        // Read at render, so a background change needs no subscription.
        let background = cx.new(|cx| {
            SelectState::new(BACKGROUNDS.to_vec(), Some(IndexPath::default()), window, cx)
        });
        let focus = cx.focus_handle();
        focus.focus(window, cx);
        // A failed read keeps the lines off; the player can still turn them on.
        let stored = run_blocking(cx, |s| {
            s.db.settings()
                .map(|st| st.maker_grid_lines())
                .map_err(|e| describe_settings(&e).into_op("Load grid lines"))
        });
        cx.spawn(async move |page, cx| {
            if let Ok(on) = stored.await {
                let _ = page.update(cx, |page, cx| {
                    if !page.grid_lines_touched {
                        page.grid_lines = on;
                        cx.notify();
                    }
                });
            }
        })
        .detach();
        Self {
            win: window.window_handle(),
            focus,
            editor,
            free: Rc::new(Cell::new(Size::default())),
            view_origin: Rc::new(Cell::new(Point::default())),
            view: Viewport::new(Size::default(), editor_side),
            zoom_carry: px(0.),
            pan: None,
            colour,
            opacity,
            preview_colour,
            preview_rgb: [255, 255, 255],
            outline_colour,
            outline_rgb: [0, 0, 0],
            background,
            name: cx.new(|cx| InputState::new(window, cx).placeholder("Crosshair name")),
            name_error: None,
            saving: false,
            saving_as_new: false,
            opening: false,
            picking: false,
            guides: true,
            hover: None,
            grid_lines: false,
            grid_lines_touched: false,
            starter: None,
            panel_scroll: ScrollHandle::new(),
            _subscriptions: subscriptions,
        }
    }

    /// The drawing and its editing state.
    pub fn editor(&self) -> &Editor {
        &self.editor
    }

    /// True while the centre guides show; on for a new page.
    pub fn guides(&self) -> bool {
        self.guides
    }

    /// True while lines show between the cells.
    pub fn grid_lines(&self) -> bool {
        self.grid_lines
    }

    // Flips the lines at once and remembers the choice off the UI thread, as the sidebar does.
    fn toggle_grid_lines(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.grid_lines = !self.grid_lines;
        self.grid_lines_touched = true;
        cx.notify();
        let on = self.grid_lines;
        let save = run_blocking(cx, move |s| {
            s.db.set_maker_grid_lines(on)
                .map_err(|e| describe_settings(&e).into_op("Save grid lines"))
        });
        cx.spawn_in(window, async move |_, cx| {
            if let Err(err) = save.await {
                let _ = cx.update(|window, cx| toast_error(window, cx, &err));
            }
        })
        .detach();
    }

    /// The crosshair name field.
    pub fn name_input(&self) -> &Entity<InputState> {
        &self.name
    }

    /// Gives the keys back to the page unless something inside it already holds them.
    pub fn reclaim_focus(&self, window: &mut gpui_kit::Window, cx: &mut gpui_kit::App) {
        if !self.focus.contains_focused(window, cx) {
            self.focus.focus(window, cx);
        }
    }

    /// True while the drawing holds no unsaved changes.
    pub fn can_leave(&self) -> bool {
        !self.editor.is_dirty()
    }

    /// False only while an open is in flight.
    pub fn is_loaded(&self) -> bool {
        !self.opening
    }

    /// Loads the painted crosshair off the UI thread; the drawing replaces the canvas.
    pub fn open(&mut self, id: CrosshairId, cx: &mut Context<Self>) {
        self.opening = true;
        cx.notify();
        let loading = run_blocking(cx, move |s| open_job(s, id));
        let win = self.win;
        cx.spawn(async move |page, cx| {
            let loaded = loading.await;
            let opened = page
                .update(cx, |page, cx| {
                    page.opening = false;
                    cx.notify();
                    let (mut editor, recovered) = loaded?;
                    let name = editor.name().to_owned();
                    editor.set_alpha(page.editor.alpha());
                    page.editor = editor;
                    page.hover = None;
                    page.name_error = None;
                    Ok((page.name.clone(), name, recovered))
                })
                .ok();
            cx.update(|cx| {
                let _ = win.update(cx, |_, window, cx| match opened {
                    Some(Ok((input, name, recovered))) => {
                        input.update(cx, |st, cx| st.set_value(name, window, cx));
                        if recovered {
                            toast(window, cx, ToastKind::Warning, RECOVERED, "");
                        }
                    }
                    Some(Err(err)) => toast_error(window, cx, &err),
                    None => {}
                });
            });
        })
        .detach();
    }

    /// True while a save runs in the background.
    pub fn is_saving(&self) -> bool {
        self.saving
    }

    // A running save or open locks everything that changes the drawing, so a save cannot
    // claim edits it never wrote and a load cannot silently replace them.
    fn busy(&self) -> bool {
        self.saving || self.opening
    }

    /// True while a right or middle drag holds the grid.
    pub fn is_panning(&self) -> bool {
        self.pan.is_some()
    }

    /// The grid's zoom and scroll.
    pub fn viewport(&self) -> &Viewport {
        &self.view
    }

    /// The cell size the grid last drew.
    pub fn cell_size(&self) -> Pixels {
        self.view.cell()
    }

    /// The paint colour picker.
    pub fn colour_state(&self) -> &Entity<ColorPickerState> {
        &self.colour
    }

    /// The opacity slider, 1 to 100 percent, at 100 for a new page.
    pub fn opacity_state(&self) -> &Entity<SliderState> {
        &self.opacity
    }

    /// The preview tint picker, white until the player picks another.
    pub fn preview_colour_state(&self) -> &Entity<ColorPickerState> {
        &self.preview_colour
    }

    /// The outline colour picker, black until the player picks another.
    pub fn outline_colour_state(&self) -> &Entity<ColorPickerState> {
        &self.outline_colour
    }

    /// The tint the preview shows the drawing under.
    pub fn preview_colour(&self) -> [u8; 3] {
        self.preview_rgb
    }

    // Starts over on a blank 64x64 canvas, asking first when that would drop unsaved changes.
    fn new_drawing(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.can_leave() {
            self.reset(window, cx);
            return;
        }
        // A dialog covers the grid, so the readout clears and waits for it to close.
        self.unhover(cx);
        let answer = confirm(Confirm::discard_changes(), window, cx);
        cx.spawn_in(window, async move |page, cx| {
            if answer.recv().await.unwrap_or(false) {
                let _ = page.update_in(cx, |page, window, cx| page.reset(window, cx));
            }
        })
        .detach();
    }

    /// Puts `drawing` on the canvas through `Editor::use_starter`, asking first through
    /// `Confirm::discard_changes()` when the drawing has unsaved changes; also clears the name
    /// field, the taken-name line and the hover readout. While `busy()` it only toasts `BUSY`.
    pub fn use_starter(&mut self, drawing: RgbaImage, window: &mut Window, cx: &mut Context<Self>) {
        self.take_new(drawing, None, window, cx);
    }

    /// Loads `crosshair`'s pixels off the UI thread (`opening` set meanwhile) and puts them on
    /// the canvas through `take_new`; a failure toasts "Failed to copy crosshair" and changes
    /// nothing.
    pub fn copy_from(&mut self, crosshair: CrosshairRef, cx: &mut Context<Self>) {
        self.opening = true;
        cx.notify();
        let loading = run_blocking(cx, move |s| copy_job(s, crosshair));
        let win = self.win;
        cx.spawn(async move |page, cx| {
            let loaded = loading.await;
            // Like `open`: the page may be gone by now, and then the copy is simply dropped.
            let landed = page
                .update(cx, |page, cx| {
                    page.opening = false;
                    cx.notify();
                })
                .is_ok();
            if !landed {
                return;
            }
            cx.update(|cx| {
                let _ = win.update(cx, |_, window, cx| match loaded {
                    Ok((drawing, change)) => {
                        let _ = page.update(cx, |page, cx| {
                            page.take_new(drawing, fit_note(change), window, cx)
                        });
                    }
                    Err(err) => toast_error(window, cx, &err),
                });
            });
        })
        .detach();
    }

    // Picks a PNG, then reads and fits it off the UI thread before `take_new`, so the confirm
    // for unsaved changes comes after the decode. `opening` is set as soon as a path arrives;
    // one that arrives while a save or open runs is turned away with a toast.
    fn import_png(&mut self, cx: &mut Context<Self>) {
        if self.busy() || self.picking {
            return;
        }
        self.picking = true;
        cx.notify();
        let picking = cx.global::<Services>().picker.pick(PickKind::MakerPng, cx);
        let win = self.win;
        cx.spawn(async move |page, cx| {
            let picked = picking.await;
            let Ok(loading) = page.update(cx, |page, cx| {
                page.picking = false;
                cx.notify();
                let path = picked?;
                if page.busy() {
                    return Some(Err(()));
                }
                page.opening = true;
                Some(Ok(run_blocking(cx, move |_| {
                    read_png(&path)
                        .map(fit_to_maker)
                        .map_err(|e| describe_png_read(&e).into_op(PNG_IMPORT_FAILED))
                })))
            }) else {
                return;
            };
            let loading = match loading {
                None => return,
                Some(Ok(loading)) => loading,
                Some(Err(())) => {
                    cx.update(|cx| {
                        let _ = win.update(cx, |_, window, cx| {
                            toast(window, cx, ToastKind::Info, BUSY, "")
                        });
                    });
                    return;
                }
            };
            let loaded = loading.await;
            // Like `copy_from`: a page dropped meanwhile just drops the picture.
            let landed = page
                .update(cx, |page, cx| {
                    page.opening = false;
                    cx.notify();
                })
                .is_ok();
            if !landed {
                return;
            }
            cx.update(|cx| {
                let _ = win.update(cx, |_, window, cx| match loaded {
                    Ok((drawing, change)) => {
                        let _ = page.update(cx, |page, cx| {
                            page.take_new(drawing, fit_note(change), window, cx)
                        });
                    }
                    Err(err) => toast_error(window, cx, &err),
                });
            });
        })
        .detach();
    }

    // Every new drawing (starter, copy, PNG) lands here: asks before dropping unsaved changes,
    // then lays the pixels on as a new unnamed crosshair and toasts `note` once they are on.
    // One that lands while a save or open runs is turned away with a toast.
    fn take_new(
        &mut self,
        drawing: RgbaImage,
        note: Option<&'static str>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.busy() {
            toast(window, cx, ToastKind::Info, BUSY, "");
            return;
        }
        if self.can_leave() {
            self.apply_new(&drawing, note, window, cx);
            return;
        }
        // A dialog covers the grid, so the readout clears and waits for it to close.
        self.unhover(cx);
        let answer = confirm(Confirm::discard_changes(), window, cx);
        cx.spawn_in(window, async move |page, cx| {
            if answer.recv().await.unwrap_or(false) {
                let _ = page.update_in(cx, |page, window, cx| {
                    page.apply_new(&drawing, note, window, cx)
                });
            }
        })
        .detach();
    }

    // The picture replaces the drawing as a new, unnamed crosshair; tool, paint and mirrors stay.
    fn apply_new(
        &mut self,
        drawing: &RgbaImage,
        note: Option<&'static str>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // A picture the editor refuses (not a 32 or 64 square) leaves the name as it was.
        if self.editor.use_starter(drawing) {
            self.forget_name(window, cx);
            if let Some(note) = note {
                toast(window, cx, ToastKind::Info, note, "");
            }
        }
    }

    /// The starter shapes dialog while it is open.
    pub fn starter_dialog(&self) -> Option<Entity<StarterDialog>> {
        self.starter.as_ref().and_then(WeakEntity::upgrade)
    }

    // The starter is drawn in the paint as it is now and at the canvas's current size.
    fn open_starters(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy() {
            return;
        }
        self.unhover(cx);
        let dialog = StarterDialog::open(
            cx.entity().downgrade(),
            self.editor.size(),
            self.editor.paint_rgba(),
            window,
            cx,
        );
        self.starter = Some(dialog.downgrade());
    }

    // A blank canvas that keeps the opacity the slider shows.
    fn reset(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let alpha = self.editor.alpha();
        self.editor = Editor::new();
        self.editor.set_alpha(alpha);
        self.forget_name(window, cx);
    }

    // A fresh drawing has no name yet, so the field, its taken-name line and the readout clear.
    fn forget_name(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.hover = None;
        self.name_error = None;
        self.name.update(cx, |st, cx| st.set_value("", window, cx));
        cx.notify();
    }

    // The picker shows the swatch too; its set_value emits nothing, so the editor is set here.
    fn pick_swatch(&mut self, rgb: [u8; 3], window: &mut Window, cx: &mut Context<Self>) {
        self.editor.set_colour(rgb);
        self.colour
            .update(cx, |state, cx| state.set_value(hsla_of(rgb), window, cx));
        cx.notify();
    }

    fn trimmed_name(&self, cx: &Context<Self>) -> String {
        self.name.read(cx).value().trim().to_owned()
    }

    // Saves the drawing as it is now, over the open crosshair or, with `as_new`, always as a
    // new one, in the background; the result marks the editor saved, names the taken name,
    // or toasts.
    fn save(&mut self, as_new: bool, cx: &mut Context<Self>) {
        let name = self.trimmed_name(cx);
        let drawing = self.editor.drawing();
        let sent = drawing.clone();
        let open = if as_new {
            None
        } else {
            self.editor.open_id().cloned()
        };
        self.saving = true;
        self.saving_as_new = as_new;
        cx.notify();
        let running = run_blocking(cx, move |s| save_job(s, open, &name, &drawing));
        let win = self.win;
        cx.spawn(async move |page, cx| {
            let done = running.await;
            let _ = page.update(cx, |page, cx| {
                page.saving = false;
                cx.notify();
            });
            let toast_of = match done {
                Ok(Saved::Done { id, name, assigned }) => {
                    let _ = page.update(cx, |page, cx| {
                        page.editor.mark_saved(id, name, &sent);
                        page.name_error = None;
                        cx.notify();
                    });
                    cx.update(LibraryEvents::changed);
                    let description = if assigned {
                        "Press Install on the Crosshairs page to update it in TF2."
                    } else {
                        ""
                    };
                    Ok(description)
                }
                Ok(Saved::NameTaken(taken)) => {
                    let _ = page.update(cx, |page, cx| {
                        let line = describe_paint(
                            &PaintError::NameTaken {
                                name: taken.clone(),
                            },
                            PaintStep::SaveNew,
                        );
                        page.name_error = Some((taken, line.message));
                        cx.notify();
                    });
                    return;
                }
                Ok(Saved::Gone(err)) => {
                    let _ = page.update(cx, |page, _| page.editor.forget_open());
                    Err(err)
                }
                Err(err) => Err(err),
            };
            cx.update(|cx| {
                let _ = win.update(cx, |_, window, cx| match toast_of {
                    Ok(description) => toast(window, cx, ToastKind::Success, "Saved", description),
                    Err(err) => toast_error(window, cx, &err),
                });
            });
        })
        .detach();
    }

    // The name field, the taken-name line under it while the field still holds that name,
    // and Save and Save as new, which wait for a name and for any running save.
    fn save_row(&self, cx: &mut Context<Self>) -> AnyElement {
        let danger = cx.theme().danger;
        let name = self.trimmed_name(cx);
        let error = self
            .name_error
            .as_ref()
            .filter(|(taken, _)| *taken == name)
            .map(|(_, line)| {
                div()
                    .id(ids::CROSSHAIR_MAKER_NAME_ERROR)
                    .aria_label(line.clone())
                    .test_support()
                    .text_xs()
                    .text_color(danger)
                    .child(line.clone())
            });
        // Only the clicked button spins and is announced as "Saving..."; both wait while either
        // runs. The visible labels stay put, so the row never widens past a narrow panel.
        let busy = self.busy();
        let saving_new = self.saving && self.saving_as_new;
        let saving_over = self.saving && !self.saving_as_new;
        v_flex()
            .gap_2()
            .child(heading("Name"))
            .child(
                div()
                    .id(ids::CROSSHAIR_MAKER_NAME)
                    .aria_label("Crosshair name")
                    .test_support()
                    .child(Input::new(&self.name)),
            )
            .children(error)
            .child(
                h_flex()
                    .gap_2()
                    .child(
                        Button::new(ids::CROSSHAIR_MAKER_SAVE)
                            .primary()
                            .icon(IconName::Save)
                            .label("Save")
                            .when(saving_over, |b| b.accessibility_label("Saving..."))
                            .loading(saving_over)
                            .disabled(name.is_empty() || busy)
                            .on_click(cx.listener(|page, _, _, cx| page.save(false, cx))),
                    )
                    .child(
                        Button::new(ids::CROSSHAIR_MAKER_SAVE_AS_NEW)
                            .outline()
                            .icon(IconName::Copy)
                            .label("Save as new")
                            .when(saving_new, |b| b.accessibility_label("Saving..."))
                            .loading(saving_new)
                            .disabled(name.is_empty() || busy)
                            .on_click(cx.listener(|page, _, _, cx| page.save(true, cx))),
                    ),
            )
            .into_any_element()
    }

    fn panel(&self, width: Pixels, cx: &mut Context<Self>) -> impl IntoElement {
        let save = self.save_row(cx);
        let theme = cx.theme();
        let (border, muted_fg) = (theme.border, theme.muted_foreground);
        let colour = self.editor.colour();
        let hint = colour_hint(colour);
        let opacity = format!("Opacity: {}%", percent_of_alpha(self.editor.alpha()));
        let background = self
            .background
            .read(cx)
            .selected_value()
            .copied()
            .unwrap_or(BACKGROUNDS[0]);
        let swatches = SWATCHES
            .iter()
            .zip(SWATCH_NAMES)
            .enumerate()
            .map(|(ix, (&rgb, name))| {
                Button::new(ids::crosshair_maker_swatch(ix))
                    .ghost()
                    .selected(colour == rgb)
                    .tooltip(swatch_tooltip(rgb, name))
                    .accessibility_label(name)
                    .child(
                        div()
                            .size(px(20.))
                            .rounded_sm()
                            .border_1()
                            .border_color(border)
                            .bg(hsla_of(rgb)),
                    )
                    .on_click(
                        cx.listener(move |page, _, window, cx| page.pick_swatch(rgb, window, cx)),
                    )
            });
        // The panel spans the row and scrolls, so a short window still reaches its last control.
        let content = v_flex()
            .id(ids::CROSSHAIR_MAKER_PANEL)
            .test_support()
            .size_full()
            .min_h_0()
            .track_scroll(&self.panel_scroll)
            .overflow_y_scroll()
            // The panel has no padding of its own, so this keeps its controls clear of the bar.
            .pr_4()
            .gap_4()
            .child(save)
            .child(
                v_flex()
                    .gap_2()
                    .child(heading("Colour"))
                    .child(h_flex().gap_1().children(swatches))
                    // One line under the swatches says what the current colour does in game.
                    .child(
                        div()
                            .id(ids::CROSSHAIR_MAKER_COLOR_HINT)
                            .aria_label(hint)
                            .test_support()
                            .text_xs()
                            .text_color(muted_fg)
                            .child(hint),
                    )
                    .child(
                        h_flex()
                            .id(ids::CROSSHAIR_MAKER_COLOR)
                            .aria_label("Paint colour")
                            .test_support()
                            .gap_2()
                            .child(
                                ColorPicker::new(&self.colour).accessibility_label("Paint colour"),
                            )
                            .child(div().text_sm().child("Any colour")),
                    )
                    .child(
                        div()
                            .id(ids::CROSSHAIR_MAKER_OPACITY_LABEL)
                            .aria_label(opacity.clone())
                            .test_support()
                            .text_sm()
                            .child(opacity),
                    )
                    .child(
                        div()
                            .id(ids::CROSSHAIR_MAKER_OPACITY)
                            .aria_label("Opacity")
                            .test_support()
                            .tooltip(|window, cx| Tooltip::new(OPACITY_TIP).build(window, cx))
                            .child(Slider::new(&self.opacity)),
                    ),
            )
            .child(
                v_flex()
                    .gap_2()
                    .child(heading("Outline"))
                    .child(
                        h_flex()
                            .id(ids::CROSSHAIR_MAKER_OUTLINE_COLOR)
                            .aria_label("Outline colour")
                            .test_support()
                            .gap_2()
                            .child(
                                ColorPicker::new(&self.outline_colour)
                                    .accessibility_label("Outline colour"),
                            )
                            .child(div().text_sm().child("Outline colour")),
                    )
                    .child(
                        Button::new(ids::CROSSHAIR_MAKER_OUTLINE)
                            .outline()
                            .icon(IconName::SquareDashed)
                            .label("Add outline")
                            .disabled(self.busy())
                            .on_click(cx.listener(|page, _, _, cx| {
                                let rgb = page.outline_rgb;
                                page.edit(cx, |e| e.outline(rgb));
                            })),
                    ),
            )
            .child(
                v_flex()
                    .gap_2()
                    .child(heading("Preview"))
                    .child(self.preview(background, cx))
                    .child(
                        div()
                            .id(ids::CROSSHAIR_MAKER_BACKGROUND)
                            .aria_label(background)
                            .test_support()
                            .w_full()
                            .child(Select::new(&self.background)),
                    )
                    .child(
                        h_flex()
                            .id(ids::CROSSHAIR_MAKER_PREVIEW_COLOR)
                            .aria_label("Preview crosshair colour")
                            .test_support()
                            .gap_2()
                            .child(
                                ColorPicker::new(&self.preview_colour)
                                    .accessibility_label("Preview crosshair colour"),
                            )
                            .child(div().text_sm().child("Crosshair colour in game")),
                    ),
            );
        scroll_area(
            ids::CROSSHAIR_MAKER_PANEL_SCROLLBAR,
            &self.panel_scroll,
            content,
        )
        .flex_none()
        .w(width)
        .h_full()
        .min_h_0()
    }

    // The drawing at in-game size, tinted as the game tints it, centred over the map.
    fn preview(&self, background: &'static str, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let (border, muted) = (theme.border, theme.muted);
        let texels = preview_texels(&self.editor.drawing(), self.preview_rgb);
        let texture = canvas(
            |_, _, _| {},
            move |bounds, _, window, _| {
                for (i, &[r, g, b, a]) in texels.pixels().as_chunks::<4>().0.iter().enumerate() {
                    if a == 0 {
                        continue;
                    }
                    let (x, y) = ((i % 64) as f32, (i / 64) as f32);
                    let at = point(bounds.origin.x + px(x), bounds.origin.y + px(y));
                    window.paint_quad(fill(
                        gpui_kit::Bounds::new(at, size(px(1.), px(1.))),
                        hsla_of_rgba([r, g, b, a]),
                    ));
                }
            },
        )
        .size(px(64.));
        let path = cx.global::<Services>().assets.background(background);
        div()
            .id(ids::CROSSHAIR_MAKER_PREVIEW)
            .aria_label(format!("Preview on {background}"))
            .test_support()
            .relative()
            .flex_none()
            .size(PREVIEW_SIZE)
            .rounded_lg()
            .border_1()
            .border_color(border)
            .bg(muted)
            .overflow_hidden()
            .child(
                img(path)
                    .absolute()
                    .inset_0()
                    .size_full()
                    .object_fit(ObjectFit::Cover),
            )
            .child(
                div()
                    .absolute()
                    .inset_0()
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(texture),
            )
    }

    fn cell_under(&self, at: Point<Pixels>) -> Option<(u32, u32)> {
        self.view.cell_at(at - self.view_origin.get())
    }

    /// Changes the zoom or scroll; the cell under a still pointer differs afterwards, so the
    /// readout waits for the next move. A change that moves nothing leaves the readout be.
    fn zoom(&mut self, cx: &mut Context<Self>, change: impl FnOnce(&mut Viewport)) {
        let before = self.view;
        change(&mut self.view);
        if self.view != before {
            self.hover = None;
            cx.notify();
        }
    }

    /// Ctrl+wheel zooms around the pointer; a plain wheel scrolls, sideways with Shift.
    fn wheel(&mut self, ev: &ScrollWheelEvent, window: &Window, cx: &mut Context<Self>) {
        if ev.modifiers.secondary() {
            let steps = wheel_zoom_steps(ev.delta, &mut self.zoom_carry);
            let anchor = ev.position - self.view_origin.get();
            self.zoom(cx, |v| v.zoom_at(steps, anchor));
        } else {
            self.zoom_carry = px(0.);
            let by = wheel_scroll(
                ev.delta.pixel_delta(window.line_height()),
                ev.modifiers.shift,
            );
            self.zoom(cx, |v| v.scroll_by(by));
        }
    }

    /// Starts a pan at window position `at`, unless a stroke is running.
    fn start_pan(&mut self, at: Point<Pixels>, cx: &mut Context<Self>) {
        if !self.editor.in_stroke() {
            self.pan = Some(at);
            cx.notify();
        }
    }

    fn end_pan(&mut self, cx: &mut Context<Self>) {
        if self.pan.take().is_some() {
            cx.notify();
        }
    }

    /// Moves a running pan with the pointer, or extends the running stroke to the cell nearest
    /// `at`. A move with the pan's or stroke's button no longer held ends it, in case its
    /// release never reached the window. Also tracks the hovered cell for the position readout,
    /// which stays clear while a dialog covers the grid.
    fn drag(&mut self, ev: &MouseMoveEvent, window: &mut Window, cx: &mut Context<Self>) {
        // A move ends a drag only with no button held: Wayland reports just the most recently
        // pressed button, so another button pressed mid-drag must not end it.
        if let Some(last) = self.pan {
            if ev.pressed_button.is_some() {
                self.pan = Some(ev.position);
                self.zoom(cx, |v| v.scroll_by(last - ev.position));
            } else {
                self.end_pan(cx);
            }
        }
        let hover = if window.has_active_dialog(cx) {
            None
        } else {
            self.cell_under(ev.position)
        };
        if hover != self.hover {
            self.hover = hover;
            cx.notify();
        }
        if !self.editor.in_stroke() {
            return;
        }
        if ev.pressed_button.is_none() {
            self.edit(cx, Editor::release);
            return;
        }
        let (x, y) = self.view.nearest_cell(ev.position - self.view_origin.get());
        self.edit(cx, |e| e.drag_to(x, y));
    }

    fn unhover(&mut self, cx: &mut Context<Self>) {
        if self.hover.take().is_some() {
            cx.notify();
        }
    }

    fn edit(&mut self, cx: &mut Context<Self>, change: impl FnOnce(&mut Editor)) {
        change(&mut self.editor);
        cx.notify();
    }

    // Undo and Redo, from the buttons or the keys, wait while a save or open runs.
    fn undo(&mut self, cx: &mut Context<Self>) {
        if !self.busy() {
            self.edit(cx, |e| {
                e.undo();
            });
        }
    }

    fn redo(&mut self, cx: &mut Context<Self>) {
        if !self.busy() {
            self.edit(cx, |e| {
                e.redo();
            });
        }
    }

    fn toolbar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let e = &self.editor;
        let tool = |id: &'static str, icon: IconName, which: Tool| {
            icon_button(id, icon, which.label(), which.tooltip())
                .selected(e.tool() == which)
                .on_click(cx.listener(move |page, _, _, cx| page.edit(cx, |e| e.set_tool(which))))
        };
        let (mirror_x, mirror_y) = (e.mirror_x(), e.mirror_y());
        let (filled, fill_switch) = (e.filled(), e.tool().has_fill_switch());
        let (busy, guides, grid_lines) = (self.busy(), self.guides, self.grid_lines);
        let view = self.view;
        // Each later group leads with its divider in a DIVIDER_LEAD padding. The row starts
        // DIVIDER_LEAD left of a clipping box, so a divider that would open a row is cut away
        // and only dividers between groups on one row show. A group wider than the column
        // shrinks and wraps its own buttons, which all start right of its divider.
        let group = |lead: Option<usize>| {
            h_flex()
                .relative()
                .min_w_0()
                .flex_wrap()
                .gap(px(4.))
                .when_some(lead, |g, ix| g.pl(DIVIDER_LEAD).child(divider(ix, cx)))
        };
        div().flex_none().w_full().overflow_hidden().child(
            h_flex()
                .flex_wrap()
                .gap(px(4.))
                .ml(-DIVIDER_LEAD)
                .child(
                    group(None)
                        .ml(DIVIDER_LEAD)
                        .child(tool(
                            ids::CROSSHAIR_MAKER_PENCIL,
                            IconName::Pencil,
                            Tool::Pencil,
                        ))
                        .child(tool(
                            ids::CROSSHAIR_MAKER_ERASER,
                            IconName::Eraser,
                            Tool::Eraser,
                        ))
                        .child(tool(
                            ids::CROSSHAIR_MAKER_FILL,
                            IconName::PaintBucket,
                            Tool::Fill,
                        ))
                        .child(tool(ids::CROSSHAIR_MAKER_LINE, IconName::Slash, Tool::Line))
                        .child(tool(
                            ids::CROSSHAIR_MAKER_RECTANGLE,
                            IconName::Square,
                            Tool::Rectangle,
                        ))
                        .child(tool(
                            ids::CROSSHAIR_MAKER_CIRCLE,
                            IconName::Circle,
                            Tool::Circle,
                        ))
                        .child(
                            icon_button(
                                ids::CROSSHAIR_MAKER_FILLED,
                                IconName::SquareSquare,
                                "Filled shapes",
                                "Filled shapes",
                            )
                            .selected(filled)
                            .toggled(filled)
                            .disabled(!fill_switch)
                            .on_click(cx.listener(
                                move |page, _, _, cx| page.edit(cx, |e| e.set_filled(!filled)),
                            )),
                        ),
                )
                .child(
                    group(Some(0))
                        .child(
                            icon_button(
                                ids::CROSSHAIR_MAKER_MIRROR_X,
                                IconName::FlipHorizontal2,
                                "Mirror left and right",
                                "Mirror left and right",
                            )
                            .selected(mirror_x)
                            .toggled(mirror_x)
                            .on_click(cx.listener(
                                move |page, _, _, cx| page.edit(cx, |e| e.set_mirror_x(!mirror_x)),
                            )),
                        )
                        .child(
                            icon_button(
                                ids::CROSSHAIR_MAKER_MIRROR_Y,
                                IconName::FlipVertical2,
                                "Mirror top and bottom",
                                "Mirror top and bottom",
                            )
                            .selected(mirror_y)
                            .toggled(mirror_y)
                            .on_click(cx.listener(
                                move |page, _, _, cx| page.edit(cx, |e| e.set_mirror_y(!mirror_y)),
                            )),
                        ),
                )
                .child(
                    group(Some(1))
                        .child(
                            icon_button(
                                ids::CROSSHAIR_MAKER_GUIDES,
                                IconName::Crosshair,
                                "Centre guides",
                                "Centre guides",
                            )
                            .selected(guides)
                            .toggled(guides)
                            .on_click(cx.listener(
                                move |page, _, _, cx| {
                                    page.guides = !guides;
                                    cx.notify();
                                },
                            )),
                        )
                        .child(
                            icon_button(
                                ids::CROSSHAIR_MAKER_GRID_LINES,
                                IconName::Grid3x3,
                                "Grid lines",
                                "Grid lines",
                            )
                            .selected(grid_lines)
                            .toggled(grid_lines)
                            .on_click(
                                cx.listener(|page, _, window, cx| {
                                    page.toggle_grid_lines(window, cx)
                                }),
                            ),
                        )
                        .child(
                            icon_button(
                                ids::CROSSHAIR_MAKER_ZOOM_OUT,
                                IconName::ZoomOut,
                                "Zoom out",
                                ZOOM_OUT_TIP,
                            )
                            .disabled(!view.can_zoom_out())
                            .on_click(
                                cx.listener(|page, _, _, cx| page.zoom(cx, |v| v.zoom_centre(-1))),
                            ),
                        )
                        .child(
                            Button::new(ids::CROSSHAIR_MAKER_ZOOM)
                                .ghost()
                                .label(view.label())
                                .tooltip(FIT_TIP)
                                .on_click(
                                    cx.listener(|page, _, _, cx| page.zoom(cx, Viewport::fit)),
                                ),
                        )
                        .child(
                            icon_button(
                                ids::CROSSHAIR_MAKER_ZOOM_IN,
                                IconName::ZoomIn,
                                "Zoom in",
                                ZOOM_IN_TIP,
                            )
                            .disabled(!view.can_zoom_in())
                            .on_click(
                                cx.listener(|page, _, _, cx| page.zoom(cx, |v| v.zoom_centre(1))),
                            ),
                        ),
                )
                .child(
                    group(Some(2))
                        .child(
                            icon_button(
                                ids::CROSSHAIR_MAKER_UNDO,
                                IconName::Undo2,
                                "Undo",
                                UNDO_TIP,
                            )
                            .disabled(!e.can_undo() || busy)
                            .on_click(cx.listener(|page, _, _, cx| page.undo(cx))),
                        )
                        .child(
                            icon_button(
                                ids::CROSSHAIR_MAKER_REDO,
                                IconName::Redo2,
                                "Redo",
                                REDO_TIP,
                            )
                            .disabled(!e.can_redo() || busy)
                            .on_click(cx.listener(|page, _, _, cx| page.redo(cx))),
                        )
                        .child(
                            icon_button(
                                ids::CROSSHAIR_MAKER_CLEAR,
                                IconName::Trash,
                                "Clear",
                                "Clear",
                            )
                            .disabled(busy)
                            .on_click(cx.listener(|page, _, _, cx| page.edit(cx, Editor::clear))),
                        ),
                )
                .child(
                    group(Some(3)).child(
                        Button::new(ids::CROSSHAIR_MAKER_SIZE)
                            .outline()
                            .label(size_label(e.size().side()))
                            .tooltip("Switch canvas size")
                            .disabled(busy)
                            .on_click(cx.listener(|page, _, _, cx| {
                                // The cell under a still pointer differs at the new size.
                                page.hover = None;
                                page.edit(cx, |e| e.set_size(e.size().other()))
                            })),
                    ),
                )
                .child(
                    group(Some(4))
                        .child(
                            Button::new(ids::CROSSHAIR_MAKER_NEW)
                                .outline()
                                .icon(IconName::FilePlus)
                                .label("New")
                                .disabled(busy)
                                .on_click(
                                    cx.listener(|page, _, window, cx| page.new_drawing(window, cx)),
                                ),
                        )
                        .child(
                            Button::new(ids::CROSSHAIR_MAKER_STARTERS)
                                .outline()
                                .icon(IconName::Shapes)
                                .label("Starters")
                                .tooltip("Starter shapes")
                                .disabled(busy)
                                .on_click(cx.listener(|page, _, window, cx| {
                                    page.open_starters(window, cx)
                                })),
                        )
                        .child(
                            Button::new(ids::CROSSHAIR_MAKER_IMPORT_PNG)
                                .outline()
                                .icon(IconName::ImageUp)
                                .label("Import PNG")
                                .tooltip("Open a PNG as a new drawing")
                                .disabled(busy || self.picking)
                                .on_click(cx.listener(|page, _, _, cx| page.import_png(cx))),
                        ),
                ),
        )
    }

    // The grid sits in a clipping view at the viewport's origin, fitted to whole-pixel cells
    // and zoomed by whole multiples. One canvas measures the view and records where it is (a
    // size change asks for another frame, as a refresh during drawing is dropped); one inside
    // the grid paints the cells.
    fn grid(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let side = self.editor.size().side();
        self.view.set_frame(self.free.get(), side);
        let (cell, length, origin) = (self.view.cell(), self.view.length(), self.view.origin());
        let theme = cx.theme();
        let (light, dark, muted_fg) = (theme.muted, theme.background, theme.muted_foreground);

        let (free, view_origin) = (self.free.clone(), self.view_origin.clone());
        let watch = canvas(
            move |bounds, window, _| {
                view_origin.set(bounds.origin);
                if free.get() != bounds.size {
                    free.set(bounds.size);
                    window.request_animation_frame();
                }
            },
            |_, _, _, _| {},
        )
        .absolute()
        .size_full();

        let drawing = self.editor.drawing();
        let lines = self.grid_lines;
        let panning = self.is_panning();
        let page = cx.entity().downgrade();
        let cells = canvas(
            |_, _, _| {},
            move |bounds, _, window, _| {
                // Moves are watched window-wide, for the hover readout and so a drag that leaves
                // the grid follows its nearest edge instead of jumping straight to the re-entry.
                let moved = page.clone();
                window.on_mouse_event(move |ev: &MouseMoveEvent, phase, window, cx| {
                    if phase != DispatchPhase::Capture {
                        return;
                    }
                    let _ = moved.update(cx, |page, cx| page.drag(ev, window, cx));
                });
                // Leaving the window sends no move, so the readout is cleared here.
                window.on_mouse_event(move |_: &MouseExitEvent, phase, _, cx| {
                    if phase == DispatchPhase::Capture {
                        let _ = page.update(cx, |page, cx| page.unhover(cx));
                    }
                });
                if panning {
                    window.set_window_cursor_style(CursorStyle::ClosedHand);
                }
                window.paint_quad(fill(bounds, dark));
                let at = |x: u32, y: u32| {
                    gpui_kit::Bounds::new(
                        point(
                            bounds.origin.x + cell * x as f32,
                            bounds.origin.y + cell * y as f32,
                        ),
                        size(cell, cell),
                    )
                };
                for y in 0..side {
                    for x in 0..side {
                        let i = ((y * side + x) * 4) as usize;
                        let [r, g, b, a] = drawing.pixels()[i..i + 4] else {
                            continue;
                        };
                        // A see-through cell shows the checkerboard under its colour.
                        if a < 255 && (x + y) % 2 == 0 {
                            window.paint_quad(fill(at(x, y), light));
                        }
                        if a != 0 {
                            window.paint_quad(fill(at(x, y), hsla_of_rgba([r, g, b, a])));
                        }
                    }
                }
                // Below 3 px a cell would be mostly line, so the lines are left out.
                if lines && cell >= px(3.) {
                    let grey = gpui_kit::hsla(0., 0., 0.5, 0.5);
                    for i in 1..side {
                        let offset = cell * i as f32;
                        let column = point(bounds.origin.x + offset, bounds.origin.y);
                        let row = point(bounds.origin.x, bounds.origin.y + offset);
                        let v = gpui_kit::Bounds::new(column, size(px(1.), length));
                        let h = gpui_kit::Bounds::new(row, size(length, px(1.)));
                        window.paint_quad(fill(v, grey));
                        window.paint_quad(fill(h, grey));
                    }
                }
            },
        )
        .absolute()
        .size_full();

        // Each guide is a black and a white strip centred on the middle, so it shows on any
        // drawing; it has no listeners, so presses fall through to the grid.
        let middle = length / 2. - px(1.);
        let strips = |row: bool| {
            let strip = |colour: Hsla| {
                let s = div().flex_none().bg(colour);
                if row {
                    s.w_full().h(px(1.))
                } else {
                    s.h_full().w(px(1.))
                }
            };
            let black = strip(Hsla::black());
            let white = strip(Hsla::white());
            if row {
                v_flex().child(black).child(white)
            } else {
                h_flex().child(black).child(white)
            }
        };
        let guides = self.guides.then(|| {
            [
                div()
                    .id(ids::CROSSHAIR_MAKER_GUIDE_V)
                    .test_support()
                    .absolute()
                    .top_0()
                    .left(middle)
                    .w(px(2.))
                    .h(length)
                    .child(strips(false).size_full()),
                div()
                    .id(ids::CROSSHAIR_MAKER_GUIDE_H)
                    .test_support()
                    .absolute()
                    .left_0()
                    .top(middle)
                    .h(px(2.))
                    .w(length)
                    .child(strips(true).size_full()),
            ]
        });

        let grid = div()
            .id(ids::CROSSHAIR_MAKER_GRID)
            .aria_label("Drawing grid")
            .test_support()
            .absolute()
            .left(origin.x)
            .top(origin.y)
            .size(length)
            .child(cells)
            .children(guides.into_iter().flatten())
            .on_mouse_down(
                MouseButton::Left,
                // Alt takes the colour under the pointer instead of painting; it leaves the
                // drawing alone, so it works even while a save runs.
                cx.listener(|page, ev: &MouseDownEvent, window, cx| {
                    if page.is_panning() {
                        return;
                    }
                    let Some((x, y)) = page.cell_under(ev.position) else {
                        return;
                    };
                    if ev.modifiers.alt {
                        // The slider shows the picked alpha's percent, which the observer
                        // sees as equal, so the editor keeps the exact alpha.
                        if page.editor.pick(x, y) {
                            page.pick_swatch(page.editor.colour(), window, cx);
                            let percent = percent_of_alpha(page.editor.alpha()) as f32;
                            page.opacity
                                .update(cx, |state, cx| state.set_value(percent, window, cx));
                        }
                    } else if !page.busy() {
                        page.edit(cx, |e| e.press(x, y));
                    }
                }),
            )
            // A stroke starts only on a press inside the grid; any release, on the grid or
            // off it, ends it.
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|page, _: &MouseUpEvent, _, cx| page.edit(cx, Editor::release)),
            )
            .on_mouse_up_out(
                MouseButton::Left,
                cx.listener(|page, _: &MouseUpEvent, _, cx| page.edit(cx, Editor::release)),
            );

        // The readout row has a fixed height, so the readout coming and going never changes
        // the free space the cells are fitted into.
        let size = self.editor.size();
        let readout = self.hover.map(|(x, y)| {
            let text = position_text(size, x, y);
            div()
                .id(ids::CROSSHAIR_MAKER_POSITION)
                .aria_label(text.clone())
                .test_support()
                .text_xs()
                .text_color(muted_fg)
                .child(text)
        });
        v_flex()
            .flex_1()
            .min_w_0()
            .min_h_0()
            .size_full()
            .child(
                div()
                    .id(ids::CROSSHAIR_MAKER_VIEW)
                    .test_support()
                    .relative()
                    .flex_1()
                    .min_h_0()
                    .w_full()
                    .overflow_hidden()
                    .on_scroll_wheel(cx.listener(|page, ev, window, cx| page.wheel(ev, window, cx)))
                    // Right and middle drags pan; the window-wide move listener in `drag` moves
                    // the grid, and a release anywhere ends the pan.
                    .on_mouse_down(MouseButton::Right, cx.listener(pan_down))
                    .on_mouse_down(MouseButton::Middle, cx.listener(pan_down))
                    .on_mouse_up(MouseButton::Right, cx.listener(pan_up))
                    .on_mouse_up(MouseButton::Middle, cx.listener(pan_up))
                    .on_mouse_up_out(MouseButton::Right, cx.listener(pan_up))
                    .on_mouse_up_out(MouseButton::Middle, cx.listener(pan_up))
                    .child(watch)
                    .child(grid),
            )
            .child(
                h_flex()
                    .flex_none()
                    .h(px(20.))
                    .justify_center()
                    .children(readout),
            )
            .into_any_element()
    }
}

// The background half of a save: core's write, the activity row, and for a re-save whether
// any weapon uses the crosshair. A taken name and a deleted crosshair come back as outcomes.
fn save_job(
    s: &Services,
    open: Option<CrosshairId>,
    name: &str,
    drawing: &RgbaImage,
) -> Result<Saved, OpError> {
    let (target, step) = match &open {
        Some(id) => (PaintTarget::Existing(id.clone()), PaintStep::Resave),
        None => (PaintTarget::New, PaintStep::SaveNew),
    };
    let entry = match s.library.save_painted_crosshair(&target, name, drawing) {
        Ok(entry) => entry,
        Err(PaintError::NameTaken { name }) => return Ok(Saved::NameTaken(name)),
        Err(e @ PaintError::NotFound { .. }) if open.is_some() => {
            return Ok(Saved::Gone(describe_paint(&e, step).into_op(SAVE_FAILED)));
        }
        Err(e) => return Err(describe_paint(&e, step).into_op(SAVE_FAILED)),
    };
    let row = NewLogEntry::new(
        LogLevel::Info,
        "crosshair",
        "paint",
        format!("Saved painted crosshair: {}", entry.name()),
    );
    activity::write(s, &row);
    // A failed read only loses the reminder to install; the save itself landed.
    let assigned = open.is_some()
        && s.library.assignments().is_ok_and(|rows| {
            let ours = CrosshairRef::Library(entry.id().clone());
            rows.iter().any(|a| *a.crosshair() == ours)
        });
    Ok(Saved::Done {
        id: entry.id().clone(),
        name: entry.name().to_owned(),
        assigned,
    })
}

// The background half of `open`: the drawing, its library name, and whether the drawing was
// rebuilt from the VTF.
fn open_job(s: &Services, id: CrosshairId) -> Result<(Editor, bool), OpError> {
    let failed = |e: PaintError| describe_paint(&e, PaintStep::Open).into_op(OPEN_FAILED);
    let source = s.library.painted_source(&id).map_err(failed)?;
    let library = s
        .library
        .list_crosshairs()
        .map_err(|e| describe_library(&e).into_op(OPEN_FAILED))?;
    let gone = || PaintError::NotFound { id: id.to_string() };
    let name = library
        .iter()
        .find(|c| *c.id() == id)
        .map(|c| c.name().to_owned())
        .ok_or_else(|| failed(gone()))?;
    let unreadable = || PaintError::Unreadable { id: id.to_string() };
    let editor =
        Editor::open(id.clone(), name, source.drawing()).ok_or_else(|| failed(unreadable()))?;
    Ok((editor, source.recovered()))
}

// The background half of `copy_from`: the crosshair's pixels, fitted to the maker.
fn copy_job(s: &Services, crosshair: CrosshairRef) -> Result<(RgbaImage, FitChange), OpError> {
    let drawing = s
        .library
        .crosshair_drawing(&crosshair, &s.builtins)
        .map_err(|e| describe_library(&e).into_op(COPY_FAILED))?;
    Ok(fit_to_maker(drawing))
}

/// A ghost icon button with accessible name `label` and tooltip `tip`, which may name a key.
fn icon_button(
    id: &'static str,
    icon: IconName,
    label: &'static str,
    tip: impl Into<SharedString>,
) -> Button {
    Button::new(id)
        .ghost()
        .icon(icon)
        .tooltip(tip)
        .accessibility_label(label)
}

fn pan_down(
    page: &mut CrosshairMakerPage,
    ev: &MouseDownEvent,
    _: &mut Window,
    cx: &mut Context<CrosshairMakerPage>,
) {
    page.start_pan(ev.position, cx);
}

fn pan_up(
    page: &mut CrosshairMakerPage,
    _: &MouseUpEvent,
    _: &mut Window,
    cx: &mut Context<CrosshairMakerPage>,
) {
    page.end_pan(cx);
}

fn heading(text: &'static str) -> impl IntoElement {
    div()
        .text_sm()
        .font_weight(FontWeight::SEMIBOLD)
        .child(text)
}

fn divider(ix: usize, cx: &Context<CrosshairMakerPage>) -> impl IntoElement {
    div()
        .id(ids::crosshair_maker_divider(ix))
        .test_support()
        // Sits in its group's lead padding, centred on the first row of 32 px buttons.
        .absolute()
        .left(px(4.))
        .top(px(6.))
        .w(px(1.))
        .h(px(20.))
        .bg(cx.theme().border)
}

fn size_label(side: u32) -> String {
    format!("{side}x{side}")
}

impl Render for CrosshairMakerPage {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let panel_width = if window.viewport_size().width < NARROW_WINDOW {
            NARROW_PANEL_WIDTH
        } else {
            PANEL_WIDTH
        };
        v_flex()
            .id(ids::page(Page::CrosshairMaker))
            .test_support()
            .key_context(MAKER_CONTEXT)
            .track_focus(&self.focus)
            // Kit Buttons stop a press from focusing anything, so every press first hands the
            // keys back to the page; a text field or picker under it takes focus right after.
            .capture_any_mouse_down(cx.listener(|page, _, window, cx| {
                page.focus.focus(window, cx);
            }))
            .on_action(
                cx.listener(|page, _: &UsePencil, _, cx| {
                    page.edit(cx, |e| e.set_tool(Tool::Pencil))
                }),
            )
            .on_action(
                cx.listener(|page, _: &UseEraser, _, cx| {
                    page.edit(cx, |e| e.set_tool(Tool::Eraser))
                }),
            )
            .on_action(
                cx.listener(|page, _: &UseFill, _, cx| page.edit(cx, |e| e.set_tool(Tool::Fill))),
            )
            .on_action(
                cx.listener(|page, _: &UseLine, _, cx| page.edit(cx, |e| e.set_tool(Tool::Line))),
            )
            .on_action(cx.listener(|page, _: &UseRectangle, _, cx| {
                page.edit(cx, |e| e.set_tool(Tool::Rectangle))
            }))
            .on_action(
                cx.listener(|page, _: &UseCircle, _, cx| {
                    page.edit(cx, |e| e.set_tool(Tool::Circle))
                }),
            )
            .on_action(cx.listener(|page, _: &UndoDrawing, _, cx| page.undo(cx)))
            .on_action(cx.listener(|page, _: &RedoDrawing, _, cx| page.redo(cx)))
            .on_action(
                cx.listener(|page, _: &ZoomInGrid, _, cx| page.zoom(cx, |v| v.zoom_centre(1))),
            )
            .on_action(
                cx.listener(|page, _: &ZoomOutGrid, _, cx| page.zoom(cx, |v| v.zoom_centre(-1))),
            )
            .on_action(cx.listener(|page, _: &FitGrid, _, cx| page.zoom(cx, Viewport::fit)))
            .size_full()
            .p(CONTENT_PADDING)
            .gap_4()
            .child(
                h_flex()
                    .flex_1()
                    .min_h_0()
                    .w_full()
                    .gap_4()
                    // The toolbar wraps inside the canvas column, so a narrow window costs the
                    // grid a row of height instead of drawing buttons over the side panel.
                    .child(
                        v_flex()
                            .flex_1()
                            .min_w_0()
                            .min_h_0()
                            .h_full()
                            .gap_4()
                            .child(self.toolbar(cx))
                            .child(self.grid(cx)),
                    )
                    .child(self.panel(panel_width, cx)),
            )
    }
}
