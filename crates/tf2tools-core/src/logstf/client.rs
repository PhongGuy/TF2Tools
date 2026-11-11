//! The logs.tf client: Steam profile lookups and log syncs, every request through one gate.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use serde::Deserialize;

use crate::error::{LogsCacheError, LogsFetchError, LogsSyncError, ResolveError};
use crate::store::StoreConfig;
use crate::store::http::{Http, NetFailure, Redirects};
use crate::store::pacer::{Pacer, Priority};

use super::cache::{ListedLog, STATE_GONE};
use super::steam_xml::xml_text;
use super::{
    LOG_BODY_LIMIT, LOGS_LONG_SPACING, LOGS_SPACING, LONG_PULL, LogDetail, LogsCache, LogsRange,
    PlayerInput, STALE_SECS, SteamId, TrackedPlayer, parse_log, window_start,
};

/// Asks a running sync to stop before its next request; clones share one flag.
#[derive(Debug, Clone, Default)]
pub struct LogsCancel(Arc<AtomicBool>);

impl LogsCancel {
    /// A flag nobody has raised yet.
    pub fn new() -> Self {
        Self::default()
    }

    /// Stops every sync holding this flag or a clone of it.
    pub fn cancel(&self) {
        self.0.store(true, Ordering::Relaxed);
    }

    /// Whether `cancel` was called on this flag or a clone.
    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Relaxed)
    }
}

/// How far a sync has got: first through the player's list, then through the downloads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum LogsProgress {
    /// The player's list is being read, page by page.
    Listing {
        /// Logs listed so far.
        listed: usize,
        /// The logs logs.tf reports for the player.
        total: usize,
    },
    /// Logs without a body are being downloaded.
    Downloading {
        /// Logs handled so far, downloaded or marked.
        done: usize,
        /// Logs this sync needs to download.
        total: usize,
        /// The gap kept between requests.
        spacing: Duration,
    },
}

/// What one `sync_player` call did.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[non_exhaustive]
pub struct LogsSyncSummary {
    skipped: bool,
    cancelled: bool,
    listed: usize,
    downloaded: usize,
    gone: usize,
    unreadable: usize,
}

impl LogsSyncSummary {
    /// The cache was fresh, so nothing was asked of logs.tf.
    pub fn skipped(&self) -> bool {
        self.skipped
    }

    /// The sync stopped early on a `LogsCancel`; what it saved stays saved.
    pub fn cancelled(&self) -> bool {
        self.cancelled
    }

    /// Logs in the window.
    pub fn listed(&self) -> usize {
        self.listed
    }

    /// Log bodies downloaded and stored.
    pub fn downloaded(&self) -> usize {
        self.downloaded
    }

    /// Listed logs that logs.tf no longer has.
    pub fn gone(&self) -> usize {
        self.gone
    }

    /// Logs whose body could not be read.
    pub fn unreadable(&self) -> usize {
        self.unreadable
    }
}

/// The part of logs.tf's `/api/v1/log` answer a sync reads.
#[derive(Deserialize)]
struct WireList {
    logs: Vec<WireListed>,
    #[serde(default)]
    total: Option<usize>,
}

/// Logs asked for per list request, the most logs.tf serves at once.
const LIST_PAGE: usize = 1000;

#[derive(Deserialize)]
struct WireListed {
    id: i64,
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    map: Option<String>,
    date: i64,
}

/// Where logs.tf and the Steam community site live; tests point both at one loopback server.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct LogsEndpoints {
    logs_tf: String,
    steam_community: String,
}

impl Default for LogsEndpoints {
    fn default() -> Self {
        LogsEndpoints {
            logs_tf: "https://logs.tf".to_owned(),
            steam_community: "https://steamcommunity.com".to_owned(),
        }
    }
}

impl LogsEndpoints {
    /// logs.tf at `base` and Steam under `{base}/steam`.
    pub fn loopback(base: &str) -> Self {
        let base = base.trim_end_matches('/');
        LogsEndpoints {
            logs_tf: base.to_owned(),
            steam_community: format!("{base}/steam"),
        }
    }

