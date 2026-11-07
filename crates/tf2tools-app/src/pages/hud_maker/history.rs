//! The HUD Maker's History section: named snapshots of a saved HUD's changes, listed in the
//! side panel in place of the HUD's own rows, with a name field to keep one and a row per
//! snapshot to compare, restore or delete.

use gpui_kit::component::button::Button;
use gpui_kit::component::input::{Input, InputState};
use gpui_kit::component::{
    ActiveTheme as _, Disableable as _, Selectable as _, Sizable as _, h_flex, v_flex,
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, Context, Entity, InteractiveElement as _, IntoElement, ParentElement as _,
    StatefulInteractiveElement as _, Styled as _, Task, TestSupportExt as _, Window, div,
};
use tf2tools_core::{ChangeTarget, HistoryError, HudId, HudSnapshot, SnapshotEntry};

use super::{FontBytes, HudMakerPage, register_font};
use crate::ids;
use crate::local_messages::left_out;
use crate::services::{OpError, Services, run_blocking};
use crate::shell::confirm::{Confirm, confirm};
use crate::shell::toast::{ToastKind, toast};
use crate::widgets::labelled;

const COMPARE_OFF: &str = "Leave the six-screen view to compare";
const NAME_BLANK: &str = "Give the snapshot a name.";
const TOO_LARGE: &str = "This HUD has too many changes for one snapshot.";
const SAVE_FAILED: &str = "Couldn't save the snapshot.";
const PICTURE_NOT_STORED: &str = "The snapshot wasn't saved because a picture couldn't be stored.";
const DELETE_FAILED: &str = "Couldn't delete the snapshot.";
const UNREADABLE: &str = "That snapshot can't be read.";
const LIST_FAILED: &str = "Couldn't read this HUD's snapshots.";
const UNSAVED: &str = "Save the HUD first, then you can keep snapshots of it here.";
const NO_SNAPSHOTS: &str = "This HUD has no snapshots yet.";

// What a snapshot job did, and the HUD's list as read right after it.
type SnapshotJob = Task<
    Result<
        (
            Result<(), HistoryError>,
            Result<Vec<SnapshotEntry>, HistoryError>,
        ),
        OpError,
    >,
>;

// Which job ran, for its failure words and whether it holds `history_busy`.
#[derive(Clone, Copy, PartialEq)]
enum SnapshotWork {
    List,
    Save,
    Delete,
}

impl HudMakerPage {
    pub fn history_open(&self) -> bool {
        self.history_open
    }

    pub fn snapshots(&self) -> &[SnapshotEntry] {
        &self.snapshots
    }

    /// The HUD the shown list was read for; rows act only while it is the open HUD.
    pub fn snapshots_for(&self) -> Option<&HudId> {
        self.snapshots_for.as_ref()
    }

    pub fn snapshot_name_input(&self) -> &Entity<InputState> {
        &self.snapshot_name
    }

    /// True while a snapshot write runs, during which Save waits.
    pub fn history_busy(&self) -> bool {
        self.history_busy
    }

    /// Shows History in the side panel in place of the HUD section: closes the More menu, ends
    /// a recipe preview, deselects, opens a collapsed panel at its top and reads the list.
    pub fn open_history(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.more_open = false;
        self.start_open = false;
        self.flush(cx);
        if let Some(editor) = self.editor_mut() {
            editor.cancel_preview();
            editor.select(None);
        }
        self.history_open = true;
        self.close_borrow(cx);
        self.open_panel_at_top(cx);
        if self.saved_hud.is_some() {
            self.read_snapshots(cx);
        }
        self.refresh(window, cx);
        cx.notify();
    }

    /// The name of the snapshot on the canvas now, only while the editor holds its layout.
    pub fn comparing(&self) -> Option<&str> {
        self.editor()?.compared()?;
        self.comparing.as_ref().map(|(_, name)| name.as_str())
    }

    // The file of the snapshot being compared, under the same condition as `comparing`.
    fn comparing_file(&self) -> Option<&str> {
        self.comparing()?;
        self.comparing.as_ref().map(|(file, _)| file.as_str())
    }

    /// Where the divider sits across the screen, 0 to 1.
    pub fn compare_split(&self) -> f32 {
        self.compare_split
    }

