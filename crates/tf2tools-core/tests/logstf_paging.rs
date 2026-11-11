//! Paging the log list: where it stops, what it records, and when a wider range relists.

use std::sync::Mutex;

use tf2tools_core::{LogsCancel, LogsProgress, LogsRange, LogsSyncSummary, SteamId, TrackedPlayer};

use crate::common;
use crate::common::logstf::{
    ARIE, LogsEnv, days_ago, hourly, list_body, list_offsets, logs_env, mount_any_log, mount_list,
    mount_page, page_body,
};

fn arie() -> SteamId {
    SteamId::from_id64(ARIE).unwrap()
}

async fn tracked_env() -> LogsEnv {
    let env = logs_env().await;
    env.client
        .cache()
        .add_player(&TrackedPlayer::new(arie(), "Arie"))
        .unwrap();
    env
}

/// A forced sync that lists, then cancels as downloads begin; returns the Listing ticks.
async fn list_only(env: &LogsEnv, range: LogsRange) -> (LogsSyncSummary, Vec<(usize, usize)>) {
    let cancel = LogsCancel::new();
    let listing = Mutex::new(Vec::new());
    let summary = env
        .client
        .sync_player(arie(), range, true, &cancel, &|p| match p {
            LogsProgress::Listing { listed, total } => {
                listing.lock().unwrap().push((listed, total))
            }
            LogsProgress::Downloading { .. } => cancel.cancel(),
            _ => {}
        })
        .await
        .unwrap();
    (summary, listing.into_inner().unwrap())
}

/// Mounts `logs` as pages of 1,000 at offsets 0, 1000, ..., each claiming `total`.
async fn mount_pages(env: &LogsEnv, logs: &[(u64, i64)], total: usize) {
    for (n, page) in logs.chunks(1000).enumerate() {
        mount_page(&env.server, ARIE, n * 1000, page_body(page, total)).await;
    }
}

#[tokio::test]
async fn all_lists_every_page_and_records_each() {
    let env = tracked_env().await;
    mount_pages(&env, &hourly(900_000, days_ago(1), 2500), 2500).await;
    let (summary, listing) = list_only(&env, LogsRange::All).await;
    assert_eq!(list_offsets(&env.server).await, ["0", "1000", "2000"]);
    assert_eq!(listing, [(1000, 2500), (2000, 2500), (2500, 2500)]);
    assert_eq!(summary.listed(), 2500);
    assert_eq!(common::count(&env.dir, "logstf_logs"), 2500);
}

#[tokio::test]
async fn the_boundary_page_ends_the_listing_and_records_only_the_window() {
    // Hourly logs: 90 days is 2,160 hours, so page 2 holds the boundary and page 3 is never asked.
    let env = tracked_env().await;
    mount_pages(&env, &hourly(900_000, days_ago(1), 4000), 5000).await;
    let (summary, listing) = list_only(&env, LogsRange::Months3).await;
    assert_eq!(list_offsets(&env.server).await, ["0", "1000", "2000"]);
    assert_eq!(listing, [(1000, 5000), (2000, 5000), (2161, 2161)]);
    assert_eq!(summary.listed(), 2161);
    assert_eq!(common::count(&env.dir, "logstf_logs"), 2161);
}

#[tokio::test]
async fn a_short_page_ends_the_listing() {
    let env = tracked_env().await;
    mount_pages(&env, &hourly(900_000, days_ago(1), 1400), 5000).await;
    let (_, listing) = list_only(&env, LogsRange::All).await;
    assert_eq!(list_offsets(&env.server).await, ["0", "1000"]);
    assert_eq!(listing, [(1000, 5000), (1400, 1400)]);
}

#[tokio::test]
async fn a_sync_cancelled_mid_listing_keeps_the_pages_it_recorded() {
    let env = tracked_env().await;
    mount_pages(&env, &hourly(900_000, days_ago(1), 2500), 2500).await;
    let cancel = LogsCancel::new();
    let summary = env
        .client
        .sync_player(arie(), LogsRange::All, true, &cancel, &|p| {
            if let LogsProgress::Listing { .. } = p {
                cancel.cancel();
            }
        })
        .await
        .unwrap();
    assert!(summary.cancelled());
    assert_eq!(list_offsets(&env.server).await, ["0"]);
    assert_eq!(
        common::count(&env.dir, "logstf_logs"),
        1000,
        "page 0 stays recorded"
    );
    assert_eq!(
        common::setting(&env.dir, &format!("logstf_listed_range:{ARIE}")),
        None,
        "an unfinished listing is not a listed range"
    );
}

#[tokio::test]
async fn reaching_the_total_ends_the_listing() {
    let env = tracked_env().await;
    let logs = hourly(900_000, days_ago(1), 2000);
    mount_page(&env.server, ARIE, 0, page_body(&logs[..1000], 1000)).await;
    mount_page(&env.server, ARIE, 1000, page_body(&logs[1000..], 1000)).await;
    let (_, listing) = list_only(&env, LogsRange::All).await;
    assert_eq!(list_offsets(&env.server).await, ["0"]);
    assert_eq!(listing, [(1000, 1000)]);
}

#[tokio::test]
async fn a_wider_range_relists_a_fresh_player_and_a_narrower_one_does_not() {
    let env = tracked_env().await;
    mount_list(
        &env.server,
        ARIE,
        list_body(&[(2, days_ago(1)), (1, days_ago(200))]),
    )
    .await;
    mount_any_log(&env.server).await;
    let sync = |range| {
        let client = env.client.clone();
        async move {
            client
                .sync_player(arie(), range, false, &LogsCancel::new(), &|_| {})
                .await
                .unwrap()
        }
    };
    assert!(!sync(LogsRange::Months3).await.skipped());
    assert_eq!(
        common::setting(&env.dir, &format!("logstf_listed_range:{ARIE}")).as_deref(),
        Some("3m")
    );
    assert!(sync(LogsRange::Months3).await.skipped());
    let wider = sync(LogsRange::All).await;
    assert!(!wider.skipped());
    assert_eq!(
        wider.downloaded(),
        1,
        "the 200-day-old log joins the window"
    );
    assert!(sync(LogsRange::All).await.skipped());
    assert!(sync(LogsRange::Month1).await.skipped());
    assert_eq!(list_offsets(&env.server).await, ["0", "0"]);
}
