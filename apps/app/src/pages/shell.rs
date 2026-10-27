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
            nav { class: "nav-top", aria_label: t!("nav-main"),
                NavItems { route: route.clone() }
            }
        }
        main { class: "content", Outlet::<Route> {} }
        nav { class: "nav-bottom glass", aria_label: t!("nav-main"),
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
            t!("nav-gallery"),
        ),
        (
            Route::Explore { q: String::new() },
            Icon::Explore,
            t!("nav-explore"),
        ),
        (Route::Tags {}, Icon::Label, t!("nav-tags")),
        (Route::Upload {}, Icon::Upload, t!("nav-upload")),
        (Route::Settings {}, Icon::Tune, t!("nav-settings")),
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
    // The highlighted suggestion, moved with the arrow keys.
    let mut active = use_signal(|| None::<usize>);
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
    let open = focused() && !list.is_empty();
    let count = list.len();
    let names: Vec<String> = list.iter().map(|t| t.name.clone()).collect();
    let selected = active().filter(|i| *i < count);
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
                placeholder: t!("search-placeholder"),
                aria_label: t!("search-label"),
                autocomplete: "off",
                spellcheck: "false",
                value: "{text}",
                role: "combobox",
                aria_autocomplete: "list",
                aria_controls: "search-suggestions",
                aria_expanded: if open { "true" } else { "false" },
                aria_activedescendant: selected.map(|i| format!("suggestion-{i}")).unwrap_or_default(),
                oninput: move |e| {
                    text.set(e.value());
                    active.set(None);
                    focused.set(true);
                },
                onfocus: move |_| focused.set(true),
                onblur: move |_| {
                    focused.set(false);
                    active.set(None);
                },
                onkeydown: move |e: KeyboardEvent| {
                    if !open {
                        return;
                    }
                    match e.key() {
                        Key::ArrowDown => {
                            e.prevent_default();
                            active.set(Some(selected.map_or(0, |i| (i + 1) % count)));
                        }
                        Key::ArrowUp => {
                            e.prevent_default();
                            active.set(Some(selected.map_or(count - 1, |i| (i + count - 1) % count)));
                        }
                        Key::Enter | Key::Tab => {
                            if let Some(name) = selected.and_then(|i| names.get(i)) {
                                e.prevent_default();
                                complete(name.clone());
                                active.set(None);
                            }
                        }
                        Key::Escape => {
                            e.prevent_default();
                            focused.set(false);
                            active.set(None);
                        }
                        _ => {}
                    }
                },
            }
            if open {
                ul { class: "suggestions", role: "listbox", id: "search-suggestions",
                    for (i, tag) in list.into_iter().enumerate() {
                        li {
                            key: "{tag.id}",
                            id: "suggestion-{i}",
                            role: "option",
                            class: if selected == Some(i) { "active" } else { "" },
                            aria_selected: if selected == Some(i) { "true" } else { "false" },
                            onmouseenter: move |_| active.set(Some(i)),
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
