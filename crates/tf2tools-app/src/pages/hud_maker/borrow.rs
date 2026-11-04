//! The HUD Maker's Borrow a part: a part of another library HUD, worked out off the UI thread
//! onto the player's HUD and previewed through the preview bar with its credit and what comes
//! along, so Apply keeps it as one undo step.

use std::ops::Range;

use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::{
    ActiveTheme as _, Disableable as _, Selectable as _, Sizable as _, v_flex,
};
use gpui_kit::{
    AnyElement, Context, FontWeight, InteractiveElement as _, IntoElement, ParentElement as _,
    Styled as _, TestSupportExt as _, Window, div, px, uniform_list,
};
use tf2tools_core::{
    BoardMode, BorrowRefused, BorrowSource, Borrowed, Brings, HudEntry, HudFiles, HudId, HudModel,
    LeftBehind, PartKey, Platform, Provenance, Scene, Screen,
};

use super::tip::tipped;
use super::{HudMakerPage, Load, parts, recipes::PreviewNote};
use crate::ids;
use crate::services::{OpError, Services, require_tf2, run_blocking};
use crate::shell::toast::{ToastKind, toast};
use crate::widgets::labelled;

const HUD_UNREADABLE: &str = "Couldn't read that HUD.";
const LIST_FAILED: &str = "Couldn't read your HUDs library.";
const NOTHING_TO_BORROW: &str =
    "No other HUD in your library has parts to borrow. Download one from the HUD store first.";
const PART_UNREADABLE: &str = "That HUD's file for this part can't be read.";
const HIDDEN_THERE: &str = "hidden in that HUD";
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
const ANIMATIONS_NOTE: &str =
    "Some of its animations run console commands, so those were left out.";

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
    pub kinds: Vec<&'static str>,
}

impl BorrowNote {
    /// The preview bar's line for this borrow.
    pub fn bar_line(&self) -> String {
        let mut line = if self.author.is_empty() {
            format!("Previewing a part from {}.", self.hud)
        } else {
            format!("Previewing a part from {}, by {}.", self.hud, self.author)
        };
        if self.animations > 0 {
            line.push(' ');
            line.push_str(ANIMATIONS_NOTE);
        }
        for kind in &self.kinds {
            line.push(' ');
            line.push_str(kind);
        }
        line
    }

    /// The section's credit line, naming the author when the HUD has one.
    pub fn credit_line(&self) -> String {
        let whose = if self.author.is_empty() {
            "This copies work from".to_owned()
        } else {
            format!("This copies {}'s work from", self.author)
        };
        format!(
            "{whose} {} ({}). Check that HUD's page for its terms before you share yours. Your HUD will list the credit.",
            self.hud, self.origin
        )
    }

    /// What comes with the part, then what was left behind, one line each.
    pub fn comes_with(&self) -> String {
        let mut lines = self.brings.clone();
        if lines.is_empty() {
            lines.push("Only the part itself.".to_owned());
        }
        if !self.left_behind.is_empty() {
            lines.push("Left behind:".to_owned());
            lines.extend(self.left_behind.iter().cloned());
        }
        lines.join("\n")
    }
}

/// The toast after Apply, saying plainly when some of the borrow could not be added.
pub(super) fn added_text(part: &str, hud: &str, dropped: usize) -> String {
    match dropped {
        0 => format!("Added {part} from {hud}."),
        _ => format!("Added {part} from {hud}. Some of it couldn't be added."),
    }
}

/// The author and where the HUD came from, as its credit names them.
pub(super) fn credit_of(entry: &HudEntry) -> (String, String) {
    match entry.provenance() {
        Provenance::GameBanana { gb_id, author } => {
            (author.clone(), format!("GameBanana item {}", gb_id.get()))
        }
        Provenance::Curated { hud_id } => (String::new(), format!("hud-db {hud_id}")),
        _ => (String::new(), "a file you imported".to_owned()),
    }
}

fn brings_line(brings: &Brings) -> String {
    match brings {
        Brings::Child(name) => format!("Part {name}"),
        Brings::Font(name) => format!("Font {name}"),
        Brings::FontFile(family) => format!("The font file for {family}"),
        Brings::Colour(name) => format!("Colour {name}"),
        Brings::Border(name) => format!("Border {name}"),
        Brings::Image(path) => format!("Picture {path}"),
        Brings::CodeImage(path) => format!("Picture {path}, in place of the game's"),
        Brings::Animation(event) => format!("Animation {event}"),
        _ => "Something else the part needs".to_owned(),
    }
}

fn plural(n: usize, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}

