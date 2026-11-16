//! The background library job: once per startup it packs loose library items, and on request
//! it measures what the library takes. One global entity runs at most one pass at a time.

use gpui_kit::{App, AppContext as _, Context, Entity, EventEmitter, Global};
use tf2tools_core::{ConvertReport, LibrarySpace};

use crate::services::{OpError, run_blocking};

/// What observers hear from the library job.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LibraryPackEvent {
    /// A pass ended, measured or not; the library's figures may have changed.
    Landed,
}

/// The library job: a conversion at most once per process, and the figures of its last pass.
#[derive(Default)]
pub struct LibraryPack {
    space: Option<LibrarySpace>,
    failed: bool,
    running: bool,
    queued: bool,
    // The conversion was asked for while a measure ran, so it runs next.
    convert_queued: bool,
    converted: bool,
    passes: u64,
}

struct LibraryPackGlobal(Entity<LibraryPack>);

impl Global for LibraryPackGlobal {}

impl EventEmitter<LibraryPackEvent> for LibraryPack {}

impl LibraryPack {
    /// The app's job, created on first use without starting anything. Needs `Services`.
    pub fn global(cx: &mut App) -> Entity<Self> {
        if let Some(job) = Self::try_global(cx) {
            return job;
        }
        let job = cx.new(|_| LibraryPack::default());
        cx.set_global(LibraryPackGlobal(job.clone()));
        job
    }

    /// The app's job if anything has used it yet, for readers that must not create it.
    pub fn try_global(cx: &App) -> Option<Entity<Self>> {
        cx.try_global::<LibraryPackGlobal>().map(|g| g.0.clone())
    }

    /// Converts the library, then measures; does nothing after the first call.
    pub fn convert_once(&mut self, cx: &mut Context<Self>) {
        if std::mem::replace(&mut self.converted, true) {
            return;
        }
        if self.running {
            self.convert_queued = true;
        } else {
            self.run(true, cx);
        }
    }

    /// Measures now, or once more after the running pass.
    pub fn measure(&mut self, cx: &mut Context<Self>) {
        if self.running {
            self.queued = true;
        } else {
            self.run(false, cx);
        }
    }

    // One pass off the UI thread: the conversion when asked, then always a measure.
    fn run(&mut self, convert: bool, cx: &mut Context<Self>) {
        self.running = true;
        cx.notify();
        let pass = run_blocking(cx, move |s| {
            if convert {
                log_conversion(&s.library.convert_library());
            }
            s.library
                .space()
                .map_err(|e| OpError::core("Measure the library", e))
        });
        cx.spawn(async move |job, cx| {
            let landed = pass.await;
            let _ = job.update(cx, |job, cx| job.land(landed, cx));
        })
        .detach();
    }

    // A failed measure keeps the old figures, is logged and flags `failed`; it never toasts.
    // A waiting conversion runs next and measures too, so it absorbs a waiting measure.
    fn land(&mut self, landed: Result<LibrarySpace, OpError>, cx: &mut Context<Self>) {
        match landed {
            Ok(space) => {
                self.space = Some(space);
                self.failed = false;
            }
            Err(err) => {
                tracing::warn!("the library could not be measured: {err}");
                self.failed = true;
            }
        }
        self.running = false;
        self.passes += 1;
        cx.emit(LibraryPackEvent::Landed);
        cx.notify();
        if std::mem::take(&mut self.convert_queued) {
            self.queued = false;
            self.run(true, cx);
        } else if std::mem::take(&mut self.queued) {
            self.run(false, cx);
        }
    }

    /// What the library took at the last pass that measured.
    pub fn space(&self) -> Option<LibrarySpace> {
        self.space
    }

    /// Whether the last pass could not measure; a later success clears it.
    pub fn failed(&self) -> bool {
        self.failed
    }

    /// How many passes have landed, failed ones included.
    pub fn passes(&self) -> u64 {
        self.passes
    }

    /// Whether a pass is running now.
    pub fn is_running(&self) -> bool {
        self.running
    }
}

// Only a pass that changed something is worth a log line; failures are logged by core.
fn log_conversion(report: &ConvertReport) {
    let (huds, addons, crosshairs) = (report.huds(), report.gb_addons(), report.crosshairs());
    let kinds = [huds, addons, crosshairs];
    let changed: usize = kinds.iter().map(|k| k.packed() + k.cleaned()).sum();
    if changed == 0 {
        return;
    }
    tracing::info!(
        "library packed: {} HUDs ({} -> {} bytes), {} add-on copies ({} -> {} bytes), \
         {} crosshairs ({} -> {} bytes), {} cleaned",
        huds.packed(),
        huds.bytes_before(),
        huds.bytes_after(),
        addons.packed(),
        addons.bytes_before(),
        addons.bytes_after(),
        crosshairs.packed(),
        crosshairs.bytes_before(),
        crosshairs.bytes_after(),
        kinds.iter().map(|k| k.cleaned()).sum::<usize>(),
    );
}

/// After the first frame: the once-per-startup conversion pass.
pub fn startup(cx: &mut App) {
    LibraryPack::global(cx).update(cx, |job, cx| job.convert_once(cx));
}
