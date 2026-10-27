//! App frame: top bar with search, navigation (top on wide screens, bottom
//! bar on phones). Shows the sign-in screen when there is no session.

use dioxus::prelude::*;
use starbooru_core::Tag;

use super::login::Login;
use crate::Route;
use crate::icons::{Ico, Icon};
use crate::store::{self, SESSION};

#[component]
pub fn Shell() -> Element {
    if SESSION.read().is_none() {
        return rsx! { Login {} };
    }
    let route = use_route::<Route>();
    rsx! {
        header { class: "topbar glass",
            Link { class: "brand", to: Route::Gallery { q: String::new() },
                Ico { icon: Icon::Star }
                span { "StarBooru" }
            }
            SearchBox { route: route.clone() }
            nav { class: "nav-top", aria_label: "Main",
                NavItems { route: route.clone() }
            }
        }
        main { class: "content", Outlet::<Route> {} }
        nav { class: "nav-bottom glass", aria_label: "Main",
            NavItems { route }
        }
    }
}

#[component]
fn NavItems(route: Route) -> Element {
    let items = [
        (
            Route::Gallery { q: String::new() },
            Icon::Gallery,
            "Gallery",
        ),
        (Route::Tags {}, Icon::Label, "Tags"),
        (Route::Upload {}, Icon::Upload, "Upload"),
        (Route::Settings {}, Icon::Tune, "Settings"),
    ];
    rsx! {
        for (to, icon, label) in items {
            Link {
                key: "{label}",
                class: if std::mem::discriminant(&to) == std::mem::discriminant(&route) { "nav-item active" } else { "nav-item" },
                to,
                span { class: "nav-pill", Ico { icon } }
                span { class: "nav-label", "{label}" }
            }
        }
    }
}

/// The query the search box shows for a route: the gallery's own, else none.
fn route_query(route: &Route) -> String {
    match route {
        Route::Gallery { q } => q.clone(),
        _ => String::new(),
    }
}

/// Tag search with autocomplete on the word being typed.
#[component]
fn SearchBox(route: Route) -> Element {
    let mut text = use_signal(|| route_query(&route));
    let mut focused = use_signal(|| false);
    // Every navigation resets the box, so a half-typed query doesn't linger.
    use_effect(use_reactive!(|route| text.set(route_query(&route))));

    let suggestions = use_resource(move || async move {
        let value = text();
        let word = value
            .split_whitespace()
            .last()
            .unwrap_or_default()
            .to_owned();
        let prefix = word.trim_start_matches('-');
        if prefix.is_empty() || value.ends_with(' ') || prefix.contains(':') {
            return Vec::<Tag>::new();
        }
        let Some(client) = store::client() else {
            return Vec::new();
        };
        client.tags(prefix, 8).await.unwrap_or_default()
    });

    let mut search = move |q: String| {
        focused.set(false);
        navigator().push(Route::Gallery {
            q: q.trim().to_owned(),
        });
    };

    let mut complete = move |tag: String| {
        let value = text();
        let mut words: Vec<&str> = value.split_whitespace().collect();
        let negated = words.last().is_some_and(|w| w.starts_with('-'));
        words.pop();
        let mut next = words.join(" ");
        if !next.is_empty() {
            next.push(' ');
        }
        if negated {
            next.push('-');
        }
        next.push_str(&tag);
        next.push(' ');
        text.set(next);
    };

    let list = suggestions.read().clone().unwrap_or_default();
    rsx! {
        form {
            class: "search",
            role: "search",
            onsubmit: move |e: FormEvent| {
                e.prevent_default();
                search(text());
            },
            Ico { icon: Icon::Search }
            input {
                r#type: "search",
                placeholder: "Search tags, e.g. blue_sky -cloud rating:g",
                aria_label: "Search tags",
                autocomplete: "off",
                spellcheck: "false",
                value: "{text}",
                oninput: move |e| text.set(e.value()),
                onfocus: move |_| focused.set(true),
                onblur: move |_| focused.set(false),
            }
            if focused() && !list.is_empty() {
                ul { class: "suggestions", role: "listbox",
                    for tag in list {
                        li {
                            key: "{tag.id}",
                            role: "option",
                            // mousedown fires before the input's blur
                            onmousedown: move |e| {
                                e.prevent_default();
                                complete(tag.name.clone());
                            },
                            span { class: "tag-name", "{tag.name}" }
                            span { class: "count", "{tag.post_count}" }
                        }
                    }
                }
            }
        }
    }
}
