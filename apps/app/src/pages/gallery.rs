use std::collections::{BTreeSet, HashSet};

use chrono::{DateTime, Utc};
use dioxus::prelude::*;
use serde::Deserialize;
use starbooru_core::color::{NAMED_COLORS, to_hex, wheel_color};
use starbooru_core::saved::MAX_SEARCH_NAME_CHARS;
use starbooru_core::{
    BulkEdit, MAX_BULK_POSTS, MediaKind, Order, Post, Rating, SavedSearch, SortKey, TagCorrection,
    parse_query, with_color, with_exact, with_order,
};

use super::post::OPENING;
use super::share::ShareSheet;
use super::{Choice, CorrectionBanner, ErrorBanner, Select, Spinner};
use crate::Route;
use crate::cache;
use crate::icons::{Ico, Icon};
use crate::layout::{self, Place, TILE_GAP};
use crate::motion;
use crate::power;
use crate::store::{self, GalleryView, PREFS, Spot};
use crate::util::{escape_attr, format_duration, rating_label, split_tag_edit};

const PAGE_SIZE: u32 = 60;
/// The colour wheel's width in pixels, as in `main.css`.
const WHEEL_SIZE: f64 = 176.0;

/// Width of the gallery's grid in pixels, kept so coming back to the page
/// lays it out right away.
static GRID_WIDTH: GlobalSignal<f64> = Signal::global(|| 0.0);

/// Reports the grid's width now and whenever it changes.
const WATCH_WIDTH: &str = r#"
const box = document.querySelector(".masonry-box");
if (box) {
  const seen = new ResizeObserver(() => {
    if (box.isConnected) dioxus.send(box.clientWidth);
    else seen.disconnect();
  });
  seen.observe(box);
}
"#;

