//! Error enums, one per operation. Each is `#[non_exhaustive]`, derives `thiserror`,
//! and carries paths and messages; no third-party error type is ever a field.

use std::path::PathBuf;

use crate::library::{ExternalHud, GbId, LibraryEntry, MissingItem};
use crate::store::StoreKind;
use crate::wav::WavInfo;

/// Why the platform data directory could not be resolved.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum DataDirError {
    /// The OS reports no per-user data directory.
    #[error("this platform has no user data directory")]
    NoPlatformDataDir,
}

/// Why the library database could not be opened or brought up to date.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum OpenError {
    /// A data directory could not be created, or `tmp/` could not be cleared.
    #[error("cannot create or clear {path}: {source}")]
    Io {
        /// The directory that failed.
        path: PathBuf,
        /// The underlying I/O error.
        source: std::io::Error,
    },
    /// The file exists but is not a readable SQLite database.
    #[error("{path} is not a usable SQLite database: {message}")]
    Corrupt {
        /// The database file.
        path: PathBuf,
        /// SQLite's description of the failure.
        message: String,
    },
    /// The database was written by a newer TF2Tools; it is left untouched.
    #[error("{path} has schema version {found}, newer than the supported {supported}")]
    NewerSchema {
        /// The database file.
        path: PathBuf,
        /// The `user_version` found in the file.
        found: u32,
        /// The highest version this build understands.
        supported: u32,
    },
    /// A migration step failed and was rolled back.
    #[error("migration step {step} failed on {path}: {message}")]
    Migration {
        /// The database file.
        path: PathBuf,
        /// The step that failed; earlier steps stay committed.
        step: u32,
        /// SQLite's description of the failure.
        message: String,
    },
    /// Any other SQLite failure while opening.
    #[error("database error on {path}: {message}")]
    Sqlite {
        /// The database file.
        path: PathBuf,
        /// SQLite's description of the failure.
        message: String,
    },
}

/// Why settings could not be read or saved.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum SettingsError {
    /// SQLite failed to read or write the `settings` table.
    #[error("settings database error: {message}")]
    Database {
        /// SQLite's description of the failure.
        message: String,
    },
    /// v5 stores the TF2 path as text, so a non-UTF-8 path has no stored form.
    #[error("TF2 path {path} is not valid UTF-8 and cannot be stored")]
    NonUtf8Path {
        /// The rejected path.
        path: PathBuf,
    },
}

/// Why a directory could not be accepted as a TF2 install.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum DetectError {
    /// The directory has no `tf` subdirectory.
    #[error("{path} is not a TF2 install (no tf directory)")]
    NotTf2 {
        /// The rejected game directory.
        path: PathBuf,
    },
    /// `tf/custom` could not be created.
    #[error("cannot create {path}: {source}")]
    Io {
        /// The directory that failed.
        path: PathBuf,
        /// The underlying I/O error.
        source: std::io::Error,
    },
}

/// Why a VPK could not be read.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum VpkError {
    /// The file itself could not be opened or read.
    #[error("cannot read {path}: {source}")]
    Io {
        /// The file that failed.
        path: PathBuf,
        /// The underlying I/O error.
        source: std::io::Error,
    },
    /// Bad header, parse error, parser panic or a short entry.
    #[error("{path} is not a readable VPK: {message}")]
    Malformed {
        /// The VPK file.
        path: PathBuf,
        /// What was wrong with it.
        message: String,
    },
    /// The VPK parsed but holds no `.wav` entry.
    #[error("{path} contains no .wav file")]
    NoWav {
        /// The VPK file.
        path: PathBuf,
    },
}

/// Why an archive could not be extracted.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ArchiveError {
    /// A file or directory could not be read or written.
    #[error("cannot read or write {path}: {source}")]
    Io {
        /// The path that failed.
        path: PathBuf,
        /// The underlying I/O error.
        source: std::io::Error,
    },
    /// The archive is password-protected.
    #[error("{path} is password-protected")]
    Encrypted {
        /// The archive.
        path: PathBuf,
    },
    /// The archive is corrupt, or its parser failed or panicked.
    #[error("{path} is not a readable archive: {message}")]
    Malformed {
        /// The archive.
        path: PathBuf,
        /// What was wrong with it.
        message: String,
    },
    /// An entry name would land outside the destination; nothing was extracted.
    #[error("{path} contains an entry outside the destination: {entry}")]
    UnsafePath {
        /// The archive.
        path: PathBuf,
        /// The offending entry name.
        entry: String,
    },
}

/// Why a WAV file cannot be used as a TF2 sound.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum WavError {
    /// The bytes are not a well-formed RIFF/WAVE file with a `fmt ` chunk.
    #[error("not a valid WAV file: {message}")]
    Malformed {
        /// What was wrong with it.
        message: String,
    },
    /// The WAV is well formed but in a format TF2 does not play.
    #[error("TF2 needs 16-bit 44100 Hz stereo PCM; this WAV is {found}")]
    Unsupported {
        /// The format the file actually has.
        found: WavInfo,
    },
}

/// Why a VTF cannot be decoded or a PNG cannot be turned into one.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum VtfError {
    /// The bytes do not parse or decode as a VTF, or the parser panicked.
    #[error("not a readable VTF: {message}")]
    Malformed {
        /// What was wrong with it.
        message: String,
    },
    /// The VTF uses a pixel format the decoder does not handle.
    #[error("VTF pixel format {format} is not supported")]
    UnsupportedFormat {
        /// The pixel format's name.
        format: String,
    },
    /// The input to `png_to_vtf` is not a readable PNG.
    #[error("not a readable PNG: {message}")]
    InvalidPng {
        /// What was wrong with it.
        message: String,
    },
    /// The VTF encoder rejected the image.
    #[error("cannot encode VTF: {message}")]
    Encode {
        /// What the encoder reported.
        message: String,
    },
}

/// Why a crosshair PNG could not be read.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum PngReadError {
    /// The file could not be inspected or read.
    #[error("cannot read {path}: {source}")]
    Io {
        /// The file that failed.
        path: PathBuf,
        /// The underlying I/O error.
        source: std::io::Error,
    },
    /// The file is over `PNG_MAX_FILE_BYTES`, so it was not read.
    #[error("a {bytes} byte file is over the PNG size limit")]
    FileTooLarge {
        /// The file's size in bytes.
        bytes: u64,
    },
    /// The picture is wider or taller than `PNG_MAX_SIDE`.
    #[error("a {width}x{height} picture is over the PNG side limit")]
    TooLarge {
        /// The picture's width in pixels.
        width: u32,
        /// The picture's height in pixels.
        height: u32,
    },
    /// The bytes do not decode as a PNG.
    #[error("not a readable PNG: {message}")]
    Invalid {
        /// What the decoder reported.
        message: String,
    },
}

/// Why a crosshair PNG could not be written.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum PngWriteError {
    /// The PNG encoder rejected the image.
    #[error("cannot encode PNG: {message}")]
    Encode {
        /// What the encoder reported.
        message: String,
    },
    /// The file could not be written.
    #[error("cannot write {path}: {source}")]
    Io {
        /// The file that failed.
        path: PathBuf,
        /// The underlying I/O error.
        source: std::io::Error,
    },
}

/// Why the builtin crosshair catalog could not be loaded.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum CatalogError {
    /// The manifest or the `vtf/` or `vmt/` directory could not be read.
    #[error("cannot read {path}: {source}")]
    Io {
        /// The file or directory that failed.
        path: PathBuf,
        /// The underlying I/O error.
        source: std::io::Error,
    },
    /// The manifest is not a JSON list of `{filename, display_name}` objects.
    #[error("{path} is not a valid crosshair manifest: {message}")]
    Manifest {
        /// The manifest file.
        path: PathBuf,
        /// What the JSON parser reported.
        message: String,
    },
}

