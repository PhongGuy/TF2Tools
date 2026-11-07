//! The session's crosshair preview cache. Builtins draw their bundled PNG, which gpui
//! loads off the UI thread; library crosshairs decode their VTF in background batches.

use std::collections::{HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use gpui_kit::{App, AppContext as _, Context, Entity, Global, ImageSource, RenderImage};
use tf2tools_core::CrosshairRef;

use crate::services::{Services, run_blocking};

/// How many crosshairs one background decode takes.
pub const DECODE_BATCH: usize = 32;

/// RGBA rows (core's `RgbaImage` layout) to the BGRA `RenderImage` gpui draws.
/// None when `rgba` is shorter than `width * height * 4`.
pub fn to_render_image(width: u32, height: u32, mut rgba: Vec<u8>) -> Option<Arc<RenderImage>> {
    let len = (width as usize)
        .checked_mul(height as usize)?
        .checked_mul(4)?;
    if rgba.len() < len {
        return None;
    }
    rgba.truncate(len);
    for pixel in rgba.as_chunks_mut::<4>().0 {
        pixel.swap(0, 2);
    }
    let buffer = image::RgbaImage::from_raw(width, height, rgba)?;
    Some(Arc::new(RenderImage::new([image::Frame::new(buffer)])))
}

/// Turns one crosshair into its preview on the background pool; None marks it Missing.
pub type Decoder = Arc<dyn Fn(&Services, &CrosshairRef) -> Option<Arc<RenderImage>> + Send + Sync>;

// The production decoder: the core's VTF decode, converted to what gpui draws.
fn decode_vtf(s: &Services, c: &CrosshairRef) -> Option<Arc<RenderImage>> {
    let rgba = s.library.crosshair_preview(c, &s.builtins).ok()?;
    let (w, h) = (rgba.width(), rgba.height());
    to_render_image(w, h, rgba.into_pixels())
}

/// What to draw for one crosshair.
#[derive(Clone)]
pub enum Preview {
    /// Requested or not yet requested; draw nothing (or a placeholder).
    Pending,
    /// A bundled PNG (`ImageSource::Resource`) or a decoded VTF (`ImageSource::Render`).
    Ready(ImageSource),
    /// Unknown crosshair, missing file or undecodable VTF.
    Missing,
}

/// Decoded previews keyed by crosshair, filled one batch at a time on the background pool.
pub struct CrosshairPreviews {
    /// A decode result; None marks a crosshair that could not be decoded.
    decoded: HashMap<CrosshairRef, Option<Arc<RenderImage>>>,
    queue: VecDeque<CrosshairRef>,
    /// Queued or in flight, so a repeat request is a no-op.
    queued: HashSet<CrosshairRef>,
    in_flight: bool,
    decodes: usize,
    decoder: Decoder,
}

struct PreviewsGlobal(Entity<CrosshairPreviews>);
impl Global for PreviewsGlobal {}

impl CrosshairPreviews {
    /// The session's cache, created on first use. Needs the `Services` global.
    pub fn global(cx: &mut App) -> Entity<Self> {
        if let Some(global) = cx.try_global::<PreviewsGlobal>() {
            return global.0.clone();
        }
        let previews = cx.new(|_| Self {
            decoded: HashMap::new(),
            queue: VecDeque::new(),
            queued: HashSet::new(),
            in_flight: false,
            decodes: 0,
            decoder: Arc::new(decode_vtf),
        });
        cx.set_global(PreviewsGlobal(previews.clone()));
        previews
    }

    fn bundled_png(crosshair: &CrosshairRef, cx: &App) -> Option<PathBuf> {
        match crosshair {
            CrosshairRef::Builtin(name) => cx
                .global::<Services>()
                .builtins
                .get(name)?
                .preview_png_path()
                .map(Path::to_path_buf),
            CrosshairRef::Library(_) | _ => None,
        }
    }

    /// What to draw for `crosshair` now. A builtin with a bundled PNG is always Ready.
    pub fn preview(&self, crosshair: &CrosshairRef, cx: &App) -> Preview {
        if let Some(png) = Self::bundled_png(crosshair, cx) {
            return Preview::Ready(png.into());
        }
        match self.decoded.get(crosshair) {
            Some(Some(image)) => Preview::Ready(Arc::clone(image).into()),
            Some(None) => Preview::Missing,
            None => Preview::Pending,
        }
    }

    /// Queues every crosshair not yet decoded, queued or covered by a PNG, and starts decoding.
    pub fn request(
        &mut self,
        crosshairs: impl IntoIterator<Item = CrosshairRef>,
        cx: &mut Context<Self>,
    ) {
        for crosshair in crosshairs {
            let known = self.decoded.contains_key(&crosshair) || self.queued.contains(&crosshair);
            if known || Self::bundled_png(&crosshair, cx).is_some() {
                continue;
            }
            self.queued.insert(crosshair.clone());
            self.queue.push_back(crosshair);
        }
        self.pump(cx);
    }

    /// Replaces the decoder for later batches (tests inject failures through it).
    pub fn set_decoder(&mut self, decoder: Decoder) {
        self.decoder = decoder;
    }

    /// Crosshairs queued or in flight.
    pub fn pending(&self) -> usize {
        self.queued.len()
    }

    /// Crosshairs sent to `crosshair_preview` this session.
    pub fn decodes(&self) -> usize {
        self.decodes
    }

    // Sends the next batch to the pool unless one is in flight; each landing batch pumps again,
    // so one batch at a time leaves the pool free for page loads.
    fn pump(&mut self, cx: &mut Context<Self>) {
        if self.in_flight || self.queue.is_empty() {
            return;
        }
        self.in_flight = true;
        let n = self.queue.len().min(DECODE_BATCH);
        let batch: Vec<CrosshairRef> = self.queue.drain(..n).collect();
        self.decodes += batch.len();
        let sent = batch.clone();
        let decoder = Arc::clone(&self.decoder);
        let decoding = run_blocking(cx, move |s| {
            Ok(batch
                .into_iter()
                // One corrupt VTF that panics the decoder marks only itself missing.
                .map(|c| {
                    let image = crate::crash::catching(|| decoder(s, &c)).unwrap_or(None);
                    (c, image)
                })
                .collect::<Vec<_>>())
        });
        cx.spawn(async move |this, cx| {
            let done = decoding.await;
            let _ = this.update(cx, |this, cx| {
                this.in_flight = false;
                // A batch that panics outside the per-item guard is marked missing, not retried.
                let results =
                    done.unwrap_or_else(|_| sent.into_iter().map(|c| (c, None)).collect());
                for (c, image) in results {
                    this.queued.remove(&c);
                    this.decoded.insert(c, image);
                }
                cx.notify();
                this.pump(cx);
            });
        })
        .detach();
    }
}
