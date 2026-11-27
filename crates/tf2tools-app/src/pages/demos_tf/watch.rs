//! Watch on a Demos.tf row: the demo comes from the cache or is downloaded into it, then
//! opens in the viewer.

use std::path::PathBuf;

use gpui_kit::{Context, Window};

use super::DemosTfPage;
use crate::pages::demo_watch::watch_demo;

impl DemosTfPage {
    /// `Some` while demo `id` downloads, holding its whole percent once the size is known.
    pub fn download_progress(&self, id: u64) -> Option<Option<u8>> {
        self.downloads.get(&id).copied()
    }

    // Opens demo `id` through the shared watch flow. A file that lands while the page is
    // hidden or already showing a demo only stays cached, so the next Watch opens it at once.
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
        watch_demo(
            self,
            demo,
            id,
            |p: &mut DemosTfPage| &mut p.downloads,
            |page: &mut DemosTfPage,
             path: PathBuf,
             window: &mut Window,
             cx: &mut Context<DemosTfPage>| {
                if !page.hidden && page.viewer().is_none() {
                    page.show_viewer(path, window, cx);
                }
            },
            window,
            cx,
        );
    }
}
