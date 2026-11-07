//! The Crosshairs page: a gallery of crosshair tiles, a large in-game-size preview, and a
//! loadout board of class columns and slot rows that assigns by click or drag. Every change
//! saves into TF2 on its own through `CrosshairSync`; each cell opens its per-weapon list.

use std::cell::Cell;
use std::rc::Rc;
use std::time::{Duration, Instant};

use gpui_kit::assets::IconName;
use gpui_kit::component::IndexPath;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::InputState;
use gpui_kit::component::select::{Select, SelectState};
use gpui_kit::component::{
    ActiveTheme as _, Disableable as _, Icon, Sizable as _, Size, h_flex, v_flex,
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, AnyWindowHandle, App, AppContext as _, Context, Entity, EventEmitter, FontWeight,
    Hsla, InteractiveElement as _, IntoElement, MouseButton, ObjectFit, ParentElement as _, Pixels,
    Render, Role, ScrollHandle, SharedString, StatefulInteractiveElement as _, Styled as _,
    StyledImage as _, Subscription, TestSupportExt as _, Window, canvas, div, img, px, rgb,
};

use tf2tools_core::{
    ApplyReport, Assignment, CrosshairEntry, CrosshairFormat, CrosshairId, CrosshairRef,
    InstalledCrosshairs, LibraryError, LogLevel, NewLogEntry, Provenance, SkipReason, TfClass,
    WeaponSlot, write_png,
};

use super::common::{SM_ICON_PX, YELLOW_500, needs_tf2, no_tf2_notice};
use crate::activity;
use crate::assets::BACKGROUNDS;
use crate::crosshair_sync::{CrosshairSync, SyncStatus};
use crate::ids;
use crate::library_events::{LibraryChanged, LibraryEvents};
use crate::loadout::{BoardTarget, CellState, Loadout, SLOTS};
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
use crate::widgets::crosshair_picker::{
    CrosshairPicker, DraggedCrosshair, IN_GAME_SIZE, preview_image,
};
use crate::widgets::labelled;
use crate::widgets::scroll::scroll_area;
use crate::widgets::weapon_assigner::{
    AssignRequested, AssignTarget, AssignedCell, AssignerState, ClassChanged, WeaponAssigner,
    slot_colour,
};

const PREVIEW_SIZE: Pixels = px(160.);
/// The preview's plain backdrop, listed first in its background picker.
const PLAIN: &str = "Dark";
/// The plain backdrop's colour, zinc-900, so a crosshair reads as it does on a dark wall.
const PLAIN_COLOUR: u32 = 0x18181b;
/// Below this main width the gallery stacks over the board.
const SIDE_BY_SIDE: Pixels = px(860.);
/// The gallery is three tiles wide, or four once the main area is this wide.
const WIDE_GALLERY: Pixels = px(1200.);
/// Below this board width the board turns, classes as rows and slots as columns.
const BOARD_COLUMNS: Pixels = px(560.);
/// The board's row header column and its cells' height.
const HEADER_WIDTH: Pixels = px(84.);
const CELL_HEIGHT: Pixels = px(68.);
/// Every Export PNG failure toasts under this action.
const EXPORT_FAILED: &str = "Failed to export PNG";

