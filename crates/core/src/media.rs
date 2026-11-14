//! Which files StarBooru keeps, told by what they hold and not by their name:
//! a saved picture is often called just `anime`.

use crate::MediaKind;

/// Accepted formats: (mime, ext, kind, decodable by `image` for thumbnails).
pub const FORMATS: &[(&str, &str, MediaKind, bool)] = &[
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

/// How much of the start of a file [`detect_mime`] needs.
pub const SNIFF_BYTES: u16 = 8192;

/// A file's type by its first bytes.
pub fn detect_mime(bytes: &[u8]) -> Option<&'static str> {
    infer::get(bytes).map(|kind| kind.mime_type())
}

/// Whether a file starting with `head` is one of the [`FORMATS`].
pub fn is_media(head: &[u8]) -> bool {
    detect_mime(head).is_some_and(|mime| FORMATS.iter().any(|(m, ..)| *m == mime))
}

/// How much of the start of a file [`picture_size`] reads: enough to get past
/// the camera data in front of a JPEG's size.
pub const SIZE_BYTES: u32 = 64 * 1024;

/// A picture's width and height, upright, from the start of its file: PNG,
/// GIF, JPEG, WebP and BMP. A JPEG turned by its camera data (EXIF
/// orientation 5 to 8) has the two swapped, as the server stores it. `None`
/// for anything else, or when the start doesn't say.
pub fn picture_size(head: &[u8]) -> Option<(u32, u32)> {
    let size = if head.starts_with(b"\x89PNG\r\n\x1A\n") {
        (be32(head, 16)?, be32(head, 20)?)
    } else if head.starts_with(b"GIF8") {
        (u32::from(le16(head, 6)?), u32::from(le16(head, 8)?))
    } else if head.starts_with(b"BM") {
        bmp_size(head)?
    } else if head.starts_with(b"RIFF") && head.get(8..12) == Some(b"WEBP") {
        webp_size(head)?
    } else if head.starts_with(b"\xFF\xD8") {
        jpeg_size(head)?
    } else {
        return None;
    };
    (size.0 > 0 && size.1 > 0).then_some(size)
}

fn le16(b: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_le_bytes(
        b.get(at..at.checked_add(2)?)?.try_into().ok()?,
    ))
}

fn be16(b: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_be_bytes(
        b.get(at..at.checked_add(2)?)?.try_into().ok()?,
    ))
}

fn le24(b: &[u8], at: usize) -> Option<u32> {
    let s = b.get(at..at.checked_add(3)?)?;
    Some(u32::from_le_bytes([*s.first()?, *s.get(1)?, *s.get(2)?, 0]))
}

fn le32(b: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(
        b.get(at..at.checked_add(4)?)?.try_into().ok()?,
    ))
}

fn be32(b: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_be_bytes(
        b.get(at..at.checked_add(4)?)?.try_into().ok()?,
    ))
}

fn bmp_size(b: &[u8]) -> Option<(u32, u32)> {
    // The old OS/2 header has 16-bit sides; the others 32-bit, the height
    // negative for a picture stored top down.
    if le32(b, 14)? == 12 {
        return Some((u32::from(le16(b, 18)?), u32::from(le16(b, 20)?)));
    }
    let side = |at| le32(b, at).map(|v| (v as i32).unsigned_abs());
    Some((side(18)?, side(22)?))
}

fn webp_size(b: &[u8]) -> Option<(u32, u32)> {
    match b.get(12..16)? {
        // Lossy: the frame's sides after its start code, 14 bits each.
        b"VP8 " => (b.get(23..26)? == b"\x9D\x01\x2A").then(|| {
            Some((
                u32::from(le16(b, 26)? & 0x3FFF),
                u32::from(le16(b, 28)? & 0x3FFF),
            ))
        })?,
        // Lossless: both sides less one, 14 bits each, after a signature byte.
        b"VP8L" => {
            if *b.get(20)? != 0x2F {
                return None;
            }
            let bits = le32(b, 21)?;
            Some(((bits & 0x3FFF) + 1, ((bits >> 14) & 0x3FFF) + 1))
        }
        // Extended: the canvas, 24 bits each, less one.
        b"VP8X" => Some((le24(b, 24)?.checked_add(1)?, le24(b, 27)?.checked_add(1)?)),
        _ => None,
    }
}

