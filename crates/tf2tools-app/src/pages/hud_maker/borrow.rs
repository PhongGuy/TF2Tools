//! The HUD Maker's Borrow a part: a part of another library HUD, worked out off the UI thread
//! onto the player's HUD and previewed through the preview bar with its credit and what comes
//! along, so Apply keeps it as one undo step.

use std::ops::Range;
use std::sync::Arc;

use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::{
    ActiveTheme as _, Disableable as _, Selectable as _, Sizable as _, v_flex,
};
use gpui_kit::{
    AnyElement, Context, FontWeight, InteractiveElement as _, IntoElement, ParentElement as _,
    SharedString, Styled as _, TestSupportExt as _, UniformListScrollHandle, Window, div, px,
    uniform_list,
};
use tf2tools_core::{
    BoardMode, BorrowRefused, BorrowSource, Borrowed, Brings, HudEntry, HudFiles, HudId, HudModel,
    LeftBehind, PartKey, Platform, Provenance, Scene, Screen,
};

use super::tip::tipped;
use super::{HudMakerPage, Load, parts, recipes::PreviewNote};
use crate::i18n::{plural, tr};
use crate::ids;
use crate::local_messages::describe_library;
use crate::services::{Services, require_tf2, run_blocking};
use crate::shell::toast::{ToastKind, toast};
use crate::widgets::labelled;
use crate::widgets::scroll::{BAR_WIDTH, scroll_area};

fn hud_unreadable() -> SharedString {
    tr!("hud_maker.borrow.hud_unreadable")
}
fn list_failed() -> SharedString {
    tr!("messages.action.list_hud_library")
}
// How far each level of the parts list steps in, and the list's tallest.
const INDENT: f32 = 12.;
const PARTS_HEIGHT: f32 = 320.;

/// The Borrow a part section's state: whether it shows, the library HUDs it offers (`None`
/// until read), the chosen HUD and its parts (`None` until read), and a count that lets only
/// the latest read land.
#[derive(Default)]
pub(super) struct BorrowSection {
    pub open: bool,
    huds: Option<Vec<(HudId, String)>>,
    chosen: Option<HudId>,
    parts: Option<Vec<SourcePart>>,
    parts_scroll: UniformListScrollHandle,
    reads: u64,
}

/// One part of the chosen HUD as the section lists it.
struct SourcePart {
    key: PartKey,
    label: String,
    depth: usize,
    hidden: bool,
    readable: bool,
}

/// Who and what a previewed borrow credits, and what came along or stayed behind, in plain
/// words for the preview bar and the Borrow section.
#[derive(Debug, Clone, Default)]
pub(super) struct BorrowNote {
    pub hud: String,
    pub author: String,
    pub origin: String,
    pub part: String,
    pub brings: Vec<String>,
    pub left_behind: Vec<String>,
    pub animations: usize,
    // One short sentence per kind of thing left behind, animations apart, for the bar.
    pub kinds: Vec<SharedString>,
}

impl BorrowNote {
    /// The preview bar's line for this borrow.
    pub fn bar_line(&self) -> String {
        let mut line = if self.author.is_empty() {
            tr!("hud_maker.borrow.previewing", hud = self.hud).to_string()
        } else {
            let (hud, author) = (&self.hud, &self.author);
            tr!("hud_maker.borrow.previewing_by", hud = hud, author = author).to_string()
        };
        if self.animations > 0 {
            line.push(' ');
            line.push_str(&tr!("hud_maker.borrow.animations_note"));
        }
        for kind in &self.kinds {
            line.push(' ');
            line.push_str(kind);
        }
        line
    }

    /// The section's credit line, naming the author when the HUD has one.
    pub fn credit_line(&self) -> String {
        let (hud, origin) = (&self.hud, &self.origin);
        if self.author.is_empty() {
            tr!(
                "hud_maker.borrow.credit.anonymous",
                hud = hud,
                origin = origin
            )
            .to_string()
        } else {
            let author = &self.author;
            let line = tr!(
                "hud_maker.borrow.credit.by",
                author = author,
                hud = hud,
                origin = origin
            );
            line.to_string()
        }
    }

