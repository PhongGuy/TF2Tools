//! Store download, listing, preview, load and update failures in player words. The messages never
//! carry URLs, paths or codes; the raw error rides along as `detail` for the developer log.

use tf2tools_core::{
    ArchiveError, BrowseError, DownloadError, FilesError, GbAddonFetchError, GbId, ImportError,
    InstallError, PreviewError, StoreKind,
};

use gpui_kit::SharedString;

use crate::i18n::{Phrase, tr};
use crate::services::OpError;
use crate::sync_messages::{GAMEBANANA, source_name, throttled};

pub(crate) fn library_update() -> SharedString {
    tr!("messages.shared.library_update")
}

pub(crate) fn store_read() -> SharedString {
    tr!("messages.shared.store_read")
}

fn download_stopped_text() -> SharedString {
    tr!("messages.shared.download_stopped")
}

fn download_disk() -> SharedString {
    tr!("messages.store.download.disk")
}

fn download_locked() -> SharedString {
    tr!("messages.store.download.locked")
}

fn download_unknown() -> SharedString {
    tr!("messages.store.download.unknown")
}

fn preview_unavailable_text() -> SharedString {
    tr!("messages.store.preview.unavailable")
}

fn gb_too_large() -> SharedString {
    tr!("messages.store.gb_fetch.too_large")
}

/// The toast title when a detail cannot read its cached store row.
pub fn details_failed_title() -> SharedString {
    tr!("messages.store.details_failed_title")
}

/// The info toast when a download cannot start because another one runs.
pub fn busy_title() -> SharedString {
    tr!("messages.store.busy_title")
}

/// The info toast's description: what the player does next.
pub fn busy_text() -> SharedString {
    tr!("messages.store.busy_text")
}

/// What the player reads about a failed store action, and the raw error for the developer log.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoreFailure {
    pub message: String,
    pub detail: String,
}

impl StoreFailure {
    /// A failure with its player message and its raw developer detail.
    pub fn new(message: impl Into<String>, detail: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            detail: detail.into(),
        }
    }

    /// Writes `detail` to the developer log as "{what} failed: {detail}".
    pub fn log(&self, what: &str) {
        tracing::warn!("{what} failed: {}", self.detail);
    }
}

/// A failed download from `kind`'s store in player words; `detail` is the error's own text.
pub fn describe_download(kind: StoreKind, err: &DownloadError) -> StoreFailure {
    StoreFailure::new(download_message(kind, err), err.to_string())
}

/// A download task that ended without an outcome, keeping `detail` for the log.
pub fn download_stopped(detail: &str) -> StoreFailure {
    StoreFailure::new(download_stopped_text(), detail)
}

// A failed rollback is worded by the failure that caused it; the names it left behind are
// appended by the caller.
fn download_message(kind: StoreKind, err: &DownloadError) -> SharedString {
    let source = source_name(kind);
    match err {
        DownloadError::AlreadyInLibrary { .. } => tr!("messages.store.download.already_in_library"),
        DownloadError::WrongKind { .. } => tr!("messages.store.download.wrong_kind"),
        DownloadError::RateLimited { .. } => tr!("messages.shared.source_busy", source = source),
        DownloadError::Http { status, .. } if *status >= 500 => {
            tr!("messages.shared.source_down", source = source)
        }
        DownloadError::Http { .. } => tr!("messages.store.download.http", source = source),
        DownloadError::Network { .. } => {
            tr!("messages.shared.source_unreachable", source = source)
        }
        DownloadError::TooLarge { .. } => tr!("messages.store.download.too_large"),
        DownloadError::EmptyBody { .. } => tr!("messages.shared.source_empty", source = source),
        DownloadError::ChecksumMismatch { .. } => tr!("messages.store.download.damaged"),
        DownloadError::Archive(archive) | DownloadError::Import(ImportError::Archive(archive)) => {
            archive_message(archive)
        }
        DownloadError::Io { .. } | DownloadError::Import(ImportError::Io { .. }) => download_disk(),
        DownloadError::Database { .. } | DownloadError::Import(ImportError::Database { .. }) => {
            library_update()
        }
        DownloadError::NoUsableFiles { .. } | DownloadError::Import(_) => {
            tr!("messages.store.download.unusable")
        }
        DownloadError::Interrupted { .. } => download_stopped_text(),
        DownloadError::RollbackFailed { cause, .. } => download_message(kind, cause),
        _ => download_unknown(),
    }
}

