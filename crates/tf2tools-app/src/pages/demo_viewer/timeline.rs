//! The Demo Viewer bar's playback row: the steps and play, the timeline with its round markers,
//! the time and the speed.

use std::sync::Arc;

use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::slider::{Slider, SliderEvent, SliderState};
use gpui_kit::component::{ActiveTheme as _, Disableable as _, Sizable as _, h_flex, v_flex};
use gpui_kit::{
    AnyElement, AppContext as _, Context, Entity, InteractiveElement as _, IntoElement,
    ParentElement as _, SharedString, StatefulInteractiveElement as _, Styled as _, Subscription,
    TestSupportExt as _, Window, canvas, div, fill, point, px, relative, size,
};
use tf2tools_core::{DemoTracks, KillParty, Team, TrackCut};

use super::DemoViewerPage;
use super::killfeed::{FeedRow, feed_key, feed_rows};
use crate::i18n;
use crate::i18n::tr;
use crate::ids;
use crate::viewer::load::{snapshot_time, timeline_time};
use crate::viewer::viewport::{
    CameraMoved, CameraState, DemoViewport, PlaybackMoved, PlaybackState,
};
use crate::widgets::tip::tip_box;
use crate::widgets::{Named as _, labelled};

/// The speed button's cycle; a speed outside it goes back to 1.
const SPEEDS: [f32; 5] = [1., 2., 4., 0.25, 0.5];

fn next_speed(speed: f32) -> f32 {
    SPEEDS
        .iter()
        .position(|&s| s == speed)
        .map_or(1., |i| SPEEDS[(i + 1) % SPEEDS.len()])
}

/// What a frame shows of playback: the whole second, the play state and the speed.
fn shown(state: PlaybackState) -> (u64, bool, f32) {
    let second = state.elapsed_seconds.max(0.).floor() as u64;
    (second, state.playing, state.speed)
}

/// `tick`'s share of the timeline from `first` to `end`, clamped to 0..=1; 0 when the span is empty.
pub fn timeline_fraction(tick: u32, first: u32, end: u32) -> f32 {
    if end <= first {
        return 0.;
    }
    (tick.saturating_sub(first) as f32 / (end - first) as f32).clamp(0., 1.)
}

/// The open demo's timeline once its tracks arrive.
pub(super) struct Timeline {
    pub(super) slider: Entity<SliderState>,
    first: u32,
    /// The slider's top end; one past `first` when the tracks hold a single tick.
    end: u32,
    rounds: Vec<u32>,
    /// Each kill's share of the timeline and its killer's team, painted under the round markers.
    kill_marks: Vec<(f32, Option<Team>)>,
    cut_note: Option<SharedString>,
    shown: Option<(u64, bool, f32)>,
    /// The camera state the bar last drew.
    camera: CameraState,
    tracks: Arc<DemoTracks>,
    /// The feed's rows at `feed_tick`, built in `feed_locale`, and the key they were built for.
    pub(super) feed: Vec<FeedRow>,
    feed_key: (usize, Option<u32>),
    feed_tick: f64,
    feed_locale: &'static str,
    _watch: [Subscription; 3],
}

impl Timeline {
    /// The note naming where playback stops early, if the tracks were cut.
    pub(super) fn cut_note(&self) -> Option<SharedString> {
        self.cut_note.clone()
    }
}

// Where a cut tracks' playback ends, in words.
fn cut_note(tracks: &DemoTracks) -> Option<SharedString> {
    let spt = tracks.seconds_per_tick();
    let words = match tracks.cut()? {
        TrackCut::FileCap { at_tick } => tr!(
            "demo_viewer.playback.cut_file",
            time = snapshot_time(at_tick, spt)
        ),
        TrackCut::SampleCap { at_tick } => tr!(
            "demo_viewer.playback.cut_samples",
            time = snapshot_time(at_tick, spt)
        ),
        TrackCut::EndedEarly { at_tick } => tr!(
            "demo_viewer.playback.cut_ended",
            time = snapshot_time(at_tick, spt)
        ),
        _ => return None,
    };
    Some(words)
}

