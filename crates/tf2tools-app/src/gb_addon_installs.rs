//! GameBanana add-on installs: the installed list and the one running operation, as an app
//! global so leaving a page never drops (and so aborts) an install.

use std::rc::Rc;
use std::time::{SystemTime, UNIX_EPOCH};

use gpui_kit::{
    AnyWindowHandle, App, AppContext as _, AsyncApp, Context, Entity, Global, Task, WeakEntity,
};
use tf2tools_core::{
    AddonProfile, AddonSection, BrowseError, DownloadProgress, GbAddonCheck, GbAddonDownload,
    GbAddonEntry, GbAddonFetchError, GbId, StoreConfig, Tf2Install,
};

use crate::library_events::LibraryEvents;
use crate::local_messages::{describe_library, gb_install_op};
use crate::pages::profiles::probe::Tf2Probe;
use crate::payload_checklist::{ChecklistItem, open_payload_checklist};
use crate::services::{OpError, Services, require_tf2, run_blocking, tf2_state};
use crate::shell::toast::{ToastKind, toast, toast_error};
use crate::store_messages::{describe_browse, describe_gb_fetch, download_stopped};
use crate::tokio_bridge;

/// What the running operation is doing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GbAddonStage {
    Downloading { downloaded: u64, total: Option<u64> },
    Choosing,
    Installing,
    Removing,
}

/// What an add-on's install control offers, from its library entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GbAddonOffer {
    Install,
    Update,
    InstallAgain,
    Installed,
}

impl GbAddonOffer {
    /// Update when a newer file is known and the item is still on GameBanana, Install again
    /// when out of TF2, Installed when in it, and Install with no entry.
    pub fn of(entry: Option<&GbAddonEntry>) -> Self {
        match entry {
            None => Self::Install,
            Some(e) if e.has_update() && !e.is_gone() => Self::Update,
            Some(e) if !e.is_installed() => Self::InstallAgain,
            Some(_) => Self::Installed,
        }
    }

    /// The control's label.
    pub fn label(self) -> &'static str {
        match self {
            Self::Install => "Install",
            Self::Update => "Update",
            Self::InstallAgain => "Install again",
            Self::Installed => "Installed",
        }
    }

    /// The card's badge: "Update" or "Installed", none otherwise.
    pub fn badge(self) -> Option<&'static str> {
        matches!(self, Self::Update | Self::Installed).then(|| self.label())
    }

    /// Whether the offer downloads GameBanana's newest file; the rest use the library copy.
    pub fn downloads(self) -> bool {
        matches!(self, Self::Install | Self::Update)
    }
}

/// The toast titles of one kind of operation: on success and on failure.
#[derive(Clone, Copy)]
struct Titles {
    done: &'static str,
    failed: &'static str,
}

const INSTALL: Titles = Titles {
    done: "Add-on installed",
    failed: "Couldn't install the add-on",
};

const UPDATE: Titles = Titles {
    done: "Add-on updated",
    failed: "Couldn't update the add-on",
};

const REMOVE: Titles = Titles {
    done: "Add-on removed",
    failed: "Couldn't remove the add-on",
};

/// One download-choose-install run: what to fetch, the payloads chosen last time, where to
/// install, and the window that asked.
struct Run {
    section: AddonSection,
    profile: AddonProfile,
    previous: Vec<String>,
    tf2: Tf2Install,
    titles: Titles,
    win: AnyWindowHandle,
}

/// How a download-choose-install run ended.
enum Outcome {
    Installed(GbAddonEntry),
    Removed { id: GbId, name: String },
    Cancelled,
    Failed(OpError),
}

/// The recorded GameBanana add-ons and the one operation running on them.
pub struct GbAddonInstalls {
    entries: Rc<[GbAddonEntry]>,
    active: Option<(GbId, GbAddonStage)>,
    task: Option<Task<()>>,
    reload_task: Option<Task<()>>,
    check_task: Option<Task<()>>,
    checked_this_session: bool,
}

struct InstallsGlobal(Entity<GbAddonInstalls>);

impl Global for InstallsGlobal {}

