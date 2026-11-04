//! The HUD Maker's recipe sections: the preview bar atop the HUD section, where a previewed
//! recipe is applied or cancelled, and Presets under the options, the player's saved option
//! choices listed by name with Apply and Delete under a name field and "Save as preset", then
//! Recipes, which shares the HUD's changes as a file or previews one read from a file. The
//! toolbar's overflow menu offers a new HUD the four starting points, each previewed like a
//! recipe.

use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::{Input, InputState};
use gpui_kit::component::popover::Popover;
use gpui_kit::component::{
    ActiveTheme as _, Disableable as _, Selectable as _, Sizable as _, h_flex, v_flex,
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    Anchor, AnyElement, Context, Entity, InteractiveElement as _, IntoElement, ParentElement as _,
    StatefulInteractiveElement as _, Styled as _, Task, TestSupportExt as _, Window, div, point,
    px,
};
use tf2tools_core::{PresetEntry, PresetError, Recipe, StartingPoint, read_recipe_file};

use std::io::Write as _;
use std::path::PathBuf;

use super::borrow::BorrowNote;
use super::tip::tipped;
use super::view::{Panel, PanelPlace};
use super::{FontBytes, HudMakerPage, colour_view, register_font};
use crate::ids;
use crate::picker::{PickKind, file_name_for, with_extension};
use crate::services::{OpError, Services, run_blocking};
use crate::shell::toast::{ToastKind, toast};
use crate::widgets::labelled;

const NAME_BLANK: &str = "Give the preset a name.";
const NAME_TAKEN: &str = "A preset with that name already exists.";
const NO_OPTIONS: &str = "Choose some options first, then save them as a preset.";
const SAVE_FAILED: &str = "Couldn't save the preset.";
const DELETE_FAILED: &str = "Couldn't delete the preset.";
const LIST_FAILED: &str = "Couldn't read your presets.";
const PRESET_UNREADABLE: &str = "That file isn't a preset TF2Tools can read.";
const RECIPE_UNREADABLE: &str = "That file isn't a recipe TF2Tools can read.";
const RECIPE_SAVE_FAILED: &str = "Couldn't save the recipe.";
const START_OFF: &str = "Only for a new HUD with no changes";
const TF2_ONLY: &str = "Some of these changes only show in TF2.";

/// What the preview bar says about the latest preview: its name and what it left out.
#[derive(Debug, Default)]
pub(super) struct PreviewNote {
    name: String,
    changes: usize,
    fonts: usize,
    // Whether the preview changes an option the canvas cannot draw.
    tf2_only: bool,
    // The credit and what came along, when the preview is a borrowed part.
    borrowed: Option<BorrowNote>,
    // The preview's number in the page's count, so a borrow started earlier never replaces it.
    pub(super) seq: u64,
}

impl PreviewNote {
    /// The note for a borrowed part's preview.
    pub(super) fn borrowed(note: BorrowNote, dropped: usize, seq: u64) -> PreviewNote {
        PreviewNote {
            name: note.part.clone(),
            changes: dropped,
            seq,
            borrowed: Some(note),
            ..PreviewNote::default()
        }
    }

    /// The borrow being previewed, if the preview is one.
    pub(super) fn borrow(&self) -> Option<&BorrowNote> {
        self.borrowed.as_ref()
    }
}

// How a recipe export ended: written with this many fonts and picture-holding parts left out,
// or refused because a file already holds the name the ".json" made.
enum Shared {
    Saved { fonts: usize, pictures: usize },
    Taken(String),
}

// Which preset job ran, for the toast its failure shows and whether success clears the name.
#[derive(Clone, Copy)]
enum PresetWork {
    List,
    Save,
    Delete,
}

impl PresetWork {
    fn failed(self) -> &'static str {
        match self {
            PresetWork::List => LIST_FAILED,
            PresetWork::Save => SAVE_FAILED,
            PresetWork::Delete => DELETE_FAILED,
        }
    }
}

