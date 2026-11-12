//! The logs.tf page: tracked players as chips, their class averages and their logs over the
//! history range the player picks.

mod class_weapons;
mod detail;
mod grid;
mod list;
pub mod sort;
pub mod tables;
pub mod text;

pub use list::STAT_COLUMNS;

use std::collections::{BTreeSet, HashSet};

use gpui_kit::assets::IconName;
use gpui_kit::component::ActiveTheme as _;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::slider::{SliderEvent, SliderState};
use gpui_kit::component::{Disableable as _, Sizable as _, WindowExt as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    App, AppContext as _, Context, Entity, FontWeight, InteractiveElement as _, IntoElement,
    ListAlignment, ListOffset, ListState, ParentElement as _, Render, SharedString,
    StatefulInteractiveElement as _, Styled as _, Subscription, Task, TestSupportExt as _, Window,
    div, px,
};
use tf2tools_core::{
    ClassAverage, ClassWeapons, LogDetail, LogRow, LogState, LogsRange, MatrixKind, SteamId,
    TfClass, TrackedPlayer, WeaponNames, parse_log, parse_player_input,
};

use crate::ids;
use crate::kill_icons::{KillIconImages, LoadedKillIcons};
use crate::logs_sync::{LogsSync, LogsSyncEvent, SyncProgress, logs_sync};
use crate::pages::common::filter_badge;
use crate::picker::PickKind;
use crate::services::{OpError, Services, run_blocking};
use crate::shell::CONTENT_PADDING;
use crate::shell::confirm::{Confirm, confirm};
use crate::shell::route::Page;
use crate::shell::toast::{ToastKind, toast, toast_error};
use crate::tokio_bridge;
use crate::weapon_names::LoadedWeaponNames;
use crate::widgets::dialog_frame::{DETAIL_WIDTH, DIALOG_WIDTH, DialogFrame};
use crate::widgets::store_error::error_line;
use crate::widgets::{Named as _, labelled};
use sort::{PlayerColumn, PlayerSort};
use text::{
    DOWNLOAD_FAILED, DOWNLOADED, EMPTY_TEXT, LOG_UNREADABLE, LOGS_TF_DOWN, LOGS_TF_URL, LOOKING_UP,
    REMOVING, SAVE_FAILED, STEAM_UNREACHABLE, WRITE_FAILED, download_reason, fetch_message,
    profile_url, progress_line, resolve_message,
};

// Logs counted per backfill batch: small enough that one batch's transaction stays short.
const WEAPON_BATCH: usize = 50;

// The open log: its id, how far its body got and which of its rounds are open.
pub(super) struct OpenLog {
    pub(super) id: u64,
    pub(super) state: OpenState,
    pub(super) rounds: BTreeSet<usize>,
}

