//! Store screenshots fetched over the network and decoded off the UI thread: the pure
//! scheduling queue, the decode step and the session cache entity that drives them, keyed
//! by URL plus the box the image is drawn in.

pub mod decode;
pub mod queue;

use std::collections::HashMap;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::Arc;

use gpui_kit::{App, AppContext as _, Context, Entity, Global, RenderImage, Task};
use tf2tools_core::{ImageError, StoreClient};

use crate::tokio_bridge;
use queue::{IMAGE_BUDGET, ImageQueue, ImageState};

/// The box an image is decoded into; a card and a carousel of one URL are two images.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ImageTarget {
    /// A store grid card, 480x270.
    Card,
    /// A mature add-on card before it is revealed: `Card`'s box, blurred.
    CardBlurred,
    /// The detail carousel, 1280x720.
    Carousel,
    /// A weapon row icon, 100x75, the wiki thumbnail's size.
    WeaponIcon,
}

impl ImageTarget {
    /// The (width, height) an image is downsized to fit inside.
    pub fn bounds(self) -> (u32, u32) {
        match self {
            ImageTarget::Card | ImageTarget::CardBlurred => (480, 270),
            ImageTarget::Carousel => (1280, 720),
            ImageTarget::WeaponIcon => (100, 75),
        }
    }
}

/// One image: a URL decoded for one target.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ImageKey {
    pub url: String,
    pub target: ImageTarget,
}

impl ImageKey {
    /// The URL decoded for a store card.
    pub fn card(url: &str) -> Self {
        Self {
            url: url.to_owned(),
            target: ImageTarget::Card,
        }
    }

    /// The URL decoded, blurred, for a mature card that is not revealed yet.
    pub fn card_blurred(url: &str) -> Self {
        Self {
            url: url.to_owned(),
            target: ImageTarget::CardBlurred,
        }
    }

    /// The URL decoded for the carousel.
    pub fn carousel(url: &str) -> Self {
        Self {
            url: url.to_owned(),
            target: ImageTarget::Carousel,
        }
    }

    /// The URL decoded for a weapon row.
    pub fn weapon_icon(url: &str) -> Self {
        Self {
            url: url.to_owned(),
            target: ImageTarget::WeaponIcon,
        }
    }
}

/// Why a fetch failed: `Permanent` is never retried this session, `Transient` after a pause.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FetchFailure {
    Permanent,
    Transient,
}

/// One fetch in flight, run on the tokio runtime.
pub type FetchFuture = Pin<Box<dyn Future<Output = Result<Vec<u8>, FetchFailure>> + Send>>;
/// Fetches an image's bytes by URL; tests inject a fake.
pub type ImageFetch = Arc<dyn Fn(String) -> FetchFuture + Send + Sync>;
/// Decodes bytes for a target; `decode::decode_image` in the app.
pub type DecodeFn = fn(&[u8], ImageTarget) -> Option<Arc<RenderImage>>;

/// `StoreClient::fetch_image`; RateLimited, Network and Http 5xx are Transient, the rest Permanent.
pub fn store_fetch(store: StoreClient) -> ImageFetch {
    Arc::new(move |url: String| {
        let store = store.clone();
        Box::pin(async move {
            store.fetch_image(&url).await.map_err(|error| match error {
                ImageError::RateLimited { .. } | ImageError::Network { .. } => {
                    FetchFailure::Transient
                }
                ImageError::Http { status, .. } if status >= 500 => FetchFailure::Transient,
                ImageError::UntrustedUrl { .. }
                | ImageError::Http { .. }
                | ImageError::TooLarge { .. }
                | _ => FetchFailure::Permanent,
            })
        })
    })
}

/// Marks an image key's URL as a file on disk rather than an address on an image host.
const LOCAL_SCHEME: &str = "file://";
/// The most bytes a picture on disk may have before it is refused unread.
const LOCAL_MAX_BYTES: u64 = 8 * 1024 * 1024;

/// The key URL for the picture at `path`. `version` changes whenever the file is rewritten, so
/// a decode of the old picture is never shown for the new one.
pub fn local_image_url(path: &Path, version: &str) -> String {
    format!("{LOCAL_SCHEME}{}#{version}", path.to_string_lossy())
}

fn local_path(url: &str) -> Option<PathBuf> {
    let rest = url.strip_prefix(LOCAL_SCHEME)?;
    Some(PathBuf::from(
        rest.rsplit_once('#').map_or(rest, |(path, _)| path),
    ))
}

/// `fetch`, except that a `local_image_url` is read from disk; a missing, unreadable or
/// oversized file fails for good.
fn with_local_files(fetch: ImageFetch) -> ImageFetch {
    Arc::new(move |url: String| match local_path(&url) {
        Some(path) => Box::pin(async move {
            let meta = tokio::fs::metadata(&path).await;
            if !meta.is_ok_and(|m| m.is_file() && m.len() <= LOCAL_MAX_BYTES) {
                return Err(FetchFailure::Permanent);
            }
            tokio::fs::read(&path)
                .await
                .map_err(|_| FetchFailure::Permanent)
        }),
        None => fetch(url),
    })
}

/// What a view draws for one image.
#[derive(Clone)]
pub enum RemoteImage {
    Loading,
    Ready(Arc<RenderImage>),
    Failed,
}

/// Session counters, for tests and the scroll probe.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ImageStats {
    pub fetches: usize,
    pub decodes: usize,
    /// Decodes that ran on the UI thread; always 0 unless the executor wiring breaks.
    pub foreground_decodes: usize,
    pub evicted: usize,
    /// Evicted images whose atlas textures were dropped through `drop_image`.
    pub released: usize,
    pub bytes: u64,
    /// `want` calls from every viewer this session.
    pub wants: usize,
}

