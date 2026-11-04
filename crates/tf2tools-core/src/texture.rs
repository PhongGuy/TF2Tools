//! VTF textures: decoding a crosshair to RGBA pixels and building a crosshair VTF
//! from a PNG or from raw pixels. Calls into the vtf crate run under
//! `panic_mark::catching`, since it panics on truncated input.

use std::fs::File;
use std::io::{Cursor, ErrorKind, Read as _};
use std::path::Path;

use image::{DynamicImage, ImageDecoder as _, ImageFormat, ImageReader, imageops};

use crate::error::{PngReadError, PngWriteError, VtfError};
use crate::panic_mark::catching;

/// TF2 crosshairs are 64x64; PNGs are fitted onto a canvas of this size.
const CANVAS: u32 = 64;

/// A VTF 7.2 header is 80 bytes, padding included.
const VTF_HEADER_SIZE: usize = 80;

/// Point sample, clamp S and T, no mipmaps, no LOD, eight-bit alpha (0x230D).
const CROSSHAIR_FLAGS: u32 = 0x0000_230D;

/// HUD picture VTF flags: clamp S and T, no mip, no LOD, eight-bit alpha.
const HUD_TEXTURE_FLAGS: u32 = 0x0000_230C;

/// The largest side and area a HUD texture is written at.
const HUD_MAX_SIDE: u32 = 2048;
const HUD_MAX_AREA: u64 = 2048 * 1024;

/// The VTF image format number of BGRA8888.
const FORMAT_BGRA8888: i32 = 12;

/// The widest or tallest picture `read_png` accepts.
pub const PNG_MAX_SIDE: u32 = 4096;

/// The largest file `read_png` reads.
pub const PNG_MAX_FILE_BYTES: u64 = 32 * 1024 * 1024;

/// What `fit_to_maker` did to the picture.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum FitChange {
    /// A 32x32 or 64x64 picture, kept as it was.
    AsIs,
    /// Placed unscaled in the middle of a clear 64x64 canvas.
    Centred,
    /// Shrunk to fit 64x64 keeping its shape, then centred.
    ShrunkToFit,
}

/// A decoded image: `width * height` pixels, 4 bytes each in R, G, B, A order, rows top to bottom.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct RgbaImage {
    width: u32,
    height: u32,
    pixels: Vec<u8>,
}

impl RgbaImage {
    /// Width in pixels.
    pub fn width(&self) -> u32 {
        self.width
    }

    /// Height in pixels.
    pub fn height(&self) -> u32 {
        self.height
    }

    /// The RGBA bytes, row by row from the top.
    pub fn pixels(&self) -> &[u8] {
        &self.pixels
    }

    /// Takes the RGBA bytes, row by row from the top.
    pub fn into_pixels(self) -> Vec<u8> {
        self.pixels
    }

    /// Wraps RGBA bytes, row by row from the top. None when `pixels.len()` is not
    /// `width * height * 4` or either side is 0.
    pub fn from_pixels(width: u32, height: u32, pixels: Vec<u8>) -> Option<RgbaImage> {
        let expected = usize::try_from(width)
            .ok()?
            .checked_mul(usize::try_from(height).ok()?)?
            .checked_mul(4)?;
        (width > 0 && height > 0 && pixels.len() == expected).then_some(RgbaImage {
            width,
            height,
            pixels,
        })
    }
}

/// Encodes `image` as an uncompressed crosshair VTF: version 7.2, an 80-byte header, flags
/// 0x230D, BGRA8888, one mip, one frame, no low-res image. Errors with `VtfError::Encode`
/// when a side exceeds 65535.
pub fn rgba_to_vtf(image: &RgbaImage) -> Result<Vec<u8>, VtfError> {
    encode_bgra(image, CROSSHAIR_FLAGS)
}

/// Encodes `image` as a HUD texture: `rgba_to_vtf`'s layout with flags 0x230C, so a stretched
/// picture does not bleed at its edges. Errors as `rgba_to_vtf`.
pub fn hud_texture_vtf(image: &RgbaImage) -> Result<Vec<u8>, VtfError> {
    encode_bgra(image, HUD_TEXTURE_FLAGS)
}

