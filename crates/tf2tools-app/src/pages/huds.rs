//! v5's HUDs page: the installed HUD, conflicts and the HUD library.

use std::collections::{HashMap, HashSet};
use std::ops::Range;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::{Arc, Mutex};

use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::tag::Tag;
use gpui_kit::component::{
    ActiveTheme as _, Disableable as _, Icon, Sizable as _, Size, h_flex, v_flex,
};
use gpui_kit::{
    AnyElement, AnyWindowHandle, App, AppContext as _, AsyncApp, ClickEvent, Context, Entity,
    EventEmitter, FontWeight, Hsla, InteractiveElement as _, IntoElement, ObjectFit,
    ParentElement as _, Pixels, Render, Role, ScrollHandle, SharedString,
    StatefulInteractiveElement as _, Styled as _, StyledImage as _, Subscription, Task,
    TestSupportExt as _, WeakEntity, Window, div, img, px, relative, rgb, transparent_black,
};

use tf2tools_core::{
    CopyUpdate, HudEntry, HudId, HudInstall, HudStatus, LibraryError, LogLevel, NewLogEntry,
    Platform, Provenance, StoreItem, StoreItemId, StoreKind, Tf2Install,
};

use super::common::{ORANGE_500, SM_ICON_PX, badge_of, card_frame};
use crate::activity;
use crate::font_check::FontCheckSession;
use crate::ids;
use crate::library_events::{LibraryChanged, LibraryEvents};
use crate::local_messages::{
    copy_in_store_text, copy_made_text, copy_updated_notice, describe_copy_hud, describe_import,
    describe_library, describe_tf2_install, describe_update_copy,
};
use crate::pages::hud_maker::live::{Live, live_job};
use crate::picker::PickKind;
use crate::remote_images::{ImageKey, RemoteImage, RemoteImages};
use crate::services::{OpError, Services, require_tf2, run_blocking, tf2_state};
use crate::shell::CONTENT_PADDING;
use crate::shell::confirm::{Confirm, confirm};
use crate::shell::external_huds::{ExternalPlan, HudAction, check_external};
use crate::shell::font_check::ask_about_fonts;
use crate::shell::route::Page;
use crate::shell::toast::{ToastKind, error_label, toast, toast_error};
use crate::timing;
use crate::widgets::card_grid::CardGrid;
use crate::widgets::carousel::open_lightbox;
use crate::widgets::labelled;
use crate::widgets::scroll::scroll_area;

/// The card's text and buttons below a store HUD's 16:9 screenshot, and a local card's height.
const CARD_TEXT_HEIGHT: Pixels = px(136.);
// The grid never gets shorter than about one row of cards; a short window scrolls the page.
const LIBRARY_MIN_HEIGHT: Pixels = px(320.);
// Room for up to three lines of the note that a copy's newer version is in the HUD Store.
const STORE_NOTE_HEIGHT: Pixels = px(48.);
/// Edit's tooltip while another HUD action runs.
const EDIT_WAIT: &str = "Wait for TF2Tools to finish, then edit";
/// The title of a failed install's toast.
pub(crate) const INSTALL_FAILED: &str = "Failed to install HUD";
const UNINSTALL_FAILED: &str = "Failed to uninstall HUD";
/// Added to the update toast when the installed copy could not follow the update.
const NOT_REINSTALLED: &str =
    "Updated, but couldn't update the installed copy. Close TF2 and install it again from HUDs.";

/// The action in flight; its button spins and every other mutating button is disabled.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Busy {
    Import,
    /// Keyed by the HUD's folder name.
    Install(String),
    Uninstall,
    /// Uninstalling one HUD of a conflict, keyed by its folder under `tf/custom`.
    UninstallOne(String),
    /// Keyed by the HUD's folder name.
    Delete(String),
    /// Keyed by the copied HUD's folder name.
    Copy(String),
    /// Keyed by the updated copy's folder name.
    Update(String),
}

/// Asks the shell to open made HUD `0` in the HUD Maker.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditInHudMaker(pub HudId);

/// The HUDs page. Built fresh on each visit; it reloads whenever the TF2 state changes.
pub struct HudsPage {
    win: AnyWindowHandle,
    huds: Rc<[HudEntry]>,
    /// Folders of the HUDs made in the HUD Maker, which offer Edit.
    made: Rc<HashSet<String>>,
    /// Copies' folders and the name of the HUD each was copied from.
    copies: Rc<HashMap<String, String>>,
    /// Copies' folders and whether a newer version of their HUD is out, checked after a load.
    updates: Rc<HashMap<String, CopyUpdate>>,
    /// Each HUD's name lowercased, index for index with `huds`, so a search keystroke allocates none.
    lowered: Vec<String>,
    /// The tf/custom scan and the folder it scanned, absent while no TF2 install is known.
    status: Option<(PathBuf, HudStatus)>,
    loaded: bool,
    busy: Option<Busy>,
    /// Bumped by each reload, so a slower, older load never overwrites a newer one.
    load_generation: u64,
    /// Library loads started and not yet landed, current or stale.
    loads_in_flight: usize,
    /// The last load failure's toast label, so a repeat stays quiet until a load succeeds.
    last_failure: Option<SharedString>,
    /// Store HUDs' screenshot URLs by folder, resolved off-thread with the HUD list.
    image_urls: Rc<HashMap<String, String>>,
    /// The grid's last drawn range, as indices into the filtered HUDs.
    visible: Range<usize>,
    /// A screenshot's gallery read is running; further screenshot clicks are ignored.
    gallery_pending: bool,
    search: Entity<InputState>,
    grid: CardGrid,
    /// The page's own scroll, which moves only in a window too short for the page.
    scroll: ScrollHandle,
    /// This page's own `RemoteImages` viewer, so a page released late cannot clear a newer one's wants.
    image_viewer: String,
    _subscriptions: Vec<Subscription>,
}

impl EventEmitter<EditInHudMaker> for HudsPage {}

