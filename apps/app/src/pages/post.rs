use dioxus::prelude::*;
use starbooru_client::ClientError;
use starbooru_core::{MediaKind, Post, PostUpdate, Rating};

use super::{ErrorBanner, Spinner};
use crate::Route;
use crate::icons::{Ico, Icon};
use crate::store;
use crate::util::human_size;

#[component]
pub fn PostView(id: i64) -> Element {
    let mut post = use_signal(|| None::<Post>);
    let mut error = use_signal(|| None::<String>);
    let mut busy = use_signal(|| false);
    let mut editing = use_signal(|| false);
    let mut tag_text = use_signal(String::new);
    let mut rating = use_signal(Rating::default);

    let mut handle = move |result: Result<Post, ClientError>| match result {
        Ok(p) => {
            post.set(Some(p));
            error.set(None);
        }
        Err(e) if e.is_unauthorized() => store::forget_session(),
        Err(e) => error.set(Some(e.to_string())),
    };

    let reload = move |id: i64| {
        let Some(client) = store::client() else {
            return;
        };
        spawn(async move { handle(client.post(id).await) });
    };
    use_effect(use_reactive!(|id| {
        post.set(None);
        editing.set(false);
        reload(id);
    }));

    // Runs one mutation, then shows the post the server returns.
    let mut act = move |op: Op| {
        if busy() {
            return;
        }
        let Some(client) = store::client() else {
            return;
        };
        busy.set(true);
        spawn(async move {
            let result = match op {
                Op::Vote(v) => client.vote(id, v).await,
                Op::Save(update) => client.update_post(id, &update).await,
                Op::Delete => match client.delete_post(id).await {
                    Ok(()) => client.post(id).await,
                    Err(e) => Err(e),
                },
                Op::Restore => client.restore_post(id).await,
            };
            let saved = result.is_ok();
            handle(result);
            if saved {
                editing.set(false);
            }
            busy.set(false);
        });
    };

    let Some(p) = post() else {
        return rsx! {
            div { class: "page",
                if let Some(message) = error() {
                    ErrorBanner { message, onretry: move |()| reload(id) }
                } else {
                    div { class: "center-page", Spinner {} }
                }
            }
        };
    };

    let client = store::client();
    let abs = |u: &str| client.as_ref().map(|c| c.absolute(u)).unwrap_or_default();
    let file_url = abs(&p.file_url);
    let ext = p.mime.rsplit('/').next().unwrap_or("bin").to_owned();
    let alt = p.tags.join(" ");
    let added = p.created_at.format("%Y-%m-%d %H:%M").to_string();
    let dims = match (p.width, p.height) {
        (Some(w), Some(h)) => format!("{w} × {h}"),
        _ => "—".into(),
    };
    let start_edit = {
        let tags = p.tags.join(" ");
        let current = p.rating;
        move |_| {
            tag_text.set(tags.clone());
            rating.set(current);
            editing.set(true);
        }
    };

    rsx! {
        div { class: "page post-layout",
            section { class: "stage",
                match p.kind {
                    MediaKind::Video => rsx! {
                        video { src: "{file_url}", controls: true, autoplay: true, r#loop: true, playsinline: true }
                    },
                    _ => rsx! { img { src: "{file_url}", alt: "{alt}" } },
                }
            }
            aside { class: "side",
                if let Some(message) = error() {
                    ErrorBanner { message }
                }
                if p.deleted {
                    div { class: "banner warn",
                        span { "This post is in the recycle bin." }
                        button { class: "btn tonal", disabled: busy(), onclick: move |_| act(Op::Restore),
                            Ico { icon: Icon::Restore } "Restore"
                        }
                    }
                }
                div { class: "card actions",
                    div { class: "vote",
                        button {
                            class: if p.my_vote > 0 { "icon-btn on" } else { "icon-btn" },
                            aria_label: "Upvote",
                            disabled: busy() || p.deleted,
                            onclick: move |_| act(Op::Vote(if p.my_vote > 0 { 0 } else { 1 })),
                            Ico { icon: Icon::ThumbUp }
                        }
                        span { class: "score", "{p.score}" }
                        button {
                            class: if p.my_vote < 0 { "icon-btn on" } else { "icon-btn" },
                            aria_label: "Downvote",
                            disabled: busy() || p.deleted,
                            onclick: move |_| act(Op::Vote(if p.my_vote < 0 { 0 } else { -1 })),
                            Ico { icon: Icon::ThumbDown }
                        }
                    }
                    div { class: "row-actions",
                        a { class: "icon-btn", href: "{file_url}", download: "{p.sha256}.{ext}", aria_label: "Download", title: "Download",
                            Ico { icon: Icon::Download }
                        }
                        if !p.deleted {
                            button { class: "icon-btn", aria_label: "Edit", title: "Edit tags", onclick: start_edit,
                                Ico { icon: Icon::Edit }
                            }
                            button { class: "icon-btn danger", aria_label: "Delete", title: "Move to recycle bin",
                                disabled: busy(), onclick: move |_| act(Op::Delete),
                                Ico { icon: Icon::Delete }
                            }
                        }
                    }
                }
                if editing() {
                    form {
                        class: "card edit",
                        onsubmit: move |e: FormEvent| {
                            e.prevent_default();
                            act(Op::Save(PostUpdate {
                                tags: Some(tag_text().split_whitespace().map(str::to_owned).collect()),
                                rating: Some(rating()),
                                source: None,
                            }));
                        },
                        label { class: "field",
                            span { "Tags" }
                            textarea {
                                rows: "5",
                                spellcheck: "false",
                                value: "{tag_text}",
                                oninput: move |e| tag_text.set(e.value()),
                            }
                        }
                        RatingPicker { value: rating() , onchange: move |r| rating.set(r) }
                        div { class: "row-end",
                            button { class: "btn text", r#type: "button", onclick: move |_| editing.set(false), "Cancel" }
                            button { class: "btn", r#type: "submit", disabled: busy(), Ico { icon: Icon::Check } "Save" }
                        }
                    }
                } else {
                    div { class: "card",
                        h2 { class: "card-title", "Tags" }
                        if p.tags.is_empty() {
                            p { class: "muted", "No tags yet." }
                        }
                        div { class: "chips",
                            for tag in p.tags.iter() {
                                Link { key: "{tag}", class: "chip", to: Route::Gallery { q: tag.clone() }, "{tag}" }
                            }
                        }
                    }
                }
                div { class: "card",
                    h2 { class: "card-title", "Details" }
                    dl { class: "meta",
                        dt { "Rating" } dd { "{p.rating.as_str()}" }
                        dt { "Size" } dd { "{human_size(p.file_size)}" }
                        dt { "Dimensions" } dd { "{dims}" }
                        dt { "Type" } dd { "{p.mime}" }
                        dt { "Added" } dd { "{added}" }
                        if let Some(source) = &p.source {
                            dt { "Source" } dd { class: "break", "{source}" }
                        }
                    }
                }
            }
        }
    }
}

enum Op {
    Vote(i16),
    Save(PostUpdate),
    Delete,
    Restore,
}

#[component]
pub fn RatingPicker(value: Rating, onchange: EventHandler<Rating>) -> Element {
    rsx! {
        div { class: "segmented", role: "radiogroup", aria_label: "Rating",
            for r in Rating::ALL {
                button {
                    key: "{r.as_str()}",
                    r#type: "button",
                    role: "radio",
                    aria_checked: if r == value { "true" } else { "false" },
                    class: if r == value { "seg on" } else { "seg" },
                    onclick: move |_| onchange.call(r),
                    "{r.as_str()}"
                }
            }
        }
    }
}
