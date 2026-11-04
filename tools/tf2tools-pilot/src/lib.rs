//! TF2Tools under gpui-pilot's headless host, run against a seeded library and a fake TF2,
//! or with `TF2TOOLS_PILOT_REAL=1` against the user's own library and TF2 install.

pub mod seed;

// Core's stock HUD fixture, shared so the fake game carries the same files the tests load.
#[path = "../../../crates/tf2tools-core/tests/common/hud_stock.rs"]
#[allow(dead_code)]
pub(crate) mod hud_stock;

use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use gpui_kit::{App, AssetSource};
use gpui_pilot_host::{Launch, PilotApp};
use tf2tools_app::app::{Boot, start};
use tf2tools_app::assets::{AppAssets, AssetRoot};
use tf2tools_app::audio::RecordingAudio;
use tf2tools_app::picker::{NativePicker, PathPicker};
use tf2tools_app::updater::UpdateBoot;
use tf2tools_core::{DataDir, Endpoints, FontHost, FontHostError};

/// Overrides the launch's data dir, so a QA run can point the pilot at a kept library.
pub const DATA_VAR: &str = "TF2TOOLS_PILOT_DATA";
/// `1` turns on reduced motion; headless, the kit never hears the system's setting.
pub const REDUCE_MOTION_VAR: &str = "TF2TOOLS_PILOT_REDUCE_MOTION";

/// `1` runs the real app: the platform data dir, the Steam detector and the network, no seed.
pub const REAL_VAR: &str = "TF2TOOLS_PILOT_REAL";

/// The private home the headless host gives every launch; real mode drops these before the
/// host starts, then sets HOME back to the user's own (see `real_home_env`).
pub const PRIVATE_HOME_VARS: [&str; 5] = [
    "HOME",
    "XDG_CONFIG_HOME",
    "XDG_DATA_HOME",
    "XDG_CACHE_HOME",
    "XDG_STATE_HOME",
];

/// The environment changes real mode makes before the host starts.
#[derive(Debug, PartialEq, Eq)]
pub struct RealHomeEnv {
    /// Removed first: the launch's private HOME and XDG dirs.
    pub unset: [&'static str; 5],
    /// HOME afterwards, when the passwd entry names one.
    pub home: Option<PathBuf>,
}

/// Unset the private home, then set HOME to `passwd_home` (what `std::env::home_dir` reads
/// once HOME is gone), so fontconfig, which has no passwd fallback, still finds the user's
/// fonts. The XDG dirs stay unset and fall back to their defaults under that home.
pub fn real_home_env(passwd_home: Option<PathBuf>) -> RealHomeEnv {
    RealHomeEnv {
        unset: PRIVATE_HOME_VARS,
        home: passwd_home,
    }
}

/// How a launch boots the app.
#[derive(Debug, PartialEq, Eq)]
pub enum BootPlan {
    /// Seed `data` with the sample library and fake game, then run offline against it.
    Seeded { data: PathBuf },
    /// `Boot::from_environment`, exactly as the shipped binary starts; `data` swaps in a kept
    /// library dir, since the host hides XDG_DATA_HOME.
    Real { data: Option<PathBuf> },
}

/// The plan for `real` (`REAL_VAR`), `data` (`DATA_VAR`) and the launch's own data dir.
/// Only `1` selects real mode; a non-empty `data` overrides `launch_data` (seeded) or the
/// platform data dir (real).
pub fn boot_plan(real: Option<&OsStr>, data: Option<&OsStr>, launch_data: &Path) -> BootPlan {
    let data = data.filter(|dir| !dir.is_empty()).map(PathBuf::from);
    if real == Some(OsStr::new("1")) {
        return BootPlan::Real { data };
    }
    BootPlan::Seeded {
        data: data.unwrap_or_else(|| launch_data.to_path_buf()),
    }
}

/// Turns reduced motion on when `value` (the `REDUCE_MOTION_VAR` value) is `1`; any other
/// value leaves the flag as the kit set it.
pub fn apply_reduce_motion(value: Option<&OsStr>, cx: &mut App) {
    if value == Some(OsStr::new("1")) {
        cx.set_reduce_motion(true);
    }
}

/// A font host that sees no installed fonts and copies into `<data>/pilot-fonts`, so a QA run
/// shows every HUD font as missing and an install never reaches the machine's font set.
pub struct PilotFontHost {
    dir: PathBuf,
}

impl PilotFontHost {
    /// A host whose fonts dir is `<data>/pilot-fonts`.
    pub fn new(data: &Path) -> Self {
        Self {
            dir: data.join("pilot-fonts"),
        }
    }
}

impl FontHost for PilotFontHost {
    fn installed_names(&self) -> Result<Vec<String>, FontHostError> {
        Ok(Vec::new())
    }

