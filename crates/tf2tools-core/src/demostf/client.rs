//! The demos.tf client: finds a log's demo and reads one demo, every request through one gate.

use std::cell::Cell;
use std::time::Duration;

use crate::error::DemosError;
use crate::logstf::LOGS_SPACING;
use crate::store::StoreConfig;
use crate::store::http::{API_BODY_LIMIT, Http, NetFailure, RATE_LIMITS, Redirects};
use crate::store::pacer::{Pacer, Priority};

use super::demo::{parse_detail, parse_list};
use super::{DEMO_LIMIT, DemoDetail, DemoQuery, DemoSummary, pick_demo};

/// How far either side of a log's upload time a search looks, in seconds.
const SEARCH_WINDOW: i64 = 3600;

/// Where demos.tf's api, demo pages and demo files live.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct DemosEndpoints {
    api: String,
    site: String,
    files: String,
}

impl Default for DemosEndpoints {
    fn default() -> Self {
        DemosEndpoints {
            api: "https://api.demos.tf".to_owned(),
            site: "https://demos.tf".to_owned(),
            files: "https://static.demos.tf".to_owned(),
        }
    }
}

impl DemosEndpoints {
    /// All three under one test server: the api at `base`, pages and files beneath it.
    pub fn loopback(base: &str) -> Self {
        let api = base.trim_end_matches('/').to_owned();
        DemosEndpoints {
            site: format!("{api}/site"),
            files: format!("{api}/files"),
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

    /// The base every demo file downloads from, without a trailing slash.
    pub fn files(&self) -> &str {
        &self.files
    }

    /// The demos.tf page that shows demo `demo_id`.
    pub fn demo_page(&self, demo_id: u64) -> String {
        format!("{}/{demo_id}", self.site)
    }

    /// Redirects may only stay on the api and file bases.
    fn redirects(&self) -> Redirects {
        Redirects::Only([self.api.clone(), self.files.clone()].as_slice().into())
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
    #[expect(dead_code, reason = "the demo download reads it")]
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

    /// Demo `id` with its players; a 404 means demos.tf no longer has it.
    pub async fn demo(&self, id: u64) -> Result<DemoDetail, DemosError> {
        let url = format!("{}/demos/{id}", self.endpoints.api);
        match self.get(&url).await {
            Ok(body) => parse_detail(&body),
            Err(NetFailure::Status { status: 404, .. }) => Err(DemosError::Gone),
            Err(failure) => Err(failure.into()),
        }
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
