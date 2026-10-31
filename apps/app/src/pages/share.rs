//! Share links: the sheet that makes one, the list in Settings and the
//! public page a link opens (no account needed).

use chrono::{DateTime, Utc};
use dioxus::prelude::*;
use starbooru_client::Client;
use starbooru_core::share::{MAX_SHARE_TITLE_CHARS, share_path};
use starbooru_core::{MediaKind, NewShare, Share, SharedView};

use super::{Choice, ErrorBanner, Select, Spinner};
use crate::brand::{Mark, Wordmark};
use crate::icons::{Ico, Icon};
use crate::power;
use crate::store;
use crate::util::{format_duration, short_date, time_ago};

/// Lifetimes offered for a new link, in days; `None` lasts until removed.
const LIFETIMES: [Option<u32>; 4] = [None, Some(1), Some(7), Some(30)];

fn lifetime_label(days: Option<u32>) -> String {
    match days {
        None => t!("share-until-removed"),
        Some(d) => t!("keys-days", count = d),
    }
}

/// The address to give out: the server serves the web app that opens it.
fn link(client: &Client, token: &str) -> String {
    client.absolute(&share_path(token))
}

/// "Made 3 days ago · opened twice · expires 27 Nov 2027", for one link.
fn share_status(share: &Share, now: DateTime<Utc>) -> String {
    let made = t!("shares-made", when = time_ago(share.created_at, now));
    let views = t!("shares-views", count = share.views);
    let expiry = match share.expires_at {
        None => t!("keys-no-expiry"),
        Some(t) if t <= now => t!("keys-expired-on", date = short_date(t)),
        Some(t) => t!("keys-expires-on", date = short_date(t)),
    };
    format!("{made} · {views} · {expiry}")
}

/// Copies `text`, flipping `copied` on success. Without clipboard access
/// (plain http) the field stays there to select and copy by hand.
pub(super) fn copy_text(text: String, mut copied: Signal<bool>) {
    spawn(async move {
        let eval = document::eval(
            "const text = await dioxus.recv(); \
             await navigator.clipboard.writeText(text); \
             return true;",
        );
        if eval.send(text).is_ok() && eval.await.is_ok() {
            copied.set(true);
        }
    });
}

#[component]
fn LinkField(url: String) -> Element {
    let copied = use_signal(|| false);
    let text = url.clone();
    rsx! {
        div { class: "key-token",
            input {
                readonly: true,
                spellcheck: "false",
                aria_label: t!("share-link"),
                value: "{url}",
                onfocus: move |_| {
                    document::eval("document.activeElement?.select?.()");
                },
            }
            button {
                class: "btn tonal small",
                r#type: "button",
                title: t!("share-copy"),
                onclick: move |_| copy_text(text.clone(), copied),
                Ico { icon: if copied() { Icon::Check } else { Icon::Copy }, size: 18 }
                span { class: "copy-label",
                    if copied() { {t!("keys-copied")} } else { {t!("share-copy")} }
                }
            }
        }
    }
}

