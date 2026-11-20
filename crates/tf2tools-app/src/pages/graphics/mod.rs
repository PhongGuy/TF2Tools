//! The Graphics page: mastercomfig's status, the graphics preset picker and the settings list.

pub mod editor;
pub mod jobs;
pub mod model;
mod picture_panel;
pub mod pictures;
mod pill;
mod presets;
mod rows;
mod settings;
mod status;
pub mod view;

use std::collections::BTreeMap;
use std::sync::Arc;

use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::component::slider::SliderState;
use gpui_kit::component::v_flex;
use gpui_kit::{
    AnyElement, AnyWindowHandle, App, AppContext as _, Context, Entity, InteractiveElement as _,
    IntoElement, ParentElement as _, Pixels, Render, ScrollHandle, SharedString,
    StatefulInteractiveElement as _, Styled as _, Subscription, TestSupportExt as _, Window, px,
};
use tf2tools_core::{Choices, MastercomfigRelease, MastercomfigStatus, ModuleCatalog, Preset};

use crate::i18n::tr;
use crate::ids::{self, IdScope};
use crate::local_messages::describe_store_status;
use crate::pages::common::no_tf2_warning;
use crate::pages::profiles::probe::Tf2Probe;
use crate::remote_images::ImageKey;
use crate::services::{OpError, Services, Tf2State, require_tf2, run_blocking, tf2_state};
use crate::shell::CONTENT_PADDING;
use crate::shell::confirm::{Confirm, confirm};
use crate::shell::route::Page;
use crate::shell::toast::{ToastKind, toast, toast_error};
use crate::widgets::card_grid::SHELL_CHROME_WIDTH;
use crate::widgets::scroll::scroll_area;
use editor::ValueSelect;
use jobs::{ModulesFailure, save_failed};
use model::{
    SettingsNote, StatusCard, describe_apply, describe_remove, describe_take_over, dropped_note,
    foreign_modules_cfg, own_modules_cfg_kept, preset_label, status_card,
};
use pictures::PictureIndex;
use view::Filter;

pub use editor::{ChoicesEdit, GraphicsChoicesEditor};

/// A finished action's success toast: (title, description).
type Done = (SharedString, SharedString);

/// A finished action's toast, if it has one: (kind, title, description).
type Outcome = Option<(ToastKind, SharedString, SharedString)>;

/// What one load read off the UI thread.
struct Loaded {
    status: Option<MastercomfigStatus>,
    release: Option<MastercomfigRelease>,
    choices: Option<Choices>,
    /// The cached catalog of a managed version the page did not hold yet.
    catalog: Option<ModuleCatalog>,
}

/// The download job whose button spins; other actions only lock the buttons.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Job {
    Install,
    Check,
}

/// The Graphics page. Built fresh on each visit; it reloads when TF2's state changes.
pub struct GraphicsPage {
    win: AnyWindowHandle,
    /// mastercomfig in `tf/custom`, from the last load that knew TF2.
    status: Option<MastercomfigStatus>,
    /// The release the last addon-store sync recorded.
    release: Option<MastercomfigRelease>,
    loaded: bool,
    /// An install, update, take-over, removal or preset pick is running; every action rests.
    busy: bool,
    /// An action has ended and its reload is out; that reload's landing lifts `busy`.
    settling: bool,
    /// The job whose button spins, held as long as `busy`.
    running: Option<Job>,
    /// Bumped by each reload, so a slower, older load never overwrites a newer one.
    load_generation: u64,
    /// The saved choices of a managed install, from the last load.
    choices: Option<Choices>,
    /// The settings list's catalog, always for the managed install's version.
    catalog: Option<Arc<ModuleCatalog>>,
    /// The editor's Selects, mirrored on each of its notifies.
    selects: BTreeMap<String, ValueSelect>,
    /// Why there is no catalog yet; None before a fetch is asked.
    note: Option<SettingsNote>,
    /// The version whose catalog this visit already asked GitHub for.
    asked: Option<String>,
    /// The settings search; rows filter on its value at render.
    search: Entity<InputState>,
    /// "Changed only" is on: the list shows only rows that differ from the preset.
    changed_only: bool,
    /// Each shown section's child index in the scroll column, from the last render.
    section_items: BTreeMap<String, usize>,
    /// The search toolbar's child index in the scroll column, from the last render.
    toolbar_item: Option<usize>,
    scroll: ScrollHandle,
    /// The bundled setting pictures, once read off the UI thread.
    pictures: Option<Arc<PictureIndex>>,
    /// The editor's image wants, copied after each frame asks for them.
    wanted: Vec<ImageKey>,
    /// The editor's open row and zoom, mirrored on each of its notifies.
    open_row: Option<String>,
    zoom: bool,
    /// The editor's wipe slider, held from construction.
    wipe: Entity<SliderState>,
    /// The shell's remembered answer to whether TF2 runs; drawing never probes processes.
    tf2_running: bool,
    /// The presets card, changes strip, settings rows and their pictures, fed the shown choices
    /// and proposing edits.
    editor: Entity<GraphicsChoicesEditor>,
    _subscriptions: Vec<Subscription>,
}

