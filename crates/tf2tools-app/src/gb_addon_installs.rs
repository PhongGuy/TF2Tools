//! GameBanana add-on installs: the installed list and the one running operation, as an app
//! global so leaving a page never drops (and so aborts) an install.

use std::rc::Rc;

use gpui_kit::{
    AnyWindowHandle, App, AppContext as _, AsyncApp, Context, Entity, Global, Task, WeakEntity,
};
use tf2tools_core::{
    AddonProfile, AddonSection, DownloadProgress, GbAddonDownload, GbAddonEntry, GbAddonFetchError,
    GbId, StoreConfig, Tf2Install,
};

use crate::library_events::LibraryEvents;
use crate::local_messages::{describe_library, gb_install_op};
use crate::pages::profiles::probe::Tf2Probe;
use crate::payload_checklist::{ChecklistItem, open_payload_checklist};
use crate::services::{OpError, Services, require_tf2, run_blocking, tf2_state};
use crate::shell::toast::{ToastKind, toast, toast_error};
use crate::store_messages::{describe_gb_fetch, download_stopped};
use crate::tokio_bridge;

/// What the running operation is doing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GbAddonStage {
    Downloading { downloaded: u64, total: Option<u64> },
    Choosing,
    Installing,
    Removing,
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
    Cancelled,
    Failed(OpError),
}

/// The recorded GameBanana add-ons and the one operation running on them.
pub struct GbAddonInstalls {
    entries: Rc<[GbAddonEntry]>,
    active: Option<(GbId, GbAddonStage)>,
    task: Option<Task<()>>,
    reload_task: Option<Task<()>>,
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
        if self.is_busy() {
            return false;
        }
        let tf2 = match require_tf2(cx) {
            Ok(tf2) => tf2,
            Err(err) => {
                let _ = win.update(cx, |_, window, cx| toast_error(window, cx, &err));
                return false;
            }
        };
        if !tokio_bridge::is_running(cx) {
            tokio_bridge::init(cx);
        }
        let stage = GbAddonStage::Downloading {
            downloaded: 0,
            total: None,
        };
        self.active = Some((profile.id(), stage));
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

    /// Lists the library again off the UI thread (catches hand-deleted files).
    pub fn reload(&mut self, cx: &mut Context<Self>) {
        let tf2 = tf2_state(cx).read(cx).install().cloned();
        let list = run_blocking(cx, move |s: &Services| {
            s.library
                .list_gb_addons(tf2.as_ref())
                .map_err(|e| describe_library(&e).into_op("Couldn't list add-ons"))
        });
        // A failed listing is already logged; the list on screen stays as it was.
        self.reload_task = Some(cx.spawn(async move |this, cx| {
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
                None => return Outcome::Cancelled,
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
