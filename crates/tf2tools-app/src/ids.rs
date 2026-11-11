//! The pilot id contract: every stable element id the app draws, so gpui-pilot and the
//! headless tests find elements by name rather than by tree position.

use gpui_kit::ElementId;
use tf2tools_core::{
    AddonName, BoardMode, GbId, HudId, LogsRange, MatrixKind, MeterKind, ModuleChoice, NewPart,
    PartKey, PinCorner, Preset, ProfileId, Scene, SceneRule, Screen, SituationSwitch, SoundKind,
    SteamId, StoreItemId, StoreKind, Team, TfClass, WeaponKind, WeaponSlot,
};

use crate::shell::route::Page;
use crate::sync::store_slug;

pub const SIDEBAR: &str = "sidebar";
pub const SIDEBAR_TITLE: &str = "sidebar-title";
pub const SIDEBAR_TOGGLE: &str = "sidebar-toggle";
/// The sidebar's nav groups between the title and the Settings/Logs footer; scrolls when the
/// window is short.
pub const SIDEBAR_GROUPS: &str = "sidebar-groups";
pub const HEADER: &str = "header";
pub const HEADER_TITLE: &str = "header-title";
/// The header's own row, left of any window buttons the title bar draws on this platform.
pub const HEADER_ROW: &str = "header-row";
pub const TF2_BADGE: &str = "tf2-badge";
/// The header's "Play TF2" button, shown while TF2 is found; disabled while TF2 runs.
pub const PLAY_TF2: &str = "play-tf2";
/// One per running kind except Addons, labelled "Syncing {store}: {fetched} / {total}" or "Syncing Curated HUDs...".
pub const SYNC_RING: &str = "sync-ring";
pub const TOAST: &str = "toast";
/// The update banner under the header, while an update is offered, downloading, ready or installed.
pub const UPDATE_BANNER: &str = "update-banner";
/// Labelled with the banner's line.
pub const UPDATE_BANNER_TEXT: &str = "update-banner-text";
/// Labelled "{n}%" while downloading.
pub const UPDATE_BANNER_PERCENT: &str = "update-banner-percent";
/// "Update", "Open release page" or "Restart".
pub const UPDATE_BANNER_ACTION: &str = "update-banner-action";
pub const UPDATE_BANNER_DISMISS: &str = "update-banner-dismiss";
/// The crash banner under the header, on the start after a crash.
pub const CRASH_BANNER: &str = "crash-banner";
/// Labelled "TF2Tools closed unexpectedly last time."
pub const CRASH_BANNER_TEXT: &str = "crash-banner-text";
/// "Copy details".
pub const CRASH_BANNER_COPY: &str = "crash-banner-copy";
pub const CRASH_BANNER_DISMISS: &str = "crash-banner-dismiss";
/// The crash banner's "Report a problem" button.
pub const CRASH_BANNER_REPORT: &str = "crash-banner-report";
/// The banner offering to remove TF2Tools 4, on Windows when it is still installed.
pub const OLD_V4_BANNER: &str = "old-v4-banner";
/// Labelled "The old version of TF2Tools is still installed."
pub const OLD_V4_BANNER_TEXT: &str = "old-v4-banner-text";
/// "Remove it": runs TF2Tools 4's own uninstaller.
pub const OLD_V4_BANNER_REMOVE: &str = "old-v4-banner-remove";
/// "Not now".
pub const OLD_V4_BANNER_LATER: &str = "old-v4-banner-later";
pub const CONFIRM_DIALOG: &str = "confirm-dialog";
pub const CONFIRM_TITLE: &str = "confirm-title";
pub const CONFIRM_DESCRIPTION: &str = "confirm-description";
pub const CONFIRM_OK: &str = "confirm-ok";
pub const CONFIRM_CANCEL: &str = "confirm-cancel";
/// The leave dialog's "Save and leave", offered when the open HUD can be saved.
pub const CONFIRM_SAVE: &str = "confirm-save";
pub const EXTERNAL_HUDS_DIALOG: &str = "external-huds-dialog";
pub const EXTERNAL_HUDS_TITLE: &str = "external-huds-title";
pub const EXTERNAL_HUDS_DESCRIPTION: &str = "external-huds-description";
pub const EXTERNAL_HUDS_KEEP: &str = "external-huds-keep";
pub const EXTERNAL_HUDS_DISCARD: &str = "external-huds-discard";
pub const EXTERNAL_HUDS_CANCEL: &str = "external-huds-cancel";
pub const STARTUP_LOADING: &str = "startup-loading";
pub const STARTUP_ERROR: &str = "startup-error";
pub const PLACEHOLDER_MESSAGE: &str = "placeholder-message";
/// Labelled "Current version: v{CARGO_PKG_VERSION}".
pub const SETTINGS_VERSION: &str = "settings-version";
/// Labelled with the App Updates card's status line.
pub const SETTINGS_UPDATE_STATUS: &str = "settings-update-status";
/// "Check for Updates".
pub const SETTINGS_UPDATE_CHECK: &str = "settings-update-check";
/// "Download & Install" or "Open release page".
pub const SETTINGS_UPDATE_INSTALL: &str = "settings-update-install";
/// "Restart Now".
pub const SETTINGS_UPDATE_RESTART: &str = "settings-update-restart";
/// Wraps the TF2 path input.
pub const SETTINGS_PATH: &str = "settings-path";
pub const SETTINGS_BROWSE: &str = "settings-browse";
pub const SETTINGS_DETECT: &str = "settings-detect";
/// Wraps the install select; present only when Auto-detect found 2 or more installs.
pub const SETTINGS_CANDIDATES: &str = "settings-candidates";
/// Wraps the volume slider.
pub const SETTINGS_VOLUME: &str = "settings-volume";
/// Labelled "Volume: {n}%".
pub const SETTINGS_VOLUME_LABEL: &str = "settings-volume-label";
/// Wraps the cache duration slider.
pub const SETTINGS_CACHE: &str = "settings-cache";
/// Labelled "Cache duration: {format_hours}".
pub const SETTINGS_CACHE_LABEL: &str = "settings-cache-label";
pub const SETTINGS_SAVE: &str = "settings-save";
/// A store's status line: "{n} items cached. Last synced: {when}", plus ". Sync failed: {error}" while failed.
pub const SETTINGS_SYNC: &str = "settings-sync";
/// A store's "Resync" button, "Syncing..." and disabled while that store syncs.
pub const SETTINGS_RESYNC: &str = "settings-resync";
/// The Licences card's "Show notices" / "Hide notices" button.
pub const SETTINGS_NOTICES: &str = "settings-notices";
/// The scrollable box of third-party notices, present while shown.
pub const SETTINGS_NOTICES_VIEW: &str = "settings-notices-view";
/// The notices box's scrollbar, over its right edge.
pub const SETTINGS_NOTICES_SCROLLBAR: &str = "settings-notices-scrollbar";
/// The Help and feedback card's "Report a problem" button.
pub const SETTINGS_REPORT_PROBLEM: &str = "settings-report-problem";
/// The Help and feedback card's "Help" button.
pub const SETTINGS_HELP: &str = "settings-help";
/// The conflicts box, labelled "Conflicts Detected. " and one line per conflict.
pub const SOUNDS_CONFLICTS: &str = "sounds-conflicts";
/// Wraps the library search input.
pub const SOUNDS_SEARCH: &str = "sounds-search";
/// Labelled "No sounds in your library yet".
pub const SOUNDS_EMPTY: &str = "sounds-empty";
/// Labelled "No sounds match your search".
pub const SOUNDS_NO_MATCH: &str = "sounds-no-match";
/// The "Add Sound" button, "Importing..." while busy; opens the sound file picker.
pub const SOUNDS_ADD: &str = "sounds-add";
/// The Sounds page's "TF2 not found" notice, present only while TF2 is not found.
pub const SOUNDS_NO_TF2: &str = "sounds-no-tf2";
/// The notice's "Open Settings" button.
pub const SOUNDS_NO_TF2_SETTINGS: &str = "sounds-no-tf2-settings";
/// The Sounds page's card grid.
pub const SOUNDS_GRID: &str = "sounds-grid";
/// The box the Sounds page's grid fills, under the library search.
pub const SOUNDS_LIBRARY: &str = "sounds-library";
/// Wraps the picker's search input.
pub const CROSSHAIRS_SEARCH: &str = "crosshairs-search";
/// The picker's virtualized list.
pub const CROSSHAIRS_LIST: &str = "crosshairs-list";
/// The crosshair list's scrollbar, over its right edge.
pub const CROSSHAIRS_LIST_SCROLLBAR: &str = "crosshairs-list-scrollbar";
/// The weapon assigner's list of the class's weapon rows.
pub const WEAPON_LIST: &str = "weapon-list";
/// The compact weapon list's scrollbar, over its right edge.
pub const WEAPON_LIST_SCROLLBAR: &str = "weapon-list-scrollbar";
/// Labelled "Built-in (n)", the count after filtering.
pub const CROSSHAIRS_GROUP_BUILTIN: &str = "crosshairs-group-builtin";
/// Labelled "My Crosshairs (n)", the count after filtering.
pub const CROSSHAIRS_GROUP_LIBRARY: &str = "crosshairs-group-library";
/// Labelled "No crosshairs match your search".
pub const CROSSHAIRS_NO_MATCH: &str = "crosshairs-no-match";
/// The selection's swatch, labelled with its name; absent without a selection.
pub const CROSSHAIRS_SWATCH: &str = "crosshairs-swatch";
/// Wraps the background Select, labelled with the background's name.
pub const CROSSHAIRS_BACKGROUND: &str = "crosshairs-background";
/// The composite preview, labelled "No crosshair" or "{name} on {background}".
pub const CROSSHAIRS_PREVIEW: &str = "crosshairs-preview";
/// The selected crosshair inside the preview, drawn at its in-game size.
pub const CROSSHAIRS_PREVIEW_CROSSHAIR: &str = "crosshairs-preview-crosshair";
/// The "Import" button, "Importing..." while busy; opens the crosshair image picker.
pub const CROSSHAIRS_IMPORT: &str = "crosshairs-import";
/// The Crosshairs page's "TF2 not found" notice, present only while TF2 is not found.
pub const CROSSHAIRS_NO_TF2: &str = "crosshairs-no-tf2";
/// The notice's "Open Settings" button.
pub const CROSSHAIRS_NO_TF2_SETTINGS: &str = "crosshairs-no-tf2-settings";
/// The "Install to TF2" button, "Installing..." while busy.
pub const CROSSHAIRS_INSTALL: &str = "crosshairs-install";
/// The "Uninstall" button, "Removing..." while busy.
pub const CROSSHAIRS_UNINSTALL: &str = "crosshairs-uninstall";
/// The rival weapon scripts box, labelled "Conflicts Detected"; present only with conflicts.
pub const CROSSHAIRS_CONFLICTS: &str = "crosshairs-conflicts";
/// The "All {Class}" bulk button, assigning the selection to every weapon of the class.
pub const CROSSHAIRS_BULK_ALL: &str = "crosshairs-bulk-all";
/// The "Primary" bulk button.
pub const CROSSHAIRS_BULK_PRIMARY: &str = "crosshairs-bulk-primary";
/// The "Secondary" bulk button.
pub const CROSSHAIRS_BULK_SECONDARY: &str = "crosshairs-bulk-secondary";
/// The "Melee" bulk button.
pub const CROSSHAIRS_BULK_MELEE: &str = "crosshairs-bulk-melee";
/// The gallery's "All" source chip, showing both groups.
pub const CROSSHAIRS_GROUP_ALL: &str = "crosshairs-group-all";
/// The status bar over the page: the waiting line and the install buttons.
pub const CROSSHAIRS_STATUS: &str = "crosshairs-status";
/// The status bar line: "N changes not in TF2 yet", or what TF2 shows now.
pub const CROSSHAIRS_WAITING: &str = "crosshairs-waiting";
/// The loadout board of class columns and slot rows.
pub const CROSSHAIRS_BOARD: &str = "crosshairs-board";
/// The board's "Everything" corner button, assigning every weapon.
pub const CROSSHAIRS_BOARD_EVERYTHING: &str = "crosshairs-board-everything";
/// The per-weapon panel of one board cell, labelled "{Class} {slot} weapons".
pub const CROSSHAIRS_DETAILS: &str = "crosshairs-details";
/// The per-weapon panel's "Reset to default" button.
pub const CROSSHAIRS_DETAILS_RESET: &str = "crosshairs-details-reset";
/// The per-weapon panel's "Close" button.
pub const CROSSHAIRS_DETAILS_CLOSE: &str = "crosshairs-details-close";
/// The selected crosshair's name beside the large preview.
pub const CROSSHAIRS_SELECTED_NAME: &str = "crosshairs-selected-name";
/// The "Loading the default HUD..." line while the game's files are read.
pub const HUD_MAKER_LOADING: &str = "hud-maker-loading";
/// The view shown without TF2's HUD files, labelled with its sentence.
pub const HUD_MAKER_NO_GAME: &str = "hud-maker-no-game";
/// The no-game view's "Settings" button.
pub const HUD_MAKER_OPEN_SETTINGS: &str = "hud-maker-open-settings";
/// The strip at the HUD Maker's top holding the scene tabs.
pub const HUD_MAKER_SCENES: &str = "hud-maker-scenes";
/// The scene tabs' button group.
pub const HUD_MAKER_SCENE_TABS: &str = "hud-maker-scene-tabs";
/// The scene menu that replaces the tabs when they do not fit.
pub const HUD_MAKER_SCENE_MENU: &str = "hud-maker-scene-menu";
/// The Scoreboard tab's match mode dropdown, labelled with the open mode.
pub const HUD_MAKER_BOARD_MODE: &str = "hud-maker-board-mode";
/// The situation menu's Minimal HUD switch, selected while the maker shows TF2's minimal HUD.
pub const HUD_MAKER_MINMODE: &str = "hud-maker-situation-switch:minimal-hud";
/// The scene strip's "Minimal HUD" chip on tabs without the situation menu, while it is on;
/// clicking it turns the minimal HUD off.
pub const HUD_MAKER_MINMODE_CHIP: &str = "hud-maker-minmode-chip";
/// The scene strip's note, while the minimal HUD is on, when the scene has no minimal-HUD keys.
pub const HUD_MAKER_MINMODE_NOTE: &str = "hud-maker-minmode-note";
/// The note over the canvas when a scene cannot show or the HUD hides it.
pub const HUD_MAKER_SCENE_NOTE: &str = "hud-maker-scene-note";
/// The free area beside the part list that the screen is fitted into.
pub const HUD_MAKER_CANVAS_AREA: &str = "hud-maker-canvas-area";
/// The screen rectangle; a click on it selects the part under the pointer.
pub const HUD_MAKER_CANVAS: &str = "hud-maker-canvas";
/// The crosshair marker at the middle of the screen.
pub const HUD_MAKER_CROSSHAIR: &str = "hud-maker-crosshair";
/// The HUD's own on-screen crosshair label, drawn on the canvas while the HUD shows it.
pub const HUD_MAKER_HUD_CROSSHAIR: &str = "hud-maker-hud-crosshair";
/// The note that the HUD crosshair is apart from the Crosshairs page.
pub const HUD_MAKER_HUD_CROSSHAIR_NOTE: &str = "hud-maker-hud-crosshair-note";
/// The warning that the HUD crosshair and weapon crosshairs both show.
pub const HUD_MAKER_HUD_CROSSHAIR_WARNING: &str = "hud-maker-hud-crosshair-warning";
/// The outline over the selected part; present only with a drawn selection.
pub const HUD_MAKER_SELECTION: &str = "hud-maker-selection";
/// The box drawn by dragging on empty HUD Maker canvas, which selects the parts inside it.
pub const HUD_MAKER_MARQUEE: &str = "hud-maker-marquee";
/// The side panel's line saying how many parts are selected together.
pub const HUD_MAKER_SEVERAL: &str = "hud-maker-several";
/// The side panel's Bring forward button in the Layer row.
pub const HUD_MAKER_FORWARD: &str = "hud-maker-forward";
/// The side panel's Send back button in the Layer row.
pub const HUD_MAKER_BACKWARD: &str = "hud-maker-backward";
/// The outline over the part under the pointer, on the canvas or its row; absent for a hidden
/// part.
pub const HUD_MAKER_HOVER: &str = "hud-maker-hover";
/// The name tag beside the hover outline, labelled with the part's name.
pub const HUD_MAKER_HOVER_NAME: &str = "hud-maker-hover-name";
/// The vertical guide line drawn while a dragged part snaps across.
pub const HUD_MAKER_GUIDE_X: &str = "hud-maker-guide-x";
/// The horizontal guide line drawn while a dragged part snaps up or down.
pub const HUD_MAKER_GUIDE_Y: &str = "hud-maker-guide-y";
/// The side panel's Mirror button in the Position row.
pub const HUD_MAKER_MIRROR: &str = "hud-maker-mirror";
/// The side panel's Match size button in the Size row.
pub const HUD_MAKER_MATCH_SIZE: &str = "hud-maker-match-size";
/// The floating toolbar over the canvas area, open or collapsed to one button.
pub const HUD_MAKER_TOOLBAR: &str = "hud-maker-toolbar";
/// The open toolbar's button that collapses it.
pub const HUD_MAKER_TOOLBAR_COLLAPSE: &str = "hud-maker-toolbar-collapse";
/// The collapsed toolbar's one button, which opens it again.
pub const HUD_MAKER_TOOLBAR_EXPAND: &str = "hud-maker-toolbar-expand";
/// The view the screen sits in on the canvas, which clips it when zoomed and takes the pans.
pub const HUD_MAKER_CANVAS_VIEW: &str = "hud-maker-canvas-view";
/// The toolbar's Zoom out button for the canvas.
pub const HUD_MAKER_ZOOM_OUT: &str = "hud-maker-zoom-out";
/// The canvas's zoom level beside the zoom buttons, labelled "100%" and so on.
pub const HUD_MAKER_ZOOM: &str = "hud-maker-zoom";
/// The toolbar's Zoom in button for the canvas.
pub const HUD_MAKER_ZOOM_IN: &str = "hud-maker-zoom-in";
/// The toolbar's Fit button, which fits the screen to the canvas again.
pub const HUD_MAKER_ZOOM_FIT: &str = "hud-maker-zoom-fit";
/// The scrolling part list.
pub const HUD_MAKER_PARTS: &str = "hud-maker-parts";
/// The part list's scrollbar, over its right edge.
pub const HUD_MAKER_PARTS_SCROLLBAR: &str = "hud-maker-parts-scrollbar";
/// The box that narrows the HUD Maker's part list by name.
pub const HUD_MAKER_PARTS_FILTER: &str = "hud-maker-parts-filter";
/// The note in the part list when no part's name holds the filter.
pub const HUD_MAKER_PARTS_NO_MATCH: &str = "hud-maker-parts-no-match";
/// The tooltip on a part list row's name, shown while the pointer rests on it.
pub const HUD_MAKER_ROW_TIP: &str = "hud-maker-row-tip";
/// The tooltip naming an alignment button's place, shown while the pointer rests on it.
pub const HUD_MAKER_ALIGN_TIP: &str = "hud-maker-align-tip";
/// The tooltip of any other HUD Maker panel button, shown while the pointer rests on it.
pub const HUD_MAKER_TIP: &str = "hud-maker-tip";
/// The parts panel holding the part list, floating over the canvas area or docked.
pub const HUD_MAKER_PARTS_PANEL: &str = "hud-maker-parts-panel";
/// The parts panel's title, the grip that drags it while floating.
pub const HUD_MAKER_PARTS_PANEL_TITLE: &str = "hud-maker-parts-panel-title";
/// The parts panel's "Dock left" button.
pub const HUD_MAKER_PARTS_PANEL_DOCK_LEFT: &str = "hud-maker-parts-panel-dock-left";
/// The parts panel's "Dock right" button.
pub const HUD_MAKER_PARTS_PANEL_DOCK_RIGHT: &str = "hud-maker-parts-panel-dock-right";
/// The parts panel's "Float" button.
pub const HUD_MAKER_PARTS_PANEL_FLOAT: &str = "hud-maker-parts-panel-float";
/// The parts panel's collapse and expand button.
pub const HUD_MAKER_PARTS_PANEL_COLLAPSE: &str = "hud-maker-parts-panel-collapse";
/// The side panel, floating over the canvas area or docked inside one of its edges.
pub const HUD_MAKER_PANEL: &str = "hud-maker-panel";
/// The side panel's title, the grip that drags a floating panel.
pub const HUD_MAKER_PANEL_TITLE: &str = "hud-maker-panel-title";
/// The side panel's rows under the title bar; absent while collapsed.
pub const HUD_MAKER_PANEL_BODY: &str = "hud-maker-panel-body";
/// The side panel body's scrollbar, over its right edge.
pub const HUD_MAKER_PANEL_BODY_SCROLLBAR: &str = "hud-maker-panel-body-scrollbar";
/// The side panel's "Dock left" button.
pub const HUD_MAKER_PANEL_DOCK_LEFT: &str = "hud-maker-panel-dock-left";
/// The side panel's "Dock right" button.
pub const HUD_MAKER_PANEL_DOCK_RIGHT: &str = "hud-maker-panel-dock-right";
/// The side panel's "Float" button.
pub const HUD_MAKER_PANEL_FLOAT: &str = "hud-maker-panel-float";
/// The side panel's collapse and expand button.
pub const HUD_MAKER_PANEL_COLLAPSE: &str = "hud-maker-panel-collapse";
/// The Picture row's "Choose picture…" button.
pub const HUD_MAKER_PICTURE_PICK: &str = "hud-maker-picture-pick";
/// The Options panel's menu background "Choose picture…" button, on the Main menu.
pub const HUD_MAKER_MENU_PICTURE_PICK: &str = "hud-maker-menu-picture-pick";
/// The menu background's Reset, shown once a background is picked.
pub const HUD_MAKER_MENU_PICTURE_RESET: &str = "hud-maker-menu-picture-reset";
/// The Picture row's Reset.
pub const HUD_MAKER_PICTURE_RESET: &str = "hud-maker-picture-reset";
/// The Picture row's note that a game-set picture changes everywhere the game shows it.
pub const HUD_MAKER_PICTURE_SHARED: &str = "hud-maker-picture-shared";
/// The side panel's "Click a part of the HUD to change it." line, shown with nothing selected.
pub const HUD_MAKER_PANEL_EMPTY: &str = "hud-maker-panel-empty";
/// The HUD's options, the side panel's body while no part is selected.
pub const HUD_MAKER_OPTIONS: &str = "hud-maker-options";
/// The Options panel's "Reset all options" button, which asks before dropping every choice.
pub const HUD_MAKER_OPTIONS_RESET_ALL: &str = "hud-maker-options-reset-all";
/// The Options panel's section of a copy's own options, under "<HUD>'s options".
pub const HUD_MAKER_HUD_OPTIONS: &str = "hud-maker-hud-options";
/// The line saying how many of a copy's options don't fit its version, labelled with it.
pub const HUD_MAKER_HUD_OPTIONS_MISFITS: &str = "hud-maker-hud-options-misfits";
/// A selected part's section of the copy's own options that change it.
pub const HUD_MAKER_PART_OPTIONS: &str = "hud-maker-part-options";
/// The Options panel's Transparent viewmodels switch, or the note saying where to get the addon.
pub const HUD_MAKER_VIEWMODELS: &str = "hud-maker-viewmodels";
/// The selected part's name in the side panel's title bar.
pub const HUD_MAKER_PANEL_PART: &str = "hud-maker-panel-part";
/// When the selected part shows in game, from its panel's rule.
pub const HUD_MAKER_PANEL_SHOWS: &str = "hud-maker-panel-shows";
/// The side panel's note for a part whose text the game writes itself, under its Text box.
pub const HUD_MAKER_PANEL_CODE_TEXT: &str = "hud-maker-panel-code-text";
/// The side panel's note for a part drawn from a file the HUD includes, which cannot be edited.
pub const HUD_MAKER_PANEL_FROM_BASE: &str = "hud-maker-panel-from-base";
/// The second note when some file behind a from-base part was not found.
pub const HUD_MAKER_PANEL_BASE_MISSING: &str = "hud-maker-panel-base-missing";
/// The side panel's line for a part whose files could not be read.
pub const HUD_MAKER_PANEL_UNREADABLE: &str = "hud-maker-panel-unreadable";
/// The Position row's X field, in screen pixels.
pub const HUD_MAKER_X: &str = "hud-maker-x";
/// The Position row's Y field, in screen pixels.
pub const HUD_MAKER_Y: &str = "hud-maker-y";
/// The Size row's width field, in screen pixels.
pub const HUD_MAKER_WIDE: &str = "hud-maker-wide";
/// The Size row's height field, in screen pixels.
pub const HUD_MAKER_TALL: &str = "hud-maker-tall";
/// The Layer row's field.
pub const HUD_MAKER_LAYER: &str = "hud-maker-layer";
/// The Hide row's switch; on while the player hid the part.
pub const HUD_MAKER_HIDE: &str = "hud-maker-hide";
/// The Text row's input.
pub const HUD_MAKER_TEXT: &str = "hud-maker-text";
/// The side panel's "All settings" button, which opens and closes the list of every setting.
pub const HUD_MAKER_ALL_SETTINGS: &str = "hud-maker-all-settings";
/// The one input under "All settings", holding the value of the setting last pressed.
pub const HUD_MAKER_SETTING_INPUT: &str = "hud-maker-setting-input";
/// The active Colour row's picker; the colour slot's while no other row is active.
pub const HUD_MAKER_COLOUR: &str = "hud-maker-colour";
/// The active Colour row's "Named colours" button, which opens the list of the HUD's colour names.
pub const HUD_MAKER_COLOUR_NAME: &str = "hud-maker-colour-name";
/// The scrolling list of the HUD's colour names under the active Colour row.
pub const HUD_MAKER_COLOUR_NAMES: &str = "hud-maker-colour-names";
/// The scrollbar beside the named colours list.
pub const HUD_MAKER_COLOUR_NAMES_SCROLLBAR: &str = "hud-maker-colour-names-scrollbar";
/// The HUD Maker toolbar's Undo button.
pub const HUD_MAKER_UNDO: &str = "hud-maker-undo";
/// The HUD Maker toolbar's Redo button.
pub const HUD_MAKER_REDO: &str = "hud-maker-redo";
/// The HUD Maker's name field.
pub const HUD_MAKER_NAME: &str = "hud-maker-name";
/// The line under the HUD Maker's name field naming a taken name.
pub const HUD_MAKER_NAME_ERROR: &str = "hud-maker-name-error";
/// The HUD Maker's Save button.
pub const HUD_MAKER_SAVE: &str = "hud-maker-save";
/// The HUD Maker's Save as new button.
pub const HUD_MAKER_SAVE_AS_NEW: &str = "hud-maker-save-as-new";
/// The HUD Maker's Install button, enabled for a saved, unchanged HUD.
pub const HUD_MAKER_INSTALL: &str = "hud-maker-install";
/// The HUD Maker toolbar's note after a Save updated the installed HUD.
pub const HUD_MAKER_LIVE_NOTE: &str = "hud-maker-live-note";
/// The live note's Copy button, which copies the console command.
pub const HUD_MAKER_LIVE_COPY: &str = "hud-maker-live-copy";
/// The live note's close button.
pub const HUD_MAKER_LIVE_CLOSE: &str = "hud-maker-live-close";
/// The line shown in place of the canvas when a made HUD can't be reopened.
pub const HUD_MAKER_CANT_OPEN: &str = "hud-maker-cant-open";
/// The Size row's note "Hidden" while the player hid the part by size zero.
pub const HUD_MAKER_SIZE_HIDDEN: &str = "hud-maker-size-hidden";
/// The More menu's "History" entry.
pub const HUD_MAKER_HISTORY_OPEN: &str = "hud-maker-history-open";
/// The History section of the side panel.
pub const HUD_MAKER_HISTORY: &str = "hud-maker-history";
/// The History section's "Back to the HUD" button.
pub const HUD_MAKER_HISTORY_CLOSE: &str = "hud-maker-history-close";
/// The note History shows for a HUD that was never saved.
pub const HUD_MAKER_HISTORY_UNSAVED: &str = "hud-maker-history-unsaved";
/// The note History shows when the HUD keeps no snapshots yet.
pub const HUD_MAKER_HISTORY_EMPTY: &str = "hud-maker-history-empty";
/// The History section's snapshot name field.
pub const HUD_MAKER_SNAPSHOT_NAME: &str = "hud-maker-snapshot-name";
/// The History section's "Save snapshot" button.
pub const HUD_MAKER_SNAPSHOT_SAVE: &str = "hud-maker-snapshot-save";
/// The More menu's "Borrow a part" entry.
pub const HUD_MAKER_BORROW_OPEN: &str = "hud-maker-borrow-open";
/// The Borrow a part section of the side panel.
pub const HUD_MAKER_BORROW: &str = "hud-maker-borrow";
/// The Borrow a part section's "Back to the HUD" button.
pub const HUD_MAKER_BORROW_CLOSE: &str = "hud-maker-borrow-close";
/// The note Borrow a part shows when no other library HUD has parts to borrow.
pub const HUD_MAKER_BORROW_EMPTY: &str = "hud-maker-borrow-empty";
/// The chosen HUD's parts list in the Borrow a part section.
pub const HUD_MAKER_BORROW_PARTS: &str = "hud-maker-borrow-parts";
/// The scrollbar over the Borrow a part parts list.
pub const HUD_MAKER_BORROW_PARTS_SCROLLBAR: &str = "hud-maker-borrow-parts-scrollbar";
/// The credit line under a previewed borrow.
pub const HUD_MAKER_BORROW_CREDIT: &str = "hud-maker-borrow-credit";
/// The list of what comes with a previewed borrow and what stays behind.
pub const HUD_MAKER_BORROW_COMES_WITH: &str = "hud-maker-borrow-comes-with";
/// The layer of the canvas showing the HUD as it is now, right of the comparison's split.
pub const HUD_MAKER_COMPARE_NOW: &str = "hud-maker-compare-now";
/// The layer of the canvas showing the compared snapshot, left of the split.
pub const HUD_MAKER_COMPARE_BEFORE: &str = "hud-maker-compare-before";
/// The 2 px line at the comparison's split.
pub const HUD_MAKER_COMPARE_DIVIDER: &str = "hud-maker-compare-divider";
/// The grip on the divider that moves the split.
pub const HUD_MAKER_COMPARE_HANDLE: &str = "hud-maker-compare-handle";
/// The "Stop comparing" chip at the comparison's top-left.
pub const HUD_MAKER_COMPARE_STOP: &str = "hud-maker-compare-stop";
/// The caption naming the snapshot, top-left of the comparison.
pub const HUD_MAKER_COMPARE_LABEL_BEFORE: &str = "hud-maker-compare-label-before";
/// The caption "Now", top-right of the comparison.
pub const HUD_MAKER_COMPARE_LABEL_NOW: &str = "hud-maker-compare-label-now";
/// The Presets section's heading.
pub const HUD_MAKER_PRESETS: &str = "hud-maker-presets";
/// The Presets section's name field.
pub const HUD_MAKER_PRESET_NAME: &str = "hud-maker-preset-name";
/// The Presets section's "Save as preset" button.
pub const HUD_MAKER_PRESET_SAVE: &str = "hud-maker-preset-save";
/// The Recipes section's heading.
pub const HUD_MAKER_RECIPES: &str = "hud-maker-recipes";
/// The Recipes section's "Share as recipe" button, saving the HUD's changes to a file.
pub const HUD_MAKER_RECIPE_EXPORT: &str = "hud-maker-recipe-export";
/// The Recipes section's "Use a recipe" button, previewing a recipe file.
pub const HUD_MAKER_RECIPE_IMPORT: &str = "hud-maker-recipe-import";
/// The preview bar's line: "Previewing <name>." and what was left out.
pub const HUD_MAKER_PREVIEW: &str = "hud-maker-preview";
/// The preview bar's Apply button, keeping the preview as one undo step.
pub const HUD_MAKER_PREVIEW_APPLY: &str = "hud-maker-preview-apply";
/// The preview bar's Cancel button.
pub const HUD_MAKER_PREVIEW_CANCEL: &str = "hud-maker-preview-cancel";
/// The overflow menu's "Start from a ready-made HUD" entry, live only on a new unedited HUD.
pub const HUD_MAKER_START: &str = "hud-maker-start";
/// The note under an off "Start from a ready-made HUD" saying why it is off.
pub const HUD_MAKER_START_WHY: &str = "hud-maker-start-why";
/// The toolbar's overflow menu button.
pub const HUD_MAKER_MORE: &str = "hud-maker-more";
/// The card over a new, unchanged HUD offering where to start.
pub const HUD_MAKER_START_CARD: &str = "hud-maker-start-card";
/// The Start from card's "A ready-made HUD", which lists the starting points in the card.
pub const HUD_MAKER_START_READY: &str = "hud-maker-start-ready";
/// The Start from card's "One of my HUDs", which goes to the HUDs library.
pub const HUD_MAKER_START_MINE: &str = "hud-maker-start-mine";
/// The Start from card's "Blank", which keeps TF2's own HUD and closes the card.
pub const HUD_MAKER_START_BLANK: &str = "hud-maker-start-blank";
/// The "?" beside the HUD Maker's title, which opens the keyboard shortcuts sheet.
pub const HUD_MAKER_SHORTCUTS: &str = "hud-maker-shortcuts";
/// The keyboard shortcuts sheet.
pub const HUD_MAKER_SHORTCUTS_SHEET: &str = "hud-maker-shortcuts-sheet";
/// The keyboard shortcuts sheet's Close button.
pub const HUD_MAKER_SHORTCUTS_CLOSE: &str = "hud-maker-shortcuts-close";
/// The toolbar's screen shape button, which opens the shapes.
pub const HUD_MAKER_SCREEN_MENU: &str = "hud-maker-screen-menu";
/// The zero-size holder of the open option picker's trigger, under its swatch.
pub const HUD_MAKER_OPTION_PICKER: &str = "hud-maker-option-picker";
/// The overflow menu's "Every screen at once" entry.
pub const HUD_MAKER_EVERY_SCREEN: &str = "hud-maker-every-screen";
/// The overflow menu's "Put a screenshot behind the HUD" entry.
pub const HUD_MAKER_BACKDROP: &str = "hud-maker-backdrop";
/// The overflow menu's "Remove the screenshot" entry, shown only while one is set.
pub const HUD_MAKER_BACKDROP_CLEAR: &str = "hud-maker-backdrop-clear";
/// The screenshot drawn behind the HUD, stretched over the screen.
pub const HUD_MAKER_BACKDROP_IMAGE: &str = "hud-maker-backdrop-image";
/// The game's picture behind the main menu scene.
pub const HUD_MAKER_MENU_BACKGROUND: &str = "hud-maker-menu-background";
/// The line shown over the main menu screen while its picture is being read.
pub const HUD_MAKER_MENU_LOADING: &str = "hud-maker-menu-loading";
/// The badge over the canvas naming the colour-blind view it shows.
pub const HUD_MAKER_VISION_BADGE: &str = "hud-maker-vision-badge";
/// The six-screen view's grid of tiles.
pub const HUD_MAKER_SCREENS: &str = "hud-maker-screens";
/// The six-screen view's "Back to one screen" button.
pub const HUD_MAKER_SCREENS_BACK: &str = "hud-maker-screens-back";
/// The painting grid; exposes its cell size so a test can turn a cell into a position.
pub const CROSSHAIR_MAKER_GRID: &str = "crosshair-maker-grid";
/// The Pencil tool button.
pub const CROSSHAIR_MAKER_PENCIL: &str = "crosshair-maker-pencil";
/// The Eraser tool button.
pub const CROSSHAIR_MAKER_ERASER: &str = "crosshair-maker-eraser";
/// The Fill tool button.
pub const CROSSHAIR_MAKER_FILL: &str = "crosshair-maker-fill";
/// The Line tool button.
pub const CROSSHAIR_MAKER_LINE: &str = "crosshair-maker-line";
/// The Rectangle tool button.
pub const CROSSHAIR_MAKER_RECTANGLE: &str = "crosshair-maker-rectangle";
/// The Circle tool button.
pub const CROSSHAIR_MAKER_CIRCLE: &str = "crosshair-maker-circle";
/// The Filled shapes toggle, labelled "Filled shapes"; disabled unless Rectangle or Circle is picked.
pub const CROSSHAIR_MAKER_FILLED: &str = "crosshair-maker-filled";
/// The left-right mirror toggle.
pub const CROSSHAIR_MAKER_MIRROR_X: &str = "crosshair-maker-mirror-x";
/// The top-bottom mirror toggle.
pub const CROSSHAIR_MAKER_MIRROR_Y: &str = "crosshair-maker-mirror-y";
/// The Undo button.
pub const CROSSHAIR_MAKER_UNDO: &str = "crosshair-maker-undo";
/// The Redo button.
pub const CROSSHAIR_MAKER_REDO: &str = "crosshair-maker-redo";
/// The Clear button.
pub const CROSSHAIR_MAKER_CLEAR: &str = "crosshair-maker-clear";
/// The New button, starting an empty 64x64 drawing.
pub const CROSSHAIR_MAKER_NEW: &str = "crosshair-maker-new";
/// The size button, labelled with the current size, switching to the other.
pub const CROSSHAIR_MAKER_SIZE: &str = "crosshair-maker-size";
/// Wraps the paint colour picker.
pub const CROSSHAIR_MAKER_COLOR: &str = "crosshair-maker-color";
/// The line under the colour, labelled with the hint it shows.
pub const CROSSHAIR_MAKER_COLOR_HINT: &str = "crosshair-maker-color-hint";
/// Wraps the preview tint colour picker.
pub const CROSSHAIR_MAKER_PREVIEW_COLOR: &str = "crosshair-maker-preview-color";
/// The tinted preview over the chosen background.
pub const CROSSHAIR_MAKER_PREVIEW: &str = "crosshair-maker-preview";
/// The drawing inside the maker's preview, drawn at its in-game size.
pub const CROSSHAIR_MAKER_PREVIEW_CROSSHAIR: &str = "crosshair-maker-preview-crosshair";
/// Wraps the preview background Select.
pub const CROSSHAIR_MAKER_BACKGROUND: &str = "crosshair-maker-background";
/// Wraps the crosshair name input.
pub const CROSSHAIR_MAKER_NAME: &str = "crosshair-maker-name";
/// The name-taken line under the name, labelled with its text; present only after a refusal.
pub const CROSSHAIR_MAKER_NAME_ERROR: &str = "crosshair-maker-name-error";
/// The Save button.
pub const CROSSHAIR_MAKER_SAVE: &str = "crosshair-maker-save";
/// The Save as new button beside Save, labelled "Save as new".
pub const CROSSHAIR_MAKER_SAVE_AS_NEW: &str = "crosshair-maker-save-as-new";
/// The centre guides toggle, labelled "Centre guides".
pub const CROSSHAIR_MAKER_GUIDES: &str = "crosshair-maker-guides";
/// The vertical centre guide line over the grid.
pub const CROSSHAIR_MAKER_GUIDE_V: &str = "crosshair-maker-guide-v";
/// The horizontal centre guide line over the grid.
pub const CROSSHAIR_MAKER_GUIDE_H: &str = "crosshair-maker-guide-h";
/// The hovered cell's position readout, labelled with its text; present only while hovering.
pub const CROSSHAIR_MAKER_POSITION: &str = "crosshair-maker-position";
/// The grid-lines toggle, labelled "Grid lines".
pub const CROSSHAIR_MAKER_GRID_LINES: &str = "crosshair-maker-grid-lines";
/// The "Add outline" button.
pub const CROSSHAIR_MAKER_OUTLINE: &str = "crosshair-maker-outline";
/// Wraps the outline colour picker.
pub const CROSSHAIR_MAKER_OUTLINE_COLOR: &str = "crosshair-maker-outline-color";
/// The clipping view the grid scrolls in; takes the wheel and right or middle drags.
pub const CROSSHAIR_MAKER_VIEW: &str = "crosshair-maker-view";
/// The maker's side panel, which scrolls when the window is too short for it.
pub const CROSSHAIR_MAKER_PANEL: &str = "crosshair-maker-panel";
/// The side panel's scrollbar, over its right edge.
pub const CROSSHAIR_MAKER_PANEL_SCROLLBAR: &str = "crosshair-maker-panel-scrollbar";
/// The zoom label button, labelled "100%" and so on; a click fits the grid.
pub const CROSSHAIR_MAKER_ZOOM: &str = "crosshair-maker-zoom";
/// The Zoom in button.
pub const CROSSHAIR_MAKER_ZOOM_IN: &str = "crosshair-maker-zoom-in";
/// The Zoom out button.
pub const CROSSHAIR_MAKER_ZOOM_OUT: &str = "crosshair-maker-zoom-out";
/// Wraps the opacity slider; its tooltip is `model::OPACITY_TIP`.
pub const CROSSHAIR_MAKER_OPACITY: &str = "crosshair-maker-opacity";
/// The "Opacity: 100%" label above the slider.
pub const CROSSHAIR_MAKER_OPACITY_LABEL: &str = "crosshair-maker-opacity-label";
/// The toolbar's Starters button, which opens the starter shapes dialog.
pub const CROSSHAIR_MAKER_STARTERS: &str = "crosshair-maker-starters";
/// The toolbar's Import PNG button, which opens a PNG as a new drawing.
pub const CROSSHAIR_MAKER_IMPORT_PNG: &str = "crosshair-maker-import-png";
/// The overlay that shows while a file is dragged over the maker, labelled with what to drop.
pub const CROSSHAIR_MAKER_DROP_TARGET: &str = "crosshair-maker-drop-target";
/// The starter shapes dialog's content.
pub const CROSSHAIR_MAKER_STARTER_DIALOG: &str = "crosshair-maker-starter-dialog";
/// The dialog's live preview, labelled "<shape> preview".
pub const CROSSHAIR_MAKER_STARTER_PREVIEW: &str = "crosshair-maker-starter-preview";
/// The drawing inside the starter preview's frame.
pub const CROSSHAIR_MAKER_STARTER_PICTURE: &str = "crosshair-maker-starter-picture";
/// The dialog's Cancel button.
pub const CROSSHAIR_MAKER_STARTER_CANCEL: &str = "crosshair-maker-starter-cancel";
/// The starter dialog's Use button.
pub const CROSSHAIR_MAKER_STARTER_USE: &str = "crosshair-maker-starter-use";
/// The Crosshairs page's Edit button; present only when a painted crosshair is selected.
pub const CROSSHAIRS_EDIT: &str = "crosshairs-edit";
/// The Crosshairs page's Delete button; present only when a library crosshair is selected.
pub const CROSSHAIRS_DELETE: &str = "crosshairs-delete";
/// The Crosshairs page's "Copy to Crosshair Maker" button; present while a crosshair is selected.
pub const CROSSHAIRS_COPY_TO_MAKER: &str = "crosshairs-copy-to-maker";
/// The Crosshairs page's Export PNG button; present while a crosshair is selected.
pub const CROSSHAIRS_EXPORT_PNG: &str = "crosshairs-export-png";
/// The installed HUD card, or the empty card when none is installed.
pub const HUDS_INSTALLED: &str = "huds-installed";
/// Labelled "No HUD installed".
pub const HUDS_INSTALLED_EMPTY: &str = "huds-installed-empty";
/// Labelled with the installed HUD's name.
pub const HUDS_INSTALLED_NAME: &str = "huds-installed-name";
/// Labelled "Managed" or "External".
pub const HUDS_INSTALLED_BADGE: &str = "huds-installed-badge";
/// Labelled "Active in TF2." or "Added to TF2 outside TF2Tools.".
pub const HUDS_INSTALLED_PATH: &str = "huds-installed-path";
/// The installed card's "Uninstall" button; removes the HUD from tf/custom.
pub const HUDS_UNINSTALL: &str = "huds-uninstall";
/// The installed card's "Edit" button; opens the HUD, or a copy of it, in the HUD Maker.
pub const HUDS_INSTALLED_EDIT: &str = "huds-installed-edit";
/// The warning shown when more than one HUD sits in tf/custom.
pub const HUDS_CONFLICTS: &str = "huds-conflicts";
/// Wraps the library search input.
pub const HUDS_SEARCH: &str = "huds-search";
/// The "Add HUD" button; opens the archive picker.
pub const HUDS_ADD: &str = "huds-add";
/// The HUDs page's "TF2 not found" notice, present only while TF2 is not found.
pub const HUDS_NO_TF2: &str = "huds-no-tf2";
/// The notice's "Open Settings" button.
pub const HUDS_NO_TF2_SETTINGS: &str = "huds-no-tf2-settings";
/// Labelled "Your library is empty".
pub const HUDS_EMPTY: &str = "huds-empty";
/// Labelled "No HUDs found", shown when a search matches nothing.
pub const HUDS_NO_MATCH: &str = "huds-no-match";
/// The HUDs page's card grid.
pub const HUDS_GRID: &str = "huds-grid";
/// The box the HUDs page's grid fills, under the "Your Library" row.
pub const HUDS_LIBRARY: &str = "huds-library";

