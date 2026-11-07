//! Image parts: one stretched piece, or nine for a scalable panel.

use crate::common;

use common::hud_stock::stock_files_with;
use tf2tools_core::{HudModel, ImageCell, PartKey, Platform, Screen, image_cells, nine_slice};

const HD: Screen = Screen::new(1920, 1080);
const BG: &str = "CMainTargetID/TargetIDBG";

/// A cell as (x, y, wide, tall, src_x, src_y, src_wide, src_tall).
type Cell = (i32, i32, i32, i32, u32, u32, u32, u32);

/// The cells of a TargetID background of `class` with `extra` keys, sized as the stock one.
fn cells(class: &str, extra: &str, texture: (u32, u32)) -> Vec<Cell> {
    let text = format!(
        r#""Resource/UI/TargetID.res" {{ TargetIDBG {{ ControlName {class} xpos 0 ypos 0 zpos -1 wide 252 tall 50 image ../hud/color_panel_brown {extra} }} }}"#
    );
    let files = stock_files_with(&[("resource/ui/targetid.res", Some(&text))]);
    let model = HudModel::load(&files, Platform::Linux).unwrap();
    let key = PartKey::new(BG);
    let b = model.layout(HD).get(&key).cloned().unwrap();
    assert_eq!((b.wide(), b.tall()), (567, 112));
    let part = model.part(&key).unwrap();
    image_cells(part, b.wide(), b.tall(), texture, HD)
        .iter()
        .map(cell)
        .collect()
}

fn cell(c: &ImageCell) -> Cell {
    (
        c.x(),
        c.y(),
        c.wide(),
        c.tall(),
        c.src_x(),
        c.src_y(),
        c.src_wide(),
        c.src_tall(),
    )
}

const CORNERS: &str =
    "src_corner_height 23 src_corner_width 23 draw_corner_width 5 draw_corner_height 5";

#[test]
fn a_scalable_panel_keeps_its_corners_and_stretches_the_rest() {
    assert_eq!(
        cells("CTFImagePanel", CORNERS, (128, 128)),
        [
            (0, 0, 11, 11, 0, 0, 23, 23),
            (11, 0, 545, 11, 23, 0, 82, 23),
            (556, 0, 11, 11, 105, 0, 23, 23),
            (0, 11, 11, 90, 0, 23, 23, 82),
            (11, 11, 545, 90, 23, 23, 82, 82),
            (556, 11, 11, 90, 105, 23, 23, 82),
            (0, 101, 11, 11, 0, 105, 23, 23),
            (11, 101, 545, 11, 23, 105, 82, 23),
            (556, 101, 11, 11, 105, 105, 23, 23),
        ]
    );
    assert_eq!(
        cells("scalableimagepanel", CORNERS, (128, 128)).len(),
        9,
        "class matched ignoring case"
    );
}

#[test]
fn a_plain_image_or_one_without_corners_stretches_whole() {
    let whole = [(0, 0, 567, 112, 0, 0, 128, 128)];
    assert_eq!(cells("ImagePanel", CORNERS, (128, 128)), whole);
    assert_eq!(cells("CTFImagePanel", "", (128, 128)), whole);
}

#[test]
fn oversized_corners_never_go_negative() {
    // Corners of 225 px leave no middle row on a 112 px box; a source corner past half the
    // texture is held to half, so the far corner starts inside it.
    let big =
        "src_corner_height 100 src_corner_width 100 draw_corner_width 100 draw_corner_height 100";
    assert_eq!(
        cells("CTFImagePanel", big, (128, 64)),
        [
            (0, 0, 225, 225, 0, 0, 64, 32),
            (342, 0, 225, 225, 64, 0, 64, 32),
            (0, 225, 225, 225, 0, 32, 64, 32),
            (342, 225, 225, 225, 64, 32, 64, 32),
        ]
    );
    assert!(cells("CTFImagePanel", CORNERS, (0, 0)).is_empty());
}

#[test]
fn a_border_slices_its_image_by_the_numbers_it_is_given() {
    // A 1440 x 922 backdrop of a 128-texel square with 23-texel corners drawn 8 units (18 px).
    let cells: Vec<Cell> = nine_slice(1440, 922, (128, 128), (8, 8), (23, 23), HD)
        .iter()
        .map(cell)
        .collect();
    assert_eq!(cells.len(), 9);
    assert_eq!(cells[0], (0, 0, 18, 18, 0, 0, 23, 23));
    assert_eq!(cells[4], (18, 18, 1404, 886, 23, 23, 82, 82));
    assert_eq!(cells[8], (1422, 904, 18, 18, 105, 105, 23, 23));
}
