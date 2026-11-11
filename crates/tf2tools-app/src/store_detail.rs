//! A store item's detail dialog: its name and library badge, the downloadable files, a
//! screenshot carousel for kinds that have screenshots, and a per-store body. Every store
//! page opens items through it.

use std::rc::Rc;

use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::progress::Progress as ProgressBar;
use gpui_kit::component::tag::Tag;
use gpui_kit::component::{ActiveTheme as _, Disableable as _, WindowExt as _, h_flex, v_flex};
use gpui_kit::{
    AnyElement, AnyWindowHandle, App, AppContext as _, Context, Entity, FontWeight,
    InteractiveElement as _, IntoElement, ParentElement as _, Pixels, Render, ScrollHandle,
    SharedString, StatefulInteractiveElement as _, Styled as _, Subscription, Task,
    TestSupportExt as _, Window, div, px, transparent_black,
};
use tf2tools_core::{StoreFile, StoreItem, StoreItemId, StoreKind};

use crate::ids;
use crate::pages::common::badge_of;
use crate::services::{OpError, Services, run_blocking};
use crate::shell::toast::{ToastKind, toast};
use crate::store_download::{
    DownloadEvent, DownloadTarget, StoreDownloads, format_bytes, progress_label, progress_percent,
};
use crate::store_messages::{DETAILS_FAILED_TITLE, describe_files, listing_stopped, read_failed};
use crate::tokio_bridge;
use crate::widgets::carousel::Carousel;
use crate::widgets::dialog_frame::{DETAIL_WIDTH, DIALOG_WIDTH, DialogFrame, content_max_height};
use crate::widgets::labelled;
use crate::widgets::scroll::scroll_beside;

// A failed listing's height: one 22.5 px line of message, the 8 px gap and the 32 px Retry.
const FILES_STATUS_HEIGHT: Pixels = px(62.5);

/// The item a detail dialog is about, as the card that opened it knows it.
#[derive(Clone, Debug, PartialEq)]
pub struct DetailItem {
    pub kind: StoreKind,
    pub id: StoreItemId,
    pub name: String,
    pub in_library: bool,
}

type BodyFn = Rc<dyn Fn(&StoreItem, &mut Window, &mut App) -> AnyElement>;
type ActionFn = Rc<dyn Fn(&StoreItem, &mut Window, &mut App) -> Option<AnyElement>>;

/// What a store kind adds to the dialog: the body under the files (and the carousel, if any),
/// and an optional action right of the name block, as v5's Repository and GameBanana buttons.
#[derive(Clone)]
pub struct DetailBody {
    body: BodyFn,
    action: Option<ActionFn>,
}

impl DetailBody {
    /// A kind's body with no header action.
    pub fn new(body: impl Fn(&StoreItem, &mut Window, &mut App) -> AnyElement + 'static) -> Self {
        Self {
            body: Rc::new(body),
            action: None,
        }
    }

    /// Adds the header action; `None` from it draws nothing.
    pub fn with_action(
        self,
        action: impl Fn(&StoreItem, &mut Window, &mut App) -> Option<AnyElement> + 'static,
    ) -> Self {
        Self {
            action: Some(Rc::new(action)),
            ..self
        }
    }

    /// Draws the body for the store row `item`.
    pub fn body(&self, item: &StoreItem, window: &mut Window, cx: &mut App) -> AnyElement {
        (self.body)(item, window, cx)
    }

    /// Draws the header action for `item`, if the kind has one for it.
    pub fn action(
        &self,
        item: &StoreItem,
        window: &mut Window,
        cx: &mut App,
    ) -> Option<AnyElement> {
        self.action.as_ref().and_then(|a| a(item, window, cx))
    }
}

/// Where the item's file listing stands.
pub enum FilesState {
    Loading,
    Ready(Vec<StoreFile>),
    Failed(String),
}

/// The dialog's view: it reads the cached store row and lists the files on open.
pub struct StoreDetail {
    item: DetailItem,
    body: DetailBody,
    store_item: Option<StoreItem>,
    files: FilesState,
    carousel: Entity<Carousel>,
    // The scroll of everything under the header.
    scroll: ScrollHandle,
    win: AnyWindowHandle,
    _item_task: Task<()>,
    files_task: Option<Task<()>>,
    _subscriptions: Vec<Subscription>,
}