// What a preset job did, and the presets folder as read right after it.
type PresetJob = Task<Result<(Result<(), PresetError>, Vec<PresetEntry>), OpError>>;

impl HudMakerPage {
    /// Previews `recipe` on the canvas, the one entry for every preview; `skipped` counts the
    /// changes already left out when it was read. The bar shows until Apply, Cancel or an edit.
    pub fn start_preview(
        &mut self,
        recipe: Recipe,
        skipped: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // Typed values commit first, since committing them later would cancel the preview.
        // An open option picker closes, as the swatches it belongs to wait for the preview.
        self.flush(cx);
        self.close_option_picker(cx);
        self.close_history(window, cx);
        self.close_borrow(cx);
        let Some(editor) = self.editor_mut() else {
            return;
        };
        // The editor counts refused fonts among its skipped changes; the bar names them apart.
        let left_out = editor.preview_recipe(&recipe);
        let fonts = editor.preview_fonts_left_out();
        let tf2_only = (editor.options().iter())
            .filter(|spec| !spec.drawn())
            .any(|spec| editor.shown_option_value(spec.id()) != editor.option_value(spec.id()));
        self.register_preview_fonts(cx);
        self.preview_seq = self.preview_seq.wrapping_add(1);
        self.preview = PreviewNote {
            seq: self.preview_seq,
            name: recipe.name().to_owned(),
            changes: skipped.saturating_add(left_out.saturating_sub(fonts)),
            fonts: fonts.saturating_add(recipe.fonts_left_out()),
            tf2_only,
            borrowed: None,
        };
        self.decode_shown_images(cx);
        // The bar heads the panel's body, so the panel opens and scrolls to the top: a preview
        // the player can't see is one they don't know is on.
        self.open_panel_at_top(cx);
        self.refresh(window, cx);
        cx.notify();
    }

    // The preview's own fonts reach the text system now so the canvas draws them; the text
    // system keeps them after a cancel, which is harmless.
    pub(super) fn register_preview_fonts(&mut self, cx: &mut Context<Self>) {
        let Some(editor) = self.editor() else {
            return;
        };
        let added: Vec<FontBytes> = (editor.model().assets().iter())
            .filter_map(|(_, source)| {
                let bytes = editor.shown_assets().get(source)?;
                (editor.assets().get(source) != Some(bytes)).then(|| FontBytes::new(bytes.to_vec()))
            })
            .collect();
        for font in added {
            let families = font.families.clone();
            if register_font(font, cx)
                && let super::Load::Ready(loaded) = &mut self.load
            {
                (loaded.font_names).extend(families.iter().map(|f| f.to_lowercase()));
            }
        }
    }

    // Opens the Options panel (staying open over the selection), expanding it when collapsed,
    // and scrolls it to the top.
    pub(super) fn open_panel_at_top(&mut self, cx: &mut Context<Self>) {
        self.panels[Panel::Options as usize].held_open = true;
        let place = self.panel_place();
        if place.collapsed {
            self.set_panel_place(
                PanelPlace {
                    collapsed: false,
                    ..place
                },
                cx,
            );
        }
        self.panel_scroll.set_offset(point(px(0.), px(0.)));
    }