impl HudsPage {
    /// Needs the `Services` and `Tf2Global` globals. Starts the first load off the UI thread.
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let mut page = Self::build(Vec::new(), false, window, cx);
        page.reload(cx);
        page
    }

    /// Like `new`, but starts loaded with `huds` and reports the first load to the timing hook; it
    /// reloads at once only when a TF2 install is already known, for the tf/custom scan.
    pub fn with_library(huds: Vec<HudEntry>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let mut page = Self::build(huds, true, window, cx);
        timing::huds_loaded(window, cx);
        if tf2_state(cx).read(cx).install().is_some() {
            page.reload(cx);
        } else {
            page.resolve_image_urls(cx);
        }
        page
    }

    // The page with its subscriptions wired; the caller decides whether a load still has to run.
    fn build(
        huds: Vec<HudEntry>,
        loaded: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let tf2 = cx.observe(&tf2_state(cx), |page, _, cx| page.reload(cx));
        let events = LibraryEvents::global(cx);
        let library = cx.subscribe(&events, |page, _, _: &LibraryChanged, cx| page.reload(cx));
        let search = cx.new(|cx| InputState::new(window, cx).placeholder("Search HUDs..."));
        // The grid filters from the input's value at render, so each edit just redraws.
        // A new filter can keep the count and range while changing the cards, so re-arm the
        // grid's visible hook to ask for the new cards' screenshots.
        let search_changed = cx.subscribe(&search, |page, _, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Change) {
                page.grid.rearm();
                cx.notify();
            }
        });
        let mut subscriptions = vec![tf2, library, search_changed];
        // Screenshots redraw the page as they land, and the page's wants go with it.
        let image_viewer = format!("huds:{}", cx.entity_id());
        if let Some(images) = RemoteImages::global(cx) {
            subscriptions.push(cx.observe(&images, |_, _, cx| cx.notify()));
            let released = image_viewer.clone();
            subscriptions.push(cx.on_release(move |_, cx| {
                images.update(cx, |images, cx| images.want(&released, Vec::new(), cx));
            }));
        }
        let mut grid = CardGrid::new(ids::HUDS_GRID);
        let this = cx.entity().downgrade();
        grid.on_visible(move |range, cx| {
            let _ = this.update(cx, |page, cx| {
                page.visible = range;
                page.want_images(cx);
            });
        });
        Self {
            win: window.window_handle(),
            lowered: huds.iter().map(|h| h.name().to_lowercase()).collect(),
            huds: huds.into(),
            made: Rc::default(),
            copies: Rc::default(),
            updates: Rc::default(),
            status: None,
            loaded,
            busy: None,
            load_generation: 0,
            loads_in_flight: 0,
            last_failure: None,
            image_urls: Rc::default(),
            visible: 0..0,
            gallery_pending: false,
            search,
            grid,
            scroll: ScrollHandle::new(),
            image_viewer,
            _subscriptions: subscriptions,
        }
    }

    /// True once a load has finished, whether it succeeded or toasted.
    pub fn is_loaded(&self) -> bool {
        self.loaded
    }

    /// True when no library load is in flight.
    pub fn is_idle(&self) -> bool {
        self.loads_in_flight == 0
    }

    /// HUDs in the library as last loaded.
    pub fn hud_count(&self) -> usize {
        self.huds.len()
    }

    /// The HUD grid's scroll handle; its offset is what the grid drew.
    pub fn grid_scroll(&self) -> ScrollHandle {
        self.grid.scroll_handle().0.borrow().base_handle.clone()
    }

    /// The library search box.
    pub fn search_input(&self) -> &Entity<InputState> {
        &self.search
    }

    // Reads the library and the tf/custom scan in the background. Any error toast goes through
    // the stored window handle, so a page dropped mid-load is harmless.
    fn reload(&mut self, cx: &mut Context<Self>) {
        self.load_generation += 1;
        let generation = self.load_generation;
        self.loads_in_flight += 1;
        let tf2 = tf2_state(cx).read(cx).install().cloned();
        let tf2_for_fonts = tf2.clone();
        let loading = run_blocking(cx, move |s| {
            let failed = |e: LibraryError| describe_library(&e).into_op("Failed to load HUDs");
            let huds = s.library.list_huds().map_err(failed)?;
            let status = tf2
                .map(|t| s.library.hud_status(&t).map(|st| (t.custom_dir(), st)))
                .transpose()
                .map_err(failed)?;
            let urls = image_urls(s, &huds);
            // A HUD whose check fails offers no Edit; opening it would fail the same way.
            let made: HashSet<String> = huds
                .iter()
                .filter(|h| s.library.is_made_hud(h.id()).unwrap_or(false))
                .map(|h| h.folder_name().to_owned())
                .collect();
            // A copy record that cannot be read leaves a plain made HUD card, never a failure.
            let copies: HashMap<String, String> = (huds.iter())
                .filter(|h| made.contains(h.folder_name()))
                .filter_map(|h| {
                    let base = s.library.copy_base(h.id()).ok().flatten()?;
                    Some((h.folder_name().to_owned(), base.name().to_owned()))
                })
                .collect();
            Ok((huds, status, urls, made, copies))
        });
        let win = self.win;
        cx.spawn(async move |page, cx| {
            let loaded = loading.await;
            // False for a stale load or a repeated failure, which both stay silent.
            let report = page
                .update(cx, |page, cx| {
                    page.loads_in_flight -= 1;
                    if page.load_generation != generation {
                        return false;
                    }
                    // A current failure toasts only when it differs from the last one shown.
                    let fresh = match &loaded {
                        Ok((huds, status, urls, made, copies)) => {
                            page.lowered = huds.iter().map(|h| h.name().to_lowercase()).collect();
                            page.huds = huds.as_slice().into();
                            page.made = Rc::new(made.clone());
                            page.copies = Rc::new(copies.clone());
                            page.check_updates(generation, cx);
                            page.status = status.clone();
                            page.set_image_urls(urls.clone(), cx);
                            page.last_failure = None;
                            // The first sight of the one active HUD checks its fonts, once per
                            // folder per session; an install or profile apply has claimed its own.
                            let active = match status {
                                Some((_, HudStatus::Managed { folder_name, .. }))
                                | Some((_, HudStatus::Unmanaged { folder_name })) => {
                                    Some(folder_name.clone())
                                }
                                _ => None,
                            };
                            if let (Some(folder), Some(tf2)) = (active, tf2_for_fonts.clone())
                                && FontCheckSession::claim(cx, &folder)
                            {
                                ask_about_fonts(win, tf2, folder, cx);
                            }
                            false
                        }
                        Err(err) => {
                            let label = Some(error_label(err));
                            let fresh = page.last_failure != label;
                            page.last_failure = label;
                            fresh
                        }
                    };
                    page.loaded = true;
                    cx.notify();
                    loaded.is_ok() || fresh
                })
                .unwrap_or(false);
            let ok = loaded.is_ok();
            if report && ok {
                let _ =
                    cx.update(|cx| win.update(cx, |_, window, cx| timing::huds_loaded(window, cx)));
            }
            if let (true, Err(err)) = (report, loaded) {
                let _ =
                    cx.update(|cx| win.update(cx, |_, window, cx| toast_error(window, cx, &err)));
            }
        })
        .detach();
    }

    // Asks core whether each copy's HUD has a newer version, off the UI thread and after the
    // list landed, since hashing a HUD can be slow. A copy whose check fails shows nothing.
    fn check_updates(&mut self, generation: u64, cx: &mut Context<Self>) {
        let copies: Vec<(String, HudId)> = (self.huds.iter())
            .filter(|h| self.copies.contains_key(h.folder_name()))
            .map(|h| (h.folder_name().to_owned(), h.id().clone()))
            .collect();
        if copies.is_empty() {
            self.updates = Rc::default();
            return;
        }
        let checking = run_blocking(cx, move |s| {
            Ok((copies.into_iter())
                .filter_map(|(folder, id)| Some((folder, s.library.copy_update(&id).ok()?)))
                .collect::<HashMap<_, _>>())
        });
        cx.spawn(async move |page, cx| {
            let Ok(updates) = checking.await else { return };
            let _ = page.update(cx, |page, cx| {
                if page.load_generation == generation {
                    page.updates = Rc::new(updates);
                    cx.notify();
                }
            });
        })
        .detach();
    }

    // The preloaded library skips the load, so its screenshot URLs resolve on their own.
    fn resolve_image_urls(&mut self, cx: &mut Context<Self>) {
        let huds = self.huds.to_vec();
        let generation = self.load_generation;
        let resolving = run_blocking(cx, move |s| Ok(image_urls(s, &huds)));
        cx.spawn(async move |page, cx| {
            let Ok(urls) = resolving.await else { return };
            let _ = page.update(cx, |page, cx| {
                if page.load_generation == generation {
                    page.set_image_urls(urls, cx);
                }
            });
        })
        .detach();
    }

    fn set_image_urls(&mut self, urls: HashMap<String, String>, cx: &mut Context<Self>) {
        self.image_urls = Rc::new(urls);
        self.grid.rearm();
        self.want_images(cx);
    }

    // The HUDs the grid shows, as indices into `huds`: those whose name holds the search text.
    fn shown(&self, cx: &App) -> Vec<usize> {
        let query = self.search.read(cx).value().to_lowercase();
        (0..self.huds.len())
            .filter(|&ix| self.lowered[ix].contains(&query))
            .collect()
    }

    // Asks for the screenshots of the store cards in the grid's last drawn range.
    fn want_images(&mut self, cx: &mut Context<Self>) {
        let Some(images) = RemoteImages::global(cx) else {
            return;
        };
        let shown = self.shown(cx);
        let end = self.visible.end.min(shown.len());
        let keys: Vec<ImageKey> = shown[self.visible.start.min(end)..end]
            .iter()
            .filter_map(|&ix| self.image_urls.get(self.huds[ix].folder_name()))
            .map(|url| ImageKey::card(url.clone()))
            .collect();
        images.update(cx, |images, cx| images.want(&self.image_viewer, keys, cx));
    }

    // Marks `busy`, runs `op` in `run_blocking`, toasts its `Done` on success or `toast_error` on
    // failure through the window handle, then clears `busy` and reloads if the page still
    // exists. `after` runs in the same window update, right after a success toast.
    fn run(
        &mut self,
        busy: Busy,
        op: impl FnOnce(&Services) -> Result<Done, OpError> + Send + 'static,
        cx: &mut Context<Self>,
        after: Option<AfterRun>,
    ) {
        self.busy = Some(busy);
        cx.notify();
        let running = run_blocking(cx, op);
        let win = self.win;
        cx.spawn(async move |page, cx| finish_run(running, win, page, after, cx).await)
            .detach();
    }

    // `run` for an action that can remove HUDs the library cannot put back: it asks about them
    // first, and a cancel only clears `busy`. Nothing here needs the page, so an action survives
    // leaving it.
    fn run_guarded(
        &mut self,
        busy: Busy,
        (action, only, title, tf2): (HudAction, Option<String>, &'static str, Tf2Install),
        op: impl FnOnce(&Services, ExternalPlan) -> Result<Done, OpError> + Send + 'static,
        cx: &mut Context<Self>,
        after: Option<AfterRun>,
    ) {
        self.busy = Some(busy);
        cx.notify();
        let win = self.win;
        cx.spawn(async move |page, cx| {
            let Some(plan) = check_external(action, only, tf2, win, title, cx).await else {
                let _ = page.update(cx, |page, cx| {
                    page.busy = None;
                    cx.notify();
                });
                return;
            };
            let running = cx.update(|cx| run_blocking(cx, move |s| op(s, plan)));
            finish_run(running, win, page, after, cx).await;
        })
        .detach();
    }

    // Asks for a HUD archive and imports it; a cancelled pick clears `busy` and says nothing.
    // The picker needs the app, so only the import itself runs off the UI thread.
    // A pick that lands after the page was dropped is discarded without a toast.
    fn add(&mut self, cx: &mut Context<Self>) {
        let picking = cx
            .global::<Services>()
            .picker
            .pick(PickKind::HudArchive, cx);
        self.busy = Some(Busy::Import);
        cx.notify();
        cx.spawn(async move |page, cx| {
            let picked = picking.await;
            let _ = page.update(cx, |page, cx| match picked {
                Some(path) => page.run(
                    Busy::Import,
                    move |s| {
                        let hud = s
                            .library
                            .import_hud(&path, Provenance::Local)
                            .map_err(|e| describe_import(&e).into_op("Failed to import HUD"))?;
                        let message = format!("Imported HUD: {}", hud.name());
                        let entry = NewLogEntry::new(LogLevel::Info, "hud", "import", message)
                            .with_details(format!("Source: {}", path.display()));
                        activity::write(s, &entry);
                        let added = format!("{} added to your library", hud.name());
                        Ok((ToastKind::Success, "HUD imported".into(), added.into()))
                    },
                    cx,
                    None,
                ),
                None => {
                    page.busy = None;
                    cx.notify();
                }
            });
        })
        .detach();
    }

    fn install(&mut self, hud: &HudEntry, window: &mut Window, cx: &mut Context<Self>) {
        let tf2 = match require_tf2(cx) {
            Ok(tf2) => tf2,
            Err(err) => return toast_error(window, cx, &err),
        };
        let (id, name) = (hud.id().clone(), hud.name().to_owned());
        let busy = Busy::Install(hud.folder_name().to_owned());
        // The job fills `installed` with the tf/custom folder, which can differ from the
        // library folder. Only a success asks (and claims that folder ahead of the reload it
        // causes); a failed install leaves it to the page-load check.
        let installed = Arc::new(Mutex::new(None::<String>));
        let (tf2_for_fonts, installed_for_fonts) = (tf2.clone(), installed.clone());
        let ask: AfterRun = Box::new(move |window, cx| {
            let folder = installed_for_fonts.lock().ok().and_then(|mut f| f.take());
            if let Some(folder) = folder {
                ask_about_fonts(window.window_handle(), tf2_for_fonts, folder, cx)
            }
        });
        self.run_guarded(
            busy,
            (HudAction::Install, None, INSTALL_FAILED, tf2.clone()),
            move |s, plan| {
                let done = install_job(s, &id, &tf2, &plan)?;
                if let Ok(mut slot) = installed.lock() {
                    *slot = Some(done.folder().to_owned());
                }
                Ok(install_toast(&name, &done, &plan))
            },
            cx,
            Some(ask),
        );
    }

    // Clears every HUD from tf/custom, or just folder `only` in a conflict; the toast names the
    // HUD the page showed.
    fn uninstall(&mut self, only: Option<String>, window: &mut Window, cx: &mut Context<Self>) {
        let tf2 = match require_tf2(cx) {
            Ok(tf2) => tf2,
            Err(err) => return toast_error(window, cx, &err),
        };
        let name = match &only {
            Some(folder) => self.hud_name(folder).to_owned(),
            None => self.installed().map(|hud| hud.name).unwrap_or_default(),
        };
        let busy = only.clone().map_or(Busy::Uninstall, Busy::UninstallOne);
        let scope = only.clone();
        self.run_guarded(
            busy,
            (HudAction::Uninstall, scope, UNINSTALL_FAILED, tf2.clone()),
            move |s, plan| {
                let discard = plan.prepare(s, &tf2, UNINSTALL_FAILED)?;
                let removed = match &only {
                    Some(folder) => s.library.uninstall_one_hud(&tf2, folder, discard),
                    None => s.library.uninstall_hud_discarding(&tf2, discard),
                };
                removed.map_err(|e| describe_tf2_install(&e).into_op(UNINSTALL_FAILED))?;
                plan.log_discarded(s);
                let message = format!("Uninstalled HUD(s): {name}");
                activity::write(
                    s,
                    &NewLogEntry::new(LogLevel::Info, "hud", "uninstall", message),
                );
                let mut removed = match name.as_str() {
                    "" => "Your HUD has been removed from TF2".to_owned(),
                    name => format!("{name} has been removed from TF2"),
                };
                if let Some(note) = plan.note() {
                    removed = format!("{removed}. {note}");
                }
                Ok((ToastKind::Success, "HUD uninstalled".into(), removed.into()))
            },
            cx,
            None,
        );
    }

    // Asks first; only a confirmed answer deletes, and a dialog closed unanswered counts as no.
    // An answer that lands after the page was dropped deletes nothing.
    fn delete(&mut self, hud: &HudEntry, window: &mut Window, cx: &mut Context<Self>) {
        let answer = confirm(Confirm::delete_hud(hud.name()), window, cx);
        let (id, name) = (hud.id().clone(), hud.name().to_owned());
        let folder = hud.folder_name().to_owned();
        let busy = Busy::Delete(folder.clone());
        cx.spawn(async move |page, cx| {
            if !answer.recv().await.unwrap_or(false) {
                return;
            }
            let _ = page.update(cx, |page, cx| {
                page.run(
                    busy,
                    move |s| {
                        s.library
                            .delete_hud(&id)
                            .map_err(|e| describe_library(&e).into_op("Failed to delete HUD"))?;
                        let message = format!("Deleted HUD from library: {folder}");
                        let entry = NewLogEntry::new(LogLevel::Info, "hud", "delete", message);
                        activity::write(s, &entry);
                        let deleted = format!("{name} has been deleted from your library");
                        Ok((ToastKind::Success, "HUD removed".into(), deleted.into()))
                    },
                    cx,
                    None,
                );
            });
        })
        .detach();
    }
}

