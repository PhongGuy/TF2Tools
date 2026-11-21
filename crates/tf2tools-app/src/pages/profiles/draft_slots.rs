//! The draft editor's HUD, Sounds and Crosshairs Set bodies, and the library they pick from.

use std::collections::HashMap;
use std::rc::Rc;

use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::select::{Select, SelectEvent};
use gpui_kit::component::{ActiveTheme as _, Disableable as _, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, Context, Entity, InteractiveElement as _, IntoElement, ParentElement as _,
    SharedString, StatefulInteractiveElement as _, Styled as _, TestSupportExt as _, Window, div,
    px,
};
use tf2tools_core::{
    CrosshairEntry, CrosshairId, CrosshairRef, DraftCrosshair, DraftItem, HudEntry, HudId,
    LibraryError, ProfileCrosshairRef, SoundEntry, SoundId, SoundKind,
};

use super::draft_editor::{BoardRef, ProfileDraftEditor, SoundOption};
use super::editor_model::{Picked, SlotKind, WeaponPick};
use crate::audio::player::NowPlaying;
use crate::i18n::names::sound_kind_name;
use crate::i18n::{plural, tr};
use crate::ids::{self, IdScope};
use crate::library_events::LibraryEvents;
use crate::loadout::{BoardTarget, Loadout};
use crate::local_messages::describe_library;
use crate::pages::common::card_frame;
use crate::pages::crosshairs::{added_message, import_job};
use crate::pages::huds::{card_images, hud_picture};
use crate::pages::sounds::{sound_key, with_play_state};
use crate::picker::PickKind;
use crate::remote_images::{ImageKey, RemoteImages};
use crate::services::{OpError, Services, run_blocking};
use crate::shell::route::{NavigateTo, Page};
use crate::shell::toast::{ToastKind, toast, toast_error};
use crate::widgets::labelled;
use crate::widgets::loadout_board::{BOARD_COLUMNS, BoardBusy, BoardEvent, LoadoutBoard, OnBoard};

/// A HUD tile's width in the wrapping grid.
const HUD_TILE_WIDTH: f32 = 200.;

/// What the Set bodies pick from: the library HUDs with their card pictures by folder, the
/// library sounds by name, and the library crosshairs.
pub(super) struct Library {
    huds: Rc<[HudEntry]>,
    sounds: Rc<[SoundEntry]>,
    crosshairs: Rc<[CrosshairEntry]>,
    images: Rc<HashMap<String, String>>,
}

impl ProfileDraftEditor {
    /// Reads the library HUDs, their pictures and the sounds off the UI thread; a newer load
    /// replaces an older one that lands late.
    pub(super) fn load_library(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.library_generation += 1;
        let generation = self.library_generation;
        let loading = run_blocking(cx, |s| {
            let failed =
                |e: LibraryError| describe_library(&e).into_op(tr!("messages.action.load_library"));
            let huds = s.library.list_huds().map_err(failed)?;
            let mut sounds = s.library.list_sounds().map_err(failed)?;
            sounds.sort_by_key(|sound| sound.name().to_lowercase());
            let crosshairs = s.library.list_crosshairs().map_err(failed)?;
            let images = card_images(s, &huds);
            Ok((huds, sounds, crosshairs, images))
        });
        let win = window.window_handle();
        cx.spawn(async move |editor, cx| {
            let loaded = loading.await;
            let _ = cx.update(|cx| {
                win.update(cx, |_, window, cx| {
                    let _ = editor.update(cx, |e, cx| {
                        if e.library_generation != generation {
                            return;
                        }
                        match loaded {
                            Ok((huds, sounds, crosshairs, images)) => {
                                let crosshairs: Rc<[CrosshairEntry]> = crosshairs.into();
                                let listed = Rc::clone(&crosshairs);
                                e.picker
                                    .update(cx, |picker, cx| picker.set_library(listed, cx));
                                e.library = Some(Library {
                                    huds: huds.into(),
                                    sounds: sounds.into(),
                                    crosshairs,
                                    images: Rc::new(images),
                                });
                                e.sync_sound_selects(window, cx);
                                e.want_images(cx);
                            }
                            Err(err) => toast_error(window, cx, &err),
                        }
                        cx.notify();
                    });
                })
            });
        })
        .detach();
    }