fn archive_message(err: &ArchiveError) -> SharedString {
    match err {
        ArchiveError::Encrypted { .. } => download_locked(),
        ArchiveError::Io { .. }
        | ArchiveError::Malformed { .. }
        | ArchiveError::UnsafePath { .. }
        | _ => tr!("messages.store.download.unpack"),
    }
}

/// A failed file listing from `kind`'s store in player words; `detail` is the error's own text.
pub fn describe_files(kind: StoreKind, err: &FilesError) -> StoreFailure {
    StoreFailure::new(files_phrase(kind, err).text(), err.to_string())
}

/// The player line for a failed file listing from `kind`'s store, worded each time it is read.
pub fn files_phrase(kind: StoreKind, err: &FilesError) -> Phrase {
    let source = source_name(kind);
    match err {
        FilesError::NotInStore { .. } => Phrase::new(|| tr!("messages.store.files.not_in_store")),
        FilesError::RateLimited { .. } => {
            Phrase::new(move || tr!("messages.shared.source_busy", source = source))
        }
        FilesError::Http { status, .. } if *status >= 500 => {
            Phrase::new(move || tr!("messages.shared.source_down", source = source))
        }
        FilesError::Network { .. } => {
            Phrase::new(move || tr!("messages.shared.source_unreachable", source = source))
        }
        FilesError::Http { .. } | FilesError::TooLarge { .. } | FilesError::Malformed { .. } => {
            Phrase::new(move || tr!("messages.shared.source_unreadable", source = source))
        }
        FilesError::Database { .. } => Phrase::new(store_read),
        FilesError::Throttled { .. } => Phrase::new(throttled),
        _ => Phrase::new(|| tr!("messages.store.files.unknown")),
    }
}

/// A failed GameBanana add-on listing, search, category list or profile in player words;
/// `detail` is the error's own text.
pub fn describe_browse(err: &BrowseError) -> StoreFailure {
    StoreFailure::new(browse_phrase(err).text(), err.to_string())
}

/// The player line for a failed GameBanana browse, worded each time it is read.
pub fn browse_phrase(err: &BrowseError) -> Phrase {
    match err {
        BrowseError::Network { .. } => {
            Phrase::new(|| tr!("messages.shared.source_unreachable", source = GAMEBANANA))
        }
        BrowseError::Throttled { .. } => Phrase::new(throttled),
        BrowseError::RateLimited { .. } => {
            Phrase::new(|| tr!("messages.shared.source_busy", source = GAMEBANANA))
        }
        // A 404 can come from a listing, a search or a category list as well as an item.
        BrowseError::Http { status: 404, .. } => {
            Phrase::new(|| tr!("messages.store.browse.not_found"))
        }
        BrowseError::Gone { .. } => Phrase::new(|| tr!("messages.store.browse.gone")),
        BrowseError::Http { status, .. } if *status >= 500 => {
            Phrase::new(|| tr!("messages.shared.source_down", source = GAMEBANANA))
        }
        BrowseError::Http { .. } | BrowseError::TooLarge { .. } | BrowseError::Malformed { .. } => {
            Phrase::new(|| tr!("messages.shared.source_unreadable", source = GAMEBANANA))
        }
        _ => Phrase::new(|| tr!("messages.store.browse.unknown")),
    }
}