    /// The logs.tf base address, without a trailing slash.
    pub fn logs_tf(&self) -> &str {
        &self.logs_tf
    }

    /// The Steam community base address, without a trailing slash.
    pub fn steam_community(&self) -> &str {
        &self.steam_community
    }

    /// Redirects may only stay on these two bases, so a hop cannot leave them or drop to http.
    fn redirects(&self) -> Redirects {
        Redirects::Only(
            [self.logs_tf.clone(), self.steam_community.clone()]
                .as_slice()
                .into(),
        )
    }
}

/// Looks up Steam players and syncs their logs into a `LogsCache`.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct LogsClient {
    cache: LogsCache,
    http: Http,
    endpoints: LogsEndpoints,
    /// The gate every request waits on; clones share it, so they never overlap their gaps.
    pacer: Pacer,
    /// The configured gap between requests, reported with the download progress.
    spacing: Duration,
    /// The gap a sync downloading more than `LONG_PULL` logs widens the gate to.
    long_spacing: Duration,
    /// Held for a whole sync, so two syncs never interleave their writes.
    sync_lock: Arc<tokio::sync::Mutex<()>>,
}

impl LogsClient {
    /// A client over `cache` that asks `endpoints`, paced by `default_config`.
    pub fn new(cache: LogsCache, endpoints: LogsEndpoints) -> Self {
        let config = Self::default_config();
        LogsClient {
            cache,
            http: unpaced_http(&config, &endpoints),
            endpoints,
            pacer: Pacer::new(config.request_spacing()),
            spacing: config.request_spacing(),
            long_spacing: LOGS_LONG_SPACING,
            sync_lock: Arc::default(),
        }
    }

    /// The store defaults with one request a second.
    pub fn default_config() -> StoreConfig {
        StoreConfig::default().with_request_spacing(LOGS_SPACING)
    }

    /// This client with `config`'s pacing, timeouts, User-Agent and offline switch, on a new
    /// gate that earlier clones do not share.
    pub fn with_config(mut self, config: StoreConfig) -> Self {
        self.http = unpaced_http(&config, &self.endpoints);
        self.pacer = Pacer::new(config.request_spacing());
        self.spacing = config.request_spacing();
        self
    }

    /// This client with `spacing` as the long-pull gap in place of `LOGS_LONG_SPACING`.
    pub fn with_long_spacing(mut self, spacing: Duration) -> Self {
        self.long_spacing = spacing;
        self
    }

    /// One GET through the gate: it starts at least one gap after this client's last request.
    async fn get(&self, url: &str, priority: Priority) -> Result<Vec<u8>, NetFailure> {
        self.pacer.slot(priority).await;
        self.http.get_api(url, LOG_BODY_LIMIT).await
    }

    /// The cache this client syncs into.
    pub fn cache(&self) -> &LogsCache {
        &self.cache
    }

    /// The addresses this client asks.
    pub fn endpoints(&self) -> &LogsEndpoints {
        &self.endpoints
    }

    /// The player behind `input`, named by their Steam persona, without touching the cache.
    ///
    /// A known Steam ID still resolves when Steam cannot be reached or answers with something
    /// other than profile XML (a maintenance page, an oversized body), named by its id64, so a player can be
    /// added offline. A vanity name needs Steam and fails as Steam did.
    pub async fn resolve_player(&self, input: &PlayerInput) -> Result<TrackedPlayer, ResolveError> {
        let steam = self.endpoints.steam_community();
        let url = match input {
            PlayerInput::Id(id) => format!("{steam}/profiles/{id}/?xml=1"),
            PlayerInput::Vanity(name) => format!("{steam}/id/{name}/?xml=1"),
        };
        let result = self.lookup(&url).await;
        match (input, result) {
            (
                PlayerInput::Id(id),
                Err(
                    ResolveError::Network { .. }
                    | ResolveError::Http { .. }
                    | ResolveError::RateLimited { .. }
                    | ResolveError::TooLarge { .. }
                    | ResolveError::Malformed { .. },
                ),
            ) => Ok(TrackedPlayer::new(*id, id.to_string())),
            (_, result) => result,
        }
    }