fn encode_bgra(image: &RgbaImage, flags: u32) -> Result<Vec<u8>, VtfError> {
    let (Ok(width), Ok(height)) = (u16::try_from(image.width), u16::try_from(image.height)) else {
        return Err(VtfError::Encode {
            message: format!(
                "a {}x{} image is too large for a VTF",
                image.width, image.height
            ),
        });
    };
    // The header follows the VTF 7.2 layout field by field. Point sampling, clamping
    // and no mips keep a small crosshair crisp, and with no low-res image the texels start at 80.
    let mut out = Vec::with_capacity(VTF_HEADER_SIZE + image.pixels.len());
    out.extend_from_slice(b"VTF\0");
    out.extend_from_slice(&7u32.to_le_bytes());
    out.extend_from_slice(&2u32.to_le_bytes());
    out.extend_from_slice(&(VTF_HEADER_SIZE as u32).to_le_bytes());
    out.extend_from_slice(&width.to_le_bytes());
    out.extend_from_slice(&height.to_le_bytes());
    out.extend_from_slice(&flags.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes()); // frames
    out.extend_from_slice(&0u16.to_le_bytes()); // first frame
    out.extend_from_slice(&[0; 4]);
    for _ in 0..3 {
        out.extend_from_slice(&0f32.to_le_bytes()); // reflectivity
    }
    out.extend_from_slice(&[0; 4]);
    out.extend_from_slice(&1f32.to_le_bytes()); // bump scale
    out.extend_from_slice(&FORMAT_BGRA8888.to_le_bytes());
    out.push(1); // mip levels
    out.extend_from_slice(&(-1i32).to_le_bytes()); // no low-res image
    out.extend_from_slice(&[0, 0]); // low-res width and height
    out.extend_from_slice(&1u16.to_le_bytes()); // depth
    out.resize(VTF_HEADER_SIZE, 0);
    out.extend(
        image
            .pixels
            .as_chunks::<4>()
            .0
            .iter()
            .flat_map(|&[r, g, b, a]| [b, g, r, a]),
    );
    Ok(out)
}

/// Decodes frame 0 of the full-resolution image of a VTF.
///
/// Fails with `UnsupportedFormat` for pixel formats the decoder lacks and with
/// `Malformed` for anything that does not parse, including truncated files.
pub fn decode_vtf(bytes: &[u8]) -> Result<RgbaImage, VtfError> {
    match catching(|| decode_unguarded(bytes)) {
        Ok(Ok(image)) => Ok(image),
        Ok(Err(vtf::Error::UnsupportedImageFormat(format))) => Err(VtfError::UnsupportedFormat {
            format: format.to_string(),
        }),
        Ok(Err(e)) => Err(VtfError::Malformed {
            message: e.to_string(),
        }),
        Err(_) => Err(VtfError::Malformed {
            message: "the VTF parser panicked".into(),
        }),
    }
}

fn decode_unguarded(bytes: &[u8]) -> Result<RgbaImage, vtf::Error> {
    let file = vtf::from_bytes(bytes)?;
    let image = &file.highres_image;
    let (width, height) = (u32::from(image.width), u32::from(image.height));
    // The vtf crate sizes ABGR8888, I8, IA88 and A8 frames but does not decode them, so
    // expand the raw frame here: luminance becomes grey, a bare alpha channel is black.
    use vtf::image::ImageFormat as F;
    let pixels = match image.format {
        F::Abgr8888 => image
            .get_frame(0)?
            .as_chunks::<4>()
            .0
            .iter()
            .flat_map(|&[a, b, g, r]| [r, g, b, a])
            .collect(),
        F::I8 => image
            .get_frame(0)?
            .iter()
            .flat_map(|&i| [i, i, i, 255])
            .collect(),
        F::Ia88 => image
            .get_frame(0)?
            .as_chunks::<2>()
            .0
            .iter()
            .flat_map(|&[i, a]| [i, i, i, a])
            .collect(),
        F::A8 => image
            .get_frame(0)?
            .iter()
            .flat_map(|&a| [0, 0, 0, a])
            .collect(),
        _ => image.decode(0)?.to_rgba8().into_raw(),
    };
    Ok(RgbaImage {
        width,
        height,
        pixels,
    })
}