/// Why an item could not be imported into the library.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ImportError {
    /// A file or directory could not be read or written.
    #[error("cannot read or write {path}: {source}")]
    Io {
        /// The path that failed.
        path: PathBuf,
        /// The underlying I/O error.
        source: std::io::Error,
    },
    /// The archive could not be extracted.
    #[error(transparent)]
    Archive(#[from] ArchiveError),
    /// The input holds no `info.vdf`, so it is not a HUD.
    #[error("{path} is not a HUD: no info.vdf inside")]
    NotAHud {
        /// The rejected input.
        path: PathBuf,
    },
    /// The input is not a file type this kind of item accepts.
    #[error("{path} is not a supported file type")]
    UnsupportedFile {
        /// The rejected input.
        path: PathBuf,
    },
    /// The WAV cannot be used as a TF2 sound.
    #[error(transparent)]
    Wav(#[from] WavError),
    /// The input does not start with the VPK magic.
    #[error("{path} is not a VPK file")]
    NotAVpk {
        /// The rejected input.
        path: PathBuf,
    },
    /// The image could not be decoded or converted to a VTF.
    #[error(transparent)]
    Vtf(#[from] VtfError),
    /// Curated provenance was given for an item that is not a HUD.
    #[error("only HUDs can come from the curated HUD store")]
    UnsupportedProvenance,
    /// SQLite failed to record the item.
    #[error("library database error: {message}")]
    Database {
        /// SQLite's description of the failure.
        message: String,
    },
}

/// Why a painted crosshair could not be saved or reopened.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum PaintError {
    /// A file or directory could not be read or written.
    #[error("cannot read or write {path}: {source}")]
    Io {
        /// The path that failed.
        path: PathBuf,
        /// The underlying I/O error.
        source: std::io::Error,
    },
    /// SQLite failed to read or record the crosshair.
    #[error("library database error: {message}")]
    Database {
        /// SQLite's description of the failure.
        message: String,
    },
    /// The drawing could not be encoded as a VTF or PNG.
    #[error(transparent)]
    Vtf(#[from] VtfError),
    /// The drawing is neither 32x32 nor 64x64.
    #[error("a {width}x{height} drawing is not 32x32 or 64x64")]
    InvalidDrawing {
        /// The drawing's width in pixels.
        width: u32,
        /// The drawing's height in pixels.
        height: u32,
    },
    /// The name is empty or only whitespace.
    #[error("the crosshair name is blank")]
    EmptyName,
    /// Another library crosshair already has this name, ignoring case.
    #[error("a crosshair called {name} already exists")]
    NameTaken {
        /// The trimmed name that was asked for.
        name: String,
    },
    /// No library crosshair has this id.
    #[error("crosshair {id} is not in the library")]
    NotFound {
        /// The id that was asked for.
        id: String,
    },
    /// The crosshair exists but was imported, not painted.
    #[error("crosshair {id} was not painted in the maker")]
    NotPainted {
        /// The id that was asked for.
        id: String,
    },
    /// Neither the saved drawing nor the VTF can be read back as a 32x32 or 64x64 image.
    #[error("crosshair {id} has no readable drawing or VTF")]
    Unreadable {
        /// The id that was asked for.
        id: String,
    },
}

/// Why the installed HUD could not be updated in place from the library.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ReinstallError {
    /// `path` could not be read or copied; the installed copy is as it was.
    #[error("cannot update the installed HUD at {path}: {source}")]
    Unchanged {
        /// The path that failed.
        path: PathBuf,
        /// The underlying I/O error.
        source: std::io::Error,
    },
    /// `path` could not be moved, as when a running game holds a file in it open; the
    /// installed copy is as it was.
    #[error("cannot move {path}: {source}")]
    InUse {
        /// The folder that could not be moved.
        path: PathBuf,
        /// The underlying I/O error.
        source: std::io::Error,
    },
    /// The installed copy was set aside at `path` and could not be put back;
    /// `Library::recover_reinstall` and the next reinstall put it back.
    #[error("the installed HUD is set aside at {path}: {source}")]
    SetAside {
        /// Where the installed copy now sits.
        path: PathBuf,
        /// The underlying I/O error.
        source: std::io::Error,
    },
    /// The HUD has no row or no files in the library.
    #[error("{what} is not in the library")]
    NotInLibrary {
        /// The item that was asked for.
        what: String,
    },
    /// The installed folder is not what TF2Tools put there, so it was left as it is.
    #[error("{folder} in tf/custom has changes that are not in the library")]
    Changed {
        /// The folder under `tf/custom/`.
        folder: String,
    },
    /// SQLite failed to read the library or the install record.
    #[error("library database error: {message}")]
    Database {
        /// SQLite's description of the failure.
        message: String,
    },
    /// The game folder (`tf/`) is no longer where the install was found, so nothing was
    /// written or created.
    #[error("Team Fortress 2 is no longer at {path}")]
    GameMissing {
        /// The `tf` folder that is gone.
        path: PathBuf,
    },
}

/// Why an item could not be installed into or removed from `tf/custom`.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum InstallError {
    /// A file or directory could not be read or written.
    #[error("cannot read or write {path}: {source}")]
    Io {
        /// The path that failed.
        path: PathBuf,
        /// The underlying I/O error.
        source: std::io::Error,
    },
    /// The item has no row or no files in the library.
    #[error("{what} is not in the library")]
    NotInLibrary {
        /// The item that was asked for.
        what: String,
    },
    /// SQLite failed to read or record the install.
    #[error("library database error: {message}")]
    Database {
        /// SQLite's description of the failure.
        message: String,
    },
    /// HUDs the library cannot put back are in the way and the caller did not name them.
    /// Nothing was changed.
    #[error("{} HUDs in tf/custom are not in the library", .0.len())]
    ExternalHuds(Vec<ExternalHud>),
    /// The game folder (`tf/`) is no longer where the install was found, so nothing was
    /// written or created.
    #[error("Team Fortress 2 is no longer at {path}")]
    GameMissing {
        /// The `tf` folder that is gone.
        path: PathBuf,
    },
}

/// Why a HUD in `tf/custom` could not be added to the library. Nothing in `tf/custom` changes.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum KeepHudError {
    /// A file or directory could not be read or written.
    #[error("cannot read or write {path}: {source}")]
    Io {
        /// The path that failed.
        path: PathBuf,
        /// The underlying I/O error.
        source: std::io::Error,
    },
    /// The HUD holds something a copy would silently lose, such as a link.
    #[error("cannot copy {path}")]
    NotCopyable {
        /// The entry that cannot be copied.
        path: PathBuf,
    },
    /// SQLite failed to read or record the HUD.
    #[error("library database error: {message}")]
    Database {
        /// SQLite's description of the failure.
        message: String,
    },
}

/// Why a library listing, deletion, scan or assignment failed.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum LibraryError {
    /// A file or directory could not be read or written.
    #[error("cannot read or write {path}: {source}")]
    Io {
        /// The path that failed.
        path: PathBuf,
        /// The underlying I/O error.
        source: std::io::Error,
    },
    /// No item has the given id or name.
    #[error("{what} not found")]
    NotFound {
        /// The item that was asked for.
        what: String,
    },
    /// The name is not a `tf_weapon_*` script name.
    #[error("{name} is not a weapon script name")]
    InvalidWeaponScript {
        /// The rejected name.
        name: String,
    },
    /// A VPK could not be read.
    #[error(transparent)]
    Vpk(#[from] VpkError),
    /// A VTF could not be decoded.
    #[error(transparent)]
    Vtf(#[from] VtfError),
    /// SQLite failed to read or write the library.
    #[error("library database error: {message}")]
    Database {
        /// SQLite's description of the failure.
        message: String,
    },
}

/// Why reading, saving, editing or deleting a profile failed.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ProfileError {
    /// No profile has the given id.
    #[error("profile {id} not found")]
    NotFound {
        /// The id that was asked for.
        id: String,
    },
    /// The profile name is blank after trimming.
    #[error("a profile name cannot be empty")]
    EmptyName,
    /// Another profile already has this name, ignoring case.
    #[error("a profile called {name} already exists")]
    NameTaken {
        /// The trimmed name that was asked for.
        name: String,
    },
    /// A slot or crosshair names an item the library does not have.
    #[error("{what} is not in the library")]
    ItemNotFound {
        /// The missing item, such as `HUD <id>`.
        what: String,
    },
    /// A crosshair assignment names something other than a `tf_weapon_*` script.
    #[error("{name} is not a weapon script name")]
    InvalidWeaponScript {
        /// The rejected name.
        name: String,
    },
    /// Scanning the installed library state failed.
    #[error(transparent)]
    Library(#[from] LibraryError),
    /// SQLite failed to read or write the profile.
    #[error("profile database error: {message}")]
    Database {
        /// SQLite's description of the failure.
        message: String,
    },
}

/// Why reading or writing the activity log failed.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum LogError {
    /// SQLite failed to read or write the `logs` table.
    #[error("activity log database error: {message}")]
    Database {
        /// SQLite's description of the failure.
        message: String,
    },
}