impl GbAddonInstalls {
    /// The app-wide instance, created and loaded on first use. Needs `Services` and the
    /// TF2 state.
    pub fn global(cx: &mut App) -> Entity<Self> {
        if let Some(existing) = cx.try_global::<InstallsGlobal>() {
            return existing.0.clone();
        }
        let installs = cx.new(|cx| {
            let mut installs = Self {
                entries: Rc::from([]),
                active: None,
                task: None,
                reload_task: None,
                check_task: None,
                checked_this_session: false,
            };
            installs.reload(cx);
            installs
        });
        cx.set_global(InstallsGlobal(installs.clone()));
        installs
    }

    pub fn entries(&self) -> &[GbAddonEntry] {
        &self.entries
    }

    pub fn entry(&self, id: GbId) -> Option<&GbAddonEntry> {
        self.entries.iter().find(|e| e.gb_id() == id)
    }

    /// The running operation's stage when it works on `id`.
    pub fn stage(&self, id: GbId) -> Option<GbAddonStage> {
        self.active
            .filter(|(active, _)| *active == id)
            .map(|(_, s)| s)
    }

    pub fn is_busy(&self) -> bool {
        self.active.is_some()
    }

    /// Downloads `profile`'s newest file and installs it for `section`, toasting the outcome
    /// in `win`. Returns false, doing nothing, while another operation runs, and false after
    /// a "TF2 not found" toast when no TF2 is set.
    pub fn install(
        &mut self,
        section: AddonSection,
        profile: AddonProfile,
        win: AnyWindowHandle,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(tf2) = self.begin(profile.id(), DOWNLOADING, win, cx) else {
            return false;
        };
        self.task = Some(cx.spawn(async move |this, cx| {
            let run = Run {
                section,
                profile,
                previous: Vec::new(),
                tf2,
                titles: INSTALL,
                win,
            };
            let outcome = fetch_and_install(&this, cx, run).await;
            let _ = this.update(cx, |me, cx| me.finish(outcome, INSTALL, win, cx));
        }));
        cx.notify();
        true
    }

    /// Reads `id`'s profile again, then installs its newest file, keeping the chosen
    /// payloads when they are still offered and asking otherwise. An item gone from
    /// GameBanana is recorded as gone. Returns false, doing nothing, while another operation
    /// runs or when `id` is not recorded, and false after a toast when no TF2 is set.
    pub fn update(&mut self, id: GbId, win: AnyWindowHandle, cx: &mut Context<Self>) -> bool {
        let Some(entry) = self.entry(id).cloned() else {
            return false;
        };
        let Some(tf2) = self.begin(id, DOWNLOADING, win, cx) else {
            return false;
        };
        self.task = Some(cx.spawn(async move |this, cx| {
            let outcome = fetch_and_update(&this, cx, entry, tf2, win).await;
            let _ = this.update(cx, |me, cx| me.finish(outcome, UPDATE, win, cx));
        }));
        cx.notify();
        true
    }

    /// Copies `id`'s library copy back into TF2. Returns false as `update` does.
    pub fn reinstall(&mut self, id: GbId, win: AnyWindowHandle, cx: &mut Context<Self>) -> bool {
        if self.entry(id).is_none() {
            return false;
        }
        let Some(tf2) = self.begin(id, GbAddonStage::Installing, win, cx) else {
            return false;
        };
        let probe = Tf2Probe::get(cx);
        let job = run_blocking(cx, move |s: &Services| {
            s.library
                .reinstall_gb_addon(id, &tf2, &*probe)
                .map_err(|e| gb_install_op(&e, INSTALL.failed))
        });
        self.task = Some(cx.spawn(async move |this, cx| {
            let outcome = match job.await {
                Ok(entry) => Outcome::Installed(entry),
                Err(err) => Outcome::Failed(err),
            };
            let _ = this.update(cx, |me, cx| me.finish(outcome, INSTALL, win, cx));
        }));
        cx.notify();
        true
    }

