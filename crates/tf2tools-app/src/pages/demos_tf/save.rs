//! Download on a Demos.tf row: the demo is saved to a file the player picks, through the
//! shared save flow.

use gpui_kit::{Context, Window};

use super::DemosTfPage;
use crate::pages::demo_save::save_demo;

impl DemosTfPage {
    /// `Some` while demo `id` saves, holding its whole percent once the size is known.
    pub fn save_progress(&self, id: u64) -> Option<Option<u8>> {
        self.save_downloads.get(&id).copied()
    }

    // Saves demo `id` from the loaded rows, keyed by its id so the row's button shows the
    // progress; a click while it runs does nothing.
    pub(super) fn save(&mut self, id: u64, window: &mut Window, cx: &mut Context<Self>) {
        if self.save_downloads.contains_key(&id) {
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
        save_demo(
            demo,
            id,
            |p: &mut DemosTfPage| &mut p.save_downloads,
            window,
            cx,
        );
    }
}
