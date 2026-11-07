//! The HUD Maker page: draw the default HUD at a chosen screen shape and move, resize, layer,
//! hide and recolour its parts.

pub mod animations;
mod backdrop;
mod borrow;
mod canvas;
pub mod checks;
mod colour_view;
pub mod editor;
mod history;
pub mod live;
mod markers;
mod menu_art;
mod options;
mod panel;
mod parts;
mod pictures;
pub mod rank_paint;
mod recipes;
mod screens;
mod shortcuts;
pub mod situation;
mod start_card;
mod step_name;
pub mod timeline;
mod tint;
mod tip;
pub mod view;
mod zoom;

pub use shortcuts::SHORTCUTS;

use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonGroup, ButtonVariants as _};
use gpui_kit::component::color_picker::{ColorPickerEvent, ColorPickerState};
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::popover::Popover;
use gpui_kit::component::select::{SelectEvent, SelectState};
use gpui_kit::component::slider::{SliderEvent, SliderState};
use gpui_kit::component::{
    ActiveTheme as _, Disableable as _, Selectable as _, Sizable as _, h_flex, v_flex,
};
use gpui_kit::prelude::FluentBuilder as _;
use std::borrow::Cow;
use std::cell::{Cell, RefCell};
use std::collections::{BTreeSet, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;

use gpui_kit::{
    Anchor, AnyElement, AnyWindowHandle, App, AppContext as _, Bounds, Context, Entity,
    EventEmitter, FocusHandle, Global, InteractiveElement as _, IntoElement, ParentElement, Pixels,
    Point, Render, RenderImage, ScrollHandle, ScrollStrategy, SharedString, Size,
    StatefulInteractiveElement as _, Styled, Subscription, Task, TestSupportExt as _, TextSystem,
    UniformListScrollHandle, Window, div, point, px, size,
};
use tf2tools_core::{
    BoardMode, Change, ChangeTarget, ClassSchema, EditRefused, FontEdit, GameText, HudAssets,
    HudFiles, HudFilesError, HudId, HudImage, HudLoadError, HudModel, HudOptions, KillIcon,
    LogLevel, MakeHudError, MakeTarget, MakerBase, MeterKind, NewLogEntry, OptionKind, OptionValue,
    PartKey, Platform, RgbaImage, SampleSet, Scene, SceneRule, Screen, Team, drawn_image,
    font_families, picture_material, scene_image,
};

use crate::activity;
use crate::ids;
use crate::keymap::{
    CancelHudDrag, HUD_MAKER_CONTEXT, HideHudPart, NUDGE_FAR, NudgeDown, NudgeDownFar, NudgeLeft,
    NudgeLeftFar, NudgeRight, NudgeRightFar, NudgeUp, NudgeUpFar, RedoHudChange, ShowHudShortcuts,
    ToggleHudPartHidden, UndoHudChange, ZoomFitHud, ZoomInHud, ZoomOutHud,
};
use crate::library_events::{LibraryChanged, LibraryEvents};
use crate::local_messages::{
    HUD_BASE_CHANGED, HUD_FILES, HUD_OPTIONS_UNREADABLE, LocalFailure, ReopenNotice,
    describe_hud_load, describe_make_hud, describe_settings, left_out, reopen_notice,
};
use crate::pages::crosshair_maker::model::{REDO_TIP, UNDO_TIP, hsla_of_rgba, rgb_of};
use crate::pages::huds::{INSTALL_FAILED, install_job, install_toast};
use crate::picker::PickKind;
use crate::previews::to_render_image;
use crate::services::{OpError, Services, require_tf2, run_blocking};
use crate::shell::CONTENT_PADDING;
use crate::shell::external_huds::{HudAction, check_external};
use crate::shell::route::{NavigateTo, Page};
use crate::shell::toast::{ToastKind, lift_toasts, toast, toast_error};
use crate::widgets::labelled;
use canvas::{LastPress, PartDrag};
use editor::{FontAddRefused, HudEditor};
use panel::PanelDrag;
pub use situation::SituationSlider;
use tip::tipped;
use view::{CanvasFit, Panel, PanelMode, PanelPlace, Places, Zoom};

const LOADING: &str = "Loading the default HUD...";
const ADD_REFUSED: &str = "That part can't be added here.";
const DETACH_FIRST: &str = "Detach the parts attached to it first.";
const SAVE_FAILED: &str = "Failed to save HUD";
const NOT_A_FONT: &str = "That file isn't a font TF2Tools can read.";
const FONT_BAD_NAME: &str = "TF2Tools can't use a font with that name in a HUD.";
const FONT_HUD_FULL: &str = "This HUD can't take another font file.";
const FONT_PART: &str = "This part's font can't be changed in this HUD, so the font wasn't added.";
// Why an arrow key moved nothing, one line per reason the HUD Maker refuses a nudge.
const NUDGE_GAME_PLACES: &str = "The game places this part itself, so it can't be moved.";
const NUDGE_SIDEWAYS_SET: &str =
    "The game sets where this part sits from side to side, so it only moves up and down.";
const NUDGE_UP_DOWN_SET: &str =
    "The game sets where this part sits from top to bottom, so it only moves left and right.";
const NUDGE_ODD_PLACE: &str =
    "This part's place is written in a way the HUD Maker can't change, so it can't be moved.";
const NUDGE_FROM_BASE: &str =
    "This part comes from another file the HUD includes, so it can't be moved here yet.";
const NUDGE_UNREADABLE: &str = "This part's file couldn't be read, so it can't be moved.";
const NUDGE_REFUSED: &str = "This part can't be moved that way.";
// A picked font file over this is refused before it is read, as core refuses one.
const MAX_FONT_BYTES: u64 = 64 * 1024 * 1024;
// A HUD's listed font files past either limit are skipped, so a hostile list can't fill memory.
const MAX_FONT_FILES: usize = 64;
const MAX_FONT_TOTAL: u64 = 128 * 1024 * 1024;
// The space between a docked panel and the screen.
const DOCK_GAP: Pixels = px(16.);
// With no place of its own, the parts panel sits this far in from the area's right edge.
const PARTS_INSET: f32 = 16.;
// The space between the floating toolbar and the area's sides.
const TOOLBAR_GAP: Pixels = px(16.);
// The space under the toolbar, and beside it when it sits in a corner.
const TOOLBAR_EDGE: Pixels = px(8.);
// The least space between the toolbar and the screen above it.
const TOOLBAR_SPACE: Pixels = px(4.);
// The space between the open toolbar's top and the toasts lifted above it.
const TOAST_GAP: Pixels = px(8.);
// The side panel edits the selection, so it folds only for a part covering at most this share
// of the canvas area.
const FOLD_SHARE_MAX: f32 = 0.25;
// The margins counted when deciding whether both floating panels fit side by side.
const NARROW_MARGIN: Pixels = px(16.);
// The share of the area the open part list's bottom keeps within; above the side panel's, so a
// roomy window shows the whole list.
const PARTS_PANEL_SHARE: f32 = 0.75;
// A dock leaving the screen shorter than this floats its panels until the window grows.
const DOCK_MIN_SCREEN: Pixels = px(200.);
// The open part list covers at most this share of the screen, but always shows this many rows.
const PARTS_COVER_MAX: f32 = 0.25;
const PARTS_MIN_ROWS: f32 = 3.;
// The narrowest toolbar that holds the screen menu, Undo, Redo and the name on one row beside
// its hide button, padding and border, so the whole bar fits two rows; narrower, the save row
// moves into More.
const TOOLBAR_TWO_ROWS: Pixels = px(416.);
// Estimates for the scene tab row: pixels per label letter and per tab, and the least kept for
// the note, which cuts itself short rather than push the tabs into a menu.
const SCENE_TAB_LETTER: f32 = 7.5;
const SCENE_TAB_PAD: f32 = 28.;
const SCENE_NOTE_ROOM: f32 = 40.;
// The match mode dropdown beside the Scoreboard tab: its longest label, icon and padding.
const BOARD_MODE_WIDTH: f32 = 150.;
// The Minimal HUD chip off the HUD tab: its label, padding and gap.
const MINMODE_CHIP_WIDTH: f32 = 120.;
const MINMODE_CHIP_TIP: &str = "The minimal HUD is on. Click to turn it off.";

// Where the toolbar sits so it never covers the screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Seat {
    // Centred in the letterbox under the screen.
    Below,
    // Shut, in the bottom corner of the letterbox beside the screen.
    Corner,
    // Centred in a band the screen gives up at the area's bottom.
    Band,
}

// One panel: where the player put it, a temporary offset clear of the selection, its window
// bounds from the last frame (measured by a watcher inside it), its size when last drawn open,
// its title bar's height and a running title drag. `folded` shows only the header for now,
// with no room beside the selection; `held_open` is the player expanding it anyway.
struct PanelState {
    place: PanelPlace,
    off: Option<Point<Pixels>>,
    bounds: Rc<Cell<Bounds<Pixels>>>,
    open_size: Rc<Cell<Size<Pixels>>>,
    title_height: Rc<Cell<Pixels>>,
    drag: Option<PanelDrag>,
    folded: bool,
    held_open: bool,
    // The height settling cut the open panel to, so it clears what lies below it.
    short: Option<Pixels>,
}

impl PanelState {
    fn new(place: PanelPlace) -> PanelState {
        PanelState {
            place,
            off: None,
            bounds: Rc::new(Cell::new(Bounds::default())),
            open_size: Rc::new(Cell::new(Size::default())),
            title_height: Rc::new(Cell::new(px(0.))),
            drag: None,
            folded: false,
            held_open: false,
            short: None,
        }
    }
}

// The part list's settling spot in area coordinates and what it was weighed against: the
// area's size, the list's size and the screen's box.
struct PartsSpot {
    at: Point<Pixels>,
    // Whether the spot was judged against the parts its scene painted, not a stale frame's.
    painted: bool,
    weighed: Weighed,
}

type Weighed = (Size<Pixels>, Size<Pixels>, Bounds<Pixels>);

// A drawn part's window bounds and whether it is the selection.
type Drawn = (Bounds<Pixels>, bool);

// The one writer of the places setting: the newest text waiting to be written, and whether a
// writer runs. Shared with that writer, so a save asked for just before leaving still lands.
#[derive(Default)]
struct PlaceSaves {
    latest: Option<String>,
    writing: bool,
}

// A Save and leave's way out, run once its save lands.
type AfterSave = Box<dyn FnOnce(&mut Window, &mut App)>;

// The page's one load: still reading, read into an editor, or no usable game files.
enum Load {
    Reading,
    Ready(Box<Loaded>),
    NoGame,
    // A made HUD that can't be reopened, with the line shown in place of the canvas.
    CantOpen(String),
}

// What a load job read: the game's files and the HUD over them, plus, for a reopened made HUD,
// its library row, saved changes and what to tell the player about them.
struct Opened {
    // The files the model reads (a copy's with its own choices switched in), and the files as
    // they are on disk, which saves and switch reloads start from.
    files: Arc<HudFiles>,
    pristine_files: Arc<HudFiles>,
    // A copy's own options and the base it was opened at; `None` for a HUD that is no copy.
    options: Option<Arc<HudOptions>>,
    opened_base: Option<MakerBase>,
    platform: Platform,
    text: Arc<GameText>,
    model: HudModel,
    images: Images,
    menu_backgrounds: MenuBackgrounds,
    changes: Vec<Change>,
    made: Option<(HudId, String)>,
    // For a HUD copy, the tf folder its files layer over, so a save can reopen them.
    copy_tf: Option<PathBuf>,
    notice: Option<ReopenNotice>,
    // The added files' bytes of a reopened made HUD, the same files as fonts by source, and the
    // `CustomFontFiles` the HUD lists.
    assets: HudAssets,
    asset_fonts: HashMap<String, FontBytes>,
    fonts: Vec<FontBytes>,
}

// A load job's result: a HUD, a made HUD that can't be reopened (its line), or no game files.
type LoadJob = Task<Result<Result<Opened, String>, OpError>>;

struct Loaded {
    // The files the editor shows, a copy's choices switched in; image, font and text reads
    // take these.
    files: Arc<HudFiles>,
    // The files without the switch layer, shared with each save job and switch reload.
    pristine_files: Arc<HudFiles>,
    // A copy's base when it was opened; a save refuses once an Update has replaced it.
    opened_base: Option<MakerBase>,
    platform: Platform,
    // The game's English text, which `#token` labels draw.
    text: Arc<GameText>,
    editor: HudEditor,
    images: Images,
    // The main menu's background names, keys into `images` once `menu_art` has read them.
    menu_backgrounds: MenuBackgrounds,
    // The names the HUD's own files give, and the source of the picked chapter list they
    // were last worked out for (`None` for the HUD's own).
    hud_menu_backgrounds: MenuBackgrounds,
    menu_source: Option<String>,
    // Counts those refreshes, so a read started before one never lands after it.
    menu_generation: u64,
    menu_art: [menu_art::MenuArt; 2],
    // The text system's family names, lowercased, once the HUD's font files are registered.
    font_names: HashSet<String>,
}

// Each image part's decoded texture; a part missing here draws as a labelled outline.
type Images = HashMap<PartKey, Arc<RenderImage>>;

// The image names of the main menu's background for a narrow and a wide screen.
type MenuBackgrounds = [Option<String>; 2];

// The area, panel and toolbar bounds, title heights, killfeed band and drawn parts the layout
// was last settled against.
type Checked = (
    Bounds<Pixels>,
    [Bounds<Pixels>; 2],
    Bounds<Pixels>,
    [Pixels; 2],
    Option<Bounds<Pixels>>,
    Option<Bounds<Pixels>>,
    Vec<Drawn>,
);

/// How a save ended, when it did not fail with a toast.
enum Saved {
    /// Saved as `id` under `name`; `dropped` changes no longer resolved and were left out.
    Done {
        id: HudId,
        name: String,
        dropped: usize,
        /// What the save did to the installed copy.
        live: live::Live,
        /// A saved copy's files read afresh, since the save rewrote its layers, and their
        /// view with the sent choices switched in.
        files: Option<Box<(HudFiles, HudFiles)>>,
    },
    /// Another library HUD has this trimmed name; the field says so.
    NameTaken(String),
    /// The HUD being saved over was deleted meanwhile; the next save makes a new one.
    Gone(OpError),
}

/// The toolbar's screen shape buttons, in `Screen::PRESETS` order.
const SCREEN_LABELS: [&str; 6] = ["16:9", "16:10", "4:3", "5:4", "21:9", "32:9"];

/// A side panel typed field.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Field {
    X,
    Y,
    Wide,
    Tall,
    Layer,
    RowHeight,
    RowGap,
    SectionGap,
    Text,
    LineHeight,
    LineSpacing,
    Lifetime,
    /// The Size of the Font row last pressed; the row's key travels with what it shows.
    FontSize,
    /// The value of the "All settings" row last pressed; its key travels with what it shows.
    Setting,
}

impl Field {
    const ALL: [Field; 14] = [
        Field::X,
        Field::Y,
        Field::Wide,
        Field::Tall,
        Field::Layer,
        Field::RowHeight,
        Field::RowGap,
        Field::SectionGap,
        Field::Text,
        Field::LineHeight,
        Field::LineSpacing,
        Field::Lifetime,
        Field::FontSize,
        Field::Setting,
    ];

    /// The Number row's key this field edits; None for the box, layer and Text fields.
    pub(crate) fn number_key(self) -> Option<&'static str> {
        match self {
            Field::RowHeight => Some("linespacing"),
            Field::RowGap => Some("linegap"),
            Field::SectionGap => Some("sectiongap"),
            Field::LineHeight => Some("LineHeight"),
            Field::LineSpacing => Some("LineSpacing"),
            Field::Lifetime => Some("delta_lifetime"),
            _ => None,
        }
    }

    // The schema row key a field shows and commits, beyond the box and layer.
    fn row_key(self) -> Option<&'static str> {
        match self {
            Field::Text => Some(TEXT_KEY),
            _ => self.number_key(),
        }
    }

    /// The field that edits the Number row `key`.
    pub(crate) fn of_number(key: &str) -> Option<Field> {
        Field::ALL.into_iter().find(|f| f.number_key() == Some(key))
    }
}

/// The Text row's key.
const TEXT_KEY: &str = "labelText";

// What a field's last refresh wrote, and for which part and row; a commit goes to them.
#[derive(Debug, Clone)]
struct ShownText {
    part: PartKey,
    // The Font row a Size field showed, or the setting a Setting field showed; the other fields
    // know their row.
    key: Option<String>,
    text: String,
}

/// Which list a Font row has open.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum FontList {
    Fonts,
    Families,
}

type Shown = [Option<ShownText>; Field::ALL.len()];

/// Why a number box's last typed value did not land as typed, for the part it was typed for.
#[derive(Debug, Clone)]
pub(super) struct FieldNote {
    part: PartKey,
    pub(super) text: SharedString,
    // A refusal reads in the warning colour; a rounding is only information.
    pub(super) refused: bool,
}

type Notes = [Option<FieldNote>; Field::ALL.len()];

