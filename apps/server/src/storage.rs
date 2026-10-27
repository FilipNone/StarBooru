//! Content-addressed media storage on disk.
//!
//! ```text
//! <data>/original/ab/cd/<sha256>.<ext>
//! <data>/thumb/ab/cd/<sha256>.jpg
//! ```

use std::io::Cursor;
use std::path::{Path, PathBuf};

use image::{DynamicImage, ImageReader, Limits, Rgb, RgbImage};
use sha2::{Digest, Sha256};
use starbooru_core::MediaKind;

use crate::error::{AppError, AppResult};

pub const THUMB_SIZE: u32 = 512;
const THUMB_EXT: &str = "jpg";
const THUMB_QUALITY: u8 = 85;
/// Refuse to decode images whose pixel buffers would exceed this.
const MAX_DECODE_BYTES: u64 = 1 << 30;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Variant {
    Original,
    Thumb,
}

impl Variant {
    pub fn as_str(self) -> &'static str {
        match self {
            Variant::Original => "original",
            Variant::Thumb => "thumb",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "original" => Some(Variant::Original),
            "thumb" => Some(Variant::Thumb),
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
    pub has_thumb: bool,
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

pub struct Storage {
    root: PathBuf,
}

impl Storage {
    pub async fn open(root: &Path) -> std::io::Result<Self> {
        for v in [Variant::Original, Variant::Thumb] {
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
        })
    }

    pub fn file_name(variant: Variant, sha_hex: &str, ext: &str) -> String {
        match variant {
            Variant::Original => format!("{sha_hex}.{ext}"),
            Variant::Thumb => format!("{sha_hex}.{THUMB_EXT}"),
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

        let (mut width, mut height, mut thumb) = (None, None, None);
        if decodable {
            let img = decode(bytes)?;
            width = i32::try_from(img.width()).ok();
            height = i32::try_from(img.height()).ok();
            thumb = Some(make_thumbnail(&img)?);
        }

        let original = self
            .path_for_file(
                Variant::Original,
                &Self::file_name(Variant::Original, &sha_hex, ext),
            )
            .ok_or_else(|| AppError::internal("bad storage path"))?;
        self.write_atomic(&original, bytes)?;
        if let Some(jpeg) = &thumb {
            let path = self
                .path_for_file(
                    Variant::Thumb,
                    &Self::file_name(Variant::Thumb, &sha_hex, ext),
                )
                .ok_or_else(|| AppError::internal("bad storage path"))?;
            self.write_atomic(&path, jpeg)?;
        }

        Ok(Ingested {
            sha256,
            ext,
            mime,
            kind,
            width,
            height,
            has_thumb: thumb.is_some(),
        })
    }

    /// Deletes a post's original and thumbnail. Missing files are not an error.
    pub fn remove(&self, sha_hex: &str, ext: &str) -> std::io::Result<()> {
        for variant in [Variant::Original, Variant::Thumb] {
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

fn decode(bytes: &[u8]) -> AppResult<DynamicImage> {
    let mut reader = ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|e| AppError::BadRequest(format!("cannot read image: {e}")))?;
    let mut limits = Limits::default();
    limits.max_alloc = Some(MAX_DECODE_BYTES);
    reader.limits(limits);
    reader
        .decode()
        .map_err(|e| AppError::BadRequest(format!("cannot decode image: {e}")))
}

/// Fits the image in `THUMB_SIZE`², flattens transparency onto white, JPEG.
fn make_thumbnail(img: &DynamicImage) -> AppResult<Vec<u8>> {
    let small = img.thumbnail(THUMB_SIZE, THUMB_SIZE).to_rgba8();
    let rgb = RgbImage::from_fn(small.width(), small.height(), |x, y| {
        let [r, g, b, a] = small.get_pixel(x, y).0;
        let a = u16::from(a);
        let blend = |c: u8| ((u16::from(c) * a + 255 * (255 - a)) / 255) as u8;
        Rgb([blend(r), blend(g), blend(b)])
    });
    let mut out = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, THUMB_QUALITY)
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

    #[tokio::test]
    async fn ingests_png_and_thumbnails() {
        let dir = tempfile::tempdir().unwrap();
        let storage = Storage::open(dir.path()).await.unwrap();
        let info = storage.ingest(&png_bytes(1024, 300)).unwrap();
        assert_eq!(info.ext, "png");
        assert_eq!((info.width, info.height), (Some(1024), Some(300)));
        assert!(info.has_thumb);
        let sha = hex::encode(info.sha256);
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
    fn path_validation_blocks_traversal() {
        let s = Storage {
            root: PathBuf::from("/data"),
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
}
