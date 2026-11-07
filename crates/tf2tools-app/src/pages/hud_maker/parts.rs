//! The parts panel's list: one fixed-height row per part, in model order, indented by depth.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::ops::Range;
use std::rc::Rc;

use gpui_kit::assets::IconName;
use gpui_kit::component::input::Input;
use gpui_kit::component::{ActiveTheme as _, Icon, Sizable as _};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, Context, Hsla, InteractiveElement as _, IntoElement, ParentElement as _, Pixels,
    SharedString, StatefulInteractiveElement as _, Styled as _, TestSupportExt as _, WeakEntity,
    Window, canvas, div, px, uniform_list,
};
use tf2tools_core::{HudModel, PanelRule, Part, PartKey};

use super::HudMakerPage;
use crate::ids;

/// Every row is this tall; `uniform_list` sizes all rows by the first.
pub(super) const ROW_HEIGHT: f32 = 28.;
const INDENT: f32 = 12.;
// A row's right padding clears the list's 16 px always-shown scrollbar with a 4 px gap, so the
// bar never covers a state tag and a press on the tag selects.
const ROW_END: f32 = 16. + 4.;
// The list's tallest; a shorter area shrinks it, since `uniform_list` needs a definite height.
pub(super) const LIST_HEIGHT: f32 = 480.;
// The filter box over the list, with the gap under it.
const FILTER_HEIGHT: f32 = 32.;
// The open panel's height beyond its title and list: the inset and the filter box.
pub(super) const LIST_PADDING: f32 = 8. + FILTER_HEIGHT;
// The fold arrow and the eye are squares this wide.
const CONTROL: f32 = 20.;
// A row's state beside the name: the game hides the part in the preview's situation, or the
// HUD or the player does.
const SHOWS_IN_GAME: &str = "shows in game";
const HIDDEN: &str = "hidden";
// "Shows in game" draws as a small icon, so the names beside it keep the row; its tooltip says it.
const SHOWS_IN_GAME_TIP: &str = "Shows in game only when TF2 needs it";

impl HudMakerPage {
    // A `uniform_list`, so only the visible rows are built however large the HUD is. Rows fit
    // the panel's width: a long name is cut short of its state tag, so nothing scrolls sideways.
    // Docked, the list fills its panel; floating, it is LIST_HEIGHT, shrunk by a short area.
    pub(super) fn part_list(&mut self, docked: bool, cx: &mut Context<Self>) -> AnyElement {
        let rows = self.list_order(cx);
        if rows.is_empty() && self.editor().is_some() {
            let note = div()
                .id(ids::HUD_MAKER_PARTS_NO_MATCH)
                .test_support()
                .p_2()
                .text_sm()
                .opacity(0.7)
                .child("No parts match");
            return match docked {
                true => note.flex_1(),
                false => note.h(px(LIST_HEIGHT)).flex_shrink(1.),
            }
            .into_any_element();
        }
        let list = uniform_list(
            ids::HUD_MAKER_PARTS,
            rows.len(),
            cx.processor(|page, range, window, cx| page.part_rows(range, window, cx)),
        )
        .track_scroll(&self.parts_scroll)
        .test_support()
        .w_full()
        .min_h_0();
        match docked {
            true => list.flex_1(),
            false => list.h(px(LIST_HEIGHT)).flex_shrink(1.),
        }
        .into_any_element()
    }

    // The filter box over the list; its height is part of LIST_PADDING.
    pub(super) fn parts_filter_box(&self) -> AnyElement {
        div()
            .id(ids::HUD_MAKER_PARTS_FILTER)
            .test_support()
            .flex_none()
            .h(px(FILTER_HEIGHT))
            .pb_1()
            .child(
                Input::new(&self.parts_filter)
                    .small()
                    .cleanable(true)
                    .prefix(Icon::new(IconName::Search).small()),
            )
            .into_any_element()
    }

