//! Shared helpers for the app's headless tests.
#![allow(dead_code)]

use std::cell::RefCell;
use std::io::Write as _;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

use gpui_kit::prelude::*;
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{
    AnyWindowHandle, Bounds, Context, ElementId, Entity, Pixels, Size, TestAppContext, Window,
    WindowBounds, WindowOptions, div, img, point, px, size,
};
use tf2tools_app::assets::AssetRoot;
use tf2tools_app::audio::RecordingAudio;
use tf2tools_app::pages::addon_store::AddonStorePage;
use tf2tools_app::pages::sound_store::SoundStorePage;
use tf2tools_app::picker::ScriptedPicker;
use tf2tools_app::remote_images::decode::decode_image;
use tf2tools_app::remote_images::queue::IMAGE_BUDGET;
use tf2tools_app::remote_images::{ImageFetch, RemoteImage, RemoteImages};
use tf2tools_app::services::{Services, Tf2Detector, Tf2State, install_tf2_state};
use tf2tools_app::shell::Shell;
use tf2tools_app::store_download::StoreDownloads;
use tf2tools_app::store_grid::{CardRenderer, StoreGrid, StoreGridConfig};
use tf2tools_app::sync::{ProgressSink, SyncHub, SyncRunner, SyncSummary};
use tf2tools_app::sync_messages::{SyncFailure, sync_stopped};
use tf2tools_app::timing::Case;
use tf2tools_app::timing::cases::{self, CaseConfig, CaseReport};
use tf2tools_app::tokio_bridge;
use tf2tools_core::Endpoints;
use tf2tools_core::StoreKind;
use tf2tools_core::{AddonName, CrosshairEntry, DataDir, DetectError, Provenance, Tf2Install};
use tf2tools_core::{LogEntry, LogQuery};
use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

pub mod fonts;
pub mod graphics;
pub mod hud_copies;
pub mod hud_switches;
pub mod maker_page;
pub mod scroll;
pub mod update_fake;
pub mod update_feed;

#[path = "../../../tf2tools-core/tests/common/hud_stock.rs"]
pub mod hud_stock;

/// Kit, theme and reduced motion, as the app sets them up.
pub fn init(cx: &mut TestAppContext) {
    cx.update(|cx| {
        tf2tools_app::init(cx);
        cx.set_reduce_motion(true);
    });
}

/// The platform's secondary modifier: Ctrl, or Command on macOS.
pub fn secondary_key(cx: &mut TestAppContext) -> gpui_kit::Modifiers {
    cx.update(|cx| gpui_kit::Modifiers::secondary_key(cx))
}

/// The repo's asset directory, which holds `crosshairs/crosshairs.json`.
pub fn repo_assets() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets")
}

/// Runs pending work and draws two frames, so layout that measures itself settles.
/// `render_frame` alone never fires `on_next_frame` or animation-frame callbacks;
/// `simulate_next_frame` does, as the platform frame loop would.
pub fn settle(cx: &mut TestAppContext, win: AnyWindowHandle) {
    cx.run_until_parked();
    for _ in 0..2 {
        cx.update_window(win, |_, w, cx| {
            w.render_frame(cx);
            w.simulate_next_frame(cx);
        })
        .unwrap();
        cx.run_until_parked();
    }
}

pub fn present(cx: &mut TestAppContext, win: AnyWindowHandle, id: impl Into<ElementId>) -> bool {
    let id = id.into();
    settle(cx, win);
    cx.update_window(win, |_, w, _| w.try_find(id).is_some())
        .unwrap()
}

pub fn label_of(
    cx: &mut TestAppContext,
    win: AnyWindowHandle,
    id: impl Into<ElementId>,
) -> Option<String> {
    let id = id.into();
    settle(cx, win);
    cx.update_window(win, |_, w, _| {
        w.try_find(id).and_then(|e| e.label().map(str::to_owned))
    })
    .unwrap()
}

/// The element's a11y selected state, `None` when it exposes none.
pub fn selected_of(
    cx: &mut TestAppContext,
    win: AnyWindowHandle,
    id: impl Into<ElementId>,
) -> Option<bool> {
    let id = id.into();
    settle(cx, win);
    cx.update_window(win, |_, w, _| w.try_find(id).and_then(|e| e.selected()))
        .unwrap()
}

/// The element's a11y checked state (a kit Switch), `None` when it exposes none.
pub fn checked_of(
    cx: &mut TestAppContext,
    win: AnyWindowHandle,
    id: impl Into<ElementId>,
) -> Option<bool> {
    let id = id.into();
    settle(cx, win);
    cx.update_window(win, |_, w, _| w.try_find(id).and_then(|e| e.checked()))
        .unwrap()
}

/// Whether `id` holds keyboard focus; `None` when it has no observed focus binding.
pub fn focused_of(
    cx: &mut TestAppContext,
    win: AnyWindowHandle,
    id: impl Into<ElementId>,
) -> Option<bool> {
    let id = id.into();
    settle(cx, win);
    cx.update_window(win, |_, w, _| w.try_find(id).and_then(|e| e.focused()))
        .unwrap()
}

/// Whether the kit Button `id` renders disabled in `w`'s last frame, read from the a11y
/// disabled flag gpui-base 0.9 sets; tests/button_state.rs pins that. Panics when `id` is not
/// drawn.
pub fn button_disabled(w: &Window, id: impl Into<ElementId>) -> bool {
    w.find(id).disabled() == Some(true)
}

/// `button_disabled` after a settle.
pub fn disabled_of(
    cx: &mut TestAppContext,
    win: AnyWindowHandle,
    id: impl Into<ElementId>,
) -> bool {
    let id = id.into();
    settle(cx, win);
    cx.update_window(win, |_, w, _| button_disabled(w, id))
        .unwrap()
}

/// The ids in `ids` whose kit Button renders disabled, in order.
pub fn disabled_ids(
    cx: &mut TestAppContext,
    win: AnyWindowHandle,
    ids: &[ElementId],
) -> Vec<ElementId> {
    ids.iter()
        .filter(|id| disabled_of(cx, win, (*id).clone()))
        .cloned()
        .collect()
}

/// Moves focus along the window's tab order until `id` holds it; panics after `limit` steps.
pub fn tab_to(cx: &mut TestAppContext, win: AnyWindowHandle, id: ElementId, limit: usize) {
    for _ in 0..limit {
        cx.update_window(win, |_, w, cx| w.focus_next(cx)).unwrap();
        if focused_of(cx, win, id.clone()) == Some(true) {
            return;
        }
    }
    panic!("Tab never reached {id:?}");
}

pub fn bounds_of(
    cx: &mut TestAppContext,
    win: AnyWindowHandle,
    id: impl Into<ElementId>,
) -> Option<Bounds<Pixels>> {
    let id = id.into();
    settle(cx, win);
    cx.update_window(win, |_, w, _| w.try_find(id).map(|e| e.bounds()))
        .unwrap()
}

/// Asserts the page draws no heading of its own: `first`, an element of the page's first row,
/// starts at the page's padding (give or take a control's own inset of a few pixels), since the
/// top bar already names the page.
pub fn assert_starts_under_top_bar(
    cx: &mut TestAppContext,
    win: AnyWindowHandle,
    page: impl Into<ElementId>,
    first: impl Into<ElementId>,
) {
    let (page, first) = (page.into(), first.into());
    settle(cx, win);
    let (page, first) = cx
        .update_window(win, |_, w, _| {
            (
                w.try_find(page).map(|e| e.bounds()).expect("page"),
                w.try_find(first).map(|e| e.bounds()).expect("first row"),
            )
        })
        .unwrap();
    let gap = first.top() - page.top();
    let padding = tf2tools_app::shell::CONTENT_PADDING;
    assert!(
        gap >= padding && gap - padding <= px(6.),
        "the first row sits {gap:?} under the page top, not right at its {padding:?} padding"
    );
}

/// The vertical middle of `b`, for same-row checks.
pub fn mid_y(b: Bounds<Pixels>) -> Pixels {
    b.origin.y + b.size.height / 2.
}

