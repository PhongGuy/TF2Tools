//! Startup: open the window on the loading view, open the services off the UI thread, then show
//! the shell or the startup error view, and detect TF2 once the shell has drawn.

use std::sync::{Arc, OnceLock};

use gpui_kit::component::{Theme, TitleBar};
use gpui_kit::{
    AnyWindowHandle, App, AppContext as _, Bounds, Font, FontWeight, TitlebarOptions, Window,
    WindowBounds, WindowDecorations, WindowOptions, font, px, size,
};
use tf2tools_core::{DataDir, Endpoints, FontHost, Recovery, Tf2Install};

use crate::activity;
use crate::assets::{AssetRoot, AssetRootError};
use crate::audio::{AudioOut, RecordingAudio, RodioOut};
use crate::crash::{self, notice::CrashNotice};
use crate::crosshair_repair;
use crate::local_messages::describe_startup;
use crate::maps_sync::{MapsSync, gb_maps_runner, maps_sync};
use crate::mastercomfig_restore;
use crate::old_v4::{self, OldV4Notice};
use crate::picker::{NativePicker, PathPicker};
use crate::remote_images::{RemoteImages, store_fetch};
use crate::services::{
    Services, StartupError, Tf2Detector, Tf2State, install_tf2_state, resolve_tf2, run_blocking,
    steam_detector, tf2_state,
};
use crate::shell::Shell;
use crate::shell::root::AppRoot;
use crate::shell::toast::toast_error;
use crate::store_download::StoreDownloads;
use crate::sync::{SyncHub, store_runner, sync_hub};
use crate::updater::{self, UpdateBoot, Updater};
use crate::{timing, tokio_bridge};

/// The window's app id; the desktop entry's `StartupWMClass` must match it.
pub const APP_ID: &str = "com.tf2tools.app";

/// A loopback base nothing listens on (the discard port), so an offline run with no fake
/// server fails every request at once instead of reaching the network.
pub const CLOSED_BASE: &str = "http://127.0.0.1:9";

/// What startup needs from the outside world; tests and the pilot supply their own.
pub struct Boot {
    pub data: DataDir,
    pub assets: Result<AssetRoot, AssetRootError>,
    pub picker: Arc<dyn PathPicker>,
    pub detector: Tf2Detector,
    pub audio: Arc<dyn AudioOut>,
    pub endpoints: Endpoints,
    /// Syncs every store after the first frame; timing and pilot runs turn it off to stay offline.
    pub auto_sync: bool,
    /// Every store client refuses a request whose host is not loopback.
    pub offline: bool,
    /// The system font host for the HUD font check; `None` turns the check off.
    pub font_host: Option<Arc<dyn FontHost>>,
    /// Where the update source comes from; the check runs once after the first frame.
    pub updates: UpdateBoot,
}

impl Boot {
    /// The real app: platform data dir, `AssetRoot::resolve()`, `NativePicker`, `steam_detector()`.
    pub fn from_environment() -> anyhow::Result<Self> {
        Ok(Self {
            data: DataDir::platform_default()?,
            assets: AssetRoot::resolve(),
            picker: Arc::new(NativePicker),
            detector: steam_detector(),
            audio: Arc::new(RodioOut::new()),
            endpoints: Endpoints::default(),
            auto_sync: true,
            offline: false,
            font_host: tf2tools_core::system_font_host(),
            updates: UpdateBoot::Environment,
        })
    }

    /// No request leaves loopback: endpoints `Endpoints::loopback(base or CLOSED_BASE)`,
    /// a loopback-only store config, and no auto sync.
    pub fn offline(self, base: Option<&str>) -> Self {
        Self {
            endpoints: Endpoints::loopback(base.unwrap_or(CLOSED_BASE)),
            auto_sync: false,
            offline: true,
            ..self
        }
    }

    /// A timing run: recording audio instead of a sound device, no font host (so no `fc-list`),
    /// updates off, plus `offline(base)`.
    pub fn for_timing(self, base: Option<&str>) -> Self {
        Self {
            audio: Arc::new(RecordingAudio::new()),
            font_host: None,
            updates: UpdateBoot::Off,
            ..self
        }
        .offline(base)
    }
}

