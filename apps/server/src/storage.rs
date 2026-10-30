//! Content-addressed media storage on disk.
//!
//! ```text
//! <data>/original/ab/cd/<sha256>.<ext>
//! <data>/thumb/ab/cd/<sha256>.jpg
//! <data>/sample/ab/cd/<sha256>.jpg
//! ```

use std::io::{Cursor, Read};
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use image::codecs::jpeg::JpegEncoder;
use image::imageops::FilterType;
use image::metadata::Orientation;
use image::{
    DynamicImage, ExtendedColorType, ImageDecoder, ImageEncoder, ImageReader, Limits, Rgb, RgbImage,
};
use sha2::{Digest, Sha256};
use starbooru_core::{MediaKind, SAMPLE_SIZE};

use crate::error::{AppError, AppResult};
use crate::probe::Tools;
use crate::similar::Fingerprint;

pub const THUMB_SIZE: u32 = 512;
/// How much of a stored image is read for its EXIF date, which sits near
/// the start.
const MAX_EXIF_READ: u64 = 64 << 20;
const THUMB_EXT: &str = "jpg";
const THUMB_QUALITY: u8 = 85;
const SAMPLE_EXT: &str = "jpg";
const SAMPLE_QUALITY: u8 = 85;
/// Originals smaller than this are shown as they are.
const SAMPLE_MIN_BYTES: usize = 1 << 20;
/// A sample is kept only when it is at most this percentage of the original.
const SAMPLE_MAX_PERCENT: usize = 75;
/// Refuse to decode images whose pixel buffers would exceed this.
const MAX_DECODE_BYTES: u64 = 1 << 30;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Variant {
    Original,
    Thumb,
    /// A smaller copy of a big image, for viewing.
    Sample,
}

impl Variant {
    pub fn as_str(self) -> &'static str {
        match self {
            Variant::Original => "original",
            Variant::Thumb => "thumb",
            Variant::Sample => "sample",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "original" => Some(Variant::Original),
            "thumb" => Some(Variant::Thumb),
            "sample" => Some(Variant::Sample),
            _ => None,
        }
    }
}

/// What we learned about an uploaded file.
#[derive(Debug, Clone)]
pub struct Ingested {
    pub sha256: [u8; 32],
    pub ext: &'static str,
    pub mime: &'static str,
    pub kind: MediaKind,
    pub width: Option<i32>,
    pub height: Option<i32>,
    pub duration_ms: Option<i64>,
    /// Whether ffprobe found an audio stream; `None` when not probed.
    pub has_audio: Option<bool>,
    pub has_thumb: bool,
    /// The thumbnail's fingerprint for the duplicate finder.
    pub print: Option<Fingerprint>,
    /// Whether a sample was made; `None` when samples are off, so they
    /// can be made later.
    pub has_sample: Option<bool>,
    /// The date the file carries.
    pub file_date: Option<DateTime<Utc>>,
}

/// What ffmpeg found in a file `image` can't decode.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Derived {
    pub width: Option<i32>,
    pub height: Option<i32>,
    pub duration_ms: Option<i64>,
    pub has_audio: Option<bool>,
    pub has_thumb: bool,
    pub print: Option<Fingerprint>,
    pub file_date: Option<DateTime<Utc>>,
}

/// Accepted formats: (mime, ext, kind, decodable by `image` for thumbnails).
const FORMATS: &[(&str, &str, MediaKind, bool)] = &[
    ("image/jpeg", "jpg", MediaKind::Image, true),
    ("image/png", "png", MediaKind::Image, true),
    ("image/webp", "webp", MediaKind::Image, true),
    ("image/bmp", "bmp", MediaKind::Image, true),
    ("image/tiff", "tiff", MediaKind::Image, true),
    ("image/gif", "gif", MediaKind::Animation, true),
    ("image/avif", "avif", MediaKind::Image, false),
    ("image/jxl", "jxl", MediaKind::Image, false),
    ("video/mp4", "mp4", MediaKind::Video, false),
    ("video/webm", "webm", MediaKind::Video, false),
    ("video/x-matroska", "mkv", MediaKind::Video, false),
    ("video/quicktime", "mov", MediaKind::Video, false),
];