impl HudsPage {
    // Makes an editable copy of `hud` off the UI thread, since a HUD can be hundreds of MB,
    // then opens the copy in the HUD Maker. The job fills `made` with the copy's id for the
    // after-run hook, which runs only on success.
    fn copy(&mut self, hud: &HudEntry, _: &mut Window, cx: &mut Context<Self>) {
        let (id, author) = (hud.id().clone(), hud.author().to_owned());
        let busy = Busy::Copy(hud.folder_name().to_owned());
        let made = Arc::new(Mutex::new(None::<HudId>));
        let (page, made_for_open) = (cx.entity().downgrade(), made.clone());
        let open: AfterRun = Box::new(move |_, cx| {
            let copy = made_for_open.lock().ok().and_then(|mut m| m.take());
            if let Some(copy) = copy {
                let _ = page.update(cx, |_, cx| cx.emit(EditInHudMaker(copy)));
            }
        });
        self.run(
            busy,
            move |s| {
                let copy = s
                    .library
                    .copy_hud(&id)
                    .map_err(|e| describe_copy_hud(&e).into_op("Failed to copy HUD"))?;
                let message = format!("Copied HUD: {}", copy.name());
                activity::write(s, &NewLogEntry::new(LogLevel::Info, "hud", "copy", message));
                if let Ok(mut slot) = made.lock() {
                    *slot = Some(copy.id().clone());
                }
                let text = copy_made_text(copy.name(), &author);
                Ok((ToastKind::Success, "Copy made".into(), text.into()))
            },
            cx,
            Some(open),
        );
    }
}

