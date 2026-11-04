//! The one playback volume: every volume slider shows it, the speakers follow it, and a slider
//! saves it when let go. It lives in a global, so no page owns it.

use async_channel::Sender;
use gpui_kit::component::slider::{SliderEvent, SliderState};
use gpui_kit::{App, AppContext as _, Context, Entity, Global, Subscription, Task, Window};
use tf2tools_core::Volume;

use crate::local_messages::describe_settings;
use crate::services::{OpError, Services, run_blocking};
use crate::shell::toast::toast_error;

/// The volume as drawn and heard right now; the database catches up on each `save`.
pub struct SharedVolume {
    volume: Volume,
    /// False until the saved value is read or the player changes it; a read then changes nothing.
    known: bool,
    writing: bool,
    /// One sender per `save` still to be covered by a write.
    waiting: Vec<Sender<Result<(), OpError>>>,
}

struct VolumeGlobal(Entity<SharedVolume>);

impl Global for VolumeGlobal {}

/// The app's volume, created at the default on first use.
pub fn shared_volume(cx: &mut App) -> Entity<SharedVolume> {
    if let Some(global) = cx.try_global::<VolumeGlobal>() {
        return global.0.clone();
    }
    let volume = cx.new(|_| SharedVolume {
        volume: Volume::default(),
        known: false,
        writing: false,
        waiting: Vec::new(),
    });
    cx.set_global(VolumeGlobal(volume.clone()));
    volume
}

/// Ties `slider` to the shared volume both ways, redraws the owner on each change and saves when
/// the slider is let go. The owner keeps the subscriptions for as long as it draws the slider.
pub fn bind_slider<V: 'static>(
    slider: &Entity<SliderState>,
    window: &mut Window,
    cx: &mut Context<V>,
) -> [Subscription; 3] {
    let shared = shared_volume(cx);
    let moved = cx.observe(slider, {
        let shared = shared.clone();
        move |_, slider, cx| {
            let percent = slider.read(cx).value().start().round().clamp(0., 100.) as u8;
            if let Some(volume) = Volume::new(percent) {
                shared.update(cx, |shared, cx| shared.set(volume, cx));
            }
            cx.notify();
        }
    });
    let followed = cx.observe_in(&shared, window, {
        let slider = slider.clone();
        move |_, shared, window, cx| {
            let percent = f32::from(shared.read(cx).get().get());
            if slider.read(cx).value().start() != percent {
                slider.update(cx, |slider, cx| slider.set_value(percent, window, cx));
            }
        }
    });
    let win = window.window_handle();
    let released = cx.subscribe(slider, move |_, _, event: &SliderEvent, cx| {
        if !matches!(event, SliderEvent::Release(_)) {
            return;
        }
        let saved = shared.update(cx, |shared, cx| shared.save(cx));
        cx.spawn(async move |_, cx| {
            if let Err(err) = saved.await {
                let _ =
                    cx.update(|cx| win.update(cx, |_, window, cx| toast_error(window, cx, &err)));
            }
        })
        .detach();
    });
    [moved, followed, released]
}

impl SharedVolume {
    pub fn get(&self) -> Volume {
        self.volume
    }

    /// The volume as `AudioOut` takes it, 0.0 to 1.0.
    pub fn gain(&self) -> f32 {
        f32::from(self.volume.get()) / 100.
    }

    /// True once the saved value is in or the player has changed the volume.
    pub fn is_known(&self) -> bool {
        self.known
    }

    /// Takes the value read from the database, unless the volume is already known.
    pub fn loaded(&mut self, saved: Volume, cx: &mut Context<Self>) {
        if !self.known {
            self.set(saved, cx);
            self.known = true;
        }
    }

    /// Changes the volume for every slider and the playing sound, without saving it.
    pub fn set(&mut self, volume: Volume, cx: &mut Context<Self>) {
        if volume == self.volume {
            return;
        }
        self.volume = volume;
        self.known = true;
        cx.global::<Services>().audio.set_volume(self.gain());
        cx.notify();
    }

    /// Saves the volume off the UI thread; the task ends with the first write of a value at
    /// least as new as this call. Only one write runs at a time and each takes the newest value,
    /// so an older value can never land last: saves during a write fold into one more write.
    pub fn save(&mut self, cx: &mut Context<Self>) -> Task<Result<(), OpError>> {
        let (done, finished) = async_channel::bounded(1);
        self.waiting.push(done);
        self.write(cx);
        cx.background_spawn(async move { finished.recv().await.unwrap_or(Ok(())) })
    }

    fn write(&mut self, cx: &mut Context<Self>) {
        if self.writing {
            return;
        }
        self.writing = true;
        let volume = self.volume;
        let waiting = std::mem::take(&mut self.waiting);
        let writing = run_blocking(cx, move |s| {
            s.db.set_volume(volume)
                .map_err(|e| describe_settings(&e).into_op("Failed to save"))
        });
        cx.spawn(async move |shared, cx| {
            let written = writing.await;
            for done in waiting {
                let _ = done.try_send(written.clone());
            }
            let _ = shared.update(cx, |shared, cx| {
                shared.writing = false;
                if !shared.waiting.is_empty() {
                    shared.write(cx);
                }
            });
        })
        .detach();
    }
}
