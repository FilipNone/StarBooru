//! The StarBooru mark and wordmark, in the current colour. docs/branding.md says
//! which to use where; the drawings come from tools/brand/brand.py.

use dioxus::prelude::*;

use crate::brand_paths as paths;

/// The crown over the compass star. `compact` is the heavier cut for anything
/// under 48 px wide.
#[component]
pub fn Mark(height: u32, #[props(default)] compact: bool) -> Element {
    let (view_height, crown, star) = if compact {
        (
            paths::COMPACT_HEIGHT,
            paths::COMPACT_CROWN,
            paths::COMPACT_STAR,
        )
    } else {
        (paths::FULL_HEIGHT, paths::FULL_CROWN, paths::FULL_STAR)
    };
    let width = height.saturating_mul(paths::WIDTH) / view_height.max(1);
    let view_box = format!("0 0 {} {view_height}", paths::WIDTH);
    rsx! {
        svg {
            class: "mark",
            width: "{width}",
            height: "{height}",
            view_box,
            fill: "currentColor",
            "aria-hidden": "true",
            path { d: crown }
            path { class: "mark-star", d: star }
        }
    }
}

/// The name, drawn so it looks the same whatever fonts a device has.
#[component]
pub fn Wordmark(height: u32) -> Element {
    let width = height.saturating_mul(paths::WORDMARK_WIDTH) / paths::WORDMARK_HEIGHT.max(1);
    let view_box = format!("0 0 {} {}", paths::WORDMARK_WIDTH, paths::WORDMARK_HEIGHT);
    rsx! {
        svg {
            class: "wordmark",
            width: "{width}",
            height: "{height}",
            view_box,
            fill: "currentColor",
            "aria-hidden": "true",
            path { d: paths::WORDMARK }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::paths;
    use std::collections::HashSet;

    /// Every point of a cubic-only path, in hundredths.
    fn points(d: &str) -> Vec<(i64, i64)> {
        let numbers: Vec<i64> = d
            .split(|c: char| matches!(c, 'M' | 'C' | 'Z' | ' '))
            .filter(|s| !s.is_empty())
            .map(|s| {
                let v: f64 = s.parse().unwrap();
                (v * 100.0).round() as i64
            })
            .collect();
        assert_eq!(numbers.len() % 2, 0, "unpaired coordinate in {d}");
        numbers.chunks(2).map(|p| (p[0], p[1])).collect()
    }

    #[test]
    fn the_mark_is_symmetrical_left_to_right() {
        let axis = i64::from(paths::WIDTH) * 100;
        for d in [
            paths::FULL_CROWN,
            paths::FULL_STAR,
            paths::COMPACT_CROWN,
            paths::COMPACT_STAR,
        ] {
            let all: HashSet<_> = points(d).into_iter().collect();
            for (x, y) in &all {
                assert!(all.contains(&(axis - x, *y)), "no mirror for {x} {y}");
            }
        }
    }

    #[test]
    fn the_drawings_stay_inside_their_boxes() {
        for (d, height) in [
            (paths::FULL_CROWN, paths::FULL_HEIGHT),
            (paths::FULL_STAR, paths::FULL_HEIGHT),
            (paths::COMPACT_CROWN, paths::COMPACT_HEIGHT),
            (paths::COMPACT_STAR, paths::COMPACT_HEIGHT),
        ] {
            // Only the points on the curve: its handles may reach outside.
            for (x, y) in points(d).into_iter().step_by(3) {
                assert!((0..=i64::from(paths::WIDTH) * 100).contains(&x), "x {x}");
                assert!((0..=i64::from(height) * 100).contains(&y), "y {y}");
            }
        }
    }
}
