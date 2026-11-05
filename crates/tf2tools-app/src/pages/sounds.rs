//! v5's Sounds page: the hitsound and killsound slots, conflicts, the sound library and the
//! floating sound player.

use std::rc::Rc;

use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::tag::Tag;
use gpui_kit::component::{
    ActiveTheme as _, Disableable as _, Icon, Sizable as _, Size, h_flex, v_flex,
};
use gpui_kit::{
    AnyElement, AnyWindowHandle, App, AppContext as _, ClickEvent, Context, ElementId, Entity,
    FontWeight, Hsla, InteractiveElement as _, IntoElement, ParentElement as _, Render, Role,
    SharedString, StatefulInteractiveElement as _, Styled as _, Subscription, TestSupportExt as _,
    WeakEntity, Window, div, px, rgb, transparent_black,
};

use tf2tools_core::{
    InstalledSounds, LibraryError, LogLevel, NewLogEntry, Provenance, SoundEntry, SoundFormat,
    SoundKind,
};

use super::common::{SM_ICON_PX, YELLOW_500, badge_of, card_frame};
use crate::activity;
use crate::audio::player::{NowPlaying, Player, sound_player};
use crate::ids;
use crate::library_events::{LibraryChanged, LibraryEvents};
use crate::local_messages::{describe_import, describe_library, describe_tf2_install};
use crate::picker::PickKind;
use crate::services::{OpError, Services, require_tf2, run_blocking, tf2_state};
use crate::shell::CONTENT_PADDING;
use crate::shell::confirm::{Confirm, confirm};
use crate::shell::route::Page;
use crate::shell::toast::{ToastKind, toast, toast_error};
use crate::widgets::Named as _;
use crate::widgets::card_grid::CardGrid;
use crate::widgets::labelled;

/// The action in flight: its button spins and the other mutating buttons disable.
#[derive(Clone, PartialEq, Eq)]
enum Busy {
    Import,
    /// Installing the sound in this folder as this kind.
    Install(String, SoundKind),
    /// Deleting the sound in this folder.
    Delete(String),
    Uninstall(SoundKind),
}

/// The Sounds page. Built fresh on each visit and plays through the app's one player; it
/// reloads whenever the TF2 state changes.
pub struct SoundsPage {
    win: AnyWindowHandle,
    sounds: Rc<[SoundEntry]>,
    /// Each sound's name lowercased, cached per load so the search filter never re-lowers them.
    lowered: Vec<String>,
    /// The tf/custom scan, absent while no TF2 install is known.
    installed: Option<InstalledSounds>,
    loaded: bool,
    busy: Option<Busy>,
    /// Bumped by each reload, so a slower, older load never overwrites a newer one.
    load_generation: u64,
    player: Entity<Player>,
    search: Entity<InputState>,
    grid: CardGrid,
    _tf2: Subscription,
    _library: Subscription,
    _player: Subscription,
    _search: Subscription,
}

