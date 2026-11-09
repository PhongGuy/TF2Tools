//! The full-page profile editor: a header with Back, the name, Activate and Delete, the four
//! slot cards beside the picked slot's gallery, and a queue that saves each change in order.

use std::collections::{HashMap, VecDeque};
use std::ops::Range;
use std::rc::Rc;

use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::tag::Tag;
use gpui_kit::component::{ActiveTheme as _, Disableable as _, Icon, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, AnyWindowHandle, App, AppContext as _, Context, Entity, FontWeight,
    InteractiveElement as _, IntoElement, ParentElement as _, Pixels, Render, SharedString,
    StatefulInteractiveElement as _, Styled as _, Subscription, TestSupportExt as _,
    UniformListScrollHandle, WeakEntity, Window, div, px, transparent_black, uniform_list,
};
use tf2tools_core::{
    CrosshairEntry, HudEntry, LibraryError, MissingItem, Profile, ProfileError, ProfileId,
    ProfileUpdate, SoundEntry, SoundKind,
};

use super::ProfilesPage;
use super::editor_model::{Edit, Slot, SlotState, crosshair_labels, slot_state, slot_text};
use super::model;
use crate::audio::player::{NowPlaying, Player, sound_player};
use crate::ids;
use crate::library_events::{LibraryChanged, LibraryEvents};
use crate::local_messages::{describe_library, describe_profile};
use crate::pages::common::{SM_ICON_PX, badge_of, card_frame};
use crate::pages::huds::{card_images, hud_picture, hud_title};
use crate::pages::sounds::{play_button, sound_key, sound_title};
use crate::remote_images::{ImageKey, RemoteImages};
use crate::services::{OpError, Services, run_blocking, tf2_state};
use crate::shell::confirm::{Confirm, confirm};
use crate::shell::route::{NavigateTo, Page};
use crate::shell::toast::{ToastKind, toast, toast_error};
use crate::widgets::card_grid::CardGrid;
use crate::widgets::crosshair_picker::CrosshairPicker;
use crate::widgets::scroll::{BAR_WIDTH, scroll_area};
use crate::widgets::weapon_assigner::{
    AssignRequested, AssignedCell, AssignerState, ClearRequested, WeaponAssigner,
};
use crate::widgets::{Named as _, labelled};

// A HUD card's name row under its 16:9 picture.
const PICK_TITLE_HEIGHT: Pixels = px(40.);
// The slot column's width; the picker takes the rest.
const SLOT_COLUMN_WIDTH: Pixels = px(280.);
// One sound row with the gap under it; `uniform_list` sizes every row alike.
const SOUND_ROW_HEIGHT: Pixels = px(56.);
// The sound list shows at most this many rows before it scrolls.
const SOUND_MAX_ROWS: usize = 8;
// The HUD gallery shows at most this many rows before it scrolls.
const PICKER_MAX_ROWS: usize = 3;

enum Job {
    Edit(Edit),
    Refresh,
}

enum Done {
    Saved(Edit, Result<ProfileUpdate, ProfileError>),
    Refreshed(Result<(Profile, Vec<MissingItem>), ProfileError>),
}

/// The library the galleries pick from, with each HUD card's picture URL by folder.
struct Galleries {
    huds: Rc<[HudEntry]>,
    sounds: Rc<[SoundEntry]>,
    crosshairs: Rc<[CrosshairEntry]>,
    images: Rc<HashMap<String, String>>,
}

/// What Back, Activate or Delete does once the queue has saved a typed name.
#[derive(Clone, Copy)]
enum AfterIdle {
    Back,
    Activate,
    Delete,
}

/// One profile, edited in place of the Profiles grid.
pub struct ProfileEditor {
    page: WeakEntity<ProfilesPage>,
    win: AnyWindowHandle,
    saved: Profile,
    missing: Vec<MissingItem>,
    not_applied: bool,
    queue: VecDeque<Job>,
    running: bool,
    name: Entity<InputState>,
    /// The name the player asked for, when core refused the rename as taken.
    name_taken: Option<String>,
    /// The page's active profile as last seen, so only a change to it can clear `not_applied`.
    seen_active: Option<ProfileId>,
    /// The rename queued or running, so Enter then Blur sends it once.
    renaming: Option<String>,
    after_idle: Option<AfterIdle>,
    selected: Slot,
    search: Entity<InputState>,
    /// None until the first gallery load lands.
    galleries: Option<Galleries>,
    /// Bumped per gallery load, so a slower older load never replaces a newer one.
    gallery_generation: u64,
    grid: CardGrid,
    /// The HUD grid's last drawn range, as indices into the shown HUDs.
    visible: Range<usize>,
    /// This editor's own `RemoteImages` viewer, handed back on release.
    image_viewer: String,
    /// The app's floating player, which plays the previews.
    player: Entity<Player>,
    /// The player key of the last preview this editor started.
    preview_key: Option<SharedString>,
    sound_scroll: UniformListScrollHandle,
    crosshair_picker: Entity<CrosshairPicker>,
    /// Whether a crosshair was picked when the weapon list was last told.
    crosshair_picked: bool,
    assigner: Entity<WeaponAssigner>,
    _subscriptions: Vec<Subscription>,
}

