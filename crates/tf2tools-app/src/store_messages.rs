//! Store download, listing, preview, load and update failures in player words. The messages never
//! carry URLs, paths or codes; the raw error rides along as `detail` for the developer log.

use tf2tools_core::{
    ArchiveError, DownloadError, FilesError, GbId, ImportError, InstallError, PreviewError,
    StoreKind,
};

use crate::services::OpError;
use crate::sync_messages::source_name;

const DL_STOPPED: &str = "The download stopped before it finished. Try again.";
const DL_DISK: &str = "Couldn't save the download. Make sure your disk has free space and TF2Tools can write to its folder, then try again.";
pub(crate) const LIBRARY_UPDATE: &str =
    "TF2Tools couldn't update your library. Restart TF2Tools and try again.";
const DL_UNUSABLE: &str = "This download has nothing TF2Tools can use. Pick another file.";
const DL_RAR: &str = "This file is a RAR archive, which TF2Tools can't open. Pick another file.";
const DL_UNPACK: &str = "The download couldn't be unpacked. Try again, or pick another file.";
const DL_UNKNOWN: &str = "Something went wrong with the download. Try again later.";
const FILES_STOPPED: &str = "Loading the files stopped before it finished. Try again.";
pub(crate) const STORE_READ: &str =
    "TF2Tools couldn't read the store. Restart TF2Tools and try again.";
const FILES_UNKNOWN: &str = "Couldn't load the files. Try again later.";
const PV_FETCH: &str = "Couldn't get this preview from GameBanana. Try again later.";
const PV_UNAVAILABLE: &str = "Couldn't play this preview. Try again later.";

/// The toast title when a detail cannot read its cached store row.
pub const DETAILS_FAILED_TITLE: &str = "Failed to load details";

/// The info toast when a download cannot start because another one runs.
pub const BUSY_TITLE: &str = "Another download is running";
/// The info toast's description: what the player does next.
pub const BUSY_TEXT: &str = "Wait for it to finish, then try again.";

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
    StoreFailure::new(DL_STOPPED, detail)
}

// A failed rollback is worded by the failure that caused it; the names it left behind are
// appended by the caller.
fn download_message(kind: StoreKind, err: &DownloadError) -> String {
    let source = source_name(kind);
    match err {
        DownloadError::AlreadyInLibrary { .. } => "This item is already in your library.".into(),
        DownloadError::WrongKind { .. } => {
            "This file doesn't belong in this store. Pick another file.".into()
        }
        DownloadError::RateLimited { .. } => {
            format!("{source} is busy right now. Wait a few minutes, then try again.")
        }
        DownloadError::Http { status, .. } if *status >= 500 => {
            format!("{source} is having problems right now. Try again later.")
        }
        DownloadError::Http { .. } => {
            format!("Couldn't get this file from {source}. Try again later.")
        }
        DownloadError::Network { .. } => {
            format!("Couldn't reach {source}. Check your internet connection and try again.")
        }
        DownloadError::TooLarge { .. } => {
            "This file is too big for TF2Tools. Pick another file.".into()
        }
        DownloadError::ChecksumMismatch { .. } => "The download was damaged. Try again.".into(),
        DownloadError::Archive(archive) | DownloadError::Import(ImportError::Archive(archive)) => {
            archive_message(archive).into()
        }
        DownloadError::Io { .. } | DownloadError::Import(ImportError::Io { .. }) => DL_DISK.into(),
        DownloadError::Database { .. } | DownloadError::Import(ImportError::Database { .. }) => {
            LIBRARY_UPDATE.into()
        }
        DownloadError::NoUsableFiles { .. } | DownloadError::Import(_) => DL_UNUSABLE.into(),
        DownloadError::Interrupted { .. } => DL_STOPPED.into(),
        DownloadError::RollbackFailed { cause, .. } => download_message(kind, cause),
        _ => DL_UNKNOWN.into(),
    }
}

fn archive_message(err: &ArchiveError) -> &'static str {
    match err {
        ArchiveError::Rar { .. } => DL_RAR,
        ArchiveError::Io { .. }
        | ArchiveError::Malformed { .. }
        | ArchiveError::UnsafePath { .. }
        | _ => DL_UNPACK,
    }
}

