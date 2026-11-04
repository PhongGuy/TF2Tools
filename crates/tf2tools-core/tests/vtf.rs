//! VTF decoding of the bundled crosshairs and PNG to VTF conversion.

use std::io::Cursor;
use std::path::{Path, PathBuf};
use tf2tools_core::{RgbaImage, VtfError, decode_vtf, png_to_vtf, rgba_to_vtf};

fn crosshairs() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/crosshairs")
}

fn bundled_vtf(name: &str) -> Vec<u8> {
    std::fs::read(crosshairs().join("vtf").join(format!("{name}.vtf"))).unwrap()
}

fn fnv1a(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |hash, &b| {
        (hash ^ u64::from(b)).wrapping_mul(0x0000_0100_0000_01b3)
    })
}

fn png(image: &image::RgbaImage) -> Vec<u8> {
    let mut out = Cursor::new(Vec::new());
    image.write_to(&mut out, image::ImageFormat::Png).unwrap();
    out.into_inner()
}

#[test]
fn builtins_decode_to_known_pixels() {
    let crossdot = decode_vtf(&bundled_vtf("Crossdot")).unwrap();
    assert_eq!((crossdot.width(), crossdot.height()), (64, 64));
    assert_eq!(crossdot.pixels().len(), 64 * 64 * 4);
    assert_eq!(fnv1a(crossdot.pixels()), 0xe55c_ffd3_31d5_a432);
    assert_eq!(
        crossdot.pixels().chunks(4).filter(|p| p[3] > 0).count(),
        200
    );

    let cpma = decode_vtf(&bundled_vtf("cpma_9")).unwrap();
    assert_eq!((cpma.width(), cpma.height()), (32, 32));
    assert_eq!(fnv1a(cpma.pixels()), 0xa5aa_a174_72a0_6f65);
}

#[test]
fn abgr8888_builtins_decode_like_their_v5_previews() {
    for name in ["Brackets_TobYZ", "crosshair4"] {
        let decoded = decode_vtf(&bundled_vtf(name)).unwrap();
        let preview = image::open(crosshairs().join("png").join(format!("{name}.png")))
            .unwrap()
            .to_rgba8();
        assert_eq!(
            (decoded.width(), decoded.height()),
            preview.dimensions(),
            "{name}"
        );
        assert!(
            decoded.pixels() == preview.as_raw().as_slice(),
            "{name} pixels differ"
        );
    }
}

#[test]
fn every_bundled_vtf_decodes() {
    let mut count = 0;
    for entry in std::fs::read_dir(crosshairs().join("vtf")).unwrap() {
        let path = entry.unwrap().path();
        let image = decode_vtf(&std::fs::read(&path).unwrap())
            .unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        assert_eq!(
            image.pixels().len() as u64,
            u64::from(image.width() * image.height()) * 4
        );
        count += 1;
    }
    assert_eq!(count, 405);
}

#[test]
fn truncated_vtf_is_malformed_without_panicking() {
    let full = bundled_vtf("Crossdot");
    for len in [0, 3, 16, 64, 80, full.len() / 2, full.len() - 1] {
        let err = decode_vtf(&full[..len]).unwrap_err();
        assert!(
            matches!(err, VtfError::Malformed { .. }),
            "len {len}: {err:?}"
        );
    }
    let err = decode_vtf(b"not a texture").unwrap_err();
    assert!(matches!(err, VtfError::Malformed { .. }), "{err:?}");
}

#[test]
fn a_64_square_png_round_trips_exactly() {
    let source = image::RgbaImage::from_fn(64, 64, |x, y| {
        image::Rgba([
            (x * 3) as u8,
            (y * 4) as u8,
            (x ^ y) as u8,
            ((x + y) * 2) as u8,
        ])
    });
    let decoded = decode_vtf(&png_to_vtf(&png(&source)).unwrap()).unwrap();
    assert_eq!((decoded.width(), decoded.height()), (64, 64));
    assert!(decoded.pixels() == source.as_raw().as_slice());
}

#[test]
fn a_small_wide_png_is_scaled_up_and_centred() {
    let red = image::RgbaImage::from_pixel(32, 16, image::Rgba([255, 0, 0, 255]));
    let decoded = decode_vtf(&png_to_vtf(&png(&red)).unwrap()).unwrap();
    assert_eq!((decoded.width(), decoded.height()), (64, 64));
    for (i, pixel) in decoded.pixels().chunks(4).enumerate() {
        let row = i / 64;
        let expected: &[u8] = if (16..48).contains(&row) {
            &[255, 0, 0, 255]
        } else {
            &[0, 0, 0, 0]
        };
        assert_eq!(pixel, expected, "pixel {i} (row {row})");
    }
}

#[test]
fn a_large_png_is_scaled_down() {
    let blue = image::RgbaImage::from_pixel(200, 100, image::Rgba([0, 0, 255, 255]));
    let decoded = decode_vtf(&png_to_vtf(&png(&blue)).unwrap()).unwrap();
    let alpha_at = |x: usize, y: usize| decoded.pixels()[(y * 64 + x) * 4 + 3];
    assert_eq!(alpha_at(32, 15), 0);
    assert!(alpha_at(32, 16) > 0);
    let centre = &decoded.pixels()[(32 * 64 + 32) * 4..][..4];
    assert!(centre[2] >= 250 && centre[3] >= 250, "{centre:?}");
}