/// The date in a file's EXIF data: when it was taken or, failing that,
/// digitised or last changed.
fn exif_date(bytes: &[u8]) -> Option<DateTime<Utc>> {
    let exif = exif::Reader::new()
        .read_from_container(&mut Cursor::new(bytes))
        .ok()?;
    [
        exif::Tag::DateTimeOriginal,
        exif::Tag::DateTimeDigitized,
        exif::Tag::DateTime,
    ]
    .into_iter()
    .find_map(|tag| {
        let field = exif.get_field(tag, exif::In::PRIMARY)?;
        let exif::Value::Ascii(parts) = &field.value else {
            return None;
        };
        starbooru_core::filter::exif_date(std::str::from_utf8(parts.first()?).ok()?)
    })
}

pub struct Storage {
    root: PathBuf,
    tools: Option<Tools>,
    samples: bool,
}

impl Storage {
    pub async fn open(root: &Path) -> std::io::Result<Self> {
        for v in [Variant::Original, Variant::Thumb, Variant::Sample] {
            tokio::fs::create_dir_all(root.join(v.as_str())).await?;
        }
        let tmp = root.join("tmp");
        tokio::fs::create_dir_all(&tmp).await?;
        // Leftovers from writes interrupted by a crash or restart.
        let mut entries = tokio::fs::read_dir(&tmp).await?;
        while let Some(entry) = entries.next_entry().await? {
            let _ = tokio::fs::remove_file(entry.path()).await;
        }
        Ok(Self {
            root: root.to_path_buf(),
            tools: None,
            samples: true,
        })
    }

    /// Enables thumbnails for video, AVIF and JPEG XL through ffmpeg.
    pub fn with_tools(mut self, tools: Option<Tools>) -> Self {
        self.tools = tools;
        self
    }

    /// Turns making samples on or off (on by default).
    pub fn with_samples(mut self, samples: bool) -> Self {
        self.samples = samples;
        self
    }

    pub fn makes_samples(&self) -> bool {
        self.samples
    }

    pub fn has_tools(&self) -> bool {
        self.tools.is_some()
    }

    pub fn tools(&self) -> Option<&Tools> {
        self.tools.as_ref()
    }

    /// Where a stored original lives.
    pub fn original_path(&self, sha_hex: &str, ext: &str) -> Option<PathBuf> {
        self.path_for_file(
            Variant::Original,
            &Self::file_name(Variant::Original, sha_hex, ext),
        )
    }

    /// A fresh path in the scratch directory, cleared at every start.
    pub fn scratch_path(&self, ext: &str) -> PathBuf {
        let mut nonce = [0u8; 8];
        rand::fill(&mut nonce);
        self.root
            .join("tmp")
            .join(format!("{}.{ext}", hex::encode(nonce)))
    }

    pub fn file_name(variant: Variant, sha_hex: &str, ext: &str) -> String {
        match variant {
            Variant::Original => format!("{sha_hex}.{ext}"),
            Variant::Thumb => format!("{sha_hex}.{THUMB_EXT}"),
            Variant::Sample => format!("{sha_hex}.{SAMPLE_EXT}"),
        }
    }

