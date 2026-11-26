//! One read of a demo that hands over its first picture, progress and playback tracks.

use std::ops::ControlFlow;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use super::snapshot::{
    Clocking, DemoSnapshot, SnapshotPicker, Step, TickFrame, drive, into_snapshot, read_prefix,
};
use super::tracks::{
    DemoTracks, MAX_DEMO_BYTES, MAX_TRACK_SAMPLES, TrackBuilder, seconds_per_tick,
};
use crate::error::{DemoSnapshotError, DemoTracksError};
use crate::panic_mark::catching;

/// What one playback read sends, in order: one `Snapshot`, any `Progress`, then one of
/// `Tracks` or `TracksFailed`.
#[derive(Debug)]
#[non_exhaustive]
pub enum PlaybackEvent {
    /// The still picture, sent as soon as the pick settles.
    Snapshot(Result<DemoSnapshot, DemoSnapshotError>),
    /// The share of the header's ticks read so far, in 0..=1, at most once per whole percent.
    Progress(f32),
    /// Every player's tracks over the read part of the demo.
    Tracks(Arc<DemoTracks>),
    /// Why the tracks could not be built.
    TracksFailed(DemoTracksError),
}

/// Feeds each frame of one parse to the picker until it settles and to the builder to the end,
/// sending events as they become known.
pub(crate) struct PlaybackRun<'a, E: FnMut(PlaybackEvent) -> ControlFlow<()>> {
    cancel: &'a AtomicBool,
    builder: TrackBuilder,
    built: bool,
    picker: Option<SnapshotPicker>,
    percent: u64,
    stopped: bool,
    emit: E,
}

impl<'a, E: FnMut(PlaybackEvent) -> ControlFlow<()>> PlaybackRun<'a, E> {
    /// A run that keeps at most `sample_cap` samples and sends through `emit`.
    pub fn new(cancel: &'a AtomicBool, sample_cap: usize, emit: E) -> Self {
        Self {
            cancel,
            builder: TrackBuilder::new(sample_cap),
            built: false,
            picker: Some(SnapshotPicker::new()),
            percent: 0,
            stopped: false,
            emit,
        }
    }

    // Sends one event unless a cancel or an earlier Break has halted the run, and remembers a
    // Break, so nothing goes out after either even partway through a frame.
    fn send(&mut self, event: PlaybackEvent) {
        if !self.halted() && (self.emit)(event).is_break() {
            self.stopped = true;
        }
    }

    fn halted(&self) -> bool {
        self.stopped || self.cancel.load(Ordering::Relaxed)
    }

    /// Takes one frame; Break ends the parse on a cancel, a Break from `emit` or the sample cap.
    pub fn frame(&mut self, frame: TickFrame, clocking: &Clocking) -> ControlFlow<()> {
        if self.halted() || self.built {
            return ControlFlow::Break(());
        }
        let tick = frame.tick;
        self.built = matches!(
            self.builder.push(&frame, clocking.seconds_per_tick()),
            Step::Done
        );
        if let Some(picker) = self.picker.as_mut()
            && matches!(picker.push(frame), Step::Done)
            && let Some(picker) = self.picker.take()
        {
            self.send(PlaybackEvent::Snapshot(snapshot(picker)));
        }
        // Progress follows the picture, in whole percents only, so the UI sees at most a hundred
        // updates; a tick past the header's count (the opening server-tick frame) says nothing.
        let total = u64::from(clocking.header_ticks);
        if self.picker.is_none() && total > 0 && tick <= clocking.header_ticks {
            let percent = u64::from(tick) * 100 / total;
            if percent > self.percent {
                self.percent = percent;
                self.send(PlaybackEvent::Progress(tick as f32 / total as f32));
            }
        }
        if self.halted() || self.built {
            ControlFlow::Break(())
        } else {
            ControlFlow::Continue(())
        }
    }

    /// Sends what the parse still owes: the snapshot if the pick never settled, then the tracks.
    pub fn end(mut self, parsed: Result<Clocking, String>, file_capped: bool) {
        if self.halted() {
            return;
        }
        match parsed {
            Err(text) => {
                if self.picker.take().is_some() {
                    self.send(PlaybackEvent::Snapshot(Err(DemoSnapshotError::Unreadable(
                        text.clone(),
                    ))));
                }
                self.send(PlaybackEvent::TracksFailed(DemoTracksError::Unreadable(
                    text,
                )));
            }
            Ok(clocking) => {
                if let Some(picker) = self.picker.take() {
                    self.send(PlaybackEvent::Snapshot(snapshot(picker)));
                }
                let tracks = std::mem::replace(&mut self.builder, TrackBuilder::new(0)).finish(
                    clocking.seconds_per_tick(),
                    clocking.header_ticks,
                    file_capped,
                );
                self.send(match tracks {
                    Some(tracks) => PlaybackEvent::Tracks(Arc::new(tracks)),
                    None => PlaybackEvent::TracksFailed(DemoTracksError::NoPlayers),
                });
            }
        }
    }
}

