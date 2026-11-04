//! The "is TF2 running" answer the Profiles page asks before activating, as a global so
//! tests can pin it or point the default scan at a fake proc tree.

use std::path::PathBuf;
use std::sync::Arc;

use gpui_kit::{App, Global};

/// Answers "is TF2 running" for the Profiles page; a scan of the proc root unless replaced.
pub struct Tf2Probe(Arc<dyn Fn() -> bool + Send + Sync>);

impl Global for Tf2Probe {}

/// The directory the default probe scans; `/proc` unless a test points it elsewhere.
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

    /// The installed probe, or a scan of the proc root with core's `tf2_running_in`. It
    /// reads the file system: call it off the UI thread.
    pub fn get(cx: &App) -> Arc<dyn Fn() -> bool + Send + Sync> {
        if let Some(probe) = cx.try_global::<Tf2Probe>() {
            return probe.0.clone();
        }
        // Off Linux there is no /proc, so the scan reads false, as core's `tf2_running` does.
        let root = cx
            .try_global::<ProcRoot>()
            .map_or_else(|| PathBuf::from("/proc"), |r| r.0.clone());
        Arc::new(move || tf2tools_core::tf2_running_in(&root))
    }
}