impl HudsPage {
    // Updates copy `hud` to the newer version `from` in the library, then refreshes the
    // installed copy when it is the one in tf/custom. The toast names any changes left out.
    fn update_copy(
        &mut self,
        hud: &HudEntry,
        from: HudId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let tf2 = match require_tf2(cx) {
            Ok(tf2) => tf2,
            Err(err) => return toast_error(window, cx, &err),
        };
        let id = hud.id().clone();
        let busy = Busy::Update(hud.folder_name().to_owned());
        let platform = Platform::current().unwrap_or(Platform::Linux);
        self.run(
            busy,
            move |s| {
                let updated = s
                    .library
                    .update_copy(&id, &from, &tf2.tf_dir(), platform)
                    .map_err(|e| describe_update_copy(&e).into_op("Failed to update HUD"))?;
                let name = updated.entry().name();
                let message = format!("Updated HUD copy: {name}");
                activity::write(
                    s,
                    &NewLogEntry::new(LogLevel::Info, "hud", "update", message),
                );
                let left: Vec<(&str, Option<&str>)> = (updated.left_out().iter())
                    .map(|note| (note.part(), note.key()))
                    .collect();
                let notice = copy_updated_notice(name, &left, updated.both_changed().len());
                let (mut kind, mut text) = match notice.warning {
                    true => (ToastKind::Warning, notice.text),
                    false => (ToastKind::Success, notice.text),
                };
                if let Live::Failed(_) = live_job(s, &id, name, &tf2) {
                    kind = ToastKind::Warning;
                    text = format!("{text} {NOT_REINSTALLED}");
                }
                Ok((kind, notice.title.into(), text.into()))
            },
            cx,
            None,
        );
    }
}

