//! A scripted update backend: every check and download waits for the test to answer it.

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::mpsc::{Receiver, channel};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use gpui_kit::{App, Context, Entity, TestAppContext};
use tf2tools_app::services::Services;
use tf2tools_app::updater::{
    AfterInstall, CheckFuture, DownloadFuture, DownloadSink, Failure, Offer, Restart,
    UpdateBackend, UpdateEvent, UpdateSource, UpdateState, Updater,
};
use tf2tools_core::LogLevel;
use tokio::sync::oneshot;

pub type CheckReply = oneshot::Sender<Result<Option<Offer>, Failure>>;
pub type DownloadReply = oneshot::Sender<Result<(), Failure>>;

pub const CURRENT: &str = "6.0.0";
pub const RELEASES: &str = "https://github.com/PhongGuy/TF2Tools/releases/latest";
pub const APPIMAGE: &str = "/apps/tf2tools_6.0.0_x86_64.AppImage";

pub fn offer(version: &str, installable: bool) -> Offer {
    Offer {
        version: version.to_owned(),
        release_page: format!("https://github.com/PhongGuy/TF2Tools/releases/tag/v{version}"),
        installable,
    }
}

pub fn failure(message: &str, use_release_page: bool) -> Failure {
    Failure {
        message: message.to_owned(),
        use_release_page,
    }
}

pub struct FakeUpdates {
    checks: Receiver<CheckReply>,
    downloads: Receiver<(DownloadSink, DownloadReply)>,
    pub apply_result: Arc<Mutex<Result<AfterInstall, Failure>>>,
    /// Lock it to hold `apply` (on tokio's blocking pool) until the guard drops.
    pub apply_hold: Arc<Mutex<()>>,
    pub restarts: Rc<RefCell<Vec<AfterInstall>>>,
}

/// A source over the scripted backend (no backend when `available` is false), the restart
/// recorder, and the test's handles.
pub fn fake_source(available: bool) -> (UpdateSource, Restart, FakeUpdates) {
    let (check_tx, checks) = channel::<CheckReply>();
    let (download_tx, downloads) = channel::<(DownloadSink, DownloadReply)>();
    let apply_result = Arc::new(Mutex::new(Ok(AfterInstall::Relaunch(PathBuf::from(
        APPIMAGE,
    )))));
    let applied = Arc::clone(&apply_result);
    let apply_hold = Arc::new(Mutex::new(()));
    let hold = Arc::clone(&apply_hold);
    let backend = UpdateBackend {
        check: Arc::new(move || -> CheckFuture {
            let (tx, rx) = oneshot::channel();
            check_tx.send(tx).unwrap();
            Box::pin(async move {
                rx.await
                    .unwrap_or_else(|_| Err(failure("the test dropped the check", false)))
            })
        }),
        download: Arc::new(move |sink: DownloadSink| -> DownloadFuture {
            let (tx, rx) = oneshot::channel();
            download_tx.send((sink, tx)).unwrap();
            Box::pin(async move {
                rx.await
                    .unwrap_or_else(|_| Err(failure("the test dropped the download", false)))
            })
        }),
        apply: Arc::new(move || {
            let _held = hold.lock().unwrap();
            applied.lock().unwrap().clone()
        }),
    };
    let restarts = Rc::new(RefCell::new(Vec::new()));
    let seen = Rc::clone(&restarts);
    let restart: Restart =
        Rc::new(move |after: AfterInstall, _: &mut App| seen.borrow_mut().push(after));
    let source = UpdateSource {
        backend: available.then_some(backend),
        current: CURRENT.to_owned(),
        releases_page: RELEASES.to_owned(),
    };
    (
        source,
        restart,
        FakeUpdates {
            checks,
            downloads,
            apply_result,
            apply_hold,
            restarts,
        },
    )
}

/// Installs an updater over the scripted backend, or with no backend when `available` is false.
pub fn fake_updater(cx: &mut TestAppContext, available: bool) -> (Entity<Updater>, FakeUpdates) {
    cx.executor().allow_parking();
    let (source, restart, fake) = fake_source(available);
    let updater = cx.update(|cx| Updater::install(cx, source, restart));
    (updater, fake)
}