/// Below this page width the category rail becomes a chip row above the settings.
const NARROW: f32 = 900.;

impl GraphicsPage {
    /// Needs the `Services` and `Tf2Global` globals. Loads the status and release off the UI thread.
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let tf2 = cx.observe(&tf2_state(cx), |page, _, cx| page.reload(cx));
        let search =
            cx.new(|cx| InputState::new(window, cx).placeholder(tr!("graphics.settings.search")));
        // The list filters from the input's value at render, so each edit just redraws.
        let searched = cx.subscribe(&search, |_, _, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Change) {
                cx.notify();
            }
        });
        let editor = cx.new(|cx| {
            GraphicsChoicesEditor::new(IdScope::PAGE, Choices::new(Preset::Medium), window, cx)
        });
        // The editor only proposes; a preset runs the pick and any other edit is saved.
        let edits = cx.subscribe_in(
            &editor,
            window,
            |page, _, edit: &ChoicesEdit, window, cx| match edit {
                ChoicesEdit::Preset(preset) => page.pick(*preset, window, cx),
                ChoicesEdit::Modules { to, .. } => page.change_to(to.clone(), cx),
            },
        );
        // The page hands out the editor's live Selects and pictures state, so it copies them
        // on every change.
        let mirrored = cx.observe(&editor, |page, editor, cx| {
            let editor = editor.read(cx);
            page.selects = editor.selects.clone();
            page.open_row = editor.open_row().map(str::to_owned);
            page.zoom = editor.zoom();
            cx.notify();
        });
        let wipe = editor.read(cx).wipe_state().clone();
        let subscriptions = vec![tf2, searched, edits, mirrored];
        let mut page = Self {
            win: window.window_handle(),
            status: None,
            release: None,
            loaded: false,
            busy: false,
            settling: false,
            running: None,
            load_generation: 0,
            choices: None,
            catalog: None,
            selects: BTreeMap::new(),
            note: None,
            asked: None,
            search,
            changed_only: false,
            section_items: BTreeMap::new(),
            toolbar_item: None,
            scroll: ScrollHandle::new(),
            pictures: None,
            wanted: Vec::new(),
            open_row: None,
            zoom: false,
            wipe,
            tf2_running: false,
            editor,
            _subscriptions: subscriptions,
        };
        page.reload(cx);
        page.load_pictures(cx);
        page
    }

    /// Takes the shell's answer to whether TF2 runs; redraws only when it changed.
    pub fn set_tf2_running(&mut self, running: bool, cx: &mut Context<Self>) {
        if self.tf2_running != running {
            self.tf2_running = running;
            cx.notify();
        }
    }

    /// Whether TF2 ran when the shell last asked; the running banner shows while it did.
    pub fn tf2_running(&self) -> bool {
        self.tf2_running
    }

    /// The bundled setting pictures once read; an empty index when the folder cannot be read.
    pub fn pictures(&self) -> Option<&PictureIndex> {
        self.pictures.as_deref()
    }

    /// The image keys the last frame asked the image cache for.
    pub fn wanted_images(&self) -> &[ImageKey] {
        &self.wanted
    }

    /// The page's own editor, for tests and for nothing else.
    pub fn choices_editor(&self) -> &Entity<GraphicsChoicesEditor> {
        &self.editor
    }

    /// The settings search box's state.
    pub fn search_input(&self) -> &Entity<InputState> {
        &self.search
    }

    /// Whether "Changed only" is on.
    pub fn changed_only(&self) -> bool {
        self.changed_only
    }

    // What the list shows: the search's value and "Changed only".
    fn filter(&self, cx: &App) -> Filter {
        Filter {
            query: self.search.read(cx).value().to_string(),
            changed_only: self.changed_only,
        }
    }

    /// The module whose pictures are open, if any.
    pub fn open_row(&self) -> Option<&str> {
        self.open_row.as_deref()
    }

    /// The compare view's wipe slider.
    pub fn wipe_state(&self) -> &Entity<SliderState> {
        &self.wipe
    }

    /// How far across the compare view the preset's picture shows, 0 to 100.
    pub fn wipe(&self, cx: &App) -> f32 {
        self.wipe.read(cx).value().start()
    }

    /// Whether the compare view shows the zoomed crops.
    pub fn zoom(&self) -> bool {
        self.zoom
    }

    fn set_changed_only(&mut self, on: bool, cx: &mut Context<Self>) {
        self.changed_only = on;
        cx.notify();
    }

    // Clears the search and "Changed only", so every row shows again.
    fn show_all(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.search
            .update(cx, |search, cx| search.set_value("", window, cx));
        self.set_changed_only(false, cx);
    }

    // Scrolls the category `key`'s section to the top of the page, if it is drawn.
    fn jump(&mut self, key: &str, cx: &mut Context<Self>) {
        if let Some(&item) = self.section_items.get(key) {
            self.scroll.scroll_to_top_of_item(item);
            cx.notify();
        }
    }

    // How far below the toolbar's top the wide rail sits, so it pins at the page's top padding
    // once the toolbar scrolls above it. The toolbar's bounds come from the last frame and are
    // unscrolled, so the current offset is added; zero until a frame has laid it out.
    fn rail_pin(&self) -> Pixels {
        let Some(toolbar) = self
            .toolbar_item
            .and_then(|ix| self.scroll.bounds_for_item(ix))
        else {
            return px(0.);
        };
        let top = toolbar.top() + self.scroll.offset().y;
        (self.scroll.bounds().top() + CONTENT_PADDING - top).max(px(0.))
    }

    /// True once a load has ended, failed or not, with or without TF2.
    pub fn is_loaded(&self) -> bool {
        self.loaded
    }

    /// mastercomfig's status from the last load; None without TF2 or before a load.
    pub fn status(&self) -> Option<&MastercomfigStatus> {
        self.status.as_ref()
    }

    /// The recorded mastercomfig release from the last load, if a sync recorded one.
    pub fn release(&self) -> Option<&MastercomfigRelease> {
        self.release.as_ref()
    }

    /// True while an action, or the reload after it, is running; every action button rests.
    pub fn is_busy(&self) -> bool {
        self.busy
    }

    /// True while an install or update runs, until its reload lands.
    pub fn is_installing(&self) -> bool {
        self.running == Some(Job::Install)
    }

    /// True while a check for updates runs, until its reload lands.
    pub fn is_checking(&self) -> bool {
        self.running == Some(Job::Check)
    }

    /// The status card from the last load; None without TF2 or before a load.
    pub fn card(&self) -> Option<StatusCard> {
        let latest = self.release.as_ref().map(MastercomfigRelease::tag);
        self.status
            .as_ref()
            .map(|status| status_card(status, latest))
    }

    /// The chosen preset of a TF2Tools-managed install; None for any other state.
    pub fn preset(&self) -> Option<Preset> {
        match self.status {
            Some(MastercomfigStatus::Managed { preset, .. }) => Some(preset),
            _ => None,
        }
    }

    /// The catalog of the managed install's version, once loaded.
    pub fn catalog(&self) -> Option<&ModuleCatalog> {
        self.catalog.as_deref()
    }

    /// The saved choices of a managed install.
    pub fn choices(&self) -> Option<&Choices> {
        self.choices.as_ref()
    }

    /// What shows in place of the list while there is no catalog; None with a catalog or unmanaged.
    pub fn settings_note(&self) -> Option<SettingsNote> {
        if self.catalog.is_some() || self.managed_version().is_none() {
            return None;
        }
        Some(self.note.unwrap_or(SettingsNote::Loading))
    }

    /// The Select state of `module`'s row.
    pub fn module_select(&self, module: &str) -> Option<ValueSelect> {
        self.selects.get(module).cloned()
    }

    /// `module`'s row as the list draws it; None without a catalog or for an unknown module.
    pub fn row(&self, module: &str) -> Option<view::RowView> {
        let catalog = self.catalog.as_ref()?;
        let module = catalog.module(module)?;
        Some(view::row(catalog, &self.shown_against(), module))
    }

    fn managed_version(&self) -> Option<&str> {
        match &self.status {
            Some(MastercomfigStatus::Managed { version, .. }) => Some(version),
            _ => None,
        }
    }

    // Downloads the release into tf/custom through `jobs::install_base`; Update is the same
    // download over an older base, and core keeps the saved preset either way.
    fn install(&mut self, update: bool, window: &mut Window, cx: &mut Context<Self>) {
        let tf2 = match require_tf2(cx) {
            Ok(tf2) => tf2,
            Err(err) => return toast_error(window, cx, &err),
        };
        let action = if update {
            tr!("messages.action.update_mastercomfig")
        } else {
            tr!("messages.action.install_mastercomfig")
        };
        let job = jobs::install_base(tf2, action, cx);
        self.running = Some(Job::Install);
        self.finish(
            async move {
                let jobs::Installed {
                    version,
                    dropped,
                    own_file_kept,
                } = job.await?;
                let (title, mut text) = if update {
                    let now = tr!("graphics.toast.updated", version = version);
                    (tr!("graphics.toast.updated_title"), now)
                } else {
                    let ready = tr!("graphics.toast.installed", version = version);
                    (tr!("graphics.toast.installed_title"), ready)
                };
                if let Some(note) = dropped_note(&dropped) {
                    text = format!("{text} {note}").into();
                }
                if own_file_kept {
                    text = format!("{text} {}", own_modules_cfg_kept()).into();
                }
                Ok((title, text))
            },
            cx,
        );
    }

    // Forces the addon-store sync; the reloaded status row then says whether an update is
    // offered, so the toast names no version.
    fn check(&mut self, cx: &mut Context<Self>) {
        let job = jobs::check_for_updates(cx);
        self.running = Some(Job::Check);
        self.finish(
            async move {
                job.await?;
                Ok((tr!("graphics.toast.checked_title"), SharedString::default()))
            },
            cx,
        );
    }

    fn take_over(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let tf2 = match require_tf2(cx) {
            Ok(tf2) => tf2,
            Err(err) => return toast_error(window, cx, &err),
        };
        self.run(
            move |s| {
                s.library.take_over_mastercomfig(&tf2).map_err(|e| {
                    describe_take_over(&e).into_op(tr!("messages.action.take_over_mastercomfig"))
                })?;
                let text = tr!("graphics.toast.taken_over");
                Ok((tr!("graphics.toast.taken_over_title"), text))
            },
            cx,
        );
    }

    // Asks first; only a confirmed answer removes, and the TF2 folder is read once answered.
    fn remove(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let answer = confirm(Confirm::remove_mastercomfig(), window, cx);
        let win = self.win;
        cx.spawn(async move |page, cx| {
            if !answer.recv().await.unwrap_or(false) {
                return;
            }
            let _ = page.update(cx, |page, cx| {
                let tf2 = match require_tf2(cx) {
                    Ok(tf2) => tf2,
                    Err(err) => {
                        let _ = win.update(cx, |_, window, cx| toast_error(window, cx, &err));
                        return;
                    }
                };
                let probe = Tf2Probe::get(cx);
                page.run(
                    move |s| {
                        jobs::refuse_while_running(&*probe)?;
                        let restored = s.library.remove_mastercomfig(&tf2).map_err(|e| {
                            describe_remove(&e).into_op(tr!("messages.action.remove_mastercomfig"))
                        })?;
                        let mut text = tr!("graphics.toast.removed").to_string();
                        if restored {
                            text.push(' ');
                            text.push_str(&crate::pages::addons::model::movement_off());
                        }
                        Ok((tr!("graphics.toast.removed_title"), text.into()))
                    },
                    cx,
                );
            });
        })
        .detach();
    }

    // Writes the picked preset at once; picking the current one again does nothing.
    fn pick(&mut self, preset: Preset, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy || self.preset() == Some(preset) {
            return;
        }
        let tf2 = match require_tf2(cx) {
            Ok(tf2) => tf2,
            Err(err) => return toast_error(window, cx, &err),
        };
        self.run(
            move |s| {
                s.library
                    .apply_preset(&tf2, preset)
                    .map_err(|e| describe_apply(&e).into_op(tr!("messages.action.apply_preset")))?;
                let text = tr!("graphics.toast.preset", preset = preset_label(preset));
                Ok((tr!("graphics.toast.preset_title"), text))
            },
            cx,
        );
    }

    // Saves `new` in place of the shown choices; choices equal to them do nothing. Only a
    // change to the override VPKs waits for TF2 to close. A player's own modules.cfg is
    // replaced only after they agree; the page stays busy while asking.
    fn change_to(&mut self, new: Choices, cx: &mut Context<Self>) {
        let Some(catalog) = self.catalog.clone() else {
            return;
        };
        if self.busy || self.managed_version().is_none() {
            return;
        }
        let old = self.shown_against();
        if new == old {
            return;
        }
        let tf2 = match require_tf2(cx) {
            Ok(tf2) => tf2,
            Err(err) => return self.settle(async move { Err(err) }, cx),
        };
        let guard_tf2 = old.override_mask() != new.override_mask();
        let job = jobs::apply_modules(
            tf2.clone(),
            Arc::clone(&catalog),
            new.clone(),
            guard_tf2,
            false,
            cx,
        );
        let win = self.win;
        let mut acx = cx.to_async();
        self.settle(
            async move {
                let dropped = match job.await {
                    Err(ModulesFailure::Foreign) => {
                        let Ok(answer) = win.update(&mut acx, |_, window, cx| {
                            confirm(Confirm::replace_modules_cfg(), window, cx)
                        }) else {
                            return Ok(None);
                        };
                        if !answer.recv().await.unwrap_or(false) {
                            return Ok(None);
                        }
                        let again = acx.update(|cx| {
                            jobs::apply_modules(tf2, catalog, new, guard_tf2, true, cx)
                        });
                        again.await
                    }
                    first => first,
                };
                match dropped {
                    Ok(dropped) => Ok(dropped_note(&dropped).map(|text| {
                        let title = tr!("graphics.toast.dropped_title");
                        (ToastKind::Warning, title, text)
                    })),
                    Err(ModulesFailure::Foreign) => {
                        Err(OpError::core(save_failed(), foreign_modules_cfg()))
                    }
                    Err(ModulesFailure::Op(err)) => Err(err),
                }
            },
            cx,
        );
    }

    // Runs a blocking library change off the UI thread through `finish`.
    fn run(
        &mut self,
        op: impl FnOnce(&Services) -> Result<Done, OpError> + Send + 'static,
        cx: &mut Context<Self>,
    ) {
        let running = run_blocking(cx, op);
        self.finish(running, cx);
    }

    // `settle` with a success toast.
    fn finish(
        &mut self,
        job: impl std::future::Future<Output = Result<Done, OpError>> + 'static,
        cx: &mut Context<Self>,
    ) {
        self.settle(
            async move {
                job.await
                    .map(|(title, text)| Some((ToastKind::Success, title, text)))
            },
            cx,
        );
    }

    // Marks the page busy until `job` ends, toasts its outcome, then reloads the status.
    fn settle(
        &mut self,
        job: impl std::future::Future<Output = Result<Outcome, OpError>> + 'static,
        cx: &mut Context<Self>,
    ) {
        self.set_busy(true, cx);
        cx.notify();
        let win = self.win;
        cx.spawn(async move |page, cx| {
            let done = job.await;
            let _ = cx.update(|cx| {
                win.update(cx, |_, window, cx| match done {
                    Ok(Some((kind, title, text))) => toast(window, cx, kind, title, text),
                    Ok(None) => {}
                    Err(err) => toast_error(window, cx, &err),
                })
            });
            // `busy` holds until the reload lands, so no button acts on the stale state.
            let _ = page.update(cx, |page, cx| {
                page.settling = true;
                page.reload(cx);
            });
        })
        .detach();
    }

    // Reads the install's status (only with TF2), the recorded release and, for a managed
    // install, its saved choices and any cached catalog the page lacks, in the background. A
    // failed read toasts through the window and leaves the last good values.
    fn reload(&mut self, cx: &mut Context<Self>) {
        self.load_generation += 1;
        let generation = self.load_generation;
        let tf2 = tf2_state(cx).read(cx).install().cloned();
        let held = self.catalog.as_ref().map(|c| c.tag().to_owned());
        let loading = run_blocking(cx, move |s| {
            let failed = || tr!("messages.action.load_mastercomfig");
            // Puts back a preset file another tool cleared; a failure only costs the restore.
            if let Some(tf2) = &tf2
                && let Err(e) = s.library.restore_mastercomfig_files(tf2)
            {
                tracing::warn!("mastercomfig restore: {e}");
            }
            let status = tf2
                .map(|tf2| s.library.mastercomfig_status(&tf2))
                .transpose()
                .map_err(|e| model::describe_status(&e).into_op(failed()))?;
            let release = s
                .store
                .mastercomfig_release()
                .map_err(|e| describe_store_status(&e).into_op(failed()))?;
            let (choices, catalog) = match &status {
                Some(MastercomfigStatus::Managed { version, .. }) => {
                    // Unreadable choices show the preset's values rather than failing the page.
                    let choices = s.library.mastercomfig_choices().unwrap_or_else(|e| {
                        tracing::warn!("mastercomfig settings: {e}");
                        None
                    });
                    let catalog = (held.as_deref() != Some(version.as_str()))
                        .then(|| s.store.cached_mastercomfig_catalog(version))
                        .flatten();
                    (choices, catalog)
                }
                _ => (None, None),
            };
            Ok(Loaded {
                status,
                release,
                choices,
                catalog,
            })
        });
        let win = self.win;
        cx.spawn(async move |page, cx| {
            let loaded = loading.await;
            // Through the window, so the catalog and its Selects land in the same update.
            let current = cx
                .update(|cx| {
                    win.update(cx, |_, window, cx| {
                        page.update(cx, |page, cx| {
                            if page.load_generation != generation {
                                return false;
                            }
                            if let Ok(loaded) = &loaded {
                                page.status = loaded.status.clone();
                                page.release = loaded.release.clone();
                                page.choices = loaded.choices.clone();
                                page.sync_list(loaded.catalog.clone(), window, cx);
                            } else {
                                // A refused pick still shows on its Select; move it back.
                                page.sync_editor(window, cx);
                            }
                            page.loaded = true;
                            if page.settling {
                                page.settling = false;
                                page.set_busy(false, cx);
                                page.running = None;
                            }
                            cx.notify();
                            true
                        })
                    })
                })
                .ok()
                .and_then(Result::ok)
                .unwrap_or(false);
            if let (true, Err(err)) = (current, loaded) {
                let _ =
                    cx.update(|cx| win.update(cx, |_, window, cx| toast_error(window, cx, &err)));
            }
        })
        .detach();
    }

    // Keeps the list on the managed version's catalog: a version change swaps the catalog
    // (`fresh`, the cached one or None), which rebuilds the editor's Selects; otherwise they
    // only move to the shown values, so focus stays put. A missing catalog is fetched once.
    fn sync_list(
        &mut self,
        fresh: Option<ModuleCatalog>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let version = self.managed_version().map(str::to_owned);
        if self.catalog.as_ref().map(|c| c.tag()) != version.as_deref() {
            self.catalog = fresh.map(Arc::new);
        }
        self.sync_editor(window, cx);
        if let Some(tag) = version
            && self.catalog.is_none()
            && self.asked.as_deref() != Some(tag.as_str())
        {
            self.fetch(tag, cx);
        }
    }

    // Asks the store for `tag`'s catalog; the answer lands only while `tag` is still managed.
    fn fetch(&mut self, tag: String, cx: &mut Context<Self>) {
        self.asked = Some(tag.clone());
        self.note = Some(SettingsNote::Loading);
        let job = jobs::fetch_catalog(tag.clone(), cx);
        let win = self.win;
        cx.spawn(async move |page, cx| {
            let fetched = job.await;
            let _ = cx.update(|cx| {
                win.update(cx, |_, window, cx| {
                    page.update(cx, |page, cx| {
                        if page.managed_version() != Some(tag.as_str()) || page.catalog.is_some() {
                            return;
                        }
                        match fetched {
                            Ok(catalog) => {
                                page.catalog = Some(Arc::new(catalog));
                                page.sync_editor(window, cx);
                            }
                            Err(note) => page.note = Some(note),
                        }
                        cx.notify();
                    })
                })
            });
        })
        .detach();
    }

    // Reads the pictures' manifest and checks their files in the background, once per page.
    fn load_pictures(&mut self, cx: &mut Context<Self>) {
        let reading = run_blocking(cx, |s| {
            Ok(PictureIndex::open(&s.assets.setting_pictures_dir()))
        });
        cx.spawn(async move |page, cx| {
            let index = reading.await.unwrap_or_default();
            let _ = page.update(cx, |page, cx| {
                let index = Arc::new(index);
                page.pictures = Some(Arc::clone(&index));
                page.editor
                    .update(cx, |e, cx| e.set_pictures(Some(index), cx));
                cx.notify();
            });
        })
        .detach();
    }

    // The only writer of `busy`, so the editor's controls rest at the same moments.
    fn set_busy(&mut self, busy: bool, cx: &mut Context<Self>) {
        self.busy = busy;
        self.editor.update(cx, |e, cx| e.set_disabled(busy, cx));
    }

    // Hands the editor the shown choices, and the catalog when it changed.
    fn sync_editor(&self, window: &mut Window, cx: &mut Context<Self>) {
        let (choices, catalog) = (self.shown_against(), self.catalog.clone());
        self.editor.update(cx, |e, cx| {
            let same = match (e.catalog(), &catalog) {
                (Some(held), Some(new)) => Arc::ptr_eq(held, new),
                (held, new) => held.is_none() && new.is_none(),
            };
            if !same {
                e.set_catalog(catalog, window, cx);
            }
            e.set_choices(choices, window, cx);
        });
    }

    // The choices the rows show: the saved ones, else none on top of the current preset.
    fn shown_against(&self) -> Choices {
        self.choices
            .clone()
            .unwrap_or_else(|| Choices::new(self.preset().unwrap_or(Preset::Medium)))
    }
}