/// Why applying a profile stopped before or outside its steps. A failing step is not an
/// error here; it lands in the report's `failed` list.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ApplyProfileError {
    /// No profile has the given id.
    #[error("profile {id} not found")]
    NotFound {
        /// The id that was asked for.
        id: String,
    },
    /// Some references resolve to nothing and missing items were not to be skipped.
    #[error("{} profile items are missing", .0.len())]
    Missing(Vec<MissingItem>),
    /// HUDs the library cannot put back are in the way and the caller did not name them.
    /// Nothing was changed.
    #[error("{} HUDs in tf/custom are not in the library", .0.len())]
    ExternalHuds(Vec<ExternalHud>),
    /// `tf/custom` could not be read during the preflight.
    #[error("cannot read {path}: {source}")]
    Io {
        /// The path that failed.
        path: PathBuf,
        /// The underlying I/O error.
        source: std::io::Error,
    },
    /// SQLite failed during the preflight or while marking the profile active.
    #[error("profile database error: {message}")]
    Database {
        /// SQLite's description of the failure.
        message: String,
    },
    /// The game folder (`tf/`) is no longer where the install was found, so nothing was
    /// written or created.
    #[error("Team Fortress 2 is no longer at {path}")]
    GameMissing {
        /// The `tf` folder that is gone.
        path: PathBuf,
    },
}

/// Why a store sync stopped. Pages committed before the failure stay.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum SyncError {
    /// The server kept answering HTTP 429 after every retry.
    #[error("{url} is rate limiting requests; try again later")]
    RateLimited {
        /// The address that was asked.
        url: String,
    },
    /// The server answered with a non-success status.
    #[error("{url} answered HTTP {status}")]
    Http {
        /// The address that was asked.
        url: String,
        /// The HTTP status code.
        status: u16,
    },
    /// The server could not be reached or the connection broke.
    #[error("cannot reach {url}: {message}")]
    Network {
        /// The address that was asked.
        url: String,
        /// The HTTP client's description of the failure.
        message: String,
    },
    /// The response body grew past its size cap.
    #[error("{url} sent more than {limit} bytes")]
    TooLarge {
        /// The address that was asked.
        url: String,
        /// The cap in bytes.
        limit: u64,
    },
    /// The response did not have the expected shape.
    #[error("unexpected response from {url}: {message}")]
    Malformed {
        /// The address that was asked.
        url: String,
        /// What was wrong with the response.
        message: String,
    },
    /// A file or directory could not be read or written.
    #[error("cannot read or write {path}: {source}")]
    Io {
        /// The path that failed.
        path: PathBuf,
        /// The underlying I/O error.
        source: std::io::Error,
    },
    /// A downloaded archive could not be extracted.
    #[error(transparent)]
    Archive(#[from] ArchiveError),
    /// SQLite failed to read or write the store tables.
    #[error("store database error: {message}")]
    Database {
        /// SQLite's description of the failure.
        message: String,
    },
    /// GameBanana kept answering with an empty reply until the pacer gave up.
    #[error("{url} kept answering with an empty reply; GameBanana is throttling requests")]
    Throttled {
        /// The address that was asked.
        url: String,
    },
    /// The sync stopped before it finished, for example on a cancelled task.
    #[error("the sync was interrupted: {message}")]
    Interrupted {
        /// What stopped it.
        message: String,
    },
}

/// Why the store tables could not be read.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum QueryError {
    /// SQLite failed to read or write the store tables.
    #[error("store database error: {message}")]
    Database {
        /// SQLite's description of the failure.
        message: String,
    },
    /// The store is small and unsorted upstream, so it has no paged reads.
    #[error("the {kind:?} store is not paged; read it whole")]
    NotPaged {
        /// The store that was asked for a page.
        kind: StoreKind,
    },
}

/// Why an item's downloadable files could not be listed.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum FilesError {
    /// The item has no row in its store.
    #[error("{what} is not in the store")]
    NotInStore {
        /// The item that was asked for.
        what: String,
    },
    /// The server kept answering HTTP 429 after every retry.
    #[error("{url} is rate limiting requests; try again later")]
    RateLimited {
        /// The address that was asked.
        url: String,
    },
    /// The server answered with a non-success status.
    #[error("{url} answered HTTP {status}")]
    Http {
        /// The address that was asked.
        url: String,
        /// The HTTP status code.
        status: u16,
    },
    /// The server could not be reached or the connection broke.
    #[error("cannot reach {url}: {message}")]
    Network {
        /// The address that was asked.
        url: String,
        /// The HTTP client's description of the failure.
        message: String,
    },
    /// The response body grew past its size cap.
    #[error("{url} sent more than {limit} bytes")]
    TooLarge {
        /// The address that was asked.
        url: String,
        /// The cap in bytes.
        limit: u64,
    },
    /// The response did not have the expected shape.
    #[error("unexpected response from {url}: {message}")]
    Malformed {
        /// The address that was asked.
        url: String,
        /// What was wrong with the response.
        message: String,
    },
    /// GameBanana kept answering with an empty reply until the pacer gave up.
    #[error("{url} kept answering with an empty reply; GameBanana is throttling requests")]
    Throttled {
        /// The address that was asked.
        url: String,
    },
    /// SQLite failed to read or write the store tables.
    #[error("store database error: {message}")]
    Database {
        /// SQLite's description of the failure.
        message: String,
    },
}

/// Why a live GameBanana add-on listing, search, category list or profile could not be read.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum BrowseError {
    /// The server kept answering HTTP 429 after every retry.
    #[error("{url} is rate limiting requests; try again later")]
    RateLimited {
        /// The address that was asked.
        url: String,
    },
    /// The server answered with a non-success status.
    #[error("{url} answered HTTP {status}")]
    Http {
        /// The address that was asked.
        url: String,
        /// The HTTP status code.
        status: u16,
    },
    /// The server could not be reached or the connection broke.
    #[error("cannot reach {url}: {message}")]
    Network {
        /// The address that was asked.
        url: String,
        /// The HTTP client's description of the failure.
        message: String,
    },
    /// The response body grew past its size cap.
    #[error("{url} sent more than {limit} bytes")]
    TooLarge {
        /// The address that was asked.
        url: String,
        /// The cap in bytes.
        limit: u64,
    },
    /// The response did not have the expected shape.
    #[error("unexpected response from {url}: {message}")]
    Malformed {
        /// The address that was asked.
        url: String,
        /// What was wrong with the response.
        message: String,
    },
    /// GameBanana kept answering with an empty reply until the pacer gave up.
    #[error("{url} kept answering with an empty reply; GameBanana is throttling requests")]
    Throttled {
        /// The address that was asked.
        url: String,
    },
    /// The item exists but GameBanana no longer shows it.
    #[error("GameBanana item {gb_id} is trashed, withheld or private")]
    Gone {
        /// The item that was asked for.
        gb_id: GbId,
    },
}

