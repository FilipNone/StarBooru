//! How the gallery lays out its tiles: how many columns fit, how big a tile is
//! and where it goes. Lengths are so many tile widths and so many gaps, so the
//! pixels are left to the CSS (`.masonry` in `main.css`) and a window that
//! resizes within the same column count needs no new layout. The same sums in
//! pixels say which tiles are near the screen without measuring anything.
//!
//! Staggered portrait stacks tiles in columns. Staggered landscape does the same
//! with rows, each as wide as the grid. Staggered everything lines up neither.

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
/// Above this a tile outgrows its thumbnail, 512 pixels for most pictures. Tall
/// tiles stop sooner, so that one still fits on the screen.
const MAX_TILE: f64 = 512.0;
const MAX_TALL_TILE: f64 = 400.0;
/// A tile of the mosaic can be twice as wide or more, which this keeps within
/// its thumbnail. Most are wider than a column, so a column can be narrower.
const MAX_MOSAIC_TILE: f64 = 300.0;
const MIN_MOSAIC_TILE: f64 = 96.0;
/// In two columns a tile of the mosaic is half the grid or all of it: tiles can't
/// start anywhere but the middle, and wide pictures end up in a single stack.
const FEWEST_MOSAIC_COLUMNS: u32 = 3;

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
        GalleryView::Portrait | GalleryView::StaggeredPortrait => (MIN_TILE, MAX_TALL_TILE),
        GalleryView::StaggeredAll => (MIN_MOSAIC_TILE, MAX_MOSAIC_TILE),
    }
}

/// The fewest and the most columns that keep tiles a sensible size in a grid
/// `width` pixels wide.
pub fn column_limits(width: f64, gap: f64, view: GalleryView) -> (u32, u32) {
    let (min_tile, max_tile) = tile_limits(view);
    let span = width.max(0.0) + gap;
    let fewest = (span / (max_tile + gap)).ceil().max(1.0);
    let most = (span / (min_tile + gap)).floor().max(fewest) as u32;
    let fewest = fewest as u32;
    match view {
        GalleryView::StaggeredAll => (fewest.max(FEWEST_MOSAIC_COLUMNS.min(most)), most),
        _ => (fewest, most),
    }
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

/// A length: so many tile widths and so many gaps.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Length {
    pub tiles: f64,
    pub gaps: f64,
}

/// One gap, the length between two tiles.
const GAP: Length = Length {
    tiles: 0.0,
    gaps: 1.0,
};

impl Length {
    const fn tiles(tiles: f64) -> Self {
        Self { tiles, gaps: 0.0 }
    }

    pub fn px(self, tile: f64, gap: f64) -> f64 {
        self.tiles * tile + self.gaps * gap
    }

    fn plus(self, other: Self) -> Self {
        Self {
            tiles: self.tiles + other.tiles,
            gaps: self.gaps + other.gaps,
        }
    }

    fn minus(self, other: Self) -> Self {
        Self {
            tiles: self.tiles - other.tiles,
            gaps: self.gaps - other.gaps,
        }
    }

    fn times(self, factor: f64) -> Self {
        Self {
            tiles: self.tiles * factor,
            gaps: self.gaps * factor,
        }
    }

    /// Roughly how long it is, to compare lengths the same way at any size.
    fn rough(self) -> f64 {
        self.px(1.0, TILE_GAP / TILE_WIDTH)
    }

    fn same(self, other: Self) -> bool {
        (self.tiles - other.tiles).abs() < 1e-6 && (self.gaps - other.gaps).abs() < 1e-6
    }
}

/// Where a tile goes and how big it is. `left` and `top` count from the grid's
/// corner.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Place {
    pub left: Length,
    pub top: Length,
    pub width: Length,
    pub height: Length,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Layout {
    pub tiles: Vec<Place>,
    /// How far down the lowest tile reaches.
    pub height: Length,
}

