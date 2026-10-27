use std::collections::HashMap;

use dioxus::html::geometry::{ClientPoint, PixelsRect};
use dioxus::prelude::*;
use starbooru_client::ClientError;
use starbooru_core::{MediaKind, Neighbors, Post, PostUpdate, Rating};

use super::{ErrorBanner, Spinner};
use crate::Route;
use crate::icons::{Ico, Icon};
use crate::store;
use crate::util::{format_duration, human_size};
use crate::zoom::{STEP, Zoom};

/// Keys the viewer handles, sent from a page-wide listener. Typing in a
/// field never triggers them.
const KEY_LISTENER: &str = r#"
if (window.__sbKeys) window.removeEventListener('keydown', window.__sbKeys);
if (window.__sbFs) document.removeEventListener('fullscreenchange', window.__sbFs);
const keys = ['ArrowLeft', 'ArrowRight', 'Escape', 'f', '+', '=', '-', '0', 'e'];
window.__sbKeys = (e) => {
  if (e.ctrlKey || e.metaKey || e.altKey || e.defaultPrevented) return;
  const t = e.target;
  if (t && (t.isContentEditable || /^(INPUT|TEXTAREA|SELECT)$/.test(t.tagName))) return;
  const key = e.key.length === 1 ? e.key.toLowerCase() : e.key;
  if (!keys.includes(key)) return;
  e.preventDefault();
  dioxus.send(key);
};
window.addEventListener('keydown', window.__sbKeys);
window.__sbFs = () => { if (!document.fullscreenElement) dioxus.send('fullscreen-exit'); };
document.addEventListener('fullscreenchange', window.__sbFs);
"#;

const KEY_LISTENER_OFF: &str = r#"
if (window.__sbKeys) window.removeEventListener('keydown', window.__sbKeys);
if (window.__sbFs) document.removeEventListener('fullscreenchange', window.__sbFs);
window.__sbKeys = null;
window.__sbFs = null;
"#;

/// Asks the browser for real fullscreen where it allows it; the CSS
/// fullscreen mode works without it (desktop webviews, iOS).
fn browser_fullscreen(on: bool) {
    document::eval(if on {
        "document.documentElement.requestFullscreen?.().catch(() => {})"
    } else {
        "if (document.fullscreenElement) document.exitFullscreen().catch(() => {})"
    });
}

