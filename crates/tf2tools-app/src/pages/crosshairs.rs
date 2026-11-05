//! v5's Crosshairs page: the searchable picker, background and composite preview, install,
//! the conflicts box, and per-class weapon assignment.

use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

use gpui_kit::assets::IconName;
use gpui_kit::component::IndexPath;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::InputState;
use gpui_kit::component::select::{Select, SelectState};
use gpui_kit::component::{
    ActiveTheme as _, Disableable as _, Icon, Sizable as _, Size, h_flex, v_flex,
};
use gpui_kit::{
    AnyElement, AnyWindowHandle, App, AppContext as _, Context, Entity, EventEmitter, FontWeight,
    Hsla, InteractiveElement as _, IntoElement, ObjectFit, ParentElement as _, Pixels, Render,
    Role, ScrollHandle, SharedString, StatefulInteractiveElement as _, Styled as _,
    StyledImage as _, Subscription, TestSupportExt as _, Window, div, img, px, rgb,
};

use tf2tools_core::{
    ApplyReport, Assignment, BuiltinCrosshairs, CrosshairEntry, CrosshairFormat, CrosshairId,
    CrosshairRef, InstalledCrosshairs, LibraryError, LogLevel, NewLogEntry, Provenance, SkipReason,
    TfClass, WeaponSlot, write_png,
};

use super::common::{SM_ICON_PX, YELLOW_500, needs_tf2, no_tf2_notice};
use crate::activity;
use crate::assets::BACKGROUNDS;
use crate::ids;
use crate::library_events::{LibraryChanged, LibraryEvents};
use crate::local_messages::{
    describe_import, describe_library, describe_png_write, describe_tf2_install,
};
use crate::picker::{PickKind, file_name_for, with_extension};
use crate::previews::CrosshairPreviews;
use crate::services::{OpError, Services, Tf2State, require_tf2, run_blocking, tf2_state};
use crate::shell::CONTENT_PADDING;
use crate::shell::confirm::{Confirm, confirm};
use crate::shell::route::{NavigateTo, Page};
use crate::shell::toast::{ToastKind, toast, toast_error};
use crate::widgets::crosshair_picker::{CrosshairPicker, IN_GAME_SIZE, preview_image};
use crate::widgets::scroll::scroll_area;
use crate::widgets::weapon_assigner::{
    AssignRequested, AssignTarget, AssignedCell, AssignerState, ClassChanged, WeaponAssigner,
};

const PREVIEW_SIZE: Pixels = px(128.);
/// Every Export PNG failure toasts under this action.
const EXPORT_FAILED: &str = "Failed to export PNG";

/// The action in flight: its button spins and the other mutating controls disable.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Busy {
    Import,
    Export,
    Install,
    Uninstall,
    Delete,
    /// Assigning to the weapon with this script.
    Assign(&'static str),
    /// Assigning to the class's weapons in this slot, or to all of them.
    Bulk(Option<WeaponSlot>),
}

/// What a finished action toasts: its kind, title and description.
type Outcome = (ToastKind, SharedString, SharedString);

/// When the page reached each stage, measured from its construction.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PageTimings {
    pub first_render: Option<Duration>,
    pub loaded: Option<Duration>,
    /// The first moment after the load when no requested preview is pending.
    pub previews_ready: Option<Duration>,
}

/// Asks the shell to open painted crosshair `0` in the Crosshair Maker.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditPainted(pub CrosshairId);

/// Asks the shell to open crosshair `0` in a fresh Crosshair Maker as a new drawing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CopyToMaker(pub CrosshairRef);

/// The Crosshairs page. Built fresh on each visit; it reloads whenever the TF2 state changes.
pub struct CrosshairsPage {
    win: AnyWindowHandle,
    builtins: Arc<BuiltinCrosshairs>,
    assignments: Vec<Assignment>,
    installed: Option<InstalledCrosshairs>,
    /// The picker's selection, mirrored on each of its notifies so `selected` needs no `cx`.
    selected: Option<CrosshairRef>,
    class: TfClass,
    loaded: bool,
    busy: Option<Busy>,
    /// Bumped by each reload, so a slower, older load never overwrites a newer one.
    load_generation: u64,
    previews: Entity<CrosshairPreviews>,
    created: Instant,
    timings: PageTimings,
    picker: Entity<CrosshairPicker>,
    assigner: Entity<WeaponAssigner>,
    /// The picker's search box and list scroll, held so their accessors need no `cx`.
    search: Entity<InputState>,
    picker_scroll: ScrollHandle,
    background: Entity<SelectState<Vec<&'static str>>>,
    scroll: ScrollHandle,
    _tf2: Subscription,
    _library: Subscription,
    _previews: Subscription,
    _picker: Subscription,
    _assigner: Subscription,
    _assigner_class: Subscription,
}

