//! Crosshair PNGs: reading under size limits, writing, and fitting any picture to the maker.

use std::fs;
use std::io::Cursor;

use tf2tools_core::{
    FitChange, PNG_MAX_FILE_BYTES, PNG_MAX_SIDE, PngReadError, PngWriteError, RgbaImage,
    fit_to_maker, read_png, write_png,
};

fn solid(w: u32, h: u32, rgba: [u8; 4]) -> RgbaImage {
    RgbaImage::from_pixels(w, h, rgba.repeat((w * h) as usize)).unwrap()
}

fn at(image: &RgbaImage, x: u32, y: u32) -> [u8; 4] {
    let i = ((y * image.width() + x) * 4) as usize;
    image.pixels()[i..i + 4].try_into().unwrap()
}

fn png_bytes(w: u32, h: u32) -> Vec<u8> {
    let img = image::RgbaImage::from_pixel(w, h, image::Rgba([1, 2, 3, 4]));
    let mut out = Cursor::new(Vec::new());
    img.write_to(&mut out, image::ImageFormat::Png).unwrap();
    out.into_inner()
}

#[test]
fn a_png_written_then_read_gives_back_the_same_pixels() {
    let tmp = tempfile::tempdir().unwrap();
    let mut pixels = vec![0u8; 32 * 32 * 4];
    pixels[0..4].copy_from_slice(&[255, 0, 0, 255]);
    pixels[100..104].copy_from_slice(&[10, 20, 30, 102]);
    pixels[200..204].copy_from_slice(&[9, 9, 9, 0]);
    let image = RgbaImage::from_pixels(32, 32, pixels).unwrap();
    let path = tmp.path().join("cross.png");
    write_png(&path, &image).unwrap();
    assert_eq!(read_png(&path).unwrap(), image);
}

#[test]
fn a_file_that_is_not_a_png_is_invalid() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("x.png");
    fs::write(&path, b"not a png").unwrap();
    assert!(matches!(read_png(&path), Err(PngReadError::Invalid { .. })));
}

#[test]
fn a_missing_file_is_an_io_error() {
    let tmp = tempfile::tempdir().unwrap();
    let got = read_png(&tmp.path().join("gone.png"));
    assert!(matches!(got, Err(PngReadError::Io { .. })), "{got:?}");
}

#[test]
fn a_picture_wider_or_taller_than_the_limit_is_too_large() {
    let tmp = tempfile::tempdir().unwrap();
    for (w, h) in [(PNG_MAX_SIDE + 1, 1), (1, PNG_MAX_SIDE + 1)] {
        let path = tmp.path().join(format!("{w}x{h}.png"));
        fs::write(&path, png_bytes(w, h)).unwrap();
        match read_png(&path) {
            Err(PngReadError::TooLarge { width, height }) => assert_eq!((width, height), (w, h)),
            other => panic!("{w}x{h}: {other:?}"),
        }
    }
    let path = tmp.path().join("edge.png");
    fs::write(&path, png_bytes(PNG_MAX_SIDE, 1)).unwrap();
    assert_eq!(read_png(&path).unwrap().width(), PNG_MAX_SIDE);
}

#[test]
fn a_file_over_the_byte_limit_is_refused_before_it_is_read() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("huge.png");
    fs::File::create(&path)
        .unwrap()
        .set_len(PNG_MAX_FILE_BYTES + 1)
        .unwrap();
    match read_png(&path) {
        Err(PngReadError::FileTooLarge { bytes }) => assert_eq!(bytes, PNG_MAX_FILE_BYTES + 1),
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_file_at_the_byte_limit_is_read() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("edge.png");
    fs::File::create(&path)
        .unwrap()
        .set_len(PNG_MAX_FILE_BYTES)
        .unwrap();
    // All zeros, so it is read in full and then fails to decode.
    let got = read_png(&path);
    assert!(matches!(got, Err(PngReadError::Invalid { .. })), "{got:?}");
}

/// True when `got` is the refusal of something that is not a regular file.
fn not_a_file(got: &Result<RgbaImage, PngReadError>) -> bool {
    matches!(got, Err(PngReadError::Io { source, .. })
        if source.kind() == std::io::ErrorKind::InvalidInput)
}

#[test]
fn a_folder_is_refused_as_not_a_file() {
    let tmp = tempfile::tempdir().unwrap();
    let got = read_png(tmp.path());
    assert!(not_a_file(&got), "{got:?}");
}

#[cfg(unix)]
#[test]
fn a_device_is_refused_as_not_a_file() {
    let got = read_png(std::path::Path::new("/dev/null"));
    assert!(not_a_file(&got), "{got:?}");
}

#[test]
fn writing_into_a_missing_folder_is_an_io_error() {
    let tmp = tempfile::tempdir().unwrap();
    let got = write_png(
        &tmp.path().join("missing/x.png"),
        &solid(32, 32, [1, 2, 3, 4]),
    );
    assert!(matches!(got, Err(PngWriteError::Io { .. })), "{got:?}");
}

#[test]
fn canvas_sizes_are_kept_as_they_are() {
    for side in [32, 64] {
        let image = solid(side, side, [1, 2, 3, 4]);
        assert_eq!(fit_to_maker(image.clone()), (image, FitChange::AsIs));
    }
}

#[test]
fn a_smaller_picture_sits_unscaled_in_the_middle_of_a_64_canvas() {
    let (out, change) = fit_to_maker(solid(10, 6, [0, 0, 255, 255]));
    assert_eq!(change, FitChange::Centred);
    assert_eq!((out.width(), out.height()), (64, 64));
    // (64 - 10) / 2 = 27 and (64 - 6) / 2 = 29.
    let inside = |x: u32, y: u32| (27..37).contains(&x) && (29..35).contains(&y);
    for y in 0..64 {
        for x in 0..64 {
            let want = if inside(x, y) {
                [0, 0, 255, 255]
            } else {
                [0, 0, 0, 0]
            };
            assert_eq!(at(&out, x, y), want, "({x}, {y})");
        }
    }
}

#[test]
fn a_32_by_64_picture_is_centred_not_scaled() {
    let (out, change) = fit_to_maker(solid(32, 64, [9, 9, 9, 255]));
    assert_eq!(change, FitChange::Centred);
    assert_eq!(at(&out, 15, 0), [0, 0, 0, 0]);
    assert_eq!(at(&out, 16, 0), [9, 9, 9, 255]);
    assert_eq!(at(&out, 47, 63), [9, 9, 9, 255]);
    assert_eq!(at(&out, 48, 63), [0, 0, 0, 0]);
}

#[test]
fn a_bigger_picture_is_shrunk_to_fit_keeping_its_shape() {
    let (out, change) = fit_to_maker(solid(128, 64, [200, 100, 50, 255]));
    assert_eq!(change, FitChange::ShrunkToFit);
    assert_eq!((out.width(), out.height()), (64, 64));
    // Shrunk to 64x32, so rows 16 to 47 hold it and the rest stays clear.
    assert_eq!(at(&out, 10, 5), [0, 0, 0, 0]);
    assert_eq!(at(&out, 10, 60), [0, 0, 0, 0]);
    assert!(at(&out, 32, 32)[3] >= 250, "{:?}", at(&out, 32, 32));
    let (square, change) = fit_to_maker(solid(65, 65, [1, 1, 1, 255]));
    assert_eq!(change, FitChange::ShrunkToFit);
    assert_eq!((square.width(), square.height()), (64, 64));
}
