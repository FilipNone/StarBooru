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
//! | `hd` | long side ≥ 1280 and short side ≥ 480 |
//! | `full_hd` | long side ≥ 1920 and short side ≥ 720 |
//! | `4k` | long side ≥ 3840 and short side ≥ 1440 |
//! | `8k` | long side ≥ 7680 and short side ≥ 2880 |
//! | `horizontal_image` | wider than tall, by more than 1% |
//! | `vertical_image` | taller than wide, by more than 1% |
//! | `square_image` | the sides within 1% of each other |
//!
//! Only the highest of `hd`, `full_hd`, `4k` and `8k` applies. Their short
//! sides are two thirds of the standard's, so cropped widescreen still counts.
//!
//! A fact that isn't known (no dimensions, no duration, audio not probed)
//! adds nothing.
//!
//! Each post is tagged once per [`META_VERSION`]: a later version adds only
//! the tags new in it, so a tag someone removed doesn't come back.

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

/// The resolution tags, highest first: (tag, long side, short side).
const RESOLUTIONS: [(&str, u32, u32); 4] = [
    ("8k", 7680, 2880),
    ("4k", 3840, 1440),
    ("full_hd", 1920, 720),
    ("hd", 1280, 480),
];
/// `square_image`: the long side is at most this percent of the short one.
const SQUARE_PERCENT: u64 = 101;

/// The current set of rules. Raise it when adding tags, and list them in
/// [`added_in`].
pub const META_VERSION: i16 = 2;

/// The [`META_VERSION`] that brought a tag.
pub fn added_in(tag: &str) -> i16 {
    match tag {
        "hd" | "full_hd" | "4k" | "8k" | "horizontal_image" | "vertical_image" | "square_image" => {
            2
        }
        _ => 1,
    }
}

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
    "hd",
    "full_hd",
    "4k",
    "8k",
    "horizontal_image",
    "vertical_image",
    "square_image",
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
    if let Some((w, h)) = size {
        let (long, short) = (w.max(h), w.min(h));
        if let Some((tag, ..)) = RESOLUTIONS
            .iter()
            .find(|(_, l, s)| long >= *l && short >= *s)
        {
            tags.push(*tag);
        }
        tags.push(
            if u64::from(long).saturating_mul(100)
                <= u64::from(short).saturating_mul(SQUARE_PERCENT)
            {
                "square_image"
            } else if w > h {
                "horizontal_image"
            } else {
                "vertical_image"
            },
        );
    }
    tags
}

/// The tags in [`meta_tags`] that a post tagged at `version` hasn't had yet.
pub fn meta_tags_since(facts: &MediaFacts, version: i16) -> Vec<&'static str> {
    let mut tags = meta_tags(facts);
    tags.retain(|tag| added_in(tag) > version);
    tags
}

#[cfg(test)]
mod tests {
    use super::*;

    fn all_for_image(w: u32, h: u32) -> Vec<&'static str> {
        meta_tags(&MediaFacts {
            kind: MediaKind::Image,
            width: Some(w),
            height: Some(h),
            duration_ms: None,
            has_audio: None,
        })
    }

    /// The first version's tags, which these tests cover.
    fn first_only(mut tags: Vec<&'static str>) -> Vec<&'static str> {
        tags.retain(|tag| added_in(tag) == 1);
        tags
    }

    fn image(w: u32, h: u32) -> Vec<&'static str> {
        first_only(all_for_image(w, h))
    }

    fn video(duration_ms: Option<i64>, has_audio: Option<bool>) -> Vec<&'static str> {
        first_only(meta_tags(&MediaFacts {
            kind: MediaKind::Video,
            width: Some(1280),
            height: Some(720),
            duration_ms,
            has_audio,
        }))
    }

    /// What the second version's rules add.
    fn second(w: u32, h: u32) -> Vec<&'static str> {
        meta_tags_since(
            &MediaFacts {
                kind: MediaKind::Image,
                width: Some(w),
                height: Some(h),
                duration_ms: None,
                has_audio: None,
            },
            1,
        )
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
        assert_eq!(first_only(meta_tags(&gif)), ["lowres", "animated"]);
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
    fn one_resolution_class_applies() {
        assert_eq!(second(1279, 720), ["horizontal_image"]);
        assert_eq!(second(1280, 479), ["horizontal_image"]);
        assert_eq!(second(1280, 720), ["hd", "horizontal_image"]);
        assert_eq!(second(1920, 1080), ["full_hd", "horizontal_image"]);
        // cropped widescreen, and a phone's portrait
        assert_eq!(second(1920, 800), ["full_hd", "horizontal_image"]);
        assert_eq!(second(1080, 1920), ["full_hd", "vertical_image"]);
        assert_eq!(second(2560, 1440), ["full_hd", "horizontal_image"]);
        assert_eq!(second(3840, 2160), ["4k", "horizontal_image"]);
        assert_eq!(second(7680, 4320), ["8k", "horizontal_image"]);
        assert_eq!(second(8000, 8000), ["8k", "square_image"]);
    }

    #[test]
    fn orientation_has_a_margin_for_square() {
        assert_eq!(second(100, 100), ["square_image"]);
        assert_eq!(second(101, 100), ["square_image"]);
        assert_eq!(second(100, 101), ["square_image"]);
        assert_eq!(second(102, 100), ["horizontal_image"]);
        assert_eq!(second(100, 102), ["vertical_image"]);
        assert_eq!(second(u32::MAX, u32::MAX), ["8k", "square_image"]);
        assert!(second(0, 100).is_empty());
    }

    #[test]
    fn a_new_version_adds_only_its_tags() {
        let facts = MediaFacts {
            kind: MediaKind::Image,
            width: Some(400),
            height: Some(400),
            duration_ms: None,
            has_audio: None,
        };
        assert_eq!(meta_tags_since(&facts, 0), ["lowres", "square_image"]);
        assert_eq!(meta_tags_since(&facts, 1), ["square_image"]);
        assert!(meta_tags_since(&facts, META_VERSION).is_empty());
    }

    #[test]
    fn every_tag_is_listed_and_valid() {
        for tag in META_TAGS {
            assert_eq!(crate::normalize_tag(tag).as_deref(), Ok(*tag));
        }
        let all = [
            all_for_image(1, 1),
            all_for_image(20_000, 10),
            all_for_image(10, 20_000),
            all_for_image(1280, 720),
            all_for_image(1920, 1080),
            all_for_image(3840, 2160),
            all_for_image(7680, 4320),
            video(Some(60_000), Some(true)),
        ];
        for tag in all.iter().flatten() {
            assert!(META_TAGS.contains(tag), "{tag}");
        }
        for tag in META_TAGS {
            assert!((1..=META_VERSION).contains(&added_in(tag)), "{tag}");
        }
    }
}
