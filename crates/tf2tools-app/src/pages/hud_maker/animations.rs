//! The side panel's Animations section: every event that moves or recolours the selected part,
//! one plain sentence per row, with a row editor, a delete and an Add row. Each edit is one
//! undo step that rewrites the event's own lines.

use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::color_picker::ColorPicker;
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::{ActiveTheme as _, Selectable as _, Sizable as _, h_flex, v_flex};
use gpui_kit::{
    AnyElement, AppContext as _, Context, Entity, FontWeight, InteractiveElement as _, IntoElement,
    ParentElement as _, SharedString, StatefulInteractiveElement as _, Styled as _, Subscription,
    TestSupportExt as _, Window, div,
};
use tf2tools_core::{AnimCommand, AnimEvent, Interp};

use super::HudMakerPage;
use super::editor::{AnimEdit, AnimRefused};
use crate::ids;
use crate::shell::toast::{ToastKind, toast};
use crate::widgets::Named as _;
use crate::widgets::labelled;
use gpui_kit::assets::IconName;

/// What a locked event says, in player words.
pub const RUNS_GAME_COMMANDS: &str =
    "This animation runs game commands, so it can't be changed here.";
const FROM_FLASH: &str = "Added by the hit flash";
const FROM_BORROW: &str = "Added by a borrow";
/// The event the hit flash fills, and the one that restarts it.
const HIT_MARKER: &str = "TF2ToolsHitMarker";
/// The curves the row editor offers.
const CURVES: [(Interp, &str); 5] = [
    (Interp::Linear, "Linear"),
    (Interp::Accel, "Accel"),
    (Interp::Deaccel, "Deaccel"),
    (Interp::Spline, "Spline"),
    (Interp::Bounce, "Bounce"),
];
/// The fields Add row offers, with their player words.
const ADD_FIELDS: [(&str, &str); 4] = [
    ("FgColor", "Colour"),
    ("Alpha", "Fade"),
    ("Position", "Move"),
    ("Size", "Resize"),
];

/// The colour picker's row key for row `row` of `event`, which no file key can be.
pub(super) fn anim_key(event: &str, row: usize) -> String {
    format!("<anim>{row}#{event}")
}

/// The event and row an `anim_key` names.
pub(super) fn parse_anim_key(key: &str) -> Option<(String, usize)> {
    let (row, event) = key.strip_prefix("<anim>")?.split_once('#')?;
    Some((event.to_owned(), row.parse().ok()?))
}

/// The command as one plain sentence: what it does, to what, and when.
pub fn anim_sentence(command: &AnimCommand) -> String {
    match command {
        AnimCommand::Animate {
            field,
            value,
            start,
            duration,
            ..
        } => {
            let verb = match field.to_ascii_lowercase().as_str() {
                "fgcolor" => "Turns colour to".to_owned(),
                "bgcolor" => "Turns background to".to_owned(),
                "alpha" => "Fades to".to_owned(),
                "xpos" | "ypos" | "position" => "Moves to".to_owned(),
                "wide" | "tall" | "size" => "Resizes to".to_owned(),
                _ => format!("Changes {field} to"),
            };
            format!("{verb} {value} over {duration} s, starting at {start} s")
        }
        AnimCommand::RunEvent { event, delay } => format!("Runs {event} at {delay} s"),
        AnimCommand::StopEvent { event, delay } => format!("Stops {event} at {delay} s"),
        AnimCommand::SetVisible { visible, delay, .. } => match visible {
            true => format!("Shows at {delay} s"),
            false => format!("Hides at {delay} s"),
        },
        AnimCommand::FireCommand { delay, .. } => format!("Runs a console command at {delay} s"),
        AnimCommand::RunEventChild { event, delay, .. } => {
            format!("Runs {event} inside it at {delay} s")
        }
        AnimCommand::SetInputEnabled { delay, .. } => format!("SetInputEnabled at {delay} s"),
        AnimCommand::PlaySound { delay, .. } => format!("PlaySound at {delay} s"),
        AnimCommand::StopAnimation { delay, .. } => format!("StopAnimation at {delay} s"),
        AnimCommand::StopPanelAnimations { delay, .. } => {
            format!("StopPanelAnimations at {delay} s")
        }
        AnimCommand::SetFont { delay, .. } => format!("SetFont at {delay} s"),
        AnimCommand::SetTexture { delay, .. } => format!("SetTexture at {delay} s"),
        AnimCommand::SetString { delay, .. } => format!("SetString at {delay} s"),
        _ => "Changes it".to_owned(),
    }
}

