//! The app-wide services global, the panic-safe bridge to blocking core calls, and TF2 state.

use std::fmt;
use std::panic::Location;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use gpui_kit::AppContext as _;
use gpui_kit::{App, Entity, Global, SharedString, Task};
use tf2tools_core::{
    ActivityLog, BuiltinCrosshairs, DataDir, Database, DemosClient, DetectError, Endpoints,
    FontHost, Library, LogsCache, LogsClient, OpenError, StoreClient, StoreConfig, Tf2Install,
};

use crate::assets::{AssetRoot, AssetRootError};
use crate::audio::AudioOut;
use crate::i18n::tr;
use crate::local_messages;
use crate::picker::PathPicker;
use crate::viewer::pointer::{PointerLockFactory, platform_pointer_locks};
use crate::viewer::{RendererFactory, SnapshotReader, core_snapshots, wgpu_renderers};

/// Finds a TF2 install; injected so tests and the pilot never touch the real Steam library.
pub type Tf2Detector = Arc<dyn Fn() -> Result<Vec<Tf2Install>, DetectError> + Send + Sync>;

/// The detector the app ships with: Steam library discovery.
pub fn steam_detector() -> Tf2Detector {
    Arc::new(Tf2Install::detect_all)
}

/// Everything a page action needs, cloned onto background threads by `run_blocking`.
#[derive(Clone)]
pub struct Services {
    pub db: Database,
    pub data: DataDir,
    pub library: Library,
    pub builtins: Arc<BuiltinCrosshairs>,
    pub assets: AssetRoot,
    pub picker: Arc<dyn PathPicker>,
    pub detector: Tf2Detector,
    pub audio: Arc<dyn AudioOut>,
    pub store: StoreClient,
    pub activity: ActivityLog,
    /// Lists and installs system fonts for the HUD font check; `None` skips the check.
    pub font_host: Option<Arc<dyn FontHost>>,
    /// Syncs tracked players' logs from logs.tf into the cache the logs.tf page reads.
    pub logs: LogsClient,
    /// Finds and downloads an open log's demo on demos.tf.
    pub demos: DemosClient,
    /// Makes the Demo Viewer's map renderers; tests swap in a stub.
    pub renderers: RendererFactory,
    /// Makes the Demo Viewer's pointer locks; tests swap in a fake.
    pub pointer_lock: PointerLockFactory,
    /// Reads a demo's players for the Demo Viewer; tests swap in a fixed answer.
    pub demo_snapshots: SnapshotReader,
}

impl Global for Services {}

impl Services {
    /// Opens the database and the builtin catalog, and a store client on `endpoints`,
    /// loopback-only when `offline`. Blocking; startup runs it off the UI thread.
    pub fn open(
        data: DataDir,
        assets: AssetRoot,
        picker: Arc<dyn PathPicker>,
        detector: Tf2Detector,
        audio: Arc<dyn AudioOut>,
        endpoints: Endpoints,
        offline: bool,
    ) -> Result<Self, StartupError> {
        // A newer database gets its own variant: the player's fix is an update, not a restart.
        let db = Database::open(&data).map_err(|e| {
            let (path, message) = (data.db_path(), e.to_string());
            match e {
                OpenError::NewerSchema { .. } => StartupError::NewerDatabase { path, message },
                OpenError::Io { .. }
                | OpenError::Corrupt { .. }
                | OpenError::Migration { .. }
                | OpenError::Sqlite { .. }
                | _ => StartupError::Database { path, message },
            }
        })?;
        let crosshairs = assets.crosshairs_dir();
        let builtins = BuiltinCrosshairs::load(&crosshairs).map_err(|e| StartupError::Catalog {
            path: crosshairs,
            message: e.to_string(),
        })?;
        let library = Library::new(db.clone(), data.clone());
        // mastercomfig's override VPKs are not addons; a failure here must not block startup.
        if let Err(error) = library.retire_override_addons() {
            tracing::warn!(%error, "could not retire mastercomfig's override addons");
        }
        let demos_endpoints = endpoints.demos().clone();
        let logs_endpoints = endpoints.logs().clone();
        // Offline narrows the client to loopback; online keeps the default config `new` built.
        let mut store = StoreClient::new(db.clone(), data.clone(), endpoints);
        if offline {
            store = store.with_config(StoreConfig::default().with_loopback_only(true));
        }
        let activity = ActivityLog::new(db.clone());
        let mut logs = LogsClient::new(LogsCache::new(db.clone()), logs_endpoints);
        if offline {
            logs = logs.with_config(LogsClient::default_config().with_loopback_only(true));
        }
        // The demos client follows the boot's endpoints, so a loopback boot never leaves loopback.
        let mut demos = DemosClient::new(demos_endpoints);
        if offline {
            demos = demos.with_config(DemosClient::default_config().with_loopback_only(true));
        }
        Ok(Self {
            db,
            data,
            library,
            builtins: Arc::new(builtins),
            assets,
            picker,
            detector,
            audio,
            store,
            activity,
            font_host: None,
            logs,
            demos,
            renderers: wgpu_renderers(),
            pointer_lock: platform_pointer_locks(),
            demo_snapshots: core_snapshots(),
        })
    }

