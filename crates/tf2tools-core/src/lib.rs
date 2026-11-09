//! UI-free core of TF2Tools: the data directory, the v5-compatible database,
//! settings, TF2 detection, file formats and catalogs. Blocking functions; the app
//! runs them off the UI thread. Network functions are `async` and must be polled inside
//! a tokio runtime.

#![deny(missing_docs)]
// The workspace denies these; unit tests and their helpers may still unwrap and panic.
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]

mod activity;
mod archive;
mod crosshair_catalog;
mod db;
mod disk;
mod error;
mod fonts;
mod hud_maker;
mod ice;
mod library;
mod old_v4;
mod pak;
pub mod panic_mark;
mod paths;
mod settings;
mod store;
mod templates;
mod texture;
mod tf2;
mod timestamp;
mod update;
mod wav;
mod weapon_icons;
mod weapons;

pub use activity::{ActivityLog, LOG_KEEP, LogEntry, LogLevel, LogQuery, NewLogEntry};
pub use archive::{extract_archive, find_content_root};
pub use crosshair_catalog::{BuiltinCrosshair, BuiltinCrosshairs};
pub use db::Database;
pub use error::{
    ApplyModulesError, ApplyPresetError, ApplyProfileError, ArchiveError, BorrowSourceError,
    BrowseError, CatalogError, CopyHudError, DataDirError, DetectError, DownloadError, FilesError,
    FontCheckError, FontInstallError, HistoryError, HudFilesError, HudLoadError, ImageError,
    ImportError, InstallError, InstallMastercomfigError, KeepHudError, KvParseError, LibraryError,
    LogError, MakeHudError, MapDownloadError, MapRemoveError, MastercomfigStatusError,
    ModuleCatalogError, OpenError, PaintError, PngReadError, PngWriteError, PresetError,
    PreviewError, ProfileError, QueryError, RecipeError, ReinstallError, RemoveMastercomfigError,
    RestoreMastercomfigError, SchemaError, SettingsError, SyncError, TakeOverError,
    UpdateApplyError, UpdateCheckError, UpdateCopyError, UpdateDownloadError, VpkError, VtfError,
    WavError,
};
pub use fonts::{
    FontCheck, FontHost, FontHostError, FontInstallReport, FontIssue, FontSkipReason, HudFont,
    HudFonts, PROTECTED_FONT_NAMES, Platform, check_hud_fonts, install_hud_fonts, scan_hud_fonts,
    system_font_host,
};
pub use hud_maker::MeterKind;
pub use hud_maker::{
    ALIGNMENTS, AddBlock, Align, AnimationAppend, AnimationChange, AssetFile, AxisSize, Backdrop,
    BlockLine, BoardMode, BorrowRefused, BorrowSource, Borrowed, Brings, CLONE_PREFIX, CfgFile,
    CfgPlace, Change, ChangeTarget, ChatLine, ChatSample, Choice, ClassSchema, CodeLabel,
    CodeToggled, ColumnKind, CopyBase, CopyView, CreditNote, DeathNotices, Derived, EditRefused,
    Edited, FontEdit, FontFile, FontVariant, GameText, Guide, GuideAxis, HiddenBy, HideTechnique,
    HudCrosshair, HudFile, HudFiles, HudImage, HudLayout, HudModel, HudOptions, HudTexture,
    ImageBorder, ImageCell, ImageProblem, KeyLine, KillIcon, KillIcons, KvDocument, KvEntry,
    KvEntryValue, KvValueId, LeftBehind, ListColumn, MIN_CONTRAST, MakerBase, MakerFile, MvmStats,
    NoMirror, NoticeLine, OptionAction, OptionChange, OptionKind, OptionNeed, OptionSource,
    OptionSpec, OptionValue, PanelRule, Part, PartBox, PartCaps, PartId, PartKey, Pick, PinCorner,
    PingLevel, PlayerListLayout, Point, PosExpr, PosTerm, PosUnit, PropertyKind, PropertySpec,
    RankControl, RankLayout, RankPieces, Recipe, RecipeApplied, Rect, RemoveKey, Rgba,
    SampleKiller, SamplePlayer, SampleSet, SampleWatched, Scene, SceneRule, Scheme, Screen,
    ScreenBaseline, ScreenCheck, ScreenFlag, ScreenProblem, ScreenScale, SetKey, ShowsFor, Size,
    SizeExpr, SizeForm, Snap, StartingPoint, SwitchAction, Switched, Team, TermOp, ValueMap,
    Vision, check_hud_path, contrast_ratio, drawn_image, font_families, font_family, font_tall_px,
    image_cells, nine_slice, normalise_game_path, rank_medal, rank_pieces, relative_luminance,
    scene_image, stand_in_families, tint_pixels,
};
pub use hud_maker::{
    AnimCommand, AnimEvent, AnimFrame, AnimScript, Animated, Fired, GAME_TRIGGERS, GameSituation,
    HudAnimations, Interp, MAX_ANIM_FILES, MAX_COMMANDS, MAX_EVENTS, MAX_TOKEN, SituationSwitch,
    WeaponKind,
};
pub use hud_maker::{ChangeNote, both_changed};
pub use hud_maker::{HudSchema, HudSchemas, SchemaOptions, normalise_repo};
pub use hud_maker::{NewPart, PinRefused, RemoveRefused};
pub use hud_maker::{NewPicture, PictureTarget, picture_id, picture_material};
pub use hud_maker::{background_corners, corner_radius, paints_background};
pub use hud_maker::{wrap_lines, wrapped_text_left};
pub use library::{
    AddonEntry, AddonName, ApplyProfileReport, ApplyReport, ApplyStep, Assignment, CrosshairEntry,
    CrosshairFormat, CrosshairId, CrosshairRef, DraftCrosshair, DraftSlot, ExternalHud, GbId,
    HudAssets, HudEntry, HudId, HudInstall, HudStatus, InstalledCrosshairs, InstalledSound,
    InstalledSounds, Library, LibraryEntry, MadeHud, MakeTarget, MastercomfigStatus, MissingItem,
    MissingKind, PaintTarget, PaintedSource, Preset, PresetEntry, Profile, ProfileCrosshair,
    ProfileCrosshairRef, ProfileDraft, ProfileId, ProfileItem, ProfileUpdate, Provenance, Recovery,
    Reinstall, SavedHud, SkipReason, SkippedAssignment, SoundConflict, SoundEntry, SoundFormat,
    SoundId, SoundKind, crosshair_scripts_need_repair, pictures_fit, read_recipe_file,
};
pub use library::{
    Choices, ComfigModule, ModuleCatalog, ModuleCategory, ModuleChoice, ModuleValue, readable_name,
};
pub use library::{CopyUpdate, CopyUpdated, DownloadUpdate, TreeHash};
pub use library::{HudSnapshot, MAX_SNAPSHOTS, SnapshotEntry};
pub use old_v4::{OldV4, UninstallEntry, detect_old_v4, find_old_v4};
pub use pak::first_wav_in_vpk;
pub use paths::{DataDir, LibraryKind};
pub use settings::{Settings, Volume};
pub use store::{
    AddonSection, DownloadPhase, DownloadProgress, Endpoints, MastercomfigRelease, StoreCard,
    StoreClient, StoreConfig, StoreCursor, StoreFile, StoreItem, StoreItemId, StoreKind,
    StoreQuery, StoreSort, StoreSyncStatus, SyncOutcome, SyncProgress,
};
pub use store::{CommunityMap, MapEntry, MapState, parse_serveme_maps};
pub use texture::{
    FitChange, PNG_MAX_FILE_BYTES, PNG_MAX_SIDE, RgbaImage, decode_vtf, fit_hud_texture,
    fit_to_maker, hud_texture_vtf, menu_textures, png_to_vtf, read_picture, read_png, rgba_to_vtf,
    write_png,
};
pub use tf2::{Tf2Install, tf2_running, tf2_running_in};
pub use update::{
    Applied, InstallTarget, StagedUpdate, UPDATE_PUBLIC_KEYS, UpdateAsset, UpdateClient,
    UpdateOffer, apply,
};
pub use wav::{WavInfo, inspect_wav};
pub use weapons::{TfClass, Weapon, WeaponSlot, weapons};
