use std::collections::BTreeSet;

use dioxus::prelude::*;
use starbooru_core::saved::MAX_SEARCH_NAME_CHARS;
use starbooru_core::{
    BulkEdit, MAX_BULK_POSTS, MediaKind, Order, Post, Rating, SavedSearch, parse_query, with_order,
};

use super::post::OPENING;
use super::share::ShareSheet;
use super::{ErrorBanner, Spinner};
use crate::Route;
use crate::cache;
use crate::icons::{Ico, Icon};
use crate::store;
use crate::util::{escape_attr, format_duration, rating_label, split_tag_edit};

const PAGE_SIZE: u32 = 60;

/// Sort menu entries: `order:` values.
const SORTS: [&str; 10] = [
    "new", "old", "score", "updated", "size", "size_asc", "pixels", "tags", "tags_asc", "random",
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
            div { class: if selecting() { "grid selecting" } else { "grid" },
                for (index, post) in posts.read().iter().enumerate() {
                    Thumb {
                        key: "{post.id}",
                        post: post.clone(),
                        index,
                        q: q.clone(),
                        selected: selecting().then(|| selected.read().contains(&post.id)),
                        ontoggle: toggle,
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
            select {
                class: "bulk-rating",
                aria_label: t!("rating"),
                onchange: move |e| rating.set(Rating::parse(&e.value())),
                option { value: "", selected: rating().is_none(), {t!("bulk-keep-rating")} }
                for r in Rating::ALL {
                    option { key: "{r.as_str()}", value: r.as_str(), selected: rating() == Some(r), {rating_label(r)} }
                }
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
        label { class: "sort",
            span { class: "muted", {t!("sort")} }
            select {
                aria_label: t!("sort-by"),
                onchange: move |e| {
                    let order = Order::parse(&e.value());
                    navigator().push(Route::Gallery { q: with_order(&q, order) });
                },
                for value in SORTS {
                    option {
                        key: "{value}",
                        value,
                        selected: Order::parse(value) == Some(current),
                        {sort_label(value)}
                    }
                }
            }
        }
    }
}

/// A post in the grid. With `selected` set (selection mode), a click
/// toggles it instead of opening it.
#[component]
pub(super) fn Thumb(
    post: Post,
    index: usize,
    q: String,
    selected: Option<bool>,
    ontoggle: EventHandler<(usize, bool)>,
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
    let alt = post.tags.join(" ");
    let duration = post.duration_ms.and_then(format_duration);
    let media = rsx! {
        div {
            class: "thumb-media",
            "data-post": "{post.id}",
            onpointerenter: move |e: PointerEvent| {
                if video && e.pointer_type() == "mouse" {
                    hovered.set(true);
                }
            },
            onpointerleave: move |_| hovered.set(false),
            if let Some(src) = src {
                img { src, alt: alt.clone(), loading: "lazy", decoding: "async" }
            } else {
                div { class: "thumb-placeholder", Ico { icon: Icon::Play, size: 40 } }
            }
            if hovered() && let Some(src) = preview_src {
                // Parsed HTML, because only a parsed `muted` attribute
                // mutes the video, and browsers autoplay muted videos only.
                div {
                    class: "preview",
                    aria_hidden: "true",
                    dangerous_inner_html: "<video src=\"{escape_attr(&src)}\" muted autoplay loop playsinline preload=\"auto\"></video>",
                }
            }
        }
        match post.kind {
            MediaKind::Video => rsx! {
                span { class: "badge",
                    Ico { icon: Icon::Play, size: 16 }
                    if let Some(d) = duration { "{d}" }
                }
            },
            MediaKind::Animation => rsx! { span { class: "badge", "GIF" } },
            MediaKind::Image => rsx! {},
        }
    };
    if let Some(on) = selected {
        return rsx! {
            button {
                class: if on { "thumb selectable on" } else { "thumb selectable" },
                style: "--delay: {delay}ms",
                r#type: "button",
                aria_pressed: on,
                aria_label: if alt.is_empty() { t!("post-alt", id = post.id) } else { alt.clone() },
                onclick: move |e: MouseEvent| ontoggle.call((index, e.modifiers().shift())),
                {media}
                span { class: "check-mark", aria_hidden: "true", Ico { icon: Icon::Check, size: 20 } }
            }
        };
    }
    let nav = navigator();
    let id = post.id;
    let open = {
        let q = q.clone();
        move |_| {
            *OPENING.write() = Some(post.clone());
            let q = q.clone();
            spawn(async move {
                let mut hero = document::eval(HERO_START);
                let _ = hero.send(id);
                // Navigates once the old page is captured, or at once without a transition.
                let _ = hero.recv::<bool>().await;
                nav.push(Route::PostView { id, q });
            });
        }
    };
    rsx! {
        Link {
            class: "thumb",
            style: "--delay: {delay}ms",
            to: Route::PostView { id, q },
            onclick: open,
            onclick_only: true,
            {media}
        }
    }
}

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
    fn shares_in_gallery_order() {
        let selected = BTreeSet::from([2, 5, 9, 40]);
        assert_eq!(in_gallery_order(&[9, 7, 5, 2], &selected), [9, 5, 2, 40]);
        assert!(in_gallery_order(&[1], &BTreeSet::new()).is_empty());
    }
}