    /// Points the store client at `endpoints`, keeping the GameBanana pacer the old client
    /// had; tests aim it at a local or closed port. Only the store client is rebuilt: the
    /// logs.tf and demos.tf clients stay as `open` built them.
    pub fn with_endpoints(self, endpoints: Endpoints) -> Self {
        let store = StoreClient::new(self.db.clone(), self.data.clone(), endpoints)
            .with_pacer_of(&self.store);
        Self { store, ..self }
    }

    /// The same services with `logs` as the logs.tf client; tests aim it at a local or closed port.
    pub fn with_logs(self, logs: LogsClient) -> Self {
        Self { logs, ..self }
    }

    /// The same services with `demos` as the demos.tf client; tests aim it at a local or closed port.
    pub fn with_demos(self, demos: DemosClient) -> Self {
        Self { demos, ..self }
    }

    /// The same services with `renderers` making the Demo Viewer's renderers.
    pub fn with_renderers(self, renderers: RendererFactory) -> Self {
        Self { renderers, ..self }
    }

    /// The same services with `pointer_lock` making the Demo Viewer's pointer locks.
    pub fn with_pointer_lock(self, pointer_lock: PointerLockFactory) -> Self {
        Self {
            pointer_lock,
            ..self
        }
    }

    /// The same services with `demo_snapshots` reading the Demo Viewer's players.
    pub fn with_demo_snapshots(self, demo_snapshots: SnapshotReader) -> Self {
        Self {
            demo_snapshots,
            ..self
        }
    }

    /// The same services with `host` as the font host; `open` leaves it `None`.
    pub fn with_font_host(self, host: Option<Arc<dyn FontHost>>) -> Self {
        Self {
            font_host: host,
            ..self
        }
    }
}

/// Why the app could not start; the startup error view shows the message.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum StartupError {
    #[error("{0}")]
    NoAssets(AssetRootError),
    #[error("Could not open the database at {}: {message}", .path.display())]
    Database { path: PathBuf, message: String },
    #[error("The database at {} is from a newer TF2Tools: {message}", .path.display())]
    NewerDatabase { path: PathBuf, message: String },
    #[error("Could not load the builtin crosshairs from {}: {message}", .path.display())]
    Catalog { path: PathBuf, message: String },
}

/// A failed page action, shaped as a toast's title and description. `Core` holds the player's
/// text, the raw `detail` and the place it was built; `NoTf2` and `Internal` read the catalogue
/// when shown. `Display` is the English developer line for the log.
#[derive(Debug, Clone)]
pub enum OpError {
    NoTf2,
    Core {
        action: SharedString,
        message: SharedString,
        detail: String,
        at: &'static Location<'static>,
    },
    Internal(String),
}

