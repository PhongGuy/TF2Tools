//! Shared Demos.tf helpers: a shell on the Demos.tf page over a mock demos.tf, list bodies and
//! the queries the mock received.
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use gpui_kit::{Entity, TestAppContext};
use serde_json::json;
use tf2tools_app::pages::demos_tf::DemosTfPage;
use tf2tools_app::picker::ScriptedPicker;
use tf2tools_app::shell::route::Page;
use tf2tools_app::viewer::pointer::{FallbackLock, PointerLockFactory};
use tf2tools_core::{LogsCache, TrackedPlayer};
use wiremock::matchers::{method, path, query_param, query_param_is_missing};
use wiremock::{Mock, ResponseTemplate};

use super::demo_viewer::{StubAnswer, StubHandle, stub_factory};
use super::logstf::{day_ago, demos_client, sid};

/// One demo of a list body; a private one has no file address.
#[derive(Clone, Copy, Debug)]
pub struct Demo {
    pub id: u64,
    pub map: &'static str,
    pub players: u32,
    pub time: i64,
    pub public: bool,
}

/// A public 6v6 demo on cp_process_f12 from a day ago.
pub fn demo(id: u64) -> Demo {
    Demo {
        id,
        map: "cp_process_f12",
        players: 12,
        time: day_ago(1),
        public: true,
    }
}

/// `count` demos with ids counting down from `top`, newest first as demos.tf lists them.
pub fn ids_down(top: u64, count: u64) -> Vec<Demo> {
    (0..count).map(|i| demo(top - i)).collect()
}

/// A demos.tf list body; a public demo's file lies at `<base>/files/<id>.dem`.
pub fn list_body(base: &str, demos: &[Demo]) -> Vec<u8> {
    let rows: Vec<_> = demos
        .iter()
        .map(|d| {
            json!({
                "id": d.id,
                "url": if d.public { format!("{base}/files/{}.dem", d.id) } else { String::new() },
                "name": format!("match-{}.dem", d.id),
                "server": "serveme.tf #7", "duration": 1714, "map": d.map, "time": d.time,
                "red": "RED", "blue": "BLU", "playerCount": d.players,
            })
        })
        .collect();
    serde_json::Value::Array(rows).to_string().into_bytes()
}

fn list_mock() -> wiremock::MockBuilder {
    Mock::given(method("GET")).and(path("/demos/"))
}

/// Answers every first page (neither `before_id` nor `page`) with `body`.
pub fn mount_first(server: &super::StoreServer, body: Vec<u8>) {
    server.mount(
        list_mock()
            .and(query_param_is_missing("before_id"))
            .and(query_param_is_missing("page"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(body)),
    );
}

/// Answers `before_id=<id>` with `status` and `body`.
pub fn mount_before(server: &super::StoreServer, id: u64, status: u16, body: Vec<u8>) {
    server.mount(
        list_mock()
            .and(query_param("before_id", id.to_string()))
            .respond_with(ResponseTemplate::new(status).set_body_bytes(body)),
    );
}

/// Answers `page=<n>` with `body`.
pub fn mount_page(server: &super::StoreServer, n: u32, body: Vec<u8>) {
    server.mount(
        list_mock()
            .and(query_param("page", n.to_string()))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(body)),
    );
}

/// Every `/demos/` request's query, in the order the server got them.
pub fn list_queries(server: &super::StoreServer) -> Vec<HashMap<String, String>> {
    server
        .rt
        .block_on(server.server.received_requests())
        .unwrap_or_default()
        .iter()
        .filter(|r| r.url.path() == "/demos/")
        .map(|r| {
            r.url
                .query_pairs()
                .map(|(k, v)| (k.into_owned(), v.into_owned()))
                .collect()
        })
        .collect()
}

/// The queries that asked for a first page, a restart of the list.
pub fn first_queries(server: &super::StoreServer) -> Vec<HashMap<String, String>> {
    list_queries(server)
        .into_iter()
        .filter(|q| !q.contains_key("before_id") && !q.contains_key("page"))
        .collect()
}

pub struct DemosTf {
    pub s: super::ShellWindow,
    pub page: Entity<DemosTfPage>,
    pub stubs: Arc<Mutex<Vec<StubHandle>>>,
}

/// A shell on the Demos.tf page with TF2 found at the env's game, demos.tf at `base`,
/// `players` (account, name) tracked and stub renderers.
pub fn demos_tf_shell(
    cx: &mut TestAppContext,
    env: &super::Env,
    base: &str,
    players: &[(u64, &'static str)],
) -> DemosTf {
    demos_tf_shell_picking(cx, env, base, players, Arc::new(ScriptedPicker::empty()))
}

/// [`demos_tf_shell`] with `picker` answering the file dialogs.
pub fn demos_tf_shell_picking(
    cx: &mut TestAppContext,
    env: &super::Env,
    base: &str,
    players: &[(u64, &'static str)],
    picker: Arc<ScriptedPicker>,
) -> DemosTf {
    cx.executor().allow_parking();
    let (factory, stubs) = stub_factory(StubAnswer::Frames);
    let (base, players) = (base.to_owned(), players.to_vec());
    let s = super::open_shell(
        cx,
        env,
        super::graphics::found(env),
        picker,
        super::no_steam(),
        None,
        move |services| {
            services.renderers = factory;
            let fallback: PointerLockFactory = Arc::new(|| Box::new(FallbackLock));
            services.pointer_lock = fallback;
            let cache = LogsCache::new(services.db.clone());
            for (account, name) in &players {
                cache
                    .add_player(&TrackedPlayer::new(sid(*account), *name))
                    .unwrap();
            }
            *services = services.clone().with_demos(demos_client(&base));
        },
    );
    super::click_nav(cx, s.win, Page::DemosTf);
    let page = cx
        .update(|cx| s.shell.read(cx).demos_tf_page())
        .expect("the Demos.tf page");
    DemosTf { s, page, stubs }
}

pub fn shown_ids(cx: &mut TestAppContext, page: &Entity<DemosTfPage>) -> Vec<u64> {
    cx.update(|cx| page.read(cx).demo_ids())
}

pub fn wait_ids(cx: &mut TestAppContext, page: &Entity<DemosTfPage>, want: &[u64]) {
    super::wait_real(cx, "the demo rows", |cx| shown_ids(cx, page) == want);
}

pub fn wait_end(cx: &mut TestAppContext, page: &Entity<DemosTfPage>) {
    super::wait_real(cx, "the end of the list", |cx| {
        cx.update(|cx| page.read(cx).is_end())
    });
}

/// Waits until a drawn frame leaves no page in flight: the footer, once it pages (Task 3),
/// asks during the draw, so each check draws first.
pub fn wait_quiet(cx: &mut TestAppContext, d: &DemosTf) {
    super::wait_real(cx, "a quiet list", |cx| {
        super::settle(cx, d.s.win);
        !cx.update(|cx| d.page.read(cx).is_loading())
    });
}

/// Scrolls the list to its footer and draws it, which asks for the next page.
pub fn to_end(cx: &mut TestAppContext, d: &DemosTf) {
    let state = cx.update(|cx| d.page.read(cx).list_state().clone());
    state.scroll_to_end();
    super::settle(cx, d.s.win);
}