    // Asks for every library HUD's card picture under this editor's viewer.
    fn want_images(&self, cx: &mut Context<Self>) {
        let (Some(images), Some(library)) = (RemoteImages::global(cx), &self.library) else {
            return;
        };
        let keys = (library.huds.iter())
            .filter_map(|hud| library.images.get(hud.folder_name()))
            .map(|url| ImageKey::card(url))
            .collect();
        images.update(cx, |images, cx| images.want(&self.image_viewer, keys, cx));
    }

    // A sound pick's entry: the library sound in its folder, or itself shown by name when the
    // library no longer has it.
    fn sound_option(&self, pick: Option<&Picked<SoundId>>) -> SoundOption {
        let Some(pick) = pick else {
            return tf2_default();
        };
        let entry = (self.library.iter())
            .flat_map(|l| l.sounds.iter())
            .find(|s| s.folder_name() == pick.folder);
        match entry {
            Some(entry) => library_option(entry),
            None => SoundOption {
                id: None,
                label: pick.name.clone().into(),
            },
        }
    }

    // The saved and current picks of `kind` the library no longer has, which keep an entry.
    fn missing_sounds(&self, kind: SoundKind) -> Vec<Picked<SoundId>> {
        let picks = [self.baseline.sound(kind), self.draft.sound(kind)];
        let mut missing: Vec<Picked<SoundId>> = Vec::new();
        for pick in picks.into_iter().flatten() {
            if self.sound_option(Some(pick)).id.is_none() && !missing.contains(pick) {
                missing.push(pick.clone());
            }
        }
        missing
    }

    // Fills both selects with TF2 default, the library sounds and any missing pick, showing
    // the draft's pick.
    pub(super) fn sync_sound_selects(&self, window: &mut Window, cx: &mut Context<Self>) {
        for kind in [SoundKind::Hitsound, SoundKind::Killsound] {
            let mut options = vec![tf2_default()];
            options.extend((self.library.iter()).flat_map(|l| l.sounds.iter().map(library_option)));
            let missing = self.missing_sounds(kind);
            options.extend(missing.iter().map(|p| self.sound_option(Some(p))));
            let shown = self.sound_option(self.draft.sound(kind));
            self.sound_select(kind).update(cx, |select, cx| {
                select.set_items(options, window, cx);
                select.set_selected_value(&shown, window, cx);
            });
        }
    }

    /// Takes a confirmed select entry into the draft's `kind` sound.
    pub(super) fn sound_confirmed(
        &mut self,
        kind: SoundKind,
        event: &SelectEvent<Vec<SoundOption>>,
        cx: &mut Context<Self>,
    ) {
        let SelectEvent::Confirm(Some(option)) = event else {
            return;
        };
        let pick = match &option.id {
            Some(id) => (self.library.iter())
                .flat_map(|l| l.sounds.iter())
                .find(|s| s.id() == id)
                .map(Picked::sound),
            None if *option == tf2_default() => None,
            // A missing pick's entry puts that pick back.
            None => (self.missing_sounds(kind).into_iter())
                .find(|p| p.name.as_str() == option.label.as_ref()),
        };
        self.edit(|d| d.set_sound(kind, pick), cx);
    }

