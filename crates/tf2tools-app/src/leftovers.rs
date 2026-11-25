//! Clears what older runs left behind: TF2Tools 5 web view data, crash-left download parts,
//! stale update files and v5 thumbnails. Each sweep logs one activity row naming what it freed.

use std::time::{Duration, SystemTime};

use gpui_kit::App;
use tf2tools_core::{
    CleanupRoots, Freed, LogLevel, NewLogEntry, Tf2Install, sweep_game_leftovers, sweep_leftovers,
};

use crate::activity;
use crate::i18n::{format, tr};
use crate::services::{Services, run_blocking};

/// A download part this old belongs to a run that crashed, not to one still downloading.
pub const MAX_PART_AGE: Duration = Duration::from_secs(24 * 60 * 60);

/// The activity row for `freed`: `None` when nothing went, else one system row with the size
/// freed and the removed paths one per line.
pub fn freed_entry(freed: &Freed) -> Option<NewLogEntry> {
    if freed.paths().is_empty() {
        return None;
    }
    let message = tr!(
        "activity_log.row.freed",
        size = format::bytes(freed.bytes())
    );
    let details = freed
        .paths()
        .iter()
        .map(|path| path.display().to_string())
        .collect::<Vec<_>>()
        .join("\n");
    Some(NewLogEntry::new(LogLevel::Info, "system", "remove", message).with_details(details))
}

/// After the first frame: sweeps the data folder's roots on the background executor (detached)
/// and logs what it freed.
pub fn sweep(cx: &App) {
    run_blocking(cx, |services| {
        let roots = CleanupRoots::for_data_dir(&services.data);
        record(
            services,
            &sweep_leftovers(&roots, SystemTime::now(), MAX_PART_AGE),
        );
        Ok(())
    })
    .detach();
}

/// Inside a `run_blocking` job once TF2 is found: sweeps its `tf` folder and logs what it freed.
pub fn sweep_game(services: &Services, tf2: &Tf2Install) {
    let freed = sweep_game_leftovers(&tf2.tf_dir(), SystemTime::now(), MAX_PART_AGE);
    record(services, &freed);
}

fn record(services: &Services, freed: &Freed) {
    if let Some(entry) = freed_entry(freed) {
        activity::write(services, &entry);
    }
}
