//! Simple media edits: turn, flip, straighten and crop an image, trim a video.
//!
//! Media is never changed in place: an edit makes a new post with the
//! original's tags, rating and source, and can move the original to the
//! recycle bin.

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::model::MediaKind;

/// Shortest clip a trim may leave.
pub const MIN_TRIM_MS: i64 = 100;

/// The furthest an image can be straightened either way, in tenths of a
/// degree: 45°. Past that a quarter turn the other way is closer.
pub const MAX_STRAIGHTEN: i16 = 450;

/// Image formats the server can decode and write back in the same format.
/// GIF is left out: only its first frame would survive.
const EDITABLE_IMAGES: [&str; 5] = [
    "image/jpeg",
    "image/png",
    "image/webp",
    "image/bmp",
    "image/tiff",
];

/// What the editor offers for a post.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditKind {
    /// Turn, flip, crop.
    Image,
    /// Trim.
    Video,
}

/// `None` for posts that can't be edited (GIF, AVIF, JPEG XL).
pub fn edit_kind(kind: MediaKind, mime: &str) -> Option<EditKind> {
    match kind {
        MediaKind::Image if EDITABLE_IMAGES.contains(&mime) => Some(EditKind::Image),
        MediaKind::Video => Some(EditKind::Video),
        _ => None,
    }
}

/// A rectangle in pixels of the turned and flipped image.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Crop {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

/// The part of a video to keep.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Trim {
    pub start_ms: i64,
    pub end_ms: i64,
}

/// `POST /posts/{id}/edit`. Applied in this order: turn, flip, straighten, crop.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MediaEdit {
    /// Clockwise degrees: 0, 90, 180 or 270.
    #[serde(default)]
    pub rotate: u16,
    /// Mirror left to right, after turning.
    #[serde(default)]
    pub flip_h: bool,
    /// Mirror top to bottom, after turning.
    #[serde(default)]
    pub flip_v: bool,
    /// A small clockwise turn after the flips, in tenths of a degree, from
    /// -450 to 450. The picture grows around its middle just enough to fill
    /// its frame again (see [`MediaEdit::straighten_scale`]), so no empty
    /// corners show and the size stays the same.
    #[serde(default)]
    pub straighten: i16,
    #[serde(default)]
    pub crop: Option<Crop>,
    /// Videos only.
    #[serde(default)]
    pub trim: Option<Trim>,
    /// Move the original to the recycle bin once the edit is saved.
    #[serde(default)]
    pub bin_original: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum EditError {
    #[error("this file type can't be edited")]
    NotEditable,
    #[error("nothing to change")]
    NoChange,
    #[error("turn by 0, 90, 180 or 270 degrees, and straighten by at most 45")]
    BadRotation,
    #[error("the crop must be inside the image")]
    CropOutside,
    #[error("images can't be trimmed, and videos can only be trimmed")]
    WrongKind,
    #[error("the trim must be inside the video")]
    TrimOutside,
    #[error("the trimmed clip is too short")]
    TrimTooShort,
}

impl MediaEdit {
    /// Turns the result a quarter clockwise (or anticlockwise). Flips swap
    /// axes with it, and the crop is dropped.
    pub fn turn(&mut self, clockwise: bool) {
        self.rotate = (self.rotate % 360 + if clockwise { 90 } else { 270 }) % 360;
        std::mem::swap(&mut self.flip_h, &mut self.flip_v);
        self.crop = None;
    }

    /// The same turn and flips with both flips folded into a half turn.
    pub fn orientation(&self) -> (u16, bool, bool) {
        if self.flip_h && self.flip_v {
            ((self.rotate % 360 + 180) % 360, false, false)
        } else {
            (self.rotate % 360, self.flip_h, self.flip_v)
        }
    }

    /// How much a `width` × `height` frame's picture grows once straightened,
    /// so that the turned picture still covers the whole frame. 1 when it
    /// isn't straightened.
    pub fn straighten_scale(&self, width: u32, height: u32) -> f64 {
        let (w, h) = (f64::from(width.max(1)), f64::from(height.max(1)));
        let turn = (f64::from(self.straighten) / 10.0).abs().to_radians();
        // The frame's corners have to stay inside the turned picture: the
        // longer side over the shorter is what the sine pushes out the most.
        turn.cos() + (w / h).max(h / w) * turn.sin()
    }

    /// Size after turning a `width` × `height` image.
    pub fn turned_size(&self, width: u32, height: u32) -> (u32, u32) {
        if self.rotate % 180 == 90 {
            (height, width)
        } else {
            (width, height)
        }
    }

