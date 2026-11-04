//! Self-update: a signed manifest names the newest release per install kind, and the
//! running install downloads and swaps in its payload only after the signature, size
//! and sha256 all check out.

mod install;
mod keys;
mod manifest;

pub use install::{Applied, InstallTarget, apply};
pub use keys::UPDATE_PUBLIC_KEYS;
pub use manifest::{UpdateAsset, UpdateOffer};

use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};
use tokio::io::AsyncWriteExt;

use crate::error::{UpdateCheckError, UpdateDownloadError};
use crate::store::http::{Http, NetFailure, Redirects};
use crate::store::{Endpoints, StoreConfig, http_for};
use keys::Keys;
use manifest::{AssetKind, Manifest};

/// The manifest's cap; a real one is a few hundred bytes.
const MANIFEST_LIMIT: u64 = 64 * 1024;
/// The detached signature's cap; a .minisig file is about 300 bytes.
const SIGNATURE_LIMIT: u64 = 4 * 1024;

/// Checks the release feed for a newer version, trusting only manifests signed by its keys.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct UpdateClient {
    http: Http,
    keys: Keys,
    github: String,
    staging: Option<PathBuf>,
}

impl UpdateClient {
    /// Uses `endpoints.github()` and `config`'s User-Agent, timeouts and loopback_only,
    /// following any redirect; `keys` are base64 minisign public keys, any of which may sign.
    /// BadKey when `keys` is empty or any key does not decode.
    pub fn new(
        endpoints: &Endpoints,
        config: &StoreConfig,
        keys: &[&str],
    ) -> Result<Self, UpdateCheckError> {
        // GitHub's release downloads redirect to another host; integrity rests on the
        // signature and sha256, not the transport, so any redirect is followed.
        Ok(Self {
            http: http_for(config, &Redirects::Any),
            keys: Keys::decode(keys)?,
            github: endpoints.github().to_owned(),
            staging: None,
        })
    }

    /// Stages Windows installers in `dir` instead of `std::env::temp_dir()`.
    pub fn with_staging_dir(mut self, dir: impl Into<PathBuf>) -> Self {
        self.staging = Some(dir.into());
        self
    }

    /// Fetches `{github}/PhongGuy/TF2Tools/releases/latest/download/tf2tools-update.json`
    /// (cap 64 KiB), then the same URL plus `.minisig` (cap 4 KiB), verifies, parses, and
    /// selects for `target` on this OS and arch. Ok(None) when nothing newer applies.
    pub async fn check(
        &self,
        current: &str,
        target: &InstallTarget,
    ) -> Result<Option<UpdateOffer>, UpdateCheckError> {
        let url = format!(
            "{}/PhongGuy/TF2Tools/releases/latest/download/tf2tools-update.json",
            self.github
        );
        let manifest = self.fetch(&url, MANIFEST_LIMIT).await?;
        // A missing signature is an unverifiable manifest, not a server fault.
        let signature = match self.fetch(&format!("{url}.minisig"), SIGNATURE_LIMIT).await {
            Err(UpdateCheckError::Status { status: 404, .. }) => {
                return Err(UpdateCheckError::BadSignature);
            }
            other => other?,
        };
        // Nothing is parsed until the exact bytes served have verified.
        self.keys.verify(&manifest, &signature)?;
        let manifest = Manifest::parse(&manifest, &self.github)?;
        manifest::select(
            &manifest,
            current,
            target,
            std::env::consts::OS,
            std::env::consts::ARCH,
            &self.github,
        )
    }

