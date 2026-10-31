//! How the gallery lays out its tiles: how many columns fit, how tall a tile
//! is and where it goes. Heights are multiples of a tile's width, so the
//! pixels are left to the CSS (`.masonry` in `main.css`) and a window that
//! resizes within the same column count needs no new layout. The same sums in
//! pixels say which tiles are near the screen without measuring anything.

use crate::store::GalleryView;

/// The size a tile aims for unless the user picks a column count, as on
/// Pinterest.
const TILE_WIDTH: f64 = 250.0;
/// The space between tiles, as `--tile-gap` in `main.css`.
pub const TILE_GAP: f64 = 8.0;

/// Below this a tile is too small to make out. Wide tiles are short, so they
/// stop sooner.
const MIN_TILE: f64 = 110.0;
const MIN_WIDE_TILE: f64 = 160.0;
/// Above this a tile outgrows its thumbnail, which fits in 512 pixels. Tall
/// tiles stop sooner, so that one still fits on the screen.
const MAX_TILE: f64 = 512.0;
const MAX_TALL_TILE: f64 = 400.0;

/// Height over width of the fixed shapes: 2:3 and 3:2.
const LANDSCAPE: f64 = 2.0 / 3.0;
const PORTRAIT: f64 = 3.0 / 2.0;
/// Staggered tiles follow the picture within these limits and crop the rest,
/// so a panorama or a long comic strip doesn't take over the grid.
const WIDEST: f64 = 0.5;
const TALLEST: f64 = 2.0;

fn tile_limits(view: GalleryView) -> (f64, f64) {
    match view {
        GalleryView::Landscape | GalleryView::StaggeredLandscape => (MIN_WIDE_TILE, MAX_TILE),
        GalleryView::Square => (MIN_TILE, MAX_TILE),
        GalleryView::Portrait | GalleryView::StaggeredPortrait | GalleryView::StaggeredAll => {
            (MIN_TILE, MAX_TALL_TILE)
        }
    }
}

/// The fewest and the most columns that keep tiles a sensible size in a grid
/// `width` pixels wide.
pub fn column_limits(width: f64, gap: f64, view: GalleryView) -> (u32, u32) {
    let (min_tile, max_tile) = tile_limits(view);
    let span = width.max(0.0) + gap;
    let fewest = (span / (max_tile + gap)).ceil().max(1.0);
    let most = (span / (min_tile + gap)).floor().max(fewest);
    (fewest as u32, most as u32)
}

/// The columns to use: the user's choice if they made one, or else as many
/// as bring a tile closest to `TILE_WIDTH`, and two on a phone. Either way
/// within `column_limits`.
pub fn columns(width: f64, gap: f64, view: GalleryView, chosen: Option<u8>) -> u32 {
    let (fewest, most) = column_limits(width, gap, view);
    let wanted = chosen.map_or_else(
        || (((width.max(0.0) + gap) / (TILE_WIDTH + gap)).round() as u32).max(2),
        u32::from,
    );
    wanted.clamp(fewest, most)
}

/// A tile's width in pixels, with `columns` in a grid `width` pixels wide.
pub fn tile_width(width: f64, gap: f64, columns: u32) -> f64 {
    let columns = f64::from(columns.max(1));
    ((width - gap * (columns - 1.0)) / columns).max(0.0)
}

/// A tile's height over its width.
pub fn tile_ratio(view: GalleryView, width: Option<i32>, height: Option<i32>) -> f64 {
    let natural = match (width, height) {
        (Some(w), Some(h)) if w > 0 && h > 0 => f64::from(h) / f64::from(w),
        _ => 1.0,
    };
    match view {
        GalleryView::Square => 1.0,
        GalleryView::Landscape => LANDSCAPE,
        GalleryView::Portrait => PORTRAIT,
        GalleryView::StaggeredPortrait => natural.clamp(1.0, TALLEST),
        GalleryView::StaggeredLandscape => natural.clamp(WIDEST, 1.0),
        GalleryView::StaggeredAll => natural.clamp(WIDEST, TALLEST),
    }
}

/// Where a tile goes. `top` is in tile widths and `row` counts the gaps
/// above it.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Place {
    pub column: u32,
    pub top: f64,
    pub row: u32,
    pub ratio: f64,
}

impl Place {
    /// How far down the grid it starts, in pixels.
    pub fn top_px(&self, tile: f64, gap: f64) -> f64 {
        self.top * tile + f64::from(self.row) * gap
    }