    /// Validates a `<sha>.<ext>` file name from a URL and maps it to disk.
    /// Returns `None` for anything that is not exactly that shape.
    pub fn path_for_file(&self, variant: Variant, file: &str) -> Option<PathBuf> {
        let (sha, ext) = file.split_once('.')?;
        let sha_ok = sha.len() == 64
            && sha
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b));
        let ext_ok = (1..=5).contains(&ext.len())
            && ext
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit());
        if !sha_ok || !ext_ok {
            return None;
        }
        let (a, b) = (sha.get(0..2)?, sha.get(2..4)?);
        Some(self.root.join(variant.as_str()).join(a).join(b).join(file))
    }

    /// Hashes, sniffs, thumbnails and stores an upload. CPU-bound: call it
    /// from `spawn_blocking`.
    pub fn ingest(&self, bytes: &[u8]) -> AppResult<Ingested> {
        let detected = infer::get(bytes)
            .ok_or_else(|| AppError::Unsupported("unrecognised file type".into()))?;
        let &(mime, ext, kind, decodable) = FORMATS
            .iter()
            .find(|(m, ..)| *m == detected.mime_type())
            .ok_or_else(|| {
                AppError::Unsupported(format!("{} files are not supported", detected.mime_type()))
            })?;

        let sha256: [u8; 32] = Sha256::digest(bytes).into();
        let sha_hex = hex::encode(sha256);

        let (mut width, mut height, mut thumb, mut sample) = (None, None, None, None);
        let mut has_sample = Some(false);
        if decodable {
            let img = decode(bytes)?;
            width = i32::try_from(img.width()).ok();
            height = i32::try_from(img.height()).ok();
            thumb = Some(make_thumbnail(&img)?);
            if kind == MediaKind::Image {
                if self.samples {
                    sample = make_sample(&img, bytes);
                    has_sample = Some(sample.is_some());
                } else {
                    has_sample = None;
                }
            }
        }

        let original = self
            .path_for_file(
                Variant::Original,
                &Self::file_name(Variant::Original, &sha_hex, ext),
            )
            .ok_or_else(|| AppError::internal("bad storage path"))?;
        self.write_atomic(&original, bytes)?;
        if let Some(jpeg) = &thumb {
            self.write_thumb(&sha_hex, ext, jpeg)?;
        }
        if let Some(jpeg) = &sample {
            self.write_sample(&sha_hex, jpeg)?;
        }
        let mut derived = Derived {
            width,
            height,
            duration_ms: None,
            has_audio: None,
            has_thumb: thumb.is_some(),
            print: thumb.as_deref().and_then(fingerprint_jpeg),
            file_date: None,
        };
        if !decodable {
            derived = self.derive(&sha_hex, ext, kind)?;
        }
        let file_date = match kind {
            MediaKind::Image => exif_date(bytes),
            MediaKind::Video => derived.file_date,
            MediaKind::Animation => None,
        };

        Ok(Ingested {
            sha256,
            ext,
            mime,
            kind,
            width: derived.width,
            height: derived.height,
            duration_ms: derived.duration_ms,
            has_audio: derived.has_audio,
            has_thumb: derived.has_thumb,
            print: derived.print,
            has_sample,
            file_date,
        })
    }

    /// Makes the sample of a stored original, for posts uploaded before
    /// samples existed. `false` when it doesn't need one or can't be read;
    /// only disk errors fail. Call from a blocking thread.
    pub fn make_sample_for(&self, sha_hex: &str, ext: &str) -> AppResult<bool> {
        let original = self
            .original_path(sha_hex, ext)
            .ok_or_else(|| AppError::internal("bad storage path"))?;
        let bytes = match std::fs::read(&original) {
            Ok(b) => b,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(false),
            Err(e) => return Err(AppError::internal(e)),
        };
        let Some(sample) = decode(&bytes)
            .ok()
            .and_then(|img| make_sample(&img, &bytes))
        else {
            return Ok(false);
        };
        self.write_sample(sha_hex, &sample)?;
        Ok(true)
    }

    fn write_sample(&self, sha_hex: &str, jpeg: &[u8]) -> AppResult<()> {
        let path = self
            .path_for_file(
                Variant::Sample,
                &Self::file_name(Variant::Sample, sha_hex, SAMPLE_EXT),
            )
            .ok_or_else(|| AppError::internal("bad storage path"))?;
        self.write_atomic(&path, jpeg)?;
        Ok(())
    }

    /// Dimensions, duration and a thumbnail through ffmpeg, for a stored
    /// original. Without the tools, or for a file they can't read, the result
    /// is empty. Only disk errors fail. Call from a blocking thread.
    pub fn derive(&self, sha_hex: &str, ext: &str, kind: MediaKind) -> AppResult<Derived> {
        let Some(tools) = &self.tools else {
            return Ok(Derived::default());
        };
        let original = self
            .path_for_file(
                Variant::Original,
                &Self::file_name(Variant::Original, sha_hex, ext),
            )
            .ok_or_else(|| AppError::internal("bad storage path"))?;
        let probed = tools.probe(&original, kind == MediaKind::Video);
        let thumb = probed
            .frame
            .as_deref()
            .and_then(|png| decode(png).ok())
            .and_then(|img| make_thumbnail(&img).ok());
        if let Some(jpeg) = &thumb {
            self.write_thumb(sha_hex, ext, jpeg)?;
        }
        Ok(Derived {
            width: probed.width,
            height: probed.height,
            duration_ms: probed.duration_ms,
            has_audio: probed.has_audio,
            has_thumb: thumb.is_some(),
            print: thumb.as_deref().and_then(fingerprint_jpeg),
            file_date: probed.file_date,
        })
    }

    /// The date a stored original carries; `None` when it has none, is gone
    /// or can't be read as media. Only disk errors fail. Videos need the
    /// tools. Call from a blocking thread.
    pub fn file_date(
        &self,
        sha_hex: &str,
        ext: &str,
        kind: MediaKind,
    ) -> AppResult<Option<DateTime<Utc>>> {
        let original = self
            .original_path(sha_hex, ext)
            .ok_or_else(|| AppError::internal("bad storage path"))?;
        match kind {
            MediaKind::Animation => Ok(None),
            MediaKind::Video => Ok(self
                .tools
                .as_ref()
                .and_then(|tools| tools.metadata(&original))
                .and_then(|p| p.file_date)),
            MediaKind::Image => {
                let file = match std::fs::File::open(&original) {
                    Ok(file) => file,
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
                    Err(e) => return Err(AppError::internal(e)),
                };
                let mut bytes = Vec::new();
                file.take(MAX_EXIF_READ)
                    .read_to_end(&mut bytes)
                    .map_err(AppError::internal)?;
                Ok(exif_date(&bytes))
            }
        }
    }

    /// Whether a stored original has an audio stream; `None` without the
    /// tools or when they can't read it. Call from a blocking thread.
    pub fn has_audio(&self, sha_hex: &str, ext: &str) -> Option<bool> {
        let tools = self.tools.as_ref()?;
        let original = self.path_for_file(
            Variant::Original,
            &Self::file_name(Variant::Original, sha_hex, ext),
        )?;
        tools.metadata(&original)?.has_audio
    }

    /// The fingerprint of a stored thumbnail, or `None` without a readable
    /// one. Call from a blocking thread.
    pub fn thumb_print(&self, sha_hex: &str, ext: &str) -> Option<Fingerprint> {
        let path = self.path_for_file(
            Variant::Thumb,
            &Self::file_name(Variant::Thumb, sha_hex, ext),
        )?;
        let bytes = std::fs::read(path).ok()?;
        fingerprint_jpeg(&bytes)
    }

    fn write_thumb(&self, sha_hex: &str, ext: &str, jpeg: &[u8]) -> AppResult<()> {
        let path = self
            .path_for_file(
                Variant::Thumb,
                &Self::file_name(Variant::Thumb, sha_hex, ext),
            )
            .ok_or_else(|| AppError::internal("bad storage path"))?;
        self.write_atomic(&path, jpeg)?;
        Ok(())
    }

    /// Deletes a post's original, thumbnail and sample. Missing files are
    /// not an error.
    pub fn remove(&self, sha_hex: &str, ext: &str) -> std::io::Result<()> {
        for variant in [Variant::Original, Variant::Thumb, Variant::Sample] {
            let Some(path) = self.path_for_file(variant, &Self::file_name(variant, sha_hex, ext))
            else {
                continue;
            };
            match std::fs::remove_file(&path) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => return Err(e),
            }
        }
        Ok(())
    }

    fn write_atomic(&self, dest: &Path, bytes: &[u8]) -> std::io::Result<()> {
        if dest.exists() {
            return Ok(()); // content-addressed: same name, same bytes
        }
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut nonce = [0u8; 8];
        rand::fill(&mut nonce);
        let tmp = self.root.join("tmp").join(hex::encode(nonce));
        std::fs::write(&tmp, bytes)?;
        std::fs::rename(&tmp, dest).inspect_err(|_| {
            let _ = std::fs::remove_file(&tmp);
        })
    }
}