impl EventEmitter<EditPainted> for CrosshairsPage {}
impl EventEmitter<CopyToMaker> for CrosshairsPage {}
impl EventEmitter<NavigateTo> for CrosshairsPage {}

impl CrosshairsPage {
    /// The picker list's scroll handle; its offset is what the list drew.
    pub fn picker_scroll(&self) -> ScrollHandle {
        self.picker_scroll.clone()
    }

    /// The page root's scroll handle.
    pub fn page_scroll(&self) -> ScrollHandle {
        self.scroll.clone()
    }

    /// Needs the `Services` and `Tf2Global` globals; loads off the UI thread, reloads on TF2 state
    /// and library changes.
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let tf2 = cx.observe(&tf2_state(cx), |page, _, cx| page.reload(cx));
        let events = LibraryEvents::global(cx);
        let library = cx.subscribe(&events, |page, _, _: &LibraryChanged, cx| page.reload(cx));
        let previews = CrosshairPreviews::global(cx);
        // Rows draw whatever the cache holds, so each landed decode batch redraws them; the
        // first time the queue drains after the load marks the previews ready.
        let observe_previews = cx.observe(&previews, |page: &mut Self, previews, cx| {
            if page.timings.loaded.is_some() && previews.read(cx).pending() == 0 {
                page.mark_previews_ready();
            }
            cx.notify();
        });
        let picker = cx.new(|cx| CrosshairPicker::new(false, window, cx));
        // The composite preview and the weapon rows read the selection, so each picker notify
        // refreshes the mirror and redraws the page.
        let observe_picker = cx.observe(&picker, |page: &mut Self, picker, cx| {
            page.selected = picker.read(cx).selected().cloned();
            page.sync_assigner(cx);
            cx.notify();
        });
        // The assigner only asks; the page writes the table, and mirrors the shown class from
        // the assigner's change event so `class` needs no `cx`.
        let assigner = cx.new(|cx| WeaponAssigner::new(false, cx));
        let assign_requested = cx.subscribe_in(
            &assigner,
            window,
            |page: &mut Self, _, event: &AssignRequested, window, cx| match event.target {
                AssignTarget::Weapon(script) => page.assign(script, cx),
                AssignTarget::Bulk(slot) => {
                    page.bulk(slot, event.class, event.scripts.clone(), window, cx)
                }
            },
        );
        let class_changed = cx
            .subscribe(&assigner, |page: &mut Self, _, event: &ClassChanged, _| {
                page.class = event.class
            });
        let (search, picker_scroll) = {
            let picker = picker.read(cx);
            (picker.search_input().clone(), picker.scroll())
        };
        // Read at render, so a background change needs no subscription.
        let background = cx.new(|cx| {
            SelectState::new(BACKGROUNDS.to_vec(), Some(IndexPath::default()), window, cx)
        });
        let builtins = cx.global::<Services>().builtins.clone();
        let mut page = Self {
            win: window.window_handle(),
            builtins,
            assignments: Vec::new(),
            installed: None,
            selected: None,
            class: TfClass::Scout,
            loaded: false,
            busy: None,
            load_generation: 0,
            previews,
            created: Instant::now(),
            timings: PageTimings::default(),
            picker,
            assigner,
            search,
            picker_scroll,
            background,
            scroll: ScrollHandle::new(),
            _tf2: tf2,
            _library: library,
            _previews: observe_previews,
            _picker: observe_picker,
            _assigner: assign_requested,
            _assigner_class: class_changed,
        };
        page.reload(cx);
        page
    }

    /// True once a load has finished, whether it succeeded or toasted.
    pub fn is_loaded(&self) -> bool {
        self.loaded
    }

    /// When the page first rendered, loaded and had every requested preview.
    pub fn timings(&self) -> PageTimings {
        self.timings
    }

    fn mark_previews_ready(&mut self) {
        if self.timings.previews_ready.is_none() {
            self.timings.previews_ready = Some(self.created.elapsed());
        }
    }

    /// The picker's search box.
    pub fn search_input(&self) -> &Entity<InputState> {
        &self.search
    }

    /// The crosshair picked in the list, if any.
    pub fn selected(&self) -> Option<&CrosshairRef> {
        self.selected.as_ref()
    }

    /// The class whose weapons the list shows; Scout until a tab is clicked.
    pub fn class(&self) -> TfClass {
        self.class
    }

    /// Shows `class`'s weapons, as clicking its tab does.
    pub fn set_class(&mut self, class: TfClass, cx: &mut Context<Self>) {
        self.class = class;
        self.assigner
            .update(cx, |assigner, cx| assigner.set_class(class, cx));
        cx.notify();
    }

    // Tells the assigner whether rows may assign and which button spins; called wherever the
    // selection or `busy` changes, or a missed call leaves the rows inert after a pick.
    fn sync_assigner(&self, cx: &mut Context<Self>) {
        let running = match self.busy {
            Some(Busy::Assign(script)) => Some(AssignTarget::Weapon(script)),
            Some(Busy::Bulk(slot)) => Some(AssignTarget::Bulk(slot)),
            _ => None,
        };
        let state = AssignerState {
            has_selection: self.selected.is_some(),
            busy: self.busy.is_some(),
            running,
        };
        self.assigner
            .update(cx, |assigner, cx| assigner.set_state(state, cx));
    }

    // Marks the action in flight (or none) and passes it on to the assigner.
    fn set_busy(&mut self, busy: Option<Busy>, cx: &mut Context<Self>) {
        self.busy = busy;
        self.sync_assigner(cx);
        cx.notify();
    }

    // Shows each assignment's name and preview on its weapon row.
    fn push_assigned(&self, cx: &mut Context<Self>) {
        let cells = self
            .assignments
            .iter()
            .map(|a| AssignedCell {
                script: a.weapon_script().to_owned(),
                label: self.assigned_name(a.crosshair(), cx),
                crosshair: Some(a.crosshair().clone()),
            })
            .collect();
        self.assigner
            .update(cx, |assigner, cx| assigner.set_assigned(cells, cx));
    }

    /// The composite preview's background picker, over `BACKGROUNDS`, starting on "2fort".
    pub fn background_select(&self) -> &Entity<SelectState<Vec<&'static str>>> {
        &self.background
    }

    // Reads the library, the assignments and the tf/custom scan in the background, then asks
    // the preview cache for every library crosshair. Errors toast through the window handle.
    fn reload(&mut self, cx: &mut Context<Self>) {
        self.load_generation += 1;
        let generation = self.load_generation;
        let tf2 = tf2_state(cx).read(cx).install().cloned();
        let loading = run_blocking(cx, move |s| {
            let failed =
                |e: LibraryError| describe_library(&e).into_op("Failed to load crosshairs");
            let library = s.library.list_crosshairs().map_err(failed)?;
            let assignments = s.library.assignments().map_err(failed)?;
            let installed = tf2
                .map(|t| s.library.installed_crosshairs(&t))
                .transpose()
                .map_err(failed)?;
            Ok((library, assignments, installed))
        });
        let win = self.win;
        cx.spawn(async move |page, cx| {
            let loaded = loading.await;
            let current = page
                .update(cx, |page, cx| {
                    if page.load_generation != generation {
                        return false;
                    }
                    if let Ok((library, assignments, installed)) = &loaded {
                        page.assignments = assignments.clone();
                        page.installed = installed.clone();
                        // A crosshair gone from the library takes the selection with it, or
                        // the actions would keep offering it.
                        let gone = matches!(&page.selected, Some(CrosshairRef::Library(id))
                            if !library.iter().any(|c| c.id() == id));
                        let library: Rc<[CrosshairEntry]> = library.as_slice().into();
                        page.picker.update(cx, |picker, cx| {
                            picker.set_library(library, cx);
                            if gone {
                                picker.select(None, cx);
                            }
                        });
                        page.push_assigned(cx);
                        if page.timings.loaded.is_none() {
                            page.timings.loaded = Some(page.created.elapsed());
                            // Nothing left to decode means ready at the same instant as the load.
                            if page.previews.read(cx).pending() == 0 {
                                page.timings.previews_ready = page.timings.loaded;
                            }
                        }
                    }
                    page.loaded = true;
                    page.sync_assigner(cx);
                    cx.notify();
                    true
                })
                .unwrap_or(false);
            if let (true, Err(err)) = (current, loaded) {
                let _ =
                    cx.update(|cx| win.update(cx, |_, window, cx| toast_error(window, cx, &err)));
            }
        })
        .detach();
    }

    // Marks `busy`, runs `op` in `run_blocking`, toasts its outcome on success or
    // `toast_error` on failure through the window handle, then clears `busy` and reloads.
    fn run(
        &mut self,
        busy: Busy,
        op: impl FnOnce(&Services) -> Result<Outcome, OpError> + Send + 'static,
        cx: &mut Context<Self>,
    ) {
        self.set_busy(Some(busy), cx);
        let running = run_blocking(cx, op);
        let win = self.win;
        cx.spawn(async move |page, cx| {
            let done = running.await;
            let ok = done.is_ok();
            cx.update(|cx| {
                let _ = win.update(cx, |_, window, cx| match done {
                    Ok((kind, title, description)) => toast(window, cx, kind, title, description),
                    Err(err) => toast_error(window, cx, &err),
                });
                // Outside the page, so a success is announced even when the page is gone; a
                // surviving page reloads through its own LibraryChanged subscription.
                if ok {
                    LibraryEvents::changed(cx);
                }
            });
            let _ = page.update(cx, |page, cx| {
                page.set_busy(None, cx);
                if !ok {
                    page.reload(cx)
                }
            });
        })
        .detach();
    }

    // Picks the image on the UI thread, then imports it in the background under the file's
    // stem as its name; core rejects anything but a PNG or VTF. A cancel clears `busy` silently.
    fn import(&mut self, cx: &mut Context<Self>) {
        let picking = cx
            .global::<Services>()
            .picker
            .pick(PickKind::CrosshairImage, cx);
        self.set_busy(Some(Busy::Import), cx);
        cx.spawn(async move |page, cx| {
            let picked = picking.await;
            let _ = page.update(cx, |page, cx| match picked {
                Some(path) => page.run(
                    Busy::Import,
                    move |s| {
                        let name = path
                            .file_stem()
                            .map_or_else(|| "crosshair".into(), |stem| stem.to_string_lossy());
                        let crosshair = s
                            .library
                            .import_crosshair(&path, &name, Provenance::Local)
                            .map_err(|e| {
                                describe_import(&e).into_op("Failed to import crosshair")
                            })?;
                        // The source path is developer detail, so it goes to tracing only.
                        tracing::info!(source = %path.display(), "imported a crosshair");
                        let entry = NewLogEntry::new(
                            LogLevel::Info,
                            "crosshair",
                            "import",
                            format!("Imported crosshair: {}", crosshair.name()),
                        );
                        activity::write(s, &entry);
                        let added = added_message(&crosshair);
                        Ok((
                            ToastKind::Success,
                            "Crosshair imported".into(),
                            added.into(),
                        ))
                    },
                    cx,
                ),
                None => page.set_busy(None, cx),
            });
        })
        .detach();
    }

    // Asks where to save, suggesting the crosshair's name, then writes the pixels Copy would
    // read as a PNG in the background. A cancel clears `busy` silently. The dialog only vetted
    // the name it was given, so a ".png" added here never replaces a file already there.
    fn export_png(&mut self, cx: &mut Context<Self>) {
        let Some(crosshair) = self.selected.clone() else {
            return;
        };
        let name = self
            .name_of(&crosshair, cx)
            .unwrap_or_else(|| "crosshair".into());
        let picking = cx.global::<Services>().picker.save_to(
            PickKind::ExportPng,
            &file_name_for(&name, "png"),
            cx,
        );
        self.set_busy(Some(Busy::Export), cx);
        cx.spawn(async move |page, cx| {
            let picked = picking.await;
            let _ = page.update(cx, |page, cx| match picked {
                Some(path) => page.run(
                    Busy::Export,
                    move |s| {
                        let target = with_extension(path.clone(), "png");
                        if target != path && target.exists() {
                            let name = target.file_name().unwrap_or_default().to_string_lossy();
                            return Ok((
                                ToastKind::Warning,
                                format!("A file named {name} is already there. Pick another name.")
                                    .into(),
                                SharedString::default(),
                            ));
                        }
                        let drawing = s
                            .library
                            .crosshair_drawing(&crosshair, &s.builtins)
                            .map_err(|e| describe_library(&e).into_op(EXPORT_FAILED))?;
                        write_png(&target, &drawing)
                            .map_err(|e| describe_png_write(&e).into_op(EXPORT_FAILED))?;
                        Ok((
                            ToastKind::Success,
                            "PNG exported".into(),
                            SharedString::default(),
                        ))
                    },
                    cx,
                ),
                None => page.set_busy(None, cx),
            });
        })
        .detach();
    }

    // Rebuilds the managed folder from the assignments; skipped rows turn the toast into a
    // Warning that lists them.
    fn install(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let tf2 = match require_tf2(cx) {
            Ok(tf2) => tf2,
            Err(err) => return toast_error(window, cx, &err),
        };
        let builtins = self.builtins.clone();
        self.run(
            Busy::Install,
            move |s| {
                let report = s.library.apply_crosshairs(&tf2, &builtins).map_err(|e| {
                    describe_tf2_install(&e).into_op("Failed to install crosshairs")
                })?;
                // v5 logs an install only when it wrote at least one weapon script.
                if !report.applied().is_empty() {
                    let dest = tf2.custom_dir().join("tf2tools-crosshairs");
                    tracing::info!(destination = %dest.display(), "installed crosshairs");
                    let entry = NewLogEntry::new(
                        LogLevel::Info,
                        "crosshair",
                        "install",
                        format!("Installed {} crosshair assignments", report.applied().len()),
                    );
                    activity::write(s, &entry);
                }
                Ok(install_outcome(&report))
            },
            cx,
        );
    }

    // Removes the managed folder; the assignments stay, as v5.
    fn uninstall(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let tf2 = match require_tf2(cx) {
            Ok(tf2) => tf2,
            Err(err) => return toast_error(window, cx, &err),
        };
        self.run(
            Busy::Uninstall,
            move |s| {
                s.library.remove_crosshairs(&tf2).map_err(|e| {
                    describe_tf2_install(&e).into_op("Failed to uninstall crosshairs")
                })?;
                let entry = NewLogEntry::new(
                    LogLevel::Info,
                    "crosshair",
                    "uninstall",
                    "Uninstalled crosshair assignments",
                );
                activity::write(s, &entry);
                let done = "Crosshairs uninstalled from TF2";
                Ok((ToastKind::Success, done.into(), SharedString::default()))
            },
            cx,
        );
    }

    // Asks first, naming what the crosshair is used by; only a confirmed answer deletes. Game
    // files go before the row, so a failed write keeps the crosshair and the delete can be
    // retried. Without a TF2 install there are no reachable files to change.
    fn delete(&mut self, id: CrosshairId, window: &mut Window, cx: &mut Context<Self>) {
        let crosshair = CrosshairRef::Library(id.clone());
        let name = self.assigned_name(&crosshair, cx);
        let weapons = self
            .assignments
            .iter()
            .filter(|a| *a.crosshair() == crosshair)
            .count();
        let installed = self.installed.as_ref().is_some_and(|i| i.is_in_game(&id));
        let answer = confirm(
            Confirm::delete_crosshair(&name, weapons, installed),
            window,
            cx,
        );
        cx.spawn(async move |page, cx| {
            if !answer.recv().await.unwrap_or(false) {
                return;
            }
            let _ = page.update(cx, |page, cx| {
                let tf2 = tf2_state(cx).read(cx).install().cloned();
                page.run(
                    Busy::Delete,
                    move |s| {
                        const FAILED: &str = "Failed to delete crosshair";
                        if let Some(tf2) = tf2 {
                            s.library
                                .uninstall_crosshair(&id, &tf2)
                                .map_err(|e| describe_tf2_install(&e).into_op(FAILED))?;
                        }
                        s.library
                            .delete_crosshair(&id)
                            .map_err(|e| describe_library(&e).into_op(FAILED))?;
                        let message = format!("Deleted crosshair from library: {name}");
                        let entry =
                            NewLogEntry::new(LogLevel::Info, "crosshair", "delete", message);
                        activity::write(s, &entry);
                        let deleted = format!("{name} has been deleted from your library");
                        Ok((
                            ToastKind::Success,
                            "Crosshair removed".into(),
                            deleted.into(),
                        ))
                    },
                    cx,
                );
            });
        })
        .detach();
    }

    // Assigns the selection to one weapon; the row is inert without a selection or while busy.
    fn assign(&mut self, script: &'static str, cx: &mut Context<Self>) {
        let Some(crosshair) = self.selected.clone() else {
            return;
        };
        self.run(
            Busy::Assign(script),
            move |s| {
                s.library
                    .assign(script, &crosshair)
                    .map_err(|e| describe_library(&e).into_op("Failed to assign crosshair"))?;
                let done = "Crosshair assigned";
                Ok((ToastKind::Success, done.into(), SharedString::default()))
            },
            cx,
        );
    }

    // Assigns the selection to `scripts`, the class's weapons (in `slot`, if any), in one
    // background job that changes every weapon or none; without a selection it only says so.
    fn bulk(
        &mut self,
        slot: Option<WeaponSlot>,
        class: TfClass,
        scripts: Vec<&'static str>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(crosshair) = self.selected.clone() else {
            let none = "No crosshair selected";
            return toast(window, cx, ToastKind::Error, none, SharedString::default());
        };
        let done = match slot {
            None => format!("Crosshair applied to all {} weapons", class.name()),
            Some(slot) => format!(
                "Crosshair applied to all {} {} weapons",
                class.name(),
                slot.name().to_lowercase()
            ),
        };
        self.run(
            Busy::Bulk(slot),
            move |s| {
                s.library
                    .assign_many(&scripts, &crosshair)
                    .map_err(|e| describe_library(&e).into_op("Failed to bulk assign"))?;
                Ok((ToastKind::Success, done.into(), SharedString::default()))
            },
            cx,
        );
    }

    // An assignment's label: the crosshair's name, or its raw builtin name or library id when
    // it is not in the catalog or the library.
    fn assigned_name(&self, crosshair: &CrosshairRef, cx: &App) -> String {
        self.name_of(crosshair, cx)
            .unwrap_or_else(|| match crosshair {
                CrosshairRef::Builtin(name) => name.clone(),
                CrosshairRef::Library(id) => id.to_string(),
                _ => String::new(),
            })
    }

    // v5's composite preview: the map background filling the box, the selected crosshair
    // centred over it, or a muted "No crosshair" without a selection.
    fn preview(&self, background: &'static str, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let (border, muted, muted_fg) = (theme.border, theme.muted, theme.muted_foreground);
        let selection = self
            .selected
            .as_ref()
            .and_then(|selected| Some((selected, self.name_of(selected, cx)?)));
        let (label, centre) = match selection {
            Some((selected, name)) => {
                let preview = self.previews.read(cx).preview(selected, cx);
                // Drawn as big as the game draws it, the same as the maker's preview.
                let centre = preview_image(preview, IN_GAME_SIZE).map(|image| {
                    div()
                        .id(ids::CROSSHAIRS_PREVIEW_CROSSHAIR)
                        .test_support()
                        .child(image)
                        .into_any_element()
                });
                (format!("{name} on {background}"), centre)
            }
            None => {
                let none = div().text_xs().text_color(muted_fg).child("No crosshair");
                ("No crosshair".to_owned(), Some(none.into_any_element()))
            }
        };
        let path = cx.global::<Services>().assets.background(background);
        div()
            .id(ids::CROSSHAIRS_PREVIEW)
            .aria_label(label)
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
                    .children(centre),
            )
    }

    // The display name of `crosshair`, as the picker knows it.
    fn name_of(&self, crosshair: &CrosshairRef, cx: &App) -> Option<String> {
        self.picker.read(cx).name_of(crosshair)
    }

    // Import, Install to TF2 and Uninstall: the running one spins, the others disable. Copy
    // and Export PNG show for any selected crosshair; Edit only for one drawn in the maker;
    // Delete only for a library one, since the bundled crosshairs ship with the app.
    fn actions(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let painted = self.selected.as_ref().and_then(|selected| {
            let CrosshairRef::Library(id) = selected else {
                return None;
            };
            let format = self.picker.read(cx).format_of(selected);
            (format == Some(CrosshairFormat::Painted)).then(|| id.clone())
        });
        let edit = painted.map(|id| {
            Button::new(ids::CROSSHAIRS_EDIT)
                .outline()
                .px(SM_ICON_PX)
                .icon(IconName::Pencil)
                .label("Edit")
                .on_click(cx.listener(move |_, _, _, cx| cx.emit(EditPainted(id.clone()))))
        });
        let deleting = self.busy == Some(Busy::Delete);
        let delete = self.selected.as_ref().and_then(|selected| {
            let CrosshairRef::Library(id) = selected else {
                return None;
            };
            let id = id.clone();
            Some(
                Button::new(ids::CROSSHAIRS_DELETE)
                    .danger()
                    .outline()
                    .px(SM_ICON_PX)
                    .icon(IconName::Trash)
                    .label(if deleting { "Deleting..." } else { "Delete" })
                    .loading(deleting)
                    .disabled(self.busy.is_some() && !deleting)
                    .on_click(
                        cx.listener(move |page, _, window, cx| page.delete(id.clone(), window, cx)),
                    ),
            )
        });
        let copy = self.selected.clone().map(|selected| {
            Button::new(ids::CROSSHAIRS_COPY_TO_MAKER)
                .outline()
                .px(SM_ICON_PX)
                .icon(IconName::Copy)
                .label("Copy to Crosshair Maker")
                .disabled(self.busy.is_some())
                .on_click(cx.listener(move |_, _, _, cx| cx.emit(CopyToMaker(selected.clone()))))
        });
        let exporting = self.busy == Some(Busy::Export);
        let export = self.selected.is_some().then(|| {
            Button::new(ids::CROSSHAIRS_EXPORT_PNG)
                .outline()
                .px(SM_ICON_PX)
                .icon(IconName::ImageDown)
                .label(if exporting {
                    "Exporting..."
                } else {
                    "Export PNG"
                })
                .loading(exporting)
                .disabled(self.busy.is_some() && !exporting)
                .on_click(cx.listener(|page, _, _, cx| page.export_png(cx)))
        });
        let importing = self.busy == Some(Busy::Import);
        let installing = self.busy == Some(Busy::Install);
        let removing = self.busy == Some(Busy::Uninstall);
        let has_tf2 = tf2_state(cx).read(cx).install().is_some();
        v_flex()
            .gap(px(6.))
            .child(
                Button::new(ids::CROSSHAIRS_IMPORT)
                    .outline()
                    .px(SM_ICON_PX)
                    .icon(IconName::Upload)
                    .label(if importing { "Importing..." } else { "Import" })
                    .loading(importing)
                    .disabled(self.busy.is_some() && !importing)
                    .on_click(cx.listener(|page, _, _, cx| page.import(cx))),
            )
            .child(needs_tf2(
                Button::new(ids::CROSSHAIRS_INSTALL)
                    .primary()
                    .px(SM_ICON_PX)
                    .icon(IconName::Download)
                    .label(if installing {
                        "Installing..."
                    } else {
                        "Install to TF2"
                    })
                    .loading(installing)
                    .disabled(self.busy.is_some() && !installing)
                    .on_click(cx.listener(|page, _, window, cx| page.install(window, cx))),
                has_tf2,
            ))
            .child(needs_tf2(
                Button::new(ids::CROSSHAIRS_UNINSTALL)
                    .outline()
                    .px(SM_ICON_PX)
                    .icon(IconName::Unplug)
                    .label(if removing { "Removing..." } else { "Uninstall" })
                    .loading(removing)
                    .disabled(self.busy.is_some() && !removing)
                    .on_click(cx.listener(|page, _, window, cx| page.uninstall(window, cx))),
                has_tf2,
            ))
            .children(copy)
            .children(export)
            .children(edit)
            .children(delete)
    }
}

