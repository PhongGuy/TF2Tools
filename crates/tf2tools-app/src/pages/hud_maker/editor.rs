//! The HUD Maker's editor state without GPUI: the change list over the pristine model, the
//! screen shape and its layout, the selection, and undo history over whole change lists.

use std::borrow::Cow;
use std::collections::VecDeque;
use std::sync::Arc;

use tf2tools_core::{
    AnimCommand, AnimEvent, AnimFrame, AnimScript, AnimationChange, BoardMode, Change,
    ChangeTarget, EditRefused, Fired, FontEdit, GameSituation, HudAssets, HudLayout, HudModel,
    HudOptions, Interp, MeterKind, NewPart, NewPicture, OptionChange, OptionSpec, OptionValue,
    Part, PartBox, PartKey, PictureTarget, PinCorner, PinRefused, Platform, Recipe, RemoveRefused,
    Scene, Screen, ShowsFor, SituationSwitch, Snap, TfClass, WeaponKind, font_families,
    font_family, pictures_fit,
};

/// Why an added font file did not land, by whose fault: the file, its name, the HUD's font
/// list or the part's font.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FontAddRefused {
    /// Not a `.ttf`/`.otf` file with a family name.
    NotAFont,
    /// A good font whose family a HUD can't use: one the system needs, or one not writable.
    BadName,
    /// The HUD's scheme can't list another font file.
    HudFull,
    /// The part's font can't be changed in this HUD, such as one the HUD's own scheme lacks.
    PartFont,
}

/// Why a picked picture did not land.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PictureRefused {
    /// The selection has no picture the maker replaces.
    NotOffered,
    /// The HUD's pictures would pass the picture caps.
    HudFull,
}

/// The menu background's chapter list, the one file only a menu background writes.
const CHAPTERS: &str = "scripts/chapterbackgrounds.txt";

/// One change to a row of an event, as the Animations section makes it.
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq)]
pub enum AnimEdit {
    /// The value an Animate row moves to.
    Value(String),
    /// When an Animate row begins, in seconds.
    Start(f32),
    /// How long an Animate row takes, in seconds.
    Duration(f32),
    /// The curve of an Animate row.
    Curve(Interp),
    /// When any other row runs, in seconds.
    Delay(f32),
    /// Whether a SetVisible row shows or hides.
    Visible(bool),
}

/// Why an animation edit changed nothing; checked in this order.
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnimRefused {
    /// Nothing is selected.
    NoSelection,
    /// The HUD has no event of that name.
    NoSuchEvent,
    /// The event does not name the selected part.
    NotNamed,
    /// The event runs console commands, so it is never changed.
    RunsCommands,
    /// The event has no such row.
    NoSuchRow,
    /// The row is one a feature appended, such as the hit flash.
    Appended,
    /// The value or time is not one the HUD Maker writes.
    BadValue,
}

/// The longest time a row may start, run or wait, in seconds.
const MAX_ANIM_SECONDS: f32 = 60.0;

/// Animate fields whose value is a colour.
pub(super) fn is_colour_field(field: &str) -> bool {
    ["FgColor", "BgColor", "fgcolor_override", "bgcolor_override"]
        .iter()
        .any(|f| f.eq_ignore_ascii_case(field))
}

fn time_ok(t: f32) -> bool {
    t.is_finite() && (0.0..=MAX_ANIM_SECONDS).contains(&t)
}

/// The text a colour value is written as: four whole numbers 0 to 255, or a name the HUD's
/// scheme defines, as a recolour takes it.
fn colour_text(model: &HudModel, colour: &str) -> Option<String> {
    if !clean(colour) {
        return None;
    }
    let numeric = (colour.trim_start())
        .starts_with(|c: char| c.is_ascii_digit() || matches!(c, '+' | '-' | '.'));
    if numeric {
        let parts: Vec<&str> = colour.split_whitespace().collect();
        let byte = |p: &&str| p.bytes().all(|b| b.is_ascii_digit()) && p.parse::<u8>().is_ok();
        return (parts.len() == 4 && parts.iter().all(byte)).then(|| parts.join(" "));
    }
    (model.scheme().named_colour(colour)).map(|_| colour.trim().to_owned())
}

/// Whether a word can be written in a script: not blank, no quote or control character.
fn clean(word: &str) -> bool {
    !word.trim().is_empty() && !(word.chars()).any(|c| c.is_control() || c == '"')
}

/// `command` with `edit` made, refused when the edit does not fit the command or its value.
fn edited(
    model: &HudModel,
    command: &AnimCommand,
    edit: AnimEdit,
) -> Result<AnimCommand, AnimRefused> {
    let mut command = command.clone();
    let time = |t: f32| time_ok(t).then_some(t).ok_or(AnimRefused::BadValue);
    match (&mut command, edit) {
        (AnimCommand::Animate { field, value, .. }, AnimEdit::Value(v)) => {
            *value = match is_colour_field(field) {
                true => colour_text(model, &v).ok_or(AnimRefused::BadValue)?,
                false if clean(&v) => v.trim().to_owned(),
                false => return Err(AnimRefused::BadValue),
            };
        }
        (AnimCommand::Animate { start, .. }, AnimEdit::Start(t)) => *start = time(t)?,
        (AnimCommand::Animate { duration, .. }, AnimEdit::Duration(t)) => *duration = time(t)?,
        (AnimCommand::Animate { interp, .. }, AnimEdit::Curve(c)) => *interp = c,
        (AnimCommand::SetVisible { visible, .. }, AnimEdit::Visible(v)) => *visible = v,
        (c, AnimEdit::Delay(t)) => match delay_of(c) {
            Some(delay) => *delay = time(t)?,
            None => return Err(AnimRefused::BadValue),
        },
        _ => return Err(AnimRefused::BadValue),
    }
    Ok(command)
}

/// The seconds a non-Animate row waits before it runs.
fn delay_of(command: &mut AnimCommand) -> Option<&mut f32> {
    match command {
        AnimCommand::RunEvent { delay, .. }
        | AnimCommand::StopEvent { delay, .. }
        | AnimCommand::RunEventChild { delay, .. }
        | AnimCommand::SetVisible { delay, .. }
        | AnimCommand::SetInputEnabled { delay, .. }
        | AnimCommand::PlaySound { delay, .. }
        | AnimCommand::StopAnimation { delay, .. }
        | AnimCommand::StopPanelAnimations { delay, .. }
        | AnimCommand::SetFont { delay, .. }
        | AnimCommand::SetTexture { delay, .. }
        | AnimCommand::SetString { delay, .. } => Some(delay),
        _ => None,
    }
}

/// The event the hit flash fills.
const HIT_MARKER: &str = "TF2ToolsHitMarker";

/// Whether script `lines` hold a command acting on panel `name`. Appended lines carry no
/// conditionals, so any platform reads them alike.
fn lines_name(lines: &str, name: &str) -> bool {
    let script = AnimScript::parse(&format!("event Lines\n{{\n{lines}\n}}"), Platform::Linux);
    (script.events().iter())
        .flat_map(|e| e.commands())
        .any(|c| c.panel().is_some_and(|p| p.eq_ignore_ascii_case(name)))
}

/// How long a newly added animation row takes, in seconds.
const NEW_ROW_SECONDS: f32 = 0.2;

/// The shortest the timeline runs while any event plays, in seconds.
const MIN_LENGTH: f32 = 1.0;

/// How many steps undo and redo each keep.
pub const UNDO_LIMIT: usize = 100;

/// One history entry: the whole change list to return to, and where a part edit between it and
/// its neighbour was made, so undo and redo can show the player the edit. `at` is `Some` only
/// when the step adds or removes a part-edit `Set` change; `meter` only when that change is in
/// a weapon meter's file.
struct Step {
    changes: Vec<Change>,
    at: Option<(Scene, BoardMode)>,
    meter: Option<MeterKind>,
    // The edit in player words ("move Health value"), the same whichever way it is stepped.
    name: Option<String>,
}

/// A picture's changes and its kept files with their bytes.
type PictureParts = (Vec<Change>, Vec<(String, Vec<u8>)>);

/// What a canvas drag grabs: the part itself, or one of the selection's eight edge handles.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Handle {
    Move,
    Left,
    Right,
    Top,
    Bottom,
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
}

impl Handle {
    /// Whether dragging this handle resizes the width and the height.
    fn sizes(self) -> (bool, bool) {
        use Handle::*;
        match self {
            Move => (false, false),
            Left | Right => (true, false),
            Top | Bottom => (false, true),
            TopLeft | TopRight | BottomLeft | BottomRight => (true, true),
        }
    }
}

/// One open HUD: its pristine model, the player's changes replayed onto it, and the history.
pub struct HudEditor {
    pristine: HudModel,
    changes: Vec<Change>,
    model: HudModel,
    dropped: usize,
    screen: Screen,
    layout: HudLayout,
    selected: Option<PartKey>,
    undo: VecDeque<Step>,
    redo: VecDeque<Step>,
    saved: Vec<Change>,
    drag: Option<Drag>,
    assets: HudAssets,
    // The added files' bytes as last opened or saved, since a change names a file, not its bytes.
    saved_assets: HudAssets,
    // A recipe shown on the canvas but kept apart from the changes, so save and undo never see it.
    recipe: Option<RecipePreview>,
    // A history snapshot laid out beside the HUD, kept apart from the changes like the recipe.
    compared: Option<Compared>,
    compare_builds: u64,
    // The game moment the preview shows: it decides which code-toggled parts lay out.
    situation: GameSituation,
    // Events the player started on the timeline, played after the situation's own.
    played: Vec<Fired>,
    // Seconds into the fired events the canvas shows; playing never touches the changes.
    moment: f32,
    // A copy's own options, and the HUD choices the pristine model was built for: when the
    // changes hold other choices, the page rebuilds the pristine model off the UI thread.
    hud_options: Option<Arc<HudOptions>>,
    built_for: Vec<Change>,
}

