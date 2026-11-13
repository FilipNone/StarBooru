use std::collections::{BTreeSet, HashMap, HashSet};
use std::rc::Rc;

use crate::link::Link;
use chrono::{DateTime, Utc};
use dioxus::prelude::*;
use serde::Deserialize;
use starbooru_core::color::{NAMED_COLORS, to_hex};
use starbooru_core::saved::MAX_SEARCH_NAME_CHARS;
use starbooru_core::{
    BulkEdit, MAX_BULK_POSTS, MediaKind, Order, Post, Rating, SavedSearch, SortKey, TagCorrection,
    parse_query, with_color, with_exact, with_order,
};

use super::menu::{ActionMenu, MenuItem};
use super::post::OPENING;
use super::share::ShareSheet;
use super::shell::{notify, notify_undo};
use super::upload::DropToUpload;
use super::{Choice, ColorPicker, CorrectionBanner, ErrorBanner, Select, Spinner};
use crate::Route;
use crate::activity;
use crate::cache;
use crate::download;
use crate::gesture::{MenuAt, Press, Side, Target, use_press};
use crate::icons::{Ico, Icon};
use crate::layout::{self, Place, TILE_GAP};
use crate::motion;
use crate::power;
use crate::store::{self, GalleryView, PREFS, Prefs, Spot, SwipeAction};
use crate::util::{escape_attr, format_duration, rating_label, split_tag_edit};

const PAGE_SIZE: u32 = 60;

/// Width of the gallery's grid in pixels, kept so coming back to the page
/// lays it out right away.
static GRID_WIDTH: GlobalSignal<f64> = Signal::global(|| 0.0);

/// Reports the grid's width now and whenever it changes.
const WATCH_WIDTH: &str = r#"
const box = document.querySelector(".masonry-box");
if (box) {
  const seen = new ResizeObserver(() => {
    // The exact width: a rounded one adds up to a visible error far down the grid.
    if (box.isConnected) dioxus.send(box.getBoundingClientRect().width);
    else seen.disconnect();
  });
  seen.observe(box);
}
"#;

/// The grid's measurements as they are now.
#[derive(Clone, Copy, PartialEq)]
struct Grid {
    view: GalleryView,
    columns: u32,
    gap: f64,
    width: f64,
    /// A tile's width in pixels.
    tile: f64,
}

impl Grid {
    fn new(prefs: &Prefs, width: f64) -> Self {
        let gap = if prefs.flush_tiles { 0.0 } else { TILE_GAP };
        let view = prefs.gallery_view;
        let columns = layout::columns(width, gap, view, prefs.gallery_columns);
        let tile = layout::tile_width(width, gap, columns);
        Self {
            view,
            columns,
            gap,
            width,
            tile,
        }
    }

    fn now() -> Self {
        Self::new(&PREFS.peek(), *GRID_WIDTH.peek())
    }

    fn place(self, list: &[Rc<Post>]) -> layout::Layout {
        layout::place(
            self.view,
            list.iter()
                .map(|p| layout::tile_ratio(self.view, p.width, p.height)),
            self.columns,
        )
    }
}

/// Where a post's tile is in pixels: its top, from the top of the grid, and
/// its height.
fn tile_box(list: &[Rc<Post>], id: i64) -> Option<(f64, f64)> {
    let grid = Grid::now();
    let index = list.iter().position(|p| p.id == id)?;
    // The whole list, as what comes after a tile can settle the height of its row.
    let placed = grid.place(list);
    let place = placed.tiles.get(index)?;
    Some((
        place.top.px(grid.tile, grid.gap),
        place.height.px(grid.tile, grid.gap),
    ))
}

/// How far around the top of the screen tiles go on the page, until the page
/// has reported how tall the screen is (`WATCH_NEAR`).
const FIRST_NEAR: (f64, f64) = (1500.0, 3000.0);

/// The part of the grid the screen can be showing, given the part the page last
/// said to keep (`WATCH_NEAR`). That part reaches a screen and a half above the
/// screen and as far below it, and the page says so again before the screen has
/// moved a quarter of its height either way.
fn may_be_on_screen((from, to): (f64, f64)) -> (f64, f64) {
    let screen = (to - from) / 4.0;
    (from + screen * 1.25, to - screen * 1.25)
}

/// A post's main colour for its tile, if it is `#rrggbb` and nothing else: it
/// goes into a style attribute as it comes from the server.
pub(super) fn tint(palette: &[String]) -> Option<&str> {
    let colour = palette.first()?;
    let hex = colour.strip_prefix('#')?;
    (hex.len() == 6 && hex.bytes().all(|b| b.is_ascii_hexdigit())).then_some(colour.as_str())
}

/// The part of the grid to put on the page for going back to a spot, so the
/// pictures above it don't start loading first.
fn near_spot(list: &[Rc<Post>], spot: Option<&Spot>) -> (f64, f64) {
    let top = spot.filter(|s| s.y >= 1.0).map_or(0.0, |s| {
        s.post
            .and_then(|id| tile_box(list, id))
            .map_or(s.y, |(top, _)| top - s.offset)
    });
    (top - FIRST_NEAR.0, top + FIRST_NEAR.1)
}

/// Results kept while the app runs, so coming back to a search shows it as it
/// was without loading it again. Each post is shared with its tile: the page
/// passes the posts near the screen to their tiles on every render, and a copy
/// of each, with its tags and addresses, was most of what a render cost.
#[derive(Clone)]
struct List {
    q: String,
    owner: String,
    posts: Vec<Rc<Post>>,
    next_page: u32,
    has_more: bool,
    corrected: Vec<TagCorrection>,
    loaded_at: DateTime<Utc>,
    /// What `order:random` was shuffled with, so the list stays as it was.
    seed: Option<i64>,
}

static LISTS: GlobalSignal<Vec<List>> = Signal::global(Vec::new);
const MAX_LISTS: usize = 4;
/// Signed media links last at least six hours (`docs/architecture.md`).
const MAX_LIST_HOURS: i64 = 4;
/// The saved searches last seen and whose they are, so their chips don't push
/// the grid down when they load.
static SAVED: GlobalSignal<Option<(String, Vec<SavedSearch>)>> = Signal::global(|| None);

fn cached_list(q: &str) -> Option<List> {
    let owner = store::owner();
    let now = Utc::now();
    LISTS
        .peek()
        .iter()
        .find(|l| l.q == q && l.owner == owner && (now - l.loaded_at).num_hours() < MAX_LIST_HOURS)
        .cloned()
}

fn keep_list(list: List) {
    let mut lists = LISTS.write();
    lists.retain(|l| l.q != list.q);
    lists.insert(0, list);
    lists.truncate(MAX_LISTS);
}

/// Takes a post out of the kept results, as it went to the recycle bin.
pub(super) fn forget_post(id: i64) {
    for list in LISTS.write().iter_mut() {
        list.posts.retain(|p| p.id != id);
    }
}

/// Drops the kept results, after a change that can move posts in or out of any
/// of them.
pub(super) fn forget_lists() {
    LISTS.write().clear();
}

/// Restoring a long list asks for this many pages of results at a time.
const RESTORE_PAGES: u32 = 3;
/// How many posts going back to a spot will load, before it settles for the
/// scroll position. Only their details: pictures load near the screen.
const MAX_RESTORE: usize = 100_000;

/// `order:random` comes out different each time, so it has no spot to go back to.
fn is_random(q: &str) -> bool {
    parse_query(q).is_ok_and(|p| p.order.key == SortKey::Random)
}

/// The seed each `order:random` search was last shuffled with. The viewer
/// sends the same one, so previous and next follow the order the gallery shows.
static SEEDS: GlobalSignal<Vec<(String, i64)>> = Signal::global(Vec::new);
const MAX_SEEDS: usize = 16;

/// The seed to shuffle search `q` with, picking one the first time. `None`
/// unless `q` is `order:random`.
pub(super) fn seed(q: &str) -> Option<i64> {
    if !is_random(q) {
        return None;
    }
    let known = SEEDS.peek().iter().find(|(k, _)| k == q).map(|(_, s)| *s);
    Some(known.unwrap_or_else(|| reseed(q)))
}

/// A new shuffle for search `q`.
fn reseed(q: &str) -> i64 {
    let seed = new_seed();
    remember_seed(q, seed);
    seed
}

fn remember_seed(q: &str, seed: i64) {
    let mut seeds = SEEDS.write();
    seeds.retain(|(k, _)| k != q);
    seeds.insert(0, (q.to_owned(), seed));
    seeds.truncate(MAX_SEEDS);
}