    // Plays the library sound in `folder`, or stops it when it is the one loading or playing.
    fn preview(&mut self, folder: String, name: String, cx: &mut Context<Self>) {
        let subtitle = (self.library.iter())
            .flat_map(|l| l.sounds.iter())
            .find(|s| s.folder_name() == folder)
            .map(|s| s.original_filename().to_owned())
            .unwrap_or_default();
        let now = NowPlaying {
            key: sound_key(&folder),
            name: name.into(),
            subtitle: subtitle.into(),
        };
        self.player
            .update(cx, |player, cx| {
                player.toggle(
                    now,
                    move |s| {
                        let action = || tr!("messages.action.play_sound");
                        let failed = |e: LibraryError| describe_library(&e).into_op(action());
                        let sounds = s.library.list_sounds().map_err(failed)?;
                        let sound = (sounds.iter())
                            .find(|x| x.folder_name() == folder)
                            .ok_or_else(|| OpError::core(action(), tr!("messages.local.gone")))?;
                        s.library.sound_bytes(sound.id()).map_err(failed)
                    },
                    cx,
                )
            })
            .detach();
    }

    // The Set body of `slot`; the slots without a body yet draw nothing.
    pub(super) fn set_body(&self, slot: SlotKind, cx: &mut Context<Self>) -> AnyElement {
        match slot {
            SlotKind::Hud => self.hud_body(cx),
            SlotKind::Sounds => self.sounds_body(cx),
            SlotKind::Crosshairs => self.crosshairs_body(cx),
            SlotKind::Graphics | SlotKind::Addons => div().into_any_element(),
        }
    }

    // The library HUDs as a wrapping grid of picture tiles, the draft's pick marked, plus a
    // saved HUD the library no longer has; or the empty line with a link to the HUDs page.
    fn hud_body(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(library) = &self.library else {
            return div().into_any_element();
        };
        let in_library = |folder: &str| library.huds.iter().any(|h| h.folder_name() == folder);
        let kept = (self.baseline.hud.as_ref())
            .filter(|p| matches!(p.item, DraftItem::Keep(_)) && !in_library(&p.folder));
        if library.huds.is_empty() && kept.is_none() {
            return self.hud_empty(cx);
        }
        let mut tiles: Vec<AnyElement> = Vec::new();
        for hud in library.huds.iter() {
            let key = library
                .images
                .get(hud.folder_name())
                .map(|u| ImageKey::card(u));
            tiles.push(self.hud_tile(Picked::hud(hud), key, cx));
        }
        tiles.extend(kept.map(|kept| self.hud_tile(kept.clone(), None, cx)));
        h_flex()
            .flex_wrap()
            .gap_3()
            .children(tiles)
            .into_any_element()
    }

    // One HUD tile: its 16:9 picture over its name, bordered when picked; a click picks it.
    fn hud_tile(
        &self,
        pick: Picked<HudId>,
        key: Option<ImageKey>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let selected = self.draft.hud.as_ref() == Some(&pick);
        let folder = pick.folder.clone();
        let name = SharedString::from(pick.name.clone());
        let primary = cx.theme().primary;
        card_frame(cx)
            .id(ids::profile_pick_hud(&folder))
            .aria_label(name.clone())
            .aria_selected(selected)
            .test_support()
            .w(px(HUD_TILE_WIDTH))
            .overflow_hidden()
            .cursor_pointer()
            .when(selected, |tile| tile.border_color(primary))
            .child(hud_picture(
                ids::hud_shot(&folder),
                ids::hud_image(&folder),
                key.as_ref(),
                cx,
            ))
            .child(
                div()
                    .px_3()
                    .py_2()
                    .truncate()
                    .font_weight(gpui_kit::FontWeight::SEMIBOLD)
                    .child(name),
            )
            .on_click(cx.listener(move |editor, _, _, cx| {
                let pick = pick.clone();
                editor.edit(|d| d.hud = Some(pick), cx);
            }))
            .into_any_element()
    }