    /// Checks the edit against the post it applies to. `width` and `height`
    /// are the image as shown (EXIF orientation applied).
    pub fn check(
        &self,
        kind: EditKind,
        width: u32,
        height: u32,
        duration_ms: Option<i64>,
    ) -> Result<(), EditError> {
        if !matches!(self.rotate, 0 | 90 | 180 | 270)
            || !(-MAX_STRAIGHTEN..=MAX_STRAIGHTEN).contains(&self.straighten)
        {
            return Err(EditError::BadRotation);
        }
        match kind {
            EditKind::Image => {
                if self.trim.is_some() {
                    return Err(EditError::WrongKind);
                }
                let (w, h) = self.turned_size(width, height);
                let crop = match self.crop {
                    Some(c)
                        if c == (Crop {
                            x: 0,
                            y: 0,
                            width: w,
                            height: h,
                        }) =>
                    {
                        None
                    }
                    other => other,
                };
                if let Some(c) = crop {
                    let fits = |at: u32, len: u32, max: u32| {
                        len > 0 && at.checked_add(len).is_some_and(|end| end <= max)
                    };
                    if !fits(c.x, c.width, w) || !fits(c.y, c.height, h) {
                        return Err(EditError::CropOutside);
                    }
                }
                if self.orientation() == (0, false, false) && self.straighten == 0 && crop.is_none()
                {
                    return Err(EditError::NoChange);
                }
            }
            EditKind::Video => {
                if self.orientation() != (0, false, false)
                    || self.straighten != 0
                    || self.crop.is_some()
                {
                    return Err(EditError::WrongKind);
                }
                let Some(t) = self.trim else {
                    return Err(EditError::NoChange);
                };
                let duration = duration_ms.ok_or(EditError::TrimOutside)?;
                if t.start_ms < 0 || t.end_ms > duration || t.start_ms >= t.end_ms {
                    return Err(EditError::TrimOutside);
                }
                if t.end_ms - t.start_ms < MIN_TRIM_MS {
                    return Err(EditError::TrimTooShort);
                }
                if t.start_ms == 0 && t.end_ms == duration {
                    return Err(EditError::NoChange);
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn editable_kinds() {
        assert_eq!(
            edit_kind(MediaKind::Image, "image/png"),
            Some(EditKind::Image)
        );
        assert_eq!(edit_kind(MediaKind::Image, "image/avif"), None);
        assert_eq!(edit_kind(MediaKind::Animation, "image/gif"), None);
        assert_eq!(
            edit_kind(MediaKind::Video, "video/webm"),
            Some(EditKind::Video)
        );
    }

    #[test]
    fn turning_and_flipping() {
        let mut e = MediaEdit {
            flip_h: true,
            crop: Some(Crop {
                x: 1,
                y: 1,
                width: 2,
                height: 2,
            }),
            ..MediaEdit::default()
        };
        e.turn(true);
        assert_eq!((e.rotate, e.flip_h, e.flip_v), (90, false, true));
        assert_eq!(e.crop, None);
        e.turn(false);
        e.turn(false);
        assert_eq!((e.rotate, e.flip_h, e.flip_v), (270, false, true));
        assert_eq!(e.turned_size(40, 30), (30, 40));
        // Both flips are a half turn.
        e.flip_h = true;
        assert_eq!(e.orientation(), (90, false, false));
        e.rotate = 180;
        assert_eq!(e.orientation(), (0, false, false));
    }

    #[test]
    fn checks_images() {
        let edit = |rotate, crop| MediaEdit {
            rotate,
            crop,
            ..MediaEdit::default()
        };
        let c = |x, y, width, height| {
            Some(Crop {
                x,
                y,
                width,
                height,
            })
        };
        let check = |e: MediaEdit| e.check(EditKind::Image, 40, 30, None);
        assert_eq!(check(edit(0, None)), Err(EditError::NoChange));
        assert_eq!(check(edit(0, c(0, 0, 40, 30))), Err(EditError::NoChange));
        assert_eq!(check(edit(45, None)), Err(EditError::BadRotation));
        assert!(check(edit(90, None)).is_ok());
        assert!(check(edit(0, c(10, 5, 30, 25))).is_ok());
        assert_eq!(
            check(edit(0, c(10, 5, 31, 25))),
            Err(EditError::CropOutside)
        );
        assert_eq!(check(edit(0, c(0, 0, 0, 5))), Err(EditError::CropOutside));
        assert_eq!(
            check(edit(0, c(u32::MAX, 0, 2, 5))),
            Err(EditError::CropOutside)
        );
        // After a quarter turn the image is 30 × 40.
        assert!(check(edit(90, c(0, 0, 30, 40))).is_ok());
        assert_eq!(
            check(edit(90, c(0, 0, 40, 30))),
            Err(EditError::CropOutside)
        );
        let both = MediaEdit {
            rotate: 180,
            flip_h: true,
            flip_v: true,
            ..MediaEdit::default()
        };
        assert_eq!(check(both), Err(EditError::NoChange));
        let trim = MediaEdit {
            trim: Some(Trim {
                start_ms: 0,
                end_ms: 10,
            }),
            rotate: 90,
            ..MediaEdit::default()
        };
        assert_eq!(check(trim), Err(EditError::WrongKind));
    }

    #[test]
    fn checks_straightening() {
        let tilt = |straighten| MediaEdit {
            straighten,
            ..MediaEdit::default()
        };
        let check = |e: MediaEdit| e.check(EditKind::Image, 40, 30, None);
        assert!(check(tilt(1)).is_ok());
        assert!(check(tilt(450)).is_ok());
        assert!(check(tilt(-450)).is_ok());
        assert_eq!(check(tilt(451)), Err(EditError::BadRotation));
        assert_eq!(check(tilt(i16::MIN)), Err(EditError::BadRotation));
        let trim = MediaEdit {
            trim: Some(Trim {
                start_ms: 0,
                end_ms: 10,
            }),
            ..tilt(10)
        };
        assert_eq!(
            trim.check(EditKind::Video, 40, 30, Some(1_000)),
            Err(EditError::WrongKind)
        );
        // An old client leaves it out.
        let e: MediaEdit = serde_json::from_str(r#"{"rotate": 90}"#).unwrap();
        assert_eq!(e.straighten, 0);
    }

    #[test]
    fn straightened_pictures_fill_their_frame() {
        let tilt = |straighten| MediaEdit {
            straighten,
            ..MediaEdit::default()
        };
        assert_eq!(tilt(0).straighten_scale(40, 30), 1.0);
        // A square at 45° grows by the square root of two, either way.
        let s = tilt(450).straighten_scale(10, 10);
        assert!((s - 2_f64.sqrt()).abs() < 1e-9, "{s}");
        assert_eq!(tilt(-450).straighten_scale(10, 10), s);
        // Each corner of a 2 × 1 frame, turned back by the angle and shrunk
        // by the scale, lands inside the picture.
        for tenths in [-450, -123, 7, 300, 450] {
            let e = tilt(tenths);
            let s = e.straighten_scale(200, 100);
            let (sin, cos) = (f64::from(tenths) / 10.0).to_radians().sin_cos();
            for (x, y) in [
                (100.0, 50.0),
                (-100.0, 50.0),
                (100.0, -50.0),
                (-100.0, -50.0),
            ] {
                let (u, v) = ((x * cos + y * sin) / s, (-x * sin + y * cos) / s);
                assert!(
                    u.abs() <= 100.0 + 1e-9 && v.abs() <= 50.0 + 1e-9,
                    "{tenths}: {u} {v}"
                );
            }
        }
        // Zero sides don't divide by zero.
        assert!(tilt(100).straighten_scale(0, 0).is_finite());
    }

    #[test]
    fn checks_videos() {
        let trim = |start_ms, end_ms| MediaEdit {
            trim: Some(Trim { start_ms, end_ms }),
            ..MediaEdit::default()
        };
        let check = |e: MediaEdit| e.check(EditKind::Video, 640, 360, Some(10_000));
        assert!(check(trim(1_000, 4_000)).is_ok());
        assert_eq!(check(trim(0, 10_000)), Err(EditError::NoChange));
        assert_eq!(check(MediaEdit::default()), Err(EditError::NoChange));
        assert_eq!(check(trim(-1, 4_000)), Err(EditError::TrimOutside));
        assert_eq!(check(trim(1_000, 10_001)), Err(EditError::TrimOutside));
        assert_eq!(check(trim(4_000, 4_000)), Err(EditError::TrimOutside));
        assert_eq!(check(trim(4_000, 4_050)), Err(EditError::TrimTooShort));
        assert_eq!(
            trim(0, 5).check(EditKind::Video, 1, 1, None),
            Err(EditError::TrimOutside)
        );
        let turned = MediaEdit {
            rotate: 90,
            ..trim(1_000, 4_000)
        };
        assert_eq!(check(turned), Err(EditError::WrongKind));
    }
}
