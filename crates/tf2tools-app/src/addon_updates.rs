//! Addon downloads and their pending reinstalls, as an app global so an Update still copies
//! the new release into tf/custom after the Addon Store page is left.

use std::collections::{HashMap, HashSet};

use gpui_kit::{AnyWindowHandle, App, AppContext as _, Context, Entity, EventEmitter, Global};
use tf2tools_core::{
    AddonName, FilesError, LogLevel, NewLogEntry, StoreFile, StoreItemId, StoreKind,
};

use crate::activity;
use crate::library_events::LibraryEvents;
use crate::services::{OpError, Services, Tf2Global, run_blocking};
use crate::shell::toast::{ToastKind, toast, toast_error};
use crate::store_download::{DownloadEvent, DownloadTarget, StoreDownloads};
use crate::store_messages::{
    BUSY_TEXT, BUSY_TITLE, StoreFailure, describe_files, describe_install, listing_stopped, no_tf2,
    release_has_no_file,
};
use crate::tokio_bridge::{self, JoinError};

const INSTALL_FAILED: &str = "Failed to install addon";

// Logs a failed reinstall's raw error and keeps only the player words for the toast.
fn install_failed(failure: StoreFailure) -> OpError {
    failure.log("addon update");
    OpError::core(INSTALL_FAILED, failure.message)
}

/// How an addon download ended for the pages that show it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AddonEvent {
    /// A download of `name` ended (or never started), its reinstall included.
    Settled { name: AddonName },
}

/// The app's addon downloads: which ones reinstall on finish, and which reinstalls run.
pub struct AddonUpdates {
    pending: HashMap<AddonName, AnyWindowHandle>,
    // Lookups in flight per name; overlapping starts on one name each hold it busy.
    looking_up: HashMap<AddonName, usize>,
    installing: HashSet<AddonName>,
    _downloads: gpui_kit::Subscription,
}

struct AddonUpdatesGlobal(Entity<AddonUpdates>);

impl Global for AddonUpdatesGlobal {}

impl EventEmitter<AddonEvent> for AddonUpdates {}

impl AddonUpdates {
    /// The app's instance, installed on first use; it subscribes to the installed `StoreDownloads`.
    pub fn global(cx: &mut App) -> Entity<Self> {
        if let Some(existing) = cx.try_global::<AddonUpdatesGlobal>() {
            return existing.0.clone();
        }
        // `install` rather than `global`, so an instance made before the downloads exist
        // still hears every download event.
        let downloads = StoreDownloads::install(cx);
        let updates = cx.new(|cx| Self {
            pending: HashMap::new(),
            looking_up: HashMap::new(),
            installing: HashSet::new(),
            _downloads: cx.subscribe(&downloads, Self::on_download),
        });
        cx.set_global(AddonUpdatesGlobal(updates.clone()));
        updates
    }

    /// Reads `name`'s release file with `item_files` on the runtime and starts it in
    /// `StoreDownloads` (toasting in `win`). With `reinstall`, its `Finished` copies it into
    /// tf/custom and toasts "Addon installed"; a failure toasts "Failed to install addon".
    pub fn start(
        &mut self,
        name: AddonName,
        display: String,
        reinstall: bool,
        win: AnyWindowHandle,
        cx: &mut Context<Self>,
    ) {
        if reinstall {
            self.pending.insert(name.clone(), win);
        }
        // Busy from the click, so no second addon starts while this release is looked up.
        *self.looking_up.entry(name.clone()).or_default() += 1;
        cx.notify();
        if !tokio_bridge::is_running(cx) {
            tokio_bridge::init(cx);
        }
        let store = cx.global::<Services>().store.clone();
        let item = StoreItemId::Addon(name.clone());
        let lookup = {
            let item = item.clone();
            tokio_bridge::spawn(cx, async move {
                store.item_files(StoreKind::Addons, &item).await
            })
        };
        cx.spawn(async move |this, cx| {
            let files = lookup.await;
            let _ = this.update(cx, |u, cx| u.begin(name, display, item, files, win, cx));
        })
        .detach();
    }

    /// Whether `name` reinstalls into tf/custom once its download finishes.
    pub fn is_pending(&self, name: &AddonName) -> bool {
        self.pending.contains_key(name)
    }

    /// Whether `name`'s reinstall into tf/custom runs now.
    pub fn is_installing(&self, name: &AddonName) -> bool {
        self.installing.contains(name)
    }

    /// Whether any release lookup is in flight, or any reinstall is pending or running.
    pub fn is_busy(&self) -> bool {
        !self.looking_up.is_empty() || !self.pending.is_empty() || !self.installing.is_empty()
    }