    // "No HUDs in your library yet." with a link to the HUDs page, asked through the page so
    // the leave guard sees it.
    fn hud_empty(&self, cx: &mut Context<Self>) -> AnyElement {
        let page = self.page.clone();
        h_flex()
            .flex_wrap()
            .items_center()
            .gap_3()
            .child(labelled(
                div().text_sm().text_color(cx.theme().muted_foreground),
                ids::PROFILE_HUD_EMPTY,
                tr!("profiles.editor.hud_empty"),
            ))
            .child(
                Button::new(ids::PROFILE_HUDS_LINK)
                    .outline()
                    .small()
                    .label(Page::Huds.label())
                    .on_click(move |_, _, cx| {
                        if let Some(page) = page.as_ref() {
                            let _ = page.update(cx, |_, cx| cx.emit(NavigateTo(Page::Huds)));
                        }
                    }),
            )
            .into_any_element()
    }

    // The Hitsound and Killsound rows: label, select and play button.
    fn sounds_body(&self, cx: &mut Context<Self>) -> AnyElement {
        if self.library.is_none() {
            return div().into_any_element();
        }
        let rows = [
            (
                SoundKind::Hitsound,
                ids::PROFILE_SOUND_HIT,
                ids::PROFILE_SOUND_HIT_PLAY,
                ids::PROFILE_SOUND_HIT_PLAY_SPINNER,
            ),
            (
                SoundKind::Killsound,
                ids::PROFILE_SOUND_KILL,
                ids::PROFILE_SOUND_KILL_PLAY,
                ids::PROFILE_SOUND_KILL_PLAY_SPINNER,
            ),
        ]
        .map(|(kind, select_id, play_id, spinner_id)| {
            let label = sound_kind_name(kind);
            h_flex()
                .flex_wrap()
                .items_center()
                .gap_3()
                .child(div().w(px(96.)).text_sm().child(label.clone()))
                .child(
                    div().id(select_id).test_support().w(px(260.)).child(
                        Select::new(self.sound_select(kind))
                            .small()
                            .disabled(self.busy)
                            .accessibility_label(label),
                    ),
                )
                .child(self.play_button(kind, play_id, spinner_id, cx))
        });
        v_flex().gap_3().children(rows).into_any_element()
    }

    // A row's play button for the draft's pick, reading Play, Cancel or Stop from the player;
    // off for TF2 default or a sound the library no longer has.
    fn play_button(
        &self,
        kind: SoundKind,
        id: &'static str,
        spinner: &'static str,
        cx: &mut Context<Self>,
    ) -> Button {
        let playable = (self.draft.sound(kind)).filter(|p| self.sound_option(Some(p)).id.is_some());
        let state = playable.and_then(|p| self.player.read(cx).state_of(&sound_key(&p.folder)));
        let target = playable.map(|p| (p.folder.clone(), p.name.clone()));
        with_play_state(Button::new(id).ghost(), spinner, state)
            .size(px(32.))
            .disabled(target.is_none())
            .on_click(cx.listener(move |editor, _, _, cx| {
                if let Some((folder, name)) = target.clone() {
                    editor.preview(folder, name, cx);
                }
            }))
    }
}

impl ProfileDraftEditor {
    /// What the board shows for `script`: None for an unassigned weapon and, for a library
    /// crosshair, until the library list has loaded.
    pub fn board_ref(&self, script: &str) -> Option<BoardRef> {
        let pick = self.draft.crosshairs.iter().find(|p| p.script == script)?;
        match &pick.shows {
            ProfileCrosshairRef::Builtin(name) => {
                Some(BoardRef::Shown(CrosshairRef::Builtin(name.clone())))
            }
            ProfileCrosshairRef::Library { folder_name } => {
                let library = self.library.as_ref()?;
                let entry = (library.crosshairs.iter()).find(|c| c.folder_name() == folder_name);
                Some(match entry {
                    Some(entry) => BoardRef::Shown(CrosshairRef::Library(entry.id().clone())),
                    None => BoardRef::Waiting(folder_name.clone()),
                })
            }
            _ => None,
        }
    }