/// The floating sound player card.
pub const SOUND_PLAYER: &str = "sound-player";
/// The player's stop button; present only while a sound plays.
pub const SOUND_PLAYER_STOP: &str = "sound-player-stop";
/// Labelled with the playing sound's name.
pub const SOUND_PLAYER_NAME: &str = "sound-player-name";
/// Labelled with the playing sound's subtitle.
pub const SOUND_PLAYER_SUBTITLE: &str = "sound-player-subtitle";
/// Wraps the player's volume slider.
pub const SOUND_PLAYER_VOLUME: &str = "sound-player-volume";
/// Labelled "{n}", the player's volume.
pub const SOUND_PLAYER_VOLUME_VALUE: &str = "sound-player-volume-value";
/// Wraps a store grid's search input.
pub const STORE_SEARCH: &str = "store-search";
/// Wraps a store grid's sort select: "Most Liked", "Most Viewed" or "Newest".
pub const STORE_SORT: &str = "store-sort";
/// The "Sort:" label before a store's sort select.
pub const STORE_SORT_LABEL: &str = "store-sort-label";
/// Labelled "{shown} of {total} {noun}", once both counts are read.
pub const STORE_COUNT: &str = "store-count";
/// A store grid's virtualized card list.
pub const STORE_GRID: &str = "store-grid";
/// A store grid's inline error line; present only after a failed query.
pub const STORE_ERROR: &str = "store-error";
/// The inline error's "Retry" button.
pub const STORE_RETRY: &str = "store-retry";
/// The prefix of a store card's id; see `store_card`.
pub const STORE_CARD: &str = "store-card";
/// The progress bar under a downloading item's label; present only while it downloads.
pub const STORE_PROGRESS: &str = "store-progress";
/// A card's download progress bar, inside `STORE_PROGRESS`.
pub const STORE_PROGRESS_BAR: &str = "store-progress-bar";
/// A GameBanana card's click target, inside the grid's `store_card` id.
pub const GAMEBANANA_CARD: &str = "gamebanana-card";
/// A carousel's "previous" button; present only with two or more images.
pub const CAROUSEL_PREV: &str = "carousel-prev";
/// A carousel's "next" button; present only with two or more images.
pub const CAROUSEL_NEXT: &str = "carousel-next";
/// A carousel's 16:9 image area, present whenever it has images.
pub const CAROUSEL_IMAGE: &str = "carousel-image";
/// Labelled "No screenshots" when a carousel has no images.
pub const CAROUSEL_EMPTY: &str = "carousel-empty";
/// A carousel's "3 / 26" position, labelled with that text; present only with two or more images.
pub const CAROUSEL_POSITION: &str = "carousel-position";
/// The screenshot lightbox dialog's content.
pub const LIGHTBOX: &str = "lightbox";
/// A store item's detail dialog content.
pub const STORE_DETAIL: &str = "store-detail";
/// The detail dialog's scrolling part under its header: files, carousel and body.
pub const STORE_DETAIL_BODY: &str = "store-detail-body";
/// The scrollbar beside `STORE_DETAIL_BODY`.
pub const STORE_DETAIL_SCROLLBAR: &str = "store-detail-scrollbar";
/// The detail dialog's "Downloaded" (curated) or "In Library" badge; present only when owned.
pub const STORE_DETAIL_BADGE: &str = "store-detail-badge";
/// The detail dialog's progress, labelled with the progress text; present only while the item downloads.
pub const STORE_DETAIL_PROGRESS: &str = "store-detail-progress";
/// The detail dialog's download progress bar, inside `STORE_DETAIL_PROGRESS`.
pub const STORE_DETAIL_BAR: &str = "store-detail-bar";
/// The detail dialog's file listing error, labelled with the message.
pub const STORE_FILES_ERROR: &str = "store-files-error";
/// The detail dialog's Retry button after a failed file listing.
pub const STORE_FILES_RETRY: &str = "store-files-retry";
/// The HUD Store's tab button group.
pub const HUD_STORE_TABS: &str = "hud-store-tabs";
/// The HUD Store's "Curated" tab button.
pub const HUD_STORE_TAB_CURATED: &str = "hud-store-tab-curated";
/// The HUD Store's "GameBanana" tab button.
pub const HUD_STORE_TAB_GAMEBANANA: &str = "hud-store-tab-gamebanana";
/// The curated tab's "Hide prereleases" checkbox.
pub const STORE_FILTER_PRERELEASE: &str = "store-filter-prerelease";
/// The curated tab's "Clear filters" button.
pub const STORE_FILTER_CLEAR: &str = "store-filter-clear";
/// The curated tab's "More filters" button; it shows or hides the facet chip rows.
pub const STORE_FILTER_MORE: &str = "store-filter-more";
/// The curated OS filter's "OS:" label.
pub const STORE_FILTER_OS_LABEL: &str = "store-filter-os-label";
/// The curated OS filter's joined toggle group; its items keep `store_filter_os(os.key())`.
pub const STORE_FILTER_OS_GROUP: &str = "store-filter-os-group";
/// A store's no-match card title, such as "No HUDs match your filters" on the curated tab
/// or "No sounds match your filters" when a store grid's search or filter finds nothing.
pub const STORE_EMPTY: &str = "store-empty";
/// Labelled "Syncing HUDs..." while the curated store is empty and its sync runs.
pub const STORE_SYNCING: &str = "store-syncing";
/// Labelled "Could not load store" when the curated store is empty and no sync runs, and
/// "Could not load {noun}" on a GameBanana store grid whose first sync failed.
pub const STORE_UNLOADED: &str = "store-unloaded";
/// The "Retry" button on a GameBanana store grid's "Could not load" card; it syncs again.
pub const STORE_UNLOADED_RETRY: &str = "store-unloaded-retry";
/// A GameBanana store's sync banner (sounds, crosshairs, GameBanana HUDs); present only while that store syncs.
pub const SYNC_BANNER: &str = "sync-banner";
/// The sync banner's "{fetched} / {total}" count.
pub const SYNC_BANNER_COUNT: &str = "sync-banner-count";
/// The Sound Store detail dialog's "Play Preview" / "Stop Preview" button.
pub const STORE_PREVIEW: &str = "store-preview";
/// A store's sync button, labelled "Refresh", or "Syncing..." while that store syncs.
pub const STORE_REFRESH: &str = "store-refresh";
/// The Addon Store header badge, labelled with the store's release tag.
pub const ADDON_STORE_TAG: &str = "addon-store-tag";
/// The Addon Store's info box about how addons install.
pub const ADDON_STORE_INFO: &str = "addon-store-info";
/// The Addon Store info line's clickable text, whose underlined ranges open their links.
pub const ADDON_STORE_INFO_TEXT: &str = "addon-store-info-text";
/// The Addon Store's "TF2 not found" warning, present only without a TF2 install.
pub const ADDON_STORE_NO_TF2: &str = "addon-store-no-tf2";
/// The Addon Store's tab strip: mastercomfig, then one tab per GameBanana section. The
/// sections wrap to a second line when the page is narrower than the strip.
pub const ADDON_STORE_TABS: &str = "addon-store-tabs";
/// The mastercomfig tab's group inside `ADDON_STORE_TABS`.
pub const ADDON_STORE_TAB_GROUP: &str = "addon-store-tab-group";
/// The GameBanana sections' joined group of tabs inside `ADDON_STORE_TABS`.
pub const ADDON_STORE_SECTION_TABS: &str = "addon-store-section-tabs";
/// Wraps a GameBanana add-on tab's search input.
pub const ADDON_SEARCH: &str = "addon-search";
/// Wraps a GameBanana add-on tab's sort select.
pub const ADDON_SORT: &str = "addon-sort";
/// The "Sort:" label before a GameBanana add-on tab's sort select.
pub const ADDON_SORT_LABEL: &str = "addon-sort-label";
/// Wraps a GameBanana add-on tab's subcategory select.
pub const ADDON_CATEGORY: &str = "addon-category";
/// Wraps the weapon select, shown only for a Skins class.
pub const ADDON_WEAPON: &str = "addon-weapon";
/// A GameBanana add-on tab's card grid.
pub const ADDON_GRID: &str = "addon-grid";
/// A GameBanana add-on tab's loading line, present while a step runs.
pub const ADDON_LOADING: &str = "addon-loading";
/// A GameBanana add-on tab's "no add-ons found" line.
pub const ADDON_EMPTY: &str = "addon-empty";
/// The "Load more" button shown after steps keep adding no cards.
pub const ADDON_LOAD_MORE: &str = "addon-load-more";
/// The GameBanana add-on detail dialog.
pub const ADDON_DETAIL: &str = "addon-detail";
/// The add-on detail's name, labelled with it.
pub const ADDON_DETAIL_NAME: &str = "addon-detail-name";
/// The add-on detail's download count.
pub const ADDON_DETAIL_DOWNLOADS: &str = "addon-detail-downloads";
/// The add-on detail's mature ratings line, present only for a mature add-on.
pub const ADDON_DETAIL_RATINGS: &str = "addon-detail-ratings";
/// The add-on detail's description.
pub const ADDON_DETAIL_DESCRIPTION: &str = "addon-detail-description";
/// The add-on detail's load failure text.
pub const ADDON_DETAIL_ERROR: &str = "addon-detail-error";
/// Retries a failed add-on detail load.
pub const ADDON_DETAIL_RETRY: &str = "addon-detail-retry";
/// The add-on detail's "Outdated" badge.
pub const ADDON_DETAIL_OUTDATED: &str = "addon-detail-outdated";
/// The add-on detail's scrolled body under its header.
pub const ADDON_DETAIL_BODY: &str = "addon-detail-body";
/// The scrollbar beside `ADDON_DETAIL_BODY`.
pub const ADDON_DETAIL_SCROLLBAR: &str = "addon-detail-scrollbar";
/// The add-on detail's Install, Update, "Install again" or "Installed" button.
pub const ADDON_DETAIL_INSTALL: &str = "addon-detail-install";
/// The add-on detail's download progress, labelled with its text.
pub const ADDON_DETAIL_PROGRESS: &str = "addon-detail-progress";
/// The add-on detail's download progress bar.
pub const ADDON_DETAIL_BAR: &str = "addon-detail-bar";
/// Why the add-on detail offers no install, in its place.
pub const ADDON_DETAIL_BLOCKED: &str = "addon-detail-blocked";
/// The Settings "Show mature add-ons" switch.
pub const SETTINGS_MATURE_ADDONS: &str = "settings-mature-addons";
/// Wraps the Activity Log's category select.
pub const LOGS_CATEGORY: &str = "logs-category";
/// Wraps the Activity Log's level select.
pub const LOGS_LEVEL: &str = "logs-level";
/// The Activity Log's "Clear All" button.
pub const LOGS_CLEAR: &str = "logs-clear";
/// The Activity Log's row list.
pub const LOGS_LIST: &str = "logs-list";
/// The log list's scrollbar, in a gutter just right of the rows.
pub const LOGS_LIST_SCROLLBAR: &str = "logs-list-scrollbar";
/// Labelled "No log entries found" when the filtered log is empty.
pub const LOGS_EMPTY: &str = "logs-empty";
/// The text of a failed log load.
pub const LOGS_ERROR: &str = "logs-error";
/// Retries a failed log load.
pub const LOGS_RETRY: &str = "logs-retry";
/// The list's last item, labelled "Load more", or "Loading..." while a page loads.
pub const LOGS_MORE: &str = "logs-more";
/// The Addons page's "TF2 not found" warning, present only without a TF2 install.
pub const ADDONS_NO_TF2: &str = "addons-no-tf2";
/// The Addons page's count line under the title.
pub const ADDONS_COUNT: &str = "addons-count";
/// The Addons header's "Browse Addon Store" button.
pub const ADDONS_BROWSE: &str = "addons-browse";
/// The Addons page's empty state, present only with an empty library.
pub const ADDONS_EMPTY: &str = "addons-empty";
/// The empty state's "Browse Addon Store" button.
pub const ADDONS_EMPTY_BROWSE: &str = "addons-empty-browse";
/// The "Installed Addons" section title.
pub const ADDONS_INSTALLED: &str = "addons-installed";
/// Labelled "No addons installed" when nothing in the library is installed.
pub const ADDONS_INSTALLED_EMPTY: &str = "addons-installed-empty";
/// The "Your Library" section title.
pub const ADDONS_LIBRARY: &str = "addons-library";
/// The Addons page's "GameBanana add-ons" section title.
pub const ADDONS_GB: &str = "addons-gb";
/// The Graphics page's line under the title.
pub const GRAPHICS_SUBTITLE: &str = "graphics-subtitle";
/// The Graphics page's "TF2 not found" warning, present only without a TF2 install.
pub const GRAPHICS_NO_TF2: &str = "graphics-no-tf2";
/// The mastercomfig status card.
pub const GRAPHICS_STATUS: &str = "graphics-status";
/// The status card's title line, labelled with its text.
pub const GRAPHICS_HEADLINE: &str = "graphics-headline";
/// The line under the status card's title, labelled with its text; absent when it has none.
pub const GRAPHICS_DETAIL: &str = "graphics-detail";
/// The status card's credit to the mastercomfig team and comfig-app.
pub const GRAPHICS_CREDIT: &str = "graphics-credit";
/// Install, shown when mastercomfig is not in tf/custom.
pub const GRAPHICS_INSTALL: &str = "graphics-install";
/// Update, shown for a managed install behind the recorded release.
pub const GRAPHICS_UPDATE: &str = "graphics-update";
/// Check for updates, shown for a managed install.
pub const GRAPHICS_CHECK: &str = "graphics-check";
/// Remove, shown for a managed install.
pub const GRAPHICS_REMOVE: &str = "graphics-remove";
/// Take over, shown for an install TF2Tools did not make.
pub const GRAPHICS_TAKE_OVER: &str = "graphics-take-over";
/// "Changes apply the next time TF2 starts.", under the preset cards.
pub const GRAPHICS_FOOTER: &str = "graphics-footer";
/// The settings section under the preset cards, present for a managed install.
pub const GRAPHICS_SETTINGS: &str = "graphics-settings";
/// The note shown in place of the settings list, labelled with its text.
pub const GRAPHICS_SETTINGS_NOTE: &str = "graphics-settings-note";
/// "Reset all": drops every setting back to the preset.
pub const GRAPHICS_RESET_ALL: &str = "graphics-reset-all";