    /// The model rows the list shows, in model order. A filter keeps the parts whose name holds
    /// it, ignoring case, with every group above them, and ignores folds; without one, the
    /// parts inside a folded group are left out.
    pub(super) fn list_order(&mut self, cx: &Context<Self>) -> Rc<[usize]> {
        let query = self.parts_filter.read(cx).value().trim().to_lowercase();
        let mut names = std::mem::take(&mut self.filter_names);
        let rows: Rc<[usize]> = match self.editor() {
            None => Rc::from([]),
            Some(editor) => {
                let model = editor.model();
                let parts = model.parts();
                if !query.is_empty() {
                    let mut keep: HashSet<&PartKey> = HashSet::new();
                    for part in parts {
                        let name = names
                            .entry(part.key().clone())
                            .or_insert_with(|| row_label(part).to_lowercase());
                        if name.contains(&query) {
                            keep.insert(part.key());
                            keep.extend(ancestors(model, part));
                        }
                    }
                    (0..parts.len())
                        .filter(|&i| keep.contains(parts[i].key()))
                        .collect()
                } else if self.folded.is_empty() {
                    (0..parts.len()).collect()
                } else {
                    (0..parts.len())
                        .filter(|&i| !ancestors(model, &parts[i]).any(|k| self.folded.contains(k)))
                        .collect()
                }
            }
        };
        self.filter_names = names;
        self.list_rows = rows.clone();
        rows
    }