/// A snapshot's changes resolved and laid out once, for the (scene, board mode, screen) it was
/// laid out at.
pub struct Compared {
    changes: Vec<Change>,
    model: HudModel,
    layout: HudLayout,
    at: (Scene, BoardMode, bool, Screen, GameSituation),
}

impl Compared {
    pub fn model(&self) -> &HudModel {
        &self.model
    }

    pub fn layout(&self) -> &HudLayout {
        &self.layout
    }
}

// A previewed recipe's whole next change list and what it resolves to.
struct RecipePreview {
    changes: Vec<Change>,
    // The editor's assets with the recipe's fonts laid over them; a cancel just drops them.
    assets: HudAssets,
    model: HudModel,
    layout: HudLayout,
    dropped: usize,
    fonts_left_out: usize,
}

// The state a running drag started from; the editor's own fields hold the preview.
struct Drag {
    changes: Vec<Change>,
    model: HudModel,
    layout: HudLayout,
    dropped: usize,
}

impl HudEditor {
    /// Starts at 1920x1080 with nothing selected; `changes` count as saved.
    pub fn new(pristine: HudModel, changes: Vec<Change>) -> HudEditor {
        let screen = Screen::new(1920, 1080);
        let edited = pristine.with_changes(&changes);
        let dropped = edited.dropped();
        let model = edited.into_model();
        let situation = GameSituation::default();
        let layout = model.layout_in(screen, &situation);
        HudEditor {
            pristine,
            saved: changes.clone(),
            changes,
            model,
            dropped,
            screen,
            layout,
            selected: None,
            undo: VecDeque::new(),
            redo: VecDeque::new(),
            drag: None,
            assets: HudAssets::new(),
            saved_assets: HudAssets::new(),
            recipe: None,
            compared: None,
            compare_builds: 0,
            situation,
            played: Vec::new(),
            moment: 0.0,
            hud_options: None,
            built_for: Vec::new(),
        }
    }

    /// Gives the editor `options`; the pristine model counts as built for the current choices.
    pub fn with_hud_options(mut self, options: Arc<HudOptions>) -> HudEditor {
        self.built_for = self.switch_choices();
        self.hud_options = Some(options);
        self
    }

    /// The copy's own options; `None` for a HUD that is no copy.
    pub fn hud_options(&self) -> Option<&Arc<HudOptions>> {
        self.hud_options.as_ref()
    }

    /// Starts with `assets` as the added files' bytes (a reopened made HUD's).
    pub fn with_assets(mut self, assets: HudAssets) -> HudEditor {
        self.saved_assets = assets.clone();
        self.assets = assets;
        self
    }

    /// The bytes of every file the player added, keyed by the asset's source. Bytes outlive an
    /// undo of their adding, so redo and save still find them.
    pub fn assets(&self) -> &HudAssets {
        &self.assets
    }

    /// The bytes the canvas draws from: `assets` overlaid with a previewed recipe's fonts,
    /// which join `assets` only when the preview is applied.
    pub fn shown_assets(&self) -> &HudAssets {
        self.recipe.as_ref().map_or(&self.assets, |r| &r.assets)
    }

    pub fn pristine(&self) -> &HudModel {
        &self.pristine
    }

    /// The pristine model with the changes replayed, or the previewed recipe's model.
    pub fn model(&self) -> &HudModel {
        self.recipe.as_ref().map_or(&self.model, |r| &r.model)
    }

    pub fn changes(&self) -> &[Change] {
        &self.changes
    }

    /// The changes the canvas shows: a previewed recipe's whole list, else `changes`.
    pub fn shown_changes(&self) -> &[Change] {
        self.recipe.as_ref().map_or(&self.changes, |r| &r.changes)
    }

    /// How many changes the last replay, or the previewed recipe's, could not apply.
    pub fn dropped(&self) -> usize {
        self.recipe.as_ref().map_or(self.dropped, |r| r.dropped)
    }

    pub fn screen(&self) -> Screen {
        self.screen
    }

    /// Lays the HUD out at `screen`; the selection and changes stay.
    pub fn set_screen(&mut self, screen: Screen) {
        self.cancel_preview();
        self.cancel_drag();
        self.screen = screen;
        self.layout = self.model.layout_in(screen, &self.situation);
        self.sync_compared();
    }

    pub fn layout(&self) -> &HudLayout {
        self.recipe.as_ref().map_or(&self.layout, |r| &r.layout)
    }

