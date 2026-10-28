//! Automatic metadata tags: facts the server reads from a file and adds to
//! its post in the `meta` category, named like Danbooru's.
//!
//! | Tag | When |
//! |---|---|
//! | `lowres` | width ≤ 500 and height ≤ 500 |
//! | `highres` | width ≥ 1600 or height ≥ 1200 |
//! | `absurdres` | width ≥ 3200 or height ≥ 2400 (also `highres`) |
//! | `incredibly_absurdres` | width ≥ 10000 or height ≥ 10000 (also the two above) |
//! | `wide_image` | width ≥ 1024 and width ≥ 4 × height |
//! | `tall_image` | height ≥ 1024 and height ≥ 4 × width (never with `wide_image`) |
//! | `animated` | animations (GIF) and videos |
//! | `video` | videos |
//! | `sound` | videos with an audio stream |
//! | `long_video` | videos of 30 s or more |
//!
//! A fact that isn't known (no dimensions, no duration, audio not probed)
//! adds nothing.

use crate::MediaKind;

/// `lowres`: both sides at most this many pixels.
pub const LOWRES_MAX: u32 = 500;
/// `highres`: at least this wide, or at least [`HIGHRES_HEIGHT`] tall.
pub const HIGHRES_WIDTH: u32 = 1600;
pub const HIGHRES_HEIGHT: u32 = 1200;
/// `absurdres`: at least this wide, or at least [`ABSURDRES_HEIGHT`] tall.
pub const ABSURDRES_WIDTH: u32 = 3200;
pub const ABSURDRES_HEIGHT: u32 = 2400;
/// `incredibly_absurdres`: either side at least this many pixels.
pub const INCREDIBLY_ABSURDRES: u32 = 10_000;
/// `wide_image` / `tall_image`: the long side is at least this many pixels
/// and at least [`EXTREME_RATIO`] times the short one.
pub const EXTREME_MIN_SIDE: u32 = 1024;
pub const EXTREME_RATIO: u64 = 4;
/// `long_video`: at least this long.
pub const LONG_VIDEO_MS: i64 = 30_000;

/// Every tag [`meta_tags`] can return.
pub const META_TAGS: &[&str] = &[
    "lowres",
    "highres",
    "absurdres",
    "incredibly_absurdres",
    "wide_image",
    "tall_image",
    "animated",
    "video",
    "sound",
    "long_video",
];

/// What is known about a file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MediaFacts {
    pub kind: MediaKind,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub duration_ms: Option<i64>,
    /// Whether a video has an audio stream; `None` when not probed.
    pub has_audio: Option<bool>,
}

/// The metadata tags for a file, in the order of [`META_TAGS`].
pub fn meta_tags(facts: &MediaFacts) -> Vec<&'static str> {
    let mut tags = Vec::new();
    let size = facts
        .width
        .zip(facts.height)
        .filter(|(w, h)| *w > 0 && *h > 0);
    if let Some((w, h)) = size {
        if w <= LOWRES_MAX && h <= LOWRES_MAX {
            tags.push("lowres");
        }
        if w >= HIGHRES_WIDTH || h >= HIGHRES_HEIGHT {
            tags.push("highres");
        }
        if w >= ABSURDRES_WIDTH || h >= ABSURDRES_HEIGHT {
            tags.push("absurdres");
        }
        if w >= INCREDIBLY_ABSURDRES || h >= INCREDIBLY_ABSURDRES {
            tags.push("incredibly_absurdres");
        }
        let (w64, h64) = (u64::from(w), u64::from(h));
        if w >= EXTREME_MIN_SIDE && w64 >= h64.saturating_mul(EXTREME_RATIO) {
            tags.push("wide_image");
        } else if h >= EXTREME_MIN_SIDE && h64 >= w64.saturating_mul(EXTREME_RATIO) {
            tags.push("tall_image");
        }
    }
    let is_video = facts.kind == MediaKind::Video;
    if is_video || facts.kind == MediaKind::Animation {
        tags.push("animated");
    }
    if is_video {
        tags.push("video");
        if facts.has_audio == Some(true) {
            tags.push("sound");
        }
        if facts.duration_ms.is_some_and(|ms| ms >= LONG_VIDEO_MS) {
            tags.push("long_video");
        }
    }
    tags
}