impl SoundsPage {
    /// Needs the `Services` and `Tf2Global` globals. Starts the first load off the UI thread;
    /// reloads whenever the TF2 state or the library changes.
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let tf2 = cx.observe(&tf2_state(cx), |page, _, cx| page.reload(cx));
        let events = LibraryEvents::global(cx);
        let library = cx.subscribe(&events, |page, _, _: &LibraryChanged, cx| page.reload(cx));
        let player = sound_player(window, cx);
        // The slot play icons read the player's sound, so redraw when it starts or ends.
        let observe_player = cx.observe(&player, |_, _, cx| cx.notify());
        let search = cx.new(|cx| InputState::new(window, cx).placeholder("Search sounds..."));
        // The grid filters from the input's value at render, so each edit just redraws.
        let search_changed = cx.subscribe(&search, |_, _, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Change) {
                cx.notify();
            }
        });
        let mut page = Self {
            win: window.window_handle(),
            sounds: Rc::from([]),
            lowered: Vec::new(),
            installed: None,
            loaded: false,
            busy: None,
            load_generation: 0,
            player,
            search,
            grid: CardGrid::new(ids::SOUNDS_GRID),
            _tf2: tf2,
            _library: library,
            _player: observe_player,
            _search: search_changed,
        };
        page.reload(cx);
        page
    }

    /// True once a load has finished, whether it succeeded or toasted.
    pub fn is_loaded(&self) -> bool {
        self.loaded
    }

    /// The app's floating player; its sound outlives the page.
    pub fn player(&self) -> &Entity<Player> {
        &self.player
    }

    /// The library search box.
    pub fn search_input(&self) -> &Entity<InputState> {
        &self.search
    }

    // Reads the library and the tf/custom scan in the background. Any error toast goes through
    // the stored window handle, so a page dropped mid-load is harmless.
    fn reload(&mut self, cx: &mut Context<Self>) {
        self.load_generation += 1;
        let generation = self.load_generation;
        let tf2 = tf2_state(cx).read(cx).install().cloned();
        let loading = run_blocking(cx, move |s| {
            let failed = |e: LibraryError| describe_library(&e).into_op("Failed to load sounds");
            let sounds = s.library.list_sounds().map_err(failed)?;
            let installed = tf2
                .map(|t| s.library.installed_sounds(&t))
                .transpose()
                .map_err(failed)?;
            Ok((sounds, installed))
        });
        let win = self.win;
        cx.spawn(async move |page, cx| {
            let loaded = loading.await;
            let current = page
                .update(cx, |page, cx| {
                    if page.load_generation != generation {
                        return false;
                    }
                    if let Ok((sounds, installed)) = &loaded {
                        page.lowered = sounds.iter().map(|s| s.name().to_lowercase()).collect();
                        page.sounds = sounds.as_slice().into();
                        page.installed = installed.clone();
                    }
                    page.loaded = true;
                    cx.notify();
                    true
                })
                .unwrap_or(false);
            if let (true, Err(err)) = (current, loaded) {
                let _ =
                    cx.update(|cx| win.update(cx, |_, window, cx| toast_error(window, cx, &err)));
            }
        })
        .detach();
    }

    // Marks `busy`, runs `op` in `run_blocking`, toasts its (title, description) on success or
    // `toast_error` on failure through the window handle, then clears `busy` and reloads.
    fn run(
        &mut self,
        busy: Busy,
        op: impl FnOnce(&Services) -> Result<(SharedString, SharedString), OpError> + Send + 'static,
        cx: &mut Context<Self>,
    ) {
        self.busy = Some(busy);
        cx.notify();
        let running = run_blocking(cx, op);
        let win = self.win;
        cx.spawn(async move |page, cx| {
            let done = running.await;
            let ok = done.is_ok();
            cx.update(|cx| {
                let _ = win.update(cx, |_, window, cx| match done {
                    Ok((title, description)) => {
                        toast(window, cx, ToastKind::Success, title, description)
                    }
                    Err(err) => toast_error(window, cx, &err),
                });
                // Outside the page, so a success is announced even when the page is gone; a
                // surviving page reloads through its own LibraryChanged subscription.
                if ok {
                    LibraryEvents::changed(cx);
                }
            });
            let _ = page.update(cx, |page, cx| {
                page.busy = None;
                cx.notify();
                if !ok {
                    page.reload(cx)
                }
            });
        })
        .detach();
    }

    fn uninstall(&mut self, kind: SoundKind, window: &mut Window, cx: &mut Context<Self>) {
        let tf2 = match require_tf2(cx) {
            Ok(tf2) => tf2,
            Err(err) => return toast_error(window, cx, &err),
        };
        let (done, failed) = match kind {
            SoundKind::Hitsound => ("Hitsound uninstalled", "Failed to uninstall hitsound"),
            _ => ("Killsound uninstalled", "Failed to uninstall killsound"),
        };
        self.run(
            Busy::Uninstall(kind),
            move |s| {
                s.library
                    .uninstall_sound(kind, &tf2)
                    .map_err(|e| describe_tf2_install(&e).into_op(failed))?;
                let message = format!("Uninstalled {}", kind.name());
                activity::write(
                    s,
                    &NewLogEntry::new(LogLevel::Info, "sound", "uninstall", message),
                );
                Ok((done.into(), SharedString::default()))
            },
            cx,
        );
    }

    // Picks the file on the UI thread, then imports it in the background under the file's
    // stem as its name. A cancelled pick clears the busy state silently.
    fn add(&mut self, cx: &mut Context<Self>) {
        let picking = cx.global::<Services>().picker.pick(PickKind::SoundFile, cx);
        self.busy = Some(Busy::Import);
        cx.notify();
        cx.spawn(async move |page, cx| {
            let picked = picking.await;
            let _ = page.update(cx, |page, cx| match picked {
                Some(path) => page.run(
                    Busy::Import,
                    move |s| {
                        let name = path
                            .file_stem()
                            .map_or_else(|| "sound".into(), |stem| stem.to_string_lossy());
                        let sound = s
                            .library
                            .import_sound(&path, &name, Provenance::Local)
                            .map_err(|e| describe_import(&e).into_op("Failed to import sound"))?;
                        let file = path
                            .file_name()
                            .map(|f| f.to_string_lossy())
                            .unwrap_or_default();
                        let (_, format) = format_texts(sound.format());
                        let message = format!("Imported sound: {}", sound.name());
                        let entry = NewLogEntry::new(LogLevel::Info, "sound", "import", message)
                            .with_details(format!("File: {file}, Format: {format}"));
                        activity::write(s, &entry);
                        let added = format!("{} added to your library", sound.name());
                        Ok(("Sound imported".into(), added.into()))
                    },
                    cx,
                ),
                None => {
                    page.busy = None;
                    cx.notify();
                }
            });
        })
        .detach();
    }

    fn install(
        &mut self,
        sound: &SoundEntry,
        kind: SoundKind,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let tf2 = match require_tf2(cx) {
            Ok(tf2) => tf2,
            Err(err) => return toast_error(window, cx, &err),
        };
        let (done, failed) = match kind {
            SoundKind::Hitsound => ("Hitsound installed", "Failed to install hitsound"),
            _ => ("Killsound installed", "Failed to install killsound"),
        };
        let id = sound.id().clone();
        let folder = sound.folder_name().to_owned();
        let (format, _) = format_texts(sound.format());
        self.run(
            Busy::Install(folder.clone(), kind),
            move |s| {
                s.library
                    .install_sound(&id, kind, &tf2)
                    .map_err(|e| describe_tf2_install(&e).into_op(failed))?;
                let message = format!("Installed {} ({format}): {folder}", kind.name());
                activity::write(
                    s,
                    &NewLogEntry::new(LogLevel::Info, "sound", "install", message),
                );
                Ok((done.into(), SharedString::default()))
            },
            cx,
        );
    }

    // Asks first; only a confirmed answer deletes, and a dialog closed unanswered counts as no.
    // The current TF2 install, if any, lets core remove the sound from tf/custom too.
    fn delete(&mut self, sound: &SoundEntry, window: &mut Window, cx: &mut Context<Self>) {
        let answer = confirm(Confirm::delete_sound(), window, cx);
        let id = sound.id().clone();
        let key = sound_key(sound.folder_name());
        let folder = sound.folder_name().to_owned();
        let busy = Busy::Delete(folder.clone());
        cx.spawn(async move |page, cx| {
            if !answer.recv().await.unwrap_or(false) {
                return;
            }
            let _ = page.update(cx, |page, cx| {
                // A deleted sound leaves the player, as it leaves the grid.
                page.player.update(cx, |player, cx| {
                    if player.now_playing().is_some_and(|now| now.key == key) {
                        player.stop(cx);
                    }
                });
                let tf2 = tf2_state(cx).read(cx).install().cloned();
                page.run(
                    busy,
                    move |s| {
                        s.library
                            .delete_sound(&id, tf2.as_ref())
                            .map_err(|e| describe_library(&e).into_op("Failed to delete sound"))?;
                        let message = format!("Deleted sound: {folder}");
                        let entry = NewLogEntry::new(LogLevel::Info, "sound", "delete", message);
                        activity::write(s, &entry);
                        Ok(("Sound deleted".into(), SharedString::default()))
                    },
                    cx,
                );
            });
        })
        .detach();
    }

    // Plays a library sound, or stops it when it is the one playing.
    fn play_sound(&mut self, sound: &SoundEntry, _: &mut Window, cx: &mut Context<Self>) {
        let now = NowPlaying {
            key: sound_key(sound.folder_name()),
            name: sound.name().to_owned().into(),
            subtitle: sound.original_filename().to_owned().into(),
        };
        let id = sound.id().clone();
        self.player
            .update(cx, |player, cx| {
                player.toggle(
                    now,
                    move |s| {
                        s.library
                            .sound_bytes(&id)
                            .map_err(|e| describe_library(&e).into_op("Failed to play sound"))
                    },
                    cx,
                )
            })
            .detach();
    }

    // Plays the sound tf/custom holds for `kind`, or stops it when it is the one playing.
    fn play_slot(
        &mut self,
        kind: SoundKind,
        name: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let tf2 = match require_tf2(cx) {
            Ok(tf2) => tf2,
            Err(err) => return toast_error(window, cx, &err),
        };
        let now = NowPlaying {
            key: slot_key(kind),
            name: name.into(),
            subtitle: format!("Installed {}", kind.name()).into(),
        };
        self.player
            .update(cx, |player, cx| {
                player.toggle(
                    now,
                    move |s| {
                        s.library
                            .installed_sound_bytes(kind, &tf2)
                            .map_err(|e| describe_library(&e).into_op("Failed to play sound"))
                    },
                    cx,
                )
            })
            .detach();
    }
}