    /// The layout the canvas draws and picks from: the frame's moved boxes past moment 0, else
    /// `layout()`.
    pub fn shown_layout(&self) -> Cow<'_, HudLayout> {
        match self.moment > 0.0 {
            true => Cow::Owned(self.frame().laid_out(self.layout())),
            false => Cow::Borrowed(self.layout()),
        }
    }

    /// The scene the model resolves.
    pub fn scene(&self) -> Scene {
        self.pristine.scene()
    }

    /// Resolves the same changes into `scene`'s parts; cancels a drag and clears the
    /// selection. The current scene is a no-op.
    pub fn set_scene(&mut self, scene: Scene) {
        if scene == self.scene() {
            return;
        }
        self.cancel_preview();
        self.cancel_drag();
        self.enter(scene);
        self.replay();
        self.sync_compared();
    }

    /// The scoreboard mode the model resolves under.
    pub fn board_mode(&self) -> BoardMode {
        self.pristine.board_mode()
    }

    /// Resolves the same changes under `mode`; cancels a drag and clears the selection. The
    /// current mode is a no-op.
    pub fn set_board_mode(&mut self, mode: BoardMode) {
        if mode == self.board_mode() {
            return;
        }
        self.cancel_preview();
        self.cancel_drag();
        self.pristine = self.pristine.with_board_mode(mode);
        self.selected = None;
        self.replay();
        self.sync_compared();
    }

    /// Whether the model reads TF2's minimal HUD keys.
    pub fn minmode(&self) -> bool {
        self.pristine.minmode()
    }

    /// Resolves the same changes with the minimal HUD `on`; cancels a drag and clears the
    /// selection. The current setting is a no-op.
    pub fn set_minmode(&mut self, on: bool) {
        if on == self.minmode() {
            return;
        }
        self.cancel_preview();
        self.cancel_drag();
        self.pristine = self.pristine.with_minmode(on);
        self.selected = None;
        self.replay();
        self.sync_compared();
    }

    /// The weapon meter the Item meter shows.
    pub fn meter(&self) -> MeterKind {
        self.pristine.meter()
    }

    /// Shows the Item meter from `kind`'s file: a new kind cancels a preview and a drag and
    /// clears the selection. `show` also turns the meter switch on so the part shows.
    pub fn set_meter(&mut self, kind: MeterKind, show: bool) {
        if kind != self.meter() {
            self.cancel_preview();
            self.cancel_drag();
            self.pristine = self.pristine.with_meter(kind);
            self.selected = None;
            self.replay();
            self.sync_compared();
        }
        if show {
            let s = self
                .situation
                .clone()
                .with_switch(SituationSwitch::Meter, true);
            self.set_situation(s);
        }
    }

    pub fn situation(&self) -> &GameSituation {
        &self.situation
    }

    /// Shows the HUD in situation `s`: cancels a drag and a preview, lays out again and returns
    /// the moment to 0. The changes and undo never see it.
    pub fn set_situation(&mut self, s: GameSituation) {
        self.cancel_preview();
        self.cancel_drag();
        self.situation = s;
        self.moment = 0.0;
        self.layout = self.model.layout_in(self.screen, &self.situation);
        self.sync_compared();
    }

    /// The events playing: the situation's, then the played ones, each event once.
    pub fn fired(&self) -> Vec<Fired> {
        let mut fired = self.situation.fired(self.model());
        for p in &self.played {
            if !(fired.iter()).any(|f| f.event().eq_ignore_ascii_case(p.event())) {
                fired.push(p.clone());
            }
        }
        fired
    }

    /// Adds `event` started in the rule element nearest a panel it names (None: whole HUD).
    /// False when the model has no such event.
    pub fn play_event(&mut self, event: &str) -> bool {
        let model = self.model();
        let Some(anim) = model.animations().event(event) else {
            return false;
        };
        let within = (anim.commands().iter())
            .find_map(|c| c.panel())
            .and_then(|panel| owning_element(model, panel));
        let fired = Fired::new(anim.name(), within);
        if !(self.played.iter()).any(|p| p.event() == fired.event()) {
            self.played.push(fired);
        }
        self.moment = 0.0;
        true
    }

    pub fn clear_played(&mut self) {
        self.played.clear();
        self.moment = 0.0;
    }

    pub fn moment(&self) -> f32 {
        self.moment
    }

    /// Moves to `t` seconds, kept within 0 and `length()`.
    pub fn set_moment(&mut self, t: f32) {
        let t = if t.is_nan() { 0.0 } else { t };
        self.moment = t.clamp(0.0, self.length());
    }

    /// How long the fired events run; loops count as 2 s, and events that end at once still
    /// get `MIN_LENGTH`, so the timeline can show what they leave behind.
    pub fn length(&self) -> f32 {
        let fired = self.fired();
        match fired.is_empty() {
            true => 0.0,
            false => (self.model().animations().length(&fired)).max(MIN_LENGTH),
        }
    }

    /// When the fired events really end, loops counting as 2 s; 0 with none fired.
    pub fn end(&self) -> f32 {
        let fired = self.fired();
        match fired.is_empty() {
            true => 0.0,
            false => self.model().animations().length(&fired),
        }
    }

    /// What the fired events change at the moment.
    pub fn frame(&self) -> AnimFrame {
        let model = self.model();
        (model.animations()).frame(model, self.layout(), &self.fired(), self.moment)
    }

    /// Whether Show it has work to do for `part`: the game shows it only in some situations
    /// and the current one hides it, or a panel holding it.
    pub fn offers_show_it(&self, part: &PartKey) -> bool {
        shows_for(self.model(), part).is_some()
            && (self.layout().get(part)).is_some_and(|b| b.hidden().is_some())
    }

    /// Sets the situation in which `part`, or its nearest code-toggled ancestor, shows; false
    /// when nothing the game toggles decides whether it shows.
    pub fn show_part_situation(&mut self, part: &PartKey) -> bool {
        let Some(shows) = shows_for(self.model(), part) else {
            return false;
        };
        let s = self.situation.clone();
        let next = match shows {
            ShowsFor::Class(name) => match TfClass::ALL.into_iter().find(|c| c.name() == name) {
                Some(class) => s.with_class(class),
                None => return false,
            },
            ShowsFor::Weapon("no clip") => s.with_weapon(WeaponKind::NoClip),
            ShowsFor::Weapon("clip") => s.with_weapon(WeaponKind::Clip),
            ShowsFor::Weapon("meter") => s.with_switch(SituationSwitch::Meter, true),
            ShowsFor::Weapon("low ammo") => s.with_ammo_percent(30),
            ShowsFor::Effect("powerup") => s.with_switch(SituationSwitch::Powerup, true),
            ShowsFor::Effect(_) => s.with_switch(SituationSwitch::Effect, true),
            ShowsFor::Mode("3D model") => s.with_switch(SituationSwitch::Portrait, false),
            ShowsFor::Mode("2D portrait") => s.with_switch(SituationSwitch::Portrait, true),
            ShowsFor::Mode("typing") => s.with_switch(SituationSwitch::Typing, true),
            ShowsFor::Mode("koth") => s.with_switch(SituationSwitch::Koth, true),
            ShowsFor::Mode("not koth") => s.with_switch(SituationSwitch::Koth, false),
            ShowsFor::Aim(_) => s.with_switch(SituationSwitch::Aiming, true),
            // Dying, the moment players ask about; overhealed when the HUD's warning never fires.
            ShowsFor::Health(_) => {
                let dying = s.clone().with_health(1);
                match dying.health_pulse(self.model()) {
                    true => dying,
                    false => s.with_health(u16::MAX),
                }
            }
            _ => return false,
        };
        self.set_situation(next);
        true
    }

    /// The events with a row naming the selected part, in load order.
    pub fn animations_selected(&self) -> Vec<AnimEvent> {
        let model = self.model();
        let Some(part) = self.selected.as_ref().and_then(|k| model.part(k)) else {
            return Vec::new();
        };
        (model.animations().naming(part.name()).into_iter())
            .cloned()
            .collect()
    }

    /// Makes `edit` on row `row` of `event`, which names the selection, as one undo step.
    pub fn edit_animation(
        &mut self,
        event: &str,
        row: usize,
        edit: AnimEdit,
    ) -> Result<(), AnimRefused> {
        let (name, mut own) = self.own_rows(event, true, Some(row))?;
        own[row] = edited(self.model(), &own[row], edit)?;
        self.commit_event(&name, &own);
        Ok(())
    }

    /// As `edit_animation`, in place of the last undo step, so a picker drag is one step.
    pub fn edit_animation_again(
        &mut self,
        event: &str,
        row: usize,
        edit: AnimEdit,
    ) -> Result<(), AnimRefused> {
        self.cancel_preview();
        self.end_drag();
        if let Some(prev) = self.undo.pop_back() {
            self.changes = prev.changes;
            self.replay();
        }
        self.edit_animation(event, row, edit)
    }

    /// Adds a row moving the selection's `field` to `event`'s own lines, as one undo step.
    pub fn add_animation_row(&mut self, event: &str, field: &str) -> Result<(), AnimRefused> {
        let (name, mut own) = self.own_rows(event, false, None)?;
        if !clean(field) {
            return Err(AnimRefused::BadValue);
        }
        let model = self.model();
        let part = (self.selected.as_ref())
            .and_then(|k| model.part(k))
            .ok_or(AnimRefused::NoSelection)?;
        // A new row starts from what the part shows, so it changes nothing until edited.
        let own_text = |key: &str| part.value(key).map(|l| l.text().trim().to_owned());
        let value = match field.to_ascii_lowercase().as_str() {
            "alpha" => "255".to_owned(),
            "fgcolor" => own_text("fgcolor").unwrap_or_else(|| "255 255 255 255".to_owned()),
            "bgcolor" => own_text("bgcolor_override").unwrap_or_else(|| "0 0 0 0".to_owned()),
            other => own_text(other)
                .filter(|v| clean(v))
                .unwrap_or_else(|| "0".to_owned()),
        };
        own.push(AnimCommand::Animate {
            panel: part.name().to_owned(),
            field: field.trim().to_owned(),
            value,
            interp: Interp::Linear,
            start: 0.0,
            duration: NEW_ROW_SECONDS,
        });
        self.commit_event(&name, &own);
        Ok(())
    }

    /// Deletes row `row` of `event`, which names the selection, as one undo step.
    pub fn delete_animation_row(&mut self, event: &str, row: usize) -> Result<(), AnimRefused> {
        let (name, mut own) = self.own_rows(event, true, Some(row))?;
        own.remove(row);
        self.commit_event(&name, &own);
        Ok(())
    }

    /// `event`'s name and own rows, refused in `AnimRefused`'s order. `named` asks that the
    /// event names the selection; `row`, when given, must be one of its own rows.
    fn own_rows(
        &self,
        event: &str,
        named: bool,
        row: Option<usize>,
    ) -> Result<(String, Vec<AnimCommand>), AnimRefused> {
        let model = self.model();
        let part = (self.selected.as_ref())
            .and_then(|k| model.part(k))
            .ok_or(AnimRefused::NoSelection)?;
        let e = (model.animations().event(event)).ok_or(AnimRefused::NoSuchEvent)?;
        let names = |c: &AnimCommand| {
            c.panel()
                .is_some_and(|p| p.eq_ignore_ascii_case(part.name()))
        };
        if named && !e.commands().iter().any(names) {
            return Err(AnimRefused::NotNamed);
        }
        if e.runs_game_commands() || model.can_change_animation(e.name()).is_err() {
            return Err(AnimRefused::RunsCommands);
        }
        let own = &e.commands()[..e.own_len().min(e.commands().len())];
        match row {
            Some(r) if r >= e.commands().len() => return Err(AnimRefused::NoSuchRow),
            Some(r) if r >= own.len() => return Err(AnimRefused::Appended),
            _ => {}
        }
        Ok((e.name().to_owned(), own.to_vec()))
    }

    /// Writes `own` as `event`'s whole own body in a fresh group, as one undo step; lines a
    /// feature appended stay after it.
    fn commit_event(&mut self, event: &str, own: &[AnimCommand]) {
        self.cancel_preview();
        let body: Vec<String> = own.iter().map(AnimCommand::to_line).collect();
        let change = AnimationChange::new(event, &body.join("\n"));
        let mut next = self.changes.clone();
        next.push(Change::animation(next_group(&self.changes), change));
        self.commit(next);
    }

    /// Adds a `kind` part near the selection as one undo step and selects it; an existing
    /// crosshair is selected without a change.
    pub fn add_part_selected(&mut self, kind: NewPart) -> Result<PartKey, EditRefused> {
        self.cancel_preview();
        self.end_drag();
        let near = self.selected.clone().ok_or(EditRefused::UnknownPart)?;
        let (key, added) = (self.model).add_part(kind, &near, next_group(&self.changes))?;
        if !added.is_empty() {
            let mut next = self.changes.clone();
            next.extend(added);
            self.commit(next);
        }
        self.select(Some(key.clone()));
        Ok(key)
    }

    /// Whether the selection is a part the player added, which Delete takes away.
    pub fn can_delete_selected(&self) -> bool {
        (self.selected.as_ref()).is_some_and(|key| self.model.is_added(&self.changes, key))
    }

    /// Deletes the selection, a part the player added, with every change it brought, as one
    /// undo step.
    pub fn delete_selected(&mut self) -> Result<(), RemoveRefused> {
        self.cancel_preview();
        self.end_drag();
        let part = self.selected.clone().ok_or(RemoveRefused::UnknownPart)?;
        let next = (self.model).remove_added(&self.changes, &part, next_group(&self.changes))?;
        self.commit(next);
        Ok(())
    }

    /// Whether some appended TF2ToolsHitMarker line names the selected part.
    pub fn hit_flash_selected(&self) -> bool {
        !self.flash_groups().is_empty()
    }

    /// The colour the selection's hit flash turns it, as written; `None` without a flash.
    pub fn hit_flash_colour_selected(&self) -> Option<String> {
        let model = self.model();
        let name = model.part(self.selected.as_ref()?)?.name().to_owned();
        let event = model.animations().event(HIT_MARKER)?;
        (event.commands().iter().skip(event.own_len())).find_map(|c| match c {
            AnimCommand::Animate { panel, value, .. } if panel.eq_ignore_ascii_case(&name) => {
                Some(value.clone())
            }
            _ => None,
        })
    }

    /// On: a flash in the part's own fgcolor text, else "255 255 255 255". Off: drops its
    /// groups. Either is one undo step.
    pub fn set_hit_flash_selected(&mut self, on: bool) -> Result<(), EditRefused> {
        self.cancel_preview();
        self.end_drag();
        let part = self.selected.clone().ok_or(EditRefused::UnknownPart)?;
        if !on {
            let kept = self.without_flash();
            if kept.len() != self.changes.len() {
                self.commit(kept);
            }
            return Ok(());
        }
        let own = (self.model.part(&part))
            .and_then(|p| p.value("fgcolor").or_else(|| p.value("fgcolor_override")))
            .map_or_else(|| "255 255 255 255".to_owned(), |l| l.text().to_owned());
        let added = (self.model).hit_flash(&part, &own, next_group(&self.changes))?;
        if !added.is_empty() {
            let mut next = self.changes.clone();
            next.extend(added);
            self.commit(next);
        }
        Ok(())
    }

    /// Replaces the selected part's flash with one in `colour`, as one undo step.
    pub fn set_hit_flash_colour_selected(&mut self, colour: &str) -> Result<(), EditRefused> {
        self.cancel_preview();
        self.end_drag();
        let part = self.selected.clone().ok_or(EditRefused::UnknownPart)?;
        let mut next = self.without_flash();
        // The new flash is built on the HUD without the old one, which it would otherwise keep.
        let model = self.pristine.with_changes(&next).into_model();
        next.extend(model.hit_flash(&part, colour, next_group(&next))?);
        if next != self.changes {
            self.commit(next);
        }
        Ok(())
    }

    /// As `set_hit_flash_colour_selected`, in place of the last undo step, so a picker drag is
    /// one step.
    pub fn set_hit_flash_colour_selected_again(&mut self, colour: &str) -> Result<(), EditRefused> {
        self.cancel_preview();
        self.end_drag();
        if let Some(prev) = self.undo.pop_back() {
            self.changes = prev.changes;
            self.replay();
        }
        self.set_hit_flash_colour_selected(colour)
    }

    /// The groups whose TF2ToolsHitMarker append names the selection.
    fn flash_groups(&self) -> Vec<u32> {
        let Some(name) = (self.selected.as_ref())
            .and_then(|k| self.model.part(k))
            .map(|p| p.name().to_owned())
        else {
            return Vec::new();
        };
        (self.changes.iter())
            .filter(|c| match c.target() {
                ChangeTarget::AnimationAppend(a) if a.event().eq_ignore_ascii_case(HIT_MARKER) => {
                    lines_name(a.lines(), &name)
                }
                _ => false,
            })
            .map(Change::group)
            .collect()
    }

    /// The changes without every group holding the selection's flash.
    fn without_flash(&self) -> Vec<Change> {
        let groups = self.flash_groups();
        (self.changes.iter())
            .filter(|c| !groups.contains(&c.group()))
            .cloned()
            .collect()
    }

    /// The siblings the selection can be attached to.
    pub fn pin_targets_selected(&self) -> Vec<PartKey> {
        (self.selected.as_ref()).map_or_else(Vec::new, |k| self.model.pin_targets(k))
    }

    /// (sibling name, own corner, sibling corner) of the selected part's pin.
    pub fn pin_of_selected(&self) -> Option<(String, PinCorner, PinCorner)> {
        let part = self.model().part(self.selected.as_ref()?)?;
        let to = part.value("pin_to_sibling")?.text().trim().to_owned();
        let corner = |key: &str| PinCorner::parse(part.value(key).map(|l| l.text().trim()));
        Some((
            to,
            corner("pin_corner_to_sibling"),
            corner("pin_to_sibling_corner"),
        ))
    }

    /// Attaches the selection's `own` corner to sibling `to`'s `corner`, as one undo step; the
    /// part stays where it is.
    pub fn pin_selected(
        &mut self,
        to: &PartKey,
        own: PinCorner,
        corner: PinCorner,
    ) -> Result<(), PinRefused> {
        self.cancel_preview();
        self.end_drag();
        let part = self.selected.clone().ok_or(PinRefused::UnknownPart)?;
        let added = (self.model).pin_part(&part, to, own, corner, next_group(&self.changes))?;
        self.commit_added(added);
        Ok(())
    }

    /// Detaches the selection, leaving it where it is, as one undo step.
    pub fn unpin_selected(&mut self) -> Result<(), PinRefused> {
        self.cancel_preview();
        self.end_drag();
        let part = self.selected.clone().ok_or(PinRefused::UnknownPart)?;
        let added = (self.model).unpin_part(&part, next_group(&self.changes))?;
        self.commit_added(added);
        Ok(())
    }

    /// Commits the changes followed by `added` as one undo step; nothing added records none.
    fn commit_added(&mut self, added: Vec<Change>) {
        if added.is_empty() {
            return;
        }
        let mut next = self.changes.clone();
        next.extend(added);
        self.commit(next);
    }

    pub fn selected(&self) -> Option<&PartKey> {
        self.selected.as_ref()
    }

    /// Selects `part`, or nothing when the model has no such part. Selecting a part ends a
    /// recipe preview; selecting nothing keeps it, since the HUD section previews.
    pub fn select(&mut self, part: Option<PartKey>) {
        if part.is_some() {
            self.cancel_preview();
        }
        self.selected = part.filter(|key| self.model.part(key).is_some());
    }

    /// Selects the part a click at screen pixel (`x`, `y`) hits; a miss clears the selection.
    pub fn select_at(&mut self, x: i32, y: i32) -> Option<&PartKey> {
        self.cancel_preview();
        self.selected = self.shown_layout().hit(x, y).cloned();
        self.selected.as_ref()
    }

    /// Selects what a canvas press picked, or nothing; a press always ends a recipe preview.
    pub fn select_pick(&mut self, part: Option<PartKey>) {
        self.cancel_preview();
        self.select(part);
    }

    /// Moves the selection by screen pixels.
    pub fn move_selected(&mut self, dx: i32, dy: i32) -> Result<(), EditRefused> {
        let screen = self.screen;
        self.edit(|m, part, group| m.move_part(part, dx, dy, screen, group))
    }

    /// Sets the selection's size in screen pixels; `None` leaves that axis. A size that rounds
    /// to no file units is refused, since the part would vanish.
    pub fn resize_selected(
        &mut self,
        wide: Option<i32>,
        tall: Option<i32>,
    ) -> Result<(), EditRefused> {
        let screen = self.screen;
        let flat =
            move |b: &PartBox| (wide.is_some() && b.wide() < 1) || (tall.is_some() && b.tall() < 1);
        self.edit_kept(
            |m, part, group| m.resize_part(part, wide, tall, screen, group),
            |b| !flat(b),
        )
    }

    /// Nudges the selection by whole HUD units as one undo step.
    pub fn nudge_selected(&mut self, dx: i32, dy: i32) -> Result<(), EditRefused> {
        self.edit(|m, part, group| m.nudge_part(part, dx, dy, group))
    }

    /// Mirrors the selection as one undo step.
    pub fn mirror_selected(&mut self) -> Result<(), EditRefused> {
        self.edit(|m, part, group| m.mirror_part(part, group))
    }

    /// Copies `source`'s size onto the selection as one undo step; the selection stays.
    pub fn match_size_selected(&mut self, source: &PartKey) -> Result<(), EditRefused> {
        let screen = self.screen;
        self.edit_kept(
            |m, part, group| m.match_size(part, source, screen, group),
            |b| b.wide() >= 1 && b.tall() >= 1,
        )
    }

    /// `HudLayout::snap_move` for the selection on the layout a running drag started from, or
    /// the current one; `None` with nothing selected.
    pub fn snap_move(&self, dx: i32, dy: i32, threshold: i32) -> Option<Snap> {
        let start = self.drag.as_ref().map_or(&self.layout, |d| &d.layout);
        Some(start.snap_move(self.selected.as_ref()?, dx, dy, threshold))
    }

    pub fn set_layer_selected(&mut self, zpos: i32) -> Result<(), EditRefused> {
        self.edit(|m, part, group| m.set_layer(part, zpos, group))
    }

    /// `colour` is `r g b a` text or a scheme colour name.
    pub fn recolour_selected(&mut self, colour: &str) -> Result<(), EditRefused> {
        self.edit(|m, part, group| m.recolour_part(part, colour, group))
    }

    /// Recolours the selection in place of the last undo step, so a picker drag is one step.
    pub fn recolour_selected_again(&mut self, colour: &str) -> Result<(), EditRefused> {
        self.cancel_preview();
        self.end_drag();
        if let Some(prev) = self.undo.pop_back() {
            self.changes = prev.changes;
            self.replay();
        }
        self.recolour_selected(colour)
    }

    /// `HudModel::set_property` on the selection, one undo step.
    pub fn set_property_selected(&mut self, key: &str, value: &str) -> Result<(), EditRefused> {
        self.edit(|m, part, group| m.set_property(part, key, value, group))
    }

    /// As `set_property_selected`, in place of the last undo step, so a picker drag is one step.
    pub fn set_property_selected_again(
        &mut self,
        key: &str,
        value: &str,
    ) -> Result<(), EditRefused> {
        self.cancel_preview();
        self.end_drag();
        if let Some(prev) = self.undo.pop_back() {
            self.changes = prev.changes;
            self.replay();
        }
        self.set_property_selected(key, value)
    }

    /// `HudModel::set_key_text` on the selection, one undo step.
    pub fn set_key_text_selected(&mut self, key: &str, text: &str) -> Result<(), EditRefused> {
        self.edit(|m, part, group| m.set_key_text(part, key, text, group))
    }

    /// `HudModel::remove_key` on the selection, one undo step.
    pub fn remove_key_selected(&mut self, key: &str) -> Result<(), EditRefused> {
        self.edit(|m, part, group| m.remove_key(part, key, group))
    }

    /// `HudModel::edit_font` on the selection at the editor's screen height, one undo step.
    pub fn edit_font_selected(&mut self, key: &str, edit: FontEdit) -> Result<(), EditRefused> {
        let tall = self.screen.tall();
        self.edit(|m, part, group| m.edit_font(part, key, edit, tall, group))
    }

    /// Adds `bytes` as a font file and sets the family of the selection's Font row `key` to its
    /// family, as one undo step; keeps the bytes under the asset's source and returns the
    /// family. Nothing changes on a refusal, which says whether the file or the HUD is at fault.
    pub fn add_font_file_selected(
        &mut self,
        key: &str,
        file_name: &str,
        bytes: Vec<u8>,
    ) -> Result<String, FontAddRefused> {
        let lower = file_name.to_ascii_lowercase();
        if !(lower.ends_with(".ttf") || lower.ends_with(".otf")) || font_family(&bytes).is_none() {
            return Err(FontAddRefused::NotAFont);
        }
        let tall = self.screen.tall();
        let mut added = None;
        // The file is known good here, so a refusal names whichever builder made it.
        let mut blame = FontAddRefused::PartFont;
        // Both builders read the same model in one group, so the whole pick undoes as one step;
        // the family edit needs no scheme entry for the new family first.
        let edited = self.edit(|m, part, group| {
            let (file, mut changes) =
                m.add_font_file(file_name, &bytes, group)
                    .inspect_err(|&e| {
                        blame = match e {
                            EditRefused::BadValue => FontAddRefused::BadName,
                            _ => FontAddRefused::HudFull,
                        };
                    })?;
            let family = FontEdit::Family(file.family().to_owned());
            changes.extend(
                m.edit_font(part, key, family, tall, group)
                    .inspect_err(|&e| {
                        if e == EditRefused::BadValue {
                            blame = FontAddRefused::BadName;
                        }
                    })?,
            );
            added = Some(file);
            Ok(changes)
        });
        if edited.is_err() {
            return Err(blame);
        }
        let file = added.ok_or(FontAddRefused::PartFont)?;
        self.assets.insert(file.source(), bytes);
        Ok(file.family().to_owned())
    }

    /// How the selection's picture is replaced; `None` when it offers none.
    pub fn picture_target_selected(&self) -> Option<PictureTarget> {
        self.model.picture_target(self.selected.as_ref()?)
    }

    /// Makes the selection draw the texture `vtf` as one undo step, keeping its files' bytes;
    /// nothing changes on a refusal.
    pub fn replace_picture_selected(&mut self, vtf: Vec<u8>) -> Result<(), PictureRefused> {
        self.cancel_preview();
        self.end_drag();
        let part = self.selected.clone().ok_or(PictureRefused::NotOffered)?;
        let group = next_group(&self.changes);
        let picture = (self.model.replace_picture(&part, vtf, group))
            .map_err(|_| PictureRefused::NotOffered)?;
        let (added, files) = self.fitting(picture)?;
        self.edit(|_, _, _| Ok(added))
            .map_err(|_| PictureRefused::NotOffered)?;
        self.keep(files);
        Ok(())
    }

    /// Whether resetting the selection's picture would remove a change.
    pub fn picture_changed(&self) -> bool {
        (self.selected.as_ref()).is_some_and(|part| {
            self.model.reset_picture(part, &self.changes).len() != self.changes.len()
        })
    }

    /// Drops every pick of the selection's picture as one undo step; false when there was none.
    pub fn reset_picture_selected(&mut self) -> bool {
        self.cancel_preview();
        let Some(part) = self.selected.as_ref() else {
            return false;
        };
        let kept = self.model.reset_picture(part, &self.changes);
        if kept.len() == self.changes.len() {
            return false;
        }
        self.commit(kept);
        true
    }

    /// Makes the main menu draw `narrow` (4:3) and `wide` (16:9) as one undo step, keeping
    /// their files' bytes; needs no selection, and nothing changes on a refusal.
    pub fn set_menu_background(
        &mut self,
        narrow: Vec<u8>,
        wide: Vec<u8>,
    ) -> Result<(), PictureRefused> {
        self.cancel_preview();
        self.end_drag();
        let group = next_group(&self.changes);
        let picture = (self.model.set_menu_background(narrow, wide, group))
            .map_err(|_| PictureRefused::NotOffered)?;
        let (added, files) = self.fitting(picture)?;
        let mut next = self.changes.clone();
        next.extend(added);
        self.keep(files);
        self.commit(next);
        Ok(())
    }

    /// Drops every menu background pick as one undo step; false when there was none.
    pub fn reset_menu_background(&mut self) -> bool {
        self.cancel_preview();
        let kept = self.model.reset_menu_background(&self.changes);
        if kept.len() == self.changes.len() {
            return false;
        }
        self.commit(kept);
        true
    }

    /// Whether a picked menu background is applied.
    pub fn has_menu_background(&self) -> bool {
        (self.model.assets().iter()).any(|(path, _)| path.eq_ignore_ascii_case(CHAPTERS))
    }

    /// The picture's changes and files when, added to the current ones, every picture still
    /// passes the caps; the bytes join a copy of the assets only for the check.
    fn fitting(&self, picture: NewPicture) -> Result<PictureParts, PictureRefused> {
        let (added, files) = picture.into_parts();
        let mut assets = self.assets.clone();
        for (source, bytes) in &files {
            assets.insert(source, bytes.clone());
        }
        let mut next = self.changes.clone();
        next.extend(added.iter().cloned());
        if !pictures_fit(&next, &assets) {
            return Err(PictureRefused::HudFull);
        }
        Ok((added, files))
    }

    /// Keeps a landed picture's bytes; ids come from the bytes, so a redo always finds them.
    fn keep(&mut self, files: Vec<(String, Vec<u8>)>) {
        for (source, bytes) in files {
            self.assets.insert(&source, bytes);
        }
    }

    /// The scheme's families plus every family each applied asset's bytes answer to, sorted
    /// ignoring case, without duplicates; a font whose adding was undone is not offered.
    pub fn font_families(&self) -> Vec<String> {
        let added = (self.model.assets().iter())
            .filter_map(|(_, source)| self.assets.get(source))
            .flat_map(font_families);
        let mut names: Vec<String> = (self.model.scheme().families().into_iter())
            .map(str::to_owned)
            .chain(added)
            .collect();
        names.sort_by_key(|name| name.to_ascii_lowercase());
        names.dedup_by(|a, b| a.eq_ignore_ascii_case(b));
        names
    }

    /// Sets the selection's Stage 1 Number key as one undo step (`HudModel::set_number`).
    pub fn set_number_selected(&mut self, key: &str, value: i32) -> Result<(), EditRefused> {
        self.edit(|m, part, group| m.set_number(part, key, value, group))
    }

    pub fn hide_selected(&mut self) -> Result<(), EditRefused> {
        self.edit(|m, part, group| m.hide_part(part, group))
    }

    /// Drops every hide of the selection as one undo step; false when there was none.
    pub fn unhide_selected(&mut self) -> bool {
        match self.selected.clone() {
            Some(part) => self.unhide_part(&part),
            None => false,
        }
    }

    /// Hides or shows `part` as one undo step with the change the selection's hide writes,
    /// keeping the selection; showing a part the player never hid records nothing.
    pub fn set_part_hidden(&mut self, part: &PartKey, hide: bool) -> Result<(), EditRefused> {
        if !hide {
            self.unhide_part(part);
            return Ok(());
        }
        self.edit_part(part, |m, part, group| m.hide_part(part, group), |_| true)
    }

    fn unhide_part(&mut self, part: &PartKey) -> bool {
        self.cancel_preview();
        let groups = self.model.hide_groups(part, &self.changes);
        if groups.is_empty() {
            return false;
        }
        let kept = self
            .changes
            .iter()
            .filter(|c| !groups.contains(&c.group()))
            .cloned()
            .collect();
        self.commit(kept);
        true
    }

    /// Whether the player hid the selection.
    pub fn hidden_by_player(&self) -> bool {
        !self.hide_groups().is_empty()
    }

    /// Whether the player hid `part`.
    pub fn part_hidden_by_player(&self, part: &PartKey) -> bool {
        !self.model.hide_groups(part, &self.changes).is_empty()
    }

    /// Whether `part` reads differently from the starting HUD: a setting's value differs, or
    /// the starting HUD has no such part.
    pub fn part_changed(&self, part: &PartKey) -> bool {
        let Some(now) = self.model.part(part) else {
            return false;
        };
        let Some(was) = self.pristine.part(part) else {
            return true;
        };
        let values = |p: &Part| {
            (p.settings().into_iter())
                .map(|l| (l.key().to_ascii_lowercase(), l.text().to_owned()))
                .collect::<Vec<_>>()
        };
        values(now) != values(was)
    }

    /// Whether resetting `keys` on the selection would remove a change.
    pub fn changed(&self, keys: &[&str]) -> bool {
        self.without(keys)
            .is_some_and(|kept| kept.len() != self.changes.len())
    }

    /// Drops the selection's changes to `keys` as one undo step; false when there were none.
    pub fn reset(&mut self, keys: &[&str]) -> bool {
        self.cancel_preview();
        match self.without(keys) {
            Some(kept) if kept.len() != self.changes.len() => {
                self.commit(kept);
                true
            }
            _ => false,
        }
    }

    /// The built-ins this HUD offers.
    pub fn options(&self) -> Vec<&'static OptionSpec> {
        self.pristine.options()
    }

    /// The value in force for option `id`, a running option preview included but not a recipe
    /// preview; `None` when not offered.
    pub fn option_value(&self, id: &str) -> Option<OptionValue> {
        self.pristine.option_value(id, &self.changes)
    }

    /// The value option `id` shows: a recipe preview's while one runs, else `option_value`.
    pub fn shown_option_value(&self, id: &str) -> Option<OptionValue> {
        match &self.recipe {
            Some(preview) => self.pristine.option_value(id, &preview.changes),
            None => self.option_value(id),
        }
    }

    /// Chooses `value` for option `id` as one undo step; the value already shown records nothing.
    pub fn set_option(&mut self, id: &str, value: OptionValue) -> Result<(), EditRefused> {
        self.cancel_preview();
        self.end_drag();
        let next = self.pristine.set_option(
            id,
            value.clone(),
            &self.changes,
            next_group(&self.changes),
        )?;
        // Re-choosing the value in force would only move the choice to a new group.
        if self.option_value(id) == Some(value) || next == self.changes {
            return Ok(());
        }
        self.commit(next);
        Ok(())
    }

    /// Previews `value` for option `id` from the state before the first preview, replacing any
    /// earlier one; `end_drag` keeps it as one undo step and `cancel_drag` restores.
    pub fn preview_option(&mut self, id: &str, value: OptionValue) -> Result<(), EditRefused> {
        self.cancel_preview();
        let base = self.base_changes();
        let back = self.pristine.option_value(id, base).as_ref() == Some(&value);
        let next = self
            .pristine
            .set_option(id, value, base, next_group(base))?;
        // A preview back to the value in force shows the base itself, so end_drag records nothing.
        let next = if back { base.to_vec() } else { next };
        self.preview(next, |_| true)
    }

    /// The value HUD option `id` shows: a recipe preview's while one runs; `None` when not offered.
    pub fn hud_option_value(&self, id: &str) -> Option<OptionValue> {
        let changes = self.recipe.as_ref().map_or(&self.changes, |r| &r.changes);
        self.hud_options.as_ref()?.value(id, changes)
    }

    /// Chooses `value` for HUD option `id` as one undo step; the value in force records nothing.
    pub fn set_hud_option(&mut self, id: &str, value: OptionValue) -> Result<(), EditRefused> {
        self.cancel_preview();
        self.end_drag();
        let options = self.hud_options.as_ref().ok_or(EditRefused::NotOffered)?;
        let next = options.set(id, value, &self.changes, next_group(&self.changes))?;
        if next != self.changes {
            self.commit(next);
        }
        Ok(())
    }

    /// Whether the changes hold a choice for a HUD option in `group`.
    pub fn hud_option_group_changed(&self, group: &str) -> bool {
        (self.hud_options.as_ref())
            .is_some_and(|o| o.reset_group(group, &self.changes).len() != self.changes.len())
    }

    /// Drops the choices of every HUD option in `group` as one undo step; false when none.
    pub fn reset_hud_option_group(&mut self, group: &str) -> bool {
        self.cancel_preview();
        self.end_drag();
        let Some(options) = &self.hud_options else {
            return false;
        };
        let kept = options.reset_group(group, &self.changes);
        if kept.len() == self.changes.len() {
            return false;
        }
        self.commit(kept);
        true
    }

    /// The HUD and editor choices in the changes, in order.
    pub fn switch_choices(&self) -> Vec<Change> {
        (self.changes.iter())
            .filter(|c| match c.target() {
                ChangeTarget::Option(o) => {
                    [OptionChange::HUD, OptionChange::SCHEMA].contains(&o.source())
                }
                _ => false,
            })
            .cloned()
            .collect()
    }

    /// The HUD choices the pristine model was built for.
    pub fn switch_built_for(&self) -> &[Change] {
        &self.built_for
    }

    /// Whether the changes hold other HUD choices than the pristine model was built for.
    pub fn needs_switch_reload(&self) -> bool {
        self.hud_options.is_some() && self.switch_choices() != self.built_for
    }

    /// Replaces the pristine model with one built for `built_for` and replays the changes over
    /// it, keeping the scene, board mode, meter, minimal HUD, selection and any running preview,
    /// drag or comparison.
    /// False, changing nothing, when the changes no longer hold exactly those choices.
    pub fn rebase(&mut self, pristine: HudModel, built_for: Vec<Change>) -> bool {
        if built_for != self.switch_choices() {
            return false;
        }
        self.pristine = pristine
            .with_scene(self.scene())
            .with_board_mode(self.board_mode())
            .with_meter(self.meter())
            .with_minmode(self.minmode());
        self.built_for = built_for;
        if let Some(drag) = &mut self.drag {
            let edited = self.pristine.with_changes(&drag.changes);
            drag.dropped = edited.dropped();
            drag.model = edited.into_model();
            drag.layout = drag.model.layout_in(self.screen, &self.situation);
        }
        // `replay` re-selects through `select`, which would end a recipe preview.
        let recipe = self.recipe.take();
        self.replay();
        if let Some(r) = recipe {
            self.show_preview(r.changes, r.assets, r.fonts_left_out);
        }
        if let Some(shown) = self.compared.take() {
            self.compared = Some(self.lay_out(shown.changes));
        }
        true
    }

    /// Whether the changes hold a choice for an option in `group`.
    pub fn option_group_changed(&self, group: &str) -> bool {
        self.pristine.reset_option_group(group, &self.changes).len() != self.changes.len()
    }

    /// Drops the choices of every option in `group` as one undo step; false when there were none.
    pub fn reset_option_group(&mut self, group: &str) -> bool {
        self.cancel_preview();
        self.end_drag();
        let kept = self.pristine.reset_option_group(group, &self.changes);
        if kept.len() == self.changes.len() {
            return false;
        }
        self.commit(kept);
        true
    }

    /// Steps back one edit; false when there is none. During a drag it cancels only the drag.
    /// A recipe preview is dropped first, even with nothing to undo.
    pub fn undo(&mut self) -> bool {
        self.cancel_preview();
        if self.drag.is_some() {
            self.cancel_drag();
            return true;
        }
        let Some(prev) = self.undo.pop_back() else {
            return false;
        };
        self.follow(prev.at, prev.meter);
        let current = std::mem::replace(&mut self.changes, prev.changes);
        push_capped(
            &mut self.redo,
            Step {
                changes: current,
                at: prev.at,
                meter: prev.meter,
                name: prev.name,
            },
        );
        self.replay();
        self.sync_compared();
        true
    }

    /// Steps forward one undone edit; false when there is none or a drag is running. A recipe
    /// preview is dropped first.
    pub fn redo(&mut self) -> bool {
        self.cancel_preview();
        if self.drag.is_some() {
            return false;
        }
        let Some(next) = self.redo.pop_back() else {
            return false;
        };
        self.follow(next.at, next.meter);
        let current = std::mem::replace(&mut self.changes, next.changes);
        push_capped(
            &mut self.undo,
            Step {
                changes: current,
                at: next.at,
                meter: next.meter,
                name: next.name,
            },
        );
        self.replay();
        self.sync_compared();
        true
    }

    /// The next undo's edit in player words, such as "move Health value".
    pub fn undo_name(&self) -> Option<&str> {
        self.undo.back()?.name.as_deref()
    }

    /// The next redo's edit in player words.
    pub fn redo_name(&self) -> Option<&str> {
        self.redo.back()?.name.as_deref()
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    /// Previews dragging the selection's `handle` by (`dx`, `dy`) screen pixels from where the
    /// drag began, replacing any earlier preview; builders run on the model as it was at the
    /// start. A refused preview leaves the last one shown.
    pub fn drag(&mut self, handle: Handle, dx: i32, dy: i32) -> Result<(), EditRefused> {
        self.cancel_preview();
        self.moment = 0.0;
        let part = self.selected.clone().ok_or(EditRefused::UnknownPart)?;
        let (start, start_layout, base) = match &self.drag {
            Some(drag) => (&drag.model, &drag.layout, &drag.changes),
            None => (&self.model, &self.layout, &self.changes),
        };
        let b = start_layout.get(&part).ok_or(EditRefused::UnknownPart)?;
        let group = next_group(base);
        let added = drag_changes(start, &part, b, self.screen, group, handle, dx, dy)?;
        let mut next = base.clone();
        next.extend(added);
        // Only an axis the handle resizes must keep a size; a part already flat on the other
        // still moves and resizes.
        let (sizes_x, sizes_y) = handle.sizes();
        self.preview(next, |layout| {
            !layout
                .get(&part)
                .is_some_and(|b| (sizes_x && b.wide() < 1) || (sizes_y && b.tall() < 1))
        })
    }

    pub fn is_dragging(&self) -> bool {
        self.drag.is_some()
    }

    /// Keeps the running preview as one undo step; a preview that wrote nothing records none.
    pub fn end_drag(&mut self) {
        let Some(drag) = self.drag.take() else {
            return;
        };
        if drag.changes != self.changes {
            let step = self.step(drag.changes);
            push_capped(&mut self.undo, step);
            self.redo.clear();
        }
    }

    /// Drops the running preview, putting back the state the drag started from.
    pub fn cancel_drag(&mut self) {
        let Some(drag) = self.drag.take() else {
            return;
        };
        (self.changes, self.model, self.layout, self.dropped) =
            (drag.changes, drag.model, drag.layout, drag.dropped);
    }

    /// Previews `recipe` applied (`HudModel::apply_recipe`) in the next free group, replacing
    /// any earlier preview, its font bytes held apart (`shown_assets`) until it is applied;
    /// returns how many changes it skipped.
    pub fn preview_recipe(&mut self, recipe: &Recipe) -> usize {
        self.cancel_drag();
        let applied = self
            .pristine
            .apply_recipe(recipe, &self.changes, next_group(&self.changes));
        // The fonts stay out of `assets` until Apply, so a cancel never overwrites the bytes of
        // an undone font that redo would bring back.
        let mut assets = self.assets.clone();
        for (source, bytes) in applied.added() {
            assets.insert(source, bytes.clone());
        }
        // Each font the recipe carries either joined the preview or was refused.
        let fonts = (recipe.changes().iter())
            .filter(|c| matches!(c.target(), ChangeTarget::Asset(_)))
            .count();
        let fonts_left_out = fonts.saturating_sub(applied.added().len());
        self.show_preview(applied.changes().to_vec(), assets, fonts_left_out);
        applied.skipped()
    }

    /// Previews the changes followed by `added` (a borrowed part) as the next change list,
    /// replacing any earlier preview; the bytes `assets` holds for the files `added` names are
    /// held apart (`shown_assets`) until Apply. Returns how many of `added` did not apply.
    pub fn preview_changes(&mut self, added: Vec<Change>, assets: &HudAssets) -> usize {
        self.cancel_drag();
        let mut shown = self.assets.clone();
        for change in &added {
            if let ChangeTarget::Asset(file) = change.target()
                && let Some(bytes) = assets.get(file.source())
            {
                shown.insert(file.source(), bytes.to_vec());
            }
        }
        let mut changes = self.changes.clone();
        changes.extend(added);
        let base = self.dropped;
        self.show_preview(changes, shown, 0);
        self.dropped().saturating_sub(base)
    }

    // Resolves `changes` once and holds them as the preview the canvas shows.
    fn show_preview(&mut self, changes: Vec<Change>, assets: HudAssets, fonts_left_out: usize) {
        let edited = self.pristine.with_changes(&changes);
        let dropped = edited.dropped();
        let model = edited.into_model();
        let layout = model.layout_in(self.screen, &self.situation);
        self.recipe = Some(RecipePreview {
            changes,
            assets,
            model,
            layout,
            dropped,
            fonts_left_out,
        });
    }

    /// The group the next player action records its changes in.
    pub fn next_group(&self) -> u32 {
        next_group(&self.changes)
    }

    pub fn previewing(&self) -> bool {
        self.recipe.is_some()
    }

    /// How many of the previewed recipe's fonts were left out, a part of the count
    /// `preview_recipe` returned; 0 when nothing is previewing.
    pub fn preview_fonts_left_out(&self) -> usize {
        self.recipe.as_ref().map_or(0, |r| r.fonts_left_out)
    }

    /// Keeps the preview as one undo step; false when none was showing. A preview that adds
    /// nothing records no step.
    pub fn apply_preview(&mut self) -> bool {
        let Some(preview) = self.recipe.take() else {
            return false;
        };
        // Nothing changed `assets` while the preview showed, so its overlay is the whole set.
        self.assets = preview.assets;
        if preview.changes != self.changes {
            // The preview already resolved this list, so it is moved in without a replay.
            let before = std::mem::replace(&mut self.changes, preview.changes);
            let step = self.step(before);
            push_capped(&mut self.undo, step);
            self.redo.clear();
            (self.model, self.layout, self.dropped) =
                (preview.model, preview.layout, preview.dropped);
            if let Some(part) = self.selected.take() {
                self.select(Some(part));
            }
        }
        true
    }

    /// Drops the preview; the model shows the changes again.
    pub fn cancel_preview(&mut self) {
        self.recipe = None;
    }

    /// Makes `changes` the change list as one undo step, taking from `assets` the bytes of any
    /// added file the editor lacks; false, recording nothing, when they equal the changes.
    /// Ends a preview, a drag and any comparison first.
    pub fn restore(&mut self, changes: Vec<Change>, assets: &HudAssets) -> bool {
        self.cancel_preview();
        self.cancel_drag();
        self.stop_comparing();
        // An editor's own bytes for a file are newer than a snapshot's kept copy, so only gaps fill.
        // Every applied file counts, not only each path's winner, so an overridden pick that
        // undo brings back keeps its bytes.
        let resolved = self.pristine.with_changes(&changes);
        let sources = (resolved.applied().iter()).filter_map(|c| match c.target() {
            ChangeTarget::Asset(file) => Some(file.source()),
            _ => None,
        });
        for source in sources {
            if self.assets.get(source).is_none()
                && let Some(bytes) = assets.get(source)
            {
                self.assets.insert(source, bytes.to_vec());
            }
        }
        if changes == self.changes {
            return false;
        }
        self.commit(changes);
        true
    }

    /// Lays `changes` out once at the current scene, board mode and screen; replaces any
    /// comparison and leaves the changes, undo and the dirty flag alone.
    pub fn compare_with(&mut self, changes: Vec<Change>) {
        self.compared = Some(self.lay_out(changes));
    }

    pub fn compared(&self) -> Option<&Compared> {
        self.compared.as_ref()
    }

    pub fn stop_comparing(&mut self) {
        self.compared = None;
    }

    /// How many times a comparison was laid out, so a test can tell the slider never does.
    pub fn compare_builds(&self) -> u64 {
        self.compare_builds
    }

    /// Lays the comparison out again exactly when the scene, board mode, minimal HUD or screen
    /// it was laid out at is no longer the editor's. Edits and a divider drag never get here.
    fn sync_compared(&mut self) {
        let now = (
            self.scene(),
            self.board_mode(),
            self.minmode(),
            self.screen,
            self.situation.clone(),
        );
        let Some(shown) = self.compared.take() else {
            return;
        };
        self.compared = Some(if shown.at == now {
            shown
        } else {
            self.lay_out(shown.changes)
        });
    }

    fn lay_out(&mut self, changes: Vec<Change>) -> Compared {
        self.compare_builds += 1;
        let model = self.pristine.with_changes(&changes).into_model();
        let layout = model.layout_in(self.screen, &self.situation);
        Compared {
            changes,
            model,
            layout,
            at: (
                self.scene(),
                self.board_mode(),
                self.minmode(),
                self.screen,
                self.situation.clone(),
            ),
        }
    }

    /// Whether the changes, or the bytes of an applied added file, differ from the ones last
    /// opened or saved.
    pub fn is_dirty(&self) -> bool {
        self.changes != self.saved
            || (self.model.assets().iter())
                .any(|(_, source)| self.assets.get(source) != self.saved_assets.get(source))
    }

    pub fn mark_saved(&mut self) {
        self.saved = self.changes.clone();
        self.saved_assets = self.assets.clone();
    }

    /// Marks `changes` and `assets` (snapshots taken when Save was clicked) as saved.
    pub fn mark_saved_as(&mut self, changes: Vec<Change>, assets: HudAssets) {
        self.saved = changes;
        self.saved_assets = assets;
    }

    /// Builds an edit of the selection on the edited model in a fresh group and records it as
    /// one undo step; an edit that writes nothing records nothing.
    fn edit(
        &mut self,
        build: impl FnOnce(&HudModel, &PartKey, u32) -> Result<Vec<Change>, EditRefused>,
    ) -> Result<(), EditRefused> {
        self.edit_kept(build, |_| true)
    }

    /// As `edit`, but refuses with `BadValue`, recording nothing, when the selection's new box
    /// fails `keep`.
    fn edit_kept(
        &mut self,
        build: impl FnOnce(&HudModel, &PartKey, u32) -> Result<Vec<Change>, EditRefused>,
        keep: impl FnOnce(&PartBox) -> bool,
    ) -> Result<(), EditRefused> {
        let part = self.selected.clone().ok_or(EditRefused::UnknownPart)?;
        self.edit_part(&part, build, keep)?;
        self.select(Some(part));
        Ok(())
    }

    /// As `edit_kept`, for `part` whether or not it is selected; the selection is left as is.
    fn edit_part(
        &mut self,
        part: &PartKey,
        build: impl FnOnce(&HudModel, &PartKey, u32) -> Result<Vec<Change>, EditRefused>,
        keep: impl FnOnce(&PartBox) -> bool,
    ) -> Result<(), EditRefused> {
        self.cancel_preview();
        self.end_drag();
        self.moment = 0.0;
        let group = next_group(&self.changes);
        let added = build(&self.model, part, group)?;
        if added.is_empty() {
            return Ok(());
        }
        let mut next = self.changes.clone();
        next.extend(added);
        let edited = self.pristine.with_changes(&next);
        let dropped = edited.dropped();
        let model = edited.into_model();
        let layout = model.layout_in(self.screen, &self.situation);
        if !layout.get(part).is_none_or(keep) {
            return Err(EditRefused::BadValue);
        }
        let before = std::mem::replace(&mut self.changes, next);
        let step = self.step(before);
        push_capped(&mut self.undo, step);
        self.redo.clear();
        (self.dropped, self.model, self.layout) = (dropped, model, layout);
        Ok(())
    }

    /// The changes without the selection's writes to `keys`, or `None` with nothing selected.
    fn without(&self, keys: &[&str]) -> Option<Vec<Change>> {
        let part = self.selected.as_ref()?;
        Some(self.model.reset_property(part, keys, &self.changes))
    }

    fn hide_groups(&self) -> Vec<u32> {
        self.selected
            .as_ref()
            .map(|part| self.model.hide_groups(part, &self.changes))
            .unwrap_or_default()
    }

    /// Shows `next` as the running preview when its layout passes `keep`, else refuses with
    /// `BadValue` and leaves the last preview shown.
    fn preview(
        &mut self,
        next: Vec<Change>,
        keep: impl FnOnce(&HudLayout) -> bool,
    ) -> Result<(), EditRefused> {
        let edited = self.pristine.with_changes(&next);
        let dropped = edited.dropped();
        let model = edited.into_model();
        let layout = model.layout_in(self.screen, &self.situation);
        if !keep(&layout) {
            return Err(EditRefused::BadValue);
        }
        self.moment = 0.0;
        // The first preview parks the start state, so cancelling puts it back without a replay.
        let changes = std::mem::replace(&mut self.changes, next);
        let model = std::mem::replace(&mut self.model, model);
        let layout = std::mem::replace(&mut self.layout, layout);
        let dropped = std::mem::replace(&mut self.dropped, dropped);
        if self.drag.is_none() {
            self.drag = Some(Drag {
                changes,
                model,
                layout,
                dropped,
            });
        }
        Ok(())
    }

    /// The changes before any running preview.
    fn base_changes(&self) -> &[Change] {
        self.drag.as_ref().map_or(&self.changes, |d| &d.changes)
    }

    /// Makes `next` the change list, keeping the old one as an undo step.
    fn commit(&mut self, next: Vec<Change>) {
        self.end_drag();
        self.moment = 0.0;
        let before = std::mem::replace(&mut self.changes, next);
        let step = self.step(before);
        push_capped(&mut self.undo, step);
        self.redo.clear();
        self.replay();
    }

    /// Re-resolves the pristine model into `scene` and drops the selection; the caller replays.
    fn enter(&mut self, scene: Scene) {
        self.pristine = self.pristine.with_scene(scene);
        self.selected = None;
    }

    /// The history step that returns to `before` from the current changes; it remembers where
    /// the edit was made, under the mode showing now.
    fn step(&self, before: Vec<Change>) -> Step {
        let at = step_scene(&self.changes, &before, self.scene())
            .map(|scene| (scene, self.board_mode()));
        let meter = step_file(&self.changes, &before).and_then(MeterKind::for_file);
        let name = super::step_name::name(&self.pristine, &self.model, &before, &self.changes);
        Step {
            changes: before,
            at,
            meter,
            name,
        }
    }

    /// Enters the scene and mode a history step was made in when they differ from the open
    /// ones, so the player sees the step. Only the Scoreboard has modes: another scene keeps
    /// the open mode. A step in a weapon meter's file shows that meter with its switch on.
    fn follow(&mut self, at: Option<(Scene, BoardMode)>, meter: Option<MeterKind>) {
        if let Some(kind) = meter {
            if kind != self.meter() {
                self.pristine = self.pristine.with_meter(kind);
                self.selected = None;
            }
            self.situation = (self.situation.clone()).with_switch(SituationSwitch::Meter, true);
        }
        let Some((scene, mode)) = at else {
            return;
        };
        let mode = if scene == Scene::Scoreboard {
            mode
        } else {
            self.board_mode()
        };
        if (scene, mode) != (self.scene(), self.board_mode()) {
            self.pristine = self.pristine.with_scene(scene).with_board_mode(mode);
            self.selected = None;
        }
    }

    /// Replays the changes onto the pristine model and lays it out again; a selection the new
    /// model lacks is cleared.
    fn replay(&mut self) {
        let edited = self.pristine.with_changes(&self.changes);
        self.dropped = edited.dropped();
        self.model = edited.into_model();
        self.layout = self.model.layout_in(self.screen, &self.situation);
        if let Some(part) = self.selected.take() {
            self.select(Some(part));
        }
    }
}

