//! Saving a demos.tf demo to a file the player picks, shared by every page with a Download
//! demo button.

use std::collections::HashMap;

use gpui_kit::{Context, Window};
use tf2tools_core::{DemoCache, DemoSummary};

use crate::pages::logs_tf::text::{
    demo_down, demo_download_failed, demo_downloaded, demo_file_name, demo_percent, demo_reason,
};
use crate::picker::PickKind;
use crate::services::Services;
use crate::shell::toast::{ToastKind, toast};
use crate::tokio_bridge;

/// Asks where to save `demo`, then saves it through the demo cache on tokio, tracking
/// progress under `key` in the map `in_flight` returns. Cancel is quiet; the end is a toast.
pub(crate) fn save_demo<T: 'static>(
    demo: DemoSummary,
    key: u64,
    in_flight: fn(&mut T) -> &mut HashMap<u64, Option<u8>>,
    window: &mut Window,
    cx: &mut Context<T>,
) {
    let suggested = demo_file_name(demo.name(), demo.id());
    let picking = cx
        .global::<Services>()
        .picker
        .save_to(PickKind::Demo, &suggested, cx);
    cx.spawn_in(window, async move |page, cx| {
        let Some(path) = picking.await else {
            return;
        };
        let dest = path.clone();
        // A second click while the dialog was up may already have started this demo.
        let started = page.update(cx, |page, cx| {
            if in_flight(page).contains_key(&key) {
                return None;
            }
            in_flight(page).insert(key, None);
            cx.notify();
            if !tokio_bridge::is_running(cx) {
                tokio_bridge::init(cx);
            }
            let (tx, rx) = tokio::sync::watch::channel(None::<u8>);
            let services = cx.global::<Services>();
            let client = services.demos.clone();
            let cache = DemoCache::new(services.data.demo_cache_dir());
            // Only a changed whole percent crosses to the page, so the UI wakes at most 100 times.
            let job = tokio_bridge::spawn(cx, async move {
                let report = move |done, total| {
                    tx.send_if_modified(|shown| {
                        let percent = demo_percent(done, total);
                        let changed = *shown != percent;
                        *shown = percent;
                        changed
                    });
                };
                cache.save_to(&client, &demo, &dest, &report).await
            });
            Some((rx, job))
        });
        let Ok(Some((mut rx, job))) = started else {
            return;
        };
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
                Ok(Ok(())) => {
                    let name = path.file_name().unwrap_or_default().to_string_lossy();
                    let name = name.into_owned();
                    toast(window, cx, ToastKind::Success, demo_downloaded(), name);
                    return;
                }
                Ok(Err(error)) => {
                    tracing::warn!(%error, key, "saving a demo failed");
                    demo_reason(&error)
                }
                Err(error) => {
                    tracing::warn!(%error, key, "the demo save stopped");
                    demo_down()
                }
            };
            toast(window, cx, ToastKind::Error, demo_download_failed(), why);
        });
    })
    .detach();
}
