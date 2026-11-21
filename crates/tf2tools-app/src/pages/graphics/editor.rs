//! The Graphics choices editor: the presets card, the changes strip and the settings rows,
//! with their pictures, over choices its host owns. It only proposes edits as `ChoicesEdit`
//! events; the host saves them and calls back with `set_choices`, so it always shows what
//! was saved.

use std::collections::BTreeMap;
use std::sync::Arc;

use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::select::{SelectEvent, SelectState};
use gpui_kit::component::slider::{SliderEvent, SliderState};
use gpui_kit::component::{ActiveTheme as _, Disableable as _, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, AppContext as _, Context, Entity, EventEmitter, FontWeight,
    InteractiveElement as _, IntoElement, ParentElement as _, Pixels,
    StatefulInteractiveElement as _, Styled as _, Subscription, TestSupportExt as _, Window, div,
    px,
};
use tf2tools_core::{Choices, ModuleCatalog, Preset};

use super::model::{ValueOption, preset_label, scoped_value_options, shown_choice};
use super::pictures::{self, PictureIndex, RowPictures};
use super::view::{self, BuiltRows};
use crate::i18n::{self, counted_plural, tr};
use crate::ids::{self, IdScope};
use crate::pages::common::card_frame;
use crate::remote_images::{ImageKey, RemoteImages};
use crate::widgets::Named as _;

// The chip strip's height with one row of 28 px chips. The empty strip reserves it, so the first
// change does not push the rows below down under the pointer.
const STRIP_HEIGHT: Pixels = px(54.);
// A narrow strip stacks its title line over the chips, so it is one 28 px line and a gap taller.
const STACKED_STRIP_HEIGHT: Pixels = px(90.);

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
    /// One Select per Select row of the catalog, rebuilt with the catalog.
    pub(super) selects: BTreeMap<String, ValueSelect>,
    /// Each Select's pick handler, rebuilt with the Selects.
    select_subscriptions: Vec<Subscription>,
    /// The bundled setting pictures, once the host has read them.
    pub(super) pictures: Option<Arc<PictureIndex>>,
    /// The module whose pictures are open; at most one row's at a time.
    open_row: Option<String>,
    /// How far across the compare view the preset's picture shows, 0 to 100.
    wipe: Entity<SliderState>,
    /// Whether the compare view shows the crops; one switch for the whole editor.
    zoom: bool,
    /// The image keys the last frame asked for.
    wanted: Vec<ImageKey>,
    /// The open row, zoom or rows changed since `wanted` was gathered.
    wants_stale: bool,
    /// Every row under the shown choices, rebuilt by `rebuild` and read by every frame.
    built: BuiltRows,
    /// Each row's full-picture set, for the modules with pictures; rebuilt with the rows.
    thumbs: BTreeMap<String, RowPictures>,
    /// The language the rows were worded in.
    language: &'static str,
    /// Bumped by each rebuild of the rows.
    generation: u64,
    /// This editor's own `RemoteImages` viewer, so two editors never clear each other's wants.
    image_viewer: String,
    _subscriptions: Vec<Subscription>,
}

/// A settings row's value picker.
pub(super) type ValueSelect = Entity<SelectState<Vec<ValueOption>>>;

impl EventEmitter<ChoicesEdit> for GraphicsChoicesEditor {}

impl GraphicsChoicesEditor {
    /// An editor showing `choices` without a catalog, drawing under `scope`'s ids.
    pub fn new(
        scope: IdScope,
        choices: Choices,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let wipe = cx.new(|_| {
            SliderState::new()
                .min(0.)
                .max(100.)
                .step(1.)
                .default_value(50.)
        });
        let mut subscriptions = vec![cx.subscribe(&wipe, |_, _, _: &SliderEvent, cx| cx.notify())];
        // Pictures redraw the editor as they land; its wants go when it does.
        let image_viewer = format!("graphics-editor:{}", cx.entity_id());
        if let Some(images) = RemoteImages::global(cx) {
            subscriptions.push(cx.observe(&images, |_, _, cx| cx.notify()));
            let released = image_viewer.clone();
            subscriptions.push(cx.on_release(move |_, cx| {
                images.update(cx, |images, cx| images.want(&released, Vec::new(), cx));
            }));
        }
        let mut editor = Self {
            scope,
            choices,
            catalog: None,
            disabled: false,
            selects: BTreeMap::new(),
            select_subscriptions: Vec::new(),
            pictures: None,
            open_row: None,
            wipe,
            zoom: false,
            wanted: Vec::new(),
            wants_stale: true,
            built: BuiltRows::default(),
            thumbs: BTreeMap::new(),
            language: i18n::current(),
            generation: 0,
            image_viewer,
            _subscriptions: subscriptions,
        };
        editor.rebuild();
        editor
    }

