//! The online stores: GameBanana HUDs, sounds and crosshairs, the curated hud-db HUDs
//! and the mastercomfig addons, cached in v5's `store_*` tables.

mod addon_api;
mod addon_catalog;
mod addon_sections;
mod cards;
mod download;
mod gamebanana;
mod github;
pub(crate) mod http;
mod images;
mod items;
mod maps;
mod mastercomfig;
pub(crate) mod pacer;
#[cfg(test)]
mod pacing_live;
#[cfg(test)]
mod pacing_tests;
mod sync;
mod unpack;

use std::cell::Cell;
use std::sync::Arc;
use std::time::Duration;

use md5::{Digest, Md5};
use rusqlite::OptionalExtension;

use crate::db::Database;
use crate::error::{BrowseError, FilesError, QueryError, SyncError};
use crate::paths::DataDir;
use crate::weapons::Weapon;
pub use addon_catalog::{
    AddonCard, AddonCatalog, AddonCategory, AddonCredit, AddonCreditGroup, AddonCursor, AddonFile,
    AddonProfile, AddonQuery, AddonSort, AddonStep, INDEX_STEP_CALLS, SEARCH_STEP_CALLS,
    STEP_CARDS,
};
pub use addon_sections::AddonSection;
pub use cards::{StoreCard, StoreCursor, StoreQuery, StoreSort};
pub use download::{DownloadPhase, DownloadProgress};
use http::{API_BODY_LIMIT, Http, NetFailure, Redirects, Timeouts};
pub use items::{StoreFile, StoreItem, StoreItemId};
pub use maps::{CommunityMap, MapEntry, MapState, parse_serveme_maps};
pub use mastercomfig::MastercomfigRelease;

const MB: u64 = 1024 * 1024;
/// The page limit of a store whose limit is neither fixed nor derived (v5's cap).
const DEFAULT_PAGE_LIMIT: u32 = 500;

/// One of the five online stores.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum StoreKind {
    /// HUDs curated by hud-db on GitHub.
    CuratedHuds,
    /// HUDs uploaded to GameBanana.
    GameBananaHuds,
    /// Hitsounds and killsounds on GameBanana.
    Sounds,
    /// Crosshairs on GameBanana.
    Crosshairs,
    /// mastercomfig's addon VPKs on GitHub.
    Addons,
}

impl StoreKind {
    /// Every store, in declaration order.
    pub const ALL: [StoreKind; 5] = [
        StoreKind::CuratedHuds,
        StoreKind::GameBananaHuds,
        StoreKind::Sounds,
        StoreKind::Crosshairs,
        StoreKind::Addons,
    ];

    /// The prefix of this store's `store_cache_meta` keys, as v5 wrote them.
    pub(crate) fn key(self) -> &'static str {
        match self {
            StoreKind::CuratedHuds => "curated",
            StoreKind::GameBananaHuds => "huds_gb",
            StoreKind::Sounds => "sounds",
            StoreKind::Crosshairs => "crosshairs",
            StoreKind::Addons => "addons",
        }
    }

    /// The table this store's items are cached in.
    pub(crate) fn table(self) -> &'static str {
        match self {
            StoreKind::CuratedHuds => "store_huds",
            StoreKind::GameBananaHuds => "store_huds_gb",
            StoreKind::Sounds => "store_sounds",
            StoreKind::Crosshairs => "store_crosshairs",
            StoreKind::Addons => "store_addons",
        }
    }

    /// The position in `ALL`, which lists the variants in declaration order.
    fn index(self) -> usize {
        self as usize
    }
}

/// Where the store talks to: production by default, redirectable for tests.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct Endpoints {
    gamebanana: String,
    github_api: String,
    github: String,
    preview_prefixes: Vec<String>,
    image_prefixes: Vec<String>,
    hud_resources: String,
    wiki_images: String,
    serveme: String,
    map_downloads: String,
    mastercomfig_data: String,
}

impl Default for Endpoints {
    fn default() -> Self {
        Self {
            gamebanana: "https://gamebanana.com/apiv11".to_owned(),
            github_api: "https://api.github.com".to_owned(),
            github: "https://github.com".to_owned(),
            preview_prefixes: vec![
                "https://files.gamebanana.com/".to_owned(),
                "https://sounds.gamebanana.com/".to_owned(),
            ],
            image_prefixes: vec![
                "https://images.gamebanana.com/".to_owned(),
                "https://raw.githubusercontent.com/".to_owned(),
                "https://wiki.teamfortress.com/w/images/".to_owned(),
            ],
            hud_resources:
                "https://raw.githubusercontent.com/mastercomfig/hud-db/main/hud-resources"
                    .to_owned(),
            wiki_images: "https://wiki.teamfortress.com/w/images".to_owned(),
            serveme: "https://serveme.tf".to_owned(),
            map_downloads: "https://fastdl.serveme.tf/maps".to_owned(),
            mastercomfig_data: "https://raw.githubusercontent.com/mastercomfig/mastercomfig"
                .to_owned(),
        }
    }
}