impl Render for GraphicsPage {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let missing = matches!(tf2_state(cx).read(cx), Tf2State::Missing { .. });
        let warning =
            missing.then(|| no_tf2_warning(ids::GRAPHICS_NO_TF2, tr!("graphics.no_tf2"), cx));
        // Each section must be a direct child of the scroll column, so the rail can scroll
        // to it by index; the children are gathered first to learn those indices.
        let narrow = window.viewport_size().width - SHELL_CHROME_WIDTH < px(NARROW);
        self.wanted = self.editor.update(cx, |e, cx| {
            e.want_images(cx);
            e.wanted_images().to_vec()
        });
        let filter = self.filter(cx);
        let sections = self
            .catalog
            .as_deref()
            .filter(|_| self.preset().is_some())
            .map(|catalog| view::sections(catalog, &self.shown_against(), &filter));
        let mut children: Vec<AnyElement> = Vec::new();
        children.extend(warning.map(IntoElement::into_any_element));
        if let Some(card) = self.card() {
            children.push(self.status_row(&card, cx));
            if self.tf2_running {
                children.push(status::running_banner(cx));
            }
        }
        let mut items = BTreeMap::new();
        let mut toolbar = None;
        if self.preset().is_some() {
            let (presets, changes) = self.editor.update(cx, |e, cx| {
                (e.presets_card(narrow, cx), e.changes_strip(cx))
            });
            children.push(presets);
            children.push(changes);
            toolbar = settings::push(
                self,
                sections.as_deref(),
                narrow,
                self.rail_pin(),
                &mut children,
                &mut items,
                cx,
            );
        }
        self.section_items = items;
        self.toolbar_item = toolbar;
        let content = v_flex()
            .id(ids::page(Page::Graphics))
            .test_support()
            .size_full()
            .track_scroll(&self.scroll)
            .overflow_y_scroll()
            .p(CONTENT_PADDING)
            .gap_6()
            .children(children);
        scroll_area(ids::page_scrollbar(Page::Graphics), &self.scroll, content).size_full()
    }
}