#[cfg(test)]
mod tests {
    use super::*;

    fn image(w: u32, h: u32) -> Vec<&'static str> {
        meta_tags(&MediaFacts {
            kind: MediaKind::Image,
            width: Some(w),
            height: Some(h),
            duration_ms: None,
            has_audio: None,
        })
    }

    fn video(duration_ms: Option<i64>, has_audio: Option<bool>) -> Vec<&'static str> {
        meta_tags(&MediaFacts {
            kind: MediaKind::Video,
            width: Some(1280),
            height: Some(720),
            duration_ms,
            has_audio,
        })
    }

    #[test]
    fn lowres_needs_both_sides_small() {
        assert_eq!(image(500, 500), ["lowres"]);
        assert_eq!(image(1, 1), ["lowres"]);
        assert!(image(501, 500).is_empty());
        assert!(image(500, 501).is_empty());
    }

    #[test]
    fn highres_needs_either_side() {
        assert!(image(1599, 1199).is_empty());
        assert_eq!(image(1600, 1000), ["highres"]);
        assert_eq!(image(1000, 1200), ["highres"]);
    }

    #[test]
    fn absurdres_implies_highres() {
        assert_eq!(image(3199, 2399), ["highres"]);
        assert_eq!(image(3200, 2000), ["highres", "absurdres"]);
        assert_eq!(image(2000, 2400), ["highres", "absurdres"]);
        assert_eq!(image(9999, 9999), ["highres", "absurdres"]);
        assert_eq!(
            image(10_000, 5000),
            ["highres", "absurdres", "incredibly_absurdres"]
        );
        assert_eq!(
            image(5000, 10_000),
            ["highres", "absurdres", "incredibly_absurdres"]
        );
    }

    #[test]
    fn wide_and_tall() {
        assert_eq!(image(1024, 256), ["wide_image"]);
        assert!(image(1024, 257).is_empty());
        // Too small to count, however thin.
        assert!(image(1023, 10).is_empty());
        assert_eq!(image(256, 1024), ["tall_image"]);
        assert!(image(257, 1024).is_empty());
        assert!(image(10, 1023).is_empty());
        assert_eq!(image(4800, 1200), ["highres", "absurdres", "wide_image"]);
        assert_eq!(
            image(u32::MAX, 1),
            ["highres", "absurdres", "incredibly_absurdres", "wide_image"]
        );
    }

    #[test]
    fn unknown_or_zero_size_adds_nothing() {
        let none = MediaFacts {
            kind: MediaKind::Image,
            width: None,
            height: Some(100),
            duration_ms: None,
            has_audio: None,
        };
        assert!(meta_tags(&none).is_empty());
        assert!(image(0, 0).is_empty());
        assert!(image(0, 5000).is_empty());
    }

    #[test]
    fn animations_are_animated() {
        let gif = MediaFacts {
            kind: MediaKind::Animation,
            width: Some(400),
            height: Some(300),
            duration_ms: Some(60_000),
            has_audio: Some(true),
        };
        // Only videos get `video`, `sound` and `long_video`.
        assert_eq!(meta_tags(&gif), ["lowres", "animated"]);
    }

    #[test]
    fn video_tags() {
        assert_eq!(video(None, None), ["animated", "video"]);
        assert_eq!(video(Some(29_999), Some(false)), ["animated", "video"]);
        assert_eq!(
            video(Some(30_000), Some(true)),
            ["animated", "video", "sound", "long_video"]
        );
        assert_eq!(
            video(Some(1000), Some(true)),
            ["animated", "video", "sound"]
        );
    }

    #[test]
    fn every_tag_is_listed_and_valid() {
        for tag in META_TAGS {
            assert_eq!(crate::normalize_tag(tag).as_deref(), Ok(*tag));
        }
        let all = [
            image(1, 1),
            image(20_000, 10),
            image(10, 20_000),
            video(Some(60_000), Some(true)),
        ];
        for tag in all.iter().flatten() {
            assert!(META_TAGS.contains(tag), "{tag}");
        }
    }
}