/// A failed add-on download or unpack in player words; `detail` is the error's own text.
pub fn describe_gb_fetch(err: &GbAddonFetchError) -> StoreFailure {
    let message = match err {
        GbAddonFetchError::NoFiles { .. } => tr!("messages.store.gb_fetch.no_files"),
        GbAddonFetchError::VirusScan { .. } => tr!("messages.store.gb_fetch.virus_scan"),
        GbAddonFetchError::TooLarge { .. } => gb_too_large(),
        GbAddonFetchError::OnlyExe { .. } => tr!("messages.store.gb_fetch.only_exe"),
        // Nothing recognisable and too many parts to choose from need different next steps.
        GbAddonFetchError::UnknownLayout { payloads: 0, .. } => {
            tr!("messages.store.gb_fetch.nothing_known")
        }
        GbAddonFetchError::UnknownLayout { .. } => tr!("messages.store.gb_fetch.too_many_parts"),
        GbAddonFetchError::Archive(ArchiveError::UnsafePath { .. }) => {
            tr!("messages.store.gb_fetch.unsafe")
        }
        GbAddonFetchError::Archive(ArchiveError::Encrypted { .. }) => download_locked(),
        GbAddonFetchError::Archive(ArchiveError::Malformed { .. }) => {
            tr!("messages.store.gb_fetch.cant_open")
        }
        GbAddonFetchError::Archive(ArchiveError::Io { .. }) | GbAddonFetchError::Io { .. } => {
            download_disk()
        }
        GbAddonFetchError::EmptyBody { .. } => {
            tr!("messages.shared.source_empty", source = GAMEBANANA)
        }
        GbAddonFetchError::ChecksumMismatch { .. } => tr!("messages.store.gb_fetch.damaged"),
        GbAddonFetchError::Network { .. } => {
            tr!("messages.shared.source_unreachable", source = GAMEBANANA)
        }
        GbAddonFetchError::RateLimited { .. } => {
            tr!("messages.shared.source_busy", source = GAMEBANANA)
        }
        GbAddonFetchError::Http { status: 404, .. } => {
            tr!("messages.store.gb_fetch.file_not_found")
        }
        GbAddonFetchError::Http { status, .. } if *status >= 500 => {
            tr!("messages.shared.source_down", source = GAMEBANANA)
        }
        GbAddonFetchError::Http { .. } => {
            tr!("messages.shared.source_unreadable", source = GAMEBANANA)
        }
        GbAddonFetchError::Interrupted { .. } => download_stopped_text(),
        GbAddonFetchError::Archive(ArchiveError::TooLarge { .. }) => gb_too_large(),
        GbAddonFetchError::Archive(_) | _ => download_unknown(),
    };
    StoreFailure::new(message, err.to_string())
}

/// A failed map download or unpack in player words: the size and layout refusals name
/// maps, the rest read as for any add-on.
pub fn describe_gb_map_fetch(err: &GbAddonFetchError) -> StoreFailure {
    let message = match err {
        GbAddonFetchError::UnknownLayout { payloads: 0, .. } => {
            tr!("messages.store.gb_map_fetch.no_map")
        }
        GbAddonFetchError::UnknownLayout { .. } => tr!("messages.store.gb_map_fetch.too_many_maps"),
        GbAddonFetchError::Archive(ArchiveError::TooLarge { .. }) => {
            tr!("messages.store.gb_map_fetch.maps_too_large")
        }
        GbAddonFetchError::TooLarge { .. } => tr!("messages.store.gb_map_fetch.too_large"),
        GbAddonFetchError::NoFiles { .. } => tr!("messages.store.gb_map_fetch.no_files"),
        GbAddonFetchError::Archive(ArchiveError::Malformed { .. }) => {
            tr!("messages.store.gb_map_fetch.cant_open")
        }
        GbAddonFetchError::VirusScan { .. }
        | GbAddonFetchError::RateLimited { .. }
        | GbAddonFetchError::Http { .. }
        | GbAddonFetchError::Network { .. }
        | GbAddonFetchError::EmptyBody { .. }
        | GbAddonFetchError::ChecksumMismatch { .. }
        | GbAddonFetchError::Archive(_)
        | GbAddonFetchError::OnlyExe { .. }
        | GbAddonFetchError::Io { .. }
        | GbAddonFetchError::Interrupted { .. }
        | _ => return describe_gb_fetch(err),
    };
    StoreFailure::new(message, err.to_string())
}