/// Why a community map was not downloaded into `tf/maps`.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum MapDownloadError {
    /// The name is not a plain map file name.
    #[error("{name:?} is not a map name")]
    BadName {
        /// The name that was asked for.
        name: String,
    },
    /// The game already has this map, and TF2Tools did not put it there.
    #[error("{name} is already in the game")]
    InGame {
        /// The map's name.
        name: String,
    },
    /// TF2Tools already downloaded this map.
    #[error("{name} is already downloaded")]
    AlreadyDownloaded {
        /// The map's name.
        name: String,
    },
    /// The file that arrived is not a map.
    #[error("the file for {name} is not a map")]
    NotAMap {
        /// The map's name.
        name: String,
    },
    /// The server kept answering HTTP 429 after every retry.
    #[error("{url} is rate limiting requests; try again later")]
    RateLimited {
        /// The address that was asked.
        url: String,
    },
    /// The server answered with a non-success status.
    #[error("{url} answered HTTP {status}")]
    Http {
        /// The address that was asked.
        url: String,
        /// The HTTP status code.
        status: u16,
    },
    /// The server could not be reached or the connection broke.
    #[error("cannot reach {url}: {message}")]
    Network {
        /// The address that was asked.
        url: String,
        /// The HTTP client's description of the failure.
        message: String,
    },
    /// The file grew past the map size cap.
    #[error("{url} sent more than {limit} bytes")]
    TooLarge {
        /// The address that was asked.
        url: String,
        /// The cap in bytes.
        limit: u64,
    },
    /// A file in the game folder could not be written or moved.
    #[error("cannot read or write {path}: {source}")]
    Io {
        /// The file or folder involved.
        path: PathBuf,
        /// The underlying error.
        #[source]
        source: std::io::Error,
    },
    /// SQLite failed to record the download.
    #[error("map database error: {message}")]
    Database {
        /// SQLite's description of the failure.
        message: String,
    },
}

/// Why a community map was not removed from `tf/maps`.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum MapRemoveError {
    /// TF2Tools did not download this map, so it is never removed.
    #[error("{name} was not downloaded by TF2Tools")]
    NotDownloaded {
        /// The map's name.
        name: String,
    },
    /// The map file could not be deleted.
    #[error("cannot delete {path}: {source}")]
    Io {
        /// The file involved.
        path: PathBuf,
        /// The underlying error.
        #[source]
        source: std::io::Error,
    },
    /// SQLite failed to read or forget the download.
    #[error("map database error: {message}")]
    Database {
        /// SQLite's description of the failure.
        message: String,
    },
}

/// Why a store download did not end up in the library.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum DownloadError {
    /// The item is already in the library.
    #[error("{what} is already in the library")]
    AlreadyInLibrary {
        /// The item that was asked for.
        what: String,
    },
    /// The file does not fit the store it was downloaded for.
    #[error("{file_name} belongs to another store")]
    WrongKind {
        /// The store's name for the file.
        file_name: String,
    },
    /// The server kept answering HTTP 429 after every retry.
    #[error("{url} is rate limiting requests; try again later")]
    RateLimited {
        /// The address that was asked.
        url: String,
    },
    /// The server answered with a non-success status.
    #[error("{url} answered HTTP {status}")]
    Http {
        /// The address that was asked.
        url: String,
        /// The HTTP status code.
        status: u16,
    },
    /// The server could not be reached or the connection broke.
    #[error("cannot reach {url}: {message}")]
    Network {
        /// The address that was asked.
        url: String,
        /// The HTTP client's description of the failure.
        message: String,
    },
    /// The response body grew past its size cap.
    #[error("{url} sent more than {limit} bytes")]
    TooLarge {
        /// The address that was asked.
        url: String,
        /// The cap in bytes.
        limit: u64,
    },
    /// The server sent no bytes for a file listed with content, as GameBanana does when busy.
    #[error("{url} sent an empty body")]
    EmptyBody {
        /// The address of the file.
        url: String,
    },
    /// The downloaded bytes do not match the store's MD5 checksum.
    #[error("{file_name} failed its checksum: expected {expected}, got {found}")]
    ChecksumMismatch {
        /// The store's name for the file.
        file_name: String,
        /// The checksum the store published.
        expected: String,
        /// The checksum of the bytes received.
        found: String,
    },
    /// The download holds nothing this kind of item can use.
    #[error("nothing usable in this download: {reason}")]
    NoUsableFiles {
        /// Why nothing could be used.
        reason: String,
    },
    /// The import stopped before it finished, for example on a cancelled task.
    #[error("the import was interrupted: {message}")]
    Interrupted {
        /// What stopped it.
        message: String,
    },
    /// A file or directory could not be read or written.
    #[error("cannot read or write {path}: {source}")]
    Io {
        /// The path that failed.
        path: PathBuf,
        /// The underlying I/O error.
        source: std::io::Error,
    },
    /// A downloaded archive could not be extracted.
    #[error(transparent)]
    Archive(#[from] ArchiveError),
    /// The library refused the downloaded item.
    #[error(transparent)]
    Import(#[from] ImportError),
    /// An entry failed to import and entries imported before it could not all be removed.
    #[error("the download failed ({cause}) and its partial import could not be undone: {source}")]
    RollbackFailed {
        /// The entries still in the library.
        imported: Vec<LibraryEntry>,
        /// Why the import failed.
        cause: Box<DownloadError>,
        /// The first removal that failed.
        source: LibraryError,
    },
    /// SQLite failed to read or write the store tables.
    #[error("store database error: {message}")]
    Database {
        /// SQLite's description of the failure.
        message: String,
    },
}

/// Why an add-on's newest file could not be made ready to install.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum GbAddonFetchError {
    /// The item lists no files.
    #[error("GameBanana item {gb_id} has no files to install")]
    NoFiles {
        /// The item that was asked for.
        gb_id: GbId,
    },
    /// GameBanana's virus scan did not pass the file.
    #[error("{file_name} did not pass GameBanana's virus scan ({result:?})")]
    VirusScan {
        /// The store's name for the file.
        file_name: String,
        /// The scan result GameBanana reported.
        result: String,
    },
    /// The server kept answering HTTP 429 after every retry.
    #[error("{url} is rate limiting requests; try again later")]
    RateLimited {
        /// The address that was asked.
        url: String,
    },
    /// The server answered with a non-success status.
    #[error("{url} answered HTTP {status}")]
    Http {
        /// The address that was asked.
        url: String,
        /// The HTTP status code.
        status: u16,
    },
    /// The server could not be reached or the connection broke.
    #[error("cannot reach {url}: {message}")]
    Network {
        /// The address that was asked.
        url: String,
        /// The HTTP client's description of the failure.
        message: String,
    },
    /// The file is listed as, or grew past, the add-on size cap.
    #[error("{url} is more than {limit} bytes")]
    TooLarge {
        /// The address of the file.
        url: String,
        /// The cap in bytes.
        limit: u64,
    },
    /// The server sent no bytes for a file listed with content, as GameBanana does when busy.
    #[error("{url} sent an empty body")]
    EmptyBody {
        /// The address of the file.
        url: String,
    },
    /// The downloaded bytes do not match the store's MD5 checksum.
    #[error("{file_name} failed its checksum: expected {expected}, got {found}")]
    ChecksumMismatch {
        /// The store's name for the file.
        file_name: String,
        /// The checksum the store published.
        expected: String,
        /// The checksum of the bytes received.
        found: String,
    },
    /// The downloaded archive could not be extracted or walked.
    #[error(transparent)]
    Archive(#[from] ArchiveError),
    /// The download holds only programs, which TF2Tools never runs.
    #[error("{file_name} holds only programs, nothing to install")]
    OnlyExe {
        /// The store's name for the file.
        file_name: String,
    },
    /// The download's layout matches no known add-on shape: nothing installable, or
    /// more than `MAX_PAYLOADS` parts.
    #[error("{}", unknown_layout_message(.file_name, *.payloads))]
    UnknownLayout {
        /// The store's name for the file.
        file_name: String,
        /// How many payloads were found.
        payloads: usize,
    },
    /// A file or directory could not be read or written.
    #[error("cannot read or write {path}: {source}")]
    Io {
        /// The path that failed.
        path: PathBuf,
        /// The underlying I/O error.
        source: std::io::Error,
    },
    /// The extraction stopped before it finished, for example on a cancelled task.
    #[error("the download was interrupted: {message}")]
    Interrupted {
        /// What stopped it.
        message: String,
    },
}

/// Words an unknown layout by whether it found nothing or too much to choose from.
fn unknown_layout_message(file_name: &str, payloads: usize) -> String {
    if payloads == 0 {
        format!("{file_name} holds nothing TF2Tools knows how to install")
    } else {
        format!("{file_name} has {payloads} possible parts, too many to tell which to install")
    }
}

/// Why a GameBanana add-on update check stopped before every due add-on was asked.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum GbAddonCheckError {
    /// GameBanana could not be reached, was rate limiting or kept sending empty replies.
    #[error(transparent)]
    Browse(#[from] BrowseError),
    /// The library's rows could not be read or written.
    #[error(transparent)]
    Library(#[from] LibraryError),
}

/// Why a GameBanana add-on could not be installed, installed again or removed.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum GbAddonInstallError {
    /// A file or directory could not be read or written.
    #[error("cannot read or write {path}: {source}")]
    Io {
        /// The path that failed.
        path: PathBuf,
        /// The underlying I/O error.
        source: std::io::Error,
    },
    /// SQLite failed to read or record the add-on.
    #[error("library database error: {message}")]
    Database {
        /// SQLite's description of the failure.
        message: String,
    },
    /// The game folder (`tf/`) is gone, so nothing was written.
    #[error("Team Fortress 2 is no longer at {path}")]
    GameMissing {
        /// The `tf` folder that is missing.
        path: PathBuf,
    },
    /// TF2 holds VPKs open; nothing changed.
    #[error("TF2 is running and a VPK would change")]
    Tf2Running,
    /// No row, or no library copy, for the add-on.
    #[error("GameBanana add-on {gb_id} is not in the library")]
    NotInLibrary {
        /// The add-on that was asked for.
        gb_id: GbId,
    },
    /// A chosen name is not one of the download's payloads.
    #[error("{name} is not a payload of this download")]
    UnknownPayload {
        /// The chosen name.
        name: String,
    },
    /// No payload was chosen.
    #[error("no payload was chosen")]
    NothingChosen,
}

/// Why an item's audio preview could not be fetched.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum PreviewError {
    /// The item has no audio preview.
    #[error("this item has no audio preview")]
    NoPreview,
    /// The preview address is outside GameBanana's media hosts.
    #[error("{url} is not a GameBanana preview address")]
    UntrustedUrl {
        /// The rejected address.
        url: String,
    },
    /// The server kept answering HTTP 429 after every retry.
    #[error("{url} is rate limiting requests; try again later")]
    RateLimited {
        /// The address that was asked.
        url: String,
    },
    /// The server answered with a non-success status.
    #[error("{url} answered HTTP {status}")]
    Http {
        /// The address that was asked.
        url: String,
        /// The HTTP status code.
        status: u16,
    },
    /// The server could not be reached or the connection broke.
    #[error("cannot reach {url}: {message}")]
    Network {
        /// The address that was asked.
        url: String,
        /// The HTTP client's description of the failure.
        message: String,
    },
    /// The response body grew past its size cap.
    #[error("{url} sent more than {limit} bytes")]
    TooLarge {
        /// The address that was asked.
        url: String,
        /// The cap in bytes.
        limit: u64,
    },
}

