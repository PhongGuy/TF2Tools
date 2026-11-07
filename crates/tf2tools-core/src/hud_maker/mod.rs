//! The HUD Maker's reading layer: lossless KeyValues documents and, in later stages, the
//! game's files, scheme, expressions and images. UI-free and policy-free.

mod animations;
mod background;
mod borrow;
mod changes;
mod chat;
mod compare;
mod document;
mod drawn_image;
mod edits;
mod expr;
mod files;
mod fonts;
mod game_text;
mod hud_crosshair;
mod hud_options;
mod hud_schema;
mod image_caps;
mod image_cells;
mod images;
mod kill_icons;
mod killfeed;
mod layout;
mod list;
mod maker_file;
mod menu_art;
mod meters;
mod model;
mod new_parts;
mod options;
mod pictures;
mod pins;
mod playback;
mod probe;
mod rank_panel;
mod recipe;
mod rules;
mod sample;
mod scene;
mod schema;
mod scheme;
mod screen_check;
mod situation;
mod snap;
mod snapshot;
mod starting;
mod text_wrap;
mod tint;
mod vision;

pub use animations::{
    AnimCommand, AnimEvent, AnimScript, GAME_TRIGGERS, HudAnimations, Interp, MAX_ANIM_FILES,
    MAX_COMMANDS, MAX_EVENTS, MAX_TOKEN,
};
pub(crate) use animations::{
    AnimationSave, MANIFEST as ANIM_MANIFEST, OWN_SCRIPT, animation_files,
};
pub use background::{background_corners, corner_radius, paints_background};
pub use borrow::{BorrowRefused, BorrowSource, Borrowed, Brings, LeftBehind};
pub(crate) use changes::is_device_name;
pub use changes::{
    AddBlock, AnimationAppend, AnimationChange, AssetFile, BlockLine, Change, ChangeTarget,
    CreditNote, OptionChange, RemoveKey, SetKey, check_hud_path,
};
pub use chat::{ChatLine, ChatSample};
pub(crate) use compare::left_out;
pub use compare::{ChangeNote, both_changed};
pub use document::{KvDocument, KvEntry, KvEntryValue, KvValueId};
pub use drawn_image::{drawn_image, scene_image};
pub use edits::{EditRefused, NoMirror, PartCaps};
pub use expr::{
    Align, AxisSize, PosExpr, PosTerm, PosUnit, ScreenScale, SizeExpr, SizeForm, TermOp,
};
pub use files::{CfgFile, CopyView, HudFile, HudFiles, normalise_game_path};
pub(crate) use fonts::{BORROWED_DIR, FONT_DIR, IMAGES_DIR, KEPT_DIR};
pub use fonts::{CLONE_PREFIX, FontEdit, FontFile, font_families, font_family, stand_in_families};
pub use game_text::GameText;
pub use hud_crosshair::HudCrosshair;
pub use hud_options::{HudOptions, Switched};
pub use hud_schema::{HudSchema, HudSchemas, SchemaOptions, normalise_repo};
pub(crate) use image_caps::{MAX_IMAGE_BYTES, within_caps};
pub use image_cells::{ImageCell, image_cells, nine_slice};
pub use images::{HudImage, HudTexture, ImageProblem};
pub use kill_icons::{KillIcon, KillIcons};
pub use killfeed::{DeathNotices, NoticeLine};
pub use layout::{HiddenBy, HudLayout, PartBox, Pick, Screen};
pub use list::{ColumnKind, ListColumn, PlayerListLayout};
pub use maker_file::{CopyBase, MakerBase, MakerFile};
pub use meters::MeterKind;
pub use model::{Edited, HudModel, KeyLine, Part, PartKey};
pub use new_parts::{NewPart, RemoveRefused};
pub use options::{
    CfgPlace, Choice, Derived, OptionAction, OptionKind, OptionNeed, OptionSource, OptionSpec,
    OptionValue, SwitchAction, ValueMap,
};
pub(crate) use pictures::is_picture_path;
pub use pictures::{NewPicture, PictureTarget, picture_id, picture_material};
pub use pins::PinRefused;
pub use playback::{AnimFrame, Animated, Fired};
pub use probe::font_tall_px;
pub use rank_panel::{RankControl, RankLayout, RankPieces, rank_medal, rank_pieces};
pub use recipe::{Recipe, RecipeApplied};
pub use rules::{HideTechnique, PanelRule, PartId, PinCorner};
pub use sample::{
    CodeLabel, MvmStats, PingLevel, SampleKiller, SamplePlayer, SampleSet, SampleWatched, Team,
};
pub use scene::{BoardMode, CodeToggled, Scene, SceneRule, ShowsFor};
pub use schema::{ALIGNMENTS, ClassSchema, PropertyKind, PropertySpec};
pub use scheme::{FontVariant, ImageBorder, Rgba, Scheme};
pub use screen_check::{ScreenBaseline, ScreenCheck, ScreenFlag, ScreenProblem};
pub use situation::{GameSituation, SituationSwitch, WeaponKind};
pub use snap::{Guide, GuideAxis, Snap};
pub(crate) use snapshot::SnapshotFile;
pub use starting::StartingPoint;
pub use text_wrap::{wrap_lines, wrapped_text_left};
pub use tint::tint_pixels;
pub use vision::{Backdrop, MIN_CONTRAST, Vision, contrast_ratio, relative_luminance};
