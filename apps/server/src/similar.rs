//! Near-duplicate detection.
//!
//! Each thumbnail gets a 64-bit difference hash: the picture shrunk to 9×8
//! grey pixels, one bit per pair of horizontal neighbours (is the left one
//! brighter?). Resizing, recompression and small edits change few bits, so
//! the Hamming distance between two hashes measures how alike they look.
//! The hash only sees brightness, so each fingerprint also keeps the
//! average colour, and two posts only match when that is close too (a
//! recoloured copy is a different picture).
//!
//! Grouping avoids comparing every pair. The hash is split into four 16-bit
//! chunks; when two hashes differ by at most `d` bits, one chunk differs by
//! at most `d / 4` (pigeonhole). Each hash is only compared with those that
//! share a chunk within that radius, found through one index per chunk.

use std::collections::{HashMap, HashSet};

use image::DynamicImage;
use image::imageops::FilterType;

const CHUNKS: u32 = 4;
const CHUNK_BITS: u32 = 16;
/// Largest difference per colour channel (0–255) between matching posts.
const COLOR_TOLERANCE: u32 = 24;

/// What the finder compares: brightness structure and average colour.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Fingerprint {
    pub hash: u64,
    /// `0xRRGGBB`.
    pub color: u32,
}

impl Fingerprint {
    pub fn of(img: &DynamicImage) -> Self {
        Self {
            hash: dhash(img),
            color: mean_color(img),
        }
    }

    /// The hash as Postgres stores it (`BIGINT`), bit for bit.
    pub fn hash_db(self) -> i64 {
        i64::from_be_bytes(self.hash.to_be_bytes())
    }

    pub fn color_db(self) -> i32 {
        i32::try_from(self.color & 0xff_ffff).unwrap_or(0)
    }

    pub fn from_db(hash: i64, color: i32) -> Self {
        Self {
            hash: u64::from_be_bytes(hash.to_be_bytes()),
            color: u32::try_from(color).unwrap_or(0),
        }
    }

    fn matches(self, other: Self, max_distance: u32) -> Option<u32> {
        let d = distance(self.hash, other.hash);
        (d <= max_distance && colors_close(self.color, other.color)).then_some(d)
    }
}

/// Difference hash of a picture.
pub fn dhash(img: &DynamicImage) -> u64 {
    let small = img.resize_exact(9, 8, FilterType::Triangle).to_luma8();
    let luma = |x: u32, y: u32| small.get_pixel_checked(x, y).map_or(0, |p| p.0[0]);
    let mut bits = 0u64;
    for y in 0..8 {
        for x in 0..8 {
            bits = (bits << 1) | u64::from(luma(x, y) > luma(x + 1, y));
        }
    }
    bits
}

fn mean_color(img: &DynamicImage) -> u32 {
    let small = img.resize_exact(8, 8, FilterType::Triangle).to_rgb8();
    let mut sum = [0u32; 3];
    for p in small.pixels() {
        for (s, c) in sum.iter_mut().zip(p.0) {
            *s += u32::from(c);
        }
    }
    let n = (small.width() * small.height()).max(1);
    let [r, g, b] = sum.map(|s| (s / n).min(255));
    r << 16 | g << 8 | b
}

fn colors_close(a: u32, b: u32) -> bool {
    [16, 8, 0]
        .iter()
        .all(|shift| ((a >> shift) & 0xff).abs_diff((b >> shift) & 0xff) <= COLOR_TOLERANCE)
}

