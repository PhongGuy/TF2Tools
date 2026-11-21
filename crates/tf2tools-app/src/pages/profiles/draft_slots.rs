//! The draft editor's five Set bodies, and the library they pick from.

use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;

use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::select::{Select, SelectEvent};
use gpui_kit::component::switch::Switch;
use gpui_kit::component::{ActiveTheme as _, Disableable as _, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, Context, Entity, InteractiveElement as _, IntoElement, ParentElement as _, Pixels,
    SharedString, StatefulInteractiveElement as _, Styled as _, TestSupportExt as _, Window, div,
    px,
};
use tf2tools_core::{
    AddonEntry, AddonName, Choices, CrosshairEntry, CrosshairId, CrosshairRef, DraftCrosshair,
    DraftItem, HudEntry, HudId, LibraryError, MastercomfigStatus, ModuleCatalog,
    ProfileCrosshairRef, SoundEntry, SoundId, SoundKind,
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
use crate::pages::crosshairs::{added_message, import_job};
use crate::pages::graphics::ChoicesEdit;
use crate::pages::graphics::model::{describe_status, version_parts};
use crate::pages::graphics::pictures::PictureIndex;
use crate::pages::graphics::view::{self, Filter};
use crate::pages::huds::{card_images, rounded_picture};
use crate::pages::sounds::{sound_key, with_play_state};
use crate::picker::PickKind;
use crate::remote_images::{ImageKey, RemoteImages};
use crate::services::{OpError, Services, run_blocking, tf2_state};
use crate::shell::route::{NavigateTo, Page};
use crate::shell::toast::{ToastKind, toast, toast_error};
use crate::widgets::card_grid::{GRID_GAP, columns_for_min};
use crate::widgets::labelled;
use crate::widgets::loadout_board::{BOARD_COLUMNS, BoardBusy, BoardEvent, LoadoutBoard, OnBoard};

/// A HUD tile's narrowest width: the grid fits as many columns of at least this as it can.
const HUD_TILE_MIN: Pixels = px(170.);

/// What the Set bodies pick from: the library HUDs with their card pictures by folder, the
/// library sounds by name, the library crosshairs and the library add-ons.
pub(super) struct Library {
    huds: Rc<[HudEntry]>,
    sounds: Rc<[SoundEntry]>,
    crosshairs: Rc<[CrosshairEntry]>,
    addons: Rc<[AddonEntry]>,
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
            let addons = s.library.list_addons(None).map_err(failed)?;
            let images = card_images(s, &huds);
            Ok((huds, sounds, crosshairs, addons, images))
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
                            Ok((huds, sounds, crosshairs, addons, images)) => {
                                let crosshairs: Rc<[CrosshairEntry]> = crosshairs.into();
                                let listed = Rc::clone(&crosshairs);
                                e.picker
                                    .update(cx, |picker, cx| picker.set_library(listed, cx));
                                e.library = Some(Library {
                                    huds: huds.into(),
                                    sounds: sounds.into(),
                                    crosshairs,
                                    addons: addons.into(),
                                    images: Rc::new(images),
                                });
                                e.drop_deleted_picks();
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

    // Drops each draft pick the fresh library no longer lists, so Save stops failing on it.
    fn drop_deleted_picks(&mut self) {
        let Some(l) = &self.library else {
            return;
        };
        let huds: Vec<HudId> = l.huds.iter().map(|h| h.id().clone()).collect();
        let sounds: Vec<SoundId> = l.sounds.iter().map(|x| x.id().clone()).collect();
        let crosshairs: Vec<CrosshairId> = l.crosshairs.iter().map(|c| c.id().clone()).collect();
        let addons: Vec<AddonName> = l.addons.iter().map(|a| a.name().clone()).collect();
        (self.draft).drop_missing_picks(&huds, &sounds, &crosshairs, &addons);
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
            SlotKind::Graphics => self.graphics_body(cx),
            SlotKind::Addons => self.addons_body(cx),
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
        let columns = columns_for_min(self.card_width, HUD_TILE_MIN);
        div()
            .grid()
            .grid_cols(columns as u16)
            .gap(GRID_GAP)
            .children(tiles)
            .into_any_element()
    }

    // One HUD tile: its 16:9 picture over its name in a padded frame, lit and filled when
    // picked; a click picks it.
    fn hud_tile(
        &self,
        pick: Picked<HudId>,
        key: Option<ImageKey>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let selected = self.draft.hud.as_ref() == Some(&pick);
        let folder = pick.folder.clone();
        let name = SharedString::from(pick.name.clone());
        let theme = cx.theme();
        let (primary, border, fill) = (theme.primary, theme.border, theme.secondary);
        v_flex()
            .id(ids::profile_pick_hud(&folder))
            .aria_label(name.clone())
            .aria_selected(selected)
            .test_support()
            .min_w_0()
            .gap(px(6.))
            .p(px(6.))
            .rounded(px(8.))
            .border_1()
            .border_color(border)
            .when(self.busy, |tile| tile.opacity(0.5))
            .when(!self.busy, |tile| tile.cursor_pointer())
            .when(selected, |tile| tile.border_color(primary).bg(fill))
            .child(rounded_picture(
                ids::profile_hud_shot(&folder),
                ids::profile_hud_image(&folder),
                key.as_ref(),
                Some(px(6.)),
                cx,
            ))
            .child(
                div()
                    .truncate()
                    .font_weight(gpui_kit::FontWeight::MEDIUM)
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

    // The Hitsound and Killsound rows: label, play button and select.
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
                .child(self.play_button(kind, play_id, spinner_id, cx))
                .child(
                    div().id(select_id).test_support().w(px(260.)).child(
                        Select::new(self.sound_select(kind))
                            .small()
                            .disabled(self.busy)
                            .accessibility_label(label),
                    ),
                )
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

    /// The board's OnBoard target: Assign calls `assign`; a dropped tile is selected, then
    /// assigned to its target, as on the Crosshairs page. Every other event, and every event
    /// while a Save runs, is ignored.
    pub fn board_event(&mut self, event: BoardEvent, _: &mut Window, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        match event {
            BoardEvent::Assign(target) => self.assign(target, cx),
            BoardEvent::Drop(target, crosshair) => {
                (self.picker).update(cx, |picker, cx| picker.select(Some(crosshair), cx));
                self.assign(target, cx);
            }
            BoardEvent::Open(..) | BoardEvent::Reset(_) | BoardEvent::Close => {}
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

impl ProfileDraftEditor {
    /// Reads, off the UI thread and once per editor, the Graphics page's saved choices for
    /// Copy, the settings catalogue (the managed version's, else the newest cached one) and
    /// the setting pictures, and hands the last two to the Graphics widget.
    pub(super) fn load_graphics(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let tf2 = tf2_state(cx).read(cx).install().cloned();
        let reading = run_blocking(cx, move |s| {
            let choices = (s.library.mastercomfig_choices())
                .map_err(|e| describe_status(&e).into_op(tr!("messages.action.load_mastercomfig")));
            let managed = tf2.and_then(|tf2| match s.library.mastercomfig_status(&tf2) {
                Ok(MastercomfigStatus::Managed { version, .. }) => Some(version),
                _ => None,
            });
            let catalog = managed
                .and_then(|tag| s.store.cached_mastercomfig_catalog(&tag))
                .or_else(|| newest_cached_catalog(s));
            let pictures = PictureIndex::open(&s.assets.setting_pictures_dir());
            Ok((choices, catalog, pictures))
        });
        let win = window.window_handle();
        cx.spawn(async move |editor, cx| {
            let Ok((choices, catalog, pictures)) = reading.await else {
                return;
            };
            let _ = cx.update(|cx| {
                win.update(cx, |_, window, cx| {
                    let _ = editor.update(cx, |e, cx| {
                        e.page_choices = Some(choices);
                        e.graphics.update(cx, |g, cx| {
                            g.set_catalog(catalog.map(Arc::new), window, cx);
                            g.set_pictures(Some(Arc::new(pictures)), cx);
                        });
                        cx.notify();
                    });
                })
            });
        })
        .detach();
    }

    /// Takes the Graphics widget's proposed edit into the draft and shows it back. A preset
    /// switch keeps every module choice, so it needs no catalogue.
    pub(super) fn graphics_edit(
        &mut self,
        edit: &ChoicesEdit,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let choices = match edit {
            ChoicesEdit::Preset(preset) => self.draft.graphics.with_preset(*preset),
            ChoicesEdit::Modules { to, .. } => to.clone(),
        };
        self.set_graphics(choices, window, cx);
    }

    fn set_graphics(&mut self, choices: Choices, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        self.edit(|d| d.graphics = choices.clone(), cx);
        (self.graphics).update(cx, |g, cx| g.set_choices(choices, window, cx));
    }

    // Copy: the Graphics page's saved choices into the draft; a failed read toasts.
    fn copy_graphics(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        match self.page_choices.clone() {
            Some(Ok(Some(choices))) => self.set_graphics(choices, window, cx),
            Some(Err(err)) => toast_error(window, cx, &err),
            Some(Ok(None)) | None => {}
        }
    }

    // The presets card, the changes strip, then one Settings card headed by Copy over each
    // category's heading and rows; without a catalogue the card holds a line saying where to
    // get one.
    fn graphics_body(&self, cx: &mut Context<Self>) -> AnyElement {
        let narrow = self.narrow;
        let theme = cx.theme();
        let (muted, border, background) = (theme.muted_foreground, theme.border, theme.background);
        let no_tf2 = tf2_state(cx).read(cx).install().is_none();
        let not_managed = matches!(self.page_choices, Some(Ok(None)));
        let copy = Button::new(ids::PROFILE_GFX_COPY)
            .ghost()
            .small()
            .label(tr!("profiles.editor.gfx_copy"))
            .disabled(no_tf2 || not_managed || self.page_choices.is_none() || self.busy)
            .when(not_managed, |b| {
                b.tooltip(tr!("profiles.editor.gfx_copy_none"))
            })
            .on_click(cx.listener(|editor, _, window, cx| editor.copy_graphics(window, cx)));
        let header = h_flex()
            .flex_wrap()
            .items_center()
            .gap_3()
            .px(px(14.))
            .py(px(10.))
            .border_b_1()
            .border_color(border)
            .child(
                div()
                    .text_sm()
                    .font_weight(gpui_kit::FontWeight::SEMIBOLD)
                    .child(tr!("profiles.editor.gfx_settings")),
            )
            .child(
                div()
                    .flex_1()
                    .min_w(px(200.))
                    .text_sm()
                    .text_color(muted)
                    .child(tr!("profiles.editor.gfx_settings_help")),
            )
            .child(copy);
        // Clipping drops a flex item's content height as its floor, so the card keeps it.
        let card = v_flex()
            .id(ids::PROFILE_GFX_SETTINGS)
            .test_support()
            .flex_none()
            .overflow_hidden()
            .rounded(px(10.))
            .border_1()
            .border_color(border)
            .bg(background)
            .child(header);
        self.graphics.update(cx, |g, cx| {
            g.want_images(cx);
            let presets = g.presets_card(narrow, cx);
            let Some(catalog) = g.catalog().cloned() else {
                let line = labelled(
                    div().p_4().text_sm().text_color(muted),
                    ids::PROFILE_GFX_NO_CATALOG,
                    tr!("profiles.editor.gfx_no_catalog"),
                );
                return (v_flex().gap_4().child(presets).child(card.child(line)))
                    .into_any_element();
            };
            let scope = g.scope();
            let mut card = card;
            let sections = view::sections(&catalog, g.choices(), &Filter::default());
            for (n, section) in sections.iter().enumerate() {
                let rows = (section.rows.iter().enumerate())
                    .filter_map(|(i, r)| g.settings_row(&r.module, i == 0, narrow, cx));
                let rows: Vec<AnyElement> = rows.collect();
                if rows.is_empty() {
                    continue;
                }
                let heading = labelled(
                    div()
                        .px_4()
                        .py_3()
                        .text_size(px(13.))
                        .font_weight(gpui_kit::FontWeight::SEMIBOLD)
                        .text_color(muted)
                        .border_b_1()
                        .border_color(border)
                        .when(n > 0, |heading| heading.border_t_1()),
                    scope.id(ids::graphics_category(&section.key)),
                    section.label.clone(),
                );
                card = card.child(heading).children(rows);
            }
            (v_flex().gap_4())
                .child(presets)
                .child(g.changes_strip(cx))
                .child(card)
                .into_any_element()
        })
    }

    // The add-ons this body lists: every library one, then any saved or picked one the
    // library no longer has, by its saved name.
    fn addon_rows(&self) -> Vec<Picked<AddonName>> {
        let mut rows: Vec<Picked<AddonName>> = (self.library.iter())
            .flat_map(|l| l.addons.iter().map(Picked::addon))
            .collect();
        for pick in self.baseline.addons.iter().chain(&self.draft.addons) {
            if !rows.contains(pick) {
                rows.push(pick.clone());
            }
        }
        rows
    }

    // Turns `pick` on or off in the draft, keeping the list sorted by asset name.
    fn toggle_addon(&mut self, pick: Picked<AddonName>, on: bool, cx: &mut Context<Self>) {
        self.edit(
            |d| {
                d.addons.retain(|a| *a != pick);
                if on {
                    d.addons.push(pick);
                    d.addons.sort_by(|a, b| a.folder.cmp(&b.folder));
                }
            },
            cx,
        );
    }

    // A row per add-on, its name left and its switch right over a divider, then Only these
    // with what it means; or, with nothing to list, the empty line and a link to the Store.
    fn addons_body(&self, cx: &mut Context<Self>) -> AnyElement {
        if self.library.is_none() {
            return div().into_any_element();
        }
        let muted = cx.theme().muted_foreground;
        let rows = self.addon_rows();
        if rows.is_empty() {
            let page = self.page.clone();
            return h_flex()
                .flex_wrap()
                .items_center()
                .gap_3()
                .child(labelled(
                    div().text_sm().text_color(muted),
                    ids::PROFILE_ADDONS_EMPTY,
                    tr!("profiles.editor.addons_empty"),
                ))
                .child(
                    Button::new(ids::PROFILE_ADDONS_STORE)
                        .outline()
                        .small()
                        .label(tr!("profiles.editor.addons_store"))
                        .on_click(move |_, _, cx| {
                            if let Some(page) = page.as_ref() {
                                let _ =
                                    page.update(cx, |_, cx| cx.emit(NavigateTo(Page::AddonStore)));
                            }
                        }),
                )
                .into_any_element();
        }
        let border = cx.theme().border;
        let switches = rows.into_iter().map(|pick| {
            let on = self.draft.addons.contains(&pick);
            let name = SharedString::from(pick.name.clone());
            h_flex()
                .items_center()
                .gap_3()
                .py_2()
                .border_b_1()
                .border_color(border)
                .child(
                    div()
                        .id(ids::profile_addon_name(&pick.folder))
                        .test_support()
                        .flex_1()
                        .min_w_0()
                        .truncate()
                        .cursor_pointer()
                        .child(name.clone())
                        .on_click({
                            let pick = pick.clone();
                            cx.listener(move |editor, _, _, cx| {
                                editor.toggle_addon(pick.clone(), !on, cx)
                            })
                        }),
                )
                .child(
                    Switch::new(ids::profile_addon(&pick.folder))
                        .checked(on)
                        .accessibility_label(name)
                        .disabled(self.busy)
                        .on_click(cx.listener(move |editor, &on: &bool, _, cx| {
                            editor.toggle_addon(pick.clone(), on, cx)
                        })),
                )
        });
        let only = self.draft.only_these;
        let help = if only {
            tr!("profiles.editor.addons_only_on")
        } else {
            tr!("profiles.editor.addons_only_off")
        };
        let title = tr!("profiles.editor.addons_only");
        v_flex()
            .children(switches)
            .child(
                h_flex()
                    .items_center()
                    .gap_3()
                    .pt_3()
                    .child(
                        v_flex()
                            .flex_1()
                            .min_w_0()
                            .gap_1()
                            .child(
                                div()
                                    .id(ids::PROFILE_ADDONS_ONLY_NAME)
                                    .test_support()
                                    .cursor_pointer()
                                    .font_weight(gpui_kit::FontWeight::MEDIUM)
                                    .child(title.clone())
                                    .on_click(cx.listener(move |editor, _, _, cx| {
                                        editor.edit(|d| d.only_these = !only, cx)
                                    })),
                            )
                            .child(div().text_sm().text_color(muted).child(help)),
                    )
                    .child(
                        Switch::new(ids::PROFILE_ADDONS_ONLY)
                            .checked(only)
                            .accessibility_label(title)
                            .disabled(self.busy)
                            .on_click(cx.listener(|editor, &on: &bool, _, cx| {
                                editor.edit(|d| d.only_these = on, cx)
                            })),
                    ),
            )
            .into_any_element()
    }
}

/// The newest release's catalogue saved in the mastercomfig cache, if any parses.
fn newest_cached_catalog(s: &Services) -> Option<ModuleCatalog> {
    let mut tags: Vec<(Vec<u64>, String)> = std::fs::read_dir(s.data.mastercomfig_dir())
        .ok()?
        .filter_map(Result::ok)
        .filter_map(|entry| entry.file_name().into_string().ok())
        .filter_map(|tag| Some((version_parts(&tag)?, tag)))
        .collect();
    tags.sort();
    (tags.iter().rev()).find_map(|(_, tag)| s.store.cached_mastercomfig_catalog(tag))
}
