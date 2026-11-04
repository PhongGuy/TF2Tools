//! The `TF2TOOLS_TIMING` hook: startup marks printed to stderr, the first frame after the HUDs
//! page's first load, and frame-interval statistics. Without the global every hook is a no-op.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use gpui_kit::{App, Global, WeakEntity, Window};

use crate::shell::Shell;

pub mod cases;

/// A scripted frame case, one per timing process.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Case {
    Calibrate,
    PageSwitch,
    ClassTab,
    PickerScroll,
    HudGridScroll,
    CrosshairsScroll,
    SoundStoreScroll,
    CuratedScroll,
    GameBananaScroll,
    SoundStoreSyncScroll,
    SoundStoreNav,
    SoundStoreSearch,
    SoundStoreWalk,
    ProfilesNav,
    AddonsNav,
    LogsNav,
    LogsScroll,
    ProfileApply,
    Idle,
}

impl Case {
    pub const ALL: [Case; 19] = [
        Case::Calibrate,
        Case::PageSwitch,
        Case::ClassTab,
        Case::PickerScroll,
        Case::HudGridScroll,
        Case::CrosshairsScroll,
        Case::SoundStoreScroll,
        Case::CuratedScroll,
        Case::GameBananaScroll,
        Case::SoundStoreSyncScroll,
        Case::SoundStoreNav,
        Case::SoundStoreSearch,
        Case::SoundStoreWalk,
        Case::ProfilesNav,
        Case::AddonsNav,
        Case::LogsNav,
        Case::LogsScroll,
        Case::ProfileApply,
        Case::Idle,
    ];

    /// The case's name as `TF2TOOLS_TIMING` spells it.
    pub fn name(self) -> &'static str {
        match self {
            Case::Calibrate => "calibrate",
            Case::PageSwitch => "page-switch",
            Case::ClassTab => "class-tab",
            Case::PickerScroll => "picker-scroll",
            Case::HudGridScroll => "hud-grid-scroll",
            Case::CrosshairsScroll => "crosshairs-scroll",
            Case::SoundStoreScroll => "sound-store-scroll",
            Case::CuratedScroll => "curated-scroll",
            Case::GameBananaScroll => "gamebanana-scroll",
            Case::SoundStoreSyncScroll => "sound-store-sync-scroll",
            Case::SoundStoreNav => "sound-store-nav",
            Case::SoundStoreSearch => "sound-store-search",
            Case::SoundStoreWalk => "sound-store-walk",
            Case::ProfilesNav => "profiles-nav",
            Case::AddonsNav => "addons-nav",
            Case::LogsNav => "logs-nav",
            Case::LogsScroll => "logs-scroll",
            Case::ProfileApply => "profile-apply",
            Case::Idle => "idle",
        }
    }

    /// True for cases that need a seeded store (`seed-store`); `--cases all` skips them.
    pub fn needs_store(self) -> bool {
        matches!(
            self,
            Case::SoundStoreScroll
                | Case::CuratedScroll
                | Case::GameBananaScroll
                | Case::SoundStoreSyncScroll
                | Case::SoundStoreNav
                | Case::SoundStoreSearch
                | Case::SoundStoreWalk
        )
    }

    /// True for the store grids whose cards load images: the two HUD Store tabs.
    pub fn shows_images(self) -> bool {
        matches!(self, Case::CuratedScroll | Case::GameBananaScroll)
    }

    /// True for cases that need seeded library data (`seed-synthetic --logs`); `--cases all`
    /// skips them.
    pub fn needs_seed(self) -> bool {
        matches!(self, Case::LogsScroll | Case::ProfileApply)
    }

    /// The case with exactly this name.
    pub fn from_name(name: &str) -> Option<Case> {
        Case::ALL.into_iter().find(|case| case.name() == name)
    }
}

/// This process's memory, in kB.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Memory {
    pub rss_kb: u64,
    pub pss_kb: u64,
}

/// VmRSS from /proc/self/status and Pss from /proc/self/smaps_rollup; None off Linux.
pub fn memory() -> Option<Memory> {
    // Both files hold `Key:   <n> kB` lines; read the number after the key.
    let field = |path: &str, key: &str| -> Option<u64> {
        let text = std::fs::read_to_string(path).ok()?;
        text.lines().find_map(|line| {
            let mut words = line.split_whitespace();
            (words.next()? == key).then(|| words.next()?.parse().ok())?
        })
    };
    Some(Memory {
        rss_kb: field("/proc/self/status", "VmRSS:")?,
        pss_kb: field("/proc/self/smaps_rollup", "Pss:")?,
    })
}

/// What `TF2TOOLS_TIMING` asked for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimingMode {
    Off,
    Startup,
    Case(Case),
}

impl TimingMode {
    /// None, "" or "0": Off; "1": Startup; a case name: Case; anything else: Err naming the value.
    pub fn parse(value: Option<&str>) -> Result<Self, String> {
        match value {
            None | Some("") | Some("0") => Ok(TimingMode::Off),
            Some("1") => Ok(TimingMode::Startup),
            Some(name) => Case::from_name(name)
                .map(TimingMode::Case)
                .ok_or_else(|| format!("unknown TF2TOOLS_TIMING value {name:?}")),
        }
    }

