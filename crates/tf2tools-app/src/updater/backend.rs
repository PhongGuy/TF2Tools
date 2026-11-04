//! The updater's backend over core's `UpdateClient`: its deadlines and the mapping from core
//! errors to player messages. The messages never carry URLs, paths or codes; those go to the log.

use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use tf2tools_core::{
    Applied, InstallTarget, StagedUpdate, UpdateApplyError, UpdateCheckError, UpdateClient,
    UpdateDownloadError, UpdateOffer, apply,
};

use super::{
    AfterInstall, CheckFuture, DownloadFuture, DownloadSink, Failure, Offer, UpdateBackend,
};

const OFFLINE: &str =
    "Couldn't reach the update server. Check your internet connection and try again.";
const UNREADABLE: &str = "The update information couldn't be read. Try again later.";
const UNVERIFIED: &str = "Update check failed: the update could not be verified. Try again later.";
pub(super) const UNAVAILABLE: &str =
    "Automatic updates aren't available in this version. Get new versions from the release page.";
const STOPPED: &str = "The download stopped. Check your internet connection and try again.";
const UNSERVED: &str = "The update couldn't be downloaded right now. Try again later, or get it from the release page.";
// A server error is usually brief, so the player tries again in the app.
const BUSY: &str = "The update server is busy right now. Try again in a few minutes.";
const DAMAGED: &str = "The download was damaged. Try again.";
const DISK: &str = "Couldn't save the update. Make sure your disk has free space, then try again.";
const USE_PAGE: &str = "TF2Tools can't update itself where it's installed. Download the new version from the release page instead.";
const INSTALL_IO: &str =
    "Couldn't install the update. Try again, or download the new version from the release page.";
const INSTALLER: &str =
    "Couldn't start the installer. Try again, or download the new version from the release page.";
// A fresh download replaces the staged installer or AppImage, so trying again can help.
const CHANGED: &str = "The update changed before it could be installed. Try again.";
pub(super) const UNKNOWN: &str = "Something went wrong with the update. Try again later.";
const CHECK_SLOW: &str =
    "Checking for updates took too long. Check your internet connection and try again.";
const DOWNLOAD_SLOW: &str =
    "The download took too long. Check your internet connection and try again.";
const CHECK_FIRST: &str = "Check for updates first.";
const DOWNLOAD_FIRST: &str = "Download the update first.";

/// Total time allowed for one check and one download, on top of core's per-read timeouts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Deadlines {
    pub check: Duration,
    pub download: Duration,
}

impl Default for Deadlines {
    fn default() -> Self {
        Self {
            check: Duration::from_secs(30),
            download: Duration::from_secs(20 * 60),
        }
    }
}

fn says(message: &str, use_release_page: bool) -> Failure {
    Failure {
        message: message.to_owned(),
        use_release_page,
    }
}

/// A failed update check in player words. A 404 reads as "couldn't be read" on purpose,
/// since the manifest is missing for everyone before the first release.
pub fn describe_check(err: &UpdateCheckError) -> Failure {
    match err {
        UpdateCheckError::Network { .. } => says(OFFLINE, false),
        UpdateCheckError::Status { .. }
        | UpdateCheckError::TooLarge { .. }
        | UpdateCheckError::Malformed { .. } => says(UNREADABLE, false),
        UpdateCheckError::BadSignature => says(UNVERIFIED, false),
        UpdateCheckError::BadKey { .. } => says(UNAVAILABLE, true),
        _ => says(UNKNOWN, false),
    }
}

/// A failed download in player words; installs that cannot update in place go to the release page.
pub fn describe_download(err: &UpdateDownloadError) -> Failure {
    match err {
        UpdateDownloadError::Network { .. } => says(STOPPED, false),
        UpdateDownloadError::Status {
            status: 500..=599, ..
        } => says(BUSY, false),
        // The server could not find the download, so the release page is the way.
        UpdateDownloadError::Status { .. } => says(UNSERVED, true),
        UpdateDownloadError::SizeMismatch { .. } | UpdateDownloadError::HashMismatch { .. } => {
            says(DAMAGED, false)
        }
        UpdateDownloadError::Io { .. } => says(DISK, false),
        UpdateDownloadError::NotWritable { .. }
        | UpdateDownloadError::NoAsset
        | UpdateDownloadError::Unsupported => says(USE_PAGE, true),
        _ => says(UNKNOWN, false),
    }
}