/// The HUD Maker page; it reads the game's default HUD once, when it is built.
pub struct HudMakerPage {
    load: Load,
    // The canvas area's window bounds from the last frame; the screen is fitted inside it.
    area: Rc<Cell<Bounds<Pixels>>>,
    // What a wrapped label's lines are measured with on the canvas.
    text_system: Arc<TextSystem>,
    ui_family: SharedString,
    // Image textures tinted by their draw colour, made as the canvas first draws them.
    tints: RefCell<tint::Tints>,
    parts_scroll: UniformListScrollHandle,
    // The side panel body's scroll, and whether a font list or the named colours just opened
    // and should be shown.
    panel_scroll: ScrollHandle,
    reveal_font_list: bool,
    reveal_names: bool,
    win: AnyWindowHandle,
    // Indexed by `Panel`.
    panels: [PanelState; 2],
    toolbar_collapsed: bool,
    // The toolbar's window bounds from the last frame.
    toolbar_bounds: Rc<Cell<Bounds<Pixels>>>,
    // Each drawn killfeed line's window bounds from the last frame; empty when none draws.
    killfeed_bounds: Rc<RefCell<Vec<Bounds<Pixels>>>>,
    // Each drawn chat line's window bounds from the last frame; empty when none draws.
    chat_bounds: Rc<RefCell<Vec<Bounds<Pixels>>>>,
    // The window bounds of every part the canvas drew last frame that the game shows, each
    // marked when it is the selection; the part list keeps off them.
    drawn_bounds: Rc<RefCell<Vec<Drawn>>>,
    // The scene `drawn_bounds` was taken from, so a scene switch judges the list's spot against
    // the new scene's parts and never the old ones.
    drawn_scene: Cell<Option<Scene>>,
    // Whether each part list row's name was cut short in the last frame it drew.
    cut_names: Rc<RefCell<HashMap<PartKey, bool>>>,
    // The part list's filter box, the groups the player folded, each part's name as the
    // filter reads it, and the model rows the list drew last frame, in list order.
    parts_filter: Entity<InputState>,
    folded: HashSet<PartKey>,
    filter_names: HashMap<PartKey, String>,
    list_rows: Rc<[usize]>,
    // Each label's drawn text in window bounds from the last frame, kept only while a
    // hard-to-read marker shows, so the markers keep off the glyphs.
    text_bounds: Rc<RefCell<HashMap<PartKey, Bounds<Pixels>>>>,
    // The HUD, scene, screen and shown changes `text_bounds` were measured for.
    text_bounds_for: Option<(Arc<HudFiles>, Scene, Screen, Vec<Change>)>,
    // The selection the panels last settled for; a new one ends any panel held open.
    settled_for: Option<PartKey>,
    // Where the part list settles from, kept while it stays off the killfeed and the other
    // panels and the area, screen and list size hold; the player placing the list forgets it.
    parts_spot: Option<PartsSpot>,
    // The floating panel the pointer was on when an edit kept the selection; it stays where it
    // is drawn until the pointer leaves it, the selection changes or the player moves it.
    held: Option<Panel>,
    // Where the latest press landed, until a key goes down or the pointer leaves the window.
    pressed_at: Option<Point<Pixels>>,
    // How many frames found moved bounds and settled again.
    relayouts: u64,
    // The toast lift last set for the open toolbar, so the theme changes only when it moves.
    toast_lift: Option<Pixels>,
    place_saves: Rc<RefCell<PlaceSaves>>,
    // The part under the pointer, on the canvas or its list row, outlined on the canvas.
    hovered: Option<PartKey>,
    // Whether the canvas, not a row, set `hovered`, so only a canvas leave clears it.
    hover_on_canvas: bool,
    // The latest canvas press, so a second press on its spot steps on to the next part.
    last_press: Option<LastPress>,
    part_drag: Option<PartDrag>,
    // The canvas's zoom and pan, for the screen shape they were made at; another shape fits.
    zoom: Option<(Screen, Zoom)>,
    // Touchpad travel gathered toward the next Ctrl+wheel zoom step.
    zoom_carry: Pixels,
    // Space is down, so a left press on the canvas pans; and the running pan's last pointer.
    space_held: bool,
    canvas_pan: Option<Point<Pixels>>,
    // Match size is waiting for the press that names the part whose size to copy.
    matching_size: bool,
    // The refused nudge already toasted for the key held now; a key coming up, a new selection
    // or a new HUD forgets it.
    nudge_refused: Option<(PartKey, &'static str)>,
    // Tracked on the page root, so Undo and Redo keys work while the page has focus.
    focus: FocusHandle,
    checked: Checked,
    fields: [Entity<InputState>; Field::ALL.len()],
    shown: Shown,
    notes: Notes,
    colour: Entity<ColorPickerState>,
    // Whether the active Colour row lists the HUD's colour names.
    names_open: bool,
    // The part and key of the Colour row the picker edits; `None` is the part's colour slot.
    // It lapses when another part is selected.
    active_colour: Option<(PartKey, String)>,
    // The part, row key and change list the last picker change left; the next change on the
    // same row, with nothing else edited and the picker not closed between, replaces it.
    picking: Option<(PartKey, Option<String>, Vec<Change>)>,
    // The part and Font row whose Size shows the input, and the one with a list open; both
    // lapse when another part is selected.
    font_size_row: Option<(PartKey, String)>,
    font_open: Option<(PartKey, String, FontList)>,
    // The families the Family lists offer, for the scene and change list they were read at;
    // reading them parses each added font file, so they are not read per frame.
    families: Option<(Scene, Vec<Change>, Vec<String>)>,
    // Whether "All settings" is open, and the part and setting its one input edits; the setting
    // lapses when another part is selected.
    settings_open: bool,
    setting_key: Option<(PartKey, String)>,
    name: Entity<InputState>,
    // The trimmed name core last refused as taken, and the line shown under the field.
    name_error: Option<(String, String)>,
    // The library HUD Save writes over, and the name it was last saved under.
    saved_hud: Option<HudId>,
    saved_name: String,
    // While the open HUD is a copy, the tf folder its files layer over.
    copy_tf: Option<PathBuf>,
    // The field's trimmed text, kept by an observer so `can_leave` needs no context.
    typed_name: String,
    saving: bool,
    // Whether the running save came from Save as new, for that button's spinner.
    saving_as_new: bool,
    installing: bool,
    // Whether the toolbar shows how to see a live update in TF2.
    live_note: bool,
    // Counts finished HUD loads, so a font file picked for one HUD never lands in the next.
    loads: u64,
    // Counts picture picks, so only the latest one lands when an earlier read finishes last.
    picks: u64,
    // How many menu background reads have started, for the tests.
    menu_reads: usize,
    // One slider per Number option the loaded HUD offers, rebuilt with their subscriptions
    // when a HUD lands.
    sliders: HashMap<&'static str, Entity<SliderState>>,
    // The running switch reload, keyed by the HUD choices it builds for; dropping it cancels
    // it. Also the choices whose reload failed, so it is not retried every frame, and the
    // counts of reloads started and landed, for the tests.
    switch_job: Option<(Vec<Change>, Task<()>)>,
    // A copy's own Number options' sliders and long Choice options' menus, by option id,
    // rebuilt with the built-in sliders.
    hud_sliders: HashMap<String, Entity<SliderState>>,
    hud_selects: HashMap<String, Entity<SelectState<Vec<options::ChoiceRow>>>>,
    hud_texts: HashMap<String, Entity<InputState>>,
    // The copy's own Colour option the option picker edits, with the colour picked so far.
    active_hud_option: Option<(String, Option<[u8; 4]>)>,
    switch_failed: Option<Vec<Change>>,
    switch_stats: (u64, u64),
    // Results that came back, landed or not; a cancelled reload never comes back.
    switch_arrivals: u64,
    // The picker for option colours, apart from the part's, with the option it edits and
    // whether it was open when last seen, so only an open-to-closed change commits.
    option_colour: Entity<ColorPickerState>,
    active_option: Option<&'static str>,
    option_colour_open: bool,
    // The Transparent viewmodels addon as last read, and a count that lets only the latest
    // read or toggle land.
    viewmodels: options::Viewmodels,
    viewmodels_reads: u64,
    // The saved presets as last read, the name field Save as preset reads, and a count that
    // lets only the latest preset job's list land.
    presets: Vec<tf2tools_core::PresetEntry>,
    preset_name: Entity<InputState>,
    preset_reads: u64,
    // History: whether its section shows, the saved HUD's snapshots as last read and the HUD
    // they were read for, the name field, a count that lets only the latest job's list land,
    // and whether a snapshot write runs (Save waits for it).
    history_open: bool,
    // Borrow a part: whether its section shows, the library HUDs and the chosen one's parts.
    borrow: borrow::BorrowSection,
    // Borrows still being worked out, the latest one's number, and the number of the preview
    // on show, so only the latest borrow lands and never over a newer preview.
    borrows_running: usize,
    borrow_latest: u64,
    preview_seq: u64,
    // The picture each part's image was last decoded for by `decode_shown_images`, and a count
    // that lets only the latest decode land.
    image_names: HashMap<PartKey, ShownPicture>,
    image_reads: u64,
    snapshots: Vec<tf2tools_core::SnapshotEntry>,
    snapshots_for: Option<HudId>,
    snapshot_name: Entity<InputState>,
    history_reads: u64,
    history_busy: bool,
    // The snapshot being compared (file, name), where its split sits (0 to 1) and whether the
    // grip is being dragged. The editor holds the laid-out snapshot itself.
    comparing: Option<(String, String)>,
    compare_split: f32,
    compare_drag: bool,
    // What the preview bar says about the latest preview; shown only while the editor holds one.
    preview: recipes::PreviewNote,
    // The part the side panel last showed rows for.
    shown_part: Option<PartKey>,
    // Whether the toolbar's overflow menu is open, and whether it lists the starting points.
    more_open: bool,
    // Whether the toolbar's screen shape menu is open.
    screen_open: bool,
    // The toolbar's zoom menu is open.
    zoom_open: bool,
    // The docked panel opened over the canvas from its rail while the window is too narrow.
    peek: Option<Panel>,
    scene_open: bool,
    board_mode_open: bool,
    // The minimal HUD lasts the session, like the colour-blind view: a newly opened HUD gets it.
    minmode: bool,
    start_open: bool,
    // The Start from card: closed by a pick or Blank for this page, and showing its points.
    start_card: start_card::StartCard,
    // What Save and leave runs once the save it started lands; any other end drops it.
    after_save: Option<AfterSave>,
    // The game situation menu on the HUD scene's strip, with its sliders.
    situation_menu: situation::SituationMenu,
    // Whether the side panel's Add and Attach menus are open.
    add_part_open: bool,
    attach_open: bool,
    // The scrolls of the Attach menu, the named colours and the open font list.
    attach_scroll: ScrollHandle,
    names_scroll: ScrollHandle,
    font_scroll: ScrollHandle,
    // The timeline's playback and its slider.
    timeline: timeline::Timeline,
    // The Animations section's open row and its row editor's fields.
    anims: animations::AnimSection,
    // Whether the six-screen view takes the canvas area, and the checks it and the markers
    // read, made fresh for each landed HUD.
    every_screen: bool,
    checks: checks::Checks,
    // The screenshot behind the HUD, an app setting that outlives HUD changes.
    backdrop: backdrop::BackdropState,
    // The colour-blind view and its simulated images; page state, never saved.
    colour_view: colour_view::ColourView,
    _slider_subscriptions: Vec<Subscription>,
    _subscriptions: Vec<Subscription>,
    // The stored places, until the running load takes them.
    stored_panel: Option<Task<Result<Option<Places>, OpError>>>,
    // Held so dropping the page cancels the wait; a result that lands later finds no page.
    _loading: Task<()>,
}

impl EventEmitter<NavigateTo> for HudMakerPage {}

impl HudMakerPage {
    /// Starts reading the game's files off the UI thread for the TF2 install `require_tf2` gives.
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let tf2 = require_tf2(cx);
        // A missing, damaged or unreadable place falls back to the default.
        let stored_panel = run_blocking(cx, |s| {
            s.db.settings()
                .map(|st| st.hud_maker_panel().and_then(Places::from_setting))
                .map_err(|e| describe_settings(&e).into_op("Load the side panel's place"))
        });
        let job = run_blocking(cx, move |_| {
            let tf_dir = tf2?.tf_dir();
            let failed = |e: HudFilesError| {
                LocalFailure::new(HUD_FILES, e.to_string()).into_op("Load the default HUD")
            };
            let files = HudFiles::open_game(&tf_dir).map_err(failed)?;
            let platform = Platform::current().unwrap_or(Platform::Linux);
            let text = Arc::new(GameText::load(&files, platform));
            let model = HudModel::load(&files, platform).map_err(failed)?;
            let menu_backgrounds = menu_backgrounds(&files, platform);
            let images = decode_images(&files, &model, platform);
            let fonts = read_font_files(&files, &model);
            let files = Arc::new(files);
            Ok(Ok(Opened {
                pristine_files: files.clone(),
                options: None,
                opened_base: None,
                files,
                platform,
                text,
                model,
                images,
                menu_backgrounds,
                changes: Vec::new(),
                made: None,
                copy_tf: None,
                notice: None,
                assets: HudAssets::new(),
                asset_fonts: HashMap::new(),
                fonts,
            }))
        });
        // Enter and leaving a field commit it; a picked colour applies at once.
        let fields = Field::ALL.map(|_| cx.new(|cx| InputState::new(window, cx)));
        let colour = cx.new(|cx| ColorPickerState::new(window, cx));
        let parts_filter = cx.new(|cx| InputState::new(window, cx).placeholder("Find a part..."));
        let mut subscriptions: Vec<Subscription> = fields
            .iter()
            .map(|input| {
                cx.subscribe_in(input, window, |page, _, ev: &InputEvent, window, cx| {
                    if matches!(ev, InputEvent::PressEnter { .. } | InputEvent::Blur) {
                        page.refresh(window, cx);
                    }
                })
            })
            .collect();
        // The list filters from the box's value as it draws, so each edit just redraws.
        subscriptions.push(cx.subscribe(&parts_filter, |_, _, ev: &InputEvent, cx| {
            if matches!(ev, InputEvent::Change) {
                cx.notify();
            }
        }));
        subscriptions.push(cx.subscribe_in(
            &colour,
            window,
            |page, _, ev: &ColorPickerEvent, window, cx| {
                if let ColorPickerEvent::Change(Some(c)) = ev {
                    page.pick_colour(rgb_of(*c), window, cx);
                }
            },
        ));
        // An option colour previews while the picker moves and commits once when it closes.
        let option_colour = cx.new(|cx| ColorPickerState::new(window, cx));
        subscriptions.push(cx.subscribe_in(
            &option_colour,
            window,
            |page, _, ev: &ColorPickerEvent, _, cx| {
                if let ColorPickerEvent::Change(Some(c)) = ev {
                    page.preview_option_colour(rgb_of(*c), cx);
                }
            },
        ));
        // A palette pick or typed hex closes the picker before it emits the colour, and an
        // earlier notification can run this observer first, so the commit waits for the
        // effects already queued, the colour among them.
        subscriptions.push(
            cx.observe_in(&option_colour, window, |page, picker, window, cx| {
                let open = picker.read(cx).is_open();
                let was = std::mem::replace(&mut page.option_colour_open, open);
                if was && !open {
                    cx.defer_in(window, |page, window, cx| {
                        if !page.option_colour_open && page.active_option.take().is_some() {
                            page.edit_selected(window, cx, HudEditor::end_drag);
                        }
                        if !page.option_colour_open
                            && let Some((id, picked)) = page.active_hud_option.take()
                            && let Some(rgba) = picked
                        {
                            page.set_hud_option(&id, OptionValue::Rgba(rgba), window, cx);
                        }
                    });
                }
            }),
        );
        let name = cx.new(|cx| InputState::new(window, cx).placeholder("Name your HUD"));
        let preset_name = cx.new(|cx| InputState::new(window, cx).placeholder("Preset name"));
        let snapshot_name = cx.new(|cx| InputState::new(window, cx).placeholder("Snapshot name"));
        // Any change to the name drops the taken-name line; the next save checks the new one.
        subscriptions.push(cx.observe(&name, |page, input, cx| {
            let typed = input.read(cx).value().trim().to_owned();
            if typed != page.typed_name {
                page.name_error = None;
                page.typed_name = typed;
            }
        }));
        // A closed picker ends the drag, so the next pick is its own undo step.
        subscriptions.push(cx.observe(&colour, |page, picker, cx| {
            if !picker.read(cx).is_open() {
                page.picking = None;
            }
        }));
        // The decoded textures live in each window's atlas until dropped, so leaving frees them.
        subscriptions.push(cx.on_release(|page, cx| {
            if let Load::Ready(loaded) = &mut page.load {
                for (_, image) in loaded.images.drain() {
                    cx.drop_image(image, None);
                }
            }
        }));
        // An addon fetched or removed elsewhere changes what the viewmodels row offers.
        let events = LibraryEvents::global(cx);
        subscriptions.push(cx.subscribe(&events, |page, _, _: &LibraryChanged, cx| {
            page.read_viewmodels(cx)
        }));
        let situation_menu = situation::SituationMenu::new(window, cx);
        let timeline = timeline::Timeline::new(window, cx);
        let anims = animations::AnimSection::new(window, cx);
        let win = window.window_handle();
        let focus = cx.focus_handle();
        focus.focus(window, cx);
        let mut page = HudMakerPage {
            load: Load::Reading,
            area: Rc::new(Cell::new(Bounds::default())),
            text_system: cx.text_system().clone(),
            ui_family: cx.theme().font_family.clone(),
            tints: RefCell::default(),
            parts_scroll: UniformListScrollHandle::new(),
            panel_scroll: ScrollHandle::new(),
            reveal_font_list: false,
            reveal_names: false,
            win,
            panels: Panel::ALL.map(|panel| PanelState::new(Places::DEFAULT.get(panel))),
            toolbar_collapsed: Places::DEFAULT.toolbar_collapsed,
            peek: None,
            toolbar_bounds: Rc::new(Cell::new(Bounds::default())),
            killfeed_bounds: Rc::default(),
            chat_bounds: Rc::default(),
            drawn_bounds: Rc::default(),
            drawn_scene: Cell::new(None),
            cut_names: Rc::default(),
            parts_filter: parts_filter.clone(),
            folded: HashSet::new(),
            filter_names: HashMap::new(),
            list_rows: Rc::from([]),
            text_bounds: Rc::default(),
            text_bounds_for: None,
            settled_for: None,
            parts_spot: None,
            held: None,
            pressed_at: None,
            relayouts: 0,
            toast_lift: None,
            place_saves: Rc::default(),
            hovered: None,
            hover_on_canvas: false,
            last_press: None,
            part_drag: None,
            zoom: None,
            zoom_carry: px(0.),
            space_held: false,
            canvas_pan: None,
            matching_size: false,
            nudge_refused: None,
            focus,
            checked: Default::default(),
            fields,
            shown: Default::default(),
            notes: Default::default(),
            colour,
            names_open: false,
            active_colour: None,
            picking: None,
            font_size_row: None,
            font_open: None,
            families: None,
            settings_open: false,
            setting_key: None,
            name,
            name_error: None,
            saved_hud: None,
            saved_name: String::new(),
            typed_name: String::new(),
            saving: false,
            saving_as_new: false,
            copy_tf: None,
            installing: false,
            live_note: false,
            loads: 0,
            picks: 0,
            menu_reads: 0,
            sliders: HashMap::new(),
            switch_job: None,
            hud_sliders: HashMap::new(),
            hud_selects: HashMap::new(),
            hud_texts: HashMap::new(),
            active_hud_option: None,
            switch_failed: None,
            switch_stats: (0, 0),
            switch_arrivals: 0,
            option_colour,
            active_option: None,
            option_colour_open: false,
            viewmodels: options::Viewmodels::Absent,
            viewmodels_reads: 0,
            presets: Vec::new(),
            preset_name,
            preset_reads: 0,
            history_open: false,
            borrow: borrow::BorrowSection::default(),
            borrows_running: 0,
            borrow_latest: 0,
            preview_seq: 0,
            image_names: HashMap::new(),
            image_reads: 0,
            snapshots: Vec::new(),
            snapshots_for: None,
            snapshot_name,
            history_reads: 0,
            history_busy: false,
            comparing: None,
            compare_split: 0.5,
            compare_drag: false,
            preview: recipes::PreviewNote::default(),
            shown_part: None,
            more_open: false,
            screen_open: false,
            zoom_open: false,
            scene_open: false,
            board_mode_open: false,
            minmode: false,
            situation_menu,
            timeline,
            anims,
            add_part_open: false,
            attach_open: false,
            attach_scroll: ScrollHandle::new(),
            names_scroll: ScrollHandle::new(),
            font_scroll: ScrollHandle::new(),
            every_screen: false,
            checks: checks::Checks::default(),
            backdrop: backdrop::BackdropState::default(),
            colour_view: colour_view::ColourView::default(),
            start_open: false,
            start_card: start_card::StartCard::default(),
            after_save: None,
            _slider_subscriptions: Vec::new(),
            _subscriptions: subscriptions,
            stored_panel: Some(stored_panel),
            _loading: Task::ready(()),
        };
        page.start_load(job, cx);
        page.load_saved_backdrop(cx);
        page
    }

    /// Reopens made HUD `id` in place of the default HUD, with its saved changes counting as
    /// saved; a running load's result is dropped.
    pub fn open(&mut self, id: HudId, cx: &mut Context<Self>) {
        let tf2 = require_tf2(cx);
        let job = run_blocking(cx, move |s| {
            let action = "Failed to open HUD";
            let tf_dir = tf2?.tf_dir();
            let load_failed = |e: HudLoadError| {
                let failure = describe_hud_load(&e);
                failure.log(action);
                failure.message
            };
            // A copy reads its own layers over the game's; any other made HUD the game's.
            let files = match s.library.made_hud_files(&id, &tf_dir) {
                Ok(files) => files,
                Err(HudLoadError::Files(e)) => return Err(OpError::core(action, e)),
                Err(e) => return Ok(Err(load_failed(e))),
            };
            let copy_tf = (s.library.copy_base(&id).ok().flatten()).map(|_| tf_dir.clone());
            let platform = Platform::current().unwrap_or(Platform::Linux);
            let made = match s.library.open_made_hud(&id, &files, platform) {
                Ok(made) => made,
                Err(HudLoadError::Files(e)) => return Err(OpError::core(action, e)),
                Err(e) => return Ok(Err(load_failed(e))),
            };
            // Everything drawn reads the switched files; saves start from the files on disk.
            let shown = made.files();
            let text = Arc::new(GameText::load(shown, platform));
            let menu_backgrounds = menu_backgrounds(shown, platform);
            let images = decode_images(shown, made.pristine(), platform);
            let fonts = read_font_files(shown, made.pristine());
            let options = copy_tf.is_some().then(|| made.options().clone());
            let opened_base = copy_tf.is_some().then(|| made.base().clone());
            let asset_fonts = (made.changes().iter())
                .filter_map(|c| match c.target() {
                    ChangeTarget::Asset(asset) => Some(asset.source()),
                    _ => None,
                })
                .filter_map(|source| {
                    let bytes = made.assets().get(source)?.to_vec();
                    Some((source.to_owned(), FontBytes::new(bytes)))
                })
                .collect();
            // Borrowed pictures past the caps were among the dropped; core's sentence says so.
            let mut notice = reopen_notice(made.dropped(), made.tf2_updated());
            if let (Some(notice), Some(line)) = (&mut notice, made.notice()) {
                notice.text = format!("{} {line}", notice.text);
            }
            let entry = made.entry();
            Ok(Ok(Opened {
                made: Some((entry.id().clone(), entry.name().to_owned())),
                copy_tf,
                notice,
                changes: made.changes().to_vec(),
                assets: made.assets().clone(),
                asset_fonts,
                fonts,
                model: made.pristine().clone(),
                files: Arc::new(shown.clone()),
                pristine_files: Arc::new(files),
                options,
                opened_base,
                platform,
                text,
                images,
                menu_backgrounds,
            }))
        });
        self.load = Load::Reading;
        self.start_load(job, cx);
    }

    // Lands `job` with the stored panel place, replacing any load still running. Missing or
    // unreadable game files are an expected state with its own view; only a crash in the job
    // also raises a toast.
    fn start_load(&mut self, job: LoadJob, cx: &mut Context<Self>) {
        // The live note spoke of the HUD being replaced.
        self.live_note = false;
        let win = self.win;
        // The place is taken once the task runs, so a load replaced before then leaves it.
        self._loading = cx.spawn(async move |page, cx| {
            let stored_panel = page
                .update(cx, |page, _| page.stored_panel.take())
                .ok()
                .flatten();
            let panel = match stored_panel {
                Some(task) => task.await.ok().flatten(),
                None => None,
            };
            let loaded = job.await;
            let landed = cx.update(|cx| {
                win.update(cx, |_, window, cx| {
                    page.update(cx, |page, cx| page.land(panel, loaded, window, cx))
                })
            });
            let notice = landed.ok().and_then(Result::ok).flatten();
            if let Some(notice) = notice {
                cx.update(|cx| {
                    let _ = win.update(cx, |_, window, cx| match notice {
                        Err(err) => toast_error(window, cx, &err),
                        Ok(notice) => {
                            let kind = match notice.warning {
                                true => ToastKind::Warning,
                                false => ToastKind::Info,
                            };
                            toast(window, cx, kind, notice.title, notice.text)
                        }
                    });
                });
            }
        });
    }