// A downloaded log reads its stored body and a pending one is fetched; either lands Ready,
// or Failed with the panel's line for why.
pub(super) enum OpenState {
    Loading,
    Ready(Box<LogDetail>),
    Failed(&'static str),
}

/// The logs.tf page. It reads the tracked players from the cache on creation and is kept
/// while hidden, so a visit elsewhere leaves it as it was.
pub struct LogsTfPage {
    players: Vec<TrackedPlayer>,
    active: Option<SteamId>,
    loaded: bool,
    rows: Vec<LogRow>,
    averages: Option<[ClassAverage; 9]>,
    // The class whose card is ringed, and the weapons the panel shows with the class they were
    // read for; a pick moves the ring at once but the panel keeps its pair until the read lands.
    selected_class: Option<TfClass>,
    weapons: Option<(TfClass, ClassWeapons)>,
    weapons_generation: u64,
    // Set while the weapon backfill runs, so a second start is a no-op.
    weapons_counting: bool,
    // The history range as the settings keep it, and the slider that picks it. `slider_range`
    // is the stop last pushed into the slider, so a drag is never snapped back mid-way.
    range: LogsRange,
    range_slider: Entity<SliderState>,
    slider_range: LogsRange,
    // Range picks not yet saved; a cache read meanwhile may still hold the old range. Reads
    // dropped for that are counted, so a test can see the guard fire.
    range_writes: u32,
    stale_reads: u32,
    // The open log and how far its body got; the list's item 0 is the page top, so row `i`
    // is item `i + 1`. A read or fetch that lands after its row closed is dropped.
    open: Option<OpenLog>,
    // The players table's order, kept across logs while the page lives.
    player_sort: PlayerSort,
    // The kill matrix tab, kept the same way.
    matrix: MatrixKind,
    list: ListState,
    // Which optional row columns fit, from the list's width at the last frame; the list's
    // state cannot be asked while it lays its own items out.
    fit: list::Fit,
    // The app-wide sync the page starts and watches; it keeps running while the page hides.
    sync: Entity<LogsSync>,
    // At most one reread follows the sync at a time; events meanwhile ask for one more.
    rereading: bool,
    reread_wanted: bool,
    // Players whose removal waits on their sync; nothing syncs them again meanwhile.
    removing: HashSet<SteamId>,
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
    names: Entity<LoadedWeaponNames>,
    icons: Entity<LoadedKillIcons>,
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
        // The active player's sync moving rereads the cache, so logs appear as they land.
        let sync = logs_sync(cx);
        let heard = cx.subscribe(&sync, |page, _, event: &LogsSyncEvent, cx| {
            let (LogsSyncEvent::Progress(id) | LogsSyncEvent::Finished(id)) = *event;
            if page.active == Some(id) {
                page.reread(cx);
            }
        });
        let redraw = cx.observe(&sync, |_, _, cx| cx.notify());
        // The slider commits on release only, so a drag across stops syncs once.
        let range_slider = cx.new(|_| {
            SliderState::new()
                .min(0.)
                .max((LogsRange::STOPS.len() - 1) as f32)
                .step(1.)
                .default_value(stop_index(LogsRange::default()) as f32)
        });
        let released = cx.subscribe_in(
            &range_slider,
            window,
            |page, _, event: &SliderEvent, window, cx| {
                if let SliderEvent::Release(value) = event {
                    page.set_range(stop_at(value.start()), window, cx);
                }
            },
        );
        // The game's weapon names landing re-renders the panel and any open class card.
        let names = LoadedWeaponNames::global(cx);
        let named = cx.observe(&names, |_, _, cx| cx.notify());
        // So do the game's kill icons.
        let icons = LoadedKillIcons::global(cx);
        let iconed = cx.observe(&icons, |_, _, cx| cx.notify());
        let mut page = Self {
            names,
            icons,
            players: Vec::new(),
            active: None,
            loaded: false,
            rows: Vec::new(),
            averages: None,
            selected_class: None,
            weapons: None,
            weapons_generation: 0,
            weapons_counting: false,
            range: LogsRange::default(),
            range_slider,
            slider_range: LogsRange::default(),
            range_writes: 0,
            stale_reads: 0,
            open: None,
            player_sort: PlayerSort::default(),
            matrix: MatrixKind::Kills,
            list: ListState::new(1, ListAlignment::Top, px(200.)),
            fit: list::Fit::ALL,
            sync,
            rereading: false,
            reread_wanted: false,
            removing: HashSet::new(),
            load_generation: 0,
            sync_wanted: false,
            add_input,
            add_error: None,
            adding: false,
            add_open: false,
            _subscriptions: vec![enter, heard, redraw, released, named, iconed],
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
    pub fn is_syncing(&self, cx: &App) -> bool {
        self.sync.read(cx).busy()
    }

    /// The tracked players, in the order they were added.
    pub fn players(&self) -> &[TrackedPlayer] {
        &self.players
    }

    /// The active player's logs in their window, newest first, from the cache.
    pub fn rows(&self) -> &[LogRow] {
        &self.rows
    }

    /// The active player's class averages; None before the first load.
    pub fn averages(&self) -> Option<&[ClassAverage; 9]> {
        self.averages.as_ref()
    }

    /// The class whose card is selected, if any.
    pub fn selected_class(&self) -> Option<TfClass> {
        self.selected_class
    }

    /// The weapons the panel holds and the class they were read for.
    pub fn class_weapons(&self) -> Option<(TfClass, &ClassWeapons)> {
        self.weapons
            .as_ref()
            .map(|(class, weapons)| (*class, weapons))
    }

    /// Selects `class` and reads its weapons, keeping the held ones shown until then; on the
    /// selected class it closes. An unplayed class does nothing.
    pub fn pick_class(&mut self, class: TfClass, cx: &mut Context<Self>) {
        if self.selected_class == Some(class) {
            self.close_class(cx);
            return;
        }
        let played = self
            .averages
            .as_ref()
            .is_some_and(|a| a.iter().any(|a| a.class() == class && a.played()));
        if !played {
            return;
        }
        self.selected_class = Some(class);
        self.read_weapons(cx);
        self.list.remeasure_items(0..1);
        cx.notify();
    }

    /// Closes the class weapons panel and clears the selection.
    pub fn close_class(&mut self, cx: &mut Context<Self>) {
        self.selected_class = None;
        self.weapons = None;
        // A read still out must not reopen the panel.
        self.weapons_generation += 1;
        self.list.remeasure_items(0..1);
        cx.notify();
    }

    // Reads the selected class's weapons off the UI thread. The landing applies only while the
    // page still shows the player, range and class it read for and no newer read went out; a
    // failed read keeps the panel as it was.
    fn read_weapons(&mut self, cx: &mut Context<Self>) {
        let (Some(active), Some(class)) = (self.active, self.selected_class) else {
            return;
        };
        let range = self.range;
        self.weapons_generation += 1;
        let generation = self.weapons_generation;
        let reading = run_blocking(cx, move |s| {
            s.logs
                .cache()
                .class_weapons(active, range, class)
                .map_err(|e| OpError::core("Failed to read class weapons", e))
        });
        cx.spawn(async move |page, cx| {
            let read = reading.await;
            let _ = page.update(cx, |page, cx| {
                let current = page.weapons_generation == generation
                    && !stale_read(page.range_writes, page.range, range)
                    && page.active == Some(active)
                    && page.range == range
                    && page.selected_class == Some(class);
                if !current {
                    return;
                }
                match read {
                    Ok(weapons) => {
                        page.weapons = Some((class, weapons));
                        page.list.remeasure_items(0..1);
                        cx.notify();
                    }
                    Err(error) => tracing::warn!(%error, "reading logs.tf class weapons failed"),
                }
            });
        })
        .detach();
    }

    /// Counts weapon stats for logs stored before weapon rows existed, one batch at a time off
    /// the UI thread, rereading the open panel after each batch that counted a log. A no-op
    /// while a backfill runs; it stops when none are left, on an error, or when a batch counts
    /// nothing, and the next run resumes.
    pub fn count_older_weapons(&mut self, cx: &mut Context<Self>) {
        if std::mem::replace(&mut self.weapons_counting, true) {
            return;
        }
        cx.spawn(async move |page, cx| {
            loop {
                let Ok(batch) = page.update(cx, |_, cx| {
                    run_blocking(cx, |s| {
                        s.logs
                            .cache()
                            .count_weapons(WEAPON_BATCH)
                            .map_err(|e| OpError::core("Failed to count logs.tf weapon stats", e))
                    })
                }) else {
                    break;
                };
                match batch.await {
                    Ok(done) => {
                        if done.counted() > 0 {
                            let _ = page.update(cx, |page, cx| page.read_weapons(cx));
                        }
                        if done.left() == 0 || done.counted() == 0 {
                            break;
                        }
                    }
                    Err(error) => {
                        tracing::warn!(%error, "counting logs.tf weapon stats failed");
                        break;
                    }
                }
            }
            let _ = page.update(cx, |page, _| page.weapons_counting = false);
        })
        .detach();
    }

    /// Why the active player's last sync failed, shown under the chips.
    pub fn error(&self, cx: &App) -> Option<&'static str> {
        self.active.and_then(|id| self.sync.read(cx).error(id))
    }

