//! The logs.tf client: Steam profile lookups and log syncs, all through one paced `Http`.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use serde::Deserialize;

use crate::error::{LogsSyncError, ResolveError};
use crate::store::StoreConfig;
use crate::store::http::{Http, NetFailure, Redirects};

use super::cache::{ListedLog, STATE_GONE};
use super::steam_xml::xml_text;
use super::{
    LOG_BODY_LIMIT, LOGS_SPACING, LogsCache, LogsRange, PlayerInput, STALE_SECS, SteamId,
    TrackedPlayer, parse_log, window_start,
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
}

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
    /// The configured gap between requests, reported with the download progress.
    spacing: Duration,
    /// Held for a whole sync, so two syncs never interleave their writes.
    sync_lock: Arc<tokio::sync::Mutex<()>>,
}

impl LogsClient {
    /// A client over `cache` that asks `endpoints`, paced by `default_config`.
    pub fn new(cache: LogsCache, endpoints: LogsEndpoints) -> Self {
        let config = Self::default_config();
        LogsClient {
            cache,
            http: crate::store::http_for(&config, &endpoints.redirects()),
            endpoints,
            spacing: config.request_spacing(),
            sync_lock: Arc::default(),
        }
    }

    /// The store defaults with one request a second.
    pub fn default_config() -> StoreConfig {
        StoreConfig::default().with_request_spacing(LOGS_SPACING)
    }

    /// This client with `config`'s pacing, timeouts, User-Agent and offline switch.
    pub fn with_config(mut self, config: StoreConfig) -> Self {
        self.http = crate::store::http_for(&config, &self.endpoints.redirects());
        self.spacing = config.request_spacing();
        self
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
        let bytes = self.http.get_api(url, LOG_BODY_LIMIT).await?;
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

    /// Lists the player's logs in `range`, downloads every listed log without a body, 1 s
    /// apart, and prunes. Skips everything when last_synced is under 30 minutes old and no log
    /// in the window is still pending, unless `force`.
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
        if !force && fresh && !self.cache.has_pending(id, range)? {
            return Ok(LogsSyncSummary {
                skipped: true,
                ..LogsSyncSummary::default()
            });
        }

        if cancel.is_cancelled() {
            return Ok(LogsSyncSummary {
                cancelled: true,
                ..LogsSyncSummary::default()
            });
        }

        // The window ends at the player's newest log, which tops the list, so the first page
        // always reaches it however long ago they played.
        let mut logs = self.list(id).await?;
        let anchor = logs.iter().map(|log| log.date).max().unwrap_or(0);
        if let Some(start) = window_start(anchor, range) {
            logs.retain(|log| log.date >= start);
        }
        logs.sort_by(|a, b| b.date.cmp(&a.date).then(b.id.cmp(&a.id)));
        let wanted = self.cache.record_list(id, &logs, now)?;

        let mut summary = LogsSyncSummary {
            listed: logs.len(),
            ..LogsSyncSummary::default()
        };
        let total = wanted.len();
        let spacing = self.spacing;
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
            let url = format!("{}/json/{log_id}", self.endpoints.logs_tf());
            // A 404 means logs.tf dropped the log; an oversize, non-UTF-8 or non-log body is
            // unreadable, unless a provisional refetch already holds a good copy. Marked logs are
            // never asked again; any other failure stops the sync.
            let readable = match self.http.get_api(&url, LOG_BODY_LIMIT).await {
                Ok(bytes) => String::from_utf8(bytes)
                    .ok()
                    .and_then(|body| parse_log(&body).ok().map(|detail| (body, detail))),
                Err(NetFailure::TooLarge { .. }) => None,
                Err(NetFailure::Status { status: 404, .. }) => {
                    self.cache.mark_failed(log_id, STATE_GONE, now)?;
                    summary.gone += 1;
                    on_progress(progress(done + 1));
                    continue;
                }
                Err(failure) => return Err(failure.into()),
            };
            match readable {
                Some((body, detail)) => {
                    self.cache.ingest(log_id, &body, &detail, now)?;
                    summary.downloaded += 1;
                }
                None => {
                    if self.cache.mark_unreadable(log_id, now)? {
                        summary.unreadable += 1;
                    }
                }
            }
            on_progress(progress(done + 1));
        }

        self.cache.mark_synced(id, now)?;
        self.cache.prune()?;
        Ok(summary)
    }

    /// The player's logs.tf list as logs.tf sent it.
    async fn list(&self, id: SteamId) -> Result<Vec<ListedLog>, LogsSyncError> {
        let url = format!(
            "{}/api/v1/log?player={}&limit=1000&offset=0",
            self.endpoints.logs_tf(),
            id.id64()
        );
        let bytes = self.http.get_api(&url, LOG_BODY_LIMIT).await?;
        let list: WireList =
            serde_json::from_slice(&bytes).map_err(|e| LogsSyncError::Malformed {
                url: url.clone(),
                message: e.to_string(),
            })?;
        Ok(list
            .logs
            .into_iter()
            .map(|log| ListedLog {
                id: log.id,
                title: log.title.unwrap_or_default(),
                map: log.map.unwrap_or_default(),
                date: log.date,
            })
            .collect())
    }
}