    /// Removes `id` from TF2 and the library; the caller has asked the player first.
    /// Returns false as `update` does.
    pub fn remove(&mut self, id: GbId, win: AnyWindowHandle, cx: &mut Context<Self>) -> bool {
        let Some(name) = self.entry(id).map(|e| e.name().to_owned()) else {
            return false;
        };
        let Some(tf2) = self.begin(id, GbAddonStage::Removing, win, cx) else {
            return false;
        };
        let probe = Tf2Probe::get(cx);
        let job = run_blocking(cx, move |s: &Services| {
            s.library
                .remove_gb_addon(id, &tf2, &*probe)
                .map_err(|e| gb_install_op(&e, REMOVE.failed))
        });
        self.task = Some(cx.spawn(async move |this, cx| {
            let outcome = match job.await {
                Ok(()) => Outcome::Removed { id, name },
                Err(err) => Outcome::Failed(err),
            };
            let _ = this.update(cx, |me, cx| me.finish(outcome, REMOVE, win, cx));
        }));
        cx.notify();
        true
    }

    /// Runs core's update check once per app session when any add-on is due, then lists the
    /// library again. A failure is logged and the stored update state stays as it was.
    pub fn check_updates(&mut self, cx: &mut Context<Self>) {
        if self.checked_this_session {
            return;
        }
        self.checked_this_session = true;
        // Asks the library first, so a page with nothing due never starts the network runtime.
        let due = run_blocking(cx, |s: &Services| {
            s.library
                .gb_addons_due_for_check(unix_now())
                .map_err(|e| describe_library(&e).into_op("Couldn't list add-ons"))
        });
        self.check_task = Some(cx.spawn(async move |this, cx| {
            if !due.await.is_ok_and(|due| !due.is_empty()) {
                return;
            }
            let job = cx.update(|cx| {
                tokio_bridge::init(cx);
                let store = cx.global::<Services>().store.clone();
                tokio_bridge::spawn(
                    cx,
                    async move { store.check_gb_addon_updates(unix_now()).await },
                )
            });
            match job.await {
                Ok(Ok(_)) => {}
                Ok(Err(err)) => tracing::warn!("add-on update check failed: {err}"),
                Err(join) => tracing::warn!("add-on update check stopped: {join}"),
            }
            let _ = this.update(cx, |me, cx| me.reload(cx));
        }));
    }

    /// Marks `id` busy at `stage` and returns the TF2 install to work on. None, doing nothing,
    /// while another operation runs, and None after a "TF2 not found" toast in `win`.
    fn begin(
        &mut self,
        id: GbId,
        stage: GbAddonStage,
        win: AnyWindowHandle,
        cx: &mut Context<Self>,
    ) -> Option<Tf2Install> {
        if self.is_busy() {
            return None;
        }
        let tf2 = match require_tf2(cx) {
            Ok(tf2) => tf2,
            Err(err) => {
                let _ = win.update(cx, |_, window, cx| toast_error(window, cx, &err));
                return None;
            }
        };
        tokio_bridge::init(cx);
        self.active = Some((id, stage));
        Some(tf2)
    }

    /// Lists the library again off the UI thread (catches hand-deleted files).
    pub fn reload(&mut self, cx: &mut Context<Self>) {
        let tf2 = tf2_state(cx).read(cx).install().cloned();
        let list = run_blocking(cx, move |s: &Services| {
            s.library
                .list_gb_addons(tf2.as_ref())
                .map_err(|e| describe_library(&e).into_op("Couldn't list add-ons"))
        });
        self.reload_task = Some(cx.spawn(async move |this, cx| {
            // A failed listing is already logged; the list on screen stays as it was.
            if let Ok(entries) = list.await {
                let _ = this.update(cx, |me, cx| {
                    me.entries = entries.into();
                    cx.notify();
                });
            }
        }));
    }

    /// Records the outcome before going idle, so the list is current when busy ends; then
    /// toasts in `win` (if still open) and lists the library again.
    fn finish(
        &mut self,
        outcome: Outcome,
        titles: Titles,
        win: AnyWindowHandle,
        cx: &mut Context<Self>,
    ) {
        self.task = None;
        let shown = match outcome {
            Outcome::Installed(entry) => {
                let name = entry.name().to_owned();
                self.put(entry);
                Some((ToastKind::Success, titles.done.to_owned(), name))
            }
            Outcome::Removed { id, name } => {
                self.entries = self
                    .entries
                    .iter()
                    .filter(|e| e.gb_id() != id)
                    .cloned()
                    .collect();
                Some((ToastKind::Success, titles.done.to_owned(), name))
            }
            Outcome::Failed(err) => Some((
                ToastKind::Error,
                err.title().to_owned(),
                err.description().to_owned(),
            )),
            Outcome::Cancelled => None,
        };
        self.active = None;
        if let Some((kind, title, description)) = shown {
            let _ = win.update(cx, |_, window, cx| {
                toast(window, cx, kind, title, description)
            });
            if kind == ToastKind::Success {
                LibraryEvents::changed(cx);
            }
        }
        self.reload(cx);
        cx.notify();
    }

