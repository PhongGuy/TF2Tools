//! Resuming after the app closed mid-sync: no saved log is downloaded twice.

use tf2tools_core::{
    Database, LogsCache, LogsCancel, LogsClient, LogsEndpoints, LogsProgress, LogsRange, SteamId,
    TrackedPlayer,
};

use crate::common;
use crate::common::logstf::{
    ARIE, LogsEnv, days_ago, hourly, list_body, list_offsets, logs_env, mount_any_log, mount_list,
    mount_page, now, page_body, paths,
};

fn arie() -> SteamId {
    SteamId::from_id64(ARIE).unwrap()
}

/// Logs 6 down to 1, one a day back from yesterday.
fn six_logs() -> Vec<(u64, i64)> {
    (1..=6u64)
        .rev()
        .map(|id| (id, days_ago(7 - id as i64)))
        .collect()
}

async fn tracked_env() -> LogsEnv {
    let env = logs_env().await;
    env.client
        .cache()
        .add_player(&TrackedPlayer::new(arie(), "Arie"))
        .unwrap();
    mount_list(&env.server, ARIE, list_body(&six_logs())).await;
    mount_any_log(&env.server).await;
    env
}

/// Runs `id`'s sync and drops it, as closing the app does, once `n` logs are saved.
async fn sync_until_closed(client: &LogsClient, id: SteamId, range: LogsRange, n: usize) {
    let saved = tokio::sync::Notify::new();
    // Bound first: the future borrows both, and is polled after this statement ends.
    let cancel = LogsCancel::new();
    let on_progress = |p: LogsProgress| {
        if let LogsProgress::Downloading { done, .. } = p
            && done == n
        {
            saved.notify_one();
        }
    };
    let sync = client.sync_player(id, range, false, &cancel, &on_progress);
    tokio::select! {
        biased;
        () = saved.notified() => {}
        finished = sync => panic!("finished before the app closed: {finished:?}"),
    }
}

/// A new client over the same database and server, as the next start builds it. A second
/// `Database::open` on a live data dir is safe: no exclusive locking mode is set, and other
/// store tests already open one beside a running env.
fn restarted(env: &LogsEnv) -> LogsClient {
    LogsClient::new(
        LogsCache::new(Database::open(&env.dir).unwrap()),
        LogsEndpoints::loopback(&env.server.uri()),
    )
    .with_config(common::fast_config())
}

/// The ids whose body is stored, compressed or plain text.
fn saved(env: &LogsEnv) -> Vec<String> {
    common::sql(&env.dir)
        .prepare("SELECT id FROM logstf_logs WHERE body IS NOT NULL ORDER BY id DESC")
        .unwrap()
        .query_map([], |r| r.get::<_, i64>(0))
        .unwrap()
        .map(|id| format!("/json/{}", id.unwrap()))
        .collect()
}

/// Forgets the first run's requests and mocks, then serves the same list and logs again.
async fn fresh_server(env: &LogsEnv) {
    env.server.reset().await;
    mount_list(&env.server, ARIE, list_body(&six_logs())).await;
    mount_any_log(&env.server).await;
}

fn downloads(all: &[String]) -> Vec<String> {
    all.iter()
        .filter(|p| p.starts_with("/json/"))
        .cloned()
        .collect()
}

#[tokio::test]
async fn a_sync_closed_mid_download_resumes_without_downloading_a_saved_log() {
    let env = tracked_env().await;
    sync_until_closed(&env.client, arie(), LogsRange::Months3, 3).await;
    let kept = saved(&env);
    // The close lands while the 4th download may be in flight, so 3 or 4 are saved.
    assert!((3..=4).contains(&kept.len()), "{kept:?}");
    fresh_server(&env).await;

    let summary = restarted(&env)
        .sync_player(
            arie(),
            LogsRange::Months3,
            false,
            &LogsCancel::new(),
            &|_| {},
        )
        .await
        .unwrap();
    assert!(
        !summary.skipped(),
        "an unfinished sync never stamped last_synced"
    );
    let fetched = downloads(&paths(&env.server).await);
    assert!(
        fetched.iter().all(|p| !kept.contains(p)),
        "{fetched:?} vs {kept:?}"
    );
    assert_eq!(fetched.len() + kept.len(), 6);
    assert_eq!(saved(&env).len(), 6);
}

