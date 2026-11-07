//! The floating sound player: one sound at a time through `AudioOut`, with v5's card showing
//! what plays and the app's one volume.

use std::sync::Arc;

use gpui_kit::assets::IconName;
use gpui_kit::base::{POPUP_PRIORITY, Root, RootPlugin};
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::slider::{Slider, SliderState};
use gpui_kit::component::{ActiveTheme as _, Icon, Sizable as _, Size, h_flex, v_flex};
use gpui_kit::{
    AnyWindowHandle, App, AppContext as _, Context, Entity, FontWeight, Global,
    InteractiveElement as _, IntoElement, ParentElement as _, Render, SharedString, Styled as _,
    Subscription, Task, TestSupportExt as _, Window, deferred, div, px,
};

use super::AudioOut;
use super::volume::{SharedVolume, bind_slider, shared_volume};
use crate::ids;
use crate::local_messages::{describe_audio, describe_settings};
use crate::services::{OpError, Services, run_blocking};
use crate::shell::toast::{lift_above_player, toast_error};
use crate::widgets::Named as _;
use crate::widgets::labelled;

/// What the floating player shows while a sound plays; `key` tells two sounds apart.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NowPlaying {
    pub key: SharedString,
    pub name: SharedString,
    pub subtitle: SharedString,
}

/// Plays one sound at a time. Each play or stop bumps `generation`, so a load or an end
/// that belongs to an older play changes nothing.
pub struct Player {
    audio: Arc<dyn AudioOut>,
    win: AnyWindowHandle,
    generation: u64,
    now: Option<NowPlaying>,
    volume: Entity<SliderState>,
    shared: Entity<SharedVolume>,
    ready: bool,
    _volume: [Subscription; 3],
}