    /// `parse` of the `TF2TOOLS_TIMING` variable; a non-UTF-8 value is an error.
    pub fn from_env() -> Result<Self, String> {
        match std::env::var("TF2TOOLS_TIMING") {
            Ok(value) => Self::parse(Some(&value)),
            Err(std::env::VarError::NotPresent) => Self::parse(None),
            Err(std::env::VarError::NotUnicode(raw)) => {
                Err(format!("unknown TF2TOOLS_TIMING value {raw:?}"))
            }
        }
    }
}

/// Summary of a run's frame intervals.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FrameStats {
    pub count: usize,
    pub median_ms: f64,
    pub p95_ms: f64,
    pub max_ms: f64,
    pub over_25ms: usize,
    pub over_100ms: usize,
}

impl FrameStats {
    /// Nearest-rank median and p95 over whole-microsecond intervals; None when empty.
    pub fn from_intervals(intervals: &[Duration]) -> Option<Self> {
        let mut micros: Vec<u128> = intervals.iter().map(Duration::as_micros).collect();
        micros.sort_unstable();
        let count = micros.len();
        let max = *micros.last()?;
        // Nearest rank in integer percent, so 95% of 20 is rank 19 without float rounding.
        let rank = |percent: usize| micros[(percent * count).div_ceil(100).max(1) - 1];
        let ms = |us: u128| us as f64 / 1000.0;
        Some(Self {
            count,
            median_ms: ms(rank(50)),
            p95_ms: ms(rank(95)),
            max_ms: ms(max),
            over_25ms: micros.iter().filter(|&&us| us > 25_000).count(),
            over_100ms: micros.iter().filter(|&&us| us > 100_000).count(),
        })
    }
}

/// The app's timing state, a gpui `Global`. Clone shares the same marks.
#[derive(Clone)]
pub struct Timing {
    mode: TimingMode,
    marks: Rc<RefCell<Vec<(&'static str, SystemTime)>>>,
    first_frame_done: Rc<Cell<bool>>,
    shell: Rc<RefCell<Option<WeakEntity<Shell>>>>,
}

impl Global for Timing {}

impl Timing {
    pub fn new(mode: TimingMode) -> Self {
        Self {
            mode,
            marks: Rc::default(),
            first_frame_done: Rc::default(),
            shell: Rc::default(),
        }
    }

    pub fn mode(&self) -> TimingMode {
        self.mode
    }

    /// Records `name` now and prints `mark <name> <unix_ns>` to stderr;
    /// "first-frame" prints `first-frame <unix_ns>` (the perf-method line).
    pub fn mark(&self, name: &'static str) {
        let now = SystemTime::now();
        self.marks.borrow_mut().push((name, now));
        let ns = now
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        if name == "first-frame" {
            eprintln!("first-frame {ns}");
        } else {
            eprintln!("mark {name} {ns}");
        }
    }

    /// Mark names in order.
    pub fn marks(&self) -> Vec<&'static str> {
        self.marks.borrow().iter().map(|(name, _)| *name).collect()
    }
}

/// Makes `timing` the app's timing global.
pub fn install(cx: &mut App, timing: Timing) {
    cx.set_global(timing);
}

/// `Timing::mark` on the global; nothing without it.
pub fn mark(cx: &App, name: &'static str) {
    if let Some(timing) = cx.try_global::<Timing>() {
        timing.mark(name);
    }
}

/// Remembers the window's shell for the frame cases; nothing without the global.
pub fn register_shell(shell: WeakEntity<Shell>, cx: &App) {
    if let Some(timing) = cx.try_global::<Timing>() {
        *timing.shell.borrow_mut() = Some(shell);
    }
}

/// Called after a successful HUDs library load. The first time only: marks "huds-loaded",
/// then marks "first-frame" from the next `on_next_frame` and starts the mode's case.
pub fn huds_loaded(window: &mut Window, cx: &mut App) {
    let Some(timing) = cx.try_global::<Timing>().cloned() else {
        return;
    };
    // The flag flips before marking, so a page rebuilt by navigation never marks again.
    if timing.first_frame_done.replace(true) {
        return;
    }
    timing.mark("huds-loaded");
    window.on_next_frame(move |window, cx| {
        timing.mark("first-frame");
        match timing.mode {
            TimingMode::Case(Case::Idle) => idle(cx),
            TimingMode::Case(case) => {
                let shell = timing.shell.borrow().as_ref().and_then(WeakEntity::upgrade);
                let Some(shell) = shell else {
                    let err = Err("no shell registered".to_string());
                    eprintln!("{}", cases::report_line(case, &err));
                    std::process::exit(1);
                };
                let config = cases::CaseConfig::default();
                cases::run_case(case, config, shell, window, cx, move |report, _, cx| {
                    eprintln!("{}", cases::report_line(case, &report));
                    // A failed case exits non-zero, so the driver cannot mistake it for a run.
                    if report.is_err() {
                        std::process::exit(1);
                    }
                    cx.quit();
                });
            }
            _ => {}
        }
    });
}

// The idle case forces nothing: it lets the window settle for 1 s, then names a 5 s window
// in which the external damage log should show no frames from this app.
fn idle(cx: &mut App) {
    cx.spawn(async move |cx| {
        let executor = cx.background_executor().clone();
        executor.timer(Duration::from_secs(1)).await;
        let from = unix_ns();
        executor.timer(Duration::from_secs(5)).await;
        eprintln!("idle {from} {}", unix_ns());
        cx.update(|cx| cx.quit());
    })
    .detach();
}

fn unix_ns() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos()
}
