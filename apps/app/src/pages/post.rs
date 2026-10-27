use std::collections::HashMap;

use dioxus::html::geometry::{ClientPoint, PixelsRect};
use dioxus::prelude::*;
use starbooru_client::{Client, ClientError};
use starbooru_core::{
    EditKind, MediaKind, Neighbors, Post, PostUpdate, Rating, SAMPLE_SIZE, edit_kind,
};

use super::editor::EditorSheet;
use super::share::ShareSheet;
use super::{ErrorBanner, Spinner};
use crate::Route;
use crate::cache;
use crate::download::DownloadLink;
use crate::icons::{Ico, Icon};
use crate::power;
use crate::store;
use crate::util::{format_duration, human_size, rating_label};
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

/// The post a gallery thumbnail is opening, so the viewer can show it
/// before the server answers.
pub static OPENING: GlobalSignal<Option<Post>> = Signal::global(|| None);

/// Ends the view transition a gallery thumbnail started, once the viewer's
/// image is in place.
const HERO_DONE: &str = "window.__sbHeroDone?.()";

#[component]
pub fn PostView(id: i64, q: String) -> Element {
    let mut post = use_signal(|| None::<Post>);
    let mut error = use_signal(|| None::<String>);
    let mut busy = use_signal(|| false);
    let mut editing = use_signal(|| false);
    let mut sharing = use_signal(|| false);
    let mut media_editing = use_signal(|| false);
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
    // Set once the original is wanted instead of the sample: zooming in
    // asks for the detail only the original has.
    let mut original = use_signal(|| false);
    use_effect(move || {
        if zoom().is_zoomed() && !*original.peek() {
            original.set(true);
        }
    });
    let rect = use_signal(|| None::<PixelsRect>);
    let mut full = use_signal(|| false);
    // Device pixels per CSS pixel. When the stage shows more device pixels
    // than the sample has (fullscreen on a big or dense screen), the
    // original is shown instead.
    let dpr = use_resource(|| async {
        document::eval("return window.devicePixelRatio || 1")
            .join::<f64>()
            .await
            .ok()
    });
    use_effect(move || {
        let (Some(r), Some(dpr)) = (rect(), (*dpr.read()).flatten()) else {
            return;
        };
        let natural = post.read().as_ref().and_then(|p| {
            (p.kind != MediaKind::Video && p.sample_url.is_some())
                .then_some(p.width.zip(p.height))
                .flatten()
        });
        if let Some(natural) = natural
            && !sample_is_sharp(natural, (r.width(), r.height()), dpr)
            && !*original.peek()
            && !power::active()
        {
            original.set(true);
        }
    });
    // The props as signals, for the key handler, which outlives a render.
    let mut current = use_signal(|| (id, q.clone()));
    // Neighbours fetched ahead, so previous/next shows at once.
    let mut ahead = use_signal(HashMap::<i64, Post>::new);
    // Which way the last previous/next went: `Some(true)` for next.
    let mut entered = use_signal(|| None::<bool>);
    use_effect(use_reactive!(|id| {
        let known = ahead.peek().get(&id).cloned();
        let opening = OPENING.write().take().filter(|p| p.id == id);
        post.set(known.or(opening));
        editing.set(false);
        media_editing.set(false);
        zoom.set(Zoom::default());
        original.set(false);
        reload(id);
    }));
    use_effect(use_reactive!(|id, q| current.set((id, q))));

    let neighbors = use_resource(move || async move {
        let (id, q) = current();
        let client = store::client()?;
        client.neighbors(id, &q).await.ok()
    });
    let nav = move || (*neighbors.read()).flatten().unwrap_or_default();
    use_effect(move || {
        let n = nav();
        let here = current.peek().0;
        ahead
            .write()
            .retain(|k, _| *k == here || Some(*k) == n.prev || Some(*k) == n.next);
        if power::active() {
            return;
        }
        let Some(client) = store::client() else {
            return;
        };
        for near in [n.prev, n.next].into_iter().flatten() {
            if ahead.peek().contains_key(&near) {
                continue;
            }
            let client = client.clone();
            spawn(async move {
                if let Ok(p) = client.post(near).await {
                    if p.kind != MediaKind::Video {
                        let url = stage_url(&client, &p, false);
                        let url = serde_json::to_string(&url).unwrap_or_default();
                        document::eval(&format!("new Image().src = {url}"));
                    }
                    ahead.write().insert(near, p);
                }
            });
        }
    });
    // Trimming needs ffmpeg on the server.
    let video_editing = use_resource(|| async move {
        let client = store::client()?;
        client.info().await.ok().map(|i| i.video_editing)
    });
    let mut go = move |to: Option<i64>| {
        if let Some(id) = to {
            let n = (*neighbors.peek()).flatten().unwrap_or_default();
            entered.set(Some(n.prev != Some(id)));
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
            if editing() || media_editing() {
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
    let sample = p
        .sample_url
        .as_deref()
        .filter(|_| !original() && !store::PREFS.read().originals);
    let src = client
        .as_ref()
        .map(|c| stage_url(c, &p, original()))
        .unwrap_or_default();
    let thumb = p
        .thumb_url
        .as_deref()
        .and_then(|u| client.as_ref().map(|c| cache::url(c, u)));
    let ext = p.mime.rsplit('/').next().unwrap_or("bin").to_owned();
    let alt = p.tags.join(" ");
    let added = p.created_at.format("%Y-%m-%d %H:%M").to_string();
    let dims = match (p.width, p.height) {
        (Some(w), Some(h)) => format!("{w} × {h}"),
        _ => "—".into(),
    };
    let duration = p.duration_ms.and_then(format_duration);
    let editable = edit_kind(p.kind, &p.mime).filter(|k| match k {
        EditKind::Image => p.width.is_some() && p.height.is_some(),
        EditKind::Video => {
            p.duration_ms.is_some() && (*video_editing.read()).flatten().unwrap_or(false)
        }
    });

    rsx! {
        div { class: "page post-layout",
            Stage {
                id: p.id,
                kind: p.kind,
                src,
                thumb,
                size: p.width.zip(p.height),
                enter: enter_class(entered()),
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
                        span { {t!("post-in-trash")} }
                        button { class: "btn tonal", disabled: busy(), onclick: move |_| act(Op::Restore),
                            Ico { icon: Icon::Restore } {t!("trash-restore")}
                        }
                    }
                }
                div { class: "card actions",
                    div { class: "vote",
                        button {
                            class: if p.my_vote > 0 { "icon-btn on" } else { "icon-btn" },
                            aria_label: t!("post-upvote"),
                            disabled: busy() || p.deleted,
                            onclick: move |_| act(Op::Vote(if p.my_vote > 0 { 0 } else { 1 })),
                            Ico { icon: Icon::ThumbUp }
                        }
                        span { class: "score", "{p.score}" }
                        button {
                            class: if p.my_vote < 0 { "icon-btn on" } else { "icon-btn" },
                            aria_label: t!("post-downvote"),
                            disabled: busy() || p.deleted,
                            onclick: move |_| act(Op::Vote(if p.my_vote < 0 { 0 } else { -1 })),
                            Ico { icon: Icon::ThumbDown }
                        }
                    }
                    div { class: "row-actions",
                        DownloadLink { path: p.file_url.clone(), name: format!("{}.{ext}", p.sha256), label: t!("post-download") }
                        if !p.deleted {
                            button { class: "icon-btn", aria_label: t!("share-button"), title: t!("share-button"), onclick: move |_| sharing.set(true),
                                Ico { icon: Icon::Share }
                            }
                            button { class: "icon-btn", aria_label: t!("post-edit"), title: t!("post-edit-tags"), onclick: move |_| start_edit(),
                                Ico { icon: Icon::Edit }
                            }
                            if let Some(kind) = editable {
                                button { class: "icon-btn", aria_label: t!("editor-open"), title: t!("editor-open"), onclick: move |_| media_editing.set(true),
                                    Ico { icon: if kind == EditKind::Video { Icon::Cut } else { Icon::Crop } }
                                }
                            }
                            button { class: "icon-btn danger", aria_label: t!("post-delete"), title: t!("post-delete-title"),
                                disabled: busy(), onclick: move |_| act(Op::Delete),
                                Ico { icon: Icon::Delete }
                            }
                        }
                    }
                }
                if sharing() {
                    ShareSheet { post_ids: vec![id], onclose: move |()| sharing.set(false) }
                }
                if let Some(kind) = editable.filter(|_| media_editing()) {
                    EditorSheet {
                        post: p.clone(),
                        kind,
                        file_url: file_url.clone(),
                        onclose: move |()| media_editing.set(false),
                        onsaved: move |new_id| {
                            media_editing.set(false);
                            let q = current.peek().1.clone();
                            navigator().push(Route::PostView { id: new_id, q });
                        },
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
                            span { {t!("post-tags")} }
                            textarea {
                                rows: "5",
                                spellcheck: "false",
                                value: "{tag_text}",
                                oninput: move |e| tag_text.set(e.value()),
                            }
                        }
                        RatingPicker { value: rating() , onchange: move |r| rating.set(r) }
                        div { class: "row-end",
                            button { class: "btn text", r#type: "button", onclick: move |_| editing.set(false), {t!("cancel")} }
                            button { class: "btn", r#type: "submit", disabled: busy(), Ico { icon: Icon::Check } {t!("save")} }
                        }
                    }
                } else {
                    div { class: "card",
                        h2 { class: "card-title", {t!("post-tags")} }
                        if p.tags.is_empty() {
                            p { class: "muted", {t!("post-no-tags")} }
                        }
                        div { class: "chips",
                            for tag in p.tags.iter() {
                                Link { key: "{tag}", class: "chip", to: Route::Gallery { q: tag.clone() }, "{tag}" }
                            }
                        }
                    }
                }
                div { class: "card",
                    h2 { class: "card-title", {t!("post-details")} }
                    dl { class: "meta",
                        dt { {t!("rating")} } dd { {rating_label(p.rating)} }
                        dt { {t!("post-size")} } dd { "{human_size(p.file_size)}" }
                        dt { {t!("post-dimensions")} } dd { "{dims}" }
                        if let Some(d) = duration {
                            dt { {t!("post-length")} } dd { "{d}" }
                        }
                        dt { {t!("post-type")} } dd { "{p.mime}" }
                        dt { {t!("post-added")} } dd { "{added}" }
                        if let Some(source) = &p.source {
                            dt { {t!("post-source")} } dd { class: "break", "{source}" }
                        }
                    }
                    if sample.is_some() {
                        div { class: "sample-note",
                            p { class: "muted hint", {t!("post-sample-note")} }
                            button { class: "btn text", onclick: move |_| original.set(true), {t!("post-show-original")} }
                        }
                    }
                }
                Related {
                    id: p.id,
                    onopen: move |near: Post| {
                        *OPENING.write() = Some(near.clone());
                        entered.set(None);
                        document::eval(SCROLL_TOP);
                        // Pushed, so Back returns to this post.
                        navigator().push(Route::PostView { id: near.id, q: String::new() });
                    },
                }
            }
        }
    }
}

/// Posts the viewer suggests under the details.
const RELATED_SHOWN: u32 = 12;

/// The viewer starts at the top when another post opens from below.
const SCROLL_TOP: &str = r#"
const smooth = !matchMedia("(prefers-reduced-motion: reduce)").matches;
window.scrollTo({ top: 0, behavior: smooth ? "smooth" : "auto" });
"#;

/// Posts like this one (shared rare tags, or a look-alike), as small
/// thumbnails. Nothing shows until there are some.
#[component]
fn Related(id: i64, onopen: EventHandler<Post>) -> Element {
    let related = use_resource(use_reactive!(|id| async move {
        let client = store::client()?;
        client.related(id, RELATED_SHOWN).await.ok()
    }));
    let posts = related
        .read()
        .clone()
        .flatten()
        .map(|r| r.posts)
        .unwrap_or_default();
    if posts.is_empty() {
        return rsx! {};
    }
    let client = store::client();
    rsx! {
        section { class: "card related",
            h2 { class: "card-title", {t!("post-related")} }
            div { class: "related-grid",
                for near in posts {
                    {
                        let src = near
                            .thumb_url
                            .as_deref()
                            .and_then(|u| client.as_ref().map(|c| cache::url(c, u)));
                        let label = if near.tags.is_empty() {
                            t!("post-alt", id = near.id)
                        } else {
                            near.tags.join(" ")
                        };
                        let id = near.id;
                        rsx! {
                            Link {
                                key: "{id}",
                                class: "thumb",
                                to: Route::PostView { id, q: String::new() },
                                title: "{label}",
                                onclick: move |_| onopen.call(near.clone()),
                                onclick_only: true,
                                if let Some(src) = src {
                                    img { src, alt: "{label}", loading: "lazy", decoding: "async" }
                                } else {
                                    div { class: "thumb-placeholder", aria_label: "{label}",
                                        Ico { icon: Icon::Play, size: 24 }
                                    }
                                }
                            }
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
        div { class: "segmented", role: "radiogroup", aria_label: t!("rating"),
            for r in Rating::ALL {
                button {
                    key: "{r.as_str()}",
                    r#type: "button",
                    role: "radio",
                    aria_checked: if r == value { "true" } else { "false" },
                    class: if r == value { "seg on" } else { "seg" },
                    onclick: move |_| onchange.call(r),
                    {rating_label(r)}
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

/// The URL the viewer shows: the sample when there is one, unless the
/// original is wanted.
fn shown_url(p: &Post, original: bool) -> &str {
    match &p.sample_url {
        Some(s) if !original && !store::PREFS.peek().originals => s,
        _ => &p.file_url,
    }
}

/// Whether the sample, which fits in `SAMPLE_SIZE`², has every pixel the
/// screen can show of an image of `natural` size fitted into a stage of
/// `stage` CSS pixels, at `dpr` device pixels per CSS pixel. The image is
/// never enlarged past its natural CSS size, and the original can't show
/// more than its own pixels, so a sample of a small image always is.
fn sample_is_sharp(natural: (i32, i32), stage: (f64, f64), dpr: f64) -> bool {
    let (w, h) = (f64::from(natural.0), f64::from(natural.1));
    if w <= 0.0 || h <= 0.0 || stage.0 <= 0.0 || stage.1 <= 0.0 {
        return true;
    }
    // Shares of the original's pixels: shown on screen, and in the sample.
    let shown = ((stage.0 / w).min(stage.1 / h).min(1.0) * dpr).min(1.0);
    let sample = (f64::from(SAMPLE_SIZE) / w.max(h)).min(1.0);
    sample >= shown
}

/// The stage's `src`. Images come through the media cache, except
/// originals too big to keep; videos stream from the server.
fn stage_url(client: &Client, p: &Post, original: bool) -> String {
    let path = shown_url(p, original);
    let small = u64::try_from(p.file_size).is_ok_and(|n| n <= cache::MAX_FILE_BYTES);
    if p.kind == MediaKind::Video || (path == p.file_url && !small) {
        client.absolute(path)
    } else {
        cache::url(client, path)
    }
}

/// The entrance after previous/next: from the side the post came from.
fn enter_class(next: Option<bool>) -> &'static str {
    match next {
        Some(true) => "from-next",
        Some(false) => "from-prev",
        None => "",
    }
}

/// Sizes the image box to the fitted image before it loads, so the
/// thumbnail can stand in and nothing jumps when the image arrives.
/// `--fit-h` is the stage's height limit.
fn fit_style(size: Option<(i32, i32)>) -> String {
    match size {
        Some((w, h)) if w > 0 && h > 0 => format!(
            "; aspect-ratio: {w} / {h}; width: min(100%, calc(var(--fit-h) * {w} / {h}), {w}px)"
        ),
        _ => String::new(),
    }
}

/// `url("…")` for inline CSS.
fn css_url(url: &str) -> String {
    format!(
        "url(\"{}\")",
        url.replace('\\', "\\\\").replace('"', "\\\"")
    )
}

/// The viewer's image. Until it loads, the thumbnail stretched over its
/// box stands in for it.
#[component]
fn StageImage(
    src: String,
    thumb: Option<String>,
    alt: String,
    size: Option<(i32, i32)>,
    transform: String,
    enter: &'static str,
) -> Element {
    let mut loaded = use_signal(|| false);
    let mut style = transform;
    style.push_str(&fit_style(size));
    if let Some(t) = thumb.as_deref() {
        style.push_str("; --thumb: ");
        style.push_str(&css_url(t));
    }
    rsx! {
        img {
            id: "stage-img",
            class: if loaded() { "{enter}" } else { "{enter} loading" },
            src: "{src}",
            alt: "{alt}",
            draggable: "false",
            style,
            onload: move |_| loaded.set(true),
            onmounted: move |_| {
                document::eval(HERO_DONE);
                // A cached image can finish before the listener exists.
                spawn(async move {
                    let complete = document::eval(
                        "return document.getElementById('stage-img')?.complete === true",
                    )
                    .join::<bool>()
                    .await;
                    if complete.unwrap_or(false) {
                        loaded.set(true);
                    }
                });
            },
        }
    }
}

/// The media, with zoom and pan for images, previous/next and fullscreen.
#[component]
fn Stage(
    id: i64,
    kind: MediaKind,
    src: String,
    thumb: Option<String>,
    size: Option<(i32, i32)>,
    enter: &'static str,
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
                    video { key: "{id}", class: enter, src: "{src}", controls: true, autoplay: !power::active(), r#loop: true, playsinline: true,
                        onmounted: move |_| { document::eval(HERO_DONE); },
                    }
                },
                _ => rsx! {
                    StageImage { key: "{id}", src, thumb, alt, size, transform: z.css(), enter }
                },
            }
            if let Some(prev) = nav.prev {
                button { class: "stage-btn stage-prev", title: t!("post-prev-title"), aria_label: t!("post-prev"),
                    onclick: move |_| onnav.call(prev),
                    Ico { icon: Icon::ChevronLeft, size: 32 }
                }
            }
            if let Some(next) = nav.next {
                button { class: "stage-btn stage-next", title: t!("post-next-title"), aria_label: t!("post-next"),
                    onclick: move |_| onnav.call(next),
                    Ico { icon: Icon::ChevronRight, size: 32 }
                }
            }
            div { class: "stage-tools",
                if zoomable {
                    button { class: "stage-btn", title: t!("zoom-out-title"), aria_label: t!("zoom-out"),
                        disabled: !z.is_zoomed(),
                        onclick: move |_| zoom.set(zoom().zoom_at(1.0 / STEP, (0.0, 0.0), size_of(*rect.peek()))),
                        Ico { icon: Icon::ZoomOut }
                    }
                    button { class: "stage-zoom", title: t!("zoom-fit-title"), aria_label: t!("zoom-reset"),
                        onclick: move |_| zoom.set(Zoom::default()),
                        "{z.percent()}%"
                    }
                    button { class: "stage-btn", title: t!("zoom-in-title"), aria_label: t!("zoom-in"),
                        disabled: z.scale >= crate::zoom::MAX_SCALE,
                        onclick: move |_| zoom.set(zoom().zoom_at(STEP, (0.0, 0.0), size_of(*rect.peek()))),
                        Ico { icon: Icon::ZoomIn }
                    }
                }
                button {
                    class: "stage-btn",
                    title: if full() { t!("fullscreen-exit-title") } else { t!("fullscreen-title") },
                    aria_label: if full() { t!("fullscreen-exit") } else { t!("fullscreen") },
                    onclick: move |_| onfull.call(!full()),
                    Ico { icon: if full() { Icon::FullscreenExit } else { Icon::Fullscreen } }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sizes_the_box_before_loading() {
        assert_eq!(
            fit_style(Some((3600, 2400))),
            "; aspect-ratio: 3600 / 2400; width: min(100%, calc(var(--fit-h) * 3600 / 2400), 3600px)"
        );
        // Unknown or nonsense sizes leave the box to the image.
        assert_eq!(fit_style(None), "");
        assert_eq!(fit_style(Some((0, 10))), "");
        assert_eq!(fit_style(Some((10, -1))), "");
    }

    #[test]
    fn swaps_in_the_original_only_when_the_screen_has_more_pixels() {
        let photo = (6000, 4000);
        // A laptop window, and a phone held upright: the sample is enough.
        assert!(sample_is_sharp(photo, (900.0, 740.0), 1.0));
        assert!(sample_is_sharp(photo, (390.0, 520.0), 3.0));
        // Fullscreen on a 4K screen, or a dense laptop's: more than 2560 wide.
        assert!(!sample_is_sharp(photo, (3840.0, 2160.0), 1.0));
        assert!(!sample_is_sharp(photo, (1512.0, 982.0), 2.0));
        // Small enough that the sample keeps every pixel.
        assert!(sample_is_sharp((2400, 1600), (3840.0, 2160.0), 2.0));
        // Shown no bigger than itself: 3000 px shown at most 3000 device px.
        assert!(!sample_is_sharp((3000, 1000), (3000.0, 2000.0), 1.0));
        assert!(sample_is_sharp((3000, 1000), (1200.0, 2000.0), 2.0));
        // Not measured yet, or no size.
        assert!(sample_is_sharp(photo, (0.0, 0.0), 2.0));
        assert!(sample_is_sharp((0, 0), (900.0, 700.0), 1.0));
    }

    #[test]
    fn quotes_css_urls() {
        assert_eq!(
            css_url("/media/thumb/a.jpg?x=1"),
            r#"url("/media/thumb/a.jpg?x=1")"#
        );
        assert_eq!(css_url(r#"a"b\c"#), r#"url("a\"b\\c")"#);
    }

    #[test]
    fn enters_from_the_side_it_came_from() {
        assert_eq!(enter_class(Some(true)), "from-next");
        assert_eq!(enter_class(Some(false)), "from-prev");
        assert_eq!(enter_class(None), "");
    }
}
