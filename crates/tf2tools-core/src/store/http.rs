//! The one HTTP path every store request takes: the TF2Tools User-Agent, request
//! spacing, 429 backoff, timeouts, a redirect policy and size-capped bodies.

use std::cell::Cell;
use std::sync::Arc;
use std::time::Duration;

use crate::error::{DownloadError, FilesError, ImageError, PreviewError, SyncError};

/// Identifies the app to GameBanana and GitHub; never carries user data.
pub(crate) const USER_AGENT: &str = concat!(
    "TF2Tools/",
    env!("CARGO_PKG_VERSION"),
    " (+https://github.com/PhongGuy/TF2Tools)"
);

/// How many times a 429 is retried before the request fails as rate limited.
const RATE_LIMIT_RETRIES: u32 = 3;

tokio::task_local! {
    /// The 429s seen by the sync this task is running; unset outside `StoreClient::sync`.
    pub(crate) static RATE_LIMITS: Cell<u32>;
}

/// The largest API response accepted; index pages and release lists are far smaller.
pub(crate) const API_BODY_LIMIT: u64 = 16 * 1024 * 1024;

/// A failed request, mapped into each operation's error enum by `From`.
#[derive(Debug)]
pub(crate) enum NetFailure {
    RateLimited {
        url: String,
    },
    Status {
        url: String,
        status: u16,
    },
    Network {
        url: String,
        message: String,
    },
    TooLarge {
        url: String,
        limit: u64,
    },
    /// A redirect to `url`, the refused hop, left the client's allowlist.
    Untrusted {
        url: String,
    },
}

/// Each target names how a refused redirect maps, since only previews have their own variant.
macro_rules! from_net_failure {
    ($($target:ident: $url:ident => $untrusted:expr),* $(,)?) => {$(
        impl From<NetFailure> for $target {
            fn from(failure: NetFailure) -> Self {
                match failure {
                    NetFailure::RateLimited { url } => Self::RateLimited { url },
                    NetFailure::Status { url, status } => Self::Http { url, status },
                    NetFailure::Network { url, message } => Self::Network { url, message },
                    NetFailure::TooLarge { url, limit } => Self::TooLarge { url, limit },
                    NetFailure::Untrusted { url: $url } => $untrusted,
                }
            }
        }
    )*};
}

const OFF_ALLOWLIST: &str = "redirected outside the allowlist";

from_net_failure!(
    SyncError: url => Self::Network { url, message: OFF_ALLOWLIST.to_owned() },
    FilesError: url => Self::Network { url, message: OFF_ALLOWLIST.to_owned() },
    DownloadError: url => Self::Network { url, message: OFF_ALLOWLIST.to_owned() },
    PreviewError: url => Self::UntrustedUrl { url },
    ImageError: url => Self::UntrustedUrl { url },
);

/// How long a connection may take to open and how long one read may stall.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Timeouts {
    pub(crate) connect: Duration,
    pub(crate) read: Duration,
}

impl Default for Timeouts {
    fn default() -> Self {
        Self {
            connect: Duration::from_secs(10),
            read: Duration::from_secs(30),
        }
    }
}

/// Which redirects a client follows: any (reqwest's default, 10 hops), or at most 5 hops
/// that each land under one of the prefixes.
#[derive(Debug, Clone)]
pub(crate) enum Redirects {
    Any,
    Only(Arc<[String]>),
}

/// The most hops an allowlisted client follows.
const ALLOWLIST_HOPS: usize = 5;

/// The refused hop, carried through reqwest's redirect error so `failure` can name it.
#[derive(Debug)]
struct RefusedRedirect(String);

impl std::fmt::Display for RefusedRedirect {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "redirect to {} is outside the allowlist", self.0)
    }
}

impl std::error::Error for RefusedRedirect {}

/// The most hops a client with no allowlist follows, as reqwest's default policy does.
const ANY_HOPS: usize = 10;

/// What a loopback-only client says about any request or hop that would leave the machine.
const OFFLINE: &str = "offline: only loopback hosts are allowed";

/// A hop off loopback refused by a loopback-only client, kept apart from `RefusedRedirect`
/// so every operation reports it as offline rather than as untrusted.
#[derive(Debug)]
struct OfflineRedirect(String);

impl std::fmt::Display for OfflineRedirect {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "redirect to {} leaves loopback", self.0)
    }
}

