//! The main menu's background pictures, decoded off the UI thread the first time the Main menu
//! tab shows each screen shape for a HUD load, and kept in the HUD's images for the rest of it.

use std::sync::Arc;

use gpui_kit::Context;
use tf2tools_core::{HudFiles, PartKey, Scene};

use super::{HudImage, HudMakerPage, Images, Load, to_render_image, top_layer};
use crate::services::run_blocking;

/// The main menu's chapter list, which a picked menu background writes.
const CHAPTERS: &str = "scripts/chapterbackgrounds.txt";

/// Where one menu background's read stands for one HUD load.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(super) enum MenuArt {
    #[default]
    Unread,
    Reading,
    Read,
}

impl HudMakerPage {
    /// How many of the HUD's menu backgrounds are decoded and held, for the tests.
    pub fn menu_backgrounds_held(&self) -> usize {
        let Load::Ready(loaded) = &self.load else {
            return 0;
        };
        let names = loaded.menu_backgrounds.iter().flatten();
        names
            .filter(|n| loaded.images.contains_key(&PartKey::new(*n)))
            .count()
    }

    /// How many menu background reads have started, for the tests.
    pub fn menu_background_reads(&self) -> usize {
        self.menu_reads
    }

    /// Whether the Main menu is waiting on the picture for the editor's screen shape.
    pub(super) fn menu_art_loading(&self) -> bool {
        let Load::Ready(loaded) = &self.load else {
            return false;
        };
        let shape = usize::from(loaded.editor.screen().is_wider());
        self.scene() == Some(Scene::MainMenu)
            && loaded.menu_backgrounds[shape].is_some()
            && loaded.menu_art[shape] != MenuArt::Read
    }

    // Works the background names out again when the picked chapter list changed: from the
    // editor's files when it holds one, else the HUD's own. Both pictures are read afresh,
    // since a pick can keep the HUD's names.
    pub(super) fn refresh_menu_backgrounds(&mut self, cx: &mut Context<Self>) {
        let Load::Ready(loaded) = &mut self.load else {
            return;
        };
        let editor = &loaded.editor;
        let source = (editor.model().assets().iter())
            .find(|(path, _)| path.eq_ignore_ascii_case(CHAPTERS))
            .map(|(_, source)| source.clone());
        if source == loaded.menu_source {
            return;
        }
        let names = match source {
            Some(_) => {
                let menu = |path: &str| {
                    path.eq_ignore_ascii_case(CHAPTERS) || path.starts_with("materials/console/")
                };
                let top = HudFiles::from_memory(vec![top_layer(editor, menu)]);
                let platform = loaded.platform;
                [false, true].map(|wide| top.menu_background(wide, platform).ok().flatten())
            }
            None => loaded.hud_menu_backgrounds.clone(),
        };
        let old = std::mem::replace(&mut loaded.menu_backgrounds, names);
        for name in old.iter().chain(loaded.menu_backgrounds.iter()).flatten() {
            if let Some(image) = loaded.images.remove(&PartKey::new(name))
                && !loaded.images.values().any(|i| Arc::ptr_eq(i, &image))
            {
                cx.drop_image(image, None);
            }
        }
        loaded.menu_source = source;
        loaded.menu_generation = loaded.menu_generation.wrapping_add(1);
        loaded.menu_art = Default::default();
        self.read_menu_art(cx);
        cx.notify();
    }

    // On the Main menu tab, decodes the background for the editor's screen shape once per load;
    // the other shape waits until the screen changes. It joins the HUD's images under its name,
    // and a colour-blind view then simulates it too.
    pub(super) fn read_menu_art(&mut self, cx: &mut Context<Self>) {
        if self.scene() != Some(Scene::MainMenu) {
            return;
        }
        let Load::Ready(loaded) = &mut self.load else {
            return;
        };
        let shape = usize::from(loaded.editor.screen().is_wider());
        if loaded.menu_art[shape] != MenuArt::Unread {
            return;
        }
        // No background, or one the other shape already holds, needs no read.
        let Some(name) = loaded.menu_backgrounds[shape].clone() else {
            loaded.menu_art[shape] = MenuArt::Read;
            return;
        };
        let key = PartKey::new(&name);
        if loaded.images.contains_key(&key) {
            loaded.menu_art[shape] = MenuArt::Read;
            return;
        }
        loaded.menu_art[shape] = MenuArt::Reading;
        tracing::debug!(%name, "menu art: read queued");
        let queued = std::time::Instant::now();
        let (files, platform) = (Arc::clone(&loaded.files), loaded.platform);
        // A picked background's files sit over the HUD's.
        let top = top_layer(&loaded.editor, |path| path.starts_with("materials/"));
        let (load, generation) = (self.loads, loaded.menu_generation);
        self.menu_reads += 1;
        let task = run_blocking(cx, move |_| {
            tracing::debug!(waited = ?queued.elapsed(), "menu art: read started");
            let mut decoded = Images::new();
            let read = std::time::Instant::now();
            // Only the pixels are needed, so the texture's own copy is not kept.
            let top = HudFiles::from_memory(vec![top]);
            let found = match top.image(&name, platform) {
                Ok(HudImage::Found(texture)) => Ok(HudImage::Found(texture)),
                _ => files.image(&name, platform),
            };
            tracing::debug!(took = ?read.elapsed(), "menu art: texture decoded");
            if let Ok(HudImage::Found(texture)) = found {
                let image = texture.image();
                let pixels = image.pixels().to_vec();
                if let Some(render) = to_render_image(image.width(), image.height(), pixels) {
                    decoded.insert(key, render);
                }
            }
            tracing::debug!(took = ?read.elapsed(), "menu art: picture converted");
            Ok(decoded)
        });
        cx.spawn(async move |page, cx| {
            // A failed read settles like a HUD without backgrounds, so nothing tries again.
            let images = task.await.unwrap_or_else(|err| {
                tracing::warn!("the HUD Maker could not read the main menu backgrounds: {err}");
                Images::new()
            });
            tracing::debug!(since_queued = ?queued.elapsed(), "menu art: result reached the page");
            let _ = page.update(cx, |page, cx| {
                // A HUD that landed meanwhile has its own read to make.
                let current = page.loads == load;
                let Load::Ready(loaded) = &mut page.load else {
                    return images.into_values().for_each(|i| cx.drop_image(i, None));
                };
                // A pick or an undo since the read began has started a read of its own.
                if !current || loaded.menu_generation != generation {
                    return images.into_values().for_each(|i| cx.drop_image(i, None));
                }
                // Each name keeps one texture, so a stray earlier copy leaves the atlas.
                for (key, image) in images {
                    if let Some(old) = loaded.images.insert(key, image) {
                        cx.drop_image(old, None);
                    }
                }
                loaded.menu_art[shape] = MenuArt::Read;
                page.simulate_menu(cx);
                cx.notify();
                tracing::debug!(since_queued = ?queued.elapsed(), "menu art: applied");
            });
        })
        .detach();
    }
}
