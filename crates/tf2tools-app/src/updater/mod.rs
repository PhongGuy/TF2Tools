//! The app's self-updater: the types shared by the update state, its backend and its views.
//! Everything slow runs behind `UpdateBackend` on the tokio bridge; the UI only reads state.

pub mod backend;
pub mod keys;
pub mod view;

use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use gpui_kit::{App, AppContext, Context, Entity, EventEmitter, Global, Task};
use tf2tools_core::{LogLevel, NewLogEntry};
use tokio::sync::watch;

use crate::services::Services;
use crate::sync::REPAINT_INTERVAL;
use crate::{activity, tokio_bridge};

/// An update as the app shows it. `installable` is false when this copy can only open the release page.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Offer {
    pub version: String,
    pub release_page: String,
    pub installable: bool,
}

/// A failure in player words: what happened and what to do next. `use_release_page` means
/// trying again in the app will not help.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Failure {
    pub message: String,
    pub use_release_page: bool,
}

/// Download progress as a whole percent; the updater shows the newest value.
pub struct DownloadSink(watch::Sender<u8>);

impl DownloadSink {
    /// A sink and the receiver the updater watches, starting at 0 percent.
    pub fn new() -> (DownloadSink, watch::Receiver<u8>) {
        let (sender, receiver) = watch::channel(0);
        (DownloadSink(sender), receiver)
    }

    /// `received * 100 / total`, at most 100; 0 when `total` is 0.
    pub fn report(&self, received: u64, total: u64) {
        let percent = received
            .saturating_mul(100)
            .checked_div(total)
            .unwrap_or(0)
            .min(100);
        // send_replace never fails, so progress after the updater stopped watching is dropped quietly.
        self.0.send_replace(u8::try_from(percent).unwrap_or(100));
    }
}

pub type CheckFuture = Pin<Box<dyn Future<Output = Result<Option<Offer>, Failure>> + Send>>;
pub type DownloadFuture = Pin<Box<dyn Future<Output = Result<(), Failure>> + Send>>;

/// What the app does once an update is applied: relaunch the replaced file, or quit so the
/// running installer can replace it (the installer relaunches the new version itself).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AfterInstall {
    Relaunch(PathBuf),
    Quit,
}

/// What the updater asks of the network and disk. The futures run on tokio; `apply` blocks
/// and says what the app does next.
#[derive(Clone)]
pub struct UpdateBackend {
    pub check: Arc<dyn Fn() -> CheckFuture + Send + Sync>,
    pub download: Arc<dyn Fn(DownloadSink) -> DownloadFuture + Send + Sync>,
    pub apply: Arc<dyn Fn() -> Result<AfterInstall, Failure> + Send + Sync>,
}

/// What the updater starts from: no backend means updates are unavailable in this build.
pub struct UpdateSource {
    pub backend: Option<UpdateBackend>,
    pub current: String,
    pub releases_page: String,
}

/// Where startup gets its update source: `Environment` (the real app: `source_from_environment`),
/// `Off` (no backend: Unavailable), or a ready `Source` (tests).
pub enum UpdateBoot {
    Environment,
    Off,
    Source(UpdateSource),
}

/// What the player reads when the update is installed but the app did not relaunch.
pub const REOPEN: &str = "TF2Tools has been updated. Close and reopen it to finish.";

/// How long the app may keep running after the Restart hook before it asks the player to reopen it.
pub const RESTART_GRACE: Duration = Duration::from_secs(10);

/// Relaunches or quits as `AfterInstall` says; tests record the call instead.
pub type Restart = Rc<dyn Fn(AfterInstall, &mut App)>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UpdateState {
    Unavailable,
    Idle,
    Checking,
    UpToDate,
    Available(Offer),
    Downloading {
        offer: Offer,
        percent: u8,
    },
    Ready(Offer),
    Installing(Offer),
    /// Installed, but the relaunch never quit the app: the player closes and reopens it (`REOPEN`).
    Updated(Offer),
    Failed(Failure),
}

impl UpdateState {
    fn offer(&self) -> Option<&Offer> {
        match self {
            UpdateState::Available(offer)
            | UpdateState::Downloading { offer, .. }
            | UpdateState::Ready(offer)
            | UpdateState::Installing(offer)
            | UpdateState::Updated(offer) => Some(offer),
            _ => None,
        }
    }
}