    /// What comes with the part, then what was left behind, one line each.
    pub fn comes_with(&self) -> String {
        let mut lines = self.brings.clone();
        if lines.is_empty() {
            lines.push(tr!("hud_maker.borrow.only_the_part").to_string());
        }
        if !self.left_behind.is_empty() {
            lines.push(tr!("hud_maker.borrow.left_behind").to_string());
            lines.extend(self.left_behind.iter().cloned());
        }
        lines.join("\n")
    }
}

/// The toast after Apply, saying plainly when some of the borrow could not be added.
pub(super) fn added_text(part: &str, hud: &str, dropped: usize) -> String {
    match dropped {
        0 => tr!("hud_maker.borrow.added", part = part, hud = hud).to_string(),
        _ => tr!("hud_maker.borrow.added_some", part = part, hud = hud).to_string(),
    }
}

/// The author and where the HUD came from, as the credit written into the player's HUD names
/// them; that file is read by anyone the HUD is shared with, so its words stay English.
pub(super) fn credit_of(entry: &HudEntry) -> (String, String) {
    match entry.provenance() {
        Provenance::GameBanana { gb_id, author } => {
            (author.clone(), format!("GameBanana item {}", gb_id.get()))
        }
        Provenance::Curated { hud_id } => (String::new(), format!("hud-db {hud_id}")),
        Provenance::Local | _ => (String::new(), "a file you imported".to_owned()),
    }
}

// Where the HUD came from, as the Borrow section shows it to the player.
fn shown_origin(entry: &HudEntry) -> String {
    let origin = match entry.provenance() {
        Provenance::GameBanana { gb_id, .. } => {
            let id = gb_id.get().to_string();
            tr!("hud_maker.borrow.origin.gamebanana", id = id)
        }
        Provenance::Curated { hud_id } => tr!("hud_maker.borrow.origin.curated", id = hud_id),
        Provenance::Local | _ => tr!("hud_maker.borrow.origin.local"),
    };
    origin.to_string()
}

fn brings_line(brings: &Brings) -> String {
    let line = match brings {
        Brings::Child(name) => tr!("hud_maker.borrow.brings.part", name = name),
        Brings::Font(name) => tr!("hud_maker.borrow.brings.font", name = name),
        Brings::FontFile(family) => tr!("hud_maker.borrow.brings.font_file", family = family),
        Brings::Colour(name) => tr!("hud_maker.borrow.brings.colour", name = name),
        Brings::Border(name) => tr!("hud_maker.borrow.brings.border", name = name),
        Brings::Image(path) => tr!("hud_maker.borrow.brings.picture", path = path),
        Brings::CodeImage(path) => tr!("hud_maker.borrow.brings.code_picture", path = path),
        Brings::Animation(event) => tr!("hud_maker.borrow.brings.animation", event = event),
        _ => tr!("hud_maker.borrow.brings.anything_else"),
    };
    line.to_string()
}

// A left-behind count as the catalogue's plural count.
fn count(n: usize) -> u64 {
    u64::try_from(n).unwrap_or(u64::MAX)
}

// The section's line for one thing left behind, and the bar's sentence for its kind.
fn left_behind_line(left: &LeftBehind) -> (String, Option<SharedString>) {
    let (line, kind) = match left {
        LeftBehind::Animations(n) => (plural("hud_maker.borrow.animations", count(*n)), None),
        LeftBehind::MissingFont(family) => (
            tr!("hud_maker.borrow.missing_font", family = family),
            Some(tr!("hud_maker.borrow.kind.missing_fonts")),
        ),
        LeftBehind::MissingFile(path) => (
            tr!("hud_maker.borrow.missing_file", path = path),
            Some(tr!("hud_maker.borrow.kind.missing_files")),
        ),
        LeftBehind::CodeKeys(n) => (
            plural("hud_maker.borrow.code_keys", count(*n)),
            Some(tr!("hud_maker.borrow.kind.code_keys")),
        ),
        LeftBehind::UnresolvedPins(n) => (
            plural("hud_maker.borrow.unresolved_pins", count(*n)),
            Some(tr!("hud_maker.borrow.kind.unresolved_pins")),
        ),
        _ => (
            tr!("hud_maker.borrow.not_copied"),
            Some(tr!("hud_maker.borrow.kind.not_copied")),
        ),
    };
    (line.to_string(), kind)
}