impl ProfileEditor {
    /// Subscribes to the page (redraw) and to `LibraryEvents` (a Refresh); queues a Refresh.
    /// `active` is the page's active profile when the editor opens.
    pub fn new(
        profile: Profile,
        page: WeakEntity<ProfilesPage>,
        active: Option<ProfileId>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let initial = profile.name().to_owned();
        let name = cx.new(|cx| InputState::new(window, cx).default_value(initial));
        // Active, busy and the activating id live on the page, so its changes redraw the header.
        // Edits count as applied once the page's active profile changes to this one; a redraw
        // before the page's reload lands still shows the old active id.
        let mut subscriptions = Vec::new();
        if let Some(page) = page.upgrade() {
            subscriptions.push(cx.observe(&page, |editor, page, cx| {
                let active = page.read(cx).active();
                if active != editor.seen_active.as_ref() {
                    editor.seen_active = active.cloned();
                    if active == Some(editor.id()) {
                        editor.not_applied = false;
                    }
                }
                cx.notify();
            }));
        }
        // gpui-kit owns InputEvent; Enter and leaving the field save the name.
        #[allow(clippy::wildcard_enum_match_arm)]
        subscriptions.push(cx.subscribe_in(
            &name,
            window,
            |editor, _, event: &InputEvent, window, cx| match event {
                InputEvent::PressEnter { .. } | InputEvent::Blur => {
                    editor.commit_name(window, cx);
                }
                InputEvent::Change => {
                    editor.name_taken = None;
                    cx.notify();
                }
                _ => {}
            },
        ));
        let events = LibraryEvents::global(cx);
        subscriptions.push(cx.subscribe(&events, |editor, _, _: &LibraryChanged, cx| {
            editor.load_galleries(cx);
            editor.queue_refresh();
            editor.pump(cx);
        }));
        // The galleries filter from the search value at render; re-arm the grid so a new filter
        // asks for its cards' pictures.
        let search = cx.new(|cx| InputState::new(window, cx).placeholder("Search HUDs..."));
        subscriptions.push(cx.subscribe(&search, |editor, _, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Change) {
                editor.grid.rearm();
                cx.notify();
            }
        }));
        // Pictures redraw the editor as they land; its wants go when it does.
        let image_viewer = format!("profile-editor:{}", cx.entity_id());
        if let Some(images) = RemoteImages::global(cx) {
            subscriptions.push(cx.observe(&images, |_, _, cx| cx.notify()));
            let released = image_viewer.clone();
            subscriptions.push(cx.on_release(move |_, cx| {
                images.update(cx, |images, cx| images.want(&released, Vec::new(), cx));
            }));
        }
        // The play icons read the player's sound, so redraw when it starts or ends.
        let player = sound_player(window, cx);
        subscriptions.push(cx.observe(&player, |_, _, cx| cx.notify()));
        // The weapon list waits for a picked crosshair; its clicks and clears save at once.
        let crosshair_picker = cx.new(|cx| CrosshairPicker::new(true, window, cx));
        let assigner = cx.new(|cx| {
            let mut assigner = WeaponAssigner::new(true, cx);
            assigner.set_clearable(true, cx);
            assigner
        });
        subscriptions.push(cx.observe(&crosshair_picker, |editor, picker, cx| {
            let picked = picker.read(cx).selected().is_some();
            if picked != editor.crosshair_picked {
                editor.crosshair_picked = picked;
                editor.sync_assigner(cx);
            }
        }));
        subscriptions.push(
            cx.subscribe(&assigner, |editor, _, event: &AssignRequested, cx| {
                let Some(crosshair) = editor.crosshair_picker.read(cx).selected().cloned() else {
                    return;
                };
                let scripts = event.scripts.iter().map(|s| (*s).to_owned()).collect();
                editor.enqueue(Edit::Assign { scripts, crosshair }, cx);
            }),
        );
        subscriptions.push(
            cx.subscribe(&assigner, |editor, _, event: &ClearRequested, cx| {
                editor.enqueue(Edit::ClearWeapon(event.script.to_owned()), cx)
            }),
        );
        let mut grid = CardGrid::new(ids::PROFILE_PICKER_GRID);
        let this = cx.entity().downgrade();
        grid.on_visible(move |range, cx| {
            let _ = this.update(cx, |editor, cx| {
                editor.visible = range;
                editor.want_images(cx);
            });
        });
        let mut editor = Self {
            page,
            win: window.window_handle(),
            saved: profile,
            missing: Vec::new(),
            not_applied: false,
            queue: VecDeque::new(),
            running: false,
            name,
            name_taken: None,
            seen_active: active,
            renaming: None,
            after_idle: None,
            selected: Slot::Hud,
            search,
            galleries: None,
            gallery_generation: 0,
            grid,
            visible: 0..0,
            image_viewer,
            player,
            preview_key: None,
            sound_scroll: UniformListScrollHandle::new(),
            crosshair_picker,
            crosshair_picked: false,
            assigner,
            _subscriptions: subscriptions,
        };
        editor.relabel(cx);
        editor.load_galleries(cx);
        editor.queue_refresh();
        editor.pump(cx);
        editor
    }

    /// The slot whose picker shows; HUD on open.
    pub fn selected(&self) -> Slot {
        self.selected
    }

    /// Shows `slot`'s picker; a change of slot clears the search.
    pub fn select(&mut self, slot: Slot, cx: &mut Context<Self>) {
        if slot == self.selected {
            return;
        }
        self.selected = slot;
        // Only a preview the editor started stops; a sound from elsewhere plays on.
        let ours = self.preview_key.take();
        self.player.update(cx, |player, cx| {
            if ours.is_some() && player.now_playing().map(|n| &n.key) == ours.as_ref() {
                player.stop(cx);
            }
        });
        let (win, search) = (self.win, self.search.clone());
        let placeholder = match slot {
            Slot::Hud => "Search HUDs...",
            Slot::Hitsound | Slot::Killsound => "Search sounds...",
            Slot::Crosshairs => "Search crosshairs...",
        };
        // Setting an input's text needs the window, which a click handler's caller holds.
        cx.defer(move |cx| {
            let _ = win.update(cx, |_, window, cx| {
                search.update(cx, |input, cx| {
                    input.set_value("", window, cx);
                    input.set_placeholder(placeholder, window, cx);
                });
            });
        });
        self.grid.rearm();
        cx.notify();
    }

    /// The picker's search field (one field, cleared on a slot change).
    pub fn search_input(&self) -> &Entity<InputState> {
        &self.search
    }

    // Reads the HUDs, sounds and HUD pictures off the UI thread; outside the queue, since the
    // galleries never touch `saved`.
    fn load_galleries(&mut self, cx: &mut Context<Self>) {
        self.gallery_generation += 1;
        let generation = self.gallery_generation;
        let loading = run_blocking(cx, |s| {
            let failed = |e: LibraryError| describe_library(&e).into_op("Failed to load library");
            let huds = s.library.list_huds().map_err(failed)?;
            let sounds = s.library.list_sounds().map_err(failed)?;
            let crosshairs = s.library.list_crosshairs().map_err(failed)?;
            let images = card_images(s, &huds);
            Ok((huds, sounds, crosshairs, images))
        });
        let win = self.win;
        cx.spawn(async move |editor, cx| {
            let loaded = loading.await;
            let _ = cx.update(|cx| {
                win.update(cx, |_, window, cx| {
                    let _ = editor.update(cx, |editor, cx| {
                        if editor.gallery_generation != generation {
                            return;
                        }
                        match loaded {
                            Ok((huds, sounds, crosshairs, images)) => {
                                let crosshairs: Rc<[CrosshairEntry]> = crosshairs.into();
                                editor.crosshair_picker.update(cx, |picker, cx| {
                                    picker.set_library(Rc::clone(&crosshairs), cx)
                                });
                                editor.galleries = Some(Galleries {
                                    huds: huds.into(),
                                    sounds: sounds.into(),
                                    crosshairs,
                                    images: Rc::new(images),
                                });
                                editor.relabel(cx);
                            }
                            Err(err) => toast_error(window, cx, &err),
                        }
                        editor.grid.rearm();
                        cx.notify();
                    });
                })
            });
        })
        .detach();
    }

    /// The Crosshairs slot's compact picker.
    pub fn picker(&self) -> &Entity<CrosshairPicker> {
        &self.crosshair_picker
    }

    /// The Crosshairs slot's compact, clearable weapon list.
    pub fn assigner(&self) -> &Entity<WeaponAssigner> {
        &self.assigner
    }

    // Shows each saved crosshair on its weapon row, by name or "(missing)". Waits for the
    // library, or every library crosshair would read "(missing)" until it lands.
    fn relabel(&self, cx: &mut Context<Self>) {
        let Some(galleries) = &self.galleries else {
            return;
        };
        let library = &galleries.crosshairs[..];
        let builtins = &cx.global::<Services>().builtins;
        let cells = crosshair_labels(&self.saved, builtins, library)
            .into_iter()
            .map(|(script, label)| AssignedCell {
                script,
                label,
                crosshair: None,
            })
            .collect();
        self.assigner
            .update(cx, |assigner, cx| assigner.set_assigned(cells, cx));
    }

    // The weapon rows and bulk buttons act only while a crosshair is picked.
    fn sync_assigner(&self, cx: &mut Context<Self>) {
        let state = AssignerState {
            has_selection: self.crosshair_picked,
            busy: false,
            running: None,
        };
        self.assigner
            .update(cx, |assigner, cx| assigner.set_state(state, cx));
    }

    // The library HUDs whose name holds the search text.
    fn shown_huds(&self, cx: &App) -> Vec<HudEntry> {
        let query = self.search.read(cx).value();
        self.galleries
            .iter()
            .flat_map(|g| g.huds.iter())
            .filter(|hud| model::matches(hud.name(), &query))
            .cloned()
            .collect()
    }

    // The library sounds whose name holds the search text.
    fn shown_sounds(&self, cx: &App) -> Vec<SoundEntry> {
        let query = self.search.read(cx).value();
        self.galleries
            .iter()
            .flat_map(|g| g.sounds.iter())
            .filter(|sound| model::matches(sound.name(), &query))
            .cloned()
            .collect()
    }

    // Plays the library sound in `folder`, or stops it when it is the one playing. The bytes are
    // found by folder, so a slot card can play before the galleries load.
    fn preview(&mut self, folder: String, name: String, cx: &mut Context<Self>) {
        let subtitle = self
            .galleries
            .iter()
            .flat_map(|g| g.sounds.iter())
            .find(|x| x.folder_name() == folder)
            .map(|x| x.original_filename().to_owned())
            .unwrap_or_default();
        let now = NowPlaying {
            key: sound_key(&folder),
            name: name.into(),
            subtitle: subtitle.into(),
        };
        self.preview_key = Some(now.key.clone());
        self.player
            .update(cx, |player, cx| {
                player.toggle(
                    now,
                    move |s| {
                        let failed =
                            |e: LibraryError| describe_library(&e).into_op("Failed to play sound");
                        let sounds = s.library.list_sounds().map_err(failed)?;
                        let sound = sounds
                            .iter()
                            .find(|x| x.folder_name() == folder)
                            .ok_or_else(|| {
                                OpError::core(
                                    "Failed to play sound",
                                    "That sound is no longer in your library",
                                )
                            })?;
                        s.library.sound_bytes(sound.id()).map_err(failed)
                    },
                    cx,
                )
            })
            .detach();
    }

    fn is_playing(&self, folder: &str, cx: &App) -> bool {
        self.player
            .read(cx)
            .now_playing()
            .is_some_and(|now| now.key == sound_key(folder))
    }

    // Asks for the pictures of the HUD cards in the grid's last drawn range.
    fn want_images(&mut self, cx: &mut Context<Self>) {
        let (Some(images), Some(galleries)) = (RemoteImages::global(cx), self.galleries.as_ref())
        else {
            return;
        };
        let shown = self.shown_huds(cx);
        let end = self.visible.end.min(shown.len());
        let keys: Vec<ImageKey> = shown[self.visible.start.min(end)..end]
            .iter()
            .filter_map(|hud| galleries.images.get(hud.folder_name()))
            .map(|url| ImageKey::card(url))
            .collect();
        images.update(cx, |images, cx| images.want(&self.image_viewer, keys, cx));
    }

    pub fn id(&self) -> &ProfileId {
        self.saved.id()
    }

    /// The profile as last saved or reloaded.
    pub fn saved(&self) -> &Profile {
        &self.saved
    }

    pub fn name_input(&self) -> &Entity<InputState> {
        &self.name
    }

    /// Focuses the name field and selects its text.
    pub fn focus_name(&self, window: &mut Window, cx: &mut Context<Self>) {
        self.name.update(cx, |input, cx| {
            input.focus(window, cx);
            input.select_all(window, cx);
        });
    }

    /// An edit ended this profile's active state and it has not been active since.
    pub fn not_applied(&self) -> bool {
        self.not_applied
    }

    pub fn missing(&self) -> &[MissingItem] {
        &self.missing
    }

    /// No job queued or running.
    pub fn is_idle(&self) -> bool {
        self.queue.is_empty() && !self.running
    }

    /// Queues a change; jobs run one at a time, in order.
    pub fn enqueue(&mut self, edit: Edit, cx: &mut Context<Self>) {
        self.queue.push_back(Job::Edit(edit));
        self.pump(cx);
    }

    // A Refresh already last in the queue will read everything this one would.
    fn queue_refresh(&mut self) {
        if !matches!(self.queue.back(), Some(Job::Refresh)) {
            self.queue.push_back(Job::Refresh);
        }
    }

    // Starts the next job unless one runs. An edit's draft is built here, when it starts, from
    // `saved`, which the job before it has already replaced, so a queued click never drops an
    // earlier one. Refresh reads the missing marks first because that read may rewrite a folder.
    fn pump(&mut self, cx: &mut Context<Self>) {
        if self.running {
            return;
        }
        let Some(job) = self.queue.pop_front() else {
            cx.notify();
            return;
        };
        self.running = true;
        let id = self.saved.id().clone();
        let task = match job {
            Job::Edit(edit) => {
                let draft = edit.draft(&self.saved);
                run_blocking(cx, move |s| {
                    Ok(Done::Saved(edit, s.library.update_profile(&id, &draft)))
                })
            }
            Job::Refresh => run_blocking(cx, move |s| {
                let read = || -> Result<_, ProfileError> {
                    let missing = s.library.missing_items(&id)?;
                    Ok((s.library.profile(&id)?, missing))
                };
                Ok(Done::Refreshed(read()))
            }),
        };
        let win = self.win;
        cx.spawn(async move |editor, cx| {
            let done = task.await;
            let _ = cx.update(|cx| {
                win.update(cx, |_, window, cx| {
                    let _ = editor.update(cx, |editor, cx| {
                        editor.running = false;
                        editor.finish(done, window, cx);
                        editor.pump(cx);
                        if editor.is_idle() {
                            editor.run_after_idle(window, cx);
                        }
                    });
                })
            });
        })
        .detach();
    }

    fn finish(&mut self, done: Result<Done, OpError>, window: &mut Window, cx: &mut Context<Self>) {
        let failed = |e: &ProfileError| describe_profile(e).into_op("Failed to save profile");
        match done {
            Ok(Done::Saved(edit, Ok(update))) => {
                self.saved = update.profile().clone();
                // A rename leaves the slots alone; any other save may fix or break a missing mark.
                if let Edit::Rename(name) = &edit {
                    self.renamed(name, window, cx);
                } else {
                    self.queue_refresh();
                }
                if update.deactivated() {
                    self.not_applied = true;
                }
                let _ = self.page.update(cx, |page, cx| page.reload(cx));
            }
            Ok(Done::Saved(_, Err(e @ ProfileError::NotFound { .. }))) => {
                self.gone(Some(failed(&e)), window, cx);
            }
            Ok(Done::Refreshed(Err(ProfileError::NotFound { .. }))) => self.gone(None, window, cx),
            // The page's list was stale: say it as the typed check would, and stay put.
            Ok(Done::Saved(Edit::Rename(sent), Err(ProfileError::NameTaken { name, .. }))) => {
                self.rename_done(&sent);
                self.name_taken = Some(name);
                self.after_idle = None;
                self.queue_refresh();
            }
            // The picked item left the library since the gallery loaded.
            Ok(Done::Saved(Edit::SetHud(_), Err(ProfileError::ItemNotFound { .. }))) => {
                self.item_gone("That HUD is no longer in your library", window, cx);
            }
            Ok(Done::Saved(Edit::SetSound(..), Err(ProfileError::ItemNotFound { .. }))) => {
                self.item_gone("That sound is no longer in your library", window, cx);
            }
            Ok(Done::Saved(Edit::Assign { .. }, Err(ProfileError::ItemNotFound { .. }))) => {
                self.item_gone("That crosshair is no longer in your library", window, cx);
            }
            Ok(Done::Saved(edit, Err(e))) => {
                if let Edit::Rename(sent) = &edit {
                    self.rename_done(sent);
                }
                toast_error(window, cx, &failed(&e));
                self.queue_refresh();
            }
            Ok(Done::Refreshed(Ok((profile, missing)))) => {
                self.saved = profile;
                self.missing = missing;
            }
            Ok(Done::Refreshed(Err(e))) => {
                toast_error(
                    window,
                    cx,
                    &describe_profile(&e).into_op("Failed to load profile"),
                );
            }
            Err(err) => toast_error(window, cx, &err),
        }
        self.relabel(cx);
        cx.notify();
    }

    // The profile no longer exists: drop the rest of the queue and, while the page still shows
    // this editor, toast `err` and return to the grid. After the editor's own Delete the page has
    // already toasted and is closing it, so nothing more is said.
    fn gone(&mut self, err: Option<OpError>, window: &mut Window, cx: &mut Context<Self>) {
        self.queue.clear();
        self.after_idle = None;
        let me = cx.entity();
        let Some(page) = self.page.upgrade() else {
            return;
        };
        if page.read(cx).editor().as_ref() != Some(&me) {
            return;
        }
        if let Some(err) = err {
            toast_error(window, cx, &err);
        }
        page.update(cx, |page, cx| page.close_editor(cx));
    }

    // The picked item is gone: say so, and reload the galleries and the missing marks.
    fn item_gone(&mut self, title: &str, window: &mut Window, cx: &mut Context<Self>) {
        toast(window, cx, ToastKind::Error, title, "");
        self.load_galleries(cx);
        self.queue_refresh();
    }

    // A finished rename stops guarding against a repeat only when it is the one last queued;
    // a later name typed meanwhile is still on its way.
    fn rename_done(&mut self, sent: &str) {
        if self.renaming.as_deref() == Some(sent) {
            self.renaming = None;
        }
    }

    // A saved rename: clear the taken line and show the trimmed name, unless the player has
    // typed something else since.
    fn renamed(&mut self, sent: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.rename_done(sent);
        self.name_taken = None;
        let name = self.saved.name().to_owned();
        if self.name.read(cx).value().trim() == name {
            self.name
                .update(cx, |input, cx| input.set_value(name, window, cx));
        }
    }

    // The typed name, trimmed: blank puts the saved name back, a name another loaded profile
    // holds shows the taken line, anything new is queued as a rename. False when refused as taken.
    fn commit_name(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let text = self.name.read(cx).value().trim().to_owned();
        if text.is_empty() {
            let name = self.saved.name().to_owned();
            self.name
                .update(cx, |input, cx| input.set_value(name, window, cx));
            return true;
        }
        if text == self.saved.name() && self.renaming.is_none()
            || self.renaming.as_deref() == Some(text.as_str())
        {
            return true;
        }
        if self.taken_name(cx).is_some() {
            cx.notify();
            return false;
        }
        self.renaming = Some(text.clone());
        self.enqueue(Edit::Rename(text), cx);
        true
    }

    // The other profile holding the typed name: core's answer, or the page's loaded list.
    fn taken_name(&self, cx: &Context<Self>) -> Option<String> {
        self.name_taken.clone().or_else(|| {
            let page = self.page.upgrade()?;
            page.read(cx)
                .taken_name(&self.name.read(cx).value(), Some(self.id()))
        })
    }

    // A kit Button never blurs the name field, so Back, Activate and Delete save a typed name
    // first and act once the queue is idle; a taken name stops them, as core's NameTaken does.
    fn leave(&mut self, then: AfterIdle, window: &mut Window, cx: &mut Context<Self>) {
        if !self.commit_name(window, cx) {
            return;
        }
        self.after_idle = Some(then);
        if self.is_idle() {
            self.run_after_idle(window, cx);
        }
    }

    fn run_after_idle(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(then) = self.after_idle.take() else {
            return;
        };
        let (id, name) = (self.id().clone(), self.saved.name().to_owned());
        let _ = self.page.update(cx, |page, cx| match then {
            AfterIdle::Back => page.close_editor(cx),
            AfterIdle::Activate => page.activate(id, window, cx),
            AfterIdle::Delete => page.delete(id, &name, window, cx),
        });
    }

    // Back, the name field, then Activate (or the Active badge) and Delete.
    fn header(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let has_tf2 = tf2_state(cx).read(cx).install().is_some();
        let page = self.page.upgrade();
        let page = page.as_ref().map(|p| p.read(cx));
        let id = self.id();
        let active = page.is_some_and(|p| p.active() == Some(id));
        let busy = page.is_some_and(|p| p.is_busy());
        let activating = page.is_some_and(|p| p.activating() == Some(id));
        let not_applied = self.not_applied && !active;
        let taken = self.taken_name(cx);
        let status = if active {
            badge_of(
                ids::PROFILE_EDITOR_ACTIVE_BADGE,
                Tag::primary(),
                transparent_black(),
                "Active",
            )
            .into_any_element()
        } else {
            let button = Button::new(ids::PROFILE_EDITOR_ACTIVATE);
            if not_applied {
                button.primary()
            } else {
                button.outline()
            }
            .px(SM_ICON_PX)
            .icon(IconName::Play)
            .label(if activating {
                "Activating..."
            } else {
                "Activate"
            })
            .loading(activating)
            .disabled(!has_tf2 || busy)
            .on_click(
                cx.listener(|editor, _, window, cx| editor.leave(AfterIdle::Activate, window, cx)),
            )
            .into_any_element()
        };
        h_flex()
            .gap_3()
            .child(
                Button::new(ids::PROFILE_EDITOR_BACK)
                    .ghost()
                    .icon(IconName::ArrowLeft)
                    .named("Back to profiles")
                    .on_click(cx.listener(|editor, _, window, cx| {
                        editor.leave(AfterIdle::Back, window, cx)
                    })),
            )
            .child(
                v_flex()
                    .flex_1()
                    .gap_1()
                    .child(
                        div()
                            .id(ids::PROFILE_EDITOR_NAME)
                            .test_support()
                            .child(Input::new(&self.name)),
                    )
                    .children(taken.map(|name| {
                        labelled(
                            div().text_xs().text_color(cx.theme().danger),
                            ids::PROFILE_EDITOR_NAME_TAKEN,
                            model::taken_message(&name),
                        )
                    }))
                    .children(not_applied.then(|| {
                        labelled(
                            div().text_xs().text_color(cx.theme().muted_foreground),
                            ids::PROFILE_EDITOR_NOT_APPLIED,
                            "Changes not applied yet",
                        )
                    })),
            )
            .child(status)
            .child(
                Button::new(ids::PROFILE_EDITOR_DELETE)
                    .outline()
                    .icon(IconName::Trash)
                    .named("Delete profile")
                    .disabled(busy)
                    .on_click(cx.listener(|editor, _, window, cx| {
                        editor.leave(AfterIdle::Delete, window, cx)
                    })),
            )
    }
}

