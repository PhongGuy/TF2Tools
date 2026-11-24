//! The demos.tf client: finds a log's demo and reads one demo, every request through one gate.

use std::cell::Cell;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use tokio::io::AsyncWriteExt as _;

use crate::error::DemosError;
use crate::logstf::LOGS_SPACING;
use crate::store::StoreConfig;
use crate::store::http::{API_BODY_LIMIT, Http, NetFailure, RATE_LIMITS, Redirects, under_prefix};
use crate::store::pacer::{Pacer, Priority};

use super::demo::{parse_detail, parse_list};
use super::list::{next_cursor, query_pairs};
use super::{
    DEMO_LIMIT, DemoCursor, DemoDetail, DemoListQuery, DemoPage, DemoQuery, DemoSummary, pick_demo,
};

/// How far either side of a log's upload time a search looks, in seconds.
const SEARCH_WINDOW: i64 = 3600;

/// Where demos.tf's api, demo pages and demo files live.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct DemosEndpoints {
    api: String,
    site: String,
    files: Vec<String>,
}

impl Default for DemosEndpoints {
    fn default() -> Self {
        DemosEndpoints {
            api: "https://api.demos.tf".to_owned(),
            site: "https://demos.tf".to_owned(),
            // Demos older than about a month move from static to the freezer host.
            files: vec![
                "https://static.demos.tf".to_owned(),
                "https://freezer.demos.tf".to_owned(),
            ],
        }
    }
}

impl DemosEndpoints {
    /// All under one test server: the api at `base`, pages and both file bases beneath it.
    pub fn loopback(base: &str) -> Self {
        let api = base.trim_end_matches('/').to_owned();
        DemosEndpoints {
            site: format!("{api}/site"),
            files: vec![format!("{api}/files"), format!("{api}/freezer")],
            api,
        }
    }

    /// The api base address, without a trailing slash.
    pub fn api(&self) -> &str {
        &self.api
    }

    /// The website base address, without a trailing slash.
    pub fn site(&self) -> &str {
        &self.site
    }

    /// The bases demo files may download from, without trailing slashes.
    pub fn files(&self) -> &[String] {
        &self.files
    }

    /// The demos.tf page that shows demo `demo_id`.
    pub fn demo_page(&self, demo_id: u64) -> String {
        format!("{}/{demo_id}", self.site)
    }

    /// True when `url` lies under one of the file bases.
    fn serves_file(&self, url: &str) -> bool {
        self.files.iter().any(|base| under_prefix(url, base))
    }

    /// Redirects may only stay on the api and file bases.
    fn redirects(&self) -> Redirects {
        let bases = std::iter::once(&self.api).chain(&self.files).cloned();
        Redirects::Only(bases.collect::<Vec<_>>().into())
    }
}

/// Finds and reads demos on demos.tf; clones share one gate, so they never overlap their gaps.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct DemosClient {
    http: Http,
    endpoints: DemosEndpoints,
    pacer: Pacer,
    loopback_only: bool,
    limit: u64,
}

impl DemosClient {
    /// A client that asks `endpoints`, paced by `default_config`.
    pub fn new(endpoints: DemosEndpoints) -> Self {
        let config = Self::default_config();
        DemosClient {
            http: unpaced_http(&config, &endpoints),
            endpoints,
            pacer: Pacer::new(config.request_spacing()),
            loopback_only: config.loopback_only(),
            limit: DEMO_LIMIT,
        }
    }

    /// The store defaults with one request a second, as logs.tf is asked.
    pub fn default_config() -> StoreConfig {
        StoreConfig::default().with_request_spacing(LOGS_SPACING)
    }

    /// This client with `config`'s pacing, timeouts, User-Agent and offline switch, on a new
    /// gate that earlier clones do not share.
    pub fn with_config(mut self, config: StoreConfig) -> Self {
        self.http = unpaced_http(&config, &self.endpoints);
        self.pacer = Pacer::new(config.request_spacing());
        self.loopback_only = config.loopback_only();
        self
    }