impl Layout {
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
                let top = place.top.px(tile, gap);
                top < to && top + place.height.px(tile, gap) > from
            })
            .map(|(index, _)| index)
            .collect()
    }

    /// Takes a tile's length down the grid into account.
    fn reach(&mut self, bottom: Length) {
        if bottom.rough() > self.height.rough() {
            self.height = bottom;
        }
    }
}

/// Lays out tiles of the given heights over widths, in list order.
pub fn place(view: GalleryView, ratios: impl IntoIterator<Item = f64>, columns: u32) -> Layout {
    match view {
        GalleryView::StaggeredLandscape => rows(ratios, columns),
        GalleryView::StaggeredAll => mosaic(ratios, columns),
        _ => stack(ratios, columns),
    }
}

/// Puts each tile in the shortest column so far, the leftmost if they tie. A
/// new page of results leaves the earlier tiles where they are.
fn stack(ratios: impl IntoIterator<Item = f64>, columns: u32) -> Layout {
    let mut stacks = vec![Length::default(); columns.max(1) as usize];
    let mut layout = Layout::default();
    for ratio in ratios {
        let mut shortest = 0_usize;
        let mut lowest = f64::INFINITY;
        for (i, top) in stacks.iter().enumerate() {
            // The margin lets a rounding error not break a tie.
            if top.tiles < lowest - 1e-9 {
                shortest = i;
                lowest = top.tiles;
            }
        }
        if let Some(top) = stacks.get_mut(shortest) {
            let height = Length::tiles(ratio);
            layout.tiles.push(Place {
                left: Length {
                    tiles: shortest as f64,
                    gaps: shortest as f64,
                },
                top: *top,
                width: Length::tiles(1.0),
                height,
            });
            layout.reach(top.plus(height));
            *top = top.plus(height).plus(GAP);
        }
    }
    layout
}

/// Breaks the list into rows that each fill the grid's width: a row's height
/// follows from the widths over heights of its tiles. A row takes as many as
/// bring it closest to `columns` landscape tiles, so the count means about the
/// same as elsewhere. Only the last row can still change, when more results
/// come; until then it keeps the height it aims for rather than stretching.
fn rows(ratios: impl IntoIterator<Item = f64>, columns: u32) -> Layout {
    let columns = f64::from(columns.max(1));
    let aim = columns / LANDSCAPE;
    let mut layout = Layout::default();
    let mut top = Length::default();
    let mut row: Vec<f64> = Vec::new();
    let mut wide = 0.0_f64;
    let mut finish = |row: &[f64], share: f64| {
        // The widths add up to `columns` tiles, plus the gaps between columns
        // that this row doesn't have.
        let height = Length {
            tiles: columns,
            gaps: columns - row.len() as f64,
        }
        .times(1.0 / share);
        let mut left = Length::default();
        for aspect in row {
            let width = height.times(*aspect);
            layout.tiles.push(Place {
                left,
                top,
                width,
                height,
            });
            left = left.plus(width).plus(GAP);
        }
        layout.reach(top.plus(height));
        top = top.plus(height).plus(GAP);
    };
    for ratio in ratios {
        let aspect = 1.0 / ratio.max(0.01);
        if !row.is_empty() && (wide + aspect - aim).abs() > (wide - aim).abs() {
            finish(&row, wide);
            row.clear();
            wide = 0.0;
        }
        row.push(aspect);
        wide += aspect;
    }
    if !row.is_empty() {
        finish(&row, wide.max(aim));
    }
    layout
}

/// Parts of a column in the mosaic, so that tiles can start halfway across one.
const PARTS: usize = 2;
/// The tallest tile of the mosaic, in tile widths, so a wide one with a tall
/// picture doesn't fill the screen.
const TALLEST_MOSAIC: f64 = 2.25;
/// How far a tile of the mosaic gives up its picture's shape to end level with
/// the tile next to it, as a share of its height. Level tiles are what a wider
/// one can then stand on: without this, the columns of the first few rows would
/// go on as they are all the way down.
const LEVEL: f64 = 0.4;
/// How much taller a tile of the mosaic gets when it would end a full row.
const UNEVEN: f64 = 1.2;
/// The most columns that count as few, see [`wanted_parts`].
const FEW_MOSAIC_COLUMNS: usize = 3;
/// How much over the tallest a tile's picture may be, on few columns.
const CUT_ON_FEW_COLUMNS: f64 = 1.35;