    // Builds the rows and their pictures from the catalog, choices, pictures and language,
    // so the frames until the next change only read them.
    fn rebuild(&mut self) {
        self.language = i18n::current();
        self.built = BuiltRows::build(self.catalog.as_deref(), &self.choices);
        self.thumbs = self
            .pictures
            .as_deref()
            .map_or_else(BTreeMap::new, |index| {
                self.built
                    .rows()
                    .filter_map(|row| {
                        let pictures = pictures::row_pictures(index, row, false)?;
                        Some((row.module.clone(), pictures))
                    })
                    .collect()
            });
        self.generation += 1;
        self.wants_stale = true;
    }

    /// Rewords the rows when the app's language changed since they were built; a host calls
    /// it before reading them each frame.
    pub fn follow_language(&mut self) {
        if self.language != i18n::current() {
            self.rebuild();
        }
    }

    /// How many times the rows were built; it moves only when what they show changes.
    pub fn rows_generation(&self) -> u64 {
        self.generation
    }

    /// The sections under the shown choices, cut down to what `filter` admits.
    pub fn sections(&self, filter: &view::Filter) -> Vec<view::SectionView> {
        self.built.sections(filter)
    }

    /// The id scope the editor draws under.
    pub fn scope(&self) -> IdScope {
        self.scope
    }

    /// The choices the editor shows.
    pub fn choices(&self) -> &Choices {
        &self.choices
    }

    /// Shows `choices` and moves every Select to its shown value, even when the choices are
    /// unchanged, so a pick the host refused moves back; notifies.
    pub fn set_choices(&mut self, choices: Choices, window: &mut Window, cx: &mut Context<Self>) {
        if self.choices != choices {
            self.choices = choices;
            self.rebuild();
        }
        self.show_choices(window, cx);
        cx.notify();
    }

    /// The catalog the rows and undo buttons work from, once the host has one.
    pub fn catalog(&self) -> Option<&Arc<ModuleCatalog>> {
        self.catalog.as_ref()
    }

    /// Call when the catalog changes; rebuilds the Selects and notifies.
    pub fn set_catalog(
        &mut self,
        catalog: Option<Arc<ModuleCatalog>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.catalog = catalog;
        self.rebuild();
        self.build_selects(window, cx);
        cx.notify();
    }

    /// `module`'s row as the list draws it; None without a catalog or for an unknown module.
    pub fn row(&self, module: &str) -> Option<view::RowView> {
        self.built.row(module).cloned()
    }

    // `module`'s built row and its full-picture set, for drawing.
    pub(super) fn built_row(&self, module: &str) -> Option<(&view::RowView, Option<&RowPictures>)> {
        Some((self.built.row(module)?, self.thumbs.get(module)))
    }

    /// The Select state of `module`'s row, while it is a Select row of the catalog.
    pub fn module_select(&self, module: &str) -> Option<ValueSelect> {
        self.selects.get(module).cloned()
    }

    // One Select per Select row of the held catalog, each on its shown value; a pick proposes
    // it, and a cleared Select proposes nothing.
    fn build_selects(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.selects.clear();
        self.select_subscriptions.clear();
        let Some(catalog) = self.catalog.clone() else {
            return;
        };
        let select_rows = catalog
            .categories()
            .iter()
            .flat_map(|c| c.modules())
            .filter(|m| view::control_kind(m) == view::ControlKind::Select);
        for module in select_rows {
            let shown = shown_choice(&catalog, &self.choices, module.name());
            let options = scoped_value_options(module, self.scope);
            let select = cx.new(|cx| {
                let mut select = SelectState::new(options, None, window, cx);
                select.set_selected_value(&shown, window, cx);
                select
            });
            let name = module.name().to_owned();
            self.select_subscriptions.push(cx.subscribe(
                &select,
                move |editor, _, event: &SelectEvent<Vec<ValueOption>>, cx| {
                    if let SelectEvent::Confirm(Some(choice)) = event {
                        let choice = choice.clone();
                        editor.propose(|c, catalog| c.set(catalog, &name, choice), cx);
                    }
                },
            ));
            self.selects.insert(module.name().to_owned(), select);
        }
    }

