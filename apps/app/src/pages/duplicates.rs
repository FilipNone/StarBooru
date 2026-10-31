//! Duplicate finder: groups of posts that look alike. Keep the best copy
//! (optionally taking the others' tags) and send the rest to the recycle
//! bin, or mark a group as not duplicates.

use dioxus::prelude::*;
use starbooru_core::{DEFAULT_DUPLICATE_DISTANCE, DuplicateGroup, Post, ResolveDuplicates};

use super::gallery::forget_lists;
use super::{ErrorBanner, Spinner};
use crate::Route;
use crate::cache;
use crate::icons::{Ico, Icon};
use crate::store;
use crate::util::{human_size, posts_count};

/// Matching levels: the largest distance in bits.
const LEVELS: [u32; 3] = [2, DEFAULT_DUPLICATE_DISTANCE, 10];

fn level_label(distance: u32) -> String {
    if distance <= 2 {
        t!("dup-strict")
    } else if distance <= DEFAULT_DUPLICATE_DISTANCE {
        t!("dup-normal")
    } else {
        t!("dup-loose")
    }
}
const PAGE: u32 = 30;
const MAX_LIMIT: u32 = 100;

#[component]
pub fn Duplicates() -> Element {
    let mut distance = use_signal(|| DEFAULT_DUPLICATE_DISTANCE);
    let mut limit = use_signal(|| PAGE);
    let mut groups = use_signal(Vec::<DuplicateGroup>::new);
    let mut total = use_signal(|| 0u64);
    let mut unhashed = use_signal(|| 0i64);
    let mut loading = use_signal(|| true);
    let mut error = use_signal(|| None::<String>);
    let mut version = use_signal(|| 0u32);

    use_effect(move || {
        let (d, l) = (distance(), limit());
        let _ = version();
        let Some(client) = store::client() else {
            return;
        };
        loading.set(true);
        error.set(None);
        spawn(async move {
            match client.duplicates(d, l).await {
                Ok(found) => {
                    total.set(found.total);
                    unhashed.set(found.unhashed);
                    groups.set(found.groups);
                }
                Err(e) if e.is_unauthorized() => store::forget_session(),
                Err(e) => error.set(Some(e.to_string())),
            }
            loading.set(false);
        });
    });

    // A resolved or dismissed group leaves the list and the count.
    let done = move |key: i64| {
        groups.with_mut(|g| g.retain(|group| first_id(group) != Some(key)));
        total.with_mut(|t| *t = t.saturating_sub(1));
    };

    let shown = groups.read().len();
    rsx! {
        div { class: "page",
            div { class: "page-head",
                h1 { class: "page-title", {t!("dup-title")} }
                div { class: "segmented", role: "radiogroup", aria_label: t!("dup-matching"),
                    for d in LEVELS {
                        button {
                            key: "{d}",
                            role: "radio",
                            aria_checked: if distance() == d { "true" } else { "false" },
                            class: if distance() == d { "seg on" } else { "seg" },
                            onclick: move |_| {
                                limit.set(PAGE);
                                distance.set(d);
                            },
                            {level_label(d)}
                        }
                    }
                }
            }
            p { class: "muted lead",
                {t!("dup-lead")}
            }
            if unhashed() > 0 {
                div { class: "banner info",
                    {t!("dup-unhashed", count = unhashed())}
                }
            }
            if let Some(message) = error() {
                ErrorBanner { message, onretry: move |()| version += 1 }
            }
            if loading() && shown == 0 {
                div { class: "center-page", Spinner {} }
            } else if shown == 0 && error().is_none() {
                div { class: "card empty",
                    Ico { icon: Icon::Copy, size: 48 }
                    h2 { {t!("dup-none")} }
                    p { class: "muted",
                        if distance() < 10 { {t!("dup-none-try-loose")} } else { {t!("dup-none-loose")} }
                    }
                }
            } else {
                p { class: "dup-count muted", {groups_phrase(total())} }
                div { class: "dup-groups",
                    for group in groups.read().iter().cloned() {
                        if let Some(key) = first_id(&group) {
                            DupGroup { key: "{key}", group, ondone: done }
                        }
                    }
                }
                if u64::try_from(shown).unwrap_or(0) < total() && limit() < MAX_LIMIT {
                    div { class: "sentinel",
                        if loading() {
                            Spinner {}
                        } else {
                            button { class: "btn tonal", onclick: move |_| limit.set((limit() + PAGE).min(MAX_LIMIT)), {t!("show-more")} }
                        }
                    }
                }
            }
        }
    }
}

fn first_id(group: &DuplicateGroup) -> Option<i64> {
    group.posts.first().map(|p| p.id)
}