    /// The preview bar while the editor holds a preview: what it shows, Apply and Cancel.
    pub(super) fn preview_bar(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !self.editor()?.previewing() {
            return None;
        }
        let note = &self.preview;
        let line = match note.borrow() {
            Some(borrow) => borrow.bar_line(),
            None => {
                let mut line = format!("Previewing {}.", note.name);
                let tf2_only = note.tf2_only.then(|| TF2_ONLY.to_owned());
                for part in [
                    changes_dont_fit(note.changes),
                    fonts_left_out(note.fonts),
                    tf2_only,
                ] {
                    line.extend(part.map(|p| format!(" {p}")));
                }
                line
            }
        };
        let theme = cx.theme();
        let apply = Button::new(ids::HUD_MAKER_PREVIEW_APPLY)
            .primary()
            .small()
            .label("Apply")
            .on_click(cx.listener(|page, _, window, cx| page.apply_preview(window, cx)));
        let cancel = Button::new(ids::HUD_MAKER_PREVIEW_CANCEL)
            .outline()
            .small()
            .label("Cancel")
            .on_click(cx.listener(|page, _, window, cx| page.cancel_preview(window, cx)));
        let bar = v_flex()
            .gap_2()
            .p_2()
            .rounded_md()
            .border_1()
            .border_color(theme.border)
            .bg(theme.muted)
            .child(labelled(
                div().text_sm().text_color(theme.foreground),
                ids::HUD_MAKER_PREVIEW,
                line,
            ))
            .child(h_flex().gap_2().child(apply).child(cancel));
        Some(bar.into_any_element())
    }

    /// Whether "Start from a ready-made HUD" is live: only on a new HUD, never saved and with
    /// no changes. A running preview leaves it live, so the next look replaces it.
    pub fn can_start_from(&self) -> bool {
        self.saved_hud().is_none() && self.editor().is_some_and(|e| e.changes().is_empty())
    }

    /// The toolbar's overflow button and its menu: "Save as new" once saved, then "Start from a
    /// ready-made HUD", which opens the four starting points in place; a pick previews like any
    /// recipe and closes the menu.
    pub(super) fn more_menu(&self, cx: &mut Context<Self>) -> AnyElement {
        let page = cx.entity().downgrade();
        let trigger = Button::new(ids::HUD_MAKER_MORE)
            .ghost()
            .icon(IconName::Ellipsis)
            .tooltip("More")
            .accessibility_label("More");
        Popover::new("hud-maker-more-menu")
            .anchor(Anchor::BottomRight)
            .open(self.more_open)
            .on_open_change(move |&open, _, cx| {
                let _ = page.update(cx, |page, cx| page.set_more_open(open, cx));
            })
            .trigger(trigger)
            .child(self.more_items(cx))
            .into_any_element()
    }

    // Opens or closes the overflow menu; it always opens on its entries, not on the points.
    fn set_more_open(&mut self, open: bool, cx: &mut Context<Self>) {
        self.more_open = open;
        self.start_open = false;
        cx.notify();
    }