    /// The addresses this client asks.
    pub fn endpoints(&self) -> &DemosEndpoints {
        &self.endpoints
    }

    /// True when this client may not leave the machine.
    pub fn is_loopback_only(&self) -> bool {
        self.loopback_only
    }

    /// The demo recorded for `query`, or `None` when demos.tf holds no single match.
    /// One search asks for the map and both players within an hour of the log's upload.
    pub async fn find_demo(&self, query: &DemoQuery) -> Result<Option<DemoSummary>, DemosError> {
        let players = query
            .players()
            .iter()
            .map(|p| p.id64().to_string())
            .collect::<Vec<_>>()
            .join(",");
        let after = query.date().saturating_sub(SEARCH_WINDOW).to_string();
        let before = query.date().saturating_add(SEARCH_WINDOW).to_string();
        let base = format!("{}/demos/", self.endpoints.api);
        let url = reqwest::Url::parse_with_params(
            &base,
            [
                ("map", query.map().to_owned()),
                ("players", players),
                ("after", after),
                ("before", before),
            ],
        )
        .map_err(|e| DemosError::Network {
            url: base.clone(),
            message: e.to_string(),
        })?;
        let body = self.get(url.as_str()).await?;
        let candidates = parse_list(&body)?;
        Ok(pick_demo(query, &candidates).cloned())
    }

    /// One page of demos for `query` from `cursor`. A players filter pages by number, so a
    /// `BeforeId` cursor with one fails with `Cursor` before any request.
    pub async fn list(
        &self,
        query: &DemoListQuery,
        cursor: DemoCursor,
    ) -> Result<DemoPage, DemosError> {
        let pairs = query_pairs(query, cursor)?;
        let base = format!("{}/demos/", self.endpoints.api);
        let url =
            reqwest::Url::parse_with_params(&base, pairs).map_err(|e| DemosError::Network {
                url: base.clone(),
                message: e.to_string(),
            })?;
        let body = self.get(url.as_str()).await?;
        let raw = parse_list(&body)?;
        // The cursor follows the raw page so a format filter that empties it keeps paging.
        let next = next_cursor(query, cursor, &raw);
        let demos = match query.format() {
            Some(f) => raw.into_iter().filter(|d| d.format() == f).collect(),
            None => raw,
        };
        Ok(DemoPage::new(demos, next))
    }

    /// Demo `id` with its players; a 404 means demos.tf no longer has it.
    pub async fn demo(&self, id: u64) -> Result<DemoDetail, DemosError> {
        let url = format!("{}/demos/{id}", self.endpoints.api);
        match self.get(&url).await {
            Ok(body) => parse_detail(&body),
            Err(NetFailure::Status { status: 404, .. }) => Err(DemosError::Gone),
            Err(failure) => Err(failure.into()),
        }
    }

