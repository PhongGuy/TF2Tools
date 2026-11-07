//! The HUD Maker's pictures: a part's picture and the main menu's background, picked as a
//! file, read and encoded off the UI thread, and landed as one undo step each.

use std::path::Path;
use std::sync::Arc;

use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::{ActiveTheme as _, Disableable as _, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{AnyElement, Context, IntoElement, ParentElement as _, Styled as _, Window, div};
use tf2tools_core::{
    HudFiles, HudImage, PartKey, PictureTarget, Platform, PngReadError, RgbaImage, Scene,
    fit_hud_texture, hud_texture_vtf, menu_textures, read_picture,
};

use super::editor::PictureRefused;
use super::panel::RESET_TIP;
use super::tip::tipped;
use super::{HudMakerPage, Load, top_layer};
use crate::ids;
use crate::picker::PickKind;
use crate::services::{Services, run_blocking};
use crate::shell::toast::{ToastKind, toast};
use crate::widgets::labelled;

const HUD_FULL: &str =
    "This HUD can't take more pictures. Undo or reset some picture changes first.";
const NOT_A_PICTURE: &str = "That file isn't a picture TF2Tools can read.";
const TOO_WIDE: &str = "That picture is too big. Pick one under 4096 pixels on each side.";
const FILE_TOO_BIG: &str = "That file is too big. Pick one under 32 MB.";
const SHARED: &str = "This changes the game's own picture, so it changes everywhere the game shows it, not only here.";

// A picked file read and encoded, or the line to toast instead.
type Read<T> = Result<T, &'static str>;

impl HudMakerPage {
    /// Lands `vtf` on `part` when it is still the selection; a HudFull refusal toasts
    /// "This HUD can't take more pictures. Undo or reset some picture changes first."
    pub fn apply_picture(
        &mut self,
        part: &PartKey,
        vtf: Vec<u8>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.editor().and_then(|e| e.selected()) != Some(part) {
            return;
        }
        let mut refused = None;
        self.edit_selected(window, cx, |e| {
            refused = e.replace_picture_selected(vtf).err()
        });
        self.landed(refused, window, cx);
    }

    /// Lands a menu background of `narrow` (4:3) and `wide` (16:9) textures as one undo step.
    pub fn apply_menu_background(
        &mut self,
        narrow: Vec<u8>,
        wide: Vec<u8>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let mut refused = None;
        self.edit_selected(window, cx, |e| {
            refused = e.set_menu_background(narrow, wide).err()
        });
        self.landed(refused, window, cx);
    }

    // Redraws what a landed pick changed, or says why a full HUD took nothing.
    fn landed(
        &mut self,
        refused: Option<PictureRefused>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.decode_shown_images(cx);
        if refused == Some(PictureRefused::HudFull) {
            toast(window, cx, ToastKind::Error, HUD_FULL, "");
        }
    }

    // Asks for a picture for the selection, then reads, sizes and encodes it off the UI
    // thread. A picture the game's code sets keeps the size of the texture it replaces. It
    // lands only on the same load and selection.
    pub(super) fn pick_picture(&mut self, cx: &mut Context<Self>) {
        self.flush(cx);
        let Load::Ready(loaded) = &self.load else {
            return;
        };
        let editor = &loaded.editor;
        let (Some(part), Some(target)) =
            (editor.selected().cloned(), editor.picture_target_selected())
        else {
            return;
        };
        let replaced = match target {
            PictureTarget::Code { materials } => materials.first().and_then(|m| image_name(m)),
            PictureTarget::Keys(_) | _ => None,
        };
        let top = match replaced {
            Some(_) => top_layer(editor, |path| path.starts_with("materials/")),
            None => Vec::new(),
        };
        let (files, platform) = (Arc::clone(&loaded.files), loaded.platform);
        let read = move |path: &Path| {
            let like = replaced.and_then(|name| texture_size(top, &files, &name, platform));
            let image = read_picture(path).map_err(|e| picture_line(&e))?;
            hud_texture_vtf(&fit_hud_texture(&image, like)).map_err(|_| NOT_A_PICTURE)
        };
        self.pick_and_land(
            read,
            move |page, vtf, window, cx| page.apply_picture(&part, vtf, window, cx),
            cx,
        );
    }

    // Asks for a picture for the main menu's background, then crops and encodes it for a 4:3
    // and a 16:9 screen off the UI thread; it lands only on the same load.
    fn pick_menu_background(&mut self, cx: &mut Context<Self>) {
        self.flush(cx);
        let read = |path: &Path| {
            let image = read_picture(path).map_err(|e| picture_line(&e))?;
            let (narrow, wide) = menu_textures(&image);
            let encode = |image: &RgbaImage| hud_texture_vtf(image).map_err(|_| NOT_A_PICTURE);
            Ok((encode(&narrow)?, encode(&wide)?))
        };
        self.pick_and_land(
            read,
            |page, (narrow, wide), window, cx| page.apply_menu_background(narrow, wide, window, cx),
            cx,
        );
    }

    // On the Main menu, the HUD section's first group: choose the menu's background, and Reset
    // once one is picked.
    pub(super) fn menu_background_rows(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let editor = self.editor().filter(|e| e.scene() == Scene::MainMenu)?;
        let theme = cx.theme();
        let name = div()
            .text_sm()
            .font_weight(gpui_kit::FontWeight::SEMIBOLD)
            .text_color(theme.foreground)
            .child("Menu background");
        let reset = editor.has_menu_background().then(|| {
            let button = Button::new(ids::HUD_MAKER_MENU_PICTURE_RESET)
                .ghost()
                .xsmall()
                .icon(IconName::RotateCcw)
                .accessibility_label("Reset the menu background")
                .on_click(cx.listener(|page, _, window, cx| {
                    page.edit_selected(window, cx, |e| {
                        e.reset_menu_background();
                    });
                    page.decode_shown_images(cx);
                }));
            tipped(&ids::HUD_MAKER_MENU_PICTURE_RESET.into(), RESET_TIP, button)
        });
        let choose = Button::new(ids::HUD_MAKER_MENU_PICTURE_PICK)
            .outline()
            .small()
            .label("Choose picture…")
            .on_click(cx.listener(|page, _, _, cx| page.pick_menu_background(cx)));
        let rows = v_flex()
            .gap_2()
            .child(
                h_flex()
                    .justify_between()
                    .items_center()
                    .child(name)
                    .children(reset),
            )
            .child(choose);
        Some(rows.into_any_element())
    }

    // Asks for a picture, runs `read` on it off the UI thread, and lands the result through
    // `land` while the same HUD is loaded and no later pick began; a file it refuses toasts in
    // plain words.
    fn pick_and_land<T: Send + 'static>(
        &mut self,
        read: impl FnOnce(&Path) -> Read<T> + Send + 'static,
        land: impl FnOnce(&mut Self, T, &mut Window, &mut Context<Self>) + 'static,
        cx: &mut Context<Self>,
    ) {
        self.picks = self.picks.wrapping_add(1);
        let (load, pick) = (self.loads, self.picks);
        let picking = cx
            .global::<Services>()
            .picker
            .pick(PickKind::HudPicture, cx);
        let win = self.win;
        cx.spawn(async move |page, cx| {
            let Some(path) = picking.await else {
                return;
            };
            let job = cx.update(|cx| run_blocking(cx, move |_| Ok(read(&path))));
            let read = job.await.unwrap_or(Err(NOT_A_PICTURE));
            cx.update(|cx| {
                let _ = win.update(cx, |_, window, cx| {
                    page.update(cx, |page, cx| {
                        if page.loads != load || page.picks != pick {
                            return;
                        }
                        match read {
                            Ok(done) => land(page, done, window, cx),
                            Err(line) => toast(window, cx, ToastKind::Error, line, ""),
                        }
                    })
                });
            });
        })
        .detach();
    }

    // The selection's Picture row: choose a file, reset the picks, and for a picture the game's
    // code sets, a note that it changes everywhere.
    pub(super) fn picture_row(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let editor = self.editor()?;
        let target = editor.picture_target_selected()?;
        let muted = cx.theme().muted_foreground;
        let reset = Button::new(ids::HUD_MAKER_PICTURE_RESET)
            .ghost()
            .xsmall()
            .icon(IconName::RotateCcw)
            .accessibility_label(RESET_TIP)
            .disabled(!editor.picture_changed())
            .on_click(cx.listener(|page, _, window, cx| {
                page.edit_selected(window, cx, |e| {
                    e.reset_picture_selected();
                });
                page.decode_shown_images(cx);
            }));
        let choose = Button::new(ids::HUD_MAKER_PICTURE_PICK)
            .outline()
            .small()
            .label("Choose picture…")
            .on_click(cx.listener(|page, _, _, cx| page.pick_picture(cx)));
        let shared = matches!(target, PictureTarget::Code { .. });
        let row = v_flex()
            .gap_1()
            .child(
                h_flex()
                    .justify_between()
                    .items_center()
                    .child(div().text_xs().text_color(muted).child("Picture"))
                    .child(tipped(
                        &ids::HUD_MAKER_PICTURE_RESET.into(),
                        RESET_TIP,
                        reset,
                    )),
            )
            .child(choose)
            .when(shared, |el| {
                el.child(labelled(
                    div().text_xs().text_color(muted),
                    ids::HUD_MAKER_PICTURE_SHARED,
                    SHARED,
                ))
            });
        Some(row.into_any_element())
    }
}

// The player's line for a file `read_picture` refused.
fn picture_line(err: &PngReadError) -> &'static str {
    match err {
        PngReadError::TooLarge { .. } => TOO_WIDE,
        PngReadError::FileTooLarge { .. } => FILE_TOO_BIG,
        PngReadError::Io { .. } | PngReadError::Invalid { .. } | _ => NOT_A_PICTURE,
    }
}

// The image name, relative to `materials/vgui/`, that draws the material at `path`.
fn image_name(path: &str) -> Option<String> {
    let inner = path.strip_prefix("materials/")?.strip_suffix(".vmt")?;
    Some(format!("../{inner}"))
}

// The size of the texture `name` draws, from the editor's files in `top`, else the HUD's.
fn texture_size(
    top: Vec<(String, Vec<u8>)>,
    files: &Arc<HudFiles>,
    name: &str,
    platform: Platform,
) -> Option<(u32, u32)> {
    let size = |files: &HudFiles| match files.image(name, platform) {
        Ok(HudImage::Found(texture)) => {
            let image: &RgbaImage = texture.image();
            Some((image.width(), image.height()))
        }
        _ => None,
    };
    size(&HudFiles::from_memory(vec![top])).or_else(|| size(files))
}