/// A failed install in player words; installs that cannot update in place go to the release page.
pub fn describe_apply(err: &UpdateApplyError) -> Failure {
    match err {
        UpdateApplyError::NotWritable { .. } | UpdateApplyError::Unsupported { .. } => {
            says(USE_PAGE, true)
        }
        UpdateApplyError::Io { .. } => says(INSTALL_IO, false),
        UpdateApplyError::Spawn { .. } => says(INSTALLER, false),
        UpdateApplyError::Changed { .. } => says(CHANGED, false),
        _ => says(UNKNOWN, false),
    }
}

/// The backend over core's client for `target`, offering versions newer than `current`; every
/// core error is logged at `warn` and mapped through `describe_*`.
pub fn core_backend(
    client: UpdateClient,
    target: InstallTarget,
    current: &str,
    deadlines: Deadlines,
) -> UpdateBackend {
    // The newest offer and the staged download live between calls: a check replaces the
    // offer, a download stages it, and apply takes the staged file so it applies once.
    let client = Arc::new(client);
    let target = Arc::new(target);
    let current: Arc<str> = Arc::from(current);
    let offer: Arc<Mutex<Option<UpdateOffer>>> = Arc::default();
    let staged: Arc<Mutex<Option<StagedUpdate>>> = Arc::default();

    // The timeout is built inside each future, since the closures run outside any runtime.
    let check = {
        let (client, target, offer) = (client.clone(), target.clone(), offer.clone());
        Arc::new(move || -> CheckFuture {
            let (client, target, offer) = (client.clone(), target.clone(), offer.clone());
            let current = current.clone();
            Box::pin(async move {
                let found =
                    match tokio::time::timeout(deadlines.check, client.check(&current, &target))
                        .await
                    {
                        Err(_) => {
                            tracing::warn!(deadline = ?deadlines.check, "update check timed out");
                            return Err(says(CHECK_SLOW, false));
                        }
                        Ok(Err(err)) => {
                            tracing::warn!(error = %err, "update check failed");
                            return Err(describe_check(&err));
                        }
                        Ok(Ok(found)) => found,
                    };
                let shown = found.as_ref().map(|o| Offer {
                    version: o.version().to_owned(),
                    release_page: o.release_page().to_owned(),
                    installable: o.asset().is_some(),
                });
                *lock(&offer) = found;
                Ok(shown)
            })
        })
    };

    // A timed-out download drops core's future, whose part-file guard deletes the part file.
    let download = {
        let staged = staged.clone();
        Arc::new(move |sink: DownloadSink| -> DownloadFuture {
            let (client, target, staged) = (client.clone(), target.clone(), staged.clone());
            let wanted = lock(&offer).clone();
            Box::pin(async move {
                let Some(wanted) = wanted else {
                    return Err(says(CHECK_FIRST, false));
                };
                let fetch = client.download(&wanted, &target, |received, total| {
                    sink.report(received, total)
                });
                match tokio::time::timeout(deadlines.download, fetch).await {
                    Err(_) => {
                        tracing::warn!(deadline = ?deadlines.download, "update download timed out");
                        Err(says(DOWNLOAD_SLOW, false))
                    }
                    Ok(Err(err)) => {
                        tracing::warn!(error = %err, "update download failed");
                        Err(describe_download(&err))
                    }
                    Ok(Ok(update)) => {
                        *lock(&staged) = Some(update);
                        Ok(())
                    }
                }
            })
        })
    };

    let apply_staged = Arc::new(move || {
        let Some(update) = lock(&staged).take() else {
            return Err(says(DOWNLOAD_FIRST, false));
        };
        match apply(update) {
            Ok(Applied::Replaced { relaunch }) => Ok(AfterInstall::Relaunch(relaunch)),
            Ok(Applied::InstallerStarted) => Ok(AfterInstall::Quit),
            Ok(other) => {
                tracing::warn!(applied = ?other, "update applied in an unexpected way");
                Err(says(UNKNOWN, false))
            }
            Err(err) => {
                tracing::warn!(error = %err, "update install failed");
                Err(describe_apply(&err))
            }
        }
    });

    UpdateBackend {
        check,
        download,
        apply: apply_staged,
    }
}

/// The slot's value; a panic elsewhere while holding it leaves a value still worth reading.
fn lock<T>(slot: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    slot.lock().unwrap_or_else(PoisonError::into_inner)
}
