//! Where the app finds its on-disk assets (the crosshair catalog, backgrounds, slot icons),
//! and the embedded icons.

use std::borrow::Cow;
use std::path::{Path, PathBuf};

use gpui_kit::assets::Assets;
use gpui_kit::{AssetSource, SharedString};
use tf2tools_core::WeaponSlot;

/// The composite preview's backgrounds, v5's order; files are `<root>/backgrounds/<name>.png`.
pub const BACKGROUNDS: [&str; 3] = ["2fort", "badlands", "process"];

/// A directory holding the app's assets, checked to contain `crosshairs/crosshairs.json`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssetRoot(PathBuf);

/// No candidate directory held `crosshairs/crosshairs.json`.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("No asset folder with crosshairs/crosshairs.json was found. Searched: {}", .searched.iter().map(|p| p.display().to_string()).collect::<Vec<_>>().join(", "))]
pub struct AssetRootError {
    /// Every candidate given, in order.
    pub searched: Vec<PathBuf>,
}

impl AssetRoot {
    /// TF2TOOLS_ASSETS, then <current exe dir>/assets, then <exe dir's parent>/lib/tf2tools/assets
    /// (where the deb and AppImage put them), then (debug builds only)
    /// CARGO_MANIFEST_DIR/../../assets.
    pub fn resolve() -> Result<Self, AssetRootError> {
        let exe_dir = std::env::current_exe()
            .ok()
            .and_then(|exe| exe.parent().map(Path::to_path_buf));
        let dev = cfg!(debug_assertions)
            .then(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets"));
        Self::resolve_from(
            std::env::var_os("TF2TOOLS_ASSETS").map(PathBuf::from),
            exe_dir,
            dev,
        )
    }

    /// The same order over explicit candidates; `exe_dir` is the directory holding the binary,
    /// and its parent (if any) contributes `<parent>/lib/tf2tools/assets`.
    /// A candidate is valid when `<candidate>/crosshairs/crosshairs.json` is a file.
    pub fn resolve_from(
        env: Option<PathBuf>,
        exe_dir: Option<PathBuf>,
        dev: Option<PathBuf>,
    ) -> Result<Self, AssetRootError> {
        // The packages install the binary in usr/bin and the assets in usr/lib/tf2tools.
        let lib = exe_dir
            .as_deref()
            .and_then(Path::parent)
            .map(|p| p.join("lib").join("tf2tools").join("assets"));
        let candidates: Vec<PathBuf> = [env, exe_dir.map(|d| d.join("assets")), lib, dev]
            .into_iter()
            .flatten()
            .collect();
        match candidates
            .iter()
            .find(|c| c.join("crosshairs").join("crosshairs.json").is_file())
        {
            Some(found) => Ok(Self(found.clone())),
            None => Err(AssetRootError {
                searched: candidates,
            }),
        }
    }

    /// The root directory.
    pub fn path(&self) -> &Path {
        &self.0
    }

    /// `<root>/crosshairs`, the crosshair catalog and its textures.
    pub fn crosshairs_dir(&self) -> PathBuf {
        self.0.join("crosshairs")
    }

    /// `<root>/backgrounds/<name>.png`.
    pub fn background(&self, name: &str) -> PathBuf {
        self.0.join("backgrounds").join(format!("{name}.png"))
    }

    /// `<root>/weapon-icons/{primary,secondary,melee}.png`, v5's 16 px slot squares.
    pub fn slot_icon(&self, slot: WeaponSlot) -> PathBuf {
        // WeaponSlot is non-exhaustive here, so a future slot falls back to the generic icon.
        let file = match slot {
            WeaponSlot::Primary => "primary.png",
            WeaponSlot::Secondary => "secondary.png",
            WeaponSlot::Melee => "melee.png",
            _ => "default.png",
        };
        self.0.join("weapon-icons").join(file)
    }
}

mod extra {
    #![allow(missing_docs)]
    use gpui_kit::assets::icon_assets;
    icon_assets!(pub ExtraIcons, [
        Paintbrush, Volume2, Crosshair, Layers, Puzzle, Store, ScrollText, PanelLeft, Monitor,
        PackageOpen, Unplug, Download, Trash, Square, VolumeX, Target, Upload, Pencil, Save, WifiOff, X,
        Library, MessageCircle, LocateFixed, Eraser, PaintBucket, FlipHorizontal2, FlipVertical2,
        Undo2, Redo2, FilePlus, Grid3x3, SquareDashed, ZoomIn, ZoomOut, Slash, Circle, SquareSquare,
        Shapes, ImageUp, ImageDown, LayoutDashboard, PanelRight, AppWindow, ChevronUp, ChevronDown,
        RotateCcw, ArrowUpLeft, ArrowUpRight, ArrowDownLeft, ArrowDownRight, Dot, Map, Gamepad2,
        CircleQuestionMark, Sparkles, Gauge,
    ]);
}
use extra::ExtraIcons;

/// The app's asset source: the embedded sidebar icons, then the kit's default bundle.
/// A path neither holds loads as `Ok(None)`.
#[derive(Clone, Copy, Debug, Default)]
pub struct AppAssets;

impl AssetSource for AppAssets {
    fn load(&self, path: &str) -> gpui_kit::Result<Option<Cow<'static, [u8]>>> {
        if let Some(bytes) = ExtraIcons.load(path)? {
            return Ok(Some(bytes));
        }
        // The kit's bundle errors on a path it lacks; a miss here is just absent.
        Ok(Assets.load(path).ok().flatten())
    }

    fn list(&self, path: &str) -> gpui_kit::Result<Vec<SharedString>> {
        let mut all = ExtraIcons.list(path)?;
        all.extend(Assets.list(path)?);
        Ok(all)
    }
}