    fn fonts_dir(&self) -> Result<PathBuf, FontHostError> {
        Ok(self.dir.clone())
    }

    fn register(&self, _: &Path, _: &str, _: bool) -> Result<(), FontHostError> {
        Ok(())
    }

    fn refresh(&self) -> Result<(), FontHostError> {
        Ok(())
    }
}

/// Off, so a QA run never checks a real feed; a build with the `update-test-key` feature
/// reads the test key and loopback feed from the environment instead.
pub fn update_boot() -> UpdateBoot {
    if cfg!(feature = "update-test-key") {
        UpdateBoot::Environment
    } else {
        UpdateBoot::Off
    }
}

/// The app's own file dialogs; under the host they reach gpui-pilot's platform, where
/// `pick_file` answers them, and never a real dialog.
pub fn picker() -> Arc<dyn PathPicker> {
    Arc::new(NativePicker)
}

/// The store `seed::EMPTY_STORE_VAR` names, if set and not empty.
pub fn empty_store() -> Option<String> {
    std::env::var(seed::EMPTY_STORE_VAR)
        .ok()
        .filter(|v| !v.is_empty())
}

/// TF2Tools' own startup inside the host, never reaching the real Steam install or file dialogs.
pub struct Tf2Tools;

impl PilotApp for Tf2Tools {
    fn assets(&self) -> Option<Arc<dyn AssetSource>> {
        Some(Arc::new(AppAssets))
    }

    // After the kit's init, so the app owns the flag and no later system reading overrides it.
    fn init(&self, cx: &mut App) {
        tf2tools_app::init(cx);
        apply_reduce_motion(std::env::var_os(REDUCE_MOTION_VAR).as_deref(), cx);
    }

    /// `boot_plan` from the environment. Seeded: data dir TF2TOOLS_PILOT_DATA if set, else
    /// launch.data_dir; seeds it (game under <data>/fake-tf2/Team Fortress 2, leaving
    /// TF2TOOLS_PILOT_EMPTY_STORE's store empty), then `start` offline with `picker()` and a
    /// detector that finds nothing. Real: `start` with `Boot::from_environment()`, on TF2TOOLS_PILOT_DATA if set.
    fn open(&self, launch: &Launch, cx: &mut App) -> anyhow::Result<()> {
        let plan = boot_plan(
            std::env::var_os(REAL_VAR).as_deref(),
            std::env::var_os(DATA_VAR).as_deref(),
            &launch.data_dir,
        );
        let boot = match plan {
            BootPlan::Real { data } => {
                let boot = Boot::from_environment()?;
                match data {
                    Some(dir) => Boot {
                        data: DataDir::at(dir),
                        ..boot
                    },
                    None => boot,
                }
            }
            BootPlan::Seeded { data } => seeded_boot(&data)?,
        };
        start(boot, cx)?;
        Ok(())
    }
}

/// Seeds `data` and returns the offline boot that runs against it.
fn seeded_boot(data: &Path) -> anyhow::Result<Boot> {
    let assets = AssetRoot::resolve();
    if let Ok(root) = &assets {
        let game = data.join("fake-tf2/Team Fortress 2");
        seed::seed_with(data, &game, root.path(), empty_store().as_deref())?;
    }
    Ok(Boot {
        data: DataDir::at(data),
        assets,
        picker: picker(),
        detector: Arc::new(|| Ok(Vec::new())),
        audio: Arc::new(RecordingAudio::new()),
        endpoints: Endpoints::default(),
        auto_sync: false,
        offline: false,
        font_host: Some(Arc::new(PilotFontHost::new(data))),
        updates: update_boot(),
    }
    // A pilot QA run never reaches the real stores; a Resync fails against a closed port.
    .offline(None))
}