/// Asserts v5's store card footer: the download button spans the card less v5's px-6 inside the
/// 1 px border, 25 px from either side, and the card ends 25 px below it (v5's py-6).
/// Both boxes come from one frame, the button's from inside the card, so a layout change
/// between two reads cannot tear them.
pub fn assert_full_width_download(
    cx: &mut TestAppContext,
    win: AnyWindowHandle,
    card: impl Into<ElementId>,
    button: impl Into<ElementId>,
) {
    let (card, button) = (card.into(), button.into());
    settle(cx, win);
    let (card, button) = cx
        .update_window(win, |_, w, _| {
            let card_box = w.try_find(card.clone()).map(|e| e.bounds());
            let button_box = w.within(card).try_find(button).map(|e| e.bounds());
            (card_box, button_box)
        })
        .unwrap();
    let card = card.expect("the card");
    let button = button.expect("the download button inside the card");
    assert_eq!(
        button.left() - card.left(),
        px(25.),
        "button {button:?} in card {card:?}"
    );
    assert_eq!(
        card.right() - button.right(),
        px(25.),
        "button {button:?} in card {card:?}"
    );
    assert_eq!(
        card.bottom() - button.bottom(),
        px(25.),
        "button {button:?} in card {card:?}"
    );
}

pub fn click_id(cx: &mut TestAppContext, win: AnyWindowHandle, id: impl Into<ElementId>) {
    let id = id.into();
    settle(cx, win);
    cx.update_window(win, |_, w, cx| w.click(id, cx)).unwrap();
    settle(cx, win);
}

/// The open dialog's own X. Scoped to the dialog, since the Windows title bar also has a "close".
pub fn click_dialog_x(cx: &mut TestAppContext, win: AnyWindowHandle) {
    settle(cx, win);
    cx.update_window(win, |_, w, cx| w.within("dialog-close").click("close", cx))
        .unwrap();
    settle(cx, win);
}

/// Whether the open dialog shows its own X.
pub fn dialog_x_present(cx: &mut TestAppContext, win: AnyWindowHandle) -> bool {
    settle(cx, win);
    cx.update_window(win, |_, w, _| {
        w.within("dialog-close").try_find("close").is_some()
    })
    .unwrap()
}

/// Clicks `id` and leaves spawned work unrun, so a second click lands before it.
pub fn click_without_settling(
    cx: &mut TestAppContext,
    win: AnyWindowHandle,
    id: impl Into<ElementId>,
) {
    let id = id.into();
    cx.update_window(win, |_, w, cx| w.click(id, cx)).unwrap();
}

/// Clicks `page`'s sidebar item, first wheeling the sidebar groups to their end when the item
/// sits below their visible area, as a player in a short window would. It only scrolls down,
/// so an item scrolled out above the visible area is not reached.
pub fn click_nav(
    cx: &mut TestAppContext,
    win: AnyWindowHandle,
    page: tf2tools_app::shell::route::Page,
) {
    use tf2tools_app::ids;
    let groups = bounds_of(cx, win, ids::SIDEBAR_GROUPS).expect("the sidebar groups");
    let item = bounds_of(cx, win, ids::nav(page)).expect("the nav item");
    if item.bottom() > groups.bottom() {
        cx.update_window(win, |_, w, cx| {
            let wheel = gpui_kit::ScrollEvent {
                position: groups.center(),
                delta: gpui_kit::ScrollDelta::Lines(point(0., -20.)),
                modifiers: gpui_kit::Modifiers::default(),
                touch_phase: gpui_kit::TouchPhase::Moved,
                source: Default::default(),
            };
            let _ = w.dispatch_event(gpui_kit::InputEvent::to_platform_input(wheel), cx);
        })
        .unwrap();
    }
    click_id(cx, win, ids::nav(page));
}

pub fn press(cx: &mut TestAppContext, win: AnyWindowHandle, keys: &str) {
    settle(cx, win);
    cx.update_window(win, |_, w, cx| w.press(keys, cx)).unwrap();
    settle(cx, win);
}

/// Polls `done` for up to ten seconds, for work that leaves the test executor.
pub fn wait_real(
    cx: &mut TestAppContext,
    what: &str,
    mut done: impl FnMut(&mut TestAppContext) -> bool,
) {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        cx.run_until_parked();
        if done(cx) {
            return;
        }
        assert!(Instant::now() < deadline, "timed out waiting for {what}");
        std::thread::sleep(Duration::from_millis(5));
    }
}

/// An empty root view, for helpers that only need a window (toasts, dialogs).
pub struct Blank;

impl Render for Blank {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().size_full()
    }
}

pub fn blank_window(cx: &mut TestAppContext) -> AnyWindowHandle {
    cx.update(|cx| gpui_kit::open_window(WindowOptions::default(), cx, |_, cx| cx.new(|_| Blank)))
        .unwrap()
        .0
}

/// A temp data dir (not yet created) and a fake TF2 game dir with `tf/custom` and a
/// `tf/tf2_misc_dir.vpk` holding a stock script for every assignable weapon.
pub struct Env {
    pub tmp: tempfile::TempDir,
    pub data: DataDir,
    pub game: PathBuf,
}

pub fn env() -> Env {
    let tmp = tempfile::tempdir().unwrap();
    let data = DataDir::at(tmp.path().join("com.tf2tools.app"));
    let game = tmp.path().join("Team Fortress 2");
    std::fs::create_dir_all(game.join("tf/custom")).unwrap();
    let entries = weapon_script_entries();
    let entries: Vec<(&str, &[u8])> = entries.iter().map(|(p, b)| (p.as_str(), *b)).collect();
    std::fs::write(game.join("tf/tf2_misc_dir.vpk"), vpk_bytes(&entries)).unwrap();
    Env { tmp, data, game }
}

/// The ICE-encrypted stock weapon script core's crosshair tests use.
const STOCK_WEAPON_CTX: &[u8] =
    include_bytes!("../../../tf2tools-core/tests/fixtures/crosshair/stock_weapon.ctx");

/// `scripts/<weapon>.ctx` entries for every assignable weapon, so crosshair applies can patch
/// a full script.
fn weapon_script_entries() -> Vec<(String, &'static [u8])> {
    tf2tools_core::weapons()
        .iter()
        .map(|w| (format!("scripts/{}.ctx", w.script_name()), STOCK_WEAPON_CTX))
        .collect()
}

pub fn repo_asset_root() -> AssetRoot {
    AssetRoot::resolve_from(None, None, Some(repo_assets())).unwrap()
}

/// A detector that finds nothing, standing in for a machine without Steam.
pub fn no_steam() -> Tf2Detector {
    Arc::new(|| -> Result<Vec<Tf2Install>, DetectError> { Ok(Vec::new()) })
}

/// A detector whose use fails the test: the saved path must be trusted instead.
pub fn detection_forbidden() -> Tf2Detector {
    Arc::new(|| -> Result<Vec<Tf2Install>, DetectError> { panic!("detection must not run") })
}

/// A detector that finds `game`.
pub fn steam_finds(game: PathBuf) -> Tf2Detector {
    Arc::new(move || Tf2Install::from_path(&game).map(|install| vec![install]))
}

/// A detector that finds every game dir in `games`, in order.
pub fn steam_finds_all(games: Vec<PathBuf>) -> Tf2Detector {
    Arc::new(move || games.iter().map(|g| Tf2Install::from_path(g)).collect())
}

/// A detector that errors; a missing dir yields a real `DetectError` without naming the non-exhaustive type.
pub fn failing_steam() -> Tf2Detector {
    Arc::new(|| {
        Tf2Install::from_path(std::path::Path::new("/nonexistent/tf2tools-test"))
            .map(|install| vec![install])
    })
}

/// Services over a fresh `RecordingAudio` nobody inspects.
pub fn services(env: &Env, detector: Tf2Detector) -> Services {
    Services::open(
        env.data.clone(),
        repo_asset_root(),
        Arc::new(ScriptedPicker::empty()),
        detector,
        Arc::new(RecordingAudio::new()),
        Endpoints::default(),
        false,
    )
    .unwrap()
}

/// Services whose audio is the `RecordingAudio` the test holds, with no TF2 found.
pub fn services_with_audio(env: &Env, audio: Arc<RecordingAudio>) -> Services {
    Services::open(
        env.data.clone(),
        repo_asset_root(),
        Arc::new(ScriptedPicker::empty()),
        no_steam(),
        audio,
        Endpoints::default(),
        false,
    )
    .unwrap()
}

pub fn install_services(cx: &mut TestAppContext, services: Services) {
    cx.update(|cx| cx.set_global(services));
}