/// The plain sentence a refused borrow shows.
fn refused_text(why: &BorrowRefused) -> SharedString {
    match why {
        BorrowRefused::UnknownPart => tr!("hud_maker.borrow.refused.unknown_part"),
        BorrowRefused::Unreadable => tr!("hud_maker.borrow.refused.unreadable"),
        BorrowRefused::SceneMismatch => tr!("hud_maker.borrow.refused.scene_mismatch"),
        BorrowRefused::NoRoom => tr!("hud_maker.borrow.refused.no_room"),
        BorrowRefused::TooMuch => tr!("hud_maker.borrow.refused.too_much"),
        _ => tr!("hud_maker.borrow.refused.any_other"),
    }
}

/// The note for a worked-out borrow of `part` from the HUD in `entry`.
fn note_of(entry: &HudEntry, part: &str, borrowed: &Borrowed) -> BorrowNote {
    let (author, _) = credit_of(entry);
    let mut note = BorrowNote {
        hud: entry.name().to_owned(),
        author,
        origin: shown_origin(entry),
        part: part.to_owned(),
        brings: borrowed.brings().iter().map(brings_line).collect(),
        ..BorrowNote::default()
    };
    for left in borrowed.left_behind() {
        if let LeftBehind::Animations(n) = left {
            note.animations = note.animations.saturating_add(*n);
        }
        let (line, kind) = left_behind_line(left);
        note.left_behind.push(line);
        if let Some(kind) = kind.filter(|k| !note.kinds.contains(k)) {
            note.kinds.push(kind);
        }
    }
    note
}

/// The library HUD `id` over the game's files at `tf_dir`, loaded for `scene` under `mode`;
/// `None` when it cannot be read.
fn open_source(
    s: &Services,
    id: &HudId,
    tf_dir: &std::path::Path,
    (platform, scene, mode): (Platform, Scene, BoardMode),
) -> Option<(HudEntry, HudFiles, HudModel)> {
    let opened = s
        .library
        .borrow_files(id, tf_dir)
        .ok()
        .and_then(|(entry, files)| {
            let model = HudModel::load_scene(&files, platform, scene).ok()?;
            Some((entry, files, model.with_board_mode(mode)))
        });
    if opened.is_none() {
        tracing::debug!(id = id.as_str(), "a HUD to borrow from didn't open");
    }
    opened
}

/// The parts of `model` as the section lists them, laid out at `screen` to tell hidden ones.
fn source_parts(model: &HudModel, screen: Screen) -> Vec<SourcePart> {
    let layout = model.layout(screen);
    (model.parts().iter())
        .map(|part| SourcePart {
            key: part.key().clone(),
            label: parts::row_label(part),
            depth: parts::depth(model, part),
            hidden: layout.get(part.key()).is_some_and(|b| b.hidden().is_some()),
            readable: part.is_readable() || part.is_from_base(),
        })
        .collect()
}

impl HudMakerPage {
    /// Whether the Borrow a part section shows.
    pub fn borrow_open(&self) -> bool {
        self.borrow.open
    }

    /// Whether a borrow is still being worked out.
    pub fn borrowing(&self) -> bool {
        self.borrows_running > 0
    }

    /// Whether the section's list of library HUDs has been read.
    pub fn borrow_listed(&self) -> bool {
        self.borrow.huds.is_some()
    }

    /// Shows Borrow a part in the side panel in place of the HUD section: closes the More menu
    /// and History, ends a preview, deselects, opens the panel at its top and reads the library.
    pub fn open_borrow(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.more_open = false;
        self.start_open = false;
        self.flush(cx);
        self.close_history(window, cx);
        if let Some(editor) = self.editor_mut() {
            editor.cancel_preview();
            editor.select(None);
        }
        self.borrow = BorrowSection {
            open: true,
            reads: self.borrow.reads.wrapping_add(1),
            ..BorrowSection::default()
        };
        self.open_panel_at_top(cx);
        let except = self.saved_hud.clone();
        let (reads, load) = (self.borrow.reads, self.loads);
        let job = run_blocking(cx, move |s: &Services| {
            s.library.list_borrowable(except.as_ref()).map_err(|e| {
                describe_library(&e).into_op(tr!("messages.action.list_borrowable_huds"))
            })
        });
        let win = self.win;
        cx.spawn(async move |page, cx| {
            let landed = job.await;
            cx.update(|cx| {
                let _ = win.update(cx, |_, window, cx| {
                    page.update(cx, |page, cx| {
                        if page.loads != load || page.borrow.reads != reads {
                            return;
                        }
                        let huds = match landed {
                            Ok(huds) => huds,
                            Err(e) => {
                                tracing::debug!(error = %e, "the borrowable HUDs didn't list");
                                toast(window, cx, ToastKind::Error, list_failed(), "");
                                Vec::new()
                            }
                        };
                        let rows = huds.iter().map(|h| (h.id().clone(), h.name().to_owned()));
                        page.borrow.huds = Some(rows.collect());
                        cx.notify();
                    })
                });
            });
        })
        .detach();
        self.refresh(window, cx);
        cx.notify();
    }