/// The row open in the editor and the editor's three fields.
pub(super) struct AnimSection {
    open: Option<(String, usize)>,
    value: Entity<InputState>,
    start: Entity<InputState>,
    duration: Entity<InputState>,
    _subscriptions: Vec<Subscription>,
}

/// One of the row editor's typed fields.
#[derive(Clone, Copy)]
enum Typed {
    Value,
    Start,
    Duration,
}

impl AnimSection {
    pub(super) fn new(window: &mut Window, cx: &mut Context<HudMakerPage>) -> AnimSection {
        let [value, start, duration] = [(); 3].map(|_| cx.new(|cx| InputState::new(window, cx)));
        // Enter and leaving a field commit it, as the part's own fields do.
        let subscriptions = [
            (Typed::Value, &value),
            (Typed::Start, &start),
            (Typed::Duration, &duration),
        ]
        .map(|(typed, input)| {
            cx.subscribe_in(
                input,
                window,
                move |page, _, ev: &InputEvent, window, cx| {
                    if matches!(ev, InputEvent::PressEnter { .. } | InputEvent::Blur) {
                        page.commit_typed_anim(typed, window, cx);
                    }
                },
            )
        })
        .into();
        AnimSection {
            open: None,
            value,
            start,
            duration,
            _subscriptions: subscriptions,
        }
    }
}

impl HudMakerPage {
    // Makes one animation edit as one undo step; a refusal changes nothing.
    fn edit_anim(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
        edit: impl FnOnce(&mut super::editor::HudEditor) -> Result<(), AnimRefused>,
    ) {
        let mut refused = None;
        self.edit_selected(window, cx, |e| refused = edit(e).err());
        match refused {
            Some(AnimRefused::RunsCommands) => {
                toast(window, cx, ToastKind::Error, RUNS_GAME_COMMANDS, "")
            }
            Some(err) => tracing::debug!("the HUD Maker refused an animation edit: {err:?}"),
            None => {}
        }
    }

    // Commits what was typed into one of the row editor's fields to the open row.
    fn commit_typed_anim(&mut self, typed: Typed, window: &mut Window, cx: &mut Context<Self>) {
        let Some((event, row)) = self.anims.open.clone() else {
            return;
        };
        let input = match typed {
            Typed::Value => &self.anims.value,
            Typed::Start => &self.anims.start,
            Typed::Duration => &self.anims.duration,
        };
        let text = input.read(cx).value().trim().to_owned();
        let shown = self.anim_row(&event, row).map(|c| field_text(&c, typed));
        if shown.as_deref() == Some(text.as_str()) {
            return;
        }
        let seconds = text.parse::<f32>().unwrap_or(f32::NAN);
        let edit = match typed {
            Typed::Value => AnimEdit::Value(text),
            Typed::Start => AnimEdit::Start(seconds),
            Typed::Duration => AnimEdit::Duration(seconds),
        };
        self.edit_anim(window, cx, |e| e.edit_animation(&event, row, edit));
        self.load_anim_fields(window, cx);
    }

    // The open event's row `row`, as the editor holds it now.
    fn anim_row(&self, event: &str, row: usize) -> Option<AnimCommand> {
        let e = self.editor()?.model().animations().event(event)?;
        e.commands().get(row).cloned()
    }

    // Opens `row` of `event` in the row editor, or closes it when it is open.
    fn toggle_anim_row(
        &mut self,
        event: String,
        row: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let key = Some((event, row));
        self.anims.open = if self.anims.open == key { None } else { key };
        self.load_anim_fields(window, cx);
        cx.notify();
    }