/// The toast line for a crosshair just added to the library, naming the numbered name it
/// took when another library crosshair already had the one it wanted.
pub(crate) fn added_message(crosshair: &CrosshairEntry) -> String {
    match crosshair.wanted_name() {
        Some(_) => format!("Added to your library as {}", crosshair.name()),
        None => format!("{} added to your library", crosshair.name()),
    }
}

// v5's toast for an apply: Success, or a Warning naming each skipped assignment and why.
fn install_outcome(report: &ApplyReport) -> Outcome {
    let title: SharedString = "Crosshairs installed to TF2".into();
    let skipped = report.skipped();
    if skipped.is_empty() {
        return (ToastKind::Success, title, SharedString::default());
    }
    let lines: Vec<String> = skipped
        .iter()
        .map(|skip| {
            let reason = match skip.reason() {
                SkipReason::UnknownBuiltin => "unknown builtin",
                SkipReason::MissingLibraryCrosshair => "missing crosshair",
                SkipReason::InvalidWeaponScript => "invalid weapon script",
                _ => "skipped",
            };
            format!("{} ({reason} {})", skip.weapon_script(), skip.reference())
        })
        .collect();
    let description = format!("Skipped {}: {}", skipped.len(), lines.join(", "));
    (ToastKind::Warning, title, description.into())
}