// The slot cards and the pickers.
impl ProfileEditor {
    // One card per slot: its label and what it holds, or the missing item's name, and Clear
    // while it holds anything. A click on the card shows its picker.
    fn slot_card(&self, slot: Slot, cx: &mut Context<Self>) -> AnyElement {
        let key = slot.key();
        let state = slot_state(&self.saved, slot, &self.missing);
        let text = slot_text(&state, slot);
        let theme = cx.theme();
        let selected = slot == self.selected;
        let value = match &state {
            SlotState::Missing(_) => labelled(
                div().text_sm().text_color(theme.danger),
                ids::profile_slot_missing(key),
                text.clone(),
            )
            .into_any_element(),
            SlotState::Empty | SlotState::Set(_) => div()
                .text_sm()
                .truncate()
                .text_color(theme.muted_foreground)
                .child(text.clone())
                .into_any_element(),
        };
        // A sound slot plays its saved sound; a missing one has nothing to play.
        let item = match slot {
            Slot::Hitsound => self.saved.hitsound(),
            Slot::Killsound => self.saved.killsound(),
            Slot::Hud | Slot::Crosshairs => None,
        };
        let play = item
            .filter(|_| matches!(state, SlotState::Set(_)))
            .map(|item| {
                let (folder, name) = (item.folder_name().to_owned(), item.name().to_owned());
                play_button(ids::profile_slot_play(key), self.is_playing(&folder, cx)).on_click(
                    cx.listener(move |editor, _, _, cx| {
                        cx.stop_propagation();
                        editor.preview(folder.clone(), name.clone(), cx);
                    }),
                )
            });
        let clear = (state != SlotState::Empty).then(|| {
            Button::new(ids::profile_slot_clear(key))
                .ghost()
                .px(SM_ICON_PX)
                .icon(IconName::X)
                .named("Clear")
                .on_click(cx.listener(move |editor, _, window, cx| {
                    cx.stop_propagation();
                    if slot == Slot::Crosshairs {
                        editor.clear_crosshairs(window, cx);
                    } else {
                        editor.enqueue(Edit::ClearSlot(slot), cx);
                    }
                }))
        });
        card_frame(cx)
            .id(ids::profile_slot(key))
            .aria_label(text)
            .aria_selected(selected)
            .test_support()
            .when(selected, |card| card.border_color(cx.theme().primary))
            .cursor_pointer()
            .p_4()
            .child(
                h_flex()
                    .gap_2()
                    .child(
                        v_flex()
                            .flex_1()
                            .min_w_0()
                            .gap_1()
                            .child(div().font_weight(FontWeight::SEMIBOLD).child(slot.label()))
                            .child(value),
                    )
                    .children(play)
                    .children(clear),
            )
            .on_click(cx.listener(move |editor, _, _, cx| editor.select(slot, cx)))
            .into_any_element()
    }

