//! The pacer GameBanana API calls and logs.tf requests go through, one instance each: calls
//! start at least a gap apart, calls the player waits on go first, and empty replies back off.

use std::future::Future;
use std::pin::pin;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use tokio::sync::Notify;
use tokio::time::{Instant, sleep_until};

/// The ceiling the gap backs off to after empty replies.
pub(crate) const MAX_GAP: Duration = Duration::from_secs(8);
/// Clean replies in a row, while backed off, that bring the gap back to its base.
pub(crate) const CLEAN_TO_RESET: u32 = 10;
/// Answers in a row, after a 429 widened the gap, that halve it back toward the base.
pub(crate) const EASE_AFTER: u32 = 20;
/// Empty replies in a row within one call before it fails as throttled.
pub(crate) const EMPTY_LIMIT: u32 = 3;

/// Who is waiting on a call: interactive calls start ahead of any waiting background call.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Priority {
    /// The player is waiting on the reply, such as a file listing.
    Interactive,
    /// Work the player does not watch, such as sync pages.
    Background,
}

/// What one attempt got back: a good value, an empty reply the pacer retries, or a
/// malformed one whose value is returned without counting as clean toward a reset.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Reply<T> {
    Done(T),
    Empty,
    Rejected(T),
}

/// Why a paced call failed.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Paced<E> {
    /// Every one of `EMPTY_LIMIT` attempts came back empty.
    Throttled,
    /// An attempt failed; it is returned at once, without a retry.
    Failed(E),
}

/// Running counts of slots given out by priority, empty replies and throttled calls.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct PacerStats {
    pub(crate) interactive: u64,
    pub(crate) background: u64,
    pub(crate) empty: u64,
    pub(crate) throttled: u64,
}

/// One priority's spacing: the gap it keeps after any start and how its own replies widened it.
#[derive(Debug)]
struct Lane {
    gap: Duration,
    /// The earliest this lane's next call may start; None before the first call.
    next: Option<Instant>,
    /// Clean replies in a row while this lane is backed off.
    clean: u32,
}

impl Lane {
    fn new(base: Duration) -> Self {
        Self {
            gap: base,
            next: None,
            clean: 0,
        }
    }
}

#[derive(Debug)]
struct State {
    base: Duration,
    /// Interactive calls keep their own gap, so a sync's empty replies never slow a page.
    interactive: Lane,
    background: Lane,
    /// When the latest call of either priority started.
    last: Option<Instant>,
    /// Answers in a row since the last 429, while a gap is widened.
    answers: u32,
    interactive_waiting: usize,
    stats: PacerStats,
    /// Every start with the gap it reserved after itself, so tests check spacing exactly.
    #[cfg(test)]
    starts: Vec<(Instant, Duration)>,
}

impl State {
    fn lane(&mut self, priority: Priority) -> &mut Lane {
        match priority {
            Priority::Interactive => &mut self.interactive,
            Priority::Background => &mut self.background,
        }
    }
}

#[derive(Debug)]
struct Shared {
    state: Mutex<State>,
    /// Fired whenever an interactive caller stops waiting (it started, or it was dropped).
    interactive_left: Notify,
}

#[derive(Debug, Clone)]
pub(crate) struct Pacer {
    shared: Arc<Shared>,
}

impl Pacer {
    pub(crate) fn new(base: Duration) -> Self {
        let state = State {
            base,
            interactive: Lane::new(base),
            background: Lane::new(base),
            last: None,
            answers: 0,
            interactive_waiting: 0,
            stats: PacerStats::default(),
            #[cfg(test)]
            starts: Vec::new(),
        };
        Self {
            shared: Arc::new(Shared {
                state: Mutex::new(state),
                interactive_left: Notify::new(),
            }),
        }
    }

    /// The background gap, the one sync work keeps between its calls.
    pub(crate) fn gap(&self) -> Duration {
        self.lock().background.gap
    }

    /// The gap a call of `priority` waits after the latest start.
    #[cfg(test)]
    pub(crate) fn gap_of(&self, priority: Priority) -> Duration {
        self.lock().lane(priority).gap
    }

