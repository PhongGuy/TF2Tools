//! A logs.tf client against wiremock, and the recorded logs.tf bodies.

use std::path::Path;

use tempfile::TempDir;
use tf2tools_core::{DataDir, Database, LogsCache, LogsClient, LogsEndpoints};
use wiremock::MockServer;

pub struct LogsEnv {
    pub tmp: TempDir,
    pub dir: DataDir,
    pub server: MockServer,
    pub client: LogsClient,
}

/// A fresh database and a client pointed at a new wiremock server, without spacing.
pub async fn logs_env() -> LogsEnv {
    let tmp = tempfile::tempdir().expect("tempdir");
    let dir = DataDir::at(tmp.path().join("com.tf2tools.app"));
    let db = Database::open(&dir).expect("open database");
    let server = MockServer::start().await;
    let client = LogsClient::new(LogsCache::new(db), LogsEndpoints::loopback(&server.uri()))
        .with_config(super::fast_config());
    LogsEnv {
        tmp,
        dir,
        server,
        client,
    }
}

/// A recorded body from `tests/fixtures/logstf/`.
pub fn logstf_fixture(name: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/logstf")
        .join(name);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

pub const FOUND_XML: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\
    <profile><steamID64>76561197960287930</steamID64><steamID><![CDATA[Rabscuttle]]></steamID>\
    <onlineState>offline</onlineState></profile>";
pub const NOT_FOUND_XML: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\
    <response><error><![CDATA[The specified profile could not be found.]]></error></response>";
