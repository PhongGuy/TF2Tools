//! Measures what a 7-day trim does to a seeded logs.tf cache: the file before, the `space`
//! estimate, the file after the pass and its compaction, and how long each step took.
//!
//! Usage: `logs_trim_sizes --data <empty or missing dir> [--logs 2000] [--incremental]`.
//! Refuses a directory that already holds `tf2tools.db`. Without `--incremental` the seeded
//! file is rewritten without auto_vacuum first, as a database from an earlier build is.

use std::error::Error;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::{Duration, Instant};

use tf2tools_core::{
    DataDir, Database, KeepFullLogs, LogsCache, LogsCancel, LogsClient, LogsEndpoints, LogsRange,
    SteamId, StoreConfig, TrackedPlayer,
};
use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, MockServer, Request, ResponseTemplate};

const USER_AGENT: &str = "tf2tools-research (+https://github.com)";
const USAGE: &str = "usage: logs_trim_sizes --data <dir> [--logs 2000] [--incremental]";
const PLAYER: u64 = 76_561_197_960_497_430;
const DAY: i64 = 86_400;
/// The span the listed logs and the seeded opens are spread over.
const SPAN: i64 = 60 * DAY;
/// The list endpoint's page size, as the client asks for it.
const PAGE: usize = 1000;

struct Args {
    data: PathBuf,
    logs: usize,
    incremental: bool,
}

/// What one run measured.
struct Measured {
    logs: usize,
    plain: usize,
    opened: usize,
    seed_vacuum: Option<Duration>,
    bytes_before: u64,
    frees_7d: u64,
    space_time: Duration,
    trimmed: usize,
    recompressed: usize,
    bytes_after: u64,
    trim_time: Duration,
    revacuum_time: Duration,
}

/// Errors when `data` already holds a database: the run must seed a fresh one.
fn refuse_warm_dir(data: &Path) -> Result<(), String> {
    let db = DataDir::at(data).db_path();
    if db.exists() {
        return Err(format!(
            "{} already exists; logs_trim_sizes needs an empty or missing data directory",
            db.display()
        ));
    }
    Ok(())
}

fn parse_args() -> Result<Args, Box<dyn Error>> {
    let mut data = None;
    let mut logs = 2000;
    let mut incremental = false;
    let mut args = std::env::args().skip(1);
    while let Some(flag) = args.next() {
        match flag.as_str() {
            "--data" => data = Some(PathBuf::from(args.next().ok_or("--data needs a value")?)),
            "--logs" => logs = args.next().ok_or("--logs needs a value")?.parse()?,
            "--incremental" => incremental = true,
            other => return Err(format!("unknown argument {other}").into()),
        }
    }
    Ok(Args {
        data: data.ok_or("--data is required")?,
        logs,
        incremental,
    })
}

fn fixture(name: &str) -> Result<String, String> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/logstf")
        .join(name);
    std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))
}

/// `count` logs, ids 1.. newest first, dated evenly over `SPAN` and all at least two hours old
/// so every one is settled.
fn listed(count: usize, now: i64) -> Vec<(u64, i64)> {
    let step = SPAN / i64::try_from(count.max(1)).unwrap_or(1);
    (1..=count as u64)
        .map(|id| (id, now - 2 * 3_600 - step * (id as i64 - 1)))
        .collect()
}