#[tokio::test]
async fn plain_text_and_compressed_bodies_both_count_as_saved() {
    let env = tracked_env().await;
    sync_until_closed(&env.client, arie(), LogsRange::Months3, 2).await;
    // An earlier build's plain-text body for a log this run had not reached yet.
    let text = common::sql(&env.dir)
        .execute(
            "UPDATE logstf_logs SET body = '{}', fetched_at = ?1 WHERE id = 1 AND body IS NULL",
            [now()],
        )
        .unwrap();
    assert_eq!(text, 1);
    let kept = saved(&env);
    fresh_server(&env).await;

    restarted(&env)
        .sync_player(
            arie(),
            LogsRange::Months3,
            false,
            &LogsCancel::new(),
            &|_| {},
        )
        .await
        .unwrap();
    let fetched = downloads(&paths(&env.server).await);
    assert!(!fetched.contains(&"/json/1".to_owned()), "{fetched:?}");
    assert!(
        fetched.iter().all(|p| !kept.contains(p)),
        "{fetched:?} vs {kept:?}"
    );
    assert_eq!(fetched.len() + kept.len(), 6);
    let kinds: (String, String) = common::sql(&env.dir)
        .query_row(
            "SELECT (SELECT typeof(body) FROM logstf_logs WHERE id = 1),
                    (SELECT typeof(body) FROM logstf_logs WHERE id = 6)",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!(kinds, ("text".to_owned(), "blob".to_owned()));
}

#[tokio::test]
async fn a_chip_switch_cancel_keeps_progress_for_the_next_open() {
    let env = tracked_env().await;
    let bea = SteamId::from_id64(ARIE + 1).unwrap();
    env.client
        .cache()
        .add_player(&TrackedPlayer::new(bea, "Bea"))
        .unwrap();
    mount_list(&env.server, ARIE + 1, list_body(&[(70, days_ago(1))])).await;

    // Switching chips cancels Arie's sync after two logs; Bea's then runs to the end.
    let cancel = LogsCancel::new();
    let arie_run = env
        .client
        .sync_player(arie(), LogsRange::Months3, false, &cancel, &|p| {
            if let LogsProgress::Downloading { done: 2, .. } = p {
                cancel.cancel();
            }
        })
        .await
        .unwrap();
    assert!(arie_run.cancelled());
    let kept = saved(&env);
    assert_eq!(kept, ["/json/6", "/json/5"]);
    let bea_run = env
        .client
        .sync_player(bea, LogsRange::Months3, false, &LogsCancel::new(), &|_| {})
        .await
        .unwrap();
    assert_eq!(bea_run.downloaded(), 1);

    fresh_server(&env).await;
    let resumed = env
        .client
        .sync_player(
            arie(),
            LogsRange::Months3,
            false,
            &LogsCancel::new(),
            &|_| {},
        )
        .await
        .unwrap();
    assert_eq!(resumed.downloaded(), 4);
    assert_eq!(
        downloads(&paths(&env.server).await),
        ["/json/4", "/json/3", "/json/2", "/json/1"]
    );
}

#[tokio::test]
async fn an_all_pull_past_1000_logs_relists_on_restart_but_downloads_only_the_rest() {
    let env = logs_env().await;
    env.client
        .cache()
        .add_player(&TrackedPlayer::new(arie(), "Arie"))
        .unwrap();
    let logs = hourly(900_000, days_ago(1), 1500);
    let (server, logs) = (&env.server, &logs);
    let mount = move || async move {
        mount_page(server, ARIE, 0, page_body(&logs[..1000], 1500)).await;
        mount_page(server, ARIE, 1000, page_body(&logs[1000..], 1500)).await;
        mount_any_log(server).await;
    };
    mount().await;
    sync_until_closed(&env.client, arie(), LogsRange::All, 5).await;
    let kept = saved(&env);
    assert!((5..=6).contains(&kept.len()), "{kept:?}");
    assert_eq!(
        common::count(&env.dir, "logstf_logs"),
        1500,
        "both pages recorded"
    );
    env.server.reset().await;
    mount().await;

    // The restart lists both pages again, then is cancelled as downloads begin.
    let cancel = LogsCancel::new();
    let total = std::sync::Mutex::new(None);
    restarted(&env)
        .sync_player(arie(), LogsRange::All, false, &cancel, &|p| {
            if let LogsProgress::Downloading { total: t, .. } = p {
                total.lock().unwrap().get_or_insert(t);
                cancel.cancel();
            }
        })
        .await
        .unwrap();
    assert_eq!(list_offsets(&env.server).await, ["0", "1000"]);
    assert!(downloads(&paths(&env.server).await).is_empty());
    assert_eq!(total.into_inner().unwrap(), Some(1500 - kept.len()));
}