    // Moves each existing Select to its row's shown value.
    fn show_choices(&self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(catalog) = &self.catalog else {
            return;
        };
        for (name, select) in &self.selects {
            let shown = shown_choice(catalog, &self.choices, name);
            select.update(cx, |select, cx| {
                select.set_selected_value(&shown, window, cx)
            });
        }
    }

    /// Takes the bundled setting pictures the rows draw from; notifies.
    pub fn set_pictures(&mut self, pictures: Option<Arc<PictureIndex>>, cx: &mut Context<Self>) {
        self.pictures = pictures;
        self.rebuild();
        cx.notify();
    }

    /// The setting pictures, once the host has handed them in.
    pub fn pictures(&self) -> Option<&PictureIndex> {
        self.pictures.as_deref()
    }

    /// The module whose pictures are open, if any.
    pub fn open_row(&self) -> Option<&str> {
        self.open_row.as_deref()
    }

    /// Whether the compare view shows the zoomed crops.
    pub fn zoom(&self) -> bool {
        self.zoom
    }

    /// How far across the compare view the preset's picture shows, 0 to 100.
    pub fn wipe(&self, cx: &App) -> f32 {
        self.wipe.read(cx).value().start()
    }

    /// The compare view's wipe slider.
    pub fn wipe_state(&self) -> &Entity<SliderState> {
        &self.wipe
    }

    /// The image keys the last `want_images` asked for.
    pub fn wanted_images(&self) -> &[ImageKey] {
        &self.wanted
    }

    // Opens `module`'s pictures, closing any other row's, or closes them when already open.
    // A freshly opened row starts with the wipe in the middle.
    pub(super) fn toggle_pictures(
        &mut self,
        module: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_row = if self.open_row.as_deref() == Some(module) {
            None
        } else {
            self.wipe
                .update(cx, |wipe, cx| wipe.set_value(50., window, cx));
            Some(module.to_owned())
        };
        self.wants_stale = true;
        cx.notify();
    }

    // Closes the open row's pictures, if any; notifies only when one was open.
    pub(super) fn close_pictures(&mut self, cx: &mut Context<Self>) {
        if self.open_row.take().is_some() {
            self.wants_stale = true;
            cx.notify();
        }
    }

    pub(super) fn toggle_zoom(&mut self, cx: &mut Context<Self>) {
        self.zoom = !self.zoom;
        self.wants_stale = true;
        cx.notify();
    }

    // The open row's compare pictures first, as the newest wants, then its strip, then the
    // current thumbnail of every row with pictures; every thumbnail is wanted, not only the
    // visible ones.
    fn image_keys(&self) -> Vec<ImageKey> {
        let Some(index) = self.pictures.as_deref() else {
            return Vec::new();
        };
        let open = self
            .open_row
            .as_deref()
            .and_then(|module| self.built.row(module))
            .and_then(|row| pictures::row_pictures(index, row, self.zoom));
        let (compared, strip) = open
            .map(|p| (p.compare.pictures(), p.strip))
            .unwrap_or_default();
        let thumbs = self
            .built
            .rows()
            .filter_map(|row| self.thumbs.get(&row.module)?.thumb.as_ref())
            .map(|thumb| ImageKey::card(&index.url(thumb)));
        compared
            .into_iter()
            .map(|picture| ImageKey::carousel(&index.url(&picture)))
            .chain(strip.iter().map(|p| ImageKey::card(&index.url(&p.picture))))
            .chain(thumbs)
            .collect()
    }

