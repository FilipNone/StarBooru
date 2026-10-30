//! Colour search: the colours a picture is stored under, and the `color:` term.
//!
//! Colours are compared in OKLab, where a distance looks about the same to the
//! eye anywhere in the space. The space is cut into a grid and a picture is
//! stored under the cells most of its pixels fall in, so a search only has to
//! ask for the cells around a colour (a GIN index on an array). Lightness counts
//! for half as much as hue: a dark red is still red, and an orange isn't.
//!
//! * `color:red`: a named colour, one of [`NAMED_COLORS`].
//! * `color:2060e0`, `color:#2060e0`, `color:26e`: any colour, in hex.
//!
//! A search takes the narrowest of [`COLOR_REACHES`] that finds anything, so it
//! gets the nearest colours stored, without reaching as far as a grey for a red.

/// Red, green and blue, 0–255.
pub type Rgb = [u8; 3];

/// Grid lines of lightness (0–1).
const L_CELLS: i16 = 13;
/// Grid lines of each of a and b. Every sRGB colour falls within these.
const AB_CELLS: i16 = 17;
const L_STEP: f32 = 1.0 / (L_CELLS - 1) as f32;
const AB_STEP: f32 = 0.04;
const AB_ORIGIN: f32 = -0.32;
const L_WEIGHT: f32 = 0.5;

/// How far a `color:` term reaches, narrowest first.
pub const COLOR_REACHES: [f32; 3] = [0.10, 0.14, 0.18];
/// How far a `-color:` term reaches.
pub const EXCLUDE_REACH: f32 = 0.14;

/// A picture is stored under its most common cells, at most this many.
const MAX_CELLS: usize = 8;
/// A cell counts from this share of the picture (1 in n pixels)…
const MIN_SHARE: u64 = 33;
/// …and a noisy picture still keeps this many.
const MIN_CELLS: usize = 3;
/// Most colours shown for a picture, and how far apart they must be.
const MAX_SHOWN: usize = 6;
const SHOWN_APART: f32 = 0.08;

/// Named colours, in the order the picker shows them.
pub const NAMED_COLORS: [(&str, Rgb); 12] = [
    ("red", [0xe0, 0x20, 0x2a]),
    ("orange", [0xff, 0x80, 0x00]),
    ("yellow", [0xff, 0xe0, 0x00]),
    ("green", [0x20, 0xb0, 0x20]),
    ("teal", [0x00, 0xa0, 0xa0]),
    ("blue", [0x20, 0x60, 0xe0]),
    ("purple", [0x80, 0x30, 0xc0]),
    ("pink", [0xff, 0x70, 0xb0]),
    ("brown", [0x80, 0x48, 0x20]),
    ("black", [0x00, 0x00, 0x00]),
    ("gray", [0x80, 0x80, 0x80]),
    ("white", [0xff, 0xff, 0xff]),
];

/// A `color:` term.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ColorTerm {
    /// The name, or the hex digits without `#`, in lower case.
    pub raw: String,
    pub rgb: Rgb,
}

/// Reads the value of a `color:` term.
pub fn parse_color(value: &str) -> Option<ColorTerm> {
    let value = value.to_ascii_lowercase();
    let value = value.strip_prefix('#').unwrap_or(&value);
    let name = if value == "grey" { "gray" } else { value };
    if let Some((name, rgb)) = NAMED_COLORS.iter().find(|(n, _)| *n == name) {
        return Some(ColorTerm {
            raw: (*name).to_owned(),
            rgb: *rgb,
        });
    }
    let rgb = parse_hex(value)?;
    Some(ColorTerm {
        raw: value.to_owned(),
        rgb,
    })
}

/// `rrggbb` or `rgb`.
fn parse_hex(value: &str) -> Option<Rgb> {
    if !value.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let digit = |at: usize, len: usize| {
        value
            .get(at..at + len)
            .and_then(|d| u8::from_str_radix(d, 16).ok())
    };
    match value.len() {
        6 => Some([digit(0, 2)?, digit(2, 2)?, digit(4, 2)?]),
        3 => Some([digit(0, 1)? * 17, digit(1, 1)? * 17, digit(2, 1)? * 17]),
        _ => None,
    }
}

