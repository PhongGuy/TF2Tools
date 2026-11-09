//! mastercomfig's background jobs, shared by every page that installs its base VPK.

use std::future::Future;

use gpui_kit::{AnyWindowHandle, App, AsyncApp};
use tf2tools_core::{MastercomfigStatus, ModuleCatalog, StoreClient, StoreKind, Tf2Install};

use super::model::{SettingsNote, catalog_note, describe_install};
use crate::local_messages::describe_store_status;
use crate::pages::profiles::probe::Tf2Probe;
use crate::services::{OpError, Services};
use crate::shell::confirm::{Confirm, confirm};
use crate::shell::toast::toast_error;
use crate::sync_messages::describe_sync;
use crate::tokio_bridge;

/// The toast title when an install, update or remove refuses because TF2 is running.
pub const CLOSE_TF2: &str = "Close TF2 first";
/// Its description.
pub const CLOSE_TF2_TEXT: &str = "TF2 is running. Close the game, then try again.";

/// Err(OpError::Core { action: CLOSE_TF2, message: CLOSE_TF2_TEXT }) when `probe` says TF2
/// runs. Blocking: call off the UI thread.
pub fn refuse_while_running(probe: &(dyn Fn() -> bool + Send + Sync)) -> Result<(), OpError> {
    if probe() {
        return Err(OpError::Core {
            action: CLOSE_TF2,
            message: CLOSE_TF2_TEXT.to_owned(),
        });
    }
    Ok(())
}

/// Installs or updates the base VPK on the tokio bridge. It refuses while TF2 runs. With no
/// recorded release, it first forces the addon-store sync. It resolves to the installed tag.
/// Failures are OpErrors titled `action`, except the refusal, which is titled CLOSE_TF2.
pub fn install_base(
    tf2: Tf2Install,
    action: &'static str,
    cx: &mut App,
) -> impl Future<Output = Result<String, OpError>> + 'static + use<> {
    let store = cx.global::<Services>().store.clone();
    let probe = Tf2Probe::get(cx);
    if !tokio_bridge::is_running(cx) {
        tokio_bridge::init(cx);
    }
    // The sync and the install each take core's download lock, so they run one after the
    // other; a forced sync covers a fresh cache that predates the release table.
    let job = tokio_bridge::spawn(cx, async move {
        tokio::task::spawn_blocking(move || refuse_while_running(&*probe))
            .await
            .map_err(|e| OpError::core(action, e))??;
        let known = store
            .mastercomfig_release()
            .map_err(|e| describe_store_status(&e).into_op(action))?
            .is_some();
        if !known {
            force_sync(&store, action).await?;
        }
        store
            .install_mastercomfig(&tf2, &|_, _| {})
            .await
            .map_err(|e| describe_install(&e).into_op(action))
    });
    async move { job.await.map_err(|e| OpError::core(action, e))? }
}

/// True only when mastercomfig's base is missing from `tf2`; a status error reads as
/// present, so it never blocks an install. Blocking: call inside a background job.
pub fn needs_base(s: &Services, tf2: &Tf2Install) -> bool {
    matches!(
        s.library.mastercomfig_status(tf2),
        Ok(MastercomfigStatus::NotInstalled)
    )
}

/// Asks `addon_needs_mastercomfig` in `win` and, on yes, installs the base through
/// `install_base`. True once the base is in. False on cancel, on a window that is gone, or
/// on a failed base install, which toasts in `win`. Changes nothing before the answer.
pub async fn ask_and_install_base(
    win: AnyWindowHandle,
    tf2: Tf2Install,
    cx: &mut AsyncApp,
) -> bool {
    let Ok(answer) = win.update(cx, |_, window, cx| {
        confirm(Confirm::addon_needs_mastercomfig(), window, cx)
    }) else {
        return false;
    };
    if !answer.recv().await.unwrap_or(false) {
        return false;
    }
    let job = cx.update(|cx| install_base(tf2, "Failed to install mastercomfig", cx));
    match job.await {
        Ok(_) => true,
        Err(err) => {
            let _ = win.update(cx, |_, window, cx| toast_error(window, cx, &err));
            false
        }
    }
}

/// Forces the addon-store sync, which records GitHub's latest mastercomfig release. A failure
/// is an OpError titled "Couldn't check for updates" with the sync's player message.
pub fn check_for_updates(
    cx: &mut App,
) -> impl Future<Output = Result<(), OpError>> + 'static + use<> {
    const ACTION: &str = "Couldn't check for updates";
    let store = cx.global::<Services>().store.clone();
    if !tokio_bridge::is_running(cx) {
        tokio_bridge::init(cx);
    }
    let job = tokio_bridge::spawn(cx, async move { force_sync(&store, ACTION).await });
    async move { job.await.map_err(|e| OpError::core(ACTION, e))? }
}

/// The catalog of `tag` through the store (disk, else GitHub) on the tokio bridge. A failure
/// is the note shown in place of the list; a job that died reads as offline.
pub fn fetch_catalog(
    tag: String,
    cx: &mut App,
) -> impl Future<Output = Result<ModuleCatalog, SettingsNote>> + 'static + use<> {
    let store = cx.global::<Services>().store.clone();
    if !tokio_bridge::is_running(cx) {
        tokio_bridge::init(cx);
    }
    let job = tokio_bridge::spawn(cx, async move {
        store.mastercomfig_catalog(&tag).await.map_err(|e| {
            tracing::warn!("mastercomfig settings list for {tag}: {e}");
            catalog_note(&e)
        })
    });
    async move { job.await.unwrap_or(Err(SettingsNote::Offline)) }
}

// Runs the addon-store sync even on a fresh cache; the player sees the sync's own message and
// the log keeps its raw detail.
async fn force_sync(store: &StoreClient, action: &'static str) -> Result<(), OpError> {
    store
        .sync(StoreKind::Addons, true, &|_| {})
        .await
        .map(|_| ())
        .map_err(|e| {
            let failure = describe_sync(StoreKind::Addons, &e);
            tracing::warn!("{action}: {}", failure.detail);
            OpError::Core {
                action,
                message: failure.message,
            }
        })
}
