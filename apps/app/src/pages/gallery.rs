use std::collections::BTreeSet;

use dioxus::prelude::*;
use starbooru_core::color::{NAMED_COLORS, to_hex, wheel_color};
use starbooru_core::saved::MAX_SEARCH_NAME_CHARS;
use starbooru_core::{
    BulkEdit, MAX_BULK_POSTS, MediaKind, Order, Post, Rating, SavedSearch, TagCorrection,
    parse_query, with_color, with_exact, with_order,
};

use super::post::OPENING;
use super::share::ShareSheet;
use super::{Choice, CorrectionBanner, ErrorBanner, Select, Spinner};
use crate::Route;
use crate::cache;
use crate::icons::{Ico, Icon};
use crate::layout::{self, Place, TILE_GAP};
use crate::power;
use crate::store::{self, GalleryView, PREFS};
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
    let mut posts = use_signal(Vec::<Post>::new);
    let mut next_page = use_signal(|| 1u32);
    let mut has_more = use_signal(|| true);
    let mut loading = use_signal(|| false);
    let mut error = use_signal(|| None::<String>);
    // Bumped on every new search so late responses for an old query are dropped.
    let mut generation = use_signal(|| 0u64);
    let mut query = use_signal(|| q.clone());
    // Unknown tags the search corrected.
    let mut corrected = use_signal(Vec::<TagCorrection>::new);

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
                    posts.write().extend(found.posts);
                    has_more.set(found.has_more);
                    corrected.set(found.corrected);
                    next_page.set(page.saturating_add(1));
                }
                Err(e) if e.is_unauthorized() => store::forget_session(),
                Err(e) => {
                    has_more.set(false);
                    error.set(Some(e.to_string()));
                }
            }
        });
    };

    let mut restart = move |q: String| {
        generation += 1;
        query.set(q);
        posts.write().clear();
        corrected.write().clear();
        next_page.set(1);
        has_more.set(true);
        loading.set(false);
        error.set(None);
        load_more();
    };
    // Bumped to run the search again, in this scope (spawns from a child's
    // task would die with the child).
    let mut reload = use_signal(|| 0u64);
    use_effect(use_reactive!(|q| {
        reload();
        restart(q);
    }));

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
        reload += 1;
    };

    // The user's saved searches; `None` until loaded.
    let mut saved = use_signal(|| None::<Vec<SavedSearch>>);
    use_hook(move || {
        if let Some(client) = store::client() {
            spawn(async move {
                if let Ok(list) = client.saved_searches().await {
                    saved.set(Some(list));
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
/// spot in the gallery's layout; other grids leave it out.
#[component]
pub(super) fn Thumb(
    post: Post,
    index: usize,
    q: String,
    selected: Option<bool>,
    ontoggle: EventHandler<(usize, bool)>,
    place: Option<Place>,
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
    let class = match selected {
        Some(true) => "thumb selectable on",
        Some(false) => "thumb selectable",
        None => "thumb",
    };
    let open = move || {
        *OPENING.write() = Some(post.clone());
        let nav = navigator();
        let q = q.clone();
        if !HERO_TRANSITION {
            nav.push(Route::PostView { id, q });
            return;
        }
        spawn(async move {
            let mut hero = document::eval(HERO_START);
            let _ = hero.send(id);
            // Navigates once the old page is captured, or at once without a transition.
            let _ = hero.recv::<bool>().await;
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

/// Whether the webview can run the hero transition. WebKitGTK (the Linux
/// desktop webview) aborts its web process inside `startViewTransition`,
/// so there the viewer opens without it.
const HERO_TRANSITION: bool = !cfg!(all(feature = "desktop", target_os = "linux"));

/// Grows the clicked thumbnail into the viewer's image with a view
/// transition, where the browser has them and motion isn't reduced. Sends
/// `true` once the current page is captured (the page change must follow),
/// or `false` at once when there is no transition.
const HERO_START: &str = r#"
const id = await dioxus.recv();
const img = document.querySelector(`[data-post="${id}"] img`);
if (!document.startViewTransition || !img || matchMedia("(prefers-reduced-motion: reduce)").matches) {
  dioxus.send(false);
  return;
}
const root = document.documentElement;
img.style.viewTransitionName = "sb-hero";
const t = document.startViewTransition(() => {
  img.style.viewTransitionName = "";
  root.classList.add("sb-hero");
  dioxus.send(true);
  // The viewer ends it when its image is in place; never wait long.
  return new Promise((done) => { window.__sbHeroDone = done; setTimeout(done, 700); });
});
const end = () => { root.classList.remove("sb-hero"); window.__sbHeroDone = null; };
t.finished.then(end, end);
"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_hero_transition_in_webkitgtk() {
        // Regression: clicking a thumbnail crashed the Linux desktop app.
        let linux_desktop = cfg!(all(feature = "desktop", target_os = "linux"));
        assert_eq!(HERO_TRANSITION, !linux_desktop);
    }

    #[test]
    fn shares_in_gallery_order() {
        let selected = BTreeSet::from([2, 5, 9, 40]);
        assert_eq!(in_gallery_order(&[9, 7, 5, 2], &selected), [9, 5, 2, 40]);
        assert!(in_gallery_order(&[1], &BTreeSet::new()).is_empty());
    }
}