    /// Moves the divider, kept on the screen; nothing else changes.
    pub fn set_compare_split(&mut self, split: f32, cx: &mut Context<Self>) {
        self.compare_split = if split.is_finite() {
            split.clamp(0., 1.)
        } else {
            0.5
        };
        cx.notify();
    }

    /// Ends the comparison; the HUD and its history are untouched.
    pub fn stop_comparing(&mut self, cx: &mut Context<Self>) {
        if let Some(editor) = self.editor_mut() {
            editor.stop_comparing();
        }
        self.comparing = None;
        self.compare_drag = false;
        cx.notify();
    }

    /// Reads the snapshot in `file` off the UI thread and shows it against the HUD as it is.
    /// A read that lands after another HUD loaded is dropped; a failed one toasts.
    pub fn compare_snapshot(&mut self, file: &str, _: &mut Window, cx: &mut Context<Self>) {
        let Some(id) = self.snapshots_for.clone().filter(|_| !self.every_screen) else {
            return;
        };
        let (file, load, win) = (file.to_owned(), self.loads, self.win);
        let name = file.clone();
        let job = run_blocking(cx, move |s: &Services| {
            Ok(s.library.load_hud_snapshot(&id, &name))
        });
        cx.spawn(async move |page, cx| {
            let landed = job.await;
            cx.update(|cx| {
                let _ = win.update(cx, |_, window, cx| {
                    page.update(cx, |page, cx| match landed {
                        _ if page.loads != load || page.every_screen => {}
                        Ok(Ok(snapshot)) => page.start_comparing(file, &snapshot, cx),
                        failed => {
                            let why = failed.err().map(|e| e.to_string());
                            tracing::debug!(error = ?why, "a snapshot didn't load");
                            toast(window, cx, ToastKind::Error, UNREADABLE, "");
                        }
                    })
                });
            });
        })
        .detach();
    }

    // Lays the snapshot out beside the HUD with the divider back in the middle.
    fn start_comparing(&mut self, file: String, snapshot: &HudSnapshot, cx: &mut Context<Self>) {
        let Some(editor) = self.editor_mut() else {
            return;
        };
        editor.compare_with(snapshot.changes().to_vec());
        // The snapshot's fonts reach the text system so its layer draws them; every file it
        // adds counts, not only each path's winner.
        let fonts: Vec<FontBytes> = (snapshot.changes().iter())
            .filter_map(|c| match c.target() {
                ChangeTarget::Asset(file) => snapshot.assets().get(file.source()),
                ChangeTarget::Set(_)
                | ChangeTarget::Remove(_)
                | ChangeTarget::AddBlock(_)
                | ChangeTarget::Option(_)
                | ChangeTarget::Credit(_)
                | ChangeTarget::Animation(_)
                | ChangeTarget::AnimationAppend(_)
                | _ => None,
            })
            .map(|bytes| FontBytes::new(bytes.to_vec()))
            .collect();
        self.register_fonts(fonts, cx);
        self.comparing = Some((file, snapshot.name().to_owned()));
        self.compare_split = 0.5;
        self.compare_drag = false;
        cx.notify();
    }

    // Registers fonts with the text system and notes their families as known to the canvas.
    pub(super) fn register_fonts(&mut self, fonts: Vec<FontBytes>, cx: &mut Context<Self>) {
        for font in fonts {
            let families = font.families.clone();
            if register_font(font, cx)
                && let super::Load::Ready(loaded) = &mut self.load
            {
                (loaded.font_names).extend(families.iter().map(|f| f.to_lowercase()));
            }
        }
    }

    /// Goes back to the HUD section.
    pub fn close_history(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        self.history_open = false;
        self.stop_comparing(cx);
    }

    /// Reads the saved HUD's snapshots off the UI thread.
    pub(super) fn read_snapshots(&mut self, cx: &mut Context<Self>) {
        let Some(id) = self.saved_hud.clone() else {
            return;
        };
        let job_id = id.clone();
        let job = run_blocking(cx, move |s: &Services| {
            Ok((Ok(()), s.library.list_hud_snapshots(&job_id)))
        });
        self.land_snapshots(id, job, SnapshotWork::List, cx);
    }