/// Sheet over the page that makes a link for `post_ids`, then shows it.
#[component]
pub fn ShareSheet(post_ids: Vec<i64>, onclose: EventHandler<()>) -> Element {
    let mut title = use_signal(String::new);
    let mut lifetime = use_signal(|| Some(7_u32));
    let mut busy = use_signal(|| false);
    let mut error = use_signal(|| None::<String>);
    let mut made = use_signal(|| None::<String>);
    let count = post_ids.len();

    let create = move |e: FormEvent| {
        e.prevent_default();
        let Some(client) = store::client() else {
            return;
        };
        let request = NewShare {
            post_ids: post_ids.clone(),
            title: Some(title.peek().clone()),
            days: *lifetime.peek(),
        };
        busy.set(true);
        error.set(None);
        spawn(async move {
            match client.create_share(&request).await {
                Ok(share) => made.set(Some(link(&client, &share.token))),
                Err(e) if e.is_unauthorized() => store::forget_session(),
                Err(e) => error.set(Some(e.to_string())),
            }
            busy.set(false);
        });
    };

    rsx! {
        div {
            class: "sheet-scrim",
            onclick: move |_| onclose.call(()),
            onkeydown: move |e| {
                if e.key() == Key::Escape {
                    onclose.call(());
                }
            },
            div {
                class: "card sheet",
                role: "dialog",
                aria_modal: "true",
                aria_labelledby: "share-sheet-title",
                onclick: move |e| e.stop_propagation(),
                div { class: "sheet-head",
                    h2 { id: "share-sheet-title", class: "card-title", {t!("share-title", count = count)} }
                    button { class: "icon-btn", r#type: "button", aria_label: t!("share-close"), title: t!("share-close"),
                        onclick: move |_| onclose.call(()),
                        Ico { icon: Icon::Close }
                    }
                }
                p { class: "muted hint", {t!("share-lead")} }
                if let Some(message) = error() {
                    ErrorBanner { message }
                }
                if let Some(url) = made() {
                    div { class: "banner info key-created", role: "status",
                        strong { {t!("share-ready")} }
                        LinkField { url }
                    }
                    div { class: "row-end",
                        button { class: "btn", r#type: "button", onclick: move |_| onclose.call(()), {t!("share-done")} }
                    }
                } else {
                    form { onsubmit: create,
                        label { class: "field",
                            span { {t!("share-name")} }
                            input {
                                placeholder: t!("share-name-placeholder"),
                                maxlength: "{MAX_SHARE_TITLE_CHARS}",
                                autofocus: true,
                                value: "{title}",
                                oninput: move |e| title.set(e.value()),
                            }
                        }
                        div { class: "key-options",
                            div { class: "field",
                                span { aria_hidden: "true", {t!("share-lifetime")} }
                                Select {
                                    aria_label: t!("share-lifetime"),
                                    value: lifetime().map(|d| d.to_string()).unwrap_or_default(),
                                    options: LIFETIMES
                                        .into_iter()
                                        .map(|d| Choice::new(d.map(|d| d.to_string()).unwrap_or_default(), lifetime_label(d)))
                                        .collect::<Vec<_>>(),
                                    onchange: move |v: String| lifetime.set(v.parse().ok()),
                                }
                            }
                        }
                        div { class: "row-end",
                            button { class: "btn text", r#type: "button", onclick: move |_| onclose.call(()), {t!("cancel")} }
                            button { class: "btn", r#type: "submit", disabled: busy(),
                                Ico { icon: Icon::Link }
                                {t!("share-create")}
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Settings › Shared links.
#[component]
pub fn SharesCard() -> Element {
    let mut list = use_signal(|| None::<Vec<Share>>);
    let mut error = use_signal(|| None::<String>);
    let mut busy = use_signal(|| false);

    use_hook(move || {
        let Some(client) = store::client() else {
            return;
        };
        spawn(async move {
            match client.shares().await {
                Ok(found) => list.set(Some(found)),
                Err(e) if e.is_unauthorized() => store::forget_session(),
                Err(e) => error.set(Some(e.to_string())),
            }
        });
    });
    let remove = move |id: i64| {
        let Some(client) = store::client() else {
            return;
        };
        busy.set(true);
        error.set(None);
        spawn(async move {
            match client.delete_share(id).await {
                Ok(()) => {
                    if let Some(l) = list.write().as_mut() {
                        l.retain(|s| s.id != id);
                    }
                }
                Err(e) => error.set(Some(e.to_string())),
            }
            busy.set(false);
        });
    };

    let client = store::client();
    let now = Utc::now();
    rsx! {
        section { class: "card",
            h2 { class: "card-title", {t!("shares-title")} }
            p { class: "muted hint", {t!("shares-lead")} }
            if let Some(message) = error() {
                ErrorBanner { message }
            }
            match (list.read().as_ref(), client.as_ref()) {
                (None, _) | (_, None) => rsx! { Spinner {} },
                (Some(shares), _) if shares.is_empty() => rsx! {
                    p { class: "muted", {t!("shares-none")} }
                },
                (Some(shares), Some(client)) => rsx! {
                    ul { class: "sessions",
                        for s in shares.iter() {
                            li { key: "{s.id}", class: "session share",
                                div { class: "share-cover",
                                    if let Some(cover) = &s.cover_url {
                                        img { src: client.absolute(cover), alt: "", loading: "lazy" }
                                    } else {
                                        Ico { icon: Icon::Link }
                                    }
                                }
                                div { class: "session-main",
                                    strong { class: "break",
                                        {s.title.clone().unwrap_or_else(|| t!("shares-untitled"))}
                                    }
                                    span { class: "pill", {t!("share-posts", count = s.posts)} }
                                    if s.expires_at.is_some_and(|t| t <= now) {
                                        span { class: "pill bad", {t!("keys-expired")} }
                                    }
                                    span { class: "muted", {share_status(s, now)} }
                                    LinkField { url: link(client, &s.token) }
                                }
                                ShareDelete {
                                    id: s.id,
                                    name: s.title.clone().unwrap_or_else(|| t!("shares-untitled")),
                                    disabled: busy(),
                                    ondelete: remove,
                                }
                            }
                        }
                    }
                },
            }
        }
    }
}

#[component]
fn ShareDelete(id: i64, name: String, disabled: bool, ondelete: EventHandler<i64>) -> Element {
    let mut confirm = use_signal(|| false);
    rsx! {
        button {
            class: if confirm() { "btn danger small" } else { "icon-btn danger" },
            title: t!("shares-delete"),
            aria_label: t!("shares-delete-named", name = name),
            disabled,
            onblur: move |_| confirm.set(false),
            onclick: move |_| {
                if confirm() {
                    ondelete.call(id);
                } else {
                    confirm.set(true);
                }
            },
            Ico { icon: Icon::Delete }
            if confirm() { {t!("sure")} }
        }
    }
}

/// Client for the server a link belongs to. The web app is served by that
/// server; the native apps ask the server they are signed in to.
fn public_client() -> Option<Client> {
    let server = if cfg!(target_arch = "wasm32") {
        store::default_server()
    } else {
        store::SESSION
            .peek()
            .as_ref()
            .map(|s| s.server.clone())
            .unwrap_or_else(store::default_server)
    };
    Client::new(&server).ok()
}

/// `/s/{token}`: what someone shared, for anyone with the link.
#[component]
pub fn SharedPage(token: String) -> Element {
    let mut view = use_signal(|| None::<Result<SharedView, String>>);
    let mut open = use_signal(|| None::<usize>);
    let client = use_hook(public_client);

    let load_client = client.clone();
    use_effect(use_reactive!(|token| {
        let Some(client) = load_client.clone() else {
            view.set(Some(Err(String::new())));
            return;
        };
        view.set(None);
        spawn(async move {
            view.set(Some(match client.shared(&token).await {
                Ok(v) => Ok(v),
                Err(e) if e.code() == Some("not_found") => Err(String::new()),
                Err(e) => Err(e.to_string()),
            }));
        });
    }));

    let abs = move |u: &str| client.as_ref().map(|c| c.absolute(u)).unwrap_or_default();
    let body = match view.read().as_ref() {
        None => rsx! { div { class: "center-page", Spinner {} } },
        Some(Err(message)) if message.is_empty() => rsx! {
            div { class: "center-page",
                div { class: "card empty",
                    h1 { {t!("shared-missing-title")} }
                    p { class: "muted", {t!("shared-missing-body")} }
                }
            }
        },
        Some(Err(message)) => rsx! {
            div { class: "center-page", ErrorBanner { message: message.clone() } }
        },
        Some(Ok(v)) => {
            let count = v.posts.len();
            let heading = v
                .title
                .clone()
                .unwrap_or_else(|| t!("share-posts", count = count));
            let viewing = open().and_then(|i| v.posts.get(i).map(|p| (i, p.clone())));
            rsx! {
                header { class: "shared-head",
                    div {
                        h1 { class: "page-title", "{heading}" }
                        p { class: "muted",
                            {t!("shared-from", server = v.server.clone())}
                            if let Some(t) = v.expires_at {
                                " · "
                                {t!("shared-until", date = short_date(t))}
                            }
                        }
                    }
                }
                if v.posts.is_empty() {
                    p { class: "muted", {t!("shared-empty")} }
                }
                div { class: "grid",
                    for (i, p) in v.posts.iter().enumerate() {
                        button {
                            key: "{i}",
                            class: "thumb",
                            style: "--delay: {i.min(40) * 16}ms",
                            aria_label: t!("shared-open", n = i + 1),
                            onclick: move |_| open.set(Some(i)),
                            div { class: "thumb-media",
                                if let Some(thumb) = &p.thumb_url {
                                    img { src: abs(thumb), alt: "", loading: "lazy" }
                                } else {
                                    div { class: "thumb-placeholder", Ico { icon: Icon::Gallery } }
                                }
                            }
                            if let Some(d) = p.duration_ms.and_then(format_duration) {
                                span { class: "badge", Ico { icon: Icon::Play, size: 14 } "{d}" }
                            }
                        }
                    }
                }
                if let Some((i, p)) = viewing {
                    div {
                        class: "lightbox",
                        role: "dialog",
                        aria_modal: "true",
                        aria_label: t!("shared-open", n = i + 1),
                        tabindex: "-1",
                        autofocus: true,
                        onmounted: move |e| {
                            spawn(async move {
                                let _ = e.set_focus(true).await;
                            });
                        },
                        onclick: move |_| open.set(None),
                        onkeydown: move |e| match e.key() {
                            Key::Escape => open.set(None),
                            Key::ArrowLeft if i > 0 => open.set(Some(i - 1)),
                            Key::ArrowRight if i + 1 < count => open.set(Some(i + 1)),
                            _ => {}
                        },
                        div { class: "lightbox-media", onclick: move |e| e.stop_propagation(),
                            if matches!(p.kind, MediaKind::Video) {
                                video { key: "{i}", src: abs(&p.file_url), controls: true, autoplay: !power::active(), playsinline: true }
                            } else {
                                img { key: "{i}", src: abs(p.sample_url.as_deref().unwrap_or(&p.file_url)), alt: "" }
                            }
                        }
                        button { class: "lightbox-btn close", aria_label: t!("share-close"),
                            onclick: move |_| open.set(None),
                            Ico { icon: Icon::Close }
                        }
                        if i > 0 {
                            button { class: "lightbox-btn prev", aria_label: t!("shared-previous"),
                                onclick: move |e| { e.stop_propagation(); open.set(Some(i - 1)); },
                                Ico { icon: Icon::ChevronLeft }
                            }
                        }
                        if i + 1 < count {
                            button { class: "lightbox-btn next", aria_label: t!("shared-next"),
                                onclick: move |e| { e.stop_propagation(); open.set(Some(i + 1)); },
                                Ico { icon: Icon::ChevronRight }
                            }
                        }
                        span { class: "lightbox-count", "{i + 1} / {count}" }
                    }
                }
            }
        }
    };
    rsx! {
        // The token is in the address: never pass it on to other sites.
        document::Meta { name: "referrer", content: "no-referrer" }
        document::Meta { name: "robots", content: "noindex, nofollow" }
        div { class: "shared-page",
            div { class: "brand legal-brand", role: "img", aria_label: "StarBooru",
                Mark { height: 30, compact: true }
                Wordmark { height: 19 }
            }
            {body}
        }
    }
}

#[cfg(test)]
mod tests {
    use chrono::TimeDelta;

    use super::*;

    #[test]
    fn describes_links() {
        let now = DateTime::from_timestamp(1_800_000_000, 0).unwrap();
        let mut share = Share {
            id: 1,
            token: "0123456789abcdef0123456789abcdef".into(),
            title: None,
            posts: 3,
            cover_url: None,
            created_at: now - TimeDelta::minutes(3),
            expires_at: None,
            views: 0,
            last_viewed_at: None,
        };
        assert_eq!(
            share_status(&share, now),
            "Made 3 minutes ago · Never opened · Never expires"
        );
        share.views = 1;
        share.expires_at = Some(now - TimeDelta::days(1));
        assert_eq!(
            share_status(&share, now),
            "Made 3 minutes ago · Opened once · Expired 14 Jan 2027"
        );
        share.views = 12;
        assert!(share_status(&share, now).contains("Opened 12 times"));
        assert_eq!(lifetime_label(None), "Until removed");
        assert_eq!(lifetime_label(Some(1)), "1 day");
        let client = Client::new("https://booru.example/").unwrap();
        assert_eq!(
            link(&client, &share.token),
            "https://booru.example/s/0123456789abcdef0123456789abcdef"
        );
    }
}