    /// Writes the picked crosshair onto every weapon `target` covers, in the draft only.
    pub fn assign(&mut self, target: BoardTarget, cx: &mut Context<Self>) {
        let Some(crosshair) = self.picker.read(cx).selected().cloned() else {
            return;
        };
        let shows = match &crosshair {
            CrosshairRef::Builtin(name) => ProfileCrosshairRef::Builtin(name.clone()),
            CrosshairRef::Library(id) => {
                let entry = (self.library.iter())
                    .flat_map(|l| l.crosshairs.iter())
                    .find(|c| c.id() == id);
                let Some(entry) = entry else {
                    return;
                };
                ProfileCrosshairRef::Library {
                    folder_name: entry.folder_name().to_owned(),
                }
            }
            _ => return,
        };
        let scripts = target.scripts();
        self.edit(
            |d| {
                d.crosshairs
                    .retain(|p| !scripts.contains(&p.script.as_str()));
                d.crosshairs.extend(scripts.iter().map(|script| WeaponPick {
                    script: (*script).to_owned(),
                    shows: shows.clone(),
                    item: DraftCrosshair::Set {
                        weapon_script: (*script).to_owned(),
                        crosshair: crosshair.clone(),
                    },
                }));
                d.crosshairs.sort_by(|a, b| a.script.cmp(&b.script));
            },
            cx,
        );
    }

    /// The board's OnBoard target: Assign calls `assign`, every other event is ignored.
    pub fn board_event(&mut self, event: BoardEvent, _: &mut Window, cx: &mut Context<Self>) {
        if let BoardEvent::Assign(target) = event {
            self.assign(target, cx);
        }
    }

    // Picks an image, imports it as the Crosshairs page does, then reloads the library and
    // selects the new crosshair; a cancel only stops the spinner.
    pub(super) fn import_crosshair(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let picking = (cx.global::<Services>().picker).pick(PickKind::CrosshairImage, cx);
        self.picker
            .update(cx, |picker, cx| picker.set_importing(true, cx));
        let win = window.window_handle();
        cx.spawn(async move |editor, cx| {
            let imported = match picking.await {
                Some(path) => {
                    let importing = cx.update(|cx| run_blocking(cx, move |s| import_job(s, &path)));
                    Some(importing.await)
                }
                None => None,
            };
            let _ = cx.update(|cx| {
                win.update(cx, |_, window, cx| {
                    let _ = editor.update(cx, |e, cx| e.imported(imported, window, cx));
                })
            });
        })
        .detach();
    }

    // The import's answer: a new crosshair toasts, is selected and joins the library lists.
    fn imported(
        &mut self,
        imported: Option<Result<CrosshairEntry, OpError>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.picker
            .update(cx, |picker, cx| picker.set_importing(false, cx));
        match imported {
            Some(Ok(entry)) => {
                let title = tr!("crosshairs_page.toast.imported");
                toast(
                    window,
                    cx,
                    ToastKind::Success,
                    title,
                    SharedString::from(added_message(&entry)),
                );
                LibraryEvents::changed(cx);
                let picked = Some(CrosshairRef::Library(entry.id().clone()));
                self.picker
                    .update(cx, |picker, cx| picker.select(picked, cx));
                self.load_library(window, cx);
            }
            Some(Err(err)) => toast_error(window, cx, &err),
            None => {}
        }
    }