    // The menu's entries: Start from, and once it is pressed, a button per starting point.
    fn more_items(&self, cx: &mut Context<Self>) -> AnyElement {
        let live = self.can_start_from();
        let start = Button::new(ids::HUD_MAKER_START)
            .ghost()
            .small()
            .label("Start from a ready-made HUD")
            .disabled(!live)
            .when(!live, |b| b.tooltip(START_OFF))
            .on_click(cx.listener(|page, _, _, cx| {
                page.start_open = true;
                cx.notify();
            }));
        let muted = cx.theme().muted_foreground;
        let points = (self.start_open && live).then(|| {
            StartingPoint::ALL.map(|which| {
                let pick = Button::new(ids::hud_maker_start(which.label()))
                    .outline()
                    .small()
                    .label(which.label())
                    .on_click(cx.listener(move |page, _, window, cx| {
                        // Built on the untouched HUD, so the recipe never carries the player's
                        // edits.
                        let Some(recipe) =
                            page.editor().map(|e| e.pristine().starting_point(which))
                        else {
                            return;
                        };
                        page.set_more_open(false, cx);
                        page.start_preview(recipe, 0, window, cx);
                    }));
                v_flex()
                    .gap_1()
                    .items_start()
                    .child(pick)
                    .child(div().text_xs().text_color(muted).child(which.about()))
            })
        });
        let borrow = Button::new(ids::HUD_MAKER_BORROW_OPEN)
            .ghost()
            .small()
            .label("Borrow a part")
            .tooltip("Copy a part from another HUD in your library")
            .on_click(cx.listener(|page, _, window, cx| page.open_borrow(window, cx)));
        let history = Button::new(ids::HUD_MAKER_HISTORY_OPEN)
            .ghost()
            .small()
            .label("History")
            .tooltip("Keep copies of your HUD to compare or go back to")
            .on_click(cx.listener(|page, _, window, cx| page.open_history(window, cx)));
        let every = Button::new(ids::HUD_MAKER_EVERY_SCREEN)
            .ghost()
            .small()
            .label("Every screen at once")
            .on_click(cx.listener(|page, _, window, cx| {
                page.set_more_open(false, cx);
                page.set_every_screen(true, window, cx);
            }));
        let backdrop = Button::new(ids::HUD_MAKER_BACKDROP)
            .ghost()
            .small()
            .label("Put a screenshot behind the HUD")
            .on_click(cx.listener(|page, _, _, cx| {
                page.set_more_open(false, cx);
                page.pick_backdrop(cx);
            }));
        let clear = self.backdrop().is_some().then(|| {
            Button::new(ids::HUD_MAKER_BACKDROP_CLEAR)
                .ghost()
                .small()
                .label("Remove the screenshot")
                .on_click(cx.listener(|page, _, _, cx| {
                    page.set_more_open(false, cx);
                    page.clear_backdrop(cx);
                }))
        });
        let current = self.vision();
        let views = colour_view::VIEWS.map(|(vision, word, name)| {
            let id = ids::hud_maker_vision(word);
            let button = Button::new(id.clone())
                .ghost()
                .small()
                .label(word)
                .selected(vision == current)
                .on_click(cx.listener(move |page, _, _, cx| {
                    page.set_more_open(false, cx);
                    page.set_vision(vision, cx);
                }));
            tipped(&id, name, button)
        });
        v_flex()
            .w(px(260.))
            .gap_2()
            .items_start()
            .children(
                self.toolbar_compact()
                    .then(|| self.save_row(cx))
                    .into_iter()
                    .flatten(),
            )
            .children(self.save_as_new_item(cx))
            .child(start)
            .children(points.into_iter().flatten())
            .child(borrow)
            .child(history)
            .child(every)
            .child(backdrop)
            .children(clear)
            .child(div().text_xs().text_color(muted).child("Colour-blind view"))
            .child(h_flex().flex_wrap().gap_1().children(views))
            .into_any_element()
    }

    // Keeps the preview as one undo step, warning about whatever it left out.
    fn apply_preview(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(editor) = self.editor_mut() else {
            return;
        };
        if !editor.apply_preview() {
            return;
        }
        let note = &self.preview;
        let parts: Vec<String> = [no_longer_fit(note.changes), fonts_left_out(note.fonts)]
            .into_iter()
            .flatten()
            .collect();
        if let Some(borrow) = note.borrow() {
            let text = super::borrow::added_text(&borrow.part, &borrow.hud, note.changes);
            let kind = match note.changes {
                0 => ToastKind::Success,
                _ => ToastKind::Warning,
            };
            toast(window, cx, kind, text, "");
            self.borrow.open = false;
        } else if !parts.is_empty() {
            toast(window, cx, ToastKind::Warning, parts.join(" "), "");
        }
        self.decode_shown_images(cx);
        self.refresh(window, cx);
        cx.notify();
    }

    fn cancel_preview(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(editor) = self.editor_mut() {
            editor.cancel_preview();
        }
        self.decode_shown_images(cx);
        self.refresh(window, cx);
        cx.notify();
    }