/// The "No TF2 installation found" alert on the Profiles page.
pub const PROFILES_NO_TF2: &str = "profiles-no-tf2";
/// The Active Profile card.
pub const PROFILES_ACTIVE: &str = "profiles-active";
/// The active profile's name.
pub const PROFILES_ACTIVE_NAME: &str = "profiles-active-name";
/// The Active Profile card's "Active" badge.
pub const PROFILES_ACTIVE_BADGE: &str = "profiles-active-badge";
/// The active profile's HUD badge.
pub const PROFILES_ACTIVE_HUD: &str = "profiles-active-hud";
/// The active profile's hitsound badge.
pub const PROFILES_ACTIVE_HITSOUND: &str = "profiles-active-hitsound";
/// The active profile's killsound badge.
pub const PROFILES_ACTIVE_KILLSOUND: &str = "profiles-active-killsound";
/// The active profile's crosshair count badge.
pub const PROFILES_ACTIVE_CROSSHAIRS: &str = "profiles-active-crosshairs";
/// "Empty profile" on the Active Profile card.
pub const PROFILES_ACTIVE_EMPTY: &str = "profiles-active-empty";
/// The card shown when no profile is active.
pub const PROFILES_NO_ACTIVE: &str = "profiles-no-active";
/// The Profiles page's card grid.
pub const PROFILES_GRID: &str = "profiles-grid";
/// The box the Profiles page's grid fills, under the "Your Profiles" row.
pub const PROFILES_LIBRARY: &str = "profiles-library";
/// The scrolling list of missing items in the profile apply dialog.
pub const PROFILES_MISSING_LIST: &str = "profiles-missing-list";
/// The scrollbar beside the Missing Items list.
pub const PROFILES_MISSING_LIST_SCROLLBAR: &str = "profiles-missing-list-scrollbar";
/// The profile search input.
pub const PROFILES_SEARCH: &str = "profiles-search";
/// The empty state shown when there are no profiles.
pub const PROFILES_EMPTY: &str = "profiles-empty";
/// Shown when the search matches no profile.
pub const PROFILES_NO_MATCH: &str = "profiles-no-match";
/// The Missing Items dialog.
pub const PROFILES_MISSING_DIALOG: &str = "profiles-missing-dialog";
/// The Missing Items dialog's title, "Missing Items".
pub const PROFILES_MISSING_TITLE: &str = "profiles-missing-title";
/// The Missing Items dialog's description.
pub const PROFILES_MISSING_DESCRIPTION: &str = "profiles-missing-description";
/// The Missing Items dialog's Cancel button.
pub const PROFILES_MISSING_CANCEL: &str = "profiles-missing-cancel";
/// The Missing Items dialog's "Install Anyway (skip missing)" button.
pub const PROFILES_MISSING_INSTALL: &str = "profiles-missing-install";
/// The New profile main button, which saves the current setup as a new profile.
pub const PROFILES_NEW: &str = "profiles-new";
/// The New split button's wrapper; the caret inside it is the kit's fixed "popup" id.
pub const PROFILES_NEW_MENU: &str = "profiles-new-menu";
/// The New button and its caret, the kit's split button inside `PROFILES_NEW_MENU`.
pub const PROFILES_NEW_SPLIT: &str = "profiles-new-split";
/// The New menu's "Start empty" item.
pub const PROFILES_NEW_EMPTY: &str = "profiles-new-empty";
/// The empty page's New button.
pub const PROFILES_EMPTY_NEW: &str = "profiles-empty-new";
/// The empty page's "Start empty" button.
pub const PROFILES_EMPTY_START_EMPTY: &str = "profiles-empty-start-empty";
/// The profile editor's root.
pub const PROFILE_EDITOR: &str = "profile-editor";
/// The editor's Back button, which returns to the grid.
pub const PROFILE_EDITOR_BACK: &str = "profile-editor-back";
/// The editor's name field.
pub const PROFILE_EDITOR_NAME: &str = "profile-editor-name";
/// The line under the name field when another profile has that name.
pub const PROFILE_EDITOR_NAME_TAKEN: &str = "profile-editor-name-taken";
/// The editor's Activate button.
pub const PROFILE_EDITOR_ACTIVATE: &str = "profile-editor-activate";
/// The editor's "Active" badge, shown when the open profile is active.
pub const PROFILE_EDITOR_ACTIVE_BADGE: &str = "profile-editor-active-badge";
/// The editor's Delete button.
pub const PROFILE_EDITOR_DELETE: &str = "profile-editor-delete";
/// The note that an edit deactivated the profile, so its changes are not in the game yet.
pub const PROFILE_EDITOR_NOT_APPLIED: &str = "profile-editor-not-applied";
/// The HUD picker's card grid.
pub const PROFILE_PICKER_GRID: &str = "profile-picker-grid";
/// The HUD slot card's picture area, drawn while the slot holds a HUD.
pub const PROFILE_SLOT_HUD_SHOT: &str = "profile-slot-hud-shot";
/// The HUD slot card's picture, once it has loaded.
pub const PROFILE_SLOT_HUD_IMAGE: &str = "profile-slot-hud-image";
/// The sound picker's list.
pub const PROFILE_PICKER_SOUNDS: &str = "profile-picker-sounds";
/// The sound picker list's scrollbar.
pub const PROFILE_PICKER_SOUNDS_SCROLLBAR: &str = "profile-picker-sounds-scrollbar";