/// A finished operation's toast: its kind, title and description.
type Done = (ToastKind, SharedString, SharedString);

/// Work `HudsPage::run` does in the window right after a success toast.
type AfterRun = Box<dyn FnOnce(&mut Window, &mut App)>;

/// What the installed card shows: the one HUD in tf/custom, read from the last scan.
struct Installed {
    name: String,
    managed: bool,
    path: String,
}

impl HudsPage {
    // Managed HUDs show their library name; a hand-placed folder shows its folder name.
    // Several HUDs at once is a conflict, which `conflict_card` lists instead.
    fn installed(&self) -> Option<Installed> {
        let (custom, status) = self.status.as_ref()?;
        let (folder, managed_id) = match status {
            HudStatus::Managed { folder_name, id } => (folder_name, Some(id)),
            HudStatus::Unmanaged { folder_name } => (folder_name, None),
            _ => return None,
        };
        let name = managed_id
            .and_then(|id| self.huds.iter().find(|h| h.id() == id))
            .map_or(folder.as_str(), HudEntry::name);
        Some(Installed {
            name: name.to_owned(),
            managed: managed_id.is_some(),
            path: custom.join(folder).display().to_string(),
        })
    }

    // The HUDs a conflict lists, as folder and name; `None` outside a conflict.
    fn conflicts(&self) -> Option<Vec<(String, String)>> {
        let Some((_, HudStatus::Multiple { folder_names })) = &self.status else {
            return None;
        };
        let named = |folder: &String| (folder.clone(), self.hud_name(folder).to_owned());
        Some(folder_names.iter().map(named).collect())
    }

    // The library's name for the HUD in `folder`, else the folder's own name.
    fn hud_name<'a>(&'a self, folder: &'a str) -> &'a str {
        (self.huds.iter())
            .find(|hud| hud.folder_name() == folder)
            .map_or(folder, HudEntry::name)
    }
}

// v5's EmptyState inside a Card: a monitor icon, the title and a muted description.
fn empty_card(cx: &App) -> AnyElement {
    let muted = cx.theme().muted_foreground;
    card_frame(cx)
        .id(ids::HUDS_INSTALLED)
        .test_support()
        .py_6()
        .child(
            v_flex()
                .items_center()
                .px_6()
                .py_8()
                .child(
                    Icon::new(IconName::Monitor)
                        .with_size(Size::Size(px(48.)))
                        .text_color(muted)
                        .mb_4(),
                )
                .child(labelled(
                    div().text_lg().font_weight(FontWeight::SEMIBOLD),
                    ids::HUDS_INSTALLED_EMPTY,
                    "No HUD installed",
                ))
                .child(
                    div()
                        .mt_1()
                        .max_w(px(384.))
                        .text_sm()
                        .text_center()
                        .text_color(muted)
                        .child(
                            "Install a HUD from your library to customize your in-game interface.",
                        ),
                ),
        )
        .into_any_element()
}

// v5's InstalledHud: the name and its badge beside Uninstall, then the folder path.
fn installed_card(hud: Installed, busy: Option<&Busy>, cx: &mut Context<HudsPage>) -> AnyElement {
    let theme = cx.theme();
    // Managed is shadcn's borderless secondary badge; External is its outline badge.
    let (badge, border, text) = if hud.managed {
        (Tag::secondary(), transparent_black(), "Managed")
    } else {
        let outline = Tag::secondary().outline().text_color(theme.foreground);
        (outline, theme.border, "External")
    };
    card_frame(cx)
        .id(ids::HUDS_INSTALLED)
        .test_support()
        .py_6()
        .child(
            v_flex()
                .gap_6()
                .child(
                    h_flex()
                        .px_6()
                        .justify_between()
                        .child(
                            h_flex()
                                .gap_2()
                                .child(labelled(
                                    div()
                                        .font_weight(FontWeight::SEMIBOLD)
                                        .line_height(relative(1.)),
                                    ids::HUDS_INSTALLED_NAME,
                                    hud.name,
                                ))
                                .child(badge_of(ids::HUDS_INSTALLED_BADGE, badge, border, text)),
                        )
                        .child(
                            Button::new(ids::HUDS_UNINSTALL)
                                .outline()
                                .px(SM_ICON_PX)
                                .icon(IconName::Unplug)
                                .label("Uninstall")
                                .loading(busy == Some(&Busy::Uninstall))
                                .disabled(busy.is_some_and(|b| *b != Busy::Uninstall))
                                .on_click(cx.listener(|page, _, window, cx| {
                                    page.uninstall(None, window, cx)
                                })),
                        ),
                )
                .child(labelled(
                    div().px_6().text_sm().text_color(theme.muted_foreground),
                    ids::HUDS_INSTALLED_PATH,
                    hud.path,
                )),
        )
        .into_any_element()
}

// The installed card during a conflict: every HUD found, each with its own Uninstall, so the
// player can get down to one. `huds` holds each one's folder and name.
fn conflict_card(
    huds: Vec<(String, String)>,
    busy: Option<&Busy>,
    cx: &mut Context<HudsPage>,
) -> AnyElement {
    let mut rows = v_flex().px_6().gap_4();
    for (folder, name) in huds {
        let removing = Busy::UninstallOne(folder.clone());
        let button = Button::new(ids::hud_uninstall(&folder))
            .outline()
            .px(SM_ICON_PX)
            .icon(IconName::Unplug)
            .label("Uninstall")
            .accessibility_label(format!("Uninstall {name}"))
            .loading(busy == Some(&removing))
            .disabled(busy.is_some_and(|b| *b != removing));
        let name = labelled(
            div().min_w_0().truncate().font_weight(FontWeight::SEMIBOLD),
            ids::hud_installed_name(&folder),
            name,
        );
        rows = rows.child(
            h_flex()
                .gap_4()
                .justify_between()
                .child(name)
                .child(button.on_click(cx.listener(move |page, _, window, cx| {
                    page.uninstall(Some(folder.clone()), window, cx)
                }))),
        );
    }
    card_frame(cx)
        .id(ids::HUDS_INSTALLED)
        .test_support()
        .py_6()
        .child(rows)
        .into_any_element()
}