/// Serves the list pages and answers `/json/<id>` with the 6v6 fixture for odd ids and the
/// Highlander one for even ids.
async fn serve(logs: &[(u64, i64)]) -> Result<MockServer, String> {
    let server = MockServer::start().await;
    for (n, page) in logs.chunks(PAGE).enumerate() {
        let entries: Vec<_> = page
            .iter()
            .map(|&(id, date)| {
                serde_json::json!({"id": id, "title": format!("Log {id}"), "map": "cp_process_f12",
                                   "date": date, "views": 0, "players": 12})
            })
            .collect();
        let body = serde_json::json!({"success": true, "results": entries.len(),
            "total": logs.len(), "parameters": {}, "logs": entries});
        Mock::given(method("GET"))
            .and(path("/api/v1/log"))
            .and(query_param("player", PLAYER.to_string()))
            .and(query_param("offset", (n * PAGE).to_string()))
            .respond_with(ResponseTemplate::new(200).set_body_string(body.to_string()))
            .mount(&server)
            .await;
    }
    let (six, hl) = (
        fixture("log-4137103-6v6.json")?,
        fixture("log-4150405-hl.json")?,
    );
    Mock::given(method("GET"))
        .and(wiremock::matchers::path_regex(r"^/json/\d+$"))
        .respond_with(move |req: &Request| {
            let id: u64 = req
                .url
                .path()
                .trim_start_matches("/json/")
                .parse()
                .unwrap_or(0);
            ResponseTemplate::new(200).set_body_string(if id % 2 == 1 { &six } else { &hl })
        })
        .mount(&server)
        .await;
    Ok(server)
}

/// Syncs `count` logs into a fresh database in `dir` through the real client.
async fn seed(dir: &DataDir, count: usize, now: i64) -> Result<(), Box<dyn Error>> {
    let server = serve(&listed(count, now)).await?;
    let cache = LogsCache::new(Database::open(dir)?);
    let id = SteamId::from_id64(PLAYER).ok_or("bad player id")?;
    cache.add_player(&TrackedPlayer::new(id, "Seeded"))?;
    let config = StoreConfig::default()
        .with_request_spacing(Duration::ZERO)
        .with_user_agent(USER_AGENT);
    let client = LogsClient::new(cache, LogsEndpoints::loopback(&server.uri()))
        .with_config(config)
        .with_long_spacing(Duration::ZERO);
    let summary = client
        .sync_player(id, LogsRange::Months3, true, &LogsCancel::new(), &|_| {})
        .await?;
    if summary.downloaded() != count {
        return Err(format!("downloaded {} of {count} logs", summary.downloaded()).into());
    }
    Ok(())
}

/// Ages the seeded cache the way a real one ages: even ids were opened at times spread over
/// `SPAN`, every fifth body is plain TEXT as earlier builds stored it, and unless `incremental`
/// the file is rewritten without auto_vacuum. Returns (opened, plain, that VACUUM's time).
fn age(
    db_path: &Path,
    now: i64,
    incremental: bool,
) -> Result<(usize, usize, Option<Duration>), Box<dyn Error>> {
    let conn = rusqlite::Connection::open(db_path)?;
    let count: i64 = conn.query_row("SELECT COUNT(*) FROM logstf_logs", [], |r| r.get(0))?;
    let step = SPAN / count.max(1);
    let opened = conn.execute(
        "UPDATE logstf_logs SET opened_at = ?1 - ?2 * id WHERE id % 2 = 0",
        rusqlite::params![now, step],
    )?;
    let bodies: Vec<(i64, Vec<u8>)> = conn
        .prepare("SELECT id, body FROM logstf_logs WHERE id % 5 = 0")?
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<Result<_, _>>()?;
    for (id, bytes) in &bodies {
        let mut text = String::new();
        flate2::read::ZlibDecoder::new(bytes.as_slice()).read_to_string(&mut text)?;
        conn.execute(
            "UPDATE logstf_logs SET body = ?2 WHERE id = ?1",
            rusqlite::params![id, text],
        )?;
    }
    let vacuum = if incremental {
        None
    } else {
        let start = Instant::now();
        conn.execute_batch("PRAGMA auto_vacuum = NONE; VACUUM;")?;
        Some(start.elapsed())
    };
    conn.query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |_| Ok(()))?;
    Ok((opened, bodies.len(), vacuum))
}