/// The sidebar item for `page`: "nav-<slug>".
pub fn nav(page: Page) -> ElementId {
    ElementId::Name(format!("nav-{}", page.slug()).into())
}

/// The root element of `page`: "page-<slug>".
pub fn page(page: Page) -> ElementId {
    ElementId::Name(format!("page-{}", page.slug()).into())
}

/// The scrollbar over `page`'s right edge, drawn while the page overflows:
/// "page-scrollbar-<slug>".
pub fn page_scrollbar(page: Page) -> ElementId {
    ElementId::Name(format!("page-scrollbar-{}", page.slug()).into())
}

/// The scrollbar of the card grid `grid`, in a gutter just right of its cards:
/// "grid-scrollbar:<grid>".
pub fn grid_scrollbar(grid: &str) -> ElementId {
    keyed("grid-scrollbar", grid)
}

/// A preset card on the Graphics page: "graphics-preset:<name>".
pub fn graphics_preset(preset: Preset) -> ElementId {
    keyed("graphics-preset", preset.name())
}

/// A settings-list category heading: "graphics-category:<key>".
pub fn graphics_category(key: &str) -> ElementId {
    keyed("graphics-category", key)
}

/// A settings row's title, labelled with its text: "graphics-module:<name>".
pub fn graphics_module(name: &str) -> ElementId {
    keyed("graphics-module", name)
}

/// A settings row's description: "graphics-module-description:<name>".
pub fn graphics_module_description(name: &str) -> ElementId {
    keyed("graphics-module-description", name)
}

/// A settings row's value Select: "graphics-module-select:<name>".
pub fn graphics_module_select(name: &str) -> ElementId {
    keyed("graphics-module-select", name)
}

/// A settings row's Reset: "graphics-module-reset:<name>".
pub fn graphics_module_reset(name: &str) -> ElementId {
    keyed("graphics-module-reset", name)
}

/// One entry of a row's Select: "graphics-value:<module>=<value>", empty for Game default.
pub fn graphics_value(module: &str, choice: &ModuleChoice) -> ElementId {
    let value = match choice {
        ModuleChoice::Value(v) => v.as_str(),
        ModuleChoice::NotSet | _ => "",
    };
    keyed("graphics-value", &format!("{module}={value}"))
}

/// An id for one of many like elements, carrying its stable key: "<kind>:<key>".
pub fn keyed(kind: &str, key: &str) -> ElementId {
    ElementId::Name(format!("{kind}:{key}").into())
}

/// One notice's heading in the notices box, labelled with its title: "settings-notice:<file>".
pub fn settings_notice(file: &str) -> ElementId {
    keyed("settings-notice", file)
}

/// An Activity Log row, keyed by its row id: "log-row:<id>".
pub fn log_row(id: i64) -> ElementId {
    keyed("log-row", &id.to_string())
}

/// A row's level badge, labelled with its level name: "log-level:<id>".
pub fn log_level(id: i64) -> ElementId {
    keyed("log-level", &id.to_string())
}

/// A row's relative time: "log-time:<id>".
pub fn log_time(id: i64) -> ElementId {
    keyed("log-time", &id.to_string())
}

/// A row's category badge: "log-category:<id>".
pub fn log_category(id: i64) -> ElementId {
    keyed("log-category", &id.to_string())
}

/// A row's action: "log-action:<id>".
pub fn log_action(id: i64) -> ElementId {
    keyed("log-action", &id.to_string())
}

/// A row's message: "log-message:<id>".
pub fn log_message(id: i64) -> ElementId {
    keyed("log-message", &id.to_string())
}

/// A row's "Details" toggle: "log-details-toggle:<id>".
pub fn log_details_toggle(id: i64) -> ElementId {
    keyed("log-details-toggle", &id.to_string())
}

/// A row's expanded details text: "log-details:<id>".
pub fn log_details(id: i64) -> ElementId {
    keyed("log-details", &id.to_string())
}

/// The scrollbar over a row's expanded details: "log-details-scrollbar:<id>".
pub fn log_details_scrollbar(id: i64) -> ElementId {
    keyed("log-details-scrollbar", &id.to_string())
}

// The kinds of the Addons page's per-card ids, each keyed by the addon's asset name.
const ADDON_CARD: &str = "addon-card";
const ADDON_CHECK: &str = "addon-check";
const ADDON_TAG: &str = "addon-tag";
const ADDON_DESCRIPTION: &str = "addon-description";
const ADDON_INSTALL: &str = "addon-install";
const ADDON_UNINSTALL: &str = "addon-uninstall";
const ADDON_UPDATE: &str = "addon-update";
const ADDON_DELETE: &str = "addon-delete";
const ADDON_BUSY: &str = "addon-busy";

/// A local addon card, keyed by its asset: "addon-card:<asset>".
pub fn addon_card(name: &AddonName) -> ElementId {
    keyed(ADDON_CARD, name.as_str())
}

/// A card's installed check mark: "addon-check:<asset>".
pub fn addon_check(name: &AddonName) -> ElementId {
    keyed(ADDON_CHECK, name.as_str())
}

/// A card's release tag badge: "addon-tag:<asset>".
pub fn addon_tag(name: &AddonName) -> ElementId {
    keyed(ADDON_TAG, name.as_str())
}

/// A card's description: "addon-description:<asset>".
pub fn addon_description(name: &AddonName) -> ElementId {
    keyed(ADDON_DESCRIPTION, name.as_str())
}

/// A card's Install button: "addon-install:<asset>".
pub fn addon_install(name: &AddonName) -> ElementId {
    keyed(ADDON_INSTALL, name.as_str())
}

/// A card's Uninstall button: "addon-uninstall:<asset>".
pub fn addon_uninstall(name: &AddonName) -> ElementId {
    keyed(ADDON_UNINSTALL, name.as_str())
}

/// A card's Update button: "addon-update:<asset>".
pub fn addon_update(name: &AddonName) -> ElementId {
    keyed(ADDON_UPDATE, name.as_str())
}

/// A card's Delete button: "addon-delete:<asset>".
pub fn addon_delete(name: &AddonName) -> ElementId {
    keyed(ADDON_DELETE, name.as_str())
}

/// A card's disabled "Downloading..." or "Installing..." button: "addon-busy:<asset>".
pub fn addon_busy(name: &AddonName) -> ElementId {
    keyed(ADDON_BUSY, name.as_str())
}

// The kinds of the Profiles page's per-card ids, each keyed by the profile id, and of the
// Missing Items dialog's per-row ids, keyed by the row's index.
const PROFILE_CARD: &str = "profile-card";
const PROFILE_ACTIVE_BADGE: &str = "profile-active-badge";
const PROFILE_HUD: &str = "profile-hud";
const PROFILE_HITSOUND: &str = "profile-hitsound";
const PROFILE_KILLSOUND: &str = "profile-killsound";
const PROFILE_CROSSHAIRS: &str = "profile-crosshairs";
const PROFILE_EMPTY: &str = "profile-empty";
const PROFILE_ACTIVATE: &str = "profile-activate";
const PROFILE_EDIT: &str = "profile-edit";
const PROFILE_DELETE: &str = "profile-delete";
const MISSING_ITEM: &str = "missing-item";
const MISSING_REDOWNLOAD: &str = "missing-redownload";
const MISSING_LOCAL: &str = "missing-local";

/// A profile card, keyed by the profile id: "profile-card:<id>".
pub fn profile_card(id: &ProfileId) -> ElementId {
    keyed(PROFILE_CARD, id.as_str())
}

/// A card's "Active" badge, keyed by the profile id: "profile-active-badge:<id>".
pub fn profile_active_badge(id: &ProfileId) -> ElementId {
    keyed(PROFILE_ACTIVE_BADGE, id.as_str())
}

/// A card's HUD badge, keyed by the profile id: "profile-hud:<id>".
pub fn profile_hud(id: &ProfileId) -> ElementId {
    keyed(PROFILE_HUD, id.as_str())
}

/// A card's hitsound badge, keyed by the profile id: "profile-hitsound:<id>".
pub fn profile_hitsound(id: &ProfileId) -> ElementId {
    keyed(PROFILE_HITSOUND, id.as_str())
}

/// A card's killsound badge, keyed by the profile id: "profile-killsound:<id>".
pub fn profile_killsound(id: &ProfileId) -> ElementId {
    keyed(PROFILE_KILLSOUND, id.as_str())
}

/// A card's crosshair count badge, keyed by the profile id: "profile-crosshairs:<id>".
pub fn profile_crosshairs(id: &ProfileId) -> ElementId {
    keyed(PROFILE_CROSSHAIRS, id.as_str())
}

/// A card's "Empty profile" text, keyed by the profile id: "profile-empty:<id>".
pub fn profile_empty(id: &ProfileId) -> ElementId {
    keyed(PROFILE_EMPTY, id.as_str())
}

/// A card's Activate button, keyed by the profile id: "profile-activate:<id>".
pub fn profile_activate(id: &ProfileId) -> ElementId {
    keyed(PROFILE_ACTIVATE, id.as_str())
}

/// A card's Edit button, keyed by the profile id: "profile-edit:<id>".
pub fn profile_edit(id: &ProfileId) -> ElementId {
    keyed(PROFILE_EDIT, id.as_str())
}

/// A card's Delete button, keyed by the profile id: "profile-delete:<id>".
pub fn profile_delete(id: &ProfileId) -> ElementId {
    keyed(PROFILE_DELETE, id.as_str())
}

/// An editor slot row, keyed by the slot: "profile-slot:<slot>".
pub fn profile_slot(slot: &str) -> ElementId {
    keyed("profile-slot", slot)
}

/// An editor slot's clear button: "profile-slot-clear:<slot>".
pub fn profile_slot_clear(slot: &str) -> ElementId {
    keyed("profile-slot-clear", slot)
}

/// A sound slot's play button: "profile-slot-play:<slot>".
pub fn profile_slot_play(slot: &str) -> ElementId {
    keyed("profile-slot-play", slot)
}

/// A slot's missing-item note: "profile-slot-missing:<slot>".
pub fn profile_slot_missing(slot: &str) -> ElementId {
    keyed("profile-slot-missing", slot)
}

/// A slot's picker: "profile-picker:<slot>".
pub fn profile_picker(slot: &str) -> ElementId {
    keyed("profile-picker", slot)
}

/// A picker's search field: "profile-picker-search:<slot>".
pub fn profile_picker_search(slot: &str) -> ElementId {
    keyed("profile-picker-search", slot)
}

/// A picker's empty-library note: "profile-picker-empty:<slot>".
pub fn profile_picker_empty(slot: &str) -> ElementId {
    keyed("profile-picker-empty", slot)
}

/// A picker's note that its search matches nothing: "profile-picker-no-match:<slot>".
pub fn profile_picker_no_match(slot: &str) -> ElementId {
    keyed("profile-picker-no-match", slot)
}

/// An empty picker's button to the store: "profile-picker-store:<slot>".
pub fn profile_picker_store(slot: &str) -> ElementId {
    keyed("profile-picker-store", slot)
}

