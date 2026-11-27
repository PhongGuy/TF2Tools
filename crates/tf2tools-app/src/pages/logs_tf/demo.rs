//! An open log's demo on demos.tf: one lookup per log id, kept while the page lives, and the
//! download and View demo its footer buttons start.

use std::path::PathBuf;

use gpui_kit::{Context, EventEmitter, Window};
use tf2tools_core::{DemoQuery, DemoSummary};

use super::LogsTfPage;
use crate::pages::demo_save::save_demo;
use crate::pages::demo_watch::watch_demo;
use crate::services::Services;
use crate::tokio_bridge;

/// How far log `id`'s demo lookup got.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DemoLookup {
    Looking,
    Found(DemoSummary),
    Missing,
}

/// Asks the shell to show the demo at this path in the Demos.tf viewer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ViewDemo(pub PathBuf);

impl EventEmitter<ViewDemo> for LogsTfPage {}

/// What the open log's footer shows of its found demo: which one, and `Some` while it
/// downloads or opens for View demo, with the whole percent once its size is known.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct DemoButton {
    pub(super) demo_id: u64,
    pub(super) downloading: Option<Option<u8>>,
    pub(super) viewing: Option<Option<u8>>,
}

impl LogsTfPage {
    /// Log `log_id`'s demo buttons, only once its lookup found a demo.
    pub(super) fn demo_button(&self, log_id: u64) -> Option<DemoButton> {
        match self.demos.get(&log_id)? {
            DemoLookup::Found(demo) => Some(DemoButton {
                demo_id: demo.id(),
                downloading: self.demo_progress(log_id),
                viewing: self.view_progress(log_id),
            }),
            DemoLookup::Looking | DemoLookup::Missing => None,
        }
    }

    /// `Some` while log `log_id`'s demo downloads, holding its whole percent once known.
    pub fn demo_progress(&self, log_id: u64) -> Option<Option<u8>> {
        self.demo_downloads.get(&log_id).copied()
    }

    /// `Some` while log `log_id`'s demo opens for View demo, holding its whole percent once known.
    pub fn view_progress(&self, log_id: u64) -> Option<Option<u8>> {
        self.demo_views.get(&log_id).copied()
    }

    /// Log `log_id`'s demo lookup, if one has started.
    pub fn demo_lookup(&self, log_id: u64) -> Option<&DemoLookup> {
        self.demos.get(&log_id)
    }

    // Asks demos.tf once per log: the map entry is both the result and the dedup, so a log
    // reopened while its lookup is out never asks again. The answer lands on its own id
    // whichever log is open by then. A failure only means no button, so it is Missing.
    pub(super) fn look_up_demo(
        &mut self,
        log_id: u64,
        query: Option<DemoQuery>,
        cx: &mut Context<Self>,
    ) {
        if self.demos.contains_key(&log_id) {
            return;
        }
        let Some(query) = query else {
            self.demos.insert(log_id, DemoLookup::Missing);
            return;
        };
        self.demos.insert(log_id, DemoLookup::Looking);
        if !tokio_bridge::is_running(cx) {
            tokio_bridge::init(cx);
        }
        let client = cx.global::<Services>().demos.clone();
        let finding = tokio_bridge::spawn(cx, async move { client.find_demo(&query).await });
        cx.spawn(async move |page, cx| {
            let lookup = match finding.await {
                Ok(Ok(Some(demo))) => DemoLookup::Found(demo),
                Ok(Ok(None)) => {
                    tracing::debug!(log_id, "demos.tf holds no demo for this log");
                    DemoLookup::Missing
                }
                Ok(Err(error)) => {
                    tracing::debug!(%error, log_id, "looking up a log's demo failed");
                    DemoLookup::Missing
                }
                Err(error) => {
                    tracing::debug!(%error, log_id, "the demo lookup task stopped");
                    DemoLookup::Missing
                }
            };
            let _ = page.update(cx, |page, cx| {
                page.demos.insert(log_id, lookup);
                cx.notify();
            });
        })
        .detach();
    }

    // Saves log `log_id`'s found demo through the shared save flow, keyed by the log id so
    // its footer button shows the progress.
    pub(super) fn download_demo(
        &mut self,
        log_id: u64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.demo_downloads.contains_key(&log_id) {
            return;
        }
        let Some(DemoLookup::Found(demo)) = self.demos.get(&log_id) else {
            return;
        };
        let demo = demo.clone();
        save_demo(
            demo,
            log_id,
            |p: &mut LogsTfPage| &mut p.demo_downloads,
            window,
            cx,
        );
    }

    // Opens log `log_id`'s found demo into the cache through the shared watch flow, then asks
    // the shell to show it, unless the player has left logs.tf meanwhile.
    pub(super) fn view_demo(&mut self, log_id: u64, window: &mut Window, cx: &mut Context<Self>) {
        if self.demo_views.contains_key(&log_id) {
            return;
        }
        let Some(DemoLookup::Found(demo)) = self.demos.get(&log_id) else {
            return;
        };
        let demo = demo.clone();
        watch_demo(
            self,
            demo,
            log_id,
            |p: &mut LogsTfPage| &mut p.demo_views,
            |page: &mut LogsTfPage, path: PathBuf, _: &mut Window, cx: &mut Context<LogsTfPage>| {
                if !page.hidden {
                    cx.emit(ViewDemo(path));
                }
            },
            window,
            cx,
        );
    }
}
