//! Explore: trending posts and tags, posts added on this day in earlier
//! years, and random picks. `q` narrows every section.

use chrono::Datelike;
use dioxus::prelude::*;
use starbooru_core::{DEFAULT_EXPLORE_DAYS, Order, Post, with_order};

use super::gallery::Thumb;
use super::{ErrorBanner, Spinner};
use crate::Route;
use crate::icons::{Ico, Icon};
use crate::store;

/// Window choices: days and label.
const WINDOWS: [(u32, &str); 4] = [(1, "Today"), (7, "Week"), (30, "Month"), (365, "Year")];
const RANDOM_COUNT: u32 = 12;

/// "today", "this week", … for a window of `days`.
fn window_phrase(days: u32) -> String {
    match days {
        1 => "today".to_owned(),
        7 => "this week".to_owned(),
        30 => "this month".to_owned(),
        365 => "this year".to_owned(),
        n => format!("in the last {n} days"),
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
        let client = store::client().ok_or_else(|| "Not signed in".to_owned())?;
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
            if random.is_empty() {
                return rsx! {
                    div { class: "page explore",
                        h1 { class: "page-title", "Explore" }
                        div { class: "card empty",
                            Ico { icon: Icon::Explore, size: 48 }
                            if q.trim().is_empty() {
                                h2 { "Nothing to explore yet" }
                                p { class: "muted", "Upload images or videos, and the best of them show up here." }
                                Link { class: "btn", to: Route::Upload {}, Ico { icon: Icon::Upload } "Upload" }
                            } else {
                                h2 { "No matches" }
                                p { class: "muted", "Nothing matches “{q}”." }
                                Link { class: "btn tonal", to: Route::Explore { q: String::new() }, "Explore everything" }
                            }
                        }
                    }
                };
            }
            let trending = found.trending.clone();
            let tags = found.tags.clone();
            let on_this_day = found.on_this_day.clone();
            rsx! {
                section { class: "explore-section", aria_labelledby: "trending",
                    div { class: "section-head",
                        Ico { icon: Icon::TrendingUp }
                        h2 { id: "trending", "Trending" }
                    }
                    if trending.is_empty() {
                        p { class: "muted", "No upvotes {phrase}. Vote on posts you like and they show up here." }
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
                            h2 { id: "popular-tags", "Popular tags" }
                            span { class: "muted", "on posts added {phrase}" }
                        }
                        div { class: "chips",
                            for trend in tags {
                                Link {
                                    key: "{trend.tag.id}",
                                    class: "chip trend",
                                    to: Route::Gallery { q: format!("{} {}", q.trim(), trend.tag.name).trim().to_owned() },
                                    "{trend.tag.name}"
                                    span { class: "trend-count", title: "Posts added {phrase}", "+{trend.recent}" }
                                }
                            }
                        }
                    }
                }
                if !on_this_day.is_empty() {
                    section { class: "explore-section", aria_labelledby: "on-this-day",
                        div { class: "section-head",
                            Ico { icon: Icon::History }
                            h2 { id: "on-this-day", "On this day" }
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
                        h2 { id: "random-picks", "Random picks" }
                        button {
                            class: "btn text small",
                            disabled: shuffling(),
                            onclick: shuffle,
                            "Shuffle"
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
                h1 { class: "page-title", "Explore" }
                div { class: "segmented", role: "radiogroup", aria_label: "Trending over",
                    for (value, label) in WINDOWS {
                        button {
                            key: "{value}",
                            r#type: "button",
                            role: "radio",
                            aria_checked: if days() == value { "true" } else { "false" },
                            class: if days() == value { "seg on" } else { "seg" },
                            onclick: move |_| days.set(value),
                            "{label}"
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
                        placeholder: "Narrow down, e.g. -rating:e",
                        aria_label: "Narrow down with a search",
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
