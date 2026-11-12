//! Applies a `MediaEdit` to an image. Videos are trimmed by ffmpeg
//! (`probe::Tools::trim`).

use std::io::Cursor;

use image::codecs::jpeg::JpegEncoder;
use image::imageops::interpolate_bilinear;
use image::{DynamicImage, ImageBuffer, ImageFormat, Pixel};
use rayon::prelude::*;
use starbooru_core::{EditKind, MediaEdit};

use crate::error::{AppError, AppResult};
use crate::storage;

/// JPEG quality for an edited photo: high, as it is encoded a second time.
const JPEG_QUALITY: u8 = 92;

/// Turns, flips, straightens and crops the image in `bytes` and encodes the result in the
/// same format. The edit is checked against the upright image. CPU-bound:
/// call from a blocking thread.
pub fn edit_image(bytes: &[u8], mime: &str, edit: &MediaEdit) -> AppResult<Vec<u8>> {
    let img = storage::decode(bytes)?;
    edit.check(EditKind::Image, img.width(), img.height(), None)
        .map_err(|e| AppError::BadRequest(e.to_string()))?;
    let (rotate, flip_h, flip_v) = edit.orientation();
    let mut img = match rotate {
        90 => img.rotate90(),
        180 => img.rotate180(),
        270 => img.rotate270(),
        _ => img,
    };
    if flip_h {
        img = img.fliph();
    }
    if flip_v {
        img = img.flipv();
    }
    if edit.straighten != 0 {
        let scale = edit.straighten_scale(img.width(), img.height());
        img = straighten(img, f64::from(edit.straighten) / 10.0, scale);
    }
    if let Some(c) = edit.crop {
        img = img.crop_imm(c.x, c.y, c.width, c.height);
    }
    encode(&img, mime)
}

/// Turns `img` clockwise by `degrees` around its middle and enlarges it by
/// `scale`, keeping its size. Formats without a buffer of their own here
/// become 8-bit RGBA.
fn straighten(img: DynamicImage, degrees: f64, scale: f64) -> DynamicImage {
    use DynamicImage as D;
    match img {
        D::ImageLuma8(b) => D::ImageLuma8(turned(&b, degrees, scale)),
        D::ImageLumaA8(b) => D::ImageLumaA8(turned(&b, degrees, scale)),
        D::ImageRgb8(b) => D::ImageRgb8(turned(&b, degrees, scale)),
        D::ImageRgba8(b) => D::ImageRgba8(turned(&b, degrees, scale)),
        D::ImageLuma16(b) => D::ImageLuma16(turned(&b, degrees, scale)),
        D::ImageLumaA16(b) => D::ImageLumaA16(turned(&b, degrees, scale)),
        D::ImageRgb16(b) => D::ImageRgb16(turned(&b, degrees, scale)),
        D::ImageRgba16(b) => D::ImageRgba16(turned(&b, degrees, scale)),
        D::ImageRgb32F(b) => D::ImageRgb32F(turned(&b, degrees, scale)),
        D::ImageRgba32F(b) => D::ImageRgba32F(turned(&b, degrees, scale)),
        other => D::ImageRgba8(turned(&other.to_rgba8(), degrees, scale)),
    }
}

/// Each pixel of the result looks up where it came from: turned back by the
/// angle and shrunk by the scale, then read between the four nearest pixels.
fn turned<P>(
    src: &ImageBuffer<P, Vec<P::Subpixel>>,
    degrees: f64,
    scale: f64,
) -> ImageBuffer<P, Vec<P::Subpixel>>
where
    P: Pixel + Send + Sync,
    P::Subpixel: Send + Sync,
{
    let (w, h) = src.dimensions();
    let mut out = ImageBuffer::new(w, h);
    if w == 0 || h == 0 || !scale.is_finite() || scale <= 0.0 {
        return out;
    }
    let (sin, cos) = degrees.to_radians().sin_cos();
    let (cx, cy) = (f64::from(w) / 2.0, f64::from(h) / 2.0);
    let (last_x, last_y) = (f64::from(w - 1), f64::from(h - 1));
    out.par_enumerate_pixels_mut().for_each(|(x, y, px)| {
        let dx = f64::from(x) + 0.5 - cx;
        let dy = f64::from(y) + 0.5 - cy;
        // Rounding can put the edges a hair outside; they read the edge.
        let sx = ((dx * cos + dy * sin) / scale + cx - 0.5).clamp(0.0, last_x);
        let sy = ((-dx * sin + dy * cos) / scale + cy - 0.5).clamp(0.0, last_y);
        if let Some(p) = interpolate_bilinear(src, sx as f32, sy as f32) {
            *px = p;
        }
    });
    out
}

fn encode(img: &DynamicImage, mime: &str) -> AppResult<Vec<u8>> {
    let mut out = Cursor::new(Vec::new());
    let flat = |img: &DynamicImage| {
        if img.color().has_alpha() {
            DynamicImage::ImageRgba8(img.to_rgba8())
        } else {
            DynamicImage::ImageRgb8(img.to_rgb8())
        }
    };
    let written = match mime {
        "image/jpeg" => JpegEncoder::new_with_quality(&mut out, JPEG_QUALITY)
            .encode_image(&DynamicImage::ImageRgb8(img.to_rgb8())),
        "image/png" => img.write_to(&mut out, ImageFormat::Png),
        "image/tiff" => img.write_to(&mut out, ImageFormat::Tiff),
        // Both take 8-bit RGB(A) only; WebP is written lossless.
        "image/webp" => flat(img).write_to(&mut out, ImageFormat::WebP),
        "image/bmp" => flat(img).write_to(&mut out, ImageFormat::Bmp),
        _ => {
            return Err(AppError::BadRequest(
                starbooru_core::EditError::NotEditable.to_string(),
            ));
        }
    };
    written.map_err(AppError::internal)?;
    Ok(out.into_inner())
}