/// Seeds, ages and trims one cache in `data`, timing `space` and `trim`, then times one more
/// VACUUM of the trimmed file so the rewrite's own cost shows apart from the pass.
async fn measure(data: &Path, logs: usize, incremental: bool) -> Result<Measured, Box<dyn Error>> {
    let dir = DataDir::at(data);
    let now = chrono::Utc::now().timestamp();
    seed(&dir, logs, now).await?;
    let (opened, plain, seed_vacuum) = age(&dir.db_path(), now, incremental)?;

    let cache = LogsCache::new(Database::open(&dir)?);
    let start = Instant::now();
    let space = cache.space(now)?;
    let space_time = start.elapsed();
    let start = Instant::now();
    let report = cache.trim(KeepFullLogs::Days7, now)?;
    let trim_time = start.elapsed();
    drop(cache);

    let conn = rusqlite::Connection::open(dir.db_path())?;
    let start = Instant::now();
    conn.execute_batch("VACUUM;")?;
    let revacuum_time = start.elapsed();
    Ok(Measured {
        logs,
        plain,
        opened,
        seed_vacuum,
        bytes_before: report.bytes_before(),
        frees_7d: space.frees(KeepFullLogs::Days7),
        space_time,
        trimmed: report.trimmed(),
        recompressed: report.recompressed(),
        bytes_after: report.bytes_after(),
        trim_time,
        revacuum_time,
    })
}

fn ms(d: Duration) -> String {
    format!("{:.1} ms", d.as_secs_f64() * 1000.0)
}

fn table(m: &Measured, incremental: bool) -> String {
    let path = if incremental {
        "incremental file"
    } else {
        "old file, auto_vacuum NONE"
    };
    let rows = [
        ("path", path.to_owned()),
        ("logs seeded", m.logs.to_string()),
        ("logs with an old open stamp", m.opened.to_string()),
        ("plain TEXT bodies", m.plain.to_string()),
        (
            "seed VACUUM (auto_vacuum NONE)",
            m.seed_vacuum.map_or_else(|| "skipped".to_owned(), ms),
        ),
        ("file bytes before", m.bytes_before.to_string()),
        ("space(now).frees(Days7)", m.frees_7d.to_string()),
        ("file bytes after trim(Days7)", m.bytes_after.to_string()),
        (
            "bytes freed",
            m.bytes_before.saturating_sub(m.bytes_after).to_string(),
        ),
        ("logs trimmed", m.trimmed.to_string()),
        ("logs recompressed", m.recompressed.to_string()),
        ("space() time", ms(m.space_time)),
        ("trim() time (with compaction)", ms(m.trim_time)),
        ("VACUUM of the trimmed file", ms(m.revacuum_time)),
    ];
    let mut out = String::from("| measure | value |\n|---|---|\n");
    for (k, v) in rows {
        out.push_str(&format!("| {k} | {v} |\n"));
    }
    out
}

#[tokio::main(flavor = "multi_thread")]
async fn main() -> ExitCode {
    let args = match parse_args() {
        Ok(args) => args,
        Err(e) => {
            eprintln!("{e}\n{USAGE}");
            return ExitCode::from(2);
        }
    };
    if let Err(e) = refuse_warm_dir(&args.data) {
        eprintln!("{e}");
        return ExitCode::from(2);
    }
    match measure(&args.data, args.logs, args.incremental).await {
        Ok(m) => {
            println!("core_version={}", env!("CARGO_PKG_VERSION"));
            print!("{}", table(&m, args.incremental));
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("{e}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_dir_with_a_database_is_refused_and_an_empty_one_is_accepted() {
        let tmp = tempfile::tempdir().unwrap();
        assert!(refuse_warm_dir(&tmp.path().join("fresh")).is_ok());
        std::fs::write(tmp.path().join("tf2tools.db"), b"").unwrap();
        assert!(refuse_warm_dir(tmp.path()).is_err());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_small_incremental_cache_is_trimmed_recompressed_and_shrinks() {
        let tmp = tempfile::tempdir().unwrap();
        let m = measure(&tmp.path().join("data"), 40, true).await.unwrap();
        assert_eq!((m.opened, m.plain), (20, 8));
        assert!(m.trimmed > 0 && m.trimmed < 20, "trimmed {}", m.trimmed);
        assert!(m.recompressed > 0, "recompressed {}", m.recompressed);
        assert!(m.frees_7d > 0);
        assert!(m.bytes_after < m.bytes_before);
    }
}