/// The player key of a slot's installed sound.
fn slot_key(kind: SoundKind) -> SharedString {
    format!("slot:{}", kind.name()).into()
}

/// The player key of a library sound.
fn sound_key(folder: &str) -> SharedString {
    format!("sound:{folder}").into()
}

fn slot_title(kind: SoundKind) -> &'static str {
    match kind {
        SoundKind::Hitsound => "Hitsound",
        _ => "Killsound",
    }
}

/// What a slot card shows when its kind is installed.
struct Slot {
    name: String,
    vpk: bool,
}

impl SoundsPage {
    // A managed install shows its library sound's name; anything else shows v5's file text.
    fn slot(&self, kind: SoundKind) -> Option<Slot> {
        let slot = self.installed.as_ref()?.slot(kind);
        let active = slot.active()?;
        let vpk = active == SoundFormat::Vpk;
        let name = slot
            .managed()
            .and_then(|id| self.sounds.iter().find(|s| s.id() == id))
            .map_or_else(
                || {
                    if vpk {
                        format!("VPK {}", kind.name())
                    } else {
                        format!("{}.wav", kind.name())
                    }
                },
                |s| s.name().to_owned(),
            );
        Some(Slot { name, vpk })
    }

    // v5's slot card: icon and title with the badges at the right, then play, name and
    // Uninstall, or the muted empty line.
    fn slot_card(&self, kind: SoundKind, cx: &mut Context<Self>) -> AnyElement {
        let theme = cx.theme();
        let muted = theme.muted_foreground;
        let icon = match kind {
            SoundKind::Hitsound => IconName::Crosshair,
            _ => IconName::Target,
        };
        let title = slot_title(kind);
        let slot = self.slot(kind);
        let badges = match &slot {
            Some(slot) => h_flex()
                .gap_2()
                .child(badge_of(
                    ids::sound_slot_status(kind),
                    Tag::primary(),
                    transparent_black(),
                    "Installed",
                ))
                .children(slot.vpk.then(|| {
                    badge_of(
                        ids::sound_slot_vpk(kind),
                        Tag::secondary(),
                        transparent_black(),
                        "VPK",
                    )
                })),
            None => h_flex().child(badge_of(
                ids::sound_slot_status(kind),
                Tag::secondary().outline().text_color(theme.foreground),
                theme.border,
                "None",
            )),
        };
        let body = match slot {
            Some(slot) => {
                let key = slot_key(kind);
                let playing = self
                    .player
                    .read(cx)
                    .now_playing()
                    .is_some_and(|now| now.key == key);
                let uninstalling = self.busy == Some(Busy::Uninstall(kind));
                let name = slot.name.clone();
                h_flex()
                    .justify_between()
                    .child(
                        h_flex()
                            .gap_2()
                            .child(
                                Button::new(ids::sound_slot_play(kind))
                                    .ghost()
                                    .icon(if playing {
                                        IconName::Square
                                    } else {
                                        IconName::Play
                                    })
                                    .size(px(32.))
                                    .named(if playing { "Stop" } else { "Play" })
                                    .on_click(cx.listener(move |page, _, window, cx| {
                                        page.play_slot(kind, name.clone(), window, cx)
                                    })),
                            )
                            .child(labelled(
                                div().text_sm().text_color(muted),
                                ids::sound_slot_name(kind),
                                slot.name,
                            )),
                    )
                    .child(
                        Button::new(ids::sound_slot_uninstall(kind))
                            .outline()
                            .px(SM_ICON_PX)
                            .icon(IconName::Unplug)
                            .label(if uninstalling {
                                "Removing..."
                            } else {
                                "Uninstall"
                            })
                            .loading(uninstalling)
                            .disabled(self.busy.is_some() && !uninstalling)
                            .on_click(cx.listener(move |page, _, window, cx| {
                                page.uninstall(kind, window, cx)
                            })),
                    )
                    .into_any_element()
            }
            None => labelled(
                div().text_sm().text_color(muted),
                ids::sound_slot_name(kind),
                format!("No {} installed", kind.name()),
            )
            .into_any_element(),
        };
        card_frame(cx)
            .id(ids::sound_slot(kind))
            .test_support()
            .flex_1()
            .min_w_0()
            .py_6()
            .child(
                v_flex()
                    .gap_6()
                    .child(
                        h_flex()
                            .px_6()
                            .justify_between()
                            .child(
                                h_flex()
                                    .gap_2()
                                    .child(
                                        Icon::new(icon)
                                            .with_size(Size::Size(px(16.)))
                                            .text_color(muted),
                                    )
                                    .child(div().font_weight(FontWeight::SEMIBOLD).child(title)),
                            )
                            .child(badges),
                    )
                    .child(div().px_6().child(body)),
            )
            .into_any_element()
    }
}

