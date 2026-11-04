//! A screenshot behind the HUD: an app setting, never part of the HUD. Picking, reading and
//! shrinking run off the UI thread; a picture that lands late never replaces a newer one.

use std::io::{Cursor, Read as _};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use gpui_kit::{Context, RenderImage};
use image::imageops::FilterType;
use std::collections::HashSet;

use tf2tools_core::{Backdrop, PartKey};

use super::{HudMakerPage, Load};
use crate::picker::PickKind;
use crate::previews::to_render_image;
use crate::services::{Services, run_blocking};
use crate::shell::toast::{ToastKind, toast};

/// Files past this are refused before decoding.
const MAX_FILE: u64 = 40 << 20;
/// The decoder refuses a side past this, or a picture needing more memory than `MAX_ALLOC`.
const MAX_SIDE: u32 = 16384;
const MAX_ALLOC: u64 = 512 << 20;
/// Larger pictures shrink to fit this box, aspect kept; smaller ones are never enlarged.
const FIT: (u32, u32) = (1920, 1200);
/// What the player sees when a picked file can't be shown.
const UNREADABLE: &str = "That file isn't a picture TF2Tools can read.";

/// The backdrop as the page holds it: what shows, and whether the saved one's read finished.
#[derive(Default)]
pub(super) struct BackdropState {
    shown: Option<ShownBackdrop>,
    settled: bool,
    // Grows with each read started; a read lands only while it is still the latest.
    reads: u64,
}

/// A picture that shows: its pixels for the checks, its image and its read's id, which the
/// checks' cache keys on so a new picture of the same size is checked again.
struct ShownBackdrop {
    picture: Arc<Backdrop>,
    image: Arc<RenderImage>,
    id: u64,
}

/// Why a picture could not be shown; logged, never shown to the player.
#[derive(Debug)]
#[allow(dead_code, reason = "the fields are read through Debug, in the log")]
pub(super) enum BackdropUnreadable {
    TooBig(u64),
    Io(std::io::Error),
    Image(image::ImageError),
    Empty,
}

/// Reads, decodes and shrinks the picture at `path`; call it off the UI thread.
pub(super) fn read_backdrop(
    path: &Path,
) -> Result<(Backdrop, Arc<RenderImage>), BackdropUnreadable> {
    let too_big = |len| BackdropUnreadable::TooBig(len);
    let len = std::fs::metadata(path)
        .map_err(BackdropUnreadable::Io)?
        .len();
    if len > MAX_FILE {
        return Err(too_big(len));
    }
    // Read at most one byte past the cap, so a file that grew meanwhile is still refused.
    let mut bytes = Vec::new();
    std::fs::File::open(path)
        .and_then(|f| f.take(MAX_FILE + 1).read_to_end(&mut bytes))
        .map_err(BackdropUnreadable::Io)?;
    if bytes.len() as u64 > MAX_FILE {
        return Err(too_big(bytes.len() as u64));
    }
    let mut reader = image::ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .map_err(BackdropUnreadable::Io)?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(MAX_SIDE);
    limits.max_image_height = Some(MAX_SIDE);
    limits.max_alloc = Some(MAX_ALLOC);
    reader.limits(limits);
    let mut picture = reader.decode().map_err(BackdropUnreadable::Image)?;
    if picture.width() > FIT.0 || picture.height() > FIT.1 {
        picture = picture.resize(FIT.0, FIT.1, FilterType::Triangle);
    }
    let rgba = picture.to_rgba8();
    let (wide, tall) = rgba.dimensions();
    let pixels = rgba.into_raw();
    let image = to_render_image(wide, tall, pixels.clone()).ok_or(BackdropUnreadable::Empty)?;
    let backdrop = Backdrop::new(wide, tall, pixels).ok_or(BackdropUnreadable::Empty)?;
    Ok((backdrop, image))
}

impl HudMakerPage {
    /// The picture behind the HUD, as the checks read it.
    pub fn backdrop(&self) -> Option<&Backdrop> {
        self.backdrop.shown.as_ref().map(|s| &*s.picture)
    }

    /// The picture behind the HUD as the canvas draws it, through the colour-blind view once
    /// its simulated copy has landed.
    pub fn backdrop_image(&self) -> Option<Arc<RenderImage>> {
        let shown = self.backdrop.shown.as_ref()?;
        Some(
            self.seen_backdrop(shown.id)
                .unwrap_or_else(|| shown.image.clone()),
        )
    }