#[component]
fn DupGroup(group: DuplicateGroup, ondone: EventHandler<i64>) -> Element {
    let key = first_id(&group).unwrap_or(0);
    let mut keep = use_signal(|| key);
    let mut merge = use_signal(|| true);
    let mut busy = use_signal(|| false);
    let mut error = use_signal(|| None::<String>);
    let ids: Vec<i64> = group.posts.iter().map(|p| p.id).collect();
    let others = ids.len().saturating_sub(1);

    let resolve = {
        let ids = ids.clone();
        move |_| {
            let Some(client) = store::client() else {
                return;
            };
            let kept = keep();
            let req = ResolveDuplicates {
                keep: kept,
                remove: ids.iter().copied().filter(|id| *id != kept).collect(),
                merge_tags: merge(),
            };
            busy.set(true);
            error.set(None);
            spawn(async move {
                let result = client.resolve_duplicates(&req).await;
                // Settle local state first: `ondone` unmounts this group.
                busy.set(false);
                match result {
                    Ok(_) => {
                        forget_lists();
                        ondone.call(key);
                    }
                    Err(e) if e.is_unauthorized() => store::forget_session(),
                    Err(e) => error.set(Some(e.to_string())),
                }
            });
        }
    };
    let dismiss = move |_| {
        let Some(client) = store::client() else {
            return;
        };
        let ids = ids.clone();
        busy.set(true);
        error.set(None);
        spawn(async move {
            let result = client.dismiss_duplicates(ids).await;
            busy.set(false);
            match result {
                Ok(()) => ondone.call(key),
                Err(e) if e.is_unauthorized() => store::forget_session(),
                Err(e) => error.set(Some(e.to_string())),
            }
        });
    };

    rsx! {
        section { class: "card dup-group",
            div { class: "dup-head",
                h2 { class: "card-title", {likeness(group.distance)} }
                span { class: "muted",
                    if group.size as usize > group.posts.len() {
                        {t!("dup-some-of", shown = group.posts.len(), count = group.size)}
                    } else {
                        {posts_count(u64::from(group.size))}
                    }
                }
            }
            div { class: "dup-items", role: "radiogroup", aria_label: t!("dup-keep-which"),
                for post in group.posts.iter().cloned() {
                    DupItem {
                        key: "{post.id}",
                        post,
                        group: key,
                        keep: keep(),
                        onpick: move |id| keep.set(id),
                    }
                }
            }
            if let Some(message) = error() {
                p { class: "field-error", role: "alert", "{message}" }
            }
            div { class: "dup-actions",
                label { class: "check",
                    input {
                        r#type: "checkbox",
                        checked: merge(),
                        onchange: move |e| merge.set(e.checked()),
                    }
                    {t!("dup-merge")}
                }
                div { class: "dup-buttons",
                    button { class: "btn text", disabled: busy(), onclick: dismiss, {t!("dup-dismiss")} }
                    button { class: "btn", disabled: busy(), onclick: resolve,
                        Ico { icon: Icon::Delete }
                        {t!("dup-resolve", count = others)}
                    }
                }
            }
        }
    }
}

#[component]
fn DupItem(post: Post, group: i64, keep: i64, onpick: EventHandler<i64>) -> Element {
    let id = post.id;
    let chosen = keep == id;
    let src = post
        .thumb_url
        .as_deref()
        .and_then(|u| store::client().map(|c| cache::url(&c, u)));
    let size = match (post.width, post.height) {
        (Some(w), Some(h)) => format!("{w}×{h}"),
        _ => t!("dup-unknown-size"),
    };
    let tags = t!("tags-count", count = post.tags.len());
    rsx! {
        div { class: if chosen { "dup-item on" } else { "dup-item" },
            label { class: "dup-pick",
                input {
                    r#type: "radio",
                    class: "visually-hidden",
                    name: "keep-{group}",
                    checked: chosen,
                    onchange: move |_| onpick.call(id),
                }
                span { class: "thumb",
                    if let Some(src) = src {
                        img { src, alt: t!("post-alt", id = id), loading: "lazy", decoding: "async" }
                    } else {
                        span { class: "thumb-placeholder", Ico { icon: Icon::Play, size: 40 } }
                    }
                    if chosen {
                        span { class: "keep-badge", Ico { icon: Icon::Check, size: 16 } {t!("dup-keep")} }
                    }
                }
            }
            div { class: "dup-meta",
                span { class: "dup-dims", "{size}" }
                span { class: "muted", "{human_size(post.file_size)} · {tags}" }
                Link { class: "dup-open", to: Route::PostView { id, q: String::new() }, "#{id}" }
            }
        }
    }
}

/// How alike a group looks, from its largest distance in bits.
fn likeness(distance: u32) -> String {
    match distance {
        0 => t!("dup-identical"),
        1..=2 => t!("dup-nearly-identical"),
        3..=6 => t!("dup-very-similar"),
        _ => t!("dup-similar"),
    }
}

fn groups_phrase(total: u64) -> String {
    t!("dup-groups", count = total)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn describes_groups() {
        assert_eq!(likeness(0), "Look identical");
        assert_eq!(likeness(2), "Nearly identical");
        assert_eq!(likeness(6), "Very similar");
        assert_eq!(likeness(10), "Similar");
        assert_eq!(groups_phrase(1), "1 group");
        assert_eq!(groups_phrase(12), "12 groups");
        assert!(LEVELS.windows(2).all(|w| w[0] < w[1]));
        assert_eq!(LEVELS[2], starbooru_core::MAX_DUPLICATE_DISTANCE);
        assert_eq!(level_label(LEVELS[0]), "Strict");
        assert_eq!(level_label(LEVELS[1]), "Normal");
        assert_eq!(level_label(LEVELS[2]), "Loose");
    }
}
