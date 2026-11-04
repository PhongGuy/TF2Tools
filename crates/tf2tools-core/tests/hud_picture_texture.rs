//! Pictures read for the HUD Maker, oriented, sized and written as HUD textures.

use std::path::Path;

use image::ImageEncoder as _;
use tf2tools_core::{
    PngReadError, RgbaImage, decode_vtf, fit_hud_texture, hud_texture_vtf, menu_textures,
    read_picture,
};

fn solid(w: u32, h: u32, rgba: [u8; 4]) -> RgbaImage {
    RgbaImage::from_pixels(w, h, rgba.repeat((w * h) as usize)).unwrap()
}

fn size(image: &RgbaImage) -> (u32, u32) {
    (image.width(), image.height())
}

fn fit(w: u32, h: u32, like: Option<(u32, u32)>) -> (u32, u32) {
    size(&fit_hud_texture(&solid(w, h, [1, 2, 3, 255]), like))
}

#[test]
fn each_side_goes_to_the_nearest_power_of_two() {
    assert_eq!(fit(100, 50, None), (128, 64));
    assert_eq!(fit(48, 48, None), (64, 64), "a tie rounds up");
    assert_eq!(fit(40, 20, None), (32, 16));
    assert_eq!(fit(1, 1, None), (1, 1));
}

#[test]
fn the_replaced_texture_sets_the_size() {
    assert_eq!(fit(300, 20, Some((64, 64))), (64, 64));
    assert_eq!(fit(8, 8, Some((100, 30))), (128, 32));
}

#[test]
fn sides_stop_at_2048_and_the_area_at_2048_by_1024() {
    assert_eq!(fit(4096, 1024, None), (2048, 1024));
    assert_eq!(fit(3000, 3000, None), (1024, 1024));
    assert_eq!(fit(1024, 4096, None), (1024, 2048));
    assert_eq!(fit(8, 8, Some((2048, 2048))), (1024, 1024));
}

#[test]
fn stretching_keeps_a_solid_colour() {
    let out = fit_hud_texture(&solid(30, 10, [10, 20, 30, 200]), None);
    assert!(out.pixels().chunks(4).all(|p| p == [10, 20, 30, 200]));
}

#[test]
fn the_hud_texture_is_a_clamped_unmipped_bgra_vtf() {
    let image = solid(4, 2, [1, 2, 3, 4]);
    let vtf = hud_texture_vtf(&image).unwrap();
    assert_eq!(&vtf[..4], b"VTF\0");
    assert_eq!(u32::from_le_bytes(vtf[20..24].try_into().unwrap()), 0x230C);
    assert_eq!(vtf.len(), 80 + 4 * 2 * 4);
    assert_eq!(decode_vtf(&vtf).unwrap(), image);
}

#[test]
fn the_menu_gets_a_4_3_and_a_16_9_crop() {
    // 160x90 with a red stripe 10 px wide on the left: the 4:3 crop (x 20..140) leaves it out.
    let mut pixels = Vec::new();
    for _y in 0..90 {
        for x in 0..160 {
            pixels.extend(if x < 10 {
                [255, 0, 0, 255]
            } else {
                [0, 0, 255, 255]
            });
        }
    }
    let (narrow, wide) = menu_textures(&RgbaImage::from_pixels(160, 90, pixels).unwrap());
    assert_eq!(size(&narrow), (1024, 1024));
    assert_eq!(size(&wide), (2048, 1024));
    assert_eq!(&narrow.pixels()[..4], &[0, 0, 255, 255]);
    assert_eq!(&wide.pixels()[..4], &[255, 0, 0, 255]);
}

#[test]
fn a_png_and_a_jpeg_read_and_other_files_do_not() {
    let dir = tempfile::tempdir().unwrap();
    let png = dir.path().join("a.png");
    image::RgbaImage::from_pixel(6, 3, image::Rgba([9, 8, 7, 255]))
        .save(&png)
        .unwrap();
    assert_eq!(size(&read_picture(&png).unwrap()), (6, 3));
    let jpeg = dir.path().join("a.jpg");
    image::RgbImage::from_pixel(8, 8, image::Rgb([200, 10, 10]))
        .save(&jpeg)
        .unwrap();
    let read = read_picture(&jpeg).unwrap();
    assert_eq!(size(&read), (8, 8));
    assert!(read.pixels()[0] > 150, "lossy, but still red");
    let text = dir.path().join("notes.png");
    std::fs::write(&text, b"hello").unwrap();
    assert!(matches!(
        read_picture(&text),
        Err(PngReadError::Invalid { .. })
    ));
}

/// A big-endian TIFF chunk with one IFD entry: Orientation (0x0112), SHORT, `value`.
fn exif_orientation(value: u16) -> Vec<u8> {
    let mut out = b"MM\0\x2a".to_vec();
    out.extend_from_slice(&8u32.to_be_bytes());
    out.extend_from_slice(&1u16.to_be_bytes());
    out.extend_from_slice(&0x0112u16.to_be_bytes());
    out.extend_from_slice(&3u16.to_be_bytes());
    out.extend_from_slice(&1u32.to_be_bytes());
    out.extend_from_slice(&value.to_be_bytes());
    out.extend_from_slice(&[0, 0]);
    out.extend_from_slice(&0u32.to_be_bytes());
    out
}

fn write_jpeg(path: &Path, image: &image::RgbImage, exif: Vec<u8>) {
    let mut bytes = Vec::new();
    let mut encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut bytes, 95);
    encoder.set_exif_metadata(exif).unwrap();
    encoder
        .write_image(
            image.as_raw(),
            image.width(),
            image.height(),
            image::ExtendedColorType::Rgb8,
        )
        .unwrap();
    std::fs::write(path, bytes).unwrap();
}

#[test]
fn a_rotated_phone_photo_comes_out_upright() {
    let dir = tempfile::tempdir().unwrap();
    // 16x8, red left half, blue right half; orientation 6 means "rotate 90 degrees clockwise".
    let photo = image::RgbImage::from_fn(16, 8, |x, _| {
        if x < 8 {
            image::Rgb([250, 0, 0])
        } else {
            image::Rgb([0, 0, 250])
        }
    });
    let path = dir.path().join("phone.jpg");
    write_jpeg(&path, &photo, exif_orientation(6));
    let read = read_picture(&path).unwrap();
    assert_eq!(size(&read), (8, 16));
    // After the turn the red half is on top.
    let top = &read.pixels()[(8 * 2 + 4) * 4..][..3];
    let bottom = &read.pixels()[(8 * 13 + 4) * 4..][..3];
    assert!(top[0] > 150 && top[2] < 100, "top is red: {top:?}");
    assert!(
        bottom[2] > 150 && bottom[0] < 100,
        "bottom is blue: {bottom:?}"
    );
}

#[test]
fn a_picture_over_4096_a_side_is_refused_before_decoding() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("long.png");
    image::RgbaImage::from_pixel(4097, 1, image::Rgba([0, 0, 0, 255]))
        .save(&path)
        .unwrap();
    assert!(matches!(
        read_picture(&path),
        Err(PngReadError::TooLarge {
            width: 4097,
            height: 1
        })
    ));
}
