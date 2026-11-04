//! Times a cold sync of the selected stores against production, all started at once, while a
//! second thread keeps reading the cache. Prints one line per store, per read op, and the total.
//!
//! Usage: `sync_timing --data <empty or missing dir> [--stores all|sounds,crosshairs,...]
//! [--read-every-ms 500]`. Refuses a directory that already holds `tf2tools.db`.

use std::error::Error;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};

use tf2tools_core::{DataDir, Database, Endpoints, StoreClient, StoreConfig, StoreKind};

const USER_AGENT: &str = "tf2tools-research (+https://github.com)";
const USAGE: &str =
    "usage: sync_timing --data <dir> [--stores all|sounds,crosshairs,...] [--read-every-ms 500]";

/// How one store's sync went. `rate_limited` is `None` when the sync errored.
struct StoreTiming {
    kind: StoreKind,
    wall: Duration,
    first_page: Option<Duration>,
    fetched: u32,
    rows: u64,
    rate_limited: Option<u32>,
    error: Option<String>,
}

/// Latency of one read op sampled during the syncs.
struct ReadStats {
    n: usize,
    median: Duration,
    max: Duration,
}

/// The store's name on the command line and in the output, as v5 keyed its cache meta.
fn name(kind: StoreKind) -> &'static str {
    match kind {
        StoreKind::CuratedHuds => "curated",
        StoreKind::GameBananaHuds => "huds_gb",
        StoreKind::Sounds => "sounds",
        StoreKind::Crosshairs => "crosshairs",
        StoreKind::Addons => "addons",
        _ => "unknown",
    }
}

/// The median of `samples`; with an even count, the mean of the two middle values.
/// Panics on an empty slice.
fn median(samples: &[Duration]) -> Duration {
    assert!(!samples.is_empty(), "median of no samples");
    let mut sorted = samples.to_vec();
    sorted.sort();
    let mid = sorted.len() / 2;
    if sorted.len() % 2 == 1 {
        sorted[mid]
    } else {
        (sorted[mid - 1] + sorted[mid]) / 2
    }
}

/// Syncs every store in `kinds` at once (`force = false`), each on its own task, and returns
/// their timings in input order. One store's failure does not stop the others.
async fn time_all(client: &StoreClient, kinds: &[StoreKind]) -> Vec<StoreTiming> {
    let handles: Vec<_> = kinds
        .iter()
        .map(|&kind| {
            let client = client.clone();
            tokio::spawn(async move {
                // The callback fires after each committed page: the first one dates the first
                // page, the last one says how far an erroring sync got.
                let first = OnceLock::new();
                let fetched = AtomicU32::new(0);
                let start = Instant::now();
                let result = client
                    .sync(kind, false, &|p| {
                        first.get_or_init(Instant::now);
                        fetched.store(p.fetched(), Ordering::Relaxed);
                    })
                    .await;
                let wall = start.elapsed();
                let first_page = first.get().map(|t| t.duration_since(start));
                match result {
                    Ok(outcome) => StoreTiming {
                        kind,
                        wall,
                        first_page,
                        fetched: outcome.fetched(),
                        rows: outcome.rows(),
                        rate_limited: Some(outcome.rate_limited()),
                        error: None,
                    },
                    Err(e) => StoreTiming {
                        kind,
                        wall,
                        first_page,
                        fetched: fetched.into_inner(),
                        rows: 0,
                        rate_limited: None,
                        error: Some(e.to_string()),
                    },
                }
            })
        })
        .collect();

    let mut timings = Vec::with_capacity(handles.len());
    for (handle, &kind) in handles.into_iter().zip(kinds) {
        timings.push(handle.await.unwrap_or_else(|e| StoreTiming {
            kind,
            wall: Duration::ZERO,
            first_page: None,
            fetched: 0,
            rows: 0,
            rate_limited: None,
            error: Some(format!("task failed: {e}")),
        }));
    }
    timings
}