/// 1200x900, v5's window.
pub fn window_options() -> WindowOptions {
    WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(Bounds {
            origin: point(px(0.), px(0.)),
            size: size(px(1200.), px(900.)),
        })),
        ..WindowOptions::default()
    }
}

/// A shell over `env` with the given TF2 state, as startup leaves it.
pub struct ShellWindow {
    pub win: AnyWindowHandle,
    pub shell: Entity<Shell>,
    pub tf2: Entity<Tf2State>,
    pub services: Services,
    pub audio: Arc<RecordingAudio>,
}

pub fn shell_window(cx: &mut TestAppContext, env: &Env, state: Tf2State) -> ShellWindow {
    shell_window_with(
        cx,
        env,
        state,
        Arc::new(ScriptedPicker::empty()),
        no_steam(),
    )
}

/// Like `shell_window`, with the folder picker and detector the test scripts.
pub fn shell_window_with(
    cx: &mut TestAppContext,
    env: &Env,
    state: Tf2State,
    picker: Arc<ScriptedPicker>,
    detector: Tf2Detector,
) -> ShellWindow {
    open_shell(cx, env, state, picker, detector, None, |_| {})
}

/// Like `shell_window`, with `tweak` adjusting the services before the shell opens.
pub fn shell_window_tweaked(
    cx: &mut TestAppContext,
    env: &Env,
    state: Tf2State,
    tweak: impl FnOnce(&mut Services),
) -> ShellWindow {
    let picker = Arc::new(ScriptedPicker::empty());
    open_shell(cx, env, state, picker, no_steam(), None, tweak)
}

/// Like `shell_window`, with `host` as the services' font host.
pub fn shell_window_fonts(
    cx: &mut TestAppContext,
    env: &Env,
    state: Tf2State,
    host: Arc<fonts::FakeFontHost>,
) -> ShellWindow {
    let host: Arc<dyn tf2tools_core::FontHost> = host;
    open_shell(
        cx,
        env,
        state,
        Arc::new(ScriptedPicker::empty()),
        no_steam(),
        Some(host),
        |_| {},
    )
}

fn open_shell(
    cx: &mut TestAppContext,
    env: &Env,
    state: Tf2State,
    picker: Arc<ScriptedPicker>,
    detector: Tf2Detector,
    font_host: Option<Arc<dyn tf2tools_core::FontHost>>,
    tweak: impl FnOnce(&mut Services),
) -> ShellWindow {
    init(cx);
    let audio = Arc::new(RecordingAudio::new());
    let mut services = Services::open(
        env.data.clone(),
        repo_asset_root(),
        picker,
        detector,
        Arc::<RecordingAudio>::clone(&audio),
        Endpoints::default().with_mastercomfig_data("http://127.0.0.1:9"),
        false,
    )
    .unwrap()
    .with_font_host(font_host);
    tweak(&mut services);
    install_services(cx, services.clone());
    // A test that pins no probe reads an empty proc tree, never the machine's processes.
    let proc_root = env.tmp.path().join("proc");
    cx.update(|cx| tf2tools_app::pages::profiles::probe::Tf2Probe::set_proc_root(cx, proc_root));
    let tf2 = cx.update(|cx| install_tf2_state(cx, state));
    let collapsed = services.db.settings().unwrap().sidebar_collapsed();
    let (win, shell) = cx
        .update(|cx| {
            gpui_kit::open_window(window_options(), cx, |window, cx| {
                cx.new(|cx| Shell::new(collapsed, window, cx))
            })
        })
        .unwrap();
    settle(cx, win);
    ShellWindow {
        win,
        shell,
        tf2,
        services,
        audio,
    }
}

/// A 16x16 PNG of one colour, a file `import_crosshair` accepts.
pub fn crosshair_png(rgba: [u8; 4]) -> Vec<u8> {
    let img = image::RgbaImage::from_pixel(16, 16, image::Rgba(rgba));
    let mut out = std::io::Cursor::new(Vec::new());
    img.write_to(&mut out, image::ImageFormat::Png).unwrap();
    out.into_inner()
}

/// Imports a one-colour PNG as library crosshair `name`.
pub fn seed_crosshair(services: &Services, env: &Env, name: &str) -> CrosshairEntry {
    let path = env.tmp.path().join(format!("{name}.png"));
    std::fs::write(&path, crosshair_png([0, 255, 0, 255])).unwrap();
    services
        .library
        .import_crosshair(&path, name, Provenance::Local)
        .unwrap()
}

/// Starts `case`, then draws frames as the platform loop would until the case reports.
pub fn run_frame_case(
    cx: &mut TestAppContext,
    s: &ShellWindow,
    case: Case,
    config: CaseConfig,
) -> Result<CaseReport, String> {
    run_frame_case_with(cx, s, case, config, false, |_| {})
}

/// Like `run_frame_case`, but each frame also advances the virtual clock by 16 ms.
pub fn run_frame_case_ticking(
    cx: &mut TestAppContext,
    s: &ShellWindow,
    case: Case,
    config: CaseConfig,
) -> Result<CaseReport, String> {
    run_frame_case_with(cx, s, case, config, true, |_| {})
}

/// Runs `case` a frame at a time, calling `each_frame` after every frame; with `ticking`, each
/// frame also advances the virtual clock by 16 ms.
pub fn run_frame_case_with(
    cx: &mut TestAppContext,
    s: &ShellWindow,
    case: Case,
    config: CaseConfig,
    ticking: bool,
    mut each_frame: impl FnMut(&mut TestAppContext),
) -> Result<CaseReport, String> {
    let out: Rc<RefCell<Option<Result<CaseReport, String>>>> = Rc::default();
    let sink = Rc::clone(&out);
    let shell = s.shell.clone();
    cx.update_window(s.win, |_, window, cx| {
        cases::run_case(case, config, shell, window, cx, move |report, _, _| {
            *sink.borrow_mut() = Some(report);
        })
    })
    .unwrap();
    let deadline = Instant::now() + Duration::from_secs(60);
    while out.borrow().is_none() {
        assert!(Instant::now() < deadline, "{} never reported", case.name());
        cx.update_window(s.win, |_, w, cx| {
            w.render_frame(cx);
            w.simulate_next_frame(cx);
        })
        .unwrap();
        if ticking {
            cx.executor().advance_clock(Duration::from_millis(16));
        }
        cx.run_until_parked();
        each_frame(cx);
        std::thread::sleep(Duration::from_millis(1));
    }
    out.take().unwrap()
}

/// One sync the fake runner was asked for; the test reports progress and ends it.
pub struct FakeRun {
    pub kind: StoreKind,
    pub force: bool,
    sink: Option<ProgressSink>,
    finish: Option<tokio::sync::oneshot::Sender<Result<SyncSummary, SyncFailure>>>,
}

impl FakeRun {
    pub fn report(&self, fetched: u32, total: u32) {
        self.sink
            .as_ref()
            .expect("a live run")
            .report(fetched, total);
    }

    /// Ends the run: drops the progress sink first, as a finished sync future does, then answers.
    pub fn finish(mut self, result: Result<SyncSummary, SyncFailure>) {
        self.sink.take();
        let _ = self.finish.take().expect("a live run").send(result);
    }

    /// True once the hub dropped the run's future, which only an abort does before `finish`.
    pub fn aborted(&self) -> bool {
        self.finish.as_ref().expect("a live run").is_closed()
    }
}

/// A GameBanana sync that could not connect, worded as `describe_sync` words it.
pub fn offline_failure() -> SyncFailure {
    SyncFailure::new(
        "Couldn't reach GameBanana. Check your internet connection and try again.",
        "cannot reach https://gamebanana.com/apiv11/Util/Search/Results: connection refused",
    )
}

