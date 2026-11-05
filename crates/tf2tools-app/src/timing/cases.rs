//! The scripted frame cases: each drives the shell one frame at a time, stamping every frame
//! inside its `on_next_frame` callback and forcing the next draw, as Dbento's app bench does.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::{Duration, Instant};

use gpui_kit::{App, Entity, ListState, ScrollHandle, Window, point, px};
use tf2tools_core::{ProfileId, StoreKind, TfClass};

use super::{Case, FrameStats, memory};
use crate::pages::hud_store::HudStoreTab;
use crate::pages::profiles::probe::Tf2Probe;
use crate::remote_images::RemoteImages;
use crate::services::Services;
use crate::shell::Shell;
use crate::shell::route::Page;
use crate::store_grid::StoreGrid;
use crate::sync::sync_hub;

#[derive(Debug, Clone, Copy)]
pub struct CaseConfig {
    /// Frame intervals to record.
    pub intervals: usize,
    /// Intervals between two switches.
    pub switch_every: usize,
    /// Pixels moved per frame; read by scroll cases.
    pub scroll_step: f32,
    /// Frames drawn after the case's page has loaded, before recording starts.
    pub warm_frames: usize,
    /// How long the case's page may take to load.
    pub load_timeout: Duration,
}

impl Default for CaseConfig {
    fn default() -> Self {
        Self {
            intervals: 300,
            switch_every: 6,
            scroll_step: 40.0,
            warm_frames: 30,
            load_timeout: Duration::from_secs(20),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct CaseReport {
    pub stats: FrameStats,
    /// Switches made, or scroll steps applied.
    pub actions: usize,
    /// The deepest scroll offset read back after a draw; 0 for cases that do not scroll.
    pub max_scroll_px: f32,
    /// Extra key=value results, printed after the standard fields.
    pub notes: Vec<(&'static str, String)>,
}

type Done = Box<dyn FnOnce(Result<CaseReport, String>, &mut Window, &mut App)>;

/// The search case cycles a rare miss, a broad match, a narrow match and the reset.
const SEARCHES: [&str; 4] = ["zq", "Sound 1", "Sound 7", ""];

/// `image_bytes` (the image cache's decoded bytes) and `foreground_decodes`; 0 without images.
#[doc(hidden)]
pub fn image_notes(cx: &App) -> [(&'static str, String); 2] {
    let images = RemoteImages::global(cx)
        .map(|images| images.read(cx).stats())
        .unwrap_or_default();
    [
        ("image_bytes", images.bytes.to_string()),
        ("foreground_decodes", images.foreground_decodes.to_string()),
    ]
}

/// Where a nav or search sample starts: the search's query start once its grid has one,
/// otherwise `acted`, the instant the sample's action ran.
#[doc(hidden)]
pub fn sample_start(case: Case, acted: Instant, query_started: Option<Instant>) -> Instant {
    match query_started {
        Some(query) if case == Case::SoundStoreSearch => query,
        _ => acted,
    }
}

/// Each search term's note keys, in `SEARCHES` order: samples, then median, p95 and max ms.
const SEARCH_KEYS: [[&str; 4]; 4] = [
    [
        "search_zq_n",
        "search_zq_median_ms",
        "search_zq_p95_ms",
        "search_zq_max_ms",
    ],
    [
        "search_sound_1_n",
        "search_sound_1_median_ms",
        "search_sound_1_p95_ms",
        "search_sound_1_max_ms",
    ],
    [
        "search_sound_7_n",
        "search_sound_7_median_ms",
        "search_sound_7_p95_ms",
        "search_sound_7_max_ms",
    ],
    [
        "search_reset_n",
        "search_reset_median_ms",
        "search_reset_p95_ms",
        "search_reset_max_ms",
    ],
];

// Sample i typed SEARCHES[i % 4], so each term's samples are every fourth interval from its own.
fn search_notes(samples: &[Duration]) -> Vec<(&'static str, String)> {
    let mut notes = Vec::new();
    for (term, [n, median, p95, max]) in SEARCH_KEYS.into_iter().enumerate() {
        let mine: Vec<Duration> = samples
            .iter()
            .skip(term)
            .step_by(SEARCHES.len())
            .copied()
            .collect();
        if let Some(s) = FrameStats::from_intervals(&mine) {
            notes.extend([
                (n, s.count.to_string()),
                (median, format!("{:.3}", s.median_ms)),
                (p95, format!("{:.3}", s.p95_ms)),
                (max, format!("{:.3}", s.max_ms)),
            ]);
        }
    }
    notes
}

/// Frames the nav case spends on the HUDs page between two visits.
const NAV_AWAY_FRAMES: usize = 10;

/// Cards the memory walk scrolls past.
const WALK_CARDS: usize = 2_000;

/// Runs `case` on `shell`'s window, then calls `done` once. `Idle` and any case without a
/// runner report `Err("<case> has no frame runner")`.
pub fn run_case(
    case: Case,
    config: CaseConfig,
    shell: Entity<Shell>,
    window: &mut Window,
    cx: &mut App,
    done: impl FnOnce(Result<CaseReport, String>, &mut Window, &mut App) + 'static,
) {
    // The applies write into the template's fake TF2, so a running game must not stop them
    // with its confirm and stall the case.
    let previous = (case == Case::ProfileApply).then(|| Tf2Probe::replace(cx, || false));
    // A runner dropped before done (window closed) leaves the probe replaced; fine for the
    // timing binary.
    let done = move |report: Result<CaseReport, String>, window: &mut Window, cx: &mut App| {
        if let Some(previous) = previous {
            Tf2Probe::restore(cx, previous);
        }
        done(report, window, cx);
    };
    let runner = Runner {
        case,
        config,
        shell,
        phase: Phase::Opening,
        started: Instant::now(),
        last: None,
        intervals: Vec::with_capacity(config.intervals),
        actions: 0,
        scroll: None,
        list: None,
        down: true,
        max_scroll_px: 0.0,
        requeries_at: 0,
        probe: Probe::Start,
        firsts_at: 0,
        max_frames: 0,
        passed: (0, Instant::now()),
        apply: Apply::default(),
        done: Some(Box::new(done)),
    };
    arm(Rc::new(RefCell::new(runner)), window);
}

/// `frames case=<name> intervals=<n> median_ms=<.3> p95_ms=<.3> max_ms=<.3> over_25ms=<n>
/// over_100ms=<n> actions=<n> max_scroll_px=<.0>` then each note as ` <key>=<value>`, or
/// `frames case=<name> error=<message>`.
pub fn report_line(case: Case, report: &Result<CaseReport, String>) -> String {
    let name = case.name();
    match report {
        Ok(r) => {
            let s = &r.stats;
            let notes: String = r.notes.iter().map(|(k, v)| format!(" {k}={v}")).collect();
            format!(
                "frames case={name} intervals={} median_ms={:.3} p95_ms={:.3} max_ms={:.3} \
                 over_25ms={} over_100ms={} actions={} max_scroll_px={:.0}{notes}",
                s.count,
                s.median_ms,
                s.p95_ms,
                s.max_ms,
                s.over_25ms,
                s.over_100ms,
                r.actions,
                r.max_scroll_px
            )
        }
        Err(message) => format!("frames case={name} error={message}"),
    }
}

// Re-arms each frame: stamp and act inside the callback, force the next draw, repeat.
fn arm(runner: Rc<RefCell<Runner>>, window: &mut Window) {
    window.on_next_frame(move |window, cx| {
        let finished = runner.borrow_mut().tick(window, cx);
        match finished {
            None => {
                window.refresh();
                arm(runner, window);
            }
            Some(result) => {
                let done = runner.borrow_mut().done.take();
                if let Some(done) = done {
                    done(result, window, cx);
                }
            }
        }
    });
}

enum Phase {
    Opening,
    Loading,
    /// Scrolling the grid to its last page; holds when paging began.
    Paging(Instant),
    Warming(usize),
    Recording,
}

/// Where a nav or search case is within its current sample.
enum Probe {
    /// The next sample starts on this frame.
    Start,
    /// The action ran at `since`; `frames` drawn since, the result not shown yet.
    Waiting { since: Instant, frames: usize },
    /// The result is in; the sample ends at the next frame callback, after the draw shows it.
    Shown { since: Instant, frames: usize },
    /// Nav only: frames left on the HUDs page before the next visit.
    Away(usize),
}

struct Runner {
    case: Case,
    config: CaseConfig,
    shell: Entity<Shell>,
    phase: Phase,
    started: Instant,
    last: Option<Instant>,
    intervals: Vec<Duration>,
    actions: usize,
    /// The scrolled view's handle, taken once the page has warmed; None for switch cases.
    scroll: Option<ScrollHandle>,
    /// The Logs list, which scrolls through a `ListState` instead of a handle.
    list: Option<ListState>,
    down: bool,
    max_scroll_px: f32,
    /// The grid's re-query count when recording began.
    requeries_at: usize,
    probe: Probe,
    /// The grid's first-page count when the current search was typed.
    firsts_at: usize,
    /// The most frames any nav or search sample took.
    max_frames: usize,
    /// The walk's deepest first visible card, and when it last grew.
    passed: (usize, Instant),
    apply: Apply,
    done: Option<Done>,
}

/// The profile apply case's bookkeeping across frames.
#[derive(Default)]
struct Apply {
    /// When the apply in flight started and on which profile; None between applies.
    started: Option<(Instant, ProfileId)>,
    durations: Vec<Duration>,
    /// Recording frames not counted because no apply was in flight.
    frames_outside: usize,
    frames_total: usize,
}

impl Runner {
    /// One frame of the state machine: load the case's page, warm, then record and act.
    /// Returns the result once the case has finished.
    fn tick(&mut self, window: &mut Window, cx: &mut App) -> Option<Result<CaseReport, String>> {
        let Some(home) = home_page(self.case) else {
            return Some(Err(format!("{} has no frame runner", self.case.name())));
        };
        match self.phase {
            // Rebuild the case's page (a kept store or log page rereads on return), so it
            // loads the library as it is now rather than as the shell's start page found it.
            Phase::Opening => {
                let away = Page::ALL.into_iter().find(|&p| p != home).unwrap_or(home);
                self.shell.update(cx, |shell, cx| {
                    shell.navigate(away, cx);
                    shell.navigate(home, cx);
                });
                self.phase = Phase::Loading;
                None
            }
            Phase::Loading => {
                self.shell.update(cx, |shell, cx| shell.navigate(home, cx));
                if self.case == Case::GameBananaScroll {
                    select_gamebanana(&self.shell, window, cx);
                }
                if self.page_ready(home, cx) {
                    self.phase = if self.case == Case::SoundStoreSyncScroll {
                        start_sound_sync(cx);
                        Phase::Paging(Instant::now())
                    } else if self.case == Case::LogsScroll {
                        Phase::Paging(Instant::now())
                    } else {
                        Phase::Warming(self.config.warm_frames)
                    };
                } else if self.started.elapsed() > self.config.load_timeout {
                    return Some(Err(format!(
                        "the {} page did not load within {:?}",
                        home.label(),
                        self.config.load_timeout
                    )));
                }
                None
            }
            // Page the grid to its end first, so the sync's anchored reloads re-read every
            // loaded page: the worst case for scrolling during a sync. The Logs case pages its
            // list to the end, so the scroll covers every row.
            Phase::Paging(since) => {
                let ended = if self.case == Case::LogsScroll {
                    self.page_logs(cx)
                } else {
                    store_grid(self.case, &self.shell, cx)
                        .is_some_and(|grid| grid.read(cx).is_end())
                };
                if ended {
                    self.phase = Phase::Warming(self.config.warm_frames);
                } else if since.elapsed() > self.config.load_timeout {
                    let view = if self.case == Case::LogsScroll {
                        "list"
                    } else {
                        "grid"
                    };
                    return Some(Err(format!(
                        "the {} {view} did not reach its end within {:?}",
                        home.label(),
                        self.config.load_timeout
                    )));
                } else if let Some(handle) = scroll_handle(self.case, &self.shell, cx) {
                    handle.set_offset(point(px(0.), -handle.max_offset().y));
                }
                None
            }
            Phase::Warming(0) => {
                self.phase = Phase::Recording;
                // A sync that is not running would record a quiet scroll under the sync name.
                if self.case == Case::SoundStoreSyncScroll {
                    if !sound_sync_running(cx) {
                        return Some(Err("the sound sync is not running".into()));
                    }
                    self.requeries_at = store_grid(self.case, &self.shell, cx)
                        .map_or(0, |grid| grid.read(cx).requeries());
                }
                // A view that cannot move would record a calibration run under a scroll name.
                if self.case == Case::LogsScroll {
                    let list = self
                        .shell
                        .read(cx)
                        .logs_page()
                        .map(|p| p.read(cx).list_state().clone())
                        .filter(|list| list.max_offset_for_scrollbar().y > px(0.));
                    let Some(list) = list else {
                        return Some(Err(format!("nothing to scroll in {}", self.case.name())));
                    };
                    self.list = Some(list);
                } else if is_scroll(self.case) {
                    let handle = scroll_handle(self.case, &self.shell, cx)
                        .filter(|h| h.max_offset().y > px(0.));
                    let Some(handle) = handle else {
                        return Some(Err(format!("nothing to scroll in {}", self.case.name())));
                    };
                    self.scroll = Some(handle);
                }
                self.passed = (0, Instant::now());
                self.step(window, cx)
            }
            Phase::Warming(left) => {
                self.phase = Phase::Warming(left - 1);
                None
            }
            Phase::Recording => self.step(window, cx),
        }
    }

    fn step(&mut self, window: &mut Window, cx: &mut App) -> Option<Result<CaseReport, String>> {
        match self.case {
            Case::ProfileApply => self.apply(window, cx),
            Case::SoundStoreNav
            | Case::SoundStoreSearch
            | Case::ProfilesNav
            | Case::AddonsNav
            | Case::LogsNav => self.probe(cx),
            Case::SoundStoreWalk => self.walk(cx),
            _ => self.record(cx),
        }
    }

    // One frame of a nav or search sample: act, wait for the grid to show the result, then
    // end the sample at the frame callback after that draw.
    fn probe(&mut self, cx: &mut App) -> Option<Result<CaseReport, String>> {
        let now = Instant::now();
        match self.probe {
            Probe::Start if self.intervals.len() == self.config.intervals => {
                let stats = FrameStats::from_intervals(&self.intervals)
                    .ok_or_else(|| format!("{} recorded no samples", self.case.name()));
                let mut notes = vec![("max_frames", self.max_frames.to_string())];
                if self.case == Case::SoundStoreSearch {
                    notes.extend(search_notes(&self.intervals));
                }
                return Some(stats.map(|stats| CaseReport {
                    stats,
                    actions: self.actions,
                    max_scroll_px: 0.0,
                    notes,
                }));
            }
            Probe::Start => {
                self.act_probe(cx);
                self.probe = Probe::Waiting {
                    since: now,
                    frames: 0,
                };
            }
            Probe::Waiting { since, frames } => {
                if now - since > self.config.load_timeout {
                    return Some(Err(format!(
                        "{} sample {} did not show within {:?}",
                        self.case.name(),
                        self.actions,
                        self.config.load_timeout
                    )));
                }
                let frames = frames + 1;
                self.probe = if self.shown(cx) {
                    Probe::Shown { since, frames }
                } else {
                    Probe::Waiting { since, frames }
                };
            }
            Probe::Shown { since, frames } => {
                // A search sample is the work after typing pauses, not the pause itself.
                let query =
                    store_grid(self.case, &self.shell, cx).map(|g| g.read(cx).query_started());
                let sample = now.saturating_duration_since(sample_start(self.case, since, query));
                self.intervals.push(sample);
                self.max_frames = self.max_frames.max(frames + 1);
                self.probe = if nav_page(self.case).is_some() {
                    self.shell
                        .update(cx, |shell, cx| shell.navigate(Page::Huds, cx));
                    Probe::Away(NAV_AWAY_FRAMES)
                } else {
                    Probe::Start
                };
            }
            Probe::Away(0) => self.probe = Probe::Start,
            Probe::Away(left) => self.probe = Probe::Away(left - 1),
        }
        None
    }

    // Starts the next sample: a visit to the case's page, or the next search typed.
    fn act_probe(&mut self, cx: &mut App) {
        if let Some(page) = nav_page(self.case) {
            self.shell.update(cx, |shell, cx| shell.navigate(page, cx));
        } else if let Some(grid) = store_grid(self.case, &self.shell, cx) {
            self.firsts_at = grid.read(cx).first_pages();
            let text = SEARCHES[self.actions % SEARCHES.len()].to_string();
            grid.update(cx, |grid, cx| grid.search_typed(text, cx));
        }
        self.actions += 1;
    }

    // Nav: the rebuilt Sound Store shows cards, or the rebuilt page has loaded. Search: the
    // new query's first page landed.
    fn shown(&self, cx: &App) -> bool {
        let Some(grid) = store_grid(self.case, &self.shell, cx) else {
            let shell = self.shell.read(cx);
            return nav_page(self.case).is_some_and(|page| page_loaded(shell, page, cx));
        };
        let grid = grid.read(cx);
        match self.case {
            Case::SoundStoreNav => grid.is_loaded() && !grid.cards().is_empty(),
            _ => grid.first_pages() > self.firsts_at,
        }
    }

    // One frame of the memory walk: scroll down a step, paging as the grid nears its end,
    // until the first visible card passes `WALK_CARDS` or the store runs out.
    fn walk(&mut self, cx: &mut App) -> Option<Result<CaseReport, String>> {
        let now = Instant::now();
        if let Some(last) = self.last.replace(now) {
            self.intervals.push(now - last);
        }
        let (Some(grid), Some(handle)) = (store_grid(self.case, &self.shell, cx), &self.scroll)
        else {
            return Some(Err("the sound store grid went away".into()));
        };
        let offset = -f32::from(handle.offset().y);
        let max = f32::from(handle.max_offset().y);
        self.max_scroll_px = self.max_scroll_px.max(offset);
        let first = grid.read(cx).first_visible_index().unwrap_or(0);
        if first > self.passed.0 {
            self.passed = (first, now);
        }
        let ended = grid.read(cx).is_end() && offset >= max;
        if self.passed.0 >= WALK_CARDS || ended {
            return Some(self.walk_report(cx));
        }
        if now - self.passed.1 > self.config.load_timeout {
            return Some(Err(format!(
                "the walk stalled at card {} for {:?}",
                self.passed.0, self.config.load_timeout
            )));
        }
        handle.set_offset(point(
            px(0.),
            px(-(offset + self.config.scroll_step).min(max)),
        ));
        self.actions += 1;
        None
    }

    // Sound cards carry no image, so image memory is measured on the image grids instead.
    fn walk_report(&self, cx: &App) -> Result<CaseReport, String> {
        let stats = FrameStats::from_intervals(&self.intervals)
            .ok_or_else(|| "the walk recorded no intervals".to_string())?;
        let mem = memory();
        let mut notes = vec![
            ("passed_cards", self.passed.0.to_string()),
            ("rss_kb", mem.map_or(0, |m| m.rss_kb).to_string()),
            ("pss_kb", mem.map_or(0, |m| m.pss_kb).to_string()),
        ];
        notes.extend(image_notes(cx));
        notes.push(("image_memory", "measured_on_image_grids".to_string()));
        Ok(CaseReport {
            stats,
            actions: self.actions,
            max_scroll_px: self.max_scroll_px,
            notes,
        })
    }

    // Every recording tick stamps; from the second on it pushes the interval since the last.
    fn record(&mut self, cx: &mut App) -> Option<Result<CaseReport, String>> {
        let now = Instant::now();
        if let Some(last) = self.last.replace(now) {
            self.intervals.push(now - last);
        }
        // The offset read here is the one the last draw clamped, so it proves the view moved.
        if let Some(handle) = &self.scroll {
            self.max_scroll_px = self.max_scroll_px.max(-f32::from(handle.offset().y));
        }
        if let Some(list) = &self.list {
            let offset = -f32::from(list.scroll_px_offset_for_scrollbar().y);
            self.max_scroll_px = self.max_scroll_px.max(offset);
        }
        let n = self.intervals.len();
        if n == self.config.intervals {
            let stats = FrameStats::from_intervals(&self.intervals)
                .ok_or_else(|| format!("{} recorded no intervals", self.case.name()));
            let mut notes = match cards_loaded(self.case, &self.shell, cx) {
                Some(cards) => vec![("cards", cards.to_string())],
                None => vec![],
            };
            // The image grids' decoded bytes at the end, for the image memory budget.
            if self.case.shows_images() {
                notes.extend(image_notes(cx));
            }
            if self.case == Case::LogsScroll {
                let rows = self
                    .shell
                    .read(cx)
                    .logs_page()
                    .map_or(0, |p| p.read(cx).entries().len());
                notes.push(("rows", rows.to_string()));
            }
            if self.case == Case::SoundStoreSyncScroll {
                // A sync that ended mid-recording left a quiet tail under the sync name.
                if !sound_sync_running(cx) {
                    return Some(Err("the sound sync ended before the recording did".into()));
                }
                let requeries = store_grid(self.case, &self.shell, cx)
                    .map_or(0, |grid| grid.read(cx).requeries())
                    .saturating_sub(self.requeries_at);
                // With no re-query the grid never reloaded, so the scroll measured no sync cost.
                if requeries == 0 {
                    return Some(Err("the grid saw no re-query during the sync".into()));
                }
                notes.push(("requeries", requeries.to_string()));
                notes.push(("sync_running", sound_sync_running(cx).to_string()));
            }
            return Some(stats.map(|stats| CaseReport {
                stats,
                actions: self.actions,
                max_scroll_px: self.max_scroll_px,
                notes,
            }));
        }
        if let Some(handle) = &self.scroll {
            let max = f32::from(handle.max_offset().y);
            let (next, down) = bounce(
                -f32::from(handle.offset().y),
                max,
                self.config.scroll_step,
                self.down,
            );
            handle.set_offset(point(px(0.), px(-next)));
            self.down = down;
            self.actions += 1;
        } else if let Some(list) = &self.list {
            // The list measures rows as they come into view, so its max grows while scrolling;
            // bounce against the value as it stands now.
            let offset = -f32::from(list.scroll_px_offset_for_scrollbar().y);
            let max = f32::from(list.max_offset_for_scrollbar().y);
            let (next, down) = bounce(offset, max, self.config.scroll_step, self.down);
            list.scroll_by(px(next - offset));
            self.down = down;
            self.actions += 1;
        } else if n.is_multiple_of(self.config.switch_every) {
            self.act(cx);
        }
        None
    }

    // One frame of the apply case: between applies, start the next on the other profile; while
    // one is in flight, count each frame's interval; once it ends, note how long it took.
    fn apply(&mut self, window: &mut Window, cx: &mut App) -> Option<Result<CaseReport, String>> {
        let now = Instant::now();
        let last = self.last.replace(now);
        self.apply.frames_total += 1;
        let Some(page) = self.shell.read(cx).profiles_page() else {
            return Some(Err("the Profiles page went away".into()));
        };
        if let Some((started, id)) = &self.apply.started {
            let started = *started;
            // The interval ending on the frame that sees the apply finish was drawn while it
            // ran, so it counts; an apply shorter than a frame still yields one interval.
            if let Some(last) = last {
                self.intervals.push(now - last);
            }
            if page.read(cx).activating().is_some() {
                if now - started > self.config.load_timeout {
                    return Some(Err(format!(
                        "profile-apply: apply {} did not end within {:?}",
                        self.actions, self.config.load_timeout
                    )));
                }
                return None;
            }
            // Core marks a profile active only when every step took, so anything else is a
            // failed apply; counting it would hide a broken run behind a smaller sample.
            let active = cx.global::<Services>().library.active_profile();
            if !matches!(active, Ok(Some(active)) if active == *id) {
                return Some(Err(format!("profile-apply: apply {} failed", self.actions)));
            }
            self.apply.durations.push(now - started);
            self.apply.started = None;
            return (self.intervals.len() >= self.config.intervals).then(|| self.apply_report());
        }
        // No apply in flight on this frame: it is not counted, and the next one starts here.
        self.apply.frames_outside += 1;
        if self.intervals.len() >= self.config.intervals {
            return Some(self.apply_report());
        }
        // The runner alternates itself: the page's active profile lags its reload.
        let id = match page.read(cx).profiles() {
            [a, b, ..] => [a, b][self.actions % 2].id().clone(),
            _ => return Some(Err("profile-apply needs two profiles".into())),
        };
        page.update(cx, |page, cx| page.activate(id.clone(), window, cx));
        // `activate` returns silently when it cannot start; looping would only hit the timeout.
        if page.read(cx).activating().is_none() {
            return Some(Err("profile-apply: activate did not start".into()));
        }
        self.apply.started = Some((now, id));
        self.actions += 1;
        None
    }

    fn apply_report(&self) -> Result<CaseReport, String> {
        let stats = FrameStats::from_intervals(&self.intervals)
            .ok_or_else(|| "profile-apply recorded no intervals".to_string())?;
        let applies = FrameStats::from_intervals(&self.apply.durations)
            .ok_or_else(|| "profile-apply finished no apply".to_string())?;
        Ok(CaseReport {
            stats,
            actions: self.actions,
            max_scroll_px: 0.0,
            notes: vec![
                ("applies", self.apply.durations.len().to_string()),
                ("apply_median_ms", format!("{:.3}", applies.median_ms)),
                ("apply_max_ms", format!("{:.3}", applies.max_ms)),
                ("frames_outside", self.apply.frames_outside.to_string()),
                ("frames_total", self.apply.frames_total.to_string()),
            ],
        })
    }

    // The k-th switch (k from 1) cycles through the sidebar's pages or the class tabs.
    fn act(&mut self, cx: &mut App) {
        match self.case {
            Case::PageSwitch => {
                self.actions += 1;
                let page = Page::ALL[self.actions % Page::ALL.len()];
                self.shell.update(cx, |shell, cx| shell.navigate(page, cx));
            }
            Case::ClassTab => {
                self.actions += 1;
                let class = TfClass::ALL[self.actions % TfClass::ALL.len()];
                if let Some(page) = self.shell.read(cx).crosshairs_page() {
                    page.update(cx, |page, cx| page.set_class(class, cx));
                }
            }
            _ => {}
        }
    }

    // One paging frame of the Logs case: true once the list has no more rows. `load_more` does
    // nothing while a page is loading, so asking every frame pages as fast as queries land.
    fn page_logs(&self, cx: &mut App) -> bool {
        let Some(page) = self.shell.read(cx).logs_page() else {
            return false;
        };
        if !page.read(cx).has_more() {
            return true;
        }
        page.update(cx, |page, cx| page.load_more(cx));
        false
    }

    fn page_ready(&self, page: Page, cx: &App) -> bool {
        let shell = self.shell.read(cx);
        if self.case == Case::CuratedScroll {
            return shell
                .hud_store_page()
                .and_then(|p| p.read(cx).curated())
                .is_some_and(|tab| tab.read(cx).is_loaded());
        }
        if let Some(grid) = store_grid(self.case, &self.shell, cx) {
            return grid.read(cx).is_loaded();
        }
        page_loaded(shell, page, cx)
    }
}

/// True once `page`'s own view has loaded: the Crosshairs page also needs its previews.
/// Store pages are judged by their grid instead, so they read false here.
fn page_loaded(shell: &Shell, page: Page, cx: &App) -> bool {
    match page {
        Page::Huds => shell.huds_page().is_some_and(|p| p.read(cx).is_loaded()),
        Page::Crosshairs => shell.crosshairs_page().is_some_and(|p| {
            let p = p.read(cx);
            p.is_loaded() && p.timings().previews_ready.is_some()
        }),
        Page::Profiles => shell
            .profiles_page()
            .is_some_and(|p| p.read(cx).is_loaded()),
        Page::Addons => shell.addons_page().is_some_and(|p| p.read(cx).is_loaded()),
        Page::Logs => shell.logs_page().is_some_and(|p| p.read(cx).is_loaded()),
        _ => false,
    }
}

/// One scroll step: (next offset, still moving down). Turns at 0 and `max`.
pub fn bounce(offset: f32, max: f32, step: f32, down: bool) -> (f32, bool) {
    if down {
        let next = (offset + step).min(max);
        (next, next < max)
    } else {
        let next = (offset - step).max(0.);
        (next, next <= 0.)
    }
}

/// The page a case loads before recording, or None when the case has no frame runner.
fn home_page(case: Case) -> Option<Page> {
    match case {
        Case::Calibrate | Case::PageSwitch | Case::HudGridScroll => Some(Page::Huds),
        Case::ClassTab | Case::PickerScroll | Case::CrosshairsScroll => Some(Page::Crosshairs),
        Case::SoundStoreNav | Case::ProfilesNav | Case::AddonsNav | Case::LogsNav => {
            Some(Page::Huds)
        }
        Case::LogsScroll => Some(Page::Logs),
        Case::ProfileApply => Some(Page::Profiles),
        Case::SoundStoreScroll
        | Case::SoundStoreSyncScroll
        | Case::SoundStoreSearch
        | Case::SoundStoreWalk => Some(Page::SoundStore),
        Case::CuratedScroll | Case::GameBananaScroll => Some(Page::HudStore),
        Case::Idle => None,
    }
}

/// The page a nav case visits on each sample, or None for other cases.
fn nav_page(case: Case) -> Option<Page> {
    match case {
        Case::SoundStoreNav => Some(Page::SoundStore),
        Case::ProfilesNav => Some(Page::Profiles),
        Case::AddonsNav => Some(Page::Addons),
        Case::LogsNav => Some(Page::Logs),
        _ => None,
    }
}

fn is_scroll(case: Case) -> bool {
    matches!(
        case,
        Case::PickerScroll
            | Case::HudGridScroll
            | Case::CrosshairsScroll
            | Case::SoundStoreScroll
            | Case::CuratedScroll
            | Case::GameBananaScroll
            | Case::SoundStoreSyncScroll
            | Case::SoundStoreWalk
    )
}

/// The handle a scroll case moves, or None for a case that does not scroll or a page not built.
fn scroll_handle(case: Case, shell: &Entity<Shell>, cx: &App) -> Option<ScrollHandle> {
    if let Some(grid) = store_grid(case, shell, cx) {
        return Some(
            grid.read(cx)
                .card_grid()
                .scroll_handle()
                .0
                .borrow()
                .base_handle
                .clone(),
        );
    }
    let shell = shell.read(cx);
    match case {
        Case::PickerScroll => shell.crosshairs_page().map(|p| p.read(cx).picker_scroll()),
        Case::CrosshairsScroll => shell.crosshairs_page().map(|p| p.read(cx).page_scroll()),
        Case::HudGridScroll => shell.huds_page().map(|p| p.read(cx).grid_scroll()),
        Case::CuratedScroll => shell
            .hud_store_page()
            .and_then(|p| p.read(cx).curated())
            .map(|tab| tab.read(cx).grid_scroll()),
        _ => None,
    }
}

/// The paged store grid a store case scrolls, once its page and tab are built.
fn store_grid(case: Case, shell: &Entity<Shell>, cx: &App) -> Option<Entity<StoreGrid>> {
    let shell = shell.read(cx);
    match case {
        Case::SoundStoreScroll
        | Case::SoundStoreSyncScroll
        | Case::SoundStoreNav
        | Case::SoundStoreSearch
        | Case::SoundStoreWalk => shell.sound_store_page().map(|p| p.read(cx).grid().clone()),
        Case::GameBananaScroll => shell
            .hud_store_page()
            .and_then(|p| p.read(cx).gamebanana())
            .map(|tab| tab.read(cx).grid().clone()),
        _ => None,
    }
}

// A forced sound sync through the app's hub; without a hub nothing starts, and the
// recording check reports it.
fn start_sound_sync(cx: &mut App) {
    if let Some(hub) = sync_hub(cx) {
        hub.update(cx, |hub, cx| hub.start(StoreKind::Sounds, true, cx));
    }
}

fn sound_sync_running(cx: &App) -> bool {
    sync_hub(cx).is_some_and(|hub| hub.read(cx).is_running(StoreKind::Sounds))
}

// The HUD Store opens on its Curated tab, and navigating rebuilds the page, so the GameBanana
// case reselects its tab whenever the page shows another.
fn select_gamebanana(shell: &Entity<Shell>, window: &mut Window, cx: &mut App) {
    let Some(page) = shell.read(cx).hud_store_page() else {
        return;
    };
    if page.read(cx).tab() != HudStoreTab::GameBanana {
        page.update(cx, |page, cx| {
            page.select(HudStoreTab::GameBanana, window, cx)
        });
    }
}

/// Cards a store case has loaded by the end of its run; None for other cases.
fn cards_loaded(case: Case, shell: &Entity<Shell>, cx: &App) -> Option<usize> {
    if case == Case::CuratedScroll {
        let tab = shell.read(cx).hud_store_page()?.read(cx).curated()?;
        return Some(tab.read(cx).shown_ids().len());
    }
    store_grid(case, shell, cx).map(|grid| grid.read(cx).cards().len())
}