    // Keeps a snapshot of the committed changes under the typed name, then re-reads the list.
    fn save_snapshot(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.saving || self.history_busy {
            return;
        }
        self.flush(cx);
        let name = self.snapshot_name.read(cx).value().trim().to_owned();
        if name.is_empty() {
            return toast(window, cx, ToastKind::Error, NAME_BLANK, "");
        }
        let (Some(id), Some((changes, assets))) = (
            self.saved_hud.clone(),
            self.editor()
                .map(|e| (e.changes().to_vec(), e.assets().clone())),
        ) else {
            return;
        };
        self.history_busy = true;
        let job_id = id.clone();
        let job = run_blocking(cx, move |s: &Services| {
            let saved = s
                .library
                .save_hud_snapshot_with_assets(&job_id, &name, &changes, &assets)
                .map(drop);
            Ok((saved, s.library.list_hud_snapshots(&job_id)))
        });
        self.land_snapshots(id, job, SnapshotWork::Save, cx);
        cx.notify();
    }

    // Reads the snapshot off the UI thread, then restores it. A read that lands after another
    // HUD loaded is dropped, and one that fails toasts and changes nothing.
    fn restore_snapshot(&mut self, file: &str, cx: &mut Context<Self>) {
        let Some(id) = self.snapshots_for.clone() else {
            return;
        };
        let (file, load, win) = (file.to_owned(), self.loads, self.win);
        let job = run_blocking(cx, move |s: &Services| {
            Ok(s.library.load_hud_snapshot(&id, &file))
        });
        cx.spawn(async move |page, cx| {
            let landed = job.await;
            cx.update(|cx| {
                let _ = win.update(cx, |_, window, cx| {
                    page.update(cx, |page, cx| match landed {
                        _ if page.loads != load => {}
                        Ok(Ok(snapshot)) => page.apply_snapshot(&snapshot, window, cx),
                        failed => {
                            let why = failed.err().map(|e| e.to_string());
                            tracing::debug!(error = ?why, "a snapshot didn't load");
                            toast(window, cx, ToastKind::Error, UNREADABLE, "");
                        }
                    })
                });
            });
        })
        .detach();
    }

    // Makes the snapshot's changes the HUD's as one undo step and says what happened.
    fn apply_snapshot(
        &mut self,
        snapshot: &HudSnapshot,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.flush(cx);
        let Some(editor) = self.editor_mut() else {
            return;
        };
        let changed = editor.restore(snapshot.changes().to_vec(), snapshot.assets());
        // Fonts the restore brought back reach the text system so the canvas draws them.
        let fonts: Vec<FontBytes> = (editor.model().assets().iter())
            .filter_map(|(_, source)| editor.assets().get(source))
            .map(|bytes| FontBytes::new(bytes.to_vec()))
            .collect();
        self.register_fonts(fonts, cx);
        self.decode_shown_images(cx);
        // The editor's restore ended the comparison; the page forgets which snapshot it was.
        self.comparing = None;
        self.compare_drag = false;
        self.refresh(window, cx);
        cx.notify();
        let name = snapshot.name();
        if !changed {
            let line = format!("Your HUD already matches {name}.");
            return toast(window, cx, ToastKind::Info, line, "");
        }
        let line = format!("Restored {name}.");
        let more = left_out(snapshot.skipped(), false).unwrap_or_default();
        let line = if more.is_empty() {
            line
        } else {
            format!("{line} {more}")
        };
        toast(window, cx, ToastKind::Success, "Snapshot restored", line);
    }

    // Asks first; only a confirmed answer deletes, and a dialog closed unanswered counts as no.
    // The job takes the list's HUD, never a newer one, and a snapshot already gone is done.
    fn delete_snapshot(
        &mut self,
        file: &str,
        name: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.saving || self.history_busy {
            return;
        }
        let Some(id) = self.snapshots_for.clone() else {
            return;
        };
        let answer = confirm(Confirm::delete_snapshot(name), window, cx);
        let (file, load) = (file.to_owned(), self.loads);
        cx.spawn(async move |page, cx| {
            if !answer.recv().await.unwrap_or(false) {
                return;
            }
            let _ = page.update(cx, |page, cx| {
                if page.loads != load || page.history_busy || page.saving {
                    return;
                }
                if page.comparing_file() == Some(file.as_str()) {
                    page.stop_comparing(cx);
                }
                page.history_busy = true;
                let job_id = id.clone();
                let job = run_blocking(cx, move |s: &Services| {
                    let deleted = match s.library.delete_hud_snapshot(&job_id, &file) {
                        Err(HistoryError::NotFound) => Ok(()),
                        done => done,
                    };
                    Ok((deleted, s.library.list_hud_snapshots(&job_id)))
                });
                page.land_snapshots(id, job, SnapshotWork::Delete, cx);
                cx.notify();
            });
        })
        .detach();
    }