impl Clocking {
    /// The tick length the tracks and the clock use.
    pub fn seconds_per_tick(&self) -> f32 {
        seconds_per_tick(
            self.server_interval,
            self.header_duration,
            self.header_ticks,
        )
    }
}

fn snapshot(picker: SnapshotPicker) -> Result<DemoSnapshot, DemoSnapshotError> {
    picker
        .finish()
        .map(|(tick, rows)| into_snapshot(tick, rows))
        .ok_or(DemoSnapshotError::NoPlayers)
}

/// Reads at most MAX_DEMO_BYTES of `dem` once, sending its snapshot, progress and tracks to
/// `emit`. A cancel (checked before and after the read and at every frame) or a Break from
/// `emit` stops it with nothing more sent.
pub fn read_demo_playback(
    dem: &Path,
    cancel: &AtomicBool,
    mut emit: impl FnMut(PlaybackEvent) -> ControlFlow<()>,
) {
    if cancel.load(Ordering::Relaxed) {
        return;
    }
    let read = std::fs::metadata(dem).and_then(|meta| {
        let bytes = read_prefix(dem, MAX_DEMO_BYTES)?;
        Ok((bytes, meta.len() > MAX_DEMO_BYTES))
    });
    let (bytes, file_capped) = match read {
        Ok(read) => read,
        Err(error) => {
            if !cancel.load(Ordering::Relaxed) {
                let text = error.to_string();
                let snapshot = Err(DemoSnapshotError::Unreadable(text.clone()));
                if emit(PlaybackEvent::Snapshot(snapshot)).is_continue() {
                    let _ = emit(PlaybackEvent::TracksFailed(DemoTracksError::Unreadable(
                        text,
                    )));
                }
            }
            return;
        }
    };
    if cancel.load(Ordering::Relaxed) {
        return;
    }
    playback_from_bytes(bytes, file_capped, cancel, || {}, emit);
}

/// Reads at most MAX_DEMO_BYTES of `dem` and builds every player's tracks over it, through the
/// playback read. `cancel` is checked before and after the read and at every frame.
pub fn read_demo_tracks(dem: &Path, cancel: &AtomicBool) -> Result<DemoTracks, DemoTracksError> {
    let mut outcome = Err(DemoTracksError::Cancelled);
    read_demo_playback(dem, cancel, |event| {
        match event {
            PlaybackEvent::Tracks(tracks) => {
                outcome = Ok(Arc::try_unwrap(tracks).unwrap_or_else(|shared| (*shared).clone()))
            }
            PlaybackEvent::TracksFailed(error) => outcome = Err(error),
            PlaybackEvent::Snapshot(_) | PlaybackEvent::Progress(_) => {}
        }
        ControlFlow::Continue(())
    });
    outcome
}

// Runs the parse under the panic guard with the run borrowed by the frame callback, so a
// panic still ends the run with an error; `before_parse` lets a test force that panic.
fn playback_from_bytes(
    bytes: impl AsRef<[u8]>,
    file_capped: bool,
    cancel: &AtomicBool,
    before_parse: impl FnOnce(),
    emit: impl FnMut(PlaybackEvent) -> ControlFlow<()>,
) {
    let mut run = PlaybackRun::new(cancel, MAX_TRACK_SAMPLES, emit);
    let parsed = catching(|| {
        before_parse();
        drive(bytes.as_ref(), |frame, clocking| run.frame(frame, clocking))
    })
    .unwrap_or_else(|_| Err("the demo parser panicked".into()));
    drop(bytes);
    run.end(parsed, file_capped);
}

#[cfg(test)]
#[allow(clippy::wildcard_enum_match_arm)]
mod tests {
    use super::*;
    use crate::demo_view::snapshot::{Clocking, TickFrame, TickRow};
    use crate::demo_view::tracks::{MAX_TRACK_SAMPLES, TrackCut};
    use crate::hud_maker::Team;
    use crate::weapons::TfClass;
    use std::sync::atomic::Ordering;