    // Reads the preset in `file` off the UI thread, then previews it on the same HUD; a read
    // that lands after another HUD loaded is dropped, preview and toast alike.
    fn apply_preset(&mut self, file: &str, cx: &mut Context<Self>) {
        let file = file.to_owned();
        let load = self.loads;
        let job = run_blocking(cx, move |s: &Services| Ok(s.library.load_preset(&file)));
        let win = self.win;
        cx.spawn(async move |page, cx| {
            let landed = job.await;
            cx.update(|cx| {
                let _ = win.update(cx, |_, window, cx| {
                    page.update(cx, |page, cx| match landed {
                        _ if page.loads != load => {}
                        Ok(Ok(recipe)) => page.start_preview(recipe, 0, window, cx),
                        failed => {
                            let why = failed.err().map(|e| e.to_string());
                            tracing::debug!(error = ?why, "a preset didn't load");
                            toast(window, cx, ToastKind::Error, PRESET_UNREADABLE, "");
                        }
                    })
                });
            });
        })
        .detach();
    }

    /// The sections under the options: Presets (name field, Save and the list), then Recipes.
    pub(super) fn recipe_rows(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = cx.theme();
        let heading = labelled(
            div()
                .text_sm()
                .font_weight(gpui_kit::FontWeight::SEMIBOLD)
                .text_color(theme.foreground),
            ids::HUD_MAKER_PRESETS,
            "Presets",
        );
        let save = Button::new(ids::HUD_MAKER_PRESET_SAVE)
            .outline()
            .small()
            .icon(IconName::Save)
            .label("Save as preset")
            .on_click(cx.listener(|page, _, window, cx| page.save_preset(window, cx)));
        let name = div()
            .id(ids::HUD_MAKER_PRESET_NAME)
            .aria_label("Preset name")
            .test_support()
            .flex_1()
            .min_w_0()
            .child(Input::new(&self.preset_name).small());
        let rows = self.presets.iter().map(|preset| {
            let file = preset.file().to_owned();
            let apply = Button::new(ids::hud_maker_preset_apply(&file))
                .ghost()
                .small()
                .label(preset.name().to_owned())
                .on_click({
                    let file = file.clone();
                    cx.listener(move |page, _, _, cx| page.apply_preset(&file, cx))
                });
            let delete_id = ids::hud_maker_preset_delete(&file);
            let delete = Button::new(delete_id.clone())
                .ghost()
                .xsmall()
                .icon(IconName::Trash)
                .accessibility_label(format!("Delete {}", preset.name()))
                .on_click(
                    cx.listener(move |page, _, window, cx| page.delete_preset(&file, window, cx)),
                );
            h_flex()
                .justify_between()
                .items_center()
                .child(div().min_w_0().overflow_hidden().child(apply))
                .child(tipped(&delete_id, "Delete", delete))
        });
        let recipes = labelled(
            div()
                .text_sm()
                .font_weight(gpui_kit::FontWeight::SEMIBOLD)
                .text_color(theme.foreground),
            ids::HUD_MAKER_RECIPES,
            "Recipes",
        );
        let share = Button::new(ids::HUD_MAKER_RECIPE_EXPORT)
            .outline()
            .small()
            .icon(IconName::Upload)
            .label("Share as recipe")
            .on_click(cx.listener(|page, _, _, cx| page.share_recipe(cx)));
        let import = Button::new(ids::HUD_MAKER_RECIPE_IMPORT)
            .outline()
            .small()
            .icon(IconName::Download)
            .label("Use a recipe")
            .on_click(cx.listener(|page, _, _, cx| page.use_recipe(cx)));
        v_flex()
            .gap_2()
            .child(heading)
            .child(h_flex().gap_2().items_center().child(name).child(save))
            .children(rows)
            .child(recipes)
            .child(h_flex().gap_2().flex_wrap().child(share).child(import))
            .into_any_element()
    }