/// A runner that hands every requested sync to the test instead of the network.
pub fn fake_runner() -> (SyncRunner, std::sync::mpsc::Receiver<FakeRun>) {
    let (tx, rx) = std::sync::mpsc::channel();
    let tx = std::sync::Mutex::new(tx);
    let runner: SyncRunner = Arc::new(move |kind, force, sink| {
        let (finish, done) = tokio::sync::oneshot::channel();
        let _ = tx.lock().unwrap().send(FakeRun {
            kind,
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

/// Installs a hub over the fake runner as the app's hub. Its runs cross the tokio runtime,
/// so the test executor must be allowed to park.
pub fn fake_hub(cx: &mut TestAppContext) -> (Entity<SyncHub>, std::sync::mpsc::Receiver<FakeRun>) {
    cx.executor().allow_parking();
    let (runner, runs) = fake_runner();
    let hub = cx.update(|cx| SyncHub::install(cx, runner));
    (hub, runs)
}

/// Lets one repaint interval of virtual time pass and runs what it woke.
pub fn next_repaint(cx: &mut TestAppContext) {
    cx.executor()
        .advance_clock(tf2tools_app::sync::REPAINT_INTERVAL);
    cx.run_until_parked();
}

/// Like `wait_real`, but moves the virtual clock each poll so a pump asleep in its
/// repaint interval wakes and sees its run end.
pub fn wait_hub(
    cx: &mut TestAppContext,
    what: &str,
    mut done: impl FnMut(&mut TestAppContext) -> bool,
) {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        next_repaint(cx);
        if done(cx) {
            return;
        }
        assert!(Instant::now() < deadline, "timed out waiting for {what}");
        std::thread::sleep(Duration::from_millis(5));
    }
}

/// Store crosshairs "Cross {id}": likes = id + `likes_bonus`, date_added = 100000 - id
/// (lowest id newest), and a GameBanana thumbnail URL when `thumbnails`.
pub fn seed_crosshairs(
    env: &Env,
    ids: impl IntoIterator<Item = i64>,
    likes_bonus: i64,
    thumbnails: bool,
) {
    let mut conn = rusqlite::Connection::open(env.data.db_path()).unwrap();
    let tx = conn.transaction().unwrap();
    for id in ids {
        let thumb = thumbnails
            .then(|| format!("https://images.gamebanana.com/img/ss/mods/100-90_{id}.jpg"));
        tx.execute(
            "INSERT INTO store_crosshairs (gb_id, name, author, thumbnail_url, like_count, view_count, date_added, fetched_at)
             VALUES (?1, 'Cross ' || ?1, 'vex', ?2, ?1 + ?3, 0, 100000 - ?1, 'x')",
            (id, thumb, likes_bonus),
        )
        .unwrap();
    }
    tx.commit().unwrap();
}

/// Services over `env`, then `seed`, then a 1200x900 window on a `StoreGrid` whose cards draw image and name.
pub fn store_grid_window(
    cx: &mut TestAppContext,
    env: &Env,
    config: StoreGridConfig,
    seed: impl FnOnce(&Env),
) -> (AnyWindowHandle, Entity<StoreGrid>) {
    init(cx);
    install_services(cx, services(env, no_steam()));
    seed(env);
    open_store_grid(cx, config)
}

/// Another 1200x900 window on a `StoreGrid` like `store_grid_window`'s, over the installed services.
pub fn open_store_grid(
    cx: &mut TestAppContext,
    config: StoreGridConfig,
) -> (AnyWindowHandle, Entity<StoreGrid>) {
    let render: CardRenderer = Rc::new(|card, image, _, _| {
        let area = div().w_full().h(px(100.));
        let area = match image {
            Some(RemoteImage::Ready(i)) => area.child(img(i).size_full()),
            _ => area,
        };
        div()
            .size_full()
            .child(area)
            .child(card.name.clone())
            .into_any_element()
    });
    cx.update(|cx| {
        gpui_kit::open_window(window_options(), cx, |window, cx| {
            cx.new(|cx| StoreGrid::new(config, render, window, cx))
        })
    })
    .unwrap()
}

pub const CURATED_HASH: &str = "abc123";

/// Where `StoreServer::endpoints` sends GitHub's latest-release call for mastercomfig.
pub const MASTERCOMFIG_RELEASE_ROUTE: &str =
    "/gh-api/repos/mastercomfig/mastercomfig/releases/latest";

/// A wiremock server on its own runtime, standing in for GameBanana and GitHub.
pub struct StoreServer {
    pub server: MockServer,
    rt: tokio::runtime::Runtime,
}

impl StoreServer {
    pub fn start() -> Self {
        let rt = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(1)
            .enable_all()
            .build()
            .unwrap();
        let server = rt.block_on(MockServer::start());
        Self { server, rt }
    }

    pub fn uri(&self) -> String {
        self.server.uri()
    }

    pub fn endpoints(&self) -> Endpoints {
        let uri = self.uri();
        Endpoints::default()
            .with_gamebanana(format!("{uri}/apiv11"))
            .with_github_api(format!("{uri}/gh-api"))
            .with_github(format!("{uri}/gh"))
            .with_preview_prefixes([format!("{uri}/preview/")])
            .with_mastercomfig_data(format!("{uri}/gh-raw/mastercomfig"))
    }

    /// Serves the fixture catalog where release `tag`'s settings list is fetched.
    pub fn mount_mastercomfig_catalog(&self, tag: &str) {
        let base = format!("/gh-raw/mastercomfig/{tag}/data");
        self.mount_bytes(&format!("{base}/modules.json"), MC_MODULES.to_vec());
        self.mount_bytes(
            &format!("{base}/preset_modules.json"),
            MC_PRESET_MODULES.to_vec(),
        );
    }

    /// Serves `body` at `{uri}/preview/{gb_id}.mp3` and returns that URL.
    pub fn mount_preview(&self, gb_id: u64, body: Vec<u8>) -> String {
        let route = format!("/preview/{gb_id}.mp3");
        self.rt.block_on(
            Mock::given(method("GET"))
                .and(path(route.clone()))
                .respond_with(ResponseTemplate::new(200).set_body_bytes(body))
                .mount(&self.server),
        );
        format!("{}{route}", self.uri())
    }

    /// GameBanana Mod `gb_id` listing `files` (name, body); file i is served at `/dl/{gb_id}/{i}` after `delay`.
    pub fn mount_gb_files(&self, gb_id: u64, files: &[(&str, Vec<u8>)], delay: Duration) {
        let uri = self.uri();
        let listed: Vec<serde_json::Value> = files
            .iter()
            .enumerate()
            .map(|(i, (name, body))| {
                serde_json::json!({
                    "_idRow": i + 1, "_sFile": name, "_nFilesize": body.len(),
                    "_sDownloadUrl": format!("{uri}/dl/{gb_id}/{i}"), "_sMd5Checksum": ""
                })
            })
            .collect();
        self.rt.block_on(async {
            Mock::given(method("GET"))
                .and(path(format!("/apiv11/Mod/{gb_id}")))
                .respond_with(ResponseTemplate::new(200).set_body_json(
                    serde_json::json!({"_idRow": gb_id, "_sName": "item", "_aFiles": listed}),
                ))
                .mount(&self.server)
                .await;
            for (i, (_, body)) in files.iter().enumerate() {
                Mock::given(method("GET"))
                    .and(path(format!("/dl/{gb_id}/{i}")))
                    .respond_with(
                        ResponseTemplate::new(200)
                            .set_body_bytes(body.clone())
                            .set_delay(delay),
                    )
                    .mount(&self.server)
                    .await;
            }
        });
    }

    /// GameBanana Mod `gb_id` answers its listing with a 404 after `delay`.
    pub fn mount_gb_missing(&self, gb_id: u64, delay: Duration) {
        self.rt.block_on(
            Mock::given(method("GET"))
                .and(path(format!("/apiv11/Mod/{gb_id}")))
                .respond_with(ResponseTemplate::new(404).set_delay(delay))
                .mount(&self.server),
        );
    }

    /// GameBanana Sound `gb_id` listing `files` (name, body); file i is served at `/dl/{gb_id}/{i}` after `delay`.
    pub fn mount_sound_files(&self, gb_id: u64, files: &[(&str, Vec<u8>)], delay: Duration) {
        let uri = self.uri();
        let listed: Vec<serde_json::Value> = files
            .iter()
            .enumerate()
            .map(|(i, (name, body))| {
                serde_json::json!({
                    "_idRow": i + 1, "_sFile": name, "_nFilesize": body.len(),
                    "_sDownloadUrl": format!("{uri}/dl/{gb_id}/{i}"), "_sMd5Checksum": ""
                })
            })
            .collect();
        self.rt.block_on(async {
            Mock::given(method("GET"))
                .and(path(format!("/apiv11/Sound/{gb_id}")))
                .respond_with(ResponseTemplate::new(200).set_body_json(
                    serde_json::json!({"_idRow": gb_id, "_sName": "item", "_aFiles": listed}),
                ))
                .mount(&self.server)
                .await;
            for (i, (_, body)) in files.iter().enumerate() {
                Mock::given(method("GET"))
                    .and(path(format!("/dl/{gb_id}/{i}")))
                    .respond_with(
                        ResponseTemplate::new(200)
                            .set_body_bytes(body.clone())
                            .set_delay(delay),
                    )
                    .mount(&self.server)
                    .await;
            }
        });
    }

    /// Serves `body` at `{uri}/img/{name}`, under `Endpoints::loopback`'s image prefix; returns that URL.
    pub fn mount_image(&self, name: &str, body: Vec<u8>) -> String {
        let route = format!("/img/{name}");
        self.rt.block_on(
            Mock::given(method("GET"))
                .and(path(route.clone()))
                .respond_with(ResponseTemplate::new(200).set_body_bytes(body))
                .mount(&self.server),
        );
        format!("{}{route}", self.uri())
    }

    /// Serves `body` at `route` (a path such as `/serveme/maps`).
    pub fn mount_bytes(&self, route: &str, body: Vec<u8>) {
        self.rt.block_on(
            Mock::given(method("GET"))
                .and(path(route))
                .respond_with(ResponseTemplate::new(200).set_body_bytes(body))
                .mount(&self.server),
        );
    }

    /// GitHub's latest mastercomfig release `tag`, listing only the base VPK at this server's
    /// `/gh/mastercomfig/mastercomfig-base.vpk`.
    pub fn mount_mastercomfig_release(&self, tag: &str) {
        let body = serde_json::json!({
            "tag_name": tag,
            "assets": [{
                "name": "mastercomfig-base.vpk",
                "browser_download_url":
                    format!("{}/gh/mastercomfig/mastercomfig-base.vpk", self.uri()),
                "size": 100,
                "download_count": 7
            }]
        });
        self.mount_bytes(MASTERCOMFIG_RELEASE_ROUTE, body.to_string().into_bytes());
    }

    /// Forgets every mounted answer, so later mounts take over.
    pub fn reset(&self) {
        self.rt.block_on(self.server.reset());
    }

    /// Answers every GET of `route` with `status` and an empty body.
    pub fn mount_status(&self, route: &str, status: u16) {
        self.rt.block_on(
            Mock::given(method("GET"))
                .and(path(route))
                .respond_with(ResponseTemplate::new(status))
                .mount(&self.server),
        );
    }

    /// A curated HUD's archive at `{uri}/repo/{hud_id}/archive/{CURATED_HASH}.zip`; returns the repo URL to seed.
    pub fn mount_curated(&self, hud_id: &str, body: Vec<u8>, delay: Duration) -> String {
        self.mount_curated_at(hud_id, CURATED_HASH, body, delay)
    }

    /// `mount_curated` at commit `hash`.
    pub fn mount_curated_at(
        &self,
        hud_id: &str,
        hash: &str,
        body: Vec<u8>,
        delay: Duration,
    ) -> String {
        self.rt.block_on(
            Mock::given(method("GET"))
                .and(path(format!("/repo/{hud_id}/archive/{hash}.zip")))
                .respond_with(
                    ResponseTemplate::new(200)
                        .set_body_bytes(body)
                        .set_delay(delay),
                )
                .mount(&self.server),
        );
        format!("{}/repo/{hud_id}", self.uri())
    }
}

/// Store sounds (id, name, category, preview URL): author "vex", likes = id, views = id * 1000,
/// tag "Purity: Fun", date_added = 1700000000 - id.
pub fn seed_sounds(env: &Env, rows: &[(i64, &str, &str, Option<&str>)]) {
    let conn = rusqlite::Connection::open(env.data.db_path()).unwrap();
    for (id, name, category, preview) in rows {
        conn.execute(
            "INSERT INTO store_sounds (gb_id, name, author, category, preview_url, like_count, view_count,
                tags_json, date_added, fetched_at)
             VALUES (?1, ?2, 'vex', ?3, ?4, ?1, ?1 * 1000, '[\"Purity: Fun\"]', 1700000000 - ?1, 'x')",
            (id, name, category, preview),
        )
        .unwrap();
    }
}

/// GameBanana store HUDs: author "vex", two gallery images, tag "Minimal", 10 likes, 200 views.
pub fn seed_gb_huds(env: &Env, rows: &[(i64, &str)]) {
    let conn = rusqlite::Connection::open(env.data.db_path()).unwrap();
    for (id, name) in rows {
        let images = format!(
            r#"["https://images.gamebanana.com/img/ss/mods/{id}-a.jpg","https://images.gamebanana.com/img/ss/mods/{id}-b.jpg"]"#
        );
        let thumb = format!("https://images.gamebanana.com/img/ss/mods/100-90_{id}-a.jpg");
        conn.execute(
            "INSERT INTO store_huds_gb (gb_id, name, author, thumbnail_url, images_json, like_count, view_count,
                tags_json, profile_url, date_added, fetched_at)
             VALUES (?1, ?2, 'vex', ?3, ?4, 10, 200, '[\"Minimal\"]', '', 1700000000 - ?1, 'x')",
            (id, name, thumb, images),
        )
        .unwrap();
    }
}

/// v5's Helltaker card's three tags, none of which shares a card-wide row with another.
pub const LONG_TAGS: [&str; 3] = [
    "Software Used: VTFEdit",
    "Software Used: GIMP",
    "Software Used: Paint.NET",
];

/// Replaces GameBanana row `gb_id`'s tags in `table` (`store_huds_gb` or `store_crosshairs`).
pub fn set_gb_tags(env: &Env, table: &str, gb_id: i64, tags: &[&str]) {
    let json = format!(
        "[{}]",
        tags.iter()
            .map(|t| format!("\"{t}\""))
            .collect::<Vec<_>>()
            .join(",")
    );
    let conn = rusqlite::Connection::open(env.data.db_path()).unwrap();
    let changed = conn
        .execute(
            &format!("UPDATE {table} SET tags_json = ?1 WHERE gb_id = ?2"),
            (json, gb_id),
        )
        .unwrap();
    assert_eq!(changed, 1, "no {table} row {gb_id}");
}

/// One curated store HUD with hash `CURATED_HASH`.
pub fn seed_curated(
    env: &Env,
    hud_id: &str,
    name: &str,
    author: &str,
    repo: Option<&str>,
    data_json: &str,
) {
    let conn = rusqlite::Connection::open(env.data.db_path()).unwrap();
    conn.execute(
        "INSERT INTO store_huds (hud_id, name, author, repo, hash, data_json, fetched_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'x')",
        (hud_id, name, author, repo, CURATED_HASH, data_json),
    )
    .unwrap();
}

/// A minimal HUD archive in memory: `<name>/info.vdf` plus a layout file.
pub fn hud_zip_bytes(name: &str) -> Vec<u8> {
    let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let options = zip::write::SimpleFileOptions::default();
    zip.start_file(format!("{name}/info.vdf"), options).unwrap();
    write!(zip, "\"{name}\"\n{{\n\t\"ui_version\"\t\"3\"\n}}\n").unwrap();
    zip.start_file(format!("{name}/resource/ui/hudlayout.res"), options)
        .unwrap();
    write!(zip, "\"Resource/HudLayout.res\"\n{{\n}}\n").unwrap();
    zip.finish().unwrap().into_inner()
}

/// Points the installed services at `server`, starts the bridge and installs `StoreDownloads`.
/// Tokio workers wake the download tasks, so the test executor must allow real parking.
pub fn store_ready(cx: &mut TestAppContext, server: &StoreServer) {
    cx.executor().allow_parking();
    let services = cx.update(|cx| cx.global::<Services>().clone());
    install_services(cx, services.with_endpoints(server.endpoints()));
    cx.update(|cx| {
        if !tokio_bridge::is_running(cx) {
            tokio_bridge::init(cx);
        }
        StoreDownloads::install(cx);
    });
}

/// Services over `env` pointed at `server`, then `seed`, then a 1200x900 window on `build`.
pub fn store_window<V: Render>(
    cx: &mut TestAppContext,
    env: &Env,
    server: &StoreServer,
    seed: impl FnOnce(&Env),
    build: impl FnOnce(&mut Window, &mut Context<V>) -> V,
) -> (AnyWindowHandle, Entity<V>) {
    init(cx);
    install_services(cx, services(env, no_steam()));
    store_ready(cx, server);
    seed(env);
    cx.update(|cx| {
        gpui_kit::open_window(window_options(), cx, |window, cx| {
            cx.new(|cx| build(window, cx))
        })
    })
    .unwrap()
}

/// A Sound Store window over `env` pointed at `server`, with the test's `RecordingAudio`.
pub struct SoundStoreWindow {
    pub win: AnyWindowHandle,
    pub page: Entity<SoundStorePage>,
    pub audio: Arc<RecordingAudio>,
}

/// Services over `env` with recording audio, pointed at `server`, then `seed`, then the page, loaded.
pub fn sound_store_window(
    cx: &mut TestAppContext,
    env: &Env,
    server: &StoreServer,
    seed: impl FnOnce(&Env),
) -> SoundStoreWindow {
    init(cx);
    let audio = Arc::new(RecordingAudio::new());
    install_services(cx, services_with_audio(env, Arc::clone(&audio)));
    store_ready(cx, server);
    seed(env);
    let (win, page) = cx
        .update(|cx| {
            gpui_kit::open_window(window_options(), cx, |window, cx| {
                cx.new(|cx| SoundStorePage::new(window, cx))
            })
        })
        .unwrap();
    wait_real(cx, "the Sound Store", |cx| {
        cx.update(|cx| {
            let page = page.read(cx);
            page.grid().read(cx).is_loaded() && page.player().read(cx).is_ready()
        })
    });
    settle(cx, win);
    SoundStoreWindow { win, page, audio }
}

/// A single-file VPK holding `entries`, built as core's tests/common `vpk_bytes` does.
pub fn vpk_bytes(entries: &[(&str, &[u8])]) -> Vec<u8> {
    let mut tree = Vec::new();
    let mut data = Vec::new();
    for &(path, bytes) in entries {
        let (dir, file) = path.rsplit_once('/').unwrap_or((" ", path));
        let (name, ext) = file.rsplit_once('.').expect("entry needs an extension");
        for part in [ext, dir, name] {
            tree.extend_from_slice(part.as_bytes());
            tree.push(0);
        }
        tree.extend_from_slice(&0u32.to_le_bytes());
        tree.extend_from_slice(&0u16.to_le_bytes());
        tree.extend_from_slice(&0x7fffu16.to_le_bytes());
        tree.extend_from_slice(&(data.len() as u32).to_le_bytes());
        tree.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
        tree.extend_from_slice(&0xffffu16.to_le_bytes());
        tree.extend_from_slice(&[0, 0]);
        data.extend_from_slice(bytes);
    }
    tree.push(0);
    let mut out = Vec::new();
    out.extend_from_slice(&0x55aa_1234u32.to_le_bytes());
    out.extend_from_slice(&1u32.to_le_bytes());
    out.extend_from_slice(&(tree.len() as u32).to_le_bytes());
    out.extend(tree);
    out.extend(data);
    out
}

/// The stock fixture's model on Linux.
pub fn stock_model() -> tf2tools_core::HudModel {
    tf2tools_core::HudModel::load(&hud_stock::stock_files(), tf2tools_core::Platform::Linux)
        .unwrap()
}

/// Writes `tf/gameinfo.txt` naming `tf/tf2_misc.vpk` and a `tf/tf2_misc_dir.vpk` holding the
/// stock files and weapon scripts, with each `(path, Some(text))` replaced or added and each `(path, None)` removed.
pub fn seed_stock_game_with(game: &std::path::Path, overrides: &[(&str, Option<&str>)]) {
    seed_stock_game_with_bytes(game, overrides, &[]);
}

/// As `seed_stock_game_with`, plus binary `extra` files such as textures.
pub fn seed_stock_game_with_bytes(
    game: &std::path::Path,
    overrides: &[(&str, Option<&str>)],
    extra: &[(&str, &[u8])],
) {
    let mut files = hud_stock::stock();
    for (path, text) in overrides {
        files.retain(|(p, _)| p != path);
        if let Some(text) = text {
            files.push(((*path).to_owned(), (*text).to_owned()));
        }
    }
    let mut entries: Vec<(&str, &[u8])> = files
        .iter()
        .map(|(p, t)| (p.as_str(), t.as_bytes()))
        .collect();
    entries.extend_from_slice(extra);
    let scripts = weapon_script_entries();
    entries.extend(scripts.iter().map(|(p, b)| (p.as_str(), *b)));
    let tf = game.join("tf");
    std::fs::create_dir_all(&tf).unwrap();
    std::fs::write(
        tf.join("gameinfo.txt"),
        "\"GameInfo\"\n{\n\tFileSystem\n\t{\n\t\tSearchPaths\n\t\t{\n\t\t\tgame+mod+vgui\ttf/tf2_misc.vpk\n\t\t}\n\t}\n}\n",
    )
    .unwrap();
    std::fs::write(tf.join("tf2_misc_dir.vpk"), vpk_bytes(&entries)).unwrap();
}

pub fn seed_stock_game(game: &std::path::Path) {
    seed_stock_game_with(game, &[]);
}

/// Store addons (asset, display name, release tag), each served at `{uri}/gh/addons/{asset}`,
/// 2355 bytes and 989 downloads.
pub fn seed_addons(env: &Env, uri: &str, rows: &[(&str, &str, &str)]) {
    let mut conn = rusqlite::Connection::open(env.data.db_path()).unwrap();
    let tx = conn.transaction().unwrap();
    for (asset, display, tag) in rows {
        tx.execute(
            "INSERT INTO store_addons (asset_name, display_name, description, download_url, file_size, download_count, release_tag, fetched_at)
             VALUES (?1, ?2, 'Utility addon', ?3, 2355, 989, ?4, 'x')",
            (asset, display, format!("{uri}/gh/addons/{asset}"), tag),
        )
        .unwrap();
    }
    tx.commit().unwrap();
}

/// Imports `vpk` as library addon `asset` at `tag`, and copies it into the env's tf/custom when `install`.
pub fn seed_library_addon(
    services: &Services,
    env: &Env,
    asset: &str,
    tag: &str,
    vpk: &[u8],
    install: bool,
) {
    let path = env.tmp.path().join(format!("seed-{asset}"));
    std::fs::write(&path, vpk).unwrap();
    let name = AddonName::new(asset).unwrap();
    services.library.import_addon(&path, &name, tag).unwrap();
    if install {
        let tf2 = Tf2Install::from_path(&env.game).unwrap();
        services.library.install_addon(&name, &tf2).unwrap();
    }
}

impl StoreServer {
    /// Serves `body` at `/gh/addons/{asset}` after `delay`, where `seed_addons` points.
    pub fn mount_addon(&self, asset: &str, body: Vec<u8>, delay: Duration) {
        self.rt.block_on(
            Mock::given(method("GET"))
                .and(path(format!("/gh/addons/{asset}")))
                .respond_with(
                    ResponseTemplate::new(200)
                        .set_body_bytes(body)
                        .set_delay(delay),
                )
                .mount(&self.server),
        );
    }
    /// GameBanana's Mod Index for `category` in `sort` (`Generic_*`), page `page`.
    pub fn mount_gb_index(&self, category: u32, sort: &str, page: u32, body: Vec<u8>) {
        self.rt.block_on(
            Mock::given(method("GET"))
                .and(path("/apiv11/Mod/Index"))
                .and(query_param(
                    "_aFilters[Generic_Category]",
                    category.to_string(),
                ))
                .and(query_param("_sSort", sort))
                .and(query_param("_nPage", page.to_string()))
                .respond_with(ResponseTemplate::new(200).set_body_bytes(body))
                .mount(&self.server),
        );
    }

    /// `(category, sort, page)` of each Mod Index request so far, in order.
    pub fn gb_index_requests(&self) -> Vec<(String, String, String)> {
        let asked = self
            .rt
            .block_on(self.server.received_requests())
            .unwrap_or_default();
        asked
            .iter()
            .filter(|r| r.url.path() == "/apiv11/Mod/Index")
            .map(|r| {
                let get = |k: &str| {
                    r.url
                        .query_pairs()
                        .find(|(q, _)| q == k)
                        .map(|(_, v)| v.into_owned())
                        .unwrap_or_default()
                };
                (
                    get("_aFilters[Generic_Category]"),
                    get("_sSort"),
                    get("_nPage"),
                )
            })
            .collect()
    }
}

/// A GameBanana add-on record under `root`, in subcategory `sub`.
pub fn gb_addon(id: u64, root: u32, sub: Option<u32>) -> serde_json::Value {
    let mut r = serde_json::json!({"_idRow": id, "_sModelName": "Mod", "_sName": format!("Addon {id}"),
        "_sProfileUrl": format!("https://gamebanana.com/mods/{id}"), "_tsDateAdded": id, "_nLikeCount": 2,
        "_bIsObsolete": false, "_sInitialVisibility": "show", "_bHasContentRatings": false,
        "_aSubmitter": {"_sName": "vex"},
        "_aRootCategory": {"_sName": "Root", "_sProfileUrl": format!("https://gamebanana.com/mods/cats/{root}")}});
    if let Some(sub) = sub {
        r["_aSubCategory"] = serde_json::json!({"_sName": "Sub", "_sProfileUrl": format!("https://gamebanana.com/mods/cats/{sub}")});
    }
    r
}

/// A Mod Index reply holding `records`; `complete` marks the listing's last page.
pub fn gb_listing(records: Vec<serde_json::Value>, complete: bool) -> Vec<u8> {
    serde_json::json!({"_aMetadata": {"_bIsComplete": complete}, "_aRecords": records})
        .to_string()
        .into_bytes()
}

/// An Addon Store window over `env` pointed at `server`.
pub struct AddonStoreWindow {
    pub win: AnyWindowHandle,
    pub page: Entity<AddonStorePage>,
    pub services: Services,
}

/// Services over `env` and TF2 `state`, pointed at `server`, then `seed`, then the page, loaded.
pub fn addon_store_window(
    cx: &mut TestAppContext,
    env: &Env,
    server: &StoreServer,
    state: Tf2State,
    seed: impl FnOnce(&Env, &Services),
) -> AddonStoreWindow {
    init(cx);
    let services = services(env, no_steam());
    install_services(cx, services.clone());
    cx.update(|cx| install_tf2_state(cx, state));
    store_ready(cx, server);
    seed(env, &services);
    let (win, page) = cx
        .update(|cx| {
            gpui_kit::open_window(window_options(), cx, |window, cx| {
                cx.new(|cx| AddonStorePage::new(window, cx))
            })
        })
        .unwrap();
    wait_real(cx, "the Addon Store", |cx| {
        cx.update(|cx| page.read(cx).is_loaded())
    });
    settle(cx, win);
    AddonStoreWindow {
        win,
        page,
        services,
    }
}

/// A crosshair archive in memory: `cross/cross.png`, one green 16x16 PNG.
pub fn crosshair_zip_bytes() -> Vec<u8> {
    let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    zip.start_file("cross/cross.png", zip::write::SimpleFileOptions::default())
        .unwrap();
    std::io::Write::write_all(&mut zip, &crosshair_png([0, 255, 0, 255])).unwrap();
    zip.finish().unwrap().into_inner()
}

/// Every URL serves one 100x56 JPEG, GameBanana's 100 px thumbnail shape.
pub fn thumbnail_fetch() -> ImageFetch {
    let mut out = std::io::Cursor::new(Vec::new());
    image::DynamicImage::ImageRgb8(image::RgbImage::from_pixel(
        100,
        56,
        image::Rgb([40, 80, 120]),
    ))
    .write_to(&mut out, image::ImageFormat::Jpeg)
    .unwrap();
    let jpeg = Arc::new(out.into_inner());
    Arc::new(move |_url: String| {
        let jpeg = Arc::clone(&jpeg);
        Box::pin(async move { Ok(jpeg.to_vec()) })
    })
}

/// A shell with the store globals a timing case needs: a fake hub, stub thumbnails, downloads.
/// `seed` writes rows into a freshly opened database before any page opens.
pub fn store_shell_window(
    cx: &mut TestAppContext,
    env: &Env,
    seed: impl FnOnce(&Env),
) -> (ShellWindow, std::sync::mpsc::Receiver<FakeRun>) {
    init(cx);
    tf2tools_core::Database::open(&env.data).unwrap();
    seed(env);
    let (_hub, runs) = fake_hub(cx);
    cx.update(|cx| {
        RemoteImages::install_with(cx, thumbnail_fetch(), IMAGE_BUDGET, decode_image);
        if !tokio_bridge::is_running(cx) {
            tokio_bridge::init(cx);
        }
        StoreDownloads::install(cx);
    });
    let s = shell_window(cx, env, Tf2State::Missing { saved: None });
    (s, runs)
}

/// Store sounds "Sound 1".."Sound n", alternating hitsound and killsound, no previews.
pub fn seed_n_sounds(env: &Env, n: i64) {
    let names: Vec<String> = (1..=n).map(|i| format!("Sound {i}")).collect();
    let rows: Vec<(i64, &str, &str, Option<&str>)> = (1..=n)
        .map(|i| {
            let category = if i % 2 == 0 { "hitsound" } else { "killsound" };
            (i, names[(i - 1) as usize].as_str(), category, None)
        })
        .collect();
    seed_sounds(env, &rows);
}

/// The value of `key` in a case report's notes; panics when absent.
pub fn note<'a>(report: &'a CaseReport, key: &str) -> &'a str {
    report
        .notes
        .iter()
        .find(|(k, _)| *k == key)
        .map(|(_, v)| v.as_str())
        .unwrap_or_else(|| panic!("no {key} in {report:?}"))
}