    // Writes the open row's value, start and length into the editor's fields.
    fn load_anim_fields(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(command) = (self.anims.open.clone()).and_then(|(e, r)| self.anim_row(&e, r))
        else {
            return;
        };
        for (typed, input) in [
            (Typed::Value, self.anims.value.clone()),
            (Typed::Start, self.anims.start.clone()),
            (Typed::Duration, self.anims.duration.clone()),
        ] {
            let text = field_text(&command, typed);
            input.update(cx, |st, cx| st.set_value(text, window, cx));
        }
    }

    /// The Animations section for the selection; nothing when no event names it.
    pub(super) fn animations_section(&mut self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let Some(editor) = self.editor() else {
            return Vec::new();
        };
        let events = editor.animations_selected();
        // A part no event names gets no section, so the panel stays as short as before.
        if events.is_empty() {
            return Vec::new();
        }
        let theme = cx.theme();
        let (muted, border) = (theme.muted_foreground, theme.border);
        let open = self.anims.open.clone();
        let mut out = vec![
            div()
                .id(ids::HUD_MAKER_ANIMATIONS)
                .test_support()
                .pt_2()
                .border_t_1()
                .border_color(border)
                .text_xs()
                .text_color(muted)
                .child("Animations")
                .into_any_element(),
        ];
        for event in &events {
            out.push(self.event_rows(event, open.as_ref(), cx));
        }
        out
    }

    // One event: its name, its rows and, unless locked, each row's controls.
    fn event_rows(
        &self,
        event: &AnimEvent,
        open: Option<&(String, usize)>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let muted = cx.theme().muted_foreground;
        let name = event.name().to_owned();
        let locked = event.runs_game_commands()
            || (self.editor()).is_some_and(|e| e.model().can_change_animation(&name).is_err());
        let mut rows = v_flex().gap_1().child(
            div()
                .text_sm()
                .font_weight(FontWeight::MEDIUM)
                .child(name.clone()),
        );
        if locked {
            rows = rows.child(labelled(
                div().text_xs().text_color(muted),
                ids::hud_maker_anim_locked(&name),
                RUNS_GAME_COMMANDS,
            ));
        }
        for (i, command) in event.commands().iter().enumerate() {
            let sentence = labelled(
                div().flex_1().min_w_0().text_xs(),
                ids::hud_maker_anim_row(&name, i),
                SharedString::from(anim_sentence(command)),
            );
            let mut line = h_flex().gap_1().items_center().child(sentence);
            if i >= event.own_len() {
                let flash = name.eq_ignore_ascii_case(HIT_MARKER)
                    || matches!(command, AnimCommand::RunEvent { event, .. }
                        | AnimCommand::StopEvent { event, .. } if event.eq_ignore_ascii_case(HIT_MARKER));
                let from = if flash { FROM_FLASH } else { FROM_BORROW };
                rows = rows.child(line.child(div().text_xs().text_color(muted).child(from)));
                continue;
            }
            if !locked {
                let (edit_event, delete_event) = (name.clone(), name.clone());
                let is_open = open.is_some_and(|(e, r)| *e == name && *r == i);
                line = line
                    .child(
                        Button::new(ids::hud_maker_anim_edit(&name, i))
                            .ghost()
                            .xsmall()
                            .icon(IconName::Pencil)
                            .selected(is_open)
                            .named("Change this row")
                            .on_click(cx.listener(move |page, _, window, cx| {
                                page.toggle_anim_row(edit_event.clone(), i, window, cx)
                            })),
                    )
                    .child(
                        Button::new(ids::hud_maker_anim_delete(&name, i))
                            .ghost()
                            .xsmall()
                            .icon(IconName::Trash)
                            .named("Delete this row")
                            .on_click(cx.listener(move |page, _, window, cx| {
                                page.anims.open = None;
                                let event = delete_event.clone();
                                page.edit_anim(window, cx, |e| e.delete_animation_row(&event, i))
                            })),
                    );
                rows = rows.child(line);
                if is_open {
                    rows = rows.child(self.row_editor(&name, i, command, cx));
                }
            } else {
                rows = rows.child(line);
            }
        }
        if !locked {
            let adds = ADD_FIELDS.map(|(field, label)| {
                let event = name.clone();
                Button::new(ids::hud_maker_anim_add(&name, field))
                    .ghost()
                    .xsmall()
                    .label(label)
                    .on_click(cx.listener(move |page, _, window, cx| {
                        let event = event.clone();
                        page.edit_anim(window, cx, |e| e.add_animation_row(&event, field))
                    }))
            });
            rows = rows.child(
                h_flex()
                    .gap_1()
                    .items_center()
                    .child(div().text_xs().text_color(muted).child("Add"))
                    .children(adds),
            );
        }
        rows.into_any_element()
    }