/// The changes a drag of `handle` by (`dx`, `dy`) writes on box `b`, kept to the ranges typed
/// values take. A left or top handle moves the part by exactly the file units its size gained, in
/// the same group, so the opposite edge stays put; an axis the pointer has not moved is left.
#[expect(
    clippy::too_many_arguments,
    reason = "the start box, screen and group travel with the drag"
)]
fn drag_changes(
    model: &HudModel,
    part: &PartKey,
    b: &PartBox,
    screen: Screen,
    group: u32,
    handle: Handle,
    dx: i32,
    dy: i32,
) -> Result<Vec<Change>, EditRefused> {
    use Handle::*;
    let units = Units::new(screen);
    if handle == Move {
        let dx = units.clamp_pos(b.x(), dx, screen.wide());
        let dy = units.clamp_pos(b.y(), dy, screen.tall());
        return model.move_part(part, dx, dy, screen, group);
    }
    // Each axis works in file units: the size the box has now, plus what the pointer moved.
    let axis = |size: i32, grow: i32, span: u32| {
        (grow != 0).then(|| {
            let from = units.of(size);
            let to = units.size(from.saturating_add(units.normalise(grow)), span);
            (units.px(to), from.saturating_sub(to))
        })
    };
    let (wide, x) = match handle {
        Left | TopLeft | BottomLeft => axis(b.wide(), dx.saturating_neg(), screen.wide())
            .map_or((None, 0), |(w, shift)| (Some(w), shift)),
        Right | TopRight | BottomRight => (axis(b.wide(), dx, screen.wide()).map(|a| a.0), 0),
        _ => (None, 0),
    };
    let (tall, y) = match handle {
        Top | TopLeft | TopRight => axis(b.tall(), dy.saturating_neg(), screen.tall())
            .map_or((None, 0), |(t, shift)| (Some(t), shift)),
        Bottom | BottomLeft | BottomRight => (axis(b.tall(), dy, screen.tall()).map(|a| a.0), 0),
        _ => (None, 0),
    };
    if wide.is_none() && tall.is_none() {
        return Ok(Vec::new());
    }
    let mut changes = model.resize_part(part, wide, tall, screen, group)?;
    if x != 0 || y != 0 {
        changes.extend(model.move_part(
            part,
            units.px_signed(x),
            units.px_signed(y),
            screen,
            group,
        )?);
    }
    Ok(changes)
}