    /// Reads one profile XML body: an `<error>` means no such profile, otherwise
    /// `<steamID64>` must be a valid id and `<steamID>` names it.
    async fn lookup(&self, url: &str) -> Result<TrackedPlayer, ResolveError> {
        let bytes = self.get(url, Priority::Interactive).await?;
        let body = String::from_utf8_lossy(&bytes);
        if xml_text(&body, "error").is_some() {
            return Err(ResolveError::NotFound);
        }
        let malformed = |message: &str| ResolveError::Malformed {
            url: url.to_owned(),
            message: message.to_owned(),
        };
        let id64 = xml_text(&body, "steamID64").ok_or_else(|| malformed("no steamID64"))?;
        let id = id64
            .parse::<u64>()
            .ok()
            .and_then(SteamId::from_id64)
            .ok_or_else(|| malformed("steamID64 is not a Steam ID"))?;
        let name = xml_text(&body, "steamID")
            .filter(|name| !name.trim().is_empty())
            .unwrap_or(id64);
        Ok(TrackedPlayer::new(id, name))
    }

    /// Lists the player's logs in `range` page by page, downloads every listed log without a
    /// body, one configured gap apart (`LOGS_LONG_SPACING` when more than `LONG_PULL`), and prunes. Skips everything when last_synced is under 30 minutes old,
    /// `range` is no wider than the last full listing and no log in the window is still
    /// pending, unless `force`.
    ///
    /// `cancel` is checked before every request; a cancelled sync returns `Ok` with what it
    /// saved, leaving last_synced unstamped and nothing pruned so the next sync resumes.
    /// One sync runs at a time per client (clones share the lock).
    pub async fn sync_player(
        &self,
        id: SteamId,
        range: LogsRange,
        force: bool,
        cancel: &LogsCancel,
        on_progress: &(dyn Fn(LogsProgress) + Send + Sync),
    ) -> Result<LogsSyncSummary, LogsSyncError> {
        let _held = self.sync_lock.lock().await;
        let now = chrono::Utc::now().timestamp();
        let last_synced = self
            .cache
            .sync_state(id)?
            .ok_or(LogsSyncError::NotTracked {
                steam_id: id.id64(),
            })?;
        // A sync that stopped partway leaves pending logs, so the next open finishes it.
        let fresh = last_synced.is_some_and(|at| now - at < STALE_SECS);
        if !force
            && fresh
            && range <= self.cache.listed_range(id)?
            && !self.cache.has_pending(id, range)?
        {
            return Ok(LogsSyncSummary {
                skipped: true,
                ..LogsSyncSummary::default()
            });
        }

        let Some((listed, wanted)) = self
            .list_window(id, range, now, cancel, on_progress)
            .await?
        else {
            return Ok(LogsSyncSummary {
                cancelled: true,
                ..LogsSyncSummary::default()
            });
        };
        // Written before the downloads, so a sync cancelled from here on still counts as listed.
        self.cache.set_listed_range(id, range)?;

        let mut summary = LogsSyncSummary {
            listed,
            ..LogsSyncSummary::default()
        };
        let total = wanted.len();
        // A long pull goes slower so it does not lean on logs.tf; the guard puts the configured
        // gap back however the sync ends, a dropped future included.
        let spacing = if total > LONG_PULL {
            self.spacing.max(self.long_spacing)
        } else {
            self.spacing
        };
        let _gap = GapGuard::widen(&self.pacer, spacing, self.spacing);
        let progress = |done| LogsProgress::Downloading {
            done,
            total,
            spacing,
        };
        on_progress(progress(0));
        for (done, log_id) in wanted.into_iter().enumerate() {
            // The list is already recorded, so a stop here leaves only bodiless logs to fetch.
            if cancel.is_cancelled() {
                summary.cancelled = true;
                return Ok(summary);
            }
            // Gone and unreadable logs are marked and never asked again; any other failure
            // stops the sync.
            match self
                .download::<LogsSyncError>(log_id, Priority::Background, now)
                .await?
            {
                Downloaded::Ready(_) => summary.downloaded += 1,
                Downloaded::Gone => summary.gone += 1,
                Downloaded::Unreadable { marked: true, .. } => summary.unreadable += 1,
                Downloaded::Unreadable { marked: false, .. } => {}
            }
            on_progress(progress(done + 1));
        }

        self.cache.mark_synced(id, now)?;
        self.cache.prune()?;
        Ok(summary)
    }