// v5's orange-500 warning box; the card above it names the HUDs.
fn conflict_box(cx: &App) -> AnyElement {
    let orange: Hsla = rgb(ORANGE_500).into();
    let message =
        "TF2 can only use one HUD at a time. Uninstall the ones you don't want until one is left.";
    h_flex()
        .id(ids::HUDS_CONFLICTS)
        .role(Role::Alert)
        .aria_label(format!("HUD Conflicts Detected. {message}"))
        .test_support()
        .items_start()
        .gap_3()
        .p_4()
        .rounded(cx.theme().radius_lg)
        .border_1()
        .border_color(orange.opacity(0.5))
        .bg(orange.opacity(0.1))
        .child(
            Icon::new(IconName::TriangleAlert)
                .with_size(Size::Size(px(20.)))
                .text_color(orange)
                .mt_0p5()
                .flex_none(),
        )
        .child(
            v_flex()
                .child(
                    div()
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(orange)
                        .child("HUD Conflicts Detected"),
                )
                .child(
                    div()
                        .mt_1()
                        .text_sm()
                        .text_color(cx.theme().muted_foreground)
                        .child(message),
                ),
        )
        .into_any_element()
}

fn is_store(hud: &HudEntry) -> bool {
    !matches!(hud.provenance(), Provenance::Local)
}

// The cached store row a store HUD came from; a local HUD, a missing row or a failed read is None.
fn store_row(s: &Services, provenance: &Provenance) -> Option<StoreItem> {
    let (kind, id) = match provenance {
        Provenance::Curated { hud_id } => {
            (StoreKind::CuratedHuds, StoreItemId::Curated(hud_id.clone()))
        }
        Provenance::GameBanana { gb_id, .. } => {
            (StoreKind::GameBananaHuds, StoreItemId::GameBanana(*gb_id))
        }
        _ => return None,
    };
    s.store.store_item(kind, &id).ok().flatten()
}

// Each store HUD's card thumbnail from its cached store row, by folder. A HUD without a row
// or a URL is left out and keeps its placeholder.
fn image_urls(s: &Services, huds: &[HudEntry]) -> HashMap<String, String> {
    huds.iter()
        .filter_map(|hud| {
            let item = store_row(s, hud.provenance())?;
            let url = item.thumbnail_url()?.to_owned();
            Some((hud.folder_name().to_owned(), url))
        })
        .collect()
}

impl HudsPage {
    // Reads the HUD's gallery off the UI thread, then opens the lightbox over it; a row that is
    // gone gives an empty gallery. A click while a read is pending is ignored, so a double click
    // opens one lightbox. The flag clears however the read ends, so it can never stick.
    fn open_gallery(
        &mut self,
        title: SharedString,
        provenance: Provenance,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.gallery_pending {
            return;
        }
        self.gallery_pending = true;
        let gallery = run_blocking(cx, move |s| {
            Ok(store_row(s, &provenance)
                .map(|item| item.images().to_vec())
                .unwrap_or_default())
        });
        let win = window.window_handle();
        cx.spawn(async move |page, cx| {
            let urls = gallery.await.unwrap_or_default();
            let _ = cx.update(|cx| {
                win.update(cx, |_, window, cx| open_lightbox(title, urls, window, cx))
            });
            let _ = page.update(cx, |page, _| page.gallery_pending = false);
        })
        .detach();
    }
}

// A store card's 16:9 screenshot area: the image once ready, else the muted placeholder.
// A click opens the HUD's gallery in the lightbox.
fn screenshot(
    hud: &HudEntry,
    url: Option<&String>,
    page: &WeakEntity<HudsPage>,
    cx: &App,
) -> AnyElement {
    let folder = hud.folder_name();
    let title = SharedString::from(hud.name().to_owned());
    let provenance = hud.provenance().clone();
    let page = page.clone();
    let area = div()
        .id(ids::hud_shot(folder))
        .test_support()
        .cursor_pointer()
        .on_click(move |_, window, cx| {
            let _ = page.update(cx, |page, cx| {
                page.open_gallery(title.clone(), provenance.clone(), window, cx)
            });
        })
        .flex_none()
        .w_full()
        .aspect_ratio(16. / 9.)
        .overflow_hidden()
        .bg(cx.theme().muted);
    let ready = url
        .zip(RemoteImages::global(cx))
        .map(|(url, images)| images.read(cx).image(&ImageKey::card(url.clone())));
    match ready {
        Some(RemoteImage::Ready(image)) => area
            .child(
                img(image)
                    .size_full()
                    .object_fit(ObjectFit::Cover)
                    .id(ids::hud_image(folder))
                    .test_support(),
            )
            .into_any_element(),
        _ => area.into_any_element(),
    }
}