// Two failures are the same failure wherever they were built, so `at` is left out.
impl PartialEq for OpError {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::NoTf2, Self::NoTf2) => true,
            (
                Self::Core {
                    action,
                    message,
                    detail,
                    ..
                },
                Self::Core {
                    action: other_action,
                    message: other_message,
                    detail: other_detail,
                    ..
                },
            ) => action == other_action && message == other_message && detail == other_detail,
            (Self::Internal(raw), Self::Internal(other_raw)) => raw == other_raw,
            (Self::NoTf2 | Self::Core { .. } | Self::Internal(_), _) => false,
        }
    }
}

impl Eq for OpError {}

impl OpError {
    /// A failure titled by the action that failed, with player text and no raw detail.
    /// A title is text already in the player's language, so a bare literal does not compile:
    /// ```compile_fail
    /// tf2tools_app::services::OpError::core("Install HUD", "disk full");
    /// ```
    /// A core error is not text, so it does not compile either; describe it first:
    /// ```compile_fail
    /// tf2tools_app::services::OpError::core("Load".into(), std::io::Error::other("raw"));
    /// ```
    #[track_caller]
    pub fn core(action: SharedString, message: impl Into<SharedString>) -> Self {
        Self::Core {
            action,
            message: message.into(),
            detail: String::new(),
            at: Location::caller(),
        }
    }

    pub fn title(&self) -> SharedString {
        match self {
            Self::NoTf2 => tr!("messages.shared.no_tf2_title"),
            Self::Core { action, .. } => action.clone(),
            Self::Internal(_) => tr!("messages.shared.internal_title"),
        }
    }

    pub fn description(&self) -> SharedString {
        match self {
            Self::NoTf2 => tr!("messages.shared.no_tf2_text"),
            Self::Core { message, .. } => message.clone(),
            Self::Internal(_) => local_messages::internal(),
        }
    }
}

// The developer log reads English whatever the player's language: the place a failure was
// built and its raw detail, never the translated title or message.
impl fmt::Display for OpError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoTf2 => f.write_str("no TF2 folder is set"),
            Self::Core { detail, at, .. } if detail.is_empty() => {
                write!(f, "{}:{}", at.file(), at.line())
            }
            Self::Core { detail, at, .. } => write!(f, "{}:{}: {detail}", at.file(), at.line()),
            Self::Internal(raw) => write!(f, "unexpected error: {raw}"),
        }
    }
}

/// Runs `f` on the background executor with a clone of the `Services` global; a panic becomes `OpError::Internal`.
/// Panics if the `Services` global is not installed.
pub fn run_blocking<T: Send + 'static>(
    cx: &App,
    f: impl FnOnce(&Services) -> Result<T, OpError> + Send + 'static,
) -> Task<Result<T, OpError>> {
    let services = cx.global::<Services>().clone();
    // gpui's pool threads have no unwind guard, so a panic must be caught here or it kills the app.
    cx.background_spawn(async move {
        crate::crash::catching(|| f(&services)).unwrap_or_else(|payload| {
            let message = crate::crash::panic_text(&*payload);
            tracing::error!("a background task panicked: {message}");
            Err(OpError::Internal(message))
        })
    })
}

/// Where the app stands on finding TF2; the header badge and every install action read it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Tf2State {
    Detecting,
    Found(Tf2Install),
    Missing { saved: Option<PathBuf> },
}

impl Tf2State {
    pub fn install(&self) -> Option<&Tf2Install> {
        match self {
            Self::Found(install) => Some(install),
            Self::Detecting | Self::Missing { .. } => None,
        }
    }
}