/// A HUD card in the picker, keyed by folder: "profile-pick-hud:<folder>".
pub fn profile_pick_hud(folder: &str) -> ElementId {
    keyed("profile-pick-hud", folder)
}

/// A sound row in a sound slot's picker: "profile-pick-sound:<slot>:<folder>".
pub fn profile_pick_sound(slot: &str, folder: &str) -> ElementId {
    keyed("profile-pick-sound", &format!("{slot}:{folder}"))
}

/// A picker sound row's play button: "profile-pick-play:<folder>".
pub fn profile_pick_play(folder: &str) -> ElementId {
    keyed("profile-pick-play", folder)
}

/// A Missing Items row, keyed by its index: "missing-item:<ix>".
pub fn missing_item(ix: usize) -> ElementId {
    keyed(MISSING_ITEM, &ix.to_string())
}

/// A row's Re-download button, keyed by its index: "missing-redownload:<ix>".
pub fn missing_redownload(ix: usize) -> ElementId {
    keyed(MISSING_REDOWNLOAD, &ix.to_string())
}

/// A row's "Local import" note, keyed by its index: "missing-local:<ix>".
pub fn missing_local(ix: usize) -> ElementId {
    keyed(MISSING_LOCAL, &ix.to_string())
}

/// A header sync ring, keyed by the store's slug: "sync-ring:<slug>".
pub fn sync_ring(kind: StoreKind) -> ElementId {
    keyed(SYNC_RING, store_slug(kind))
}

/// The header's logs.tf sync ring: "sync-ring:logs-tf".
pub fn logstf_sync_ring() -> ElementId {
    keyed(SYNC_RING, "logs-tf")
}

/// A Settings sync card's status line, keyed by the store's slug: "settings-sync:<slug>".
pub fn settings_sync(kind: StoreKind) -> ElementId {
    keyed(SETTINGS_SYNC, store_slug(kind))
}

/// A Settings sync card's Resync button, keyed by the store's slug: "settings-resync:<slug>".
pub fn settings_resync(kind: StoreKind) -> ElementId {
    keyed(SETTINGS_RESYNC, store_slug(kind))
}

/// One option row of a labelled Select, under the id of the Select's wrapper and keyed by its
/// label: "<select>-option:<label>", for example "maps-sort-option:Most played".
pub fn select_option(select: &str, label: &str) -> ElementId {
    keyed(&format!("{select}-option"), label)
}

/// One row of the install select, keyed by its index: "settings-candidate:<i>".
pub fn settings_candidate(index: usize) -> ElementId {
    keyed("settings-candidate", &index.to_string())
}

/// A library HUD card, keyed by its folder.
pub fn hud_card(folder: &str) -> ElementId {
    keyed("hud-card", folder)
}

/// A store HUD card's screenshot; present only once the image is ready.
pub fn hud_image(folder: &str) -> ElementId {
    keyed("hud-image", folder)
}

/// A store HUD card's 16:9 screenshot area, always present; a click opens its gallery.
pub fn hud_shot(folder: &str) -> ElementId {
    keyed("hud-shot", folder)
}

/// A HUD's name in the list of installed HUDs a conflict shows, keyed by its folder.
pub fn hud_installed_name(folder: &str) -> ElementId {
    keyed("hud-installed-name", folder)
}

/// A HUD's Uninstall button in the list of installed HUDs a conflict shows.
pub fn hud_uninstall(folder: &str) -> ElementId {
    keyed("hud-uninstall", folder)
}

/// A library HUD card's Install button.
pub fn hud_install(folder: &str) -> ElementId {
    keyed("hud-install", folder)
}

/// A library HUD card's delete button.
pub fn hud_delete(folder: &str) -> ElementId {
    keyed("hud-delete", folder)
}

/// A made HUD card's "Edit in HUD Maker" button.
pub fn hud_edit(folder: &str) -> ElementId {
    keyed("hud-edit", folder)
}

/// A library HUD card's "Edit a copy" button, on every HUD not made in the HUD Maker.
pub fn hud_copy(folder: &str) -> ElementId {
    keyed("hud-copy", folder)
}

/// A HUD copy's card line naming the HUD it was copied from.
pub fn hud_copy_of(folder: &str) -> ElementId {
    keyed("hud-copy-of", folder)
}

/// A library HUD card's "Update" button: a copy's when its HUD's newer version is in the
/// library, an unedited store HUD's when the store has a newer version.
pub fn hud_update(folder: &str) -> ElementId {
    keyed("hud-update", folder)
}

/// A store HUD card's "Visit page" button, opening the HUD's own page in the browser.
pub fn hud_visit(folder: &str) -> ElementId {
    keyed("hud-visit", folder)
}

/// An unedited store HUD's "Update available" line, shown when the store has a newer version.
pub fn hud_update_mark(folder: &str) -> ElementId {
    keyed("hud-update-mark", folder)
}

/// A HUD copy's line saying its HUD's newer version is only in the HUD Store.
pub fn hud_update_note(folder: &str) -> ElementId {
    keyed("hud-update-note", folder)
}

/// A library HUD card's badge, labelled "Store" or "Local".
pub fn hud_badge(folder: &str) -> ElementId {
    keyed("hud-badge", folder)
}

/// A library HUD card's "by {author}" line; absent when the author is unknown.
pub fn hud_author(folder: &str) -> ElementId {
    keyed("hud-author", folder)
}

/// A library sound card, keyed by its folder.
pub fn sound_card(folder: &str) -> ElementId {
    keyed("sound-card", folder)
}

/// A library sound card's play button; it stops the sound when that one is playing.
pub fn sound_play(folder: &str) -> ElementId {
    keyed("sound-play", folder)
}

/// A library sound card's "VPK" badge; present only for VPK sounds.
pub fn sound_vpk(folder: &str) -> ElementId {
    keyed("sound-vpk", folder)
}

/// A library sound card's ghost delete button; asks before deleting.
pub fn sound_delete(folder: &str) -> ElementId {
    keyed("sound-delete", folder)
}

/// A library sound card's outline "Hit" button; installs it as the hitsound.
pub fn sound_hit(folder: &str) -> ElementId {
    keyed("sound-hit", folder)
}

/// A library sound card's outline "Kill" button; installs it as the killsound.
pub fn sound_kill(folder: &str) -> ElementId {
    keyed("sound-kill", folder)
}

/// A hitsound or killsound slot card, keyed by the kind's name.
pub fn sound_slot(kind: SoundKind) -> ElementId {
    keyed("sound-slot", kind.name())
}

/// A slot's status badge, labelled "Installed" or "None".
pub fn sound_slot_status(kind: SoundKind) -> ElementId {
    keyed("sound-slot-status", kind.name())
}

/// A slot's "VPK" badge; present only when the active form is a VPK.
pub fn sound_slot_vpk(kind: SoundKind) -> ElementId {
    keyed("sound-slot-vpk", kind.name())
}

/// A slot's name line, or "No <kind> installed" when empty.
pub fn sound_slot_name(kind: SoundKind) -> ElementId {
    keyed("sound-slot-name", kind.name())
}

/// A slot's play button; present only when installed.
pub fn sound_slot_play(kind: SoundKind) -> ElementId {
    keyed("sound-slot-play", kind.name())
}

/// A slot's Uninstall button; present only when installed.
pub fn sound_slot_uninstall(kind: SoundKind) -> ElementId {
    keyed("sound-slot-uninstall", kind.name())
}

/// A builtin crosshair option, keyed by its catalog name.
pub fn crosshair_builtin(name: &str) -> ElementId {
    keyed("crosshair-option", &format!("builtin:{name}"))
}

/// A library crosshair option, keyed by its folder.
pub fn crosshair_library(folder: &str) -> ElementId {
    keyed("crosshair-option", &format!("library:{folder}"))
}

/// One conflict line, keyed by the rival folder, labelled with the whole line.
pub fn crosshair_conflict(folder: &str) -> ElementId {
    keyed("crosshair-conflict", folder)
}

/// A TF2 class tab on the crosshairs page.
pub fn class_tab(class: &str) -> ElementId {
    keyed("class-tab", class)
}

/// A loadout board cell, labelled with its crosshair, "Mixed" or "Default":
/// "loadout-cell:<class>:<slot>".
pub fn loadout_cell(class: TfClass, slot: WeaponSlot) -> ElementId {
    keyed("loadout-cell", &board_key(class, slot))
}

/// A board cell's menu button, opening its per-weapon panel.
pub fn loadout_cell_menu(class: TfClass, slot: WeaponSlot) -> ElementId {
    keyed("loadout-cell-menu", &board_key(class, slot))
}

/// A board class header, assigning that class: "loadout-class:<class>".
pub fn loadout_class(class: TfClass) -> ElementId {
    keyed("loadout-class", &class.name().to_lowercase())
}

/// A board slot header, assigning that slot for every class: "loadout-slot:<slot>".
pub fn loadout_slot(slot: WeaponSlot) -> ElementId {
    keyed("loadout-slot", &slot.name().to_lowercase())
}

fn board_key(class: TfClass, slot: WeaponSlot) -> String {
    format!(
        "{}:{}",
        class.name().to_lowercase(),
        slot.name().to_lowercase()
    )
}

/// A weapon row on the crosshairs page, keyed by its weapon script.
pub fn weapon_row(script: &str) -> ElementId {
    keyed("weapon-row", script)
}

/// A weapon row's wiki icon in its slot-coloured frame, present once the icon has loaded:
/// "weapon-icon:<script>".
pub fn weapon_icon(script: &str) -> ElementId {
    keyed("weapon-icon", script)
}

/// A weapon row's 16 px slot square, shown while its icon is missing, loading or failed:
/// "weapon-slot-icon:<script>".
pub fn weapon_slot_icon(script: &str) -> ElementId {
    keyed("weapon-slot-icon", script)
}

/// A weapon row's assignment text: the crosshair's name, or "None".
pub fn weapon_assigned(script: &str) -> ElementId {
    keyed("weapon-assigned", script)
}

/// A weapon row's check mark, present only when the weapon has an assignment.
pub fn weapon_check(script: &str) -> ElementId {
    keyed("weapon-check", script)
}

/// A weapon row's clear button, shown only when the assigner is clearable: "weapon-clear:<script>".
pub fn weapon_clear(script: &str) -> ElementId {
    keyed("weapon-clear", script)
}

/// A weapon row's spinner, present only while that weapon is being assigned.
pub fn weapon_spinner(script: &str) -> ElementId {
    keyed("weapon-spinner", script)
}

/// A store card, keyed by store and item: "store-card:<slug>:<id>", the id being the GameBanana
/// number, the curated `hud_id` or the addon name.
pub fn store_card(kind: StoreKind, id: &StoreItemId) -> ElementId {
    keyed(STORE_CARD, &store_key(kind, id))
}

/// A store card's tag by position: "store-card-tag:<slug>:<id>:<index>".
pub fn store_card_tag(kind: StoreKind, id: &StoreItemId, index: usize) -> ElementId {
    keyed(
        "store-card-tag",
        &format!("{}:{index}", store_key(kind, id)),
    )
}

/// A store card's badge rows: "store-card-tags:<slug>:<id>", keyed as `store_card`.
pub fn store_card_tags(kind: StoreKind, id: &StoreItemId) -> ElementId {
    keyed("store-card-tags", &store_key(kind, id))
}

/// A Sound Store card's category badge: "store-card-category:<slug>:<id>".
pub fn store_card_category(kind: StoreKind, id: &StoreItemId) -> ElementId {
    keyed("store-card-category", &store_key(kind, id))
}

/// A store card's 16:9 image area: "store-card-image:<slug>:<id>", keyed as `store_card`.
pub fn store_card_image(kind: StoreKind, id: &StoreItemId) -> ElementId {
    keyed("store-card-image", &store_key(kind, id))
}

/// A store card's name: "store-card-name:<slug>:<id>", keyed as `store_card`, labelled with the name.
pub fn store_card_name(kind: StoreKind, id: &StoreItemId) -> ElementId {
    keyed("store-card-name", &store_key(kind, id))
}

/// A curated card's repository button: "store-card-repo:<slug>:<id>", keyed as `store_card`.
pub fn store_card_repo(kind: StoreKind, id: &StoreItemId) -> ElementId {
    keyed("store-card-repo", &store_key(kind, id))
}

/// A sound store card's click target inside its `store_card` wrapper; opens the detail dialog.
pub const SOUND_STORE_CARD_TARGET: &str = "sound-card";

/// An item's download button, or its progress label while it downloads:
/// "store-download:<slug>:<id>", keyed as `store_card`.
pub fn store_download(kind: StoreKind, id: &StoreItemId) -> ElementId {
    keyed("store-download", &store_key(kind, id))
}

/// A store card's preview play button: "store-play:<slug>:<id>", keyed as `store_card`.
pub fn store_play(kind: StoreKind, id: &StoreItemId) -> ElementId {
    keyed("store-play", &store_key(kind, id))
}

/// A card's state badge: "store-state:<slug>:<id>", keyed as `store_card`.
pub fn store_state(kind: StoreKind, id: &StoreItemId) -> ElementId {
    keyed("store-state", &store_key(kind, id))
}

/// A store card's in-library check, present only when installed and current:
/// "store-check:<slug>:<id>", keyed as `store_card`.
pub fn store_check(kind: StoreKind, id: &StoreItemId) -> ElementId {
    keyed("store-check", &store_key(kind, id))
}

/// A store card's release tag: "store-release-tag:<slug>:<id>", keyed as `store_card`.
pub fn store_release_tag(kind: StoreKind, id: &StoreItemId) -> ElementId {
    keyed("store-release-tag", &store_key(kind, id))
}

/// A store card's description: "store-description:<slug>:<id>", keyed as `store_card`.
pub fn store_description(kind: StoreKind, id: &StoreItemId) -> ElementId {
    keyed("store-description", &store_key(kind, id))
}

/// A category filter pill, keyed by its slug: "store-category:<slug>".
pub fn store_category(slug: &str) -> ElementId {
    keyed("store-category", slug)
}

fn store_key(kind: StoreKind, id: &StoreItemId) -> String {
    let key = match id {
        StoreItemId::GameBanana(id) => id.get().to_string(),
        StoreItemId::Curated(hud_id) => hud_id.clone(),
        StoreItemId::Addon(name) => name.as_str().to_owned(),
        _ => "unknown".to_owned(),
    };
    format!("{}:{key}", store_slug(kind))
}

/// A detail dialog file row's Download button: "store-file:<i>".
pub fn store_file(index: usize) -> ElementId {
    keyed("store-file", &index.to_string())
}

/// A detail body's link button, keyed by what it opens ("repo", "gamebanana", a social key).
pub fn store_detail_link(key: &str) -> ElementId {
    keyed("store-detail-link", key)
}

/// The 14 px icon leading a curated social button: "store-detail-social-icon:<key>".
pub fn store_detail_social_icon(key: &str) -> ElementId {
    keyed("store-detail-social-icon", key)
}

/// A curated social button's label, after its icon: "store-detail-social-label:<key>".
pub fn store_detail_social_label(key: &str) -> ElementId {
    keyed("store-detail-social-label", key)
}

/// A curated OS filter toggle: "store-filter-os:<windows|mac|linux>", keyed by `Os::key`.
pub fn store_filter_os(os: &str) -> ElementId {
    keyed("store-filter-os", os)
}

/// A curated facet chip: "store-filter:<facet>:<value>", the facet keyed by `Facet::key`.
pub fn store_filter(facet: &str, value: &str) -> ElementId {
    keyed("store-filter", &format!("{facet}:{value}"))
}

/// The detail dialog's item name: "store-detail-name".
pub const STORE_DETAIL_NAME: &str = "store-detail-name";

/// v5's EmptyState card around a store's empty message.
pub const STORE_EMPTY_CARD: &str = "store-empty-card";
/// The empty-state card's 48 px icon.
pub const STORE_EMPTY_ICON: &str = "store-empty-icon";
/// Labelled "Loading..." over a store's skeleton cards before its first load ends.
pub const STORE_LOADING: &str = "store-loading";

/// A store's placeholder card while it loads or syncs: "skeleton-card:<ix>".
pub fn skeleton_card(ix: usize) -> ElementId {
    keyed("skeleton-card", &ix.to_string())
}

/// The hint under a store's "nothing matches your filters" card.
pub const STORE_EMPTY_HINT: &str = "store-empty-hint";

/// A store card's like count: "store-likes:<slug>:<id>", keyed as `store_card`.
pub fn store_likes(kind: StoreKind, id: &StoreItemId) -> ElementId {
    keyed("store-likes", &store_key(kind, id))
}

/// A store card's view count: "store-views:<slug>:<id>", keyed as `store_card`.
pub fn store_views(kind: StoreKind, id: &StoreItemId) -> ElementId {
    keyed("store-views", &store_key(kind, id))
}

/// The open detail dialog's like count.
pub const STORE_DETAIL_LIKES: &str = "store-detail-likes";
/// The open detail dialog's view count.
pub const STORE_DETAIL_VIEWS: &str = "store-detail-views";
/// The sound detail's category badge, labelled with the category.
pub const STORE_DETAIL_CATEGORY: &str = "store-detail-category";

/// A detail badge by row ("flags" or "tags") and position: "store-detail-tag:<row>:<index>".
pub fn store_detail_tag(row: &str, index: usize) -> ElementId {
    keyed("store-detail-tag", &format!("{row}:{index}"))
}

/// The HUD fonts dialog, opened when a HUD's fonts are not installed.
pub const HUD_FONTS_DIALOG: &str = "hud-fonts-dialog";
/// The HUD fonts dialog's title, "HUD fonts".
pub const HUD_FONTS_TITLE: &str = "hud-fonts-title";
/// The HUD fonts dialog's description, naming the HUD folder.
pub const HUD_FONTS_DESCRIPTION: &str = "hud-fonts-description";
/// Installs the listed missing fonts; absent when nothing is missing.
pub const HUD_FONTS_INSTALL: &str = "hud-fonts-install";
/// Closes the HUD fonts dialog without installing.
pub const HUD_FONTS_SKIP: &str = "hud-fonts-skip";
/// Remembers this check for the HUD so it is not asked again.
pub const HUD_FONTS_DONT_ASK: &str = "hud-fonts-dont-ask";

/// A font row in the HUD fonts dialog, missing fonts then protected names: "hud-fonts-font:<ix>".
pub fn hud_fonts_font(ix: usize) -> ElementId {
    keyed("hud-fonts-font", &ix.to_string())
}

/// A Linux font issue row in the HUD fonts dialog: "hud-fonts-issue:<ix>".
pub fn hud_fonts_issue(ix: usize) -> ElementId {
    keyed("hud-fonts-issue", &ix.to_string())
}

/// The HUD fonts dialog's scrolling list of font and issue rows.
pub const HUD_FONTS_LIST: &str = "hud-fonts-list";
/// The HUD fonts list's scrollbar, over its right edge.
pub const HUD_FONTS_LIST_SCROLLBAR: &str = "hud-fonts-list-scrollbar";