    // Puts a finished load in place; returns the reopen notice or a crash to toast.
    fn land(
        &mut self,
        places: Option<Places>,
        loaded: Result<Result<Opened, String>, OpError>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<Result<ReopenNotice, OpError>> {
        cx.notify();
        // The new HUD's list replaces the rows under a still pointer, so no hover-out comes.
        self.hovered = None;
        // Picking named a part of the HUD being replaced, so the next press is an ordinary one.
        self.matching_size = false;
        // The option picker edits an option of the HUD being replaced, so it closes unkept.
        self.close_option_picker(cx);
        if let Some(places) = places {
            for panel in Panel::ALL {
                self.panels[panel as usize].place = places.get(panel);
            }
            self.parts_spot = None;
            self.toolbar_collapsed = places.toolbar_collapsed;
        }
        match loaded {
            Ok(Ok(opened)) => {
                self.loads = self.loads.wrapping_add(1);
                self.copy_tf = opened.copy_tf;
                if let Some((id, name)) = opened.made {
                    self.name
                        .update(cx, |input, cx| input.set_value(&name, window, cx));
                    self.typed_name = name.clone();
                    self.saved_name = name;
                    self.saved_hud = Some(id);
                }
                let mut editor =
                    HudEditor::new(opened.model, opened.changes).with_assets(opened.assets);
                if let Some(options) = opened.options {
                    editor = editor.with_hud_options(options);
                }
                editor.set_minmode(self.minmode);
                (self.switch_job, self.switch_failed) = (None, None);
                let mut asset_fonts = opened.asset_fonts;
                let added = (editor.model().assets().iter())
                    .filter_map(|(_, source)| asset_fonts.remove(source))
                    .collect::<Vec<_>>();
                // Each file is its own call, so one bad file never keeps the others out; a
                // registered file's own families count even where the text system lists none.
                let mut font_names: HashSet<String> = (opened.fonts.into_iter().chain(added))
                    .flat_map(|font| {
                        let families = font.families.clone();
                        if register_font(font, cx) {
                            families
                        } else {
                            Vec::new()
                        }
                    })
                    .collect();
                font_names.extend(cx.text_system().all_font_names());
                let font_names = font_names.iter().map(|n| n.to_lowercase()).collect();
                let loaded = Loaded {
                    files: opened.files,
                    pristine_files: opened.pristine_files,
                    opened_base: opened.opened_base,
                    platform: opened.platform,
                    text: opened.text,
                    editor,
                    images: opened.images,
                    hud_menu_backgrounds: opened.menu_backgrounds.clone(),
                    menu_backgrounds: opened.menu_backgrounds,
                    menu_source: None,
                    menu_generation: 0,
                    menu_art: Default::default(),
                    font_names,
                };
                self.families = None;
                self.checks = checks::Checks::new(loaded.text.clone());
                self.every_screen = false;
                self.history_open = false;
                self.borrow = borrow::BorrowSection::default();
                self.image_names.clear();
                self.comparing = None;
                self.compare_drag = false;
                self.snapshots.clear();
                self.snapshots_for = None;
                // A new HUD's rows measure afresh, so names from the last one do not pile up.
                self.cut_names.borrow_mut().clear();
                // Folds and names belong to the last HUD's parts; the filter starts empty.
                self.folded.clear();
                self.filter_names.clear();
                self.parts_filter
                    .update(cx, |input, cx| input.set_value("", window, cx));
                self.nudge_refused = None;
                self.load = Load::Ready(Box::new(loaded));
                self.build_sliders(window, cx);
                self.simulate_images(cx);
                // A reopened HUD's own pictures, such as borrowed ones, are only in its edits.
                self.decode_shown_images(cx);
                self.read_viewmodels(cx);
                self.read_presets(cx);
                opened.notice.map(Ok)
            }
            Ok(Err(line)) => {
                self.load = Load::CantOpen(line);
                None
            }
            Err(err) => {
                self.load = Load::NoGame;
                match err {
                    OpError::Internal(_) => Some(Err(err)),
                    other => {
                        tracing::warn!("the HUD Maker found no HUD files: {other}");
                        None
                    }
                }
            }
        }
    }

    // A slider per Number option of the loaded HUD, set to its value: a move previews the
    // option and the release keeps it as one undo step.
    fn build_sliders(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.sliders.clear();
        self._slider_subscriptions.clear();
        let specs = self.editor().map(HudEditor::options).unwrap_or_default();
        for spec in specs {
            let OptionKind::Number { min, max, step } = *spec.kind() else {
                continue;
            };
            let id = spec.id();
            let state = cx.new(|_| {
                SliderState::new()
                    .min(min as f32)
                    .max(max as f32)
                    .step(step as f32)
            });
            self._slider_subscriptions.push(cx.subscribe_in(
                &state,
                window,
                move |page, _, ev: &SliderEvent, window, cx| match ev {
                    SliderEvent::Change(v) => page.slide_option(id, v.start(), cx),
                    SliderEvent::Release(_) => page.edit_selected(window, cx, HudEditor::end_drag),
                },
            ));
            self.sliders.insert(id, state);
        }
        self.build_hud_controls(window, cx);
        self.sync_sliders(window, cx);
    }

    // A slider per Number option and a menu per long Choice option of the copy's own; only a
    // release or a pick records a value, since each value rebuilds the HUD's files.
    fn build_hud_controls(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.hud_sliders.clear();
        self.hud_selects.clear();
        self.hud_texts.clear();
        let Some(options) = self.editor().and_then(|e| e.hud_options()).cloned() else {
            return;
        };
        for spec in options.specs() {
            let id = spec.id().to_owned();
            match spec.kind() {
                &OptionKind::Number { min, max, step } => {
                    let state = cx.new(|_| {
                        SliderState::new()
                            .min(min as f32)
                            .max(max as f32)
                            .step(step as f32)
                    });
                    let option = id.clone();
                    self._slider_subscriptions.push(cx.subscribe_in(
                        &state,
                        window,
                        move |page, _, ev: &SliderEvent, window, cx| {
                            if let SliderEvent::Release(v) = ev {
                                let value = OptionValue::Int(v.start().round() as i32);
                                page.set_hud_option(&option, value, window, cx);
                            }
                        },
                    ));
                    self.hud_sliders.insert(id, state);
                }
                OptionKind::Choice(choices) if choices.len() > options::MAX_CHOICE_BUTTONS => {
                    let rows: Vec<_> = (choices.iter())
                        .map(|c| options::ChoiceRow::of(&id, c))
                        .collect();
                    let state = cx.new(|cx| SelectState::new(rows, None, window, cx));
                    let option = id.clone();
                    self._slider_subscriptions.push(cx.subscribe_in(
                        &state,
                        window,
                        move |page, _, ev: &SelectEvent<Vec<options::ChoiceRow>>, window, cx| {
                            let SelectEvent::Confirm(Some(key)) = ev else {
                                return;
                            };
                            let value = OptionValue::Pick(key.to_string());
                            page.set_hud_option(&option, value, window, cx);
                        },
                    ));
                    self.hud_selects.insert(id, state);
                }
                // Typed text is recorded on Enter or when the box loses focus, not per key.
                OptionKind::Text => {
                    let state = cx.new(|cx| InputState::new(window, cx));
                    let option = id.clone();
                    self._slider_subscriptions.push(cx.subscribe_in(
                        &state,
                        window,
                        move |page, input: &Entity<InputState>, ev: &InputEvent, window, cx| {
                            if matches!(ev, InputEvent::PressEnter { .. } | InputEvent::Blur) {
                                let text = input.read(cx).value().to_string();
                                let value = OptionValue::Text(text.trim().to_owned());
                                page.set_hud_option(&option, value, window, cx);
                            }
                        },
                    ));
                    self.hud_texts.insert(id, state);
                }
                _ => {}
            }
        }
    }

    /// The slider of the copy's own Number option `id`, if it offers one.
    pub fn hud_slider_state(&self, id: &str) -> Option<Entity<SliderState>> {
        self.hud_sliders.get(id).cloned()
    }

    /// The text box of the copy's own Text option `id`.
    fn hud_text(&self, id: &str) -> Option<Entity<InputState>> {
        self.hud_texts.get(id).cloned()
    }

    /// The menu of the copy's own Choice option `id` when it has too many choices for buttons.
    fn hud_select(&self, id: &str) -> Option<Entity<SelectState<Vec<options::ChoiceRow>>>> {
        self.hud_selects.get(id).cloned()
    }

    // Previews a slider's value; the value already shown, which the slider repeats on every
    // pointer move, skips the replay, and a value core refuses changes nothing.
    fn slide_option(&mut self, id: &'static str, at: f32, cx: &mut Context<Self>) {
        let value = OptionValue::Int(at.round() as i32);
        if let Some(editor) = self.editor_mut()
            && editor.option_value(id).as_ref() != Some(&value)
            && let Err(err) = editor.preview_option(id, value)
        {
            tracing::debug!("the HUD Maker refused option {id} at {at}: {err:?}");
        }
        cx.notify();
    }

    // Moves each slider to the value shown, so undo, redo, resets and previews never leave
    // one stale.
    fn sync_sliders(&self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(editor) = self.editor() else {
            return;
        };
        for (id, slider) in &self.sliders {
            if let Some(OptionValue::Int(v)) = editor.shown_option_value(id) {
                let at = v as f32;
                if slider.read(cx).value() != at.into() {
                    slider.update(cx, |s, cx| s.set_value(at, window, cx));
                }
            }
        }
        for (id, slider) in &self.hud_sliders {
            if let Some(OptionValue::Int(v)) = editor.hud_option_value(id) {
                let at = v as f32;
                if slider.read(cx).value() != at.into() {
                    slider.update(cx, |s, cx| s.set_value(at, window, cx));
                }
            }
        }
        for (id, input) in &self.hud_texts {
            if let Some(OptionValue::Text(text)) = editor.hud_option_value(id)
                && input.read(cx).value().as_ref() != text.as_str()
            {
                input.update(cx, |s, cx| s.set_value(&text, window, cx));
            }
        }
        for (id, select) in &self.hud_selects {
            if let Some(OptionValue::Pick(key)) = editor.hud_option_value(id) {
                let key = gpui_kit::SharedString::from(key);
                if select.read(cx).selected_value() != Some(&key) {
                    select.update(cx, |s, cx| s.set_selected_value(&key, window, cx));
                }
            }
        }
    }

    // Moves an open option picker to its option's colour in force, as sync_sliders does.
    fn sync_option_picker(&self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(id) = self.active_option else {
            return;
        };
        if let Some(OptionValue::Rgba(rgba)) = self.editor().and_then(|e| e.option_value(id)) {
            let at = hsla_of_rgba(rgba);
            if self.option_colour.read(cx).displayed_color() != Some(at) {
                self.option_colour
                    .update(cx, |st, cx| st.set_value(at, window, cx));
            }
        }
    }

    // Closes the option picker and forgets its option; the caller settles any colour drag.
    fn close_option_picker(&mut self, cx: &mut Context<Self>) {
        let hud = self.active_hud_option.take().is_some();
        if self.active_option.take().is_some() || hud {
            self.option_colour
                .update(cx, |st, cx| st.set_open(false, cx));
        }
    }

    /// The picker the Colour options share.
    pub fn option_colour_state(&self) -> &Entity<ColorPickerState> {
        &self.option_colour
    }

    /// The Colour option the option picker edits, if one is open.
    pub(super) fn active_option(&self) -> Option<&'static str> {
        self.active_option
    }

    // A swatch press keeps the running colour as one step, then opens the picker on `id`.
    pub(super) fn choose_option_colour(
        &mut self,
        id: &'static str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.keep_picked_colour(window, cx);
        let Some(OptionValue::Rgba(rgba)) = self.editor().and_then(|e| e.option_value(id)) else {
            cx.notify();
            return;
        };
        self.active_option = Some(id);
        self.option_colour.update(cx, |st, cx| {
            st.set_value(hsla_of_rgba(rgba), window, cx);
            st.set_open(true, cx);
        });
        cx.notify();
    }

    // Before a swatch takes the picker over, keeps what its last option picked as one step: a
    // built-in's running preview, or the copy's own colour waiting for the picker to close.
    fn keep_picked_colour(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(editor) = self.editor_mut() {
            editor.end_drag();
        }
        self.active_option = None;
        if let Some((id, Some(rgba))) = self.active_hud_option.take() {
            self.set_hud_option(&id, OptionValue::Rgba(rgba), window, cx);
        }
    }

    /// The copy's own Colour option the option picker edits, if one is open.
    pub(super) fn active_hud_option(&self) -> Option<&str> {
        self.active_hud_option.as_ref().map(|(id, _)| id.as_str())
    }

    // A swatch press opens the option picker on the copy's own Colour option `id`; the colour
    // picked is recorded as one step when the picker closes.
    pub(super) fn choose_hud_option_colour(
        &mut self,
        id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.keep_picked_colour(window, cx);
        let Some(OptionValue::Rgba(rgba)) = self.editor().and_then(|e| e.hud_option_value(id))
        else {
            cx.notify();
            return;
        };
        self.active_hud_option = Some((id.to_owned(), None));
        self.option_colour.update(cx, |st, cx| {
            st.set_value(hsla_of_rgba(rgba), window, cx);
            st.set_open(true, cx);
        });
        cx.notify();
    }

    // Previews the active option's colour; a colour core refuses changes nothing. A copy's
    // own colour is only noted, keeping its alpha, and recorded when the picker closes.
    fn preview_option_colour(&mut self, [r, g, b]: [u8; 3], cx: &mut Context<Self>) {
        if let Some((id, picked)) = &self.active_hud_option {
            let alpha = match picked {
                Some([.., a]) => *a,
                None => match self.editor().and_then(|e| e.hud_option_value(id)) {
                    Some(OptionValue::Rgba([.., a])) => a,
                    _ => 255,
                },
            };
            let id = id.clone();
            self.active_hud_option = Some((id, Some([r, g, b, alpha])));
            return;
        }
        let Some(id) = self.active_option else {
            return;
        };
        if let Some(editor) = self.editor_mut()
            && let Err(err) = editor.preview_option(id, OptionValue::Rgba([r, g, b, 255]))
        {
            tracing::debug!("the HUD Maker refused colour option {id}: {err:?}");
        }
        cx.notify();
    }

    /// The slider of Number option `id`; `None` when the loaded HUD does not offer it.
    pub fn option_slider(&self, id: &str) -> Option<Entity<SliderState>> {
        self.sliders.get(id).cloned()
    }

    /// True once the load finished, with or without a HUD.
    pub fn is_loaded(&self) -> bool {
        !matches!(self.load, Load::Reading)
    }

    /// The editor over the loaded HUD; `None` while loading or without game files.
    pub fn editor(&self) -> Option<&HudEditor> {
        match &self.load {
            Load::Ready(loaded) => Some(&loaded.editor),
            _ => None,
        }
    }

    /// Decodes off the UI thread the pictures the shown model's parts name that are not yet
    /// decoded as that picture, the editor's added files (such as picked pictures) first.
    /// A result that lands after another HUD loaded or a later decode started is dropped.
    pub(super) fn decode_shown_images(&mut self, cx: &mut Context<Self>) {
        // Every edit, undo, redo, restore and preview path comes through here.
        self.refresh_menu_backgrounds(cx);
        let Load::Ready(loaded) = &self.load else {
            return;
        };
        let editor = &loaded.editor;
        let (model, pristine) = (editor.model(), editor.pristine());
        // A picture is its name and the picked material behind it, since a pick for a picture
        // the game's code sets keeps the name.
        let shown_of = |model: &HudModel, key: &PartKey| {
            let image = scene_image(model, model.part(key)?, canvas::LOCAL_TEAM)?.trim();
            (!image.is_empty()).then(|| ShownPicture::of(model, image))
        };
        // A part is wanted when its picture differs from the one its image was decoded for,
        // or, once, when it has none yet.
        let wanted: Vec<(PartKey, ShownPicture)> = (model.parts().iter())
            .filter_map(|part| {
                let key = part.key();
                let shown = shown_of(model, key)?;
                let had = match self.image_names.get(key) {
                    Some(had) => Some(had.clone()),
                    None if !loaded.images.contains_key(key) => None,
                    None => shown_of(pristine, key),
                };
                (had.as_ref() != Some(&shown)).then(|| (key.clone(), shown))
            })
            .collect();
        if wanted.is_empty() {
            return;
        }
        let top = top_layer(editor, |path| path.starts_with("materials/"));
        let mut shown: HashMap<PartKey, ShownPicture> = HashMap::new();
        let wanted = (wanted.into_iter())
            .map(|(key, picture)| {
                let name = picture.name.clone();
                shown.insert(key.clone(), picture);
                (key, name)
            })
            .collect();
        let (files, platform) = (loaded.files.clone(), loaded.platform);
        self.image_reads = self.image_reads.wrapping_add(1);
        let (reads, load) = (self.image_reads, self.loads);
        let job = run_blocking(cx, move |_| {
            Ok(decode_layered(top, &files, wanted, platform))
        });
        cx.spawn(async move |page, cx| {
            let Ok(decoded) = job.await else {
                return;
            };
            let _ = page.update(cx, |page, cx| {
                if page.loads != load || page.image_reads != reads {
                    return;
                }
                let Load::Ready(loaded) = &mut page.load else {
                    return;
                };
                let (mut stale, mut changed) = (Vec::new(), false);
                for (key, _, image) in decoded {
                    changed |= image.is_some();
                    let old = match image {
                        Some(image) => loaded.images.insert(key.clone(), image),
                        None => loaded.images.remove(&key),
                    };
                    changed |= old.is_some();
                    stale.extend(old);
                    if let Some(picture) = shown.remove(&key) {
                        page.image_names.insert(key, picture);
                    }
                }
                // A replaced texture leaves the atlas unless another part still draws it.
                let images = &loaded.images;
                for old in stale {
                    if !images.values().any(|i| Arc::ptr_eq(i, &old)) {
                        cx.drop_image(old, None);
                    }
                }
                // A decode that found nothing new leaves the colour-blind sets as they are.
                if changed {
                    page.simulate_images(cx);
                    cx.notify();
                }
            });
        })
        .detach();
    }

    /// The editor, for a caller that edits and then notifies.
    pub fn editor_mut(&mut self) -> Option<&mut HudEditor> {
        match &mut self.load {
            Load::Ready(loaded) => Some(&mut loaded.editor),
            _ => None,
        }
    }

    /// The family a label whose scheme font names `family` draws in: the family itself when
    /// installed or added, else its first installed stand-in; `None` (the app's own font) when
    /// neither is, or before the HUD loads.
    pub fn drawn_family(&self, family: &str) -> Option<String> {
        match &self.load {
            Load::Ready(loaded) => {
                canvas::drawn_family(family, &loaded.font_names).map(|f| f.to_string())
            }
            _ => None,
        }
    }

    /// Whether the text system knows the font family `family`, ignoring case.
    pub fn knows_font(&self, family: &str) -> bool {
        match &self.load {
            Load::Ready(loaded) => loaded.font_names.contains(&family.to_lowercase()),
            _ => false,
        }
    }

    // Picks a font file for Font row `key` of the selection and reads it off the UI thread; it
    // lands only while that HUD and part are still selected, never on ones chosen since.
    pub(super) fn add_font_file(&mut self, key: &'static str, cx: &mut Context<Self>) {
        self.flush(cx);
        let Some(part) = self.editor().and_then(|e| e.selected().cloned()) else {
            return;
        };
        let load = self.loads;
        let picking = cx.global::<Services>().picker.pick(PickKind::FontFile, cx);
        let win = self.win;
        cx.spawn(async move |page, cx| {
            let Some(path) = picking.await else {
                return;
            };
            let reading = cx.update(|cx| {
                run_blocking(cx, move |_| {
                    Ok(read_font_file(&path).map(|(name, bytes)| (name, FontBytes::new(bytes))))
                })
            });
            let read = reading.await.ok().flatten();
            cx.update(|cx| {
                let _ = win.update(cx, |_, window, cx| {
                    page.update(cx, |page, cx| {
                        if page.loads == load {
                            page.land_font(&part, key, read, window, cx);
                        }
                    })
                });
            });
        })
        .detach();
    }

    // Adds a read font file to the selection as one undo step and registers it for drawing; an
    // unreadable file, or one core refuses, toasts whose fault it was and changes nothing.
    fn land_font(
        &mut self,
        part: &PartKey,
        key: &str,
        read: Option<(String, FontBytes)>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.editor().and_then(|e| e.selected()) != Some(part) {
            return;
        }
        let Some((name, font)) = read else {
            toast(window, cx, ToastKind::Error, NOT_A_FONT, "");
            return;
        };
        let mut added = None;
        self.edit_selected(window, cx, |e| {
            added = Some(e.add_font_file_selected(key, &name, font.bytes.clone()));
        });
        match added {
            Some(Ok(_)) => {
                let families = font.families.clone();
                if register_font(font, cx)
                    && let Load::Ready(loaded) = &mut self.load
                {
                    (loaded.font_names).extend(families.iter().map(|f| f.to_lowercase()));
                }
            }
            Some(Err(refused)) => {
                let message = match refused {
                    FontAddRefused::NotAFont => NOT_A_FONT,
                    FontAddRefused::BadName => FONT_BAD_NAME,
                    FontAddRefused::HudFull => FONT_HUD_FULL,
                    FontAddRefused::PartFont => FONT_PART,
                };
                toast(window, cx, ToastKind::Error, message, "");
            }
            None => {}
        }
    }

    /// The editor's scene; `None` until a HUD is loaded.
    pub fn scene(&self) -> Option<Scene> {
        self.editor().map(HudEditor::scene)
    }

    /// The scoreboard mode the editor shows; `None` until a HUD is loaded.
    pub fn board_mode(&self) -> Option<BoardMode> {
        self.editor().map(HudEditor::board_mode)
    }

    // Adds a `kind` part near the selection and selects it; a refusal says so plainly.
    fn add_part(
        &mut self,
        kind: tf2tools_core::NewPart,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let mut refused = None;
        self.edit_selected(window, cx, |e| refused = e.add_part_selected(kind).err());
        if let Some(err) = refused {
            tracing::debug!("the HUD Maker refused adding {kind:?}: {err:?}");
            toast(window, cx, ToastKind::Error, ADD_REFUSED, "");
        }
    }

