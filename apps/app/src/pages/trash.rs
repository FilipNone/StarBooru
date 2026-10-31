//! The recycle bin: restore posts or delete them for good.

use dioxus::prelude::*;
use starbooru_core::{Post, Role};

use super::gallery::forget_lists;
use super::{ErrorBanner, Spinner};
use crate::Route;
use crate::cache;
use crate::icons::{Ico, Icon};
use crate::store::{self, SESSION};

const PAGE_SIZE: u32 = 60;

#[component]
pub fn Trash() -> Element {
    let mut posts = use_signal(Vec::<Post>::new);
    let mut next_page = use_signal(|| 1u32);
    let mut has_more = use_signal(|| true);
    let mut loading = use_signal(|| false);
    let mut error = use_signal(|| None::<String>);
    let mut busy = use_signal(|| false);
    let mut confirm_empty = use_signal(|| false);
    let mut notice = use_signal(|| None::<String>);

    let mut load_more = move || {
        if *loading.peek() || !*has_more.peek() {
            return;
        }
        let Some(client) = store::client() else {
            return;
        };
        let page = *next_page.peek();
        loading.set(true);
        spawn(async move {
            let result = client.search_posts("status:deleted", page, PAGE_SIZE).await;
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
    use_hook(load_more);

    let mut remove_from_list = move |id: i64| posts.write().retain(|p| p.id != id);

    let restore = move |id: i64| {
        let Some(client) = store::client() else {
            return;
        };
        spawn(async move {
            match client.restore_post(id).await {
                Ok(_) => {
                    forget_lists();
                    remove_from_list(id);
                }
                Err(e) => error.set(Some(e.to_string())),
            }
        });
    };
    let purge = move |id: i64| {
        let Some(client) = store::client() else {
            return;
        };
        spawn(async move {
            match client.purge_post(id).await {
                Ok(()) => remove_from_list(id),
                Err(e) => error.set(Some(e.to_string())),
            }
        });
    };
    let empty_all = move |_| {
        if !confirm_empty() {
            confirm_empty.set(true);
            return;
        }
        let Some(client) = store::client() else {
            return;
        };
        confirm_empty.set(false);
        busy.set(true);
        spawn(async move {
            match client.empty_trash().await {
                Ok(done) => {
                    posts.write().clear();
                    has_more.set(false);
                    notice.set(Some(t!("trash-purged", count = done.purged)));
                }
                Err(e) => error.set(Some(e.to_string())),
            }
            busy.set(false);
        });
    };

    let is_admin = SESSION
        .read()
        .as_ref()
        .is_some_and(|s| s.user.role == Role::Admin);
    let is_empty = posts.read().is_empty() && !loading() && !has_more();

    rsx! {
        div { class: "page",
            div { class: "page-head",
                h1 { class: "page-title", {t!("trash-title")} }
                if is_admin && !posts.read().is_empty() {
                    button {
                        class: if confirm_empty() { "btn danger" } else { "btn tonal" },
                        disabled: busy(),
                        onclick: empty_all,
                        onblur: move |_| confirm_empty.set(false),
                        Ico { icon: Icon::DeleteForever }
                        if confirm_empty() { {t!("trash-empty-confirm")} } else { {t!("trash-empty")} }
                    }
                }
            }
            p { class: "muted lead",
                {t!("trash-lead")}
            }
            if let Some(message) = error() {
                ErrorBanner { message }
            }
            if let Some(message) = notice() {
                div { class: "banner info", role: "status", "{message}" }
            }
            if is_empty {
                div { class: "card empty",
                    Ico { icon: Icon::Delete, size: 48 }
                    h2 { {t!("trash-none")} }
                    Link { class: "btn tonal", to: Route::Gallery { q: String::new() }, {t!("notfound-back")} }
                }
            }
            div { class: "trash-grid",
                for post in posts.read().iter() {
                    TrashItem {
                        key: "{post.id}",
                        post: post.clone(),
                        onrestore: restore,
                        onpurge: purge,
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
        }
    }
}

#[component]
fn TrashItem(post: Post, onrestore: EventHandler<i64>, onpurge: EventHandler<i64>) -> Element {
    let mut confirm = use_signal(|| false);
    let src = post
        .thumb_url
        .as_deref()
        .and_then(|u| store::client().map(|c| cache::url(&c, u)));
    let id = post.id;
    rsx! {
        div { class: "trash-item",
            Link { class: "thumb", to: Route::PostView { id, q: "status:deleted".to_owned() },
                if let Some(src) = src {
                    img { src, alt: "{post.tags.join(\" \")}", loading: "lazy", decoding: "async" }
                } else {
                    div { class: "thumb-placeholder", Ico { icon: Icon::Play, size: 40 } }
                }
            }
            div { class: "trash-actions",
                button { class: "icon-btn", title: t!("trash-restore"), aria_label: t!("trash-restore-post", id = id),
                    onclick: move |_| onrestore.call(id),
                    Ico { icon: Icon::Restore }
                }
                button {
                    class: if confirm() { "btn danger small" } else { "icon-btn danger" },
                    title: t!("trash-purge"),
                    aria_label: t!("trash-purge-post", id = id),
                    onblur: move |_| confirm.set(false),
                    onclick: move |_| {
                        if confirm() {
                            onpurge.call(id);
                        } else {
                            confirm.set(true);
                        }
                    },
                    Ico { icon: Icon::DeleteForever }
                    if confirm() { {t!("sure")} }
                }
            }
        }
    }
}