    // The strip, the selection line with Put on every weapon, then the board over the draft.
    fn crosshairs_body(&self, cx: &mut Context<Self>) -> AnyElement {
        let picker = self.picker.read(cx);
        let selected = picker.selected().cloned();
        // A weapon waiting on a deleted library crosshair is drawn as an unknown one, by folder.
        let shown: Vec<(String, CrosshairRef)> = (self.draft.crosshairs.iter())
            .filter_map(|p| {
                let crosshair = match self.board_ref(&p.script)? {
                    BoardRef::Shown(crosshair) => crosshair,
                    BoardRef::Waiting(folder) => CrosshairRef::Library(CrosshairId::new(folder)),
                };
                Some((p.script.clone(), crosshair))
            })
            .collect();
        let on_selected = (shown.iter())
            .filter(|(_, c)| Some(c) == selected.as_ref())
            .count();
        let name_of = |crosshair: &CrosshairRef| {
            picker
                .name_of(crosshair)
                .unwrap_or_else(|| match crosshair {
                    CrosshairRef::Builtin(name) => name.clone(),
                    CrosshairRef::Library(id) => id.to_string(),
                    _ => String::new(),
                })
        };
        let theme = cx.theme();
        let (label, line) = match &selected {
            Some(crosshair) => {
                let count = plural("profiles.editor.xh_on", on_selected as u64);
                let name = name_of(crosshair);
                let line = h_flex()
                    .gap_2()
                    .child(
                        div()
                            .font_weight(gpui_kit::FontWeight::SEMIBOLD)
                            .child(name.clone()),
                    )
                    .child(
                        div()
                            .text_color(theme.muted_foreground)
                            .child(count.clone()),
                    );
                (SharedString::from(format!("{name}. {count}")), line)
            }
            None => {
                let pick_first = tr!("profiles.editor.xh_pick_first");
                let line = (h_flex().text_color(theme.muted_foreground)).child(pick_first.clone());
                (pick_first, line)
            }
        };
        let editor = cx.entity().downgrade();
        let everywhere = Button::new(ids::PROFILE_XH_EVERYWHERE)
            .outline()
            .small()
            .label(tr!("profiles.editor.xh_everywhere"))
            .disabled(selected.is_none() || self.busy)
            .on_click(move |_, _, cx| {
                let _ = editor.update(cx, |e, cx| e.assign(BoardTarget::Everything, cx));
            });
        let selection = h_flex()
            .id(ids::PROFILE_XH_SELECTION)
            .aria_label(label)
            .test_support()
            .flex_wrap()
            .items_center()
            .justify_between()
            .gap_3()
            .text_sm()
            .child(line)
            .child(everywhere);
        let editor = cx.entity().downgrade();
        let on: OnBoard = Rc::new(move |event, window, cx| {
            let _ = editor.update(cx, |e, cx| e.board_event(event, window, cx));
        });
        let loadout = Loadout::new(shown, Vec::new());
        let turned = self.card_width < BOARD_COLUMNS;
        let board = LoadoutBoard {
            loadout: &loadout,
            scope: IdScope::PROFILE_CROSSHAIRS,
            turned,
            short_names: !turned && self.card_width < px(1100.),
            selected: selected.as_ref(),
            focus: None,
            busy: if self.busy {
                BoardBusy::Other
            } else {
                BoardBusy::Idle
            },
            previews: &self.previews,
            name_of: &name_of,
        }
        .board(on, cx);
        v_flex()
            .gap_4()
            .child(self.picker.clone())
            .child(selection)
            .child(board)
            .into_any_element()
    }
}

/// The first entry of both sound selects.
fn tf2_default() -> SoundOption {
    SoundOption {
        id: None,
        label: tr!("profiles.card.tf2_default"),
    }
}

fn library_option(sound: &SoundEntry) -> SoundOption {
    SoundOption {
        id: Some(sound.id().clone()),
        label: sound.name().to_owned().into(),
    }
}

/// Subscribes the editor to `select`'s confirmed entries for `kind`.
pub(super) fn on_sound_confirm(
    select: &Entity<gpui_kit::component::select::SelectState<Vec<SoundOption>>>,
    kind: SoundKind,
    cx: &mut Context<ProfileDraftEditor>,
) -> gpui_kit::Subscription {
    cx.subscribe(select, move |editor, _, event, cx| {
        editor.sound_confirmed(kind, event, cx)
    })
}