    // Attaches the selection's `own` corner to sibling `to`'s `corner` as one undo step.
    fn attach(
        &mut self,
        to: PartKey,
        own: tf2tools_core::PinCorner,
        corner: tf2tools_core::PinCorner,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.edit_selected(window, cx, |e| {
            if let Err(err) = e.pin_selected(&to, own, corner) {
                tracing::debug!("the HUD Maker refused an attach: {err:?}");
            }
        });
    }

    // Deletes the selection, a part the player added; one other parts attach to stays.
    fn delete_part(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let mut refused = None;
        self.edit_selected(window, cx, |e| refused = e.delete_selected().err());
        match refused {
            Some(tf2tools_core::RemoveRefused::Attached) => {
                toast(window, cx, ToastKind::Error, DETACH_FIRST, "")
            }
            Some(err) => tracing::debug!("the HUD Maker refused a delete: {err:?}"),
            None => {}
        }
    }

    // Turns the selection's hit flash on or off; an event the game commands lock says so.
    fn set_hit_flash(&mut self, on: bool, window: &mut Window, cx: &mut Context<Self>) {
        let mut refused = None;
        self.edit_selected(window, cx, |e| refused = e.set_hit_flash_selected(on).err());
        match refused {
            Some(EditRefused::RunsGameCommands) => toast(
                window,
                cx,
                ToastKind::Error,
                animations::RUNS_GAME_COMMANDS,
                "",
            ),
            Some(err) => tracing::debug!("the HUD Maker refused the hit flash: {err:?}"),
            None => {}
        }
    }

    /// Commits typed values, switches the scoreboard's mode and clears the hover outline and
    /// the picker run. The current mode is a no-op.
    pub fn set_board_mode(&mut self, mode: BoardMode, window: &mut Window, cx: &mut Context<Self>) {
        if self.board_mode().is_none_or(|now| now == mode) {
            return;
        }
        self.flush(cx);
        self.hovered = None;
        self.picking = None;
        self.part_drag = None;
        self.matching_size = false;
        if let Some(editor) = self.editor_mut() {
            editor.set_board_mode(mode);
        }
        self.decode_shown_images(cx);
        self.refresh(window, cx);
    }

    /// Whether the maker shows TF2's minimal HUD; it lasts the session.
    pub fn minmode(&self) -> bool {
        self.minmode
    }

    /// Commits typed values, shows the HUD with the minimal HUD `on` or off and clears the
    /// hover outline and the picker run. The current setting is a no-op.
    pub fn set_minmode(&mut self, on: bool, window: &mut Window, cx: &mut Context<Self>) {
        if self.minmode == on {
            return;
        }
        self.minmode = on;
        self.flush(cx);
        self.hovered = None;
        self.picking = None;
        self.part_drag = None;
        self.matching_size = false;
        if let Some(editor) = self.editor_mut() {
            editor.set_minmode(on);
        }
        self.decode_shown_images(cx);
        self.refresh(window, cx);
    }

    /// The weapon meter the Item meter shows; Plain before a HUD loads.
    pub fn meter(&self) -> MeterKind {
        self.editor().map(HudEditor::meter).unwrap_or_default()
    }

    /// Commits typed values, shows the Item meter from `kind`'s file and clears the hover
    /// outline and the picker run; `show` also turns the meter switch on.
    pub fn set_meter(
        &mut self,
        kind: MeterKind,
        show: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.flush(cx);
        self.hovered = None;
        self.picking = None;
        self.part_drag = None;
        self.matching_size = false;
        if let Some(editor) = self.editor_mut() {
            editor.set_meter(kind, show);
        }
        // A new situation stops playback, as the menu's other picks do.
        if show {
            self.stop_playing();
        }
        self.decode_shown_images(cx);
        self.refresh(window, cx);
    }

    /// Commits typed values, switches the editor's scene and clears the hover outline and
    /// the picker run. The current scene is a no-op.
    pub fn set_scene(&mut self, scene: Scene, window: &mut Window, cx: &mut Context<Self>) {
        if self.scene().is_none_or(|now| now == scene) {
            return;
        }
        self.flush(cx);
        // The other scene's rows replace the list under a still pointer, so no hover-out comes.
        self.hovered = None;
        self.picking = None;
        self.part_drag = None;
        self.matching_size = false;
        if let Some(editor) = self.editor_mut() {
            editor.set_scene(scene);
        }
        self.decode_shown_images(cx);
        self.refresh(window, cx);
    }

    /// The part under the pointer, on the canvas or its list row, outlined on the canvas.
    pub fn hovered(&self) -> Option<&PartKey> {
        self.hovered.as_ref()
    }

    /// Whether Match size waits for a press on the part whose size to copy.
    pub fn matching_size(&self) -> bool {
        self.matching_size
    }

    /// Whether picker changes still join the last colour change as one undo step.
    pub fn is_picking(&self) -> bool {
        self.picking.is_some()
    }

    /// The texture drawn for image part `part`, through the colour-blind view once its
    /// simulated set has landed.
    pub fn image(&self, part: &PartKey) -> Option<Arc<RenderImage>> {
        match &self.load {
            Load::Ready(loaded) => (self.seen_images().unwrap_or(&loaded.images))
                .get(part)
                .cloned(),
            _ => None,
        }
    }

    /// The texture the canvas draws for image part `part`: [`Self::image`] tinted by the colour
    /// the game draws it in at the situation shown.
    pub fn drawn_image(&self, part: &PartKey) -> Option<Arc<RenderImage>> {
        let image = self.image(part)?;
        let Load::Ready(loaded) = &self.load else {
            return None;
        };
        let model = loaded.editor.model();
        let colour = model.draw_colour(model.part(part)?, loaded.editor.situation());
        Some(tint::drawn(&self.tints, &image, colour, self.vision()))
    }

    /// The game's picture behind the main menu at the editor's screen shape, through the
    /// colour-blind view; `None` when the HUD names none or it does not decode.
    pub fn menu_background(&self) -> Option<Arc<RenderImage>> {
        let Load::Ready(loaded) = &self.load else {
            return None;
        };
        let wide = loaded.editor.screen().is_wider();
        let key = PartKey::new(loaded.menu_backgrounds[usize::from(wide)].as_ref()?);
        // Until a colour-blind view's copy lands, the plain picture stands in for it.
        self.image(&key)
            .or_else(|| loaded.images.get(&key).cloned())
    }

    /// The side panel's number field `field`.
    /// The box that narrows the part list by name.
    pub fn parts_filter(&self) -> &Entity<InputState> {
        &self.parts_filter
    }

    pub fn field(&self, field: Field) -> &Entity<InputState> {
        &self.fields[field as usize]
    }

    // The note under `field` while the selection is the part it was typed for.
    pub(super) fn field_note(&self, field: Field) -> Option<&FieldNote> {
        let note = self.notes[field as usize].as_ref()?;
        let selected = self.editor()?.selected()?;
        (*selected == note.part).then_some(note)
    }

    /// The Colour row's picker.
    pub fn colour_state(&self) -> &Entity<ColorPickerState> {
        &self.colour
    }

    /// Where the side panel sits by the player's choice, before any move off the selection.
    pub fn panel_place(&self) -> PanelPlace {
        self.place_of(Panel::Options)
    }

    /// Places the side panel at `place` and remembers it off the UI thread.
    pub fn set_panel_place(&mut self, place: PanelPlace, cx: &mut Context<Self>) {
        self.set_place_of(Panel::Options, place, cx)
    }

    /// Where `panel` sits by the player's choice, before any move off the selection.
    pub fn place_of(&self, panel: Panel) -> PanelPlace {
        self.panels[panel as usize].place
    }

    /// Places `panel` at `place` and saves every place (`Places::to_setting`) off the UI thread.
    pub fn set_place_of(&mut self, panel: Panel, place: PanelPlace, cx: &mut Context<Self>) {
        if panel == Panel::Parts && self.panels[panel as usize].place != place {
            self.parts_spot = None;
        }
        self.panels[panel as usize].place = place;
        // The player placed it, so nothing stays held or opened from a rail.
        self.held = None;
        self.peek = None;
        // Where the two floating panels can't stand side by side, opening one closes the other.
        let opens = self.mode_shown(panel) == PanelMode::Floating && !place.collapsed;
        let other = panel.other();
        if opens && self.too_narrow_for_both() && self.mode_shown(other) == PanelMode::Floating {
            self.panels[other as usize].place.collapsed = true;
        }
        // A collapsing list takes its rows away under a still pointer, so no hover-out comes.
        if self.place_of(Panel::Parts).collapsed {
            self.hovered = None;
        }
        self.keep_clear();
        cx.notify();
        self.save_places(cx);
    }

    // Saves every place through one writer: each write takes the newest text, so writes never
    // land out of order and a burst of changes costs at most one write past the running one.
    fn save_places(&mut self, cx: &mut Context<Self>) {
        let mut saves = self.place_saves.borrow_mut();
        saves.latest = Some(self.places().to_setting());
        if std::mem::replace(&mut saves.writing, true) {
            return;
        }
        drop(saves);
        let saves = self.place_saves.clone();
        let win = self.win;
        cx.spawn(async move |_, cx| {
            loop {
                let next = {
                    let mut saves = saves.borrow_mut();
                    let next = saves.latest.take();
                    saves.writing = next.is_some();
                    next
                };
                let Some(text) = next else {
                    break;
                };
                let save = cx.update(|cx| {
                    run_blocking(cx, move |s| {
                        s.db.set_hud_maker_panel(&text).map_err(|e| {
                            describe_settings(&e).into_op("Save the side panel's place")
                        })
                    })
                });
                if let Err(err) = save.await {
                    cx.update(|cx| {
                        let _ = win.update(cx, |_, window, cx| toast_error(window, cx, &err));
                    });
                }
            }
        })
        .detach();
    }

    /// True while the toolbar shows only its one button.
    pub fn toolbar_collapsed(&self) -> bool {
        self.toolbar_collapsed
    }

    /// Collapses or opens the toolbar and saves every place.
    pub fn set_toolbar_collapsed(&mut self, collapsed: bool, cx: &mut Context<Self>) {
        self.toolbar_collapsed = collapsed;
        self.keep_clear();
        cx.notify();
        self.save_places(cx);
    }

    /// The window bounds the drawn killfeed lines cover together, last frame; `None` when no
    /// line draws.
    pub(super) fn killfeed_band(&self) -> Option<Bounds<Pixels>> {
        band(&self.killfeed_bounds.borrow())
    }

    /// The window bounds the drawn chat lines cover together, last frame; `None` when no line
    /// draws.
    pub fn chat_band(&self) -> Option<Bounds<Pixels>> {
        band(&self.chat_bounds.borrow())
    }

    // The toolbar's own place in area coordinates, 8 px above the area's bottom with last
    // frame's size: in the screen's bottom-right letterbox corner when `Seat::Corner`, else
    // centred under the screen.
    fn toolbar_home(&self) -> Point<Pixels> {
        let area = self.area.get();
        let bar = self.toolbar_bounds.get().size;
        let columns = self.columns_area();
        let x = match self.toolbar_seat(columns) {
            Seat::Corner => columns.right() - area.origin.x - TOOLBAR_EDGE - bar.width,
            Seat::Below | Seat::Band => {
                self.screen_area().center().x - area.origin.x - bar.width / 2.
            }
        };
        let y = area.size.height - TOOLBAR_EDGE - bar.height;
        point(
            x.min(area.size.width - bar.width).max(px(0.)),
            y.max(px(0.)),
        )
    }

    // Where the toolbar is drawn: always its home, since the screen keeps clear of it.
    fn toolbar_shown(&self) -> Point<Pixels> {
        self.toolbar_home()
    }

    // How the toolbar keeps off the screen fitted to `columns`: in the letterbox below it, in
    // the side letterbox's bottom corner when shut, or in a band the screen gives up.
    fn toolbar_seat(&self, columns: Bounds<Pixels>) -> Seat {
        let Some(screen) = self.editor().map(|e| e.screen()) else {
            return Seat::Band;
        };
        let bar = self.toolbar_bounds.get().size;
        let fit = CanvasFit::new(columns, screen).screen_bounds();
        if columns.bottom() - fit.bottom() >= bar.height + TOOLBAR_EDGE + TOOLBAR_SPACE {
            Seat::Below
        } else if self.toolbar_collapsed
            && columns.right() - fit.right() >= bar.width + TOOLBAR_EDGE * 2.
        {
            Seat::Corner
        } else {
            Seat::Band
        }
    }

    // The tallest a floating panel grows: the area less a 16 px top margin and the band the
    // toolbar takes at the bottom, so a panel always has room clear of the toolbar.
    fn floating_height(&self) -> Pixels {
        let toolbar = self.toolbar_bounds.get().size.height + TOOLBAR_GAP + TOAST_GAP;
        (self.area.get().size.height - toolbar - NARROW_MARGIN).max(px(0.))
    }

    // True when the area is narrower than both floating panels side by side with a 16 px
    // margin at each side and between them.
    fn too_narrow_for_both(&self) -> bool {
        let both = Panel::Options.width() + Panel::Parts.width() + NARROW_MARGIN * 3.;
        self.area.get().size.width < both
    }

    // Every place as the one stored setting holds them.
    fn places(&self) -> Places {
        let mut places = Places {
            toolbar_collapsed: self.toolbar_collapsed,
            ..Places::DEFAULT
        };
        for panel in Panel::ALL {
            places.set(panel, self.place_of(panel));
        }
        places
    }

    // Where `panel` is drawn: the player's place, moved off the selection and perhaps folded to
    // its header, kept inside the area.
    fn panel_shown(&self, panel: Panel) -> PanelPlace {
        let state = &self.panels[panel as usize];
        let folded = state.folded && !state.held_open;
        let place = match state.off {
            Some(off) => {
                let size = if folded {
                    self.header_size(panel)
                } else {
                    self.size_at(panel, off.y)
                };
                PanelPlace {
                    x: f32::from(off.x),
                    y: f32::from(off.y),
                    ..state.place
                }
                .clamp(self.area.get().size, size)
            }
            None => self.home(panel),
        };
        let collapsed = place.collapsed && !self.peeking(panel);
        PanelPlace {
            mode: self.mode_shown(panel),
            collapsed: collapsed || folded || self.is_railed(panel),
            ..place
        }
    }

    // The panel's size as the player left it, before any height cap: open, the size it last
    // had open plus what its body scrolled; collapsed, its header as last measured.
    fn stored_size(&self, panel: Panel) -> Size<Pixels> {
        let state = &self.panels[panel as usize];
        if state.place.collapsed {
            return state.bounds.get().size;
        }
        // The open list's height is computed, not measured, so a capped frame never reads back
        // as its natural height; until the title is measured the last open size stands in.
        if panel == Panel::Parts && state.title_height.get() > px(0.) {
            let list = px(parts::LIST_HEIGHT + parts::LIST_PADDING);
            return size(panel.width(), self.header_size(panel).height + list);
        }
        let open = state.open_size.get();
        if open.height <= px(0.) {
            return state.bounds.get().size;
        }
        let hidden = match panel {
            Panel::Options => self.panel_scroll.max_offset().y.max(px(0.)),
            Panel::Parts => px(0.),
        };
        size(open.width, open.height + hidden)
    }

    // How a floating panel's height follows its top, for both settle and drawing; each panel
    // keeps its bottom within its own share of the area.
    pub(super) fn height_cap(&self, panel: Panel) -> Option<view::HeightCap> {
        // Settling plans with the base cap; a panel it cut shorter draws at that height.
        let cap = self.base_cap(panel)?;
        Some(match self.panels[panel as usize].short {
            Some(short) => view::HeightCap {
                max: cap.max.min(short),
                min: cap.min.min(short),
                ..cap
            },
            None => cap,
        })
    }

    // The height cap before settling cuts a panel short.
    fn base_cap(&self, panel: Panel) -> Option<view::HeightCap> {
        let share = match panel {
            Panel::Options => panel::SIDE_PANEL_SHARE,
            Panel::Parts => PARTS_PANEL_SHARE,
        };
        let floating = self.floating_height();
        let max = match panel {
            Panel::Options => floating,
            Panel::Parts => self.parts_cover_max().map_or(floating, |c| c.min(floating)),
        };
        Some(view::HeightCap {
            share,
            min: px(panel::SIDE_PANEL_MIN),
            max,
        })
    }

    // The open part list's tallest: a quarter of the fitted screen's area at its width, never
    // shorter than its header and three rows.
    fn parts_cover_max(&self) -> Option<Pixels> {
        let editor = self.editor()?;
        let screen = CanvasFit::new(self.screen_area(), editor.screen()).screen_bounds();
        let cover = f32::from(screen.size.width) * f32::from(screen.size.height) * PARTS_COVER_MAX
            / f32::from(Panel::Parts.width());
        let rows = px(parts::ROW_HEIGHT * PARTS_MIN_ROWS + parts::LIST_PADDING);
        Some(px(cover).max(self.header_size(Panel::Parts).height + rows))
    }

    // The open panel's size with its top `y` below the area's top.
    fn size_at(&self, panel: Panel, y: Pixels) -> Size<Pixels> {
        let natural = self.stored_size(panel);
        match self.height_cap(panel) {
            Some(cap) if !self.place_of(panel).collapsed => size(
                natural.width,
                natural.height.min(cap.at(self.area.get().size.height, y)),
            ),
            _ => natural,
        }
    }

    // The panel collapsed to its title bar: the bar and its bottom border inside the frame's
    // 1 px border, at the panel's width.
    fn header_size(&self, panel: Panel) -> Size<Pixels> {
        let title = self.panels[panel as usize].title_height.get();
        size(panel.width(), title + px(3.))
    }

    /// How many frames found the area, a panel or the toolbar moved and settled the layout again;
    /// it stops growing once the layout is at rest.
    pub fn relayouts(&self) -> u64 {
        self.relayouts
    }

    /// True while `panel` shows only its header because no open spot clears the selection.
    pub fn is_folded(&self, panel: Panel) -> bool {
        let state = &self.panels[panel as usize];
        state.folded && !state.held_open
    }

    // The collapse button on `panel`: expanding a folded panel holds it open until the
    // selection changes without touching the stored place; otherwise it flips the stored place.
    pub(super) fn toggle_collapsed(&mut self, panel: Panel, cx: &mut Context<Self>) {
        self.held = None;
        // In a window too narrow for the dock, a rail opens its panel for now and closes it again.
        let docked = self.place_of(panel).mode != PanelMode::Floating;
        if docked && (self.peeking(panel) || self.dock_floored() && self.is_railed(panel)) {
            self.peek = (!self.peeking(panel)).then_some(panel);
            cx.notify();
            return;
        }
        let shown = self.panel_shown(panel);
        let place = self.place_of(panel);
        if shown.collapsed && !place.collapsed {
            self.panels[panel as usize].held_open = true;
            self.keep_clear();
            cx.notify();
            return;
        }
        // Expanding holds the panel open over the selection; collapsing ends the hold.
        self.panels[panel as usize].held_open = shown.collapsed;
        let place = PanelPlace {
            collapsed: !shown.collapsed,
            ..place
        };
        self.set_place_of(panel, place, cx);
        // Opened, a docked panel can leave too little room for the screen; it opens over it then.
        if docked && !place.collapsed && self.dock_floored() {
            self.peek = Some(panel);
        }
    }

    // The player's place kept inside the area. The parts panel's default place, open or
    // collapsed, stands up to 16 px in from the right edge, less where that would bring it
    // within 8 px of the side panel's default place. With both at their defaults in an area
    // too narrow for 16 px margins, the outer margins shrink alike so neither panel goes flush.
    fn home(&self, panel: Panel) -> PanelPlace {
        let at_default = |p: Panel, d: PanelPlace| {
            let place = self.place_of(p);
            (place.mode, place.x, place.y) == (d.mode, d.x, d.y)
        };
        let both_default = at_default(Panel::Options, PanelPlace::DEFAULT)
            && at_default(Panel::Parts, PanelPlace::PARTS_DEFAULT);
        let state = &self.panels[panel as usize];
        let area = self.area.get().size;
        let mut home = state
            .place
            .clamp(area, self.size_at(panel, px(state.place.y)));
        if panel == Panel::Options && both_default {
            let spare = f32::from(area.width - Panel::Options.width() - Panel::Parts.width())
                - panel::STACK_GAP;
            home.x = home.x.min((spare / 2.).clamp(0., PARTS_INSET));
        }
        if panel == Panel::Parts && at_default(Panel::Parts, PanelPlace::PARTS_DEFAULT) {
            let mut inset = PARTS_INSET;
            if both_default {
                let left = self.home(Panel::Options).x;
                let clear = left + f32::from(Panel::Options.width()) + panel::STACK_GAP;
                inset = inset.min((home.x - clear).max(0.));
            }
            home.x = (home.x - inset).max(0.);
        }
        // A floating panel keeps off the rails at the area's edges, so both stay in reach.
        let (left, right) = self.rail_insets();
        let width = f32::from(self.size_at(panel, px(home.y)).width);
        let last = f32::from(area.width) - right - width;
        home.x = home.x.min(last).max(left);
        home
    }

    // How far rails reach in from the area's left and right edges, with a gap past them.
    fn rail_insets(&self) -> (f32, f32) {
        let reach = |side: PanelMode| {
            let railed =
                (Panel::ALL.into_iter()).any(|p| self.mode_shown(p) == side && self.is_railed(p));
            match railed {
                true => f32::from(panel::RAIL_WIDTH) + panel::STACK_GAP,
                false => 0.,
            }
        };
        (reach(PanelMode::DockLeft), reach(PanelMode::DockRight))
    }

    // The canvas area less the docked columns and rails.
    fn columns_area(&self) -> Bounds<Pixels> {
        self.docked_columns(self.dock_floored())
    }

