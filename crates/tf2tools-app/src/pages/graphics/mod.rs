//! The Graphics page: mastercomfig's status, the graphics preset picker and the settings list.

pub mod jobs;
pub mod model;
mod picture_panel;
pub mod pictures;
mod settings;
pub mod view;

use std::collections::BTreeMap;
use std::sync::Arc;

use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::component::select::{SelectEvent, SelectState};
use gpui_kit::component::slider::{SliderEvent, SliderState};
use gpui_kit::component::{ActiveTheme as _, Disableable as _, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, AnyWindowHandle, App, AppContext as _, Context, Entity, FontWeight,
    InteractiveElement as _, IntoElement, MouseButton, ParentElement as _, Render, Role,
    ScrollHandle, SharedString, StatefulInteractiveElement as _, Styled as _, Subscription,
    TestSupportExt as _, Window, div, px,
};
use tf2tools_core::{
    Choices, MastercomfigRelease, MastercomfigStatus, ModuleCatalog, ModuleChoice, Preset,
};

use crate::i18n::{counted_plural, tr};
use crate::ids;
use crate::local_messages::describe_store_status;
use crate::pages::common::{card_frame, no_tf2_warning};
use crate::pages::profiles::probe::Tf2Probe;
use crate::remote_images::{ImageKey, RemoteImages};
use crate::services::{OpError, Services, Tf2State, require_tf2, run_blocking, tf2_state};
use crate::shell::CONTENT_PADDING;
use crate::shell::confirm::{Confirm, confirm};
use crate::shell::route::Page;
use crate::shell::toast::{ToastKind, toast, toast_error};
use crate::widgets::card_grid::SHELL_CHROME_WIDTH;
use crate::widgets::scroll::scroll_area;
use crate::widgets::{Named as _, labelled};
use jobs::{ModulesFailure, save_failed};
use model::{
    OWN_MODULES_CFG_KEPT, SettingsNote, StatusCard, ValueOption, describe_apply, describe_remove,
    describe_take_over, detail, dropped_note, foreign_modules_cfg, headline, preset_description,
    preset_label, shown_choice, status_card, update_to, value_options,
};
use pictures::PictureIndex;
use view::Filter;

const CREDIT: &str = "mastercomfig is made by the mastercomfig team and shared under the MIT licence. Preset descriptions come from comfig-app.";
const FOOTER: &str = "Changes apply the next time TF2 starts.";

/// A finished action's success toast: (title, description).
type Done = (SharedString, SharedString);

/// A finished action's toast, if it has one: (kind, title, description).
type Outcome = Option<(ToastKind, SharedString, SharedString)>;

