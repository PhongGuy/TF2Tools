//! The TF2Tools desktop app: a GPUI window over `tf2tools-core` in v5's zinc dark look.
#![forbid(unsafe_code)]

pub mod activity;
pub mod addon_detail;
pub mod addon_feed;
pub mod addon_updates;
pub mod app;
pub mod assets;
pub mod audio;
pub mod counts;
pub mod crash;
pub mod crosshair_repair;
pub mod crosshair_sync;
pub mod diagnostics;
pub mod folder_load;
pub mod font_check;
pub mod gb_addon_installs;
pub mod gb_card;
pub(crate) mod hud_textures;
pub mod ids;
pub mod keymap;
pub mod kill_icons;
pub mod library_events;
pub mod links;
pub mod loadout;
pub mod local_messages;
pub mod log_nicknames;
pub mod logs_sync;
pub mod map_messages;
pub mod maps_sync;
pub mod mastercomfig_restore;
pub mod mature;
pub mod notices;
pub mod old_v4;
pub mod pages;
pub mod payload_checklist;
pub mod picker;
pub mod previews;
pub mod remote_images;
pub mod services;
pub mod shell;
pub mod store_detail;
pub mod store_download;
pub mod store_grid;
pub mod store_messages;
pub mod store_preview;
pub mod sync;
pub mod sync_messages;
pub mod theme;
pub mod timing;
pub mod tokio_bridge;
pub mod updater;
pub mod weapon_icons;
pub mod weapon_names;
pub mod widgets;

/// Kit init, the theme and the app's key bindings. Tests and the pilot call it.
pub fn init(cx: &mut gpui_kit::App) {
    gpui_kit::init(cx);
    shell::toast::ToastLayer::register(cx);
    audio::player::PlayerLayer::register(cx);
    theme::install(cx);
    keymap::bind(cx);
}

/// Runs the desktop app; exits 1 when the environment gives no data directory and 2 when
/// `TF2TOOLS_TIMING` holds an unknown value.
pub fn run() {
    diagnostics::init();
    let mode = match timing::TimingMode::from_env() {
        Ok(mode) => mode,
        Err(err) => {
            eprintln!("tf2tools: {err}");
            std::process::exit(2);
        }
    };
    let timer = (mode != timing::TimingMode::Off).then(|| timing::Timing::new(mode));
    if let Some(timer) = &timer {
        timer.mark("main");
    }
    let mut boot = match app::Boot::from_environment() {
        Ok(boot) => boot,
        Err(err) => {
            eprintln!("tf2tools: {err}");
            std::process::exit(1);
        }
    };
    crash::install_hook(
        crash::crash_dir(boot.data.root()),
        crash::recent::global().clone(),
    );
    if timer.is_some() {
        // Timing runs reach only a loopback fake server, or a closed port when none is named.
        let base = std::env::var("TF2TOOLS_LOOPBACK_BASE").ok();
        boot = boot.for_timing(base.as_deref());
    }
    gpui_kit::application()
        .with_assets(assets::AppAssets)
        .run(|cx| {
            init(cx);
            if let Some(timer) = timer {
                timing::install(cx, timer);
            }
            cx.on_window_closed(|cx, _| {
                if cx.windows().is_empty() {
                    cx.quit();
                }
            })
            .detach();
            match app::start(boot, cx) {
                Ok(_) => cx.activate(true),
                Err(err) => {
                    eprintln!("tf2tools: the window failed to open: {err}");
                    cx.quit();
                }
            }
        });
}