#[cfg(test)]
mod tests {
    use image::{GenericImageView, Rgb, RgbImage};
    use starbooru_core::Crop;

    use super::*;

    /// 4 × 2: red on the left half, blue on the right; the top-left pixel green.
    fn sample() -> DynamicImage {
        let mut img = RgbImage::from_fn(4, 2, |x, _| {
            if x < 2 {
                Rgb([255, 0, 0])
            } else {
                Rgb([0, 0, 255])
            }
        });
        img.put_pixel(0, 0, Rgb([0, 255, 0]));
        DynamicImage::ImageRgb8(img)
    }

    fn png(img: &DynamicImage) -> Vec<u8> {
        let mut out = Cursor::new(Vec::new());
        img.write_to(&mut out, ImageFormat::Png).unwrap();
        out.into_inner()
    }

    fn pixel(img: &DynamicImage, x: u32, y: u32) -> [u8; 3] {
        let p = img.get_pixel(x, y).0;
        [p[0], p[1], p[2]]
    }

    #[test]
    fn turns_flips_and_crops() {
        let src = png(&sample());
        let edit = MediaEdit {
            rotate: 90,
            ..MediaEdit::default()
        };
        let out = storage::decode(&edit_image(&src, "image/png", &edit).unwrap()).unwrap();
        assert_eq!(out.dimensions(), (2, 4));
        // Clockwise: the top-left corner moves to the top right.
        assert_eq!(pixel(&out, 1, 0), [0, 255, 0]);

        let edit = MediaEdit {
            flip_h: true,
            crop: Some(Crop {
                x: 2,
                y: 0,
                width: 2,
                height: 1,
            }),
            ..MediaEdit::default()
        };
        let out = storage::decode(&edit_image(&src, "image/png", &edit).unwrap()).unwrap();
        assert_eq!(out.dimensions(), (2, 1));
        assert_eq!(pixel(&out, 0, 0), [255, 0, 0]);
        assert_eq!(pixel(&out, 1, 0), [0, 255, 0]);

        // Both flips are a half turn.
        let edit = MediaEdit {
            flip_h: true,
            flip_v: true,
            ..MediaEdit::default()
        };
        let out = storage::decode(&edit_image(&src, "image/png", &edit).unwrap()).unwrap();
        assert_eq!(pixel(&out, 3, 1), [0, 255, 0]);
    }

    #[test]
    fn straightens_and_fills_the_frame() {
        // Left half red, right half blue: turned a little, the frame still
        // has the same size, no empty corners, and red stays on the left.
        let img = DynamicImage::ImageRgb8(RgbImage::from_fn(40, 20, |x, _| {
            if x < 20 {
                Rgb([255, 0, 0])
            } else {
                Rgb([0, 0, 255])
            }
        }));
        let edit = MediaEdit {
            straighten: 100,
            ..MediaEdit::default()
        };
        let out = storage::decode(&edit_image(&png(&img), "image/png", &edit).unwrap()).unwrap();
        assert_eq!(out.dimensions(), (40, 20));
        for (x, y) in [(0, 0), (39, 0), (0, 19), (39, 19)] {
            let [r, g, b] = pixel(&out, x, y);
            assert!(r + g + b > 200, "corner {x},{y} is empty: {r} {g} {b}");
        }
        assert_eq!(pixel(&out, 2, 10), [255, 0, 0]);
        assert_eq!(pixel(&out, 37, 10), [0, 0, 255]);
        // Clockwise: the line between the halves leans right at the top, so
        // red reaches past the middle there and blue at the bottom.
        assert_eq!(pixel(&out, 20, 0), [255, 0, 0]);
        assert_eq!(pixel(&out, 19, 19), [0, 0, 255]);
        // Formats without a buffer of their own still come out.
        let odd = storage::decode(&png(&DynamicImage::ImageLumaA16(
            image::ImageBuffer::from_pixel(5, 3, image::LumaA([9000_u16, 65535])),
        )))
        .unwrap();
        let _ = straighten(odd, 45.0, 2.0);
        let empty = DynamicImage::ImageRgb8(RgbImage::new(0, 0));
        assert_eq!(straighten(empty, 10.0, 1.2).width(), 0);
    }

    #[test]
    fn keeps_the_format() {
        let img = sample();
        let edit = MediaEdit {
            rotate: 180,
            ..MediaEdit::default()
        };
        for (mime, format) in [
            ("image/jpeg", ImageFormat::Jpeg),
            ("image/webp", ImageFormat::WebP),
            ("image/bmp", ImageFormat::Bmp),
            ("image/tiff", ImageFormat::Tiff),
        ] {
            let mut src = Cursor::new(Vec::new());
            DynamicImage::ImageRgb8(img.to_rgb8())
                .write_to(&mut src, format)
                .unwrap();
            let out = edit_image(src.get_ref(), mime, &edit).unwrap();
            assert_eq!(image::guess_format(&out).unwrap(), format, "{mime}");
        }
    }

    #[test]
    fn refuses_bad_edits() {
        let src = png(&sample());
        let outside = MediaEdit {
            crop: Some(Crop {
                x: 3,
                y: 0,
                width: 2,
                height: 1,
            }),
            ..MediaEdit::default()
        };
        assert!(matches!(
            edit_image(&src, "image/png", &outside),
            Err(AppError::BadRequest(_))
        ));
        assert!(matches!(
            edit_image(&src, "image/png", &MediaEdit::default()),
            Err(AppError::BadRequest(_))
        ));
        assert!(matches!(
            edit_image(b"not an image", "image/png", &outside),
            Err(AppError::BadRequest(_))
        ));
    }
}
