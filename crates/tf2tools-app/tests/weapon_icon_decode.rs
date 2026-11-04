//! Weapon icons decode at the wiki's thumbnail size, and an opaque white ground is cleared.

use std::io::Cursor;

use image::{DynamicImage, ImageFormat, Rgba, RgbaImage};
use tf2tools_app::remote_images::ImageTarget;
use tf2tools_app::remote_images::decode::{clear_white_backdrop, decode_image};

const CLEAR: Rgba<u8> = Rgba([0, 0, 0, 0]);
const WHITE: Rgba<u8> = Rgba([255, 255, 255, 255]);
const RED: Rgba<u8> = Rgba([200, 30, 30, 255]);

// A 100x75 icon: a red weapon block in the middle of `ground`.
fn icon_on(ground: Rgba<u8>) -> RgbaImage {
    RgbaImage::from_fn(100, 75, |x, y| {
        if (30..70).contains(&x) && (20..55).contains(&y) {
            RED
        } else {
            ground
        }
    })
}

fn png(image: RgbaImage) -> Vec<u8> {
    let mut out = Cursor::new(Vec::new());
    DynamicImage::ImageRgba8(image)
        .write_to(&mut out, ImageFormat::Png)
        .unwrap();
    out.into_inner()
}

#[test]
fn a_weapon_icon_keeps_the_wiki_thumbnail_size() {
    let image = decode_image(&png(icon_on(CLEAR)), ImageTarget::WeaponIcon).unwrap();
    let size = image.size(0);
    assert_eq!((i32::from(size.width), i32::from(size.height)), (100, 75));
}

#[test]
fn a_transparent_ground_is_left_alone() {
    let mut image = icon_on(CLEAR);
    let before = image.clone();
    clear_white_backdrop(&mut image);
    assert_eq!(image, before);
}

#[test]
fn an_opaque_white_ground_is_cleared_and_the_weapon_kept() {
    let mut image = icon_on(Rgba([250, 248, 255, 255]));
    clear_white_backdrop(&mut image);
    assert_eq!(*image.get_pixel(0, 0), CLEAR);
    assert_eq!(*image.get_pixel(99, 74), CLEAR);
    assert_eq!(*image.get_pixel(29, 40), CLEAR);
    assert_eq!(*image.get_pixel(50, 40), RED);
}

#[test]
fn white_enclosed_by_the_weapon_is_kept() {
    let mut image = icon_on(WHITE);
    image.put_pixel(50, 40, WHITE);
    clear_white_backdrop(&mut image);
    assert_eq!(*image.get_pixel(50, 40), WHITE);
    assert_eq!(*image.get_pixel(0, 0), CLEAR);
}

#[test]
fn white_in_only_some_corners_is_left_alone() {
    let mut image = icon_on(CLEAR);
    image.put_pixel(0, 0, WHITE);
    image.put_pixel(99, 0, WHITE);
    let before = image.clone();
    clear_white_backdrop(&mut image);
    assert_eq!(image, before);
}

// RenderImage bytes are BGRA: a weapon icon's white ground decodes clear, a card keeps it.
#[test]
fn only_weapon_icons_are_cleared_when_decoded() {
    let bytes = png(icon_on(WHITE));
    let icon = decode_image(&bytes, ImageTarget::WeaponIcon).unwrap();
    assert_eq!(&icon.as_bytes(0).unwrap()[..4], &[0, 0, 0, 0]);
    let card = decode_image(&bytes, ImageTarget::Card).unwrap();
    assert_eq!(&card.as_bytes(0).unwrap()[..4], &[255, 255, 255, 255]);
}
