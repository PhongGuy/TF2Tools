//! The Graphics choices editor: the presets card and the changes strip over choices its host
//! owns. It only proposes edits as `ChoicesEdit` events; the host saves them and calls back
//! with `set_choices`, so it always shows what was saved.

use std::sync::Arc;

use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::{ActiveTheme as _, Disableable as _, Sizable as _, h_flex};
use gpui_kit::{
    AnyElement, Context, EventEmitter, FontWeight, InteractiveElement as _, IntoElement,
    ParentElement as _, StatefulInteractiveElement as _, Styled as _, TestSupportExt as _, Window,
    div, px,
};
use tf2tools_core::{Choices, ModuleCatalog, Preset};

use super::model::preset_label;
use super::view;
use crate::i18n::{counted_plural, tr};
use crate::ids::{self, IdScope};
use crate::pages::common::card_frame;
use crate::widgets::{Named as _, labelled};

/// An edit the editor proposes to its host; the editor's own choices stay as they are.
#[derive(Debug, Clone, PartialEq)]
pub enum ChoicesEdit {
    /// A preset segment other than the current one was picked.
    Preset(Preset),
    /// An undo, Reset all, or a row's control; never an edit that changes nothing.
    Modules { from: Choices, to: Choices },
}

/// Draws a set of Graphics choices under an id scope and proposes edits to them.
pub struct GraphicsChoicesEditor {
    pub(super) scope: IdScope,
    pub(super) choices: Choices,
    pub(super) catalog: Option<Arc<ModuleCatalog>>,
    /// Every control draws disabled and proposes nothing.
    pub(super) disabled: bool,
}

impl EventEmitter<ChoicesEdit> for GraphicsChoicesEditor {}

impl GraphicsChoicesEditor {
    /// An editor showing `choices` without a catalog, drawing under `scope`'s ids.
    pub fn new(
        scope: IdScope,
        choices: Choices,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Self {
        Self {
            scope,
            choices,
            catalog: None,
            disabled: false,
        }
    }

    /// The id scope the editor draws under.
    pub fn scope(&self) -> IdScope {
        self.scope
    }

    /// The choices the editor shows.
    pub fn choices(&self) -> &Choices {
        &self.choices
    }

    /// Shows `choices` and notifies.
    pub fn set_choices(&mut self, choices: Choices, _window: &mut Window, cx: &mut Context<Self>) {
        self.choices = choices;
        cx.notify();
    }

    /// The catalog the rows and undo buttons work from, once the host has one.
    pub fn catalog(&self) -> Option<&Arc<ModuleCatalog>> {
        self.catalog.as_ref()
    }

    /// Call when the catalog changes; notifies.
    pub fn set_catalog(
        &mut self,
        catalog: Option<Arc<ModuleCatalog>>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.catalog = catalog;
        cx.notify();
    }

    /// While disabled, every control is drawn disabled and proposes nothing; notifies.
    pub fn set_disabled(&mut self, disabled: bool, cx: &mut Context<Self>) {
        self.disabled = disabled;
        cx.notify();
    }

    // Proposes a preset other than the shown one; the host decides whether it lands.
    pub(super) fn propose_preset(&mut self, preset: Preset, cx: &mut Context<Self>) {
        if !self.disabled && preset != self.choices.preset() {
            cx.emit(ChoicesEdit::Preset(preset));
        }
    }

    // Proposes the shown choices with `edit` applied, unless that changes nothing.
    pub(super) fn propose(
        &mut self,
        edit: impl FnOnce(&mut Choices, &ModuleCatalog),
        cx: &mut Context<Self>,
    ) {
        let Some(catalog) = self.catalog.as_deref().filter(|_| !self.disabled) else {
            return;
        };
        let from = self.choices.clone();
        let mut to = from.clone();
        edit(&mut to, catalog);
        if to != from {
            cx.emit(ChoicesEdit::Modules { from, to });
        }
    }

    /// The changes from the preset as chips with an undo each, and Reset all; a dashed "as is"
    /// strip when there are none. Without a catalog the chips only name the saved values.
    pub fn changes_strip(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = cx.theme();
        let (border, muted, radius) = (theme.border, theme.muted_foreground, theme.radius);
        let (accent, fill) = (theme.sidebar_primary, theme.background);
        let preset = preset_label(self.choices.preset());
        let catalog = self.catalog.as_deref();
        let list = view::changes(catalog, &self.choices);
        if list.is_empty() {
            return labelled(
                div()
                    .text_sm()
                    .text_color(muted)
                    .py_3()
                    .px_4()
                    .border_1()
                    .border_dashed()
                    .border_color(border)
                    .rounded(radius),
                self.scope.id(ids::GRAPHICS_NO_CHANGES),
                tr!("graphics.changes.as_is", preset = preset),
            )
            .into_any_element();
        }
        let n = u64::try_from(list.len()).unwrap_or(u64::MAX);
        let title = counted_plural(
            "graphics.changes.title",
            n,
            &n.to_string(),
            &[("preset", &preset)],
        );
        let editable = catalog.is_some();
        let chips: Vec<AnyElement> =
            list.into_iter()
                .map(|change| {
                    let sentence = match &change.was {
                        Some(was) => tr!(
                            "graphics.changes.chip",
                            name = change.label,
                            was = was,
                            now = change.now
                        ),
                        None => tr!(
                            "graphics.changes.chip_saved",
                            name = change.label,
                            now = change.now
                        ),
                    };
                    let undo = change.was.clone().filter(|_| editable).map(|was| {
                        let module = change.module.clone();
                        Button::new(self.scope.id(ids::graphics_change_undo(&change.module)))
                            .ghost()
                            .xsmall()
                            .rounded(px(999.))
                            .icon(IconName::X)
                            .named(tr!("graphics.changes.undo", name = change.label, was = was))
                            .disabled(self.disabled)
                            .on_click(cx.listener(move |editor, _, _, cx| {
                                editor.propose(|c, _| c.reset(&module), cx)
                            }))
                    });
                    h_flex()
                        .id(self.scope.id(ids::graphics_change(&change.module)))
                        .aria_label(sentence)
                        .test_support()
                        .h(px(28.))
                        .gap(px(6.))
                        .pl(px(10.))
                        .pr_1()
                        .text_sm()
                        .border_1()
                        .border_color(accent)
                        .bg(fill)
                        .rounded_full()
                        .child(div().font_weight(FontWeight::MEDIUM).child(change.label))
                        .children(change.was.map(|was| {
                            h_flex().gap(px(6.)).text_color(muted).child(was).child("→")
                        }))
                        .child(change.now)
                        .children(undo)
                        .into_any_element()
                })
                .collect();
        let reset_all = editable.then(|| {
            Button::new(self.scope.id(ids::GRAPHICS_RESET_ALL))
                .ghost()
                .small()
                .label(tr!("graphics.changes.reset_all"))
                .disabled(self.disabled)
                .on_click(cx.listener(|editor, _, _, cx| editor.propose(|c, _| c.clear(), cx)))
        });
        // One wrapping row: the title, the chips, then a spacer that pushes Reset all to the end.
        card_frame(cx)
            .id(self.scope.id(ids::GRAPHICS_CHANGES))
            .aria_label(title.clone())
            .test_support()
            .flex()
            .items_center()
            .flex_wrap()
            .gap_2()
            .py_3()
            .px_4()
            .border_color(accent)
            .child(div().mr_1().font_weight(FontWeight::SEMIBOLD).child(title))
            .children(chips)
            .child(div().flex_1())
            .children(reset_all)
            .into_any_element()
    }
}