/// A download or install that failed after the player asked for it; the shell toasts it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UpdateEvent {
    DownloadFailed(Failure),
    InstallFailed(Failure),
}

/// The app's update state machine over its backend. Dropping it drops the running task,
/// which cancels the tokio job.
pub struct Updater {
    backend: Option<UpdateBackend>,
    current: String,
    releases_page: String,
    restart: Restart,
    restart_grace: Duration,
    state: UpdateState,
    /// The version whose banner the player closed this session.
    dismissed: Option<String>,
    task: Option<Task<()>>,
}

impl EventEmitter<UpdateEvent> for Updater {}

struct UpdaterGlobal(Entity<Updater>);
impl Global for UpdaterGlobal {}

impl Updater {
    /// Creates the updater (Idle, or Unavailable without a backend) and makes it the app's.
    pub fn install(cx: &mut App, source: UpdateSource, restart: Restart) -> Entity<Updater> {
        let state = match source.backend {
            Some(_) => UpdateState::Idle,
            None => UpdateState::Unavailable,
        };
        let updater = cx.new(|_| Updater {
            backend: source.backend,
            current: source.current,
            releases_page: source.releases_page,
            restart,
            restart_grace: RESTART_GRACE,
            state,
            dismissed: None,
            task: None,
        });
        cx.set_global(UpdaterGlobal(updater.clone()));
        updater
    }

    pub fn state(&self) -> &UpdateState {
        &self.state
    }

    pub fn current(&self) -> &str {
        &self.current
    }

    /// Replaces `RESTART_GRACE`; tests shorten it.
    pub fn set_restart_grace(&mut self, grace: Duration) {
        self.restart_grace = grace;
    }

    /// Starts a background check, starting the tokio bridge if needed; false when ignored.
    pub fn check(&mut self, cx: &mut Context<Self>) -> bool {
        // A staged or running download would be forgotten by a new offer, and an installed
        // one would be offered again, so those states and builds without a backend ignore it.
        let Some(backend) = self.backend.as_ref() else {
            return false;
        };
        if matches!(
            self.state,
            UpdateState::Checking
                | UpdateState::Downloading { .. }
                | UpdateState::Ready(_)
                | UpdateState::Installing(_)
                | UpdateState::Updated(_)
        ) {
            return false;
        }
        if !tokio_bridge::is_running(cx) {
            tokio_bridge::init(cx);
        }
        let job = tokio_bridge::spawn(cx, (backend.check)());
        self.task = Some(cx.spawn(async move |this, cx| {
            let outcome = job.await.unwrap_or_else(|e| stopped("update check", &e));
            let _ = this.update(cx, |u, cx| u.finish_check(outcome, cx));
        }));
        self.state = UpdateState::Checking;
        cx.notify();
        true
    }

    /// From Available with an installable offer only; false otherwise.
    pub fn download(&mut self, cx: &mut Context<Self>) -> bool {
        let (Some(backend), UpdateState::Available(offer)) = (self.backend.as_ref(), &self.state)
        else {
            return false;
        };
        if !offer.installable {
            return false;
        }
        let offer = offer.clone();
        if !tokio_bridge::is_running(cx) {
            tokio_bridge::init(cx);
        }
        let (sink, mut progress) = DownloadSink::new();
        let job = tokio_bridge::spawn(cx, (backend.download)(sink));
        // Publish the newest percent, then hold off one interval: at most ten repaints a
        // second. The sink drops with the download future, which ends the loop.
        self.task = Some(cx.spawn(async move |this, cx| {
            while progress.changed().await.is_ok() {
                let percent = *progress.borrow_and_update();
                if this.update(cx, |u, cx| u.set_percent(percent, cx)).is_err() {
                    return;
                }
                cx.background_executor().timer(REPAINT_INTERVAL).await;
            }
            let outcome = job.await.unwrap_or_else(|e| stopped("update download", &e));
            let _ = this.update(cx, |u, cx| u.finish_download(outcome, cx));
        }));
        self.state = UpdateState::Downloading { offer, percent: 0 };
        cx.notify();
        true
    }

