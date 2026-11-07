//! Writes v5's activity-log rows. A failed append is a diagnostics `warn!`, never an error the
//! operation reports, since the log is a record of work and not part of it.

use gpui_kit::App;
use gpui_kit::AppContext as _;
use tf2tools_core::{LOG_KEEP, LogLevel, NewLogEntry, StoreItemId, StoreKind};

use crate::services::Services;
use crate::store_download::{DownloadEvent, StoreDownloads};
use crate::sync::{SyncEvent, SyncSummary, sync_hub};

/// Appends `entry` now (call inside a `run_blocking` job); a failure is a `warn!`, never an error.
pub fn write(services: &Services, entry: &NewLogEntry) {
    if let Err(err) = services.activity.append(entry) {
        tracing::warn!(%err, "could not append an activity log row");
    }
}

/// Appends `entry` on the background executor; without a `Services` global it warns and drops it.
pub fn record(cx: &App, entry: NewLogEntry) {
    let Some(services) = cx.try_global::<Services>().cloned() else {
        tracing::warn!("no services installed; dropping an activity log row");
        return;
    };
    cx.background_spawn(async move { write(&services, &entry) })
        .detach();
}

/// After the first frame: appends v5's startup row, then trims to `LOG_KEEP`, on the background
/// executor (detached). Either failure is a `warn!`.
pub fn startup(cx: &App) {
    let Some(services) = cx.try_global::<Services>().cloned() else {
        tracing::warn!("no services installed; skipping the startup log row");
        return;
    };
    cx.background_spawn(async move {
        // The database path is developer detail, so it goes to tracing, not the player's log.
        tracing::info!(database = %services.data.db_path().display(), "TF2Tools started");
        let entry = NewLogEntry::new(LogLevel::Info, "system", "startup", "TF2Tools started");
        write(&services, &entry);
        // Trimming after the append keeps the table at exactly the cap, newest row included.
        if let Err(err) = services.activity.trim(LOG_KEEP) {
            tracing::warn!(%err, "could not trim the activity log");
        }
    })
    .detach();
}

/// v5's row for a finished sync, or None when it was skipped as fresh.
pub fn sync_entry(kind: StoreKind, summary: SyncSummary) -> Option<NewLogEntry> {
    if summary.fresh {
        return None;
    }
    // GameBanana kinds count what this run fetched; the curated lists count every row, as v5.
    let (category, message) = match kind {
        StoreKind::GameBananaHuds => (
            "hud_store",
            format!("Synced {} HUDs from GameBanana", summary.fetched),
        ),
        StoreKind::Sounds => (
            "sound_store",
            format!("Synced {} sounds from GameBanana", summary.fetched),
        ),
        StoreKind::Crosshairs => (
            "crosshair_store",
            format!("Synced {} crosshairs from GameBanana", summary.fetched),
        ),
        StoreKind::Addons => (
            "addon_store",
            format!("Synced {} addons from mastercomfig", summary.rows),
        ),
        StoreKind::CuratedHuds | _ => ("store", format!("Synced {} curated HUDs", summary.rows)),
    };
    Some(NewLogEntry::new(LogLevel::Info, category, "sync", message))
}

/// v5's row for a finished or failed store download.
pub fn download_entry(event: &DownloadEvent) -> NewLogEntry {
    let (target, failure) = match event {
        DownloadEvent::Finished { target } => (target, None),
        DownloadEvent::Failed { target, message } => (target, Some(message)),
    };
    let name = &target.name;
    let id = match &target.item {
        StoreItemId::GameBanana(id) => id.to_string(),
        StoreItemId::Curated(id) => id.to_string(),
        StoreItemId::Addon(asset) => asset.as_str().to_owned(),
        _ => name.clone(),
    };
    let (category, done, failed, gb) = match target.kind {
        StoreKind::GameBananaHuds => (
            "hud_store",
            format!("Downloaded HUD from GameBanana: {name}"),
            format!("HUD download failed (GameBanana ID: {id})"),
            true,
        ),
        StoreKind::Sounds => (
            "sound_store",
            format!("Downloaded sound from store: {name}"),
            format!("Sound download failed (GameBanana ID: {id})"),
            true,
        ),
        StoreKind::Crosshairs => (
            "crosshair_store",
            format!("Downloaded crosshair from store: {name}"),
            format!("Crosshair download failed (GameBanana ID: {id})"),
            true,
        ),
        StoreKind::Addons => (
            "addon_store",
            format!("Downloaded addon to library: {name}"),
            format!("Addon download failed: {name}"),
            false,
        ),
        StoreKind::CuratedHuds | _ => (
            "store",
            format!("Downloaded HUD: {name}"),
            format!("HUD download failed: {name}"),
            false,
        ),
    };
    match failure {
        Some(message) => NewLogEntry::new(LogLevel::Error, category, "download", failed)
            .with_details(message.clone()),
        None if gb => NewLogEntry::new(LogLevel::Info, category, "download", done)
            .with_details(format!("GameBanana ID: {id}")),
        None => NewLogEntry::new(LogLevel::Info, category, "download", done),
    }
}

/// Subscribes (detached) to the app's `SyncHub` and `StoreDownloads`, whichever exist, and
/// `record`s a row for each `SyncEvent::Finished` (reading `SyncHub::last`) and `DownloadEvent`.
pub fn watch_stores(cx: &mut App) {
    if let Some(hub) = sync_hub(cx) {
        cx.subscribe(&hub, |hub, event: &SyncEvent, cx| {
            // The hub stores the summary before it emits, so `last` is this run's.
            if let SyncEvent::Finished { kind } = event
                && let Some(entry) = hub.read(cx).last(*kind).and_then(|s| sync_entry(*kind, s))
            {
                record(cx, entry);
            }
        })
        .detach();
    }
    if let Some(downloads) = StoreDownloads::global(cx) {
        cx.subscribe(&downloads, |_, event: &DownloadEvent, cx| {
            record(cx, download_entry(event));
        })
        .detach();
    }
}
