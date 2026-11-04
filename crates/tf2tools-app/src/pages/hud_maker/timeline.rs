//! The timeline strip under the canvas: the events playing, Play and Pause, a slider over the
//! moment shown and what only shows in game. Playing never changes the HUD.

use std::time::Instant;

use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::slider::{Slider, SliderEvent, SliderState};
use gpui_kit::component::{ActiveTheme as _, Sizable as _, h_flex};
use gpui_kit::{
    AnyElement, AppContext as _, Context, Entity, InteractiveElement as _, IntoElement,
    ParentElement as _, SharedString, Styled as _, Subscription, TestSupportExt as _, Window, div,
};

use super::HudMakerPage;
use crate::ids;
use crate::widgets::labelled;

/// The slider runs over this many steps of the events' length.
const STEPS: f32 = 1000.;

/// Whether the timeline plays, from which moment and since when, and its slider.
pub(super) struct Timeline {
    playing: Option<(Instant, f32)>,
    slider: Entity<SliderState>,
    _subscription: Subscription,
}

impl Timeline {
    pub(super) fn new(window: &mut Window, cx: &mut Context<HudMakerPage>) -> Timeline {
        let slider = cx.new(|_| SliderState::new().min(0.).max(STEPS).step(1.));
        // Dragging the slider shows that moment and stops playback.
        let subscription =
            cx.subscribe_in(&slider, window, |page, _, ev: &SliderEvent, window, cx| {
                if let SliderEvent::Change(v) = ev {
                    let length = page.editor().map_or(0., |e| e.length());
                    page.timeline.playing = None;
                    page.set_moment(v.start() / STEPS * length, window, cx);
                }
            });
        Timeline {
            playing: None,
            slider,
            _subscription: subscription,
        }
    }
}

impl HudMakerPage {
    /// Starts `event` on the timeline from 0; false when the HUD has no such event.
    pub fn play_event(&mut self, event: &str, window: &mut Window, cx: &mut Context<Self>) -> bool {
        self.flush(cx);
        self.stop_playing();
        let played = self.editor_mut().is_some_and(|e| e.play_event(event));
        self.sync_timeline(window, cx);
        cx.notify();
        played
    }

    /// Shows the fired events at `t` seconds.
    pub fn set_moment(&mut self, t: f32, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(editor) = self.editor_mut() {
            editor.set_moment(t);
        }
        self.sync_timeline(window, cx);
        cx.notify();
    }

    /// Plays from the moment shown, or pauses where it is.
    pub fn toggle_playing(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        self.timeline.playing = match self.timeline.playing {
            Some(_) => None,
            None => self.editor().map(|e| (Instant::now(), e.moment())),
        };
        cx.notify();
    }

    pub fn is_playing(&self) -> bool {
        self.timeline.playing.is_some()
    }

    /// Ends playback where it is; the moment stays.
    pub(super) fn stop_playing(&mut self) {
        self.timeline.playing = None;
    }

    // While playing, moves the moment on with the clock, wrapping at the events' length, and
    // asks for the next frame.
    pub(super) fn tick_timeline(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some((since, from)) = self.timeline.playing else {
            return;
        };
        let Some(editor) = self.editor_mut() else {
            return;
        };
        let length = editor.length();
        if length <= 0. || editor.fired().is_empty() {
            self.timeline.playing = None;
            return;
        }
        let t = (from + since.elapsed().as_secs_f32()) % length;
        editor.set_moment(t);
        self.sync_timeline(window, cx);
        window.request_animation_frame();
    }

    // Moves the slider to the moment shown.
    fn sync_timeline(&self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(editor) = self.editor() else {
            return;
        };
        let length = editor.length();
        let at = match length > 0. {
            true => editor.moment() / length * STEPS,
            false => 0.,
        };
        if self.timeline.slider.read(cx).value() != at.into() {
            (self.timeline.slider).update(cx, |s, cx| s.set_value(at, window, cx));
        }
    }

    // The strip under the canvas, while any event plays.
    pub(super) fn timeline(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let editor = self.editor()?;
        let fired = editor.fired();
        if fired.is_empty() {
            return None;
        }
        let names: Vec<&str> = fired.iter().map(|f| f.event()).collect();
        let names = SharedString::from(names.join(", "));
        let in_game = (editor.moment() > 0.)
            .then(|| editor.frame().in_game_only().join(", "))
            .filter(|fields| !fields.is_empty());
        // The events' real end, though the slider runs at least a second; a hundredth is plenty.
        let end = editor.end();
        let hundredths = |t: f32| (t * 100.).round() / 100.;
        let time = format!(
            "{} s of {} s",
            hundredths(editor.moment().min(end)),
            hundredths(end)
        );
        let muted = cx.theme().muted_foreground;
        let playing = self.is_playing();
        // The icon set has no pause glyph, so a playing timeline says Pause in words.
        let play = Button::new(ids::HUD_MAKER_TIMELINE_PLAY).ghost().xsmall();
        let play = match playing {
            true => play.label("Pause").tooltip("Pause"),
            false => play.icon(IconName::Play).tooltip("Play"),
        }
        .on_click(cx.listener(|page, _, window, cx| page.toggle_playing(window, cx)));
        let clear = Button::new(ids::HUD_MAKER_TIMELINE_CLEAR)
            .ghost()
            .xsmall()
            .label("Clear")
            .tooltip("Stop the animations you started")
            .on_click(cx.listener(|page, _, window, cx| {
                page.stop_playing();
                if let Some(editor) = page.editor_mut() {
                    editor.clear_played();
                }
                page.sync_timeline(window, cx);
                cx.notify();
            }));
        Some(
            h_flex()
                .id(ids::HUD_MAKER_TIMELINE)
                .test_support()
                .w_full()
                .px_2()
                .py_1()
                .gap_2()
                .items_center()
                .border_t_1()
                .border_color(cx.theme().border)
                .child(play)
                .child(labelled(
                    div().max_w_64().truncate().text_sm(),
                    ids::HUD_MAKER_TIMELINE_EVENTS,
                    names,
                ))
                .child(
                    div()
                        .flex_1()
                        .min_w_24()
                        .child(Slider::new(&self.timeline.slider)),
                )
                .child(labelled(
                    div().text_xs().text_color(muted),
                    ids::HUD_MAKER_TIMELINE_TIME,
                    SharedString::from(time),
                ))
                .children(in_game.map(|fields| {
                    labelled(
                        div().max_w_64().truncate().text_xs().text_color(muted),
                        ids::HUD_MAKER_TIMELINE_IN_GAME,
                        SharedString::from(format!("Changes in game: {fields}")),
                    )
                }))
                .child(clear)
                .into_any_element(),
        )
    }
}