/// Decodes an image upright: the EXIF orientation is applied, as browsers
/// do when they show the original.
pub fn decode(bytes: &[u8]) -> AppResult<DynamicImage> {
    let mut reader = ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|e| AppError::BadRequest(format!("cannot read image: {e}")))?;
    let mut limits = Limits::default();
    limits.max_alloc = Some(MAX_DECODE_BYTES);
    reader.limits(limits);
    let mut decoder = reader
        .into_decoder()
        .map_err(|e| AppError::BadRequest(format!("cannot decode image: {e}")))?;
    let orientation = decoder.orientation().unwrap_or(Orientation::NoTransforms);
    let mut img = DynamicImage::from_decoder(decoder)
        .map_err(|e| AppError::BadRequest(format!("cannot decode image: {e}")))?;
    img.apply_orientation(orientation);
    Ok(img)
}

fn fingerprint_jpeg(bytes: &[u8]) -> Option<Fingerprint> {
    decode(bytes).ok().map(|img| Fingerprint::of(&img))
}

/// A JPEG copy of a big still image that fits in `SAMPLE_SIZE`² and keeps
/// the original's colour profile. `None` when the original is small, has
/// transparency, or the copy wouldn't save at least a quarter of its size.
fn make_sample(img: &DynamicImage, original: &[u8]) -> Option<Vec<u8>> {
    if original.len() < SAMPLE_MIN_BYTES || !is_opaque(img) {
        return None;
    }
    let small;
    let src = if img.width() > SAMPLE_SIZE || img.height() > SAMPLE_SIZE {
        small = img.resize(SAMPLE_SIZE, SAMPLE_SIZE, FilterType::CatmullRom);
        &small
    } else {
        img
    };
    let rgb = src.to_rgb8();
    let mut out = Vec::new();
    let mut encoder = JpegEncoder::new_with_quality(&mut out, SAMPLE_QUALITY);
    if let Some(icc) = icc_profile(original) {
        // Unsupported profiles are left out; the colours are then sRGB.
        let _ = encoder.set_icc_profile(icc);
    }
    encoder
        .write_image(
            rgb.as_raw(),
            rgb.width(),
            rgb.height(),
            ExtendedColorType::Rgb8,
        )
        .ok()?;
    let worth_it =
        out.len().saturating_mul(100) <= original.len().saturating_mul(SAMPLE_MAX_PERCENT);
    worth_it.then_some(out)
}