    /// Replaces the entry with `entry`'s id, or adds it.
    fn put(&mut self, entry: GbAddonEntry) {
        let mut entries: Vec<GbAddonEntry> = self.entries.to_vec();
        match entries.iter_mut().find(|e| e.gb_id() == entry.gb_id()) {
            Some(slot) => *slot = entry,
            None => entries.push(entry),
        }
        self.entries = entries.into();
    }

    fn set_stage(&mut self, stage: GbAddonStage, cx: &mut Context<Self>) {
        if let Some((_, current)) = &mut self.active {
            *current = stage;
            cx.notify();
        }
    }
}

/// The stage an operation starts in when it downloads first.
const DOWNLOADING: GbAddonStage = GbAddonStage::Downloading {
    downloaded: 0,
    total: None,
};

/// The current time in Unix seconds, as core's update records take it.
fn unix_now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_secs()).unwrap_or(i64::MAX))
}

/// Reads `entry`'s profile again and runs the install with its payloads as the previous
/// choice. An item GameBanana reports gone is recorded so, and `this` lists it as gone
/// before the failure is returned.
async fn fetch_and_update(
    this: &WeakEntity<GbAddonInstalls>,
    cx: &mut AsyncApp,
    entry: GbAddonEntry,
    tf2: Tf2Install,
    win: AnyWindowHandle,
) -> Outcome {
    let id = entry.gb_id();
    let store = cx.update(|cx| cx.global::<Services>().store.clone());
    let job = cx.update(|cx| {
        tokio_bridge::spawn(cx, async move { store.addon_catalog().profile(id).await })
    });
    let (failure, gone) = match job.await {
        Ok(Ok(profile)) => {
            let Some(section) = AddonSection::by_key(entry.section()) else {
                // Only a tampered row names a section this build does not know.
                tracing::warn!("add-on {id} has an unknown section {:?}", entry.section());
                return Outcome::Failed(OpError::core(UPDATE.failed, UPDATE_UNKNOWN_SECTION));
            };
            let run = Run {
                section,
                profile,
                previous: entry.payloads().to_vec(),
                tf2,
                titles: UPDATE,
                win,
            };
            return fetch_and_install(this, cx, run).await;
        }
        Ok(Err(err)) => (
            describe_browse(&err),
            matches!(err, BrowseError::Gone { .. }),
        ),
        Err(join) => (download_stopped(&join.to_string()), false),
    };
    failure.log("add-on update");
    if gone {
        mark_gone(this, cx, id, tf2).await;
    }
    Outcome::Failed(OpError::core(UPDATE.failed, failure.message))
}

/// The player words for an update whose recorded section this build does not know.
const UPDATE_UNKNOWN_SECTION: &str = "TF2Tools can't tell where this add-on goes. Remove it, then install it again from the Addon Store.";

/// Records `id` as gone from GameBanana and puts the listed result into `this`, so the
/// list shows it before busy ends. A failed write is logged and the list stays as it was.
async fn mark_gone(
    this: &WeakEntity<GbAddonInstalls>,
    cx: &mut AsyncApp,
    id: GbId,
    tf2: Tf2Install,
) {
    let job = cx.update(|cx| {
        run_blocking(cx, move |s: &Services| {
            let fail = |e: &tf2tools_core::LibraryError| {
                describe_library(e).into_op("Couldn't record the add-on as gone")
            };
            s.library
                .record_gb_addon_check(id, GbAddonCheck::Gone, unix_now())
                .map_err(|e| fail(&e))?;
            s.library.list_gb_addons(Some(&tf2)).map_err(|e| fail(&e))
        })
    });
    if let Ok(entries) = job.await {
        let _ = this.update(cx, |me, _| me.entries = entries.into());
    }
}