/// A failed file listing from `kind`'s store in player words; `detail` is the error's own text.
pub fn describe_files(kind: StoreKind, err: &FilesError) -> StoreFailure {
    let source = source_name(kind);
    let message = match err {
        FilesError::NotInStore { .. } => "This item is no longer in the store.".into(),
        FilesError::RateLimited { .. } => {
            format!("{source} is busy right now. Wait a few minutes, then try again.")
        }
        FilesError::Http { status, .. } if *status >= 500 => {
            format!("{source} is having problems right now. Try again later.")
        }
        FilesError::Network { .. } => {
            format!("Couldn't reach {source}. Check your internet connection and try again.")
        }
        FilesError::Http { .. } | FilesError::TooLarge { .. } | FilesError::Malformed { .. } => {
            format!("{source} sent something TF2Tools couldn't read. Try again later.")
        }
        FilesError::Database { .. } => STORE_READ.into(),
        _ => FILES_UNKNOWN.into(),
    };
    StoreFailure::new(message, err.to_string())
}

/// A listing task that ended without an outcome, keeping `detail` for the log.
pub fn listing_stopped(detail: String) -> StoreFailure {
    StoreFailure::new(FILES_STOPPED, detail)
}

/// A failed preview fetch in player words; previews only come from GameBanana.
pub fn describe_preview(err: &PreviewError) -> StoreFailure {
    let message = match err {
        PreviewError::NoPreview => "This sound has no preview.",
        PreviewError::UntrustedUrl { .. } => {
            "This preview isn't hosted on GameBanana, so TF2Tools won't play it."
        }
        PreviewError::RateLimited { .. } => {
            "GameBanana is busy right now. Wait a few minutes, then try again."
        }
        PreviewError::Http { status, .. } if *status >= 500 => {
            "GameBanana is having problems right now. Try again later."
        }
        PreviewError::Network { .. } => {
            "Couldn't reach GameBanana. Check your internet connection and try again."
        }
        PreviewError::Http { .. } | PreviewError::TooLarge { .. } => PV_FETCH,
        _ => PV_UNAVAILABLE,
    };
    StoreFailure::new(message, err.to_string())
}

/// The sound's store row is gone; `detail` names the id.
pub fn preview_gone(id: GbId) -> StoreFailure {
    StoreFailure::new(
        "This sound is no longer in the store.",
        format!("sound {} is no longer in the store", id.get()),
    )
}

/// The row read or the fetch task failed without a `PreviewError`, keeping `detail` for the log.
pub fn preview_unavailable(detail: String) -> StoreFailure {
    StoreFailure::new(PV_UNAVAILABLE, detail)
}

/// A failed local read of a store or the library behind it; the message is fixed and
/// `detail` is the error's own text, a panic included.
pub fn describe_store_read(err: &OpError) -> StoreFailure {
    StoreFailure::new(STORE_READ, err.to_string())
}

/// Logs a failed store read as "store load failed: ..." and returns the line the player reads.
pub fn read_failed(err: &OpError) -> String {
    let failure = describe_store_read(err);
    failure.log("store load");
    failure.message
}

/// A failed copy of a library item into tf/custom in player words; `detail` is the error's own text.
pub fn describe_install(err: &InstallError) -> StoreFailure {
    let message = match err {
        InstallError::Io { .. } => {
            "Couldn't copy the addon into TF2. Make sure TF2Tools can write to your TF2 folder, then try again."
        }
        InstallError::NotInLibrary { .. } => {
            "This addon is no longer in your library. Download it again."
        }
        InstallError::Database { .. } => LIBRARY_UPDATE,
        InstallError::GameMissing { .. } => crate::local_messages::GAME_MOVED,
        InstallError::ExternalHuds(_) | _ => {
            "Something went wrong while installing. Try again later."
        }
    };
    StoreFailure::new(message, err.to_string())
}

/// No TF2 folder is known, so nothing can be installed.
pub fn no_tf2() -> StoreFailure {
    StoreFailure::new(
        "Set your TF2 folder in Settings, then try again.",
        OpError::NoTf2.to_string(),
    )
}

/// The looked-up mastercomfig release lists no file.
pub fn release_has_no_file() -> StoreFailure {
    StoreFailure::new(
        "This mastercomfig release has nothing to download yet. Try again later.",
        "the release has no file",
    )
}