impl DemoViewerPage {
    // A fresh slider over the tracks' ticks, since a slider's range is fixed when it is built.
    // Its moves seek the viewport, and the viewport's moves set it back without a Change.
    pub(super) fn start_timeline(
        &mut self,
        tracks: &Arc<DemoTracks>,
        viewport: &Entity<DemoViewport>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let first = tracks.first_tick();
        let end = tracks.last_tick().max(first + 1);
        let state = viewport.read(cx).playback();
        let at = state.map_or(first as f32, |s| s.tick as f32);
        let slider = cx.new(|_| {
            SliderState::new()
                .min(first as f32)
                .max(end as f32)
                .step(1.)
                .default_value(at)
        });
        let watch = [
            cx.subscribe_in(&slider, window, |page, _, ev: &SliderEvent, _, cx| {
                if let SliderEvent::Change(v) = ev {
                    page.seek(f64::from(v.start()), cx);
                }
            }),
            cx.subscribe_in(
                viewport,
                window,
                |page, _, ev: &PlaybackMoved, window, cx| {
                    page.playback_moved(ev.0, window, cx);
                },
            ),
            cx.subscribe(viewport, |page, _, ev: &CameraMoved, cx| {
                page.camera_moved(ev.0, cx);
            }),
        ];
        let tick = state.map_or(f64::from(first), |s| s.tick);
        self.timeline = Some(Timeline {
            slider,
            first,
            end,
            rounds: tracks.round_starts().to_vec(),
            kill_marks: tracks
                .kills()
                .iter()
                .map(|kill| {
                    let team = kill.killer().and_then(KillParty::team);
                    (timeline_fraction(kill.tick(), first, end), team)
                })
                .collect(),
            cut_note: cut_note(tracks),
            shown: state.map(shown),
            camera: viewport.read(cx).camera_state(),
            tracks: Arc::clone(tracks),
            feed: feed_rows(tracks, tick),
            feed_key: feed_key(tracks, tick),
            feed_tick: tick,
            feed_locale: i18n::current(),
            _watch: watch,
        });
        cx.notify();
    }

    // The slider follows every move; the page draws again only when what it shows changed.
    fn playback_moved(
        &mut self,
        state: PlaybackState,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(timeline) = self.timeline.as_mut() else {
            return;
        };
        timeline
            .slider
            .update(cx, |st, cx| st.set_value(state.tick as f32, window, cx));
        // The key is two searches and no allocation, so only a changed feed builds rows.
        timeline.feed_tick = state.tick;
        let key = feed_key(&timeline.tracks, state.tick);
        let fed = timeline.feed_key != key;
        if fed {
            timeline.feed_key = key;
            timeline.feed = feed_rows(&timeline.tracks, state.tick);
        }
        let now = Some(shown(state));
        if fed || timeline.shown != now {
            timeline.shown = now;
            cx.notify();
        }
    }

    // The rows hold translated fallback names, so a language switch rebuilds them as it draws.
    pub(super) fn refresh_feed_locale(&mut self) {
        if let Some(timeline) = self.timeline.as_mut()
            && timeline.feed_locale != i18n::current()
        {
            timeline.feed_locale = i18n::current();
            timeline.feed = feed_rows(&timeline.tracks, timeline.feed_tick);
        }
    }

    fn camera_moved(&mut self, state: CameraState, cx: &mut Context<Self>) {
        if let Some(timeline) = self.timeline.as_mut()
            && timeline.camera != state
        {
            timeline.camera = state;
            cx.notify();
        }
    }

    fn seek(&mut self, tick: f64, cx: &mut Context<Self>) {
        if let Some(viewport) = self.viewport().cloned() {
            viewport.update(cx, |v, cx| v.seek(tick, cx));
        }
    }

    // A bar control acts on the viewport, then hands it the keyboard back, so Space works
    // after a click on the bar without a page-wide key handler that text inputs would lose.
    pub(super) fn on_viewport(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
        f: impl FnOnce(&mut DemoViewport, &mut Context<DemoViewport>),
    ) {
        if let Some(viewport) = self.viewport().cloned() {
            viewport.update(cx, f);
            viewport.update(cx, |v, cx| v.focus(window, cx));
        }
    }

    // One marker per round start, at its share of the slider's range; a click seeks there.
    fn round_markers(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let Some(timeline) = &self.timeline else {
            return Vec::new();
        };
        let colour = cx.theme().muted_foreground;
        timeline
            .rounds
            .iter()
            .enumerate()
            .map(|(n, &tick)| {
                let fraction = timeline_fraction(tick, timeline.first, timeline.end);
                let words = tr!("demo_viewer.playback.round", n = n + 1);
                let tip = words.clone();
                div()
                    .id(ids::demo_viewer_round_marker(n))
                    .aria_label(words)
                    .test_support()
                    .absolute()
                    .top_0()
                    .left(relative(fraction))
                    .ml(px(-3.))
                    .size(px(6.))
                    .rounded_full()
                    .bg(colour)
                    .cursor_pointer()
                    .tooltip(move |window, cx| tip_box(tip.clone()).build(window, cx))
                    .on_click(cx.listener(move |page, _, _, cx| page.seek(f64::from(tick), cx)))
                    .into_any_element()
            })
            .collect()
    }