/// How many parts wide a tile of the mosaic would like to be: wider the wider
/// its picture is, and now and then wider still if that doesn't make it too tall,
/// as otherwise a collection of one shape would come out as plain columns. By
/// place in the list, so that it stays the same as more results come.
///
/// A few columns are the width of a phone, where the choice is tighter. A tile
/// two columns wide leaves one column beside it, and those two then go on as
/// they are all the way down, so the widest get narrower now and then. And a tall
/// picture may widen even if it then shows less of itself, or a folder of phone
/// photos would be a plain grid.
fn wanted_parts(ratio: f64, index: usize, columns: usize) -> usize {
    let shape = if ratio <= 0.62 {
        4
    } else if ratio <= 0.85 {
        3
    } else {
        2
    };
    let scatter = (index as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15) >> 60;
    let few = columns <= FEW_MOSAIC_COLUMNS;
    let tallest = if few {
        TALLEST_MOSAIC * CUT_ON_FEW_COLUMNS
    } else {
        TALLEST_MOSAIC
    };
    let wider = shape + 1;
    if scatter < 5 && wider <= 4 && ratio * wider as f64 / PARTS as f64 <= tallest {
        wider
    } else if few && shape == 4 && scatter < 5 {
        3
    } else if few && shape == 4 && scatter < 8 {
        2
    } else {
        shape
    }
}