    fn row(entity: u32, team: Team) -> TickRow {
        TickRow {
            entity,
            team: Some(team),
            class: Some(TfClass::Soldier),
            feet: [100. + entity as f32, 200., 64.],
            yaw: 90.,
            pitch: 0.,
            view_offset_z: Some(68.),
            ducked: false,
            has_userinfo: true,
            deleted: false,
            connected: true,
            life_alive: true,
            resource_alive: true,
            in_pvs: true,
        }
    }

    fn clocking(header_ticks: u32) -> Clocking {
        Clocking {
            header_ticks,
            header_duration: header_ticks as f32 / 66.,
            server_interval: 1. / 66.,
        }
    }

    // PREROUND until tick 50, RUNNING after: the picker settles at tick 2049 on tick 1050.
    fn settling(tick: u32) -> TickFrame {
        let round_state = if tick < 50 { 3 } else { 4 };
        TickFrame {
            tick,
            round_state,
            rows: vec![row(1, Team::Red), row(101, Team::Blue)],
        }
    }

    // Feeds `frames`, ends with `parsed`, and returns what was emitted plus every frame step.
    fn run(
        frames: impl IntoIterator<Item = TickFrame>,
        parsed: Result<Clocking, String>,
        cap: usize,
        header_ticks: u32,
    ) -> (Vec<PlaybackEvent>, Vec<ControlFlow<()>>) {
        let cancel = AtomicBool::new(false);
        let mut events = Vec::new();
        let mut steps = Vec::new();
        {
            let mut playback = PlaybackRun::new(&cancel, cap, |e| {
                events.push(e);
                ControlFlow::Continue(())
            });
            for f in frames {
                steps.push(playback.frame(f, &clocking(header_ticks)));
            }
            playback.end(parsed, false);
        }
        (events, steps)
    }

    fn kinds(events: &[PlaybackEvent]) -> String {
        events
            .iter()
            .map(|e| match e {
                PlaybackEvent::Snapshot(_) => 'S',
                PlaybackEvent::Progress(_) => 'P',
                PlaybackEvent::Tracks(_) => 'T',
                PlaybackEvent::TracksFailed(_) => 'F',
            })
            .collect()
    }