impl Endpoints {
    /// Every base under `base` (no trailing slash): GameBanana `{base}/apiv11`, GitHub API
    /// `{base}/gh-api`, github.com `{base}/gh`, previews `[{base}/preview/]`, images
    /// `[{base}/img/]`, hud-db screenshots `{base}/img/hud-resources`, wiki images
    /// `{base}/img/wiki`, serveme.tf `{base}/serveme`, map files `{base}/fastdl/maps` and
    /// mastercomfig's repository files `{base}/gh-raw/mastercomfig`.
    pub fn loopback(base: &str) -> Self {
        Self {
            gamebanana: format!("{base}/apiv11"),
            github_api: format!("{base}/gh-api"),
            github: format!("{base}/gh"),
            preview_prefixes: vec![format!("{base}/preview/")],
            image_prefixes: vec![format!("{base}/img/")],
            hud_resources: format!("{base}/img/hud-resources"),
            wiki_images: format!("{base}/img/wiki"),
            serveme: format!("{base}/serveme"),
            map_downloads: format!("{base}/fastdl/maps"),
            mastercomfig_data: format!("{base}/gh-raw/mastercomfig"),
        }
    }

    /// The GameBanana API base, without a trailing slash.
    pub fn gamebanana(&self) -> &str {
        &self.gamebanana
    }

    /// The GitHub REST API base.
    pub fn github_api(&self) -> &str {
        &self.github_api
    }

    /// The github.com base that archives are downloaded from.
    pub fn github(&self) -> &str {
        &self.github
    }

    /// The address prefixes an audio preview may be fetched from.
    pub fn preview_prefixes(&self) -> &[String] {
        &self.preview_prefixes
    }

    /// The address prefixes a store image may be fetched from.
    pub fn image_prefixes(&self) -> &[String] {
        &self.image_prefixes
    }

    /// The base hud-db screenshots live under, without a trailing slash; a curated image is
    /// `{base}/{hud_id}/{resource}.webp`.
    pub fn hud_resources(&self) -> &str {
        &self.hud_resources
    }

    /// The wiki's image root, without a trailing slash; production is
    /// `https://wiki.teamfortress.com/w/images`, loopback `{base}/img/wiki`.
    pub fn wiki_images(&self) -> &str {
        &self.wiki_images
    }

    /// The 100 px thumbnail of the wiki item `name`'s icon: MediaWiki's hashed path
    /// `{root}/thumb/{h[0]}/{h[0..2]}/{file}/100px-{file}` for `file` = `Item_icon_<name>.png`
    /// with spaces as `_`, `h` the hex md5 of that file name, percent-encoded outside
    /// `A-Za-z0-9_.-`.
    pub fn weapon_icon_url(&self, name: &str) -> String {
        // MediaWiki shards uploads by the md5 of the raw file name, so the path is computable
        // without asking the wiki (its redirect page sits behind a bot check).
        let file = format!("Item_icon_{}.png", name.replace(' ', "_"));
        let hash: String = Md5::digest(file.as_bytes())
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        let encoded: String = file
            .bytes()
            .map(|b| match b {
                b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'_' | b'.' | b'-' => {
                    char::from(b).to_string()
                }
                _ => format!("%{b:02X}"),
            })
            .collect();
        format!(
            "{}/thumb/{}/{}/{encoded}/100px-{encoded}",
            self.wiki_images,
            &hash[..1],
            &hash[..2]
        )
    }

    /// The serveme.tf base whose `/maps` page lists the community maps.
    pub fn serveme(&self) -> &str {
        &self.serveme
    }

    /// The base a map downloads from, as `{base}/{name}.bsp`.
    pub fn map_downloads(&self) -> &str {
        &self.map_downloads
    }

    /// The base of mastercomfig's raw repository files; a release's file is
    /// `{base}/{tag}/{path}`.
    pub fn mastercomfig_data(&self) -> &str {
        &self.mastercomfig_data
    }

