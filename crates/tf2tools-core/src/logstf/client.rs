//! The logs.tf client: Steam profile lookups now, log syncs later, all through one paced `Http`.

use crate::error::ResolveError;
use crate::store::StoreConfig;
use crate::store::http::{Http, Redirects};

use super::steam_xml::xml_text;
use super::{LOG_BODY_LIMIT, LOGS_SPACING, LogsCache, PlayerInput, SteamId, TrackedPlayer};

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
}

impl LogsClient {
    /// A client over `cache` that asks `endpoints`, paced by `default_config`.
    pub fn new(cache: LogsCache, endpoints: LogsEndpoints) -> Self {
        LogsClient {
            cache,
            http: crate::store::http_for(&Self::default_config(), &endpoints.redirects()),
            endpoints,
        }
    }

    /// The store defaults with one request a second.
    pub fn default_config() -> StoreConfig {
        StoreConfig::default().with_request_spacing(LOGS_SPACING)
    }

    /// This client with `config`'s pacing, timeouts, User-Agent and offline switch.
    pub fn with_config(mut self, config: StoreConfig) -> Self {
        self.http = crate::store::http_for(&config, &self.endpoints.redirects());
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
    /// other than profile XML (a maintenance page), named by its id64, so a player can be
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
}