impl std::error::Error for OfflineRedirect {}

/// True when `url` names a loopback host (127.0.0.0/8, ::1 or "localhost"); an unparsable
/// URL is not loopback.
fn is_loopback(url: &str) -> bool {
    // The parser has already normalised the host, so an IPv6 address only needs its brackets off.
    let Ok(url) = reqwest::Url::parse(url) else {
        return false;
    };
    url.host_str().is_some_and(|host| {
        host == "localhost"
            || host
                .trim_start_matches('[')
                .trim_end_matches(']')
                .parse::<std::net::IpAddr>()
                .is_ok_and(|ip| ip.is_loopback())
    })
}

/// Every hop is checked: a loopback-only client refuses one off loopback first, then an
/// allowlisted client refuses one outside its prefixes.
fn policy(redirects: &Redirects, loopback_only: bool) -> reqwest::redirect::Policy {
    let (hops, prefixes) = match redirects {
        Redirects::Any if !loopback_only => return reqwest::redirect::Policy::default(),
        Redirects::Any => (ANY_HOPS, None),
        Redirects::Only(prefixes) => (ALLOWLIST_HOPS, Some(Arc::clone(prefixes))),
    };
    reqwest::redirect::Policy::custom(move |attempt| {
        // `previous` includes the first URL, so its length is the hop about to be taken.
        if attempt.previous().len() > hops {
            return attempt.error("too many redirects");
        }
        let next = attempt.url().as_str().to_owned();
        if loopback_only && !is_loopback(&next) {
            return attempt.error(OfflineRedirect(next));
        }
        match &prefixes {
            Some(prefixes) if !prefixes.iter().any(|p| under_prefix(&next, p)) => {
                attempt.error(RefusedRedirect(next))
            }
            _ => attempt.follow(),
        }
    })
}

/// True when `url` starts with `prefix` at a path boundary, so a slash-less prefix such as
/// `https://files.gamebanana.com` does not admit `https://files.gamebanana.com.evil.example/`.
pub(crate) fn under_prefix(url: &str, prefix: &str) -> bool {
    url.strip_prefix(prefix)
        .is_some_and(|rest| prefix.ends_with('/') || rest.starts_with('/'))
}

/// A shared connection pool with the store's pacing rules. Clones share the pool.
#[derive(Debug, Clone)]
pub(crate) struct Http {
    client: reqwest::Client,
    spacing: Duration,
    backoff: Duration,
    user_agent: Arc<str>,
    /// Refuses every request and hop whose host is not loopback, before it is sent.
    loopback_only: bool,
}

/// A response whose status was a success, read chunk by chunk.
pub(crate) struct Body {
    url: String,
    response: reqwest::Response,
}

impl Http {
    /// A client that waits `spacing` before each API request, backs off from `backoff`
    /// and identifies itself as `user_agent`, with default timeouts and any redirect.
    #[cfg(test)]
    pub(crate) fn new(spacing: Duration, backoff: Duration, user_agent: &str) -> Self {
        Self::with(
            spacing,
            backoff,
            user_agent,
            Timeouts::default(),
            &Redirects::Any,
            false,
        )
    }

    /// Like `new`, with `timeouts`, the `redirects` policy and, when `loopback_only`, no
    /// request or hop off loopback. No total timeout is set, so a long download that keeps
    /// arriving is never cut.
    pub(crate) fn with(
        spacing: Duration,
        backoff: Duration,
        user_agent: &str,
        timeouts: Timeouts,
        redirects: &Redirects,
        loopback_only: bool,
    ) -> Self {
        // Building fails only when the TLS backend cannot start, which Client::new treats as fatal too.
        #[allow(clippy::expect_used)]
        let client = reqwest::Client::builder()
            .connect_timeout(timeouts.connect)
            .read_timeout(timeouts.read)
            .redirect(policy(redirects, loopback_only))
            .build()
            .expect("the HTTP client's TLS backend starts");
        Self {
            client,
            spacing,
            backoff,
            user_agent: user_agent.into(),
            loopback_only,
        }
    }