/// A seed that differs from the last one. The clock is enough: it only has to
/// shuffle differently, not be hard to guess. Mixed (splitmix64) so seeds a
/// moment apart don't come out alike.
fn new_seed() -> i64 {
    static COUNT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let count = COUNT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let nanos = Utc::now().timestamp_nanos_opt().unwrap_or_default();
    let mut x = u64::from_ne_bytes(nanos.to_ne_bytes()) ^ count.rotate_left(32);
    x = x.wrapping_add(0x9e37_79b9_7f4a_7c15);
    x = (x ^ (x >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    i64::from_ne_bytes((x ^ (x >> 31)).to_ne_bytes())
}

/// The first page as it is now, followed by the rest of what was loaded. `None`
/// if nothing changed. With no overlap, or no further pages, the rest is gone.
fn merge_head(current: &[Rc<Post>], head: Vec<Post>, has_more: bool) -> Option<Vec<Rc<Post>>> {
    let same = current
        .iter()
        .map(|p| p.id)
        .take(head.len())
        .eq(head.iter().map(|p| p.id));
    if same && (has_more || head.len() == current.len()) {
        return None;
    }
    let ids: HashSet<i64> = head.iter().map(|p| p.id).collect();
    let rest = current
        .iter()
        .rposition(|p| ids.contains(&p.id))
        .filter(|_| has_more)
        .and_then(|last| current.get(last + 1..))
        .unwrap_or_default()
        .to_vec();
    Some(head.into_iter().map(Rc::new).chain(rest).collect())
}

/// What the scroll watcher reports.
#[derive(Deserialize)]
struct Seen {
    post: Option<i64>,
    offset: f64,
    y: f64,
}

/// Reports the first tile on the screen once scrolling stops: a whole one, if
/// there is one, as it stays on the screen when the layout changes. While the
/// page moves, `html` carries `sb-scrolling`, which holds back the light of
/// the tile under the pointer (`main.css`).
const WATCH_SCROLL: &str = r#"
window.__sbSpot?.stop();
const root = document.documentElement;
let timer = 0, last = 0, moving = false;
const rest = () => {
  moving = false;
  root.classList.remove("sb-scrolling");
};
const report = () => {
  const bar = document.querySelector(".topbar")?.getBoundingClientRect().bottom ?? 0;
  let tile = null;
  for (const each of document.querySelectorAll(".masonry .thumb")) {
    const box = each.getBoundingClientRect();
    if (box.bottom > bar && !tile) tile = each;
    if (box.top >= bar) {
      tile = each;
      break;
    }
  }
  if (tile) {
    const post = Number(tile.querySelector("[data-post]")?.dataset.post) || null;
    dioxus.send({ post, offset: tile.getBoundingClientRect().top, y: scrollY });
  }
};
// One timer that looks at how long ago the page last moved, in place of one
// set and cleared on every scroll event: the light comes back after 120 ms of
// rest and the spot is reported after 200.
const settle = () => {
  timer = 0;
  const still = performance.now() - last;
  if (moving && still >= 120) rest();
  if (still >= 200) return report();
  timer = setTimeout(settle, (still < 120 ? 120 : 200) - still);
};
const onscroll = () => {
  last = performance.now();
  if (!moving) {
    moving = true;
    root.classList.add("sb-scrolling");
  }
  timer ||= setTimeout(settle, 120);
};
addEventListener("scroll", onscroll, { passive: true });
window.__sbSpot = {
  report,
  stop() {
    clearTimeout(timer);
    rest();
    removeEventListener("scroll", onscroll);
  },
};
"#;
const WATCH_SCROLL_OFF: &str = "window.__sbSpot?.stop(); window.__sbSpot = null;";

/// Reports which part of the grid to keep on the page, in pixels from its top:
/// the screen and a screen and a half either way, so pictures are there before
/// they scroll into view. It reports again each time the screen has moved a
/// quarter of its height, which is about a row of tiles. Coming less often, a
/// report put three rows on the page at once, and a slow device lost a few
/// frames to each. Browsers go on downloading a picture taken off the page,
/// which after a fast scroll keeps the ones on the screen waiting behind
/// hundreds of others, so it cancels those.
const WATCH_NEAR: &str = r#"
window.__sbNear?.stop();
let held = null;
let frame = 0;
const gone = new MutationObserver((changes) => {
  for (const change of changes) {
    for (const node of change.removedNodes) {
      if (node.nodeType !== 1) continue;
      for (const img of node.querySelectorAll("img")) {
        if (!img.complete) img.removeAttribute("src");
      }
    }
  }
});
const box = document.querySelector(".masonry");
if (box) gone.observe(box, { childList: true });
const check = (force) => {
  frame = 0;
  const grid = document.querySelector(".masonry");
  if (!grid) return;
  const top = -grid.getBoundingClientRect().top;
  const tall = innerHeight;
  if (force || !held || top - tall * 1.25 < held[0] || top + tall * 2.25 > held[1]) {
    held = [top - tall * 1.5, top + tall * 2.5];
    dioxus.send(held);
  }
};
const soon = () => {
  frame ||= requestAnimationFrame(() => check(false));
};
addEventListener("scroll", soon, { passive: true });
addEventListener("resize", soon);
window.__sbNear = {
  check,
  stop() {
    cancelAnimationFrame(frame);
    gone.disconnect();
    removeEventListener("scroll", soon);
    removeEventListener("resize", soon);
  },
};
check(true);
"#;
const WATCH_NEAR_OFF: &str = "window.__sbNear?.stop(); window.__sbNear = null;";

/// Puts a remembered tile back where it was on the screen, or failing that,
/// the scroll position. The list may land a frame after this. It works from the
/// layout, as the tile itself isn't on the page until the screen gets near it.
const JUMP_TO_SPOT: &str = r#"
const [top, offset, y, home] = await dioxus.recv();
const go = tries => {
  const grid = document.querySelector(".masonry")?.getBoundingClientRect();
  const anchored = grid && y >= 1 && top != null;
  if (anchored && top > grid.height && tries > 0) return requestAnimationFrame(() => go(tries - 1));
  if (anchored) scrollBy({ top: grid.top + top - offset, behavior: "instant" });
  else scrollTo({ top: y, behavior: "instant" });
  if (home) window.__sbHero?.home(...home);
  window.__sbNear?.check(true);
  // Only now do the tiles stand where a picture coming back can land (motion.rs).
  window.__sbHero?.back();
};
go(5);
"#;

/// Sort menu entries: `order:` values.
const SORTS: [&str; 11] = [
    "new", "old", "score", "updated", "size", "size_asc", "pixels", "longside", "tags", "tags_asc",
    "random",
];

fn sort_label(order: &str) -> String {
    match order {
        "new" => t!("sort-new"),
        "old" => t!("sort-old"),
        "score" => t!("sort-score"),
        "updated" => t!("sort-updated"),
        "size" => t!("sort-size"),
        "size_asc" => t!("sort-size-asc"),
        "pixels" => t!("sort-pixels"),
        "longside" => t!("sort-longside"),
        "tags" => t!("sort-tags"),
        "tags_asc" => t!("sort-tags-asc"),
        "random" => t!("sort-random"),
        other => other.to_owned(),
    }
}

#[component]
pub fn Gallery(q: String) -> Element {
    // A list kept from earlier is there from the first frame, so that coming
    // back never flashes an empty page. `restart` takes over right after.
    let mut posts = use_signal(|| cached_list(&q).map(|l| l.posts).unwrap_or_default());
    let mut next_page = use_signal(|| 1u32);
    let mut has_more = use_signal(|| cached_list(&q).is_none_or(|l| l.has_more));
    let mut loading = use_signal(|| posts.peek().is_empty());
    let mut error = use_signal(|| None::<String>);
    // Bumped on every new search so late responses for an old query are dropped.
    let mut generation = use_signal(|| 0u64);
    let mut query = use_signal(|| q.clone());
    // Unknown tags the search corrected.
    let mut corrected = use_signal(Vec::<TagCorrection>::new);

    // Tiles below this index aren't new, and don't animate in. Only the ones
    // that land on the page along with new results are.
    let mut settled = use_signal(|| posts.peek().len());
    use_effect(move || settled.set(posts.read().len()));
    let mut loaded_at = use_signal(Utc::now);
    // Bumped to put the page back at the remembered spot.
    let mut jump = use_signal(|| 0u64);
    // The part of the grid on the page, in pixels from its top (`WATCH_NEAR`).
    let mut near = use_signal(|| near_spot(&posts.peek(), store::spot(&q).as_ref()));
    let grid = use_memo(|| Grid::new(&PREFS.read(), GRID_WIDTH()));
    let tiles = use_memo(move || grid().place(&posts.read()));

    let keep = move || {
        keep_list(List {
            q: query.peek().clone(),
            owner: store::owner(),
            posts: posts.peek().clone(),
            next_page: *next_page.peek(),
            has_more: *has_more.peek(),
            corrected: corrected.peek().clone(),
            loaded_at: *loaded_at.peek(),
            seed: seed(&query.peek()),
        });
    };

    let mut load_more = move || {
        if *loading.peek() || !*has_more.peek() {
            return;
        }
        let Some(client) = store::client() else {
            return;
        };
        let generation_at_start = *generation.peek();
        let page = *next_page.peek();
        let q = query.peek().clone();
        loading.set(true);
        spawn(async move {
            let result = client.search_posts(&q, seed(&q), page, PAGE_SIZE).await;
            if *generation.peek() != generation_at_start {
                return;
            }
            loading.set(false);
            match result {
                Ok(found) => {
                    {
                        // Posts added since can push earlier ones onto this page.
                        let mut list = posts.write();
                        let seen: HashSet<i64> = list.iter().map(|p| p.id).collect();
                        let new = found.posts.into_iter().filter(|p| !seen.contains(&p.id));
                        list.extend(new.map(Rc::new));
                    }
                    has_more.set(found.has_more);
                    corrected.set(found.corrected);
                    next_page.set(page.saturating_add(1));
                    keep();
                }
                Err(e) if e.is_unauthorized() => store::forget_session(),
                Err(e) => {
                    has_more.set(false);
                    error.set(Some(e.to_string()));
                }
            }
        });
    };

    // Loads as far as the user had got, all at once, and goes back there.
    let mut restore = move |spot: Spot| {
        let Some(client) = store::client() else {
            return;
        };
        let generation_at_start = *generation.peek();
        let wanted = usize::try_from(spot.count)
            .unwrap_or(MAX_RESTORE)
            .min(MAX_RESTORE);
        loading.set(true);
        spawn(async move {
            let mut found = Vec::<Rc<Post>>::new();
            let mut seen = HashSet::new();
            let mut fixed = Vec::new();
            let mut page = 1u32;
            let mut more = true;
            let mut failure = None;
            while more && found.len() < wanted {
                let at = (page - 1) / RESTORE_PAGES + 1;
                let result = client
                    .search_posts(&spot.q, seed(&spot.q), at, PAGE_SIZE * RESTORE_PAGES)
                    .await;
                if *generation.peek() != generation_at_start {
                    return;
                }
                match result {
                    Ok(results) => {
                        let new = results.posts.into_iter().filter(|p| seen.insert(p.id));
                        found.extend(new.map(Rc::new));
                        more = results.has_more;
                        fixed = results.corrected;
                        page = page.saturating_add(RESTORE_PAGES);
                    }
                    Err(e) if e.is_unauthorized() => {
                        store::forget_session();
                        return;
                    }
                    Err(e) => {
                        failure = Some(e.to_string());
                        break;
                    }
                }
            }
            loading.set(false);
            settled.set(found.len());
            if failure.is_none() {
                near.set(near_spot(&found, Some(&spot)));
            }
            posts.set(found);
            corrected.set(fixed);
            next_page.set(page);
            has_more.set(more && failure.is_none());
            if failure.is_none() {
                keep();
                jump += 1;
            }
            error.set(failure);
        });
    };

    // Picks up what changed at the top of a list kept from earlier.
    let refresh_head = move || {
        let q = query.peek().clone();
        let Some(client) = store::client().filter(|_| !is_random(&q)) else {
            return;
        };
        let generation_at_start = *generation.peek();
        spawn(async move {
            let Ok(found) = client.search_posts(&q, None, 1, PAGE_SIZE).await else {
                return;
            };
            if *generation.peek() != generation_at_start {
                return;
            }
            let head = found.posts.len();
            let merged = merge_head(&posts.peek(), found.posts, found.has_more);
            let Some(merged) = merged else {
                return;
            };
            if merged.len() == head {
                next_page.set(2);
                has_more.set(found.has_more);
            }
            settled.set(merged.len());
            posts.set(merged);
            keep();
            jump += 1;
        });
    };

    let mut restart = move |q: String| {
        generation += 1;
        if parse_query(&q).is_ok() {
            store::visit_search(&q);
        }
        query.set(q.clone());
        loading.set(false);
        error.set(None);
        if let Some(list) = cached_list(&q) {
            if let Some(seed) = list.seed {
                remember_seed(&q, seed);
            }
            settled.set(list.posts.len());
            near.set(near_spot(&list.posts, store::spot(&q).as_ref()));
            posts.set(list.posts);
            corrected.set(list.corrected);
            next_page.set(list.next_page);
            has_more.set(list.has_more);
            loaded_at.set(list.loaded_at);
            jump += 1;
            refresh_head();
            return;
        }
        posts.write().clear();
        corrected.write().clear();
        settled.set(0);
        next_page.set(1);
        has_more.set(true);
        loaded_at.set(Utc::now());
        // A search run again is shuffled again.
        if is_random(&q) {
            reseed(&q);
        }
        match store::spot(&q).filter(|s| s.y >= 1.0 && s.count > 0 && !is_random(&q)) {
            Some(spot) => restore(spot),
            None => load_more(),
        }
    };
    // Bumped to run the search again, in this scope (spawns from a child's
    // task would die with the child).
    let mut reload = use_signal(|| 0u64);
    use_effect(use_reactive!(|q| {
        reload();
        restart(q);
    }));

    // More of the list comes before the user gets to the end of it.
    use_effect(move || {
        let (_, to) = near();
        let Grid { gap, tile, .. } = grid();
        let end = tiles.read().height.px(tile, gap);
        if tile > 0.0
            && to >= end
            && generation() > 0
            && has_more()
            && !loading()
            && error.read().is_none()
        {
            load_more();
        }
    });

    // What finishes uploading while the user stays here shows up here.
    let finished = use_memo(activity::finished_count);
    let mut seen_finished = use_signal(|| *finished.peek());
    use_effect(move || {
        let now = finished();
        if now > *seen_finished.peek() {
            refresh_head();
        }
        seen_finished.set(now);
    });

    // Remembers the spot as the user scrolls. Nothing to remember while the
    // list is loading.
    use_hook(move || {
        spawn(async move {
            let mut watch = document::eval(WATCH_SCROLL);
            while let Ok(seen) = watch.recv::<Seen>().await {
                let count = posts.peek().len();
                if count > 0 {
                    store::remember_spot(Spot {
                        q: query.peek().clone(),
                        post: seen.post,
                        offset: seen.offset,
                        y: seen.y,
                        count: u32::try_from(count).unwrap_or(u32::MAX),
                    });
                }
            }
        });
    });
    use_drop(|| {
        motion::leaving_gallery();
        document::eval(WATCH_SCROLL_OFF);
        document::eval(WATCH_NEAR_OFF);
    });
    use_effect(move || {
        if jump() == 0 {
            return;
        }
        let list = posts.peek();
        let spot = store::spot(&query.peek());
        let (post, offset, y) = spot.map_or((None, 0.0, 0.0), |s| (s.post, s.offset, s.y));
        let top = post.and_then(|id| tile_box(&list, id)).map(|(top, _)| top);
        // The viewer may have moved on since, to a post far from the spot.
        let home = motion::shown()
            .and_then(|id| tile_box(&list, id).map(|(top, height)| (id, top, height)));
        let eval = document::eval(JUMP_TO_SPOT);
        let _ = eval.send((top, offset, y, home));
    });

    // Selection mode for mass edits. `anchor` is the last thumb clicked, for
    // shift-click ranges.
    let mut selecting = use_signal(|| false);
    let mut selected = use_signal(BTreeSet::<i64>::new);
    let mut anchor = use_signal(|| None::<usize>);
    let mut notice = use_signal(|| None::<String>);
    // One handler kept between renders. A new closure each time would be a new
    // property for every tile, and each would render again whenever the part of
    // the grid near the screen moved.
    let toggle = use_callback(move |(index, shift): (usize, bool)| {
        let list = posts.read();
        let Some(id) = list.get(index).map(|p| p.id) else {
            return;
        };
        let mut set = selected.write();
        let range = anchor()
            .filter(|_| shift)
            .map(|a| a.min(index)..=a.max(index));
        if let Some(range) = range {
            let on = !set.contains(&id);
            for p in list.iter().skip(*range.start()).take(range.count()) {
                if on && set.len() < MAX_BULK_POSTS {
                    set.insert(p.id);
                } else if !on {
                    set.remove(&p.id);
                }
            }
        } else if !set.remove(&id) && set.len() < MAX_BULK_POSTS {
            set.insert(id);
        }
        anchor.set(Some(index));
    });
    let mut stop_selecting = move || {
        selecting.set(false);
        selected.write().clear();
        anchor.set(None);
    };
    let ondone = move |message: String| {
        stop_selecting();
        notice.set(Some(message));
        forget_lists();
        reload += 1;
    };

    // What swiping a tile does, and the menu of one held or right-clicked.
    let swipe = use_memo(|| {
        let prefs = PREFS.read();
        (prefs.swipe_left, prefs.swipe_right)
    });
    let mut menu = use_signal(|| None::<(Rc<Post>, MenuAt)>);
    let mut sharing = use_signal(|| None::<i64>);
    let find = move |id: i64| posts.peek().iter().find(|p| p.id == id).cloned();
    let mut run = move |action: SwipeAction, post: Rc<Post>| {
        let id = post.id;
        match action {
            SwipeAction::Off => {}
            SwipeAction::Select => {
                notice.set(None);
                selecting.set(true);
                let mut set = selected.write();
                if !set.remove(&id) && set.len() < MAX_BULK_POSTS {
                    set.insert(id);
                }
            }
            SwipeAction::Share => sharing.set(Some(id)),
            SwipeAction::Download => download::start(&post.file_url, &file_name(&post)),
            SwipeAction::Like => {
                let Some(client) = store::client() else {
                    return;
                };
                let vote = if post.my_vote > 0 { 0 } else { 1 };
                spawn(async move {
                    match client.vote(id, vote).await {
                        Ok(fresh) => {
                            if let Some(slot) = posts.write().iter_mut().find(|p| p.id == id) {
                                *slot = Rc::new(fresh);
                            }
                            keep();
                            notify(if vote > 0 {
                                t!("press-liked")
                            } else {
                                t!("press-unliked")
                            });
                        }
                        Err(e) if e.is_unauthorized() => store::forget_session(),
                        Err(e) => notify(e.to_string()),
                    }
                });
            }
            SwipeAction::Delete => {
                let Some(client) = store::client() else {
                    return;
                };
                spawn(async move {
                    match client.delete_post(id).await {
                        Ok(()) => {
                            forget_post(id);
                            posts.write().retain(|p| p.id != id);
                            keep();
                            notify_undo(t!("press-deleted"), t!("press-undo"), move || {
                                let Some(client) = store::client() else {
                                    return;
                                };
                                // From the notice, which outlives this page.
                                dioxus::core::spawn_forever(async move {
                                    match client.restore_post(id).await {
                                        Ok(_) => {
                                            forget_lists();
                                            if let Ok(mut again) = reload.try_write() {
                                                *again += 1;
                                            }
                                        }
                                        Err(e) => notify(e.to_string()),
                                    }
                                });
                            });
                        }
                        Err(e) if e.is_unauthorized() => store::forget_session(),
                        Err(e) => notify(e.to_string()),
                    }
                });
            }
        }
    };
    let on_press = use_callback(move |press: Press| match press {
        Press::Menu(Target::Post(id), at) => {
            if let Some(post) = find(id) {
                menu.set(Some((post, at)));
            }
        }
        Press::Swipe(Target::Post(id), side) => {
            let (left, right) = *swipe.peek();
            let action = if side == Side::Left { left } else { right };
            if let Some(post) = find(id) {
                run(action, post);
            }
        }
        _ => {}
    });
    use_press("post", on_press);

    // The user's saved searches; `None` until loaded.
    let mut saved = use_signal(|| {
        let owner = store::owner();
        SAVED
            .peek()
            .as_ref()
            .filter(|(whose, _)| *whose == owner)
            .map(|(_, list)| list.clone())
    });
    use_hook(move || {
        if let Some(client) = store::client() {
            spawn(async move {
                if let Ok(list) = client.saved_searches().await {
                    *SAVED.write() = Some((store::owner(), list.clone()));
                    if saved.peek().as_ref() != Some(&list) {
                        saved.set(Some(list));
                        jump += 1;
                    }
                }
            });
        }
    });

    let retry = move |()| {
        error.set(None);
        has_more.set(true);
        load_more();
    };
    // The sort menu shows the order, so the title leaves it out.
    let title = with_order(&q, None);
    // A search for one plain tag links to that tag's page.
    let single_tag = (!title.is_empty() && !title.contains([' ', ':']) && !title.starts_with('-'))
        .then(|| title.clone());
    let is_empty = posts.read().is_empty() && !loading() && error().is_none() && !has_more();
    let Grid {
        columns,
        gap,
        width,
        tile,
        ..
    } = grid();
    // A new layout moves everything, so the same tile goes back to the top.
    let mut laid_out = use_signal(|| *grid.peek());
    use_effect(move || {
        let now = grid();
        if *laid_out.peek() != now {
            laid_out.set(now);
            jump += 1;
        }
    });
    let placed = tiles.read();
    let list = posts.read();
    let (from, to) = near();
    let shown: Vec<_> = placed
        .within(tile, gap, from, to)
        .into_iter()
        .filter_map(|index| Some((index, list.get(index)?, *placed.tiles.get(index)?)))
        .collect();
    // New tiles only make an entrance where it can be seen. The rest, below the
    // screen, would each be drawn on a layer of their own for nobody, and a tile
    // reached by a quick scroll would still be waiting for its turn to appear.
    let (seen_from, seen_to) = may_be_on_screen((from, to));
    let in_sight = move |place: &Place| {
        let top = place.top.px(tile, gap);
        top < seen_to && top + place.height.px(tile, gap) > seen_from
    };

    rsx! {
        div { class: "page gallery",
            div { class: "page-head",
                h1 { class: if title.is_empty() { "page-title visually-hidden" } else { "page-title" },
                    if title.is_empty() { {t!("nav-gallery")} } else { "{title}" }
                }
                if let Some(name) = single_tag {
                    Link { class: "icon-btn tag-info", title: t!("gallery-tag-info"), aria_label: t!("gallery-tag-info"),
                        to: Route::TagPage { name },
                        Ico { icon: Icon::Label }
                    }
                }
                if !q.trim().is_empty() {
                    SaveToggle { q: q.clone(), saved }
                }
                SortMenu { q: q.clone() }
                ColorMenu { q: q.clone() }
                ViewMenu { width, gap }
                button {
                    class: if selecting() { "icon-btn select-toggle on" } else { "icon-btn select-toggle" },
                    title: if selecting() { t!("gallery-stop-selecting") } else { t!("gallery-select") },
                    aria_label: if selecting() { t!("gallery-stop-selecting") } else { t!("gallery-select") },
                    aria_pressed: selecting(),
                    onclick: move |_| {
                        if selecting() {
                            stop_selecting();
                        } else {
                            notice.set(None);
                            selecting.set(true);
                        }
                    },
                    Ico { icon: if selecting() { Icon::Close } else { Icon::Check } }
                }
            }
            if let Some(message) = notice() {
                div { class: "banner info", role: "status", "{message}" }
            }
            if !corrected.read().is_empty() {
                CorrectionBanner {
                    corrected: corrected(),
                    exact: Route::Gallery { q: with_exact(&q) },
                }
            }
            if q.trim().is_empty() {
                SavedList { saved }
            }
            if let Some(message) = error() {
                ErrorBanner { message, onretry: retry }
                if parse_query(&q).is_err() {
                    Link { class: "btn text help-link", to: Route::Help {},
                        Ico { icon: Icon::Help }
                        {t!("gallery-syntax")}
                    }
                }
            }
            if is_empty {
                div { class: "card empty",
                    Ico { icon: Icon::Gallery, size: 48 }
                    if q.is_empty() {
                        h2 { {t!("gallery-empty")} }
                        p { class: "muted", {t!("gallery-empty-body")} }
                        if store::can_upload() {
                            Link { class: "btn", to: Route::Upload {}, Ico { icon: Icon::Upload } {t!("nav-upload")} }
                        }
                    } else {
                        h2 { {t!("no-matches")} }
                        p { class: "muted", {t!("gallery-try-fewer")} }
                        Link { class: "btn text", to: Route::Help {}, {t!("gallery-syntax")} }
                    }
                }
            }
            div {
                class: if selecting() { "masonry-box selecting" } else { "masonry-box" },
                onmounted: move |_| {
                    spawn(async move {
                        let mut watch = document::eval(WATCH_WIDTH);
                        while let Ok(seen) = watch.recv::<f64>().await {
                            if *GRID_WIDTH.peek() != seen {
                                *GRID_WIDTH.write() = seen;
                            }
                        }
                    });
                    spawn(async move {
                        let mut watch = document::eval(WATCH_NEAR);
                        while let Ok(seen) = watch.recv::<(f64, f64)>().await {
                            near.set(seen);
                        }
                    });
                },
                div {
                    class: "masonry",
                    style: "--columns: {columns}; --height: {placed.height.tiles:.5}; --height-gaps: {placed.height.gaps:.5}",
                    for (index, post, place) in shown {
                        Thumb {
                            key: "{post.id}",
                            post: post.clone(),
                            index,
                            q: q.clone(),
                            selected: selecting().then(|| selected.read().contains(&post.id)),
                            ontoggle: toggle,
                            place,
                            still: index < settled() || !in_sight(&place),
                            swipe: swipe(),
                        }
                    }
                }
            }
            if has_more() && error().is_none() {
                div { class: "sentinel",
                    if loading() {
                        Spinner {}
                    } else {
                        button { class: "btn tonal", onclick: move |_| load_more(), {t!("load-more")} }
                    }
                }
            }
            if selecting() {
                BulkBar {
                    selected,
                    loaded: posts.read().iter().map(|p| p.id).collect::<Vec<_>>(),
                    ondone,
                }
            }
            DropToUpload {}
        }
        ToTop { selecting: selecting() }
        if let Some((post, at)) = menu() {
            PostMenu {
                post,
                at,
                onopen: {
                    let q = q.clone();
                    move |post: Rc<Post>| open_post(&post, q.clone())
                },
                onpick: move |(action, post)| {
                    menu.set(None);
                    run(action, post);
                },
                onclose: move |()| menu.set(None),
            }
        }
        if let Some(id) = sharing() {
            ShareSheet { post_ids: vec![id], onclose: move |()| sharing.set(None) }
        }
    }
}

/// Says whether the page is more than a screen down, each time that changes.
const WATCH_FAR: &str = r#"
window.__sbFar?.stop();
let frame = 0, far = null;
const check = () => {
  frame = 0;
  const now = scrollY > innerHeight;
  if (now !== far) {
    far = now;
    dioxus.send(now);
  }
};
const soon = () => {
  frame ||= requestAnimationFrame(check);
};
addEventListener("scroll", soon, { passive: true });
addEventListener("resize", soon);
window.__sbFar = {
  stop() {
    cancelAnimationFrame(frame);
    removeEventListener("scroll", soon);
    removeEventListener("resize", soon);
  },
};
check();
"#;
const WATCH_FAR_OFF: &str = "window.__sbFar?.stop(); window.__sbFar = null;";

/// The arrow back to the top, once the page is a screen down. A component of
/// its own, so the gallery isn't drawn again each time it comes or goes. It
/// stays out of the way of a selection's bar.
#[component]
fn ToTop(selecting: bool) -> Element {
    let mut far = use_signal(|| false);
    use_hook(move || {
        spawn(async move {
            let mut watch = document::eval(WATCH_FAR);
            while let Ok(seen) = watch.recv::<bool>().await {
                if *far.peek() != seen {
                    far.set(seen);
                }
            }
        });
    });
    use_drop(|| {
        document::eval(WATCH_FAR_OFF);
        motion::stop_to_top();
    });
    let shown = far() && !selecting;
    rsx! {
        button {
            class: if shown { "to-top shown" } else { "to-top" },
            r#type: "button",
            title: t!("gallery-to-top"),
            aria_label: t!("gallery-to-top"),
            onclick: move |_| motion::to_top(),
            Ico { icon: Icon::ArrowUp }
        }
    }
}

/// The bar of a selection: tags, rating and delete for every selected post.
#[component]
fn BulkBar(
    selected: Signal<BTreeSet<i64>>,
    loaded: Vec<i64>,
    ondone: EventHandler<String>,
) -> Element {
    let mut selected = selected;
    let mut tags = use_signal(String::new);
    let mut rating = use_signal(|| None::<Rating>);
    let mut busy = use_signal(|| false);
    let mut confirm_delete = use_signal(|| false);
    let mut sharing = use_signal(|| false);
    let mut error = use_signal(|| None::<String>);
    let count = selected.read().len();
    let (add, remove) = split_tag_edit(&tags());
    let has_edit = !add.is_empty() || !remove.is_empty() || rating().is_some();
    let all_selected = !loaded.is_empty() && loaded.iter().all(|id| selected.read().contains(id));

    let mut send = move |edit: BulkEdit| {
        let Some(client) = store::client() else {
            return;
        };
        busy.set(true);
        error.set(None);
        spawn(async move {
            let deleting = edit.delete;
            let result = client.bulk_edit(&edit).await;
            // Before `ondone`, which closes this bar and drops its signals.
            busy.set(false);
            match result {
                Ok(done) => {
                    let count = done.updated;
                    let main = if deleting {
                        t!("bulk-moved", count = count)
                    } else {
                        t!("bulk-updated", count = count)
                    };
                    let message = match (done.skipped, deleting) {
                        (0, _) => t!("bulk-done", done = main),
                        (n, true) => t!(
                            "bulk-done-skipped",
                            done = main,
                            skipped = t!("bulk-skipped-undeletable", count = n)
                        ),
                        (n, false) => t!(
                            "bulk-done-skipped",
                            done = main,
                            skipped = t!("bulk-skipped", count = n)
                        ),
                    };
                    ondone.call(message);
                }
                Err(e) => error.set(Some(e.to_string())),
            }
        });
    };
    let apply = move |e: FormEvent| {
        e.prevent_default();
        let (add_tags, remove_tags) = split_tag_edit(&tags.peek());
        send(BulkEdit {
            ids: selected.peek().iter().copied().collect(),
            add_tags,
            remove_tags,
            rating: *rating.peek(),
            delete: false,
        });
    };
    let delete = move |_| {
        if !confirm_delete() {
            confirm_delete.set(true);
            return;
        }
        confirm_delete.set(false);
        send(BulkEdit {
            ids: selected.peek().iter().copied().collect(),
            delete: true,
            ..BulkEdit::default()
        });
    };

    rsx! {
        form { class: "bulk-bar glass", aria_label: t!("bulk-label"), onsubmit: apply,
            div { class: "bulk-count",
                strong { role: "status", if count == 0 { {t!("gallery-select")} } else { {t!("bulk-selected", count = count)} } }
                button {
                    r#type: "button",
                    class: "btn text small",
                    disabled: loaded.is_empty(),
                    onclick: move |_| {
                        let mut set = selected.write();
                        if all_selected {
                            set.clear();
                        } else {
                            for id in loaded.iter().take(MAX_BULK_POSTS.saturating_sub(set.len())) {
                                set.insert(*id);
                            }
                        }
                    },
                    if all_selected { {t!("bulk-select-none")} } else { {t!("bulk-select-all")} }
                }
            }
            input {
                class: "bulk-tags",
                r#type: "text",
                placeholder: t!("bulk-tags-placeholder"),
                title: t!("bulk-tags-title"),
                aria_label: t!("bulk-tags-label"),
                autocapitalize: "none",
                spellcheck: false,
                value: "{tags}",
                oninput: move |e| tags.set(e.value()),
            }
            Select {
                class: "bulk-rating",
                aria_label: t!("rating"),
                value: rating().map(|r| r.as_str().to_string()).unwrap_or_default(),
                options: std::iter::once(Choice::new("", t!("bulk-keep-rating")))
                    .chain(Rating::ALL.into_iter().map(|r| Choice::new(r.as_str(), rating_label(r))))
                    .collect::<Vec<_>>(),
                onchange: move |v: String| rating.set(Rating::parse(&v)),
            }
            button {
                class: "btn",
                r#type: "submit",
                disabled: busy() || count == 0 || !has_edit,
                Ico { icon: Icon::Check }
                {t!("bulk-apply")}
            }
            button {
                r#type: "button",
                class: "btn tonal",
                disabled: busy() || count == 0,
                aria_label: t!("share-button"),
                title: t!("share-button"),
                onclick: move |_| sharing.set(true),
                Ico { icon: Icon::Share }
                span { class: "bulk-label", {t!("share-button")} }
            }
            button {
                r#type: "button",
                class: if confirm_delete() { "btn danger" } else { "btn tonal" },
                disabled: busy() || count == 0,
                onclick: delete,
                onblur: move |_| confirm_delete.set(false),
                aria_label: if confirm_delete() { t!("bulk-delete-confirm-label", count = count) } else { t!("post-delete") },
                Ico { icon: Icon::Delete }
                if confirm_delete() {
                    {t!("bulk-delete-confirm", count = count)}
                } else {
                    span { class: "bulk-label", {t!("post-delete")} }
                }
            }
            if let Some(message) = error() {
                p { class: "bulk-error", role: "alert", "{message}" }
            }
        }
        if sharing() {
            ShareSheet {
                post_ids: in_gallery_order(&loaded, &selected.read()),
                onclose: move |()| sharing.set(false),
            }
        }
    }
}

/// The selected ids as the gallery shows them, so a link keeps that order.
fn in_gallery_order(loaded: &[i64], selected: &BTreeSet<i64>) -> Vec<i64> {
    let mut ids: Vec<i64> = loaded
        .iter()
        .copied()
        .filter(|id| selected.contains(id))
        .collect();
    ids.extend(selected.iter().filter(|id| !loaded.contains(id)));
    ids
}

/// Star in the header: saves the current search, or forgets it.
#[component]
fn SaveToggle(q: String, saved: Signal<Option<Vec<SavedSearch>>>) -> Element {
    let mut saved = saved;
    let mut busy = use_signal(|| false);
    let query = q.split_whitespace().collect::<Vec<_>>().join(" ");
    let existing = saved
        .read()
        .as_ref()
        .and_then(|list| list.iter().find(|s| s.query == query).map(|s| s.id));
    let label = if existing.is_some() {
        t!("search-forget")
    } else {
        t!("search-save")
    };
    let toggle = move |_| {
        let Some(client) = store::client() else {
            return;
        };
        let query = query.clone();
        busy.set(true);
        spawn(async move {
            let result = match existing {
                Some(id) => client.delete_search(id).await.map(|()| None),
                None => {
                    let name: String = query.chars().take(MAX_SEARCH_NAME_CHARS).collect();
                    client.save_search(&name, &query).await.map(Some)
                }
            };
            if let Ok(added) = result
                && let Some(list) = saved.write().as_mut()
            {
                list.retain(|s| Some(s.id) != existing);
                if let Some(s) = added {
                    list.push(s);
                    list.sort_by_key(|s| s.name.to_lowercase());
                }
            }
            busy.set(false);
        });
    };
    rsx! {
        button {
            class: if existing.is_some() { "icon-btn save-search on" } else { "icon-btn save-search" },
            title: label.clone(),
            aria_label: label,
            aria_pressed: existing.is_some(),
            disabled: busy() || saved.read().is_none(),
            onclick: toggle,
            Ico { icon: if existing.is_some() { Icon::Star } else { Icon::StarOutline } }
        }
    }
}

/// The saved searches as chips, on the unfiltered gallery.
#[component]
fn SavedList(saved: Signal<Option<Vec<SavedSearch>>>) -> Element {
    let list = saved.read();
    let Some(list) = list.as_ref().filter(|l| !l.is_empty()) else {
        return rsx! {};
    };
    rsx! {
        nav { class: "saved-searches", aria_label: t!("saved-searches"),
            for s in list.iter() {
                Link { key: "{s.id}", class: "chip", title: "{s.query}", to: Route::Gallery { q: s.query.clone() },
                    Ico { icon: Icon::Star, size: 16 }
                    "{s.name}"
                }
            }
        }
    }
}

/// Rewrites the query's `order:` term and navigates to the result.
#[component]
fn SortMenu(q: String) -> Element {
    let current = parse_query(&q).map(|p| p.order).unwrap_or_default();
    rsx! {
        div { class: "sort",
            span { class: "muted", aria_hidden: "true", {t!("sort")} }
            Select {
                aria_label: t!("sort-by"),
                value: SORTS.iter().find(|v| Order::parse(v) == Some(current)).copied().unwrap_or_default().to_string(),
                options: SORTS.iter().map(|v| Choice::new(*v, sort_label(v))).collect::<Vec<_>>(),
                onchange: move |v: String| {
                    navigator().push(Route::Gallery { q: with_order(&q, Order::parse(&v)) });
                },
            }
        }
    }
}

fn color_label(name: &str) -> String {
    match name {
        "red" => t!("color-red"),
        "orange" => t!("color-orange"),
        "yellow" => t!("color-yellow"),
        "green" => t!("color-green"),
        "teal" => t!("color-teal"),
        "blue" => t!("color-blue"),
        "purple" => t!("color-purple"),
        "pink" => t!("color-pink"),
        "brown" => t!("color-brown"),
        "black" => t!("color-black"),
        "gray" => t!("color-gray"),
        "white" => t!("color-white"),
        other => other.to_owned(),
    }
}

/// Rewrites the query's `color:` term from the colour picker or named swatches.
#[component]
fn ColorMenu(q: String) -> Element {
    let mut open = use_signal(|| false);
    let current = parse_query(&q).ok().and_then(|p| p.colors.first().cloned());
    let pick = use_callback({
        let q = q.clone();
        move |value: Option<String>| {
            open.set(false);
            navigator().push(Route::Gallery {
                q: with_color(&q, value.as_deref()),
            });
        }
    });
    let current_name = current.as_ref().map(|c| c.raw.clone());
    let dot = current.as_ref().map(|c| to_hex(c.rgb));
    rsx! {
        div {
            class: "color-pick",
            onkeydown: move |e: KeyboardEvent| {
                if open() && e.key() == Key::Escape {
                    e.stop_propagation();
                    open.set(false);
                }
            },
            button {
                r#type: "button",
                class: if dot.is_some() { "icon-btn color-toggle on" } else { "icon-btn color-toggle" },
                title: t!("color-search"),
                aria_label: t!("color-search"),
                aria_haspopup: "dialog",
                aria_expanded: if open() { "true" } else { "false" },
                onclick: move |_| open.toggle(),
                Ico { icon: Icon::Palette }
                if let Some(hex) = &dot {
                    span { class: "color-dot", style: "background-color: #{hex}" }
                }
            }
            if open() {
                div { class: "select-backdrop", onclick: move |_| open.set(false) }
                div { class: "color-menu", role: "dialog", aria_label: t!("color-search"),
                    ColorPicker {
                        value: current.as_ref().map(|c| c.rgb),
                        on_pick: move |rgb| pick.call(Some(to_hex(rgb))),
                    }
                    div { class: "swatches",
                        for (name, rgb) in NAMED_COLORS {
                            button {
                                key: "{name}",
                                r#type: "button",
                                class: "swatch",
                                style: "background-color: #{to_hex(rgb)}",
                                title: color_label(name),
                                aria_label: color_label(name),
                                aria_pressed: if current_name.as_deref() == Some(name) { "true" } else { "false" },
                                onclick: move |_| pick.call(Some(name.to_owned())),
                            }
                        }
                    }
                    if current_name.is_some() {
                        button {
                            r#type: "button",
                            class: "btn text",
                            onclick: move |_| pick.call(None),
                            {t!("color-clear")}
                        }
                    }
                }
            }
        }
    }
}

/// How the gallery's tiles look: shape and number of columns. The limits
/// follow the grid's width, so the slider only offers sizes that work here.
#[component]
fn ViewMenu(width: f64, gap: f64) -> Element {
    let mut open = use_signal(|| false);
    let (view, chosen) = {
        let prefs = PREFS.read();
        (prefs.gallery_view, prefs.gallery_columns)
    };
    let (fewest, most) = layout::column_limits(width, gap, view);
    let columns = layout::columns(width, gap, view, chosen);
    let set_view = |next: GalleryView| PREFS.write().gallery_view = next;
    rsx! {
        div {
            class: "view-pick",
            onkeydown: move |e: KeyboardEvent| {
                if open() && e.key() == Key::Escape {
                    e.stop_propagation();
                    open.set(false);
                }
            },
            button {
                r#type: "button",
                class: "icon-btn view-toggle",
                title: t!("view-title"),
                aria_label: t!("view-title"),
                aria_haspopup: "dialog",
                aria_expanded: if open() { "true" } else { "false" },
                onclick: move |_| open.toggle(),
                Ico { icon: Icon::Layout }
            }
            if open() {
                div { class: "select-backdrop", onclick: move |_| open.set(false) }
                div { class: "view-menu", role: "dialog", aria_label: t!("view-title"),
                    div { class: "segmented", role: "radiogroup", aria_label: t!("view-shape"),
                        for shape in GalleryView::FIXED {
                            button {
                                key: "{shape.as_str()}",
                                r#type: "button",
                                role: "radio",
                                aria_checked: if view == shape { "true" } else { "false" },
                                class: if view == shape { "seg on" } else { "seg" },
                                onclick: move |_| set_view(shape),
                                {shape.label()}
                            }
                        }
                        button {
                            r#type: "button",
                            role: "radio",
                            aria_checked: if view.is_staggered() { "true" } else { "false" },
                            class: if view.is_staggered() { "seg on" } else { "seg" },
                            onclick: move |_| {
                                if !view.is_staggered() {
                                    set_view(GalleryView::default());
                                }
                            },
                            {t!("view-staggered")}
                        }
                    }
                    if view.is_staggered() {
                        div { class: "view-row",
                            span { class: "muted", {t!("view-staggered-by")} }
                            div { class: "segmented", role: "radiogroup", aria_label: t!("view-staggered-by"),
                                for shape in GalleryView::STAGGERED {
                                    button {
                                        key: "{shape.as_str()}",
                                        r#type: "button",
                                        role: "radio",
                                        aria_checked: if view == shape { "true" } else { "false" },
                                        class: if view == shape { "seg on" } else { "seg" },
                                        onclick: move |_| set_view(shape),
                                        {shape.label()}
                                    }
                                }
                            }
                        }
                    }
                    div { class: "view-row",
                        label {
                            class: "muted",
                            r#for: "view-columns",
                            if view == GalleryView::StaggeredLandscape {
                                {t!("view-per-row", count = columns)}
                            } else {
                                {t!("view-columns", count = columns)}
                            }
                        }
                        input {
                            id: "view-columns",
                            r#type: "range",
                            min: "{fewest}",
                            max: "{most}",
                            step: "1",
                            disabled: fewest == most,
                            value: "{columns}",
                            oninput: move |e| {
                                if let Ok(n) = e.value().parse::<u8>() {
                                    PREFS.write().gallery_columns = Some(n);
                                }
                            },
                        }
                        button {
                            r#type: "button",
                            class: "btn text small",
                            aria_pressed: if chosen.is_none() { "true" } else { "false" },
                            onclick: move |_| PREFS.write().gallery_columns = None,
                            {t!("view-columns-auto")}
                        }
                    }
                }
            }
        }
    }
}

/// What can be done with a post held or right-clicked in the grid. Outside the
/// gallery (`listed` off) it leaves out what needs the gallery's list:
/// selecting, and deleting from it.
#[component]
fn PostMenu(
    post: Rc<Post>,
    at: MenuAt,
    #[props(default = true)] listed: bool,
    onopen: EventHandler<Rc<Post>>,
    onpick: EventHandler<(SwipeAction, Rc<Post>)>,
    onclose: EventHandler<()>,
) -> Element {
    let held = post.clone();
    let pick = move |action: SwipeAction| {
        let post = held.clone();
        move |()| onpick.call((action, post.clone()))
    };
    let opened = post.clone();
    let liked = post.my_vote > 0;
    rsx! {
        ActionMenu { at, title: t!("post-alt", id = post.id), onclose,
            MenuItem {
                icon: Icon::Fullscreen,
                label: t!("press-open"),
                onclick: move |()| {
                    onclose.call(());
                    onopen.call(opened.clone());
                },
            }
            MenuItem {
                icon: Icon::ThumbUp,
                label: if liked { t!("press-unlike") } else { t!("swipe-like") },
                onclick: pick(SwipeAction::Like),
            }
            if listed {
                MenuItem { icon: Icon::Check, label: t!("swipe-select"), onclick: pick(SwipeAction::Select) }
            }
            MenuItem { icon: Icon::Share, label: t!("share-button"), onclick: pick(SwipeAction::Share) }
            MenuItem { icon: Icon::Download, label: t!("post-download"), onclick: pick(SwipeAction::Download) }
            if listed {
                MenuItem { icon: Icon::Delete, label: t!("post-delete-title"), danger: true, onclick: pick(SwipeAction::Delete) }
            }
        }
    }
}

/// The menu a held or right-clicked thumbnail opens on a page that isn't the
/// gallery: the related posts under a picture, Explore. `find` gives the post
/// a thumbnail stands for, and `onopen` opens it as a click on it would.
#[component]
pub(super) fn PostPress(find: Callback<i64, Option<Post>>, onopen: EventHandler<Post>) -> Element {
    let mut menu = use_signal(|| None::<(Rc<Post>, MenuAt)>);
    let mut sharing = use_signal(|| None::<i64>);
    // Votes cast from here. The lists these posts come from aren't asked for
    // again, and the menu should still say which way the next press goes.
    let mut votes = use_signal(HashMap::<i64, i16>::new);
    use_press(
        "post",
        use_callback(move |press: Press| {
            if let Press::Menu(Target::Post(id), at) = press
                && let Some(mut post) = find.call(id)
            {
                if let Some(vote) = votes.peek().get(&id) {
                    post.my_vote = *vote;
                }
                menu.set(Some((Rc::new(post), at)));
            }
        }),
    );
    let run = move |(action, post): (SwipeAction, Rc<Post>)| {
        menu.set(None);
        let id = post.id;
        match action {
            SwipeAction::Share => sharing.set(Some(id)),
            SwipeAction::Download => download::start(&post.file_url, &file_name(&post)),
            SwipeAction::Like => {
                let Some(client) = store::client() else {
                    return;
                };
                let vote = if post.my_vote > 0 { 0 } else { 1 };
                spawn(async move {
                    match client.vote(id, vote).await {
                        Ok(fresh) => {
                            votes.write().insert(id, fresh.my_vote);
                            forget_lists();
                            notify(if vote > 0 {
                                t!("press-liked")
                            } else {
                                t!("press-unliked")
                            });
                        }
                        Err(e) if e.is_unauthorized() => store::forget_session(),
                        Err(e) => notify(e.to_string()),
                    }
                });
            }
            SwipeAction::Off | SwipeAction::Select | SwipeAction::Delete => {}
        }
    };
    rsx! {
        if let Some((post, at)) = menu() {
            PostMenu {
                post,
                at,
                listed: false,
                onopen: move |post: Rc<Post>| onopen.call((*post).clone()),
                onpick: run,
                onclose: move |()| menu.set(None),
            }
        }
        if let Some(id) = sharing() {
            ShareSheet { post_ids: vec![id], onclose: move |()| sharing.set(None) }
        }
    }
}

/// Opens a post from the grid, the picture flying from its tile.
pub(super) fn open_post(post: &Post, q: String) {
    *OPENING.write() = Some(post.clone());
    let id = post.id;
    let nav = navigator();
    spawn(async move {
        motion::open(id).await;
        nav.push(Route::PostView { id, q });
    });
}

fn swipe_icon(action: SwipeAction) -> Icon {
    match action {
        SwipeAction::Off => Icon::Close,
        SwipeAction::Like => Icon::ThumbUp,
        SwipeAction::Select => Icon::Check,
        SwipeAction::Share => Icon::Share,
        SwipeAction::Download => Icon::Download,
        SwipeAction::Delete => Icon::Delete,
    }
}

/// The name a post's file is saved under.
fn file_name(post: &Post) -> String {
    let ext = post.mime.rsplit('/').next().unwrap_or("bin");
    format!("{}.{ext}", post.sha256)
}

/// A post in the grid. With `selected` set (selection mode), a click
/// toggles it instead of opening it. Both modes render the same elements, so
/// turning selection on doesn't load the thumbnails again. `place` is its
/// spot in the gallery's layout; other grids leave it out. `still` skips the
/// entrance, for a tile that isn't new to the user; only its value when the
/// tile appears counts, so that a later change can't cut the entrance short.
#[component]
pub(super) fn Thumb(
    post: Rc<Post>,
    index: usize,
    q: String,
    selected: Option<bool>,
    ontoggle: EventHandler<(usize, bool)>,
    place: Option<Place>,
    #[props(default)] still: bool,
    /// What swiping it to the left and to the right does, in the gallery.
    #[props(default)]
    swipe: (SwipeAction, SwipeAction),
) -> Element {
    let still = use_hook(|| still);
    let client = store::client();
    let src = post
        .thumb_url
        .as_deref()
        .and_then(|u| client.as_ref().map(|c| cache::url(c, u)));
    let video = matches!(post.kind, MediaKind::Video);
    let preview_src = client
        .as_ref()
        .filter(|_| video)
        .map(|c| c.absolute(&post.file_url));
    // Mouse hover only: a tap on a phone should open the post, not play it.
    let mut hovered = use_signal(|| false);
    // Staggered entrance within a page of results.
    let delay = (index % PAGE_SIZE as usize) * 16;
    let mut style = format!("--delay: {delay}ms");
    if let Some(colour) = tint(&post.palette) {
        style.push_str(&format!("; --tint: {colour}"));
    }
    if let Some(p) = place {
        style.push_str(&format!(
            "; --x: {:.5}; --x-gaps: {:.5}; --y: {:.5}; --y-gaps: {:.5}\
             ; --w: {:.5}; --w-gaps: {:.5}; --h: {:.5}; --h-gaps: {:.5}",
            p.left.tiles,
            p.left.gaps,
            p.top.tiles,
            p.top.gaps,
            p.width.tiles,
            p.width.gaps,
            p.height.tiles,
            p.height.gaps,
        ));
    }
    let alt = post.tags.join(" ");
    let label = if alt.is_empty() {
        t!("post-alt", id = post.id)
    } else {
        alt.clone()
    };
    let duration = post.duration_ms.and_then(format_duration);
    let id = post.id;
    let kind = post.kind;
    let to = Route::PostView { id, q: q.clone() };
    let class = match (selected, still) {
        (Some(true), false) => "thumb selectable on",
        (Some(true), true) => "thumb selectable on still",
        (Some(false), false) => "thumb selectable",
        (Some(false), true) => "thumb selectable still",
        (None, false) => "thumb",
        (None, true) => "thumb still",
    };
    let open = move || open_post(&post, q.clone());
    let picture = rsx! {
        if let Some(src) = src {
            // The gallery only puts tiles near the screen on the page.
            img { src, alt, loading: if place.is_some() { "eager" } else { "lazy" }, decoding: "async" }
        } else {
            div { class: "thumb-placeholder", Ico { icon: Icon::Play, size: 40 } }
        }
    };
    rsx! {
        a {
            class,
            style,
            href: "{to}",
            aria_label: label,
            role: selected.map(|_| "button"),
            aria_pressed: selected.map(|on| on.to_string()),
            // Held or right-clicked for its menu, and swiped (gesture.rs).
            "data-press": "post:{id}",
            onclick: move |e: MouseEvent| {
                if selected.is_some() {
                    e.prevent_default();
                    ontoggle.call((index, e.modifiers().shift()));
                } else if e.modifiers().is_empty() {
                    // With a modifier the browser opens the link elsewhere.
                    e.prevent_default();
                    open();
                }
            },
            onkeydown: move |e: KeyboardEvent| {
                if selected.is_some() && e.key() == Key::Character(" ".to_owned()) {
                    e.prevent_default();
                    ontoggle.call((index, e.modifiers().shift()));
                }
            },
            // Under the picture, which uncovers them as it's swiped aside.
            for (side, action) in [("left", swipe.0), ("right", swipe.1)] {
                if action != SwipeAction::Off {
                    span { class: "swipe-hint", "data-side": side, "data-action": action.as_str(), aria_hidden: "true",
                        Ico { icon: swipe_icon(action) }
                    }
                }
            }
            if video {
                // Only a video has a use for the pointer coming and going. These
                // events don't bubble, so each tile that listens gets a listener
                // of its own, and a page scrolling under the pointer would call
                // into the app twice a frame for pictures that ignore it.
                div {
                    class: "thumb-media",
                    "data-post": "{id}",
                    onpointerenter: move |e: PointerEvent| {
                        if e.pointer_type() == "mouse" {
                            hovered.set(true);
                        }
                    },
                    onpointerleave: move |_| {
                        if *hovered.peek() {
                            hovered.set(false);
                        }
                    },
                    {picture}
                    if hovered() && !power::active() && let Some(src) = preview_src {
                        // Parsed HTML, because only a parsed `muted` attribute
                        // mutes the video, and browsers autoplay muted videos only.
                        div {
                            class: "preview",
                            aria_hidden: "true",
                            dangerous_inner_html: "<video src=\"{escape_attr(&src)}\" muted autoplay loop playsinline preload=\"auto\"></video>",
                        }
                    }
                }
            } else {
                div { class: "thumb-media", "data-post": "{id}", {picture} }
            }
            match kind {
                MediaKind::Video => rsx! {
                    span { class: "badge",
                        Ico { icon: Icon::Play, size: 16 }
                        if let Some(d) = duration { "{d}" }
                    }
                },
                MediaKind::Animation => rsx! { span { class: "badge", "GIF" } },
                MediaKind::Image => rsx! {},
            }
            if selected.is_some() {
                span { class: "check-mark", aria_hidden: "true", Ico { icon: Icon::Check, size: 20 } }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn post(id: i64) -> Post {
        serde_json::from_value(serde_json::json!({
            "id": id, "kind": "image", "mime": "image/png", "file_size": 1,
            "width": 1, "height": 1, "sha256": "", "rating": "general", "score": 0,
            "my_vote": 0, "source": null, "tags": [],
            "created_at": "2026-01-01T00:00:00Z", "deleted": false,
            "file_url": "", "thumb_url": null,
        }))
        .unwrap()
    }

    fn merged(current: &[i64], head: &[i64], has_more: bool) -> Option<Vec<i64>> {
        let current: Vec<Rc<Post>> = current.iter().copied().map(post).map(Rc::new).collect();
        let head = head.iter().copied().map(post).collect();
        merge_head(&current, head, has_more).map(|l| l.iter().map(|p| p.id).collect())
    }

    #[test]
    fn nothing_new_changes_nothing() {
        assert_eq!(merged(&[5, 4, 3, 2, 1], &[5, 4], true), None);
        assert_eq!(merged(&[5, 4], &[5, 4], false), None);
    }

    #[test]
    fn new_posts_go_on_top_of_what_was_loaded() {
        assert_eq!(
            merged(&[5, 4, 3, 2, 1], &[7, 6], true),
            Some(vec![7, 6]),
            "no overlap: a whole page newer, so the rest is out of step",
        );
        assert_eq!(
            merged(&[5, 4, 3, 2, 1], &[6, 5], true),
            Some(vec![6, 5, 4, 3, 2, 1]),
        );
    }

    #[test]
    fn posts_gone_from_the_first_page_go() {
        assert_eq!(
            merged(&[5, 4, 3, 2, 1], &[5, 3], true),
            Some(vec![5, 3, 2, 1]),
        );
        // No further pages: the first page is the whole list.
        assert_eq!(merged(&[5, 4, 3], &[5, 4], false), Some(vec![5, 4]));
        assert_eq!(merged(&[5, 4, 3], &[], false), Some(vec![]));
    }

    #[test]
    fn the_screen_is_inside_the_part_kept_whichever_way_it_moved() {
        // The page reported with the screen at 3000..3800, so it keeps 1800..5000.
        let (screen_top, screen) = (3000.0, 800.0);
        let kept = (screen_top - screen * 1.5, screen_top + screen * 2.5);
        let (from, to) = may_be_on_screen(kept);
        // It reports again once the screen has moved a quarter of itself.
        for moved in [-0.25, 0.0, 0.25] {
            let top = screen_top + screen * moved;
            assert!(from <= top && top + screen <= to, "moved {moved}");
        }
        // Tiles the next report would still not show are out.
        assert!(to < kept.1 && from > kept.0);
        // The page's own numbers, which this follows.
        assert!(WATCH_NEAR.contains("top - tall * 1.25 < held[0] || top + tall * 2.25 > held[1]"));
        assert!(WATCH_NEAR.contains("held = [top - tall * 1.5, top + tall * 2.5];"));
    }

    #[test]
    fn only_a_plain_colour_tints_a_tile() {
        let one = |c: &str| vec![c.to_owned()];
        assert_eq!(tint(&one("#41ab78")), Some("#41ab78"));
        assert_eq!(tint(&one("#41AB78")), Some("#41AB78"));
        assert_eq!(tint(&[]), None);
        for bad in [
            "41ab78",
            "#41ab7",
            "#41ab788",
            "#41ab7g",
            "red",
            "#fff; background: url(//x)",
            "#àààààà",
            "",
        ] {
            assert_eq!(tint(&one(bad)), None, "{bad}");
        }
    }

    #[test]
    fn the_light_under_the_pointer_waits_for_the_page_to_stop() {
        // Regression: scrolling with the pointer over the grid started a fade,
        // and a layer for it, on a new tile every frame.
        let css = include_str!("../../assets/main.css");
        assert!(css.contains("html.sb-scrolling .thumb img { filter: none; transition: none; }"));
        assert!(WATCH_SCROLL.contains(r#"root.classList.add("sb-scrolling")"#));
        // Leaving the gallery mid-scroll takes the mark with it.
        let (_, stop) = WATCH_SCROLL.split_once("stop() {").unwrap();
        assert!(stop.contains("rest();"));
        assert!(WATCH_SCROLL.contains(r#"root.classList.remove("sb-scrolling")"#));
    }

    #[test]
    fn a_new_page_of_results_restyles_the_grid_alone() {
        // Regression: `--height` was inherited, so setting it on the grid meant
        // working out the style of every tile on the page again.
        let css = include_str!("../../assets/main.css");
        for name in ["--height", "--height-gaps"] {
            assert!(css.contains(&format!(
                "@property {name} {{ syntax: \"<number>\"; inherits: false; initial-value: 0; }}"
            )));
        }
    }

    #[test]
    fn a_tile_waits_in_its_own_colour() {
        let css = include_str!("../../assets/main.css");
        assert!(css.contains("background: var(--tint, var(--surface-2));"));
        // A selected tile keeps its frame, whatever the colour under it.
        assert!(css.contains(".thumb.selectable.on { background: var(--primary-container); }"));
    }

    /// How many times `Probe` has rendered.
    static PROBED: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

    /// A component with a handler among its properties, as `Thumb` has.
    #[component]
    fn Probe(id: i64, onpick: EventHandler<(usize, bool)>) -> Element {
        PROBED.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        rsx! { button { onclick: move |_| onpick.call((0, false)), "{id}" } }
    }

    /// Renders `app` once, then again `again` times, and says how often
    /// `Probe` rendered in all.
    fn probe_renders(app: fn() -> Element, again: usize) -> usize {
        use dioxus::core::{NoOpMutations, ScopeId, VirtualDom};
        PROBED.store(0, std::sync::atomic::Ordering::SeqCst);
        let mut dom = VirtualDom::new(app);
        dom.rebuild_in_place();
        for _ in 0..again {
            dom.mark_dirty(ScopeId::APP);
            dom.render_immediate(&mut NoOpMutations);
        }
        PROBED.load(std::sync::atomic::Ordering::SeqCst)
    }

    #[test]
    fn a_handler_kept_between_renders_leaves_the_tile_alone() {
        // Regression: the gallery gave each tile a new closure on every render.
        // A handler is the same only if it is the same one, so all the tiles on
        // the page rendered again each time the part near the screen moved.
        fn fresh() -> Element {
            rsx! { Probe { id: 1, onpick: move |_| {} } }
        }
        fn kept() -> Element {
            let onpick = use_callback(move |_: (usize, bool)| {});
            rsx! { Probe { id: 1, onpick } }
        }
        // One test, as the two share the counter.
        assert_eq!(probe_renders(fresh, 3), 4);
        assert_eq!(probe_renders(kept, 3), 1);
    }

    #[test]
    fn the_arrow_to_the_top_is_in_the_middle_unless_the_bar_is_at_the_bottom() {
        let css = include_str!("../../assets/main.css");
        // In the middle of what the rail leaves, wherever the rail is.
        assert!(css.contains("left: calc(50% + (var(--rail-l) - var(--rail-r)) / 2 - 24px);"));
        // On a phone the bar is at the bottom, and the arrow goes above it on
        // the side of the hand, the right unless the left is chosen.
        assert!(css.contains(
            "@media (max-width: 760px) {\n  .to-top { left: auto; right: 12px; bottom: calc(max(12px, env(safe-area-inset-bottom)) + 72px); }\n  .app[data-hand=\"left\"] .to-top { left: 12px; right: auto; }"
        ));
    }

    #[test]
    fn the_arrow_to_the_top_cannot_be_reached_while_it_is_away() {
        // Hidden, it is out of the way of a click and of the Tab key.
        let css = include_str!("../../assets/main.css");
        assert!(
            css.contains("opacity: 0; visibility: hidden; transform: translateY(16px) scale(0.8);")
        );
        assert!(css.contains(".to-top.shown { opacity: 1; visibility: visible;"));
        // It comes a screen down, and the app hears only when that changes.
        assert!(WATCH_FAR.contains("const now = scrollY > innerHeight;"));
        assert!(WATCH_FAR.contains("if (now !== far) {"));
    }

    #[test]
    fn random_order_has_no_spot() {
        assert!(is_random("cat order:random"));
        assert!(!is_random("cat"));
        assert!(!is_random("order:new"));
        assert!(!is_random("order:nonsense"));
    }

    #[test]
    fn only_a_random_order_is_shuffled() {
        // Other orders never get a seed, so the server sorts them as asked.
        assert_eq!(seed("cat"), None);
        assert_eq!(seed("order:score"), None);
        let seeds: HashSet<i64> = (0..100).map(|_| new_seed()).collect();
        assert_eq!(seeds.len(), 100);
    }

    #[test]
    fn shares_in_gallery_order() {
        let selected = BTreeSet::from([2, 5, 9, 40]);
        assert_eq!(in_gallery_order(&[9, 7, 5, 2], &selected), [9, 5, 2, 40]);
        assert!(in_gallery_order(&[1], &BTreeSet::new()).is_empty());
    }

    #[test]
    fn the_menu_outside_the_gallery_leaves_out_what_needs_its_list() {
        let source = include_str!("gallery.rs");
        let (_, menu) = source.split_once("fn PostMenu(").unwrap();
        let (menu, _) = menu.split_once("\npub(super) fn PostPress(").unwrap();
        for item in ["SwipeAction::Select)", "SwipeAction::Delete)"] {
            let at = menu.find(item).unwrap();
            let before = menu.get(..at).unwrap_or_default();
            let guard = before.rfind("if listed {").unwrap();
            // Nothing else between the guard and the item.
            assert_eq!(
                before
                    .get(guard..)
                    .unwrap_or_default()
                    .matches("MenuItem")
                    .count(),
                1
            );
        }
        let (_, press) = source.split_once("pub(super) fn PostPress(").unwrap();
        let (press, _) = press.split_once("\n}\n").unwrap();
        assert!(press.contains("listed: false,"));
        assert!(
            press.contains("SwipeAction::Off | SwipeAction::Select | SwipeAction::Delete => {}")
        );
    }
}
