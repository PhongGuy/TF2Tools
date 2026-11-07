//! HUD options: named choices a player makes, stored as `Option` changes and compiled to key
//! edits on replay.

use std::sync::LazyLock;

use super::HudModel;
use super::changes::{Change, ChangeTarget, OptionChange, SetKey, check_hud_path};
use super::edits::EditRefused;
use super::expr::atoi;
use super::model::{SCHEME_FILE, live_blocks, root_block};

const HUDLAYOUT: &str = "scripts/hudlayout.res";

/// An option's chosen value.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum OptionValue {
    /// A switch.
    Bool(bool),
    /// A whole number.
    Int(i32),
    /// A colour as red, green, blue and alpha.
    Rgba([u8; 4]),
    /// The key of the picked choice; empty picks the group's "None".
    Pick(String),
    /// A line of text the player typed.
    Text(String),
}

/// How a player picks an option's value.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum OptionKind {
    /// An on and off switch.
    Toggle,
    /// A colour, also written to the scheme's derived names.
    Colour {
        /// The derived colours written beside the picked one.
        derived: Vec<Derived>,
    },
    /// A whole number in a range.
    Number {
        /// The smallest value.
        min: i32,
        /// The largest value.
        max: i32,
        /// The step between values.
        step: i32,
    },
    /// One of a list of choices, picked by key.
    Choice(Vec<Choice>),
    /// A short line of text.
    Text,
}

/// One pick of a [`OptionKind::Choice`]: its stored key, label and the switches it makes.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct Choice {
    key: String,
    label: String,
    actions: Vec<OptionAction>,
}

impl Choice {
    /// A choice stored as `key`, shown as `label`, making `actions` when picked.
    pub(super) fn new(key: &str, label: &str, actions: Vec<OptionAction>) -> Choice {
        Choice {
            key: key.to_owned(),
            label: label.to_owned(),
            actions,
        }
    }

    /// The key stored in a `Pick` value.
    pub fn key(&self) -> &str {
        &self.key
    }

    /// The player-facing label.
    pub fn label(&self) -> &str {
        &self.label
    }

    /// The switches the choice makes when picked.
    pub fn actions(&self) -> &[OptionAction] {
        &self.actions
    }
}

/// How an option's value becomes a key's text.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum ValueMap {
    /// The value itself, as the file spells it.
    Value,
    /// `on` when the switch is on; `off` when off, where `None` writes nothing.
    OnOff {
        /// The text written when on.
        on: String,
        /// The text written when off, if any.
        off: Option<String>,
    },
    /// A pick's own text, by choice key; a choice with no entry writes nothing.
    PerChoice(Vec<(String, String)>),
    /// The text with each `$value` replaced by the value, as the file spells it.
    Template(String),
}

/// A scheme colour derived from a picked colour.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Derived {
    /// `Dimm<name>`: the same colour at alpha 100.
    Dimm,
    /// `<name>_GreyedOut`: each channel at a quarter, opaque.
    GreyedOut,
    /// `<name>Shadow`: each channel at 60%, opaque.
    Shadow,
    /// `<name>Pulse`: the same colour, 50 less alpha when it has at least 50.
    Pulse,
}

/// One edit an option makes.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum OptionAction {
    /// Sets a key; `block` is relative to the file's root block.
    SetKey {
        /// The game path.
        file: String,
        /// The block path under the root block.
        block: Vec<String>,
        /// The key.
        key: String,
        /// The key's conditional, if any.
        cond: Option<String>,
        /// How the value becomes the key's text.
        value: ValueMap,
    },
    /// Flips one of the HUD's own file switches.
    Switch(SwitchAction),
    /// Writes a small settings file that one of the HUD's `#base` lines reads: `on` while on,
    /// `off` while off, where an empty text writes no file.
    WriteCfg {
        /// Where the file goes.
        place: CfgPlace,
        /// The text written when on.
        on: String,
        /// The text written when off.
        off: String,
    },
}

/// Where a settings file an option writes goes, as the HUD's `#base` line names it.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum CfgPlace {
    /// Under the game's `tf/cfg`, as a lowercase path below it.
    Game(String),
    /// Inside the HUD copy, as a copy path.
    Hud(String),
}