/// A fixed RFC 3339 timestamp for seeded log rows.
pub const T0: &str = "2026-10-17T12:00:00.000000+00:00";

/// Every log row, newest first (core's order).
pub fn log_rows(services: &Services) -> Vec<LogEntry> {
    services.activity.query(&LogQuery::new(100_000)).unwrap()
}

/// Inserts `n` info rows (`hud`/`install`, message "row <i>", i from 1) with timestamp `T0`.
/// Opens the database first, so it also seeds an env no window has opened yet.
pub fn seed_log_rows(env: &Env, n: u32) {
    drop(tf2tools_core::Database::open(&env.data).unwrap());
    let mut conn = rusqlite::Connection::open(env.data.db_path()).unwrap();
    let tx = conn.transaction().unwrap();
    for i in 1..=n {
        tx.execute(
            "INSERT INTO logs (timestamp, level, category, action, message, details)
             VALUES (?1, 'info', 'hud', 'install', ?2, NULL)",
            (T0, format!("row {i}")),
        )
        .unwrap();
    }
    tx.commit().unwrap();
}

/// Renames a table, so a test can make the log's queries fail and then restore them.
pub fn rename_table(env: &Env, from: &str, to: &str) {
    let conn = rusqlite::Connection::open(env.data.db_path()).unwrap();
    conn.execute_batch(&format!("ALTER TABLE {from} RENAME TO {to}"))
        .unwrap();
}