/// One of the maker's colour swatches, keyed by its index: "crosshair-maker-swatch:<ix>".
pub fn crosshair_maker_swatch(ix: usize) -> ElementId {
    keyed("crosshair-maker-swatch", &ix.to_string())
}

/// A divider between the maker's toolbar groups, keyed by its order: "crosshair-maker-divider:<ix>".
pub fn crosshair_maker_divider(ix: usize) -> ElementId {
    keyed("crosshair-maker-divider", &ix.to_string())
}

/// A starter shape button, keyed by `Starter::slug`: "crosshair-maker-starter:<slug>".
pub fn crosshair_maker_starter(slug: &str) -> ElementId {
    keyed("crosshair-maker-starter", slug)
}

/// A starter slider's wrapper, keyed by `Param::slug`: "crosshair-maker-starter-slider:<slug>".
pub fn crosshair_maker_starter_slider(slug: &str) -> ElementId {
    keyed("crosshair-maker-starter-slider", slug)
}

/// A starter slider's "Length: 8 px" label: "crosshair-maker-starter-value:<slug>".
pub fn crosshair_maker_starter_value(slug: &str) -> ElementId {
    keyed("crosshair-maker-starter-value", slug)
}

/// A HUD part drawn on the HUD Maker canvas, keyed by its part key: "hud-maker-part:<key>".
pub fn hud_maker_part(key: &PartKey) -> ElementId {
    keyed("hud-maker-part", key.as_str())
}

/// The outline over one of several selected parts on the HUD Maker canvas:
/// "hud-maker-selected:<key>".
pub fn hud_maker_selected(key: &PartKey) -> ElementId {
    keyed("hud-maker-selected", key.as_str())
}

/// The dashed outline of a selected part's whole box where it pokes out of its parent and the
/// game cuts it off: "hud-maker-cut-off:<key>".
pub fn hud_maker_cut_off(key: &PartKey) -> ElementId {
    keyed("hud-maker-cut-off", key.as_str())
}

/// One of the side panel's Align and Distribute buttons for several parts, by what it does
/// ("left", "centre", "right", "top", "middle", "bottom", "across", "down"):
/// "hud-maker-line-up:<name>".
pub fn hud_maker_line_up(name: &str) -> ElementId {
    keyed("hud-maker-line-up", name)
}

/// One step of the HUD Maker panel title's way down to the selected part, a part enclosing it
/// that a click selects: "hud-maker-crumb:<key>".
pub fn hud_maker_crumb(key: &PartKey) -> ElementId {
    keyed("hud-maker-crumb", key.as_str())
}

/// The clip a part paints inside on the HUD Maker canvas, its box cut to its parents and the
/// screen: "hud-maker-part-clip:<key>".
pub fn hud_maker_part_clip(key: &PartKey) -> ElementId {
    keyed("hud-maker-part-clip", key.as_str())
}

/// A label's drawn text inside its part on the HUD Maker canvas: "hud-maker-text:<key>".
pub fn hud_maker_text(key: &PartKey) -> ElementId {
    keyed("hud-maker-text", key.as_str())
}

/// The box that cuts a label's text to the width the game draws it in, with room above and
/// below for its ink: "hud-maker-text-clip:<key>".
pub fn hud_maker_text_clip(key: &PartKey) -> ElementId {
    keyed("hud-maker-text-clip", key.as_str())
}

/// One line of a wrapped label's text, first line 0: "hud-maker-text-line:<key>:<line>".
pub fn hud_maker_text_line(key: &PartKey, line: usize) -> ElementId {
    keyed("hud-maker-text-line", &format!("{}:{line}", key.as_str()))
}

/// A label's drop shadow copy, drawn only for a `dropshadow` font: "hud-maker-text-shadow:<key>".
pub fn hud_maker_text_shadow(key: &PartKey) -> ElementId {
    keyed("hud-maker-text-shadow", key.as_str())
}

/// A label's outline copies, drawn only for an `outline` font: "hud-maker-text-outline:<key>".
pub fn hud_maker_text_outline(key: &PartKey) -> ElementId {
    keyed("hud-maker-text-outline", key.as_str())
}

/// A HUD part's row in the HUD Maker's part list: "hud-maker-row:<key>".
pub fn hud_maker_row(key: &PartKey) -> ElementId {
    keyed("hud-maker-row", key.as_str())
}

/// The part's name in its part list row: "hud-maker-row-name:<key>".
pub fn hud_maker_row_name(key: &PartKey) -> ElementId {
    keyed("hud-maker-row-name", key.as_str())
}

/// The part's state tag ("hidden", "shows in game") in its part list row:
/// "hud-maker-row-state:<key>".
pub fn hud_maker_row_state(key: &PartKey) -> ElementId {
    keyed("hud-maker-row-state", key.as_str())
}

/// The eye that hides or shows a part from its part list row: "hud-maker-row-eye:<key>".
pub fn hud_maker_row_eye(key: &PartKey) -> ElementId {
    keyed("hud-maker-row-eye", key.as_str())
}

/// The arrow that folds or unfolds a group's row in the part list: "hud-maker-row-fold:<key>".
pub fn hud_maker_row_fold(key: &PartKey) -> ElementId {
    keyed("hud-maker-row-fold", key.as_str())
}

/// The dot on a part list row whose part differs from the starting HUD:
/// "hud-maker-row-changed:<key>".
pub fn hud_maker_row_changed(key: &PartKey) -> ElementId {
    keyed("hud-maker-row-changed", key.as_str())
}

/// The pin marker on a part placed against a sibling: "hud-maker-pin:<key>".
pub fn hud_maker_pin(key: &PartKey) -> ElementId {
    keyed("hud-maker-pin", key.as_str())
}

/// One piece of an image part's texture: "hud-maker-image-cell:<key>/<index>".
pub fn hud_maker_image_cell(key: &PartKey, index: usize) -> ElementId {
    keyed("hud-maker-image-cell", &format!("{}/{index}", key.as_str()))
}

/// One piece of a part's scalable image border: "hud-maker-border-cell:<key>/<index>".
pub fn hud_maker_border_cell(key: &PartKey, index: usize) -> ElementId {
    keyed(
        "hud-maker-border-cell",
        &format!("{}/{index}", key.as_str()),
    )
}

/// A killfeed line's weapon icon: "hud-maker-killfeed-icon:<index>".
pub fn hud_maker_killfeed_icon(index: usize) -> ElementId {
    keyed("hud-maker-killfeed-icon", &index.to_string())
}

/// A player list row's class icon: "hud-maker-list-class:<list>/<row>".
pub fn hud_maker_list_class(list: &PartKey, row: usize) -> ElementId {
    keyed("hud-maker-list-class", &format!("{}/{row}", list.as_str()))
}

/// The class portrait a 3D class model draws as: "hud-maker-portrait:<key>".
pub fn hud_maker_portrait(key: &PartKey) -> ElementId {
    keyed("hud-maker-portrait", key.as_str())
}

/// The plain square a game-filled image draws as: "hud-maker-stand-in:<key>".
pub fn hud_maker_stand_in(key: &PartKey) -> ElementId {
    keyed("hud-maker-stand-in", key.as_str())
}

/// The sample rank name a rank panel draws: "hud-maker-rank-name:<key>".
pub fn hud_maker_rank_name(key: &PartKey) -> ElementId {
    keyed("hud-maker-rank-name", key.as_str())
}

/// A rank panel's level bar track: "hud-maker-rank-bar:<key>".
pub fn hud_maker_rank_bar(key: &PartKey) -> ElementId {
    keyed("hud-maker-rank-bar", key.as_str())
}

/// The filled part of a rank panel's level bar: "hud-maker-rank-fill:<key>".
pub fn hud_maker_rank_fill(key: &PartKey) -> ElementId {
    keyed("hud-maker-rank-fill", key.as_str())
}

/// The colour a part paints behind its content: "hud-maker-fill:<key>".
pub fn hud_maker_fill(key: &PartKey) -> ElementId {
    keyed("hud-maker-fill", key.as_str())
}

/// A scoreboard player list's column header: "hud-maker-list-header:<key>".
pub fn hud_maker_list_header(list: &PartKey) -> ElementId {
    keyed("hud-maker-list-header", list.as_str())
}

/// One sample player's row in a player list, a11y-labelled with the player's name:
/// "hud-maker-list-row:<key>/<row>".
pub fn hud_maker_list_row(list: &PartKey, row: usize) -> ElementId {
    keyed("hud-maker-list-row", &format!("{}/{row}", list.as_str()))
}

/// A sample killfeed line on the HUD Maker canvas, top first: "hud-maker-killfeed-line:<i>".
pub fn hud_maker_killfeed_line(index: usize) -> ElementId {
    keyed("hud-maker-killfeed-line", &index.to_string())
}

/// A sample chat line on the HUD Maker canvas, top first: "hud-maker-chat-line:<i>".
pub fn hud_maker_chat_line(index: usize) -> ElementId {
    keyed("hud-maker-chat-line", &index.to_string())
}

/// The ping bar in a player list's row: "hud-maker-list-ping:<key>/<row>".
pub fn hud_maker_list_ping(list: &PartKey, row: usize) -> ElementId {
    keyed("hud-maker-list-ping", &format!("{}/{row}", list.as_str()))
}

/// A column title in a player list's header, by its lower-case column name:
/// "hud-maker-list-title:<key>/<column>".
pub fn hud_maker_list_title(list: &PartKey, column: &str) -> ElementId {
    keyed(
        "hud-maker-list-title",
        &format!("{}/{column}", list.as_str()),
    )
}

/// The avatar placeholder in a player list's row: "hud-maker-list-avatar:<key>/<row>".
pub fn hud_maker_list_avatar(list: &PartKey, row: usize) -> ElementId {
    keyed("hud-maker-list-avatar", &format!("{}/{row}", list.as_str()))
}

/// A HUD colour name in the Colour row's list: "hud-maker-colour-name:<name>".
pub fn hud_maker_colour_name(name: &str) -> ElementId {
    keyed("hud-maker-colour-name", name)
}

/// A Colour row's swatch button, keyed by its key: "hud-maker-colour-row:TeamRed". Pressing
/// it makes the row the one the picker edits; the slot's shows while another row is active.
pub fn hud_maker_colour_row(key: &str) -> ElementId {
    keyed("hud-maker-colour-row", key)
}

/// A Number row's field, keyed by its key: "hud-maker-number:linespacing".
pub fn hud_maker_number(key: &str) -> ElementId {
    keyed("hud-maker-number", key)
}

/// An Alignment row button, keyed by its `textAlignment` value: "hud-maker-align:<value>".
/// The holder that carries the tooltip of the HUD Maker control `of`.
pub fn hud_maker_tip_for(of: &ElementId) -> ElementId {
    keyed("hud-maker-tip-for", &of.to_string())
}

/// The note under the HUD Maker number box `of`, labelled with why a typed value was rounded
/// or refused; absent once a typed value lands as typed.
pub fn hud_maker_note_for(of: &ElementId) -> ElementId {
    keyed("hud-maker-note-for", &of.to_string())
}

pub fn hud_maker_align(value: &str) -> ElementId {
    keyed("hud-maker-align", value)
}

/// The kit button inside an Alignment row button: "hud-maker-align-button:<value>".
pub fn hud_maker_align_button(value: &str) -> ElementId {
    keyed("hud-maker-align-button", value)
}

/// A Choice row's option, the wrapper that reports whether it is chosen:
/// "hud-maker-choice:<key>=<value>".
pub fn hud_maker_choice(key: &str, value: &str) -> ElementId {
    keyed("hud-maker-choice", &format!("{key}={value}"))
}

/// The button inside a Choice row's option: "hud-maker-choice-button:<key>=<value>".
pub fn hud_maker_choice_button(key: &str, value: &str) -> ElementId {
    keyed("hud-maker-choice-button", &format!("{key}={value}"))
}

/// A Switch row's switch, keyed by the row's key: "hud-maker-toggle:<key>".
pub fn hud_maker_toggle(key: &str) -> ElementId {
    keyed("hud-maker-toggle", key)
}

/// A starting point's button, keyed by its label: "hud-maker-start:Big numbers".
pub fn hud_maker_start(label: &str) -> ElementId {
    keyed("hud-maker-start", label)
}

/// A starting point's button in the Start from card: "hud-maker-start-card:Big numbers".
pub fn hud_maker_start_card_point(label: &str) -> ElementId {
    keyed("hud-maker-start-card", label)
}

/// Row `i` of the keyboard shortcuts sheet, labelled "<keys>: <what they do>".
pub fn hud_maker_shortcut(i: usize) -> ElementId {
    keyed("hud-maker-shortcut", &i.to_string())
}

/// A snapshot row's name, keyed by its file: "hud-maker-snapshot:<file>".
pub fn hud_maker_snapshot(file: &str) -> ElementId {
    keyed("hud-maker-snapshot", file)
}

/// A HUD to borrow from in the Borrow a part section: "hud-maker-borrow-hud:<id>".
pub fn hud_maker_borrow_hud(id: &HudId) -> ElementId {
    keyed("hud-maker-borrow-hud", id.as_str())
}

/// A part of the chosen HUD in the Borrow a part section: "hud-maker-borrow-part:<key>".
pub fn hud_maker_borrow_part(key: &PartKey) -> ElementId {
    keyed("hud-maker-borrow-part", key.as_str())
}

/// A snapshot row's time and change count: "hud-maker-snapshot-when:<file>".
pub fn hud_maker_snapshot_when(file: &str) -> ElementId {
    keyed("hud-maker-snapshot-when", file)
}

/// A snapshot row's Compare button: "hud-maker-snapshot-compare:<file>".
pub fn hud_maker_snapshot_compare(file: &str) -> ElementId {
    keyed("hud-maker-snapshot-compare", file)
}

/// A snapshot row's Restore button: "hud-maker-snapshot-restore:<file>".
pub fn hud_maker_snapshot_restore(file: &str) -> ElementId {
    keyed("hud-maker-snapshot-restore", file)
}

/// A snapshot row's Delete button: "hud-maker-snapshot-delete:<file>".
pub fn hud_maker_snapshot_delete(file: &str) -> ElementId {
    keyed("hud-maker-snapshot-delete", file)
}

/// A preset's Apply button, labelled with its name: "hud-maker-preset-apply:<file>".
pub fn hud_maker_preset_apply(file: &str) -> ElementId {
    keyed("hud-maker-preset-apply", file)
}

/// A preset's Delete button: "hud-maker-preset-delete:<file>".
pub fn hud_maker_preset_delete(file: &str) -> ElementId {
    keyed("hud-maker-preset-delete", file)
}

/// An option's control, keyed by the option id: "hud-maker-option:chat_top".
pub fn hud_maker_option(id: &str) -> ElementId {
    keyed("hud-maker-option", id)
}

/// An option group's heading, keyed by the group name: "hud-maker-option-group:Main menu".
pub fn hud_maker_option_group(group: &str) -> ElementId {
    keyed("hud-maker-option-group", group)
}

/// An option group's Reset button, keyed by the group name: "hud-maker-option-reset:HUD".
pub fn hud_maker_option_reset(group: &str) -> ElementId {
    keyed("hud-maker-option-reset", group)
}

/// A copy's own option group's heading, labelled with its name: "hud-maker-hud-option-group:Damage".
pub fn hud_maker_hud_option_group(group: &str) -> ElementId {
    keyed("hud-maker-hud-option-group", group)
}

/// A copy's own option group's Reset button: "hud-maker-hud-option-reset:Switches".
pub fn hud_maker_hud_option_reset(group: &str) -> ElementId {
    keyed("hud-maker-hud-option-reset", group)
}

/// One choice of a copy's own Choice option, by option id and choice key:
/// "hud-maker-option-choice:<id>|<key>".
pub fn hud_maker_option_choice(id: &str, key: &str) -> ElementId {
    keyed("hud-maker-option-choice", &format!("{id}|{key}"))
}

/// The note a group's options share, under its heading: "hud-maker-option-group-note:<group>".
pub fn hud_maker_option_group_note(group: &str) -> ElementId {
    keyed("hud-maker-option-group-note", group)
}

/// The note under an option's row, labelled with its text: "hud-maker-option-note:<id>".
pub fn hud_maker_option_note(id: &str) -> ElementId {
    keyed("hud-maker-option-note", id)
}

/// What a HUD option still needs outside the HUD, under its control, labelled with it:
/// "hud-maker-option-need:<id>".
pub fn hud_maker_option_need(id: &str) -> ElementId {
    keyed("hud-maker-option-need", id)
}

/// A Number option's value beside its slider, labelled with it: "hud-maker-option-value:<id>".
pub fn hud_maker_option_value(id: &str) -> ElementId {
    keyed("hud-maker-option-value", id)
}

/// A Font row's current font name, keyed by the row's key: "hud-maker-font:delta_item_font".
pub fn hud_maker_font(key: &str) -> ElementId {
    keyed("hud-maker-font", key)
}

/// A HUD font in a Font row's "Fonts" list, keyed by its name: "hud-maker-font-name:<name>".
pub fn hud_maker_font_name(name: &str) -> ElementId {
    keyed("hud-maker-font-name", name)
}

/// A family in a Font row's "Family" list: "hud-maker-font-family:TF2 Build".
pub fn hud_maker_font_family(name: &str) -> ElementId {
    keyed("hud-maker-font-family", name)
}

/// A Font row's Size, keyed by the row's key: "hud-maker-font-size:<key>". It is the input on
/// the row last pressed and the size as text on the others.
pub fn hud_maker_font_size(key: &str) -> ElementId {
    keyed("hud-maker-font-size", key)
}

/// A Font row's Outline switch: "hud-maker-font-outline:<key>".
pub fn hud_maker_font_outline(key: &str) -> ElementId {
    keyed("hud-maker-font-outline", key)
}

/// A Font row's Shadow switch: "hud-maker-font-shadow:<key>".
pub fn hud_maker_font_shadow(key: &str) -> ElementId {
    keyed("hud-maker-font-shadow", key)
}

/// A Font row's "Add a font file" button: "hud-maker-font-add:<key>".
pub fn hud_maker_font_add(key: &str) -> ElementId {
    keyed("hud-maker-font-add", key)
}

/// A Font row's "Fonts" button, opening the HUD's fonts: "hud-maker-font-list:<key>".
pub fn hud_maker_font_list(key: &str) -> ElementId {
    keyed("hud-maker-font-list", key)
}

/// A Font row's "Family" button, opening the families: "hud-maker-font-family-list:<key>".
pub fn hud_maker_font_family_list(key: &str) -> ElementId {
    keyed("hud-maker-font-family-list", key)
}

/// The open list under a Font row: "hud-maker-font-choices:<key>".
pub fn hud_maker_font_choices(key: &str) -> ElementId {
    keyed("hud-maker-font-choices", key)
}

/// The scrollbar beside font row `key`'s open choices list.
pub fn hud_maker_font_choices_scrollbar(key: &str) -> ElementId {
    keyed("hud-maker-font-choices-scrollbar", key)
}

/// A row of "All settings", keyed by the setting as spelled: "hud-maker-setting:xpos".
pub fn hud_maker_setting(key: &str) -> ElementId {
    keyed("hud-maker-setting", key)
}