    fn part_rows(
        &mut self,
        range: Range<usize>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let Some(editor) = self.editor() else {
            return Vec::new();
        };
        let theme = cx.theme();
        let (primary, primary_fg, fg, hover) = (
            theme.primary,
            theme.primary_foreground,
            theme.foreground,
            theme.accent,
        );
        let radius = theme.radius;
        let page = cx.entity().downgrade();
        let model = editor.model();
        let groups: HashSet<&PartKey> = model.parts().iter().filter_map(Part::parent).collect();
        let shown = &self.list_rows
            [range.start.min(self.list_rows.len())..range.end.min(self.list_rows.len())];
        let rows: Vec<_> = shown
            .iter()
            .filter_map(|&i| model.parts().get(i))
            .map(|part| {
                let key = part.key();
                let hidden = editor
                    .layout()
                    .get(key)
                    .is_some_and(|b| b.hidden().is_some());
                let by_game =
                    super::canvas::game_hidden_root(model, editor.layout(), key).is_some();
                let state = if by_game {
                    Some(SHOWS_IN_GAME)
                } else if hidden {
                    Some(HIDDEN)
                } else {
                    None
                };
                let name = row_label(part);
                let file = (!same_letters(&name, part.name())).then(|| part.name().to_owned());
                Row {
                    key: key.clone(),
                    name,
                    file,
                    state,
                    depth: depth(model, part),
                    selected: editor.selected() == Some(key),
                    group: groups.contains(key).then(|| self.folded.contains(key)),
                    changed: editor.part_changed(key),
                    eye: part
                        .is_readable()
                        .then(|| editor.part_hidden_by_player(key)),
                }
            })
            .collect();
        rows.into_iter()
            .map(|row| {
                let Row {
                    key,
                    name,
                    file,
                    state,
                    depth,
                    selected,
                    ..
                } = &row;
                let (key, depth, selected) = (key.clone(), *depth, *selected);
                let label = SharedString::from(match state {
                    Some(state) => format!("{name} ({state})"),
                    None => name.clone(),
                });
                // The name takes the room left by its state tag, pinned at the row's right in
                // smaller, dimmer text; a name too long for it ends in an ellipsis. The tooltip
                // shows only what the row hides: the whole name when it is cut (or not yet
                // measured), and the element name as the files spell it when the plain name
                // hides it.
                let cut = self.cut_names.borrow().get(&key).copied().unwrap_or(true);
                let tip = match (cut, file.clone()) {
                    (true, Some(file)) => Some(format!("{name}\n{file}")),
                    (true, None) => Some(name.clone()),
                    (false, file) => file,
                };
                let measure = measure_cut(
                    key.clone(),
                    SharedString::from(name.clone()),
                    self.cut_names.clone(),
                    page.clone(),
                );
                let name = div()
                    .id(ids::hud_maker_row_name(&key))
                    .test_support()
                    .relative()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .text_sm()
                    .when_some(tip, |name, tip| {
                        let tip = SharedString::from(tip);
                        name.tooltip(super::tip::tip(ids::HUD_MAKER_ROW_TIP, tip))
                    })
                    .child(name.clone())
                    .child(measure);
                let state = state.map(|state| {
                    let tag = div()
                        .id(ids::hud_maker_row_state(&key))
                        .aria_label(state)
                        .test_support()
                        .flex_none()
                        .ml_2()
                        .opacity(0.7);
                    if state != SHOWS_IN_GAME {
                        return tag.whitespace_nowrap().text_xs().child(state);
                    }
                    tag.size(px(CONTROL))
                        .flex()
                        .items_center()
                        .justify_center()
                        .tooltip(super::tip::tip(ids::HUD_MAKER_TIP, SHOWS_IN_GAME_TIP))
                        .child(Icon::new(IconName::Gamepad2).small())
                });
                // A part hovered on the canvas lights its row as the pointer on the row would.
                let (bg, text) = if selected {
                    (primary, primary_fg)
                } else if self.hovered.as_ref() == Some(&key) {
                    (hover, fg)
                } else {
                    (gpui_kit::transparent_black(), fg)
                };
                let fold = self.fold_arrow(&key, row.group, hover, radius, cx);
                let changed = row.changed.then(|| changed_dot(&key, text));
                let eye = row
                    .eye
                    .map(|hidden| self.eye(&key, hidden, hover, radius, cx));
                let row = div()
                    .id(ids::hud_maker_row(&key))
                    .aria_label(label.clone())
                    .aria_selected(selected)
                    .test_support()
                    .w_full()
                    .h(px(ROW_HEIGHT))
                    .pl(px(4. + INDENT * depth as f32))
                    .pr(px(ROW_END))
                    .flex()
                    .items_center()
                    .rounded(radius)
                    .cursor_pointer()
                    .bg(bg)
                    .text_color(text)
                    .on_hover({
                        let key = key.clone();
                        cx.listener(move |page, hovered: &bool, _, cx| {
                            page.hover_row(&key, *hovered, cx)
                        })
                    })
                    // A row click hands the canvas the keys, as a press on the canvas does, so
                    // the arrows nudge the part rather than type into a field.
                    .on_click(cx.listener(move |page, _, window, cx| {
                        page.select(Some(key.clone()), window, cx);
                        page.focus.focus(window, cx);
                    }))
                    .child(fold)
                    .child(name)
                    .children(changed)
                    .children(state)
                    .children(eye);
                if selected {
                    row.into_any_element()
                } else {
                    row.hover(|row| row.bg(hover)).into_any_element()
                }
            })
            .collect()
    }

    // The arrow before a group's name, or an empty slot of its width so names stay aligned.
    // A press folds or unfolds without selecting the row.
    fn fold_arrow(
        &self,
        key: &PartKey,
        folded: Option<bool>,
        hover: Hsla,
        radius: Pixels,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let slot = div().flex_none().size(px(CONTROL)).mr_0p5();
        let Some(folded) = folded else {
            return slot.into_any_element();
        };
        let key = key.clone();
        slot.id(ids::hud_maker_row_fold(&key))
            .aria_label(if folded { "Unfold" } else { "Fold" })
            .test_support()
            .flex()
            .items_center()
            .justify_center()
            .rounded(radius)
            .hover(|el| el.bg(hover))
            .child(
                Icon::new(match folded {
                    true => IconName::ChevronRight,
                    false => IconName::ChevronDown,
                })
                .small(),
            )
            .on_click(cx.listener(move |page, _, _, cx| {
                cx.stop_propagation();
                page.toggle_fold(&key, cx);
            }))
            .into_any_element()
    }