/// Whether `kind`'s items have screenshots; the detail shows its carousel only for these.
pub fn has_screenshots(kind: StoreKind) -> bool {
    matches!(
        kind,
        StoreKind::CuratedHuds | StoreKind::GameBananaHuds | StoreKind::Crosshairs
    )
}

impl StoreDetail {
    /// Starts reading the store row (for the carousel and body) and listing the files; toasts land in `win`.
    pub fn new(
        item: DetailItem,
        body: DetailBody,
        win: AnyWindowHandle,
        cx: &mut Context<Self>,
    ) -> Self {
        let carousel = cx.new(|cx| Carousel::new(Vec::new(), cx));
        let mut subscriptions = Vec::new();
        // Redraw on progress, and mark the item owned when its own download finishes.
        if let Some(downloads) = StoreDownloads::global(cx) {
            subscriptions.push(cx.observe(&downloads, |_, _, cx| cx.notify()));
            subscriptions.push(cx.subscribe(&downloads, |this, _, event, cx| {
                if let DownloadEvent::Finished { target } = event
                    && target.kind == this.item.kind
                    && target.item == this.item.id
                {
                    this.item.in_library = true;
                    cx.notify();
                }
            }));
        }
        let (kind, id) = (item.kind, item.id.clone());
        let read = run_blocking(cx, move |s| {
            s.store
                .store_item(kind, &id)
                .map_err(|e| OpError::core("Failed to read the store", e))
        });
        // A missing row silently leaves the carousel and body empty; an unreadable one also toasts.
        let item_task = cx.spawn(async move |this, cx| match read.await {
            Ok(Some(row)) => {
                let _ = this.update(cx, |d, cx| {
                    let urls = row.images().to_vec();
                    d.carousel.update(cx, |c, cx| c.set_urls(urls, cx));
                    d.store_item = Some(row);
                    cx.notify();
                });
            }
            Ok(None) => {}
            Err(err) => {
                let _ = this.update(cx, |d, cx| {
                    let description = read_failed(&err);
                    let _ = d.win.update(cx, |_, window, cx| {
                        toast(
                            window,
                            cx,
                            ToastKind::Error,
                            DETAILS_FAILED_TITLE,
                            description,
                        )
                    });
                });
            }
        });
        let mut detail = Self {
            item,
            body,
            store_item: None,
            files: FilesState::Loading,
            carousel,
            scroll: ScrollHandle::new(),
            win,
            _item_task: item_task,
            files_task: None,
            _subscriptions: subscriptions,
        };
        detail.load_files(cx);
        detail
    }

    pub fn item(&self) -> &DetailItem {
        &self.item
    }

    /// The cached store row, once read.
    pub fn store_item(&self) -> Option<&StoreItem> {
        self.store_item.as_ref()
    }

    pub fn files(&self) -> &FilesState {
        &self.files
    }

    pub fn carousel(&self) -> &Entity<Carousel> {
        &self.carousel
    }

    /// Lists the files again after a failure.
    pub fn retry_files(&mut self, cx: &mut Context<Self>) {
        self.load_files(cx);
    }

    /// Starts file `index` through `StoreDownloads`; false when busy or absent.
    pub fn download(&mut self, index: usize, cx: &mut Context<Self>) -> bool {
        let FilesState::Ready(files) = &self.files else {
            return false;
        };
        let Some(file) = files.get(index) else {
            return false;
        };
        let Some(downloads) = StoreDownloads::global(cx) else {
            return false;
        };
        let target = DownloadTarget {
            kind: self.item.kind,
            item: self.item.id.clone(),
            name: self.item.name.clone(),
        };
        let (file, win) = (file.clone(), self.win);
        downloads.update(cx, |d, cx| d.start(target, file, win, cx))
    }