/// Results kept while the app runs, so coming back to a search shows it as it
/// was without loading it again.
#[derive(Clone)]
struct List {
    q: String,
    owner: String,
    posts: Vec<Post>,
    next_page: u32,
    has_more: bool,
    corrected: Vec<TagCorrection>,
    loaded_at: DateTime<Utc>,
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
/// scroll position.
const MAX_RESTORE: usize = 6000;

/// `order:random` comes out different each time, so it has no spot to go back to.
fn is_random(q: &str) -> bool {
    parse_query(q).is_ok_and(|p| p.order.key == SortKey::Random)
}

/// The first page as it is now, followed by the rest of what was loaded. `None`
/// if nothing changed. With no overlap, or no further pages, the rest is gone.
fn merge_head(current: &[Post], head: Vec<Post>, has_more: bool) -> Option<Vec<Post>> {
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
    Some(head.into_iter().chain(rest).collect())
}

/// What the scroll watcher reports.
#[derive(Deserialize)]
struct Seen {
    post: Option<i64>,
    offset: f64,
    y: f64,
}

/// Reports the first tile on the screen once scrolling stops: a whole one, if
/// there is one, as it stays on the screen when the layout changes.
const WATCH_SCROLL: &str = r#"
window.__sbSpot?.stop();
let timer = 0;
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
const onscroll = () => {
  clearTimeout(timer);
  timer = setTimeout(report, 200);
};
addEventListener("scroll", onscroll, { passive: true });
window.__sbSpot = {
  stop() {
    clearTimeout(timer);
    removeEventListener("scroll", onscroll);
  },
};
"#;
const WATCH_SCROLL_OFF: &str = "window.__sbSpot?.stop(); window.__sbSpot = null;";

/// Puts a remembered tile back where it was on the screen, or failing that,
/// the scroll position. The tile may land a frame after the list does.
const JUMP_TO_SPOT: &str = r#"
const [post, offset, y] = await dioxus.recv();
const go = tries => {
  const tile = y >= 1 && post != null
    ? document.querySelector(`.masonry [data-post="${post}"]`)?.closest(".thumb")
    : null;
  if (tile) scrollBy({ top: tile.getBoundingClientRect().top - offset, behavior: "instant" });
  else if (y >= 1 && tries > 0) return requestAnimationFrame(() => go(tries - 1));
  else scrollTo({ top: y, behavior: "instant" });
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

    // Tiles below this index appeared with the list, and don't animate in.
    let mut settled = use_signal(|| posts.peek().len());
    let mut loaded_at = use_signal(Utc::now);
    // Bumped to put the page back at the remembered spot.
    let mut jump = use_signal(|| 0u64);

    let keep = move || {
        keep_list(List {
            q: query.peek().clone(),
            owner: store::owner(),
            posts: posts.peek().clone(),
            next_page: *next_page.peek(),
            has_more: *has_more.peek(),
            corrected: corrected.peek().clone(),
            loaded_at: *loaded_at.peek(),
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
            let result = client.search_posts(&q, page, PAGE_SIZE).await;
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
                        list.extend(found.posts.into_iter().filter(|p| !seen.contains(&p.id)));
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
            let mut found = Vec::<Post>::new();
            let mut seen = HashSet::new();
            let mut fixed = Vec::new();
            let mut page = 1u32;
            let mut more = true;
            let mut failure = None;
            while more && found.len() < wanted {
                let at = (page - 1) / RESTORE_PAGES + 1;
                let result = client
                    .search_posts(&spot.q, at, PAGE_SIZE * RESTORE_PAGES)
                    .await;
                if *generation.peek() != generation_at_start {
                    return;
                }
                match result {
                    Ok(results) => {
                        found.extend(results.posts.into_iter().filter(|p| seen.insert(p.id)));
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
            let Ok(found) = client.search_posts(&q, 1, PAGE_SIZE).await else {
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
            settled.set(list.posts.len());
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
        document::eval(WATCH_SCROLL_OFF);
    });
    use_effect(move || {
        if jump() == 0 {
            return;
        }
        if let Some(spot) = store::spot(&query.peek()) {
            let eval = document::eval(JUMP_TO_SPOT);
            let _ = eval.send((spot.post, spot.offset, spot.y));
        } else {
            motion::back();
        }
    });

    // Selection mode for mass edits. `anchor` is the last thumb clicked, for
    // shift-click ranges.
    let mut selecting = use_signal(|| false);
    let mut selected = use_signal(BTreeSet::<i64>::new);
    let mut anchor = use_signal(|| None::<usize>);
    let mut notice = use_signal(|| None::<String>);
    let toggle = move |(index, shift): (usize, bool)| {
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
    };
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
    let (view, chosen, gap) = {
        let prefs = PREFS.read();
        let gap = if prefs.flush_tiles { 0.0 } else { TILE_GAP };
        (prefs.gallery_view, prefs.gallery_columns, gap)
    };
    let width = GRID_WIDTH();
    let columns = layout::columns(width, gap, view, chosen);
    let tiles = layout::place(
        posts
            .read()
            .iter()
            .map(|p| layout::tile_ratio(view, p.width, p.height)),
        columns,
    );
    // A new layout moves everything, so the same tile goes back to the top.
    let mut laid_out = use_signal(|| (columns, view, gap, width));
    use_effect(use_reactive!(|columns, view, gap, width| {
        let now = (columns, view, gap, width);
        if *laid_out.peek() != now {
            laid_out.set(now);
            jump += 1;
        }
    }));

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
                        Link { class: "btn", to: Route::Upload {}, Ico { icon: Icon::Upload } {t!("nav-upload")} }
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
                },
                div {
                    class: "masonry",
                    style: "--columns: {columns}; --height: {tiles.height:.4}; --rows: {tiles.rows}",
                    for (index, (post, place)) in posts.read().iter().zip(&tiles.tiles).enumerate() {
                        Thumb {
                            key: "{post.id}",
                            post: post.clone(),
                            index,
                            q: q.clone(),
                            selected: selecting().then(|| selected.read().contains(&post.id)),
                            ontoggle: toggle,
                            place: *place,
                            still: index < settled(),
                        }
                    }
                }
            }
            if has_more() && error().is_none() {
                div {
                    class: "sentinel",
                    onvisible: move |e| {
                        if e.is_intersecting().unwrap_or(false) {
                            load_more();
                        }
                    },
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

/// Rewrites the query's `color:` term from a wheel or named swatches. The
/// wheel takes a pointer, so every part of it has a swatch close by for the
/// keyboard.
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
                    div {
                        class: "color-wheel",
                        title: t!("color-wheel"),
                        aria_hidden: "true",
                        onclick: move |e: MouseEvent| {
                            let at = e.element_coordinates();
                            let radius = WHEEL_SIZE / 2.0;
                            let rgb = wheel_color((at.x - radius) / radius, (at.y - radius) / radius);
                            pick.call(Some(to_hex(rgb)));
                        },
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
                        label { class: "muted", r#for: "view-columns", {t!("view-columns", count = columns)} }
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

/// A post in the grid. With `selected` set (selection mode), a click
/// toggles it instead of opening it. Both modes render the same elements, so
/// turning selection on doesn't load the thumbnails again. `place` is its
/// spot in the gallery's layout; other grids leave it out. `still` skips the
/// entrance, for a tile that isn't new to the user.
#[component]
pub(super) fn Thumb(
    post: Post,
    index: usize,
    q: String,
    selected: Option<bool>,
    ontoggle: EventHandler<(usize, bool)>,
    place: Option<Place>,
    #[props(default)] still: bool,
) -> Element {
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
    if let Some(p) = place {
        style.push_str(&format!(
            "; --column: {}; --top: {:.4}; --row: {}; --ratio: {:.4}",
            p.column, p.top, p.row, p.ratio
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
    let open = move || {
        *OPENING.write() = Some(post.clone());
        let nav = navigator();
        let q = q.clone();
        spawn(async move {
            motion::open(id).await;
            nav.push(Route::PostView { id, q });
        });
    };
    rsx! {
        a {
            class,
            style,
            href: "{to}",
            aria_label: label,
            role: selected.map(|_| "button"),
            aria_pressed: selected.map(|on| on.to_string()),
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
            div {
                class: "thumb-media",
                "data-post": "{id}",
                onpointerenter: move |e: PointerEvent| {
                    if video && e.pointer_type() == "mouse" {
                        hovered.set(true);
                    }
                },
                onpointerleave: move |_| {
                    if *hovered.peek() {
                        hovered.set(false);
                    }
                },
                if let Some(src) = src {
                    img { src, alt, loading: "lazy", decoding: "async" }
                } else {
                    div { class: "thumb-placeholder", Ico { icon: Icon::Play, size: 40 } }
                }
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
        let current: Vec<Post> = current.iter().copied().map(post).collect();
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
    fn random_order_has_no_spot() {
        assert!(is_random("cat order:random"));
        assert!(!is_random("cat"));
        assert!(!is_random("order:new"));
        assert!(!is_random("order:nonsense"));
    }

    #[test]
    fn shares_in_gallery_order() {
        let selected = BTreeSet::from([2, 5, 9, 40]);
        assert_eq!(in_gallery_order(&[9, 7, 5, 2], &selected), [9, 5, 2, 40]);
        assert!(in_gallery_order(&[1], &BTreeSet::new()).is_empty());
    }
}