// v5's HudCard: name, author and source badge, then delete and Install bottom right.
// Store HUDs lead with their 16:9 screenshot area.
fn hud_card(
    hud: &HudEntry,
    url: Option<&String>,
    made: bool,
    copy: Option<(&String, Option<&CopyUpdate>)>,
    busy: Option<&Busy>,
    page: &WeakEntity<HudsPage>,
    cx: &App,
) -> AnyElement {
    let theme = cx.theme();
    let folder = hud.folder_name();
    let installing = busy == Some(&Busy::Install(folder.to_owned()));
    let deleting = busy == Some(&Busy::Delete(folder.to_owned()));
    let copying = busy == Some(&Busy::Copy(folder.to_owned()));
    let updating = busy == Some(&Busy::Update(folder.to_owned()));
    let update = copy.and_then(|(_, update)| update);
    let store_note = matches!(update, Some(CopyUpdate::InStore)).then(|| {
        labelled(
            div()
                .text_xs()
                .line_clamp(3)
                .text_color(theme.muted_foreground),
            ids::hud_update_note(folder),
            copy_in_store_text(copy.map_or("", |(base, _)| base.as_str())),
        )
    });
    let update_button = match update {
        Some(CopyUpdate::InLibrary { from, .. }) => {
            let (page, entry, from) = (page.clone(), hud.clone(), from.clone());
            Some(
                Button::new(ids::hud_update(folder))
                    .ghost()
                    .icon(IconName::RefreshCw)
                    .tooltip("Update")
                    .accessibility_label("Update")
                    .loading(updating)
                    .disabled(busy.is_some() && !updating)
                    .on_click(move |_, window, cx| {
                        let _ = page.update(cx, |page, cx| {
                            page.update_copy(&entry, from.clone(), window, cx)
                        });
                    }),
            )
        }
        _ => None,
    };
    // Both card buttons call back into the page through a weak handle and a clone of the entry.
    let on_page = |act: fn(&mut HudsPage, &HudEntry, &mut Window, &mut Context<HudsPage>)| {
        let (page, entry) = (page.clone(), hud.clone());
        move |_: &ClickEvent, window: &mut Window, cx: &mut App| {
            let _ = page.update(cx, |page, cx| act(page, &entry, window, cx));
        }
    };
    let (badge, text) = if is_store(hud) {
        (Tag::primary(), "Store")
    } else {
        (Tag::secondary(), "Local")
    };
    // A copy names the HUD it came from in place of its author.
    let byline = div()
        .text_sm()
        .truncate()
        .text_color(theme.muted_foreground);
    let author = match copy {
        Some((base, _)) => Some(labelled(
            byline,
            ids::hud_copy_of(folder),
            format!("Copy of {base}"),
        )),
        None => (!hud.author().is_empty()).then(|| {
            labelled(
                byline,
                ids::hud_author(folder),
                format!("by {}", hud.author()),
            )
        }),
    };
    card_frame(cx)
        .id(ids::hud_card(folder))
        .accessibility_id(ids::hud_card(folder).to_string())
        .aria_label(hud.name().to_owned())
        .test_support()
        .size_full()
        .flex()
        .flex_col()
        .overflow_hidden()
        .children(is_store(hud).then(|| screenshot(hud, url, page, cx)))
        .child(
            h_flex()
                .px_6()
                .pt_6()
                .gap_2()
                .justify_between()
                .child(
                    v_flex()
                        .min_w_0()
                        .child(
                            div()
                                .truncate()
                                .font_weight(FontWeight::SEMIBOLD)
                                .child(hud.name().to_owned()),
                        )
                        .children(author)
                        .children(store_note),
                )
                .child(badge_of(
                    ids::hud_badge(folder),
                    badge,
                    transparent_black(),
                    text,
                )),
        )
        .child(
            h_flex()
                .mt_auto()
                .px_6()
                .pb_6()
                .gap_2()
                .justify_end()
                .children(made.then(|| {
                    let (page, id) = (page.clone(), hud.id().clone());
                    // Opening a copy mid-update would edit files about to be swapped out.
                    let tip = match busy {
                        Some(_) => EDIT_WAIT,
                        None => "Edit in HUD Maker",
                    };
                    Button::new(ids::hud_edit(folder))
                        .ghost()
                        .icon(IconName::Pencil)
                        .tooltip(tip)
                        .accessibility_label("Edit in HUD Maker")
                        .disabled(busy.is_some())
                        .on_click(move |_, _, cx| {
                            let _ = page.update(cx, |_, cx| cx.emit(EditInHudMaker(id.clone())));
                        })
                }))
                .children(update_button)
                .children((!made).then(|| {
                    Button::new(ids::hud_copy(folder))
                        .ghost()
                        .icon(IconName::Copy)
                        .tooltip("Edit a copy")
                        .accessibility_label("Edit a copy")
                        .loading(copying)
                        .disabled(busy.is_some() && !copying)
                        .on_click(on_page(HudsPage::copy))
                }))
                .child(
                    Button::new(ids::hud_delete(folder))
                        .ghost()
                        .icon(IconName::Trash)
                        .loading(deleting)
                        .disabled(busy.is_some() && !deleting)
                        .on_click(on_page(HudsPage::delete)),
                )
                .child(
                    Button::new(ids::hud_install(folder))
                        .primary()
                        .px(SM_ICON_PX)
                        .icon(IconName::Download)
                        .label("Install")
                        .loading(installing)
                        .disabled(busy.is_some() && !installing)
                        .on_click(on_page(HudsPage::install)),
                ),
        )
        .into_any_element()
}

// v5's library EmptyState: an empty library, or a search that matched nothing.
fn empty_library(searching: bool, cx: &App) -> AnyElement {
    let (id, title, description) = if searching {
        (
            ids::HUDS_NO_MATCH,
            "No HUDs found",
            "Try a different search term.",
        )
    } else {
        (
            ids::HUDS_EMPTY,
            "Your library is empty",
            "Add a HUD from a ZIP file or browse the store to download one.",
        )
    };
    let muted = cx.theme().muted_foreground;
    card_frame(cx)
        .py_6()
        .child(
            v_flex().items_center().px_6().py_8().child(
                v_flex()
                    .items_center()
                    .py_12()
                    .child(
                        Icon::new(IconName::PackageOpen)
                            .with_size(Size::Size(px(48.)))
                            .text_color(muted)
                            .mb_4(),
                    )
                    .child(labelled(
                        div().text_lg().font_weight(FontWeight::SEMIBOLD),
                        id,
                        title,
                    ))
                    .child(
                        div()
                            .mt_1()
                            .max_w(px(384.))
                            .text_sm()
                            .text_center()
                            .text_color(muted)
                            .child(description),
                    ),
            ),
        )
        .into_any_element()
}