/// An "All settings" row's value, which opens it for editing: "hud-maker-setting-value:<key>".
pub fn hud_maker_setting_value(key: &str) -> ElementId {
    keyed("hud-maker-setting-value", key)
}

/// Where an "All settings" row's value comes from: "hud-maker-setting-origin:<key>".
pub fn hud_maker_setting_origin(key: &str) -> ElementId {
    keyed("hud-maker-setting-origin", key)
}

/// The line number at the end of an "All settings" row's origin, which never truncates:
/// "hud-maker-setting-line:<key>".
pub fn hud_maker_setting_line(key: &str) -> ElementId {
    keyed("hud-maker-setting-line", key)
}

/// An "All settings" row's "Remove this setting" button: "hud-maker-setting-remove:<key>".
pub fn hud_maker_setting_remove(key: &str) -> ElementId {
    keyed("hud-maker-setting-remove", key)
}

/// The kit button inside an "All settings" Remove: "hud-maker-setting-remove-button:<key>".
pub fn hud_maker_setting_remove_button(key: &str) -> ElementId {
    keyed("hud-maker-setting-remove-button", key)
}

/// A side panel row's Reset button, keyed by the row's key: "hud-maker-reset:<key>".
pub fn hud_maker_reset(key: &str) -> ElementId {
    keyed("hud-maker-reset", key)
}

/// One of the selection's resize handles: "hud-maker-handle:<side>", such as "top-right".
pub fn hud_maker_handle(handle: crate::pages::hud_maker::editor::Handle) -> ElementId {
    use crate::pages::hud_maker::editor::Handle;
    let side = match handle {
        Handle::Move => "move",
        Handle::Left => "left",
        Handle::Right => "right",
        Handle::Top => "top",
        Handle::Bottom => "bottom",
        Handle::TopLeft => "top-left",
        Handle::TopRight => "top-right",
        Handle::BottomLeft => "bottom-left",
        Handle::BottomRight => "bottom-right",
    };
    keyed("hud-maker-handle", side)
}

/// A scene tab, keyed by its label lowercased with each space a hyphen:
/// "hud-maker-scene:scoreboard", "hud-maker-scene:main-menu".
pub fn hud_maker_scene(scene: Scene) -> ElementId {
    keyed(
        "hud-maker-scene",
        &SceneRule::get(scene)
            .label()
            .to_lowercase()
            .replace(' ', "-"),
    )
}

/// An entry in the match mode dropdown, keyed by the label lowercased:
/// "hud-maker-board-mode:casual", "hud-maker-board-mode:mvm".
pub fn hud_maker_board_mode(mode: BoardMode) -> ElementId {
    keyed("hud-maker-board-mode", &mode.label().to_lowercase())
}

/// An entry in the toolbar's screen shape menu: "hud-maker-screen:<wide>x<tall>".
pub fn hud_maker_screen(screen: Screen) -> ElementId {
    keyed(
        "hud-maker-screen",
        &format!("{}x{}", screen.wide(), screen.tall()),
    )
}

/// A tile of the six-screen view, labelled with its caption: "hud-maker-screen-tile:<w>x<h>".
pub fn hud_maker_screen_tile(screen: Screen) -> ElementId {
    keyed(
        "hud-maker-screen-tile",
        &format!("{}x{}", screen.wide(), screen.tall()),
    )
}

/// A flagged part on a six-screen tile, labelled with its problems:
/// "hud-maker-flag:<w>x<h>/<part key>".
pub fn hud_maker_flag(screen: Screen, key: &PartKey) -> ElementId {
    keyed(
        "hud-maker-flag",
        &format!("{}x{}/{}", screen.wide(), screen.tall(), key.as_str()),
    )
}

/// An arrow at a six-screen tile's edge pointing at a flagged part off that tile, labelled with
/// its problems: "hud-maker-edge-arrow:<w>x<h>/<part key>".
pub fn hud_maker_edge_arrow(screen: Screen, key: &PartKey) -> ElementId {
    keyed(
        "hud-maker-edge-arrow",
        &format!("{}x{}/{}", screen.wide(), screen.tall(), key.as_str()),
    )
}

/// The "Hard to read here" marker on a label: "hud-maker-hard-to-read:<part key>".
pub fn hud_maker_hard_to_read(key: &PartKey) -> ElementId {
    keyed("hud-maker-hard-to-read", key.as_str())
}

/// The badge beside a hard-to-read label: "hud-maker-hard-to-read-badge:<part key>".
pub fn hud_maker_hard_to_read_badge(key: &PartKey) -> ElementId {
    keyed("hud-maker-hard-to-read-badge", key.as_str())
}

/// One piece of the line from a moved badge back to its label's outline:
/// "hud-maker-hard-to-read-leader:<part key>#<piece>".
pub fn hud_maker_hard_to_read_leader(key: &PartKey, piece: usize) -> ElementId {
    keyed(
        "hud-maker-hard-to-read-leader",
        &format!("{}#{piece}", key.as_str()),
    )
}

/// A colour-blind view in the overflow menu, keyed by its word: "hud-maker-vision:Red-blind".
pub fn hud_maker_vision(label: &str) -> ElementId {
    keyed("hud-maker-vision", label)
}

/// The HUD scene's game situation menu, labelled "<Class>, <health> health".
pub const HUD_MAKER_SITUATION: &str = "hud-maker-situation";
/// The situation menu's button back to the usual moment.
pub const HUD_MAKER_SITUATION_RESET: &str = "hud-maker-situation-reset";
/// The situation menu's meter row, labelled "Meter: <weapons>"; it opens the meter list.
pub const HUD_MAKER_SITUATION_METER: &str = "hud-maker-situation-meter";
/// The situation menu's scrolling list of meters, while open.
pub const HUD_MAKER_SITUATION_METERS: &str = "hud-maker-situation-meters";
/// The scrollbar over the situation menu's meter list.
pub const HUD_MAKER_SITUATION_METERS_SCROLLBAR: &str = "hud-maker-situation-meters-scrollbar";
/// The side panel's Attach to menu list.
pub const HUD_MAKER_ATTACH_LIST: &str = "hud-maker-attach-list";
/// The scrollbar over the Attach to menu list.
pub const HUD_MAKER_ATTACH_LIST_SCROLLBAR: &str = "hud-maker-attach-list-scrollbar";
/// The side panel's Show it, which picks the situation a part needs to show.
pub const HUD_MAKER_PANEL_SHOW_IT: &str = "hud-maker-panel-show-it";
/// The side panel's note that the selection pokes out of its parent, which the game cuts off.
pub const HUD_MAKER_PANEL_CUT_OFF: &str = "hud-maker-panel-cut-off";
/// The button under that note that grows the parent to hold the selection.
pub const HUD_MAKER_GROW_PARENT: &str = "hud-maker-grow-parent";
/// The timeline strip under the canvas, while any event plays.
pub const HUD_MAKER_TIMELINE: &str = "hud-maker-timeline";
/// The timeline's list of the events playing, joined by ", ".
pub const HUD_MAKER_TIMELINE_EVENTS: &str = "hud-maker-timeline-events";
/// The timeline's Play and Pause button.
pub const HUD_MAKER_TIMELINE_PLAY: &str = "hud-maker-timeline-play";
/// The timeline's Clear, which stops the events the player started.
pub const HUD_MAKER_TIMELINE_CLEAR: &str = "hud-maker-timeline-clear";
/// The timeline's note of fields that change only in game: "Changes in game: <fields>".
pub const HUD_MAKER_TIMELINE_IN_GAME: &str = "hud-maker-timeline-in-game";

/// A class in the situation menu: "hud-maker-situation-class:medic".
pub fn hud_maker_situation_class(class: TfClass) -> ElementId {
    keyed("hud-maker-situation-class", &class.name().to_lowercase())
}

/// A yes/no fact in the situation menu: "hud-maker-situation-switch:just-hit".
pub fn hud_maker_situation_switch(switch: SituationSwitch) -> ElementId {
    let slug = match switch {
        SituationSwitch::Meter => "meter",
        SituationSwitch::Aiming => "aiming",
        SituationSwitch::Effect => "effect",
        SituationSwitch::Powerup => "powerup",
        SituationSwitch::Typing => "typing",
        SituationSwitch::Portrait => "portrait",
        SituationSwitch::JustHit => "just-hit",
        SituationSwitch::Koth => "koth",
        _ => "other",
    };
    keyed("hud-maker-situation-switch", slug)
}

/// A weapon kind in the situation menu: "hud-maker-situation-weapon:no-clip".
pub fn hud_maker_situation_weapon(weapon: WeaponKind) -> ElementId {
    let slug = match weapon {
        WeaponKind::Clip => "clip",
        WeaponKind::NoClip => "no-clip",
        _ => "other",
    };
    keyed("hud-maker-situation-weapon", slug)
}

/// A meter in the situation menu's meter list, by its place in `MeterKind::ALL`:
/// "hud-maker-situation-meter:7".
pub fn hud_maker_situation_meter(kind: MeterKind) -> ElementId {
    let index = MeterKind::ALL.iter().position(|&k| k == kind).unwrap_or(0);
    keyed("hud-maker-situation-meter", &index.to_string())
}

/// A slider in the situation menu, by its number: "hud-maker-situation-slider:health".
pub fn hud_maker_situation_slider(name: &str) -> ElementId {
    keyed("hud-maker-situation-slider", name)
}

/// The value beside a situation slider: "hud-maker-situation-value:health".
pub fn hud_maker_situation_value(name: &str) -> ElementId {
    keyed("hud-maker-situation-value", name)
}

/// The side panel's Animations section heading.
pub const HUD_MAKER_ANIMATIONS: &str = "hud-maker-animations";

/// One row of an event in the Animations section: "hud-maker-anim-row:<event>#<row>".
pub fn hud_maker_anim_row(event: &str, row: usize) -> ElementId {
    keyed("hud-maker-anim-row", &format!("{event}#{row}"))
}

/// A row's delete in the Animations section: "hud-maker-anim-delete:<event>#<row>".
pub fn hud_maker_anim_delete(event: &str, row: usize) -> ElementId {
    keyed("hud-maker-anim-delete", &format!("{event}#{row}"))
}

/// A row's edit toggle in the Animations section: "hud-maker-anim-edit:<event>#<row>".
pub fn hud_maker_anim_edit(event: &str, row: usize) -> ElementId {
    keyed("hud-maker-anim-edit", &format!("{event}#{row}"))
}

/// A curve in an open row's editor: "hud-maker-anim-curve:<event>#<row>/<curve>".
pub fn hud_maker_anim_curve(event: &str, row: usize, curve: &str) -> ElementId {
    keyed("hud-maker-anim-curve", &format!("{event}#{row}/{curve}"))
}

/// An event's Add for one field: "hud-maker-anim-add:<event>/<field>".
pub fn hud_maker_anim_add(event: &str, field: &str) -> ElementId {
    keyed("hud-maker-anim-add", &format!("{event}/{field}"))
}

/// The note on an event locked because it runs game commands: "hud-maker-anim-locked:<event>".
pub fn hud_maker_anim_locked(event: &str) -> ElementId {
    keyed("hud-maker-anim-locked", event)
}

/// The side panel's Add menu, which adds a part near the selection.
pub const HUD_MAKER_ADD_PART: &str = "hud-maker-add-part";
/// The side panel's Delete, for a part the player added.
pub const HUD_MAKER_DELETE_PART: &str = "hud-maker-delete-part";
/// The side panel's "Flash when you hit someone" switch on a label.
pub const HUD_MAKER_HIT_FLASH: &str = "hud-maker-hit-flash";
/// The swatch of the hit flash's colour, which points the colour picker at the flash.
pub const HUD_MAKER_HIT_FLASH_COLOUR: &str = "hud-maker-hit-flash-colour";

/// A kind in the Add menu: "hud-maker-add-part:<shadow|box|anchor|crosshair>".
pub fn hud_maker_add_part(kind: NewPart) -> ElementId {
    let slug = match kind {
        NewPart::Shadow => "shadow",
        NewPart::Box => "box",
        NewPart::Anchor => "anchor",
        NewPart::Crosshair => "crosshair",
        _ => "other",
    };
    keyed("hud-maker-add-part", slug)
}

/// The cross an added anchor draws on the canvas: "hud-maker-anchor:<part key>".
pub fn hud_maker_anchor(key: &PartKey) -> ElementId {
    keyed("hud-maker-anchor", key.as_str())
}

/// The side panel's "Attach to" menu.
pub const HUD_MAKER_ATTACH: &str = "hud-maker-attach";
/// The side panel's Detach, while the selection is attached.
pub const HUD_MAKER_DETACH: &str = "hud-maker-detach";

/// A sibling in the "Attach to" menu: "hud-maker-attach-to:<part key>".
pub fn hud_maker_attach_to(key: &PartKey) -> ElementId {
    keyed("hud-maker-attach-to", key.as_str())
}

/// The selection's own corner in the attach grid: "hud-maker-pin-own:pin_topleft".
pub fn hud_maker_pin_own(corner: PinCorner) -> ElementId {
    keyed("hud-maker-pin-own", &corner.key_text().to_lowercase())
}

/// The sibling's corner in the attach grid: "hud-maker-pin-corner:pin_bottomright".
pub fn hud_maker_pin_corner(corner: PinCorner) -> ElementId {
    keyed("hud-maker-pin-corner", &corner.key_text().to_lowercase())
}

/// The timeline's moment against the events' real end: "0.1 s of 0.25 s".
pub const HUD_MAKER_TIMELINE_TIME: &str = "hud-maker-timeline-time";

/// An open colour row's swatch in the Animations section: "hud-maker-anim-colour:<event>#<row>".
pub fn hud_maker_anim_colour(event: &str, row: usize) -> ElementId {
    keyed("hud-maker-anim-colour", &format!("{event}#{row}"))
}

/// The Maps page's search box.
pub const MAPS_SEARCH: &str = "maps-search";
/// Wraps the Maps page's sort select.
pub const MAPS_SORT: &str = "maps-sort";
/// Labelled with the map count, such as "4368 maps" or "2 of 4368 maps".
pub const MAPS_COUNT: &str = "maps-count";
/// Fetches serveme.tf's map list again.
pub const MAPS_REFRESH: &str = "maps-refresh";
/// The Maps page's row list.
pub const MAPS_LIST: &str = "maps-list";
/// The map list's scrollbar, in a gutter just right of the rows.
pub const MAPS_LIST_SCROLLBAR: &str = "maps-list-scrollbar";
/// Labelled "Getting the map list..." or "No maps match your search" when no row shows.
pub const MAPS_EMPTY: &str = "maps-empty";
/// The text of a failed map list fetch.
pub const MAPS_ERROR: &str = "maps-error";
/// Retries a failed map list fetch.
pub const MAPS_RETRY: &str = "maps-retry";
/// The Maps page's "TF2 not found" warning, present only without a TF2 install.
pub const MAPS_NO_TF2: &str = "maps-no-tf2";

/// The logs.tf page's "Stats from logs.tf" credit, which opens logs.tf.
pub const LOGSTF_CREDIT: &str = "logstf-credit";
/// Labelled with the logs.tf page's explanation while no player is tracked.
pub const LOGSTF_EMPTY: &str = "logstf-empty";
/// The empty logs.tf page's "Add player" button.
pub const LOGSTF_EMPTY_ADD: &str = "logstf-empty-add";
/// The "+ Add player" chip after the tracked players.
pub const LOGSTF_ADD: &str = "logstf-add";
/// The add-player dialog's Steam link or ID field.
pub const LOGSTF_ADD_INPUT: &str = "logstf-add-input";
/// The add-player dialog's Add button, disabled while the field is blank.
pub const LOGSTF_ADD_CONFIRM: &str = "logstf-add-confirm";
/// The add-player dialog's Cancel button.
pub const LOGSTF_ADD_CANCEL: &str = "logstf-add-cancel";
/// Labelled with why the add-player dialog could not add the player.
pub const LOGSTF_ADD_ERROR: &str = "logstf-add-error";
/// Forces a fresh logs.tf sync of the active player.
pub const LOGSTF_REFRESH: &str = "logstf-refresh";
/// "Open profile on logs.tf" for the active player.
pub const LOGSTF_PROFILE: &str = "logstf-profile";
/// Labelled "Downloading logs: <done> of <total>" while a sync downloads logs.
pub const LOGSTF_PROGRESS: &str = "logstf-progress";
/// Labelled with why the last logs.tf sync failed.
pub const LOGSTF_ERROR: &str = "logstf-error";
/// Retries a failed logs.tf sync.
pub const LOGSTF_RETRY: &str = "logstf-retry";

/// Labelled "Last <range>, <n> logs" (or "All logs, <n>") under the class grid, once the
/// player has logs.
pub const LOGSTF_SUMMARY: &str = "logstf-summary";
/// The history range slider beside the summary, six stops from 1 month to All.
pub const LOGSTF_RANGE: &str = "logstf-range";

/// One stop's label under the history range slider, a click picks it:
/// "logstf-range-stop:<key>".
pub fn logstf_range_stop(range: LogsRange) -> ElementId {
    keyed("logstf-range-stop", range.key())
}

/// One class's card in the logs.tf averages grid: "logstf-class:<class>".
pub fn logstf_class(class: TfClass) -> ElementId {
    keyed("logstf-class", &class.name().to_lowercase())
}

/// A class card's rate name, by its label: "logstf-class-label:<class>:<label>".
pub fn logstf_class_label(class: TfClass, label: &str) -> ElementId {
    keyed(
        "logstf-class-label",
        &format!("{}:{label}", class.name().to_lowercase()),
    )
}

/// A class card's rate value, labelled with its text: "logstf-class-value:<class>:<label>".
pub fn logstf_class_value(class: TfClass, label: &str) -> ElementId {
    keyed(
        "logstf-class-value",
        &format!("{}:{label}", class.name().to_lowercase()),
    )
}

/// The dimmed "Not played" on a class card with no time: "logstf-class-unplayed:<class>".
pub fn logstf_class_unplayed(class: TfClass) -> ElementId {
    keyed("logstf-class-unplayed", &class.name().to_lowercase())
}

/// The logs.tf page's scrolling list: the chips and grid as its first item, then the logs.
pub const LOGSTF_LIST: &str = "logstf-list";
/// The scrollbar beside the logs.tf page's list.
pub const LOGSTF_LIST_SCROLLBAR: &str = "logstf-list-scrollbar";
/// Labelled "No logs on logs.tf for this player yet." when the active player has no logs.
pub const LOGSTF_NO_LOGS: &str = "logstf-no-logs";
/// The link to the active player's logs.tf profile beside the no-logs line.
pub const LOGSTF_NO_LOGS_PROFILE: &str = "logstf-no-logs-profile";

/// One log's row in the logs.tf list, which opens or closes it: "logstf-log:<id>".
pub fn logstf_log(id: u64) -> ElementId {
    keyed("logstf-log", &id.to_string())
}