    pub fn height_px(&self, tile: f64) -> f64 {
        self.ratio * tile
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Layout {
    pub tiles: Vec<Place>,
    /// The tallest column, in the units of `Place`.
    pub height: f64,
    pub rows: u32,
}

impl Layout {
    /// The grid's height in pixels.
    pub fn height_px(&self, tile: f64, gap: f64) -> f64 {
        self.height * tile + f64::from(self.rows.saturating_sub(1)) * gap
    }

    /// The tiles that reach into `from..to`, pixels from the top of the grid,
    /// in list order. None while the grid has no size, as every tile would
    /// stand at the top.
    pub fn within(&self, tile: f64, gap: f64, from: f64, to: f64) -> Vec<usize> {
        if tile <= 0.0 {
            return Vec::new();
        }
        self.tiles
            .iter()
            .enumerate()
            .filter(|(_, place)| {
                let top = place.top_px(tile, gap);
                top < to && top + place.height_px(tile) > from
            })
            .map(|(index, _)| index)
            .collect()
    }
}

/// Puts each tile in the shortest column so far, the leftmost if they tie. A
/// new page of results leaves the earlier tiles where they are.
pub fn place(ratios: impl IntoIterator<Item = f64>, columns: u32) -> Layout {
    let mut stacks = vec![(0.0_f64, 0_u32); columns.max(1) as usize];
    let mut layout = Layout::default();
    for ratio in ratios {
        let mut shortest = 0_usize;
        let mut lowest = f64::INFINITY;
        for (i, (height, _)) in stacks.iter().enumerate() {
            // The margin lets a rounding error not break a tie.
            if *height < lowest - 1e-9 {
                shortest = i;
                lowest = *height;
            }
        }
        if let Some((height, rows)) = stacks.get_mut(shortest) {
            layout.tiles.push(Place {
                column: shortest as u32,
                top: *height,
                row: *rows,
                ratio,
            });
            *height += ratio;
            *rows += 1;
            layout.height = layout.height.max(*height);
            layout.rows = layout.rows.max(*rows);
        }
    }
    layout
}

#[cfg(test)]
mod tests {
    use super::*;

    const GAP: f64 = TILE_GAP;

    /// A tile's width with `n` columns in a grid `width` pixels wide.
    fn tile(width: f64, n: u32) -> f64 {
        (width - GAP * f64::from(n - 1)) / f64::from(n)
    }

    #[test]
    fn aims_for_pinterest_width_by_default() {
        for width in [1248.0, 1568.0, 2500.0, 3800.0] {
            let n = columns(width, GAP, GalleryView::StaggeredPortrait, None);
            let size = tile(width, n);
            assert!((200.0..=320.0).contains(&size), "{width}: {n} of {size}");
        }
        assert_eq!(columns(1248.0, GAP, GalleryView::default(), None), 5);
        // A phone shows two columns, not one big tile.
        assert_eq!(columns(374.0, GAP, GalleryView::default(), None), 2);
    }

    #[test]
    fn default_view_is_staggered_portrait() {
        assert_eq!(GalleryView::default(), GalleryView::StaggeredPortrait);
    }

    #[test]
    fn a_chosen_count_stays_within_the_screen() {
        for view in [
            GalleryView::Square,
            GalleryView::Landscape,
            GalleryView::Portrait,
            GalleryView::StaggeredPortrait,
            GalleryView::StaggeredLandscape,
            GalleryView::StaggeredAll,
        ] {
            let (min_tile, max_tile) = tile_limits(view);
            for width in [320.0, 374.0, 800.0, 1248.0, 2500.0, 3800.0] {
                let (fewest, most) = column_limits(width, GAP, view);
                assert!((1..=most).contains(&fewest), "{view:?} at {width}");
                assert_eq!(columns(width, GAP, view, Some(1)), fewest);
                assert_eq!(columns(width, GAP, view, Some(255)), most);
                assert!(
                    tile(width, most) >= min_tile.min(width),
                    "{view:?} at {width}"
                );
                assert!(tile(width, fewest) <= max_tile, "{view:?} at {width}");
            }
        }
        // Wide tiles stop sooner on a phone.
        assert_eq!(
            column_limits(374.0, GAP, GalleryView::StaggeredPortrait),
            (1, 3)
        );
        assert_eq!(column_limits(374.0, GAP, GalleryView::Landscape), (1, 2));
    }

    #[test]
    fn a_grid_not_measured_yet_still_has_a_column() {
        assert_eq!(column_limits(0.0, GAP, GalleryView::default()), (1, 1));
        assert_eq!(columns(0.0, GAP, GalleryView::default(), None), 1);
        assert_eq!(columns(-5.0, 0.0, GalleryView::default(), Some(9)), 1);
    }

    #[test]
    fn staggered_tiles_follow_the_picture_within_limits() {
        let ratio = |view, w, h| tile_ratio(view, Some(w), Some(h));
        let tall = GalleryView::StaggeredPortrait;
        assert_eq!(ratio(tall, 800, 1200), 1.5);
        assert_eq!(ratio(tall, 1200, 800), 1.0);
        assert_eq!(ratio(tall, 500, 5000), 2.0);
        let wide = GalleryView::StaggeredLandscape;
        assert_eq!(ratio(wide, 1600, 1000), 0.625);
        assert_eq!(ratio(wide, 800, 1200), 1.0);
        assert_eq!(ratio(wide, 5000, 500), 0.5);
        let all = GalleryView::StaggeredAll;
        assert_eq!(ratio(all, 800, 1200), 1.5);
        assert_eq!(ratio(all, 1600, 1000), 0.625);
        assert_eq!(ratio(all, 5000, 500), 0.5);
        assert_eq!(ratio(all, 500, 5000), 2.0);
    }

    #[test]
    fn fixed_shapes_ignore_the_picture() {
        for (view, expected) in [
            (GalleryView::Square, 1.0),
            (GalleryView::Landscape, LANDSCAPE),
            (GalleryView::Portrait, PORTRAIT),
        ] {
            assert_eq!(tile_ratio(view, Some(5000), Some(500)), expected);
        }
    }

    #[test]
    fn missing_or_bad_sizes_read_as_square() {
        let view = GalleryView::StaggeredAll;
        assert_eq!(tile_ratio(view, None, None), 1.0);
        assert_eq!(tile_ratio(view, Some(100), None), 1.0);
        assert_eq!(tile_ratio(view, Some(0), Some(100)), 1.0);
        assert_eq!(tile_ratio(view, Some(-1), Some(100)), 1.0);
    }

    #[test]
    fn equal_tiles_fill_rows_in_order() {
        let layout = place([1.5; 7], 3);
        let spots: Vec<_> = layout.tiles.iter().map(|p| (p.column, p.row)).collect();
        assert_eq!(
            spots,
            [(0, 0), (1, 0), (2, 0), (0, 1), (1, 1), (2, 1), (0, 2)]
        );
        assert_eq!(layout.tiles.last().map(|p| p.top), Some(3.0));
        assert_eq!((layout.height, layout.rows), (4.5, 3));
    }

    #[test]
    fn each_tile_goes_in_the_shortest_column() {
        let layout = place([2.0, 1.0, 1.0, 1.0], 2);
        let columns: Vec<_> = layout.tiles.iter().map(|p| p.column).collect();
        assert_eq!(columns, [0, 1, 1, 0]);
        assert_eq!(layout.tiles.get(3).map(|p| p.top), Some(2.0));
        assert_eq!((layout.height, layout.rows), (3.0, 2));
    }

    #[test]
    fn more_results_leave_earlier_tiles_in_place() {
        let first = [1.5, 1.0, 2.0, 1.2, 1.7];
        let more = [1.1, 1.9, 1.3];
        let all = place(first.iter().chain(&more).copied(), 3);
        assert_eq!(all.tiles.get(..5), Some(&place(first, 3).tiles[..]));
    }

    #[test]
    fn pixels_match_the_css() {
        assert_eq!(tile_width(1248.0, 8.0, 5), 243.2);
        assert_eq!(tile_width(1000.0, 0.0, 4), 250.0);
        assert_eq!(tile_width(0.0, 8.0, 5), 0.0, "a grid not measured yet");
        assert_eq!(tile_width(100.0, 8.0, 0), 100.0);
        let layout = place([1.5, 1.0, 1.0, 2.0], 2);
        let last = layout.tiles[3];
        assert_eq!((last.column, last.row, last.top), (0, 1, 1.5));
        assert_eq!(last.top_px(100.0, 8.0), 158.0);
        assert_eq!(last.height_px(100.0), 200.0);
        assert_eq!(layout.height_px(100.0, 8.0), 358.0);
        assert_eq!(Layout::default().height_px(100.0, 8.0), 0.0);
    }

    #[test]
    fn only_tiles_reaching_into_the_range_are_near() {
        // Two columns of squares 100 tall, 8 apart: rows start at 0, 108, 216…
        let layout = place([1.0; 40], 2);
        let near = |from, to| layout.within(100.0, 8.0, from, to);
        assert_eq!(near(0.0, 100.0), [0, 1]);
        assert_eq!(near(-500.0, 1.0), [0, 1]);
        assert_eq!(near(150.0, 300.0), [2, 3, 4, 5]);
        assert!(near(100.0, 108.0).is_empty(), "just the gap");
        assert_eq!(near(2000.0, 2100.0), [36, 37, 38, 39]);
        assert!(near(9000.0, 9500.0).is_empty(), "past the end");
    }

    #[test]
    fn a_long_list_only_ever_has_a_few_tiles_near() {
        let layout = place((0..100_000).map(|i| 1.0 + f64::from(i % 5) / 5.0), 5);
        let tile = 243.2;
        let end = layout.height_px(tile, TILE_GAP);
        // Four screens of 800 pixels, anywhere in the grid.
        for top in [0.0, end / 3.0, end / 2.0, end - 3200.0] {
            let count = layout.within(tile, TILE_GAP, top, top + 3200.0).len();
            assert!((30..=90).contains(&count), "{count} at {top}");
        }
    }

    #[test]
    fn a_grid_not_measured_yet_has_nothing_near() {
        // Every tile would count as near, and a long list would all go on the page.
        let layout = place([1.0; 100], 3);
        assert!(layout.within(0.0, 8.0, -100.0, 3000.0).is_empty());
    }

    #[test]
    fn nothing_to_place() {
        assert_eq!(place([], 4), Layout::default());
        // No columns can't divide by zero or drop a tile.
        assert_eq!(place([1.0], 0).tiles.len(), 1);
    }
}