    // The eye at the row's end: open while the part shows, struck through once the player hid
    // it. A press writes the "Hide this part" switch's change, one undo step, without selecting.
    fn eye(
        &self,
        key: &PartKey,
        hidden: bool,
        hover: Hsla,
        radius: Pixels,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let key = key.clone();
        let words = if hidden {
            "Show this part"
        } else {
            "Hide this part"
        };
        div()
            .id(ids::hud_maker_row_eye(&key))
            .aria_label(words)
            .test_support()
            .flex_none()
            .ml_1()
            .size(px(CONTROL))
            .flex()
            .items_center()
            .justify_center()
            .rounded(radius)
            .when(!hidden, |el| el.opacity(0.6))
            .hover(|el| el.bg(hover).opacity(1.))
            .tooltip(super::tip::tip(ids::HUD_MAKER_TIP, words))
            .child(
                Icon::new(match hidden {
                    true => IconName::EyeOff,
                    false => IconName::Eye,
                })
                .small(),
            )
            .on_click(cx.listener(move |page, _, window, cx| {
                cx.stop_propagation();
                page.edit_selected(window, cx, |e| {
                    if let Err(err) = e.set_part_hidden(&key, !hidden) {
                        tracing::debug!("the HUD Maker refused to hide {key:?}: {err:?}");
                    }
                });
            }))
            .into_any_element()
    }

    fn toggle_fold(&mut self, key: &PartKey, cx: &mut Context<Self>) {
        if !self.folded.remove(key) {
            self.folded.insert(key.clone());
        }
        cx.notify();
    }
}

// What one list row shows, read from the editor before any element is built.
struct Row {
    key: PartKey,
    name: String,
    file: Option<String>,
    state: Option<&'static str>,
    depth: usize,
    selected: bool,
    // `Some(folded)` for a part that holds others.
    group: Option<bool>,
    changed: bool,
    // `Some(hidden by the player)` for a part that can be hidden.
    eye: Option<bool>,
}

// The small dot that marks a part differing from the starting HUD, in the row's text colour.
fn changed_dot(key: &PartKey, colour: Hsla) -> AnyElement {
    div()
        .id(ids::hud_maker_row_changed(key))
        .aria_label("Changed")
        .test_support()
        .flex_none()
        .ml_2()
        .size(px(6.))
        .rounded_full()
        .bg(colour)
        .tooltip(super::tip::tip(
            ids::HUD_MAKER_TIP,
            "Changed from the starting HUD",
        ))
        .into_any_element()
}

impl HudMakerPage {
    // A leave clears the outline only while it still names this row's part, so entering the
    // next row before the last one's leave lands keeps the new outline.
    fn hover_row(&mut self, key: &PartKey, hovered: bool, cx: &mut Context<Self>) {
        let next = if hovered {
            Some(key.clone())
        } else if self.hovered.as_ref() == Some(key) {
            None
        } else {
            return;
        };
        self.hover_on_canvas = false;
        if self.hovered != next {
            self.hovered = next;
            cx.notify();
        }
    }
}

// An invisible layer over a row's name box that measures the name in the box's own text style
// each frame and records whether it is cut; a change redraws the list so the tooltip follows.
fn measure_cut(
    key: PartKey,
    name: SharedString,
    cuts: Rc<RefCell<HashMap<PartKey, bool>>>,
    page: WeakEntity<HudMakerPage>,
) -> impl IntoElement {
    canvas(
        move |bounds, window, _| {
            let style = window.text_style();
            let size = style.font_size.to_pixels(window.rem_size());
            let run = style.to_run(name.len());
            let line = window
                .text_system()
                .shape_line(name.clone(), size, &[run], None);
            // Half a pixel of slack keeps rounding from calling a name that fits cut.
            let cut = line.width > bounds.size.width + px(0.5);
            if cuts.borrow_mut().insert(key.clone(), cut) != Some(cut) {
                let page = page.clone();
                window.on_next_frame(move |_, cx| {
                    let _ = page.update(cx, |_, cx| cx.notify());
                });
            }
        },
        |_, _, _, _| {},
    )
    .absolute()
    .top_0()
    .left_0()
    .size_full()
}