    /// Forces a sync of the active player; a no-op while a sync runs.
    pub fn refresh(&mut self, cx: &mut Context<Self>) {
        if let (false, Some(id)) = (self.is_syncing(cx), self.active) {
            self.sync(id, true, cx);
        }
    }

    /// How far back the list, averages and sync reach, as the settings keep it.
    pub fn range(&self) -> LogsRange {
        self.range
    }

    /// How many cache reads were dropped for seeing a range a pick had not yet replaced.
    pub fn stale_reads_dropped(&self) -> u32 {
        self.stale_reads
    }

    /// The history range slider: stops 0 to 5 in `LogsRange::STOPS` order, step 1.
    pub fn range_slider(&self) -> &Entity<SliderState> {
        &self.range_slider
    }

    /// Picks the history range: the slider moves to it, the settings keep it and the reload
    /// that follows syncs the active player with it. Picking the current range does nothing.
    pub fn set_range(&mut self, range: LogsRange, window: &mut Window, cx: &mut Context<Self>) {
        if range == self.range {
            return;
        }
        self.range = range;
        // A cache read already out holds the old range; it must not land and put it back.
        self.load_generation += 1;
        self.move_slider(window, cx);
        self.range_writes += 1;
        let saving = run_blocking(cx, move |s| {
            s.logs.cache().set_range(range).map_err(|error| {
                tracing::warn!(%error, "saving the logs.tf history range failed");
                OpError::core("Failed to change the history range", SAVE_FAILED)
            })
        });
        cx.spawn_in(window, async move |page, cx| {
            let written = saving.await;
            let _ = page.update_in(cx, |page, window, cx| {
                page.range_writes -= 1;
                page.written(written, window, cx);
            });
        })
        .detach();
    }

    // Puts the slider's thumb on `self.range`'s stop when it was last put elsewhere.
    fn move_slider(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.slider_range == self.range {
            return;
        }
        self.slider_range = self.range;
        let stop = stop_index(self.range) as f32;
        self.range_slider
            .update(cx, |slider, cx| slider.set_value(stop, window, cx));
    }

    /// The page's one scrolling list: the top, then a row per log.
    pub fn list_state(&self) -> &ListState {
        &self.list
    }

    /// The open log's players table order.
    pub fn player_sort(&self) -> PlayerSort {
        self.player_sort
    }