/// `rrggbb`, as a `color:` term takes it.
pub fn to_hex(rgb: Rgb) -> String {
    let [r, g, b] = rgb;
    format!("{r:02x}{g:02x}{b:02x}")
}

/// The colour at a point of the picker's wheel, with x and y from -1 to 1 from
/// its centre: the hue goes clockwise from red at the top, and the middle is
/// white. A point outside the circle counts as on its edge.
pub fn wheel_color(x: f64, y: f64) -> Rgb {
    let saturation = x.hypot(y).min(1.0);
    let hue = x.atan2(-y).to_degrees().rem_euclid(360.0) / 60.0;
    let rising = 1.0 - (hue.rem_euclid(2.0) - 1.0).abs();
    let (r, g, b) = match hue as u32 {
        0 => (1.0, rising, 0.0),
        1 => (rising, 1.0, 0.0),
        2 => (0.0, 1.0, rising),
        3 => (0.0, rising, 1.0),
        4 => (rising, 0.0, 1.0),
        _ => (1.0, 0.0, rising),
    };
    [r, g, b].map(|c: f64| (255.0 * (1.0 - saturation * (1.0 - c))).round() as u8)
}

/// The colours of a picture.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Palette {
    /// The grid cells it is searched by.
    pub cells: Vec<i16>,
    /// The colours shown for it, most common first, as `0xRRGGBB`.
    pub colors: Vec<i32>,
}

/// Works out the colours of a picture from its pixels.
pub fn palette_of(pixels: impl IntoIterator<Item = Rgb>) -> Palette {
    let mut counts = std::collections::HashMap::<i16, (u64, [u64; 3])>::new();
    let mut total = 0u64;
    for pixel in pixels {
        let (count, sum) = counts.entry(cell_of(oklab(pixel))).or_default();
        *count += 1;
        for (s, c) in sum.iter_mut().zip(pixel) {
            *s += u64::from(c);
        }
        total += 1;
    }
    let mut cells: Vec<(i16, u64, [u64; 3])> = counts
        .into_iter()
        .map(|(cell, (count, sum))| (cell, count, sum))
        .collect();
    cells.sort_unstable_by_key(|&(cell, count, _)| (std::cmp::Reverse(count), cell));
    let kept: Vec<_> = cells
        .into_iter()
        .enumerate()
        .take_while(|(i, (_, count, _))| *i < MIN_CELLS || count * MIN_SHARE >= total)
        .map(|(_, cell)| cell)
        .take(MAX_CELLS)
        .collect();

    let mut shown: Vec<Rgb> = Vec::new();
    for &(_, count, sum) in &kept {
        let mean = sum.map(|s| u8::try_from(s / count.max(1)).unwrap_or(u8::MAX));
        let apart = shown
            .iter()
            .all(|s| distance(oklab(*s), oklab(mean)) >= SHOWN_APART);
        if apart && shown.len() < MAX_SHOWN {
            shown.push(mean);
        }
    }
    let mut stored: Vec<i16> = kept.iter().map(|&(cell, ..)| cell).collect();
    stored.sort_unstable();
    Palette {
        cells: stored,
        colors: shown
            .into_iter()
            .map(|[r, g, b]| i32::from(r) << 16 | i32::from(g) << 8 | i32::from(b))
            .collect(),
    }
}

/// The grid cells within `reach` of a colour, ascending.
pub fn cells_near(rgb: Rgb, reach: f32) -> Vec<i16> {
    let target = oklab(rgb);
    let mut out = Vec::new();
    for l in 0..L_CELLS {
        for a in 0..AB_CELLS {
            for b in 0..AB_CELLS {
                let center = [
                    f32::from(l) * L_STEP,
                    AB_ORIGIN + f32::from(a) * AB_STEP,
                    AB_ORIGIN + f32::from(b) * AB_STEP,
                ];
                if distance(target, center) <= reach {
                    out.push(cell_index(l, a, b));
                }
            }
        }
    }
    out
}

/// `#rrggbb` of a stored colour.
pub fn stored_hex(color: i32) -> String {
    format!("#{:06x}", color & 0xff_ffff)
}

fn cell_index(l: i16, a: i16, b: i16) -> i16 {
    (l * AB_CELLS + a) * AB_CELLS + b
}

