//! Applies a `MediaEdit` to an image. Videos are trimmed by ffmpeg
//! (`probe::Tools::trim`).

use std::io::Cursor;

use image::codecs::jpeg::JpegEncoder;
use image::{DynamicImage, ImageFormat};
use starbooru_core::{EditKind, MediaEdit};

use crate::error::{AppError, AppResult};
use crate::storage;

/// JPEG quality for an edited photo: high, as it is encoded a second time.
const JPEG_QUALITY: u8 = 92;

/// Turns, flips and crops the image in `bytes` and encodes the result in the
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
    if let Some(c) = edit.crop {
        img = img.crop_imm(c.x, c.y, c.width, c.height);
    }
    encode(&img, mime)
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