    /// Waits `spacing`, sends with the User-Agent, retries a 429 after backoff, 2x and 4x
    /// backoff, then fails RateLimited; any other non-success status is Status. Every 429
    /// is counted in RATE_LIMITS when a sync is running on this task.
    pub(crate) async fn open_api(&self, url: &str) -> Result<Body, NetFailure> {
        tokio::time::sleep(self.spacing).await;
        let mut retries = 0;
        loop {
            let result = self.open(url).await;
            if let Err(NetFailure::Status { status: 429, .. }) = result {
                let _ = RATE_LIMITS.try_with(|count| count.set(count.get() + 1));
            }
            match result {
                Err(NetFailure::Status { status: 429, .. }) if retries < RATE_LIMIT_RETRIES => {
                    tokio::time::sleep(self.backoff * 2u32.pow(retries)).await;
                    retries += 1;
                }
                Err(NetFailure::Status { status: 429, .. }) => {
                    return Err(NetFailure::RateLimited {
                        url: url.to_owned(),
                    });
                }
                other => return other,
            }
        }
    }

    /// A plain GET with the User-Agent: no spacing and no retry; non-success is Status.
    pub(crate) async fn open(&self, url: &str) -> Result<Body, NetFailure> {
        let response = self.send(url).await?;
        let status = response.status();
        if !status.is_success() {
            return Err(NetFailure::Status {
                url: url.to_owned(),
                status: status.as_u16(),
            });
        }
        Ok(Body {
            url: url.to_owned(),
            response,
        })
    }

    /// The whole body of an API response, capped at `limit` bytes.
    pub(crate) async fn get_api(&self, url: &str, limit: u64) -> Result<Vec<u8>, NetFailure> {
        self.open_api(url).await?.read_to_end(limit).await
    }

    async fn send(&self, url: &str) -> Result<reqwest::Response, NetFailure> {
        if self.loopback_only && !is_loopback(url) {
            return Err(offline(url));
        }
        self.client
            .get(url)
            .header(reqwest::header::USER_AGENT, &*self.user_agent)
            .send()
            .await
            .map_err(|e| failure(url, &e))
    }
}

impl Body {
    /// The size the server announced in Content-Length, if it did.
    pub(crate) fn total(&self) -> Option<u64> {
        self.response.content_length()
    }

    /// The next chunk of the body, or None once it has all arrived.
    pub(crate) async fn chunk(&mut self) -> Result<Option<Vec<u8>>, NetFailure> {
        let chunk = self
            .response
            .chunk()
            .await
            .map_err(|e| network(&self.url, &e))?;
        Ok(chunk.map(|bytes| bytes.to_vec()))
    }

    /// The whole body; TooLarge as soon as more than `limit` bytes have arrived.
    pub(crate) async fn read_to_end(mut self, limit: u64) -> Result<Vec<u8>, NetFailure> {
        let mut out = Vec::new();
        while let Some(chunk) = self.chunk().await? {
            out.extend_from_slice(&chunk);
            if out.len() as u64 > limit {
                return Err(NetFailure::TooLarge {
                    url: self.url,
                    limit,
                });
            }
        }
        Ok(out)
    }
}

/// A failed send: a refused redirect names its target, a hop off loopback reads as offline,
/// anything else is a network failure.
fn failure(url: &str, error: &reqwest::Error) -> NetFailure {
    let mut cause = std::error::Error::source(error);
    while let Some(e) = cause {
        if let Some(hop) = e.downcast_ref::<OfflineRedirect>() {
            return offline(&hop.0);
        }
        if let Some(refused) = e.downcast_ref::<RefusedRedirect>() {
            return NetFailure::Untrusted {
                url: refused.0.clone(),
            };
        }
        cause = e.source();
    }
    network(url, error)
}

fn offline(url: &str) -> NetFailure {
    NetFailure::Network {
        url: url.to_owned(),
        message: OFFLINE.to_owned(),
    }
}