// Whether two names spell the same letters, ignoring case, spaces and underscores.
fn same_letters(a: &str, b: &str) -> bool {
    let letters = |s: &str| {
        (s.chars().filter(|c| c.is_alphanumeric()))
            .flat_map(char::to_lowercase)
            .collect::<String>()
    };
    letters(a) == letters(b)
}

// A root part is named by its panel's rule, any other by a readable form of its element name.
pub(super) fn row_label(part: &Part) -> String {
    match part.owner().filter(|_| part.is_root()) {
        Some(id) => PanelRule::get(id).label().to_owned(),
        None => plain_name(part.name()),
    }
}

// Words that open many element names and tell a player nothing, dropped from the front.
// A lone "C" opens the game's class names ("CHudAccountPanel", "CDamageAccountPanel").
const NOISE: [&[&str]; 4] = [&["player", "status"], &["c"], &["hud"], &["tf"]];

// Shouted team colours, read as plain words rather than acronyms.
const COLOURS: [&str; 2] = ["red", "blue"];

/// `raw` element name in player words: split at underscores and case changes, a leading
/// "PlayerStatus", "C", "Hud" or "TF" dropped, "BG" spelled out, in sentence case with
/// acronyms kept and team colours lower-cased.
pub(super) fn plain_name(raw: &str) -> String {
    let mut words = split_known(split_bg(split_words(raw)));
    for noise in NOISE {
        let leads = words.len() > noise.len()
            && (words.iter().zip(noise)).all(|(w, n)| w.eq_ignore_ascii_case(n));
        if leads {
            words.drain(..noise.len());
        }
    }
    let mut plain = Vec::with_capacity(words.len());
    for (i, word) in words.iter().enumerate() {
        let colour = COLOURS.iter().any(|c| word.eq_ignore_ascii_case(c));
        let acronym = !colour && word.len() > 1 && word.chars().all(|c| !c.is_lowercase());
        plain.push(if word.eq_ignore_ascii_case("bg") {
            "background".to_owned()
        } else if acronym {
            word.clone()
        } else {
            word.to_lowercase()
        });
        if i == 0 {
            plain[0] = capitalise(&plain[0]);
        }
    }
    if plain.is_empty() {
        raw.to_owned()
    } else {
        plain.join(" ")
    }
}

// Breaks at separators, before an upper-case letter that follows a lower-case one or a digit,
// before the last capital of a run that starts a word ("ALonger"), and between letters and digits.
fn split_words(raw: &str) -> Vec<String> {
    let chars: Vec<char> = raw.chars().collect();
    let mut words = Vec::new();
    let mut word = String::new();
    for (i, &c) in chars.iter().enumerate() {
        if !c.is_alphanumeric() {
            words.extend((!word.is_empty()).then(|| std::mem::take(&mut word)));
            continue;
        }
        let prev = i
            .checked_sub(1)
            .map(|p| chars[p])
            .filter(|p| p.is_alphanumeric());
        let next = chars.get(i + 1).copied();
        let breaks = prev.is_some_and(|p| {
            (c.is_uppercase() && (p.is_lowercase() || p.is_ascii_digit()))
                || (c.is_uppercase() && p.is_uppercase() && next.is_some_and(char::is_lowercase))
                || (c.is_ascii_digit() != p.is_ascii_digit())
        });
        if breaks && !word.is_empty() {
            words.push(std::mem::take(&mut word));
        }
        word.push(c);
    }
    words.extend((!word.is_empty()).then_some(word));
    words
}

// A capital run ending in "BG" ("IDBG", "BLUEBG") is two words; case changes cannot see it.
fn split_bg(words: Vec<String>) -> Vec<String> {
    let mut split = Vec::with_capacity(words.len());
    for word in words {
        match word.strip_suffix("BG") {
            Some(head) if !head.is_empty() && head.chars().all(|c| c.is_ascii_uppercase()) => {
                split.push(head.to_owned());
                split.push("BG".to_owned());
            }
            _ => split.push(word),
        }
    }
    split
}

