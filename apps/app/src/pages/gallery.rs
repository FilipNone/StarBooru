use dioxus::prelude::*;
use starbooru_core::{MediaKind, Order, Post, parse_query, with_order};

use super::{ErrorBanner, Spinner};
use crate::Route;
use crate::icons::{Ico, Icon};
use crate::store;
use crate::util::{escape_attr, format_duration};

const PAGE_SIZE: u32 = 60;

/// Sort menu entries: label and `order:` value.
const SORTS: [(&str, &str); 10] = [
    ("Newest", "new"),
    ("Oldest", "old"),
    ("Top score", "score"),
    ("Recently edited", "updated"),
    ("Largest file", "size"),
    ("Smallest file", "size_asc"),
    ("Highest resolution", "pixels"),
    ("Most tags", "tags"),
    ("Fewest tags", "tags_asc"),
    ("Random", "random"),
];

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

    use_effect(use_reactive!(|q| {
        generation += 1;
        query.set(q);
        posts.write().clear();
        next_page.set(1);
        has_more.set(true);
        loading.set(false);
        error.set(None);
        load_more();
    }));

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
                    if title.is_empty() { "Gallery" } else { "{title}" }
                }
                if let Some(name) = single_tag {
                    Link { class: "icon-btn tag-info", title: "About this tag", aria_label: "About this tag",
                        to: Route::TagPage { name },
                        Ico { icon: Icon::Label }
                    }
                }
                SortMenu { q: q.clone() }
            }
            if let Some(message) = error() {
                ErrorBanner { message, onretry: retry }
            }
            if is_empty {
                div { class: "card empty",
                    Ico { icon: Icon::Gallery, size: 48 }
                    if q.is_empty() {
                        h2 { "No posts yet" }
                        p { class: "muted", "Upload images or videos to start your collection." }
                        Link { class: "btn", to: Route::Upload {}, Ico { icon: Icon::Upload } "Upload" }
                    } else {
                        h2 { "No matches" }
                        p { class: "muted", "Nothing matches this search. Try fewer tags." }
                    }
                }
            }
            div { class: "grid",
                for (index, post) in posts.read().iter().enumerate() {
                    Thumb { key: "{post.id}", post: post.clone(), index, q: q.clone() }
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
                        button { class: "btn tonal", onclick: move |_| load_more(), "Load more" }
                    }
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
            span { class: "muted", "Sort" }
            select {
                aria_label: "Sort by",
                onchange: move |e| {
                    let order = Order::parse(&e.value());
                    navigator().push(Route::Gallery { q: with_order(&q, order) });
                },
                for (label, value) in SORTS {
                    option {
                        key: "{value}",
                        value,
                        selected: Order::parse(value) == Some(current),
                        "{label}"
                    }
                }
            }
        }
    }
}

#[component]
fn Thumb(post: Post, index: usize, q: String) -> Element {
    let client = store::client();
    let src = post
        .thumb_url
        .as_deref()
        .and_then(|u| client.as_ref().map(|c| c.absolute(u)));
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
    rsx! {
        Link {
            class: "thumb",
            style: "--delay: {delay}ms",
            to: Route::PostView { id: post.id, q },
            div {
                class: "thumb-media",
                onpointerenter: move |e: PointerEvent| {
                    if video && e.pointer_type() == "mouse" {
                        hovered.set(true);
                    }
                },
                onpointerleave: move |_| hovered.set(false),
                if let Some(src) = src {
                    img { src, alt, loading: "lazy", decoding: "async" }
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
        }
    }
}