fn network(url: &str, error: &reqwest::Error) -> NetFailure {
    NetFailure::Network {
        url: url.to_owned(),
        message: error.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};
    use wiremock::matchers::{header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn fast() -> Http {
        Http::new(Duration::ZERO, Duration::from_millis(1), USER_AGENT)
    }

    async fn serve(status: u16, body: &str) -> (MockServer, String) {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/x"))
            .respond_with(ResponseTemplate::new(status).set_body_string(body))
            .mount(&server)
            .await;
        let url = format!("{}/x", server.uri());
        (server, url)
    }

    async fn requests(server: &MockServer) -> usize {
        server.received_requests().await.unwrap().len()
    }

    #[tokio::test]
    async fn api_requests_carry_the_tf2tools_user_agent() {
        let server = MockServer::start().await;
        Mock::given(path("/x"))
            .and(header("user-agent", USER_AGENT))
            .respond_with(ResponseTemplate::new(200).set_body_string("ok"))
            .expect(1)
            .mount(&server)
            .await;
        let body = fast()
            .get_api(&format!("{}/x", server.uri()), 1024)
            .await
            .unwrap();
        assert_eq!(body, b"ok");
        assert!(
            USER_AGENT.starts_with(concat!("TF2Tools/", env!("CARGO_PKG_VERSION"))),
            "{USER_AGENT}"
        );
        assert!(USER_AGENT.ends_with(" (+https://github.com/PhongGuy/TF2Tools)"));
    }

    #[tokio::test]
    async fn a_rate_limit_backs_off_then_succeeds() {
        let server = MockServer::start().await;
        Mock::given(path("/x"))
            .respond_with(ResponseTemplate::new(429))
            .up_to_n_times(2)
            .mount(&server)
            .await;
        Mock::given(path("/x"))
            .respond_with(ResponseTemplate::new(200).set_body_string("ok"))
            .mount(&server)
            .await;
        let body = fast()
            .get_api(&format!("{}/x", server.uri()), 1024)
            .await
            .unwrap();
        assert_eq!(body, b"ok");
        assert_eq!(requests(&server).await, 3);
    }

    #[tokio::test]
    async fn three_retries_then_rate_limited() {
        let (server, url) = serve(429, "").await;
        let err = fast().get_api(&url, 1024).await.unwrap_err();
        assert!(
            matches!(&err, NetFailure::RateLimited { url: u } if *u == url),
            "{err:?}"
        );
        assert_eq!(requests(&server).await, 4);
    }

    #[tokio::test]
    async fn backoff_doubles_from_its_base() {
        let (_server, url) = serve(429, "").await;
        let start = Instant::now();
        let http = Http::new(Duration::ZERO, Duration::from_millis(40), USER_AGENT);
        assert!(http.get_api(&url, 1024).await.is_err());
        assert!(start.elapsed() >= Duration::from_millis(40 + 80 + 160));
    }

    #[tokio::test]
    async fn api_requests_wait_the_spacing_first() {
        let (_server, url) = serve(200, "ok").await;
        let http = Http::new(
            Duration::from_millis(100),
            Duration::from_millis(1),
            USER_AGENT,
        );
        let start = Instant::now();
        http.get_api(&url, 1024).await.unwrap();
        http.get_api(&url, 1024).await.unwrap();
        assert!(start.elapsed() >= Duration::from_millis(200));
    }

    #[tokio::test]
    async fn an_error_status_is_reported_with_its_url() {
        let (_server, url) = serve(404, "").await;
        let err = fast().get_api(&url, 1024).await.unwrap_err();
        assert!(
            matches!(&err, NetFailure::Status { url: u, status: 404 } if *u == url),
            "{err:?}"
        );
    }

    #[tokio::test]
    async fn a_body_over_the_limit_is_too_large() {
        let (_server, url) = serve(200, &"x".repeat(2048)).await;
        let err = fast().get_api(&url, 1024).await.unwrap_err();
        assert!(
            matches!(err, NetFailure::TooLarge { limit: 1024, .. }),
            "{err:?}"
        );
        assert_eq!(fast().get_api(&url, 2048).await.unwrap().len(), 2048);
    }

    #[test]
    fn only_loopback_hosts_count_as_loopback() {
        for url in [
            "http://127.0.0.1:9/x",
            "http://127.8.0.1/",
            "http://[::1]:80/",
            "http://LOCALHOST/",
        ] {
            assert!(is_loopback(url), "{url}");
        }
        for url in [
            "https://gamebanana.com/",
            "http://192.0.2.1/",
            "http://localhost.evil.example/",
            "not a url",
        ] {
            assert!(!is_loopback(url), "{url}");
        }
    }

    #[tokio::test]
    async fn a_refused_connection_is_a_network_failure() {
        let err = fast()
            .get_api("http://127.0.0.1:1/x", 1024)
            .await
            .unwrap_err();
        assert!(matches!(err, NetFailure::Network { .. }), "{err:?}");
    }
}