    // Lands a snapshot job for HUD `id`: its list replaces the shown one when no later job
    // started and the same HUD is still open, and each failure toasts in player words.
    fn land_snapshots(
        &mut self,
        id: HudId,
        job: SnapshotJob,
        work: SnapshotWork,
        cx: &mut Context<Self>,
    ) {
        self.history_reads = self.history_reads.wrapping_add(1);
        let (read, load, win) = (self.history_reads, self.loads, self.win);
        cx.spawn(async move |page, cx| {
            let landed = job.await;
            cx.update(|cx| {
                let _ = win.update(cx, |_, window, cx| {
                    page.update(cx, |page, cx| {
                        if work != SnapshotWork::List {
                            page.history_busy = false;
                        }
                        // A HUD loaded since owns the page now; this job's list is not for it.
                        if page.loads != load {
                            return cx.notify();
                        }
                        let (done, list) = match landed {
                            Ok(both) => both,
                            Err(err) => {
                                tracing::debug!(error = %err, "a snapshot job failed");
                                let failed = work.failed();
                                return toast(window, cx, ToastKind::Error, failed, "");
                            }
                        };
                        let listed = list.is_ok();
                        match list {
                            Ok(list) if page.history_reads == read => {
                                page.snapshots = list;
                                page.snapshots_for = Some(id);
                            }
                            Ok(_) => {}
                            Err(why) => {
                                tracing::debug!(error = %why, "the snapshots didn't list");
                                toast(window, cx, ToastKind::Error, LIST_FAILED, "");
                            }
                        }
                        match done {
                            Ok(()) if work == SnapshotWork::Save && listed => page
                                .snapshot_name
                                .update(cx, |input, cx| input.set_value("", window, cx)),
                            Ok(()) => {}
                            Err(why) => {
                                tracing::debug!(error = %why, "a snapshot didn't save");
                                // Only a save holding a picked picture writes picture copies.
                                let picked = page.editor().is_some_and(|e| {
                                    e.changes().iter().any(|c| match c.target() {
                                        ChangeTarget::Asset(file) => is_picked(file.source()),
                                        ChangeTarget::Set(_)
                                        | ChangeTarget::Remove(_)
                                        | ChangeTarget::AddBlock(_)
                                        | ChangeTarget::Option(_)
                                        | ChangeTarget::Credit(_)
                                        | ChangeTarget::Animation(_)
                                        | ChangeTarget::AnimationAppend(_)
                                        | _ => false,
                                    })
                                });
                                let words = match (work, &why) {
                                    (SnapshotWork::Delete, _) => DELETE_FAILED.to_owned(),
                                    (_, HistoryError::Io { .. }) if picked => {
                                        PICTURE_NOT_STORED.to_owned()
                                    }
                                    _ => save_message(&why),
                                };
                                toast(window, cx, ToastKind::Error, words, "");
                            }
                        }
                        cx.notify();
                    })
                });
            });
        })
        .detach();
    }