/// File units (a 480-high screen) against pixels at one screen, rounding so that a pixel
/// count from `px` normalises back to exactly the units it came from.
#[derive(Clone, Copy)]
struct Units {
    tall: i64,
}

impl Units {
    fn new(screen: Screen) -> Units {
        Units {
            tall: i64::from(screen.tall().max(1)),
        }
    }

    fn fit(v: i64) -> i32 {
        i32::try_from(v.clamp(i64::from(i32::MIN), i64::from(i32::MAX))).unwrap_or(0)
    }

    /// Pixels to units, truncating toward zero as the game does.
    fn normalise(self, px: i32) -> i32 {
        Units::fit(i64::from(px) * 480 / self.tall)
    }

    /// The units a laid-out size of `px` came from: the least whose scaled size reaches it.
    fn of(self, px: i32) -> i32 {
        Units::fit(ceil_div(i64::from(px) * 480, self.tall))
    }

    /// The fewest pixels that normalise to `units` (positive).
    fn px(self, units: i32) -> i32 {
        Units::fit(ceil_div(i64::from(units) * self.tall, 480))
    }

    /// As `px`, keeping the sign, for a move.
    fn px_signed(self, units: i32) -> i32 {
        self.px(units.saturating_abs())
            .saturating_mul(units.signum())
    }

