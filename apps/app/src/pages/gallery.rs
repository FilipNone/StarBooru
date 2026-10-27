use dioxus::prelude::*;
use starbooru_core::{MediaKind, Post};

use super::{ErrorBanner, Spinner};
use crate::Route;
use crate::icons::{Ico, Icon};
use crate::store;

const PAGE_SIZE: u32 = 60;

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
    let is_empty = posts.read().is_empty() && !loading() && error().is_none() && !has_more();

    rsx! {
        div { class: "page gallery",
            if !q.is_empty() {
                h1 { class: "page-title", "{q}" }
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
                    Thumb { key: "{post.id}", post: post.clone(), index }
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

#[component]
fn Thumb(post: Post, index: usize) -> Element {
    let client = store::client();
    let src = post
        .thumb_url
        .as_deref()
        .and_then(|u| client.as_ref().map(|c| c.absolute(u)));
    // Staggered entrance within a page of results.
    let delay = (index % PAGE_SIZE as usize) * 16;
    let alt = post.tags.join(" ");
    rsx! {
        Link {
            class: "thumb",
            style: "--delay: {delay}ms",
            to: Route::PostView { id: post.id },
            if let Some(src) = src {
                img { src, alt, loading: "lazy", decoding: "async" }
            } else {
                div { class: "thumb-placeholder", Ico { icon: Icon::Play, size: 40 } }
            }
            match post.kind {
                MediaKind::Video => rsx! { span { class: "badge", Ico { icon: Icon::Play, size: 16 } } },
                MediaKind::Animation => rsx! { span { class: "badge", "GIF" } },
                MediaKind::Image => rsx! {},
            }
        }
    }
}