    // One painted strip with a 2 px line per kill in its killer's colour. It has no handlers,
    // so the round markers drawn after it keep every click and tooltip.
    fn kill_strip(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let timeline = self
            .timeline
            .as_ref()
            .filter(|t| !t.kill_marks.is_empty())?;
        let theme = cx.theme();
        let marks: Vec<_> = timeline
            .kill_marks
            .iter()
            .map(|&(fraction, team)| {
                let colour = match team {
                    Some(Team::Red) => theme.red,
                    Some(_) => theme.blue,
                    None => theme.muted_foreground,
                };
                (fraction, colour)
            })
            .collect();
        let count = marks.len();
        let paint = canvas(
            |_, _, _| {},
            move |bounds, _, window, _| {
                for &(fraction, colour) in &marks {
                    let x = bounds.origin.x + bounds.size.width * fraction - px(1.);
                    let at = point(x, bounds.origin.y);
                    window.paint_quad(fill(
                        gpui_kit::Bounds::new(at, size(px(2.), bounds.size.height)),
                        colour,
                    ));
                }
            },
        )
        .size_full();
        Some(
            div()
                .id(ids::DEMO_VIEWER_KILL_MARKERS)
                .aria_label(tr!("demo_viewer.killfeed.markers", count = count))
                .test_support()
                .absolute()
                .top_0()
                .left_0()
                .size_full()
                .child(paint)
                .into_any_element(),
        )
    }

    /// The bar's second row for `viewport`; every control waits, disabled, for the tracks.
    pub(super) fn playback_row(
        &self,
        viewport: &Entity<DemoViewport>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let state = viewport.read(cx).playback();
        let waiting = state.is_none();
        let muted = cx.theme().muted_foreground;
        let step = |id: &'static str, icon: IconName, words: SharedString, delta: i32| {
            Button::new(id)
                .ghost()
                .small()
                .icon(icon)
                .named(words)
                .disabled(waiting)
                .on_click(cx.listener(move |page, _, window, cx| {
                    page.on_viewport(window, cx, |v, cx| v.step(delta, cx));
                }))
        };
        let back = step(
            ids::DEMO_VIEWER_STEP_BACK,
            IconName::StepBack,
            tr!("demo_viewer.playback.step_back"),
            -1,
        );
        let forward = step(
            ids::DEMO_VIEWER_STEP_FORWARD,
            IconName::StepForward,
            tr!("demo_viewer.playback.step_forward"),
            1,
        );
        let (icon, words) = match state.is_some_and(|s| s.playing) {
            true => (IconName::Pause, tr!("demo_viewer.playback.pause")),
            false => (IconName::Play, tr!("demo_viewer.playback.play")),
        };
        let play = Button::new(ids::DEMO_VIEWER_PLAY)
            .ghost()
            .small()
            .icon(icon)
            .named(words)
            .disabled(waiting)
            .on_click(cx.listener(|page, _, window, cx| {
                page.on_viewport(window, cx, |v, cx| v.toggle_play(cx));
            }));
        let speed = state.map_or(1., |s| s.speed);
        // The name carries the speed as well, so a screen reader hears what the button shows.
        let speed_button = Button::new(ids::DEMO_VIEWER_SPEED)
            .ghost()
            .small()
            .label(format!("{speed}×"))
            .named(format!("{} {speed}×", tr!("demo_viewer.playback.speed")))
            .disabled(waiting)
            .on_click(cx.listener(move |page, _, window, cx| {
                page.on_viewport(window, cx, |v, cx| v.set_speed(next_speed(speed), cx));
            }));
        let slider = match &self.timeline {
            Some(timeline) => Slider::new(&timeline.slider),
            None => Slider::new(&self.timeline_placeholder).disabled(true),
        };
        let timeline = v_flex()
            .id(ids::DEMO_VIEWER_TIMELINE)
            .aria_label(tr!("demo_viewer.playback.timeline"))
            .test_support()
            .flex_1()
            .min_w(px(160.))
            .gap_0p5()
            .child(
                div()
                    .relative()
                    .w_full()
                    .h(px(6.))
                    .children(self.kill_strip(cx))
                    .children(self.round_markers(cx)),
            )
            .child(slider);
        let time = state.map_or_else(
            || timeline_time(0., 0.),
            |s| timeline_time(s.elapsed_seconds, s.total_seconds),
        );
        h_flex()
            .w_full()
            .gap_1()
            .items_center()
            .child(back)
            .child(play)
            .child(forward)
            .child(timeline)
            .child(labelled(
                div()
                    .flex_none()
                    .px_1()
                    .whitespace_nowrap()
                    .text_xs()
                    .text_color(muted),
                ids::DEMO_VIEWER_TIME,
                time,
            ))
            .child(speed_button)
            .into_any_element()
    }
}