/// Why a store image could not be fetched.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ImageError {
    /// The image address, or a redirect hop, is outside the store image hosts.
    #[error("{url} is not a store image address")]
    UntrustedUrl {
        /// The rejected address.
        url: String,
    },
    /// The server kept answering HTTP 429.
    #[error("{url} is rate limiting requests; try again later")]
    RateLimited {
        /// The address that was asked.
        url: String,
    },
    /// The server answered with a non-success status.
    #[error("{url} answered HTTP {status}")]
    Http {
        /// The address that was asked.
        url: String,
        /// The HTTP status code.
        status: u16,
    },
    /// The server could not be reached or the connection broke.
    #[error("cannot reach {url}: {message}")]
    Network {
        /// The address that was asked.
        url: String,
        /// The HTTP client's description of the failure.
        message: String,
    },
    /// The response body grew past its size cap.
    #[error("{url} sent more than {limit} bytes")]
    TooLarge {
        /// The address that was asked.
        url: String,
        /// The cap in bytes.
        limit: u64,
    },
}

/// Why a HUD's fonts could not be scanned or checked against the system.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum FontCheckError {
    /// The HUD directory or its scheme file could not be read.
    #[error("cannot read {path}: {source}")]
    Io {
        /// The path that failed.
        path: PathBuf,
        /// The underlying I/O error.
        source: std::io::Error,
    },
    /// The font host could not list the installed fonts.
    #[error("cannot list the installed fonts: {reason}")]
    Host {
        /// The host's description of the failure.
        reason: String,
    },
}

/// Why installing a HUD's fonts stopped; files copied before the failure stay installed.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum FontInstallError {
    /// A HUD font or the fonts directory could not be read or written.
    #[error("cannot read or write {path}: {source}")]
    Io {
        /// The path that failed.
        path: PathBuf,
        /// The underlying I/O error.
        source: std::io::Error,
    },
    /// The host could not name the user font directory.
    #[error("cannot find the user font directory: {reason}")]
    Host {
        /// The host's description of the failure.
        reason: String,
    },
    /// The host refused to register a copied font.
    #[error("cannot register the font {name}: {reason}")]
    Register {
        /// The full name passed to the host.
        name: String,
        /// The host's description of the failure.
        reason: String,
    },
    /// Every font was copied and registered but the system was not told.
    #[error("fonts were copied but the system was not told: {reason}")]
    Refresh {
        /// The host's description of the failure.
        reason: String,
    },
}

/// Why an update check found no usable answer.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum UpdateCheckError {
    /// The server could not be reached or the connection broke.
    #[error("{url}: {message}")]
    Network {
        /// The address that was asked.
        url: String,
        /// What went wrong.
        message: String,
    },
    /// The server answered with a non-success status.
    #[error("{url} answered HTTP {status}")]
    Status {
        /// The address that was asked.
        url: String,
        /// The HTTP status code.
        status: u16,
    },
    /// The response body exceeded its cap.
    #[error("{url} is larger than {limit} bytes")]
    TooLarge {
        /// The address that was asked.
        url: String,
        /// The cap in bytes.
        limit: u64,
    },
    /// No usable public key: the list is empty or a key does not decode.
    #[error("update key: {message}")]
    BadKey {
        /// Why the key was rejected.
        message: String,
    },
    /// The manifest signature is missing or does not verify under any key.
    #[error("the update could not be verified")]
    BadSignature,
    /// The manifest is not valid JSON or breaks a manifest rule.
    #[error("malformed update manifest: {message}")]
    Malformed {
        /// The field or rule that failed.
        message: String,
    },
}

/// Why an offered update could not be downloaded and staged.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum UpdateDownloadError {
    /// The server could not be reached or the connection broke.
    #[error("{url}: {message}")]
    Network {
        /// The address that was asked.
        url: String,
        /// What went wrong.
        message: String,
    },
    /// The server answered with a non-success status.
    #[error("{url} answered HTTP {status}")]
    Status {
        /// The address that was asked.
        url: String,
        /// The HTTP status code.
        status: u16,
    },
    /// The body was not the size the manifest declared.
    #[error("the update should be {expected} bytes but {received} arrived")]
    SizeMismatch {
        /// The size the manifest declared.
        expected: u64,
        /// The bytes read; past `expected` the read stops one chunk over it.
        received: u64,
    },
    /// The body's sha256 does not match the manifest.
    #[error("the update's sha256 is {actual}, expected {expected}")]
    HashMismatch {
        /// The digest the manifest declared.
        expected: String,
        /// The digest of what arrived.
        actual: String,
    },
    /// A filesystem operation on the staged file failed.
    #[error("{}: {source}", path.display())]
    Io {
        /// The file involved.
        path: PathBuf,
        /// The underlying error.
        source: std::io::Error,
    },
    /// The update cannot be written beside the install for lack of permission.
    #[error("{} is not writable", dir.display())]
    NotWritable {
        /// The directory the update would be staged in.
        dir: PathBuf,
    },
    /// The offer carries no payload, so it can only be notified.
    #[error("this update has no download for this install")]
    NoAsset,
    /// This install kind cannot download and apply an update itself.
    #[error("this install cannot update itself")]
    Unsupported,
}