/// Whether every pixel is fully opaque, so JPEG loses nothing there.
fn is_opaque(img: &DynamicImage) -> bool {
    match img {
        DynamicImage::ImageLumaA8(b) => b.pixels().all(|p| p.0[1] == u8::MAX),
        DynamicImage::ImageRgba8(b) => b.pixels().all(|p| p.0[3] == u8::MAX),
        DynamicImage::ImageLumaA16(b) => b.pixels().all(|p| p.0[1] == u16::MAX),
        DynamicImage::ImageRgba16(b) => b.pixels().all(|p| p.0[3] == u16::MAX),
        other if other.color().has_alpha() => other.to_rgba8().pixels().all(|p| p.0[3] == u8::MAX),
        _ => true,
    }
}

fn icc_profile(bytes: &[u8]) -> Option<Vec<u8>> {
    ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .ok()?
        .into_decoder()
        .ok()?
        .icc_profile()
        .ok()
        .flatten()
}

/// Fits the image in `THUMB_SIZE`² (never enlarging it), flattens
/// transparency onto white, JPEG.
fn make_thumbnail(img: &DynamicImage) -> AppResult<Vec<u8>> {
    fit_jpeg(img, THUMB_SIZE, FilterKind::Fast)
}

/// How [`fit_jpeg`] scales down.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FilterKind {
    /// Quick, for thumbnails.
    Fast,
    /// Sharper, for images a model has to read text in.
    Sharp,
}