    // True when the open docked panels would leave the fitted screen shorter than
    // `DOCK_MIN_SCREEN`; it reads only the area, the screen shape and the stored places, so it
    // cannot oscillate.
    fn dock_floored(&self) -> bool {
        let Some(editor) = self.editor() else {
            return false;
        };
        let columns = self.docked_columns(false);
        columns != self.area.get()
            && CanvasFit::new(columns, editor.screen())
                .screen_bounds()
                .size
                .height
                < DOCK_MIN_SCREEN
    }

    /// How `panel` is drawn now: floating or docked, as the player placed it.
    pub fn mode_shown(&self, panel: Panel) -> PanelMode {
        self.place_of(panel).mode
    }

    /// True while docked `panel` shows as a slim rail at its edge: collapsed with nothing open on
    /// its side, or in a window too narrow for the open dock unless opened from its rail.
    pub fn is_railed(&self, panel: Panel) -> bool {
        let place = self.place_of(panel);
        match place.mode {
            PanelMode::Floating => false,
            _ if self.dock_floored() => self.peek != Some(panel),
            side => place.collapsed && !self.side_open(side),
        }
    }

    // The narrow window's panel opened from its rail, drawn over the canvas.
    pub(super) fn peeking(&self, panel: Panel) -> bool {
        self.peek == Some(panel) && self.dock_floored()
    }

    // Some panel docked on `side` is open by the player's choice.
    fn side_open(&self, side: PanelMode) -> bool {
        (Panel::ALL.into_iter()).any(|p| {
            let place = self.place_of(p);
            place.mode == side && !place.collapsed
        })
    }

    // The canvas area less one column per docked side: as wide as its widest open panel, or a
    // rail where all it holds are rails (every docked panel once `floored`).
    fn docked_columns(&self, floored: bool) -> Bounds<Pixels> {
        let area = self.area.get();
        let column = |side: PanelMode| {
            let on_side =
                || (Panel::ALL.into_iter()).filter(move |&p| self.place_of(p).mode == side);
            let open = (on_side().filter(|&p| !floored && !self.place_of(p).collapsed))
                .map(|p| p.width() + DOCK_GAP)
                .max();
            let rail = on_side().next().map(|_| panel::RAIL_WIDTH + DOCK_GAP);
            open.or(rail).unwrap_or_default()
        };
        // A docked side leaves a gutter against its panel; the other side gets the same gutter
        // so the screen never touches the window edge while the canvas is narrowed.
        let (docked_left, docked_right) =
            (column(PanelMode::DockLeft), column(PanelMode::DockRight));
        let any = docked_left > px(0.) || docked_right > px(0.);
        let gutter = if any { DOCK_GAP } else { px(0.) };
        let left = docked_left.max(gutter).min(area.size.width);
        let right = docked_right.max(gutter).min(area.size.width - left);
        Bounds::new(
            area.origin + point(left, px(0.)),
            size(area.size.width - left - right, area.size.height),
        )
    }

    // The part of the canvas area the screen is fitted into: `columns_area`, less the band at
    // the bottom the toolbar needs when no letterbox holds it, so it never covers the screen.
    fn screen_area(&self) -> Bounds<Pixels> {
        let columns = self.columns_area();
        if self.toolbar_seat(columns) != Seat::Band {
            return columns;
        }
        let band = self.toolbar_bounds.get().size.height + TOOLBAR_EDGE + TOOLBAR_SPACE;
        let tall = (columns.size.height - band).max(px(0.));
        Bounds::new(columns.origin, size(columns.size.width, tall))
    }

    // An edit made with the pointer that keeps the selection leaves the floating panel it was
    // made on where it is, so the button just pressed stays under the pointer; a keyboard edit
    // holds nothing. `settle_when_left` and `let_go` release the hold.
    fn hold_pointed(&mut self, pointer: Point<Pixels>) {
        let selected = self.editor().and_then(|e| e.selected());
        if self.every_screen || selected != self.settled_for.as_ref() {
            return;
        }
        let pressed = self.pressed_at;
        self.held = (Panel::ALL.into_iter())
            .filter(|&p| self.mode_shown(p) == PanelMode::Floating)
            .find(|&p| {
                let b = self.panels[p as usize].bounds.get();
                b.contains(&pointer) && pressed.is_some_and(|at| b.contains(&at))
            });
    }

    /// Notes a press at `at`, which makes the edits it leads to the pointer's.
    pub(super) fn pointer_pressed(&mut self, at: Point<Pixels>) {
        self.pressed_at = Some(at);
    }

    /// A key or the pointer leaving the window: later edits are not the pointer's, and a held
    /// panel settles now.
    pub(super) fn let_go(&mut self, cx: &mut Context<Self>) {
        self.pressed_at = None;
        if self.held.take().is_some() {
            self.keep_clear();
            cx.notify();
        }
    }

    /// Settles a held panel once `pointer` has left it.
    pub(super) fn settle_when_left(&mut self, pointer: Point<Pixels>, cx: &mut Context<Self>) {
        if let Some(p) = self.held
            && !self.panels[p as usize].bounds.get().contains(&pointer)
        {
            self.held = None;
            self.keep_clear();
            cx.notify();
        }
    }

    // Settles the toolbar and the floating panels off the selected part, the docked panels and
    // each other, starting from the players' own places, with the sizes measured last frame.
    // A panel with no open spot clear of the selection folds to its header. Neither the moves
    // nor the folds are saved.
    fn keep_clear(&mut self) {
        // The six-screen view draws no panels; leaving it settles them afresh.
        if self.every_screen {
            return;
        }
        let selected = self.editor().and_then(|e| e.selected().cloned());
        if selected != self.settled_for {
            self.held = None;
        }
        let held = self.held;
        // Where each panel was drawn, kept when it ties for the best spot, so nothing flickers.
        let shown = Panel::ALL.map(|p| {
            let place = self.panel_shown(p);
            point(px(place.x), px(place.y))
        });
        for p in Panel::ALL.into_iter().filter(|&p| Some(p) != held) {
            let state = &mut self.panels[p as usize];
            state.off = None;
            state.folded = false;
            state.short = None;
        }
        if selected != self.settled_for {
            for state in &mut self.panels {
                state.held_open = false;
            }
            self.settled_for = selected;
        }
        let area = self.area.get();
        let floating: Vec<Panel> = (Panel::ALL.into_iter())
            .filter(|&p| self.mode_shown(p) == PanelMode::Floating && Some(p) != held)
            .collect();
        // Only an open panel the player has not held open may fold, once its title is measured.
        let header = |page: &Self, p: Panel, large: bool| {
            let state = &page.panels[p as usize];
            let can = !state.place.collapsed && !state.held_open;
            let editor = p == Panel::Options && large;
            (can && !editor && state.title_height.get() > px(0.)).then(|| page.header_size(p))
        };
        let Some(editor) = self.editor() else {
            return;
        };
        let shape = editor.screen();
        let scene = editor.scene();
        // A selected part the game hides draws faintly, so the panels keep clear of it too.
        let part = editor
            .selected()
            .filter(|key| {
                let shown = editor
                    .layout()
                    .get(key)
                    .is_some_and(|b| b.hidden().is_none());
                shown || canvas::game_hidden_root(editor.model(), editor.layout(), key).is_some()
            })
            .and_then(|key| editor.layout().get(key))
            .map(|b| {
                let fit = self.canvas_fit(editor.screen());
                let part = fit.rect(b.x(), b.y(), b.wide(), b.tall());
                Bounds::new(part.origin - area.origin, part.size)
            });
        let share = |b: Bounds<Pixels>| f32::from(b.size.width) * f32::from(b.size.height);
        let large = part.is_some_and(|b| share(b) > share(area) * FOLD_SHARE_MAX);
        let toolbar = Bounds::new(self.toolbar_home(), self.toolbar_bounds.get().size);
        let mut things: Vec<view::Floating> = (floating.iter())
            .map(|&p| {
                let home = self.home(p);
                let size = self.stored_size(p);
                // A collapsed panel is all title bar, and no other panel may cover it.
                let title = if self.place_of(p).collapsed {
                    size.height
                } else {
                    self.header_size(p).height
                };
                view::Floating {
                    home: Bounds::new(point(px(home.x), px(home.y)), size),
                    cap: self.base_cap(p).filter(|_| !self.place_of(p).collapsed),
                    header: header(self, p, large),
                    title,
                    spill: p == Panel::Options,
                    // The part list folds where it stands rather than cross the canvas.
                    fold_first: p == Panel::Parts,
                    shown: Some(shown[p as usize]),
                    avoid: [None, None],
                }
            })
            .collect();
        // Docked panels never move, but nothing may settle onto them; they are open-sized, and
        // an open one is as tall as its share of the column.
        let docked: Vec<(Panel, Bounds<Pixels>)> = (Panel::ALL.into_iter())
            .filter(|&p| self.mode_shown(p) != PanelMode::Floating && !self.is_railed(p))
            .map(|p| {
                let b = self.panels[p as usize].bounds.get();
                let open = self.stored_size(p);
                let open = size(open.width, open.height.max(b.size.height));
                (p, Bounds::new(b.origin - area.origin, open))
            })
            .collect();
        // A docked panel over the selection folds too; its column stays, so nothing else moves.
        for &(p, b) in &docked {
            if header(self, p, large).is_some() && part.is_some_and(|part| b.intersects(&part)) {
                self.panels[p as usize].folded = true;
            }
        }
        // The toolbar stays home, clear of the screen, and the floating panels settle around it
        // in `Panel` order; only panels may lie on one another.
        // A held panel stands where it is drawn, as a docked one would.
        let fixed: Vec<Bounds<Pixels>> = (docked.iter().map(|&(_, b)| b))
            .chain(held.map(|p| {
                let b = self.panels[p as usize].bounds.get();
                Bounds::new(b.origin - area.origin, b.size)
            }))
            .chain(std::iter::once(toolbar))
            .collect();
        // The part list keeps its spot while it stays off the killfeed, the chat and the other
        // panels, across tab switches too; covering the selection is settling's to handle, and
        // covering other parts is not reason enough to move. Otherwise it moves to the nearest
        // spot covering the least of what the canvas draws, selection included so the selection
        // never draws it in. A new area, screen or list size weighs it afresh from the player's
        // place.
        let homes: Vec<Point<Pixels>> = things.iter().map(|t| t.home.origin).collect();
        let to_area = |b: &Bounds<Pixels>| Bounds::new(b.origin - area.origin, b.size);
        let killfeed = self.killfeed_band().map(|b| to_area(&b));
        let chat = self.chat_band().map(|b| to_area(&b));
        // The side panel keeps off the chat lines as it does the selection, settling from its
        // home each frame, so it returns there once the chat leaves.
        if let Some(o) = floating.iter().position(|&p| p == Panel::Options) {
            things[o].avoid = [None, chat];
        }
        let mut search = None;
        if let Some(i) = floating.iter().position(|&p| p == Panel::Parts) {
            let lines: Vec<Bounds<Pixels>> = (self.killfeed_bounds.borrow().iter())
                .chain(self.chat_bounds.borrow().iter())
                .filter(|b| b.size.width > px(0.) && b.size.height > px(0.))
                .map(to_area)
                .collect();
            let drawn: Vec<Bounds<Pixels>> = (self.drawn_bounds.borrow().iter())
                .map(|(b, _)| *b)
                .filter(|b| b.size.width > px(0.) && b.size.height > px(0.))
                .map(|b| to_area(&b))
                .chain(lines.iter().copied())
                .collect();
            // Zoomed in, only the part of the screen the canvas shows can be covered.
            let shown = self.canvas_fit(shape).screen_bounds();
            let screen_box = to_area(&shown.intersect(&self.screen_area()));
            let screen = (self.place_of(Panel::Parts).collapsed).then_some(screen_box);
            let keep_off: Vec<Bounds<Pixels>> = (fixed.iter().copied())
                .chain(
                    (things.iter().enumerate())
                        .filter(|&(j, _)| j != i)
                        .map(|(_, t)| t.home),
                )
                .collect();
            let thing = things[i];
            let sized = |y: Pixels| thing.size_at(area.size, y);
            let boxed = |at: Point<Pixels>| Bounds::new(at, sized(at.y));
            let fits = |at: Point<Pixels>| {
                let b = boxed(at);
                at.x >= px(0.)
                    && at.y >= px(0.)
                    && b.right() <= area.size.width
                    && b.bottom() <= area.size.height
            };
            let in_way = |at: Point<Pixels>| {
                (lines.iter().chain(&killfeed).chain(&chat).chain(&keep_off))
                    .any(|d| boxed(at).intersects(d))
            };
            let weighed = (area.size, thing.home.size, screen_box);
            let dragging = self.panels[Panel::Parts as usize].drag.is_some();
            let kept = (self.parts_spot.as_ref())
                .filter(|kept| !dragging && kept.weighed == weighed && fits(kept.at))
                .map(|kept| (kept.at, kept.painted));
            let from = kept.map_or(thing.home.origin, |(at, _)| at);
            let least = || {
                view::least_covering(
                    from,
                    &sized,
                    thing.shown,
                    &drawn,
                    screen,
                    &keep_off,
                    area.size,
                )
            };
            // The parts drawn belong to this scene once the canvas has painted it.
            let painted = self.drawn_scene.get() == Some(scene);
            // A tab switch alone never moves the list. An answer given before the scene painted
            // is weighed again once it has.
            let (at, judged) = match kept {
                Some((at, was)) if in_way(at) || (!was && painted) => (least(), painted),
                Some((at, was)) => (at, was),
                None => (least(), painted),
            };
            things[i].home.origin = at;
            things[i].avoid = [killfeed, chat];
            // A list the player drags settles afresh from where it is dropped.
            self.parts_spot = (!dragging).then(|| PartsSpot {
                at: things[i].home.origin,
                painted: judged,
                weighed,
            });
            search = (!dragging).then_some((i, drawn, screen, keep_off));
        }
        let (mut settled, mut shorts): (Vec<_>, Vec<_>) =
            (view::settle_shortening(&things, &fixed, part, area.size).into_iter()).unzip();
        // A side panel the list leaves no room folds; the list makes way to the nearest spot
        // clear of where the side panel would stand without it.
        let side = floating.iter().position(|&p| p == Panel::Options);
        if let (Some((i, drawn, screen, mut keep_off)), Some(o)) = (search, side)
            && settled[o].collapsed
        {
            let mut without = things.clone();
            without.remove(i);
            let o_alone = if o < i { o } else { o - 1 };
            let alone = view::settle_folding(&without, &fixed, part, area.size)[o_alone];
            if !alone.collapsed {
                let side = &things[o];
                keep_off.push(Bounds::new(
                    alone.origin,
                    side.size_at(area.size, alone.origin.y),
                ));
                let thing = things[i];
                let sized = |y: Pixels| thing.size_at(area.size, y);
                let at = view::least_covering_off(
                    thing.home.origin,
                    &sized,
                    thing.shown,
                    &drawn,
                    screen,
                    &keep_off,
                    area.size,
                );
                if at != thing.home.origin {
                    things[i].home.origin = at;
                    (settled, shorts) = (view::settle_shortening(&things, &fixed, part, area.size)
                        .into_iter())
                    .unzip();
                    if let Some(spot) = &mut self.parts_spot {
                        spot.at = at;
                    }
                }
            }
        }
        // A panel is moved off when it stands anywhere but the player's place.
        for (((panel, home), to), short) in
            floating.into_iter().zip(homes).zip(&settled).zip(shorts)
        {
            let state = &mut self.panels[panel as usize];
            state.folded = to.collapsed;
            state.short = short;
            if to.origin != home {
                state.off = Some(to.origin);
            }
        }
        // A folded list takes its rows away under a still pointer, so no hover-out comes.
        if self.panel_shown(Panel::Parts).collapsed {
            self.hovered = None;
        }
    }

    /// The HUD name field.
    pub fn name_input(&self) -> &Entity<InputState> {
        &self.name
    }

    /// The library HUD Save writes over, once this edit has one.
    pub fn saved_hud(&self) -> Option<&HudId> {
        self.saved_hud.as_ref()
    }

    /// True while a save runs in the background.
    pub fn is_saving(&self) -> bool {
        self.saving
    }

    /// Chooses `value` for the copy's own option `id` as one undo step, as its row does.
    pub fn set_hud_option(
        &mut self,
        id: &str,
        value: OptionValue,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.edit_selected(window, cx, |e| {
            if let Err(err) = e.set_hud_option(id, value) {
                tracing::debug!("the HUD Maker refused HUD option {id}: {err:?}");
            }
        });
    }

    /// True while the copy's files are rebuilt off the UI thread for new choices.
    pub fn is_switch_reloading(&self) -> bool {
        self.switch_job.is_some()
    }

    /// Switch reloads started and landed since the page was made.
    pub fn switch_reload_stats(&self) -> (u64, u64) {
        self.switch_stats
    }

    /// The files the canvas draws from: a copy's with its choices switched in.
    pub fn shown_files(&self) -> Option<Arc<HudFiles>> {
        match &self.load {
            Load::Ready(loaded) => Some(loaded.files.clone()),
            _ => None,
        }
    }

    /// Takes a saved copy's files read afresh: `pristine` always, and `shown`, its view with the
    /// `sent` choices switched in, only while those are the choices on show. A reload that
    /// landed after the save was sent already shows newer ones.
    pub fn land_saved_files(&mut self, pristine: HudFiles, shown: HudFiles, sent: &[Change]) {
        let Load::Ready(loaded) = &mut self.load else {
            return;
        };
        loaded.pristine_files = Arc::new(pristine);
        if loaded.editor.switch_built_for() == sent {
            loaded.files = Arc::new(shown);
        }
    }

    /// Switch reload results that came back, including ones dropped as stale.
    pub fn switch_arrivals(&self) -> u64 {
        self.switch_arrivals
    }

    /// How many switch reloads the page holds: 0 or 1.
    pub fn live_switch_jobs(&self) -> usize {
        usize::from(self.switch_job.is_some())
    }

    // Starts rebuilding the switched files and model when the editor's HUD choices differ from
    // the ones its model was built for. One job runs, for the newest choices: a job for other
    // choices is dropped, which cancels it, and none runs once the choices are back.
    fn reload_switches(&mut self, cx: &mut Context<Self>) {
        let Load::Ready(loaded) = &self.load else {
            self.switch_job = None;
            return;
        };
        let editor = &loaded.editor;
        let Some(options) = editor
            .hud_options()
            .filter(|_| editor.needs_switch_reload())
        else {
            self.switch_job = None;
            return;
        };
        let choices = editor.switch_choices();
        let running = self.switch_job.as_ref().map(|(key, _)| key);
        if running == Some(&choices) || self.switch_failed.as_ref() == Some(&choices) {
            return;
        }
        let (options, files, platform) = (
            options.clone(),
            loaded.pristine_files.clone(),
            loaded.platform,
        );
        let key = choices.clone();
        let job = run_blocking(cx, move |_| {
            // The raw error goes to the log; the player reads one plain sentence.
            let failed = |e: HudFilesError| {
                LocalFailure::new(HUD_OPTIONS_UNREADABLE, e.to_string())
                    .into_op("Failed to change the HUD's options")
            };
            let shown = (options.switch(&files, &choices))
                .map_err(failed)?
                .into_files();
            let model = HudModel::load(&shown, platform).map_err(failed)?;
            let images = decode_images(&shown, &model, platform);
            let fonts = read_font_files(&shown, &model);
            Ok((shown, model, images, fonts))
        });
        self.switch_stats.0 += 1;
        let (win, load, landing) = (self.win, self.loads, key.clone());
        let task = cx.spawn(async move |page, cx| {
            let done = job.await;
            cx.update(|cx| {
                let _ = win.update(cx, |_, window, cx| {
                    page.update(cx, |page, cx| {
                        page.land_switches(load, landing, done, window, cx)
                    })
                });
            });
        });
        self.switch_job = Some((key, task));
    }

    // Rebases the editor onto a finished switch reload's model and shows its files, images
    // and fonts; a result for another HUD or for older choices is dropped. A failure keeps the
    // model and says so.
    fn land_switches(
        &mut self,
        load: u64,
        key: Vec<Change>,
        done: Result<(HudFiles, HudModel, Images, Vec<FontBytes>), OpError>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.switch_arrivals += 1;
        if self.switch_job.as_ref().is_some_and(|(k, _)| *k == key) {
            self.switch_job = None;
        }
        let Load::Ready(loaded) = &mut self.load else {
            return;
        };
        if self.loads != load {
            return;
        }
        // The editor refuses a model built for other choices than its changes hold now.
        let (shown, images, fonts) = match done {
            Ok((shown, model, images, fonts)) => {
                if !loaded.editor.rebase(model, key) {
                    return;
                }
                (shown, images, fonts)
            }
            Err(err) => {
                if loaded.editor.switch_choices() == key {
                    self.switch_stats.1 += 1;
                    self.switch_failed = Some(key);
                    toast_error(window, cx, &err);
                }
                return;
            }
        };
        loaded.files = Arc::new(shown);
        let old = std::mem::replace(&mut loaded.images, images);
        for image in old.into_values() {
            if !loaded.images.values().any(|i| Arc::ptr_eq(i, &image)) {
                cx.drop_image(image, None);
            }
        }
        self.switch_stats.1 += 1;
        self.register_fonts(fonts, cx);
        // The new images are the switched pristine model's, so every shown picture is checked again.
        self.image_names.clear();
        self.families = None;
        self.decode_shown_images(cx);
        self.simulate_images(cx);
        self.sync_sliders(window, cx);
        cx.notify();
    }