    /// Streams `demo`'s file into a part file beside `dest`, reporting (bytes so far,
    /// Content-Length) after each chunk, then replaces `dest`. ForeignUrl when the url is not
    /// under a files base; TooLarge past the limit; 404 Gone; I/O failures Write. On any
    /// failure the part file is gone and an existing `dest` is untouched.
    pub async fn download_demo(
        &self,
        demo: &DemoSummary,
        dest: &Path,
        progress: &(dyn Fn(u64, Option<u64>) + Send + Sync),
    ) -> Result<(), DemosError> {
        let url = demo.url();
        if !self.endpoints.serves_file(url) {
            return Err(DemosError::ForeignUrl {
                url: url.to_owned(),
            });
        }
        // The part file sits beside `dest` so the final rename never crosses a filesystem,
        // and it is created before asking so an unwritable folder costs no request.
        let dir = match dest.parent() {
            Some(dir) if !dir.as_os_str().is_empty() => dir,
            _ => Path::new("."),
        };
        let name = dest.file_name().unwrap_or_default().to_string_lossy();
        let part = tempfile::Builder::new()
            .prefix(&format!(".{name}."))
            .suffix(".part")
            .tempfile_in(dir)
            .map_err(write_failure)?;

        // As in `get`: each 429 widens the gate, and an answer with none helps ease it back.
        self.pacer.slot(Priority::Interactive).await;
        // An atomic, not a Cell, so the future stays Send for the app's tokio runtime.
        let rate_limited = AtomicBool::new(false);
        let opened = self
            .http
            .open_retrying(url, || {
                rate_limited.store(true, Ordering::Relaxed);
                self.pacer.rate_limited();
            })
            .await;
        if !rate_limited.load(Ordering::Relaxed) && opened.is_ok() {
            self.pacer.answered();
        }
        let mut body = match opened {
            Ok(body) => body,
            Err(NetFailure::Status { status: 404, .. }) => return Err(DemosError::Gone),
            Err(failure) => return Err(failure.into()),
        };
        let too_large = || DemosError::TooLarge {
            url: url.to_owned(),
            limit: self.limit,
        };
        let total = body.total();
        if total.is_some_and(|total| total > self.limit) {
            return Err(too_large());
        }
        let mut out = tokio::fs::File::from_std(part.reopen().map_err(write_failure)?);
        let mut downloaded = 0u64;
        while let Some(chunk) = body.chunk().await? {
            downloaded += chunk.len() as u64;
            if downloaded > self.limit {
                return Err(too_large());
            }
            out.write_all(&chunk).await.map_err(write_failure)?;
            progress(downloaded, total);
        }
        out.flush().await.map_err(write_failure)?;
        out.sync_all().await.map_err(write_failure)?;
        drop(out);

        // The part file starts private to its owner; a saved demo is readable like any
        // download. Set before the rename, so a failure here never replaces `dest`.
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            part.as_file()
                .set_permissions(std::fs::Permissions::from_mode(0o644))
                .map_err(write_failure)?;
        }
        part.persist(dest).map_err(|e| write_failure(e.error))?;
        Ok(())
    }

    /// This client with a download cap of `bytes` instead of `DEMO_LIMIT`.
    #[cfg(test)]
    pub(crate) fn with_limit(mut self, bytes: u64) -> Self {
        self.limit = bytes;
        self
    }

    /// One API GET through the gate. Each 429 it met widens the gate and a clean answer
    /// helps ease it back, so demos.tf is asked less often while it pushes back.
    async fn get(&self, url: &str) -> Result<Vec<u8>, NetFailure> {
        self.pacer.slot(Priority::Interactive).await;
        let (result, rate_limits) = RATE_LIMITS
            .scope(Cell::new(0), async {
                let result = self.http.get_api(url, API_BODY_LIMIT).await;
                (result, RATE_LIMITS.with(Cell::get))
            })
            .await;
        for _ in 0..rate_limits {
            self.pacer.rate_limited();
        }
        if rate_limits == 0 && result.is_ok() {
            self.pacer.answered();
        }
        result
    }
}

/// An `Http` with no spacing of its own, so the client's gate is the only pacing.
fn unpaced_http(config: &StoreConfig, endpoints: &DemosEndpoints) -> Http {
    let config = config.clone().with_request_spacing(Duration::ZERO);
    crate::store::http_for(&config, &endpoints.redirects())
}

/// A filesystem failure while saving a demo.
fn write_failure(error: std::io::Error) -> DemosError {
    DemosError::Write {
        message: error.to_string(),
    }
}

/// A refused redirect left demos.tf's hosts; a 404 is mapped to Gone before it gets here.
impl From<NetFailure> for DemosError {
    fn from(failure: NetFailure) -> Self {
        match failure {
            NetFailure::RateLimited { url } => DemosError::RateLimited { url },
            NetFailure::Status { url, status } => DemosError::Http { url, status },
            NetFailure::Network { url, message } => DemosError::Network { url, message },
            NetFailure::TooLarge { url, limit } => DemosError::TooLarge { url, limit },
            NetFailure::Untrusted { url } => DemosError::ForeignUrl { url },
        }
    }
}

#[cfg(test)]
#[path = "client_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "download_tests.rs"]
mod download_tests;