    /// Goes back to the HUD section; a borrow being previewed stays under the preview bar.
    pub fn close_borrow(&mut self, cx: &mut Context<Self>) {
        if self.borrow.open {
            self.borrow.open = false;
            cx.notify();
        }
    }

    // Chooses the HUD to borrow from and reads its parts for the scene on screen.
    fn choose_borrow_hud(&mut self, id: HudId, cx: &mut Context<Self>) {
        let tf2 = require_tf2(cx);
        let Some((platform, editor)) = (match &self.load {
            Load::Ready(loaded) => Some((loaded.platform, &loaded.editor)),
            Load::Reading | Load::NoGame | Load::CantOpen(_) => None,
        }) else {
            return;
        };
        let at = (platform, editor.scene(), editor.board_mode());
        let screen = editor.screen();
        self.borrow.chosen = Some(id.clone());
        self.borrow.parts = None;
        self.borrow.reads = self.borrow.reads.wrapping_add(1);
        let (reads, load) = (self.borrow.reads, self.loads);
        let job = run_blocking(cx, move |s: &Services| {
            let tf_dir = tf2?.tf_dir();
            Ok(open_source(s, &id, &tf_dir, at).map(|(_, _, model)| source_parts(&model, screen)))
        });
        let win = self.win;
        cx.spawn(async move |page, cx| {
            let landed = job.await;
            cx.update(|cx| {
                let _ = win.update(cx, |_, window, cx| {
                    page.update(cx, |page, cx| {
                        if page.loads != load || page.borrow.reads != reads {
                            return;
                        }
                        match landed {
                            Ok(Some(rows)) => page.borrow.parts = Some(rows),
                            _ => {
                                page.borrow.chosen = None;
                                toast(window, cx, ToastKind::Error, hud_unreadable(), "");
                            }
                        }
                        cx.notify();
                    })
                });
            });
        })
        .detach();
        cx.notify();
    }