fn next<T>(cx: &mut TestAppContext, rx: &Receiver<T>, what: &str) -> T {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        cx.run_until_parked();
        if let Ok(value) = rx.try_recv() {
            return value;
        }
        assert!(Instant::now() < deadline, "timed out waiting for {what}");
        std::thread::sleep(Duration::from_millis(5));
    }
}

impl FakeUpdates {
    pub fn next_check(&self, cx: &mut TestAppContext) -> CheckReply {
        next(cx, &self.checks, "a check")
    }

    /// The sink and the reply of the next download. Drop the sink before replying, as the
    /// real download future does, or the updater keeps waiting for progress.
    pub fn next_download(&self, cx: &mut TestAppContext) -> (DownloadSink, DownloadReply) {
        next(cx, &self.downloads, "a download")
    }

    pub fn check_pending(&self) -> bool {
        self.checks.try_recv().is_ok()
    }

    pub fn download_pending(&self) -> bool {
        self.downloads.try_recv().is_ok()
    }
}

pub fn state(cx: &mut TestAppContext, updater: &Entity<Updater>) -> UpdateState {
    cx.update(|cx| updater.read(cx).state().clone())
}

/// Waits, moving the virtual clock, until the state matches.
pub fn wait_state(
    cx: &mut TestAppContext,
    updater: &Entity<Updater>,
    what: &str,
    done: impl Fn(&UpdateState) -> bool,
) {
    super::wait_hub(cx, what, |cx| done(&state(cx, updater)));
}

/// Checks and answers with `offer`, ending in Available.
pub fn reach_available(
    cx: &mut TestAppContext,
    updater: &Entity<Updater>,
    fake: &FakeUpdates,
    offer: Offer,
) {
    assert!(
        cx.update(|cx| updater.update(cx, |u, cx| u.check(cx))),
        "the updater should start a check"
    );
    fake.next_check(cx).send(Ok(Some(offer))).unwrap();
    wait_state(cx, updater, "Available", |s| {
        matches!(s, UpdateState::Available(_))
    });
}

/// Goes on from Available to Ready through a download reporting 100%.
pub fn reach_ready(
    cx: &mut TestAppContext,
    updater: &Entity<Updater>,
    fake: &FakeUpdates,
    offer: Offer,
) {
    reach_available(cx, updater, fake, offer);
    assert!(
        cx.update(|cx| updater.update(cx, |u, cx| u.download(cx))),
        "the updater should start a download from Available"
    );
    let (sink, reply) = fake.next_download(cx);
    sink.report(1, 1);
    drop(sink);
    reply.send(Ok(())).unwrap();
    wait_state(cx, updater, "Ready", |s| matches!(s, UpdateState::Ready(_)));
}

pub fn act<R>(
    cx: &mut TestAppContext,
    u: &Entity<Updater>,
    f: impl FnOnce(&mut Updater, &mut Context<Updater>) -> R,
) -> R {
    cx.update(|cx| u.update(cx, f))
}

pub fn visible(cx: &mut TestAppContext, u: &Entity<Updater>) -> bool {
    cx.update(|cx| u.read(cx).banner_visible())
}

/// Every event the updater emits from now on.
pub fn events(cx: &mut TestAppContext, u: &Entity<Updater>) -> Rc<RefCell<Vec<UpdateEvent>>> {
    let seen = Rc::new(RefCell::new(Vec::new()));
    let sink = Rc::clone(&seen);
    cx.update(|cx| {
        cx.subscribe(u, move |_, ev: &UpdateEvent, _| {
            sink.borrow_mut().push(ev.clone())
        })
        .detach()
    });
    seen
}

/// Every `settings` row as (level, action, message), oldest first.
pub fn update_rows(s: &Services) -> Vec<(LogLevel, String, String)> {
    let mut rows: Vec<_> = super::log_rows(s)
        .into_iter()
        .filter(|r| r.category() == "settings")
        .map(|r| (r.level(), r.action().to_owned(), r.message().to_owned()))
        .collect();
    rows.reverse();
    rows
}

/// Services installed as the app global, so activity rows have somewhere to go.
pub fn with_services(cx: &mut TestAppContext, env: &super::Env) -> Services {
    let services = super::services(env, super::no_steam());
    super::install_services(cx, services.clone());
    services
}