    /// A click on the players table's `column` header: sorts by it, or flips it if it leads.
    pub fn sort_players_by(&mut self, column: PlayerColumn, cx: &mut Context<Self>) {
        self.player_sort = self.player_sort.clicked(column);
        cx.notify();
    }

    /// The open log's kill matrix tab.
    pub fn matrix_kind(&self) -> MatrixKind {
        self.matrix
    }

    /// A click on the kill matrix's `kind` tab.
    pub fn show_matrix(&mut self, kind: MatrixKind, cx: &mut Context<Self>) {
        self.matrix = kind;
        cx.notify();
    }

    /// The log whose row is open, if any.
    pub fn expanded(&self) -> Option<u64> {
        self.open.as_ref().map(|open| open.id)
    }

    /// The game's names for logs.tf weapons, empty until the found TF2's load lands.
    pub fn weapon_names<'a>(&self, cx: &'a App) -> &'a WeaponNames {
        self.names.read(cx).names()
    }

    /// The game's kill icons for logs.tf weapons, empty until the found TF2's load lands.
    pub fn kill_icons<'a>(&self, cx: &'a App) -> &'a KillIconImages {
        self.icons.read(cx).icons()
    }

    /// The open row's log, once its body is read and parsed.
    pub fn detail(&self) -> Option<&LogDetail> {
        match &self.open.as_ref()?.state {
            OpenState::Ready(detail) => Some(detail),
            OpenState::Loading | OpenState::Failed(_) => None,
        }
    }

    /// Opens log `id`'s row, closing any other; on the open row it closes it. A downloaded
    /// log reads its stored body, one still pending is fetched from logs.tf.
    pub fn toggle(&mut self, id: u64, cx: &mut Context<Self>) {
        let closing = self.open.take().map(|open| open.id);
        if closing != Some(id) {
            self.open = Some(OpenLog {
                id,
                state: OpenState::Loading,
                rounds: BTreeSet::new(),
            });
            match self.rows.iter().find(|r| r.id() == id).map(LogRow::state) {
                Some(LogState::Pending) => self.fetch_detail(id, cx),
                _ => self.read_detail(id, cx),
            }
        }
        // Both rows change height, so both remeasure and the rows below move with them.
        for changed in closing.into_iter().chain([id]) {
            self.remeasure_log(changed);
        }
        cx.notify();
    }

    /// The open log's open rounds, ascending.
    pub fn open_rounds(&self) -> Vec<usize> {
        self.open
            .as_ref()
            .map_or_else(Vec::new, |open| open.rounds.iter().copied().collect())
    }

    /// Opens round `round` of the open log, or closes it if open; any number stay open.
    pub fn toggle_round(&mut self, round: usize, cx: &mut Context<Self>) {
        let Some(open) = &mut self.open else {
            return;
        };
        if !open.rounds.remove(&round) {
            open.rounds.insert(round);
        }
        let id = open.id;
        self.remeasure_log(id);
        cx.notify();
    }

    // Remeasures log `id`'s list item, whose panel changed height.
    fn remeasure_log(&mut self, id: u64) {
        if let Some(ix) = self.rows.iter().position(|r| r.id() == id) {
            self.list.remeasure_items(ix + 1..ix + 2);
        }
    }

    // Moves open log `id` to `state`, unless its row closed or another opened meanwhile.
    fn land(&mut self, id: u64, state: OpenState, cx: &mut Context<Self>) -> bool {
        match &mut self.open {
            Some(open) if open.id == id => open.state = state,
            _ => return false,
        }
        self.remeasure_log(id);
        cx.notify();
        true
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
                let state = match read {
                    Ok(detail) => OpenState::Ready(Box::new(detail)),
                    Err(error) => {
                        tracing::warn!(%error, id, "reading a logs.tf log failed");
                        OpenState::Failed(LOG_UNREADABLE)
                    }
                };
                page.land(id, state, cx);
            });
        })
        .detach();
    }

    // Fetches pending log `id` from logs.tf, which also stores it. A fetch the row's note will
    // explain (removed, unreadable) closes the row; any other failure keeps it open with why.
    // Either way the list is reread, since core marked the row.
    fn fetch_detail(&mut self, id: u64, cx: &mut Context<Self>) {
        if !tokio_bridge::is_running(cx) {
            tokio_bridge::init(cx);
        }
        let client = cx.global::<Services>().logs.clone();
        let fetching = tokio_bridge::spawn(cx, async move { client.fetch_log(id).await });
        cx.spawn(async move |page, cx| {
            let fetched = fetching.await;
            let _ = page.update(cx, |page, cx| {
                let state = match fetched {
                    Ok(Ok(detail)) => Some(OpenState::Ready(Box::new(detail))),
                    Ok(Err(error)) => {
                        tracing::warn!(%error, id, "fetching a logs.tf log failed");
                        fetch_message(&error).map(OpenState::Failed)
                    }
                    Err(error) => {
                        tracing::warn!(%error, id, "the logs.tf fetch task stopped");
                        Some(OpenState::Failed(LOGS_TF_DOWN))
                    }
                };
                // Core stored or marked the log unless the failure has a line, so its row is
                // reread even when the result no longer lands.
                let marked = !matches!(state, Some(OpenState::Failed(_)));
                // The sync may have stored the log while this fetch failed; then it reads.
                let stored = page
                    .rows
                    .iter()
                    .any(|r| r.id() == id && r.state() == LogState::Ready);
                match state {
                    Some(OpenState::Failed(_)) if stored => {
                        if page.land(id, OpenState::Loading, cx) {
                            page.read_detail(id, cx);
                        }
                    }
                    Some(state) => {
                        page.land(id, state, cx);
                    }
                    None if page.expanded() == Some(id) => {
                        page.open = None;
                        page.remeasure_log(id);
                        cx.notify();
                    }
                    None => {}
                }
                if marked {
                    page.reread(cx);
                }
            });
        })
        .detach();
    }

    /// Fetches open log `id` again after a failed fetch, showing it as loading meanwhile.
    pub fn retry(&mut self, id: u64, cx: &mut Context<Self>) {
        if self.land(id, OpenState::Loading, cx) {
            self.fetch_detail(id, cx);
        }
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
            s.logs.cache().set_active_player(id).map_err(|error| {
                tracing::warn!(%error, "saving the active logs.tf player failed");
                OpError::core("Failed to switch player", SAVE_FAILED)
            })
        });
        self.after_write(saving, window, cx);
    }

    // Asks before forgetting a player, stops their sync, then drops them with every log no
    // other tracked player shares.
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
            // Their sync ends first, so a list page it was recording cannot outlive the removal.
            let Ok(stopped) = page.update(cx, |page, cx| {
                page.removing.insert(id);
                cx.notify();
                page.sync.update(cx, |sync, cx| sync.stop(id, cx))
            }) else {
                return;
            };
            stopped.await;
            let Ok(removing) = page.update(cx, |_, cx| {
                run_blocking(cx, move |s| {
                    s.logs.cache().remove_player(id).map_err(|error| {
                        tracing::warn!(%error, "removing a logs.tf player failed");
                        OpError::core("Failed to stop tracking", SAVE_FAILED)
                    })
                })
            }) else {
                return;
            };
            let removed = removing.await;
            let _ = page.update_in(cx, |page, window, cx| {
                page.removing.remove(&id);
                page.written(removed, window, cx);
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
                                    .label(if this.adding { LOOKING_UP } else { "Add" })
                                    .loading(this.adding)
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

    // Opens the open log's chat over the page. The rows are shaped once, since the log they
    // come from never changes while the dialog is up.
    fn open_chat(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(rows) = self.detail().map(tables::chat_rows) else {
            return;
        };
        open_detail_dialog(window, cx, move |window, cx| {
            detail::footer::chat_dialog(&rows, window, cx)
        });
    }

    // Opens the open log's killstreaks over the page, shaped once as the chat is.
    fn open_streaks(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(rows) = self.detail().map(detail::footer::streak_rows) else {
            return;
        };
        open_detail_dialog(window, cx, move |window, cx| {
            detail::footer::streaks_dialog(&rows, window, cx)
        });
    }

    // Asks where to save log `id`'s raw zip, downloads it from logs.tf, then writes it off the
    // UI thread. A cancelled dialog ends it; any failure toasts why and leaves no file.
    fn download_log(&mut self, id: u64, window: &mut Window, cx: &mut Context<Self>) {
        let picking = cx.global::<Services>().picker.save_to(
            PickKind::LogZip,
            &format!("log_{id}.log.zip"),
            cx,
        );
        cx.spawn_in(window, async move |page, cx| {
            let Some(path) = picking.await else {
                return;
            };
            let downloading = page.update(cx, |_, cx| {
                if !tokio_bridge::is_running(cx) {
                    tokio_bridge::init(cx);
                }
                let client = cx.global::<Services>().logs.clone();
                tokio_bridge::spawn(cx, async move { client.download_zip(id).await })
            });
            let Ok(downloading) = downloading else {
                return;
            };
            let saved = match downloading.await {
                Ok(Ok(bytes)) => {
                    let target = path.clone();
                    let writing = page.update(cx, |_, cx| {
                        run_blocking(cx, move |_| Ok(write_replacing(&target, &bytes)))
                    });
                    let Ok(writing) = writing else {
                        return;
                    };
                    match writing.await {
                        Ok(Ok(())) => Ok(()),
                        Ok(Err(error)) => {
                            tracing::warn!(%error, id, "writing a logs.tf zip failed");
                            Err(WRITE_FAILED)
                        }
                        Err(error) => {
                            tracing::warn!(%error, id, "the logs.tf zip write stopped");
                            Err(WRITE_FAILED)
                        }
                    }
                }
                Ok(Err(error)) => {
                    tracing::warn!(%error, id, "downloading a logs.tf zip failed");
                    Err(download_reason(&error))
                }
                Err(error) => {
                    tracing::warn!(%error, id, "the logs.tf zip download stopped");
                    Err(LOGS_TF_DOWN)
                }
            };
            let _ = page.update_in(cx, |_, window, cx| match saved {
                Ok(()) => {
                    let name = path.file_name().unwrap_or_default().to_string_lossy();
                    toast(
                        window,
                        cx,
                        ToastKind::Success,
                        DOWNLOADED,
                        name.into_owned(),
                    );
                }
                Err(why) => toast(window, cx, ToastKind::Error, DOWNLOAD_FAILED, why),
            });
        })
        .detach();
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

    // Awaits a chip write, then ends it with `written`.
    fn after_write(
        &mut self,
        write: impl Future<Output = Result<(), OpError>> + 'static,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        cx.spawn_in(window, async move |page, cx| {
            let written = write.await;
            let _ = page.update_in(cx, |page, window, cx| page.written(written, window, cx));
        })
        .detach();
    }

    // Every chip write ends the same way: a failure toasts, then the page rereads the cache,
    // which is the truth either way.
    fn written(
        &mut self,
        written: Result<(), OpError>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Err(err) = written {
            toast_error(window, cx, &err);
        }
        self.load(true, cx).detach();
    }

    // Reads the players, the active chip and that player's logs and averages off the UI
    // thread. With `sync`, the landing also syncs the active player, in the same update that
    // sets `loaded`. A failed read leaves the page as it was; the warning keeps the cause.
    fn load(&mut self, sync: bool, cx: &mut Context<Self>) -> Task<()> {
        self.sync_wanted |= sync;
        self.load_generation += 1;
        let generation = self.load_generation;
        let selected = self.selected_class;
        let loading = run_blocking(cx, move |s| {
            let cache = s.logs.cache();
            let read = |e| OpError::core("Failed to load logs.tf players", e);
            let players = cache.players().map_err(read)?;
            let active = cache.active_player().map_err(read)?;
            let range = cache.range().map_err(read)?;
            let logs = match active {
                Some(id) => Some((
                    cache.logs(id, range).map_err(read)?,
                    cache.class_averages(id, range).map_err(read)?,
                )),
                None => None,
            };
            // A failed weapon read costs only the panel, which keeps its last rows; the rest
            // of the load still lands.
            let weapons = match (active, selected) {
                (Some(id), Some(class)) => match cache.class_weapons(id, range, class) {
                    Ok(weapons) => Some((class, weapons)),
                    Err(error) => {
                        tracing::warn!(%error, "reading the logs.tf class weapons failed");
                        None
                    }
                },
                _ => None,
            };
            Ok((players, active, range, logs, weapons))
        });
        cx.spawn(async move |page, cx| {
            let loaded = loading.await;
            let _ = page.update(cx, |page, cx| {
                // A read that ran before a range pick was saved saw the old range; the pick's
                // own reread follows, so this one is dropped like a superseded one.
                let before_pick = matches!(&loaded, Ok((_, _, range, _, _))
                    if stale_read(page.range_writes, page.range, *range));
                page.stale_reads += u32::from(before_pick);
                if page.load_generation != generation || before_pick {
                    return;
                }
                match loaded {
                    Ok((players, active, range, logs, weapons)) => {
                        page.players = players;
                        page.active = active;
                        page.range = range;
                        let (rows, averages) = logs.unzip();
                        let old = std::mem::replace(&mut page.rows, rows.unwrap_or_default());
                        page.averages = averages;
                        page.carry_class(weapons, cx);
                        page.fit_list(&old);
                        page.read_downloaded(&old, cx);
                    }
                    Err(error) => tracing::warn!(%error, "reading the logs.tf cache failed"),
                }
                // The first landing starts counting logs an earlier build stored.
                if !std::mem::replace(&mut page.loaded, true) {
                    page.count_older_weapons(cx);
                }
                if std::mem::take(&mut page.sync_wanted)
                    && let Some(id) = page.active
                {
                    page.sync(id, false, cx);
                }
                cx.notify();
            });
        })
    }

    // Keeps the panel in step with fresh averages: it closes when the active player never
    // played the selected class, else takes the load's weapons if they were read for the
    // class still selected. The bump drops a click read that went out before this one. A
    // class picked while the load read gets its own read, since its click read saw the old
    // player or range and is dropped.
    fn carry_class(&mut self, weapons: Option<(TfClass, ClassWeapons)>, cx: &mut Context<Self>) {
        let Some(class) = self.selected_class else {
            return;
        };
        let played = self
            .averages
            .as_ref()
            .is_some_and(|a| a.iter().any(|a| a.class() == class && a.played()));
        if !played {
            self.selected_class = None;
            self.weapons = None;
            self.weapons_generation += 1;
            return;
        }
        if let Some((read, weapons)) = weapons
            && read == class
        {
            self.weapons = Some((read, weapons));
            self.weapons_generation += 1;
        } else {
            self.read_weapons(cx);
        }
    }

    // Matches the list to fresh rows that replaced `old`: added or dropped rows splice in
    // after the top, and every item remeasures since a row's state or the top's height may
    // have changed. An open row that is gone from the list, or that logs.tf removed or sent
    // unreadable, closes.
    fn fit_list(&mut self, old: &[LogRow]) {
        let items = self.rows.len() + 1;
        let count = self.list.item_count();
        if count != items {
            // A splice over the scroll top moves it to the first row, so the page keeps the
            // log it showed at the top where it was, wherever that log now sits.
            let top = self.list.logical_scroll_top();
            let shown = top.item_ix.checked_sub(1).and_then(|ix| old.get(ix));
            self.list.splice(1..count, self.rows.len());
            if let Some(ix) = shown.and_then(|r| self.rows.iter().position(|n| n.id() == r.id())) {
                self.list.scroll_to(ListOffset {
                    item_ix: ix + 1,
                    offset_in_item: top.offset_in_item,
                });
            }
        }
        self.list.remeasure_items(0..items);
        let opens = |r: &LogRow| matches!(r.state(), LogState::Ready | LogState::Pending);
        let still_open = |r: &LogRow| Some(r.id()) == self.expanded() && opens(r);
        if !self.rows.iter().any(still_open) {
            self.open = None;
        }
    }

    // An open log whose fetch failed reads its stored body once the sync has downloaded it,
    // that is when it was pending in `old` and is ready now.
    fn read_downloaded(&mut self, old: &[LogRow], cx: &mut Context<Self>) {
        let Some(OpenLog {
            id,
            state: OpenState::Failed(_),
            ..
        }) = self.open
        else {
            return;
        };
        let was = |rows: &[LogRow], state| rows.iter().any(|r| r.id() == id && r.state() == state);
        let downloaded = was(old, LogState::Pending) && was(&self.rows, LogState::Ready);
        if downloaded && self.land(id, OpenState::Loading, cx) {
            self.read_detail(id, cx);
        }
    }

    // Asks the app-wide sync for `id`; a run for anyone else is cancelled in its favour.
    fn sync(&mut self, id: SteamId, force: bool, cx: &mut Context<Self>) {
        if self.removing.contains(&id) {
            return;
        }
        self.sync
            .update(cx, |sync, cx| sync.start(id, self.range, force, cx));
    }

    // Each reread lands before the next starts, so a burst of events costs one more read.
    fn reread(&mut self, cx: &mut Context<Self>) {
        if self.rereading {
            self.reread_wanted = true;
            return;
        }
        self.rereading = true;
        let reading = self.load(false, cx);
        cx.spawn(async move |page, cx| {
            reading.await;
            let _ = page.update(cx, |page, cx| {
                page.rereading = false;
                if std::mem::take(&mut page.reread_wanted) {
                    page.reread(cx);
                }
            });
        })
        .detach();
    }

    // Refresh and the active player's logs.tf profile, beside the chips.
    fn actions(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let syncing = self.is_syncing(cx);
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
        let sync = self.sync.read(cx);
        let ours = self.active.is_some() && sync.running() == self.active;
        // A count of 0 of 0 says nothing, so the line waits for the list to arrive.
        let progress = sync.progress().filter(|progress| {
            let (SyncProgress::Finding { total, .. } | SyncProgress::Downloading { total, .. }) =
                *progress;
            ours && total > 0
        });
        let busy = sync.busy();
        let progress = progress.map(|progress| {
            labelled(
                div().text_sm().text_color(cx.theme().muted_foreground),
                ids::LOGSTF_PROGRESS,
                progress_line(progress),
            )
        });
        let error = self.error(cx).filter(|_| !busy).map(|error| {
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
            // A chip being removed says so and takes no clicks until it goes.
            let removing = self.removing.contains(&id);
            let chip = filter_badge(
                ids::logstf_chip(id),
                self.active == Some(id),
                if removing { REMOVING } else { name.as_str() }.to_owned(),
                window,
                cx,
            )
            .when(!removing, |chip| {
                chip.on_click(cx.listener(move |page, _, window, cx| page.select(id, window, cx)))
            });
            let remove = Button::new(ids::logstf_chip_remove(id))
                .ghost()
                .xsmall()
                .icon(IconName::X)
                .named(format!("Stop tracking {name}"))
                .disabled(removing)
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
        // Only a finished sync can say a player has no logs: one still running or waiting
        // its turn may bring some, and a player never synced shows the error line instead.
        let sync = self.sync.read(cx);
        let fetching = self.active.is_some()
            && (sync.running() == self.active || sync.queued() == self.active);
        let synced = |id: &SteamId| {
            self.players
                .iter()
                .any(|p| p.steam_id() == *id && p.last_synced().is_some())
        };
        let no_logs = self
            .active
            .filter(|id| self.loaded && !fetching && self.rows.is_empty() && synced(id));
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
                    .map(|averages| {
                        grid::class_grid(
                            averages,
                            self.rows.len(),
                            self.rows.first().map(LogRow::date),
                            self.range,
                            &self.range_slider,
                            cx.entity().downgrade(),
                            self.selected_class,
                            cx,
                        )
                    }),
            )
            .children(self.weapons_panel(cx))
            .children(empty.then(|| self.empty_state(cx)))
            .children(no_logs.map(|id| list::no_logs(id, cx)))
            .children((!self.rows.is_empty()).then(|| list::column_header(self.fit, cx)))
    }

    // The panel for the held weapons, while a class is selected.
    fn weapons_panel(&self, cx: &mut Context<Self>) -> Option<impl IntoElement + use<>> {
        self.selected_class?;
        let (class, weapons) = self.weapons.as_ref()?;
        let page = cx.entity().downgrade();
        Some(class_weapons::class_weapons_panel(
            *class,
            weapons,
            self.weapon_names(cx),
            self.kill_icons(cx),
            self.range,
            page,
            cx,
        ))
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
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.fit = list::fit(&self.list);
        // A cache read has no window to move the slider with, so the frame after it does.
        self.move_slider(window, cx);
        // The page scrolls as one list, so the grid and the logs move together.
        v_flex()
            .id(ids::page(Page::LogsTf))
            .test_support()
            .size_full()
            .p(CONTENT_PADDING)
            .child(list::page_list(self, cx).flex_1().min_h_0())
    }
}

// A read-only dialog of the log's detail width, deferred as the add dialog is so the click
// that opened it cannot pull focus back out.
fn open_detail_dialog<E: IntoElement>(
    window: &mut Window,
    cx: &mut Context<LogsTfPage>,
    content: impl Fn(&Window, &App) -> E + 'static,
) {
    window.defer(cx, move |window, cx| {
        let frame = DialogFrame::new(DETAIL_WIDTH);
        window.open_dialog(cx, move |dialog, window, cx| {
            frame
                .apply(dialog, &*window, &*cx)
                .child(frame.measure(content(&*window, &*cx)))
        });
    });
}

// A cache read that saw `read_range` while `pending_picks` range picks wait to be saved ran
// before the newest pick, `picked`, landed; the pick's own reread follows it.
fn stale_read(pending_picks: u32, picked: LogsRange, read_range: LogsRange) -> bool {
    pending_picks > 0 && read_range != picked
}

// A range's place on the slider, narrow to wide.
fn stop_index(range: LogsRange) -> usize {
    LogsRange::STOPS
        .iter()
        .position(|stop| *stop == range)
        .unwrap_or_default()
}

// The range at a slider value, rounded to the nearest stop; a NaN reads as the first.
fn stop_at(value: f32) -> LogsRange {
    let ix = if value.is_nan() {
        0
    } else {
        value.round().clamp(0., (LogsRange::STOPS.len() - 1) as f32) as usize
    };
    LogsRange::STOPS[ix]
}

// Writes `bytes` to a temporary file beside `path`, then renames it over `path`, so a failed
// write never leaves a partial file where the player asked for one.
fn write_replacing(path: &std::path::Path, bytes: &[u8]) -> std::io::Result<()> {
    use std::io::Write as _;
    let dir = path.parent().filter(|d| !d.as_os_str().is_empty());
    let mut part = tempfile::NamedTempFile::new_in(dir.unwrap_or(std::path::Path::new(".")))?;
    part.write_all(bytes)?;
    part.as_file().sync_all()?;
    part.persist(path).map_err(|e| e.error)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_read_is_stale_only_while_a_pick_is_unsaved_and_it_saw_another_range() {
        use LogsRange::{Months3, Year1};
        assert!(
            stale_read(1, Year1, Months3),
            "it saw the range the pick replaced"
        );
        assert!(stale_read(2, Year1, Months3), "two picks out");
        assert!(!stale_read(1, Year1, Year1), "it already saw the pick");
        assert!(
            !stale_read(0, Year1, Months3),
            "no pick out: the cache is the truth"
        );
        assert!(!stale_read(0, Year1, Year1));
    }
}