/// The action in flight: its button spins and the other mutating controls disable.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Busy {
    Import,
    Export,
    Uninstall,
    Delete,
    /// Assigning to the weapon with this script.
    Assign(&'static str),
    /// Assigning to what a board click covers.
    Board(BoardTarget),
    /// Resetting a board cell to the game's crosshair.
    Reset(BoardTarget),
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
    assignments: Vec<Assignment>,
    /// The board's view of `assignments` and of what an apply would change.
    loadout: Loadout,
    installed: Option<InstalledCrosshairs>,
    /// The picker's selection, mirrored on each of its notifies so `selected` needs no `cx`.
    selected: Option<CrosshairRef>,
    class: TfClass,
    /// The board cell whose per-weapon list shows below the board.
    focus: Option<(TfClass, WeaponSlot)>,
    /// Set when a cell opens its list, so the next frame scrolls the list into view.
    reveal: bool,
    /// The main area's painted width, which picks the layout.
    width: Rc<Cell<Pixels>>,
    loaded: bool,
    busy: Option<Busy>,
    /// Bumped by each reload, so a slower, older load never overwrites a newer one.
    load_generation: u64,
    previews: Entity<CrosshairPreviews>,
    created: Instant,
    timings: PageTimings,
    picker: Entity<CrosshairPicker>,
    assigner: Entity<WeaponAssigner>,
    /// The app's apply queue, whose status the page's line shows.
    sync: Entity<CrosshairSync>,
    /// The queue's finished count the page last reloaded for.
    sync_seen: usize,
    _sync: Subscription,
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
        let picker = cx.new(|cx| CrosshairPicker::gallery(window, cx));
        // The composite preview and the weapon rows read the selection, so each picker notify
        // refreshes the mirror and redraws the page.
        let observe_picker = cx.observe(&picker, |page: &mut Self, picker, cx| {
            page.selected = picker.read(cx).selected().cloned();
            page.sync_assigner(cx);
            cx.notify();
        });
        // The assigner only asks; the page writes the table, and mirrors the shown class from
        // the assigner's change event so `class` needs no `cx`.
        let assigner = cx.new(|cx| {
            let mut assigner = WeaponAssigner::new(false, cx);
            assigner.set_bar(false, cx);
            assigner
        });
        let assign_requested = cx.subscribe_in(
            &assigner,
            window,
            |page: &mut Self, _, event: &AssignRequested, window, cx| match event.target {
                AssignTarget::Weapon(script) => page.assign(script, cx),
                AssignTarget::Bulk(slot) => {
                    let target = match slot {
                        Some(slot) => BoardTarget::Cell(event.class, slot),
                        None => BoardTarget::Class(event.class),
                    };
                    page.board_assign(target, None, window, cx)
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
        let backgrounds: Vec<&'static str> = std::iter::once(PLAIN).chain(BACKGROUNDS).collect();
        let background =
            cx.new(|cx| SelectState::new(backgrounds, Some(IndexPath::default()), window, cx));
        // Each finished apply changes what is in the game, so the page reloads its scan.
        let sync = CrosshairSync::get_or_install(window.window_handle(), cx);
        let observe_sync = cx.observe(&sync, |page: &mut Self, sync, cx| {
            let finished = sync.read(cx).finished();
            if finished != page.sync_seen {
                page.sync_seen = finished;
                page.reload(cx);
            }
            cx.notify();
        });
        let sync_seen = sync.read(cx).finished();
        let mut page = Self {
            win: window.window_handle(),
            assignments: Vec::new(),
            loadout: Loadout::default(),
            installed: None,
            selected: None,
            class: TfClass::Scout,
            focus: None,
            reveal: false,
            width: Rc::new(Cell::new(px(928.))),
            loaded: false,
            busy: None,
            load_generation: 0,
            previews,
            created: Instant::now(),
            timings: PageTimings::default(),
            picker,
            assigner,
            sync,
            sync_seen,
            _sync: observe_sync,
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

    /// The class whose weapons the per-weapon list shows; Scout until a cell opens it.
    pub fn class(&self) -> TfClass {
        self.class
    }

    /// Opens the per-weapon list on `class`, keeping the open slot (Primary at first).
    pub fn set_class(&mut self, class: TfClass, cx: &mut Context<Self>) {
        let slot = self.focus.map_or(WeaponSlot::Primary, |(_, slot)| slot);
        self.open_details(class, slot, cx);
    }

    /// The board cell whose per-weapon list is open, if any.
    pub fn focus(&self) -> Option<(TfClass, WeaponSlot)> {
        self.focus
    }

    // Shows the weapons of one board cell below the board, for per-weapon picks and reset.
    fn open_details(&mut self, class: TfClass, slot: WeaponSlot, cx: &mut Context<Self>) {
        self.focus = Some((class, slot));
        self.reveal = true;
        self.class = class;
        self.assigner.update(cx, |assigner, cx| {
            assigner.set_class(class, cx);
            assigner.set_slot(Some(slot), cx);
        });
        cx.notify();
    }

    // Tells the assigner whether rows may assign and which button spins; called wherever the
    // selection or `busy` changes, or a missed call leaves the rows inert after a pick.
    fn sync_assigner(&self, cx: &mut Context<Self>) {
        let running = match self.busy {
            Some(Busy::Assign(script)) => Some(AssignTarget::Weapon(script)),
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
                .as_ref()
                .map(|t| s.library.installed_crosshairs(t))
                .transpose()
                .map_err(failed)?;
            let waiting = match &tf2 {
                Some(t) => s
                    .library
                    .crosshairs_not_in_game(t, &s.builtins)
                    .map_err(failed)?,
                None => Vec::new(),
            };
            Ok((library, assignments, installed, waiting))
        });
        let win = self.win;
        cx.spawn(async move |page, cx| {
            let loaded = loading.await;
            let current = page
                .update(cx, |page, cx| {
                    if page.load_generation != generation {
                        return false;
                    }
                    if let Ok((library, assignments, installed, waiting)) = &loaded {
                        page.assignments = assignments.clone();
                        page.loadout = Loadout::from_assignments(assignments, waiting);
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

    // Marks `busy`, runs `op` in `run_blocking`, toasts its outcome (if any) on success or
    // `toast_error` on failure through the window handle, then clears `busy` and reloads. A
    // changed assignment asks the apply queue to bring the game up to date.
    fn run(
        &mut self,
        busy: Busy,
        op: impl FnOnce(&Services) -> Result<Option<Outcome>, OpError> + Send + 'static,
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
                    Ok(Some((kind, title, description))) => {
                        toast(window, cx, kind, title, description)
                    }
                    Ok(None) => {}
                    Err(err) => toast_error(window, cx, &err),
                });
                // Outside the page, so a success is announced even when the page is gone; a
                // surviving page reloads through its own LibraryChanged subscription.
                if ok {
                    LibraryEvents::changed(cx);
                    match busy {
                        Busy::Assign(_) | Busy::Board(_) | Busy::Reset(_) => {
                            CrosshairSync::request_global(cx)
                        }
                        Busy::Uninstall => {
                            if let Some(sync) = CrosshairSync::global(cx) {
                                sync.update(cx, |sync, cx| sync.mark_removed(cx));
                            }
                        }
                        Busy::Import | Busy::Export | Busy::Delete => {}
                    }
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
                        Ok(Some((
                            ToastKind::Success,
                            "Crosshair imported".into(),
                            added.into(),
                        )))
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
                            return Ok(Some((
                                ToastKind::Warning,
                                format!("A file named {name} is already there. Pick another name.")
                                    .into(),
                                SharedString::default(),
                            )));
                        }
                        let drawing = s
                            .library
                            .crosshair_drawing(&crosshair, &s.builtins)
                            .map_err(|e| describe_library(&e).into_op(EXPORT_FAILED))?;
                        write_png(&target, &drawing)
                            .map_err(|e| describe_png_write(&e).into_op(EXPORT_FAILED))?;
                        Ok(Some((
                            ToastKind::Success,
                            "PNG exported".into(),
                            SharedString::default(),
                        )))
                    },
                    cx,
                ),
                None => page.set_busy(None, cx),
            });
        })
        .detach();
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
                Ok(Some((
                    ToastKind::Success,
                    done.into(),
                    SharedString::default(),
                )))
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
                        Ok(Some((
                            ToastKind::Success,
                            "Crosshair removed".into(),
                            deleted.into(),
                        )))
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
                Ok(None)
            },
            cx,
        );
    }

    // Assigns `crosshair`, or the selection, to every weapon `target` covers in one job that
    // changes every weapon or none. Without either it asks for a pick; while busy it waits.
    fn board_assign(
        &mut self,
        target: BoardTarget,
        crosshair: Option<CrosshairRef>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.busy.is_some() {
            return;
        }
        let Some(crosshair) = crosshair.or_else(|| self.selected.clone()) else {
            let none = "Pick a crosshair first";
            return toast(window, cx, ToastKind::Error, none, SharedString::default());
        };
        let scripts = target.scripts();
        self.run(
            Busy::Board(target),
            move |s| {
                s.library
                    .assign_many(&scripts, &crosshair)
                    .map_err(|e| describe_library(&e).into_op("Failed to assign crosshairs"))?;
                Ok(None)
            },
            cx,
        );
    }

    // Takes every weapon `target` covers back to the game's own crosshair.
    fn reset(&mut self, target: BoardTarget, cx: &mut Context<Self>) {
        if self.busy.is_some() {
            return;
        }
        let scripts = target.scripts();
        self.run(
            Busy::Reset(target),
            move |s| {
                s.library
                    .unassign_many(&scripts)
                    .map_err(|e| describe_library(&e).into_op("Failed to reset crosshairs"))?;
                Ok(None)
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

    // The large preview: the backdrop filling the box (plain dark, or a map), the selected
    // crosshair centred over it at the size the game draws it, or a muted "No crosshair".
    fn preview(&self, background: &'static str, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let (border, muted_fg) = (theme.border, theme.muted_foreground);
        let selection = self
            .selected
            .as_ref()
            .and_then(|selected| Some((selected, self.name_of(selected, cx)?)));
        let (label, centre) = match selection {
            Some((selected, name)) => {
                let preview = self.previews.read(cx).preview(selected, cx);
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
        let backdrop = (background != PLAIN).then(|| {
            let path = cx.global::<Services>().assets.background(background);
            img(path)
                .absolute()
                .inset_0()
                .size_full()
                .object_fit(ObjectFit::Cover)
        });
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
            .bg(rgb(PLAIN_COLOUR))
            .overflow_hidden()
            .children(backdrop)
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

    // The quiet line over the page: where the last change stands in TF2, and a ghost button
    // that takes every crosshair out of the game while keeping the board.
    fn status_bar(&self, cx: &mut Context<Self>) -> AnyElement {
        let has_tf2 = tf2_state(cx).read(cx).install().is_some();
        let sync = self.sync.read(cx);
        let (status, saving) = (sync.status().clone(), sync.is_saving());
        let line = status.line(!self.assignments.is_empty(), self.loadout.waiting_count());
        let theme = cx.theme();
        let muted = theme.muted_foreground;
        let icon = match status {
            SyncStatus::Saving => IconName::Loader,
            SyncStatus::Skipped(_) | SyncStatus::Failed => IconName::TriangleAlert,
            SyncStatus::Idle
            | SyncStatus::InGame
            | SyncStatus::InGameWhileRunning
            | SyncStatus::WaitingForTf2
            | SyncStatus::Removed => IconName::Check,
        };
        let removing = self.busy == Some(Busy::Uninstall);
        h_flex()
            .id(ids::CROSSHAIRS_STATUS)
            .test_support()
            .gap_2()
            .min_h(px(32.))
            .child(
                Icon::new(icon)
                    .with_size(Size::Size(px(14.)))
                    .text_color(muted),
            )
            .child(labelled(
                div().flex_1().min_w_0().text_sm().text_color(muted),
                ids::CROSSHAIRS_WAITING,
                line,
            ))
            .child(needs_tf2(
                Button::new(ids::CROSSHAIRS_UNINSTALL)
                    .ghost()
                    .small()
                    .icon(IconName::Unplug)
                    .label(if removing {
                        "Removing..."
                    } else {
                        "Remove from TF2"
                    })
                    .loading(removing)
                    .disabled((self.busy.is_some() || saving) && !removing)
                    .on_click(cx.listener(|page, _, window, cx| page.uninstall(window, cx))),
                has_tf2,
            ))
            .into_any_element()
    }

    // The gallery column: a hint and Import over the picker's tile grid.
    fn gallery(&self, height: Pixels, cx: &mut Context<Self>) -> AnyElement {
        let importing = self.busy == Some(Busy::Import);
        let muted = cx.theme().muted_foreground;
        v_flex()
            .gap_2()
            .h(height)
            .child(
                h_flex()
                    .gap_2()
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .text_xs()
                            .text_color(muted)
                            .child("Click or drag one onto the board"),
                    )
                    .child(
                        Button::new(ids::CROSSHAIRS_IMPORT)
                            .outline()
                            .small()
                            .icon(IconName::Upload)
                            .label(if importing { "Importing..." } else { "Import" })
                            .loading(importing)
                            .disabled(self.busy.is_some() && !importing)
                            .on_click(cx.listener(|page, _, _, cx| page.import(cx))),
                    ),
            )
            .child(div().flex_1().min_h_0().child(self.picker.clone()))
            .into_any_element()
    }

    // The selected crosshair: the large preview, its name and how many weapons use it, the
    // backdrop picker, and what can be done with it. Edit shows only for one drawn in the
    // maker; Delete only for a library one, since the bundled crosshairs ship with the app.
    fn selection_panel(&self, background: &'static str, cx: &mut Context<Self>) -> AnyElement {
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
                .small()
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
                    .small()
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
                .small()
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
                .small()
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
        let muted = cx.theme().muted_foreground;
        let (title, detail) = match &self.selected {
            Some(selected) => {
                let used = self
                    .assignments
                    .iter()
                    .filter(|a| a.crosshair() == selected)
                    .count();
                let detail = match used {
                    0 => "Not on any weapon yet".to_owned(),
                    1 => "On 1 weapon".to_owned(),
                    n => format!("On {n} weapons"),
                };
                (self.assigned_name(selected, cx), detail)
            }
            None => (
                "No crosshair picked".to_owned(),
                "Pick one from the gallery to see it at its in-game size".to_owned(),
            ),
        };
        h_flex()
            .items_start()
            .gap_4()
            .child(self.preview(background, cx))
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .gap_2()
                    .child(labelled(
                        div()
                            .text_base()
                            .font_weight(FontWeight::SEMIBOLD)
                            .truncate(),
                        ids::CROSSHAIRS_SELECTED_NAME,
                        title,
                    ))
                    .child(div().text_xs().text_color(muted).child(detail))
                    .child(
                        div()
                            .id(ids::CROSSHAIRS_BACKGROUND)
                            .aria_label(background)
                            .test_support()
                            .w(px(180.))
                            .child(Select::new(&self.background).small()),
                    )
                    .child(
                        h_flex()
                            .flex_wrap()
                            .gap_2()
                            .children(copy)
                            .children(export)
                            .children(edit)
                            .children(delete),
                    ),
            )
            .into_any_element()
    }

    // The loadout board: an "Everything" corner, class headers and slot headers that assign
    // their whole line, and one cell per class and slot. Narrow boards turn, classes as rows.
    fn board(&self, turned: bool, cx: &mut Context<Self>) -> AnyElement {
        let short = px(32.);
        let everything = self.header(
            ids::CROSSHAIRS_BOARD_EVERYTHING.into(),
            ("Everything".into(), "Everything".into()),
            None,
            BoardTarget::Everything,
            (Some(HEADER_WIDTH), short),
            cx,
        );
        let primary = cx.theme().primary;
        // Players' own short names keep narrow class columns readable.
        let short_names = !turned && self.width.get() < px(1100.);
        let class_header = |page: &Self, class: TfClass, size, cx: &mut Context<Self>| {
            let shown = match class {
                TfClass::Demoman if short_names => "Demo",
                TfClass::Engineer if short_names => "Engie",
                TfClass::Scout
                | TfClass::Soldier
                | TfClass::Pyro
                | TfClass::Demoman
                | TfClass::Heavy
                | TfClass::Engineer
                | TfClass::Medic
                | TfClass::Sniper
                | TfClass::Spy
                | _ => class.name(),
            };
            page.header(
                ids::loadout_class(class),
                (class.name().into(), shown.into()),
                None,
                BoardTarget::Class(class),
                size,
                cx,
            )
        };
        let slot_header = |page: &Self, slot: WeaponSlot, size, cx: &mut Context<Self>| {
            page.header(
                ids::loadout_slot(slot),
                (slot.name().into(), slot.name().into()),
                Some(slot_colour(slot, primary)),
                BoardTarget::Slot(slot),
                size,
                cx,
            )
        };
        let mut rows = Vec::new();
        if turned {
            let mut head = h_flex().gap_1().child(everything);
            for slot in SLOTS {
                head = head.child(slot_header(self, slot, (None, short), cx));
            }
            rows.push(head);
            for class in TfClass::ALL {
                let mut row = h_flex().gap_1().child(class_header(
                    self,
                    class,
                    (Some(HEADER_WIDTH), CELL_HEIGHT),
                    cx,
                ));
                for slot in SLOTS {
                    row = row.child(self.cell(class, slot, cx));
                }
                rows.push(row);
            }
        } else {
            let mut head = h_flex().gap_1().child(everything);
            for class in TfClass::ALL {
                head = head.child(class_header(self, class, (None, short), cx));
            }
            rows.push(head);
            for slot in SLOTS {
                let mut row = h_flex().gap_1().child(slot_header(
                    self,
                    slot,
                    (Some(HEADER_WIDTH), CELL_HEIGHT),
                    cx,
                ));
                for class in TfClass::ALL {
                    row = row.child(self.cell(class, slot, cx));
                }
                rows.push(row);
            }
        }
        v_flex()
            .id(ids::CROSSHAIRS_BOARD)
            .test_support()
            .gap_1()
            .children(rows)
            .into_any_element()
    }

    // A board header: a click assigns the selection to its whole line, and a crosshair
    // dropped on it does the same.
    fn header(
        &self,
        id: gpui_kit::ElementId,
        (label, shown): (SharedString, SharedString),
        dot: Option<Hsla>,
        target: BoardTarget,
        size: (Option<Pixels>, Pixels),
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = cx.theme();
        let (hover, muted, primary, input) = (
            theme.accent,
            theme.muted_foreground,
            theme.primary,
            theme.input,
        );
        let running = self.busy == Some(Busy::Board(target));
        let (width, height) = size;
        div()
            .map(|h| match width {
                Some(width) => h.flex_none().w(width),
                None => h.flex_1().flex_basis(px(0.)).min_w_0(),
            })
            .h(height)
            .id(id)
            .aria_label(label)
            .test_support()
            .px_1()
            .flex()
            .items_center()
            .justify_center()
            .gap_1()
            .rounded(theme.radius)
            .text_xs()
            .font_weight(FontWeight::MEDIUM)
            .text_color(muted)
            .cursor_pointer()
            .when(running, |h| h.bg(input.opacity(0.3)))
            .hover(|h| h.bg(hover).text_color(primary))
            .drag_over::<DraggedCrosshair>(move |h, _, _, _| h.bg(hover).text_color(primary))
            .on_click(
                cx.listener(move |page, _, window, cx| page.board_assign(target, None, window, cx)),
            )
            .on_drop(
                cx.listener(move |page, dragged: &DraggedCrosshair, window, cx| {
                    page.board_assign(target, Some(dragged.crosshair.clone()), window, cx)
                }),
            )
            .children(dot.map(|dot| div().flex_none().size(px(6.)).rounded_full().bg(dot)))
            .child(div().min_w_0().truncate().child(shown))
            .into_any_element()
    }

    // One board cell: the shared crosshair's drawing and name, "Mixed", or "Default" while
    // the game draws its own. A click or a drop assigns; a right-click or the corner button
    // opens its per-weapon list. Cells that use the selection are ringed.
    fn cell(&self, class: TfClass, slot: WeaponSlot, cx: &mut Context<Self>) -> AnyElement {
        let state = self.loadout.cell(class, slot);
        let label = match &state {
            CellState::Default => "Default".to_owned(),
            CellState::Mixed => "Mixed".to_owned(),
            CellState::Single(crosshair) => self.assigned_name(crosshair, cx),
        };
        let drawing = match &state {
            CellState::Single(crosshair) => {
                let preview = self.previews.read(cx).preview(crosshair, cx);
                preview_image(preview, px(32.)).map(IntoElement::into_any_element)
            }
            CellState::Mixed => Some(
                Icon::new(IconName::Layers)
                    .with_size(Size::Size(px(16.)))
                    .text_color(cx.theme().muted_foreground)
                    .into_any_element(),
            ),
            _ => None,
        };
        let theme = cx.theme();
        let uses_selection =
            matches!((&state, &self.selected), (CellState::Single(c), Some(s)) if c == s);
        let focused = self.focus == Some((class, slot));
        let border = if uses_selection {
            theme.primary
        } else if focused {
            theme.ring
        } else {
            theme.border
        };
        let (hover, muted, fg, accent) = (
            theme.accent,
            theme.muted_foreground,
            theme.foreground,
            theme.primary,
        );
        let target = BoardTarget::Cell(class, slot);
        let running = matches!(self.busy, Some(Busy::Board(t) | Busy::Reset(t)) if t == target);
        let menu = div()
            .id(ids::loadout_cell_menu(class, slot))
            .aria_label(format!(
                "{} {} weapons",
                class.name(),
                slot.name().to_lowercase()
            ))
            .test_support()
            .absolute()
            .top(px(2.))
            .right(px(2.))
            .size(px(18.))
            .rounded(px(4.))
            .flex()
            .items_center()
            .justify_center()
            .text_color(muted)
            .hover(|m| m.bg(theme.secondary).text_color(fg))
            .on_click(cx.listener(move |page, _, _, cx| {
                cx.stop_propagation();
                page.open_details(class, slot, cx);
            }))
            .child(Icon::new(IconName::EllipsisVertical).with_size(Size::Size(px(12.))));
        v_flex()
            .id(ids::loadout_cell(class, slot))
            .aria_label(SharedString::from(label.clone()))
            .test_support()
            .relative()
            .flex_1()
            .flex_basis(px(0.))
            .min_w_0()
            .h(CELL_HEIGHT)
            .items_center()
            .justify_center()
            .gap_1()
            .px_1()
            .rounded(theme.radius)
            .border_1()
            .border_color(border)
            .bg(if focused {
                theme.secondary
            } else {
                theme.background
            })
            .cursor_pointer()
            .when(running, |c| c.opacity(0.6))
            .hover(|c| c.bg(hover))
            .drag_over::<DraggedCrosshair>(move |c, _, _, _| c.border_color(accent).bg(hover))
            .on_click(
                cx.listener(move |page, _, window, cx| page.board_assign(target, None, window, cx)),
            )
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(move |page, _, _, cx| page.open_details(class, slot, cx)),
            )
            .on_drop(
                cx.listener(move |page, dragged: &DraggedCrosshair, window, cx| {
                    page.board_assign(target, Some(dragged.crosshair.clone()), window, cx)
                }),
            )
            .child(
                div()
                    .flex_none()
                    .size(px(36.))
                    .rounded(px(4.))
                    .bg(if matches!(state, CellState::Default) {
                        cx.theme().muted
                    } else {
                        rgb(0x000000).into()
                    })
                    .flex()
                    .items_center()
                    .justify_center()
                    .children(drawing),
            )
            .child(
                div()
                    .w_full()
                    .text_center()
                    .truncate()
                    .text_xs()
                    .text_color(if matches!(state, CellState::Single(_)) {
                        fg
                    } else {
                        muted
                    })
                    .child(label),
            )
            .child(menu)
            .into_any_element()
    }

    // The open cell's weapons, each assignable on its own, with reset and close.
    fn details(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let (class, slot) = self.focus?;
        let target = BoardTarget::Cell(class, slot);
        let title = format!("{} {} weapons", class.name(), slot.name().to_lowercase());
        let resetting = self.busy == Some(Busy::Reset(target));
        let theme = cx.theme();
        Some(
            v_flex()
                .id(ids::CROSSHAIRS_DETAILS)
                .aria_label(SharedString::from(title.clone()))
                .test_support()
                .gap_3()
                .p_4()
                .rounded_lg()
                .border_1()
                .border_color(theme.border)
                .child(
                    h_flex()
                        .gap_2()
                        .child(
                            v_flex()
                                .flex_1()
                                .min_w_0()
                                .child(
                                    div()
                                        .text_sm()
                                        .font_weight(FontWeight::SEMIBOLD)
                                        .child(title),
                                )
                                .child(
                                    div()
                                        .text_xs()
                                        .text_color(theme.muted_foreground)
                                        .child("Click a weapon to give it the picked crosshair"),
                                ),
                        )
                        .child(
                            Button::new(ids::CROSSHAIRS_DETAILS_RESET)
                                .outline()
                                .small()
                                .icon(IconName::RotateCcw)
                                .label("Reset to default")
                                .loading(resetting)
                                .disabled(self.busy.is_some() && !resetting)
                                .on_click(
                                    cx.listener(move |page, _, _, cx| page.reset(target, cx)),
                                ),
                        )
                        .child(
                            Button::new(ids::CROSSHAIRS_DETAILS_CLOSE)
                                .ghost()
                                .small()
                                .icon(IconName::X)
                                .label("Close")
                                .on_click(cx.listener(|page, _, _, cx| {
                                    page.focus = None;
                                    cx.notify();
                                })),
                        ),
                )
                .child(self.assigner.clone())
                .into_any_element(),
        )
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

/// The apply toast's line naming each skipped assignment and why, or `None` when none was.
pub(crate) fn skipped_description(report: &ApplyReport) -> Option<String> {
    let skipped = report.skipped();
    if skipped.is_empty() {
        return None;
    }
    let lines: Vec<String> = skipped
        .iter()
        .map(|skip| {
            let reason = match skip.reason() {
                SkipReason::UnknownBuiltin => "unknown builtin",
                SkipReason::MissingLibraryCrosshair => "missing crosshair",
                SkipReason::InvalidWeaponScript => "invalid weapon script",
                SkipReason::NoStockScript => "game script unreadable",
                _ => "skipped",
            };
            format!("{} ({reason} {})", skip.weapon_script(), skip.reference())
        })
        .collect();
    Some(format!("Skipped {}: {}", skipped.len(), lines.join(", ")))
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
    // The bar, then the gallery beside (or over) the preview and the board, then the open
    // cell's weapons; the whole page scrolls when the window is short.
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.timings
            .first_render
            .get_or_insert_with(|| self.created.elapsed());
        let background = self
            .background
            .read(cx)
            .selected_value()
            .copied()
            .unwrap_or(PLAIN);
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

        // The main area's width picks the layout; a change asks for another frame, since a
        // refresh requested while drawing is dropped.
        let measured = self.width.clone();
        let watch = canvas(
            move |bounds, window, _| {
                if measured.get() != bounds.size.width {
                    measured.set(bounds.size.width);
                    window.request_animation_frame();
                }
            },
            |_, _, _, _| {},
        )
        .absolute()
        .size_full();
        let width = self.width.get();
        let side_by_side = width >= SIDE_BY_SIDE;
        let gallery_width = if width >= WIDE_GALLERY {
            px(408.)
        } else {
            px(312.)
        };
        let board_width = if side_by_side {
            width - gallery_width - px(24.)
        } else {
            width
        };
        let board = self.board(board_width < BOARD_COLUMNS, cx);
        let selection = self.selection_panel(background, cx);
        let details = self.details(cx);
        let main = if side_by_side {
            h_flex()
                .items_start()
                .gap_6()
                .child(
                    div()
                        .flex_none()
                        .w(gallery_width)
                        .child(self.gallery(px(560.), cx)),
                )
                .child(
                    v_flex()
                        .flex_1()
                        .min_w_0()
                        .gap_4()
                        .child(selection)
                        .child(board),
                )
                .into_any_element()
        } else {
            v_flex()
                .gap_4()
                .child(self.gallery(px(320.), cx))
                .child(selection)
                .child(board)
                .into_any_element()
        };
        // The list sits last, after the warning, bar, conflicts and main area that are present.
        if details.is_some() && std::mem::take(&mut self.reveal) {
            let ix = usize::from(warning.is_some()) + usize::from(conflicts.is_some()) + 2;
            self.scroll.scroll_to_item(ix);
        }
        let content = v_flex()
            .id(ids::page(Page::Crosshairs))
            .test_support()
            .size_full()
            .overflow_y_scroll()
            .track_scroll(&self.scroll)
            .p(CONTENT_PADDING)
            .gap_4()
            .children(warning)
            .child(self.status_bar(cx))
            .children(conflicts)
            .child(div().relative().w_full().child(watch).child(main))
            .children(details);
        scroll_area(ids::page_scrollbar(Page::Crosshairs), &self.scroll, content).size_full()
    }
}
