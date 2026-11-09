//! The one pacer every GameBanana API call goes through: calls start at least a gap apart,
//! calls the player waits on go first, and empty replies back the gap off.

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

#[derive(Debug)]
struct State {
    base: Duration,
    gap: Duration,
    /// The earliest the next call may start; None before the first call.
    next: Option<Instant>,
    /// When the latest call started.
    last: Option<Instant>,
    clean: u32,
    interactive_waiting: usize,
    stats: PacerStats,
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
            gap: base,
            next: None,
            last: None,
            clean: 0,
            interactive_waiting: 0,
            stats: PacerStats::default(),
        };
        Self {
            shared: Arc::new(Shared {
                state: Mutex::new(state),
                interactive_left: Notify::new(),
            }),
        }
    }

    #[cfg_attr(
        not(test),
        expect(dead_code, reason = "read by the tests and the live probe")
    )]
    pub(crate) fn gap(&self) -> Duration {
        self.lock().gap
    }

    #[cfg_attr(
        not(test),
        expect(dead_code, reason = "read by the tests and the live probe")
    )]
    pub(crate) fn stats(&self) -> PacerStats {
        self.lock().stats
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
                    self.empty(throttled);
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

    /// Waits for this caller's start; a background caller also waits while any interactive one does.
    async fn slot(&self, priority: Priority) {
        let _ticket = Ticket::new(self, priority);
        loop {
            // Made before the state is read, so a notify between the read and the await is kept.
            let notified = pin!(self.shared.interactive_left.notified());
            let wait = {
                let mut s = self.lock();
                let now = Instant::now();
                if priority == Priority::Background && s.interactive_waiting > 0 {
                    None
                } else if s.next.is_none_or(|next| next <= now) {
                    let gap = s.gap;
                    s.next = Some(now + gap);
                    s.last = Some(now);
                    match priority {
                        Priority::Interactive => s.stats.interactive += 1,
                        Priority::Background => s.stats.background += 1,
                    }
                    return;
                } else {
                    s.next
                }
            };
            match wait {
                Some(at) => sleep_until(at).await,
                None => notified.await,
            }
        }
    }

    fn empty(&self, throttled: bool) {
        let mut s = self.lock();
        s.stats.empty += 1;
        if throttled {
            s.stats.throttled += 1;
        }
        s.clean = 0;
        s.gap = s.gap.saturating_mul(2).min(MAX_GAP);
        // Never pull a pending start forward: a reset gap must not shorten a longer wait.
        let gap = s.gap;
        s.next = s
            .last
            .map(|last| s.next.map_or(last + gap, |next| next.max(last + gap)));
    }

    fn clean(&self) {
        let mut s = self.lock();
        if s.gap > s.base {
            s.clean += 1;
            if s.clean >= CLEAN_TO_RESET {
                s.gap = s.base;
                s.clean = 0;
            }
        }
    }
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
    async fn a_zero_base_never_waits() {
        let (pacer, log, t0) = (Pacer::new(Duration::ZERO), Log::default(), Instant::now());
        let result = scripted(&pacer, Background, &[true, true, true], &log, t0).await;
        assert_eq!(result, Err(Paced::Throttled));
        scripted(&pacer, Background, &[], &log, t0).await.unwrap();
        assert_eq!(starts(&log), [ms(0); 4]);
    }
}