/// A settings row's value picker.
type ValueSelect = Entity<SelectState<Vec<ValueOption>>>;

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
    /// One Select per Select row of the catalog, built when the catalog arrives.
    selects: BTreeMap<String, ValueSelect>,
    /// Each Select's pick handler, rebuilt with the Selects.
    select_subscriptions: Vec<Subscription>,
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
    scroll: ScrollHandle,
    /// The bundled setting pictures, once read off the UI thread.
    pictures: Option<Arc<PictureIndex>>,
    /// The image keys the last frame asked for.
    wanted: Vec<ImageKey>,
    /// This page's own `RemoteImages` viewer, so a page released late cannot clear a newer one's wants.
    image_viewer: String,
    /// The module whose pictures are open; at most one row's at a time.
    open_row: Option<String>,
    /// How far across the compare view the preset's picture shows, 0 to 100.
    wipe: Entity<SliderState>,
    /// Whether the compare view shows the crops; one switch for the whole page.
    zoom: bool,
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
        let wipe = cx.new(|_| {
            SliderState::new()
                .min(0.)
                .max(100.)
                .step(1.)
                .default_value(50.)
        });
        let wiped = cx.subscribe(&wipe, |_, _, _: &SliderEvent, cx| cx.notify());
        let mut subscriptions = vec![tf2, searched, wiped];
        // Pictures redraw the page as they land, and the page's wants go with it.
        let image_viewer = format!("graphics:{}", cx.entity_id());
        if let Some(images) = RemoteImages::global(cx) {
            subscriptions.push(cx.observe(&images, |_, _, cx| cx.notify()));
            let released = image_viewer.clone();
            subscriptions.push(cx.on_release(move |_, cx| {
                images.update(cx, |images, cx| images.want(&released, Vec::new(), cx));
            }));
        }
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
            select_subscriptions: Vec::new(),
            note: None,
            asked: None,
            search,
            changed_only: false,
            section_items: BTreeMap::new(),
            scroll: ScrollHandle::new(),
            pictures: None,
            wanted: Vec::new(),
            image_viewer,
            open_row: None,
            wipe,
            zoom: false,
            _subscriptions: subscriptions,
        };
        page.reload(cx);
        page.load_pictures(cx);
        page
    }

    /// The bundled setting pictures once read; an empty index when the folder cannot be read.
    pub fn pictures(&self) -> Option<&PictureIndex> {
        self.pictures.as_deref()
    }

    /// The image keys the last frame asked the image cache for.
    pub fn wanted_images(&self) -> &[ImageKey] {
        &self.wanted
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

    // Opens `module`'s pictures, closing any other row's, or closes them when already open.
    // A freshly opened row starts with the wipe in the middle.
    fn toggle_pictures(&mut self, module: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.open_row = if self.open_row.as_deref() == Some(module) {
            None
        } else {
            self.wipe
                .update(cx, |wipe, cx| wipe.set_value(50., window, cx));
            Some(module.to_owned())
        };
        cx.notify();
    }

    fn toggle_zoom(&mut self, cx: &mut Context<Self>) {
        self.zoom = !self.zoom;
        cx.notify();
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
                    ("mastercomfig updated", format!("Now on {version}."))
                } else {
                    let ready = format!("mastercomfig {version} is ready. Pick a preset below.");
                    ("mastercomfig installed", ready)
                };
                if let Some(note) = dropped_note(&dropped) {
                    text = format!("{text} {note}");
                }
                if own_file_kept {
                    text = format!("{text} {OWN_MODULES_CFG_KEPT}");
                }
                Ok((title.into(), text.into()))
            },
            cx,
        );
    }

    // Forces the addon-store sync; the reloaded card then says "Up to date" or "Update to X",
    // so the toast names no version.
    fn check(&mut self, cx: &mut Context<Self>) {
        let job = jobs::check_for_updates(cx);
        self.running = Some(Job::Check);
        self.finish(
            async move {
                job.await?;
                Ok(("Checked for updates".into(), SharedString::default()))
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
                let text = "TF2Tools now looks after mastercomfig.";
                Ok(("mastercomfig taken over".into(), text.into()))
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
                        let mut text = "mastercomfig is no longer in TF2.".to_owned();
                        if restored {
                            text.push(' ');
                            text.push_str(&crate::pages::addons::model::movement_off());
                        }
                        Ok(("mastercomfig removed".into(), text.into()))
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
                let text = format!("{} applies the next time TF2 starts.", preset_label(preset));
                Ok(("Preset applied".into(), text.into()))
            },
            cx,
        );
    }

    // Saves the choices `edit` makes from the saved ones; an edit that changes nothing does
    // nothing. Only a change to the override VPKs waits for TF2 to close. A player's own
    // modules.cfg is replaced only after they agree; the page stays busy while asking.
    fn change(&mut self, edit: impl FnOnce(&mut Choices, &ModuleCatalog), cx: &mut Context<Self>) {
        let Some(catalog) = self.catalog.clone() else {
            return;
        };
        if self.busy || self.managed_version().is_none() {
            return;
        }
        let old = self.shown_against();
        let mut new = old.clone();
        edit(&mut new, &catalog);
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
                        let title = "Some settings went back to the preset";
                        (ToastKind::Warning, title.into(), text.into())
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
        self.busy = true;
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
                                page.show_choices(window, cx);
                            }
                            page.loaded = true;
                            if page.settling {
                                page.settling = false;
                                page.busy = false;
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
    // (`fresh`, the cached one or None) and rebuilds the Selects; otherwise the Selects only
    // move to the shown values, so focus stays put. A missing catalog is fetched once.
    fn sync_list(
        &mut self,
        fresh: Option<ModuleCatalog>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let version = self.managed_version().map(str::to_owned);
        if self.catalog.as_ref().map(|c| c.tag()) != version.as_deref() {
            self.catalog = fresh.map(Arc::new);
            self.build_selects(window, cx);
        } else {
            self.show_choices(window, cx);
        }
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
                                page.build_selects(window, cx);
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

    // One Select per Select row of the held catalog, each on its shown value; a pick saves it,
    // and a cleared Select changes nothing.
    fn build_selects(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.selects.clear();
        self.select_subscriptions.clear();
        let Some(catalog) = self.catalog.clone() else {
            return;
        };
        let choices = self.shown_against();
        let select_rows = catalog
            .categories()
            .iter()
            .flat_map(|c| c.modules())
            .filter(|m| view::control_kind(m) == view::ControlKind::Select);
        for module in select_rows {
            let shown = shown_choice(&catalog, &choices, module.name());
            let options = value_options(module);
            let select = cx.new(|cx| {
                let mut select = SelectState::new(options, None, window, cx);
                select.set_selected_value(&shown, window, cx);
                select
            });
            let name = module.name().to_owned();
            self.select_subscriptions.push(cx.subscribe(
                &select,
                move |page, _, event: &SelectEvent<Vec<ValueOption>>, cx| {
                    if let SelectEvent::Confirm(Some(choice)) = event {
                        let choice = choice.clone();
                        page.change(|c, catalog| c.set(catalog, &name, choice), cx);
                    }
                },
            ));
            self.selects.insert(module.name().to_owned(), select);
        }
    }

    // Saves `value` for `module`, as the row's own control does.
    fn pick_value(&mut self, module: &str, value: &str, cx: &mut Context<Self>) {
        self.change(
            |c, catalog| c.set(catalog, module, ModuleChoice::Value(value.to_owned())),
            cx,
        );
    }

    // Drops `module`'s saved choice, so the preset's value applies.
    fn reset_module(&mut self, module: &str, cx: &mut Context<Self>) {
        self.change(|c, _| c.reset(module), cx);
    }

    // Reads the pictures' manifest and checks their files in the background, once per page.
    fn load_pictures(&mut self, cx: &mut Context<Self>) {
        let reading = run_blocking(cx, |s| {
            Ok(PictureIndex::open(&s.assets.setting_pictures_dir()))
        });
        cx.spawn(async move |page, cx| {
            let index = reading.await.unwrap_or_default();
            let _ = page.update(cx, |page, cx| {
                page.pictures = Some(Arc::new(index));
                cx.notify();
            });
        })
        .detach();
    }

    // The open row's compare pictures first, as the newest wants, then its strip, then the
    // current thumbnail of every row with pictures; all thumbnails are wanted, not just the visible.
    fn image_keys(&self) -> Vec<ImageKey> {
        let (Some(index), Some(catalog)) = (self.pictures.as_deref(), self.catalog.as_deref())
        else {
            return Vec::new();
        };
        let open = self
            .open_row
            .as_deref()
            .and_then(|module| self.row(module))
            .and_then(|row| pictures::row_pictures(index, &row, self.zoom));
        let (compared, strip) = open
            .map(|p| (p.compare.pictures(), p.strip))
            .unwrap_or_default();
        let choices = self.shown_against();
        let thumbs = catalog
            .categories()
            .iter()
            .flat_map(|category| category.modules())
            .filter(|module| index.has_pictures(module.name()))
            .filter_map(|module| {
                let row = view::row(catalog, &choices, module);
                pictures::row_pictures(index, &row, false)?.thumb
            })
            .map(|thumb| ImageKey::card(&index.url(&thumb)));
        compared
            .into_iter()
            .map(|picture| ImageKey::carousel(&index.url(&picture)))
            .chain(strip.iter().map(|p| ImageKey::card(&index.url(&p.picture))))
            .chain(thumbs)
            .collect()
    }

    // Records this frame's keys and, when they changed, hands them to the image cache after
    // the draw, so no entity is updated mid-render.
    fn want_images(&mut self, cx: &mut Context<Self>) {
        let keys = self.image_keys();
        if keys == self.wanted {
            return;
        }
        self.wanted = keys.clone();
        let Some(images) = RemoteImages::global(cx) else {
            return;
        };
        let viewer = self.image_viewer.clone();
        cx.defer(move |cx| images.update(cx, |images, cx| images.want(&viewer, keys, cx)));
    }

    // Drops every saved choice and keeps the preset.
    fn reset_all(&mut self, cx: &mut Context<Self>) {
        self.change(|c, _| c.clear(), cx);
    }

    // Moves each existing Select to its row's shown value.
    fn show_choices(&self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(catalog) = &self.catalog else {
            return;
        };
        let choices = self.shown_against();
        for (name, select) in &self.selects {
            let shown = shown_choice(catalog, &choices, name);
            select.update(cx, |select, cx| {
                select.set_selected_value(&shown, window, cx)
            });
        }
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
        let warning = missing.then(|| {
            no_tf2_warning(
                ids::GRAPHICS_NO_TF2,
                "Set the TF2 folder in Settings to use mastercomfig.",
                cx,
            )
        });
        // Each section must be a direct child of the scroll column, so the rail can scroll
        // to it by index; the children are gathered first to learn those indices.
        let narrow = window.viewport_size().width - SHELL_CHROME_WIDTH < px(NARROW);
        self.want_images(cx);
        let filter = self.filter(cx);
        let sections = self
            .catalog
            .as_deref()
            .filter(|_| self.preset().is_some())
            .map(|catalog| view::sections(catalog, &self.shown_against(), &filter));
        let mut children: Vec<AnyElement> = Vec::new();
        children.extend(warning.map(IntoElement::into_any_element));
        children.push(
            labelled(
                div().text_sm().text_color(cx.theme().muted_foreground),
                ids::GRAPHICS_SUBTITLE,
                "Graphics and game settings from mastercomfig",
            )
            .into_any_element(),
        );
        children.extend(self.card().map(|card| self.status_card(&card, cx)));
        let mut items = BTreeMap::new();
        if let Some(preset) = self.preset() {
            children.push(self.presets(preset, window, cx));
            children.push(self.changes_strip(preset, cx));
            settings::push(
                self,
                sections.as_deref(),
                narrow,
                &mut children,
                &mut items,
                cx,
            );
            children.push(
                labelled(
                    div().text_sm().text_color(cx.theme().muted_foreground),
                    ids::GRAPHICS_FOOTER,
                    FOOTER,
                )
                .into_any_element(),
            );
        }
        self.section_items = items;
        let content = v_flex()
            .id(ids::page(Page::Graphics))
            .test_support()
            .size_full()
            .track_scroll(&self.scroll)
            .overflow_y_scroll()
            .p(CONTENT_PADDING)
            .gap_6()
            .children(children);
        let column = scroll_area(ids::page_scrollbar(Page::Graphics), &self.scroll, content);
        let rail = sections
            .as_deref()
            .filter(|_| !narrow)
            .map(|sections| settings::rail(sections, false, cx));
        h_flex()
            .size_full()
            .items_start()
            .children(rail)
            .child(column.flex_1().min_w_0().h_full())
    }
}

impl GraphicsPage {
    // The headline, its detail line, the buttons this state offers and the credit line.
    fn status_card(&self, card: &StatusCard, cx: &mut Context<Self>) -> AnyElement {
        let muted = cx.theme().muted_foreground;
        let busy = self.busy;
        let installing = self.running == Some(Job::Install);
        let checking = self.running == Some(Job::Check);
        let mut buttons: Vec<AnyElement> = Vec::new();
        match card {
            StatusCard::NotInstalled => buttons.push(
                Button::new(ids::GRAPHICS_INSTALL)
                    .primary()
                    .icon(IconName::Download)
                    .label("Install")
                    .loading(installing)
                    .disabled(busy)
                    .on_click(cx.listener(|page, _, window, cx| page.install(false, window, cx)))
                    .into_any_element(),
            ),
            StatusCard::Installed { .. } => {
                if update_to(card).is_some() {
                    buttons.push(
                        Button::new(ids::GRAPHICS_UPDATE)
                            .primary()
                            .icon(IconName::RefreshCw)
                            .label("Update")
                            .loading(installing)
                            .disabled(busy)
                            .on_click(
                                cx.listener(|page, _, window, cx| page.install(true, window, cx)),
                            )
                            .into_any_element(),
                    );
                }
                buttons.push(
                    Button::new(ids::GRAPHICS_CHECK)
                        .outline()
                        .icon(IconName::RotateCw)
                        .label("Check for updates")
                        .loading(checking)
                        .disabled(busy)
                        .on_click(cx.listener(|page, _, _, cx| page.check(cx)))
                        .into_any_element(),
                );
                buttons.push(
                    Button::new(ids::GRAPHICS_REMOVE)
                        .outline()
                        .icon(IconName::Trash)
                        .label("Remove")
                        .disabled(busy)
                        .on_click(cx.listener(|page, _, window, cx| page.remove(window, cx)))
                        .into_any_element(),
                );
            }
            StatusCard::Outside { .. } => buttons.push(
                Button::new(ids::GRAPHICS_TAKE_OVER)
                    .primary()
                    .label("Take over")
                    .disabled(busy)
                    .on_click(cx.listener(|page, _, window, cx| page.take_over(window, cx)))
                    .into_any_element(),
            ),
        }
        let detail = detail(card).map(|text| {
            labelled(
                div().text_sm().text_color(muted),
                ids::GRAPHICS_DETAIL,
                text,
            )
        });
        card_frame(cx)
            .id(ids::GRAPHICS_STATUS)
            .test_support()
            .p_6()
            .child(
                v_flex()
                    .gap_4()
                    .child(
                        v_flex()
                            .gap_1()
                            .child(labelled(
                                div().text_lg().font_weight(FontWeight::SEMIBOLD),
                                ids::GRAPHICS_HEADLINE,
                                headline(card),
                            ))
                            .children(detail),
                    )
                    .child(h_flex().flex_wrap().gap_2().children(buttons))
                    .child(labelled(
                        div().text_xs().text_color(muted),
                        ids::GRAPHICS_CREDIT,
                        CREDIT,
                    )),
            )
            .into_any_element()
    }

    // The five preset cards in comfig-app's order, the current one in the primary border. Each
    // is a tab stop where Enter or Space picks it; a click does not take focus, as on the pills.
    fn presets(&self, current: Preset, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let theme = cx.theme();
        let (primary, border, ring, muted, radius) = (
            theme.primary,
            theme.border,
            theme.ring,
            theme.muted_foreground,
            theme.radius_lg,
        );
        let cards: Vec<AnyElement> = Preset::PICKER
            .into_iter()
            .map(|preset| {
                let chosen = preset == current;
                let id = ids::graphics_preset(preset);
                let focus = window
                    .use_keyed_state(id.clone(), cx, |_, cx| cx.focus_handle().tab_stop(true))
                    .read(cx)
                    .clone();
                let edge = if focus.is_focused(window) {
                    ring
                } else if chosen {
                    primary
                } else {
                    border
                };
                card_frame(cx)
                    .id(id)
                    .role(Role::Button)
                    .aria_label(preset_label(preset))
                    .aria_selected(chosen)
                    .test_support()
                    .track_focus(&focus)
                    .on_mouse_down(MouseButton::Left, |_, window, _| window.prevent_default())
                    .flex_1()
                    .min_w(px(160.))
                    .p_4()
                    .rounded(radius)
                    .border_color(edge)
                    .cursor_pointer()
                    .when(self.busy, |card| card.opacity(0.6))
                    .on_click(cx.listener(move |page, _, window, cx| page.pick(preset, window, cx)))
                    .child(
                        v_flex()
                            .gap_1()
                            .child(
                                div()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child(preset_label(preset)),
                            )
                            .child(
                                div()
                                    .text_sm()
                                    .text_color(muted)
                                    .child(preset_description(preset)),
                            ),
                    )
                    .into_any_element()
            })
            .collect();
        v_flex()
            .gap_3()
            .child(
                div()
                    .text_lg()
                    .font_weight(FontWeight::SEMIBOLD)
                    .child("Preset"),
            )
            .child(h_flex().flex_wrap().gap_4().children(cards))
            .into_any_element()
    }

    // The player's changes from `preset` as chips with an undo each, and Reset all; a dashed
    // "as is" strip when there are none. Offline the chips only name the saved values, since
    // undoing needs the catalogue.
    fn changes_strip(&self, preset: Preset, cx: &mut Context<Self>) -> AnyElement {
        let theme = cx.theme();
        let (border, muted, radius) = (theme.border, theme.muted_foreground, theme.radius);
        let preset = preset_label(preset);
        let catalog = self.catalog.as_deref();
        let list = view::changes(catalog, &self.shown_against());
        if list.is_empty() {
            return labelled(
                div()
                    .text_sm()
                    .text_color(muted)
                    .p_3()
                    .border_1()
                    .border_dashed()
                    .border_color(border)
                    .rounded(radius),
                ids::GRAPHICS_NO_CHANGES,
                tr!("graphics.changes.as_is", preset = preset),
            )
            .into_any_element();
        }
        let n = u64::try_from(list.len()).unwrap_or(u64::MAX);
        let title = counted_plural(
            "graphics.changes.title",
            n,
            &n.to_string(),
            &[("preset", &preset)],
        );
        let editable = catalog.is_some();
        let chips: Vec<AnyElement> = list
            .into_iter()
            .map(|change| {
                let sentence = match &change.was {
                    Some(was) => tr!(
                        "graphics.changes.chip",
                        name = change.label,
                        was = was,
                        now = change.now
                    ),
                    None => tr!(
                        "graphics.changes.chip_saved",
                        name = change.label,
                        now = change.now
                    ),
                };
                let undo = change.was.clone().filter(|_| editable).map(|was| {
                    let module = change.module.clone();
                    Button::new(ids::graphics_change_undo(&change.module))
                        .ghost()
                        .xsmall()
                        .icon(IconName::X)
                        .named(tr!("graphics.changes.undo", name = change.label, was = was))
                        .disabled(self.busy)
                        .on_click(cx.listener(move |page, _, _, cx| page.reset_module(&module, cx)))
                });
                h_flex()
                    .id(ids::graphics_change(&change.module))
                    .aria_label(sentence)
                    .test_support()
                    .gap_1()
                    .pl_2()
                    .pr_1()
                    .py_0p5()
                    .text_sm()
                    .border_1()
                    .border_color(border)
                    .rounded(radius)
                    .child(change.label)
                    .children(
                        change
                            .was
                            .map(|was| div().text_color(muted).child(format!("{was} →"))),
                    )
                    .child(div().font_weight(FontWeight::MEDIUM).child(change.now))
                    .children(undo)
                    .into_any_element()
            })
            .collect();
        let reset_all = editable.then(|| {
            Button::new(ids::GRAPHICS_RESET_ALL)
                .outline()
                .small()
                .label(tr!("graphics.changes.reset_all"))
                .disabled(self.busy)
                .on_click(cx.listener(|page, _, _, cx| page.reset_all(cx)))
        });
        v_flex()
            .id(ids::GRAPHICS_CHANGES)
            .aria_label(title.clone())
            .test_support()
            .gap_3()
            .p_3()
            .border_1()
            .border_color(border)
            .rounded(radius)
            .child(
                h_flex()
                    .justify_between()
                    .gap_4()
                    .child(div().font_weight(FontWeight::SEMIBOLD).child(title))
                    .children(reset_all),
            )
            .child(h_flex().flex_wrap().gap_2().children(chips))
            .into_any_element()
    }
}
