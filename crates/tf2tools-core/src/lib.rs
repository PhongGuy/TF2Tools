//! UI-free core of TF2Tools: the data directory, the v5-compatible database,
//! settings, TF2 detection, file formats and catalogs. Blocking functions; the app
//! runs them off the UI thread. Network functions are `async` and must be polled inside
//! a tokio runtime.

#![deny(missing_docs)]
// The workspace denies these; unit tests and their helpers may still unwrap and panic.
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]

mod activity;
mod archive;
mod cleanup;
mod crosshair_catalog;
pub mod cursor;
mod db;
mod demo_view;
mod demostf;
mod disk;
mod error;
mod fonts;
mod game_icons;
mod hud_maker;
mod ice;
mod library;
mod logstf;
mod map_payloads;
mod old_v4;
mod pak;
pub mod panic_mark;
mod paths;
mod payloads;
mod setting_pictures;
mod settings;
mod store;
mod templates;
#[cfg(test)]
mod test_archives;
mod texture;
mod tf2;
mod timestamp;
mod update;
mod wav;
mod weapon_icons;
mod weapon_names;
mod weapons;

pub use activity::{ActivityLog, LOG_KEEP, LogEntry, LogLevel, LogQuery, NewLogEntry};
pub use archive::{extract_archive, find_content_root};
pub use cleanup::{CleanupRoots, Freed, sweep_game_leftovers, sweep_leftovers};
pub use crosshair_catalog::{BuiltinCrosshair, BuiltinCrosshairs};
pub use db::Database;
pub use demo_view::{
    CameraMode, CameraView, DemoHeader, DemoKill, DemoSnapshot, DemoTestKill, DemoTestParty,
    DemoTestShot, DemoTracks, EyePose, FlyInput, FollowPose, FreeFly, KillParty, MAX_DEMO_BYTES,
    MAX_KILLS, MAX_MAP_BYTES, MAX_MAP_TRIANGLES, MAX_PROJECTILE_SAMPLES, MAX_SNAPSHOT_BYTES,
    MAX_TRACK_SAMPLES, MapMesh, PlaybackClock, PlaybackEvent, PlayerId, ProjectileKind,
    ProjectileView, SnapshotPlayer, SpectatorCamera, TargetLife, TrackCut, load_map_file,
    load_map_file_capped, load_map_mesh, read_demo_header, read_demo_playback, read_demo_snapshot,
    read_demo_tracks,
};
// The logs page already owns `TrackedPlayer` at the root, so the demo one is renamed here.
pub use demo_view::TrackedPlayer as DemoTrackedPlayer;
pub use demostf::{
    DEMO_CACHE_HIGH, DEMO_CACHE_LOW, DEMO_LIMIT, DemoCache, DemoCursor, DemoDetail, DemoFormat,
    DemoListQuery, DemoPage, DemoPlayer, DemoQuery, DemoSummary, DemosClient, DemosEndpoints,
    DemosRange, merge_demos, pick_demo,
};
pub use error::{
    ApplyModulesError, ApplyPresetError, ApplyProfileError, ArchiveError, BorrowSourceError,
    BrowseError, CatalogError, CopyHudError, DataDirError, DemoCacheError, DemoClearError,
    DemoHeaderError, DemoSnapshotError, DemoTracksError, DemosError, DetectError, DownloadError,
    FilesError, FontCheckError, FontInstallError, GbAddonCheckError, GbAddonFetchError,
    GbAddonInstallError, GbMapInstallError, GbMapRemoveError, GbMapSearchError, HistoryError,
    HudFilesError, HudLoadError, ImageError, ImportError, InstallError, InstallMastercomfigError,
    KeepHudError, KvParseError, LibraryError, LogError, LogParseError, LogsCacheError,
    LogsFetchError, LogsSyncError, MakeHudError, MapDownloadError, MapMeshError, MapRemoveError,
    MastercomfigStatusError, ModuleCatalogError, OpenError, PaintError, PlayerInputError,
    PngReadError, PngWriteError, PresetError, PreviewError, ProfileError, QueryError, RecipeError,
    ReinstallError, RemoveMastercomfigError, ResolveError, RestoreMastercomfigError, SchemaError,
    SettingPicturesError, SettingsError, StorageError, SyncError, TakeOverError, UpdateApplyError,
    UpdateCheckError, UpdateCopyError, UpdateDownloadError, VpkError, VtfError, WavError,
    WeaponIconsError, WeaponNamesError,
};
pub use fonts::{
    FontCheck, FontHost, FontHostError, FontInstallReport, FontIssue, FontSkipReason, HudFont,
    HudFonts, PROTECTED_FONT_NAMES, Platform, check_hud_fonts, install_hud_fonts, scan_hud_fonts,
    system_font_host,
};
pub use game_icons::GameWeaponIcons;
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
    RankControl, RankLabel, RankLayout, RankPieces, Recipe, RecipeApplied, Rect, RemoveKey, Rgba,
    SampleKiller, SamplePlayer, SampleSet, SampleWatched, Scene, SceneRule, Scheme, Screen,
    ScreenBaseline, ScreenCheck, ScreenFlag, ScreenProblem, ScreenScale, SetKey, ShowsFor, Size,
    SizeExpr, SizeForm, Snap, StartingPoint, SwitchAction, Switched, Team, TeamStatusPanel, TermOp,
    ValueMap, Vision, check_hud_path, contrast_ratio, drawn_image, font_families, font_family,
    font_tall_px, image_cells, nine_slice, normalise_game_path, rank_medal, rank_pieces,
    relative_luminance, scene_image, stand_in_families, tint_pixels,
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
    AddonEntry, AddonName, ApplyProfileReport, ApplyReport, ApplySkipReason, ApplyStep, Assignment,
    CrosshairEntry, CrosshairFormat, CrosshairId, CrosshairRef, CurrentSetup, DraftCrosshair,
    DraftItem, ExternalHud, GbAddonCheck, GbAddonEntry, GbId, HudAssets, HudEntry, HudId,
    HudInstall, HudStatus, InstalledCrosshairs, InstalledSound, InstalledSounds, Library,
    LibraryEntry, MadeHud, MakeTarget, MastercomfigStatus, MissingItem, MissingKind, MoveDirection,
    MovementKeys, PaintTarget, PaintedSource, Preset, PresetEntry, Profile, ProfileAddons,
    ProfileCrosshair, ProfileCrosshairRef, ProfileDraft, ProfileGraphics, ProfileId, ProfileItem,
    ProfileSounds, ProfileUpdate, Provenance, Recovery, Reinstall, SavedHud, SkipReason,
    SkippedAssignment, SlotMode, SoundConflict, SoundEntry, SoundFormat, SoundId, SoundKind,
    crosshair_scripts_need_repair, pictures_fit, read_recipe_file,
};
pub use library::{
    Choices, ComfigModule, ModuleCatalog, ModuleCategory, ModuleChoice, ModuleValue, readable_name,
};
pub use library::{ConvertReport, KindConverted, LibrarySpace, StorageKind, StorageUse};
pub use library::{CopyUpdate, CopyUpdated, DownloadUpdate, TreeHash};
pub use library::{HudSnapshot, MAX_SNAPSHOTS, SnapshotEntry};
pub use logstf::{
    ClassAverage, ClassLine, ClassWeapons, EventKind, HealTarget, KeepFullLogs, Killstreak,
    LOG_BODY_LIMIT, LOG_ZIP_LIMIT, LOGS_LONG_SPACING, LOGS_SPACING, LONG_PULL, LogChatLine,
    LogColumns, LogDetail, LogRow, LogState, LogsCache, LogsCancel, LogsClient, LogsEndpoints,
    LogsProgress, LogsRange, LogsSpace, LogsSyncSummary, MatrixKind, MatrixRow, MedicLine, Medigun,
    OpenedBody, PROVISIONAL_SECS, PlayerInput, PlayerLine, RoundEvent, RoundLine, RoundPlayer,
    RoundTeam, STALE_SECS, STEAM_ID64_BASE, SteamId, TeamTotals, TrackedPlayer, TrimReport,
    WeaponBackfill, WeaponLine, WeaponSum, leaderboard_icon_stem, logstf_class, parse_log,
    parse_player_input, window_start,
};
pub use map_payloads::{MAX_MAPS, MapPayload, find_maps};
pub use old_v4::{OldV4, UninstallEntry, detect_old_v4, find_old_v4};
pub use pak::first_wav_in_vpk;
pub use paths::{DataDir, LibraryKind};
pub use payloads::{MAX_PAYLOADS, Payload, PayloadKind, find_payloads};
pub use setting_pictures::shoot;
pub use setting_pictures::{
    Crop, PictureManifest, REASON_KEYS, Reload, SettingPicture, SettingPictures, SettingShot,
    ShotStep, ValueShot,
};
pub use settings::{LookSensitivity, Settings, Volume};
pub use store::{
    AddonCard, AddonCatalog, AddonCategory, AddonCredit, AddonCreditGroup, AddonCursor, AddonFile,
    AddonProfile, AddonQuery, AddonSection, AddonSort, AddonStep, DownloadPhase, DownloadProgress,
    Endpoints, GbAddonDownload, GbMapCard, GbMapCursor, GbMapDownload, GbMapGamemode, GbMapLibrary,
    GbMapQuery, GbMapSearchProgress, GbMapShow, GbMapSort, INDEX_STEP_CALLS, MastercomfigRelease,
    SEARCH_STEP_CALLS, STEP_CARDS, StoreCard, StoreClient, StoreConfig, StoreCursor, StoreFile,
    StoreItem, StoreItemId, StoreKind, StoreQuery, StoreSort, StoreSyncStatus, SyncOutcome,
    SyncProgress,
};
pub use store::{
    CommunityMap, MapEntry, MapSource, MapState, find_installed_map, parse_serveme_maps,
};
pub use texture::{
    FitChange, PNG_MAX_FILE_BYTES, PNG_MAX_SIDE, RgbaImage, decode_vtf, fit_hud_texture,
    fit_to_maker, hud_texture_vtf, menu_textures, png_to_vtf, read_picture, read_png, rgba_to_vtf,
    write_png,
};
pub use tf2::{Tf2Install, tf2_running, tf2_running_in};
pub use update::{
    Applied, InstallTarget, NotesGroup, NotesItem, NotesKind, NotesStatus, ReleaseNotes,
    StagedUpdate, UPDATE_PUBLIC_KEYS, UpdateAsset, UpdateClient, UpdateOffer, apply,
};
pub use wav::{WavInfo, inspect_wav};
pub use weapon_icons::weapon_inventory_images;
pub use weapon_names::WeaponNames;
pub use weapons::{TfClass, Weapon, WeaponSlot, weapons};
