//! Explore: trending posts and tags, posts added on this day in earlier
//! years, and random picks. `q` narrows every section.

use chrono::Datelike;
use dioxus::prelude::*;
use starbooru_core::{DEFAULT_EXPLORE_DAYS, Order, Post, with_exact, with_order};

use super::gallery::Thumb;
use super::{CorrectionBanner, ErrorBanner, Spinner};
use crate::Route;
use crate::icons::{Ico, Icon};
use crate::store;

/// Window choices in days.
const WINDOWS: [u32; 4] = [1, 7, 30, 365];

fn window_label(days: u32) -> String {
    match days {
        1 => t!("explore-today"),
        7 => t!("explore-week"),
        30 => t!("explore-month"),
        _ => t!("explore-year"),
    }
}
const RANDOM_COUNT: u32 = 12;

/// "today", "this week", … for a window of `days`.
fn window_phrase(days: u32) -> String {
    match days {
        1 => t!("explore-phrase-today"),
        7 => t!("explore-phrase-week"),
        30 => t!("explore-phrase-month"),
        365 => t!("explore-phrase-year"),
        n => t!("explore-phrase-days", count = n),
    }
}

#[component]
pub fn Explore(q: String) -> Element {
    let mut days = use_signal(|| DEFAULT_EXPLORE_DAYS);
    let mut filter = use_signal(|| q.clone());
    // Replaces the random picks after a shuffle, until the next load.
    let mut shuffled = use_signal(|| None::<Vec<Post>>);
    let mut shuffling = use_signal(|| false);

    let data = use_resource(use_reactive!(|q| async move {
        let days = days();
        shuffled.set(None);
        let client = store::client().ok_or_else(|| t!("not-signed-in"))?;
        match client.explore(days, &q).await {
            Ok(found) => Ok(found),
            Err(e) if e.is_unauthorized() => {
                store::forget_session();
                Err(e.to_string())
            }
            Err(e) => Err(e.to_string()),
        }
    }));
    use_effect(use_reactive!(|q| filter.set(q)));

    let random_query = with_order(&q, Order::parse("random"));
    let shuffle_query = random_query.clone();
    let shuffle = move |_| {
        let Some(client) = store::client() else {
            return;
        };
        let query = shuffle_query.clone();
        shuffling.set(true);
        spawn(async move {
            if let Ok(page) = client.search_posts(&query, 1, RANDOM_COUNT).await {
                shuffled.set(Some(page.posts));
            }
            shuffling.set(false);
        });
    };

    let phrase = window_phrase(days());
    let body = match &*data.read() {
        None => rsx! { div { class: "sentinel", Spinner {} } },
        Some(Err(message)) => rsx! {
            ErrorBanner { message: message.clone(), onretry: move |()| data.clone().restart() }
        },
        Some(Ok(found)) => {
            let random = shuffled().unwrap_or_else(|| found.random.clone());
            let corrected = found.corrected.clone();
            let exact = Route::Explore { q: with_exact(&q) };
            if random.is_empty() {
                return rsx! {
                    div { class: "page explore",
                        h1 { class: "page-title", {t!("nav-explore")} }
                        if !corrected.is_empty() {
                            CorrectionBanner { corrected, exact }
                        }
                        div { class: "card empty",
                            Ico { icon: Icon::Explore, size: 48 }
                            if q.trim().is_empty() {
                                h2 { {t!("explore-nothing")} }
                                p { class: "muted", {t!("explore-nothing-body")} }
                                Link { class: "btn", to: Route::Upload {}, Ico { icon: Icon::Upload } {t!("nav-upload")} }
                            } else {
                                h2 { {t!("no-matches")} }
                                p { class: "muted", {t!("nothing-matches", query = q.clone())} }
                                Link { class: "btn tonal", to: Route::Explore { q: String::new() }, {t!("explore-everything")} }
                            }
                        }
                    }
                };
            }
            let trending = found.trending.clone();
            let tags = found.tags.clone();
            let on_this_day = found.on_this_day.clone();
            rsx! {
                if !corrected.is_empty() {
                    CorrectionBanner { corrected, exact }
                }
                section { class: "explore-section", aria_labelledby: "trending",
                    div { class: "section-head",
                        Ico { icon: Icon::TrendingUp }
                        h2 { id: "trending", {t!("explore-trending")} }
                    }
                    if trending.is_empty() {
                        p { class: "muted", {t!("explore-no-upvotes", when = phrase.clone())} }
                    } else {
                        div { class: "grid",
                            for (index, post) in trending.into_iter().enumerate() {
                                Thumb { key: "{post.id}", post, index, q: String::new(), ontoggle: move |_| {} }
                            }
                        }
                    }
                }
                if !tags.is_empty() {
                    section { class: "explore-section", aria_labelledby: "popular-tags",
                        div { class: "section-head",
                            Ico { icon: Icon::Label }
                            h2 { id: "popular-tags", {t!("explore-popular-tags")} }
                            span { class: "muted", {t!("explore-tags-note", when = phrase.clone())} }
                        }
                        div { class: "chips",
                            for trend in tags {
                                Link {
                                    key: "{trend.tag.id}",
                                    class: "chip trend",
                                    to: Route::Gallery { q: format!("{} {}", q.trim(), trend.tag.name).trim().to_owned() },
                                    "{trend.tag.name}"
                                    span { class: "trend-count", title: t!("explore-added", when = phrase.clone()), "+{trend.recent}" }
                                }
                            }
                        }
                    }
                }
                if !on_this_day.is_empty() {
                    section { class: "explore-section", aria_labelledby: "on-this-day",
                        div { class: "section-head",
                            Ico { icon: Icon::History }
                            h2 { id: "on-this-day", {t!("explore-on-this-day")} }
                        }
                        div { class: "grid",
                            for (index, post) in on_this_day.into_iter().enumerate() {
                                div { key: "{post.id}", class: "memory",
                                    span { class: "memory-year", "{post.created_at.year()}" }
                                    Thumb { post, index, q: String::new(), ontoggle: move |_| {} }
                                }
                            }
                        }
                    }
                }
                section { class: "explore-section", aria_labelledby: "random-picks",
                    div { class: "section-head",
                        Ico { icon: Icon::Shuffle }
                        h2 { id: "random-picks", {t!("explore-random")} }
                        button {
                            class: "btn text small",
                            disabled: shuffling(),
                            onclick: shuffle,
                            {t!("explore-shuffle")}
                        }
                    }
                    div { class: "grid",
                        for (index, post) in random.into_iter().enumerate() {
                            Thumb { key: "{post.id}", post, index, q: random_query.clone(), ontoggle: move |_| {} }
                        }
                    }
                }
            }
        }
    };

    rsx! {
        div { class: "page explore",
            div { class: "page-head",
                h1 { class: "page-title", {t!("nav-explore")} }
                div { class: "segmented", role: "radiogroup", aria_label: t!("explore-window"),
                    for value in WINDOWS {
                        button {
                            key: "{value}",
                            r#type: "button",
                            role: "radio",
                            aria_checked: if days() == value { "true" } else { "false" },
                            class: if days() == value { "seg on" } else { "seg" },
                            onclick: move |_| days.set(value),
                            {window_label(value)}
                        }
                    }
                }
                form {
                    class: "explore-filter",
                    role: "search",
                    onsubmit: move |e| {
                        e.prevent_default();
                        let q = filter.peek().split_whitespace().collect::<Vec<_>>().join(" ");
                        navigator().push(Route::Explore { q });
                    },
                    Ico { icon: Icon::Search, size: 20 }
                    input {
                        r#type: "search",
                        placeholder: t!("explore-filter-placeholder"),
                        aria_label: t!("explore-filter-label"),
                        autocapitalize: "none",
                        spellcheck: false,
                        value: "{filter}",
                        oninput: move |e| filter.set(e.value()),
                    }
                }
            }
            {body}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn phrases_windows() {
        assert_eq!(window_phrase(1), "today");
        assert_eq!(window_phrase(7), "this week");
        assert_eq!(window_phrase(365), "this year");
        assert_eq!(window_phrase(14), "in the last 14 days");
    }
}