    // The listing goes over the network, so it runs on the tokio bridge, started on demand.
    fn load_files(&mut self, cx: &mut Context<Self>) {
        self.files = FilesState::Loading;
        if !tokio_bridge::is_running(cx) {
            tokio_bridge::init(cx);
        }
        let store = cx.global::<Services>().store.clone();
        let (kind, id) = (self.item.kind, self.item.id.clone());
        let job = tokio_bridge::spawn(cx, async move { store.item_files(kind, &id).await });
        self.files_task = Some(cx.spawn(async move |this, cx| {
            // The player reads the plain message; the raw error only reaches the developer log.
            let result = match job.await {
                Ok(Ok(files)) => Ok(files),
                Ok(Err(err)) => Err(describe_files(kind, &err)),
                Err(_) => Err(listing_stopped(
                    "the file listing task stopped unexpectedly".to_owned(),
                )),
            }
            .map_err(|failure| {
                failure.log("store file listing");
                failure.message
            });
            let _ = this.update(cx, |d, cx| d.files_loaded(result, cx));
        }));
        cx.notify();
    }

    fn files_loaded(&mut self, result: Result<Vec<StoreFile>, String>, cx: &mut Context<Self>) {
        self.files_task = None;
        // A failure shows only inline with Retry: a toast would repeat it, or speak for a closed dialog.
        self.files = match result {
            Ok(files) => FilesState::Ready(files),
            Err(message) => FilesState::Failed(message),
        };
        cx.notify();
    }

    // One file row: its name and size, and its own Download button.
    fn file_row(
        &self,
        index: usize,
        file: &StoreFile,
        busy: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let muted = cx.theme().muted_foreground;
        let curated = self.item.kind == StoreKind::CuratedHuds;
        // Curated re-downloads replace the HUD in place; a GameBanana item is owned once.
        let (label, owned_gb) = match (self.item.in_library, curated) {
            (true, true) => ("Re-download", false),
            (true, false) => ("In Library", true),
            _ => ("Download", false),
        };
        let button = Button::new(ids::store_file(index))
            .label(label)
            .disabled(busy || owned_gb)
            .on_click(cx.listener(move |this, _, _, cx| {
                this.download(index, cx);
            }));
        let button = if self.item.in_library {
            button.secondary()
        } else {
            button.primary()
        };
        h_flex()
            .w_full()
            .gap_3()
            .justify_between()
            .child(
                v_flex()
                    .min_w_0()
                    .child(div().text_sm().child(file.file_name().to_owned()))
                    .children(
                        file.size().map(|size| {
                            div().text_xs().text_color(muted).child(format_bytes(size))
                        }),
                    ),
            )
            .child(button)
            .into_any_element()
    }

    fn files_section(&self, busy: bool, cx: &mut Context<Self>) -> AnyElement {
        let muted = cx.theme().muted_foreground;
        match &self.files {
            // Held at the failure's height, one line and the Retry button, so the dialog keeps still.
            FilesState::Loading => div()
                .min_h(FILES_STATUS_HEIGHT)
                .text_sm()
                .text_color(muted)
                .child("Loading files...")
                .into_any_element(),
            FilesState::Failed(message) => v_flex()
                .gap_2()
                .items_start()
                .child(labelled(
                    div().text_sm().text_color(cx.theme().danger),
                    ids::STORE_FILES_ERROR,
                    message.clone(),
                ))
                .child(
                    Button::new(ids::STORE_FILES_RETRY)
                        .outline()
                        .label("Retry")
                        .on_click(cx.listener(|this, _, _, cx| this.retry_files(cx))),
                )
                .into_any_element(),
            FilesState::Ready(files) if files.is_empty() => div()
                .text_sm()
                .text_color(muted)
                .child("No files to download")
                .into_any_element(),
            FilesState::Ready(files) => v_flex()
                .gap_2()
                .children(
                    files
                        .iter()
                        .enumerate()
                        .map(|(i, file)| self.file_row(i, file, busy, cx)),
                )
                .into_any_element(),
        }
    }
}