    // Hands the looked-up release file to `StoreDownloads`; a failed lookup or a refused start
    // ends here, so no reinstall lingers for a download that never runs.
    fn begin(
        &mut self,
        name: AddonName,
        display: String,
        item: StoreItemId,
        files: Result<Result<Vec<StoreFile>, FilesError>, JoinError>,
        win: AnyWindowHandle,
        cx: &mut Context<Self>,
    ) {
        // Release the name only when its last overlapping lookup ends.
        if let Some(count) = self.looking_up.get_mut(&name) {
            *count -= 1;
            if *count == 0 {
                self.looking_up.remove(&name);
            }
        }
        let file = match files {
            Ok(Ok(mut files)) if !files.is_empty() => files.remove(0),
            failed => {
                let failure = match failed {
                    Ok(Err(err)) => describe_files(StoreKind::Addons, &err),
                    Err(err) => listing_stopped(err.to_string()),
                    Ok(Ok(_)) => release_has_no_file(),
                };
                failure.log("addon update");
                let err = OpError::core("Failed to download addon", failure.message);
                let _ = win.update(cx, |_, window, cx| toast_error(window, cx, &err));
                self.settle(&name, cx);
                return;
            }
        };
        let downloads = StoreDownloads::install(cx);
        let target = DownloadTarget {
            kind: StoreKind::Addons,
            item: item.clone(),
            name: display,
        };
        let started = downloads.update(cx, |d, cx| d.start(target, file, win, cx));
        if !started {
            // A refusal because this same addon already downloads keeps that run's reinstall,
            // and that run's own end settles it; any other refusal tells the player to wait,
            // toasting before it settles so a page reacting to `Settled` finds it queued.
            if downloads.read(cx).is_downloading(StoreKind::Addons, &item) {
                cx.notify();
            } else {
                let _ = win.update(cx, |_, window, cx| {
                    toast(window, cx, ToastKind::Info, BUSY_TITLE, BUSY_TEXT)
                });
                self.settle(&name, cx);
            }
        }
    }

    fn on_download(
        &mut self,
        _: Entity<StoreDownloads>,
        event: &DownloadEvent,
        cx: &mut Context<Self>,
    ) {
        let (target, finished) = match event {
            DownloadEvent::Finished { target } => (target, true),
            DownloadEvent::Failed { target, .. } => (target, false),
        };
        let StoreItemId::Addon(name) = &target.item else {
            return;
        };
        // A failure drops the pending reinstall, so the old copy stays in tf/custom.
        match self.pending.remove(name) {
            Some(win) if finished => self.reinstall(name.clone(), target.name.clone(), win, cx),
            _ => self.settle(name, cx),
        }
    }

    // Copies the freshly imported release into tf/custom off the UI thread, then toasts in
    // the window the Update was clicked in.
    fn reinstall(
        &mut self,
        name: AddonName,
        display: String,
        win: AnyWindowHandle,
        cx: &mut Context<Self>,
    ) {
        self.installing.insert(name.clone());
        cx.notify();
        let tf2 = cx
            .try_global::<Tf2Global>()
            .and_then(|g| g.0.read(cx).install().cloned());
        let job = {
            let name = name.clone();
            let display = display.clone();
            run_blocking(cx, move |services| {
                let tf2 = tf2.ok_or_else(|| install_failed(no_tf2()))?;
                services
                    .library
                    .install_addon(&name, &tf2)
                    .map_err(|e| install_failed(describe_install(&e)))?;
                let message = format!("Installed addon: {display}");
                let entry = NewLogEntry::new(LogLevel::Info, "addon_store", "install", message);
                activity::write(services, &entry);
                Ok(())
            })
        };
        cx.spawn(async move |this, cx| {
            let result = job.await;
            let _ = this.update(cx, |u, cx| {
                u.installing.remove(&name);
                let _ = win.update(cx, |_, window, cx| match &result {
                    Ok(()) => toast(window, cx, ToastKind::Success, "Addon installed", display),
                    Err(err) => toast_error(window, cx, err),
                });
                // Outside the window update, so a closed window still signals the change.
                if result.is_ok() {
                    LibraryEvents::changed(cx);
                }
                u.settle(&name, cx);
            });
        })
        .detach();
    }

    fn settle(&mut self, name: &AddonName, cx: &mut Context<Self>) {
        self.pending.remove(name);
        cx.emit(AddonEvent::Settled { name: name.clone() });
        cx.notify();
    }
}
