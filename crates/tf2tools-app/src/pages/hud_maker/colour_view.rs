//! Colour-blind views of the preview: page state, never saved. Colours are simulated as they
//! are drawn; part images and the screenshot are simulated off the UI thread, once per view
//! until the HUD's images or the screenshot change.

use std::sync::Arc;

use gpui_kit::{App, Context, RenderImage};
use tf2tools_core::{PartKey, Rgba, Vision};

use super::{HudMakerPage, Images, Load};
use crate::previews::to_render_image;
use crate::services::run_blocking;

/// Each view with its menu word and its condition's name.
pub(super) const VIEWS: [(Vision, &str, &str); 4] = [
    (Vision::Normal, "Normal", "Normal colour vision"),
    (Vision::Protanopia, "Red-blind", "Protanopia"),
    (Vision::Deuteranopia, "Green-blind", "Deuteranopia"),
    (Vision::Tritanopia, "Blue-blind", "Tritanopia"),
];

/// What the badge over the canvas says under `vision`; `None` for Normal.
pub(super) fn badge(vision: Vision) -> Option<String> {
    let (_, word, name) = VIEWS.iter().find(|(v, _, _)| *v == vision)?;
    (vision != Vision::Normal).then(|| format!("{word} view ({})", name.to_lowercase()))
}

/// The view, and the images simulated for each colour-blind view seen since the HUD's images or
/// the screenshot last changed: at most one set per view, so switching back is instant.
#[derive(Default)]
pub(super) struct ColourView {
    vision: Option<Vision>,
    simulated: Vec<Simulated>,
    // Views with a job running for the current inputs, and with one adding the menu backgrounds.
    running: Vec<Vision>,
    menu_running: Vec<Vision>,
    // Grows when the inputs change; a job started before lands nowhere.
    epoch: u64,
    runs: usize,
}

/// One view's images: the part images of one HUD load and, when set, the screenshot of one read.
struct Simulated {
    vision: Vision,
    load: u64,
    images: Images,
    backdrop: Option<(u64, Arc<RenderImage>)>,
}

impl HudMakerPage {
    /// How the preview is seen; Normal unless the player chose a colour-blind view.
    pub fn vision(&self) -> Vision {
        self.colour_view.vision.unwrap_or(Vision::Normal)
    }

    /// Shows the preview through `vision`, simulating its images unless a set for the current
    /// HUD and screenshot is kept; nothing is written.
    pub fn set_vision(&mut self, vision: Vision, cx: &mut Context<Self>) {
        if vision == self.vision() {
            return;
        }
        self.colour_view.vision = Some(vision);
        self.simulate_view(cx);
        self.simulate_menu(cx);
        cx.notify();
    }

    /// How many image simulations have started, for the tests.
    pub fn simulation_runs(&self) -> usize {
        self.colour_view.runs
    }

    /// The colour `part`'s text draws in under the view.
    pub fn drawn_colour(&self, part: &PartKey) -> Option<Rgba> {
        let model = self.editor()?.model();
        Some(self.vision().simulate(model.text_colour(model.part(part)?)))
    }

    /// The simulated images for the loaded HUD under a colour-blind view; until its set lands,
    /// the last set to land stays, and Normal has none.
    pub(super) fn seen_images(&self) -> Option<&Images> {
        self.seen().map(|s| &s.images)
    }

    /// The simulated screenshot for the read `id` under a colour-blind view.
    pub(super) fn seen_backdrop(&self, id: u64) -> Option<Arc<RenderImage>> {
        let (made_for, image) = self.seen()?.backdrop.as_ref()?;
        (*made_for == id).then(|| image.clone())
    }

    // The current view's set, else the latest to land, for the current HUD load.
    fn seen(&self) -> Option<&Simulated> {
        let vision = self.vision();
        if vision == Vision::Normal {
            return None;
        }
        let kept = &self.colour_view.simulated;
        (kept.iter().find(|s| s.vision == vision))
            .or_else(|| kept.last())
            .filter(|s| s.load == self.loads)
    }

    // The HUD's images or the screenshot changed: every kept set is stale, so drop them all
    // and simulate again for the current view.
    pub(super) fn simulate_images(&mut self, cx: &mut Context<Self>) {
        let view = &mut self.colour_view;
        view.epoch = view.epoch.wrapping_add(1);
        view.running.clear();
        view.menu_running.clear();
        for stale in std::mem::take(&mut view.simulated) {
            drop_simulated(Some(stale), cx);
        }
        self.simulate_view(cx);
    }