/// A listing task that ended without an outcome, keeping `detail` for the log.
pub fn listing_stopped(detail: String) -> StoreFailure {
    StoreFailure::new(listing_stopped_phrase().text(), detail)
}

/// The player line for a listing task that ended without an outcome.
pub fn listing_stopped_phrase() -> Phrase {
    Phrase::new(|| tr!("messages.store.files.stopped"))
}

/// An add-on listing step that ended without an outcome, keeping `detail` for the log.
pub fn addons_stopped(detail: String) -> StoreFailure {
    StoreFailure::new(addons_stopped_phrase().text(), detail)
}

/// The player line for an add-on listing step that ended without an outcome.
pub fn addons_stopped_phrase() -> Phrase {
    Phrase::new(|| tr!("messages.store.browse.stopped"))
}

/// An add-on profile read that ended without an outcome, keeping `detail` for the log.
pub fn addon_stopped(detail: String) -> StoreFailure {
    StoreFailure::new(addon_stopped_phrase().text(), detail)
}

/// The player line for an add-on profile read that ended without an outcome.
pub fn addon_stopped_phrase() -> Phrase {
    Phrase::new(|| tr!("messages.store.browse.addon_stopped"))
}

/// A failed preview fetch in player words; previews only come from GameBanana.
pub fn describe_preview(err: &PreviewError) -> StoreFailure {
    let message = match err {
        PreviewError::NoPreview => tr!("messages.store.preview.none"),
        PreviewError::UntrustedUrl { .. } => tr!("messages.store.preview.untrusted"),
        PreviewError::RateLimited { .. } => tr!("messages.shared.source_busy", source = GAMEBANANA),
        PreviewError::Http { status, .. } if *status >= 500 => {
            tr!("messages.shared.source_down", source = GAMEBANANA)
        }
        PreviewError::Network { .. } => {
            tr!("messages.shared.source_unreachable", source = GAMEBANANA)
        }
        PreviewError::Http { .. } | PreviewError::TooLarge { .. } => {
            tr!("messages.store.preview.fetch")
        }
        _ => preview_unavailable_text(),
    };
    StoreFailure::new(message, err.to_string())
}

/// The sound's store row is gone; `detail` names the id.
pub fn preview_gone(id: GbId) -> StoreFailure {
    StoreFailure::new(
        tr!("messages.store.preview.gone"),
        format!("sound {} is no longer in the store", id.get()),
    )
}

/// The row read or the fetch task failed without a `PreviewError`, keeping `detail` for the log.
pub fn preview_unavailable(detail: String) -> StoreFailure {
    StoreFailure::new(preview_unavailable_text(), detail)
}

/// A failed local read of a store or the library behind it; the message is fixed and
/// `detail` is the error's own text, a panic included.
pub fn describe_store_read(err: &OpError) -> StoreFailure {
    StoreFailure::new(store_read(), err.to_string())
}

/// Logs a failed store read as "store load failed: ..." and returns the line the player reads.
pub fn read_failed(err: &OpError) -> Phrase {
    describe_store_read(err).log("store load");
    Phrase::new(store_read)
}

/// A failed copy of a library item into tf/custom in player words; `detail` is the error's own text.
pub fn describe_install(err: &InstallError) -> StoreFailure {
    let message = match err {
        InstallError::Io { .. } => tr!("messages.store.install.io"),
        InstallError::NotInLibrary { .. } => tr!("messages.store.install.not_in_library"),
        InstallError::Database { .. } => library_update(),
        InstallError::GameMissing { .. } => crate::local_messages::game_moved(),
        InstallError::ExternalHuds(_) | _ => tr!("messages.store.install.unknown"),
    };
    StoreFailure::new(message, err.to_string())
}

/// No TF2 folder is known, so nothing can be installed.
pub fn no_tf2() -> StoreFailure {
    StoreFailure::new(tr!("messages.store.no_tf2"), OpError::NoTf2.to_string())
}

/// The looked-up mastercomfig release lists no file.
pub fn release_has_no_file() -> StoreFailure {
    StoreFailure::new(
        tr!("messages.store.release_no_file"),
        "the release has no file",
    )
}