    #[test]
    fn the_snapshot_comes_once_as_soon_as_the_pick_settles_then_the_tracks() {
        let (events, _) = run(
            (0..=2100).map(settling),
            Ok(clocking(2100)),
            MAX_TRACK_SAMPLES,
            2100,
        );
        let k = kinds(&events);
        assert_eq!(k.matches('S').count(), 1, "{k}");
        assert!(k.ends_with('T'), "{k}");
        assert_eq!(k.matches('T').count(), 1, "{k}");
        // Progress waits for the picture; the pick settles at tick 2049 of 2100, so the
        // first Progress is 97 percent, then 98, 99 and 100.
        assert_eq!(k.find('S'), Some(0), "{k}");
        match &events[0] {
            PlaybackEvent::Snapshot(Ok(snap)) => assert_eq!(snap.tick(), 1050),
            other => panic!("{other:?}"),
        }
        let progress: Vec<f32> = events
            .iter()
            .filter_map(|e| match e {
                PlaybackEvent::Progress(p) => Some(*p),
                _ => None,
            })
            .collect();
        assert_eq!(progress.len(), 4, "{k}");
        assert_eq!(
            progress
                .iter()
                .map(|p| (p * 2100.).round() as u32 * 100 / 2100)
                .collect::<Vec<_>>(),
            [97, 98, 99, 100],
            "{progress:?}"
        );
        assert!(progress.windows(2).all(|w| w[1] > w[0]), "{progress:?}");
        assert!(
            progress.iter().all(|p| (0. ..=1.).contains(p)),
            "{progress:?}"
        );
        match events.last() {
            Some(PlaybackEvent::Tracks(t)) => {
                assert_eq!((t.first_tick(), t.last_tick()), (0, 2100))
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn a_demo_too_short_to_settle_sends_its_snapshot_at_the_end() {
        let (events, _) = run(
            (0..100).map(settling),
            Ok(clocking(0)),
            MAX_TRACK_SAMPLES,
            0,
        );
        assert_eq!(kinds(&events), "ST");
        assert!(
            matches!(
                events[0],
                PlaybackEvent::Snapshot(Err(DemoSnapshotError::NoPlayers))
            ),
            "{events:?}"
        );
    }

    #[test]
    fn a_failed_parse_before_the_pick_sends_both_failures() {
        let (events, _) = run([], Err("bad".into()), MAX_TRACK_SAMPLES, 0);
        assert_eq!(kinds(&events), "SF");
        assert!(
            matches!(&events[0], PlaybackEvent::Snapshot(Err(DemoSnapshotError::Unreadable(t))) if t == "bad"),
            "{events:?}"
        );
        assert!(
            matches!(&events[1], PlaybackEvent::TracksFailed(DemoTracksError::Unreadable(t)) if t == "bad"),
            "{events:?}"
        );
    }

    #[test]
    fn a_failed_parse_after_the_pick_sends_only_the_tracks_failure() {
        let (events, _) = run(
            (0..=2100).map(settling),
            Err("bad".into()),
            MAX_TRACK_SAMPLES,
            0,
        );
        assert_eq!(kinds(&events), "SF");
        assert!(
            matches!(events[0], PlaybackEvent::Snapshot(Ok(_))),
            "{events:?}"
        );
    }

    #[test]
    fn no_drawable_player_fails_the_tracks_with_no_players() {
        let hidden = (0..100).map(|t| {
            let mut f = settling(t);
            f.rows.iter_mut().for_each(|r| r.in_pvs = false);
            f
        });
        let (events, _) = run(hidden, Ok(clocking(0)), MAX_TRACK_SAMPLES, 0);
        assert_eq!(kinds(&events), "SF");
        assert!(
            matches!(
                events[1],
                PlaybackEvent::TracksFailed(DemoTracksError::NoPlayers)
            ),
            "{events:?}"
        );
    }

    #[test]
    fn the_sample_cap_breaks_the_parse_and_still_hands_over_tracks() {
        let (events, steps) = run((0..10).map(settling), Ok(clocking(0)), 4, 0);
        assert_eq!(
            steps[..2],
            [ControlFlow::Continue(()), ControlFlow::Continue(())]
        );
        assert_eq!(steps[2], ControlFlow::Break(()));
        assert_eq!(kinds(&events), "ST");
        match events.last() {
            Some(PlaybackEvent::Tracks(t)) => {
                assert_eq!(t.cut(), Some(TrackCut::SampleCap { at_tick: 1 }))
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn a_break_from_emit_stops_everything() {
        let cancel = AtomicBool::new(false);
        let mut count = 0;
        let mut playback = PlaybackRun::new(&cancel, MAX_TRACK_SAMPLES, |_| {
            count += 1;
            ControlFlow::Break(())
        });
        let steps: Vec<_> = (0..=2100)
            .map(|t| playback.frame(settling(t), &clocking(2100)))
            .collect();
        playback.end(Ok(clocking(2100)), false);
        assert_eq!(count, 1);
        assert_eq!(steps.last(), Some(&ControlFlow::Break(())));
    }

    #[test]
    fn a_cancel_mid_parse_sends_nothing_more() {
        let cancel = AtomicBool::new(false);
        let mut events = Vec::new();
        {
            let mut playback = PlaybackRun::new(&cancel, MAX_TRACK_SAMPLES, |e| {
                events.push(e);
                ControlFlow::Continue(())
            });
            assert_eq!(
                playback.frame(settling(0), &clocking(0)),
                ControlFlow::Continue(())
            );
            cancel.store(true, Ordering::SeqCst);
            assert_eq!(
                playback.frame(settling(1), &clocking(0)),
                ControlFlow::Break(())
            );
            playback.end(Ok(clocking(0)), false);
        }
        assert!(events.is_empty(), "{events:?}");
    }

    #[test]
    fn a_cancel_set_while_sending_the_snapshot_lets_nothing_more_out() {
        let cancel = AtomicBool::new(false);
        let mut events = Vec::new();
        {
            let mut playback = PlaybackRun::new(&cancel, MAX_TRACK_SAMPLES, |e| {
                if matches!(e, PlaybackEvent::Snapshot(_)) {
                    cancel.store(true, Ordering::SeqCst);
                }
                events.push(e);
                ControlFlow::Continue(())
            });
            for t in 0..=2100 {
                if playback.frame(settling(t), &clocking(2100)).is_break() {
                    break;
                }
            }
            playback.end(Ok(clocking(2100)), false);
        }
        assert_eq!(kinds(&events), "S");
    }

    #[test]
    fn a_parser_panic_sends_both_failures_and_leaves_the_thread_unmarked() {
        let cancel = AtomicBool::new(false);
        let mut events = Vec::new();
        playback_from_bytes(
            b"HL2DEMO\0",
            false,
            &cancel,
            || panic!("forced"),
            |e| {
                events.push(e);
                ControlFlow::Continue(())
            },
        );
        assert_eq!(kinds(&events), "SF");
        assert!(
            matches!(&events[0], PlaybackEvent::Snapshot(Err(DemoSnapshotError::Unreadable(t))) if t == "the demo parser panicked"),
            "{events:?}"
        );
        assert!(!crate::panic_mark::is_marked());
    }
}