    /// A size in units from one unit up to four screens along an axis `span` pixels long.
    fn size(self, units: i32, span: u32) -> i32 {
        units.clamp(
            1,
            self.normalise(4_i32.saturating_mul(span.try_into().unwrap_or(i32::MAX)))
                .max(1),
        )
    }

    /// A move of `by` pixels from `at`, cut short so the result stays within four screens.
    fn clamp_pos(self, at: i32, by: i32, span: u32) -> i32 {
        let reach = 4 * i64::from(span);
        let to = (i64::from(at) + i64::from(by)).clamp(-reach, reach);
        Units::fit(to - i64::from(at))
    }
}

/// `n / d` rounded up, for a positive `d`.
fn ceil_div(n: i64, d: i64) -> i64 {
    n / d + i64::from(n % d > 0)
}

/// The scene of the first change a step from `before` to `after` removes, else adds; `None`
/// when the lists match or the change's file belongs to no scene. A file two scenes read
/// belongs to `open`, the scene showing when the edit was made.
fn step_scene(before: &[Change], after: &[Change], open: Scene) -> Option<Scene> {
    Scene::for_file_near(step_file(before, after)?, open)
}

/// The file of the first part-edit `Set` change a step from `before` to `after` removes, else
/// adds; `None` when the lists match or that change is not a `Set`.
fn step_file<'a>(before: &'a [Change], after: &'a [Change]) -> Option<&'a str> {
    fn differs<'a>(from: &'a [Change], to: &[Change]) -> Option<&'a Change> {
        from.iter().find(|c| !to.contains(c))
    }
    match differs(before, after)
        .or_else(|| differs(after, before))?
        .target()
    {
        ChangeTarget::Set(set) => Some(set.file()),
        _ => None,
    }
}