    /// Streams the offer's asset to a part file, stops past `size` bytes, checks size and
    /// sha256, then renames it into place. An AppImage stages `<dir>/.tf2tools-update-<version>`
    /// beside itself; the Windows installer stages `<staging>/tf2tools-update-<version>-setup.exe`
    /// (`std::env::temp_dir()` unless `with_staging_dir`). The part file is the staged name plus
    /// `.part`, and any failure after it exists deletes it. Unsupported, with nothing fetched,
    /// for an Unmanaged target or an asset of another install kind.
    /// `progress(received, size)` runs after every chunk.
    pub async fn download(
        &self,
        offer: &UpdateOffer,
        target: &InstallTarget,
        progress: impl Fn(u64, u64) + Send,
    ) -> Result<StagedUpdate, UpdateDownloadError> {
        // An AppImage stages beside itself so the later swap is a same-directory rename;
        // the installer is a separate program, so any writable dir will do.
        let version = offer.version();
        let (dir, kind, part, staged) = match target {
            InstallTarget::AppImage(appimage) => {
                let dir = match appimage.parent() {
                    Some(dir) if !dir.as_os_str().is_empty() => dir.to_path_buf(),
                    _ => return Err(UpdateDownloadError::Unsupported),
                };
                let part = dir.join(format!(".tf2tools-update-{version}.part"));
                let staged = dir.join(format!(".tf2tools-update-{version}"));
                (dir, AssetKind::AppImage, part, staged)
            }
            // Windows only runs a program by its `.exe` name, so the suffix is kept.
            InstallTarget::WindowsInstaller => {
                let dir = self.staging.clone().unwrap_or_else(std::env::temp_dir);
                let part = dir.join(format!("tf2tools-update-{version}-setup.exe.part"));
                let staged = dir.join(format!("tf2tools-update-{version}-setup.exe"));
                (dir, AssetKind::Nsis, part, staged)
            }
            InstallTarget::Unmanaged => return Err(UpdateDownloadError::Unsupported),
        };
        let asset = offer.asset().ok_or(UpdateDownloadError::NoAsset)?;
        // An offer made to another install kind carries the wrong payload for this one.
        if asset.kind() != kind {
            return Err(UpdateDownloadError::Unsupported);
        }
        let create_failure = |source: std::io::Error, path: &Path| match source.kind() {
            std::io::ErrorKind::PermissionDenied | std::io::ErrorKind::ReadOnlyFilesystem => {
                UpdateDownloadError::NotWritable { dir: dir.clone() }
            }
            _ => UpdateDownloadError::Io {
                path: path.to_path_buf(),
                source,
            },
        };
        // A part file or symlink a crash or another user left is removed, then the part is
        // created fresh, so the download never writes through a planted link.
        match tokio::fs::remove_file(&part).await {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => {
                return Err(create_failure(e, &part));
            }
            _ => {}
        }
        let file = tokio::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&part)
            .await
            .map_err(|e| create_failure(e, &part))?;
        // From here on, any exit before the rename, including a dropped future, removes it.
        let guard = PartGuard(Some(part.clone()));
        self.stream(asset, file, &part, progress).await?;
        tokio::fs::rename(&part, &staged)
            .await
            .map_err(|source| UpdateDownloadError::Io {
                path: staged.clone(),
                source,
            })?;
        guard.disarm();
        Ok(StagedUpdate {
            path: staged,
            version: version.to_owned(),
            target: target.clone(),
            sha256: asset.sha256().to_owned(),
        })
    }

    /// Writes the asset's body into `file`, hashing as it goes, and closes it once the
    /// size and sha256 match the manifest.
    async fn stream(
        &self,
        asset: &UpdateAsset,
        mut file: tokio::fs::File,
        part: &Path,
        progress: impl Fn(u64, u64) + Send,
    ) -> Result<(), UpdateDownloadError> {
        let io = |source| UpdateDownloadError::Io {
            path: part.to_path_buf(),
            source,
        };
        let expected = asset.size();
        let mut body = self.http.open(asset.url()).await?;
        let mut hasher = Sha256::new();
        let mut received = 0u64;
        while let Some(chunk) = body.chunk().await? {
            received += chunk.len() as u64;
            // The manifest's size is a hard cap: a longer body is never written out in full.
            if received > expected {
                return Err(UpdateDownloadError::SizeMismatch { expected, received });
            }
            file.write_all(&chunk).await.map_err(io)?;
            hasher.update(&chunk);
            progress(received, expected);
        }
        if received != expected {
            return Err(UpdateDownloadError::SizeMismatch { expected, received });
        }
        let actual = hex(&hasher.finalize());
        if actual != asset.sha256() {
            return Err(UpdateDownloadError::HashMismatch {
                expected: asset.sha256().to_owned(),
                actual,
            });
        }
        // Tokio finishes writes in the background; flush, then sync so a crash after the
        // rename cannot leave a staged file with missing bytes.
        file.flush().await.map_err(io)?;
        file.sync_all().await.map_err(io)?;
        drop(file);
        Ok(())
    }

    async fn fetch(&self, url: &str, limit: u64) -> Result<Vec<u8>, UpdateCheckError> {
        Ok(self.http.open(url).await?.read_to_end(limit).await?)
    }
}

impl From<NetFailure> for UpdateCheckError {
    fn from(failure: NetFailure) -> Self {
        match failure {
            NetFailure::Network { url, message } => Self::Network { url, message },
            NetFailure::Status { url, status } => Self::Status { url, status },
            NetFailure::TooLarge { url, limit } => Self::TooLarge { url, limit },
            NetFailure::RateLimited { url } => Self::Status { url, status: 429 },
            NetFailure::Untrusted { url } => Self::Network {
                message: format!("redirect to {url} refused"),
                url,
            },
        }
    }
}

impl From<NetFailure> for UpdateDownloadError {
    fn from(failure: NetFailure) -> Self {
        match failure {
            NetFailure::Network { url, message } => Self::Network { url, message },
            NetFailure::Status { url, status } => Self::Status { url, status },
            NetFailure::TooLarge { url, limit } => Self::Network {
                message: format!("larger than {limit} bytes"),
                url,
            },
            NetFailure::RateLimited { url } => Self::Status { url, status: 429 },
            NetFailure::Untrusted { url } => Self::Network {
                message: format!("redirect to {url} refused"),
                url,
            },
        }
    }
}

/// A downloaded, size- and hash-checked update waiting to be applied.
#[derive(Debug)]
#[non_exhaustive]
pub struct StagedUpdate {
    path: PathBuf,
    version: String,
    target: InstallTarget,
    sha256: String,
}

impl StagedUpdate {
    /// The staged file: `<dir>/.tf2tools-update-<version>` beside the AppImage, or
    /// `<staging>/tf2tools-update-<version>-setup.exe` for the Windows installer.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// The staged version as semver text.
    pub fn version(&self) -> &str {
        &self.version
    }

    /// The install the update was staged for.
    pub fn target(&self) -> &InstallTarget {
        &self.target
    }
}

/// Lowercase hex, the form manifest digests are kept in.
fn hex(digest: &[u8]) -> String {
    digest.iter().map(|b| format!("{b:02x}")).collect()
}

/// The sha256 of everything `reader` yields, as lowercase hex.
fn sha256_hex(mut reader: impl std::io::Read) -> std::io::Result<String> {
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 64 * 1024];
    loop {
        match reader.read(&mut buf)? {
            0 => return Ok(hex(&hasher.finalize())),
            n => hasher.update(&buf[..n]),
        }
    }
}

/// Removes the part file when dropped, so an error or a cancelled download leaves nothing.
struct PartGuard(Option<PathBuf>);

impl PartGuard {
    /// Keeps the file: it has been renamed into place.
    fn disarm(mut self) {
        self.0 = None;
    }
}

impl Drop for PartGuard {
    fn drop(&mut self) {
        if let Some(part) = self.0.take() {
            let _ = std::fs::remove_file(part);
        }
    }
}