    /// These endpoints with another mastercomfig repository file base.
    pub fn with_mastercomfig_data(mut self, base: impl Into<String>) -> Self {
        self.mastercomfig_data = base.into();
        self
    }

    /// These endpoints with another serveme.tf base.
    pub fn with_serveme(mut self, base: impl Into<String>) -> Self {
        self.serveme = base.into();
        self
    }

    /// These endpoints with another map download base.
    pub fn with_map_downloads(mut self, base: impl Into<String>) -> Self {
        self.map_downloads = base.into();
        self
    }

    /// These endpoints with another GameBanana API base.
    pub fn with_gamebanana(mut self, base: impl Into<String>) -> Self {
        self.gamebanana = base.into();
        self
    }

    /// These endpoints with another GitHub API base.
    pub fn with_github_api(mut self, base: impl Into<String>) -> Self {
        self.github_api = base.into();
        self
    }

    /// These endpoints with another github.com base.
    pub fn with_github(mut self, base: impl Into<String>) -> Self {
        self.github = base.into();
        self
    }

    /// These endpoints with another preview allowlist.
    pub fn with_preview_prefixes(
        mut self,
        prefixes: impl IntoIterator<Item = impl Into<String>>,
    ) -> Self {
        self.preview_prefixes = prefixes.into_iter().map(Into::into).collect();
        self
    }

    /// These endpoints with another wiki image root.
    pub fn with_wiki_images(mut self, base: impl Into<String>) -> Self {
        self.wiki_images = base.into();
        self
    }

    /// These endpoints with another image allowlist.
    pub fn with_image_prefixes(
        mut self,
        prefixes: impl IntoIterator<Item = impl Into<String>>,
    ) -> Self {
        self.image_prefixes = prefixes.into_iter().map(Into::into).collect();
        self
    }
}

/// The store's limits and pacing. Defaults are v5's numbers.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct StoreConfig {
    page_limits: [Option<u32>; 5],
    request_spacing: Duration,
    retry_backoff: Duration,
    size_caps: [u64; 5],
    user_agent: String,
    connect_timeout: Duration,
    read_timeout: Duration,
    loopback_only: bool,
    map_size_cap: u64,
    gb_addon_size_cap: u64,
}

impl Default for StoreConfig {
    fn default() -> Self {
        Self {
            page_limits: [None; 5],
            request_spacing: Duration::from_millis(200),
            retry_backoff: Duration::from_secs(2),
            size_caps: [200 * MB, 200 * MB, 100 * MB, 50 * MB, 50 * MB],
            user_agent: http::USER_AGENT.to_owned(),
            connect_timeout: Duration::from_secs(10),
            read_timeout: Duration::from_secs(30),
            loopback_only: false,
            map_size_cap: 512 * MB,
            gb_addon_size_cap: 300 * MB,
        }
    }
}

impl StoreConfig {
    /// The last GameBanana page a sync of `kind` fetches, given the catalogue size the
    /// first page reported: 500, or for sounds every page plus 10%, unless fixed.
    pub fn page_limit(&self, kind: StoreKind, record_count: u32) -> u32 {
        match self.page_limits[kind.index()] {
            Some(pages) => pages,
            // A missing catalogue size reads as 0, so it falls back to the flat limit.
            None if kind == StoreKind::Sounds && record_count > 0 => {
                let pages = record_count.div_ceil(gamebanana::PAGE_SIZE);
                pages.saturating_mul(11).div_ceil(10)
            }
            None => DEFAULT_PAGE_LIMIT,
        }
    }

    /// The gap between API requests: the GameBanana pacer's base gap, measured start to start,
    /// and the wait before each other API request.
    pub fn request_spacing(&self) -> Duration {
        self.request_spacing
    }

    /// The first wait after an HTTP 429; each later retry doubles it.
    pub fn retry_backoff(&self) -> Duration {
        self.retry_backoff
    }

    /// The largest download accepted for `kind`, in bytes.
    pub fn size_cap(&self, kind: StoreKind) -> u64 {
        self.size_caps[kind.index()]
    }

    /// The largest map file accepted, in bytes; serveme's biggest is about 210 MB.
    pub fn map_size_cap(&self) -> u64 {
        self.map_size_cap
    }

    /// This config with another map size cap.
    pub fn with_map_size_cap(mut self, bytes: u64) -> Self {
        self.map_size_cap = bytes;
        self
    }