fn cell_of([l, a, b]: [f32; 3]) -> i16 {
    let line = |value: f32, step: f32, cells: i16| {
        ((value / step).round() as i16).clamp(0, cells.saturating_sub(1))
    };
    cell_index(
        line(l, L_STEP, L_CELLS),
        line(a - AB_ORIGIN, AB_STEP, AB_CELLS),
        line(b - AB_ORIGIN, AB_STEP, AB_CELLS),
    )
}

fn distance(x: [f32; 3], y: [f32; 3]) -> f32 {
    let [dl, da, db] = [x[0] - y[0], x[1] - y[1], x[2] - y[2]];
    ((L_WEIGHT * dl).powi(2) + da.powi(2) + db.powi(2)).sqrt()
}

/// sRGB to OKLab (Björn Ottosson's matrices).
fn oklab(rgb: Rgb) -> [f32; 3] {
    let [r, g, b] = rgb.map(|c| {
        let c = f32::from(c) / 255.0;
        if c <= 0.04045 {
            c / 12.92
        } else {
            ((c + 0.055) / 1.055).powf(2.4)
        }
    });
    let l = (0.412_221_46 * r + 0.536_332_55 * g + 0.051_445_995 * b).cbrt();
    let m = (0.211_903_5 * r + 0.680_699_5 * g + 0.107_396_96 * b).cbrt();
    let s = (0.088_302_46 * r + 0.281_718_85 * g + 0.629_978_7 * b).cbrt();
    [
        0.210_454_26 * l + 0.793_617_8 * m - 0.004_072_047 * s,
        1.977_998_5 * l - 2.428_592_2 * m + 0.450_593_7 * s,
        0.025_904_037 * l + 0.782_771_77 * m - 0.808_675_77 * s,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rgb(name: &str) -> Rgb {
        parse_color(name).unwrap().rgb
    }

    fn cell(rgb: Rgb) -> i16 {
        cell_of(oklab(rgb))
    }

    #[test]
    fn reads_names_and_hex() {
        let term = parse_color("RED").unwrap();
        assert_eq!(term.raw, "red");
        assert_eq!(parse_color("grey").unwrap().raw, "gray");
        for value in ["2060e0", "#2060E0"] {
            let term = parse_color(value).unwrap();
            assert_eq!(term.raw, "2060e0");
            assert_eq!(term.rgb, [0x20, 0x60, 0xe0]);
        }
        assert_eq!(parse_color("#f0a").unwrap().rgb, [0xff, 0x00, 0xaa]);
        for bad in ["", "#", "ff", "ffff", "12345g", "bluish", "#ééé", "ééé"] {
            assert_eq!(parse_color(bad), None, "{bad}");
        }
    }

    #[test]
    fn every_named_color_has_its_own_cell_and_parses() {
        let mut cells: Vec<i16> = NAMED_COLORS.iter().map(|(_, c)| cell(*c)).collect();
        cells.sort_unstable();
        cells.dedup();
        assert_eq!(cells.len(), NAMED_COLORS.len());
        for (name, color) in NAMED_COLORS {
            assert_eq!(parse_color(name).unwrap().rgb, color);
            assert_eq!(parse_color(&to_hex(color)).unwrap().rgb, color);
        }
    }

    #[test]
    fn oklab_matches_known_values() {
        let close = |a: [f32; 3], b: [f32; 3]| a.iter().zip(b).all(|(x, y)| (x - y).abs() < 0.002);
        assert!(close(oklab([255, 255, 255]), [1.0, 0.0, 0.0]));
        assert!(close(oklab([0, 0, 0]), [0.0, 0.0, 0.0]));
        assert!(close(oklab([255, 0, 0]), [0.628, 0.225, 0.126]));
        assert!(close(oklab([0, 0, 255]), [0.452, -0.032, -0.312]));
    }

    #[test]
    fn no_color_falls_off_the_grid() {
        // Every corner of the sRGB cube lies inside it, so none is clamped.
        for r in [0u8, 255] {
            for g in [0u8, 255] {
                for b in [0u8, 255] {
                    let [l, a, bb] = oklab([r, g, b]);
                    assert!((-0.001..=1.001).contains(&l));
                    assert!((AB_ORIGIN..=-AB_ORIGIN).contains(&a));
                    assert!((AB_ORIGIN..=-AB_ORIGIN).contains(&bb));
                }
            }
        }
        let top = cell_index(L_CELLS - 1, AB_CELLS - 1, AB_CELLS - 1);
        assert!(top > 0, "fits an i16");
    }

    #[test]
    fn reach_finds_near_colors_and_not_far_ones() {
        let widest = COLOR_REACHES[2];
        let red = cells_near(rgb("red"), widest);
        // Other reds, light and dark.
        for near in [[0xc0, 0x20, 0x20], [0xff, 0x30, 0x30], [0x90, 0x10, 0x18]] {
            assert!(red.contains(&cell(near)), "{near:?}");
        }
        // Even the widest reach stops short of grey, blue and green.
        for far in ["gray", "white", "black", "blue", "green", "teal"] {
            assert!(!red.contains(&cell(rgb(far))), "{far}");
        }
        assert!(
            !cells_near(rgb("white"), widest).contains(&cell(rgb("black"))),
            "white reaches black"
        );
        // A wider reach holds the narrower one, and every colour holds its own cell.
        for (_, color) in NAMED_COLORS {
            let [a, b, c] = COLOR_REACHES.map(|r| cells_near(color, r));
            assert!(a.iter().all(|x| b.contains(x)) && b.iter().all(|x| c.contains(x)));
            assert!(a.contains(&cell(color)));
        }
    }

    #[test]
    fn a_palette_keeps_the_common_colors() {
        let pixels = |runs: &[(Rgb, usize)]| -> Vec<Rgb> {
            runs.iter()
                .flat_map(|&(c, n)| std::iter::repeat_n(c, n))
                .collect()
        };
        let (red, blue, white) = ([220, 30, 40], [30, 90, 220], [255, 255, 255]);
        let p = palette_of(pixels(&[(white, 600), (red, 300), (blue, 100)]));
        assert_eq!(p.cells.len(), 3);
        assert!(p.cells.is_sorted());
        assert!(p.cells.contains(&cell(red)) && p.cells.contains(&cell(blue)));
        assert_eq!(
            p.colors,
            [0xff_ffff, 0xdc_1e28, 0x1e_5adc],
            "most common first"
        );

        // A speck is left out, unless the picture has nothing else to keep.
        let p = palette_of(pixels(&[
            (white, 900),
            ([0, 0, 0], 60),
            (red, 30),
            (blue, 5),
            ([20, 160, 40], 5),
        ]));
        assert!(!p.cells.contains(&cell([20, 160, 40])));
        assert_eq!(p.cells.len(), 3, "the top three stay however small");

        // Near colours are one swatch, but both can be searched.
        let p = palette_of(pixels(&[([220, 30, 40], 500), ([200, 28, 36], 500)]));
        assert_eq!(p.cells.len(), 2);
        assert_eq!(p.colors.len(), 1);

        assert_eq!(palette_of(Vec::new()), Palette::default());
        // At most eight cells and six swatches, for a very mixed picture.
        let mixed: Vec<Rgb> = (0..=255u8)
            .flat_map(|r| [0u8, 128, 255].map(move |g| [r, g, 255 - r]))
            .collect();
        let p = palette_of(mixed);
        assert!(p.cells.len() <= MAX_CELLS && p.colors.len() <= MAX_SHOWN);
    }

    #[test]
    fn the_wheel_runs_through_the_hues() {
        assert_eq!(wheel_color(0.0, 0.0), [255, 255, 255]);
        assert_eq!(wheel_color(0.0, -1.0), [255, 0, 0]);
        assert_eq!(wheel_color(1.0, 0.0), [128, 255, 0]);
        assert_eq!(wheel_color(0.0, 1.0), [0, 255, 255]);
        assert_eq!(wheel_color(-1.0, 0.0), [128, 0, 255]);
        // Half way out is half way to white, and past the edge is the edge.
        assert_eq!(wheel_color(0.0, -0.5), [255, 128, 128]);
        assert_eq!(wheel_color(0.0, -3.0), [255, 0, 0]);
        // Hostile numbers still give a colour.
        let _ = wheel_color(f64::NAN, f64::INFINITY);
    }

    #[test]
    fn hex_round_trips() {
        assert_eq!(to_hex([0x20, 0x06, 0xe0]), "2006e0");
        assert_eq!(stored_hex(0x20_06e0), "#2006e0");
        assert_eq!(stored_hex(0), "#000000");
    }
}