/// Downloads `profile`'s newest file, picks its payloads (keeping `previous` when it still
/// fits, else asking in `win`) and installs them, reporting each stage on `this`.
async fn fetch_and_install(
    this: &WeakEntity<GbAddonInstalls>,
    cx: &mut AsyncApp,
    run: Run,
) -> Outcome {
    let Run {
        section,
        profile,
        previous,
        tf2,
        titles,
        win,
    } = run;
    // The progress callback runs on a tokio worker, so it only sends; the channel closes
    // when the download future ends.
    let (tx, rx) = async_channel::unbounded::<DownloadProgress>();
    let store = cx.update(|cx| cx.global::<Services>().store.clone());
    let job = cx.update(|cx| {
        tokio_bridge::spawn(cx, async move {
            let report = move |p: DownloadProgress| {
                let _ = tx.try_send(p);
            };
            store.download_gb_addon(section, &profile, &report).await
        })
    });
    while let Ok(p) = rx.recv().await {
        let stage = GbAddonStage::Downloading {
            downloaded: p.downloaded(),
            total: p.total(),
        };
        let _ = this.update(cx, |me, cx| me.set_stage(stage, cx));
    }
    let failure = match job.await {
        Ok(Ok(download)) => Ok(download),
        Ok(Err(err)) => Err(describe_gb_fetch(&err)),
        Err(join) => Err(download_stopped(&join.to_string())),
    };
    let download = match failure {
        Ok(download) => download,
        Err(failure) => {
            failure.log("add-on download");
            return Outcome::Failed(OpError::core(titles.failed, failure.message));
        }
    };

    let chosen = match download.kept_payloads(&previous) {
        Some(kept) => kept,
        None => {
            let _ = this.update(cx, |me, cx| me.set_stage(GbAddonStage::Choosing, cx));
            match ask_payloads(&download, win, cx).await {
                Some(chosen) => chosen,
                None => {
                    // Dropping the download deletes its extracted files, which for a big
                    // pack is slow disk work that must stay off the UI thread.
                    cx.background_spawn(async move { drop(download) }).detach();
                    return Outcome::Cancelled;
                }
            }
        }
    };

    let _ = this.update(cx, |me, cx| me.set_stage(GbAddonStage::Installing, cx));
    let install = cx.update(|cx| {
        let probe = Tf2Probe::get(cx);
        run_blocking(cx, move |s: &Services| {
            s.library
                .install_gb_addon(download, &chosen, &tf2, &*probe)
                .map_err(|e| gb_install_op(&e, titles.failed))
        })
    });
    match install.await {
        Ok(entry) => Outcome::Installed(entry),
        Err(err) => Outcome::Failed(err),
    }
}

/// Asks in `win` which of `download`'s payloads to install. None when the player backs out,
/// the dialog goes away or `win` is closed.
async fn ask_payloads(
    download: &GbAddonDownload,
    win: AnyWindowHandle,
    cx: &mut AsyncApp,
) -> Option<Vec<String>> {
    let items = download
        .payloads()
        .iter()
        .map(|p| ChecklistItem {
            name: p.name().to_owned(),
            label: p.label().to_owned(),
        })
        .collect();
    let name = download.name().to_owned();
    // The checklist entity holds its own sender, so its handle is dropped before waiting:
    // a dialog that goes away without answering then closes the channel instead of hanging.
    let answer = win
        .update(cx, |_, window, cx| {
            open_payload_checklist(&name, items, window, cx).1
        })
        .ok()?;
    answer.recv().await.ok().flatten()
}

/// Why `profile`'s newest file can't be installed, in player words; None when it can.
/// These are the refusals core makes before downloading, worded as its errors are.
pub fn install_blocker(profile: &AddonProfile) -> Option<String> {
    let err = match profile.files().first() {
        None => GbAddonFetchError::NoFiles {
            gb_id: profile.id(),
        },
        Some(file) if file.av_result() != "clean" => GbAddonFetchError::VirusScan {
            file_name: file.name().to_owned(),
            result: file.av_result().to_owned(),
        },
        Some(file) => {
            let limit = StoreConfig::default().gb_addon_size_cap();
            if !file.size().is_some_and(|size| size > limit) {
                return None;
            }
            GbAddonFetchError::TooLarge {
                url: file.download_url().to_owned(),
                limit,
            }
        }
    };
    Some(describe_gb_fetch(&err).message)
}