#[test]
fn non_png_input_is_rejected() {
    let err = png_to_vtf(b"GIF89a\x01\x00\x01\x00").unwrap_err();
    assert!(matches!(err, VtfError::InvalidPng { .. }), "{err:?}");
}

/// The 80-byte header of a 64x64 crosshair, from the VTF 7.2 layout.
const HEADER_64: [u8; 80] = [
    0x56, 0x54, 0x46, 0x00, 0x07, 0x00, 0x00, 0x00, 0x02, 0x00, 0x00, 0x00, 0x50, 0x00, 0x00, 0x00,
    0x40, 0x00, 0x40, 0x00, 0x0D, 0x23, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x80, 0x3F, 0x0C, 0x00, 0x00, 0x00, 0x01, 0xFF, 0xFF, 0xFF, 0xFF, 0x00, 0x00, 0x01,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
];

/// Every texel differs and every channel differs within a texel, so a swizzle or a flip shows.
fn pattern(width: u32, height: u32) -> RgbaImage {
    let pixels = (0..width * height)
        .flat_map(|i| {
            [
                i as u8,
                ((i >> 8) as u8) ^ 0x5A,
                (i * 3) as u8,
                (i * 7 + 1) as u8,
            ]
        })
        .collect();
    RgbaImage::from_pixels(width, height, pixels).unwrap()
}

#[test]
fn from_pixels_wants_four_bytes_per_texel_and_no_empty_side() {
    assert!(RgbaImage::from_pixels(2, 3, vec![0; 24]).is_some());
    assert!(RgbaImage::from_pixels(2, 3, vec![0; 23]).is_none());
    assert!(RgbaImage::from_pixels(2, 3, vec![0; 25]).is_none());
    assert!(RgbaImage::from_pixels(0, 3, Vec::new()).is_none());
    assert!(RgbaImage::from_pixels(3, 0, Vec::new()).is_none());
    let image = RgbaImage::from_pixels(1, 1, vec![1, 2, 3, 4]).unwrap();
    assert_eq!(
        (image.width(), image.height(), image.pixels()),
        (1, 1, &[1u8, 2, 3, 4][..])
    );
}

#[test]
fn the_crosshair_vtf_header_is_pinned_byte_for_byte() {
    let big = rgba_to_vtf(&pattern(64, 64)).unwrap();
    assert_eq!(big.len(), 16_464);
    assert_eq!(&big[..80], &HEADER_64[..]);
    let small = rgba_to_vtf(&pattern(32, 32)).unwrap();
    assert_eq!(small.len(), 80 + 32 * 32 * 4);
    let mut header_32 = HEADER_64;
    header_32[16..20].copy_from_slice(&[32, 0, 32, 0]);
    assert_eq!(&small[..80], &header_32[..]);
}

// collaide_dot is a 7.2 crosshair TF2 already ships with no low-res image; ours differs from it
// only in flags, reflectivity and pixel format.
#[test]
fn the_header_matches_a_shipped_crosshair_but_for_flags_reflectivity_and_format() {
    let shipped = bundled_vtf("collaide_dot");
    let ours = rgba_to_vtf(&pattern(64, 64)).unwrap();
    let mut expected = shipped[..80].to_vec();
    expected[20..24].copy_from_slice(&0x230Du32.to_le_bytes());
    expected[32..44].fill(0);
    expected[52..56].copy_from_slice(&12i32.to_le_bytes());
    assert_eq!(&ours[..80], &expected[..]);
    assert_eq!(ours.len(), shipped.len());
}

#[test]
fn texels_are_stored_bgra_top_row_first() {
    let image = RgbaImage::from_pixels(2, 2, (1..=16).collect()).unwrap();
    let bytes = rgba_to_vtf(&image).unwrap();
    assert_eq!(
        &bytes[80..],
        &[3, 2, 1, 4, 7, 6, 5, 8, 11, 10, 9, 12, 15, 14, 13, 16]
    );
}

#[test]
fn both_decoders_read_the_written_vtf_back_exactly() {
    for (w, h) in [(64, 64), (32, 32), (3, 5)] {
        let image = pattern(w, h);
        let bytes = rgba_to_vtf(&image).unwrap();
        assert!(decode_vtf(&bytes).unwrap() == image, "decode_vtf {w}x{h}");
        let file = vtf::from_bytes(&bytes).unwrap();
        assert_eq!(file.header.version, [7, 2]);
        assert_eq!(file.header.header_size, 80);
        assert_eq!(file.header.flags, 0x230D);
        assert_eq!(file.header.mipmap_count, 1);
        assert_eq!(file.header.frames, 1);
        assert_eq!(file.header.highres_image_format, vtf::ImageFormat::Bgra8888);
        assert_eq!(file.header.lowres_image_format, vtf::ImageFormat::None);
        let pixels = file.highres_image.decode(0).unwrap().to_rgba8().into_raw();
        assert!(pixels == image.pixels(), "vtf crate {w}x{h}");
    }
}

#[test]
fn a_side_over_65535_is_an_encode_error() {
    for (w, h) in [(65_536, 1), (1, 65_536)] {
        let image = RgbaImage::from_pixels(w, h, vec![0; 65_536 * 4]).unwrap();
        let err = rgba_to_vtf(&image).unwrap_err();
        assert!(matches!(err, VtfError::Encode { .. }), "{w}x{h}: {err:?}");
    }
    let widest = RgbaImage::from_pixels(65_535, 1, vec![0; 65_535 * 4]).unwrap();
    assert!(rgba_to_vtf(&widest).is_ok());
}