    /// The largest GameBanana add-on download accepted, in bytes; 300 MiB by default.
    pub fn gb_addon_size_cap(&self) -> u64 {
        self.gb_addon_size_cap
    }

    /// This config with another GameBanana add-on size cap.
    pub fn with_gb_addon_size_cap(mut self, bytes: u64) -> Self {
        self.gb_addon_size_cap = bytes;
        self
    }

    /// How long a connection may take to open.
    pub fn connect_timeout(&self) -> Duration {
        self.connect_timeout
    }

    /// How long one read of a response may stall; it resets after every read, so a large
    /// download that keeps arriving is never cut.
    pub fn read_timeout(&self) -> Duration {
        self.read_timeout
    }

    /// True when no request may leave the machine; off by default.
    pub fn loopback_only(&self) -> bool {
        self.loopback_only
    }

    /// When on, every request and redirect hop whose host is not loopback (127.0.0.0/8, ::1,
    /// "localhost") fails before it is sent, with a Network failure whose message starts
    /// "offline". A warm image cache still serves.
    pub fn with_loopback_only(mut self, on: bool) -> Self {
        self.loopback_only = on;
        self
    }

    /// This config with a fixed page limit for `kind`.
    pub fn with_page_limit(mut self, kind: StoreKind, pages: u32) -> Self {
        self.page_limits[kind.index()] = Some(pages);
        self
    }

    /// This config with another request spacing.
    pub fn with_request_spacing(mut self, d: Duration) -> Self {
        self.request_spacing = d;
        self
    }

    /// This config with another base retry backoff.
    pub fn with_retry_backoff(mut self, base: Duration) -> Self {
        self.retry_backoff = base;
        self
    }

    /// This config with another download cap for `kind`.
    pub fn with_size_cap(mut self, kind: StoreKind, bytes: u64) -> Self {
        self.size_caps[kind.index()] = bytes;
        self
    }

    /// This config with other connect and per-read timeouts.
    pub fn with_timeouts(mut self, connect: Duration, read: Duration) -> Self {
        self.connect_timeout = connect;
        self.read_timeout = read;
        self
    }

    /// The User-Agent sent on every request; defaults to the app's own.
    pub fn with_user_agent(mut self, agent: impl Into<String>) -> Self {
        self.user_agent = agent.into();
        self
    }
}

/// When a store last finished syncing and how many items it holds.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct StoreSyncStatus {
    kind: StoreKind,
    last_synced: Option<String>,
    item_count: u64,
}

impl StoreSyncStatus {
    /// The store.
    pub fn kind(&self) -> StoreKind {
        self.kind
    }

    /// The RFC 3339 time the last full sync finished, or None if it never has.
    pub fn last_synced(&self) -> Option<&str> {
        self.last_synced.as_deref()
    }

    /// The number of cached items.
    pub fn item_count(&self) -> u64 {
        self.item_count
    }
}

/// A running sync's progress, reported after every committed page.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub struct SyncProgress {
    fetched: u32,
    total: u32,
}

impl SyncProgress {
    /// Items fetched so far this session, including pages fetched before a resume.
    pub fn fetched(self) -> u32 {
        self.fetched
    }

    /// The catalogue size the store reported on the first page this run fetched.
    pub fn total(self) -> u32 {
        self.total
    }
}

/// How a sync ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub struct SyncOutcome {
    fresh: bool,
    fetched: u32,
    rows: u64,
    rate_limited: u32,
}

impl SyncOutcome {
    /// True when the sync was skipped because the cache was younger than `sync_cache_hours`.
    pub fn is_fresh(self) -> bool {
        self.fresh
    }

    /// Items fetched this session, including pages fetched before a resume.
    pub fn fetched(self) -> u32 {
        self.fetched
    }

    /// Rows in the store's table afterwards; 0 when the sync was skipped.
    pub fn rows(self) -> u64 {
        self.rows
    }

    /// HTTP 429 responses this `sync` call received, retried ones included; 0 for a skip.
    pub fn rate_limited(self) -> u32 {
        self.rate_limited
    }

    /// The outcome of a sync skipped because the cache was still fresh.
    pub(crate) fn skipped() -> SyncOutcome {
        SyncOutcome {
            fresh: true,
            fetched: 0,
            rows: 0,
            rate_limited: 0,
        }
    }
}