// Common HUD name words, for names the game's authors wrote in one lower-case run.
const KNOWN: [&str; 27] = [
    "ammo",
    "background",
    "bar",
    "bg",
    "button",
    "chat",
    "class",
    "count",
    "health",
    "holder",
    "icon",
    "image",
    "label",
    "list",
    "menu",
    "model",
    "name",
    "panel",
    "player",
    "score",
    "status",
    "target",
    "text",
    "time",
    "value",
    "weapon",
    "kill",
];

// A lower-case run (its first letter may be a capital) made wholly of known words is those
// words ("Classmodelpanel"). Any other run stays whole, so ordinary words that merely contain a known one are never broken.
fn split_known(words: Vec<String>) -> Vec<String> {
    fn pieces(rest: &str) -> Option<Vec<&str>> {
        if rest.is_empty() {
            return Some(Vec::new());
        }
        KNOWN
            .iter()
            .filter(|k| rest.starts_with(**k))
            .find_map(|k| {
                let mut tail = pieces(&rest[k.len()..])?;
                tail.insert(0, k);
                Some(tail)
            })
    }
    let mut split = Vec::with_capacity(words.len());
    for word in words {
        let lower = (word.chars().skip(1)).all(|c| c.is_ascii_lowercase());
        match pieces(&word.to_ascii_lowercase()).filter(|p| lower && p.len() > 1) {
            Some(parts) => split.extend(parts.into_iter().map(str::to_owned)),
            None => split.push(word),
        }
    }
    split
}

fn capitalise(word: &str) -> String {
    let mut chars = word.chars();
    chars.next().map_or_else(String::new, |first| {
        first.to_uppercase().chain(chars).collect()
    })
}

// The groups `part` sits in, nearest first.
fn ancestors<'a>(model: &'a HudModel, part: &'a Part) -> impl Iterator<Item = &'a PartKey> {
    std::iter::successors(part.parent(), |key| model.part(key)?.parent())
}

// How many parents the part has.
pub(super) fn depth(model: &HudModel, part: &Part) -> usize {
    std::iter::successors(part.parent(), |key| model.part(key)?.parent()).count()
}

#[cfg(test)]
mod tests {
    use super::plain_name;

    #[test]
    fn element_names_read_as_player_words() {
        for (raw, plain) in [
            ("PlayerStatusHealthValue", "Health value"),
            (
                "PlayerStatus_MedicSmallBulletResistImageWithALongerName",
                "Medic small bullet resist image with a longer name",
            ),
            ("HudWeaponAmmoBG", "Weapon ammo background"),
            ("TFCharacterImage", "Character image"),
            ("TargetIDBG", "Target ID background"),
            ("TargetIDBG_Spec_Blue", "Target ID background spec blue"),
            ("HudTournamentBLUEBG", "Tournament blue background"),
            ("HudTournamentREDBG", "Tournament red background"),
            ("TournamentREDLabel", "Tournament red label"),
            ("CHudAccountPanel", "Account panel"),
            ("CDamageAccountPanel", "Damage account panel"),
            ("CTFLogoPanel", "CTF logo panel"),
            ("FwdButton", "Fwd button"),
            ("ammo_in_clip", "Ammo in clip"),
            ("Slot1Label", "Slot 1 label"),
            ("classmodelpanel", "Class model panel"),
            ("Classmodelpanel", "Class model panel"),
            ("Classmodelpanel background", "Class model panel background"),
            ("healthbar", "Health bar"),
            ("Classmodelholder", "Class model holder"),
            ("Panelty", "Panelty"),
            ("Hud", "Hud"),
            ("PlayerStatus", "Player status"),
            ("___", "___"),
        ] {
            assert_eq!(plain_name(raw), plain, "{raw}");
        }
    }
}