/// A file switch a HUD ships: lines commented in or out, or files moved.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum SwitchAction {
    /// Replaces the `#base` lines of `file`: `on` when on, `off` when off.
    BaseLines {
        /// The copy path.
        file: String,
        /// The `#base` targets written when on.
        on: Vec<String>,
        /// The `#base` targets written when off.
        off: Vec<String>,
    },
    /// Uncomments (on) or comments (off) the line of `file` that names `target`.
    CommentLine {
        /// The copy path holding the line.
        file: String,
        /// The path the line names, as written in the line.
        target: String,
    },
    /// Moves the file or folder `from` to `to` when on; back when off.
    Move {
        /// The shipped path.
        from: String,
        /// The path it takes when on.
        to: String,
    },
    /// Comments out every line of `file` holding one of `comment` and uncomments every line
    /// holding one of `uncomment` when on. Off does the reverse when `reverses`, else nothing,
    /// as for one choice of a pick.
    Lines {
        /// The copy path holding the lines.
        file: String,
        /// Text whose lines on comments out.
        comment: Vec<String>,
        /// Text whose lines on uncomments.
        uncomment: Vec<String>,
        /// Whether off undoes on, as for a switch.
        reverses: bool,
    },
    /// Copies every file under copy folder `from` into folder `to` (empty for the HUD's top),
    /// pictures left out, when on; off does nothing, as for one choice of a pick.
    CopyFolder {
        /// The option folder.
        from: String,
        /// The folder its files land in.
        to: String,
    },
    /// Takes the HUD's menu art (`.vtf` and `.vmt` files right in `folder`) out of the view
    /// when on, so the game's stock and seasonal backgrounds show; off does nothing.
    StockBackgrounds {
        /// The copy folder holding the menu art.
        folder: String,
    },
    /// Makes shipped background `image` (a picture under `materials/console/_disabled`) the
    /// menu art when on, writing the chapter list that names it; off does nothing.
    MenuBackground {
        /// The picture's name without `.vtf`.
        image: String,
    },
    /// Writes copy file `file` whole: `on` while on, `off` while off, where `None` leaves it.
    WriteText {
        /// The copy path written.
        file: String,
        /// The text written when on.
        on: String,
        /// The text written when off, if any.
        off: Option<String>,
    },
}

/// Where an option comes from.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum OptionSource {
    /// Shipped with the app.
    BuiltIn,
    /// Read from the HUD's own switches.
    Detected,
    /// Read from the bundled HUD option schema of that name.
    HudSchema {
        /// The schema's name.
        schema: String,
    },
}

/// Something outside the HUD an option needs before it shows in game.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum OptionNeed {
    /// The Transparent Viewmodels addon, installed in TF2.
    TransparentViewmodels,
}

/// One option a player can set: its label, kind, default and the edits it makes.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct OptionSpec {
    id: String,
    label: String,
    tooltip: Option<String>,
    group: String,
    kind: OptionKind,
    default: OptionValue,
    actions: Vec<OptionAction>,
    restart: bool,
    drawn: bool,
    source: OptionSource,
    needs: Option<OptionNeed>,
}

impl OptionSpec {
    /// A built-in option; the table is the only caller.
    pub(super) fn built_in(
        id: &str,
        label: &str,
        group: &str,
        kind: OptionKind,
        default: OptionValue,
        action: OptionAction,
        restart: bool,
    ) -> OptionSpec {
        OptionSpec {
            id: id.to_owned(),
            label: label.to_owned(),
            tooltip: None,
            group: group.to_owned(),
            kind,
            default,
            actions: vec![action],
            restart,
            drawn: false,
            source: OptionSource::BuiltIn,
            needs: None,
        }
    }