    /// From Ready only: applies off the UI thread, logs, then calls the Restart hook.
    pub fn restart(&mut self, cx: &mut Context<Self>) -> bool {
        let (Some(backend), UpdateState::Ready(offer)) = (self.backend.as_ref(), &self.state)
        else {
            return false;
        };
        let offer = offer.clone();
        let apply = Arc::clone(&backend.apply);
        let services = cx.try_global::<Services>().cloned();
        let version = offer.version.clone();
        if !tokio_bridge::is_running(cx) {
            tokio_bridge::init(cx);
        }
        // The install row is written inside the blocking job, before the relaunch, so a
        // quit right after cannot drop it.
        let job = tokio_bridge::spawn(cx, async move {
            // Tokio catches this closure's panic as a JoinError, so it is no crash.
            tokio::task::spawn_blocking(move || {
                crate::crash::marked(move || {
                    let after = apply()?;
                    let installed = match after {
                        AfterInstall::Relaunch(_) => format!("Installed TF2Tools v{version}"),
                        AfterInstall::Quit => format!("Started the TF2Tools v{version} installer"),
                    };
                    match &services {
                        Some(services) => activity::write(
                            services,
                            &NewLogEntry::new(
                                LogLevel::Info,
                                "settings",
                                "update-install",
                                installed,
                            ),
                        ),
                        None => tracing::warn!("no services installed; dropping the install row"),
                    }
                    Ok(after)
                })
            })
            .await
        });
        self.task = Some(cx.spawn(async move |this, cx| {
            let outcome = match job.await {
                Ok(Ok(outcome)) => outcome,
                Ok(Err(e)) | Err(e) => stopped("update install", &e),
            };
            let _ = this.update(cx, |u, cx| u.finish_install(outcome, cx));
        }));
        self.state = UpdateState::Installing(offer);
        cx.notify();
        true
    }

    /// Opens the offer's release page, or `releases_page` when there is no offer.
    pub fn open_release_page(&self, cx: &App) {
        let page = self
            .state
            .offer()
            .map_or(self.releases_page.as_str(), |o| o.release_page.as_str());
        cx.open_url(page);
    }

    /// Hides the banner for this session; ignored while Downloading or Installing.
    pub fn dismiss(&mut self, cx: &mut Context<Self>) {
        if matches!(
            self.state,
            UpdateState::Downloading { .. } | UpdateState::Installing(_)
        ) {
            return;
        }
        if let Some(offer) = self.state.offer() {
            self.dismissed = Some(offer.version.clone());
            cx.notify();
        }
    }

    /// Available, Downloading, Ready, Installing or Updated, and not dismissed.
    pub fn banner_visible(&self) -> bool {
        self.state
            .offer()
            .is_some_and(|o| self.dismissed.as_deref() != Some(o.version.as_str()))
    }

    // A failed check stays in Settings: it is logged, never toasted.
    fn finish_check(&mut self, outcome: Result<Option<Offer>, Failure>, cx: &mut Context<Self>) {
        self.state = match outcome {
            Ok(Some(offer)) => UpdateState::Available(offer),
            Ok(None) => UpdateState::UpToDate,
            Err(failure) => {
                tracing::warn!(message = %failure.message, "update check failed");
                UpdateState::Failed(failure)
            }
        };
        self.task = None;
        cx.notify();
    }

    fn set_percent(&mut self, value: u8, cx: &mut Context<Self>) {
        if let UpdateState::Downloading { percent, .. } = &mut self.state {
            *percent = value;
            cx.notify();
        }
    }

    fn finish_download(&mut self, outcome: Result<(), Failure>, cx: &mut Context<Self>) {
        let Some(offer) = self.state.offer().cloned() else {
            return;
        };
        self.task = None;
        match outcome {
            Ok(()) => {
                let message = format!("Downloaded TF2Tools v{}", offer.version);
                activity::record(
                    cx,
                    NewLogEntry::new(LogLevel::Info, "settings", "update-download", message),
                );
                self.state = UpdateState::Ready(offer);
                cx.notify();
            }
            Err(failure) => self.fail(
                offer,
                ("update-download", UpdateEvent::DownloadFailed),
                failure,
                cx,
            ),
        }
    }

    fn finish_install(&mut self, outcome: Result<AfterInstall, Failure>, cx: &mut Context<Self>) {
        let Some(offer) = self.state.offer().cloned() else {
            return;
        };
        self.task = None;
        match outcome {
            // The hook normally quits the app. gpui only logs when it cannot spawn its relaunch
            // helper, so an app still running after the grace asks the player to reopen it.
            Ok(after) => {
                Rc::clone(&self.restart)(after, cx);
                let grace = self.restart_grace;
                self.task = Some(cx.spawn(async move |this, cx| {
                    cx.background_executor().timer(grace).await;
                    let _ = this.update(cx, |u, cx| u.restart_stalled(cx));
                }));
            }
            Err(failure) => self.fail(
                offer,
                ("update-install", UpdateEvent::InstallFailed),
                failure,
                cx,
            ),
        }
    }

