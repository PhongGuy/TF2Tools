//! The Maps page's GameBanana helpers: a shell that opens Maps, a seeded catalogue, and a
//! maps sync whose runs the test drives.

use std::sync::{Arc, Mutex};

use gpui_kit::{Entity, TestAppContext};
use tf2tools_app::ids;
use tf2tools_app::maps_sync::{MapsRunner, MapsSync};
use tf2tools_app::pages::maps::gb_tab::GbMapsTab;
use tf2tools_app::pages::maps::{MapListSource, MapsPage};
use tf2tools_app::services::Tf2State;
use tf2tools_app::shell::route::Page;
use tf2tools_app::sync::{ProgressSink, SyncSummary};
use tf2tools_app::sync_messages::{SyncFailure, sync_stopped};

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
    cx.executor().allow_parking();
    server.mount_bytes("/serveme/maps", LIST.as_bytes().to_vec());
    let uri = server.uri();
    let endpoints = server
        .endpoints()
        .with_serveme(format!("{uri}/serveme"))
        .with_map_downloads(format!("{uri}/fastdl/maps"));
    let s = super::shell_window_tweaked(cx, env, Tf2State::Missing { saved: None }, |services| {
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