/// The kit's bottom-layer dialog box ("surface" inside "dialog"), after its 250 ms slide-in,
/// which runs on the executor clock.
pub fn dialog_box(cx: &mut TestAppContext, win: AnyWindowHandle) -> Bounds<Pixels> {
    settle(cx, win);
    cx.executor().advance_clock(Duration::from_millis(300));
    settle(cx, win);
    cx.update_window(win, |_, w, _| {
        w.within("dialog").try_find("surface").map(|e| e.bounds())
    })
    .unwrap()
    .expect("an open dialog box")
}

/// The window's drawable size.
pub fn viewport(cx: &mut TestAppContext, win: AnyWindowHandle) -> Size<Pixels> {
    cx.update_window(win, |_, w, _| w.viewport_size()).unwrap()
}

/// Every visible toast's label, in order; `ids::TOAST` can match several at once.
pub fn toast_labels(cx: &mut TestAppContext, win: AnyWindowHandle) -> Vec<String> {
    settle(cx, win);
    let toast = ElementId::from(tf2tools_app::ids::TOAST);
    cx.update_window(win, |_, w, _| {
        gpui_kit::base::test_support::snapshots(w)
            .iter()
            .filter(|s| s.path().last() == Some(&toast))
            .filter_map(|s| s.label().map(str::to_owned))
            .collect()
    })
    .unwrap()
}