    // Asks where to save, suggesting the HUD's name, then writes the committed changes as a
    // recipe off the UI thread. A preview is never shared, since `changes()` doesn't hold it.
    fn share_recipe(&mut self, cx: &mut Context<Self>) {
        self.flush(cx);
        let Some(editor) = self.editor() else {
            return;
        };
        let name = match self.trimmed_name(cx) {
            n if n.is_empty() => "My HUD".to_owned(),
            n => n,
        };
        let (changes, assets) = (editor.changes().to_vec(), editor.assets().clone());
        let picking = cx.global::<Services>().picker.save_to(
            PickKind::RecipeExport,
            &file_name_for(&name, "json"),
            cx,
        );
        let win = self.win;
        cx.spawn(async move |page, cx| {
            let Some(path) = picking.await else {
                return;
            };
            let job = cx.update(|cx| {
                run_blocking(cx, move |_| {
                    let recipe = Recipe::new(&name, &changes, &assets);
                    let written = write_recipe(path, &recipe).map_err(|e| e.to_string());
                    Ok(written.map(|taken| match taken {
                        Some(name) => Shared::Taken(name),
                        // Core counts a picture's files among the fonts left out.
                        None => Shared::Saved {
                            fonts: (recipe.fonts_left_out())
                                .saturating_sub(recipe.pictures_left_out()),
                            pictures: recipe.parts_left_out_for_pictures(),
                        },
                    }))
                })
            });
            let landed = job.await;
            cx.update(|cx| {
                let _ = win.update(cx, |_, window, cx| {
                    page.update(cx, |_, cx| match landed {
                        Ok(Ok(Shared::Saved {
                            fonts: 0,
                            pictures: 0,
                        })) => toast(window, cx, ToastKind::Success, "Recipe saved", ""),
                        Ok(Ok(Shared::Saved { fonts, pictures })) => {
                            let pictures = (pictures > 0).then(|| PICTURES_LEFT_OUT.to_owned());
                            let left: Vec<String> = [fonts_left_out(fonts), pictures]
                                .into_iter()
                                .flatten()
                                .collect();
                            toast(
                                window,
                                cx,
                                ToastKind::Warning,
                                "Recipe saved",
                                left.join(" "),
                            )
                        }
                        Ok(Ok(Shared::Taken(name))) => {
                            let line =
                                format!("A file named {name} is already there. Pick another name.");
                            toast(window, cx, ToastKind::Warning, line, "")
                        }
                        Ok(Err(why)) => share_failed(window, cx, why),
                        Err(err) => share_failed(window, cx, err.to_string()),
                    })
                });
            });
        })
        .detach();
    }

    // Asks for a recipe file, reads and checks it off the UI thread, then previews it. A file
    // that can't be read, is too large or isn't a recipe toasts and changes nothing. A file that
    // lands after another HUD loaded is dropped, as the preview would land on the wrong HUD.
    fn use_recipe(&mut self, cx: &mut Context<Self>) {
        let load = self.loads;
        let picking = cx.global::<Services>().picker.pick(PickKind::Recipe, cx);
        let win = self.win;
        cx.spawn(async move |page, cx| {
            let Some(path) = picking.await else {
                return;
            };
            let job = cx.update(|cx| {
                run_blocking(cx, move |_| {
                    Ok(read_recipe_file(&path).map_err(|e| e.to_string()))
                })
            });
            let landed = job.await;
            cx.update(|cx| {
                let _ = win.update(cx, |_, window, cx| {
                    page.update(cx, |page, cx| match landed {
                        _ if page.loads != load => {}
                        Ok(Ok((recipe, skipped))) => {
                            page.start_preview(recipe, skipped, window, cx)
                        }
                        failed => {
                            let why = match failed {
                                Ok(Err(why)) => why,
                                Err(err) => err.to_string(),
                                Ok(Ok(_)) => String::new(),
                            };
                            tracing::debug!(error = %why, "a recipe didn't load");
                            toast(window, cx, ToastKind::Error, RECIPE_UNREADABLE, "");
                        }
                    })
                });
            });
        })
        .detach();
    }

    /// The preset name field.
    pub fn preset_name_input(&self) -> &Entity<InputState> {
        &self.preset_name
    }

    /// Reads the presets folder off the UI thread; called when a HUD lands.
    pub(super) fn read_presets(&mut self, cx: &mut Context<Self>) {
        let job = run_blocking(cx, |s| Ok((Ok(()), s.library.list_presets())));
        self.land_presets(job, PresetWork::List, cx);
    }