    fn restart_stalled(&mut self, cx: &mut Context<Self>) {
        let UpdateState::Installing(offer) = &self.state else {
            return;
        };
        tracing::warn!("the app is still running after the update relaunch");
        activity::record(
            cx,
            NewLogEntry::new(LogLevel::Warning, "settings", "update-restart", REOPEN),
        );
        self.state = UpdateState::Updated(offer.clone());
        self.task = None;
        cx.notify();
    }

    // Back to Available, offering only the release page when retrying in the app cannot help.
    fn fail(
        &mut self,
        mut offer: Offer,
        (action, event): (&str, fn(Failure) -> UpdateEvent),
        failure: Failure,
        cx: &mut Context<Self>,
    ) {
        tracing::warn!(message = %failure.message, action, "update failed");
        offer.installable &= !failure.use_release_page;
        activity::record(
            cx,
            NewLogEntry::new(LogLevel::Error, "settings", action, failure.message.clone()),
        );
        self.state = UpdateState::Available(offer);
        cx.emit(event(failure));
        cx.notify();
    }
}

// A tokio job that panicked or was cancelled, in player words.
fn stopped<T>(what: &str, error: &tokio_bridge::JoinError) -> Result<T, Failure> {
    tracing::warn!(%error, "{what} stopped");
    Err(Failure {
        message: backend::UNKNOWN.to_owned(),
        use_release_page: false,
    })
}

/// The app's updater, if one was installed.
pub fn updater(cx: &App) -> Option<Entity<Updater>> {
    cx.try_global::<UpdaterGlobal>().map(|g| g.0.clone())
}

/// Relaunches into the installed update, or quits so the running installer can replace the app.
pub fn system_restart() -> Restart {
    Rc::new(|after, cx| match after {
        AfterInstall::Relaunch(path) => {
            cx.set_restart_path(path);
            cx.restart();
        }
        AfterInstall::Quit => cx.quit(),
    })
}

#[cfg(test)]
mod tests {
    use std::rc::Rc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use gpui_kit::TestAppContext;

    use super::*;

    // Ready is reachable only through a download, so the busy states are set directly here
    // to pin that a check never replaces an offer whose download is staged or running.
    #[gpui_kit::test]
    fn a_check_is_ignored_while_an_update_is_staged_or_busy(cx: &mut TestAppContext) {
        let checks = Arc::new(AtomicUsize::new(0));
        let counted = Arc::clone(&checks);
        let backend = UpdateBackend {
            check: Arc::new(move || -> CheckFuture {
                counted.fetch_add(1, Ordering::SeqCst);
                Box::pin(async { Ok(None) })
            }),
            download: Arc::new(|_| -> DownloadFuture { Box::pin(async { Ok(()) }) }),
            apply: Arc::new(|| {
                Ok(AfterInstall::Relaunch(PathBuf::from(
                    "/apps/tf2tools.AppImage",
                )))
            }),
        };
        let source = UpdateSource {
            backend: Some(backend),
            current: "6.0.0".to_owned(),
            releases_page: "https://example.invalid/releases".to_owned(),
        };
        let u = cx.update(|cx| Updater::install(cx, source, Rc::new(|_, _| {})));
        let offer = Offer {
            version: "6.0.1".to_owned(),
            release_page: "https://example.invalid/v6.0.1".to_owned(),
            installable: true,
        };
        for busy in [
            UpdateState::Ready(offer.clone()),
            UpdateState::Downloading {
                offer: offer.clone(),
                percent: 40,
            },
            UpdateState::Installing(offer.clone()),
        ] {
            cx.update(|cx| u.update(cx, |u, _| u.state = busy.clone()));
            assert!(
                !cx.update(|cx| u.update(cx, |u, cx| u.check(cx))),
                "{busy:?}"
            );
            assert_eq!(cx.update(|cx| u.read(cx).state().clone()), busy);
        }
        cx.run_until_parked();
        assert_eq!(
            checks.load(Ordering::SeqCst),
            0,
            "an ignored check reached the backend"
        );
    }
}