    /// The shown picture's pixels and read id, for simulating it under a view.
    pub(super) fn backdrop_pixels(&self) -> Option<(u64, Arc<Backdrop>)> {
        let shown = self.backdrop.shown.as_ref()?;
        Some((shown.id, shown.picture.clone()))
    }

    /// False until the saved picture's read has finished, found or not.
    pub fn backdrop_settled(&self) -> bool {
        self.backdrop.settled
    }

    /// The labels hard to read over the picture at the editor's screen, from the checks' cache;
    /// `None` without a picture, in the six-screen view or while a part drag runs.
    pub(super) fn hard_to_read(&mut self) -> Option<Arc<HashSet<PartKey>>> {
        let vision = self.vision();
        let shown = self.backdrop.shown.as_ref()?;
        let Load::Ready(loaded) = &self.load else {
            return None;
        };
        if self.every_screen || self.part_drag.is_some() {
            return None;
        }
        let hard = (self.checks).hard_to_read(&loaded.editor, &shown.picture, shown.id, vision);
        Some(hard)
    }

    // Reads the saved picture when the page opens. A file that is gone or unreadable shows
    // nothing, keeps the setting and says nothing: the player may have unplugged a drive.
    pub(super) fn load_saved_backdrop(&mut self, cx: &mut Context<Self>) {
        let id = self.next_backdrop_read();
        let job = run_blocking(cx, |s| {
            let Some(path) =
                s.db.settings()
                    .ok()
                    .and_then(|st| st.hud_maker_backdrop().map(Path::to_path_buf))
            else {
                return Ok(None);
            };
            match read_backdrop(&path) {
                Ok(read) => Ok(Some(read)),
                Err(why) => {
                    tracing::debug!(?why, "the saved HUD Maker backdrop didn't load");
                    Ok(None)
                }
            }
        });
        cx.spawn(async move |page, cx| {
            let landed = job.await;
            let _ = page.update(cx, |page, cx| {
                page.backdrop.settled = true;
                if let Ok(Some(read)) = landed
                    && page.backdrop.reads == id
                {
                    page.show_backdrop(read);
                    page.simulate_images(cx);
                }
                cx.notify();
            });
        })
        .detach();
    }

    // Asks for a picture, reads it off the UI thread and, when it shows, remembers it.
    pub(super) fn pick_backdrop(&mut self, cx: &mut Context<Self>) {
        let picking = cx.global::<Services>().picker.pick(PickKind::Backdrop, cx);
        let win = self.win;
        cx.spawn(async move |page, cx| {
            let Some(path) = picking.await else {
                return;
            };
            let Ok(id) = page.update(cx, |page, _| page.next_backdrop_read()) else {
                return;
            };
            let read_path = path.clone();
            let job = cx.update(|cx| run_blocking(cx, move |_| Ok(read_backdrop(&read_path))));
            let landed = job.await;
            cx.update(|cx| {
                let _ = win.update(cx, |_, window, cx| {
                    page.update(cx, |page, cx| match landed {
                        _ if page.backdrop.reads != id => {}
                        Ok(Ok(read)) => {
                            page.show_backdrop(read);
                            page.simulate_images(cx);
                            page.store_backdrop(Some(path), cx);
                            cx.notify();
                        }
                        failed => {
                            tracing::debug!(?failed, "a HUD Maker backdrop didn't load");
                            toast(window, cx, ToastKind::Error, UNREADABLE, "");
                        }
                    })
                });
            });
        })
        .detach();
    }

    // Removes the picture and its setting; a read still running lands nowhere.
    pub(super) fn clear_backdrop(&mut self, cx: &mut Context<Self>) {
        self.next_backdrop_read();
        self.backdrop.shown = None;
        self.drop_seen_backdrop(cx);
        self.store_backdrop(None, cx);
        cx.notify();
    }

    fn next_backdrop_read(&mut self) -> u64 {
        self.backdrop.reads = self.backdrop.reads.wrapping_add(1);
        self.backdrop.reads
    }

    fn show_backdrop(&mut self, (picture, image): (Backdrop, Arc<RenderImage>)) {
        self.backdrop.shown = Some(ShownBackdrop {
            picture: Arc::new(picture),
            image,
            id: self.backdrop.reads,
        });
    }

    fn store_backdrop(&self, path: Option<PathBuf>, cx: &mut Context<Self>) {
        run_blocking(cx, move |s| {
            if let Err(err) = s.db.set_hud_maker_backdrop(path.as_deref()) {
                tracing::warn!("the HUD Maker backdrop setting wasn't saved: {err}");
            }
            Ok(())
        })
        .detach();
    }
}