    // Saves the committed changes' option choices under the typed name; a blank name is
    // refused here, before any file work.
    fn save_preset(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.flush(cx);
        let name = self.preset_name.read(cx).value().trim().to_owned();
        if name.is_empty() {
            return toast(window, cx, ToastKind::Error, NAME_BLANK, "");
        }
        let Some(changes) = self.editor().map(|e| e.changes().to_vec()) else {
            return;
        };
        let job = run_blocking(cx, move |s: &Services| {
            let saved = s.library.save_preset(&name, &changes).map(drop);
            Ok((saved, s.library.list_presets()))
        });
        self.land_presets(job, PresetWork::Save, cx);
    }

    // Deletes the preset in `file`; the HUD's changes are untouched.
    fn delete_preset(&mut self, file: &str, _: &mut Window, cx: &mut Context<Self>) {
        let file = file.to_owned();
        let job = run_blocking(cx, move |s: &Services| {
            // A preset already gone is what the player asked for.
            let deleted = match s.library.delete_preset(&file) {
                Err(PresetError::NotFound) => Ok(()),
                done => done,
            };
            Ok((deleted, s.library.list_presets()))
        });
        self.land_presets(job, PresetWork::Delete, cx);
    }

    // Lands a preset job: its fresh list replaces the shown one when no later job started, and
    // a failure toasts in player words. A save that worked empties the name field.
    fn land_presets(&mut self, job: PresetJob, work: PresetWork, cx: &mut Context<Self>) {
        let failed = work.failed();
        let clear = matches!(work, PresetWork::Save);
        self.preset_reads = self.preset_reads.wrapping_add(1);
        let read = self.preset_reads;
        let win = self.win;
        cx.spawn(async move |page, cx| {
            let landed = job.await;
            cx.update(|cx| {
                let _ = win.update(cx, |_, window, cx| {
                    page.update(cx, |page, cx| {
                        let done = match landed {
                            Ok((done, list)) => {
                                if page.preset_reads == read {
                                    page.presets = list;
                                    cx.notify();
                                }
                                done.map_err(|e| {
                                    (preset_message(&e).unwrap_or(failed), e.to_string())
                                })
                            }
                            Err(err) => Err((failed, err.to_string())),
                        };
                        match done {
                            Ok(()) if clear => page
                                .preset_name
                                .update(cx, |input, cx| input.set_value("", window, cx)),
                            Ok(()) => {}
                            Err((message, why)) => {
                                tracing::debug!(error = %why, "a preset job failed");
                                toast(window, cx, ToastKind::Error, message, "");
                            }
                        }
                    })
                });
            });
        })
        .detach();
    }
}

fn share_failed(window: &mut Window, cx: &mut gpui_kit::App, why: String) {
    tracing::debug!(error = %why, "a recipe didn't save");
    toast(window, cx, ToastKind::Error, RECIPE_SAVE_FAILED, "");
}

// Writes `recipe` where the player picked, as a `.json`. The dialog only vetted the name it
// returned, so a name the ".json" changed onto an existing file is refused (`Some(name)`).
// The bytes go to a fresh, uniquely named `.tmp` beside the target first, so the target is
// never half-written and nothing already there is written through.
fn write_recipe(picked: PathBuf, recipe: &Recipe) -> std::io::Result<Option<String>> {
    static TMP_COUNT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let target = with_extension(picked.clone(), "json");
    let name = target.file_name().unwrap_or_default().to_string_lossy();
    let n = TMP_COUNT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let tmp = target.with_file_name(format!("{name}.{}-{n}.tmp", std::process::id()));
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&tmp)?;
    let written = file
        .write_all(&recipe.to_json())
        .and_then(|()| file.sync_all());
    drop(file);
    let placed = written.and_then(|()| {
        if target == picked {
            return std::fs::rename(&tmp, &target).map(|()| None);
        }
        // A hard link fails on an existing name, so a file that appears meanwhile is refused,
        // never replaced. Where links aren't supported, the name is checked, then renamed onto.
        match std::fs::hard_link(&tmp, &target) {
            Ok(()) => Ok(None),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => Ok(Some(name.into_owned())),
            Err(_) if target.exists() => Ok(Some(name.into_owned())),
            Err(_) => std::fs::rename(&tmp, &target).map(|()| None),
        }
    });
    // The tmp is gone after a rename; elsewhere it is a spare name or a half-written file.
    let _ = std::fs::remove_file(&tmp);
    placed
}