/// What the game checks before it shows `part`: the part's own toggle, else its nearest
/// code-toggled ancestor's.
pub fn shows_for(model: &HudModel, part: &PartKey) -> Option<ShowsFor> {
    std::iter::successors(Some(part), |key| model.part(key)?.parent())
        .filter_map(|key| model.part(key)?.code_toggled())
        .find_map(|toggled| toggled.shows_for())
}

/// The rule element nearest the first part named `panel`, where the game starts an event
/// naming it; `None` when no rule element holds it.
fn owning_element(model: &HudModel, panel: &str) -> Option<PartKey> {
    let named = (model.parts().iter()).find(|p| p.name().eq_ignore_ascii_case(panel))?;
    let rules = model.scene_rule().rules();
    std::iter::successors(Some(named), |p: &&Part| model.part(p.parent()?))
        .find(|p| {
            rules
                .iter()
                .any(|r| r.panel().eq_ignore_ascii_case(p.name()))
        })
        .map(|p| p.key().clone())
}

/// The first group no change in `changes` uses, for a new edit's changes.
fn next_group(changes: &[Change]) -> u32 {
    (changes.iter())
        .map(|c| c.group().saturating_add(1))
        .max()
        .unwrap_or(0)
}

/// Pushes onto a history stack, dropping the oldest step past `UNDO_LIMIT`.
fn push_capped(stack: &mut VecDeque<Step>, step: Step) {
    if stack.len() == UNDO_LIMIT {
        stack.pop_front();
    }
    stack.push_back(step);
}
