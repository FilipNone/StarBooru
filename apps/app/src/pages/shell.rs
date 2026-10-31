//! App frame: top bar with search, navigation (top on wide screens, bottom
//! bar on phones). Shows the sign-in screen when there is no session.

use dioxus::prelude::*;
use starbooru_core::Tag;

use super::login::Login;
use crate::Route;
use crate::activity::{self, Activity};
use crate::icons::{Ico, Icon};
use crate::store::{self, SESSION};

#[component]
pub fn Shell() -> Element {
    if SESSION.read().is_none() {
        return rsx! { Login {} };
    }
    let route = use_route::<Route>();
    // Opening the app carries on from the last search.
    use_hook(|| {
        let last = store::last_search();
        if !last.is_empty() && matches!(&route, Route::Gallery { q } if q.is_empty()) {
            navigator().replace(Route::Gallery { q: last });
        }
    });
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
        Snackbar {}
        Activity {}
    }
}

/// The message the snackbar shows, numbered so a new one restarts its timer.
static NOTICE: GlobalSignal<Option<(u64, String)>> = Signal::global(|| None);

/// Shows a short message at the bottom of the screen for a few seconds,
/// for things that finish after the click (a download, say).
pub fn notify(message: String) {
    let n = NOTICE.peek().as_ref().map_or(0, |(n, _)| n.wrapping_add(1));
    *NOTICE.write() = Some((n, message));
}

/// How long a notice stays, in milliseconds.
const NOTICE_MS: u32 = 6000;

#[component]
fn Snackbar() -> Element {
    use_effect(|| {
        let Some((n, _)) = *NOTICE.read() else {
            return;
        };
        spawn(async move {
            // A timer that works the same in the browser and the webviews.
            let _ = document::eval(&format!(
                "await new Promise((r) => setTimeout(r, {NOTICE_MS})); return true;"
            ))
            .join::<bool>()
            .await;
            if NOTICE.peek().as_ref().is_some_and(|(m, _)| *m == n) {
                *NOTICE.write() = None;
            }
        });
    });
    let notice = NOTICE.read().clone();
    rsx! {
        div { class: "snackbar-slot", role: "status", aria_live: "polite",
            if let Some((n, message)) = notice {
                div { key: "{n}", class: "snackbar",
                    span { class: "break", "{message}" }
                    button {
                        class: "icon-btn",
                        aria_label: t!("notice-close"),
                        title: t!("notice-close"),
                        onclick: move |_| *NOTICE.write() = None,
                        Ico { icon: Icon::Close }
                    }
                }
            }
        }
    }
}

#[component]
fn NavItems(route: Route) -> Element {
    let items = [
        (
            Route::Gallery {
                q: store::last_search(),
            },
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
    // Uploads and imports still going, shown on the Upload item.
    let pending = activity::pending();
    let note = t!("activity-pending", count = pending);
    rsx! {
        for (to, icon, label) in items.into_iter().filter(|(to, ..)| !matches!(to, Route::Upload {}) || store::can_upload()) {
            NavLink {
                key: "{label}",
                active: std::mem::discriminant(&to) == std::mem::discriminant(&route),
                badge: if matches!(to, Route::Upload {}) { pending } else { 0 },
                note: note.clone(),
                to,
                icon,
                label,
            }
        }
    }
}

/// `badge` counts work still going on that page; `note` says so in words.
#[component]
fn NavLink(
    to: Route,
    icon: Icon,
    label: String,
    active: bool,
    badge: usize,
    note: String,
) -> Element {
    // The upload sheet comes out of this tab (`.nav-upload`).
    let upload = if matches!(to, Route::Upload {}) {
        " nav-upload"
    } else {
        ""
    };
    let on = if active { " active" } else { "" };
    rsx! {
        Link {
            class: "nav-item{upload}{on}",
            title: if badge > 0 { note.clone() } else { String::new() },
            to,
            span { class: "nav-pill",
                Ico { icon }
                if badge > 0 {
                    span { class: "nav-badge", aria_hidden: "true", "{badge.min(99)}" }
                }
            }
            span { class: "nav-label", "{label}" }
            if badge > 0 {
                span { class: "visually-hidden", "{note}" }
            }
        }
    }
}

/// The query the search box shows for a route: the one in the route, else the
/// last search, which is where the Gallery tab goes.
fn route_query(route: &Route, last: &str) -> String {
    match route {
        Route::Gallery { q } | Route::PostView { q, .. } => q.clone(),
        _ => last.to_owned(),
    }
}

/// The word being typed, split into what leads it (`-`, `{`) and the tag.
fn split_word(word: &str) -> (&str, &str) {
    let tag = word.trim_start_matches(['-', '{']);
    (word.strip_suffix(tag).unwrap_or_default(), tag)
}

/// Tag search with autocomplete on the word being typed.
#[component]
fn SearchBox(route: Route) -> Element {
    let mut text = use_signal(|| route_query(&route, &store::last_search()));
    let mut focused = use_signal(|| false);
    // The highlighted suggestion, moved with the arrow keys.
    let mut active = use_signal(|| None::<usize>);
    // Every navigation resets the box, so a half-typed query doesn't linger.
    use_effect(use_reactive!(
        |route| text.set(route_query(&route, &store::last_search()))
    ));

    let suggestions = use_resource(move || async move {
        let value = text();
        let word = value
            .split_whitespace()
            .last()
            .unwrap_or_default()
            .to_owned();
        let (_, prefix) = split_word(&word);
        if prefix.is_empty()
            || value.ends_with(' ')
            || prefix.contains(':')
            || prefix.ends_with('}')
        {
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
        let lead = words.pop().map_or("", |w| split_word(w).0);
        let mut next = words.join(" ");
        if !next.is_empty() {
            next.push(' ');
        }
        next.push_str(lead);
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

#[cfg(test)]
mod tests {
    use super::{route_query, split_word};
    use crate::Route;

    #[test]
    fn the_box_shows_the_routes_search_or_else_the_last() {
        let gallery = Route::Gallery { q: "cat".into() };
        assert_eq!(route_query(&gallery, "dog"), "cat");
        let everything = Route::Gallery { q: String::new() };
        assert_eq!(route_query(&everything, "dog"), "");
        let post = Route::PostView {
            id: 1,
            q: "cat".into(),
        };
        assert_eq!(route_query(&post, "dog"), "cat");
        assert_eq!(route_query(&Route::Settings {}, "dog"), "dog");
    }

    #[test]
    fn words_split_from_what_leads_them() {
        assert_eq!(split_word("boots"), ("", "boots"));
        assert_eq!(split_word("-boots"), ("-", "boots"));
        assert_eq!(split_word("{boots"), ("{", "boots"));
        assert_eq!(split_word("-{boots"), ("-{", "boots"));
        assert_eq!(split_word("-{"), ("-{", ""));
        assert_eq!(split_word(""), ("", ""));
    }
}