// v5's compact yellow-500 box: the icon beside a column of the title and one line per
// tf/custom folder whose weapon scripts may override ours.
fn conflict_box(folders: &[String], cx: &App) -> Option<AnyElement> {
    if folders.is_empty() {
        return None;
    }
    let yellow: Hsla = rgb(YELLOW_500).into();
    let muted = cx.theme().muted_foreground;
    let lines = folders.iter().map(|folder| {
        let rest =
            format!(": Crosshair scripts found in '{folder}' — may override your assignments");
        h_flex()
            .id(ids::crosshair_conflict(folder))
            .aria_label(format!("{folder}{rest}"))
            .test_support()
            .text_sm()
            .text_color(muted)
            .child(div().font_weight(FontWeight::MEDIUM).child(folder.clone()))
            .child(rest)
    });
    Some(
        v_flex()
            .id(ids::CROSSHAIRS_CONFLICTS)
            .role(Role::Alert)
            .aria_label("Conflicts Detected")
            .test_support()
            .flex_row()
            .items_start()
            .gap_2()
            .p_3()
            .rounded(cx.theme().radius)
            .border_1()
            .border_color(yellow.opacity(0.5))
            .bg(yellow.opacity(0.05))
            .text_sm()
            .child(
                Icon::new(IconName::TriangleAlert)
                    .with_size(Size::Size(px(16.)))
                    .text_color(yellow)
                    .mt(px(2.)),
            )
            .child(
                v_flex()
                    .child(
                        div()
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(yellow)
                            .child("Conflicts Detected"),
                    )
                    .child(v_flex().mt_1().gap(px(2.)).children(lines)),
            )
            .into_any_element(),
    )
}