fn jpeg_size(b: &[u8]) -> Option<(u32, u32)> {
    let mut turned = false;
    let mut at = 2usize;
    loop {
        if *b.get(at)? != 0xFF {
            return None;
        }
        let marker = *b.get(at.checked_add(1)?)?;
        match marker {
            // Padding before a marker.
            0xFF => {
                at = at.checked_add(1)?;
                continue;
            }
            // Markers with nothing after them.
            0x01 | 0xD0..=0xD7 => {
                at = at.checked_add(2)?;
                continue;
            }
            // The image's start without its size: the file says no more.
            0xD9 | 0xDA => return None,
            _ => {}
        }
        let len = usize::from(be16(b, at.checked_add(2)?)?);
        let body = at.checked_add(4)?;
        // Start of a frame (not the Huffman, arithmetic coding or JPEG-LS
        // markers that share the range).
        if matches!(marker, 0xC0..=0xCF) && !matches!(marker, 0xC4 | 0xC8 | 0xCC) {
            let height = u32::from(be16(b, body.checked_add(1)?)?);
            let width = u32::from(be16(b, body.checked_add(3)?)?);
            return Some(if turned {
                (height, width)
            } else {
                (width, height)
            });
        }
        if marker == 0xE1 && b.get(body..body.checked_add(6)?) == Some(b"Exif\0\0") {
            turned = exif_turned(b.get(body.checked_add(6)?..)?).unwrap_or(false);
        }
        at = at.checked_add(2)?.checked_add(len)?;
    }
}