    /// One of the HUD's own options; the readers are the only callers.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn hud(
        id: &str,
        label: &str,
        group: &str,
        kind: OptionKind,
        default: OptionValue,
        actions: Vec<OptionAction>,
        restart: bool,
        source: OptionSource,
    ) -> OptionSpec {
        OptionSpec {
            id: id.to_owned(),
            label: label.to_owned(),
            tooltip: None,
            group: group.to_owned(),
            kind,
            default,
            actions,
            restart,
            drawn: false,
            source,
            needs: None,
        }
    }

    /// The option with `tooltip` shown beside its control.
    pub(super) fn with_tooltip(mut self, tooltip: Option<String>) -> OptionSpec {
        self.tooltip = tooltip;
        self
    }

    /// The option marked as needing `needs` to show in game.
    pub(super) fn with_needs(mut self, needs: Option<OptionNeed>) -> OptionSpec {
        self.needs = needs;
        self
    }

    /// Adds `choice` to a one-of option unless its key is taken; `restart` joins the option's.
    pub(super) fn add_choice(&mut self, choice: Choice, restart: bool) {
        if let OptionKind::Choice(choices) = &mut self.kind
            && !choices.iter().any(|c| c.key == choice.key)
        {
            choices.push(choice);
            self.restart |= restart;
        }
    }

    /// The stable id stored in `maker.json`.
    pub fn id(&self) -> &str {
        &self.id
    }

    /// The player-facing label.
    pub fn label(&self) -> &str {
        &self.label
    }

    /// A hint shown beside the control, if any.
    pub fn tooltip(&self) -> Option<&str> {
        self.tooltip.as_deref()
    }

    /// The group the option is listed and reset under.
    pub fn group(&self) -> &str {
        &self.group
    }

    /// How the value is picked.
    pub fn kind(&self) -> &OptionKind {
        &self.kind
    }

    /// The value when neither a choice nor the HUD gives one.
    pub fn default(&self) -> &OptionValue {
        &self.default
    }

    /// The edits the option makes.
    pub fn actions(&self) -> &[OptionAction] {
        &self.actions
    }

    /// Whether TF2 needs a restart to show a change.
    pub fn restart(&self) -> bool {
        self.restart
    }

    /// Whether the canvas draws a change; every other option shows only in TF2.
    pub fn drawn(&self) -> bool {
        self.drawn
    }

    /// Where the option comes from.
    pub fn source(&self) -> &OptionSource {
        &self.source
    }

    /// What outside the HUD the option needs to show in game, if anything.
    pub fn needs(&self) -> Option<OptionNeed> {
        self.needs
    }
}

/// A `SetKey` action on `block` (under the root) of `file`.
fn set_key(file: &str, block: &str, key: &str, value: ValueMap) -> OptionAction {
    OptionAction::SetKey {
        file: file.to_owned(),
        block: vec![block.to_owned()],
        key: key.to_owned(),
        cond: None,
        value,
    }
}

/// A switch that writes `on` when on and leaves the file alone when off.
fn on_only(on: &str) -> ValueMap {
    ValueMap::OnOff {
        on: on.to_owned(),
        off: None,
    }
}

/// The item colours as (label, scheme key, stock colour) from the default HUD's scheme.
const ITEM_COLOURS: [(&str, &str, [u8; 3]); 16] = [
    ("Normal", "QualityColorNormal", [178, 178, 178]),
    ("Unique", "QualityColorUnique", [255, 215, 0]),
    ("Strange", "QualityColorStrange", [207, 106, 50]),
    ("Vintage", "QualityColorVintage", [71, 98, 145]),
    ("Haunted", "QualityColorHaunted", [56, 243, 171]),
    ("Genuine", "QualityColorrarity1", [77, 116, 85]),
    ("Collector's", "QualityColorCollectors", [170, 0, 0]),
    ("Unusual", "QualityColorrarity4", [134, 80, 172]),
    ("Community", "QualityColorCommunity", [112, 176, 74]),
    ("Valve", "QualityColorDeveloper", [165, 15, 121]),
    ("Civilian", "ItemRarityCommon", [176, 195, 217]),
    ("Freelance", "ItemRarityUncommon", [94, 152, 217]),
    ("Mercenary", "ItemRarityRare", [75, 105, 255]),
    ("Commando", "ItemRarityMythical", [136, 71, 255]),
    ("Assassin", "ItemRarityLegendary", [211, 44, 230]),
    ("Elite", "ItemRarityAncient", [235, 75, 75]),
];

