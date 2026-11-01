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
}