/// Asserts `text` reads as player words: no URL, host, path, status code or raw error text.
pub fn assert_plain(text: &str) {
    let lower = text.to_lowercase();
    for banned in [
        "http",
        "://",
        ".com",
        "127.0.0.1",
        "localhost",
        "/",
        "\\",
        "error",
        "refused",
        "panic",
        "os ",
        "sqlite",
        "database",
        "cannot",
        "answered",
    ] {
        assert!(!lower.contains(banned), "{text:?} contains {banned:?}");
    }
    // The game's and the app's names are the only numbers a player may read.
    let words = text.replace("TF2Tools", "").replace("TF2", "");
    assert!(
        !words.chars().any(|c| c.is_ascii_digit()),
        "{text:?} has a number"
    );
    assert!(text.ends_with('.'), "{text:?} is not a sentence");
}

/// Counts `LibraryChanged` emissions from now on; keep the subscription alive while counting.
pub fn library_changes(
    cx: &mut TestAppContext,
) -> (std::rc::Rc<std::cell::Cell<usize>>, gpui_kit::Subscription) {
    let count = std::rc::Rc::new(std::cell::Cell::new(0));
    let seen = Rc::clone(&count);
    let sub = cx.update(|cx| {
        let events = tf2tools_app::library_events::LibraryEvents::global(cx);
        cx.subscribe(
            &events,
            move |_, _: &tf2tools_app::library_events::LibraryChanged, _| seen.set(seen.get() + 1),
        )
    });
    (count, sub)
}

