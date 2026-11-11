//! The logs.tf page: tracked players as chips, their 90-day class averages and their logs.

mod detail;
mod grid;
mod list;
pub mod text;

use gpui_kit::assets::IconName;
use gpui_kit::component::ActiveTheme as _;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::{Disableable as _, Sizable as _, WindowExt as _, h_flex, v_flex};
use gpui_kit::{
    App, AppContext as _, Context, Entity, FontWeight, InteractiveElement as _, IntoElement,
    ListAlignment, ListState, ParentElement as _, Render, SharedString,
    StatefulInteractiveElement as _, Styled as _, Subscription, Task, TestSupportExt as _, Window,
    div, px,
};
use tf2tools_core::{
    ClassAverage, LogDetail, LogRow, LogState, LogsProgress, LogsSyncError, LogsSyncSummary,
    SteamId, TrackedPlayer, parse_log, parse_player_input, window_start,
};

use crate::ids;
use crate::pages::common::filter_badge;
use crate::services::{OpError, Services, run_blocking};
use crate::shell::CONTENT_PADDING;
use crate::shell::confirm::{Confirm, confirm};
use crate::shell::route::Page;
use crate::shell::toast::toast_error;
use crate::tokio_bridge;
use crate::widgets::dialog_frame::{DIALOG_WIDTH, DialogFrame};
use crate::widgets::store_error::error_line;
use crate::widgets::{Named as _, labelled};
use text::{
    EMPTY_TEXT, LOGS_TF_DOWN, LOGS_TF_URL, SAVE_FAILED, STEAM_UNREACHABLE, profile_url,
    progress_text, resolve_message, sync_message,
};

/// The logs.tf page. It reads the tracked players from the cache on creation and is kept
/// while hidden, so a visit elsewhere leaves it as it was.
pub struct LogsTfPage {
    players: Vec<TrackedPlayer>,
    active: Option<SteamId>,
    loaded: bool,
    rows: Vec<LogRow>,
    averages: Option<[ClassAverage; 9]>,
    // The open log's id; the list's item 0 is the page top, so row `i` is item `i + 1`.
    expanded: Option<u64>,
    // The open log as read from its stored body, keyed by its id so a read that lands after
    // its row closed is dropped; None inside means the body could not be read.
    detail: Option<(u64, Option<LogDetail>)>,
    list: ListState,
    // Which optional row columns fit, from the list's width at the last frame; the list's
    // state cannot be asked while it lays its own items out.
    fit: list::Fit,
    // One sync runs at a time and is never cancelled; a player chosen meanwhile waits in
    // `queued`, so `syncing` stays true until the queue is empty.
    syncing: bool,
    running: Option<SteamId>,
    queued: Option<SteamId>,
    progress: Option<LogsProgress>,
    error: Option<&'static str>,
    // Cache reads can land out of order; only the newest applies, and it starts the sync a
    // superseded read asked for.
    load_generation: u64,
    sync_wanted: bool,
    // The add dialog's field, its last failure, whether a lookup is running and whether the
    // dialog is still up. They live here because the dialog builder runs every frame.
    add_input: Entity<InputState>,
    add_error: Option<SharedString>,
    adding: bool,
    add_open: bool,
    _subscriptions: Vec<Subscription>,
}