impl Render for CrosshairsPage {
    // The whole page scrolls, as v5's does, since the weapon list runs past the window.
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.timings
            .first_render
            .get_or_insert_with(|| self.created.elapsed());
        let background = self
            .background
            .read(cx)
            .selected_value()
            .copied()
            .unwrap_or(BACKGROUNDS[0]);
        let conflicts = self
            .installed
            .as_ref()
            .and_then(|installed| conflict_box(installed.conflicts(), cx));
        let missing = matches!(tf2_state(cx).read(cx), Tf2State::Missing { .. });
        let warning = missing.then(|| {
            no_tf2_notice(
                ids::CROSSHAIRS_NO_TF2,
                ids::CROSSHAIRS_NO_TF2_SETTINGS,
                "Set the TF2 folder in Settings to install crosshairs.",
                cx,
            )
        });
        let content = v_flex()
            .id(ids::page(Page::Crosshairs))
            .test_support()
            .size_full()
            .overflow_y_scroll()
            .track_scroll(&self.scroll)
            .p(CONTENT_PADDING)
            .gap_4()
            .children(warning)
            .child(
                h_flex()
                    .gap_4()
                    .items_center()
                    .child(
                        v_flex()
                            .flex_1()
                            .min_w(px(200.))
                            .gap_2()
                            .child(self.picker.clone())
                            .child(
                                div()
                                    .id(ids::CROSSHAIRS_BACKGROUND)
                                    .aria_label(background)
                                    .test_support()
                                    .w_full()
                                    .child(Select::new(&self.background)),
                            ),
                    )
                    .child(self.preview(background, cx))
                    .child(self.actions(cx)),
            )
            .children(conflicts)
            .child(self.assigner.clone());
        scroll_area(ids::page_scrollbar(Page::Crosshairs), &self.scroll, content).size_full()
    }
}