impl Player {
    /// Needs the `Services` global. Reads the saved volume off the UI thread unless the app
    /// already knows it (`is_ready` after).
    /// Releasing the entity stops its sound, if it holds one.
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let audio = Arc::clone(&cx.global::<Services>().audio);
        let shared = shared_volume(cx);
        let percent = f32::from(shared.read(cx).get().get());
        let volume = cx.new(|_| {
            SliderState::new()
                .min(0.)
                .max(100.)
                .step(1.)
                .default_value(percent)
        });
        let bound = bind_slider(&volume, window, cx);
        // A player holding nothing leaves the shared audio alone; another may be playing.
        cx.on_release(|player, _| {
            if player.now.is_some() {
                player.audio.stop();
            }
        })
        .detach();
        let mut player = Self {
            audio,
            win: window.window_handle(),
            generation: 0,
            now: None,
            volume,
            shared,
            ready: false,
            _volume: bound,
        };
        player.load_volume(cx);
        player
    }

    pub fn is_ready(&self) -> bool {
        self.ready
    }

    pub fn now_playing(&self) -> Option<&NowPlaying> {
        self.now.as_ref()
    }

    /// The 0..100 volume slider, tied to the app's one volume; letting go saves it.
    pub fn volume(&self) -> &Entity<SliderState> {
        &self.volume
    }

    /// Stops if `now.key` is playing; else stops the old sound, shows `now`, reads the bytes with
    /// `load` in `run_blocking` and plays them. A newer toggle or `stop` drops a slower load.
    /// A failed load or play clears the player and toasts its error. Returns the load-and-play
    /// task (a ready one for a stop); the caller detaches it, and dropping it cancels the play.
    pub fn toggle(
        &mut self,
        now: NowPlaying,
        load: impl FnOnce(&Services) -> Result<Vec<u8>, OpError> + Send + 'static,
        cx: &mut Context<Self>,
    ) -> Task<()> {
        self.toggle_with(now, |cx| run_blocking(cx, load), cx)
    }

    /// As `toggle`, but the bytes come from the task `start` returns; `start` runs only when
    /// this toggle plays (a toggle of the playing key stops without calling it).
    pub fn toggle_with(
        &mut self,
        now: NowPlaying,
        start: impl FnOnce(&mut App) -> Task<Result<Vec<u8>, OpError>>,
        cx: &mut Context<Self>,
    ) -> Task<()> {
        if self
            .now
            .as_ref()
            .is_some_and(|playing| playing.key == now.key)
        {
            self.stop(cx);
            return Task::ready(());
        }
        self.generation += 1;
        let generation = self.generation;
        self.audio.stop();
        self.now = Some(now);
        cx.notify();
        let loading = start(cx);
        let win = self.win;
        cx.spawn(async move |player, cx| {
            let loaded = loading.await;
            // Start the play only if this toggle is still the latest; the end is awaited outside
            // the update so the entity is not leased while the sound plays.
            let started = player.update(cx, |player, cx| {
                (player.generation == generation).then(|| {
                    loaded.map(|wav| player.audio.play(wav, player.shared.read(cx).gain()))
                })
            });
            let failed = match started {
                Ok(Some(Ok(playback))) => playback
                    .ended()
                    .await
                    .err()
                    .map(|err| describe_audio(&err).into_op("Failed to play sound")),
                Ok(Some(Err(err))) => Some(err),
                Ok(None) | Err(_) => return,
            };
            let current = player
                .update(cx, |player, cx| player.clear(generation, cx))
                .unwrap_or(false);
            // Toasting through the window handle keeps the Player unleased; it may be the root.
            if let Some(err) = failed.filter(|_| current) {
                let _ =
                    cx.update(|cx| win.update(cx, |_, window, cx| toast_error(window, cx, &err)));
            }
        })
    }

    pub fn stop(&mut self, cx: &mut Context<Self>) {
        self.generation += 1;
        self.audio.stop();
        self.now = None;
        cx.notify();
    }

    // Clears the player if `generation` is still the latest play; says whether it did.
    fn clear(&mut self, generation: u64, cx: &mut Context<Self>) -> bool {
        if self.generation != generation {
            return false;
        }
        self.now = None;
        cx.notify();
        true
    }

    // Reads the saved volume once per run; a failed read toasts and leaves the default.
    fn load_volume(&mut self, cx: &mut Context<Self>) {
        if self.shared.read(cx).is_known() {
            self.ready = true;
            return;
        }
        let reading = run_blocking(cx, |s| {
            s.db.settings()
                .map_err(|e| describe_settings(&e).into_op("Load settings"))
        });
        let (win, shared) = (self.win, self.shared.clone());
        cx.spawn(async move |player, cx| {
            let read = reading.await;
            cx.update(|cx| match read {
                Ok(settings) => {
                    shared.update(cx, |shared, cx| shared.loaded(settings.volume(), cx))
                }
                Err(err) => {
                    let _ = win.update(cx, |_, window, cx| toast_error(window, cx, &err));
                }
            });
            let _ = player.update(cx, |player, cx| {
                player.ready = true;
                cx.notify();
            });
        })
        .detach();
    }
}

// The app's one player, built on first use, and whether it shows: always while it plays, else
// only where `show_idle` asks. Pages send sounds to it and the window root's `PlayerLayer` draws
// it, so a sound outlives the page that started it.
#[derive(Default)]
struct SoundPlayer {
    player: Option<Entity<Player>>,
    idle_shown: bool,
    shown: bool,
}

impl Global for SoundPlayer {}

/// The app's one floating player, built in `window` on first use.
pub fn sound_player(window: &mut Window, cx: &mut App) -> Entity<Player> {
    if let Some(player) = cx
        .try_global::<SoundPlayer>()
        .and_then(|g| g.player.clone())
    {
        return player;
    }
    let player = cx.new(|cx| Player::new(window, cx));
    cx.observe(&player, |_, cx| sync_shown(cx)).detach();
    cx.default_global::<SoundPlayer>().player = Some(player.clone());
    player
}

/// Shows or hides the app's player while it plays nothing; a playing player always shows.
pub fn show_idle(shown: bool, cx: &mut App) {
    cx.default_global::<SoundPlayer>().idle_shown = shown;
    sync_shown(cx);
}