    /// Gives the keys back to the page unless something inside it already holds them.
    pub fn reclaim_focus(&self, window: &mut gpui_kit::Window, cx: &mut gpui_kit::App) {
        if !self.focus.contains_focused(window, cx) {
            self.focus.focus(window, cx);
        }
    }

    /// True unless the editor holds unsaved changes or the name differs from the saved one.
    pub fn can_leave(&self) -> bool {
        self.editor().is_none_or(|e| !e.is_dirty()) && self.typed_name == self.saved_name
    }

    /// True when Save would run now: a HUD is open, it has a name, and no save, install or
    /// history job holds it.
    pub fn can_save(&self, cx: &gpui_kit::App) -> bool {
        matches!(self.load, Load::Ready(_))
            && !self.trimmed_name(cx).is_empty()
            && !(self.saving || self.installing || self.history_busy)
    }

    /// Saves as Save does, then runs `proceed` once the save lands; a save that fails or finds
    /// the name taken keeps the player here, where the toast or the name field says why.
    pub fn save_then(
        &mut self,
        proceed: impl FnOnce(&mut Window, &mut gpui_kit::App) + 'static,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.can_save(cx) {
            return;
        }
        self.after_save = Some(Box::new(proceed));
        self.save(false, window, cx);
    }

    fn trimmed_name(&self, cx: &gpui_kit::App) -> String {
        self.name.read(cx).value().trim().to_owned()
    }

    // Saves the HUD as it is now, over the HUD this edit came from or, with `as_new` or none
    // yet, as a new one. Only the change list taken here is marked saved, so an edit made
    // while the save runs stays unsaved. A save never runs beside an install, whose copy a
    // re-save's folder swap would break.
    fn save(&mut self, as_new: bool, window: &mut Window, cx: &mut Context<Self>) {
        if self.saving || self.installing || self.history_busy {
            return;
        }
        self.end_part_drag(window, cx);
        self.flush(cx);
        let name = self.trimmed_name(cx);
        // Save as new from a copy copies it whole, so the new HUD keeps every base file.
        let target = match &self.saved_hud {
            Some(id) if !as_new => MakeTarget::Existing(id.clone()),
            Some(id) if self.copy_tf.is_some() => MakeTarget::CopyOf(id.clone()),
            _ => MakeTarget::New,
        };
        let copy_tf = self.copy_tf.clone();
        let Load::Ready(loaded) = &self.load else {
            return;
        };
        // Core switches the files on disk itself, so the switched view is never sent.
        let (files, platform) = (loaded.pristine_files.clone(), loaded.platform);
        let changes = loaded.editor.changes().to_vec();
        let assets = loaded.editor.assets().clone();
        let options = loaded.editor.hud_options().cloned();
        let sent_choices = loaded.editor.switch_choices();
        let opened = loaded.opened_base.clone();
        let (sent, sent_assets) = (changes.clone(), assets.clone());
        self.saving = true;
        self.saving_as_new = as_new;
        cx.notify();
        // TF2 found means the save may also update the installed copy, in the same job.
        let tf2 = require_tf2(cx).ok();
        let sent_choices_land = sent_choices.clone();
        let running = run_blocking(cx, move |s| {
            let mut saved = save_job(
                s,
                &target,
                &name,
                &files,
                platform,
                &changes,
                &assets,
                opened.as_ref(),
            )?;
            if let (Saved::Done { id, name, live, .. }, Some(tf2)) = (&mut saved, &tf2) {
                *live = live::live_job(s, id, name, tf2);
            }
            // A copy's save rewrote its originals and hidden paths, so the next save must read
            // them anew rather than take this save's output as the HUD's own files.
            if let (Saved::Done { id, files, .. }, Some(tf_dir)) = (&mut saved, &copy_tf) {
                let reopened = (s.library.made_hud_files(id, tf_dir))
                    .inspect_err(|e| tracing::warn!("a saved copy's files didn't reopen: {e}"))
                    .ok();
                // The options are not read again: the files on disk are the same HUD.
                *files = reopened.and_then(|pristine| {
                    let shown = match &options {
                        Some(options) => (options.switch(&pristine, &sent_choices))
                            .inspect_err(|e| tracing::warn!("a saved copy didn't switch: {e}"))
                            .ok()?
                            .into_files(),
                        None => pristine.clone(),
                    };
                    Some(Box::new((pristine, shown)))
                });
            }
            Ok(saved)
        });
        let win = self.win;
        cx.spawn(async move |page, cx| {
            let done = running.await;
            // The library changed even when the page is gone, so an open HUDs page still reloads.
            if matches!(done, Ok(Saved::Done { .. })) {
                cx.update(LibraryEvents::changed);
            }
            let mut then = None;
            let toast_of = page
                .update(cx, |page, cx| {
                    page.saving = false;
                    cx.notify();
                    let after = page.after_save.take();
                    match done {
                        Ok(Saved::Done {
                            id,
                            name,
                            dropped,
                            live,
                            files,
                        }) => {
                            if let Some(editor) = page.editor_mut() {
                                editor.mark_saved_as(sent, sent_assets);
                            }
                            if let Some(both) = files {
                                let (pristine, shown) = *both;
                                page.land_saved_files(pristine, shown, &sent_choices_land);
                            }
                            page.saved_hud = Some(id);
                            page.name_error = None;
                            if page.history_open {
                                page.read_snapshots(cx);
                            }
                            let mut text = format!("{name} is in your HUDs library.");
                            if let Some(line) = left_out(dropped, false) {
                                text = format!("{text} {line}");
                            }
                            page.saved_name = name;
                            then = after;
                            Some(Ok((text, live)))
                        }
                        Ok(Saved::NameTaken(taken)) => {
                            let err = MakeHudError::NameTaken {
                                name: taken.clone(),
                            };
                            page.name_error = Some((taken, describe_make_hud(&err).message));
                            None
                        }
                        Ok(Saved::Gone(err)) => {
                            page.saved_hud = None;
                            Some(Err(err))
                        }
                        Err(err) => Some(Err(err)),
                    }
                })
                .ok()
                .flatten();
            cx.update(|cx| {
                let _ = win.update(cx, |_, window, cx| match toast_of {
                    Some(Ok((text, live))) => {
                        toast(window, cx, ToastKind::Success, "HUD saved", text);
                        let _ = page.update(cx, |page, cx| page.land_live(&live, window, cx));
                    }
                    Some(Err(err)) => toast_error(window, cx, &err),
                    None => {}
                });
                if let Some(proceed) = then {
                    let _ = win.update(cx, |_, window, cx| proceed(window, cx));
                }
            });
        })
        .detach();
    }

    // The name field with the taken-name line under it, then Save and, once there is a HUD to
    // save over, Save as new; both wait for a running save or install. Each is its own item in
    // the toolbar's row, so a narrow toolbar wraps between them.
    fn save_row(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let name = self.trimmed_name(cx);
        let error = self
            .name_error
            .as_ref()
            .filter(|(taken, _)| *taken == name)
            .map(|(_, line)| {
                div()
                    .id(ids::HUD_MAKER_NAME_ERROR)
                    .aria_label(line.clone())
                    .test_support()
                    .text_xs()
                    .text_color(cx.theme().danger)
                    .child(line.clone())
            });
        let saving_over = self.saving && !self.saving_as_new;
        let name_field = v_flex()
            .w(px(200.))
            .gap_1()
            .child(
                div()
                    .id(ids::HUD_MAKER_NAME)
                    .aria_label("HUD name")
                    .test_support()
                    .child(Input::new(&self.name)),
            )
            .children(error);
        let save = Button::new(ids::HUD_MAKER_SAVE)
            .primary()
            .icon(IconName::Save)
            .label("Save")
            .when(saving_over, |b| b.accessibility_label("Saving..."))
            .loading(saving_over)
            .disabled(name.is_empty() || self.saving || self.installing || self.history_busy)
            .on_click(cx.listener(|page, _, window, cx| page.save(false, window, cx)));
        let install = Button::new(ids::HUD_MAKER_INSTALL)
            .primary()
            .icon(IconName::Download)
            .label("Install")
            .when(!self.can_install(cx), |b| {
                b.tooltip(install_tip(self.saving, require_tf2(cx).is_ok()))
            })
            .loading(self.installing)
            .disabled(self.installing || !self.can_install(cx))
            .on_click(cx.listener(|page, _, window, cx| page.install(window, cx)));
        std::iter::once(name_field.into_any_element())
            .chain(Some(save.into_any_element()))
            .chain(Some(install.into_any_element()))
            .collect()
    }

    /// The overflow menu's "Save as new", once the HUD has been saved: it saves a copy under
    /// the typed name and closes the menu.
    pub(super) fn save_as_new_item(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        self.saved_hud.as_ref()?;
        let saving_new = self.saving && self.saving_as_new;
        let name = self.trimmed_name(cx);
        let button = Button::new(ids::HUD_MAKER_SAVE_AS_NEW)
            .ghost()
            .small()
            .icon(IconName::Copy)
            .label("Save as new")
            .when(saving_new, |b| b.accessibility_label("Saving..."))
            .loading(saving_new)
            .disabled(name.is_empty() || self.saving || self.installing || self.history_busy)
            .on_click(cx.listener(|page, _, window, cx| {
                page.more_open = false;
                page.save(true, window, cx);
            }));
        Some(button.into_any_element())
    }

    // True when the library HUD matches the editor exactly, the name included, and TF2 is found.
    fn can_install(&self, cx: &gpui_kit::App) -> bool {
        self.saved_hud.is_some()
            && !self.saving
            && self.editor().is_some_and(|e| !e.is_dirty())
            && self.trimmed_name(cx) == self.saved_name
            && require_tf2(cx).is_ok()
    }

    // Installs the saved HUD through the library's install, the same job the HUDs page runs
    // minus its font question: a made HUD carries no fonts.
    fn install(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.installing || !self.can_install(cx) {
            return;
        }
        let tf2 = match require_tf2(cx) {
            Ok(tf2) => tf2,
            Err(err) => return toast_error(window, cx, &err),
        };
        let Some(id) = self.saved_hud.clone() else {
            return;
        };
        let name = self.saved_name.clone();
        self.installing = true;
        cx.notify();
        let win = self.win;
        cx.spawn(async move |page, cx| {
            let checking = check_external(
                HudAction::Install,
                None,
                tf2.clone(),
                win,
                INSTALL_FAILED,
                cx,
            );
            let Some(plan) = checking.await else {
                let _ = page.update(cx, |page, cx| {
                    page.installing = false;
                    cx.notify();
                });
                return;
            };
            let running = cx.update(|cx| {
                run_blocking(cx, move |s| {
                    let done = install_job(s, &id, &tf2, &plan)?;
                    Ok(install_toast(&name, &done, &plan))
                })
            });
            let done = running.await;
            let _ = page.update(cx, |page, cx| {
                page.installing = false;
                cx.notify();
            });
            if done.is_ok() {
                cx.update(LibraryEvents::changed);
            }
            cx.update(|cx| {
                let _ = win.update(cx, |_, window, cx| match done {
                    Ok((kind, title, description)) => toast(window, cx, kind, title, description),
                    Err(err) => toast_error(window, cx, &err),
                });
            });
        })
        .detach();
    }

    fn no_game(&self, cx: &mut Context<Self>) -> AnyElement {
        v_flex()
            .flex_1()
            .items_center()
            .justify_center()
            .gap_4()
            .child(labelled(
                div().text_color(cx.theme().muted_foreground),
                ids::HUD_MAKER_NO_GAME,
                HUD_FILES,
            ))
            .child(
                Button::new(ids::HUD_MAKER_OPEN_SETTINGS)
                    .outline()
                    .label("Settings")
                    .on_click(cx.listener(|_, _, _, cx| cx.emit(NavigateTo(Page::Settings)))),
            )
            .into_any_element()
    }

    /// Lays the HUD out at `screen`; typed values commit first, in the pixels they were typed in.
    fn set_screen(&mut self, screen: Screen, window: &mut Window, cx: &mut Context<Self>) {
        self.flush(cx);
        if let Some(editor) = self.editor_mut() {
            editor.set_screen(screen);
        }
        self.refresh(window, cx);
    }

    /// Selects `part`, from a part-list row or the canvas; a new selection ends Match size.
    fn select(&mut self, part: Option<PartKey>, window: &mut Window, cx: &mut Context<Self>) {
        // Picking a part is starting from the HUD as it is, so the Start from card goes.
        self.close_start_card_on(part.is_some());
        self.matching_size = false;
        self.nudge_refused = None;
        if let Some(editor) = self.editor_mut() {
            editor.select(part);
        }
        self.refresh(window, cx);
    }

    // Runs one edit of the selection after committing typed values, then shows the result.
    fn edit_selected(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
        edit: impl FnOnce(&mut HudEditor),
    ) {
        self.flush(cx);
        self.stop_playing();
        if let Some(editor) = self.editor_mut() {
            edit(editor);
        }
        self.refresh(window, cx);
    }

    /// The key of the Colour row the picker edits for the selection; `None` is the colour slot.
    pub(super) fn active_colour(&self) -> Option<&str> {
        let (part, key) = self.active_colour.as_ref()?;
        let selected = self.editor()?.selected()?;
        (part == selected).then_some(key.as_str())
    }

    // Makes `key` (`None` for the slot) the row the picker and named colours edit, and loads
    // the picker with that row's colour.
    pub(super) fn set_active_colour(
        &mut self,
        key: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let part = self.editor().and_then(|e| e.selected().cloned());
        self.active_colour = part.zip(key);
        self.names_open = false;
        self.picking = None;
        self.refresh(window, cx);
    }

    // Writes one colour to the active row: the slot through `recolour`, any other key as a
    // property; `again` replaces the last undo step, so a picker drag is one step.
    fn write_colour(&mut self, colour: &str, again: bool) -> Result<(), EditRefused> {
        let key = self.active_colour().map(str::to_owned);
        let Some(editor) = self.editor_mut() else {
            return Err(EditRefused::UnknownPart);
        };
        // An animation row's colour goes to that row, a picker drag as one step.
        if let Some((event, row)) = key.as_deref().and_then(animations::parse_anim_key) {
            let value = editor::AnimEdit::Value(colour.to_owned());
            let done = match again {
                true => editor.edit_animation_again(&event, row, value),
                false => editor.edit_animation(&event, row, value),
            };
            return done.map_err(|e| match e {
                editor::AnimRefused::RunsCommands => EditRefused::RunsGameCommands,
                _ => EditRefused::BadValue,
            });
        }
        match (key, again) {
            (None, false) => editor.recolour_selected(colour),
            (None, true) => editor.recolour_selected_again(colour),
            (Some(key), false) if key == panel::HIT_FLASH_KEY => {
                editor.set_hit_flash_colour_selected(colour)
            }
            (Some(key), true) if key == panel::HIT_FLASH_KEY => {
                editor.set_hit_flash_colour_selected_again(colour)
            }
            (Some(key), false) => editor.set_property_selected(&key, colour),
            (Some(key), true) => editor.set_property_selected_again(&key, colour),
        }
    }

    // A named colour from the list under the active row, as one undo step.
    pub(super) fn name_colour(&mut self, name: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.names_open = false;
        self.flush(cx);
        if let Err(err) = self.write_colour(name, false) {
            tracing::debug!("the HUD Maker refused named colour {name}: {err:?}");
        }
        self.refresh(window, cx);
    }

    // Applies one picker change; a run of changes on one row is one undo step, and the picker
    // keeps the value the player is dragging.
    fn pick_colour(&mut self, [r, g, b]: [u8; 3], window: &mut Window, cx: &mut Context<Self>) {
        self.flush(cx);
        let picking = self.picking.take();
        let row = self.active_colour().map(str::to_owned);
        let Some(editor) = self.editor() else {
            return;
        };
        let Some(part) = editor.selected().cloned() else {
            return;
        };
        let again = picking.is_some_and(|(key, was, changes)| {
            key == part && was == row && changes == editor.changes()
        });
        if self
            .write_colour(&format!("{r} {g} {b} 255"), again)
            .is_ok()
            && let Some(editor) = self.editor()
        {
            self.picking = Some((part, row, editor.changes().to_vec()));
        }
        self.show(false, window, cx);
    }

    // Commits typed values, then writes the selection's values into the fields and the picker,
    // recording what each field shows so a later commit goes to this part.
    fn refresh(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.flush(cx);
        self.reload_switches(cx);
        self.show(true, window, cx);
    }

    // Drops what the player typed into `fields` without committing it.
    fn discard(&mut self, fields: &[Field]) {
        for &field in fields {
            self.shown[field as usize] = None;
        }
    }

    // Writes the selection's values into the fields, and into the picker unless it is open.
    fn show(&mut self, picker: bool, window: &mut Window, cx: &mut Context<Self>) {
        self.sync_sliders(window, cx);
        self.sync_option_picker(window, cx);
        // A row picked on another part lapses, so selecting it again starts at its slot.
        let selected = self.editor().and_then(|e| e.selected()).cloned();
        // Another part, or none, swaps the panel's rows, so it opens at their top; the part
        // list scrolls the new part's row into view only when it is out of view.
        if selected != self.shown_part {
            self.panel_scroll.set_offset(point(px(0.), px(0.)));
            // A part chosen on the canvas inside a folded group opens the groups above it.
            let above: Vec<PartKey> = match (&selected, self.editor().map(HudEditor::model)) {
                (Some(key), Some(model)) => {
                    std::iter::successors(model.part(key).and_then(|p| p.parent()), |k| {
                        model.part(k)?.parent()
                    })
                    .cloned()
                    .collect()
                }
                _ => Vec::new(),
            };
            for group in &above {
                self.folded.remove(group);
            }
            let order = self.list_order(cx);
            let row = selected.as_ref().and_then(|key| {
                let parts = self.editor()?.model().parts();
                let at = parts.iter().position(|p| p.key() == key)?;
                order.iter().position(|&i| i == at)
            });
            if let Some(row) = row {
                self.parts_scroll
                    .scroll_to_item(row, ScrollStrategy::Nearest);
            }
            self.shown_part = selected.clone();
            self.notes = Default::default();
        }
        let selected = selected.as_ref();
        if self
            .active_colour
            .as_ref()
            .is_some_and(|(part, _)| Some(part) != selected)
        {
            self.active_colour = None;
            self.names_open = false;
        }
        if self
            .font_size_row
            .as_ref()
            .is_some_and(|(part, _)| Some(part) != selected)
        {
            self.font_size_row = None;
        }
        if self
            .font_open
            .as_ref()
            .is_some_and(|(part, _, _)| Some(part) != selected)
        {
            self.font_open = None;
        }
        if self
            .setting_key
            .as_ref()
            .is_some_and(|(part, _)| Some(part) != selected)
        {
            self.setting_key = None;
        }
        let shown = self.field_texts();
        for (input, shown) in self.fields.iter().zip(&shown) {
            let text = shown
                .as_ref()
                .map_or("", |shown| shown.text.as_str())
                .to_owned();
            if input.read(cx).value() != text.as_str() {
                input.update(cx, |st, cx| st.set_value(text, window, cx));
            }
        }
        self.shown = shown;
        let open = self.colour.read(cx).is_open();
        if let Some(colour) = self.selected_colour().filter(|_| picker && !open) {
            self.colour
                .update(cx, |st, cx| st.set_value(colour, window, cx));
        }
        self.hold_pointed(window.mouse_position());
        self.keep_clear();
        cx.notify();
    }

    // Each field's text for the selection: its box in screen pixels, its `zpos`, and each Text or
    // Number key its class offers as the file's text (empty when unset; Text keeps its spaces).
    fn field_texts(&self) -> Shown {
        let Some(editor) = self.editor() else {
            return Default::default();
        };
        let Some((key, part)) = editor
            .selected()
            .and_then(|key| Some((key, editor.model().part(key)?)))
            .filter(|(_, part)| part.is_readable())
        else {
            return Default::default();
        };
        let Some(b) = editor.layout().get(key) else {
            return Default::default();
        };
        let layer = part
            .value("zpos")
            .map_or_else(|| "0".to_owned(), |line| line.text().trim().to_owned());
        let schema = ClassSchema::for_part(part);
        let size_row = (self.font_size_row.as_ref())
            .filter(|(at, _)| at == key)
            .map(|(_, row)| row.clone());
        let setting = (self.setting_key.as_ref())
            .filter(|(at, _)| at == key)
            .map(|(_, setting)| setting.clone());
        Field::ALL.map(|field| {
            let text = match field {
                Field::Setting => {
                    let setting = setting.clone()?;
                    let text = part.value(&setting)?.text().trim().to_owned();
                    return Some(ShownText {
                        part: key.clone(),
                        key: Some(setting),
                        text,
                    });
                }
                Field::FontSize => {
                    let row = size_row.clone()?;
                    let size = self.font_variant(&row).map(|f| f.tall().to_string());
                    return Some(ShownText {
                        part: key.clone(),
                        key: Some(row),
                        text: size.unwrap_or_default(),
                    });
                }
                Field::X => b.x().to_string(),
                Field::Y => b.y().to_string(),
                Field::Wide => b.wide().to_string(),
                Field::Tall => b.tall().to_string(),
                Field::Layer => layer.clone(),
                _ => {
                    let row = field.row_key()?;
                    schema.get(row)?;
                    let text = part.value(row).map_or("", |line| line.text());
                    match field {
                        Field::Text => text.to_owned(),
                        _ => text.trim().to_owned(),
                    }
                }
            };
            Some(ShownText {
                part: key.clone(),
                key: None,
                text,
            })
        })
    }

    // Commits every field whose text differs from what its last refresh wrote, to the part that
    // refresh showed, once: the field is then cleared until the next refresh. A whole-number
    // field's text out of range commits nothing; Text, "Time on screen" and a setting go to core
    // as typed, which refuses what the row cannot take. Either way the refresh puts the real
    // value back, and the box's note says why when that value is not what was typed.
    fn flush(&mut self, cx: &mut Context<Self>) {
        for field in Field::ALL {
            let Some(shown) = self.shown[field as usize].clone() else {
                continue;
            };
            let typed = self.fields[field as usize].read(cx).value().to_string();
            let text = match field {
                Field::Text => typed,
                _ => typed.trim().to_owned(),
            };
            if text == shown.text {
                continue;
            }
            self.shown[field as usize] = None;
            let Some(editor) = self.editor_mut() else {
                return;
            };
            let before = editor.selected().cloned();
            editor.select(Some(shown.part.clone()));
            let result = commit_typed(editor, field, &shown, &text);
            editor.select(before);
            // A box value the file holds in coarser steps lands on the nearest one.
            let note = match result {
                Ok(()) => typed_value(field, &text, editor.screen())
                    .ok()
                    .zip(landed(editor, field, &shown.part))
                    .filter(|(typed, landed)| typed != landed)
                    .map(|(_, landed)| {
                        let said =
                            format!("Rounded to {landed}, the nearest step the HUD file can hold");
                        (said, false)
                    }),
                Err(Refusal::Note(said)) => Some((said, true)),
                Err(Refusal::Refused(err)) => {
                    tracing::debug!("the HUD Maker refused a typed {field:?}: {err:?}");
                    None
                }
                Err(Refusal::Skipped) => None,
            };
            self.notes[field as usize] = note.map(|(said, refused)| FieldNote {
                part: shown.part,
                text: said.into(),
                refused,
            });
        }
    }

    /// Undoes the last change, first committing typed values so each step is one the player
    /// sees; during a drag it cancels only the drag.
    pub fn undo(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.flush(cx);
        self.part_drag = None;
        let before = self.scene();
        if let Some(editor) = self.editor_mut() {
            editor.undo();
        }
        self.left_scene(before);
        self.decode_shown_images(cx);
        self.refresh(window, cx);
    }

    // Escape drops a running drag, else ends Match size picking, else leaves the six-screen view,
    // else deselects the part; with none of these the key goes on to whatever else wants it.
    fn cancel_part_drag(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.part_drag.take().is_some() {
            if let Some(editor) = self.editor_mut() {
                editor.cancel_drag();
            }
            return self.refresh(window, cx);
        }
        if std::mem::take(&mut self.matching_size) {
            return cx.notify();
        }
        if self.every_screen {
            return self.set_every_screen(false, window, cx);
        }
        if self.editor().is_some_and(|e| e.compared().is_some()) {
            return self.stop_comparing(cx);
        }
        if self.editor().and_then(|e| e.selected()).is_some() {
            return self.select(None, window, cx);
        }
        cx.propagate();
    }

    // H (`toggle`) and Delete hide the selection through the Parts list eye's switch, one undo
    // step; H on a part the player hid shows it again. With nothing selected the key goes on.
    fn hide_selected(&mut self, toggle: bool, window: &mut Window, cx: &mut Context<Self>) {
        self.let_go(cx);
        if self.part_drag.is_some() {
            return;
        }
        let Some(part) = self.editor().and_then(|e| e.selected()).cloned() else {
            return cx.propagate();
        };
        let hidden = self
            .editor()
            .is_some_and(|e| e.part_hidden_by_player(&part));
        if hidden && !toggle {
            return;
        }
        self.edit_selected(window, cx, |e| {
            if let Err(err) = e.set_part_hidden(&part, !hidden) {
                tracing::debug!("the HUD Maker refused to hide {part:?}: {err:?}");
            }
        });
    }

    // An arrow key moves the selection by whole HUD units; with nothing selected the key goes
    // on, and during a drag the pointer owns the part.
    fn nudge(&mut self, dx: i32, dy: i32, window: &mut Window, cx: &mut Context<Self>) {
        self.let_go(cx);
        if self.part_drag.is_some() {
            return;
        }
        let Some(part) = self.editor().and_then(|e| e.selected()).cloned() else {
            return cx.propagate();
        };
        // The real nudge runs first; only one that changed nothing is asked why.
        let mut outcome = None;
        self.edit_selected(window, cx, |e| {
            let before = e.changes().len();
            let result = e.nudge_selected(dx, dy);
            outcome = Some((result, e.changes().len() != before));
        });
        let why = match outcome {
            Some((_, true)) | None => None,
            Some((result, false)) => self
                .editor()
                .and_then(|e| why_no_nudge(e.model(), &part, result, dx, dy)),
        };
        let Some(why) = why else {
            self.nudge_refused = None;
            return;
        };
        let refused = Some((part, why));
        if self.nudge_refused != refused {
            self.nudge_refused = refused;
            toast(window, cx, ToastKind::Info, why, "");
        }
    }

    /// The Undo button's tooltip, naming the step it takes back when it has a name.
    pub fn undo_tip(&self) -> String {
        match self.editor().and_then(HudEditor::undo_name) {
            Some(name) => format!("Undo {name} (Ctrl+Z)"),
            None => UNDO_TIP.to_owned(),
        }
    }

    /// The Redo button's tooltip, naming the step it puts back when it has a name.
    pub fn redo_tip(&self) -> String {
        match self.editor().and_then(HudEditor::redo_name) {
            Some(name) => format!("Redo {name} (Ctrl+Y)"),
            None => REDO_TIP.to_owned(),
        }
    }

    /// Redoes the last undone change; nothing while a drag runs.
    pub fn redo(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.part_drag.is_some() {
            return;
        }
        self.flush(cx);
        let before = self.scene();
        if let Some(editor) = self.editor_mut() {
            editor.redo();
        }
        self.left_scene(before);
        self.decode_shown_images(cx);
        self.refresh(window, cx);
    }

    // An undo or redo that entered another scene swapped the list under a still pointer.
    fn left_scene(&mut self, before: Option<Scene>) {
        if self.scene() != before {
            self.hovered = None;
            self.picking = None;
            self.matching_size = false;
        }
    }

    // A strip of one small tab per scene, the current one selected, then the scene's note at
    // the right, where no panel or the toolbar can cover it; a narrow strip cuts its text short.
    fn scene_tabs(&self, cx: &mut Context<Self>) -> AnyElement {
        let current = self.scene();
        let note = self
            .editor()
            .and_then(|e| canvas::scene_note(e.model()))
            .map(|text| {
                labelled(
                    div()
                        .min_w_0()
                        .truncate()
                        .text_sm()
                        .text_color(cx.theme().muted_foreground),
                    ids::HUD_MAKER_SCENE_NOTE,
                    text,
                )
            });
        // The current tab is filled, the others outlined, so the open scene reads at a glance.
        let pick = |scene: Scene, cx: &mut Context<Self>| {
            Button::new(ids::hud_maker_scene(scene))
                .small()
                .label(SceneRule::get(scene).label())
                .selected(current == Some(scene))
                .on_click(cx.listener(move |page, _, window, cx| {
                    page.scene_open = false;
                    page.set_scene(scene, window, cx);
                }))
        };
        let chooser = match self.scene_tabs_fit() {
            true => {
                let tabs = Scene::ALL.map(|scene| {
                    let tab = pick(scene, cx);
                    match current == Some(scene) {
                        true => tab.primary(),
                        false => tab.outline(),
                    }
                });
                ButtonGroup::new(ids::HUD_MAKER_SCENE_TABS)
                    .children(tabs)
                    .into_any_element()
            }
            false => self.scene_menu(current, &pick, cx),
        };
        let modes = (current == Some(Scene::Scoreboard)).then(|| self.board_mode_menu(cx));
        let situation = (current == Some(Scene::Hud))
            .then(|| self.situation_menu(cx))
            .flatten();
        let chip = (current != Some(Scene::Hud) && self.minmode).then(|| self.minmode_chip(cx));
        let chooser = h_flex()
            .gap_2()
            .child(chooser)
            .children(modes)
            .children(situation)
            .children(chip);
        h_flex()
            .id(ids::HUD_MAKER_SCENES)
            .test_support()
            .w_full()
            .px_2()
            .py_1()
            .border_b_1()
            .border_color(cx.theme().border)
            .gap_3()
            .justify_between()
            .child(chooser)
            .children(self.minmode_note(cx))
            .children(note)
            .into_any_element()
    }

    // Away from the HUD tab's situation menu, a chip says the minimal HUD is on and turns it off.
    fn minmode_chip(&self, cx: &mut Context<Self>) -> AnyElement {
        let chip = Button::new(ids::HUD_MAKER_MINMODE_CHIP)
            .outline()
            .small()
            .label("Minimal HUD")
            .selected(true)
            .on_click(cx.listener(|page, _, window, cx| page.set_minmode(false, window, cx)));
        tipped(&ids::HUD_MAKER_MINMODE_CHIP.into(), MINMODE_CHIP_TIP, chip)
    }

    // Says so while the switch is on and the scene has nothing the minimal HUD changes.
    fn minmode_note(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let plain = self
            .editor()
            .is_some_and(|e| e.minmode() && !e.model().has_minmode_keys());
        plain.then(|| {
            labelled(
                div()
                    .min_w_0()
                    .truncate()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground),
                ids::HUD_MAKER_MINMODE_NOTE,
                "This HUD looks the same with the minimal HUD on.",
            )
            .into_any_element()
        })
    }

    // Whether every scene tab fits the strip with room left for the note. It estimates the row
    // from the labels and reads only the area and the Minimal HUD switch, never the scene, so
    // the strip keeps one layout per width; before the first layout it says yes.
    fn scene_tabs_fit(&self) -> bool {
        let width = f32::from(self.area.get().size.width);
        let letters: usize = Scene::ALL
            .iter()
            .map(|&s| SceneRule::get(s).label().chars().count())
            .sum();
        let row = letters as f32 * SCENE_TAB_LETTER + Scene::ALL.len() as f32 * SCENE_TAB_PAD;
        // Room for the widest extras any scene adds, the Scoreboard's mode dropdown and the
        // Minimal HUD chip; the HUD tab's situation menu takes room from the note instead.
        let chip = match self.minmode {
            true => MINMODE_CHIP_WIDTH,
            false => 0.,
        };
        width <= 0. || row + BOARD_MODE_WIDTH + chip + SCENE_NOTE_ROOM <= width
    }

    // One button named for the open mode; its menu lists the three modes and a pick switches.
    fn board_mode_menu(&self, cx: &mut Context<Self>) -> AnyElement {
        let page = cx.entity().downgrade();
        let current = self.board_mode().unwrap_or_default();
        let trigger = Button::new(ids::HUD_MAKER_BOARD_MODE)
            .outline()
            .small()
            .icon(IconName::ChevronsUpDown)
            .label(current.label());
        let items = BoardMode::ALL.map(|mode| {
            Button::new(ids::hud_maker_board_mode(mode))
                .ghost()
                .small()
                .label(mode.label())
                .selected(mode == current)
                .on_click(cx.listener(move |page, _, window, cx| {
                    page.board_mode_open = false;
                    page.set_board_mode(mode, window, cx);
                }))
        });
        Popover::new("hud-maker-board-modes")
            .anchor(Anchor::TopLeft)
            .open(self.board_mode_open)
            .on_open_change(move |&open, _, cx| {
                let _ = page.update(cx, |page, cx| {
                    page.board_mode_open = open;
                    cx.notify();
                });
            })
            .trigger(trigger)
            .child(v_flex().gap_1().items_start().children(items))
            .into_any_element()
    }

    // One button named for the open scene; its menu lists every scene and a pick switches.
    fn scene_menu(
        &self,
        current: Option<Scene>,
        pick: &dyn Fn(Scene, &mut Context<Self>) -> Button,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let page = cx.entity().downgrade();
        let name = current.map_or("Scene", |s| SceneRule::get(s).label());
        let trigger = Button::new(ids::HUD_MAKER_SCENE_MENU)
            .outline()
            .small()
            .icon(IconName::ChevronsUpDown)
            .label(name);
        let items = Scene::ALL.map(|scene| pick(scene, cx).ghost());
        Popover::new("hud-maker-scene-choices")
            .anchor(Anchor::TopLeft)
            .open(self.scene_open)
            .on_open_change(move |&open, _, cx| {
                let _ = page.update(cx, |page, cx| {
                    page.scene_open = open;
                    cx.notify();
                });
            })
            .trigger(trigger)
            .child(v_flex().gap_1().items_start().children(items))
            .into_any_element()
    }

    // The screen shapes sit behind one button named for the shape in use, so the toolbar keeps
    // to one row in a small window; a pick changes the shape and closes the menu.
    fn screen_menu(&self, screen: Screen, cx: &mut Context<Self>) -> AnyElement {
        let page = cx.entity().downgrade();
        let current = (Screen::PRESETS.iter().zip(SCREEN_LABELS))
            .find(|&(&preset, _)| preset == screen)
            .map_or_else(
                || format!("{}x{}", screen.wide(), screen.tall()),
                |(_, label)| label.to_owned(),
            );
        let trigger = Button::new(ids::HUD_MAKER_SCREEN_MENU)
            .outline()
            .icon(IconName::ChevronsUpDown)
            .label(current)
            .tooltip("Screen shape");
        let shapes = (Screen::PRESETS.iter().zip(SCREEN_LABELS)).map(|(&preset, label)| {
            Button::new(ids::hud_maker_screen(preset))
                .ghost()
                .small()
                .label(label)
                .selected(preset == screen)
                .on_click(cx.listener(move |page, _, window, cx| {
                    page.screen_open = false;
                    page.set_screen(preset, window, cx);
                }))
        });
        Popover::new("hud-maker-screen-shapes")
            .anchor(Anchor::BottomLeft)
            .open(self.screen_open)
            .on_open_change(move |&open, _, cx| {
                let _ = page.update(cx, |page, cx| {
                    page.screen_open = open;
                    cx.notify();
                });
            })
            .trigger(trigger)
            .child(v_flex().gap_1().items_start().children(shapes))
            .into_any_element()
    }

    /// True when the toolbar is too narrow for two rows, so its save row waits in More. It reads
    /// the columns, never the toolbar's own height.
    pub(super) fn toolbar_compact(&self) -> bool {
        self.columns_area().size.width - TOOLBAR_GAP * 2. < TOOLBAR_TWO_ROWS
    }

    // The screen shape menu, then Undo and Redo, the name and save row, the zoom menu and the
    // overflow menu, which wrap among themselves; the button that collapses the toolbar sits
    // outside the wrap, so it always closes the first row.
    fn toolbar(&self, screen: Screen, cx: &mut Context<Self>) -> AnyElement {
        let (can_undo, can_redo) = self
            .editor()
            .map_or((false, false), |e| (e.can_undo(), e.can_redo()));
        let history = [
            (
                ids::HUD_MAKER_UNDO,
                IconName::Undo2,
                "Undo",
                self.undo_tip(),
                can_undo,
            ),
            (
                ids::HUD_MAKER_REDO,
                IconName::Redo2,
                "Redo",
                self.redo_tip(),
                can_redo,
            ),
        ]
        .map(|(id, icon, label, words, enabled)| {
            let button = Button::new(id)
                .ghost()
                .icon(icon)
                .accessibility_label(label)
                .disabled(!enabled)
                .on_click(cx.listener(move |page, _, window, cx| {
                    if id == ids::HUD_MAKER_UNDO {
                        page.undo(window, cx);
                    } else {
                        page.redo(window, cx);
                    }
                }));
            // The holder keeps the tooltip on a greyed-out button too.
            tip::tipped(&id.into(), words, button)
        });
        let collapse = Button::new(ids::HUD_MAKER_TOOLBAR_COLLAPSE)
            .ghost()
            .icon(IconName::ChevronDown)
            .tooltip("Hide toolbar")
            .accessibility_label("Hide toolbar")
            .on_click(cx.listener(|page, _, _, cx| page.set_toolbar_collapsed(true, cx)));
        let body = h_flex()
            .flex_1()
            .min_w_0()
            .flex_wrap()
            .items_start()
            .gap_2()
            .child(self.screen_menu(screen, cx))
            .child(h_flex().gap_1().children(history))
            .children(
                (!self.toolbar_compact())
                    .then(|| self.save_row(cx))
                    .into_iter()
                    .flatten(),
            )
            // The zoom menu sits after the save row, so the name keeps its place on the first row.
            .child(self.zoom_controls(screen, cx))
            .child(self.more_menu(cx))
            .children(self.live_note_row(cx));
        h_flex()
            .items_start()
            .gap_2()
            .child(body)
            .child(collapse.flex_none())
            .into_any_element()
    }

    // A column holding the scene tabs over the canvas area, which the toolbar and panels
    // float over.
    fn editor_layout(&mut self, cx: &mut Context<Self>) -> AnyElement {
        v_flex()
            .size_full()
            .child(self.scene_tabs(cx))
            .child(self.canvas_area(cx))
            .children(self.timeline(cx))
            .into_any_element()
    }
}