/// Why a staged update could not be swapped in for the running install.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum UpdateApplyError {
    /// The backup or the swap was refused for lack of permission or a read-only filesystem.
    #[error("{} is not writable", path.display())]
    NotWritable {
        /// The file that could not be written.
        path: PathBuf,
    },
    /// A filesystem operation failed for another reason.
    #[error("{}: {source}", path.display())]
    Io {
        /// The file involved.
        path: PathBuf,
        /// The underlying error.
        source: std::io::Error,
    },
    /// The install is not an AppImage this can replace, or a file involved is not a
    /// regular file.
    #[error("this install cannot update itself")]
    Unsupported {
        /// The offending file, when one is known.
        path: Option<PathBuf>,
    },
    /// The installer could not be started.
    #[error("could not start {}: {source}", path.display())]
    Spawn {
        /// The installer that failed to start.
        path: PathBuf,
        /// The underlying error.
        source: std::io::Error,
    },
    /// The staged file no longer matches the sha256 it was downloaded with.
    #[error("{} changed after it was downloaded", path.display())]
    Changed {
        /// The staged file.
        path: PathBuf,
    },
}

/// Why a KeyValues file could not be read as a document.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum KvParseError {
    /// The file is larger than the 8 MiB limit.
    #[error("the file is {bytes} bytes, over the 8 MB limit")]
    TooLarge {
        /// The file's size.
        bytes: u64,
    },
    /// More than 64 blocks are open at once.
    #[error("blocks nest deeper than 64 levels")]
    TooDeep,
    /// The file starts with a UTF-16 byte order mark; only byte-oriented text is read.
    #[error("the file is UTF-16 text")]
    Utf16,
}

/// Why the game's files could not be opened or one of them read.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum HudFilesError {
    /// The folder has no readable `gameinfo.txt`.
    #[error("{path} has no gameinfo.txt")]
    NoGameInfo {
        /// The folder that was opened.
        path: PathBuf,
    },
    /// A library HUD folder is itself a link, so it is not opened.
    #[error("{path} is a link")]
    LinkedFolder {
        /// The folder that was refused.
        path: PathBuf,
    },
    /// A loose file could not be read.
    #[error("cannot read {path}: {source}")]
    Io {
        /// The file that failed.
        path: PathBuf,
        /// The underlying I/O error.
        source: std::io::Error,
    },
    /// A listed VPK, or an entry in one, could not be read.
    #[error(transparent)]
    Vpk(#[from] VpkError),
    /// A file is larger than the 64 MiB limit.
    #[error("{path} is {bytes} bytes, over the 64 MB limit")]
    TooLarge {
        /// The game path of the file.
        path: String,
        /// The file's size.
        bytes: u64,
    },
}

/// Why a copy of a library HUD could not be made.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum CopyHudError {
    /// No library HUD has this id.
    #[error("HUD {id} is not in the library")]
    NotFound {
        /// The HUD's id.
        id: String,
    },
    /// The HUD's library folder is gone or is a link.
    #[error("HUD {id}'s files are missing")]
    BaseMissing {
        /// The HUD's id.
        id: String,
    },
    /// The HUD was made in the HUD Maker; Save as covers it.
    #[error("HUD {id} was made in the HUD Maker")]
    MadeHud {
        /// The HUD's id.
        id: String,
    },
    /// Every copy name from "(copy)" to "(copy 99)" is taken.
    #[error("no free copy name for {name}")]
    NameTaken {
        /// The base HUD's name.
        name: String,
    },
    /// The disk holding the library is short of room for the copy.
    #[error("needs {needed} bytes, {free} free")]
    NoSpace {
        /// Bytes the copy needs, headroom included.
        needed: u64,
        /// Bytes free on the disk.
        free: u64,
    },
    /// A file or folder could not be read or written.
    #[error("cannot write {path}: {source}")]
    Io {
        /// The path that failed.
        path: PathBuf,
        /// The underlying I/O error.
        source: std::io::Error,
    },
    /// The library database failed.
    #[error("library database error: {message}")]
    Database {
        /// The database's message.
        message: String,
    },
}

/// Why a HUD copy could not be updated to a newer base.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum UpdateCopyError {
    /// No library HUD has the copy's id.
    #[error("HUD {id} is not in the library")]
    NotFound {
        /// The copy's id.
        id: String,
    },
    /// The HUD is no copy of a library HUD.
    #[error("HUD {id} is not a copy")]
    NotACopy {
        /// The HUD's id.
        id: String,
    },
    /// The HUD to update from is not in the library.
    #[error("HUD {id} to update from is gone")]
    SourceGone {
        /// The id given to update from.
        id: String,
    },
    /// The HUD to update from is not a newer version from the copy's own source.
    #[error("HUD {id} is not this copy's source")]
    NotSameSource {
        /// The id given to update from.
        id: String,
    },
    /// The HUD to update from was made in the HUD Maker.
    #[error("HUD {id} was made in the HUD Maker")]
    MadeHud {
        /// The id given to update from.
        id: String,
    },
    /// The disk holding the library is short of room for the new base.
    #[error("needs {needed} bytes, {free} free")]
    NoSpace {
        /// Bytes the update needs, headroom included.
        needed: u64,
        /// Bytes free on the disk.
        free: u64,
    },
    /// The copy or its files could not be read.
    #[error(transparent)]
    Load(#[from] HudLoadError),
    /// The updated copy could not be written.
    #[error(transparent)]
    Save(#[from] MakeHudError),
}

/// Why a made HUD could not be saved into the library.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum MakeHudError {
    /// The name is empty once trimmed.
    #[error("the HUD name is blank")]
    EmptyName,
    /// Another library HUD has the name, ignoring case.
    #[error("a HUD called {name} already exists")]
    NameTaken {
        /// The trimmed name.
        name: String,
    },
    /// No library HUD has this id.
    #[error("HUD {id} is not in the library")]
    NotFound {
        /// The HUD's id.
        id: String,
    },
    /// The HUD has no `maker.json`, so it is never overwritten.
    #[error("HUD {id} was not made in the HUD Maker")]
    NotMade {
        /// The HUD's id.
        id: String,
    },
    /// The disk holding the library is short of room for a whole copy.
    #[error("needs {needed} bytes, {free} free")]
    NoSpace {
        /// Bytes the copy needs, headroom included.
        needed: u64,
        /// Bytes free on the disk.
        free: u64,
    },
    /// The game's files could not be read.
    #[error(transparent)]
    Files(#[from] HudFilesError),
    /// A file or folder could not be read or written.
    #[error("cannot read or write {path}: {source}")]
    Io {
        /// The path that failed.
        path: PathBuf,
        /// The underlying I/O error.
        source: std::io::Error,
    },
    /// The library database failed.
    #[error("library database error: {message}")]
    Database {
        /// The database's message.
        message: String,
    },
    /// The copy's base changed (an Update) after the page saving it was opened.
    #[error("the HUD copy was updated after it was opened")]
    BaseChanged,
}

/// Why another library HUD could not be opened for borrowing from.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum BorrowSourceError {
    /// No library HUD has this id.
    #[error("no library HUD {id}")]
    NotFound {
        /// The id that was asked for.
        id: String,
    },
    /// The HUD's folder or the library database could not be read.
    #[error("the HUD's files cannot be read")]
    Unreadable,
    /// The game's files could not be read.
    #[error(transparent)]
    Files(#[from] HudFilesError),
}

/// Why a recipe file could not be read.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum RecipeError {
    /// The bytes are not a recipe file this or any build could have written.
    #[error("not a recipe: {reason}")]
    Damaged {
        /// What was wrong.
        reason: String,
    },
    /// The file uses a version this build does not know.
    #[error("made with a newer TF2Tools")]
    Newer,
}

/// Why a HUD Maker preset could not be saved, loaded or deleted.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum PresetError {
    /// The name is empty once trimmed.
    #[error("a preset needs a name")]
    NameBlank,
    /// The changes hold no option choice.
    #[error("there are no option choices to save")]
    NoOptions,
    /// Another preset has this name, ignoring case.
    #[error("a preset named {name} already exists")]
    NameTaken {
        /// The name asked for.
        name: String,
    },
    /// No preset file has this name.
    #[error("no such preset")]
    NotFound,
    /// The preset file is not a readable recipe.
    #[error(transparent)]
    Recipe(#[from] RecipeError),
    /// Reading or writing the presets folder failed.
    #[error("cannot write or read {path}: {source}")]
    Io {
        /// The file or folder involved.
        path: PathBuf,
        /// The underlying error.
        source: std::io::Error,
    },
}