/// Syncs, reads and downloads from the stores.
/// Cheap to clone; clones share the database, the HTTP connection pool and the download lock.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct StoreClient {
    db: Database,
    dir: DataDir,
    endpoints: Endpoints,
    config: StoreConfig,
    http: Http,
    /// Follows redirects only under the preview allowlist.
    preview_http: Http,
    /// Follows redirects only under the image allowlist.
    image_http: Http,
    /// Spaces and orders every GameBanana API call; shared by clones and `with_pacer_of`.
    pacer: pacer::Pacer,
    /// The on-disk image cache, shared by clones.
    images: Arc<images::DiskCache>,
    /// The add-on browsing caches, shared by clones.
    addon_caches: Arc<addon_catalog::Caches>,
    download_lock: Arc<tokio::sync::Mutex<()>>,
}

impl StoreClient {
    /// The icon URL for `weapon` under this client's endpoints; None when it has no wiki name.
    pub fn weapon_icon_url(&self, weapon: &Weapon) -> Option<String> {
        weapon
            .icon_name()
            .map(|name| self.endpoints.weapon_icon_url(name))
    }

    /// A client over `db` and `dir` talking to `endpoints`, with `StoreConfig::default()`.
    pub fn new(db: Database, dir: DataDir, endpoints: Endpoints) -> Self {
        let config = StoreConfig::default();
        Self {
            http: http_for(&config, &Redirects::Any),
            preview_http: http_for(&config, &previews(&endpoints)),
            image_http: http_for(&config, &images(&endpoints)),
            pacer: pacer::Pacer::new(config.request_spacing),
            images: Arc::new(images::DiskCache::new(
                dir.image_cache_dir(),
                images::CACHE_HIGH,
                images::CACHE_LOW,
            )),
            addon_caches: Arc::default(),
            db,
            dir,
            endpoints,
            config,
            download_lock: Arc::new(tokio::sync::Mutex::new(())),
        }
    }

    /// This client with `config`; its HTTP pacing follows the new config, through a new
    /// GameBanana pacer that no other client shares.
    pub fn with_config(mut self, config: StoreConfig) -> Self {
        self.http = http_for(&config, &Redirects::Any);
        self.preview_http = http_for(&config, &previews(&self.endpoints));
        self.image_http = http_for(&config, &images(&self.endpoints));
        self.pacer = pacer::Pacer::new(config.request_spacing);
        self.config = config;
        self
    }

    /// This client with `other`'s GameBanana pacer, so both space their calls together.
    /// Call it after `with_config`, which makes a new pacer.
    pub fn with_pacer_of(mut self, other: &StoreClient) -> Self {
        self.pacer = other.pacer.clone();
        self
    }

    /// One GameBanana API call through the shared pacer: `read` sorts each body into a
    /// result or an empty reply, and an empty reply is tried again until the pacer gives up.
    pub(crate) async fn gamebanana_call<T>(
        &self,
        url: &str,
        priority: pacer::Priority,
        read: impl Fn(&[u8]) -> pacer::Reply<T>,
    ) -> Result<T, GbFailure> {
        // Shared references are Copy, so every attempt's future owns its own captures.
        let (http, read) = (&self.http, &read);
        let attempt = move || async move {
            let body = http
                .open_retrying(url)
                .await?
                .read_to_end(API_BODY_LIMIT)
                .await?;
            Ok::<_, NetFailure>(read(&body))
        };
        match self.pacer.call(priority, attempt).await {
            Ok(value) => Ok(value),
            Err(pacer::Paced::Throttled) => {
                tracing::warn!(url, "GameBanana kept answering empty");
                Err(GbFailure::Throttled {
                    url: url.to_owned(),
                })
            }
            Err(pacer::Paced::Failed(failure)) => Err(GbFailure::Net(failure)),
        }
    }

    /// The body; a blank body (after trim_ascii) is an empty reply.
    pub(crate) async fn gamebanana_get(
        &self,
        url: &str,
        priority: pacer::Priority,
    ) -> Result<Vec<u8>, GbFailure> {
        self.gamebanana_call(url, priority, |body| {
            if body.trim_ascii().is_empty() {
                pacer::Reply::Empty
            } else {
                pacer::Reply::Done(body.to_vec())
            }
        })
        .await
    }

    /// Live add-on browsing over this client's GameBanana pacer.
    pub fn addon_catalog(&self) -> addon_catalog::AddonCatalog {
        addon_catalog::AddonCatalog::new(self.clone())
    }

    /// True when this client's config refuses every request off loopback.
    pub fn is_loopback_only(&self) -> bool {
        self.config.loopback_only
    }