/// Why Install is off for a HUD that isn't saved and unchanged: a save still running, no TF2
/// found, or else the HUD needing a save.
pub fn install_tip(saving: bool, tf2_found: bool) -> &'static str {
    match (saving, tf2_found) {
        (true, _) => "Saving...",
        (false, false) => "TF2 not found",
        (false, true) => "Save the HUD first",
    }
}

// The background half of a save: core's write and the activity row. A taken name and a deleted
// HUD come back as outcomes.
// A copy opened at `opened` is saved only while that is still its base; a refusal toasts.
#[expect(
    clippy::too_many_arguments,
    reason = "a save's own arguments plus the base the page opened"
)]
fn save_job(
    s: &Services,
    target: &MakeTarget,
    name: &str,
    files: &HudFiles,
    platform: Platform,
    changes: &[Change],
    assets: &HudAssets,
    opened: Option<&MakerBase>,
) -> Result<Saved, OpError> {
    // Core writes only the assets the changes still apply, so undone adds stay behind.
    let library = &s.library;
    let saved = match opened {
        Some(opened) => {
            library.save_made_hud_opened(target, name, files, platform, changes, assets, opened)
        }
        None => library.save_made_hud_with_assets(target, name, files, platform, changes, assets),
    };
    let saved = match saved {
        Ok(saved) => saved,
        Err(MakeHudError::NameTaken { name }) => return Ok(Saved::NameTaken(name)),
        Err(e @ MakeHudError::NotFound { .. }) => {
            return Ok(Saved::Gone(describe_make_hud(&e).into_op(SAVE_FAILED)));
        }
        // The sentence is the whole toast; the editor keeps its changes.
        Err(MakeHudError::BaseChanged) => {
            return Err(OpError::Core {
                action: HUD_BASE_CHANGED,
                message: String::new(),
            });
        }
        Err(e) => return Err(describe_make_hud(&e).into_op(SAVE_FAILED)),
    };
    let entry = saved.entry();
    let row = NewLogEntry::new(
        LogLevel::Info,
        "hud",
        "make",
        format!("Saved HUD: {}", entry.name()),
    );
    activity::write(s, &row);
    Ok(Saved::Done {
        id: entry.id().clone(),
        name: entry.name().to_owned(),
        dropped: saved.dropped(),
        live: live::Live::NotInstalled,
        files: None,
    })
}

// Why a typed value did not commit: a note for the player, core's refusal, or no row to take it.
enum Refusal {
    Note(String),
    Refused(EditRefused),
    Skipped,
}

// Commits `text` typed into `field` to `part`, which the caller has selected.
fn commit_typed(
    editor: &mut HudEditor,
    field: Field,
    shown: &ShownText,
    text: &str,
) -> Result<(), Refusal> {
    let screen = editor.screen();
    let value = || typed_value(field, text, screen).map_err(Refusal::Note);
    let key = || shown.key.as_deref().ok_or(Refusal::Skipped);
    let row = || field.row_key().ok_or(Refusal::Skipped);
    let done = match field {
        Field::Text => editor.set_property_selected(TEXT_KEY, text),
        // Core judges a time, so its bad value is the one refusal the note explains.
        Field::Lifetime => {
            return editor
                .set_property_selected("delta_lifetime", text)
                .map_err(|err| match err {
                    EditRefused::BadValue => Refusal::Note(time_note(text)),
                    err => Refusal::Refused(err),
                });
        }
        Field::Setting => editor.set_key_text_selected(key()?, text),
        Field::FontSize => editor.edit_font_selected(key()?, FontEdit::Size(value()?)),
        // Every other field is a whole number for a part the layout places.
        _ => {
            let b = (editor.layout().get(&shown.part).cloned()).ok_or(Refusal::Skipped)?;
            match field {
                Field::X => editor.move_selected(value()? - b.x(), 0),
                Field::Y => editor.move_selected(0, value()? - b.y()),
                Field::Wide => editor.resize_selected(Some(value()?), None),
                Field::Tall => editor.resize_selected(None, Some(value()?)),
                Field::Layer => editor.set_layer_selected(value()?),
                Field::RowHeight | Field::RowGap | Field::SectionGap => {
                    editor.set_number_selected(row()?, value()?)
                }
                // The killfeed's line numbers are Stage 2 rows, which core writes through
                // set_property.
                _ => editor.set_property_selected(row()?, &value()?.to_string()),
            }
        }
    };
    done.map_err(Refusal::Refused)
}

// Where a box field's value landed on `part`; None for the other fields.
fn landed(editor: &HudEditor, field: Field, part: &PartKey) -> Option<i32> {
    let b = editor.layout().get(part)?;
    match field {
        Field::X => Some(b.x()),
        Field::Y => Some(b.y()),
        Field::Wide => Some(b.wide()),
        Field::Tall => Some(b.tall()),
        _ => None,
    }
}