/// v5's startup rule: a saved path is validated (no detection); with none saved, the detector runs and its first install is saved.
/// Returns the state plus any error to toast; a hit whose save fails is still `Found`.
pub fn resolve_tf2(services: &Services) -> (Tf2State, Option<OpError>) {
    let settings = match services.db.settings() {
        Ok(settings) => settings,
        Err(e) => {
            let error =
                local_messages::describe_settings(&e).into_op(tr!("messages.action.load_settings"));
            return (Tf2State::Missing { saved: None }, Some(error));
        }
    };
    if let Some(saved) = settings.tf2_path() {
        return (saved_state(Some(saved)), None);
    }
    match (services.detector)().map(|installs| installs.into_iter().next()) {
        // A path the user saved while the scan ran beats the scan's hit.
        Ok(Some(install)) => match services.db.set_tf2_path_if_unset(install.game_dir()) {
            Ok(true) => (Tf2State::Found(install), None),
            Ok(false) => match services.db.settings() {
                Ok(settings) => (saved_state(settings.tf2_path()), None),
                Err(e) => (
                    Tf2State::Missing { saved: None },
                    Some(
                        local_messages::describe_settings(&e)
                            .into_op(tr!("messages.action.load_settings")),
                    ),
                ),
            },
            Err(e) => (
                Tf2State::Found(install),
                Some(
                    local_messages::describe_settings(&e)
                        .into_op(tr!("messages.action.save_tf2_path")),
                ),
            ),
        },
        Ok(None) | Err(_) => (Tf2State::Missing { saved: None }, None),
    }
}

// The state a saved path gives: found if it is still a TF2 install, else missing but kept.
fn saved_state(saved: Option<&Path>) -> Tf2State {
    match saved {
        Some(saved) => match Tf2Install::from_path(saved) {
            Ok(install) => Tf2State::Found(install),
            Err(_) => Tf2State::Missing {
                saved: Some(saved.to_path_buf()),
            },
        },
        None => Tf2State::Missing { saved: None },
    }
}

/// The state after looking at the disk again, or `None` when nothing changed: a found install
/// whose game folder moved away turns missing with its path kept, and a kept path that holds
/// TF2 again turns found, as Save Settings would.
fn rechecked(state: &Tf2State) -> Option<Tf2State> {
    match state {
        Tf2State::Found(install) if !install.is_present() => Some(Tf2State::Missing {
            saved: Some(install.game_dir().to_path_buf()),
        }),
        Tf2State::Missing { saved: Some(saved) } => match saved_state(Some(saved)) {
            found @ Tf2State::Found(_) => Some(found),
            Tf2State::Detecting | Tf2State::Missing { .. } => None,
        },
        Tf2State::Detecting | Tf2State::Found(_) | Tf2State::Missing { .. } => None,
    }
}

/// Looks at the game folder again and updates the shared state when it changed. The shell
/// calls it on a page change and when its window becomes active; there is no timer.
pub fn recheck_tf2(cx: &mut App) {
    let state = tf2_state(cx);
    if let Some(next) = rechecked(state.read(cx)) {
        state.update(cx, |tf2, cx| {
            *tf2 = next;
            cx.notify();
        });
    }
}

/// Holds the shared `Tf2State` entity, so `Services` itself stays `Send + Sync`.
pub struct Tf2Global(pub Entity<Tf2State>);

impl Global for Tf2Global {}

/// Creates the TF2 state entity and installs it as a global.
pub fn install_tf2_state(cx: &mut App, initial: Tf2State) -> Entity<Tf2State> {
    let state = cx.new(|_| initial);
    cx.set_global(Tf2Global(state.clone()));
    state
}

/// The installed TF2 state entity. Panics if `install_tf2_state` has not run.
pub fn tf2_state(cx: &App) -> Entity<Tf2State> {
    cx.global::<Tf2Global>().0.clone()
}

/// The install a page action needs, or `OpError::NoTf2`. Panics if the `Tf2Global` is not installed.
pub fn require_tf2(cx: &App) -> Result<Tf2Install, OpError> {
    tf2_state(cx)
        .read(cx)
        .install()
        .cloned()
        .ok_or(OpError::NoTf2)
}