/// Why a made HUD's `maker.json` could not be loaded.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum HudLoadError {
    /// The file is not a `maker.json` this or any build could have written.
    #[error("maker.json is damaged: {reason}")]
    Damaged {
        /// What was wrong.
        reason: String,
    },
    /// The file uses a version, base or change kind this build does not know.
    #[error("made with a newer TF2Tools")]
    Newer,
    /// No library HUD has this id.
    #[error("HUD {id} is not in the library")]
    NotFound {
        /// The HUD's id.
        id: String,
    },
    /// The HUD has no `maker.json`.
    #[error("HUD {id} was not made in the HUD Maker")]
    NotMade {
        /// The HUD's id.
        id: String,
    },
    /// The game's files could not be read.
    #[error(transparent)]
    Files(#[from] HudFilesError),
    /// A file could not be read.
    #[error("cannot read {path}: {source}")]
    Io {
        /// The file that failed.
        path: PathBuf,
        /// The underlying I/O error.
        source: std::io::Error,
    },
    /// The library database failed.
    #[error("library database error: {message}")]
    Database {
        /// The database's message.
        message: String,
    },
}

/// Why a made HUD's history snapshot could not be saved, listed, read or deleted.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum HistoryError {
    /// The name is empty once cleaned.
    #[error("a snapshot needs a name")]
    NameBlank,
    /// The HUD already keeps the most snapshots allowed.
    #[error("the HUD already has {limit} snapshots")]
    Full {
        /// The most snapshots a HUD keeps.
        limit: usize,
    },
    /// The snapshot would hold more changes, or be a larger file, than one snapshot may.
    #[error("too many changes for one snapshot")]
    TooLarge,
    /// No library HUD has this id, or it was not made in the HUD Maker.
    #[error("HUD {id} is not a made HUD in the library")]
    NotMade {
        /// The HUD's id.
        id: String,
    },
    /// No snapshot file has this name.
    #[error("no such snapshot")]
    NotFound,
    /// The file is not a snapshot this or any build could have written.
    #[error("not a snapshot: {reason}")]
    Damaged {
        /// What was wrong.
        reason: String,
    },
    /// The file uses a version this build does not know.
    #[error("made with a newer TF2Tools")]
    Newer,
    /// A file or folder could not be read or written.
    #[error("cannot read or write {path}: {source}")]
    Io {
        /// The path that failed.
        path: PathBuf,
        /// The underlying I/O error.
        source: std::io::Error,
    },
    /// The library database failed.
    #[error("library database error: {message}")]
    Database {
        /// The database's message.
        message: String,
    },
}

/// Why a HUD option schema could not be read.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum SchemaError {
    /// The file, a string in it or its control count is past a cap.
    #[error("too large")]
    TooLarge,
    /// The file is no JSON object with a `Controls` map of control lists.
    #[error("not a schema: {reason}")]
    Malformed {
        /// What was wrong, for the developer log.
        reason: String,
    },
}

/// Why mastercomfig's install state could not be read.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum MastercomfigStatusError {
    /// The library database failed.
    #[error("library database error: {message}")]
    Database {
        /// The database's message.
        message: String,
    },
}

/// Why mastercomfig could not be removed from `tf/custom`.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum RemoveMastercomfigError {
    /// TF2Tools did not install this mastercomfig, so it leaves it alone.
    #[error("mastercomfig was not installed by TF2Tools")]
    NotManaged,
    /// The game folder (`tf/`) is no longer where the install was found.
    #[error("Team Fortress 2 is no longer at {path}")]
    GameMissing {
        /// The `tf` folder that is gone.
        path: PathBuf,
    },
    /// A file could not be read or removed.
    #[error("cannot read or write {path}: {source}")]
    Io {
        /// The path that failed.
        path: PathBuf,
        /// The underlying I/O error.
        source: std::io::Error,
    },
    /// The library database failed.
    #[error("library database error: {message}")]
    Database {
        /// The database's message.
        message: String,
    },
}

/// Why mastercomfig-base.vpk could not be installed or updated.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum InstallMastercomfigError {
    /// No addon-store sync has recorded a mastercomfig release yet.
    #[error("no mastercomfig release is known yet")]
    NoRelease,
    /// The game folder (`tf/`) is no longer where the install was found.
    #[error("Team Fortress 2 is no longer at {path}")]
    GameMissing {
        /// The `tf` folder that is gone.
        path: PathBuf,
    },
    /// The file that arrived is not a VPK.
    #[error("the file at {url} is not a VPK")]
    NotAVpk {
        /// The address that was asked.
        url: String,
    },
    /// The server kept answering HTTP 429 after every retry.
    #[error("{url} is rate limiting requests; try again later")]
    RateLimited {
        /// The address that was asked.
        url: String,
    },
    /// The server answered with a non-success status.
    #[error("{url} answered HTTP {status}")]
    Http {
        /// The address that was asked.
        url: String,
        /// The HTTP status code.
        status: u16,
    },
    /// The server could not be reached or the connection broke.
    #[error("cannot reach {url}: {message}")]
    Network {
        /// The address that was asked.
        url: String,
        /// The HTTP client's description of the failure.
        message: String,
    },
    /// The file grew past the addon size cap.
    #[error("{url} sent more than {limit} bytes")]
    TooLarge {
        /// The address that was asked.
        url: String,
        /// The cap in bytes.
        limit: u64,
    },
    /// A file could not be written, renamed or removed.
    #[error("cannot read or write {path}: {source}")]
    Io {
        /// The path that failed.
        path: PathBuf,
        /// The underlying I/O error.
        source: std::io::Error,
    },
    /// The library database failed.
    #[error("library database error: {message}")]
    Database {
        /// The database's message.
        message: String,
    },
}

/// Why a mastercomfig preset could not be applied.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ApplyPresetError {
    /// TF2Tools does not manage this mastercomfig, so it does not pick its preset.
    #[error("mastercomfig was not installed by TF2Tools")]
    NotManaged,
    /// The game folder (`tf/`) is no longer where the install was found.
    #[error("Team Fortress 2 is no longer at {path}")]
    GameMissing {
        /// The `tf` folder that is gone.
        path: PathBuf,
    },
    /// The preset file could not be written.
    #[error("cannot read or write {path}: {source}")]
    Io {
        /// The path that failed.
        path: PathBuf,
        /// The underlying I/O error.
        source: std::io::Error,
    },
    /// The library database failed.
    #[error("library database error: {message}")]
    Database {
        /// The database's message.
        message: String,
    },
}

/// Why TF2Tools could not take over a mastercomfig installed by hand.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum TakeOverError {
    /// There is no `mastercomfig-base.vpk` in `tf/custom` to take over.
    #[error("mastercomfig is not installed")]
    NotInstalled,
    /// TF2Tools already manages this mastercomfig.
    #[error("TF2Tools already manages mastercomfig")]
    AlreadyManaged,
    /// The library database failed.
    #[error("library database error: {message}")]
    Database {
        /// The database's message.
        message: String,
    },
}

/// Why mastercomfig's cleared files could not be written back.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum RestoreMastercomfigError {
    /// A file could not be read or written.
    #[error("cannot read or write {path}: {source}")]
    Io {
        /// The path that failed.
        path: PathBuf,
        /// The underlying I/O error.
        source: std::io::Error,
    },
    /// The library database failed.
    #[error("library database error: {message}")]
    Database {
        /// The database's message.
        message: String,
    },
}