// "1 change doesn't fit this HUD." and the like, or nothing for none.
fn changes_dont_fit(n: usize) -> Option<String> {
    match n {
        0 => None,
        1 => Some("1 change doesn't fit this HUD.".to_owned()),
        n => Some(format!("{n} changes don't fit this HUD.")),
    }
}

// The warning after an apply that left changes out.
fn no_longer_fit(n: usize) -> Option<String> {
    (n > 0).then(|| format!("{n} of your changes no longer fit and were left out."))
}

const PICTURES_LEFT_OUT: &str =
    "Pictures aren't included in shared recipes yet, so parts that use them were left out.";

fn fonts_left_out(n: usize) -> Option<String> {
    match n {
        0 => None,
        1 => Some("1 font was left out.".to_owned()),
        n => Some(format!("{n} fonts were left out.")),
    }
}

// The player's line for a refusal they can fix; `None` for a read or write failure.
fn preset_message(err: &PresetError) -> Option<&'static str> {
    match err {
        PresetError::NameBlank => Some(NAME_BLANK),
        PresetError::NameTaken { .. } => Some(NAME_TAKEN),
        PresetError::NoOptions => Some(NO_OPTIONS),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // The folder's file names, sorted.
    fn names(dir: &std::path::Path) -> Vec<String> {
        let mut names: Vec<String> = (std::fs::read_dir(dir).unwrap())
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    }

    #[test]
    fn a_tmp_left_by_a_crash_never_blocks_the_name() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("mine.json.tmp"), b"stale").unwrap();
        let recipe = Recipe::preset("Mine", &[]);
        let written = write_recipe(dir.path().join("mine"), &recipe).unwrap();
        assert!(written.is_none());
        assert_eq!(names(dir.path()), ["mine.json", "mine.json.tmp"]);
        assert_eq!(
            std::fs::read(dir.path().join("mine.json")).unwrap(),
            recipe.to_json()
        );
    }

    #[test]
    fn a_name_the_json_lands_on_is_refused_and_left_alone() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("mine.json"), b"theirs").unwrap();
        let written = write_recipe(dir.path().join("mine"), &Recipe::preset("Mine", &[]));
        assert_eq!(written.unwrap().as_deref(), Some("mine.json"));
        assert_eq!(names(dir.path()), ["mine.json"]);
        assert_eq!(
            std::fs::read(dir.path().join("mine.json")).unwrap(),
            b"theirs"
        );
    }

    #[test]
    fn the_picked_name_itself_is_replaced() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("mine.json"), b"old").unwrap();
        let recipe = Recipe::preset("Mine", &[]);
        let written = write_recipe(dir.path().join("mine.json"), &recipe).unwrap();
        assert!(written.is_none());
        assert_eq!(names(dir.path()), ["mine.json"]);
        assert_eq!(
            std::fs::read(dir.path().join("mine.json")).unwrap(),
            recipe.to_json()
        );
    }

    #[test]
    fn a_failed_list_read_never_says_a_save_failed() {
        assert_eq!(PresetWork::List.failed(), "Couldn't read your presets.");
        assert_eq!(PresetWork::Save.failed(), SAVE_FAILED);
        assert_eq!(PresetWork::Delete.failed(), DELETE_FAILED);
    }
}
