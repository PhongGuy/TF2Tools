//! The updater's backend over core's `UpdateClient`: its deadlines and the mapping from core
//! errors to player messages. The messages never carry URLs, paths or codes; those go to the log.

use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use gpui_kit::SharedString;
use tf2tools_core::{
    Applied, InstallTarget, StagedUpdate, UpdateApplyError, UpdateCheckError, UpdateClient,
    UpdateDownloadError, UpdateOffer, apply,
};

use super::{
    AfterInstall, CheckFuture, DownloadFuture, DownloadSink, Failure, Offer, UpdateBackend,
};
use crate::i18n::tr;

/// The release page is the way to update when this build cannot update itself.
pub(super) fn unavailable() -> SharedString {
    tr!("updater.status.unavailable")
}

/// The catch-all for an update failure nothing else describes.
pub(super) fn unknown() -> SharedString {
    tr!("updater.failure.unknown")
}

/// The title of the toast that reports a failed download or install.
pub fn failure_title() -> SharedString {
    tr!("updater.failure.title")
}

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

fn says(message: SharedString, use_release_page: bool) -> Failure {
    Failure {
        message,
        use_release_page,
    }
}

/// A failed update check in player words. A 404 reads as "couldn't be read" on purpose,
/// since the manifest is missing for everyone before the first release.
pub fn describe_check(err: &UpdateCheckError) -> Failure {
    match err {
        UpdateCheckError::Network { .. } => says(tr!("updater.failure.offline"), false),
        UpdateCheckError::Status { .. }
        | UpdateCheckError::TooLarge { .. }
        | UpdateCheckError::Malformed { .. } => says(tr!("updater.failure.unreadable"), false),
        UpdateCheckError::BadSignature => says(tr!("updater.failure.unverified"), false),
        UpdateCheckError::BadKey { .. } => says(unavailable(), true),
        _ => says(unknown(), false),
    }
}

/// A failed download in player words; installs that cannot update in place go to the release page.
pub fn describe_download(err: &UpdateDownloadError) -> Failure {
    match err {
        UpdateDownloadError::Network { .. } => says(tr!("updater.failure.stopped"), false),
        // A server error is usually brief, so the player tries again in the app.
        UpdateDownloadError::Status {
            status: 500..=599, ..
        } => says(tr!("updater.failure.busy"), false),
        // The server could not find the download, so the release page is the way.
        UpdateDownloadError::Status { .. } => says(tr!("updater.failure.unserved"), true),
        UpdateDownloadError::SizeMismatch { .. } | UpdateDownloadError::HashMismatch { .. } => {
            says(tr!("updater.failure.damaged"), false)
        }
        UpdateDownloadError::Io { .. } => says(tr!("updater.failure.disk"), false),
        UpdateDownloadError::NotWritable { .. }
        | UpdateDownloadError::NoAsset
        | UpdateDownloadError::Unsupported => says(tr!("updater.failure.use_page"), true),
        _ => says(unknown(), false),
    }
}

/// A failed install in player words; installs that cannot update in place go to the release page.
pub fn describe_apply(err: &UpdateApplyError) -> Failure {
    match err {
        UpdateApplyError::NotWritable { .. } | UpdateApplyError::Unsupported { .. } => {
            says(tr!("updater.failure.use_page"), true)
        }
        UpdateApplyError::Io { .. } => says(tr!("updater.failure.install_io"), false),
        UpdateApplyError::Spawn { .. } => says(tr!("updater.failure.installer"), false),
        // A fresh download replaces the staged installer or AppImage, so trying again can help.
        UpdateApplyError::Changed { .. } => says(tr!("updater.failure.changed"), false),
        _ => says(unknown(), false),
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
        let (client, target, offer) =
            (Arc::clone(&client), Arc::clone(&target), Arc::clone(&offer));
        Arc::new(move || -> CheckFuture {
            let (client, target, offer) =
                (Arc::clone(&client), Arc::clone(&target), Arc::clone(&offer));
            let current = Arc::clone(&current);
            Box::pin(async move {
                let found =
                    match tokio::time::timeout(deadlines.check, client.check(&current, &target))
                        .await
                    {
                        Err(_) => {
                            tracing::warn!(deadline = ?deadlines.check, "update check timed out");
                            return Err(says(tr!("updater.failure.check_slow"), false));
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
        let staged = Arc::clone(&staged);
        Arc::new(move |sink: DownloadSink| -> DownloadFuture {
            let (client, target, staged) = (
                Arc::clone(&client),
                Arc::clone(&target),
                Arc::clone(&staged),
            );
            let wanted = lock(&offer).clone();
            Box::pin(async move {
                let Some(wanted) = wanted else {
                    return Err(says(tr!("updater.failure.check_first"), false));
                };
                let fetch = client.download(&wanted, &target, |received, total| {
                    sink.report(received, total)
                });
                match tokio::time::timeout(deadlines.download, fetch).await {
                    Err(_) => {
                        tracing::warn!(deadline = ?deadlines.download, "update download timed out");
                        Err(says(tr!("updater.failure.download_slow"), false))
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
            return Err(says(tr!("updater.failure.download_first"), false));
        };
        match apply(update) {
            Ok(Applied::Replaced { relaunch }) => Ok(AfterInstall::Relaunch(relaunch)),
            Ok(Applied::InstallerStarted) => Ok(AfterInstall::Quit),
            Ok(other) => {
                tracing::warn!(applied = ?other, "update applied in an unexpected way");
                Err(says(unknown(), false))
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
