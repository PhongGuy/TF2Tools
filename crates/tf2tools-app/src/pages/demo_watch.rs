//! Opening a demos.tf demo from the demo cache, downloading it there first if needed, shared
//! by every page that opens a demo in the viewer.

use std::collections::HashMap;
use std::path::PathBuf;

use gpui_kit::{Context, Window};
use tf2tools_core::{DemoCache, DemoCacheError, DemoSummary};

use crate::pages::logs_tf::text::{
    demo_down, demo_download_failed, demo_percent, demo_reason, write_failed,
};
use crate::services::Services;
use crate::shell::toast::{ToastKind, toast};
use crate::tokio_bridge;

/// Opens `demo` from the demo cache, downloading it there first if needed, tracking progress
/// under `key` in the map `in_flight` returns. A key already in flight does nothing; the
/// opened path goes to `opened`; a failure says why in a toast.
pub(crate) fn watch_demo<T: 'static>(
    page: &mut T,
    demo: DemoSummary,
    key: u64,
    in_flight: fn(&mut T) -> &mut HashMap<u64, Option<u8>>,
    opened: fn(&mut T, PathBuf, &mut Window, &mut Context<T>),
    window: &mut Window,
    cx: &mut Context<T>,
) {
    if in_flight(page).contains_key(&key) {
        return;
    }
    in_flight(page).insert(key, None);
    cx.notify();
    if !tokio_bridge::is_running(cx) {
        tokio_bridge::init(cx);
    }
    let services = cx.global::<Services>();
    let (client, cache) = (
        services.demos.clone(),
        DemoCache::new(services.data.demo_cache_dir()),
    );
    // Only a changed whole percent crosses to the page, so the UI wakes at most 100 times.
    let (tx, mut rx) = tokio::sync::watch::channel(None::<u8>);
    let job = tokio_bridge::spawn(cx, async move {
        let report = move |done, total| {
            tx.send_if_modified(|shown| {
                let percent = demo_percent(done, total);
                let changed = *shown != percent;
                *shown = percent;
                changed
            });
        };
        cache.open_or_download(&client, &demo, &report).await
    });
    cx.spawn_in(window, async move |page, cx| {
        while rx.changed().await.is_ok() {
            let percent = *rx.borrow_and_update();
            let _ = page.update(cx, |page, cx| {
                in_flight(page).insert(key, percent);
                cx.notify();
            });
        }
        let joined = job.await;
        let _ = page.update_in(cx, |page, window, cx| {
            in_flight(page).remove(&key);
            cx.notify();
            let why = match joined {
                Ok(Ok(path)) => {
                    opened(page, path, window, cx);
                    return;
                }
                Ok(Err(DemoCacheError::Download(error))) => {
                    tracing::warn!(%error, key, "downloading a demos.tf demo failed");
                    demo_reason(&error)
                }
                Ok(Err(DemoCacheError::Folder { message })) => {
                    tracing::warn!(%message, key, "the demo cache folder failed");
                    write_failed()
                }
                // Callers disable opening a private demo; the wildcard shares its text.
                Ok(Err(error @ (DemoCacheError::Private | _))) => {
                    tracing::warn!(%error, key, "watching a demos.tf demo failed");
                    demo_down()
                }
                Err(error) => {
                    tracing::warn!(%error, key, "the demo download stopped");
                    demo_down()
                }
            };
            toast(window, cx, ToastKind::Error, demo_download_failed(), why);
        });
    })
    .detach();
}