/// Opens the 1200x900 window (min 800x600, title "TF2Tools", app id com.tf2tools.app) on the loading view,
/// opens `Services`, reads settings and the HUD list off the UI thread, then shows the shell or the error view.
/// After the shell's first frame it runs `resolve_tf2` in `run_blocking` and stores the result in `Tf2State`,
/// repairs pre-6.0.2 crosshair scripts once TF2 is found, starts the tokio runtime, checks for an update once and, when `auto_sync`, syncs every store and the maps catalogue.
pub fn start(boot: Boot, cx: &mut App) -> anyhow::Result<AnyWindowHandle> {
    let Boot {
        data,
        assets,
        picker,
        detector,
        audio,
        endpoints,
        auto_sync,
        offline,
        font_host,
        updates,
    } = boot;
    let data_dir = data.root().to_path_buf();
    let update_endpoints = endpoints.clone();
    let crash_dir = crash::crash_dir(&data_dir);
    let archive_dir = crash_dir.clone();
    // Started before the window so the library read overlaps the GPU setup inside `open_window`.
    let opening = cx.background_spawn(async move {
        let assets = assets.map_err(StartupError::NoAssets)?;
        let services = Services::open(data, assets, picker, detector, audio, endpoints, offline)?
            .with_font_host(font_host);
        let settings = services.db.settings().map_err(|e| StartupError::Database {
            path: services.data.db_path(),
            message: e.to_string(),
        })?;
        // A failed list is dropped here; the HUDs page then loads, and toasts, on its own.
        let huds = services.library.list_huds().ok();
        // Core's update client builds an HTTP client: off the UI thread, after the library read.
        let update_source = match updates {
            UpdateBoot::Environment => {
                updater::keys::source_from_environment(&update_endpoints, offline)
            }
            UpdateBoot::Off => updater::keys::unavailable_source(&update_endpoints),
            UpdateBoot::Source(source) => source,
        };
        // Read only once the app is sure to open, and moved aside after the first frame draws
        // the banner, so a failed start or a window closed earlier keeps it for the next.
        let crash = crash::read_crash(&crash_dir);
        // A registry read on Windows only, and skipped once the player has answered.
        let old_v4 = old_v4::pending(&settings, tf2tools_core::detect_old_v4);
        Ok::<_, StartupError>((
            services,
            settings.sidebar_collapsed(),
            huds,
            update_source,
            crash,
            old_v4,
        ))
    });
    // Font matching costs several ms per weight on the first frame; resolving the theme's
    // weights now overlaps that with the GPU setup inside `open_window`.
    let text_system = Arc::clone(cx.text_system());
    let fonts = startup_fonts(cx);
    cx.background_spawn(async move { text_system.prewarm_fonts(&fonts) })
        .detach();
    let (win, root) = gpui_kit::open_window(window_options(cx), cx, |window, cx| {
        // X11 takes its window class only from this call, never from the options' app id.
        window.set_app_id(APP_ID);
        cx.new(|_| AppRoot::Loading)
    })?;
    timing::mark(cx, "window");
    cx.spawn(async move |cx| {
        let opened = opening.await;
        let _ = win.update(cx, |_, window, cx| {
            let next = match opened {
                Ok((services, collapsed, huds, update_source, crash, old_v4)) => {
                    timing::mark(cx, "services");
                    let store = services.store.clone();
                    let store_for_maps = store.clone();
                    cx.set_global(services);
                    // The shell subscribes to the hub, so the hub must exist before it.
                    SyncHub::install(cx, store_runner(store.clone()));
                    MapsSync::install(cx, gb_maps_runner(store.clone()));
                    RemoteImages::install(cx, store_fetch(store));
                    StoreDownloads::install(cx);
                    crate::pages::maps::MapListSource::install(cx, store_for_maps);
                    let updates = Updater::install(cx, update_source, updater::system_restart());
                    activity::watch_stores(cx);
                    install_tf2_state(cx, Tf2State::Detecting);
                    crosshair_repair::install(win, cx);
                    mastercomfig_restore::install(cx);
                    CrashNotice::install(cx, crash);
                    OldV4Notice::install(cx, old_v4, old_v4::system_runner());
                    let shell = cx.new(|cx| Shell::with_huds(collapsed, huds, window, cx));
                    // The runtime and the syncs wait for the first frame, so they never delay it.
                    window.on_next_frame(move |_, cx| {
                        cx.background_spawn(async move { crash::archive_crash(&archive_dir) })
                            .detach();
                        activity::startup(cx);
                        crate::pages::huds::make_missing_thumbnails(cx);
                        detect_tf2(win, cx);
                        tokio_bridge::init(cx);
                        updates.update(cx, |updates, cx| updates.check(cx));
                        if auto_sync && let Some(hub) = sync_hub(cx) {
                            hub.update(cx, |hub, cx| hub.start_all(cx));
                        }
                        if auto_sync && let Some(maps) = maps_sync(cx) {
                            maps.update(cx, |maps, cx| maps.start(false, cx));
                        }
                    });
                    AppRoot::Ready(shell)
                }
                Err(error) => {
                    // The card shows only plain words, so the path and raw error go to the log.
                    describe_startup(&error)
                        .log(&format!("startup with data folder {}", data_dir.display()));
                    AppRoot::Failed { error }
                }
            };
            let ready = matches!(next, AppRoot::Ready(_));
            root.update(cx, |root, cx| {
                *root = next;
                cx.notify();
            });
            if ready {
                timing::mark(cx, "shell");
            }
        });
    })
    .detach();
    Ok(win)
}