/// The built-in options in the order the panel lists them.
static BUILT_INS: LazyLock<Vec<OptionSpec>> = LazyLock::new(|| {
    let mut chat = OptionSpec::built_in(
        "chat_top",
        "Chat at the top",
        "HUD",
        OptionKind::Toggle,
        OptionValue::Bool(false),
        set_key("resource/ui/basechat.res", "HudChat", "ypos", on_only("0")),
        false,
    );
    // The canvas draws the chat, so the preview shows where this option moves it.
    chat.drawn = true;
    let mut rows = OptionSpec::built_in(
        "killfeed_rows",
        "Killfeed rows",
        "HUD",
        OptionKind::Number {
            min: 0,
            max: 20,
            step: 1,
        },
        OptionValue::Int(4),
        set_key(
            HUDLAYOUT,
            "HudDeathNotice",
            "MaxDeathNotices",
            ValueMap::Value,
        ),
        false,
    );
    // The canvas draws the killfeed's sample lines, so a row count change shows there.
    rows.drawn = true;
    let mut table = vec![
        rows,
        chat,
        OptionSpec::built_in(
            "hide_friends",
            "Hide the friends list",
            "Main menu",
            OptionKind::Toggle,
            OptionValue::Bool(false),
            set_key(
                "resource/ui/mainmenuoverride.res",
                "FriendsContainer",
                "visible",
                on_only("0"),
            ),
            true,
        ),
    ];
    table.extend(ITEM_COLOURS.iter().map(|&(label, key, [r, g, b])| {
        let id = format!("item_{}", label.to_lowercase().replace('\'', ""));
        OptionSpec::built_in(
            &id,
            label,
            "Item colours",
            OptionKind::Colour {
                derived: vec![Derived::Dimm, Derived::GreyedOut],
            },
            OptionValue::Rgba([r, g, b, 255]),
            set_key(SCHEME_FILE, "Colors", key, ValueMap::Value),
            true,
        )
    }));
    table
});

impl HudModel {
    /// The built-ins that fit this HUD, in table order.
    pub fn options(&self) -> Vec<&'static OptionSpec> {
        BUILT_INS.iter().filter(|spec| self.fits(spec)).collect()
    }

    /// An option fits when every key it sets lies in a loaded file's existing block.
    fn fits(&self, spec: &OptionSpec) -> bool {
        spec.actions.iter().all(|action| match action {
            OptionAction::SetKey { file, block, .. } => {
                let Some(doc) = check_hud_path(file).and_then(|p| self.pristine_document(&p))
                else {
                    return false;
                };
                let Some(root) = root_block(doc) else {
                    return false;
                };
                let path: Vec<&str> = std::iter::once(root.key())
                    .chain(block.iter().map(String::as_str))
                    .collect();
                !live_blocks(doc, &path, self.platform()).is_empty()
            }
            // No built-in flips a HUD's own switches or writes a settings file.
            OptionAction::Switch(_) | OptionAction::WriteCfg { .. } => false,
        })
    }
}

/// One key edit an option compiles to, `None` when its value cannot be spelled; a derived
/// colour's edit never counts as dropped.
pub(super) struct CompiledSet {
    pub(super) set: Option<SetKey>,
    pub(super) counts: bool,
}

impl HudModel {
    /// The key edits a stored choice makes on this (pristine) HUD, or `None` when the choice is
    /// refused: an unknown source or id, an option that does not fit, or a value of the wrong
    /// kind, out of range or off-step. A switch turned off may compile to no edits.
    pub(super) fn compile_option(&self, choice: &OptionChange) -> Option<Vec<CompiledSet>> {
        // The HUD's own choices are applied beneath the model, by the switch layer.
        if [OptionChange::HUD, OptionChange::SCHEMA].contains(&choice.source()) {
            return Some(Vec::new());
        }
        if choice.source() != OptionChange::BUILTIN {
            return None;
        }
        let spec = BUILT_INS.iter().find(|spec| spec.id == choice.id())?;
        if !self.fits(spec) || !value_suits(&spec.kind, choice.value()) {
            return None;
        }
        let mut sets = Vec::new();
        for action in &spec.actions {
            let OptionAction::SetKey {
                file,
                block,
                key,
                cond,
                value,
            } = action
            else {
                continue;
            };
            // Fit guarantees the root block, whose spelling differs between HUDs.
            let root = check_hud_path(file)
                .and_then(|p| self.pristine_document(&p))
                .and_then(root_block)?;
            let path: Vec<&str> = std::iter::once(root.key())
                .chain(block.iter().map(String::as_str))
                .collect();
            let make =
                |key: &str, text: &str| SetKey::new(file, &path, key, cond.as_deref(), None, text);
            let text = match (value, choice.value()) {
                (ValueMap::Value, OptionValue::Int(v)) => Some(v.to_string()),
                (ValueMap::Value, OptionValue::Rgba(c)) => Some(rgba_text(*c)),
                (ValueMap::Template(t), v) => template_text(t, v, &|_| false),
                (ValueMap::OnOff { on, .. }, OptionValue::Bool(true)) => Some(on.clone()),
                (ValueMap::OnOff { off, .. }, OptionValue::Bool(false)) => match off {
                    Some(off) => Some(off.clone()),
                    None => continue,
                },
                // A table entry pairing a map with a value it cannot spell leaves a dropped edit.
                _ => None,
            };
            let Some(text) = text else {
                sets.push(CompiledSet {
                    set: None,
                    counts: true,
                });
                continue;
            };
            sets.push(CompiledSet {
                set: Some(make(key, &text)),
                counts: true,
            });
            // Derived colours go only where the HUD's scheme already names them.
            if let (OptionKind::Colour { derived }, OptionValue::Rgba(c)) =
                (&spec.kind, choice.value())
            {
                for d in derived {
                    let (name, colour) = derived_colour(*d, key, *c);
                    if self.loaded_scheme().named_colour(&name).is_some() {
                        sets.push(CompiledSet {
                            set: Some(make(&name, &rgba_text(colour))),
                            counts: false,
                        });
                    }
                }
            }
        }
        Some(sets)
    }
}

