//! Pictures in, tensors out.

use std::io::Cursor;

use base64::Engine;
use image::imageops::FilterType;
use image::{ImageReader, Limits, RgbImage};

/// The server sends pictures of 1536 px at most.
const MAX_DECODE_BYTES: u64 = 256 * 1024 * 1024;

pub fn decode(base64_jpeg: &str) -> Result<RgbImage, String> {
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(base64_jpeg)
        .map_err(|_| "not a picture".to_string())?;
    let mut reader = ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|_| "not a picture".to_string())?;
    let mut limits = Limits::default();
    limits.max_alloc = Some(MAX_DECODE_BYTES);
    reader.limits(limits);
    reader
        .decode()
        .map(|i| i.to_rgb8())
        .map_err(|_| "not a picture".to_string())
}

/// Channels first, 0 to 1, stretched to a square.
pub fn stretched_chw(image: &RgbImage, size: u32) -> (Vec<i64>, Vec<f32>) {
    let small = image::imageops::resize(image, size, size, FilterType::CatmullRom);
    let side = i64::from(size);
    let pixels = small.as_raw();
    let mut data = Vec::with_capacity(pixels.len());
    for channel in 0..3 {
        data.extend(
            pixels
                .iter()
                .skip(channel)
                .step_by(3)
                .map(|&v| f32::from(v) / 255.0),
        );
    }
    (vec![1, 3, side, side], data)
}

/// The picture on a white square, at `size`².
fn padded(image: &RgbImage, size: u32) -> RgbImage {
    let side = image.width().max(image.height()).max(1);
    let mut square = RgbImage::from_pixel(side, side, image::Rgb([255, 255, 255]));
    let (x, y) = (
        i64::from((side - image.width()) / 2),
        i64::from((side - image.height()) / 2),
    );
    image::imageops::replace(&mut square, image, x, y);
    image::imageops::resize(&square, size, size, FilterType::CatmullRom)
}

/// Channels first in the order given, padded to a white square, with each
/// channel's 0 to 1 shifted by `mean` and scaled by `std`.
pub fn padded_chw(
    image: &RgbImage,
    size: u32,
    order: [usize; 3],
    mean: [f32; 3],
    std: [f32; 3],
) -> (Vec<i64>, Vec<f32>) {
    let small = padded(image, size);
    let pixels = small.as_raw();
    let mut data = Vec::with_capacity(pixels.len());
    for ((channel, mean), std) in order.into_iter().zip(mean).zip(std) {
        data.extend(
            pixels
                .iter()
                .skip(channel)
                .step_by(3)
                .map(|&v| (f32::from(v) / 255.0 - mean) / std),
        );
    }
    let side = i64::from(size);
    (vec![1, 3, side, side], data)
}

/// Channels last, blue first, 0 to 255, padded to a white square.
pub fn padded_bgr_hwc(image: &RgbImage, size: u32) -> (Vec<i64>, Vec<f32>) {
    let data = padded(image, size)
        .pixels()
        .flat_map(|p| [f32::from(p.0[2]), f32::from(p.0[1]), f32::from(p.0[0])])
        .collect();
    let side = i64::from(size);
    (vec![1, side, side, 3], data)
}

/// A square grey crop for Manga OCR: channels first, -1 to 1.
pub fn ocr_crop(
    image: &RgbImage,
    x: u32,
    y: u32,
    w: u32,
    h: u32,
    size: u32,
) -> (Vec<i64>, Vec<f32>) {
    let crop = image::imageops::crop_imm(image, x, y, w.max(1), h.max(1)).to_image();
    let small = image::imageops::resize(&crop, size, size, FilterType::Triangle);
    let grey: Vec<f32> = small
        .pixels()
        .map(|p| {
            let [r, g, b] = p.0;
            let luma = (299 * u32::from(r) + 587 * u32::from(g) + 114 * u32::from(b)) / 1000;
            (luma as f32 / 255.0 - 0.5) / 0.5
        })
        .collect();
    let mut data = Vec::with_capacity(grey.len().saturating_mul(3));
    for _ in 0..3 {
        data.extend_from_slice(&grey);
    }
    let side = i64::from(size);
    (vec![1, 3, side, side], data)
}

/// A repeatable picture with some detail, for timing models.
pub fn generated(width: u32, height: u32) -> RgbImage {
    RgbImage::from_fn(width, height, |x, y| {
        let wave = (x.wrapping_mul(7) ^ y.wrapping_mul(13)) & 0xff;
        image::Rgb([
            (x.wrapping_mul(255) / width.max(1)) as u8,
            (y.wrapping_mul(255) / height.max(1)) as u8,
            wave as u8,
        ])
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rubbish_is_not_a_picture() {
        assert!(decode("AAAA").is_err());
        assert!(decode("not base64 !").is_err());
    }

    #[test]
    fn tensors_have_the_shape_they_claim() {
        let image = generated(30, 10);
        let (shape, data) = stretched_chw(&image, 8);
        assert_eq!(shape, [1, 3, 8, 8]);
        assert_eq!(data.len(), 3 * 8 * 8);
        assert!(data.iter().all(|v| (0.0..=1.0).contains(v)));
        let (shape, data) = padded_bgr_hwc(&image, 8);
        assert_eq!(shape, [1, 8, 8, 3]);
        assert_eq!(data.len(), 3 * 8 * 8);
        // The padding is white.
        assert_eq!(data.first(), Some(&255.0));
        let (shape, data) = padded_chw(&image, 8, [2, 1, 0], [0.5; 3], [0.5; 3]);
        assert_eq!(shape, [1, 3, 8, 8]);
        assert_eq!(data.len(), 3 * 8 * 8);
        assert_eq!(data.first(), Some(&1.0));
        let (shape, data) = ocr_crop(&image, 2, 2, 5, 5, 8);
        assert_eq!(shape, [1, 3, 8, 8]);
        assert!(data.iter().all(|v| (-1.0..=1.0).contains(v)));
    }

    #[test]
    fn channels_come_in_the_order_asked() {
        let red = RgbImage::from_pixel(2, 2, image::Rgb([255, 0, 0]));
        let (_, rgb) = padded_chw(&red, 2, [0, 1, 2], [0.0; 3], [1.0; 3]);
        assert_eq!(rgb.first(), Some(&1.0));
        assert_eq!(rgb.last(), Some(&0.0));
        let (_, bgr) = padded_chw(&red, 2, [2, 1, 0], [0.0; 3], [1.0; 3]);
        assert_eq!(bgr.first(), Some(&0.0));
        assert_eq!(bgr.last(), Some(&1.0));
    }

    #[test]
    fn crops_past_the_edge_stay_inside() {
        let image = generated(10, 10);
        let (_, data) = ocr_crop(&image, 50, 50, 500, 0, 4);
        assert_eq!(data.len(), 3 * 4 * 4);
    }
}