    /// Each store's `{key}_last_synced` and row count, in StoreKind::ALL order (v5
    /// get_store_sync_status). Blocking; reads through the reader connection.
    pub fn sync_status(&self) -> Result<Vec<StoreSyncStatus>, QueryError> {
        self.db
            .read(|conn| {
                StoreKind::ALL
                    .iter()
                    .map(|&kind| {
                        let last_synced = conn
                            .query_row(
                                "SELECT value FROM store_cache_meta WHERE key = ?1",
                                [format!("{}_last_synced", kind.key())],
                                |r| r.get(0),
                            )
                            .optional()?;
                        // The table name is one of the crate's own constants, never caller text.
                        let count: i64 = conn.query_row(
                            &format!("SELECT COUNT(*) FROM {}", kind.table()),
                            [],
                            |r| r.get(0),
                        )?;
                        Ok(StoreSyncStatus {
                            kind,
                            last_synced,
                            item_count: u64::try_from(count).unwrap_or(0),
                        })
                    })
                    .collect()
            })
            .map_err(|e| QueryError::Database {
                message: e.to_string(),
            })
    }

    /// Syncs one store when its cache is older than `sync_cache_hours`, `force` is set, or
    /// a checkpoint exists. `progress` fires after every committed page and must not block.
    /// Needs a tokio runtime; a dropped future keeps the last committed page's checkpoint.
    pub async fn sync(
        &self,
        kind: StoreKind,
        force: bool,
        progress: &(dyn Fn(SyncProgress) + Send + Sync),
    ) -> Result<SyncOutcome, SyncError> {
        // The 429 count is task-local, so concurrent syncs each see only their own.
        let (mut result, rate_limited) = http::RATE_LIMITS
            .scope(Cell::new(0), async {
                let result = match kind {
                    StoreKind::GameBananaHuds | StoreKind::Sounds | StoreKind::Crosshairs => {
                        sync::gamebanana(self, kind, force, progress).await
                    }
                    StoreKind::CuratedHuds => github::curated(self, force, progress).await,
                    StoreKind::Addons => github::addons(self, force, progress).await,
                };
                (result, http::RATE_LIMITS.with(Cell::get))
            })
            .await;
        if let Ok(outcome) = &mut result {
            outcome.rate_limited = rate_limited;
        }
        // Core writes no `logs` rows; the app decides what to show from these events.
        match &result {
            Ok(outcome) if !outcome.fresh => tracing::info!(
                ?kind,
                fetched = outcome.fetched,
                rows = outcome.rows,
                rate_limited = outcome.rate_limited,
                "store sync finished"
            ),
            Ok(_) => {}
            Err(error) => tracing::warn!(?kind, %error, "store sync failed"),
        }
        result
    }
}

pub(crate) fn http_for(config: &StoreConfig, redirects: &Redirects) -> Http {
    let timeouts = Timeouts {
        connect: config.connect_timeout,
        read: config.read_timeout,
    };
    Http::with(
        config.request_spacing,
        config.retry_backoff,
        &config.user_agent,
        timeouts,
        redirects,
        config.loopback_only,
    )
}

fn previews(endpoints: &Endpoints) -> Redirects {
    Redirects::Only(endpoints.preview_prefixes.as_slice().into())
}

fn images(endpoints: &Endpoints) -> Redirects {
    Redirects::Only(endpoints.image_prefixes.as_slice().into())
}

/// Why a paced GameBanana call failed.
#[derive(Debug)]
pub(crate) enum GbFailure {
    /// Every try answered empty.
    Throttled { url: String },
    /// A try failed on the network or with a status.
    Net(NetFailure),
}

/// Throttled stays Throttled; a network failure maps as `NetFailure` does.
impl From<GbFailure> for SyncError {
    fn from(failure: GbFailure) -> Self {
        match failure {
            GbFailure::Throttled { url } => SyncError::Throttled { url },
            GbFailure::Net(net) => net.into(),
        }
    }
}

/// Throttled stays Throttled; a network failure maps as `NetFailure` does.
impl From<GbFailure> for BrowseError {
    fn from(failure: GbFailure) -> Self {
        match failure {
            GbFailure::Throttled { url } => BrowseError::Throttled { url },
            GbFailure::Net(net) => net.into(),
        }
    }
}

/// Throttled stays Throttled; a network failure maps as `NetFailure` does.
impl From<GbFailure> for FilesError {
    fn from(failure: GbFailure) -> Self {
        match failure {
            GbFailure::Throttled { url } => FilesError::Throttled { url },
            GbFailure::Net(net) => net.into(),
        }
    }
}