    #[cfg_attr(
        not(test),
        expect(dead_code, reason = "read by the tests and the live probe")
    )]
    pub(crate) fn stats(&self) -> PacerStats {
        self.lock().stats
    }

    /// Every start given out so far, in order, each with the gap it reserved after itself.
    #[cfg(test)]
    pub(crate) fn starts(&self) -> Vec<(Instant, Duration)> {
        self.lock().starts.clone()
    }

    pub(crate) async fn call<T, E, F, Fut>(
        &self,
        priority: Priority,
        mut attempt: F,
    ) -> Result<T, Paced<E>>
    where
        F: FnMut() -> Fut,
        Fut: Future<Output = Result<Reply<T>, E>>,
    {
        let mut empties = 0;
        loop {
            self.slot(priority).await;
            match attempt().await {
                Ok(Reply::Done(value)) => {
                    self.clean();
                    return Ok(value);
                }
                Ok(Reply::Rejected(value)) => return Ok(value),
                Ok(Reply::Empty) => {
                    empties += 1;
                    let throttled = empties >= EMPTY_LIMIT;
                    self.empty(priority, throttled);
                    if throttled {
                        return Err(Paced::Throttled);
                    }
                }
                Err(error) => return Err(Paced::Failed(error)),
            }
        }
    }

    fn lock(&self) -> MutexGuard<'_, State> {
        self.shared
            .state
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }

    /// Sets the base gap and both lanes' gaps to `base` from the next start on, clearing any
    /// widening and the runs easing it; a start already promised keeps its time, so a
    /// shorter gap never pulls it forward.
    pub(crate) fn set_base(&self, base: Duration) {
        let mut guard = self.lock();
        let s = &mut *guard;
        s.base = base;
        for lane in [&mut s.interactive, &mut s.background] {
            lane.gap = base;
            lane.clean = 0;
        }
        s.answers = 0;
    }

    /// Waits for this caller's start; a background caller also waits while any interactive one does.
    pub(crate) async fn slot(&self, priority: Priority) {
        let _ticket = Ticket::new(self, priority);
        let asked = Instant::now();
        loop {
            // Made before the state is read, so a notify between the read and the await is kept.
            let notified = pin!(self.shared.interactive_left.notified());
            let wait = {
                let mut guard = self.lock();
                let s = &mut *guard;
                let now = Instant::now();
                if priority == Priority::Background && s.interactive_waiting > 0 {
                    None
                } else if s.lane(priority).next.is_none_or(|next| next <= now) {
                    // Each lane keeps its own gap after every start, whichever lane took it.
                    for lane in [&mut s.interactive, &mut s.background] {
                        let after = now + lane.gap;
                        lane.next = Some(lane.next.map_or(after, |next| next.max(after)));
                    }
                    s.last = Some(now);
                    #[cfg(test)]
                    {
                        let gap = s.lane(priority).gap;
                        s.starts.push((now, gap));
                    }
                    match priority {
                        Priority::Interactive => s.stats.interactive += 1,
                        Priority::Background => s.stats.background += 1,
                    }
                    drop(guard);
                    let waited = now - asked;
                    tracing::debug!(?priority, ?waited, "pacer slot");
                    return;
                } else {
                    s.lane(priority).next
                }
            };
            match wait {
                Some(at) => sleep_until(at).await,
                None => notified.await,
            }
        }
    }

    /// A 429 came back: doubles both lanes' gaps up to `MAX_GAP`, pushes their next starts
    /// back to match and restarts the run of answers that eases them.
    pub(crate) fn rate_limited(&self) {
        let mut s = self.lock();
        s.answers = 0;
        for priority in [Priority::Interactive, Priority::Background] {
            let gap = widen(&mut s, priority);
            tracing::debug!(?priority, ?gap, "pacer widened after a 429");
        }
    }

    /// A request was answered without a 429: every `EASE_AFTER` in a row halve each widened
    /// gap, never below the base.
    pub(crate) fn answered(&self) {
        let mut guard = self.lock();
        let s = &mut *guard;
        let base = s.base;
        if s.interactive.gap > base || s.background.gap > base {
            s.answers += 1;
            if s.answers >= EASE_AFTER {
                for lane in [&mut s.interactive, &mut s.background] {
                    lane.gap = (lane.gap / 2).max(base);
                }
                s.answers = 0;
            }
        }
    }

    /// An empty reply widens only the lane of the call that got it.
    fn empty(&self, priority: Priority, throttled: bool) {
        let mut s = self.lock();
        s.stats.empty += 1;
        if throttled {
            s.stats.throttled += 1;
        }
        s.lane(priority).clean = 0;
        let gap = widen(&mut s, priority);
        tracing::debug!(
            ?priority,
            ?gap,
            throttled,
            "pacer widened after an empty reply"
        );
    }

    /// A clean reply counts toward resetting every widened lane, since the server is answering.
    fn clean(&self) {
        let mut guard = self.lock();
        let s = &mut *guard;
        let base = s.base;
        for (priority, lane) in [
            (Priority::Interactive, &mut s.interactive),
            (Priority::Background, &mut s.background),
        ] {
            if lane.gap > base {
                lane.clean += 1;
                if lane.clean >= CLEAN_TO_RESET {
                    lane.gap = base;
                    lane.clean = 0;
                    tracing::debug!(?priority, gap = ?base, "pacer gap back to base");
                }
            }
        }
    }
}