    /// Hands the image cache this frame's picture wants; a host calls it once per frame it
    /// draws rows. The cache hears of a change after the draw, so no entity updates mid-render.
    pub fn want_images(&mut self, cx: &mut Context<Self>) {
        self.follow_language();
        if !std::mem::take(&mut self.wants_stale) {
            return;
        }
        let keys = self.image_keys();
        if keys == self.wanted {
            return;
        }
        self.wanted = keys.clone();
        let Some(images) = RemoteImages::global(cx) else {
            return;
        };
        let viewer = self.image_viewer.clone();
        cx.defer(move |cx| images.update(cx, |images, cx| images.want(&viewer, keys, cx)));
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
    /// strip when there are none. Without a catalog the chips only name the saved values. When
    /// `narrow`, the title and Reset all sit on a line above the chips.
    pub fn changes_strip(&self, narrow: bool, cx: &mut Context<Self>) -> AnyElement {
        let theme = cx.theme();
        let (border, muted, radius) = (theme.border, theme.muted_foreground, theme.radius);
        let (accent, fill) = (theme.sidebar_primary, theme.background);
        let preset = preset_label(self.choices.preset());
        let catalog = self.catalog.as_deref();
        let list = self.built.changes();
        if list.is_empty() {
            let text = tr!("graphics.changes.as_is", preset = preset);
            // The text is its own shrinkable block, so it wraps inside the strip's padding.
            let line = div()
                .id(self.scope.id(ids::GRAPHICS_NO_CHANGES_TEXT))
                .test_support()
                .flex_1()
                .min_w_0()
                .child(text.clone());
            return div()
                .id(self.scope.id(ids::GRAPHICS_NO_CHANGES))
                .aria_label(text)
                .test_support()
                .flex()
                .items_center()
                .min_h(if narrow {
                    STACKED_STRIP_HEIGHT
                } else {
                    STRIP_HEIGHT
                })
                .text_sm()
                .text_color(muted)
                // Narrow, the longest translations wrap to three lines; a slimmer padding keeps
                // them inside the reserved height.
                .when(narrow, |strip| strip.py_2())
                .when(!narrow, |strip| strip.py_3())
                .px_4()
                .border_1()
                .border_dashed()
                .border_color(border)
                .rounded(radius)
                .child(line)
                .into_any_element();
        }
        let n = u64::try_from(list.len()).unwrap_or(u64::MAX);
        let title = counted_plural(
            "graphics.changes.title",
            n,
            &n.to_string(),
            &[("preset", &preset)],
        );
        let title_name = title.clone();
        let editable = catalog.is_some();
        let chips: Vec<AnyElement> = list
            .iter()
            .map(|change| {
                // A saved value the preset now matches names itself once, not "High → High".
                let was = change.was.clone().filter(|_| !change.same);
                let sentence = match &was {
                    None if change.gone => tr!(
                        "graphics.changes.chip_gone",
                        name = change.label,
                        now = change.now
                    ),
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
                // A matched value resets like its row; a dropped setting has no preset value to
                // go back to, so its X removes it.
                let undo_name = match (&change.was, &was) {
                    (Some(preset), Some(_)) => Some(tr!(
                        "graphics.changes.undo",
                        name = change.label,
                        was = preset
                    )),
                    (Some(_), None) => Some(tr!("graphics.row.reset")),
                    (None, _) if change.gone => {
                        Some(tr!("graphics.changes.remove", name = change.label))
                    }
                    (None, _) => None,
                };
                let undo = undo_name.filter(|_| editable).map(|name| {
                    let module = change.module.clone();
                    Button::new(self.scope.id(ids::graphics_change_undo(&change.module)))
                        .ghost()
                        .xsmall()
                        .rounded(px(999.))
                        .icon(IconName::X)
                        .named(name)
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
                    .child(
                        div()
                            .font_weight(FontWeight::MEDIUM)
                            .child(change.label.clone()),
                    )
                    .children(
                        was.map(|was| h_flex().gap(px(6.)).text_color(muted).child(was).child("→")),
                    )
                    .child(change.now.clone())
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
        let title = div().mr_1().font_weight(FontWeight::SEMIBOLD).child(title);
        let strip = card_frame(cx)
            .id(self.scope.id(ids::GRAPHICS_CHANGES))
            .aria_label(title_name)
            .test_support()
            .py_3()
            .px_4()
            .border_color(accent);
        // Narrow, one chip beside the title would already wrap, so the title line stands above
        // the chips and the strip's height no longer depends on where the first chip lands.
        if narrow {
            return strip
                .child(
                    v_flex()
                        .gap_2()
                        .child(
                            h_flex()
                                .h(px(28.))
                                .child(title)
                                .child(div().flex_1())
                                .children(reset_all),
                        )
                        .child(h_flex().flex_wrap().gap_2().children(chips)),
                )
                .into_any_element();
        }
        // One wrapping row: the title, the chips, then a spacer that pushes Reset all to the end.
        strip
            .flex()
            .items_center()
            .flex_wrap()
            .gap_2()
            .child(title)
            .children(chips)
            .child(div().flex_1())
            .children(reset_all)
            .into_any_element()
    }
}