// v5's yellow-500 box: one muted line per folder that ships a sound which may override ours.
fn conflict_box(installed: &InstalledSounds, cx: &App) -> Option<AnyElement> {
    let conflicts = installed.conflicts();
    if conflicts.is_empty() {
        return None;
    }
    let yellow: Hsla = rgb(YELLOW_500).into();
    let lines: Vec<(String, String)> = conflicts
        .iter()
        .map(|c| {
            let kind = c.kind().name();
            let title = slot_title(c.kind());
            let folder = c.folder_name().to_owned();
            let message = format!("{title} found in '{folder}' — may override your {kind}");
            (folder, message)
        })
        .collect();
    let label = lines
        .iter()
        .map(|(folder, message)| format!("{folder}: {message}"))
        .collect::<Vec<_>>()
        .join(" ");
    let muted = cx.theme().muted_foreground;
    Some(
        v_flex()
            .id(ids::SOUNDS_CONFLICTS)
            .role(Role::Alert)
            .aria_label(format!("Conflicts Detected. {label}"))
            .test_support()
            .gap_3()
            .p_6()
            .rounded(cx.theme().radius_lg)
            .border_1()
            .border_color(yellow.opacity(0.5))
            .bg(yellow.opacity(0.05))
            .child(
                h_flex()
                    .gap_2()
                    .child(
                        Icon::new(IconName::TriangleAlert)
                            .with_size(Size::Size(px(16.)))
                            .text_color(yellow),
                    )
                    .child(
                        div()
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(yellow)
                            .child("Conflicts Detected"),
                    ),
            )
            .child(
                v_flex()
                    .gap_1()
                    .children(lines.into_iter().map(|(folder, message)| {
                        h_flex()
                            .text_sm()
                            .text_color(muted)
                            .child(div().font_weight(FontWeight::MEDIUM).child(folder))
                            .child(format!(": {message}"))
                    })),
            )
            .into_any_element(),
    )
}