/// Fits the image in `size`² (never enlarging it), flattens transparency
/// onto white, JPEG.
pub fn fit_jpeg(img: &DynamicImage, size: u32, filter: FilterKind) -> AppResult<Vec<u8>> {
    let small = if img.width() <= size && img.height() <= size {
        img.to_rgba8()
    } else if filter == FilterKind::Sharp {
        img.resize(size, size, FilterType::CatmullRom).to_rgba8()
    } else {
        img.thumbnail(size, size).to_rgba8()
    };
    let rgb = RgbImage::from_fn(small.width(), small.height(), |x, y| {
        let [r, g, b, a] = small.get_pixel(x, y).0;
        let a = u16::from(a);
        let blend = |c: u8| ((u16::from(c) * a + 255 * (255 - a)) / 255) as u8;
        Rgb([blend(r), blend(g), blend(b)])
    });
    let mut out = Vec::new();
    JpegEncoder::new_with_quality(&mut out, THUMB_QUALITY)
        .encode_image(&rgb)
        .map_err(AppError::internal)?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn png_bytes(w: u32, h: u32) -> Vec<u8> {
        let img = image::RgbaImage::from_pixel(w, h, image::Rgba([255, 0, 0, 128]));
        let mut out = Cursor::new(Vec::new());
        img.write_to(&mut out, image::ImageFormat::Png).unwrap();
        out.into_inner()
    }

    fn rgba_noise(w: u32, h: u32, alpha: u8) -> DynamicImage {
        let rgb = image::load_from_memory(&crate::testing::noise_png(w, h, 9)).unwrap();
        let mut rgba = rgb.to_rgba8();
        for p in rgba.pixels_mut() {
            p.0[3] = alpha;
        }
        DynamicImage::ImageRgba8(rgba)
    }

    fn encoded(img: &DynamicImage, format: image::ImageFormat) -> Vec<u8> {
        let mut out = Cursor::new(Vec::new());
        img.write_to(&mut out, format).unwrap();
        out.into_inner()
    }

    #[test]
    fn samples_only_big_opaque_images() {
        let png = crate::testing::noise_png(3000, 500, 1);
        assert!(png.len() > SAMPLE_MIN_BYTES);
        let img = decode(&png).unwrap();
        let sample = make_sample(&img, &png).unwrap();
        assert!(sample.len() * 100 <= png.len() * SAMPLE_MAX_PERCENT);
        let s = decode(&sample).unwrap();
        assert_eq!(s.width(), SAMPLE_SIZE);
        assert!((426..=427).contains(&s.height()), "{}", s.height());

        // Fits already: the same size, only re-encoded.
        let png = crate::testing::noise_png(800, 600, 2);
        let s = decode(&make_sample(&decode(&png).unwrap(), &png).unwrap()).unwrap();
        assert_eq!((s.width(), s.height()), (800, 600));

        // Small files, transparency and already compact JPEGs are left alone.
        let small = png_bytes(64, 64);
        assert_eq!(make_sample(&decode(&small).unwrap(), &small), None);
        let clear = rgba_noise(800, 600, 128);
        assert_eq!(
            make_sample(&clear, &encoded(&clear, image::ImageFormat::Png)),
            None
        );
        let solid = rgba_noise(800, 600, 255);
        assert!(make_sample(&solid, &encoded(&solid, image::ImageFormat::Png)).is_some());
        let photo = rgba_noise(1600, 1200, 255).to_rgb8();
        let mut jpeg = Vec::new();
        JpegEncoder::new_with_quality(&mut jpeg, SAMPLE_QUALITY)
            .encode_image(&photo)
            .unwrap();
        assert!(jpeg.len() > SAMPLE_MIN_BYTES);
        assert_eq!(make_sample(&decode(&jpeg).unwrap(), &jpeg), None);
    }

    #[test]
    fn samples_keep_the_colour_profile() {
        let img = decode(&crate::testing::noise_png(800, 600, 4))
            .unwrap()
            .to_rgb8();
        // Any bytes do: the encoders store the profile without reading it.
        let icc: Vec<u8> = (0..=255u8).cycle().take(3000).collect();
        let mut png = Vec::new();
        let mut encoder = image::codecs::png::PngEncoder::new(&mut png);
        encoder.set_icc_profile(icc.clone()).unwrap();
        encoder
            .write_image(img.as_raw(), 800, 600, ExtendedColorType::Rgb8)
            .unwrap();
        let sample = make_sample(&decode(&png).unwrap(), &png).unwrap();
        assert_eq!(icc_profile(&sample), Some(icc));
        assert_eq!(icc_profile(b"not an image"), None);
    }

    #[test]
    fn opacity() {
        assert!(is_opaque(&rgba_noise(4, 4, 255)));
        assert!(!is_opaque(&rgba_noise(4, 4, 254)));
        let luma = DynamicImage::ImageLumaA16(image::ImageBuffer::from_pixel(
            2,
            2,
            image::LumaA([7u16, u16::MAX]),
        ));
        assert!(is_opaque(&luma));
        assert!(is_opaque(&DynamicImage::new_rgb8(2, 2)));
        assert!(!is_opaque(&DynamicImage::new_rgba32f(2, 2)));
    }

    #[tokio::test]
    async fn stores_and_removes_samples() {
        let dir = tempfile::tempdir().unwrap();
        let storage = Storage::open(dir.path()).await.unwrap();
        let png = crate::testing::noise_png(800, 600, 3);
        let info = storage.ingest(&png).unwrap();
        assert_eq!(info.has_sample, Some(true));
        let sha = hex::encode(info.sha256);
        let path = storage
            .path_for_file(
                Variant::Sample,
                &Storage::file_name(Variant::Sample, &sha, "png"),
            )
            .unwrap();
        assert!(path.ends_with(format!("{sha}.jpg")));
        assert_eq!(
            image::guess_format(&std::fs::read(&path).unwrap()).unwrap(),
            image::ImageFormat::Jpeg
        );

        // Made again for an original stored without one.
        std::fs::remove_file(&path).unwrap();
        assert!(storage.make_sample_for(&sha, "png").unwrap());
        assert!(path.exists());
        assert!(!storage.make_sample_for(&"0".repeat(64), "png").unwrap());
        storage.remove(&sha, "png").unwrap();
        assert!(!path.exists());

        // Small images have none; with samples off, they're left for later.
        assert_eq!(
            storage.ingest(&png_bytes(64, 64)).unwrap().has_sample,
            Some(false)
        );
        let off = Storage::open(dir.path()).await.unwrap().with_samples(false);
        assert_eq!(off.ingest(&png).unwrap().has_sample, None);
        assert!(!path.exists());
    }

    #[test]
    fn decodes_photos_upright() {
        use image::ImageEncoder;
        use image::codecs::jpeg::JpegEncoder;
        // Big-endian TIFF with one IFD entry: Orientation (0x0112) = 6,
        // "turn 90° clockwise to show", as phones write it.
        let exif = [
            b"MM\0*\0\0\0\x08\0\x01".as_slice(),
            &[0x01, 0x12, 0, 3, 0, 0, 0, 1, 0, 6, 0, 0],
            &[0, 0, 0, 0],
        ]
        .concat();
        let img = image::RgbImage::from_pixel(40, 20, image::Rgb([200, 30, 30]));
        let mut out = Vec::new();
        let mut encoder = JpegEncoder::new(&mut out);
        encoder.set_exif_metadata(exif).unwrap();
        encoder
            .write_image(img.as_raw(), 40, 20, image::ExtendedColorType::Rgb8)
            .unwrap();
        let decoded = decode(&out).unwrap();
        assert_eq!((decoded.width(), decoded.height()), (20, 40));
    }

    #[tokio::test]
    async fn ingests_png_and_thumbnails() {
        let dir = tempfile::tempdir().unwrap();
        let storage = Storage::open(dir.path()).await.unwrap();
        let info = storage.ingest(&png_bytes(1024, 300)).unwrap();
        assert_eq!(info.ext, "png");
        assert_eq!((info.width, info.height), (Some(1024), Some(300)));
        assert!(info.has_thumb);
        assert!(info.print.is_some());
        let sha = hex::encode(info.sha256);
        assert_eq!(storage.thumb_print(&sha, "png"), info.print);
        assert_eq!(storage.thumb_print(&"0".repeat(64), "png"), None);
        let thumb = storage
            .path_for_file(
                Variant::Thumb,
                &Storage::file_name(Variant::Thumb, &sha, "png"),
            )
            .unwrap();
        let t = image::open(thumb).unwrap();
        assert_eq!((t.width(), t.height()), (512, 150));
        // Same bytes again: idempotent.
        assert!(storage.ingest(&png_bytes(1024, 300)).is_ok());

        storage.remove(&sha, "png").unwrap();
        assert!(!thumb_path(&storage, &sha).exists());
        // Removing twice is fine.
        storage.remove(&sha, "png").unwrap();
    }

    fn thumb_path(storage: &Storage, sha: &str) -> PathBuf {
        storage
            .path_for_file(
                Variant::Thumb,
                &Storage::file_name(Variant::Thumb, sha, "png"),
            )
            .unwrap()
    }

    #[tokio::test]
    async fn open_clears_staging_leftovers() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("tmp")).unwrap();
        std::fs::write(dir.path().join("tmp/partial"), b"half a file").unwrap();
        Storage::open(dir.path()).await.unwrap();
        assert!(!dir.path().join("tmp/partial").exists());
    }

    #[tokio::test]
    async fn rejects_garbage_without_panicking() {
        let dir = tempfile::tempdir().unwrap();
        let storage = Storage::open(dir.path()).await.unwrap();
        assert!(matches!(storage.ingest(b""), Err(AppError::Unsupported(_))));
        assert!(matches!(
            storage.ingest(b"%PDF-1.7 hello"),
            Err(AppError::Unsupported(_))
        ));
        // Valid PNG signature, truncated body.
        let mut png = png_bytes(4, 4);
        png.truncate(40);
        assert!(matches!(storage.ingest(&png), Err(AppError::BadRequest(_))));
    }

    #[test]
    fn reads_exif_dates() {
        let date = |s: &str| s.parse::<DateTime<Utc>>().ok();
        let good = "2019:05:06 07:08:09";
        let changed = "2020:01:02 03:04:05";
        let both = crate::testing::jpeg_with_exif(16, 16, 1, changed, good);
        assert_eq!(exif_date(&both), date("2019-05-06T07:08:09Z"));
        // A camera with an unset clock: the next date in the file.
        let blank = crate::testing::jpeg_with_exif(16, 16, 1, changed, "0000:00:00 00:00:00");
        assert_eq!(exif_date(&blank), date("2020-01-02T03:04:05Z"));
        let none =
            crate::testing::jpeg_with_exif(16, 16, 1, "    :  :     :  :  ", "nonsense value here");
        assert_eq!(exif_date(&none), None);
        assert_eq!(exif_date(&crate::testing::png(16, 16, 1)), None);
        assert_eq!(exif_date(b""), None);
        assert_eq!(exif_date(b"not an image"), None);
        // Cut off anywhere, the file is only missing its date.
        for end in 0..both.len() {
            let _ = exif_date(&both[..end]);
        }
        // Damaged anywhere in its EXIF block, too.
        for at in 0..160.min(both.len()) {
            let mut bad = both.clone();
            bad[at] ^= 0xff;
            let _ = exif_date(&bad);
        }
    }

    #[test]
    fn path_validation_blocks_traversal() {
        let s = Storage {
            root: PathBuf::from("/data"),
            tools: None,
            samples: true,
        };
        let sha = "a".repeat(64);
        assert!(
            s.path_for_file(Variant::Original, &format!("{sha}.png"))
                .is_some()
        );
        assert!(
            s.path_for_file(Variant::Original, "../etc/passwd")
                .is_none()
        );
        assert!(
            s.path_for_file(Variant::Original, &format!("{sha}.p/g"))
                .is_none()
        );
        assert!(
            s.path_for_file(Variant::Original, &format!("{}.png", "A".repeat(64)))
                .is_none()
        );
        assert!(s.path_for_file(Variant::Original, "ab.png").is_none());
    }

    #[tokio::test]
    async fn thumbnails_video_avif_and_jxl_with_ffmpeg() {
        let Some(tools) = crate::probe::tests::tools() else {
            return;
        };
        let src = tempfile::tempdir().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let storage = Storage::open(dir.path())
            .await
            .unwrap()
            .with_tools(Some(tools));
        // File name, lavfi input, encoder args, kind, size, thumbnail expected.
        type Case<'a> = (&'a str, &'a str, &'a [&'a str], MediaKind, (i32, i32), bool);
        let cases: [Case; 3] = [
            (
                "clip.webm",
                "testsrc=duration=1.5:size=96x64:rate=10",
                &["-c:v", "libvpx-vp9", "-b:v", "100k"],
                MediaKind::Video,
                (96, 64),
                true,
            ),
            (
                "still.avif",
                "testsrc=size=80x60",
                &[
                    "-frames:v",
                    "1",
                    "-c:v",
                    "libaom-av1",
                    "-still-picture",
                    "1",
                ],
                MediaKind::Image,
                (80, 60),
                false,
            ),
            (
                "still.jxl",
                "testsrc=size=72x40",
                &["-frames:v", "1", "-c:v", "libjxl"],
                MediaKind::Image,
                (72, 40),
                false,
            ),
        ];
        for (name, input, args, kind, dims, timed) in cases {
            let Some(bytes) = crate::probe::tests::generate(src.path(), name, input, args) else {
                continue;
            };
            let info = storage.ingest(&bytes).unwrap();
            assert_eq!(info.kind, kind, "{name}");
            assert!(info.has_thumb, "{name}");
            assert_eq!(
                (info.width, info.height),
                (Some(dims.0), Some(dims.1)),
                "{name}"
            );
            assert_eq!(info.duration_ms.is_some(), timed, "{name}");
            let thumb = thumb_path(&storage, &hex::encode(info.sha256));
            let t = image::open(thumb).unwrap();
            assert_eq!(
                (t.width(), t.height()),
                (dims.0 as u32, dims.1 as u32),
                "{name}"
            );
        }
    }

    #[tokio::test]
    async fn without_tools_videos_have_no_thumbnail() {
        let Some(_) = crate::probe::tests::tools() else {
            return;
        };
        let src = tempfile::tempdir().unwrap();
        let Some(bytes) = crate::probe::tests::generate(
            src.path(),
            "clip.mp4",
            "testsrc=duration=1:size=64x48:rate=5",
            &["-pix_fmt", "yuv420p"],
        ) else {
            return;
        };
        let dir = tempfile::tempdir().unwrap();
        let storage = Storage::open(dir.path()).await.unwrap();
        let info = storage.ingest(&bytes).unwrap();
        assert_eq!(info.kind, MediaKind::Video);
        assert!(!info.has_thumb);
        assert_eq!((info.width, info.duration_ms), (None, None));
    }
}