impl HudsPage {
    // The "Your Library" row, then the grid of HUDs whose name holds the search text.
    // The filter is read from the input on every render, so a reload keeps it.
    fn library(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let query = self.search.read(cx).value().to_lowercase();
        let shown = self.shown(cx);
        let body = if !self.loaded {
            None
        } else if shown.is_empty() {
            Some(empty_library(!query.is_empty(), cx))
        } else {
            // One store card makes every card tall enough for its 16:9 screenshot.
            let any_store = shown.iter().any(|&ix| is_store(&self.huds[ix]));
            let any_note = (shown.iter()).any(|&ix| {
                let folder = self.huds[ix].folder_name();
                matches!(self.updates.get(folder), Some(CopyUpdate::InStore))
            });
            let height = move |width: Pixels| {
                let image = if any_store {
                    width * (9. / 16.)
                } else {
                    px(0.)
                };
                let note = if any_note { STORE_NOTE_HEIGHT } else { px(0.) };
                image + CARD_TEXT_HEIGHT + note
            };
            let (busy, page) = (self.busy.clone(), cx.entity().downgrade());
            let (huds, urls, made, copies, updates) = (
                self.huds.clone(),
                self.image_urls.clone(),
                self.made.clone(),
                self.copies.clone(),
                self.updates.clone(),
            );
            Some(
                self.grid
                    .render_sized(shown.len(), height, move |ix, _, cx| {
                        let hud = &huds[shown[ix]];
                        let url = urls.get(hud.folder_name());
                        let made = made.contains(hud.folder_name());
                        let copy = (copies.get(hud.folder_name()))
                            .map(|base| (base, updates.get(hud.folder_name())));
                        hud_card(hud, url, made, copy, busy.as_ref(), &page, cx)
                    }),
            )
        };
        let muted = cx.theme().muted_foreground;
        let importing = self.busy == Some(Busy::Import);
        // The search box gives up width before Add HUD, so a narrow page keeps its margin.
        v_flex()
            .flex_1()
            .gap_4()
            .child(
                h_flex()
                    .justify_between()
                    .gap_4()
                    .child(
                        div()
                            .flex_none()
                            .text_lg()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child("Your Library"),
                    )
                    .child(
                        h_flex()
                            .flex_1()
                            .min_w_0()
                            .justify_end()
                            .gap_3()
                            .child(
                                div()
                                    .id(ids::HUDS_SEARCH)
                                    .test_support()
                                    .flex_1()
                                    .min_w_0()
                                    .max_w(px(256.))
                                    .child(
                                        Input::new(&self.search)
                                            .h_9()
                                            .prefix(Icon::new(IconName::Search).text_color(muted)),
                                    ),
                            )
                            .child(
                                Button::new(ids::HUDS_ADD)
                                    .flex_none()
                                    .primary()
                                    .h_9()
                                    .px_3()
                                    .icon(IconName::Plus)
                                    .label(if importing { "Importing..." } else { "Add HUD" })
                                    .loading(importing)
                                    .disabled(self.busy.is_some() && !importing)
                                    .on_click(cx.listener(|page, _, _, cx| page.add(cx))),
                            ),
                    ),
            )
            .child(
                div()
                    .id(ids::HUDS_LIBRARY)
                    .test_support()
                    .flex_1()
                    .min_h(LIBRARY_MIN_HEIGHT)
                    .children(body),
            )
    }
}

impl Render for HudsPage {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let conflicts = self.conflicts();
        let conflicted = conflicts.is_some();
        let installed = match (conflicts, self.installed()) {
            (Some(huds), _) => conflict_card(huds, self.busy.as_ref(), cx),
            (None, Some(hud)) => installed_card(hud, self.busy.as_ref(), cx),
            (None, None) => empty_card(cx),
        };
        let content =
            v_flex()
                .id(ids::page(Page::Huds))
                .test_support()
                .size_full()
                .track_scroll(&self.scroll)
                .overflow_y_scroll()
                .p(CONTENT_PADDING)
                .gap_6()
                .child(
                    v_flex()
                        .flex_none()
                        .gap_3()
                        .child(div().text_lg().font_weight(FontWeight::SEMIBOLD).child(
                            if conflicted {
                                "Installed HUDs"
                            } else {
                                "Installed HUD"
                            },
                        ))
                        .child(installed),
                )
                .children(conflicted.then(|| conflict_box(cx)))
                .child(self.library(cx));
        scroll_area(ids::page_scrollbar(Page::Huds), &self.scroll, content).size_full()
    }
}

// Toasts a finished job through the window handle, then clears `busy` and reloads if the page
// still exists. `after` runs in the same window update, right after a success toast.
async fn finish_run(
    running: Task<Result<Done, OpError>>,
    win: AnyWindowHandle,
    page: WeakEntity<HudsPage>,
    after: Option<AfterRun>,
    cx: &mut AsyncApp,
) {
    let done = running.await;
    let ok = done.is_ok();
    cx.update(|cx| {
        let _ = win.update(cx, |_, window, cx| match done {
            Ok((kind, title, description)) => {
                toast(window, cx, kind, title, description);
                if let Some(after) = after {
                    after(window, cx);
                }
            }
            Err(err) => toast_error(window, cx, &err),
        });
        // Outside the page, so a success is announced even when the page is gone; a
        // surviving page reloads through its own LibraryChanged subscription.
        if ok {
            LibraryEvents::changed(cx);
        }
    });
    let _ = page.update(cx, |page, cx| {
        page.busy = None;
        cx.notify();
        if !ok {
            page.reload(cx)
        }
    });
}

/// The toast for a finished install of the HUD called `name`: a warning when the player's
/// own folder held the name the HUD expects, since parts of it may then not show.
pub(crate) fn install_toast(name: &str, done: &HudInstall, plan: &ExternalPlan) -> Done {
    let note = plan.note();
    match done.blocked_name() {
        None => {
            let active = format!("{name} is now active");
            let description = match note {
                Some(note) => format!("{active}. {note}"),
                None => active,
            };
            (
                ToastKind::Success,
                "HUD installed".into(),
                description.into(),
            )
        }
        Some(blocked) => {
            let description = blocked_description(blocked);
            (
                ToastKind::Warning,
                format!("{name} installed").into(),
                match note {
                    Some(note) => format!("{description} {note}").into(),
                    None => description.into(),
                },
            )
        }
    }
}

/// The warning text for a HUD that went under its library folder because the player's own
/// folder or file held `blocked`, the name it expects.
pub(crate) fn blocked_description(blocked: &str) -> String {
    format!(
        "A folder named {blocked} is already in your custom folder, so parts of this HUD may \
         not show. Move or rename that folder and install again."
    )
}

/// Carries out `plan`, installs library HUD `id` into `tf2`, writes the activity row naming the
/// `tf/custom` folder it landed in and returns where it went; the HUDs page and the HUD Maker
/// share it.
pub(crate) fn install_job(
    s: &Services,
    id: &HudId,
    tf2: &Tf2Install,
    plan: &ExternalPlan,
) -> Result<HudInstall, OpError> {
    let discard = plan.prepare(s, tf2, INSTALL_FAILED)?;
    let installed = s
        .library
        .install_hud_discarding(id, tf2, discard)
        .map_err(|e| describe_tf2_install(&e).into_op(INSTALL_FAILED))?;
    plan.log_discarded(s);
    let folder = installed.folder();
    let target = tf2.custom_dir().join(folder);
    let entry = NewLogEntry::new(
        LogLevel::Info,
        "hud",
        "install",
        format!("Installed HUD: {folder}"),
    )
    .with_details(format!("Installed to: {}", target.display()));
    activity::write(s, &entry);
    Ok(installed)
}