/// The theme's UI family at the weights the first frame draws: 400, 500, 600 and 700, normal style.
pub fn startup_fonts(cx: &App) -> Vec<Font> {
    let family = Theme::global(cx).font_family.clone();
    [
        FontWeight::NORMAL,
        FontWeight::MEDIUM,
        FontWeight::SEMIBOLD,
        FontWeight::BOLD,
    ]
    .into_iter()
    .map(|weight| Font {
        weight,
        ..font(family.clone())
    })
    .collect()
}

/// Resolves TF2 off the UI thread; the result applies only while the state is still `Detecting`.
pub fn detect_tf2(win: AnyWindowHandle, cx: &mut App) {
    // Recovery runs before the state turns Found, so the HUDs page first reads a whole HUD.
    let resolving = run_blocking(cx, |services| {
        let resolved = resolve_tf2(services);
        if let Some(tf2) = resolved.0.install() {
            recover_live_link(services, tf2);
        }
        Ok(resolved)
    });
    cx.spawn(async move |cx| {
        let resolved = resolving.await;
        cx.update(|cx| {
            // A Save that landed meanwhile already set the state; detection must not undo it.
            let state = tf2_state(cx);
            if *state.read(cx) != Tf2State::Detecting {
                return;
            }
            let (resolved, error) = match resolved {
                Ok(resolved) => resolved,
                Err(err) => (Tf2State::Missing { saved: None }, Some(err)),
            };
            if let Some(err) = error {
                let _ = win.update(cx, |_, window: &mut Window, cx| {
                    toast_error(window, cx, &err)
                });
            }
            state.update(cx, |tf2, cx| {
                *tf2 = resolved;
                cx.notify();
            });
        });
    })
    .detach();
}

/// Puts back a HUD an interrupted live-link reinstall set aside; a failure is only logged.
fn recover_live_link(services: &Services, tf2: &Tf2Install) {
    match services.library.recover_reinstall(tf2) {
        Ok(Recovery::Restored { folder }) => {
            tracing::info!(folder, "put back a HUD an interrupted reinstall set aside");
        }
        Ok(_) => {}
        Err(err) => tracing::warn!(error = %err, "could not clear an interrupted reinstall"),
    }
}

/// The packaged 256 px app icon for the taskbar and window switcher, decoded once; `None`
/// only if the committed PNG were corrupt.
pub fn window_icon() -> Option<Arc<image::RgbaImage>> {
    static ICON: OnceLock<Option<Arc<image::RgbaImage>>> = OnceLock::new();
    let png = include_bytes!("../../../packaging/icons/128x128@2x.png");
    ICON.get_or_init(|| {
        image::load_from_memory_with_format(png, image::ImageFormat::Png)
            .ok()
            .map(|img| Arc::new(img.to_rgba8()))
    })
    .clone()
}

/// The main window's options: centred at 1200x900, never smaller than 800x600, built on the kit
/// title bar's. On Linux the header draws the title bar, so the compositor adds none.
pub fn window_options(cx: &App) -> WindowOptions {
    WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
            None,
            size(px(1200.), px(900.)),
            cx,
        ))),
        window_decorations: cfg!(target_os = "linux").then_some(WindowDecorations::Client),
        titlebar: Some(TitlebarOptions {
            title: Some("TF2Tools".into()),
            ..TitleBar::title_bar_options()
        }),
        app_id: Some(APP_ID.into()),
        window_min_size: Some(size(px(800.), px(600.))),
        icon: window_icon(),
        ..TitleBar::window_options()
    }
}