/// The width and height in a VTF header, read without parsing the rest; `None` when the
/// bytes are too short or do not start with `VTF\0`.
pub(crate) fn vtf_size(bytes: &[u8]) -> Option<(u32, u32)> {
    let side = |at: usize| {
        Some(u32::from(u16::from_le_bytes(
            bytes.get(at..at + 2)?.try_into().ok()?,
        )))
    };
    bytes.starts_with(b"VTF\0").then_some(())?;
    Some((side(16)?, side(18)?))
}

/// Converts a PNG to an RGBA8888 crosshair VTF. The image is scaled with Lanczos3
/// to fit 64x64 keeping its aspect ratio and centred on a transparent canvas.
///
/// Fails with `InvalidPng` when the input is not a PNG and `Encode` when the VTF
/// cannot be written.
pub fn png_to_vtf(png: &[u8]) -> Result<Vec<u8>, VtfError> {
    let source = image::load_from_memory_with_format(png, ImageFormat::Png).map_err(|e| {
        VtfError::InvalidPng {
            message: e.to_string(),
        }
    })?;
    let canvas = fit_to_canvas(source.to_rgba8());
    match catching(|| {
        vtf::create(
            DynamicImage::ImageRgba8(canvas),
            vtf::image::ImageFormat::Rgba8888,
        )
    }) {
        Ok(Ok(bytes)) => Ok(bytes),
        Ok(Err(e)) => Err(VtfError::Encode {
            message: e.to_string(),
        }),
        Err(_) => Err(VtfError::Encode {
            message: "the VTF encoder panicked".into(),
        }),
    }
}

/// Reads a PNG file into straight RGBA pixels.
///
/// Fails with `FileTooLarge` over `PNG_MAX_FILE_BYTES` before reading, `TooLarge` when a side
/// is over `PNG_MAX_SIDE` (checked from the header alone), `Invalid` when it does not decode
/// as a PNG and `Io` when the file cannot be read or is not a regular file (kind
/// `InvalidInput`).
pub fn read_png(path: &Path) -> Result<RgbaImage, PngReadError> {
    read_image(path, Accept::Png)
}

/// Reads a PNG or JPEG file (format guessed from its bytes) into straight RGBA pixels, turned
/// upright by its EXIF orientation. Same limits and errors as `read_png`; any other format is
/// `Invalid`.
pub fn read_picture(path: &Path) -> Result<RgbaImage, PngReadError> {
    read_image(path, Accept::Picture)
}

/// Which formats a file read accepts.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Accept {
    /// PNG only, as stored.
    Png,
    /// PNG or JPEG, upright.
    Picture,
}