/// Whether camera data says the picture is shown a quarter turn round
/// (orientation 5 to 8).
fn exif_turned(tiff: &[u8]) -> Option<bool> {
    let big = match tiff.get(0..2)? {
        b"MM" => true,
        b"II" => false,
        _ => return None,
    };
    let u16_at = |at| if big { be16(tiff, at) } else { le16(tiff, at) };
    let u32_at = |at| if big { be32(tiff, at) } else { le32(tiff, at) };
    let ifd = usize::try_from(u32_at(4)?).ok()?;
    let count = usize::from(u16_at(ifd)?);
    for i in 0..count.min(64) {
        let entry = ifd.checked_add(2)?.checked_add(i.checked_mul(12)?)?;
        if u16_at(entry)? == 0x0112 {
            return Some(matches!(u16_at(entry.checked_add(8)?)?, 5..=8));
        }
    }
    Some(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The start of a file of each format.
    const STARTS: &[(&str, &[u8])] = &[
        ("image/jpeg", b"\xFF\xD8\xFF\xE0\0\x10JFIF\0"),
        ("image/png", b"\x89PNG\r\n\x1A\n\0\0\0\rIHDR"),
        ("image/webp", b"RIFF\x24\0\0\0WEBPVP8 "),
        ("image/bmp", b"BM\x46\0\0\0\0\0\0\0\x36\0\0\0"),
        ("image/tiff", b"II*\0\x08\0\0\0\0\0"),
        ("image/gif", b"GIF89a\x01\0\x01\0"),
        ("image/avif", b"\0\0\0\x18ftypavif\0\0\0\0avifmif1\0\0\0\0"),
        ("image/jxl", b"\xFF\x0A\xFA\x7F\0\0"),
        ("video/mp4", b"\0\0\0\x18ftypisom\0\0\x02\0isomiso2"),
        (
            "video/webm",
            b"\x1AE\xDF\xA3\x01\0\0\0\0\0\0\x1F\x42\x82\x84webm",
        ),
        (
            "video/x-matroska",
            b"\x1AE\xDF\xA3\x93\x42\x82\x88matroska\x42\x87\x81\x04",
        ),
        ("video/quicktime", b"\0\0\0\x14ftypqt  \0\0\x02\0qt  "),
    ];

    #[test]
    fn every_format_is_known_by_its_start() {
        assert_eq!(STARTS.len(), FORMATS.len());
        for (mime, ..) in FORMATS {
            let (_, start) = STARTS.iter().find(|(m, _)| m == mime).unwrap();
            assert_eq!(detect_mime(start), Some(*mime));
            assert!(is_media(start), "{mime}");
        }
    }

    #[test]
    fn the_start_of_a_big_file_says_the_same_as_the_whole() {
        let cut = usize::from(SNIFF_BYTES);
        for (mime, start) in STARTS {
            let mut whole = start.to_vec();
            whole.resize(cut * 4, 0xAB);
            assert_eq!(detect_mime(&whole), Some(*mime));
            assert_eq!(detect_mime(&whole[..cut]), Some(*mime));
        }
    }

    #[test]
    fn other_files_are_not_media() {
        assert!(!is_media(b""));
        assert!(!is_media(b"# Read me\nThese are pictures.\n"));
        // Known types, but not ones we keep.
        assert!(!is_media(b"%PDF-1.7\n"));
        assert!(!is_media(b"PK\x03\x04\x14\0\0\0"));
        assert!(!is_media(b"RIFF\x24\0\0\0WAVEfmt "));
    }

    /// A JPEG start: an optional EXIF block with `orientation`, then a frame
    /// of `w` × `h`.
    fn jpeg(w: u16, h: u16, orientation: Option<u16>) -> Vec<u8> {
        let mut b = vec![0xFF, 0xD8];
        b.extend_from_slice(b"\xFF\xE0\0\x10JFIF\0\x01\x01\0\0\x01\0\x01\0\0");
        if let Some(o) = orientation {
            // Big-endian TIFF, IFD0 at 8 with one entry.
            let mut tiff = b"MM\0\x2A\0\0\0\x08\0\x01".to_vec();
            tiff.extend_from_slice(&[0x01, 0x12, 0, 3, 0, 0, 0, 1]);
            tiff.extend_from_slice(&o.to_be_bytes());
            tiff.extend_from_slice(&[0, 0, 0, 0, 0, 0]);
            let len = u16::try_from(2 + 6 + tiff.len()).unwrap();
            b.extend_from_slice(&[0xFF, 0xE1]);
            b.extend_from_slice(&len.to_be_bytes());
            b.extend_from_slice(b"Exif\0\0");
            b.extend_from_slice(&tiff);
        }
        // A table in between, which isn't a frame though its marker is in range.
        b.extend_from_slice(&[0xFF, 0xC4, 0, 3, 0]);
        b.extend_from_slice(&[0xFF, 0xC0, 0, 11, 8]);
        b.extend_from_slice(&h.to_be_bytes());
        b.extend_from_slice(&w.to_be_bytes());
        b.extend_from_slice(&[1, 1, 0x11, 0]);
        b
    }

    #[test]
    fn a_pictures_size_is_read_from_its_start() {
        let mut png = b"\x89PNG\r\n\x1A\n\0\0\0\rIHDR".to_vec();
        png.extend_from_slice(&420u32.to_be_bytes());
        png.extend_from_slice(&720u32.to_be_bytes());
        assert_eq!(picture_size(&png), Some((420, 720)));
        assert_eq!(picture_size(b"GIF89a\x40\x01\xF0\0"), Some((320, 240)));
        let mut bmp = b"BM\0\0\0\0\0\0\0\0\x36\0\0\0\x28\0\0\0".to_vec();
        bmp.extend_from_slice(&64i32.to_le_bytes());
        bmp.extend_from_slice(&(-48i32).to_le_bytes());
        assert_eq!(picture_size(&bmp), Some((64, 48)));
        let mut lossy = b"RIFF\0\0\0\0WEBPVP8 \0\0\0\0\0\0\0\x9D\x01\x2A".to_vec();
        lossy.extend_from_slice(&[0x20, 0x03, 0x58, 0x02]);
        assert_eq!(picture_size(&lossy), Some((800, 600)));
        let mut lossless = b"RIFF\0\0\0\0WEBPVP8L\0\0\0\0\x2F".to_vec();
        lossless.extend_from_slice(&(99u32 | (49 << 14)).to_le_bytes());
        assert_eq!(picture_size(&lossless), Some((100, 50)));
        let mut extended = b"RIFF\0\0\0\0WEBPVP8X\0\0\0\0\0\0\0\0".to_vec();
        extended.extend_from_slice(&[0xFF, 0x0F, 0, 0x37, 0x04, 0]);
        assert_eq!(picture_size(&extended), Some((4096, 1080)));
        assert_eq!(picture_size(&jpeg(1920, 1080, None)), Some((1920, 1080)));
        assert_eq!(picture_size(&jpeg(1920, 1080, Some(1))), Some((1920, 1080)));
    }

    #[test]
    fn a_jpeg_turned_by_its_camera_data_is_measured_upright() {
        for o in 5..=8 {
            assert_eq!(
                picture_size(&jpeg(4000, 3000, Some(o))),
                Some((3000, 4000)),
                "{o}"
            );
        }
    }

    #[test]
    fn a_start_that_doesnt_say_gives_no_size() {
        assert_eq!(picture_size(b""), None);
        assert_eq!(picture_size(b"\x89PNG\r\n\x1A\n\0\0"), None);
        assert_eq!(picture_size(b"\0\0\0\x18ftypisom"), None);
        // Zero sides, a JPEG cut before its frame, a length that runs off
        // the end, and a camera block pointing far away.
        assert_eq!(picture_size(b"GIF89a\0\0\x10\0"), None);
        let whole = jpeg(10, 10, Some(6));
        for cut in 0..whole.len() - 4 {
            let _ = picture_size(&whole[..cut]);
        }
        assert_eq!(picture_size(&whole[..20]), None);
        assert_eq!(picture_size(b"\xFF\xD8\xFF\xE1\xFF\xFF"), None);
        let mut far = b"\xFF\xD8\xFF\xE1\0\x10Exif\0\0MM\0\x2A\xFF\xFF\xFF\xF0".to_vec();
        far.extend_from_slice(&[0xFF, 0xC0, 0, 11, 8, 0, 5, 0, 7, 1, 1, 0x11, 0]);
        assert_eq!(picture_size(&far), Some((7, 5)));
    }
}