#[component]
pub fn PostView(id: i64, q: String) -> Element {
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
    let mut zoom = use_signal(Zoom::default);
    let rect = use_signal(|| None::<PixelsRect>);
    let mut full = use_signal(|| false);
    // The props as signals, for the key handler, which outlives a render.
    let mut current = use_signal(|| (id, q.clone()));
    use_effect(use_reactive!(|id| {
        post.set(None);
        editing.set(false);
        zoom.set(Zoom::default());
        reload(id);
    }));
    use_effect(use_reactive!(|id, q| current.set((id, q))));

    let neighbors = use_resource(move || async move {
        let (id, q) = current();
        let client = store::client()?;
        client.neighbors(id, &q).await.ok()
    });
    let nav = move || (*neighbors.read()).flatten().unwrap_or_default();
    let go = move |to: Option<i64>| {
        if let Some(id) = to {
            let q = current.peek().1.clone();
            // Replace, so Back returns to the gallery rather than through every post.
            navigator().replace(Route::PostView { id, q });
        }
    };
    let mut set_full = move |on: bool| {
        full.set(on);
        browser_fullscreen(on);
    };
    let mut start_edit = move || {
        if let Some(p) = post.peek().as_ref() {
            tag_text.set(p.tags.join(" "));
            rating.set(p.rating);
            editing.set(true);
        }
    };

    use_future(move || async move {
        let mut keys = document::eval(KEY_LISTENER);
        while let Ok(key) = keys.recv::<String>().await {
            if editing() {
                continue;
            }
            match key.as_str() {
                "ArrowLeft" => go(nav().prev),
                "ArrowRight" => go(nav().next),
                "f" => set_full(!full()),
                "fullscreen-exit" => full.set(false),
                "+" | "=" => zoom.set(zoom().zoom_at(STEP, (0.0, 0.0), size_of(rect()))),
                "-" => zoom.set(zoom().zoom_at(1.0 / STEP, (0.0, 0.0), size_of(rect()))),
                "0" => zoom.set(Zoom::default()),
                "e" => start_edit(),
                "Escape" if full() => set_full(false),
                "Escape" if zoom().is_zoomed() => zoom.set(Zoom::default()),
                "Escape" => {
                    let q = current.peek().1.clone();
                    navigator().push(Route::Gallery { q });
                }
                _ => {}
            }
        }
    });
    use_drop(|| {
        document::eval(KEY_LISTENER_OFF);
        browser_fullscreen(false);
    });

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
    let duration = p.duration_ms.and_then(format_duration);

    rsx! {
        div { class: "page post-layout",
            Stage {
                kind: p.kind,
                src: file_url.clone(),
                alt,
                zoom,
                full,
                nav: nav(),
                onnav: move |to| go(Some(to)),
                onfull: set_full,
                rect,
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
                            button { class: "icon-btn", aria_label: "Edit", title: "Edit tags", onclick: move |_| start_edit(),
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
                        if let Some(d) = duration {
                            dt { "Length" } dd { "{d}" }
                        }
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

/// A pointer on the stage, for dragging, pinching and swiping.
#[derive(Clone, Copy)]
struct Touch {
    start: ClientPoint,
    last: ClientPoint,
}

fn distance(a: ClientPoint, b: ClientPoint) -> f64 {
    (a - b).length()
}

/// Where a client point is relative to the centre of `rect`.
fn from_centre(p: ClientPoint, rect: Option<PixelsRect>) -> (f64, f64) {
    match rect {
        Some(r) => {
            let c = r.center();
            (p.x - c.x, p.y - c.y)
        }
        None => (0.0, 0.0),
    }
}

fn size_of(rect: Option<PixelsRect>) -> (f64, f64) {
    rect.map_or((0.0, 0.0), |r| (r.width(), r.height()))
}

/// Swipe distance, in pixels, that turns the page.
const SWIPE: f64 = 60.0;

/// The media, with zoom and pan for images, previous/next and fullscreen.
#[component]
fn Stage(
    kind: MediaKind,
    src: String,
    alt: String,
    zoom: Signal<Zoom>,
    full: Signal<bool>,
    nav: Neighbors,
    onnav: EventHandler<i64>,
    onfull: EventHandler<bool>,
    rect: Signal<Option<PixelsRect>>,
) -> Element {
    let mut mounted = use_signal(|| None::<std::rc::Rc<MountedData>>);
    let mut touches = use_signal(HashMap::<i32, Touch>::new);
    let zoomable = kind != MediaKind::Video;

    // Rect of the stage; measured when a gesture starts, as scrolling moves it.
    let measure = move || async move {
        let handle = mounted.peek().clone();
        if let Some(m) = handle
            && let Ok(r) = m.get_client_rect().await
        {
            rect.set(Some(r));
        }
        *rect.peek()
    };

    let z = zoom();
    let mut class = String::from("stage");
    for (on, name) in [
        (full(), " full"),
        (z.is_zoomed(), " zoomed"),
        (!touches.read().is_empty(), " dragging"),
    ] {
        if on {
            class.push_str(name);
        }
    }
    rsx! {
        section {
            class,
            onmounted: move |e| {
                mounted.set(Some(e.data()));
                spawn(async move {
                    measure().await;
                });
            },
            onresize: move |_| {
                zoom.set(Zoom::default());
                spawn(async move {
                    measure().await;
                });
            },
            onwheel: move |e| {
                if !zoomable {
                    return;
                }
                e.prevent_default();
                let dy = e.delta().strip_units().y;
                let at = e.client_coordinates();
                spawn(async move {
                    let r = measure().await;
                    // About 10% per notch of a mouse wheel.
                    let factor = (-dy / 1000.0).exp();
                    zoom.set(zoom().zoom_at(factor, from_centre(at, r), size_of(r)));
                });
            },
            ondoubleclick: move |e| {
                if !zoomable {
                    return;
                }
                let at = e.client_coordinates();
                let r = *rect.peek();
                zoom.set(zoom().toggle_at(from_centre(at, r), size_of(r)));
            },
            onpointerdown: move |e| {
                if !zoomable {
                    return;
                }
                let p = e.client_coordinates();
                touches.write().insert(e.pointer_id(), Touch { start: p, last: p });
                spawn(async move {
                    measure().await;
                });
            },
            onpointermove: move |e| {
                let id = e.pointer_id();
                let p = e.client_coordinates();
                let mut map = touches.write();
                let Some(prev) = map.get(&id).copied() else {
                    return;
                };
                let r = *rect.peek();
                if map.len() == 2 {
                    // Pinch: scale by the change in finger distance, around the midpoint.
                    let other = map.iter().find(|(k, _)| **k != id).map(|(_, t)| t.last);
                    if let Some(o) = other {
                        let before = distance(prev.last, o);
                        let after = distance(p, o);
                        if before > 1.0 {
                            let mid = ClientPoint::new((p.x + o.x) / 2.0, (p.y + o.y) / 2.0);
                            zoom.set(zoom().zoom_at(after / before, from_centre(mid, r), size_of(r)));
                        }
                    }
                } else if zoom().is_zoomed() {
                    zoom.set(zoom().pan(p.x - prev.last.x, p.y - prev.last.y, size_of(r)));
                }
                if let Some(t) = map.get_mut(&id) {
                    t.last = p;
                }
            },
            onpointerup: move |e| {
                let ended = touches.write().remove(&e.pointer_id());
                let Some(t) = ended else {
                    return;
                };
                let (dx, dy) = (t.last.x - t.start.x, t.last.y - t.start.y);
                if touches.read().is_empty() && !zoom().is_zoomed() && dx.abs() > SWIPE && dx.abs() > dy.abs() * 1.5 {
                    let to = if dx > 0.0 { nav.prev } else { nav.next };
                    if let Some(to) = to {
                        onnav.call(to);
                    }
                }
            },
            onpointercancel: move |e| {
                touches.write().remove(&e.pointer_id());
            },
            onpointerleave: move |e| {
                touches.write().remove(&e.pointer_id());
            },
            match kind {
                MediaKind::Video => rsx! {
                    video { src: "{src}", controls: true, autoplay: true, r#loop: true, playsinline: true }
                },
                _ => rsx! {
                    img { src: "{src}", alt: "{alt}", draggable: "false", style: z.css() }
                },
            }
            if let Some(prev) = nav.prev {
                button { class: "stage-btn stage-prev", title: "Previous (←)", aria_label: "Previous post",
                    onclick: move |_| onnav.call(prev),
                    Ico { icon: Icon::ChevronLeft, size: 32 }
                }
            }
            if let Some(next) = nav.next {
                button { class: "stage-btn stage-next", title: "Next (→)", aria_label: "Next post",
                    onclick: move |_| onnav.call(next),
                    Ico { icon: Icon::ChevronRight, size: 32 }
                }
            }
            div { class: "stage-tools",
                if zoomable {
                    button { class: "stage-btn", title: "Zoom out (−)", aria_label: "Zoom out",
                        disabled: !z.is_zoomed(),
                        onclick: move |_| zoom.set(zoom().zoom_at(1.0 / STEP, (0.0, 0.0), size_of(*rect.peek()))),
                        Ico { icon: Icon::ZoomOut }
                    }
                    button { class: "stage-zoom", title: "Fit (0)", aria_label: "Reset zoom",
                        onclick: move |_| zoom.set(Zoom::default()),
                        "{z.percent()}%"
                    }
                    button { class: "stage-btn", title: "Zoom in (+)", aria_label: "Zoom in",
                        disabled: z.scale >= crate::zoom::MAX_SCALE,
                        onclick: move |_| zoom.set(zoom().zoom_at(STEP, (0.0, 0.0), size_of(*rect.peek()))),
                        Ico { icon: Icon::ZoomIn }
                    }
                }
                button {
                    class: "stage-btn",
                    title: if full() { "Exit fullscreen (F)" } else { "Fullscreen (F)" },
                    aria_label: if full() { "Exit fullscreen" } else { "Fullscreen" },
                    onclick: move |_| onfull.call(!full()),
                    Ico { icon: if full() { Icon::FullscreenExit } else { Icon::Fullscreen } }
                }
            }
        }
    }
}
