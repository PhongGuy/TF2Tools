//! The Maps page's GameBanana helpers: a shell that opens Maps, a seeded catalogue, and a
//! maps sync whose runs the test drives.

use std::sync::{Arc, Mutex};

use gpui_kit::{Entity, TestAppContext};
use tf2tools_app::ids;
use tf2tools_app::maps_sync::{MapsRunner, MapsSync};
use tf2tools_app::pages::maps::gb_tab::GbMapsTab;
use tf2tools_app::pages::maps::{MapListSource, MapsPage};
use tf2tools_app::pages::profiles::probe::Tf2Probe;
use tf2tools_app::services::Tf2State;
use tf2tools_app::shell::route::Page;
use tf2tools_app::sync::{ProgressSink, SyncSummary};
use tf2tools_app::sync_messages::{SyncFailure, sync_stopped};
use tf2tools_core::Tf2Install;

/// One maps sync the fake runner was asked for; the test reports progress and ends it.
pub struct FakeMapsRun {
    pub force: bool,
    sink: Option<ProgressSink>,
    finish: Option<tokio::sync::oneshot::Sender<Result<SyncSummary, SyncFailure>>>,
}

impl FakeMapsRun {
    pub fn report(&self, fetched: u32, total: u32) {
        self.sink
            .as_ref()
            .expect("a live run")
            .report(fetched, total);
    }

    /// Drops the progress sink first, as a finished sync future does, then answers.
    pub fn finish(mut self, result: Result<SyncSummary, SyncFailure>) {
        self.sink.take();
        let _ = self.finish.take().expect("a live run").send(result);
    }
}

pub fn fake_maps_runner() -> (MapsRunner, std::sync::mpsc::Receiver<FakeMapsRun>) {
    let (tx, rx) = std::sync::mpsc::channel();
    let tx = Mutex::new(tx);
    let runner: MapsRunner = Arc::new(move |force, sink| {
        let (finish, done) = tokio::sync::oneshot::channel();
        let _ = tx.lock().unwrap().send(FakeMapsRun {
            force,
            sink: Some(sink),
            finish: Some(finish),
        });
        Box::pin(async move {
            done.await
                .unwrap_or_else(|_| Err(sync_stopped("the test dropped the run".to_owned())))
        })
    });
    (runner, rx)
}

/// Installs a maps sync over the fake runner as the app's; its runs cross tokio, so parking is allowed.
pub fn fake_maps_sync(
    cx: &mut TestAppContext,
) -> (Entity<MapsSync>, std::sync::mpsc::Receiver<FakeMapsRun>) {
    cx.executor().allow_parking();
    let (runner, runs) = fake_maps_runner();
    let sync = cx.update(|cx| MapsSync::install(cx, runner));
    (sync, runs)
}

const LIST: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../tf2tools-core/tests/fixtures/store/serveme-maps.html"
));

/// A shell whose store talks to `server` for serveme.tf and GameBanana alike.
pub fn maps_shell(
    cx: &mut TestAppContext,
    env: &super::Env,
    server: &super::StoreServer,
) -> super::ShellWindow {
    maps_shell_in(cx, env, server, Tf2State::Missing { saved: None })
}

/// `maps_shell` with TF2 found at the env's game and not running, so maps install.
pub fn maps_shell_found(
    cx: &mut TestAppContext,
    env: &super::Env,
    server: &super::StoreServer,
) -> super::ShellWindow {
    let state = Tf2State::Found(Tf2Install::from_path(&env.game).unwrap());
    let s = maps_shell_in(cx, env, server, state);
    cx.update(|cx| Tf2Probe::install(cx, || false));
    s
}

fn maps_shell_in(
    cx: &mut TestAppContext,
    env: &super::Env,
    server: &super::StoreServer,
    state: Tf2State,
) -> super::ShellWindow {
    cx.executor().allow_parking();
    server.mount_bytes("/serveme/maps", LIST.as_bytes().to_vec());
    let uri = server.uri();
    let endpoints = server
        .endpoints()
        .with_serveme(format!("{uri}/serveme"))
        .with_map_downloads(format!("{uri}/fastdl/maps"));
    let s = super::shell_window_tweaked(cx, env, state, |services| {
        *services = services.clone().with_endpoints(endpoints);
    });
    let store = s.services.store.clone();
    cx.update(|cx| MapListSource::install(cx, store));
    s
}