fn read_image(path: &Path, accept: Accept) -> Result<RgbaImage, PngReadError> {
    let io = |source| PngReadError::Io {
        path: path.to_path_buf(),
        source,
    };
    let not_a_file = || {
        io(std::io::Error::new(
            ErrorKind::InvalidInput,
            "not a regular file",
        ))
    };
    // Refused before opening, since opening a pipe blocks; then checked again on the handle,
    // and read through a cap so a file that grows after the check is still refused.
    if !std::fs::metadata(path).map_err(io)?.is_file() {
        return Err(not_a_file());
    }
    let file = File::open(path).map_err(io)?;
    let meta = file.metadata().map_err(io)?;
    if !meta.is_file() {
        return Err(not_a_file());
    }
    let bytes = meta.len();
    if bytes > PNG_MAX_FILE_BYTES {
        return Err(PngReadError::FileTooLarge { bytes });
    }
    let mut data = Vec::new();
    file.take(PNG_MAX_FILE_BYTES + 1)
        .read_to_end(&mut data)
        .map_err(io)?;
    let read = data.len() as u64;
    if read > PNG_MAX_FILE_BYTES {
        return Err(PngReadError::FileTooLarge { bytes: read });
    }
    let invalid = |e: image::ImageError| PngReadError::Invalid {
        message: e.to_string(),
    };
    let reader = match accept {
        Accept::Png => ImageReader::with_format(Cursor::new(&data), ImageFormat::Png),
        Accept::Picture => {
            let guessed = ImageReader::new(Cursor::new(&data))
                .with_guessed_format()
                .map_err(|e| invalid(e.into()))?;
            match guessed.format() {
                Some(ImageFormat::Png | ImageFormat::Jpeg) => guessed,
                _ => {
                    return Err(PngReadError::Invalid {
                        message: "not a PNG or JPEG picture".into(),
                    });
                }
            }
        }
    };
    // Check the sides from the header first, so a huge picture is never decoded; the
    // orientation turns the picture only after the limit holds on the stored sides.
    let mut decoder = reader.into_decoder().map_err(invalid)?;
    let (width, height) = decoder.dimensions();
    if width > PNG_MAX_SIDE || height > PNG_MAX_SIDE {
        return Err(PngReadError::TooLarge { width, height });
    }
    let orientation = match accept {
        Accept::Png => None,
        Accept::Picture => Some(decoder.orientation().map_err(invalid)?),
    };
    let mut decoded = DynamicImage::from_decoder(decoder).map_err(invalid)?;
    if let Some(orientation) = orientation {
        decoded.apply_orientation(orientation);
    }
    let rgba = decoded.to_rgba8();
    let (width, height) = rgba.dimensions();
    RgbaImage::from_pixels(width, height, rgba.into_raw()).ok_or_else(|| PngReadError::Invalid {
        message: "the decoded picture is empty".into(),
    })
}

/// Writes `image` to `path` as a straight-RGBA PNG.
///
/// Fails with `Encode` when the encoder rejects it and `Io` when the file cannot be written.
pub fn write_png(path: &Path, image: &RgbaImage) -> Result<(), PngWriteError> {
    let buffer = image::RgbaImage::from_raw(image.width, image.height, image.pixels.clone())
        .ok_or_else(|| PngWriteError::Encode {
            message: "the pixel buffer does not match the size".into(),
        })?;
    let mut out = Cursor::new(Vec::new());
    buffer
        .write_to(&mut out, ImageFormat::Png)
        .map_err(|e| PngWriteError::Encode {
            message: e.to_string(),
        })?;
    std::fs::write(path, out.into_inner()).map_err(|source| PngWriteError::Io {
        path: path.to_path_buf(),
        source,
    })
}

/// Fits any picture to the maker: 32x32 and 64x64 as is; else onto a clear 64x64, unscaled
/// and centred when both sides fit, else shrunk with Lanczos3 keeping its shape and centred.
pub fn fit_to_maker(image: RgbaImage) -> (RgbaImage, FitChange) {
    let (w, h) = (image.width, image.height);
    if w == h && (w == 32 || w == CANVAS) {
        return (image, FitChange::AsIs);
    }
    // `RgbaImage` always holds width * height * 4 bytes, so the copy fills the buffer exactly.
    let mut source = image::RgbaImage::new(w, h);
    source.copy_from_slice(&image.pixels);
    let (canvas, change) = if w <= CANVAS && h <= CANVAS {
        // `replace` copies exactly; `overlay` would blend and alter see-through texels.
        let mut canvas = image::RgbaImage::new(CANVAS, CANVAS);
        let (x, y) = ((CANVAS - w) / 2, (CANVAS - h) / 2);
        imageops::replace(&mut canvas, &source, i64::from(x), i64::from(y));
        (canvas, FitChange::Centred)
    } else {
        (fit_to_canvas(source), FitChange::ShrunkToFit)
    };
    let fitted = RgbaImage {
        width: CANVAS,
        height: CANVAS,
        pixels: canvas.into_raw(),
    };
    (fitted, change)
}