// The section's line for one thing left behind, and the bar's sentence for its kind.
fn left_behind_line(left: &LeftBehind) -> (String, Option<&'static str>) {
    match left {
        LeftBehind::Animations(n) => (
            format!(
                "Animations: {} left out.",
                plural(*n, "console command", "console commands")
            ),
            None,
        ),
        LeftBehind::MissingFont(family) => (
            format!(
                "Font {family}: that HUD does not include the file, so the game uses a stand-in."
            ),
            Some("Some of its fonts aren't included, so the game uses stand-ins."),
        ),
        LeftBehind::MissingFile(path) => (
            format!("Picture {path}: that HUD does not have it."),
            Some("Some of its pictures are missing from that HUD."),
        ),
        LeftBehind::CodeKeys(n) => (
            format!("The game sets {n} of this part's settings itself."),
            Some("The game sets some of its settings itself."),
        ),
        LeftBehind::UnresolvedPins(n) => (
            format!(
                "{} that aren't coming along, so it may sit in a different place.",
                plural(*n, "link to a part", "links to parts")
            ),
            Some("It may sit in a different place."),
        ),
        _ => (
            "Something the part uses could not be copied.".to_owned(),
            Some("Something it uses could not be copied."),
        ),
    }
}

/// The plain sentence a refused borrow shows.
fn refused_text(why: &BorrowRefused) -> &'static str {
    match why {
        BorrowRefused::UnknownPart => "That HUD doesn't have this part on this screen.",
        BorrowRefused::Unreadable => "That part can't be read, so it can't be borrowed.",
        BorrowRefused::SceneMismatch => "That part is on a different screen of the HUD.",
        BorrowRefused::NoRoom => "There's nowhere in your HUD to put that part.",
        BorrowRefused::TooMuch => "That part has more pictures than a HUD can hold.",
        _ => "That part can't be borrowed.",
    }
}

/// The note for a worked-out borrow of `part` from the HUD in `entry`.
fn note_of(entry: &HudEntry, part: &str, borrowed: &Borrowed) -> BorrowNote {
    let (author, origin) = credit_of(entry);
    let mut note = BorrowNote {
        hud: entry.name().to_owned(),
        author,
        origin,
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
            s.library
                .list_borrowable(except.as_ref())
                .map_err(|e| OpError::core("List the HUDs to borrow from", e))
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
                                toast(window, cx, ToastKind::Error, LIST_FAILED, "");
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
            _ => None,
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
                                toast(window, cx, ToastKind::Error, HUD_UNREADABLE, "");
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
            .label("Back to the HUD")
            .on_click(cx.listener(|page, _, _, cx| page.close_borrow(cx)));
        let heading = |text: &'static str| {
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
                .child(heading("Comes with it"))
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
                .child("Reading your library...");
            return vec![section.child(reading).into_any_element()];
        };
        if huds.is_empty() {
            let note = labelled(
                div().text_sm().text_color(muted),
                ids::HUD_MAKER_BORROW_EMPTY,
                NOTHING_TO_BORROW,
            );
            return vec![section.child(note).into_any_element()];
        }
        section = section.child(heading("Borrow from"));
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
                .child("Reading that HUD...");
            return vec![section.child(reading).into_any_element()];
        };
        // Only the rows in view are built, however many parts the HUD has.
        let tall = (rows.len() as f32 * parts::ROW_HEIGHT).min(PARTS_HEIGHT);
        let list = uniform_list(
            ids::HUD_MAKER_BORROW_PARTS,
            rows.len(),
            cx.processor(|page, range, _, cx| page.borrow_part_rows(range, cx)),
        )
        .test_support()
        .w_full()
        .h(px(tall));
        section = section.child(heading("Choose a part")).child(list);
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
                let label = match row.hidden {
                    true => format!("{} ({HIDDEN_THERE})", row.label),
                    false => row.label.clone(),
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
                    false => tipped(&id, PART_UNREADABLE, pick),
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
        let (files, platform) = (loaded.files.clone(), loaded.platform);
        let load = self.loads;
        self.preview_seq = self.preview_seq.wrapping_add(1);
        let seq = self.preview_seq;
        self.borrow_latest = seq;
        self.borrows_running = self.borrows_running.saturating_add(1);
        let job = run_blocking(cx, move |s: &Services| {
            let tf_dir = tf2?.tf_dir();
            let at = (platform, scene, mode);
            let Some((entry, source_files, model)) = open_source(s, &source_id, &tf_dir, at) else {
                return Ok(Err(HUD_UNREADABLE));
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
                                toast(window, cx, ToastKind::Error, HUD_UNREADABLE, "");
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
    use super::added_text;

    #[test]
    fn the_apply_toast_says_when_some_of_the_borrow_was_not_added() {
        assert_eq!(added_text("Health", "Mine", 0), "Added Health from Mine.");
        assert_eq!(
            added_text("Health", "Mine", 2),
            "Added Health from Mine. Some of it couldn't be added."
        );
    }
}