pub fn distance(a: u64, b: u64) -> u32 {
    (a ^ b).count_ones()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Group {
    /// Post ids, ascending.
    pub ids: Vec<i64>,
    /// Largest distance between two members found within reach of each other.
    pub distance: u32,
}

/// Groups posts whose hashes are within `max_distance` of another member
/// (transitively) and whose colours are close. Pairs in `dismissed` (smaller id first) are never linked.
/// Groups come back closest first, then largest, then by first id.
pub fn group(
    items: &[(i64, Fingerprint)],
    max_distance: u32,
    dismissed: &HashSet<(i64, i64)>,
) -> Vec<Group> {
    let radius = max_distance / CHUNKS;
    let chunk = |hash: u64, c: u32| ((hash >> (c * CHUNK_BITS)) & 0xffff) as u16;
    let mut index: Vec<HashMap<u16, Vec<usize>>> = vec![HashMap::new(); CHUNKS as usize];
    for (i, (_, print)) in items.iter().enumerate() {
        for (c, map) in (0..CHUNKS).zip(index.iter_mut()) {
            map.entry(chunk(print.hash, c)).or_default().push(i);
        }
    }

    let mut sets = DisjointSets::new(items.len());
    let mut seen = HashSet::new();
    for (i, &(id, print)) in items.iter().enumerate() {
        seen.clear();
        for (c, map) in (0..CHUNKS).zip(index.iter()) {
            for probe in neighbours(chunk(print.hash, c), radius) {
                for &j in map.get(&probe).into_iter().flatten() {
                    let Some(&(other, other_print)) = items.get(j) else {
                        continue;
                    };
                    if j <= i || !seen.insert(j) {
                        continue;
                    }
                    if let Some(d) = print.matches(other_print, max_distance)
                        && !dismissed.contains(&(id.min(other), id.max(other)))
                    {
                        sets.union(i, j, d);
                    }
                }
            }
        }
    }

    let mut members: HashMap<usize, Vec<i64>> = HashMap::new();
    for (i, (id, _)) in items.iter().enumerate() {
        members.entry(sets.find(i)).or_default().push(*id);
    }
    let mut groups: Vec<Group> = members
        .into_iter()
        .filter(|(_, ids)| ids.len() > 1)
        .map(|(root, mut ids)| {
            ids.sort_unstable();
            Group {
                ids,
                distance: sets.distance.get(root).copied().unwrap_or(0),
            }
        })
        .collect();
    groups.sort_by(|a, b| {
        a.distance
            .cmp(&b.distance)
            .then(b.ids.len().cmp(&a.ids.len()))
            .then(a.ids.first().cmp(&b.ids.first()))
    });
    groups
}

/// `value` and every 16-bit value within `radius` (at most 2) bits of it.
fn neighbours(value: u16, radius: u32) -> Vec<u16> {
    let mut out = vec![value];
    if radius >= 1 {
        for a in 0..CHUNK_BITS {
            out.push(value ^ (1 << a));
        }
    }
    if radius >= 2 {
        for a in 0..CHUNK_BITS {
            for b in (a + 1)..CHUNK_BITS {
                out.push(value ^ (1 << a) ^ (1 << b));
            }
        }
    }
    out
}

/// Union-find that remembers the largest link distance per set.
struct DisjointSets {
    parent: Vec<usize>,
    distance: Vec<u32>,
}

impl DisjointSets {
    fn new(n: usize) -> Self {
        Self {
            parent: (0..n).collect(),
            distance: vec![0; n],
        }
    }

    fn find(&mut self, mut i: usize) -> usize {
        let mut root = i;
        while let Some(&p) = self.parent.get(root) {
            if p == root {
                break;
            }
            root = p;
        }
        // Path compression.
        while let Some(&p) = self.parent.get(i) {
            if p == root {
                break;
            }
            if let Some(slot) = self.parent.get_mut(i) {
                *slot = root;
            }
            i = p;
        }
        root
    }

    fn union(&mut self, a: usize, b: usize, d: u32) {
        let (ra, rb) = (self.find(a), self.find(b));
        let joined = d
            .max(self.distance.get(ra).copied().unwrap_or(0))
            .max(self.distance.get(rb).copied().unwrap_or(0));
        if ra != rb
            && let Some(slot) = self.parent.get_mut(rb)
        {
            *slot = ra;
        }
        if let Some(slot) = self.distance.get_mut(ra) {
            *slot = joined;
        }
    }
}

#[cfg(test)]
pub mod tests {
    use std::io::Cursor;

    use image::{ImageFormat, Rgb, RgbImage};

    use super::*;

    /// A generated test pattern: diagonal stripes whose angle depends on `seed`.
    pub fn pattern(seed: u32, w: u32, h: u32) -> DynamicImage {
        DynamicImage::ImageRgb8(RgbImage::from_fn(w, h, |x, y| {
            let v = ((x * (seed + 1) + y * (7 - seed % 7)) * 255 / (w + h)) % 256;
            let v = v as u8;
            Rgb([v, v.wrapping_mul(3), 255 - v])
        }))
    }

    fn jpeg(img: &DynamicImage, quality: u8) -> DynamicImage {
        let mut out = Cursor::new(Vec::new());
        image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, quality)
            .encode_image(&img.to_rgb8())
            .unwrap();
        image::load_from_memory_with_format(out.get_ref(), ImageFormat::Jpeg).unwrap()
    }

    #[test]
    fn similar_pictures_hash_alike() {
        let original = pattern(2, 640, 480);
        let smaller = jpeg(&original.resize_exact(320, 240, FilterType::Lanczos3), 60);
        let other = pattern(5, 640, 480);
        let f = Fingerprint::of(&original);
        assert!(f.matches(Fingerprint::of(&smaller), 3).is_some());
        assert!(distance(f.hash, dhash(&other)) > 12);
        // Flipped left to right inverts most comparisons.
        assert!(distance(f.hash, dhash(&original.fliph())) > 20);
        // Same brightness structure, other colours: only the colour tells.
        let grey = DynamicImage::ImageRgb8(RgbImage::from_fn(320, 240, |x, y| {
            let v = u8::try_from((x * 3 + y * 5) % 256).unwrap();
            Rgb([v, v, v])
        }));
        let tinted = DynamicImage::ImageRgb8(RgbImage::from_fn(320, 240, |x, y| {
            let v = u8::try_from((x * 3 + y * 5) % 256).unwrap() / 2;
            Rgb([v, v, v + 120])
        }));
        let (g, t) = (Fingerprint::of(&grey), Fingerprint::of(&tinted));
        assert!(distance(g.hash, t.hash) <= 2);
        assert_eq!(g.matches(t, 10), None);

        let edge = Fingerprint {
            hash: 1 << 63,
            color: 0xff_ffff,
        };
        assert_eq!(edge.hash_db(), i64::MIN);
        assert_eq!(Fingerprint::from_db(edge.hash_db(), edge.color_db()), edge);
        assert_eq!(Fingerprint::from_db(0, -5).color, 0);
    }

    #[test]
    fn averages_colour() {
        let red = DynamicImage::ImageRgb8(image::RgbImage::from_pixel(30, 20, Rgb([200, 10, 0])));
        assert_eq!(mean_color(&red), 0xc8_0a00);
        assert!(colors_close(0x80_8080, 0x98_6868));
        assert!(!colors_close(0x80_8080, 0xa8_5858));
        assert!(!colors_close(0x80_8080, 0x80_80ff));
    }

    #[test]
    fn groups_within_distance() {
        let base = 0x0123_4567_89ab_cdefu64;
        let items = prints(&[
            (1, base),
            (2, base ^ 0b1),                // 1 bit from 1
            (3, base ^ 0b111 << 20),        // 3 bits from 1
            (4, !base),                     // far away
            (5, !base ^ (0b11_1111 << 40)), // 6 bits from 4
            (6, 0xffff_0000_ffff_0000),     // alone
        ]);
        let none = HashSet::new();
        let g = group(&items, 6, &none);
        assert_eq!(
            g,
            [
                Group {
                    ids: vec![1, 2, 3],
                    distance: 4
                },
                Group {
                    ids: vec![4, 5],
                    distance: 6
                },
            ]
        );
        let g = group(&items, 1, &none);
        assert_eq!(
            g,
            [Group {
                ids: vec![1, 2],
                distance: 1
            }]
        );
        assert!(group(&items, 0, &none).is_empty());
        // A dismissed pair isn't linked, but the group can still form through
        // another member.
        let dismissed: HashSet<(i64, i64)> = [(1, 2), (4, 5)].into();
        assert_eq!(
            group(&items, 6, &dismissed),
            [Group {
                ids: vec![1, 2, 3],
                distance: 4
            }]
        );
        assert!(group(&[], 10, &none).is_empty());
        // Same structure, different colour: no match.
        let mut recoloured = items.clone();
        if let Some(item) = recoloured.get_mut(1) {
            item.1.color = 0x00_ff00;
        }
        assert_eq!(group(&recoloured, 1, &none), Vec::<Group>::new());
    }

    fn prints(raw: &[(i64, u64)]) -> Vec<(i64, Fingerprint)> {
        raw.iter()
            .map(|&(id, hash)| {
                (
                    id,
                    Fingerprint {
                        hash,
                        color: 0x80_8080,
                    },
                )
            })
            .collect()
    }

    #[test]
    fn finds_every_pair_up_to_the_largest_distance() {
        // Ten bits spread over all four chunks: no chunk matches exactly, so
        // this only works through the radius-2 probes.
        let a = 0u64;
        let b = 0b11 | 0b111 << 16 | 0b11 << 32 | 0b111 << 48;
        assert_eq!(distance(a, b), 10);
        let items = prints(&[(1, a), (2, b)]);
        let g = group(&items, 10, &HashSet::new());
        assert_eq!(
            g,
            [Group {
                ids: vec![1, 2],
                distance: 10
            }]
        );
        assert!(group(&items, 9, &HashSet::new()).is_empty());
    }

    #[test]
    fn identical_hashes_group_once() {
        let raw: Vec<(i64, u64)> = (1..=40).map(|i| (i, 42)).collect();
        let items = prints(&raw);
        let g = group(&items, 0, &HashSet::new());
        assert_eq!(g.len(), 1);
        assert_eq!(g[0].ids.len(), 40);
        assert_eq!(g[0].distance, 0);
    }
}