    /// Downloads one log the player opened before the sync reached it, at interactive
    /// priority and without waiting for a running sync, and stores it as the sync would.
    /// A gone, unreadable or oversized log is marked like the sync marks it; a failed request
    /// leaves the log pending.
    pub async fn fetch_log(&self, log_id: u64) -> Result<LogDetail, LogsFetchError> {
        // The cache keys logs by i64, so an id past it names no log logs.tf could have.
        let Ok(log_id) = i64::try_from(log_id) else {
            return Err(LogsFetchError::Gone);
        };
        let now = chrono::Utc::now().timestamp();
        match self
            .download::<LogsFetchError>(log_id, Priority::Interactive, now)
            .await?
        {
            Downloaded::Ready(detail) => Ok(*detail),
            Downloaded::Gone => Err(LogsFetchError::Gone),
            Downloaded::Unreadable {
                too_large: Some((url, limit)),
                ..
            } => Err(LogsFetchError::TooLarge { url, limit }),
            Downloaded::Unreadable {
                too_large: None, ..
            } => Err(LogsFetchError::Unreadable),
        }
    }

    /// Asks for one log body and records the outcome, shared by the sync and `fetch_log` so
    /// the two never mark a log differently. A 404 marks it gone; an oversize, non-UTF-8 or
    /// non-log body marks it unreadable unless a good copy is already stored.
    async fn download<E>(&self, log_id: i64, priority: Priority, now: i64) -> Result<Downloaded, E>
    where
        E: From<NetFailure> + From<LogsCacheError>,
    {
        let url = format!("{}/json/{log_id}", self.endpoints.logs_tf());
        let (readable, too_large) = match self.get(&url, priority).await {
            Ok(bytes) => (
                String::from_utf8(bytes)
                    .ok()
                    .and_then(|body| parse_log(&body).ok().map(|detail| (body, detail))),
                None,
            ),
            Err(NetFailure::TooLarge { url, limit }) => (None, Some((url, limit))),
            Err(NetFailure::Status { status: 404, .. }) => {
                self.cache.mark_failed(log_id, STATE_GONE, now)?;
                return Ok(Downloaded::Gone);
            }
            Err(failure) => return Err(failure.into()),
        };
        Ok(match readable {
            Some((body, detail)) => {
                self.cache.ingest(log_id, &body, &detail, now)?;
                Downloaded::Ready(Box::new(detail))
            }
            None => Downloaded::Unreadable {
                marked: self.cache.mark_unreadable(log_id, now)?,
                too_large,
            },
        })
    }

    /// Reads the player's list page by page, recording each page's logs inside `range`'s
    /// window as it arrives, and returns the in-window count with the ids to download, newest
    /// first. `None` when `cancel` stopped it between pages; recorded pages stay recorded.
    async fn list_window(
        &self,
        id: SteamId,
        range: LogsRange,
        now: i64,
        cancel: &LogsCancel,
        on_progress: &(dyn Fn(LogsProgress) + Send + Sync),
    ) -> Result<Option<(usize, Vec<i64>)>, LogsSyncError> {
        let mut offset = 0;
        let mut start = None;
        let mut listed = 0;
        let mut wanted = Vec::new();
        loop {
            if cancel.is_cancelled() {
                return Ok(None);
            }
            let (mut page, api_total) = self.list_page(id, offset).await?;
            let fetched = page.len();
            let oldest = page.iter().map(|log| log.date).min();
            // The window ends at the player's newest log, which tops the first page, so the
            // window is known however long ago they played.
            let start = *start.get_or_insert_with(|| {
                let anchor = page.iter().map(|log| log.date).max().unwrap_or(0);
                window_start(anchor, range)
            });
            if let Some(start) = start {
                page.retain(|log| log.date >= start);
            }
            let dates: HashMap<i64, i64> = page.iter().map(|log| (log.id, log.date)).collect();
            let asked = self.cache.record_list(id, &page, now)?;
            wanted.extend(asked.into_iter().map(|log_id| (dates[&log_id], log_id)));
            listed += page.len();
            offset += fetched;

            // A short page, the API's total or a page reaching past the window start ends it.
            let past_start =
                matches!((start, oldest), (Some(start), Some(oldest)) if oldest < start);
            let done = fetched < LIST_PAGE || api_total.is_some_and(|t| offset >= t) || past_start;
            let total = if done {
                listed
            } else {
                api_total.unwrap_or(listed)
            };
            on_progress(LogsProgress::Listing { listed, total });
            if done {
                break;
            }
        }
        // Pages can overlap when a log is uploaded mid-listing, so a log is downloaded once.
        wanted.sort_unstable_by(|a, b| b.cmp(a));
        wanted.dedup();
        Ok(Some((
            listed,
            wanted.into_iter().map(|(_, id)| id).collect(),
        )))
    }