// A card button's click handler: it calls back into the page through a weak handle with a
// clone of the card's entry, so a click after the page is gone does nothing.
fn on_card(
    page: &WeakEntity<SoundsPage>,
    sound: &SoundEntry,
    act: impl Fn(&mut SoundsPage, &SoundEntry, &mut Window, &mut Context<SoundsPage>) + 'static,
) -> impl Fn(&ClickEvent, &mut Window, &mut App) + 'static {
    let (page, entry) = (page.clone(), sound.clone());
    move |_, window, cx| {
        let _ = page.update(cx, |page, cx| act(page, &entry, window, cx));
    }
}

// v5's library sound card: the name with its VPK badge, then play and delete at the bottom
// left and the outline Hit and Kill installs at the right.
fn sound_card(
    sound: &SoundEntry,
    playing: bool,
    busy: Option<&Busy>,
    page: &WeakEntity<SoundsPage>,
    cx: &App,
) -> AnyElement {
    let folder = sound.folder_name();
    let deleting = busy == Some(&Busy::Delete(folder.to_owned()));
    let install = |id: ElementId, kind: SoundKind, icon: IconName, label: &'static str| {
        let installing = busy == Some(&Busy::Install(folder.to_owned(), kind));
        Button::new(id)
            .outline()
            .px(SM_ICON_PX)
            .icon(icon)
            .label(label)
            .loading(installing)
            .disabled(busy.is_some() && !installing)
            .on_click(on_card(page, sound, move |page, entry, window, cx| {
                page.install(entry, kind, window, cx)
            }))
    };
    let vpk = (sound.format() == SoundFormat::Vpk).then(|| {
        badge_of(
            ids::sound_vpk(folder),
            Tag::secondary(),
            transparent_black(),
            "VPK",
        )
    });
    card_frame(cx)
        .id(ids::sound_card(folder))
        .accessibility_id(ids::sound_card(folder).to_string())
        .aria_label(sound.name().to_owned())
        .test_support()
        .size_full()
        .flex()
        .flex_col()
        .overflow_hidden()
        .child(
            h_flex()
                .px_6()
                .pt_6()
                .gap_2()
                .child(
                    div()
                        .min_w_0()
                        .truncate()
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(sound.name().to_owned()),
                )
                .children(vpk),
        )
        .child(
            h_flex()
                .mt_auto()
                .px_6()
                .pb_6()
                .justify_between()
                .child(
                    h_flex()
                        .gap_1()
                        .child(
                            Button::new(ids::sound_play(folder))
                                .ghost()
                                .px(SM_ICON_PX)
                                .icon(if playing {
                                    IconName::Square
                                } else {
                                    IconName::Play
                                })
                                .named(if playing { "Stop" } else { "Play" })
                                .on_click(on_card(page, sound, SoundsPage::play_sound)),
                        )
                        .child(
                            Button::new(ids::sound_delete(folder))
                                .ghost()
                                .px(SM_ICON_PX)
                                .icon(IconName::Trash)
                                .named("Delete from library")
                                .loading(deleting)
                                .disabled(busy.is_some() && !deleting)
                                .on_click(on_card(page, sound, SoundsPage::delete)),
                        ),
                )
                .child(
                    h_flex()
                        .gap_1()
                        .child(install(
                            ids::sound_hit(folder),
                            SoundKind::Hitsound,
                            IconName::Crosshair,
                            "Hit",
                        ))
                        .child(install(
                            ids::sound_kill(folder),
                            SoundKind::Killsound,
                            IconName::Target,
                            "Kill",
                        )),
                ),
        )
        .into_any_element()
}

