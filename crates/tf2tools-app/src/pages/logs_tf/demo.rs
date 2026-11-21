//! An open log's demo on demos.tf: one lookup per log id, kept while the page lives.

use gpui_kit::Context;
use tf2tools_core::{DemoQuery, DemoSummary};

use super::LogsTfPage;
use crate::services::Services;
use crate::tokio_bridge;

/// How far log `id`'s demo lookup got.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DemoLookup {
    Looking,
    Found(DemoSummary),
    Missing,
}

impl LogsTfPage {
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
}