/// Scales the image to fit the canvas without stretching (never `resize_exact`)
/// and centres it on transparency; an exact 64x64 image is kept as is.
fn fit_to_canvas(rgba: image::RgbaImage) -> image::RgbaImage {
    let (w, h) = rgba.dimensions();
    if (w, h) == (CANVAS, CANVAS) {
        return rgba;
    }
    let scale = f64::min(
        f64::from(CANVAS) / f64::from(w),
        f64::from(CANVAS) / f64::from(h),
    );
    // Both sides are at most 64 after scaling, so the casts cannot overflow.
    let fit = |side: u32| ((f64::from(side) * scale) as u32).clamp(1, CANVAS);
    let (new_w, new_h) = (fit(w), fit(h));
    let resized = imageops::resize(&rgba, new_w, new_h, imageops::FilterType::Lanczos3);
    let mut canvas = image::RgbaImage::new(CANVAS, CANVAS);
    imageops::overlay(
        &mut canvas,
        &resized,
        i64::from((CANVAS - new_w) / 2),
        i64::from((CANVAS - new_h) / 2),
    );
    canvas
}

/// Sizes a picture as a HUD texture: like `like` when given, else like the picture; each side
/// goes to the nearest power of two (a tie rounds up, at most 2048), then the longer side (both
/// when equal) halves while the area is over 2048 x 1024. Stretched with Catmull-Rom.
pub fn fit_hud_texture(image: &RgbaImage, like: Option<(u32, u32)>) -> RgbaImage {
    let (w, h) = like.unwrap_or((image.width, image.height));
    let (mut w, mut h) = (nearest_power_of_two(w), nearest_power_of_two(h));
    while u64::from(w) * u64::from(h) > HUD_MAX_AREA {
        match w.cmp(&h) {
            std::cmp::Ordering::Greater => w /= 2,
            std::cmp::Ordering::Less => h /= 2,
            std::cmp::Ordering::Equal => (w, h) = (w / 2, h / 2),
        }
    }
    stretch(&as_buffer(image), w, h)
}

/// Centre crops of `image` covering a 4:3 and a 16:9 frame, stretched to the menu background
/// textures: (1024x1024, 2048x1024).
pub fn menu_textures(image: &RgbaImage) -> (RgbaImage, RgbaImage) {
    let source = as_buffer(image);
    let crop = |aspect_w: u64, aspect_h: u64, out_w: u32, out_h: u32| {
        let (w, h) = (u64::from(image.width), u64::from(image.height));
        // Keep the full height when the picture is wider than the frame, else the full width.
        let (cw, ch) = if w * aspect_h > h * aspect_w {
            ((h * aspect_w / aspect_h).max(1), h)
        } else {
            (w, (w * aspect_h / aspect_w).max(1))
        };
        // Both crop sides are at most the picture's own, so they fit in u32.
        let (cw, ch) = (cw as u32, ch as u32);
        let (x, y) = ((image.width - cw) / 2, (image.height - ch) / 2);
        let cropped = imageops::crop_imm(&source, x, y, cw, ch).to_image();
        stretch(&cropped, out_w, out_h)
    };
    (crop(4, 3, 1024, 1024), crop(16, 9, 2048, 1024))
}

/// The power of two nearest `side` (a tie rounds up), between 1 and 2048.
fn nearest_power_of_two(side: u32) -> u32 {
    let side = side.clamp(1, HUD_MAX_SIDE);
    let below = 1 << side.ilog2();
    if side == below {
        return below;
    }
    let above = below * 2;
    if side - below < above - side {
        below
    } else {
        above
    }
}

fn as_buffer(image: &RgbaImage) -> image::RgbaImage {
    // `RgbaImage` always holds width * height * 4 bytes, so the copy fills the buffer exactly.
    let mut buffer = image::RgbaImage::new(image.width, image.height);
    buffer.copy_from_slice(&image.pixels);
    buffer
}

fn stretch(source: &image::RgbaImage, w: u32, h: u32) -> RgbaImage {
    let resized = imageops::resize(source, w, h, imageops::FilterType::CatmullRom);
    RgbaImage {
        width: w,
        height: h,
        pixels: resized.into_raw(),
    }
}