// v5's library empty state: a faint speaker and a muted line, plus a hint when nothing is
// filtering.
fn empty_library(searching: bool, cx: &App) -> AnyElement {
    let muted = cx.theme().muted_foreground;
    let (id, title) = if searching {
        (ids::SOUNDS_NO_MATCH, "No sounds match your search")
    } else {
        (ids::SOUNDS_EMPTY, "No sounds in your library yet")
    };
    v_flex()
        .items_center()
        .justify_center()
        .py_12()
        .child(
            Icon::new(IconName::Volume2)
                .with_size(Size::Size(px(48.)))
                .text_color(muted.opacity(0.3))
                .mb_4(),
        )
        .child(labelled(div().text_color(muted), id, title))
        .children((!searching).then(|| {
            div()
                .mt_1()
                .text_sm()
                .text_color(muted)
                .child("Click \"Add Sound\" to import a WAV or VPK file")
        }))
        .into_any_element()
}

impl SoundsPage {
    // "Your Library", a full-width search, then the grid of sounds whose name holds the search
    // text. The filter is read from the input on every render, so a reload keeps it.
    fn library(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let query = self.search.read(cx).value().to_lowercase();
        let shown: Vec<usize> = (0..self.sounds.len())
            .filter(|&ix| self.lowered[ix].contains(&query))
            .collect();
        let body = if !self.loaded {
            None
        } else if shown.is_empty() {
            Some(empty_library(!query.is_empty(), cx))
        } else {
            let playing = self
                .player
                .read(cx)
                .now_playing()
                .map(|now| now.key.clone());
            let page = cx.entity().downgrade();
            let busy = self.busy.clone();
            let sounds = self.sounds.clone();
            Some(self.grid.render(shown.len(), px(136.), move |ix, _, cx| {
                let sound = &sounds[shown[ix]];
                let on = playing.as_ref() == Some(&sound_key(sound.folder_name()));
                sound_card(sound, on, busy.as_ref(), &page, cx)
            }))
        };
        let muted = cx.theme().muted_foreground;
        let importing = self.busy == Some(Busy::Import);
        v_flex()
            .flex_1()
            .min_h_0()
            .gap_4()
            .child(
                h_flex()
                    .justify_between()
                    .gap_4()
                    .child(
                        div()
                            .text_lg()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child("Your Library"),
                    )
                    .child(
                        Button::new(ids::SOUNDS_ADD)
                            .primary()
                            .h_9()
                            .px_3()
                            .icon(IconName::Plus)
                            .label(if importing {
                                "Importing..."
                            } else {
                                "Add Sound"
                            })
                            .loading(importing)
                            .disabled(self.busy.is_some() && !importing)
                            .on_click(cx.listener(|page, _, _, cx| page.add(cx))),
                    ),
            )
            .child(
                div().id(ids::SOUNDS_SEARCH).test_support().child(
                    Input::new(&self.search)
                        .h_9()
                        .prefix(Icon::new(IconName::Search).text_color(muted)),
                ),
            )
            .child(div().flex_1().min_h_0().children(body))
    }
}

impl Render for SoundsPage {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let slots = h_flex()
            .gap_4()
            .items_start()
            .child(self.slot_card(SoundKind::Hitsound, cx))
            .child(self.slot_card(SoundKind::Killsound, cx));
        let conflicts = self
            .installed
            .as_ref()
            .and_then(|installed| conflict_box(installed, cx));
        v_flex()
            .id(ids::page(Page::Sounds))
            .test_support()
            .relative()
            .size_full()
            .p(CONTENT_PADDING)
            .gap_6()
            .child(slots)
            .children(conflicts)
            .child(self.library(cx))
    }
}

/// v5's format texts in the log rows: the install row's lower-case tag, then the import row's
/// description. Core's own `as_str` is private.
fn format_texts(format: SoundFormat) -> (&'static str, &'static str) {
    match format {
        SoundFormat::Vpk => ("vpk", "VPK sound pack"),
        _ => ("wav", "16-bit PCM, 44100Hz, Stereo"),
    }
}