// Recomputes whether the player shows; a change moves the toasts clear of it and redraws, since
// a hidden player's own notify reaches no window.
fn sync_shown(cx: &mut App) {
    let global = cx.default_global::<SoundPlayer>();
    let (idle, player) = (global.idle_shown, global.player.clone());
    let shown = idle || player.is_some_and(|p| p.read(cx).now_playing().is_some());
    let global = cx.global_mut::<SoundPlayer>();
    if global.shown == shown {
        return;
    }
    global.shown = shown;
    lift_above_player(shown, cx);
    cx.refresh_windows();
}

/// Draws the app's player in every window over the page, whichever page shows.
pub struct PlayerLayer;

impl PlayerLayer {
    /// Registers the layer for every window opened after this call.
    pub fn register(cx: &mut App) {
        Root::register_plugin(cx, |_, _| Self);
    }
}

impl RootPlugin for PlayerLayer {}

impl Render for PlayerLayer {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let player = cx
            .try_global::<SoundPlayer>()
            .filter(|global| global.shown)
            .and_then(|global| global.player.clone());
        div().size_full().children(player)
    }
}

// An open dialog keeps anything under the pop-up layer from taking clicks (see
// tests/sound_store_play.rs), so the player sits just above it and under the tooltips.
const PLAYER_PRIORITY: usize = POPUP_PRIORITY + 1;

impl Render for Player {
    // v5's SoundPreviewPlayer: a card pinned bottom right, the playing sound over the volume row.
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let muted = theme.muted_foreground;
        let volume = self.shared.read(cx).get().get();
        let speaker = if volume == 0 {
            IconName::VolumeX
        } else {
            IconName::Volume2
        };
        let playing = self.now.clone().map(|now| {
            h_flex()
                .gap_2()
                .mb_2()
                .child(
                    Button::new(ids::SOUND_PLAYER_STOP)
                        .ghost()
                        .icon(IconName::Square)
                        .size(px(32.))
                        .named("Stop")
                        .on_click(cx.listener(|player, _, _, cx| player.stop(cx))),
                )
                .child(
                    // Tailwind's text-sm and text-xs lines (20 and 16 px);
                    // gpui's defaults run taller.
                    v_flex()
                        .flex_1()
                        .min_w_0()
                        .child(labelled(
                            div()
                                .text_sm()
                                .line_height(px(20.))
                                .font_weight(FontWeight::MEDIUM)
                                .truncate(),
                            ids::SOUND_PLAYER_NAME,
                            now.name,
                        ))
                        .child(labelled(
                            div()
                                .text_xs()
                                .line_height(px(16.))
                                .text_color(muted)
                                .truncate(),
                            ids::SOUND_PLAYER_SUBTITLE,
                            now.subtitle,
                        )),
                )
        });
        // The player floats over the page and any open dialog, and swallows its own clicks so
        // a press on it never reaches the card or backdrop beneath.
        let card = v_flex()
            .id(ids::SOUND_PLAYER)
            .test_support()
            .occlude()
            .absolute()
            .bottom(px(16.))
            .right(px(16.))
            .w(px(288.))
            .p_3()
            .rounded(theme.radius_lg)
            .border_1()
            .border_color(theme.border)
            .bg(theme.group_box)
            .shadow_lg()
            .children(playing)
            // v5's 16 px row; the slider's 24 px track overflows it evenly above and below.
            .child(
                h_flex()
                    .gap_2()
                    .h(px(16.))
                    .child(
                        Icon::new(speaker)
                            .with_size(Size::Size(px(16.)))
                            .text_color(muted),
                    )
                    .child(
                        div()
                            .id(ids::SOUND_PLAYER_VOLUME)
                            .test_support()
                            .flex_1()
                            .flex()
                            .items_center()
                            .h(px(24.))
                            .child(Slider::new(&self.volume)),
                    )
                    .child(labelled(
                        div()
                            .w(px(28.))
                            .text_xs()
                            .line_height(px(16.))
                            .text_right()
                            .text_color(muted),
                        ids::SOUND_PLAYER_VOLUME_VALUE,
                        volume.to_string(),
                    )),
            );
        deferred(card).with_priority(PLAYER_PRIORITY)
    }
}