/// Times `store_items(Sounds)` and `sync_status` every `every` until `stop` is set, always taking
/// at least one sample of each. A failed read is reported on stderr and still timed.
fn sample_reads(
    client: StoreClient,
    stop: Arc<AtomicBool>,
    every: Duration,
) -> (ReadStats, ReadStats) {
    let (mut items, mut status) = (Vec::new(), Vec::new());
    loop {
        let start = Instant::now();
        if let Err(e) = client.store_items(StoreKind::Sounds) {
            eprintln!("store_items failed: {e}");
        }
        items.push(start.elapsed());

        let start = Instant::now();
        if let Err(e) = client.sync_status() {
            eprintln!("sync_status failed: {e}");
        }
        status.push(start.elapsed());

        if stop.load(Ordering::SeqCst) {
            break;
        }
        std::thread::sleep(every);
    }
    (stats(&items), stats(&status))
}

fn stats(samples: &[Duration]) -> ReadStats {
    ReadStats {
        n: samples.len(),
        median: median(samples),
        max: samples.iter().copied().max().unwrap_or_default(),
    }
}

/// Errors when `data` already holds a database: the run must measure a cold sync.
fn refuse_warm_dir(data: &Path) -> Result<(), String> {
    let db = DataDir::at(data).db_path();
    if db.exists() {
        return Err(format!(
            "{} already exists; sync_timing needs an empty or missing data directory",
            db.display()
        ));
    }
    Ok(())
}

struct Args {
    data: PathBuf,
    stores: Vec<StoreKind>,
    read_every: Duration,
}

fn parse_stores(list: &str) -> Result<Vec<StoreKind>, String> {
    if list == "all" {
        return Ok(StoreKind::ALL.to_vec());
    }
    list.split(',')
        .map(|s| {
            StoreKind::ALL
                .into_iter()
                .find(|k| name(*k) == s)
                .ok_or_else(|| format!("unknown store {s}"))
        })
        .collect()
}

fn parse_args() -> Result<Args, Box<dyn Error>> {
    let mut data = None;
    let mut stores = StoreKind::ALL.to_vec();
    let mut read_every = Duration::from_millis(500);
    let mut args = std::env::args().skip(1);
    while let Some(flag) = args.next() {
        let mut value = || args.next().ok_or_else(|| format!("{flag} needs a value"));
        match flag.as_str() {
            "--data" => data = Some(PathBuf::from(value()?)),
            "--stores" => stores = parse_stores(&value()?)?,
            "--read-every-ms" => read_every = Duration::from_millis(value()?.parse()?),
            other => return Err(format!("unknown argument {other}").into()),
        }
    }
    let data = data.ok_or("--data is required")?;
    Ok(Args {
        data,
        stores,
        read_every,
    })
}

fn secs(d: Duration) -> String {
    format!("{:.3}", d.as_secs_f64())
}

fn ms(d: Duration) -> f64 {
    d.as_secs_f64() * 1000.0
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
    match run(args).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("{e}");
            ExitCode::FAILURE
        }
    }
}

