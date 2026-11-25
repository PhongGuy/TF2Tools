//! Watch on a Demos.tf row: the demo comes from the cache or is downloaded into it, then
//! opens in the viewer.

use gpui_kit::{Context, Window};
use tf2tools_core::{DemoCache, DemoCacheError};

use super::DemosTfPage;
use crate::pages::logs_tf::text::{
    demo_down, demo_download_failed, demo_percent, demo_reason, write_failed,
};
use crate::services::Services;
use crate::shell::toast::{ToastKind, toast};
use crate::tokio_bridge;

impl DemosTfPage {
    /// `Some` while demo `id` downloads, holding its whole percent once the size is known.
    pub fn download_progress(&self, id: u64) -> Option<Option<u8>> {
        self.downloads.get(&id).copied()
    }

    // Fetches demo `id` through the cache on tokio, crossing only changed whole percents to
    // the page. A file that lands while the page is hidden or already showing a demo only
    // stays cached, so the next Watch opens it at once; a failure says why in a toast.
    pub(super) fn watch(&mut self, id: u64, window: &mut Window, cx: &mut Context<Self>) {
        if self.downloads.contains_key(&id) {
            return;
        }
        let Some(demo) = self
            .feed
            .items()
            .iter()
            .find(|row| row.demo.id() == id)
            .map(|row| row.demo.clone())
        else {
            return;
        };
        self.downloads.insert(id, None);
        cx.notify();
        if !tokio_bridge::is_running(cx) {
            tokio_bridge::init(cx);
        }
        let services = cx.global::<Services>();
        let (client, cache) = (
            services.demos.clone(),
            DemoCache::new(services.data.demo_cache_dir()),
        );
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
                    page.downloads.insert(id, percent);
                    cx.notify();
                });
            }
            let joined = job.await;
            let _ = page.update_in(cx, |page, window, cx| {
                page.downloads.remove(&id);
                cx.notify();
                let why = match joined {
                    Ok(Ok(path)) => {
                        if !page.hidden && page.viewer().is_none() {
                            page.show_viewer(path, window, cx);
                        }
                        return;
                    }
                    Ok(Err(DemoCacheError::Download(error))) => {
                        tracing::warn!(%error, id, "downloading a demos.tf demo failed");
                        demo_reason(&error)
                    }
                    Ok(Err(DemoCacheError::Folder { message })) => {
                        tracing::warn!(%message, id, "the demo cache folder failed");
                        write_failed()
                    }
                    // The row disables Watch on a private demo; the wildcard shares its text.
                    Ok(Err(error @ (DemoCacheError::Private | _))) => {
                        tracing::warn!(%error, id, "watching a demos.tf demo failed");
                        demo_down()
                    }
                    Err(error) => {
                        tracing::warn!(%error, id, "the demo download stopped");
                        demo_down()
                    }
                };
                toast(window, cx, ToastKind::Error, demo_download_failed(), why);
            });
        })
        .detach();
    }
}