    /// One page of the player's logs.tf list starting at `offset`, and the total logs.tf
    /// reports for the player when it sends one.
    async fn list_page(
        &self,
        id: SteamId,
        offset: usize,
    ) -> Result<(Vec<ListedLog>, Option<usize>), LogsSyncError> {
        let url = format!(
            "{}/api/v1/log?player={}&limit={LIST_PAGE}&offset={offset}",
            self.endpoints.logs_tf(),
            id.id64()
        );
        let bytes = self.get(&url, Priority::Background).await?;
        let list: WireList =
            serde_json::from_slice(&bytes).map_err(|e| LogsSyncError::Malformed {
                url: url.clone(),
                message: e.to_string(),
            })?;
        let logs = list
            .logs
            .into_iter()
            .map(|log| ListedLog {
                id: log.id,
                title: log.title.unwrap_or_default(),
                map: log.map.unwrap_or_default(),
                date: log.date,
            })
            .collect();
        Ok((logs, list.total))
    }
}

/// An `Http` with no spacing of its own, so the client's gate is the only pacing.
fn unpaced_http(config: &StoreConfig, endpoints: &LogsEndpoints) -> Http {
    let config = config.clone().with_request_spacing(Duration::ZERO);
    crate::store::http_for(&config, &endpoints.redirects())
}

/// Holds the gate at a sync's gap and restores the configured gap when dropped.
struct GapGuard<'a> {
    pacer: &'a Pacer,
    configured: Duration,
}

impl<'a> GapGuard<'a> {
    fn widen(pacer: &'a Pacer, gap: Duration, configured: Duration) -> Self {
        pacer.set_base(gap);
        GapGuard { pacer, configured }
    }
}

impl Drop for GapGuard<'_> {
    fn drop(&mut self) {
        self.pacer.set_base(self.configured);
    }
}

/// What became of one log body download; every outcome is already recorded in the cache.
enum Downloaded {
    Ready(Box<LogDetail>),
    Gone,
    /// `marked` is false when a good copy was already stored; `too_large` names the cap hit.
    Unreadable {
        marked: bool,
        too_large: Option<(String, u64)>,
    },
}

/// A fetch-on-open has no HTTP-status variant: only a 404 means something to the player, so
/// every other status and a refused redirect read as a network failure.
impl From<NetFailure> for LogsFetchError {
    fn from(failure: NetFailure) -> Self {
        match failure {
            NetFailure::RateLimited { url } => LogsFetchError::RateLimited { url },
            NetFailure::Status { url, status } => LogsFetchError::Network {
                url,
                message: format!("HTTP {status}"),
            },
            NetFailure::Network { url, message } => LogsFetchError::Network { url, message },
            NetFailure::TooLarge { url, limit } => LogsFetchError::TooLarge { url, limit },
            NetFailure::Untrusted { url } => LogsFetchError::Network {
                url,
                message: "redirected outside the allowlist".to_owned(),
            },
        }
    }
}