/// Opens a fresh database in `args.data`, syncs the selected stores while sampling reads, and
/// prints the report.
async fn run(args: Args) -> Result<(), Box<dyn Error>> {
    // The operator captures the date, load and competing builds around the run.
    let names: Vec<_> = args.stores.iter().map(|k| name(*k)).collect();
    println!(
        "context core_version={} data={} stores={} read_every_ms={}",
        env!("CARGO_PKG_VERSION"),
        args.data.display(),
        names.join(","),
        args.read_every.as_millis()
    );

    let dir = DataDir::at(&args.data);
    let db = Database::open(&dir)?;
    let config = StoreConfig::default().with_user_agent(USER_AGENT);
    let client = StoreClient::new(db, dir, Endpoints::default()).with_config(config);

    let stop = Arc::new(AtomicBool::new(false));
    let sampler = {
        let (client, stop) = (client.clone(), stop.clone());
        std::thread::spawn(move || sample_reads(client, stop, args.read_every))
    };
    let start = Instant::now();
    let timings = time_all(&client, &args.stores).await;
    let total = start.elapsed();
    stop.store(true, Ordering::SeqCst);
    let (items, status) = sampler.join().map_err(|_| "read sampler panicked")?;

    for t in &timings {
        println!(
            "store={} wall_s={} first_page_s={} fetched={} rows={} rate_limited={} status={}",
            name(t.kind),
            secs(t.wall),
            t.first_page.map_or_else(|| "none".to_string(), secs),
            t.fetched,
            t.rows,
            t.rate_limited
                .map_or_else(|| "unknown".to_string(), |n| n.to_string()),
            t.error
                .as_ref()
                .map_or_else(|| "ok".to_string(), |e| format!("error: {e}")),
        );
    }
    for (op, s) in [("store_items_sounds", &items), ("sync_status", &status)] {
        println!(
            "reads op={op} n={} median_ms={:.3} max_ms={:.3}",
            s.n,
            ms(s.median),
            ms(s.max)
        );
    }
    println!("total_wall_s={}", secs(total));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::time::Duration;
    use tf2tools_core::{DataDir, Database, Endpoints, StoreClient, StoreConfig, StoreKind};
    use wiremock::matchers::{method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn fixture(name: &str) -> Vec<u8> {
        std::fs::read(format!(
            "{}/tests/fixtures/store/{name}",
            env!("CARGO_MANIFEST_DIR")
        ))
        .unwrap()
    }

    async fn sound_client(tmp: &tempfile::TempDir) -> (MockServer, StoreClient) {
        let server = MockServer::start().await;
        for (page, name) in [(1, "gb-sounds-page1.json"), (2, "gb-sounds-last.json")] {
            Mock::given(method("GET"))
                .and(path("/apiv11/Sound/Index"))
                .and(query_param("_nPage", page.to_string()))
                .respond_with(ResponseTemplate::new(200).set_body_bytes(fixture(name)))
                .mount(&server)
                .await;
        }
        let dir = DataDir::at(tmp.path().join("data"));
        let db = Database::open(&dir).unwrap();
        let endpoints = Endpoints::default().with_gamebanana(format!("{}/apiv11", server.uri()));
        let config = StoreConfig::default()
            .with_request_spacing(Duration::ZERO)
            .with_retry_backoff(Duration::from_millis(1));
        (
            server,
            StoreClient::new(db, dir, endpoints).with_config(config),
        )
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn time_all_reports_wall_first_page_and_counts() {
        let tmp = tempfile::tempdir().unwrap();
        let (_server, client) = sound_client(&tmp).await;
        let timings = time_all(&client, &[StoreKind::Sounds]).await;
        assert_eq!(timings.len(), 1);
        let t = &timings[0];
        assert_eq!(t.error, None);
        assert_eq!(t.fetched, 5);
        assert_eq!(t.rows, 5);
        assert_eq!(t.rate_limited, Some(0));
        let first = t.first_page.expect("a committed page");
        assert!(first <= t.wall);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn sample_reads_takes_samples_until_stopped() {
        let tmp = tempfile::tempdir().unwrap();
        let (_server, client) = sound_client(&tmp).await;
        let stop = Arc::new(AtomicBool::new(false));
        let sampler = {
            let (client, stop) = (client.clone(), stop.clone());
            std::thread::spawn(move || sample_reads(client, stop, Duration::from_millis(5)))
        };
        time_all(&client, &[StoreKind::Sounds]).await;
        std::thread::sleep(Duration::from_millis(30));
        stop.store(true, Ordering::SeqCst);
        let (items, status) = sampler.join().unwrap();
        assert!(items.n >= 1 && status.n >= 1);
        assert!(items.median <= items.max);
    }

    #[test]
    fn a_dir_with_a_database_is_refused_and_an_empty_one_is_accepted() {
        let tmp = tempfile::tempdir().unwrap();
        assert!(refuse_warm_dir(&tmp.path().join("fresh")).is_ok());
        std::fs::write(tmp.path().join("tf2tools.db"), b"").unwrap();
        assert!(refuse_warm_dir(tmp.path()).is_err());
    }
}