impl HudModel {
    /// The last stored choice for built-in `id` in `changes` that replay keeps, else the HUD's
    /// pristine value, else the default; `None` when `id` is not an option this HUD offers.
    pub fn option_value(&self, id: &str, changes: &[Change]) -> Option<OptionValue> {
        let spec = self.offered(id)?;
        // A stored value replay would refuse, or one that writes nothing (a switch whose off
        // leaves the file alone), is skipped for the choice before it, so the panel shows what
        // is in force.
        let stored = changes.iter().rev().find_map(|change| {
            match change.target() {
                ChangeTarget::Option(o) if o.source() == OptionChange::BUILTIN && o.id() == id => {
                    Some(o)
                }
                _ => None,
            }
            .filter(|o| value_suits(&spec.kind, o.value()))
            .filter(|o| {
                self.compile_option(o)
                    .is_some_and(|sets| sets.iter().any(|s| s.set.is_some()))
            })
        });
        Some(
            stored
                .map(|o| o.value().clone())
                .or_else(|| self.pristine_value(spec))
                .unwrap_or_else(|| spec.default.clone()),
        )
    }

    /// Whether turning built-in switch `id` off changes this HUD: false for a switch whose off
    /// writes nothing on a HUD that already has it on, and for an id that is no offered switch.
    pub fn option_can_turn_off(&self, id: &str) -> bool {
        let off = OptionChange::new(OptionChange::BUILTIN, id, OptionValue::Bool(false));
        self.offered(id)
            .is_some_and(|spec| matches!(spec.kind, OptionKind::Toggle))
            && self.option_value(id, &[Change::option(0, off)]) == Some(OptionValue::Bool(false))
    }

    /// The next change list with `value` chosen for built-in `id` in `group`: earlier choices
    /// for `id` are removed and one `Option` change appended, unless `value` is the pristine
    /// value, when none is appended. NotOffered for an id this HUD does not offer; BadValue for
    /// a value of the wrong kind, out of range or off-step.
    pub fn set_option(
        &self,
        id: &str,
        value: OptionValue,
        changes: &[Change],
        group: u32,
    ) -> Result<Vec<Change>, EditRefused> {
        let spec = self.offered(id).ok_or(EditRefused::NotOffered)?;
        if !value_suits(&spec.kind, &value) {
            return Err(EditRefused::BadValue);
        }
        let mut next = without_choices(changes, |c| c == id);
        if self.option_value(id, &[]).as_ref() != Some(&value) {
            let choice = OptionChange::new(OptionChange::BUILTIN, id, value);
            next.push(Change::option(group, choice));
        }
        Ok(next)
    }

    /// `changes` without the choices of every option in `group` (an option group name).
    pub fn reset_option_group(&self, group: &str, changes: &[Change]) -> Vec<Change> {
        let in_group = |id: &str| {
            BUILT_INS
                .iter()
                .any(|spec| spec.group == group && spec.id == id)
        };
        without_choices(changes, in_group)
    }

    /// `changes` without any built-in option choice, every group at once.
    pub fn reset_all_options(&self, changes: &[Change]) -> Vec<Change> {
        without_choices(changes, |_| true)
    }