    // A colour row's swatch, which points the shared colour picker at the row, and the picker
    // while it does.
    fn row_colour_picker(&self, event: &str, row: usize, cx: &mut Context<Self>) -> AnyElement {
        let key = anim_key(event, row);
        let active = self.active_colour() == Some(key.as_str());
        let theme = cx.theme();
        let fill = self.row_colour(&key).unwrap_or(theme.background);
        let swatch = div()
            .id(ids::hud_maker_anim_colour(event, row))
            .aria_label("Colour")
            .test_support()
            .w_10()
            .h_6()
            .rounded(theme.radius)
            .border_1()
            .border_color(theme.border)
            .cursor_pointer()
            .bg(fill)
            .on_click(cx.listener(move |page, _, window, cx| {
                page.set_active_colour(Some(key.clone()), window, cx)
            }));
        let picker = active.then(|| {
            div()
                .id(ids::HUD_MAKER_COLOUR)
                .aria_label("Colour")
                .test_support()
                .child(ColorPicker::new(&self.colour).accessibility_label("Colour"))
        });
        h_flex()
            .gap_2()
            .items_center()
            .child(swatch)
            .children(picker)
            .into_any_element()
    }

    // The open row's fields: the value and times typed, the curve picked.
    fn row_editor(
        &self,
        event: &str,
        row: usize,
        command: &AnimCommand,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let muted = cx.theme().muted_foreground;
        let field = |label: &'static str, input: &Entity<InputState>| {
            v_flex()
                .flex_1()
                .min_w_0()
                .child(div().text_xs().text_color(muted).child(label))
                .child(Input::new(input).small())
        };
        let AnimCommand::Animate {
            interp, field: key, ..
        } = command
        else {
            return div()
                .text_xs()
                .text_color(muted)
                .child("Only its row can be deleted here.")
                .into_any_element();
        };
        let curves = CURVES.map(|(curve, label)| {
            let event = event.to_owned();
            Button::new(ids::hud_maker_anim_curve(event.as_str(), row, label))
                .ghost()
                .xsmall()
                .label(label)
                .selected(std::mem::discriminant(interp) == std::mem::discriminant(&curve))
                .on_click(cx.listener(move |page, _, window, cx| {
                    let event = event.clone();
                    page.edit_anim(window, cx, |e| {
                        e.edit_animation(&event, row, AnimEdit::Curve(curve))
                    })
                }))
        });
        let colour =
            super::editor::is_colour_field(key).then(|| self.row_colour_picker(event, row, cx));
        let to = if colour.is_some() {
            "Colour name"
        } else {
            "To"
        };
        v_flex()
            .gap_1()
            .pl_2()
            .children(colour)
            .child(
                h_flex()
                    .gap_1()
                    .child(field(to, &self.anims.value))
                    .child(field("Start (s)", &self.anims.start))
                    .child(field("Takes (s)", &self.anims.duration)),
            )
            .child(h_flex().flex_wrap().gap_1().children(curves))
            .into_any_element()
    }
}

// The text a typed field shows for `command`.
fn field_text(command: &AnimCommand, typed: Typed) -> String {
    match (command, typed) {
        (AnimCommand::Animate { value, .. }, Typed::Value) => value.clone(),
        (AnimCommand::Animate { start, .. }, Typed::Start) => start.to_string(),
        (AnimCommand::Animate { duration, .. }, Typed::Duration) => duration.to_string(),
        _ => String::new(),
    }
}