    // Every crosshair in a profile goes at once, so Clear asks first.
    fn clear_crosshairs(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let answer = confirm(Confirm::clear_profile_crosshairs(), window, cx);
        cx.spawn(async move |editor, cx| {
            if answer.recv().await.unwrap_or(false) {
                let _ = editor.update(cx, |editor, cx| editor.enqueue(Edit::ClearCrosshairs, cx));
            }
        })
        .detach();
    }

    // The picked slot's search over its gallery, "No matches" when it filters out everything,
    // or a pointer to the store while the library has nothing for it. Crosshairs show their own picker beside the weapon list; builtins are
    // always listed, so they have no empty state.
    fn picker_column(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let slot = self.selected;
        let key = slot.key();
        if slot == Slot::Crosshairs {
            return Some(
                h_flex()
                    .id(ids::profile_picker(key))
                    .test_support()
                    .flex_1()
                    .min_w_0()
                    .items_start()
                    .gap_4()
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .child(self.crosshair_picker.clone()),
                    )
                    .child(div().flex_1().min_w_0().child(self.assigner.clone()))
                    .into_any_element(),
            );
        }
        let galleries = self.galleries.as_ref()?;
        let (empty, no_match, empty_text, store_text, store) = match slot {
            Slot::Hud => (
                galleries.huds.is_empty(),
                self.shown_huds(cx).is_empty(),
                "No HUDs in your library yet",
                "Browse the HUD Store",
                Page::HudStore,
            ),
            Slot::Hitsound | Slot::Killsound => (
                galleries.sounds.is_empty(),
                self.shown_sounds(cx).is_empty(),
                "No sounds in your library yet",
                "Browse the Sound Store",
                Page::SoundStore,
            ),
            Slot::Crosshairs => return None,
        };
        let body = if empty {
            v_flex()
                .items_center()
                .gap_3()
                .py_12()
                .child(labelled(
                    div().text_color(cx.theme().muted_foreground),
                    ids::profile_picker_empty(key),
                    empty_text,
                ))
                .child(
                    Button::new(ids::profile_picker_store(key))
                        .outline()
                        .icon(IconName::Store)
                        .label(store_text)
                        .on_click(cx.listener(move |editor, _, _, cx| {
                            let _ = editor.page.update(cx, |_, cx| cx.emit(NavigateTo(store)));
                        })),
                )
                .into_any_element()
        } else if no_match {
            v_flex()
                .items_center()
                .py_12()
                .child(labelled(
                    div().text_color(cx.theme().muted_foreground),
                    ids::profile_picker_no_match(key),
                    "No matches",
                ))
                .into_any_element()
        } else if slot == Slot::Hud {
            self.hud_gallery(galleries, cx)
        } else {
            self.sound_gallery(slot, cx)
        };
        let muted = cx.theme().muted_foreground;
        Some(
            v_flex()
                .id(ids::profile_picker(key))
                .test_support()
                .flex_1()
                .min_w_0()
                .gap_4()
                .child(
                    div()
                        .id(ids::profile_picker_search(key))
                        .test_support()
                        .max_w(px(320.))
                        .child(
                            Input::new(&self.search)
                                .h_9()
                                .prefix(Icon::new(IconName::Search).text_color(muted)),
                        ),
                )
                .child(body)
                .into_any_element(),
        )
    }

    // The library HUDs as picture cards, the saved one marked; a click saves it to the slot.
    fn hud_gallery(&self, galleries: &Galleries, cx: &mut Context<Self>) -> AnyElement {
        let shown: Rc<[HudEntry]> = self.shown_huds(cx).into();
        let height = |width: Pixels| width * (9. / 16.) + PICK_TITLE_HEIGHT;
        let rows = shown
            .len()
            .div_ceil(self.grid.columns())
            .clamp(1, PICKER_MAX_ROWS);
        let grid_height = self.grid.row_height_for(height) * rows as f32;
        let saved = self.saved.hud().map(|h| h.folder_name().to_owned());
        let images = Rc::clone(&galleries.images);
        let editor = cx.entity().downgrade();
        let count = shown.len();
        div()
            .h(grid_height)
            .child(self.grid.render_sized(count, height, move |ix, _, cx| {
                let hud = &shown[ix];
                let folder = hud.folder_name();
                let url = images.get(folder).map(String::as_str);
                let id = hud.id().clone();
                let editor = editor.clone();
                card_frame(cx)
                    .id(ids::profile_pick_hud(folder))
                    .aria_label(hud.name().to_owned())
                    .aria_selected(saved.as_deref() == Some(folder))
                    .test_support()
                    .when(saved.as_deref() == Some(folder), |card| {
                        card.border_color(cx.theme().primary)
                    })
                    .size_full()
                    .overflow_hidden()
                    .cursor_pointer()
                    .child(hud_picture(
                        ids::hud_shot(folder),
                        ids::hud_image(folder),
                        url,
                        cx,
                    ))
                    .child(div().px_3().py_2().child(hud_title(hud)))
                    .on_click(move |_, _, cx| {
                        let _ = editor.update(cx, |editor, cx| {
                            editor.enqueue(Edit::SetHud(id.clone()), cx)
                        });
                    })
                    .into_any_element()
            }))
            .into_any_element()
    }
}