impl LogsTfPage {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let add_input =
            cx.new(|cx| InputState::new(window, cx).placeholder("Steam profile link or Steam ID"));
        // gpui-kit owns InputEvent; Enter in the field adds like the Add button.
        let enter = cx.subscribe_in(
            &add_input,
            window,
            |page, _, event: &InputEvent, window, cx| {
                if matches!(event, InputEvent::PressEnter { .. }) {
                    page.submit_add(window, cx);
                }
            },
        );
        let mut page = Self {
            players: Vec::new(),
            active: None,
            loaded: false,
            rows: Vec::new(),
            averages: None,
            expanded: None,
            detail: None,
            list: ListState::new(1, ListAlignment::Top, px(200.)),
            fit: list::Fit::ALL,
            syncing: false,
            running: None,
            queued: None,
            progress: None,
            error: None,
            load_generation: 0,
            sync_wanted: false,
            add_input,
            add_error: None,
            adding: false,
            add_open: false,
            _subscriptions: vec![enter],
        };
        page.load(true, cx).detach();
        page
    }

    /// Kept pages hear when they leave and return; coming back rereads the cache and syncs
    /// the active player if their logs are stale.
    pub fn set_hidden(&mut self, hidden: bool, cx: &mut Context<Self>) {
        if !hidden {
            self.load(true, cx).detach();
        }
    }

    /// True once the first read of the cache has landed.
    pub fn is_loaded(&self) -> bool {
        self.loaded
    }

    /// True while a player's logs sync or another player's sync waits its turn.
    pub fn is_syncing(&self) -> bool {
        self.syncing
    }

    /// The tracked players, in the order they were added.
    pub fn players(&self) -> &[TrackedPlayer] {
        &self.players
    }

    /// The active player's logs in the 90-day window, newest first, from the cache.
    pub fn rows(&self) -> &[LogRow] {
        &self.rows
    }

    /// The active player's class averages; None before the first load.
    pub fn averages(&self) -> Option<&[ClassAverage; 9]> {
        self.averages.as_ref()
    }

    /// Why the last sync failed, shown under the chips.
    pub fn error(&self) -> Option<&str> {
        self.error
    }

    /// Forces a sync of the active player; a no-op while a sync runs.
    pub fn refresh(&mut self, cx: &mut Context<Self>) {
        if let (false, Some(id)) = (self.syncing, self.active) {
            self.sync(id, true, cx);
        }
    }

    /// The page's one scrolling list: the top, then a row per log.
    pub fn list_state(&self) -> &ListState {
        &self.list
    }

    /// The log whose row is open, if any.
    pub fn expanded(&self) -> Option<u64> {
        self.expanded
    }

    /// The open row's log, once its body is read and parsed.
    pub fn detail(&self) -> Option<&LogDetail> {
        self.detail
            .as_ref()
            .filter(|(id, _)| self.expanded == Some(*id))
            .and_then(|(_, detail)| detail.as_ref())
    }

    /// Opens log `id`'s row, closing any other; on the open row it closes it.
    pub fn toggle(&mut self, id: u64, cx: &mut Context<Self>) {
        let closing = self.expanded.take();
        self.detail = None;
        if closing != Some(id) {
            self.expanded = Some(id);
            self.read_detail(id, cx);
        }
        // Both rows change height, so both remeasure and the rows below move with them.
        for changed in closing.into_iter().chain([id]) {
            if let Some(ix) = self.rows.iter().position(|r| r.id() == changed) {
                self.list.remeasure_items(ix + 1..ix + 2);
            }
        }
        cx.notify();
    }

    // Reads and parses log `id`'s stored body off the UI thread. It lands only if that row is
    // still the open one, and its row remeasures to fit the panel.
    fn read_detail(&mut self, id: u64, cx: &mut Context<Self>) {
        let reading = run_blocking(cx, move |s| {
            let body = s
                .logs
                .cache()
                .log_body(id)
                .map_err(|e| OpError::core("Failed to read a logs.tf log", e))?
                .ok_or_else(|| OpError::core("Failed to read a logs.tf log", "no stored body"))?;
            parse_log(&body).map_err(|e| OpError::core("Failed to read a logs.tf log", e))
        });
        cx.spawn(async move |page, cx| {
            let read = reading.await;
            let _ = page.update(cx, |page, cx| {
                if page.expanded != Some(id) {
                    return;
                }
                let detail = read
                    .inspect_err(|error| tracing::warn!(%error, id, "reading a logs.tf log failed"))
                    .ok();
                page.detail = Some((id, detail));
                if let Some(ix) = page.rows.iter().position(|r| r.id() == id) {
                    page.list.remeasure_items(ix + 1..ix + 2);
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// The player whose chip is selected, if any is tracked.
    pub fn active(&self) -> Option<SteamId> {
        self.active
    }

    /// The add dialog's Steam link or ID field, emptied after a successful add.
    pub fn add_input(&self) -> &Entity<InputState> {
        &self.add_input
    }

    /// Makes `id`'s chip the active one, saves that choice and reloads the page from the cache.
    pub fn select(&mut self, id: SteamId, window: &mut Window, cx: &mut Context<Self>) {
        let saving = run_blocking(cx, move |s| {
            s.logs
                .cache()
                .set_active_player(id)
                .map_err(|_| OpError::core("Failed to switch player", SAVE_FAILED))
        });
        self.after_write(saving, window, cx);
    }

    // Asks before forgetting a player; their logs stay cached until pruned.
    fn remove(&mut self, id: SteamId, name: &str, window: &mut Window, cx: &mut Context<Self>) {
        let answer = confirm(
            Confirm {
                title: format!("Stop tracking {name}?").into(),
                description: format!("{name}'s chip goes away. You can add them again later.")
                    .into(),
                confirm_label: "Stop tracking".into(),
                cancel_label: "Cancel".into(),
            },
            window,
            cx,
        );
        cx.spawn_in(window, async move |page, cx| {
            if answer.recv().await != Ok(true) {
                return;
            }
            let _ = page.update_in(cx, |page, window, cx| {
                let removing = run_blocking(cx, move |s| {
                    s.logs
                        .cache()
                        .remove_player(id)
                        .map_err(|_| OpError::core("Failed to stop tracking", SAVE_FAILED))
                });
                page.after_write(removing, window, cx);
            });
        })
        .detach();
    }

    // Opens the add dialog over the page. It reads the field, error and busy flag from the
    // page on every frame, so a failure shows in place and the typed text stays.
    fn open_add(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.add_error = None;
        self.add_open = true;
        let page = cx.entity().downgrade();
        let input = self.add_input.clone();
        // Deferred so the click that opened it cannot pull focus back out of the field.
        window.defer(cx, move |window, cx| {
            let frame = DialogFrame::new(DIALOG_WIDTH);
            window.open_dialog(cx, move |dialog, window, cx| {
                let Some(entity) = page.upgrade() else {
                    return dialog;
                };
                let this = entity.read(cx);
                let blank = this.add_input.read(cx).value().trim().is_empty();
                let content = v_flex()
                    .gap_4()
                    .child(
                        div()
                            .text_lg()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child("Add player"),
                    )
                    .child(
                        div()
                            .id(ids::LOGSTF_ADD_INPUT)
                            .test_support()
                            .child(Input::new(&this.add_input)),
                    )
                    .children(this.add_error.clone().map(|error| {
                        labelled(
                            div().text_sm().text_color(cx.theme().danger),
                            ids::LOGSTF_ADD_ERROR,
                            error,
                        )
                    }))
                    .child(
                        h_flex()
                            .justify_end()
                            .gap_2()
                            .child(
                                Button::new(ids::LOGSTF_ADD_CANCEL)
                                    .outline()
                                    .label("Cancel")
                                    .on_click(|_, window, cx| window.close_dialog(cx)),
                            )
                            .child(
                                Button::new(ids::LOGSTF_ADD_CONFIRM)
                                    .primary()
                                    .label("Add")
                                    .disabled(blank || this.adding)
                                    .on_click({
                                        let entity = entity.clone();
                                        move |_, window, cx| {
                                            entity.update(cx, |page, cx| {
                                                page.submit_add(window, cx);
                                            });
                                        }
                                    }),
                            ),
                    );
                // Enter belongs to the field, which adds; the kit's Confirm never closes it.
                // Every way out clears `add_open`, so a late lookup closes no other dialog.
                frame
                    .apply(dialog, &*window, &*cx)
                    .on_ok(|_, _, _| false)
                    .on_close(move |_, _, cx| entity.update(cx, |page, _| page.add_open = false))
                    .child(frame.measure(content))
            });
            // The dialog takes focus as it opens, so the field asks for it back afterwards.
            input.update(cx, |state, cx| state.focus(window, cx));
        });
    }

    // Reads the field, looks the player up on Steam, then tracks and activates them. A bad
    // input or a failed lookup or write stays in the dialog with its text kept.
    fn submit_add(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let typed = self.add_input.read(cx).value();
        if self.adding || typed.trim().is_empty() {
            return;
        }
        let input = match parse_player_input(typed.trim()) {
            Ok(input) => input,
            Err(error) => {
                self.add_error = Some(error.to_string().into());
                cx.notify();
                return;
            }
        };
        self.add_error = None;
        self.adding = true;
        cx.notify();
        if !tokio_bridge::is_running(cx) {
            tokio_bridge::init(cx);
        }
        let client = cx.global::<Services>().logs.clone();
        let resolving = tokio_bridge::spawn(cx, async move { client.resolve_player(&input).await });
        cx.spawn_in(window, async move |page, cx| {
            let added = match resolving.await {
                Ok(Ok(player)) => {
                    let saving = page.update(cx, |_, cx| {
                        run_blocking(cx, move |s| {
                            s.logs
                                .cache()
                                .add_player(&player)
                                .map_err(|e| OpError::core("Failed to add player", e))
                        })
                    });
                    match saving {
                        Ok(saving) => saving.await.map_err(|error| {
                            tracing::warn!(%error, "saving a logs.tf player failed");
                            SAVE_FAILED
                        }),
                        Err(_) => return,
                    }
                }
                Ok(Err(error)) => {
                    tracing::warn!(%error, "looking up a Steam profile failed");
                    Err(resolve_message(&error))
                }
                Err(error) => {
                    tracing::warn!(%error, "the Steam lookup task stopped");
                    Err(STEAM_UNREACHABLE)
                }
            };
            let _ = page.update_in(cx, |page, window, cx| {
                page.adding = false;
                match added {
                    Ok(()) => {
                        page.add_input
                            .update(cx, |state, cx| state.set_value("", window, cx));
                        if page.add_open {
                            page.add_open = false;
                            window.close_dialog(cx);
                        }
                        page.load(true, cx).detach();
                    }
                    Err(message) => page.add_error = Some(message.into()),
                }
                cx.notify();
            });
        })
        .detach();
    }

    // Every chip write ends the same way: a failure toasts, then the page rereads the cache,
    // which is the truth either way.
    fn after_write(
        &mut self,
        write: impl Future<Output = Result<(), OpError>> + 'static,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        cx.spawn_in(window, async move |page, cx| {
            let written = write.await;
            let _ = page.update_in(cx, |page, window, cx| {
                if let Err(err) = written {
                    toast_error(window, cx, &err);
                }
                page.load(true, cx).detach();
            });
        })
        .detach();
    }

    // Reads the players, the active chip and that player's logs and averages off the UI
    // thread. With `sync`, the landing also syncs the active player, in the same update that
    // sets `loaded`. A failed read leaves the page as it was; the warning keeps the cause.
    fn load(&mut self, sync: bool, cx: &mut Context<Self>) -> Task<()> {
        self.sync_wanted |= sync;
        self.load_generation += 1;
        let generation = self.load_generation;
        let loading = run_blocking(cx, |s| {
            let cache = s.logs.cache();
            let read = |e| OpError::core("Failed to load logs.tf players", e);
            let players = cache.players().map_err(read)?;
            let active = cache.active_player().map_err(read)?;
            let since = window_start(chrono::Utc::now().timestamp());
            let logs = match active {
                Some(id) => Some((
                    cache.logs(id, since).map_err(read)?,
                    cache.class_averages(id, since).map_err(read)?,
                )),
                None => None,
            };
            Ok((players, active, logs))
        });
        cx.spawn(async move |page, cx| {
            let loaded = loading.await;
            let _ = page.update(cx, |page, cx| {
                if page.load_generation != generation {
                    return;
                }
                match loaded {
                    Ok((players, active, logs)) => {
                        page.players = players;
                        page.active = active;
                        let (rows, averages) = logs.unzip();
                        page.rows = rows.unwrap_or_default();
                        page.averages = averages;
                        page.fit_list();
                    }
                    Err(error) => tracing::warn!(%error, "reading the logs.tf cache failed"),
                }
                page.loaded = true;
                if std::mem::take(&mut page.sync_wanted)
                    && let Some(id) = page.active
                {
                    page.sync(id, false, cx);
                }
                cx.notify();
            });
        })
    }

    // Matches the list to fresh rows: added or dropped rows splice in after the top, and
    // every item remeasures since a row's state or the top's height may have changed. An open
    // row that is gone or no longer downloaded closes.
    fn fit_list(&mut self) {
        let items = self.rows.len() + 1;
        let old = self.list.item_count();
        if old != items {
            self.list.splice(1..old, self.rows.len());
        }
        self.list.remeasure_items(0..items);
        let open = |r: &LogRow| Some(r.id()) == self.expanded && r.state() == LogState::Ready;
        if !self.rows.iter().any(open) {
            self.expanded = None;
            self.detail = None;
        }
    }

    // Syncs `id` on the tokio runtime, rereading the cache after each progress event so logs
    // appear as they land. While another sync runs, `id` replaces whoever waits next.
    fn sync(&mut self, id: SteamId, force: bool, cx: &mut Context<Self>) {
        if self.syncing {
            self.queued = (self.running != Some(id)).then_some(id);
            return;
        }
        self.syncing = true;
        self.running = Some(id);
        self.progress = None;
        cx.notify();
        if !tokio_bridge::is_running(cx) {
            tokio_bridge::init(cx);
        }
        let client = cx.global::<Services>().logs.clone();
        // Only the newest count is kept, so a slow frame skips counts rather than queueing them.
        let (tx, mut rx) = tokio::sync::watch::channel(None::<LogsProgress>);
        let job = tokio_bridge::spawn(cx, async move {
            let report = move |progress| {
                tx.send_replace(Some(progress));
            };
            client.sync_player(id, force, &report).await
        });
        // Each reread lands before the next event is taken, so events that arrive meanwhile
        // collapse into one reread of the newest count.
        cx.spawn(async move |page, cx| {
            while rx.changed().await.is_ok() {
                let progress = *rx.borrow_and_update();
                let reread = page.update(cx, |page, cx| {
                    page.progress = progress;
                    cx.notify();
                    page.load(false, cx)
                });
                match reread {
                    Ok(reread) => reread.await,
                    Err(_) => return,
                }
            }
            let joined = job.await;
            let _ = page.update(cx, |page, cx| page.synced(joined, cx));
        })
        .detach();
    }

    // Ends a sync: keeps or clears the error line, starts the queued player's sync if any and
    // rereads the cache. A queued player removed before their turn is no failure.
    fn synced(
        &mut self,
        joined: Result<Result<LogsSyncSummary, LogsSyncError>, tokio_bridge::JoinError>,
        cx: &mut Context<Self>,
    ) {
        match joined {
            Ok(Ok(_)) => self.error = None,
            Ok(Err(LogsSyncError::NotTracked { .. })) => {}
            Ok(Err(error)) => {
                tracing::warn!(%error, "syncing logs.tf logs failed");
                self.error = Some(sync_message(&error));
            }
            Err(error) => {
                tracing::warn!(%error, "the logs.tf sync task stopped");
                self.error = Some(LOGS_TF_DOWN);
            }
        }
        self.syncing = false;
        self.running = None;
        self.progress = None;
        if let Some(next) = self.queued.take() {
            self.sync(next, false, cx);
        }
        self.load(false, cx).detach();
        cx.notify();
    }

    // Refresh and the active player's logs.tf profile, beside the chips.
    fn actions(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let syncing = self.syncing;
        h_flex()
            .flex_none()
            .gap_2()
            .child(
                Button::new(ids::LOGSTF_REFRESH)
                    .outline()
                    .icon(IconName::RefreshCw)
                    .label(if syncing { "Refreshing..." } else { "Refresh" })
                    .loading(syncing)
                    .loading_icon(IconName::RefreshCw)
                    .disabled(syncing)
                    .on_click(cx.listener(|page, _, _, cx| page.refresh(cx))),
            )
            .children(self.active.map(|id| {
                Button::new(ids::LOGSTF_PROFILE)
                    .ghost()
                    .icon(IconName::ExternalLink)
                    .label("Open profile on logs.tf")
                    .on_click(move |_, _, cx| cx.open_url(&profile_url(id)))
            }))
    }

    // The active player's progress line while their logs download, then the last failure
    // with its Retry, hidden while a sync runs since Retry could not start another.
    fn status(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let ours = self.syncing && self.running == self.active;
        // A count of 0 of 0 says nothing, so the line waits for the list to arrive.
        let progress = self.progress.filter(|p| ours && p.total() > 0).map(|p| {
            labelled(
                div().text_sm().text_color(cx.theme().muted_foreground),
                ids::LOGSTF_PROGRESS,
                progress_text(p.done(), p.total()),
            )
        });
        let error = self.error.filter(|_| !self.syncing).map(|error| {
            let page = cx.entity().downgrade();
            let on_retry = move |_: &mut Window, cx: &mut App| {
                let _ = page.update(cx, |page, cx| page.refresh(cx));
            };
            error_line(ids::LOGSTF_ERROR, ids::LOGSTF_RETRY, error, on_retry, cx)
        });
        v_flex().gap_2().children(progress).children(error)
    }

    // One chip per tracked player in order, each with its x, then "+ Add player", wrapping.
    fn chips(&self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let mut row = h_flex().flex_wrap().gap_2();
        for player in &self.players {
            let id = player.steam_id();
            let name = player.name().to_owned();
            let chip = filter_badge(
                ids::logstf_chip(id),
                self.active == Some(id),
                name.clone(),
                window,
                cx,
            )
            .on_click(cx.listener(move |page, _, window, cx| page.select(id, window, cx)));
            let remove = Button::new(ids::logstf_chip_remove(id))
                .ghost()
                .xsmall()
                .icon(IconName::X)
                .named(format!("Stop tracking {name}"))
                .on_click(cx.listener(move |page, _, window, cx| {
                    page.remove(id, &name, window, cx);
                }));
            row = row.child(h_flex().gap_0p5().child(chip).child(remove));
        }
        row.child(
            filter_badge(ids::LOGSTF_ADD, false, "+ Add player", window, cx)
                .on_click(cx.listener(|page, _, window, cx| page.open_add(window, cx))),
        )
    }

    // Everything above the logs, the list's first item: the credit, chips and their buttons,
    // the status line, the grid, then the column titles or the no-logs line.
    fn top(&self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let empty = self.loaded && self.players.is_empty();
        let fetching = self.syncing && self.running == self.active;
        let no_logs = self
            .active
            .filter(|_| self.loaded && !fetching && self.rows.is_empty());
        v_flex()
            .w_full()
            .gap_4()
            .pb_4()
            .child(
                h_flex().justify_end().child(
                    Button::new(ids::LOGSTF_CREDIT)
                        .ghost()
                        .icon(IconName::ExternalLink)
                        .label("Stats from logs.tf")
                        .on_click(|_, _, cx| cx.open_url(LOGS_TF_URL)),
                ),
            )
            .children((!self.players.is_empty()).then(|| {
                h_flex()
                    .items_start()
                    .gap_4()
                    .child(div().flex_1().min_w_0().child(self.chips(window, cx)))
                    .child(self.actions(cx))
            }))
            .child(self.status(cx))
            .children(
                self.averages
                    .as_ref()
                    .filter(|_| !self.players.is_empty())
                    .map(|averages| grid::class_grid(averages, self.rows.len(), cx)),
            )
            .children(empty.then(|| self.empty_state(cx)))
            .children(no_logs.map(|id| list::no_logs(id, cx)))
            .children((!self.rows.is_empty()).then(|| list::column_header(self.fit, cx)))
    }

    // The no-player explanation and its way to track someone.
    fn empty_state(&self, cx: &mut Context<Self>) -> impl IntoElement {
        v_flex()
            .items_center()
            .gap_4()
            .py_12()
            .child(labelled(
                div().text_sm().text_color(cx.theme().muted_foreground),
                ids::LOGSTF_EMPTY,
                EMPTY_TEXT,
            ))
            .child(
                Button::new(ids::LOGSTF_EMPTY_ADD)
                    .primary()
                    .icon(IconName::Plus)
                    .label("Add player")
                    .on_click(cx.listener(|page, _, window, cx| page.open_add(window, cx))),
            )
    }
}

impl Render for LogsTfPage {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.fit = list::fit(&self.list);
        // The page scrolls as one list, so the grid and the logs move together.
        v_flex()
            .id(ids::page(Page::LogsTf))
            .test_support()
            .size_full()
            .p(CONTENT_PADDING)
            .child(list::page_list(self, cx).flex_1().min_h_0())
    }
}