/// The session's remote image cache: fetches on the tokio runtime, decodes on the
/// background executor, and keeps decoded images within a byte budget.
pub struct RemoteImages {
    queue: ImageQueue<Arc<RenderImage>>,
    fetch: ImageFetch,
    decode: DecodeFn,
    /// The fetch or decode in flight per key; dropping the entity cancels them.
    tasks: HashMap<ImageKey, Task<()>>,
    stats: ImageStats,
}

struct RemoteImagesGlobal(Entity<RemoteImages>);
impl Global for RemoteImagesGlobal {}

impl RemoteImages {
    /// Installs the session cache with the default budget and decoder.
    pub fn install(cx: &mut App, fetch: ImageFetch) -> Entity<Self> {
        Self::install_with(cx, fetch, IMAGE_BUDGET, decode::decode_image)
    }

    /// Installs a cache keeping at most `budget` decoded bytes, and sets it as the global.
    /// `fetch` serves every key but a `local_image_url`, which is read from disk.
    /// Does not start the tokio runtime; the first fetch does.
    pub fn install_with(
        cx: &mut App,
        fetch: ImageFetch,
        budget: u64,
        decode: DecodeFn,
    ) -> Entity<Self> {
        let images = cx.new(|_| Self {
            queue: ImageQueue::new(budget),
            fetch: with_local_files(fetch),
            decode,
            tasks: HashMap::new(),
            stats: ImageStats::default(),
        });
        cx.set_global(RemoteImagesGlobal(images.clone()));
        images
    }

    /// The installed cache, if any.
    pub fn global(cx: &App) -> Option<Entity<Self>> {
        cx.try_global::<RemoteImagesGlobal>().map(|g| g.0.clone())
    }

    /// What to draw for `key` now; a ready image counts as just used.
    pub fn image(&self, key: &ImageKey) -> RemoteImage {
        match self.queue.get(key) {
            ImageState::Loading => RemoteImage::Loading,
            ImageState::Ready(image) => RemoteImage::Ready(image),
            ImageState::Failed => RemoteImage::Failed,
        }
    }

    /// Replaces `viewer`'s wanted images (newest first) and starts what slots allow.
    /// A view that stops drawing images calls this with an empty list.
    pub fn want(&mut self, viewer: &str, keys: Vec<ImageKey>, cx: &mut Context<Self>) {
        self.stats.wants += 1;
        let now = cx.background_executor().now();
        self.queue.want(viewer, keys, now);
        self.pump(cx);
    }

    /// Viewers holding a non-empty want; a diagnostic for tests, not used by the app.
    pub fn viewers(&self) -> usize {
        self.queue.viewers()
    }

    /// Session counters, with the decoded bytes held now.
    pub fn stats(&self) -> ImageStats {
        ImageStats {
            bytes: self.queue.bytes(),
            ..self.stats
        }
    }

    // Fill the free fetch and decode slots. The runtime starts only once something needs
    // fetching, so a want of cached, failed or no keys never starts it.
    fn pump(&mut self, cx: &mut Context<Self>) {
        while let Some(key) = self.queue.next_fetch() {
            if !tokio_bridge::is_running(cx) {
                tokio_bridge::init(cx);
            }
            self.stats.fetches += 1;
            let job = tokio_bridge::spawn(cx, (self.fetch)(key.url.clone()));
            let landed = key.clone();
            let task = cx.spawn(async move |this, cx| {
                // A runtime that shut down mid-fetch is worth retrying later.
                let result = job.await.unwrap_or(Err(FetchFailure::Transient));
                let _ = this.update(cx, |images, cx| {
                    images.tasks.remove(&landed);
                    let now = cx.background_executor().now();
                    images.queue.fetched(&landed, result, now);
                    cx.notify();
                    images.pump(cx);
                });
            });
            self.tasks.insert(key, task);
        }
        while let Some((key, bytes)) = self.queue.next_decode() {
            let decode = self.decode;
            let target = key.target;
            let executor = cx.background_executor().clone();
            // A decoder panic fails only this image.
            let job = cx.background_spawn(async move {
                let foreground = executor.is_main_thread();
                let image = crate::crash::catching(|| decode(&bytes, target))
                    .ok()
                    .flatten();
                (image, foreground)
            });
            let landed = key.clone();
            let task = cx.spawn(async move |this, cx| {
                let (image, foreground) = job.await;
                let _ = this.update(cx, |images, cx| {
                    images.tasks.remove(&landed);
                    images.decoded(&landed, image, foreground, cx);
                    images.pump(cx);
                });
            });
            self.tasks.insert(key, task);
        }
    }

    // Cache a decode and release the atlas textures of every image it evicts.
    // Every frame is charged, and a frameless image still costs 1 so it can be evicted.
    fn decoded(
        &mut self,
        key: &ImageKey,
        image: Option<Arc<RenderImage>>,
        foreground: bool,
        cx: &mut Context<Self>,
    ) {
        self.stats.decodes += 1;
        self.stats.foreground_decodes += usize::from(foreground);
        let sized = image.map(|image| {
            let bytes = (0..image.frame_count())
                .filter_map(|i| image.as_bytes(i))
                .map(|b| b.len() as u64)
                .sum::<u64>()
                .max(1);
            (image, bytes)
        });
        for evicted in self.queue.decoded(key, sized) {
            self.stats.evicted += 1;
            cx.drop_image(evicted, None);
            self.stats.released += 1;
        }
        cx.notify();
    }
}