/// Wheels the HUD Maker's part list until `key`'s row is wholly in view, from the list's top.
/// The list is capped to the window, so a row far down may start out of view, as for a player.
pub fn reveal_part_row(
    cx: &mut TestAppContext,
    win: AnyWindowHandle,
    key: &tf2tools_core::PartKey,
) {
    let row = tf2tools_app::ids::hud_maker_row(key);
    let list = tf2tools_app::ids::HUD_MAKER_PARTS;
    let up = gpui_kit::ScrollDelta::Lines(gpui_kit::point(0., 1000.));
    cx.update_window(win, |_, w, cx| w.scroll(list, up, cx))
        .unwrap();
    settle(cx, win);
    for _ in 0..80 {
        let seen = cx
            .update_window(win, |_, w, _| {
                let shown = w.try_find(list).map(|l| l.bounds());
                w.try_find(row.clone()).is_some_and(|e| {
                    let b = e.bounds();
                    shown.is_none_or(|l| b.top() >= l.top() && b.bottom() <= l.bottom())
                })
            })
            .unwrap();
        if seen {
            return;
        }
        let down = gpui_kit::ScrollDelta::Lines(gpui_kit::point(0., -1.));
        cx.update_window(win, |_, w, cx| w.scroll(list, down, cx))
            .unwrap();
        settle(cx, win);
    }
    panic!("{row:?} never showed in the part list");
}

/// Clicks `key`'s row in the open part list, wheeling it into view first.
pub fn click_part_row(cx: &mut TestAppContext, win: AnyWindowHandle, key: &tf2tools_core::PartKey) {
    reveal_part_row(cx, win, key);
    click_id(cx, win, tf2tools_app::ids::hud_maker_row(key));
}

/// Switches scene by its tab, or through the scene menu when the strip is too narrow for the tabs.
pub fn click_scene(cx: &mut TestAppContext, win: AnyWindowHandle, scene: tf2tools_core::Scene) {
    if !present(cx, win, tf2tools_app::ids::hud_maker_scene(scene)) {
        click_id(cx, win, tf2tools_app::ids::HUD_MAKER_SCENE_MENU);
    }
    click_id(cx, win, tf2tools_app::ids::hud_maker_scene(scene));
}

/// Puts the HUD Maker's Start from card away with Blank, for a test that presses the middle
/// of the canvas, where the card sits over a new HUD.
pub fn put_start_card_away(cx: &mut TestAppContext, win: AnyWindowHandle) {
    if present(cx, win, tf2tools_app::ids::HUD_MAKER_START_BLANK) {
        click_id(cx, win, tf2tools_app::ids::HUD_MAKER_START_BLANK);
    }
}

/// For a test that sets process-wide state (the panic hook, the global tracing subscriber):
/// returns true in a child process of this binary that runs only `test` (its full name, such as
/// `crash_hook::name`), where the caller runs its body. In the parent it runs that child, asserts
/// it passed and returns false.
pub fn in_own_process(test: &str) -> bool {
    const CHILD: &str = "TF2TOOLS_TEST_OWN_PROCESS";
    if std::env::var_os(CHILD).is_some() {
        return true;
    }
    let out = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", test])
        .env(CHILD, "1")
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "{stdout}\n{stderr}");
    assert!(
        stdout.contains("1 passed"),
        "the child ran no test: {stdout}"
    );
    false
}

/// A `mastercomfig-base.vpk` whose `cfg/comfig/echo.cfg` names `version`, as a release carries it.
pub fn mastercomfig_base(version: &str) -> Vec<u8> {
    let echo =
        format!("alias version_comfig\"echo mastercomfig version: {version} | July 3, 2026\"\n");
    vpk_bytes(&[("cfg/comfig/echo.cfg", echo.as_bytes())])
}

/// Records release `tag` with its base at `{uri}/gh/mastercomfig/mastercomfig-base.vpk`.
pub fn seed_mastercomfig_release(env: &Env, uri: &str, tag: &str) {
    rusqlite::Connection::open(env.data.db_path())
        .unwrap()
        .execute(
            "INSERT OR REPLACE INTO mastercomfig_release (id, tag, base_url, override_urls)
             VALUES (1, ?1, ?2, '[null,null,null]')",
            (tag, format!("{uri}/gh/mastercomfig/mastercomfig-base.vpk")),
        )
        .unwrap();
}

/// Puts `version`'s base VPK in the env's tf/custom and records it as managed on `preset`.
pub fn seed_managed_mastercomfig(env: &Env, version: &str, preset: &str) {
    let base = env.game.join("tf/custom/mastercomfig-base.vpk");
    std::fs::write(base, mastercomfig_base(version)).unwrap();
    rusqlite::Connection::open(env.data.db_path())
        .unwrap()
        .execute(
            "INSERT OR REPLACE INTO mastercomfig_state (id, version, preset, modules)
             VALUES (1, ?1, ?2, '{}')",
            (version, preset),
        )
        .unwrap();
}

/// Core's trimmed mastercomfig 9.100.1 catalog files.
pub const MC_MODULES: &[u8] =
    include_bytes!("../../../tf2tools-core/tests/fixtures/mastercomfig/modules.json");
pub const MC_PRESET_MODULES: &[u8] =
    include_bytes!("../../../tf2tools-core/tests/fixtures/mastercomfig/preset_modules.json");

pub fn module_catalog() -> tf2tools_core::ModuleCatalog {
    tf2tools_core::ModuleCatalog::parse("9.100.1", MC_MODULES, MC_PRESET_MODULES).unwrap()
}

/// Saves the fixture catalog as release `tag`'s, where the store's cache reads it.
pub fn seed_mastercomfig_catalog(env: &Env, tag: &str) {
    let dir = env.data.mastercomfig_dir().join(tag);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("modules.json"), MC_MODULES).unwrap();
    std::fs::write(dir.join("preset_modules.json"), MC_PRESET_MODULES).unwrap();
}

/// Sets the saved module choices of a managed install, as JSON (`{"fpscap":"120","lod":null}`).
pub fn seed_mastercomfig_choices(env: &Env, modules: &str) {
    rusqlite::Connection::open(env.data.db_path())
        .unwrap()
        .execute("UPDATE mastercomfig_state SET modules = ?1", [modules])
        .unwrap();
}

/// Points the recorded release's three override assets at `{uri}/gh/mastercomfig/`.
pub fn seed_mastercomfig_overrides(env: &Env, uri: &str) {
    let urls: Vec<String> = (1..=3)
        .map(|n| format!("{uri}/gh/mastercomfig/mastercomfig-addon-override-{n}.vpk"))
        .collect();
    rusqlite::Connection::open(env.data.db_path())
        .unwrap()
        .execute(
            "UPDATE mastercomfig_release SET override_urls = ?1",
            [serde_json::to_string(&urls).unwrap()],
        )
        .unwrap();
}