impl ProfileEditor {
    // The library sounds as a list capped at SOUND_MAX_ROWS, built only for the rows in view.
    fn sound_gallery(&self, slot: Slot, cx: &mut Context<Self>) -> AnyElement {
        let count = self.shown_sounds(cx).len();
        let tall = SOUND_ROW_HEIGHT * count.clamp(1, SOUND_MAX_ROWS) as f32;
        let list = uniform_list(
            ids::PROFILE_PICKER_SOUNDS,
            count,
            cx.processor(move |editor, range, _, cx| editor.sound_rows(slot, range, cx)),
        )
        .track_scroll(&self.sound_scroll)
        .test_support()
        .w_full()
        .h(tall)
        .pr(BAR_WIDTH);
        scroll_area(
            ids::PROFILE_PICKER_SOUNDS_SCROLLBAR,
            &self.sound_scroll,
            list,
        )
        .into_any_element()
    }

    // The sound rows in `range`, the saved one marked; a click on a row saves it to the slot,
    // its play button only previews.
    fn sound_rows(
        &mut self,
        slot: Slot,
        range: Range<usize>,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let (kind, saved) = match slot {
            Slot::Hitsound => (SoundKind::Hitsound, self.saved.hitsound()),
            Slot::Killsound => (SoundKind::Killsound, self.saved.killsound()),
            // Only the sound slots show this list.
            Slot::Hud | Slot::Crosshairs => return Vec::new(),
        };
        let saved = saved.map(|item| item.folder_name().to_owned());
        let shown = self.shown_sounds(cx);
        (shown.get(range).unwrap_or_default().iter())
            .map(|sound| {
                let folder = sound.folder_name().to_owned();
                let picked = saved.as_deref() == Some(folder.as_str());
                let id = sound.id().clone();
                let (play_folder, name) = (folder.clone(), sound.name().to_owned());
                let play = play_button(
                    ids::profile_pick_play(&folder),
                    self.is_playing(&folder, cx),
                )
                .on_click(cx.listener(move |editor, _, _, cx| {
                    cx.stop_propagation();
                    editor.preview(play_folder.clone(), name.clone(), cx);
                }));
                let row = card_frame(cx)
                    .id(ids::profile_pick_sound(slot.key(), &folder))
                    .aria_label(sound.name().to_owned())
                    .aria_selected(picked)
                    .test_support()
                    .when(picked, |row| row.border_color(cx.theme().primary))
                    .cursor_pointer()
                    .px_3()
                    .py_2()
                    .child(
                        h_flex()
                            .gap_2()
                            .child(div().flex_1().min_w_0().child(sound_title(sound)))
                            .child(play),
                    )
                    .on_click(cx.listener(move |editor, _, _, cx| {
                        editor.enqueue(Edit::SetSound(kind, id.clone()), cx)
                    }));
                div()
                    .h(SOUND_ROW_HEIGHT)
                    .pb_2()
                    .child(row)
                    .into_any_element()
            })
            .collect()
    }
}

impl Render for ProfileEditor {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let slots: Vec<AnyElement> = Slot::ALL
            .into_iter()
            .map(|slot| self.slot_card(slot, cx))
            .collect();
        v_flex()
            .id(ids::PROFILE_EDITOR)
            .test_support()
            .gap_6()
            .child(self.header(cx))
            .child(
                h_flex()
                    .items_start()
                    .gap_6()
                    .child(
                        v_flex()
                            .flex_none()
                            .w(SLOT_COLUMN_WIDTH)
                            .gap_3()
                            .children(slots),
                    )
                    .children(self.picker_column(cx)),
            )
    }
}