/// Why the player's mastercomfig module choices could not be applied.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ApplyModulesError {
    /// TF2Tools does not manage this mastercomfig install.
    #[error("mastercomfig is not installed by TF2Tools")]
    NotManaged,
    /// `modules.cfg` exists and TF2Tools did not write it.
    #[error("{path} was not written by TF2Tools")]
    ForeignModulesCfg {
        /// The player's own `modules.cfg`.
        path: PathBuf,
    },
    /// The game folder (`tf/`) is no longer where the install was found.
    #[error("Team Fortress 2 is no longer at {path}")]
    GameMissing {
        /// The `tf` folder that is gone.
        path: PathBuf,
    },
    /// A file could not be read, written, renamed or removed.
    #[error("cannot read or write {path}: {source}")]
    Io {
        /// The path that failed.
        path: PathBuf,
        /// The underlying I/O error.
        source: std::io::Error,
    },
    /// The library database failed.
    #[error("library database error: {message}")]
    Database {
        /// The database's message.
        message: String,
    },
    /// No addon-store sync has recorded a mastercomfig release yet.
    #[error("no mastercomfig release is known yet")]
    NoRelease,
    /// The override file comes from the recorded release, which is newer than the installed one.
    #[error("mastercomfig {installed} is installed but the override file is for {latest}")]
    UpdateWaiting {
        /// The installed mastercomfig version.
        installed: String,
        /// The recorded release tag.
        latest: String,
    },
    /// The override file that arrived is not a VPK.
    #[error("the file at {url} is not a VPK")]
    NotAVpk {
        /// The address that was asked.
        url: String,
    },
    /// The server kept answering HTTP 429 after every retry.
    #[error("{url} is rate limiting requests; try again later")]
    RateLimited {
        /// The address that was asked.
        url: String,
    },
    /// The server answered with a non-success status.
    #[error("{url} answered HTTP {status}")]
    Http {
        /// The address that was asked.
        url: String,
        /// The HTTP status code.
        status: u16,
    },
    /// The server could not be reached or the connection broke.
    #[error("cannot reach {url}: {message}")]
    Network {
        /// The address that was asked.
        url: String,
        /// The HTTP client's description of the failure.
        message: String,
    },
    /// The override file grew past the addon size cap.
    #[error("{url} sent more than {limit} bytes")]
    TooLarge {
        /// The address that was asked.
        url: String,
        /// The cap in bytes.
        limit: u64,
    },
}

/// Why a mastercomfig release's module catalog could not be read.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ModuleCatalogError {
    /// `modules.json` or `preset_modules.json` is not in the shape mastercomfig ships.
    #[error("mastercomfig's module list is not readable: {reason}")]
    Format {
        /// What the JSON parser reported.
        reason: String,
    },
    /// The release tag cannot name a cache folder.
    #[error("unsupported mastercomfig release tag {tag:?}")]
    UnsupportedTag {
        /// The rejected tag.
        tag: String,
    },
    /// The server kept answering HTTP 429 after every retry.
    #[error("{url} is rate limiting requests; try again later")]
    RateLimited {
        /// The address that was asked.
        url: String,
    },
    /// The server answered with a non-success status.
    #[error("{url} answered HTTP {status}")]
    Http {
        /// The address that was asked.
        url: String,
        /// The HTTP status code.
        status: u16,
    },
    /// The server could not be reached or the connection broke.
    #[error("cannot reach {url}: {message}")]
    Network {
        /// The address that was asked.
        url: String,
        /// The HTTP client's description of the failure.
        message: String,
    },
    /// The file grew past the size cap.
    #[error("{url} sent more than {limit} bytes")]
    TooLarge {
        /// The address that was asked.
        url: String,
        /// The cap in bytes.
        limit: u64,
    },
}

/// Why the player box text could not be read.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum PlayerInputError {
    /// The text is no Steam link, ID or vanity name.
    #[error("That doesn't look like a Steam profile link or Steam ID.")]
    NotSteam,
}

/// Why the logs.tf cache could not be read or written.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum LogsCacheError {
    /// SQLite failed on one of the `logstf_` tables or the active-player setting.
    #[error("the logs.tf cache could not be read or written: {message}")]
    Database {
        /// SQLite's description of the failure.
        message: String,
    },
}

/// Why a Steam profile could not be looked up.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ResolveError {
    /// Steam answered that no such profile exists.
    #[error("We couldn't find that Steam profile.")]
    NotFound,
    /// Steam kept answering HTTP 429 after every retry.
    #[error("{url} is rate limiting requests; try again later")]
    RateLimited {
        /// The address that was asked.
        url: String,
    },
    /// Steam answered with a non-success status.
    #[error("{url} answered HTTP {status}")]
    Http {
        /// The address that was asked.
        url: String,
        /// The HTTP status code.
        status: u16,
    },
    /// Steam could not be reached or the connection broke.
    #[error("cannot reach {url}: {message}")]
    Network {
        /// The address that was asked.
        url: String,
        /// The HTTP client's description of the failure.
        message: String,
    },
    /// The profile body grew past its size cap.
    #[error("{url} sent more than {limit} bytes")]
    TooLarge {
        /// The address that was asked.
        url: String,
        /// The cap in bytes.
        limit: u64,
    },
    /// The profile body named neither a Steam ID nor an error.
    #[error("unexpected response from {url}: {message}")]
    Malformed {
        /// The address that was asked.
        url: String,
        /// What was wrong with the body.
        message: String,
    },
}

/// Why a player's logs.tf sync stopped.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum LogsSyncError {
    /// logs.tf kept answering HTTP 429 after every retry.
    #[error("{url} is rate limiting requests; try again later")]
    RateLimited {
        /// The address that was asked.
        url: String,
    },
    /// logs.tf answered with a non-success status.
    #[error("{url} answered HTTP {status}")]
    Http {
        /// The address that was asked.
        url: String,
        /// The HTTP status code.
        status: u16,
    },
    /// logs.tf could not be reached or the connection broke.
    #[error("cannot reach {url}: {message}")]
    Network {
        /// The address that was asked.
        url: String,
        /// The HTTP client's description of the failure.
        message: String,
    },
    /// The log list grew past its size cap.
    #[error("{url} sent more than {limit} bytes")]
    TooLarge {
        /// The address that was asked.
        url: String,
        /// The cap in bytes.
        limit: u64,
    },
    /// The log list or a log body was not the JSON logs.tf sends.
    #[error("unexpected response from {url}: {message}")]
    Malformed {
        /// The address that was asked.
        url: String,
        /// What was wrong with the body.
        message: String,
    },
    /// SQLite failed while writing the cache.
    #[error("the logs.tf cache could not be read or written: {message}")]
    Database {
        /// SQLite's description of the failure.
        message: String,
    },
    /// The player has no row in the tracked-player list.
    #[error("{steam_id} is not a tracked player")]
    NotTracked {
        /// The player's SteamID64.
        steam_id: u64,
    },
}

impl From<LogsCacheError> for LogsSyncError {
    fn from(e: LogsCacheError) -> Self {
        match e {
            LogsCacheError::Database { message } => LogsSyncError::Database { message },
        }
    }
}

/// Why a log opened before the sync reached it could not be fetched.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum LogsFetchError {
    /// logs.tf no longer has the log; it is marked gone.
    #[error("logs.tf no longer has this log")]
    Gone,
    /// The body was not UTF-8 or not a log; it is marked unreadable.
    #[error("logs.tf sent a log that could not be read")]
    Unreadable,
    /// logs.tf could not be reached, answered an error status or redirected off the allowlist.
    #[error("cannot reach {url}: {message}")]
    Network {
        /// The address that was asked.
        url: String,
        /// What went wrong.
        message: String,
    },
    /// logs.tf kept answering HTTP 429 after every retry.
    #[error("{url} is rate limiting requests; try again later")]
    RateLimited {
        /// The address that was asked.
        url: String,
    },
    /// The body grew past its size cap; it is marked unreadable.
    #[error("{url} sent more than {limit} bytes")]
    TooLarge {
        /// The address that was asked.
        url: String,
        /// The cap in bytes.
        limit: u64,
    },
    /// SQLite failed while writing the cache.
    #[error("the logs.tf cache could not be read or written: {message}")]
    Cache {
        /// SQLite's description of the failure.
        message: String,
    },
}

impl From<LogsCacheError> for LogsFetchError {
    fn from(e: LogsCacheError) -> Self {
        match e {
            LogsCacheError::Database { message } => LogsFetchError::Cache { message },
        }
    }
}

/// Why a stored logs.tf body could not be read as a log.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum LogParseError {
    /// The body is not JSON or lacks `length`, `teams` or `players`.
    #[error("the log could not be read: {message}")]
    Format {
        /// What the JSON reader objected to.
        message: String,
    },
}
