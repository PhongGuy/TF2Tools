//! mastercomfig's background jobs, shared by every page that installs its base VPK.

use std::future::Future;
use std::sync::Arc;

use gpui_kit::{AnyWindowHandle, App, AsyncApp};
use tf2tools_core::{
    ApplyModulesError, Choices, Library, MastercomfigStatus, ModuleCatalog, StoreClient, StoreKind,
    Tf2Install,
};

use super::model::{SettingsNote, catalog_note, describe_apply_modules, describe_install};
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

/// What `install_base` installed: the tag, the saved settings that tag lacks, and whether a
/// player's own modules.cfg kept the saved settings from being written.
#[derive(Debug)]
pub struct Installed {
    pub version: String,
    pub dropped: Vec<String>,
    pub own_file_kept: bool,
}

/// Installs or updates the base VPK on the tokio bridge. It refuses while TF2 runs. With no
/// recorded release, it first forces the addon-store sync. Saved settings are then re-applied
/// against the new tag's catalog, leaving a player's own modules.cfg alone; a failure there is
/// only logged. Failures are OpErrors titled `action`, except the refusal, titled CLOSE_TF2.
pub fn install_base(
    tf2: Tf2Install,
    action: &'static str,
    cx: &mut App,
) -> impl Future<Output = Result<Installed, OpError>> + 'static + use<> {
    let store = cx.global::<Services>().store.clone();
    let library = cx.global::<Services>().library.clone();
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
        let version = store
            .install_mastercomfig(&tf2, &|_, _| {})
            .await
            .map_err(|e| describe_install(&e).into_op(action))?;
        let (dropped, own_file_kept) = reapply_choices(&store, library, &tf2, &version).await;
        Ok(Installed {
            version,
            dropped,
            own_file_kept,
        })
    });
    async move { job.await.map_err(|e| OpError::core(action, e))? }
}

// Applies the saved choices (if any) against `version`'s catalog and returns the names it
// dropped, and true when a player's own modules.cfg stood in the way. Every other failure is
// logged and yields no names, so it never fails the install.
async fn reapply_choices(
    store: &StoreClient,
    library: Library,
    tf2: &Tf2Install,
    version: &str,
) -> (Vec<String>, bool) {
    let saved = tokio::task::spawn_blocking(move || library.mastercomfig_choices())
        .await
        .map_err(|e| e.to_string())
        .and_then(|read| read.map_err(|e| e.to_string()));
    let choices = match saved {
        Ok(Some(choices)) => choices,
        Ok(None) => return (Vec::new(), false),
        Err(e) => {
            tracing::warn!("mastercomfig settings after install: {e}");
            return (Vec::new(), false);
        }
    };
    let catalog = match store.mastercomfig_catalog(version).await {
        Ok(catalog) => catalog,
        Err(e) => {
            tracing::warn!("mastercomfig settings list for {version}: {e}");
            return (Vec::new(), false);
        }
    };
    match store
        .apply_mastercomfig_modules(tf2, &catalog, &choices, false)
        .await
    {
        Ok(dropped) => (dropped, false),
        Err(e) => {
            tracing::warn!("mastercomfig settings after install: {e}");
            (
                Vec::new(),
                matches!(e, ApplyModulesError::ForeignModulesCfg { .. }),
            )
        }
    }
}

/// The title of a setting that could not be saved.
pub const SAVE_FAILED: &str = "Couldn't save the setting";

/// Why `apply_modules` changed nothing.
#[derive(Debug)]
pub enum ModulesFailure {
    /// TF2 has a modules.cfg TF2Tools did not write.
    Foreign,
    Op(OpError),
}

/// Applies `choices` through core on the tokio bridge. With `guard_tf2` it first refuses while
/// TF2 runs (CLOSE_TF2). Resolves to the dropped module names. Errors are titled SAVE_FAILED.
pub fn apply_modules(
    tf2: Tf2Install,
    catalog: Arc<ModuleCatalog>,
    choices: Choices,
    guard_tf2: bool,
    replace_foreign: bool,
    cx: &mut App,
) -> impl Future<Output = Result<Vec<String>, ModulesFailure>> + 'static + use<> {
    let store = cx.global::<Services>().store.clone();
    let probe = Tf2Probe::get(cx);
    if !tokio_bridge::is_running(cx) {
        tokio_bridge::init(cx);
    }
    let died = |e: tokio::task::JoinError| ModulesFailure::Op(OpError::core(SAVE_FAILED, e));
    let job = tokio_bridge::spawn(cx, async move {
        if guard_tf2 {
            tokio::task::spawn_blocking(move || refuse_while_running(&*probe))
                .await
                .map_err(died)?
                .map_err(ModulesFailure::Op)?;
        }
        store
            .apply_mastercomfig_modules(&tf2, &catalog, &choices, replace_foreign)
            .await
            .map_err(|e| {
                if let ApplyModulesError::ForeignModulesCfg { .. } = e {
                    ModulesFailure::Foreign
                } else {
                    ModulesFailure::Op(describe_apply_modules(&e).into_op(SAVE_FAILED))
                }
            })
    });
    async move {
        job.await
            .map_err(|e| ModulesFailure::Op(OpError::core(SAVE_FAILED, e)))?
    }
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