// A typed whole number the field accepts at `screen`, or the note saying why not.
fn typed_value(field: Field, text: &str, screen: Screen) -> Result<i32, String> {
    let digits = text.strip_prefix('-').unwrap_or(text);
    let range = number_range(field, screen)
        .filter(|_| !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit()))
        .ok_or_else(|| "Whole numbers only".to_owned())?;
    (text.parse::<i64>().ok())
        .filter(|v| range.contains(v))
        .and_then(|v| i32::try_from(v).ok())
        .ok_or_else(|| format!("Must be between {} and {}", range.start(), range.end()))
}

// What a whole-number field takes: a size from 1 to four screens on its axis, a position within
// four screens either side of the origin, any layer that fits, a Number up to core's four
// 480-unit screens, or a font size core takes.
fn number_range(field: Field, screen: Screen) -> Option<std::ops::RangeInclusive<i64>> {
    let reach = |span: u32| 4 * i64::from(span);
    Some(match field {
        Field::X => -reach(screen.wide())..=reach(screen.wide()),
        Field::Y => -reach(screen.tall())..=reach(screen.tall()),
        Field::Wide => 1..=reach(screen.wide()),
        Field::Tall => 1..=reach(screen.tall()),
        Field::Layer => i64::from(i32::MIN)..=i64::from(i32::MAX),
        Field::RowHeight
        | Field::RowGap
        | Field::SectionGap
        | Field::LineHeight
        | Field::LineSpacing => 0..=reach(480),
        Field::FontSize => 1..=256,
        Field::Text | Field::Lifetime | Field::Setting => return None,
    })
}

// Why core refused a typed time on screen.
fn time_note(text: &str) -> String {
    if text.parse::<f64>().is_ok() {
        "Must be between 0 and 1920, with at most two decimals".to_owned()
    } else {
        "Numbers only".to_owned()
    }
}

// Each `CustomFontFiles` file the HUD lists; a missing or unreadable one is skipped.
fn read_font_files(files: &HudFiles, model: &HudModel) -> Vec<FontBytes> {
    let listed = read_listed_fonts(model.scheme().font_files(), |path| {
        files
            .read(path)
            .ok()
            .flatten()
            .map(|file| file.into_bytes())
    });
    listed.into_iter().map(FontBytes::new).collect()
}

// A font file's bytes with every family it answers to and its registration key, worked out
// off the UI thread.
struct FontBytes {
    key: (usize, u64),
    families: Vec<String>,
    bytes: Vec<u8>,
}

impl FontBytes {
    fn new(bytes: Vec<u8>) -> FontBytes {
        FontBytes {
            key: RegisteredFonts::key(&bytes),
            families: font_families(&bytes),
            bytes,
        }
    }
}

// Reads each listed path once, ignoring case, up to MAX_FONT_FILES files and MAX_FONT_TOTAL
// bytes kept; a file that would pass either limit is skipped and logged, and reading stops
// once MAX_FONT_TOTAL bytes have been read, kept or not.
fn read_listed_fonts(
    paths: &[String],
    mut read: impl FnMut(&str) -> Option<Vec<u8>>,
) -> Vec<Vec<u8>> {
    let mut seen = HashSet::new();
    let (mut total, mut read_total): (u64, u64) = (0, 0);
    let mut fonts = Vec::new();
    for path in paths {
        if !seen.insert(path.to_ascii_lowercase()) {
            continue;
        }
        if fonts.len() >= MAX_FONT_FILES || read_total >= MAX_FONT_TOTAL {
            tracing::warn!("the HUD Maker skipped font files past its file or byte limit");
            break;
        }
        let Some(bytes) = read(path) else {
            continue;
        };
        read_total = read_total.saturating_add(bytes.len() as u64);
        let after = total.saturating_add(bytes.len() as u64);
        if after > MAX_FONT_TOTAL {
            tracing::warn!("the HUD Maker skipped font file {path}: the HUD's fonts are too big");
            continue;
        }
        total = after;
        fonts.push(bytes);
    }
    fonts
}

// The font files the text system already holds this run, by length and hash, so a reload never
// hands it the same bytes twice.
#[derive(Default)]
struct RegisteredFonts(HashSet<(usize, u64)>);

impl Global for RegisteredFonts {}

impl RegisteredFonts {
    fn key(bytes: &[u8]) -> (usize, u64) {
        use std::hash::{BuildHasher as _, BuildHasherDefault, DefaultHasher};
        (
            bytes.len(),
            BuildHasherDefault::<DefaultHasher>::default().hash_one(bytes),
        )
    }

    // True the first time this key is seen, and records it.
    fn first_time(&mut self, key: (usize, u64)) -> bool {
        self.0.insert(key)
    }
}

// Hands one font file to the text system once per run; a file it refuses is logged, never
// shown, and false.
fn register_font(font: FontBytes, cx: &mut App) -> bool {
    let FontBytes { key, bytes, .. } = font;
    if !cx.default_global::<RegisteredFonts>().first_time(key) {
        return true;
    }
    let registered = cx.text_system().add_fonts(vec![Cow::Owned(bytes)]);
    if let Err(e) = &registered {
        tracing::warn!("the HUD Maker could not register a font file: {e}");
        cx.global_mut::<RegisteredFonts>().0.remove(&key);
    }
    registered.is_ok()
}

// A picked font file's name and bytes, or `None` when it is not a regular file, is over the
// cap or cannot be read; the read is bounded in case the file grows meanwhile.
fn read_font_file(path: &Path) -> Option<(String, Vec<u8>)> {
    use std::io::Read as _;
    let name = path.file_name()?.to_string_lossy().into_owned();
    // Opening a FIFO blocks, so the kind is checked before the open.
    if !std::fs::metadata(path).ok()?.is_file() {
        return None;
    }
    let file = std::fs::File::open(path).ok()?;
    if file.metadata().ok()?.len() > MAX_FONT_BYTES {
        return None;
    }
    let mut bytes = Vec::new();
    file.take(MAX_FONT_BYTES + 1).read_to_end(&mut bytes).ok()?;
    (bytes.len() as u64 <= MAX_FONT_BYTES).then_some((name, bytes))
}

// Decodes the texture every scene's image parts draw for the local team, keyed by part; each
// scalable border's, sample player's ping bar's and class icon's, keyed by image name (no part
// key starts with ".."); and each sample kill's weapon icons cut from their sheets. Each name
// is read once. The menu's backgrounds wait for the Main menu tab; see `menu_art`.
fn decode_images(files: &HudFiles, model: &HudModel, platform: Platform) -> Images {
    let mut by_name: HashMap<String, Decoded> = HashMap::new();
    let mut decode = |name: &str| {
        let decoded = (by_name.entry(name.to_owned()))
            .or_insert_with_key(|name| decode_texture(files, name, platform));
        decoded.clone()
    };
    let mut images = Images::new();
    for rule in SceneRule::every() {
        let model = model.with_scene(rule.scene()).with_board_mode(rule.mode());
        let sample = rule.sample();
        for part in model.parts() {
            if let Some(name) = scene_image(&model, part, canvas::LOCAL_TEAM)
                && let Some((_, image)) = decode(name)
            {
                images.insert(part.key().clone(), image);
            }
            // A scalable border's texture, keyed by its image name as the ping bars are.
            // The game's code can set the border itself, so the file's own is not decoded then.
            let border = sample
                .code_border(part.name())
                .or_else(|| part.value("border").map(|l| l.text().trim()));
            if let Some(border) = border.and_then(|name| model.scheme().image_border(name))
                && let Some((_, image)) = decode(border.image())
            {
                images.insert(PartKey::new(border.image()), image);
            }
        }
        let players = [Team::Red, Team::Blue].map(|team| sample.players(team));
        let player_images = players
            .iter()
            .flat_map(|p| p.iter())
            .flat_map(|p| [p.ping_image(), p.class_image(canvas::LOCAL_TEAM)])
            .flatten();
        for name in player_images {
            if let Some((_, image)) = decode(name) {
                images.insert(PartKey::new(name), image);
            }
        }
    }
    // The menu's class picture is keyed by its part only when that part has no texture of its
    // own, so the key never collides.
    let menu = model.with_scene(Scene::MainMenu);
    let picture = (menu.parts().iter()).find(|p| {
        p.name().eq_ignore_ascii_case("TFCharacterImage")
            && p.parent()
                .and_then(|parent| menu.part(parent))
                .is_some_and(|parent| parent.is_root())
    });
    if let Some(part) = picture.filter(|p| drawn_image(p, canvas::LOCAL_TEAM).is_none())
        && let Ok(Some(name)) = files.menu_character(platform)
        && let Some((_, image)) = decode(&name)
    {
        images.insert(part.key().clone(), image);
    }
    // The class model's portrait, keyed by its name as the ping bars are.
    if let Some((_, image)) = decode(canvas::MODEL_STAND_IN) {
        images.insert(PartKey::new(canvas::MODEL_STAND_IN), image);
    }
    // Every sample kill's icon, plain and for a line the local player is in.
    let weapons: BTreeSet<&str> = [Team::Red, Team::Blue]
        .iter()
        .flat_map(|&team| SampleSet::Casual.players(team))
        .map(|p| p.kill_weapon())
        .collect();
    let icons = files.kill_icons(platform).ok();
    for weapon in weapons {
        for name in [format!("d_{weapon}"), format!("dneg_{weapon}")] {
            let Some(icon) = icons.as_ref().and_then(|icons| icons.get(&name)) else {
                continue;
            };
            if let Some((sheet, _)) = decode(icon.image())
                && let Some(cut) = crop(&sheet, &icon)
                && let Some(image) = to_render_image(cut.width(), cut.height(), cut.into_pixels())
            {
                images.insert(canvas::kill_icon_key(&name), image);
            }
        }
    }
    images
}

/// A part's picture as last decoded: its name, and the source of the editor's file at the
/// material that name resolves to, if one overrides it.
#[derive(Debug, Clone, PartialEq, Eq)]
struct ShownPicture {
    name: String,
    source: Option<String>,
}

impl ShownPicture {
    fn of(model: &HudModel, name: &str) -> ShownPicture {
        let material = picture_material(name);
        let source = (model.assets().iter())
            .find(|(path, _)| {
                material
                    .as_deref()
                    .is_some_and(|m| path.eq_ignore_ascii_case(m))
            })
            .map(|(_, source)| source.clone());
        ShownPicture {
            name: name.to_owned(),
            source,
        }
    }
}

// The editor's added files at the shown model's paths that `keep` takes, with their bytes, as
// a layer over the HUD.
fn top_layer(editor: &HudEditor, keep: impl Fn(&str) -> bool) -> Vec<(String, Vec<u8>)> {
    (editor.model().assets().iter())
        .filter(|(path, _)| keep(path))
        .filter_map(|(path, source)| {
            let bytes = editor.shown_assets().get(source)?;
            Some((path.clone(), bytes.to_vec()))
        })
        .collect()
}

// Each wanted (part, picture name) decoded from the editor's added files in `top`, else from
// `files` (the HUD over the game); `None` for a picture neither has.
fn decode_layered(
    top: Vec<(String, Vec<u8>)>,
    files: &HudFiles,
    wanted: Vec<(PartKey, String)>,
    platform: Platform,
) -> Vec<(PartKey, String, Option<Arc<RenderImage>>)> {
    let top = HudFiles::from_memory(vec![top]);
    wanted
        .into_iter()
        .map(|(key, name)| {
            let image = decode_texture(&top, &name, platform)
                .or_else(|| decode_texture(files, &name, platform))
                .map(|(_, image)| image);
            (key, name, image)
        })
        .collect()
}

// A texture decoded, with the pixels kept for cutting icons out of a sheet.
type Decoded = Option<(Arc<RgbaImage>, Arc<RenderImage>)>;

// `name`'s texture as pixels and as a render image; `None` when it is missing or unreadable.
fn decode_texture(files: &HudFiles, name: &str, platform: Platform) -> Decoded {
    let Ok(HudImage::Found(texture)) = files.image(name, platform) else {
        return None;
    };
    let image = texture.image();
    let render = to_render_image(image.width(), image.height(), image.pixels().to_vec())?;
    Some((Arc::new(image.clone()), render))
}

// The main menu's background names for a narrow and a wide screen; an unreadable list is none.
fn menu_backgrounds(files: &HudFiles, platform: Platform) -> MenuBackgrounds {
    [false, true].map(|wide| files.menu_background(wide, platform).ok().flatten())
}

// The icon's texel rectangle out of its sheet; `None` when the rectangle leaves the sheet.
fn crop(sheet: &RgbaImage, icon: &KillIcon) -> Option<RgbaImage> {
    let right = icon.x().checked_add(icon.wide())?;
    let bottom = icon.y().checked_add(icon.tall())?;
    if right > sheet.width() || bottom > sheet.height() {
        return None;
    }
    let (stride, row_bytes) = (sheet.width() as usize * 4, icon.wide() as usize * 4);
    let mut pixels = Vec::with_capacity(row_bytes * icon.tall() as usize);
    for row in icon.y()..bottom {
        let start = row as usize * stride + icon.x() as usize * 4;
        pixels.extend_from_slice(sheet.pixels().get(start..start + row_bytes)?);
    }
    RgbaImage::from_pixels(icon.wide(), icon.tall(), pixels)
}

impl Render for HudMakerPage {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        for image in self.tints.get_mut().take_stale() {
            cx.drop_image(image, Some(window));
        }
        self.preview_part_drag();
        self.tick_timeline(window, cx);
        self.read_menu_art(cx);
        self.reload_switches(cx);
        // While the open toolbar shows, toasts stand above its home at the bottom instead of
        // over its buttons, even while it has settled elsewhere; leaving the page puts them
        // back through the shell.
        let bar = self.toolbar_bounds.get();
        let ready = matches!(self.load, Load::Ready(_));
        let lift = (ready && !self.toolbar_collapsed && bar.size.height > px(0.)).then(|| {
            let top = self.area.get().origin.y + self.toolbar_home().y;
            window.viewport_size().height - top + TOAST_GAP
        });
        if lift != self.toast_lift {
            self.toast_lift = lift;
            lift_toasts(lift, cx);
        }
        // A new area, or a panel or toolbar that grew or moved (rows appearing, a resized
        // window, a dock), settles again, except mid-drag, where the player places by hand.
        let panels = Panel::ALL.map(|p| self.panels[p as usize].bounds.get());
        let titles = Panel::ALL.map(|p| self.panels[p as usize].title_height.get());
        let now = (
            self.area.get(),
            panels,
            self.toolbar_bounds.get(),
            titles,
            self.killfeed_band(),
            self.chat_band(),
            self.drawn_bounds.borrow().clone(),
        );
        let dragging = self.panels.iter().any(|state| state.drag.is_some());
        if now != self.checked && !dragging {
            self.checked = now;
            self.relayouts = self.relayouts.saturating_add(1);
            self.keep_clear();
        }
        let body = match self.load {
            Load::Reading => labelled(
                div().text_color(cx.theme().muted_foreground),
                ids::HUD_MAKER_LOADING,
                LOADING,
            )
            .into_any_element(),
            Load::NoGame => self.no_game(cx),
            Load::CantOpen(ref line) => v_flex()
                .flex_1()
                .items_center()
                .justify_center()
                .child(labelled(
                    div().text_color(cx.theme().muted_foreground),
                    ids::HUD_MAKER_CANT_OPEN,
                    line.clone(),
                ))
                .into_any_element(),
            Load::Ready(_) => self.editor_layout(cx),
        };
        v_flex()
            .id(ids::page(Page::HudMaker))
            .test_support()
            .key_context(HUD_MAKER_CONTEXT)
            .track_focus(&self.focus)
            // A key in the page makes the next edit the keyboard's, which holds no panel. A bound
            // key never reaches key listeners, so its edit actions let go themselves.
            .capture_key_down(cx.listener(|page, _, _, cx| page.let_go(cx)))
            .on_key_down(cx.listener(|page, ev, _, cx| page.space_down(ev, cx)))
            .on_key_up(cx.listener(|page, ev, _, cx| {
                page.nudge_refused = None;
                page.space_up(ev, cx);
            }))
            .on_action(cx.listener(|page, _: &ZoomInHud, _, cx| page.zoom_canvas(1, cx)))
            .on_action(cx.listener(|page, _: &ZoomOutHud, _, cx| page.zoom_canvas(-1, cx)))
            .on_action(cx.listener(|page, _: &ZoomFitHud, _, cx| page.fit_canvas(cx)))
            .on_action(cx.listener(|page, _: &UndoHudChange, window, cx| {
                page.let_go(cx);
                page.undo(window, cx)
            }))
            .on_action(cx.listener(|page, _: &RedoHudChange, window, cx| {
                page.let_go(cx);
                page.redo(window, cx)
            }))
            .on_action(cx.listener(|p, _: &NudgeLeft, w, cx| p.nudge(-1, 0, w, cx)))
            .on_action(cx.listener(|p, _: &NudgeRight, w, cx| p.nudge(1, 0, w, cx)))
            .on_action(cx.listener(|p, _: &NudgeUp, w, cx| p.nudge(0, -1, w, cx)))
            .on_action(cx.listener(|p, _: &NudgeDown, w, cx| p.nudge(0, 1, w, cx)))
            .on_action(cx.listener(|p, _: &NudgeLeftFar, w, cx| p.nudge(-NUDGE_FAR, 0, w, cx)))
            .on_action(cx.listener(|p, _: &NudgeRightFar, w, cx| p.nudge(NUDGE_FAR, 0, w, cx)))
            .on_action(cx.listener(|p, _: &NudgeUpFar, w, cx| p.nudge(0, -NUDGE_FAR, w, cx)))
            .on_action(cx.listener(|p, _: &NudgeDownFar, w, cx| p.nudge(0, NUDGE_FAR, w, cx)))
            .on_action(
                cx.listener(|page, _: &CancelHudDrag, window, cx| {
                    page.cancel_part_drag(window, cx)
                }),
            )
            .on_action(
                cx.listener(|p, _: &ToggleHudPartHidden, w, cx| p.hide_selected(true, w, cx)),
            )
            .on_action(cx.listener(|p, _: &HideHudPart, w, cx| p.hide_selected(false, w, cx)))
            .on_action(cx.listener(|p, _: &ShowHudShortcuts, w, cx| p.show_shortcuts(w, cx)))
            .size_full()
            .when(!ready, |el| el.p(CONTENT_PADDING).gap_4())
            .child(body)
    }
}

// Why a nudge of `part` that changed nothing, ending in `result`, moved nothing, in player
// words; `None` when there is nothing to say.
fn why_no_nudge(
    model: &HudModel,
    part: &PartKey,
    result: Result<(), EditRefused>,
    dx: i32,
    dy: i32,
) -> Option<&'static str> {
    let caps = model.capabilities(part)?;
    match result {
        Err(EditRefused::NotOffered) => Some(NUDGE_GAME_PLACES),
        Err(EditRefused::ReadOnly) if model.part(part)?.is_from_base() => Some(NUDGE_FROM_BASE),
        Err(EditRefused::ReadOnly) if !model.part(part)?.is_readable() => Some(NUDGE_UNREADABLE),
        Err(EditRefused::ReadOnly) => Some(NUDGE_ODD_PLACE),
        Err(_) => Some(NUDGE_REFUSED),
        Ok(()) if dx != 0 && !caps.move_x() => Some(NUDGE_SIDEWAYS_SET),
        Ok(()) if dy != 0 && !caps.move_y() => Some(NUDGE_UP_DOWN_SET),
        Ok(()) => None,
    }
}

// The union of the lines that drew with some size; `None` when none did.
fn band(lines: &[Bounds<Pixels>]) -> Option<Bounds<Pixels>> {
    let mut drawn = lines
        .iter()
        .filter(|b| b.size.width > px(0.) && b.size.height > px(0.));
    let first = *drawn.next()?;
    Some(drawn.fold(first, |band, line| band.union(line)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_font_file_listed_many_times_is_read_once() {
        let paths: Vec<String> = (0..200)
            .map(|i| {
                if i % 2 == 0 {
                    "resource/Big.ttf"
                } else {
                    "RESOURCE/big.TTF"
                }
                .into()
            })
            .collect();
        let mut reads = 0;
        let fonts = read_listed_fonts(&paths, |_| {
            reads += 1;
            Some(vec![0; 10])
        });
        assert_eq!((reads, fonts.len()), (1, 1));
    }

    #[test]
    fn font_files_past_the_count_or_byte_budget_are_skipped() {
        let many: Vec<String> = (0..200).map(|i| format!("resource/f{i}.ttf")).collect();
        let fonts = read_listed_fonts(&many, |_| Some(vec![0; 10]));
        assert_eq!(fonts.len(), MAX_FONT_FILES);
        let huge = MAX_FONT_BYTES as usize;
        let fonts = read_listed_fonts(&many[..4], |_| Some(vec![0; huge]));
        let total: usize = fonts.iter().map(Vec::len).sum();
        assert!(total as u64 <= MAX_FONT_TOTAL, "{total} bytes kept");
        assert_eq!(fonts.len(), 2);
    }

    #[test]
    fn reading_stops_once_the_byte_budget_is_spent() {
        let many: Vec<String> = (0..10).map(|i| format!("resource/f{i}.ttf")).collect();
        let mut reads = 0;
        let fonts = read_listed_fonts(&many, |_| {
            reads += 1;
            Some(vec![0; MAX_FONT_BYTES as usize])
        });
        assert_eq!((reads, fonts.len()), (2, 2));
    }

    #[test]
    fn the_same_font_bytes_register_once() {
        let mut seen = RegisteredFonts::default();
        let key = RegisteredFonts::key;
        assert!(seen.first_time(key(b"font one")));
        assert!(!seen.first_time(key(b"font one")));
        assert!(seen.first_time(key(b"font two")));
    }
}
