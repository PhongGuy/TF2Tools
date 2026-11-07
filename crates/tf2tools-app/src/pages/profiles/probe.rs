//! The "is TF2 running" answer the Profiles page and the header ask, as a global so
//! tests can pin it or point the default scan at a fake proc tree.

use std::path::PathBuf;
use std::sync::Arc;

use gpui_kit::{App, Global};

/// Answers "is TF2 running" for the Profiles page and the header; core's process check unless replaced.
pub struct Tf2Probe(Arc<dyn Fn() -> bool + Send + Sync>);

impl Global for Tf2Probe {}

/// A fake proc tree a test points the default probe at instead of the real processes.
struct ProcRoot(PathBuf);

impl Global for ProcRoot {}

impl Tf2Probe {
    /// Replaces the answer; tests pin it.
    pub fn install(cx: &mut App, probe: impl Fn() -> bool + Send + Sync + 'static) {
        cx.set_global(Tf2Probe(Arc::new(probe)));
    }

    /// Installs `probe` and returns the probe it replaced, if any, for `restore`.
    pub(crate) fn replace(
        cx: &mut App,
        probe: impl Fn() -> bool + Send + Sync + 'static,
    ) -> Option<Tf2Probe> {
        // `remove_global` panics on an absent global, so only take one that is there.
        let previous = cx
            .has_global::<Tf2Probe>()
            .then(|| cx.remove_global::<Tf2Probe>());
        Self::install(cx, probe);
        previous
    }

    /// Puts back what `replace` returned: that probe, or no global at all.
    pub(crate) fn restore(cx: &mut App, previous: Option<Tf2Probe>) {
        match previous {
            Some(probe) => cx.set_global(probe),
            None if cx.has_global::<Tf2Probe>() => drop(cx.remove_global::<Tf2Probe>()),
            None => {}
        }
    }

    /// Points the default probe at a fake proc tree, so a test never scans the real /proc.
    pub fn set_proc_root(cx: &mut App, root: PathBuf) {
        cx.set_global(ProcRoot(root));
    }

    /// The installed probe, a scan of the test's proc root, or core's `tf2_running`. It
    /// reads the system's processes: call it off the UI thread.
    pub fn get(cx: &App) -> Arc<dyn Fn() -> bool + Send + Sync> {
        if let Some(probe) = cx.try_global::<Tf2Probe>() {
            return probe.0.clone();
        }
        let Some(root) = cx.try_global::<ProcRoot>().map(|r| r.0.clone()) else {
            return Arc::new(tf2tools_core::tf2_running);
        };
        Arc::new(move || tf2tools_core::tf2_running_in(&root))
    }
}