    /// The built-in `id` when it fits this HUD.
    fn offered(&self, id: &str) -> Option<&'static OptionSpec> {
        BUILT_INS
            .iter()
            .find(|spec| spec.id == id)
            .filter(|spec| self.fits(spec))
    }

    /// The value the pristine HUD already has at the option's first key, read as the option's
    /// kind: a colour from the merged scheme (it may live in a `#base` file), any other kind
    /// from the line the option would replace.
    fn pristine_value(&self, spec: &OptionSpec) -> Option<OptionValue> {
        let Some(OptionAction::SetKey {
            file,
            block,
            key,
            cond,
            value,
        }) = spec.actions.first()
        else {
            return None;
        };
        if let OptionKind::Colour { .. } = spec.kind {
            return self
                .loaded_scheme()
                .named_colour(key)
                .map(|c| OptionValue::Rgba(c.to_array()));
        }
        let doc = check_hud_path(file).and_then(|p| self.pristine_document(&p))?;
        let root = root_block(doc)?;
        let path: Vec<&str> = std::iter::once(root.key())
            .chain(block.iter().map(String::as_str))
            .collect();
        // The line in force: the first direct line of the key with the same conditional (any
        // case) in the last live block on the path that has one, as duplicates apply in order.
        let same_cond = |c: Option<&str>| match (c, cond.as_deref()) {
            (Some(a), Some(b)) => a.eq_ignore_ascii_case(b),
            (a, b) => a == b,
        };
        let text = (live_blocks(doc, &path, self.platform()).into_iter().rev())
            .find_map(|block| {
                block.children().iter().find(|e| {
                    e.text().is_some() && e.key().eq_ignore_ascii_case(key) && same_cond(e.cond())
                })
            })
            .and_then(|e| e.text())?;
        match (value, &spec.kind) {
            (ValueMap::Value, OptionKind::Number { .. }) => {
                Some(OptionValue::Int(atoi(text.as_bytes())))
            }
            (ValueMap::OnOff { on, .. }, _) => Some(OptionValue::Bool(text.trim() == on)),
            _ => None,
        }
    }
}

/// The value of a built-in option choice whose id passes `wanted`.
fn built_in_choice(change: &Change, wanted: impl Fn(&str) -> bool) -> Option<&OptionValue> {
    match change.target() {
        ChangeTarget::Option(o) if o.source() == OptionChange::BUILTIN && wanted(o.id()) => {
            Some(o.value())
        }
        _ => None,
    }
}

/// `changes` without the built-in option choices whose id passes `wanted`.
fn without_choices(changes: &[Change], wanted: impl Fn(&str) -> bool) -> Vec<Change> {
    changes
        .iter()
        .filter(|change| built_in_choice(change, &wanted).is_none())
        .cloned()
        .collect()
}

/// Whether `value` is the kind the option takes and, for a number, in range and on a step.
pub(super) fn value_suits(kind: &OptionKind, value: &OptionValue) -> bool {
    match (kind, value) {
        (OptionKind::Toggle, OptionValue::Bool(_)) => true,
        (OptionKind::Colour { .. }, OptionValue::Rgba(_)) => true,
        (OptionKind::Choice(choices), OptionValue::Pick(key)) => {
            choices.iter().any(|c| &c.key == key)
        }
        (OptionKind::Text, OptionValue::Text(text)) => text_suits(text),
        (OptionKind::Number { min, max, step }, OptionValue::Int(v)) => {
            // Widened so a hostile value or bound cannot overflow; a zero step takes nothing.
            (*min..=*max).contains(v)
                && (i64::from(*v) - i64::from(*min))
                    .checked_rem(i64::from(*step))
                    .is_some_and(|r| r == 0)
        }
        _ => false,
    }
}

/// A typed text is at most this many bytes.
pub(super) const MAX_TEXT: usize = 128;

/// Whether typed `text` can stand in a quoted file value: short, one line, and free of the
/// quotes and braces that would end the value or open a block.
pub(super) fn text_suits(text: &str) -> bool {
    text.len() <= MAX_TEXT && !text.chars().any(|c| c.is_control() || "\"{}".contains(c))
}