/// The open log's panel under its row: "logstf-log-panel:<id>".
pub fn logstf_log_panel(id: u64) -> ElementId {
    keyed("logstf-log-panel", &id.to_string())
}

/// The open log's loading or failure line in its panel: "logstf-log-status:<id>".
pub fn logstf_log_status(id: u64) -> ElementId {
    keyed("logstf-log-status", &id.to_string())
}

/// "Try again" on the open log's failure line, which fetches it again: "logstf-log-retry:<id>".
pub fn logstf_log_retry(id: u64) -> ElementId {
    keyed("logstf-log-retry", &id.to_string())
}

/// A row's note in place of its stats, labelled with why it has none: "logstf-log-note:<id>".
pub fn logstf_log_note(id: u64) -> ElementId {
    keyed("logstf-log-note", &id.to_string())
}

/// "Open on logs.tf" for one log, in its panel or on an unreadable row: "logstf-log-open:<id>".
pub fn logstf_log_open(id: u64) -> ElementId {
    keyed("logstf-log-open", &id.to_string())
}

/// The DPM column title over the logs.tf rows.
pub const LOGSTF_HEAD_DPM: &str = "logstf-head-dpm";

/// A log row's damage-per-minute cell: "logstf-log-dpm:<id>".
pub fn logstf_log_dpm(id: u64) -> ElementId {
    keyed("logstf-log-dpm", &id.to_string())
}

/// A log row's K/D/A cell: "logstf-log-kda:<id>".
pub fn logstf_log_kda(id: u64) -> ElementId {
    keyed("logstf-log-kda", &id.to_string())
}

/// A log row's icon for a class the player played: "logstf-log-class:<id>:<class>".
pub fn logstf_log_class(id: u64, class: TfClass) -> ElementId {
    keyed(
        "logstf-log-class",
        &format!("{id}:{}", class.name().to_lowercase()),
    )
}

/// A log row's title cell: "logstf-log-title:<id>".
pub fn logstf_log_title(id: u64) -> ElementId {
    keyed("logstf-log-title", &id.to_string())
}

/// A log row's map, its own column or a line under the title, labelled with the map name:
/// "logstf-log-map:<id>".
pub fn logstf_log_map(id: u64) -> ElementId {
    keyed("logstf-log-map", &id.to_string())
}

/// The open log's team table, which scrolls sideways in a narrow window.
pub const LOGSTF_TEAMS_SCROLL: &str = "logstf-teams-scroll";

/// The open log's players table, which scrolls sideways in a narrow window.
pub const LOGSTF_PLAYERS_SCROLL: &str = "logstf-players-scroll";

/// One player's name cell in the open log's players table: "logstf-player-name:<id64>".
pub fn logstf_player_name(id: SteamId) -> ElementId {
    keyed("logstf-player-name", &id.id64().to_string())
}

/// The open log's score banner, labelled "BLU 2, RED 3": "logstf-banner:<id>".
pub fn logstf_banner(id: u64) -> ElementId {
    keyed("logstf-banner", &id.to_string())
}

/// One cell of the open log's team table, labelled with its value:
/// "logstf-team-cell:blu:Midfights".
pub fn logstf_team_cell(team: Team, column: &str) -> ElementId {
    keyed("logstf-team-cell", &format!("{}:{column}", team_slug(team)))
}

/// One team's half of open log `id`'s score banner: "logstf-banner-team:<id>:red".
pub fn logstf_banner_team(id: u64, team: Team) -> ElementId {
    keyed("logstf-banner-team", &format!("{id}:{}", team_slug(team)))
}

fn team_slug(team: Team) -> &'static str {
    match team {
        Team::Red => "red",
        Team::Blue | _ => "blu",
    }
}

/// A players table header in the open log, which sorts by its column, keyed by its title:
/// "logstf-sort:K/D".
pub fn logstf_sort(title: &str) -> ElementId {
    keyed("logstf-sort", title)
}

/// The arrow on the sorted players header, labelled "high to low" or "low to high":
/// "logstf-sort-arrow:K/D".
pub fn logstf_sort_arrow(title: &str) -> ElementId {
    keyed("logstf-sort-arrow", title)
}

/// One player's line in the open log's players table: "logstf-player:<id64>".
pub fn logstf_player_row(id: SteamId) -> ElementId {
    keyed("logstf-player", &id.id64().to_string())
}

/// A player's class icon in the open log's players table, which opens its class card on hover:
/// "logstf-class-card:<id64>:<class>".
pub fn logstf_class_card(id: SteamId, class: TfClass) -> ElementId {
    keyed("logstf-class-card", &player_class(id, class))
}

/// The hover card around a class icon, keying its open state: "logstf-class-hover:<id64>:<class>".
pub fn logstf_class_hover(id: SteamId, class: TfClass) -> ElementId {
    keyed("logstf-class-hover", &player_class(id, class))
}

/// The open class card's content: "logstf-class-popover:<id64>:<class>".
pub fn logstf_class_popover(id: SteamId, class: TfClass) -> ElementId {
    keyed("logstf-class-popover", &player_class(id, class))
}

/// One weapon's row in a class card, labelled "scattergun 7 (100%) 3334 (86%) 56%":
/// "logstf-weapon:<id64>:<class>:<weapon>".
pub fn logstf_weapon(id: SteamId, class: TfClass, weapon: &str) -> ElementId {
    keyed(
        "logstf-weapon",
        &format!("{}:{weapon}", player_class(id, class)),
    )
}

/// A column's header in a class card, by its title ("K", "DA"):
/// "logstf-card-column:<id64>:<class>:<title>".
pub fn logstf_card_column(id: SteamId, class: TfClass, title: &str) -> ElementId {
    keyed(
        "logstf-card-column",
        &format!("{}:{title}", player_class(id, class)),
    )
}

fn player_class(id: SteamId, class: TfClass) -> String {
    format!("{}:{}", id.id64(), class.name().to_lowercase())
}

/// The open log's rounds section: "logstf-rounds".
pub const LOGSTF_ROUNDS: &str = "logstf-rounds";

/// The sideways scroll around the open log's rounds table.
pub const LOGSTF_ROUNDS_SCROLL: &str = "logstf-rounds-scroll";

/// The rounds table's header line: "logstf-rounds-head".
pub const LOGSTF_ROUNDS_HEAD: &str = "logstf-rounds-head";

/// The header line over an open round's players: "logstf-round-players-head:<log>:<n>".
pub fn logstf_round_players_head(log: u64, round: usize) -> ElementId {
    keyed("logstf-round-players-head", &format!("{log}:{round}"))
}

/// One round's row in the open log, which opens or closes it: "logstf-round:<log>:<n>".
pub fn logstf_round(log: u64, round: usize) -> ElementId {
    keyed("logstf-round", &format!("{log}:{round}"))
}

/// An open round's players and events under its row: "logstf-round-panel:<log>:<n>".
pub fn logstf_round_panel(log: u64, round: usize) -> ElementId {
    keyed("logstf-round-panel", &format!("{log}:{round}"))
}

/// A round's running score, labelled "0 - 1" (BLU - RED): "logstf-round-score:<log>:<n>".
pub fn logstf_round_score(log: u64, round: usize) -> ElementId {
    keyed("logstf-round-score", &format!("{log}:{round}"))
}

/// A round's midfight winner, labelled "BLU", "RED" or "-": "logstf-round-midfight:<log>:<n>".
pub fn logstf_round_midfight(log: u64, round: usize) -> ElementId {
    keyed("logstf-round-midfight", &format!("{log}:{round}"))
}

/// One player's line in an open round: "logstf-round-player:<log>:<n>:<id64>".
pub fn logstf_round_player(log: u64, round: usize, id: SteamId) -> ElementId {
    keyed(
        "logstf-round-player",
        &format!("{log}:{round}:{}", id.id64()),
    )
}

/// One event in an open round, labelled with its time, type and description:
/// "logstf-event:<log>:<n>:<ix>".
pub fn logstf_event(log: u64, round: usize, ix: usize) -> ElementId {
    keyed("logstf-event", &format!("{log}:{round}:{ix}"))
}

/// One Medic's block in the open log: "logstf-medic:<id64>".
pub fn logstf_medic(id: SteamId) -> ElementId {
    keyed("logstf-medic", &id.id64().to_string())
}

/// The open log's Medics section: "logstf-medics".
pub const LOGSTF_MEDICS: &str = "logstf-medics";

/// One row of a Medic's block, labelled with its value: "logstf-medic-fact:<id64>:<label>".
/// Charges rows use "Charges:<medigun key>" as their label.
pub fn logstf_medic_fact(id: SteamId, label: &str) -> ElementId {
    keyed("logstf-medic-fact", &format!("{}:{label}", id.id64()))
}

/// One heal target's share in a Medic's block, labelled with it:
/// "logstf-heal-target:<medic id64>:<target id64>".
pub fn logstf_heal_target(medic: SteamId, target: SteamId) -> ElementId {
    keyed(
        "logstf-heal-target",
        &format!("{}:{}", medic.id64(), target.id64()),
    )
}

/// One heal target's row in a Medic's block: "logstf-heal-row:<medic id64>:<target id64>".
pub fn logstf_heal_row(medic: SteamId, target: SteamId) -> ElementId {
    keyed(
        "logstf-heal-row",
        &format!("{}:{}", medic.id64(), target.id64()),
    )
}

/// The open log's kill matrix section: "logstf-matrix".
pub const LOGSTF_MATRIX: &str = "logstf-matrix";
/// The kill matrix's total header, labelled K, KA or D by tab: "logstf-matrix-total".
pub const LOGSTF_MATRIX_TOTAL: &str = "logstf-matrix-total";
/// The kill matrix's tab group: "logstf-matrix-tabs".
pub const LOGSTF_MATRIX_TABS: &str = "logstf-matrix-tabs";
/// The kill matrix's sideways scroll: "logstf-matrix-scroll".
pub const LOGSTF_MATRIX_SCROLL: &str = "logstf-matrix-scroll";

/// One kill matrix tab, selected while shown: "logstf-matrix-tab:kills" | "kills-assists" |
/// "deaths".
pub fn logstf_matrix_tab(kind: MatrixKind) -> ElementId {
    keyed("logstf-matrix-tab", matrix_key(kind))
}

fn matrix_key(kind: MatrixKind) -> &'static str {
    match kind {
        MatrixKind::KillsAssists => "kills-assists",
        MatrixKind::Deaths => "deaths",
        MatrixKind::Kills | _ => "kills",
    }
}

/// The button inside a kill matrix tab: "logstf-matrix-tab-button:<kind>", keyed as the tab.
pub fn logstf_matrix_tab_button(kind: MatrixKind) -> ElementId {
    keyed("logstf-matrix-tab-button", matrix_key(kind))
}

/// One player's row of the kill matrix: "logstf-matrix-row:<id64>".
pub fn logstf_matrix_row(id: SteamId) -> ElementId {
    keyed("logstf-matrix-row", &id.id64().to_string())
}

/// One count in a player's kill matrix row: "logstf-matrix-cell:<id64>:<class>", or
/// "logstf-matrix-cell:<id64>:k" for the total.
pub fn logstf_matrix_cell(id: SteamId, class: Option<TfClass>) -> ElementId {
    match class {
        Some(class) => keyed("logstf-matrix-cell", &player_class(id, class)),
        None => keyed("logstf-matrix-cell", &format!("{}:k", id.id64())),
    }
}

/// The open log's Show Chat button: "logstf-chat-toggle".
pub const LOGSTF_CHAT_TOGGLE: &str = "logstf-chat-toggle";
/// The open log's Show Killstreaks button, disabled without streaks: "logstf-streaks-toggle".
pub const LOGSTF_STREAKS_TOGGLE: &str = "logstf-streaks-toggle";
/// The chat dialog's scrolling list: "logstf-chat".
pub const LOGSTF_CHAT: &str = "logstf-chat";
/// The killstreaks dialog's scrolling list: "logstf-streaks".
pub const LOGSTF_STREAKS: &str = "logstf-streaks";

/// One line of the chat dialog, labelled "<name>: <msg>": "logstf-chat-line:<ix>".
pub fn logstf_chat_line(ix: usize) -> ElementId {
    keyed("logstf-chat-line", &ix.to_string())
}

/// One row of the killstreaks dialog, labelled "0:39 ORANGEjuice 3": "logstf-streak:<ix>".
pub fn logstf_streak(ix: usize) -> ElementId {
    keyed("logstf-streak", &ix.to_string())
}

/// The player's name in one killstreaks row, labelled with it: "logstf-streak-name:<ix>".
pub fn logstf_streak_name(ix: usize) -> ElementId {
    keyed("logstf-streak-name", &ix.to_string())
}

/// The open log's Download log button: "logstf-download:<log id>".
pub fn logstf_download(id: u64) -> ElementId {
    keyed("logstf-download", &id.to_string())
}

/// One tracked player's chip, selected while active: "logstf-chip:<id64>".
pub fn logstf_chip(id: SteamId) -> ElementId {
    keyed("logstf-chip", &id.id64().to_string())
}

/// The x beside a player's chip that asks to stop tracking them: "logstf-chip-remove:<id64>".
pub fn logstf_chip_remove(id: SteamId) -> ElementId {
    keyed("logstf-chip-remove", &id.id64().to_string())
}

/// One map's row: "map-row:<name>".
pub fn map_row(name: &str) -> ElementId {
    keyed("map-row", name)
}

/// A row's Download button, labelled with its percent while it runs: "map-download:<name>".
pub fn map_download(name: &str) -> ElementId {
    keyed("map-download", name)
}

/// A downloaded map's Remove button: "map-remove:<name>".
pub fn map_remove(name: &str) -> ElementId {
    keyed("map-remove", name)
}

/// A row's state, labelled "Downloaded" or "In game": "map-state:<name>".
pub fn map_state(name: &str) -> ElementId {
    keyed("map-state", name)
}

/// An Addon Store tab: "addon-store-tab:<key>", the key being "mastercomfig" or a section key.
pub fn addon_store_tab(key: &str) -> ElementId {
    keyed("addon-store-tab", key)
}

/// A GameBanana add-on card: "addon-gb-card:<section>:<id>".
pub fn addon_gb_card(section: &str, id: GbId) -> ElementId {
    keyed("addon-gb-card", &format!("{section}:{id}"))
}

/// A GameBanana add-on card's name: "addon-gb-card-name:<section>:<id>".
pub fn addon_gb_card_name(section: &str, id: GbId) -> ElementId {
    keyed("addon-gb-card-name", &format!("{section}:{id}"))
}

/// A GameBanana add-on card's thumbnail: "addon-gb-card-image:<section>:<id>".
pub fn addon_gb_card_image(section: &str, id: GbId) -> ElementId {
    keyed("addon-gb-card-image", &format!("{section}:{id}"))
}

/// Unblurs a mature card's thumbnail: "addon-gb-card-reveal:<section>:<id>".
pub fn addon_gb_card_reveal(section: &str, id: GbId) -> ElementId {
    keyed("addon-gb-card-reveal", &format!("{section}:{id}"))
}

/// A GameBanana add-on card's "Outdated" badge: "addon-gb-card-outdated:<section>:<id>".
pub fn addon_gb_card_outdated(section: &str, id: GbId) -> ElementId {
    keyed("addon-gb-card-outdated", &format!("{section}:{id}"))
}

/// A GameBanana add-on card's "Installed" or "Update" badge: "addon-gb-card-badge:<section>:<id>".
pub fn addon_gb_card_badge(section: &str, id: GbId) -> ElementId {
    keyed("addon-gb-card-badge", &format!("{section}:{id}"))
}

/// A GameBanana add-on card's like count: "addon-gb-card-likes:<section>:<id>".
pub fn addon_gb_card_likes(section: &str, id: GbId) -> ElementId {
    keyed("addon-gb-card-likes", &format!("{section}:{id}"))
}

/// One section's GameBanana add-ons on the Addons page, titled: "addons-gb-group:<key>".
pub fn addons_gb_group(key: &str) -> ElementId {
    keyed("addons-gb-group", key)
}

/// An installed GameBanana add-on's card on the Addons page: "addons-gb-card:<id>".
pub fn addons_gb_card(id: GbId) -> ElementId {
    keyed("addons-gb-card", &id.to_string())
}

/// The card's "Remove" button: "addons-gb-remove:<id>".
pub fn addons_gb_remove(id: GbId) -> ElementId {
    keyed("addons-gb-remove", &id.to_string())
}

/// The card's "Update" button, present only with a newer file: "addons-gb-update:<id>".
pub fn addons_gb_update(id: GbId) -> ElementId {
    keyed("addons-gb-update", &id.to_string())
}

/// The card's "Install again" button, present only when not installed: "addons-gb-reinstall:<id>".
pub fn addons_gb_reinstall(id: GbId) -> ElementId {
    keyed("addons-gb-reinstall", &id.to_string())
}

/// The card's one loading button while an operation works on it: "addons-gb-busy:<id>".
pub fn addons_gb_busy(id: GbId) -> ElementId {
    keyed("addons-gb-busy", &id.to_string())
}

/// The card's "Open on GameBanana" button: "addons-gb-open:<id>".
pub fn addons_gb_open(id: GbId) -> ElementId {
    keyed("addons-gb-open", &id.to_string())
}

/// The card's state tag ("No longer on GameBanana", "Update available" or "Not installed"),
/// absent when installed and current: "addons-gb-tag:<id>".
pub fn addons_gb_tag(id: GbId) -> ElementId {
    keyed("addons-gb-tag", &id.to_string())
}

/// One credit in the add-on detail, by group and position: "addon-detail-credit:<g>:<i>".
pub fn addon_detail_credit(group: usize, index: usize) -> ElementId {
    keyed("addon-detail-credit", &format!("{group}:{index}"))
}

/// One file row in the add-on detail, by position: "addon-detail-file:<i>".
pub fn addon_detail_file(index: usize) -> ElementId {
    keyed("addon-detail-file", &index.to_string())
}

/// The "Choose what to install" dialog for an add-on with several parts.
pub const PAYLOAD_CHECKLIST: &str = "payload-checklist";
/// The checklist's title, "Choose what to install".
pub const PAYLOAD_CHECKLIST_TITLE: &str = "payload-checklist-title";
/// The checklist's description, naming the add-on.
pub const PAYLOAD_CHECKLIST_DESCRIPTION: &str = "payload-checklist-description";
/// The checklist's rows, which scroll when there are many.
pub const PAYLOAD_CHECKLIST_ROWS: &str = "payload-checklist-rows";
/// The scrollbar beside the checklist's rows.
pub const PAYLOAD_CHECKLIST_SCROLLBAR: &str = "payload-checklist-scrollbar";
/// The checklist's Install button, disabled while nothing is checked.
pub const PAYLOAD_INSTALL: &str = "payload-install";
/// The checklist's Cancel button.
pub const PAYLOAD_CANCEL: &str = "payload-cancel";

/// One part's checkbox in the checklist, by row: "payload-check:<i>".
pub fn payload_check(index: usize) -> ElementId {
    keyed("payload-check", &index.to_string())
}