/// Doubles the lane's gap up to `MAX_GAP`, moves its next start to one new gap after the
/// last start and returns the new gap.
fn widen(s: &mut State, priority: Priority) -> Duration {
    let last = s.last;
    let lane = s.lane(priority);
    lane.gap = lane.gap.saturating_mul(2).min(MAX_GAP);
    // Never pull a pending start forward: a reset gap must not shorten a longer wait.
    let gap = lane.gap;
    lane.next = last.map(|last| lane.next.map_or(last + gap, |next| next.max(last + gap)));
    gap
}

/// Counts an interactive caller as waiting until its slot starts or its future is dropped.
struct Ticket<'a> {
    pacer: &'a Pacer,
    interactive: bool,
}

impl<'a> Ticket<'a> {
    fn new(pacer: &'a Pacer, priority: Priority) -> Self {
        let interactive = priority == Priority::Interactive;
        if interactive {
            pacer.lock().interactive_waiting += 1;
        }
        Self { pacer, interactive }
    }
}

impl Drop for Ticket<'_> {
    fn drop(&mut self) {
        if self.interactive {
            let mut s = self.pacer.lock();
            s.interactive_waiting = s.interactive_waiting.saturating_sub(1);
            drop(s);
            self.pacer.shared.interactive_left.notify_waiters();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use Priority::{Background, Interactive};
    use std::sync::atomic::{AtomicUsize, Ordering};

    const BASE: Duration = Duration::from_millis(200);
    type Log = Mutex<Vec<(Duration, Priority)>>;

    fn ms(n: u64) -> Duration {
        Duration::from_millis(n)
    }

    /// One paced call whose attempts log their start and reply Empty while `script` says true.
    async fn scripted(
        pacer: &Pacer,
        priority: Priority,
        script: &[bool],
        log: &Log,
        t0: Instant,
    ) -> Result<(), Paced<()>> {
        let tries = AtomicUsize::new(0);
        let tries = &tries;
        pacer
            .call(priority, move || async move {
                log.lock().unwrap().push((t0.elapsed(), priority));
                let n = tries.fetch_add(1, Ordering::SeqCst);
                Ok::<_, ()>(if script.get(n).copied().unwrap_or(false) {
                    Reply::Empty
                } else {
                    Reply::Done(())
                })
            })
            .await
    }

    fn starts(log: &Log) -> Vec<Duration> {
        log.lock().unwrap().iter().map(|(at, _)| *at).collect()
    }

    #[tokio::test(start_paused = true)]
    async fn calls_start_one_gap_apart_and_the_first_does_not_wait() {
        let (pacer, log, t0) = (Pacer::new(BASE), Log::default(), Instant::now());
        for _ in 0..3 {
            scripted(&pacer, Background, &[], &log, t0).await.unwrap();
        }
        assert_eq!(starts(&log), [ms(0), ms(200), ms(400)]);
    }

    #[tokio::test(start_paused = true)]
    async fn the_gap_runs_start_to_start_so_a_slow_call_adds_no_wait() {
        let (pacer, log, t0) = (Pacer::new(BASE), Log::default(), Instant::now());
        pacer
            .call(Background, || async {
                tokio::time::sleep(ms(500)).await;
                Ok::<_, ()>(Reply::Done(()))
            })
            .await
            .unwrap();
        scripted(&pacer, Background, &[], &log, t0).await.unwrap();
        assert_eq!(starts(&log), [ms(500)]);
    }

    #[tokio::test(start_paused = true)]
    async fn concurrent_calls_and_clones_share_one_gap() {
        let (pacer, log, t0) = (Pacer::new(BASE), Log::default(), Instant::now());
        let clone = pacer.clone();
        let (a, b, c) = tokio::join!(
            scripted(&pacer, Background, &[], &log, t0),
            scripted(&clone, Background, &[], &log, t0),
            scripted(&pacer, Background, &[], &log, t0),
        );
        assert!(a.is_ok() && b.is_ok() && c.is_ok(), "{a:?} {b:?} {c:?}");
        let mut at = starts(&log);
        at.sort();
        assert_eq!(at, [ms(0), ms(200), ms(400)]);
    }

    #[tokio::test(start_paused = true)]
    async fn an_interactive_call_goes_ahead_of_a_waiting_background_call() {
        let (pacer, log, t0) = (Pacer::new(BASE), Log::default(), Instant::now());
        scripted(&pacer, Background, &[], &log, t0).await.unwrap();
        let (bg, fg) = tokio::join!(
            scripted(&pacer, Background, &[], &log, t0),
            scripted(&pacer, Interactive, &[], &log, t0),
        );
        assert!(bg.is_ok() && fg.is_ok(), "{bg:?} {fg:?}");
        assert_eq!(
            *log.lock().unwrap(),
            [
                (ms(0), Background),
                (ms(200), Interactive),
                (ms(400), Background)
            ]
        );
    }

    #[tokio::test(start_paused = true)]
    async fn a_cancelled_interactive_call_does_not_hold_back_background_calls() {
        let (pacer, log, t0) = (Pacer::new(BASE), Log::default(), Instant::now());
        scripted(&pacer, Background, &[], &log, t0).await.unwrap();
        let cancelled =
            tokio::time::timeout(ms(50), scripted(&pacer, Interactive, &[], &log, t0)).await;
        assert!(
            cancelled.is_err(),
            "the interactive call should still have been waiting"
        );
        let later = tokio::time::timeout(
            Duration::from_secs(60),
            scripted(&pacer, Background, &[], &log, t0),
        )
        .await;
        assert!(matches!(later, Ok(Ok(()))), "{later:?}");
        assert_eq!(starts(&log), [ms(0), ms(200)]);
    }

    #[tokio::test(start_paused = true)]
    async fn an_empty_reply_doubles_the_gap_and_retries() {
        let (pacer, log, t0) = (Pacer::new(BASE), Log::default(), Instant::now());
        scripted(&pacer, Background, &[true], &log, t0)
            .await
            .unwrap();
        assert_eq!(starts(&log), [ms(0), ms(400)]);
        assert_eq!(pacer.gap(), ms(400));
        assert_eq!(pacer.stats().empty, 1);
    }

    #[tokio::test(start_paused = true)]
    async fn three_empty_replies_in_a_row_fail_as_throttled() {
        let (pacer, log, t0) = (Pacer::new(BASE), Log::default(), Instant::now());
        let result = scripted(&pacer, Background, &[true, true, true], &log, t0).await;
        assert_eq!(result, Err(Paced::Throttled));
        assert_eq!(starts(&log), [ms(0), ms(400), ms(1200)]);
        assert_eq!(pacer.gap(), ms(1600));
        let stats = pacer.stats();
        assert_eq!((stats.empty, stats.throttled), (3, 1));
    }

    #[tokio::test(start_paused = true)]
    async fn empty_replies_only_count_toward_throttled_within_one_call() {
        let (pacer, log, t0) = (Pacer::new(BASE), Log::default(), Instant::now());
        assert!(
            scripted(&pacer, Background, &[true, true], &log, t0)
                .await
                .is_ok()
        );
        assert!(
            scripted(&pacer, Interactive, &[true, true], &log, t0)
                .await
                .is_ok()
        );
    }

    #[tokio::test(start_paused = true)]
    async fn the_gap_never_passes_eight_seconds() {
        let (pacer, log, t0) = (Pacer::new(BASE), Log::default(), Instant::now());
        for _ in 0..3 {
            let result = scripted(&pacer, Background, &[true, true, true], &log, t0).await;
            assert_eq!(result, Err(Paced::Throttled));
        }
        assert_eq!(MAX_GAP, Duration::from_secs(8));
        assert_eq!(pacer.gap(), MAX_GAP);
        let at = starts(&log);
        assert_eq!(at[8] - at[7], MAX_GAP);
    }

    #[tokio::test(start_paused = true)]
    async fn an_interactive_call_never_waits_a_gap_background_replies_widened() {
        let (pacer, log, t0) = (Pacer::new(BASE), Log::default(), Instant::now());
        for _ in 0..3 {
            let result = scripted(&pacer, Background, &[true, true, true], &log, t0).await;
            assert_eq!(result, Err(Paced::Throttled));
        }
        assert_eq!(pacer.gap(), MAX_GAP);
        assert_eq!(pacer.gap_of(Interactive), BASE);
        scripted(&pacer, Interactive, &[], &log, t0).await.unwrap();
        let at = starts(&log);
        assert_eq!(at[9] - at[8], BASE, "the page waits only the base spacing");
    }

    #[tokio::test(start_paused = true)]
    async fn ten_clean_replies_return_the_gap_to_its_base() {
        let (pacer, log, t0) = (Pacer::new(BASE), Log::default(), Instant::now());
        scripted(&pacer, Background, &[true], &log, t0)
            .await
            .unwrap();
        for _ in 0..8 {
            scripted(&pacer, Background, &[], &log, t0).await.unwrap();
        }
        assert_eq!(pacer.gap(), ms(400), "nine clean replies keep the backoff");
        scripted(&pacer, Background, &[], &log, t0).await.unwrap();
        assert_eq!(pacer.gap(), BASE);
    }

    #[tokio::test(start_paused = true)]
    async fn a_rejected_reply_is_returned_but_does_not_count_as_clean() {
        let (pacer, log, t0) = (Pacer::new(BASE), Log::default(), Instant::now());
        scripted(&pacer, Background, &[true], &log, t0)
            .await
            .unwrap();
        for _ in 0..8 {
            scripted(&pacer, Background, &[], &log, t0).await.unwrap();
        }
        let rejected = pacer
            .call(Background, || async { Ok::<_, ()>(Reply::Rejected(7)) })
            .await;
        assert_eq!(rejected, Ok(7));
        assert_eq!(pacer.gap(), ms(400), "a rejected reply keeps the backoff");
        scripted(&pacer, Background, &[], &log, t0).await.unwrap();
        assert_eq!(pacer.gap(), BASE);
    }

    #[tokio::test(start_paused = true)]
    async fn an_empty_reply_after_a_reset_keeps_a_longer_pending_wait() {
        // Back off to 800 ms with nine clean replies counted, so the next clean one resets.
        let (pacer, log, t0) = (Pacer::new(BASE), Log::default(), Instant::now());
        scripted(&pacer, Background, &[true, true], &log, t0)
            .await
            .unwrap();
        for _ in 0..8 {
            scripted(&pacer, Background, &[], &log, t0).await.unwrap();
        }
        assert_eq!(pacer.gap(), ms(800));
        // A resets the gap mid-flight; B's empty reply lands after that, while its own
        // 800 ms wait is still pending, and must not pull its retry forward.
        let b_starts = Mutex::new(Vec::new());
        let b_tries = AtomicUsize::new(0);
        let a = pacer.call(Background, || async {
            tokio::time::sleep(ms(1000)).await;
            Ok::<_, ()>(Reply::Done(()))
        });
        let b = async {
            // Queued just after A, so A takes the first slot and B the one after it.
            tokio::time::sleep(ms(1)).await;
            pacer
                .call(Background, || async {
                    b_starts.lock().unwrap().push(Instant::now());
                    if b_tries.fetch_add(1, Ordering::SeqCst) == 0 {
                        tokio::time::sleep(ms(300)).await;
                        Ok::<_, ()>(Reply::Empty)
                    } else {
                        Ok(Reply::Done(()))
                    }
                })
                .await
        };
        let (a, b) = tokio::join!(a, b);
        assert!(a.is_ok() && b.is_ok(), "{a:?} {b:?}");
        let at = b_starts.lock().unwrap().clone();
        assert_eq!(at[1] - at[0], ms(800));
    }

    #[tokio::test(start_paused = true)]
    async fn a_failed_attempt_is_returned_without_a_retry_or_a_gap_change() {
        let pacer = Pacer::new(BASE);
        let tries = AtomicUsize::new(0);
        let tries_ref = &tries;
        let result: Result<(), Paced<&str>> = pacer
            .call(Interactive, move || async move {
                tries_ref.fetch_add(1, Ordering::SeqCst);
                Err::<Reply<()>, _>("boom")
            })
            .await;
        assert_eq!(result, Err(Paced::Failed("boom")));
        assert_eq!(tries.load(Ordering::SeqCst), 1);
        assert_eq!(pacer.gap(), BASE);
    }

    #[tokio::test(start_paused = true)]
    async fn stats_count_slots_by_priority() {
        let (pacer, log, t0) = (Pacer::new(BASE), Log::default(), Instant::now());
        scripted(&pacer, Interactive, &[true], &log, t0)
            .await
            .unwrap();
        scripted(&pacer, Background, &[], &log, t0).await.unwrap();
        assert_eq!(
            pacer.stats(),
            PacerStats {
                interactive: 2,
                background: 1,
                empty: 1,
                throttled: 0
            }
        );
    }

    #[tokio::test(start_paused = true)]
    async fn a_new_base_spaces_the_next_starts_and_keeps_the_pending_one() {
        let (pacer, log, t0) = (Pacer::new(BASE), Log::default(), Instant::now());
        scripted(&pacer, Background, &[], &log, t0).await.unwrap();
        pacer.set_base(ms(500));
        for _ in 0..2 {
            scripted(&pacer, Background, &[], &log, t0).await.unwrap();
        }
        assert_eq!(pacer.gap(), ms(500));
        pacer.set_base(BASE);
        scripted(&pacer, Background, &[], &log, t0).await.unwrap();
        assert_eq!(starts(&log), [ms(0), ms(200), ms(700), ms(1200)]);
    }

    #[tokio::test(start_paused = true)]
    async fn a_zero_base_never_waits() {
        let (pacer, log, t0) = (Pacer::new(Duration::ZERO), Log::default(), Instant::now());
        let result = scripted(&pacer, Background, &[true, true, true], &log, t0).await;
        assert_eq!(result, Err(Paced::Throttled));
        scripted(&pacer, Background, &[], &log, t0).await.unwrap();
        assert_eq!(starts(&log), [ms(0); 4]);
    }

    #[tokio::test(start_paused = true)]
    async fn a_rate_limit_doubles_the_gap_up_to_eight_seconds() {
        let pacer = Pacer::new(Duration::from_secs(1));
        pacer.rate_limited();
        assert_eq!(pacer.gap(), Duration::from_secs(2));
        for _ in 0..3 {
            pacer.rate_limited();
        }
        assert_eq!(pacer.gap(), MAX_GAP);
    }

    #[tokio::test(start_paused = true)]
    async fn a_rate_limit_widens_the_interactive_lane_too() {
        let (pacer, log, t0) = (Pacer::new(BASE), Log::default(), Instant::now());
        scripted(&pacer, Background, &[], &log, t0).await.unwrap();
        pacer.rate_limited();
        assert_eq!(pacer.gap_of(Interactive), ms(400));
        scripted(&pacer, Interactive, &[], &log, t0).await.unwrap();
        assert_eq!(starts(&log), [ms(0), ms(400)]);
    }

    #[tokio::test(start_paused = true)]
    async fn a_rate_limit_pushes_the_next_start_back_by_the_wider_gap() {
        let (pacer, log, t0) = (Pacer::new(BASE), Log::default(), Instant::now());
        scripted(&pacer, Background, &[], &log, t0).await.unwrap();
        pacer.rate_limited();
        scripted(&pacer, Background, &[], &log, t0).await.unwrap();
        assert_eq!(starts(&log), [ms(0), ms(400)]);
    }

    #[tokio::test(start_paused = true)]
    async fn twenty_answers_in_a_row_halve_a_widened_gap_down_to_its_base() {
        let pacer = Pacer::new(BASE);
        pacer.rate_limited();
        pacer.rate_limited();
        for _ in 0..EASE_AFTER - 1 {
            pacer.answered();
        }
        assert_eq!(
            pacer.gap(),
            ms(800),
            "nineteen answers keep the widened gap"
        );
        pacer.answered();
        assert_eq!(pacer.gap(), ms(400));
        for _ in 0..2 * EASE_AFTER {
            pacer.answered();
        }
        assert_eq!(pacer.gap(), BASE, "easing never drops below the base");
    }

    #[tokio::test(start_paused = true)]
    async fn a_rate_limit_restarts_the_run_of_answers() {
        let pacer = Pacer::new(BASE);
        pacer.rate_limited();
        for _ in 0..EASE_AFTER - 1 {
            pacer.answered();
        }
        pacer.rate_limited();
        for _ in 0..EASE_AFTER - 1 {
            pacer.answered();
        }
        assert_eq!(pacer.gap(), ms(800));
    }
}