impl Render for StoreDetail {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let (mine, busy, progress) = match StoreDownloads::global(cx) {
            Some(d) => {
                let d = d.read(cx);
                (
                    d.is_downloading(self.item.kind, &self.item.id),
                    d.is_busy(),
                    d.progress(),
                )
            }
            None => (false, false, None),
        };
        let badge_text = if self.item.kind == StoreKind::CuratedHuds {
            "Downloaded"
        } else {
            "In Library"
        };
        let badge = self.item.in_library.then(|| {
            badge_of(
                ids::STORE_DETAIL_BADGE,
                Tag::secondary(),
                transparent_black(),
                badge_text,
            )
        });
        let author = self
            .store_item
            .as_ref()
            .map(|row| row.author().to_owned())
            .filter(|a| !a.is_empty())
            .map(|a| {
                div()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child(format!("by {a}"))
            });
        let progress_el = mine.then(|| {
            let label = SharedString::from(progress_label(progress));
            v_flex()
                .id(ids::STORE_DETAIL_PROGRESS)
                .aria_label(label.clone())
                .test_support()
                .gap_1()
                .child(div().text_sm().child(label))
                .child(ProgressBar::new(ids::STORE_DETAIL_BAR).value(progress_percent(progress)))
        });
        let (action, body) = match self.store_item.as_ref() {
            Some(row) => (
                self.body.action(row, window, cx),
                Some(self.body.body(row, window, cx)),
            ),
            None => (None, None),
        };
        // The files sit right under the title so they stay on screen above a tall carousel.
        // v5 caps the dialog at 90% of the window (`max-h-[90vh]`) and scrolls its body; here
        // the header stays, so the bar in the box padding starts below the close X.
        let scrolled = v_flex()
            .id(ids::STORE_DETAIL_BODY)
            .test_support()
            .min_h_0()
            .track_scroll(&self.scroll)
            .overflow_y_scroll()
            .gap_4()
            .children(progress_el)
            .child(self.files_section(busy, cx))
            // A 16:9 box has no content height to defend, so a short window would squash it.
            .children(
                has_screenshots(self.item.kind)
                    .then(|| div().flex_shrink_0().child(self.carousel.clone())),
            )
            .children(body);
        v_flex()
            .id(ids::STORE_DETAIL)
            .test_support()
            .w_full()
            .max_h(content_max_height(window, cx))
            .gap_4()
            // v5's header: the badge rides 8 px after the name, the author on the line below,
            // and the kind's action at the top right beside that block. v5 draws the header
            // under the image; here the kit's 24 px close X floats 14 px into this corner, so
            // the row keeps 22 px clear and the action ends 8 px left of the X.
            .child(
                h_flex()
                    .items_start()
                    .justify_between()
                    .gap_3()
                    .pr(px(22.))
                    .child(
                        v_flex()
                            .min_w_0()
                            .child(
                                // The name may shrink and wrap, so a long one stays clear
                                // of the header button and the close X.
                                h_flex()
                                    .min_w_0()
                                    .items_center()
                                    .gap_2()
                                    .child(labelled(
                                        div()
                                            .min_w_0()
                                            .text_xl()
                                            .line_height(px(28.))
                                            .font_weight(FontWeight::SEMIBOLD),
                                        ids::STORE_DETAIL_NAME,
                                        self.item.name.clone(),
                                    ))
                                    .children(badge),
                            )
                            .children(author),
                    )
                    .children(action),
            )
            .child(scroll_beside(ids::STORE_DETAIL_SCROLLBAR, &self.scroll, scrolled).min_h_0())
    }
}

/// v5's detail width: `max-w-lg` for sounds, `max-w-2xl` for the rest.
pub fn detail_width(kind: StoreKind) -> Pixels {
    match kind {
        StoreKind::Sounds => DIALOG_WIDTH,
        StoreKind::CuratedHuds
        | StoreKind::GameBananaHuds
        | StoreKind::Crosshairs
        | StoreKind::Addons
        | _ => DETAIL_WIDTH,
    }
}

/// Opens the dialog (`ids::STORE_DETAIL`) and returns its view.
pub fn open_detail(
    item: DetailItem,
    body: DetailBody,
    window: &mut Window,
    cx: &mut App,
) -> Entity<StoreDetail> {
    let win = window.window_handle();
    let item_kind = item.kind;
    let detail = cx.new(|cx| StoreDetail::new(item, body, win, cx));
    let view = detail.clone();
    let frame = DialogFrame::new(detail_width(item_kind));
    // Deferred, as confirm does, so the click that opened it cannot pull focus back out.
    window.defer(cx, move |window, cx| {
        window.open_dialog(cx, move |dialog, window, cx| {
            frame
                .apply(dialog, &*window, &*cx)
                .child(frame.measure(view.clone()))
        });
    });
    detail
}
