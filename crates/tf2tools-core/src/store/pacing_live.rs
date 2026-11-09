//! Live pacing probe for the A0 note: interactive Index paging while a sound sync runs.
//! Calls production GameBanana with the User-Agent "TF2Tools"; run by hand only.

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use super::gamebanana;
use super::pacer::{PacerStats, Priority};
use super::{Endpoints, StoreClient, StoreConfig, StoreKind, SyncProgress};
use crate::db::Database;
use crate::paths::DataDir;

const SYNC_PAGES: u32 = 120;
const INTERACTIVE_PAGES: u32 = 20;

fn live_client(tmp: &tempfile::TempDir, run: &str, base: Duration) -> StoreClient {
    let dir = DataDir::at(tmp.path().join(run));
    let db = Database::open(&dir).unwrap();
    let config = StoreConfig::default()
        .with_user_agent("TF2Tools")
        .with_request_spacing(base)
        .with_page_limit(StoreKind::Sounds, SYNC_PAGES);
    StoreClient::new(db, dir, Endpoints::default()).with_config(config)
}

type Pages = Arc<Mutex<Vec<Duration>>>;

fn spawn_sync(client: &StoreClient, t0: Instant) -> (tokio::task::JoinHandle<String>, Pages) {
    let pages = Pages::default();
    let (client, seen) = (client.clone(), Arc::clone(&pages));
    let handle = tokio::spawn(async move {
        let progress = move |_: SyncProgress| seen.lock().unwrap().push(t0.elapsed());
        match client.sync(StoreKind::Sounds, true, &progress).await {
            Ok(outcome) => format!("ok, {} fetched", outcome.fetched()),
            Err(error) => format!("failed: {error}"),
        }
    });
    (handle, pages)
}

fn report(
    run: &str,
    wall: Duration,
    pages: &[Duration],
    result: &str,
    stats: PacerStats,
    gap: Duration,
) {
    let steps: Vec<f64> = pages
        .windows(2)
        .map(|w| (w[1] - w[0]).as_secs_f64())
        .collect();
    let mean = if steps.is_empty() {
        0.0
    } else {
        steps.iter().sum::<f64>() / steps.len() as f64
    };
    let max = steps.iter().copied().fold(0.0, f64::max);
    println!(
        "{run}: wall {:.1} s, {} pages, page step mean {mean:.3} s max {max:.3} s, {result}; {stats:?}; gap now {} ms",
        wall.as_secs_f64(),
        pages.len(),
        gap.as_millis()
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "calls production GameBanana; run by hand for the pacing note"]
async fn interactive_paging_during_a_sound_sync() {
    let base = std::env::var("TF2TOOLS_PACING_BASE_MS")
        .ok()
        .and_then(|v| v.parse().ok())
        .map_or(Duration::from_millis(200), Duration::from_millis);
    let tmp = tempfile::tempdir().unwrap();

    let alone = live_client(&tmp, "alone", base);
    let t0 = Instant::now();
    let (handle, pages) = spawn_sync(&alone, t0);
    let result = handle.await.unwrap();
    report(
        "alone",
        t0.elapsed(),
        &pages.lock().unwrap(),
        &result,
        alone.pacer.stats(),
        alone.pacer.gap(),
    );

    let busy = live_client(&tmp, "busy", base);
    let t0 = Instant::now();
    let (handle, pages) = spawn_sync(&busy, t0);
    while pages.lock().unwrap().len() < 10 && t0.elapsed() < Duration::from_secs(60) {
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    println!(
        "| page | asked at s | wait + fetch ms | empty replies meanwhile | outcome |\n|---|---|---|---|---|"
    );
    let mut done = 0;
    for page in 1..=INTERACTIVE_PAGES {
        let url = gamebanana::index_url(busy.endpoints.gamebanana(), StoreKind::Crosshairs, page)
            .unwrap();
        let before = busy.pacer.stats().empty;
        let asked = t0.elapsed();
        let outcome = match busy.gamebanana_get(&url, Priority::Interactive).await {
            Ok(body) => format!("ok, {} bytes", body.len()),
            Err(failure) => format!("{failure:?}"),
        };
        let took = t0.elapsed() - asked;
        let empties = busy.pacer.stats().empty - before;
        println!(
            "| {page} | {:.2} | {} | {empties} | {outcome} |",
            asked.as_secs_f64(),
            took.as_millis()
        );
        done += 1;
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
    let result = handle.await.unwrap();
    report(
        "busy",
        t0.elapsed(),
        &pages.lock().unwrap(),
        &result,
        busy.pacer.stats(),
        busy.pacer.gap(),
    );
    assert_eq!(done, INTERACTIVE_PAGES, "every interactive page was asked");
}