    // Starts one background job that simulates every part image and the screenshot for the
    // current view, unless it is Normal, already kept or already running.
    fn simulate_view(&mut self, cx: &mut Context<Self>) {
        let vision = self.vision();
        let view = &self.colour_view;
        let kept = view.simulated.iter().any(|s| s.vision == vision);
        if vision == Vision::Normal || kept || view.running.contains(&vision) {
            return;
        }
        let Load::Ready(loaded) = &self.load else {
            return;
        };
        let (epoch, load, images) = (view.epoch, self.loads, loaded.images.clone());
        let backdrop = self.backdrop_pixels();
        self.colour_view.running.push(vision);
        self.colour_view.runs += 1;
        let task = run_blocking(cx, move |_| {
            let images: Images = (images.into_iter())
                .filter_map(|(key, image)| Some((key, simulate_image(&image, vision)?)))
                .collect();
            let backdrop = backdrop.and_then(|(id, picture)| {
                let mut rgba = picture.pixels().to_vec();
                vision.simulate_rgba(&mut rgba);
                Some((id, to_render_image(picture.wide(), picture.tall(), rgba)?))
            });
            Ok(Simulated {
                vision,
                load,
                images,
                backdrop,
            })
        });
        cx.spawn(async move |page, cx| {
            let Ok(simulated) = task.await else {
                return;
            };
            let _ = page.update(cx, |page, cx| {
                let view = &mut page.colour_view;
                if view.epoch != epoch || page.loads != simulated.load {
                    return drop_simulated(Some(simulated), cx);
                }
                view.running.retain(|v| *v != simulated.vision);
                let old = (view.simulated.iter())
                    .position(|s| s.vision == simulated.vision)
                    .map(|i| view.simulated.remove(i));
                view.simulated.push(simulated);
                drop_simulated(old, cx);
                page.simulate_menu(cx);
                cx.notify();
            });
        })
        .detach();
    }
}

impl HudMakerPage {
    // The current view's kept set predates the menu backgrounds being read: simulate just those
    // into it off the UI thread, rather than every image again.
    pub(super) fn simulate_menu(&mut self, cx: &mut Context<Self>) {
        let vision = self.vision();
        let Load::Ready(loaded) = &self.load else {
            return;
        };
        let view = &self.colour_view;
        let Some(set) =
            (view.simulated.iter()).find(|s| s.vision == vision && s.load == self.loads)
        else {
            return;
        };
        let missing: Images = (loaded.menu_backgrounds.iter().flatten())
            .map(PartKey::new)
            .filter(|key| !set.images.contains_key(key))
            .filter_map(|key| Some((key.clone(), loaded.images.get(&key)?.clone())))
            .collect();
        if missing.is_empty() || view.menu_running.contains(&vision) {
            return;
        }
        let (epoch, load) = (view.epoch, self.loads);
        self.colour_view.menu_running.push(vision);
        let task = run_blocking(cx, move |_| {
            Ok((missing.into_iter())
                .filter_map(|(key, image)| Some((key, simulate_image(&image, vision)?)))
                .collect::<Images>())
        });
        cx.spawn(async move |page, cx| {
            let Ok(images) = task.await else {
                return;
            };
            let _ = page.update(cx, |page, cx| {
                let current = page.colour_view.epoch == epoch && page.loads == load;
                let view = &mut page.colour_view;
                // An older epoch's marker is already gone, and a newer job's must stay.
                if current {
                    view.menu_running.retain(|v| *v != vision);
                }
                let set = (view.simulated.iter_mut()).find(|s| current && s.vision == vision);
                let Some(set) = set else {
                    return images.into_values().for_each(|i| cx.drop_image(i, None));
                };
                for (key, image) in images {
                    if let Some(old) = set.images.insert(key, image) {
                        cx.drop_image(old, None);
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    // A removed screenshot's simulated copies leave the atlas with it.
    pub(super) fn drop_seen_backdrop(&mut self, cx: &mut App) {
        for seen in &mut self.colour_view.simulated {
            if let Some((_, image)) = seen.backdrop.take() {
                cx.drop_image(image, None);
            }
        }
    }
}

/// A BGRA texture's first frame seen through `vision`, rebuilt as a new texture.
fn simulate_image(image: &RenderImage, vision: Vision) -> Option<Arc<RenderImage>> {
    let size = image.size(0);
    let (wide, tall) = (
        u32::try_from(size.width.0).ok()?,
        u32::try_from(size.height.0).ok()?,
    );
    let mut rgba = image.as_bytes(0)?.to_vec();
    for px in rgba.as_chunks_mut::<4>().0 {
        px.swap(0, 2);
    }
    vision.simulate_rgba(&mut rgba);
    to_render_image(wide, tall, rgba)
}

// Simulated textures live in each window's atlas until dropped, as the HUD's own do.
fn drop_simulated(simulated: Option<Simulated>, cx: &mut App) {
    let Some(simulated) = simulated else {
        return;
    };
    let backdrop = simulated.backdrop.map(|(_, image)| image);
    for image in simulated.images.into_values().chain(backdrop) {
        cx.drop_image(image, None);
    }
}