/// Opens Maps through the sidebar and waits for serveme.tf's list.
pub fn open_maps(cx: &mut TestAppContext, s: &super::ShellWindow) -> Entity<MapsPage> {
    super::click_id(cx, s.win, ids::nav(Page::Maps));
    let page = cx
        .update(|cx| s.shell.read(cx).maps_page())
        .expect("a Maps page");
    super::wait_real(cx, "the serveme list", |cx| {
        cx.update(|cx| {
            let p = page.read(cx);
            p.is_loaded() && !p.is_syncing() && !p.shown_names().is_empty()
        })
    });
    super::settle(cx, s.win);
    page
}

/// Clicks the GameBanana source and returns its tab once its first page has landed.
pub fn open_gamebanana(
    cx: &mut TestAppContext,
    s: &super::ShellWindow,
    page: &Entity<MapsPage>,
) -> Entity<GbMapsTab> {
    super::click_id(cx, s.win, ids::maps_source("gamebanana"));
    let tab = cx
        .update(|cx| page.read(cx).gamebanana())
        .expect("the tab is built on first show");
    super::wait_real(cx, "the first map page", |cx| {
        cx.update(|cx| tab.read(cx).is_loaded())
    });
    super::settle(cx, s.win);
    tab
}

/// Three synced maps over a two-level tree: cp_alpha (rank 1), trade_plaza (Trade, under
/// Achievement/Idle), koth_beta; dates 300, 500, 100.
pub fn seed_maps(env: &super::Env) {
    rusqlite::Connection::open(env.data.db_path())
        .unwrap()
        .execute_batch(
            "
        INSERT INTO store_gb_map_categories (category_id, name, gamemode_id) VALUES
            (5373, 'Control Points', 5373), (5383, 'Achievement/Idle', 5383),
            (5388, 'Trade', 5383), (5385, 'King of the Hill', 5385);
        INSERT INTO store_gb_maps (gb_id, name, author, date_added, category_id, category_name,
            gamemode_id, download_rank, profile_url, fetched_at) VALUES
            (10, 'cp_alpha', 'vex', 300, 5373, 'Control Points', 5373, 1, 'https://gamebanana.com/mods/10', 'x'),
            (11, 'trade_plaza', 'mira', 500, 5388, 'Trade', 5383, 2, 'https://gamebanana.com/mods/11', 'x'),
            (12, 'koth_beta', 'vex', 100, 5385, 'King of the Hill', 5385, 3, 'https://gamebanana.com/mods/12', 'x');
    ",
        )
        .unwrap();
}

/// The GameBanana ids of the tab's cards, in order.
pub fn shown(cx: &mut TestAppContext, tab: &Entity<GbMapsTab>) -> Vec<u64> {
    cx.update(|cx| {
        tab.read(cx)
            .cards()
            .iter()
            .map(|c| c.card.id.get())
            .collect()
    })
}

/// Library rows over `seed_maps`, all checked just now so nothing is due: 10 installed; 11
/// installed with an update; 12 with an update but its folder deleted; 99 installed but gone
/// from GameBanana and the catalogue.
pub fn seed_map_library(env: &super::Env) {
    let custom = env.game.join("tf/custom");
    for folder in [
        "tf2tools-gb-10-cp_alpha",
        "tf2tools-gb-11-trade_plaza",
        "tf2tools-gb-99-gone_map",
    ] {
        std::fs::create_dir_all(custom.join(folder).join("maps")).unwrap();
    }
    let row = |id: u64, name: &str, latest: i64, gone: u8| {
        format!(
            "({id}, 'maps', '{name}', 'x', 1, 100, {latest}, CAST(strftime('%s','now') AS INTEGER), \
             '[\"{name}\"]', '[\"custom/tf2tools-gb-{id}-{name}\"]', 1, {gone})"
        )
    };
    let rows = [
        row(10, "cp_alpha", 100, 0),
        row(11, "trade_plaza", 200, 0),
        row(12, "koth_beta", 200, 0),
        row(99, "gone_map", 100, 1),
    ];
    super::sql(env)
        .execute_batch(&format!(
            "INSERT INTO library_gb_addons (gb_id, section, name, author, file_id, file_date,
                latest_file_date, checked_at, payloads, files, installed, gone) VALUES {};",
            rows.join(", ")
        ))
        .unwrap();
}