    /// The Borrow a part section: Back to the HUD, a previewed borrow's bar, credit and what
    /// comes with it, then the library HUDs and, once one is chosen, its parts.
    pub(super) fn borrow_rows(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let theme = cx.theme();
        let (muted, fg) = (theme.muted_foreground, theme.foreground);
        let back = Button::new(ids::HUD_MAKER_BORROW_CLOSE)
            .outline()
            .small()
            .label(tr!("hud_maker.borrow.back"))
            .on_click(cx.listener(|page, _, _, cx| page.close_borrow(cx)));
        let heading = |text: SharedString| {
            div()
                .text_sm()
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(fg)
                .child(text)
        };
        let mut section = v_flex()
            .id(ids::HUD_MAKER_BORROW)
            .test_support()
            .gap_2()
            .child(div().child(back));
        let previewed = self
            .editor()
            .filter(|e| e.previewing())
            .and(self.preview.borrow());
        if let Some(note) = previewed {
            section = section
                .children(self.preview_bar(cx))
                .child(labelled(
                    div().text_sm().text_color(fg),
                    ids::HUD_MAKER_BORROW_CREDIT,
                    note.credit_line(),
                ))
                .child(heading(tr!("hud_maker.borrow.comes_with")))
                .child(labelled(
                    div().text_sm().text_color(muted),
                    ids::HUD_MAKER_BORROW_COMES_WITH,
                    note.comes_with(),
                ));
        }
        let Some(huds) = &self.borrow.huds else {
            let reading = div()
                .text_sm()
                .text_color(muted)
                .child(tr!("hud_maker.borrow.reading_library"));
            return vec![section.child(reading).into_any_element()];
        };
        if huds.is_empty() {
            let note = labelled(
                div().text_sm().text_color(muted),
                ids::HUD_MAKER_BORROW_EMPTY,
                tr!("hud_maker.borrow.nothing"),
            );
            return vec![section.child(note).into_any_element()];
        }
        section = section.child(heading(tr!("hud_maker.borrow.borrow_from")));
        for (id, name) in huds {
            let chosen = self.borrow.chosen.as_ref() == Some(id);
            let pick = Button::new(ids::hud_maker_borrow_hud(id))
                .ghost()
                .small()
                .label(name.clone())
                .selected(chosen)
                .on_click({
                    let id = id.clone();
                    cx.listener(move |page, _, _, cx| page.choose_borrow_hud(id.clone(), cx))
                });
            section = section.child(div().min_w_0().overflow_hidden().child(pick));
        }
        if self.borrow.chosen.is_none() {
            return vec![section.into_any_element()];
        }
        let Some(rows) = &self.borrow.parts else {
            let reading = div()
                .text_sm()
                .text_color(muted)
                .child(tr!("hud_maker.borrow.reading_hud"));
            return vec![section.child(reading).into_any_element()];
        };
        // Only the rows in view are built, however many parts the HUD has.
        let tall = (rows.len() as f32 * parts::ROW_HEIGHT).min(PARTS_HEIGHT);
        let list = uniform_list(
            ids::HUD_MAKER_BORROW_PARTS,
            rows.len(),
            cx.processor(|page, range, _, cx| page.borrow_part_rows(range, cx)),
        )
        .track_scroll(&self.borrow.parts_scroll)
        .test_support()
        .w_full()
        .h(px(tall))
        .pr(BAR_WIDTH);
        let bar = ids::HUD_MAKER_BORROW_PARTS_SCROLLBAR;
        let list = scroll_area(bar, &self.borrow.parts_scroll, list);
        section = section
            .child(heading(tr!("hud_maker.borrow.choose_part")))
            .child(list);
        vec![section.into_any_element()]
    }

    // The chosen HUD's part rows in `range`: a button each, stepped in by depth.
    fn borrow_part_rows(&mut self, range: Range<usize>, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let Some(rows) = &self.borrow.parts else {
            return Vec::new();
        };
        let source = self.borrow.chosen.clone();
        (rows.get(range).unwrap_or_default().iter())
            .map(|row| {
                let id = ids::hud_maker_borrow_part(&row.key);
                let label: SharedString = match row.hidden {
                    true => tr!("hud_maker.borrow.hidden_there", name = row.label),
                    false => row.label.clone().into(),
                };
                let (source, key) = (source.clone(), row.key.clone());
                let pick = Button::new(id.clone())
                    .ghost()
                    .small()
                    .label(label)
                    .disabled(!row.readable)
                    .on_click(cx.listener(move |page, _, window, cx| {
                        if let Some(source) = source.clone() {
                            page.start_borrow(source, key.clone(), window, cx);
                        }
                    }));
                let pick = match row.readable {
                    true => pick.into_any_element(),
                    false => tipped(&id, tr!("hud_maker.borrow.part_unreadable"), pick),
                };
                let indent = px(INDENT * row.depth as f32);
                div()
                    .h(px(parts::ROW_HEIGHT))
                    .pl(indent)
                    .min_w_0()
                    .overflow_hidden()
                    .child(pick)
                    .into_any_element()
            })
            .collect()
    }