/// `template` with each `{$name ? a : b}` reference resolved (`a` when switch `name` is on
/// by `is_on`, `$value` meaning this option) and then each `$value` replaced by `value` as a
/// file spells it; `None` for a value with no spelling or a malformed reference.
pub(super) fn template_text(
    template: &str,
    value: &OptionValue,
    is_on: &dyn Fn(&str) -> bool,
) -> Option<String> {
    let mut resolved = String::with_capacity(template.len());
    let mut rest = template;
    while let Some(open) = rest.find('{') {
        resolved.push_str(&rest[..open]);
        let close = rest[open..].find('}')? + open;
        let (name, on, off) = reference(&rest[open + 1..close])?;
        let on_now = match name {
            "value" => *value == OptionValue::Bool(true),
            name => is_on(name),
        };
        resolved.push_str(if on_now { on } else { off });
        rest = &rest[close + 1..];
    }
    resolved.push_str(rest);
    if resolved.contains('}') {
        return None;
    }
    let text = match value {
        OptionValue::Int(v) => v.to_string(),
        OptionValue::Rgba(c) => rgba_text(*c),
        OptionValue::Text(t) => t.clone(),
        OptionValue::Pick(k) => k.clone(),
        OptionValue::Bool(_) => return None,
    };
    Some(resolved.replace("$value", &text))
}

/// The option names `template`'s `{$name ? a : b}` references name, `value` left out;
/// `None` when a brace holds anything else or a `$` names no reference or `$value`.
pub(super) fn template_refs(template: &str) -> Option<Vec<String>> {
    let mut names = Vec::new();
    let mut plain = String::new();
    let mut rest = template;
    while let Some(open) = rest.find('{') {
        plain.push_str(&rest[..open]);
        let close = rest[open..].find('}')? + open;
        let (name, ..) = reference(&rest[open + 1..close])?;
        if name != "value" {
            names.push(name.to_owned());
        }
        rest = &rest[close + 1..];
    }
    plain.push_str(rest);
    let stray = plain.contains('}') || plain.replace("$value", "").contains('$');
    (!stray).then_some(names)
}

/// A reference's inner text `$name ? a : b` as its parts, each a plain word.
fn reference(inner: &str) -> Option<(&str, &str, &str)> {
    let (name, sides) = inner.trim().strip_prefix('$')?.split_once('?')?;
    let (on, off) = sides.split_once(':')?;
    Some((word(name)?, word(on)?, word(off)?))
}

/// `w` trimmed when it is one plain word of letters, digits and underscores.
fn word(w: &str) -> Option<&str> {
    let w = w.trim();
    (!w.is_empty() && w.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')).then_some(w)
}

/// Whether `key` is one of the item-colour scheme names (any case).
pub(super) fn is_item_colour(key: &str) -> bool {
    ITEM_COLOURS
        .iter()
        .any(|(_, name, _)| name.eq_ignore_ascii_case(key))
}

/// The scheme name and colour `derived` writes beside `key` picked as `c`.
pub(super) fn derived_colour(derived: Derived, key: &str, c: [u8; 4]) -> (String, [u8; 4]) {
    // A share of a byte, rounded half to even as the schemas expect, always fits a byte.
    let share = |v: u8, f: f64| (f64::from(v) * f).round_ties_even() as u8;
    match derived {
        Derived::Dimm => (format!("Dimm{key}"), [c[0], c[1], c[2], 100]),
        Derived::GreyedOut => (format!("{key}_GreyedOut"), greyed_out(c)),
        Derived::Shadow => (
            format!("{key}Shadow"),
            [share(c[0], 0.6), share(c[1], 0.6), share(c[2], 0.6), 255],
        ),
        Derived::Pulse => (
            format!("{key}Pulse"),
            [c[0], c[1], c[2], if c[3] >= 50 { c[3] - 50 } else { c[3] }],
        ),
    }
}

/// A colour as the scheme spells it: `"r g b a"`.
pub(super) fn rgba_text([r, g, b, a]: [u8; 4]) -> String {
    format!("{r} {g} {b} {a}")
}

/// Each channel at a quarter, rounded half to even as the schemas expect, and opaque.
fn greyed_out([r, g, b, _]: [u8; 4]) -> [u8; 4] {
    // A quarter of a byte, rounded, always fits a byte.
    let quarter = |c: u8| (f64::from(c) * 0.25).round_ties_even() as u8;
    [quarter(r), quarter(g), quarter(b), 255]
}