    /// The History section: Back to the HUD, then the name field and the snapshot rows, or a
    /// note when the HUD was never saved.
    pub(super) fn history_rows(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let theme = cx.theme();
        let muted = theme.muted_foreground;
        let back = Button::new(ids::HUD_MAKER_HISTORY_CLOSE)
            .outline()
            .small()
            .label("Back to the HUD")
            .on_click(cx.listener(|page, _, window, cx| page.close_history(window, cx)));
        let mut section = v_flex()
            .id(ids::HUD_MAKER_HISTORY)
            .test_support()
            .gap_2()
            .child(div().child(back));
        if self.saved_hud.is_none() {
            let note = labelled(
                div().text_sm().text_color(muted),
                ids::HUD_MAKER_HISTORY_UNSAVED,
                UNSAVED,
            );
            return vec![section.child(note).into_any_element()];
        }
        let save = Button::new(ids::HUD_MAKER_SNAPSHOT_SAVE)
            .outline()
            .small()
            .label("Save snapshot")
            .disabled(self.saving || self.history_busy)
            .on_click(cx.listener(|page, _, window, cx| page.save_snapshot(window, cx)));
        let name = div()
            .id(ids::HUD_MAKER_SNAPSHOT_NAME)
            .aria_label("Snapshot name")
            .test_support()
            .flex_1()
            .min_w_0()
            .child(Input::new(&self.snapshot_name).small());
        section = section.child(h_flex().gap_2().items_center().child(name).child(save));
        if self.snapshots.is_empty() {
            let note = labelled(
                div().text_sm().text_color(muted),
                ids::HUD_MAKER_HISTORY_EMPTY,
                NO_SNAPSHOTS,
            );
            return vec![section.child(note).into_any_element()];
        }
        // Rows belong to the list's HUD; while that is not the open one they only look.
        let stale = self.snapshots_for != self.saved_hud;
        let rows = self.snapshots.iter().map(|snap| {
            let file = snap.file().to_owned();
            let title = labelled(
                div().text_sm().text_color(theme.foreground).truncate(),
                ids::hud_maker_snapshot(&file),
                snap.name().to_owned(),
            );
            let when = labelled(
                div().text_xs().text_color(muted),
                ids::hud_maker_snapshot_when(&file),
                when_line(snap),
            );
            let comparing_it = self.comparing_file() == Some(file.as_str());
            let compare = Button::new(ids::hud_maker_snapshot_compare(&file))
                .outline()
                .xsmall()
                .label(if comparing_it {
                    "Stop comparing"
                } else {
                    "Compare"
                })
                .selected(comparing_it)
                .disabled(stale || self.every_screen)
                .when(self.every_screen, |b| b.tooltip(COMPARE_OFF))
                .on_click({
                    let file = file.clone();
                    cx.listener(move |page, _, window, cx| {
                        if page.comparing_file() == Some(file.as_str()) {
                            page.stop_comparing(cx);
                        } else {
                            page.compare_snapshot(&file, window, cx);
                        }
                    })
                });
            let restore = Button::new(ids::hud_maker_snapshot_restore(&file))
                .outline()
                .xsmall()
                .label("Restore")
                .disabled(stale)
                .on_click({
                    let file = file.clone();
                    cx.listener(move |page, _, _, cx| page.restore_snapshot(&file, cx))
                });
            let delete = Button::new(ids::hud_maker_snapshot_delete(&file))
                .outline()
                .xsmall()
                .label("Delete")
                .disabled(stale || self.saving || self.history_busy)
                .on_click({
                    let (file, name) = (file.clone(), snap.name().to_owned());
                    cx.listener(move |page, _, window, cx| {
                        page.delete_snapshot(&file, &name, window, cx)
                    })
                });
            v_flex()
                .gap_1()
                .child(title)
                .child(when)
                .child(h_flex().gap_1().child(compare).child(restore).child(delete))
        });
        vec![section.children(rows).into_any_element()]
    }
}

impl SnapshotWork {
    fn failed(self) -> &'static str {
        match self {
            SnapshotWork::List => LIST_FAILED,
            SnapshotWork::Save => SAVE_FAILED,
            SnapshotWork::Delete => DELETE_FAILED,
        }
    }
}

// Whether a kept file is a picture the player picked.
fn is_picked(source: &str) -> bool {
    source.starts_with(".tf2tools/images/")
}

// The player-words toast for a refused save.
fn save_message(why: &HistoryError) -> String {
    match why {
        HistoryError::NameBlank => NAME_BLANK.to_owned(),
        HistoryError::Full { limit } => {
            format!("You have {limit} snapshots. Delete one to save another.")
        }
        HistoryError::TooLarge => TOO_LARGE.to_owned(),
        HistoryError::NotMade { .. }
        | HistoryError::NotFound
        | HistoryError::Damaged { .. }
        | HistoryError::Newer
        | HistoryError::Io { .. }
        | HistoryError::Database { .. }
        | _ => SAVE_FAILED.to_owned(),
    }
}

// "2026-10-27 21:04:09, 3 changes": the local time with seconds, so two snapshots taken in the
// same minute never read alike.
fn when_line(snap: &SnapshotEntry) -> String {
    let time = chrono::DateTime::parse_from_rfc3339(snap.at())
        .map(|at| {
            let local = at.with_timezone(&chrono::Local);
            local.format("%Y-%m-%d %H:%M:%S").to_string()
        })
        .unwrap_or_else(|_| "Unknown".to_owned());
    match snap.changes() {
        1 => format!("{time}, 1 change"),
        n => format!("{time}, {n} changes"),
    }
}