/// Tiles of different widths and heights, lining up in neither rows nor
/// columns. Each goes on the lowest level stretch of what is there so far, the
/// leftmost if they tie, and stands on all of it or on its left end. So no tile
/// ever leaves a hole under it, and a new page of results leaves the earlier
/// tiles where they are.
fn mosaic(ratios: impl IntoIterator<Item = f64>, columns: u32) -> Layout {
    let columns = columns.max(1) as usize;
    let parts = columns * PARTS;
    let mut tops = vec![Length::default(); parts];
    let mut layout = Layout::default();
    for (index, ratio) in ratios.into_iter().enumerate() {
        let mut start = 0_usize;
        let mut lowest = f64::INFINITY;
        for (i, top) in tops.iter().enumerate() {
            if top.rough() < lowest - 1e-9 {
                start = i;
                lowest = top.rough();
            }
        }
        let Some(floor) = tops.get(start).copied() else {
            continue;
        };
        let free = tops
            .iter()
            .skip(start)
            .take_while(|top| top.same(floor))
            .count();
        // A stretch never gets narrower than a tile: a tile that would leave one
        // that narrow takes it as well, or else leaves a whole tile's width.
        let mut taken = wanted_parts(ratio, index, columns).min(free);
        if free - taken == 1 {
            let wider = (taken + 1) as f64 / PARTS as f64;
            taken = if taken > PARTS && ratio * wider > TALLEST_MOSAIC {
                taken - 1
            } else {
                taken + 1
            };
        }
        let across = taken as f64 / PARTS as f64;
        let width = Length {
            tiles: across,
            gaps: across - 1.0,
        };
        let natural = Length::tiles((ratio * across).min(TALLEST_MOSAIC));
        // Ends level with the tile on one side or the other, if that's close.
        let beside = [
            start.checked_sub(1).and_then(|i| tops.get(i)),
            tops.get(start + taken).filter(|_| taken == free),
        ];
        let level = beside
            .into_iter()
            .flatten()
            .map(|top| top.minus(floor).minus(GAP))
            .filter(|level| (level.rough() - natural.rough()).abs() <= natural.rough() * LEVEL)
            .min_by(|a, b| {
                let off = |l: &Length| (l.rough() - natural.rough()).abs();
                off(a).total_cmp(&off(b))
            })
            .unwrap_or(natural);
        // Not level with all the rest, as that would read as a row. On few columns
        // it has to be allowed: a level stretch is the only way to start again
        // with other widths, and it's only ever a tile or two wide.
        let next = floor.plus(level).plus(GAP);
        let others = tops
            .iter()
            .enumerate()
            .filter(|(i, _)| !(start..start + taken).contains(i));
        let mut others = others.map(|(_, top)| top).peekable();
        let closes_row = columns > FEW_MOSAIC_COLUMNS
            && others.peek().is_some()
            && others.all(|top| top.same(next));
        let height = if closes_row {
            level.times(UNEVEN)
        } else {
            level
        };
        let step = Length {
            tiles: 1.0 / PARTS as f64,
            gaps: 1.0 / PARTS as f64,
        };
        layout.tiles.push(Place {
            left: step.times(start as f64),
            top: floor,
            width,
            height,
        });
        layout.reach(floor.plus(height));
        let next = floor.plus(height).plus(GAP);
        for top in tops.iter_mut().skip(start).take(taken) {
            *top = next;
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
        let layout = stack([1.5; 7], 3);
        let spots: Vec<_> = layout.tiles.iter().map(|p| (p.left, p.top)).collect();
        let at = |column: f64, row: f64| {
            (
                Length {
                    tiles: column,
                    gaps: column,
                },
                Length {
                    tiles: row * 1.5,
                    gaps: row,
                },
            )
        };
        assert_eq!(
            spots,
            [
                at(0.0, 0.0),
                at(1.0, 0.0),
                at(2.0, 0.0),
                at(0.0, 1.0),
                at(1.0, 1.0),
                at(2.0, 1.0),
                at(0.0, 2.0)
            ]
        );
        assert_eq!(
            layout.height,
            Length {
                tiles: 4.5,
                gaps: 2.0
            }
        );
    }

    #[test]
    fn each_tile_goes_in_the_shortest_column() {
        let layout = stack([2.0, 1.0, 1.0, 1.0], 2);
        let columns: Vec<_> = layout.tiles.iter().map(|p| p.left.tiles).collect();
        assert_eq!(columns, [0.0, 1.0, 1.0, 0.0]);
        assert_eq!(layout.tiles.get(3).map(|p| p.top.tiles), Some(2.0));
        assert_eq!(
            layout.height,
            Length {
                tiles: 3.0,
                gaps: 1.0
            }
        );
    }

    #[test]
    fn views_choose_their_layout() {
        let ratios = [1.0, 0.7, 0.6, 1.5, 2.0, 0.8, 1.2, 0.5, 0.9];
        for view in [
            GalleryView::Square,
            GalleryView::Landscape,
            GalleryView::Portrait,
            GalleryView::StaggeredPortrait,
        ] {
            assert_eq!(place(view, ratios, 3), stack(ratios, 3), "{view:?}");
        }
        assert_eq!(
            place(GalleryView::StaggeredLandscape, ratios, 3),
            rows(ratios, 3)
        );
        assert_eq!(
            place(GalleryView::StaggeredAll, ratios, 3),
            mosaic(ratios, 3)
        );
    }

    /// Sizes to check a layout at, as it has to hold at any of them: narrow,
    /// the default, wide, and without gaps.
    const SIZES: [(f64, f64); 4] = [(110.0, 8.0), (243.2, 8.0), (512.0, 8.0), (250.0, 0.0)];

    /// Assorted shapes, the same on every run.
    fn assorted(count: u32, low: f64, high: f64) -> Vec<f64> {
        (0..count)
            .map(|i| low + (high - low) * f64::from(i.wrapping_mul(2_654_435_761) >> 22) / 1024.0)
            .collect()
    }

    /// The tiles of a layout, grouped by row.
    fn rows_of(layout: &Layout) -> Vec<Vec<Place>> {
        let mut found: Vec<Vec<Place>> = Vec::new();
        for place in &layout.tiles {
            match found.last_mut() {
                Some(row) if row.first().is_some_and(|first| first.top == place.top) => {
                    row.push(*place);
                }
                _ => found.push(vec![*place]),
            }
        }
        found
    }

    #[test]
    fn rows_fill_the_width_at_any_size() {
        for columns in [1, 2, 3, 5, 9] {
            let layout = rows(assorted(300, 0.5, 1.0), columns);
            let found = rows_of(&layout);
            assert!(found.len() > 2);
            let (last, full) = found.split_last().expect("rows");
            for (tile, gap) in SIZES {
                let c = f64::from(columns);
                let width = c * tile + (c - 1.0) * gap;
                for row in full {
                    let end = row.last().expect("tile");
                    let right = end.left.px(tile, gap) + end.width.px(tile, gap);
                    assert!((right - width).abs() < 1e-6, "{right} of {width}");
                }
                // The unfinished one stops short rather than stretching.
                let end = last.last().expect("tile");
                let right = end.left.px(tile, gap) + end.width.px(tile, gap);
                assert!(right <= width + 1e-6);
            }
        }
    }

    #[test]
    fn a_row_is_one_height_and_tiles_keep_their_shape() {
        let ratios = assorted(120, 0.5, 1.0);
        let layout = rows(ratios.iter().copied(), 4);
        for row in rows_of(&layout) {
            let first = row.first().expect("tile");
            assert!(row.iter().all(|place| place.height == first.height));
        }
        let (tile, gap) = (243.2, 8.0);
        for (place, ratio) in layout.tiles.iter().zip(&ratios) {
            let shape = place.height.px(tile, gap) / place.width.px(tile, gap);
            assert!((shape - ratio).abs() < 1e-9, "{shape} for {ratio}");
        }
    }

    #[test]
    fn rows_are_staggered_and_stand_one_gap_apart() {
        let layout = rows(assorted(120, 0.5, 1.0), 4);
        let found = rows_of(&layout);
        let (tile, gap) = (243.2, 8.0);
        let starts: Vec<f64> = found
            .iter()
            .filter_map(|row| row.get(1))
            .map(|place| place.left.px(tile, gap))
            .collect();
        assert!(
            starts
                .windows(2)
                .any(|pair| (pair[0] - pair[1]).abs() > 1.0),
            "second tiles all line up: {starts:?}"
        );
        for pair in found.windows(2) {
            let above = pair[0][0];
            let below = pair[1][0];
            let bottom = above.top.px(tile, gap) + above.height.px(tile, gap);
            assert!((below.top.px(tile, gap) - bottom - gap).abs() < 1e-6);
        }
    }

    #[test]
    fn a_new_page_only_changes_the_last_row() {
        let list = assorted(200, 0.5, 1.0);
        let all = rows(list.iter().copied(), 4);
        let part = rows(list.iter().take(90).copied(), 4);
        let found = rows_of(&part);
        let (last, _) = found.split_last().expect("rows");
        let settled = 90 - last.len();
        assert_eq!(all.tiles.get(..settled), part.tiles.get(..settled));
    }

    /// Every part of every column, from the top: tiles there stand one gap
    /// apart, with nothing overlapping and no holes, at any size.
    fn assert_stacks_exactly(layout: &Layout, columns: u32) {
        let parts = columns as usize * PARTS;
        for (tile, gap) in SIZES {
            let step = (tile + gap) / PARTS as f64;
            for part in 0..parts {
                // The middle of this part of the column.
                let x = (part as f64 + 0.25) * step;
                let mut above = None::<f64>;
                for place in &layout.tiles {
                    let left = place.left.px(tile, gap);
                    if left > x || left + place.width.px(tile, gap) < x {
                        continue;
                    }
                    let top = place.top.px(tile, gap);
                    let expected = above.map_or(0.0, |bottom| bottom + gap);
                    assert!(
                        (top - expected).abs() < 1e-6,
                        "part {part} at {tile}/{gap}: {top} after {expected}"
                    );
                    above = Some(top + place.height.px(tile, gap));
                }
            }
        }
    }

    #[test]
    fn the_mosaic_leaves_no_holes_at_any_size() {
        for columns in [1, 2, 3, 5, 9] {
            for (low, high) in [(0.5, 2.0), (1.0, 1.0), (1.5, 1.5), (0.5, 0.5)] {
                let layout = mosaic(assorted(400, low, high), columns);
                assert_eq!(layout.tiles.len(), 400);
                assert_stacks_exactly(&layout, columns);
            }
        }
    }

    #[test]
    fn the_mosaic_stays_within_the_grid() {
        for columns in [1, 2, 3, 5, 9] {
            let layout = mosaic(assorted(400, 0.5, 2.0), columns);
            let c = f64::from(columns);
            for (tile, gap) in SIZES {
                for place in &layout.tiles {
                    let right = place.left.px(tile, gap) + place.width.px(tile, gap);
                    assert!(right <= c * tile + (c - 1.0) * gap + 1e-6);
                    assert!(place.height.px(tile, gap) > 0.0);
                }
            }
        }
    }

    #[test]
    fn the_mosaic_lines_up_in_neither_rows_nor_columns() {
        // Not even with pictures all of one shape.
        for list in [assorted(500, 0.5, 2.0), vec![1.5; 500], vec![0.66; 500]] {
            let layout = mosaic(list, 5);
            let distinct = |values: Vec<f64>| {
                let mut seen: Vec<f64> = Vec::new();
                for value in values {
                    if !seen.iter().any(|v| (v - value).abs() < 1e-6) {
                        seen.push(value);
                    }
                }
                seen.len()
            };
            let widths = distinct(layout.tiles.iter().map(|p| p.width.tiles).collect());
            let tops = distinct(layout.tiles.iter().map(|p| p.top.tiles).collect());
            assert!(widths >= 2, "{widths} widths");
            assert!(tops > 40, "{tops} places down");
            let halfway = layout
                .tiles
                .iter()
                .any(|p| (p.left.tiles.fract() - 0.5).abs() < 1e-9);
            assert!(halfway, "no tile starts halfway across a column");
        }
    }

    #[test]
    fn the_mosaic_stays_mixed_all_the_way_down() {
        // Without tiles ending level, the columns of the first rows go on as
        // they are, however many tiles follow.
        let layout = mosaic(assorted(2000, 0.5, 2.0), 5);
        for part in layout.tiles.chunks(200) {
            let wide = part.iter().filter(|p| p.width.tiles > 1.0).count();
            assert!(wide >= 20, "{wide} of 200 are wider than a column");
        }
    }

    #[test]
    fn the_mosaic_has_no_full_rows() {
        let (tile, gap) = (243.2, 8.0);
        for list in [assorted(500, 0.5, 2.0), vec![1.0; 500], vec![0.66; 500]] {
            let layout = mosaic(list, 5);
            let across = 5.0 * tile + 4.0 * gap;
            let ends: Vec<f64> = layout
                .tiles
                .iter()
                .map(|p| p.top.px(tile, gap) + p.height.px(tile, gap))
                .collect();
            for end in &ends {
                let ending_here: f64 = layout
                    .tiles
                    .iter()
                    .zip(&ends)
                    .filter(|(_, other)| (*other - end).abs() < 1e-6)
                    .map(|(p, _)| p.width.px(tile, gap) + gap)
                    .sum();
                assert!(ending_here - gap < across - 1.0, "a full row ends at {end}");
            }
        }
    }

    /// Shapes that have nothing to do with the place in the list, as `assorted`'s
    /// have: the same hash picks the width of a tile there.
    fn unrelated(count: usize, seed: u64) -> Vec<f64> {
        let mut state = seed;
        (0..count)
            .map(|_| {
                state ^= state << 13;
                state ^= state >> 7;
                state ^= state << 17;
                0.5 + 1.5 * (state % 1000) as f64 / 1000.0
            })
            .collect()
    }

    #[test]
    fn a_phone_gets_columns_the_mosaic_can_offset() {
        for width in [304.0, 344.0, 376.0, 400.0, 600.0] {
            let view = GalleryView::StaggeredAll;
            let (fewest, most) = column_limits(width, GAP, view);
            assert!(fewest >= 3, "{width}");
            assert!(most >= fewest, "{width}");
            assert!(columns(width, GAP, view, None) >= 3, "{width}");
            assert!(columns(width, GAP, view, Some(2)) >= 3, "{width}");
            assert!(tile_width(width, GAP, fewest) >= MIN_MOSAIC_TILE, "{width}");
        }
        // Too narrow for that: as many as fit.
        assert_eq!(column_limits(200.0, GAP, GalleryView::StaggeredAll), (2, 2));
    }

    /// The fewest different values of `of` in any run of `size` tiles.
    fn fewest_in_any_window(layout: &Layout, size: usize, of: fn(&Place) -> f64) -> usize {
        layout
            .tiles
            .windows(size)
            .map(|window| {
                let mut seen: Vec<i64> = window
                    .iter()
                    .map(|p| (of(p) * 1000.0).round() as i64)
                    .collect();
                seen.sort_unstable();
                seen.dedup();
                seen.len()
            })
            .min()
            .unwrap_or(0)
    }

    #[test]
    fn the_mosaic_staggers_on_a_phone_with_pictures_of_one_shape() {
        // A folder from one camera, the portrait phone photo among them.
        for ratio in [
            0.5625,
            2.0 / 3.0,
            0.75,
            0.8,
            1.0,
            1.25,
            1.5,
            16.0 / 9.0,
            2.0,
        ] {
            let layout = mosaic(vec![ratio; 300], 3);
            assert_stacks_exactly(&layout, 3);
            assert!(
                fewest_in_any_window(&layout, 20, |p| p.left.tiles) >= 3,
                "{ratio}"
            );
            assert!(
                fewest_in_any_window(&layout, 20, |p| p.width.tiles) >= 2,
                "{ratio}"
            );
            for window in layout.tiles.windows(20) {
                let halfway = window
                    .iter()
                    .any(|p| (p.left.tiles.fract() - 0.5).abs() < 1e-9);
                assert!(halfway, "{ratio} runs on in whole columns");
            }
            assert!(layout.tiles.iter().all(|p| p.width.tiles < 2.9), "{ratio}");
        }
    }

    #[test]
    fn the_mosaic_staggers_on_a_phone_with_pictures_of_all_shapes() {
        for seed in [1, 7, 42, 1999] {
            let layout = mosaic(unrelated(600, seed), 3);
            assert_stacks_exactly(&layout, 3);
            assert!(layout.tiles.iter().all(|p| p.width.tiles < 2.9), "{seed}");
            for part in layout.tiles.chunks(100) {
                let wide = part.iter().filter(|p| p.width.tiles > 1.0).count();
                let halfway = part
                    .iter()
                    .filter(|p| (p.left.tiles.fract() - 0.5).abs() < 1e-9)
                    .count();
                assert!(wide >= 15, "{wide} of 100 are wide, {seed}");
                assert!(halfway >= 3, "{halfway} of 100 start halfway, {seed}");
            }
        }
    }

    #[test]
    fn the_mosaic_never_leaves_a_sliver() {
        for columns in [2, 3, 4, 5, 9] {
            for list in [
                assorted(1000, 0.5, 2.0),
                assorted(1000, 0.5, 0.9),
                unrelated(1000, 5),
                unrelated(1000, 77),
            ] {
                let layout = mosaic(list, columns);
                for place in &layout.tiles {
                    assert!(place.width.tiles >= 1.0, "{place:?} of {columns}");
                }
            }
        }
    }

    #[test]
    fn the_mosaic_widens_wide_pictures_and_keeps_shapes_close() {
        assert_eq!(wanted_parts(0.55, 1, 5), 4);
        assert_eq!(wanted_parts(0.75, 1, 5), 3);
        assert_eq!(wanted_parts(1.5, 1, 5), 2);
        let ratios = assorted(300, 0.5, 2.0);
        let layout = mosaic(ratios.iter().copied(), 5);
        for (place, ratio) in layout.tiles.iter().zip(&ratios) {
            let (tile, gap) = (243.2, 8.0);
            let shape = place.height.px(tile, gap) / place.width.px(tile, gap);
            // Ending level moves it a little, and the tallest cap a lot more, but
            // never to another kind of shape altogether.
            assert!(
                shape <= ratio * 1.7 && shape >= ratio * 0.5,
                "{shape}, {ratio}"
            );
        }
    }

    #[test]
    fn more_results_leave_earlier_tiles_in_place() {
        let first = [1.5, 1.0, 2.0, 1.2, 1.7];
        let more = [1.1, 1.9, 1.3];
        let all = stack(first.iter().chain(&more).copied(), 3);
        assert_eq!(all.tiles.get(..5), Some(&stack(first, 3).tiles[..]));
        let list = assorted(300, 0.5, 2.0);
        let all = mosaic(list.iter().copied(), 5);
        let part = mosaic(list.iter().take(120).copied(), 5);
        assert_eq!(all.tiles.get(..120), Some(&part.tiles[..]));
    }

    #[test]
    fn pixels_match_the_css() {
        assert_eq!(tile_width(1248.0, 8.0, 5), 243.2);
        assert_eq!(tile_width(1000.0, 0.0, 4), 250.0);
        assert_eq!(tile_width(0.0, 8.0, 5), 0.0, "a grid not measured yet");
        assert_eq!(tile_width(100.0, 8.0, 0), 100.0);
        let layout = stack([1.5, 1.0, 1.0, 2.0], 2);
        let last = layout.tiles[3];
        assert_eq!(last.left.px(100.0, 8.0), 0.0);
        assert_eq!(last.top.px(100.0, 8.0), 158.0);
        assert_eq!(last.width.px(100.0, 8.0), 100.0);
        assert_eq!(last.height.px(100.0, 8.0), 200.0);
        assert_eq!(layout.height.px(100.0, 8.0), 358.0);
        assert_eq!(Layout::default().height.px(100.0, 8.0), 0.0);
    }

    #[test]
    fn only_tiles_reaching_into_the_range_are_near() {
        // Two columns of squares 100 tall, 8 apart: rows start at 0, 108, 216…
        let layout = stack([1.0; 40], 2);
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
        let layout = stack((0..100_000).map(|i| 1.0 + f64::from(i % 5) / 5.0), 5);
        let tile = 243.2;
        let end = layout.height.px(tile, TILE_GAP);
        // Four screens of 800 pixels, anywhere in the grid.
        for top in [0.0, end / 3.0, end / 2.0, end - 3200.0] {
            let count = layout.within(tile, TILE_GAP, top, top + 3200.0).len();
            assert!((30..=90).contains(&count), "{count} at {top}");
        }
    }

    #[test]
    fn a_grid_not_measured_yet_has_nothing_near() {
        // Every tile would count as near, and a long list would all go on the page.
        let layout = stack([1.0; 100], 3);
        assert!(layout.within(0.0, 8.0, -100.0, 3000.0).is_empty());
    }

    #[test]
    fn nothing_to_place() {
        for view in [
            GalleryView::StaggeredPortrait,
            GalleryView::StaggeredLandscape,
            GalleryView::StaggeredAll,
        ] {
            assert_eq!(place(view, [], 4), Layout::default());
            // No columns can't divide by zero or drop a tile.
            assert_eq!(place(view, [1.0], 0).tiles.len(), 1);
        }
    }
}