    /// Works out borrowing `part` of library HUD `source_id` onto the HUD as the player's
    /// changes leave it, off the UI thread, then previews it. A result that lands after another
    /// HUD loaded or the changes moved on is dropped; a refusal or unreadable HUD toasts.
    pub fn start_borrow(
        &mut self,
        source_id: HudId,
        part: PartKey,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.flush(cx);
        self.close_option_picker(cx);
        let tf2 = require_tf2(cx);
        let Load::Ready(loaded) = &mut self.load else {
            return;
        };
        // The borrow fits the HUD as the player left it, not one a preview shows.
        loaded.editor.cancel_preview();
        let editor = &loaded.editor;
        let target = editor.model().clone();
        let (scene, mode, group) = (editor.scene(), editor.board_mode(), editor.next_group());
        let changes = editor.changes().to_vec();
        let (files, platform) = (Arc::clone(&loaded.files), loaded.platform);
        let load = self.loads;
        self.preview_seq = self.preview_seq.wrapping_add(1);
        let seq = self.preview_seq;
        self.borrow_latest = seq;
        self.borrows_running = self.borrows_running.saturating_add(1);
        let job = run_blocking(cx, move |s: &Services| {
            let tf_dir = tf2?.tf_dir();
            let at = (platform, scene, mode);
            let Some((entry, source_files, model)) = open_source(s, &source_id, &tf_dir, at) else {
                return Ok(Err(hud_unreadable()));
            };
            let name = model.part(&part).map_or("", |p| p.name()).to_owned();
            let (author, origin) = credit_of(&entry);
            let source = BorrowSource::new(model, source_files, entry.name(), &author, &origin);
            Ok(match target.borrow_part(&source, &files, &part, group) {
                Ok(borrowed) => {
                    let note = note_of(&entry, &name, &borrowed);
                    Ok((borrowed, note))
                }
                Err(why) => Err(refused_text(&why)),
            })
        });
        let win = self.win;
        cx.spawn(async move |page, cx| {
            let landed = job.await;
            cx.update(|cx| {
                let _ = win.update(cx, |_, window, cx| {
                    page.update(cx, |page, cx| {
                        page.borrows_running = page.borrows_running.saturating_sub(1);
                        // Only the latest borrow lands, on the changes it was worked out for and
                        // never over a preview started after it.
                        let editor = page.editor();
                        let current = editor.is_some_and(|e| e.changes() == changes);
                        let newer =
                            editor.is_some_and(|e| e.previewing()) && page.preview.seq > seq;
                        match landed {
                            _ if page.loads != load || !current => {}
                            _ if page.borrow_latest != seq || newer => {}
                            Ok(Ok((borrowed, note))) => {
                                page.show_borrow(&borrowed, note, seq, window, cx)
                            }
                            Ok(Err(text)) => toast(window, cx, ToastKind::Error, text, ""),
                            Err(e) => {
                                tracing::debug!(error = %e, "a borrow didn't run");
                                toast(window, cx, ToastKind::Error, hud_unreadable(), "");
                            }
                        }
                    })
                });
            });
        })
        .detach();
    }

    // Previews a worked-out borrow through the preview bar, which heads the opened panel.
    fn show_borrow(
        &mut self,
        borrowed: &Borrowed,
        note: BorrowNote,
        seq: u64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(editor) = self.editor_mut() else {
            return;
        };
        let dropped = editor.preview_changes(borrowed.changes().to_vec(), borrowed.assets());
        self.register_preview_fonts(cx);
        self.preview = PreviewNote::borrowed(note, dropped, seq);
        self.decode_shown_images(cx);
        self.open_panel_at_top(cx);
        self.refresh(window, cx);
        cx.notify();
    }
}

#[cfg(test)]
mod tests {
    use super::{added_text, left_behind_line};

    #[test]
    fn left_behind_lines_count_what_stayed() {
        use tf2tools_core::LeftBehind;
        let line = |left: LeftBehind| left_behind_line(&left).0;
        let commands = "console command";
        assert_eq!(
            line(LeftBehind::Animations(1)),
            format!("Animations: 1 {commands} left out.")
        );
        assert_eq!(
            line(LeftBehind::Animations(2)),
            format!("Animations: 2 {commands}s left out.")
        );
        let sets = "of this part's settings itself.";
        assert_eq!(
            line(LeftBehind::CodeKeys(1)),
            format!("The game sets 1 {sets}")
        );
        assert_eq!(
            line(LeftBehind::CodeKeys(2)),
            format!("The game sets 2 {sets}")
        );
        let away = "coming along, so it may sit in a different place.";
        assert_eq!(
            line(LeftBehind::UnresolvedPins(1)),
            format!("1 link to a part that isn't {away}")
        );
        assert_eq!(
            line(LeftBehind::UnresolvedPins(3)),
            format!("3 links to parts that aren't {away}")
        );
    }

    #[test]
    fn the_apply_toast_says_when_some_of_the_borrow_was_not_added() {
        assert_eq!(added_text("Health", "Mine", 0), "Added Health from Mine.");
        assert_eq!(
            added_text("Health", "Mine", 2),
            "Added Health from Mine. Some of it couldn't be added."
        );
    }
}
