use std::collections::HashMap;

use dioxus::core::Task;
use dioxus::html::geometry::{ClientPoint, PixelsRect};
use dioxus::prelude::*;
use starbooru_client::{Client, ClientError};
use starbooru_core::{
    AiStatus, EditKind, MediaKind, Neighbors, PanelTranslation, Post, PostUpdate, Rating,
    SAMPLE_SIZE, TagCorrection, TagSuggestion, TagSuggestions, TextBlock, edit_kind, normalize_tag,
};

use super::editor::{EditorSheet, Saved};
use super::gallery::{forget_lists, forget_post};
use super::people::People;
use super::share::{ShareSheet, copy_text};
use super::shell::{notify, notify_undo};
use super::tags::TagGroups;
use super::{ErrorBanner, Spinner};
use crate::Route;
use crate::cache;
use crate::download::DownloadLink;
use crate::i18n::LANG;
use crate::icons::{Ico, Icon};
use crate::motion;
use crate::power;
use crate::store;
use crate::util::{format_duration, human_size, rating_label};
use crate::zoom::{STEP, Zoom, wheel_factor};

/// Keys the viewer handles, sent from a page-wide listener. Typing in a
/// field never triggers them. In fullscreen the controls rest a moment after the
/// last sign of anyone using them, and any pointer or key brings them back.
///
/// The wheel gets past the picture too: rolling down, or up from further down
/// the page, moves the page, and `stopPropagation` keeps that from the zoom. One
/// gesture stays one thing to its end, so a trackpad's coasting doesn't turn a
/// scroll into a zoom when the page gets to the top.
const KEY_LISTENER: &str = r#"
const REST_ON = ['pointermove', 'pointerdown', 'keydown'];
if (window.__sbKeys) window.removeEventListener('keydown', window.__sbKeys);
if (window.__sbFs) document.removeEventListener('fullscreenchange', window.__sbFs);
for (const type of REST_ON) if (window.__sbWake) window.removeEventListener(type, window.__sbWake);
let resting = 0;
window.__sbWake = () => {
  document.querySelector('.stage')?.removeAttribute('data-rest');
  clearTimeout(resting);
  resting = setTimeout(() => {
    // A touch leaves `:hover` on whatever it last tapped, so only a real pointer counts.
    const held = matchMedia('(hover: hover)').matches ? ':focus-visible, .stage-btn:hover, .stage-tools:hover' : ':focus-visible';
    document.querySelector(`.stage.full:not(:has(${held}))`)?.setAttribute('data-rest', '');
  }, 2800);
};
for (const type of REST_ON) window.addEventListener(type, window.__sbWake, { passive: true });
if (window.__sbWheel) window.removeEventListener('wheel', window.__sbWheel, true);
let quiet = 0;
let scrolls = false;
window.__sbWheel = (e) => {
  const stage = e.target.closest?.('.stage');
  if (!stage) return;
  if (e.timeStamp > quiet) scrolls = !e.ctrlKey && !stage.matches('.full, .zoomed') && (e.deltaY > 0 || scrollY > 0);
  quiet = e.timeStamp + 300;
  if (scrolls) e.stopPropagation();
};
window.addEventListener('wheel', window.__sbWheel, { capture: true, passive: true });
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
for (const type of ['pointermove', 'pointerdown', 'keydown']) if (window.__sbWake) window.removeEventListener(type, window.__sbWake);
window.__sbWake = null;
if (window.__sbWheel) window.removeEventListener('wheel', window.__sbWheel, true);
window.__sbWheel = null;
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

/// Nothing pressed for this long, and the user has stopped on a post. Until then
/// only what it takes to flip through loads.
const SETTLE_MS: u32 = 250;

/// Presses of previous or next that can wait for the posts to be known. Holding
/// a key down shouldn't leave the viewer walking on long after it's let go.
const MOST_QUEUED: i32 = 6;

/// The queue of presses after one more: the other way cancels one out.
fn queued(steps: i32, forward: bool) -> i32 {
    let step = if forward { 1 } else { -1 };
    steps.saturating_add(step).clamp(-MOST_QUEUED, MOST_QUEUED)
}

/// Where the next press in the queue goes.
fn step_target(steps: i32, n: Neighbors) -> Option<i64> {
    if steps > 0 { n.next } else { n.prev }
}

/// The post a gallery thumbnail is opening, so the viewer can show it
/// before the server answers.
pub static OPENING: GlobalSignal<Option<Post>> = Signal::global(|| None);

#[component]
pub fn PostView(id: i64, q: String) -> Element {
    // In place from the first render: an empty page in between would flash.
    let mut post = use_signal(|| OPENING.peek().clone().filter(|p| p.id == id));
    let mut error = use_signal(|| None::<String>);
    let mut busy = use_signal(|| false);
    let mut editing = use_signal(|| false);
    let mut sharing = use_signal(|| false);
    let mut media_editing = use_signal(|| false);
    let mut tag_text = use_signal(String::new);
    // The tag text as of the last finished word, for "did you mean".
    let mut tag_check = use_signal(String::new);
    let mut rating = use_signal(Rating::default);

    // The props as signals, for the key handler, which outlives a render.
    let mut current = use_signal(|| (id, q.clone()));
    // One answer for all of the viewer, rather than one per post flipped past.
    let ai = use_resource(|| async move {
        let client = store::client()?;
        client.ai_status().await.ok()
    });
    use_context_provider(|| ai);
    // Presses of previous/next not acted on yet: next counts up, previous down.
    let mut steps = use_signal(|| 0_i32);
    // A turn is on its way to the router.
    let mut moving = use_signal(|| false);
    // Pressing again before the last press has settled: flipping through, so
    // pictures come without their entrance and with only their thumbnail.
    let mut rushing = use_signal(|| false);
    // The post the user has stopped on, the one worth the rest of what it has.
    let mut settled = use_signal(|| id);
    let mut settling = use_hook(|| CopyValue::new(None::<Task>));
    // A load is running, and the post the last one found out about.
    let mut loading = use_hook(|| CopyValue::new(false));
    let mut fresh = use_hook(|| CopyValue::new(None::<i64>));

    let mut handle = move |result: Result<Post, ClientError>| match result {
        Ok(p) => {
            post.set(Some(p));
            error.set(None);
        }
        Err(e) if e.is_unauthorized() => store::forget_session(),
        Err(e) => {
            // The post before it stays while this one loads, but not if it won't.
            if post
                .peek()
                .as_ref()
                .is_some_and(|p| p.id != current.peek().0)
            {
                post.set(None);
            }
            error.set(Some(e.to_string()));
        }
    };

    let reload = move |id: i64| {
        let Some(client) = store::client() else {
            return;
        };
        spawn(async move {
            let result = client.post(id).await;
            // The user may have moved on while it loaded.
            if current.peek().0 == id {
                handle(result);
            }
        });
    };
    // A post not known yet. Only ever one at a time, always the one the viewer has
    // got to: the pictures flipped past show as they come, and the last one always
    // gets its turn.
    let mut load = move || {
        let Some(client) = store::client() else {
            return;
        };
        if *loading.peek() {
            return;
        }
        loading.set(true);
        spawn(async move {
            loop {
                let asked = current.peek().0;
                let result = client.post(asked).await;
                if current.peek().0 == asked {
                    fresh.set(Some(asked));
                    handle(result);
                    break;
                }
                if let Ok(p) = result {
                    post.set(Some(p));
                }
            }
            loading.set(false);
        });
    };
    let mut zoom = use_signal(Zoom::default);
    // The post whose original is wanted instead of the sample: zooming in asks
    // for the detail only the original has.
    let mut original = use_signal(|| None::<i64>);
    use_effect(move || {
        if zoom().is_zoomed() {
            original.set(post.peek().as_ref().map(|p| p.id));
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
    // Neighbours fetched ahead, so previous/next shows at once.
    let mut ahead = use_signal(HashMap::<i64, Post>::new);
    // Which way the last previous/next went: `Some(true)` for next.
    let mut entered = use_signal(|| None::<bool>);
    // First, as what follows goes by it.
    use_effect(use_reactive!(|id, q| {
        current.set((id, q));
        moving.set(false);
    }));
    use_effect(use_reactive!(|id| {
        let known = ahead.peek().get(&id).cloned();
        let opening = OPENING.write().take().filter(|p| p.id == id);
        fresh.set(None);
        // Not known yet: the post before it stays until it is, as a spinner
        // in between would throw out the whole page.
        let shown = known.or(opening);
        let have = shown.is_some();
        if let Some(p) = shown {
            post.set(Some(p));
        } else {
            load();
        }
        editing.set(false);
        media_editing.set(false);
        zoom.set(Zoom::default());
        if let Some(task) = settling.take() {
            task.cancel();
        }
        // Where the viewer opens, there's nothing to wait for.
        if *settled.peek() == id && !*rushing.peek() {
            if have {
                reload(id);
            }
            return;
        }
        settling.set(Some(spawn(async move {
            loop {
                crate::activity::wait_ms(SETTLE_MS).await;
                if *steps.peek() == 0 && !*moving.peek() {
                    break;
                }
            }
            rushing.set(false);
            settled.set(id);
            if *fresh.peek() != Some(id) && !*loading.peek() {
                reload(id);
            }
        })));
    }));

    // Said which post they're for: until they've come, they're the last post's.
    let neighbors = use_resource(move || async move {
        let (id, q) = current();
        let client = store::client()?;
        Some((id, client.neighbors(id, &q).await.ok()))
    });
    let links = move || {
        (*neighbors.read())
            .flatten()
            .filter(|(at, _)| *at == current.read().0)
            .map(|(_, n)| n.unwrap_or_default())
    };
    use_effect(move || {
        let Some(n) = links() else {
            return;
        };
        let here = current.peek().0;
        // Flipping through, the next ones aren't worth fetching yet.
        if settled() != here {
            return;
        }
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
                    let shown = (p.kind != MediaKind::Video).then(|| {
                        let dpr = (*dpr.peek()).flatten();
                        stage_url(&client, &p, needs_original(&p, *rect.peek(), dpr))
                    });
                    let thumb = p.thumb_url.as_deref().map(|u| cache::url(&client, u));
                    motion::ahead(&shown.into_iter().chain(thumb).collect::<Vec<_>>());
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
    let mut press = move |forward: bool| {
        if *moving.peek() || *steps.peek() != 0 || *settled.peek() != current.peek().0 {
            rushing.set(true);
        }
        let now = *steps.peek();
        steps.set(queued(now, forward));
    };
    // Acts on the presses one at a time, as soon as it's known where each one goes.
    use_effect(move || {
        let want = steps();
        if want == 0 || moving() {
            return;
        }
        let Some(n) = links() else {
            return;
        };
        let here = current.peek().0;
        // The end of the list.
        let Some(id) = step_target(want, n).filter(|to| *to != here) else {
            steps.set(0);
            return;
        };
        steps.set(want - want.signum());
        moving.set(true);
        spawn(async move {
            motion::hold().await;
            entered.set(Some(want > 0));
            let q = current.peek().1.clone();
            // Replace, so Back returns to the gallery rather than through every post.
            navigator().replace(Route::PostView { id, q });
        });
    });
    let mut set_full = move |on: bool| {
        full.set(on);
        browser_fullscreen(on);
    };
    // Where the arrow goes: where you came from, or the gallery if this is the
    // first page. The web build always says it can go back, so that one finds out
    // by trying.
    let step_back = move || {
        let nav = navigator();
        let q = current.peek().1.clone();
        if !nav.can_go_back() {
            nav.push(Route::Gallery { q });
            return;
        }
        let here = router().full_route_string();
        nav.go_back();
        spawn(async move {
            crate::activity::wait_ms(250).await;
            if router().full_route_string() == here {
                navigator().push(Route::Gallery { q });
            }
        });
    };
    let mut start_edit = move || {
        if let Some(p) = post.peek().as_ref().filter(|p| p.id == current.peek().0) {
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
                "ArrowLeft" => press(false),
                "ArrowRight" => press(true),
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
        motion::leave();
    });

    // Runs one mutation, then shows the post the server returns.
    let mut act = move |op: Op| {
        // What's on the page is still the post before this one.
        if busy() || post.peek().as_ref().is_some_and(|p| p.id != id) {
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
                    Ok(()) => {
                        forget_post(id);
                        client.post(id).await
                    }
                    Err(e) => Err(e),
                },
                Op::Restore => {
                    forget_lists();
                    client.restore_post(id).await
                }
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

    let known = links();
    let client = store::client();
    let abs = |u: &str| client.as_ref().map(|c| c.absolute(u)).unwrap_or_default();
    let file_url = abs(&p.file_url);
    let shown_id = p.id;
    // Right from the first render, as changing it later loads the picture twice.
    let show_original =
        original() == Some(p.id) || needs_original(&p, rect(), (*dpr.read()).flatten());
    let sample = p
        .sample_url
        .as_deref()
        .filter(|_| !show_original && !store::PREFS.read().originals);
    let src = client
        .as_ref()
        .map(|c| stage_url(c, &p, show_original))
        .unwrap_or_default();
    let thumb = p
        .thumb_url
        .as_deref()
        .and_then(|u| client.as_ref().map(|c| cache::url(c, u)));
    let ext = p.mime.rsplit('/').next().unwrap_or("bin").to_owned();
    let alt = p.tags.join(" ");
    let added = p.created_at.format("%Y-%m-%d %H:%M").to_string();
    let file_date = p.file_date.map(|d| d.format("%Y-%m-%d %H:%M").to_string());
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
        div { class: if full() { "page post-layout full-open" } else { "page post-layout" },
            PostGlow { thumb: thumb.clone() }
            div { class: "post-main",
                Stage {
                    id: p.id,
                    kind: p.kind,
                    src,
                    thumb,
                    size: p.width.zip(p.height),
                    enter: enter_class(entered(), rushing()),
                    passing: rushing(),
                    ready: settled() == p.id,
                    alt,
                    zoom,
                    full,
                    // Until the post's neighbours are known, either way may be open.
                    first: known.is_some_and(|n| n.prev.is_none()),
                    last: known.is_some_and(|n| n.next.is_none()),
                    onstep: press,
                    onfull: set_full,
                    onback: move |()| step_back(),
                    rect,
                }
                // What you do with the post, apart from how you look at it: an
                // opinion, what leaves with the file, and the one that removes it.
                // `act` ignores a press while another is running, so nothing here
                // greys out for the moment a request takes.
                div { class: "post-bar glass float", role: "toolbar", aria_label: t!("post-actions"),
                    div { class: "vote",
                        button {
                            class: if p.my_vote > 0 { "icon-btn on" } else { "icon-btn" },
                            aria_label: t!("post-upvote"),
                            aria_pressed: "{p.my_vote > 0}",
                            disabled: p.deleted,
                            onclick: move |_| act(Op::Vote(if p.my_vote > 0 { 0 } else { 1 })),
                            Ico { icon: Icon::ThumbUp }
                        }
                        span { class: "score", "{p.score}" }
                        button {
                            class: if p.my_vote < 0 { "icon-btn on" } else { "icon-btn" },
                            aria_label: t!("post-downvote"),
                            aria_pressed: "{p.my_vote < 0}",
                            disabled: p.deleted,
                            onclick: move |_| act(Op::Vote(if p.my_vote < 0 { 0 } else { -1 })),
                            Ico { icon: Icon::ThumbDown }
                        }
                    }
                    div { class: "bar-acts",
                        DownloadLink { path: p.file_url.clone(), name: format!("{}.{ext}", p.sha256), label: t!("post-download") }
                        if !p.deleted {
                            button { class: "icon-btn", aria_label: t!("share-button"), title: t!("share-button"), onclick: move |_| sharing.set(true),
                                Ico { icon: Icon::Share }
                                span { class: "bar-label", {t!("share-button")} }
                            }
                            if let Some(kind) = editable {
                                button { class: "icon-btn", aria_label: t!("editor-open"), title: t!("editor-open"), onclick: move |_| media_editing.set(true),
                                    Ico { icon: if kind == EditKind::Video { Icon::Cut } else { Icon::Crop } }
                                    span { class: "bar-label",
                                        if kind == EditKind::Video { {t!("editor-video-short")} } else { {t!("editor-image-short")} }
                                    }
                                }
                            }
                        }
                    }
                    if !p.deleted {
                        button { class: "icon-btn danger bar-delete", aria_label: t!("post-delete"), title: t!("post-delete-title"),
                            onclick: move |_| act(Op::Delete),
                            Ico { icon: Icon::Delete }
                        }
                    }
                }
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
                // What's there stays until the user stops on a post.
                Related {
                    id: settled(),
                    onopen: move |near: Post| {
                        *OPENING.write() = Some(near.clone());
                        entered.set(None);
                        document::eval(SCROLL_TOP);
                        // Pushed, so Back returns to this post.
                        navigator().push(Route::PostView { id: near.id, q: String::new() });
                    },
                }
            }
            aside { class: "side",
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
                                oninput: move |e| {
                                    let text = e.value();
                                    if text.ends_with(char::is_whitespace) {
                                        tag_check.set(text.clone());
                                    }
                                    tag_text.set(text);
                                },
                                onblur: move |_| tag_check.set(tag_text()),
                            }
                        }
                        p { class: "muted hint", {t!("post-tags-hint")} }
                        DidYouMean { tag_text, tag_check, saved: p.tags.clone() }
                        TagSuggester { id, tag_text, rating }
                        RatingPicker { value: rating() , onchange: move |r| rating.set(r) }
                        div { class: "row-end",
                            button { class: "btn text", r#type: "button", onclick: move |_| editing.set(false), {t!("cancel")} }
                            button { class: "btn", r#type: "submit", disabled: busy(), Ico { icon: Icon::Check } {t!("save")} }
                        }
                    }
                } else {
                    div { class: "card",
                        div { class: "card-head",
                            h2 { class: "card-title", {t!("post-tags")} }
                            if !p.deleted {
                                button { class: "btn text small", title: t!("post-edit-tags"), onclick: move |_| start_edit(),
                                    Ico { icon: Icon::Edit, size: 18 }
                                    {t!("post-edit")}
                                }
                            }
                        }
                        if p.tags.is_empty() {
                            p { class: "muted", {t!("post-no-tags")} }
                        }
                        TagGroups { details: p.tag_details.clone(), names: p.tags.clone() }
                    }
                }
                People {
                    key: "{p.id}",
                    id: p.id,
                    figures: p.figures.clone(),
                    deleted: p.deleted,
                    onsaved: move |saved: Post| handle(Ok(saved)),
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
                        if let Some(date) = file_date {
                            dt { {t!("post-file-date")} } dd { "{date}" }
                        }
                        if !p.palette.is_empty() {
                            dt { {t!("post-colors")} }
                            dd { class: "post-swatches",
                                for hex in p.palette.clone() {
                                    Link {
                                        key: "{hex}",
                                        class: "swatch",
                                        style: "background-color: {hex}",
                                        title: t!("post-color-search", color = hex.as_str()),
                                        to: Route::Gallery { q: format!("color:{}", hex.trim_start_matches('#')) },
                                    }
                                }
                            }
                        }
                        dt { {t!("post-added")} } dd { "{added}" }
                        if let Some(source) = &p.source {
                            dt { {t!("post-source")} } dd { class: "break", "{source}" }
                        }
                    }
                    if sample.is_some() {
                        div { class: "sample-note",
                            p { class: "muted hint", {t!("post-sample-note")} }
                            button { class: "btn text", onclick: move |_| original.set(Some(shown_id)), {t!("post-show-original")} }
                        }
                    }
                }
                if let Some(text) = p.ocr_text.clone().filter(|t| !t.trim().is_empty()) {
                    OcrText { key: "{p.id}", id: p.id, text }
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
                    onsaved: move |saved: Saved| {
                        media_editing.set(false);
                        let original = id;
                        if saved.binned {
                            forget_post(original);
                        }
                        forget_lists();
                        let q = current.peek().1.clone();
                        navigator().push(Route::PostView { id: saved.id, q: q.clone() });
                        let message = if saved.binned { t!("editor-saved-binned") } else { t!("editor-saved") };
                        notify_undo(message, t!("editor-undo-save"), move || undo_edit(original, saved, q.clone()));
                    },
                }
            }
        }
    }
}

/// Takes an edit back from the notice after saving: the original comes out of
/// the recycle bin, the edited copy is deleted, and the original shows again.
fn undo_edit(original: i64, saved: Saved, q: String) {
    let Some(client) = store::client() else {
        return;
    };
    spawn(async move {
        if saved.binned
            && let Err(e) = client.restore_post(original).await
        {
            notify(e.to_string());
            return;
        }
        // The copy goes for good: left in the recycle bin, it would make the
        // same edit a duplicate the next time. If the purge fails, it just
        // stays in the bin.
        match client.delete_post(saved.id).await {
            Ok(()) => {
                let _ = client.purge_post(saved.id).await;
            }
            Err(e) => notify(e.to_string()),
        }
        forget_post(saved.id);
        forget_lists();
        navigator().push(Route::PostView { id: original, q });
    });
}

/// The picture's own colours, blurred, as the light the viewer stands in, and all
/// there is around the picture in fullscreen. Two layers take turns: see
/// `motion::glow`. The battery saver leaves it out.
#[component]
fn PostGlow(thumb: Option<String>) -> Element {
    use_effect(use_reactive!(|thumb| motion::glow(thumb.as_deref())));
    rsx! {
        div { class: "post-glow", aria_hidden: "true",
            canvas { class: "glow-layer" }
            canvas { class: "glow-layer" }
        }
    }
}

/// OCR text longer than this many characters or lines starts folded.
const OCR_FOLD_CHARS: usize = 280;
const OCR_FOLD_LINES: usize = 6;

/// Which AI tools the server offers, for showing AI buttons.
pub(super) fn use_ai_status() -> AiStatus {
    try_use_context::<Resource<Option<AiStatus>>>()
        .and_then(|status| status.read().clone().flatten())
        .unwrap_or_default()
}

/// "Text in image": what OCR read in the picture, with a copy button and,
/// when the server has an AI provider, a translation into the app's
/// language. Long text starts folded.
#[component]
fn OcrText(id: i64, text: String) -> Element {
    let copied = use_signal(|| false);
    let ai = use_ai_status().translation;
    let mut translation = use_signal(|| None::<String>);
    let mut translated = use_signal(|| false);
    let mut busy = use_signal(|| false);
    let mut error = use_signal(|| None::<String>);
    let shown = match (translated(), translation()) {
        (true, Some(t)) => t,
        _ => text.clone(),
    };
    let long = shown.chars().count() > OCR_FOLD_CHARS || shown.lines().count() > OCR_FOLD_LINES;
    let mut open = use_signal(|| false);
    let folded = long && !open();
    let copy = shown.clone();
    let translate = move |_| {
        if translation.peek().is_some() {
            translated.toggle();
            return;
        }
        let Some(client) = store::client() else {
            return;
        };
        let to = LANG.peek().code();
        busy.set(true);
        error.set(None);
        spawn(async move {
            match client.translate_text(id, to).await {
                Ok(t) => {
                    translation.set(Some(t.text));
                    translated.set(true);
                }
                Err(e) => error.set(Some(e.to_string())),
            }
            busy.set(false);
        });
    };
    rsx! {
        div { class: "card ocr",
            div { class: "card-head",
                h2 { class: "card-title", {t!("post-ocr-title")} }
                button {
                    class: "btn tonal small",
                    r#type: "button",
                    onclick: move |_| copy_text(copy.clone(), copied),
                    Ico { icon: if copied() { Icon::Check } else { Icon::Copy }, size: 18 }
                    if copied() { {t!("post-ocr-copied")} } else { {t!("post-ocr-copy")} }
                }
            }
            if let Some(message) = error() {
                ErrorBanner { message }
            }
            p {
                class: if folded { "ocr-text folded" } else { "ocr-text" },
                lang: if translated() { LANG.read().code() },
                "{shown}"
            }
            if translated() {
                p { class: "muted hint ocr-note", {t!("post-ocr-machine")} }
            }
            if long || ai {
                div { class: "ocr-foot",
                    if long {
                        button {
                            class: "btn text",
                            r#type: "button",
                            aria_expanded: "{open()}",
                            onclick: move |_| open.toggle(),
                            if open() { {t!("post-ocr-less")} } else { {t!("post-ocr-more")} }
                        }
                    }
                    if ai {
                        button {
                            class: "btn text",
                            r#type: "button",
                            disabled: busy(),
                            aria_pressed: "{translated()}",
                            onclick: translate,
                            if busy() {
                                {t!("post-ocr-translating")}
                            } else if translated() {
                                {t!("post-ocr-original")}
                            } else {
                                {t!("post-ocr-translate")}
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Posts the viewer suggests under the picture.
const RELATED_SHOWN: u32 = 12;

/// The viewer starts at the top when another post opens from below.
const SCROLL_TOP: &str = r#"
const smooth = !matchMedia("(prefers-reduced-motion: reduce)").matches;
window.scrollTo({ top: 0, behavior: smooth ? "smooth" : "auto" });
"#;

/// Adds `name` to a whitespace-separated tag text unless it is there.
fn with_tag(text: &str, name: &str) -> String {
    if text.split_whitespace().any(|t| t == name) {
        return text.to_owned();
    }
    let mut out = text.trim_end().to_owned();
    if !out.is_empty() {
        out.push(' ');
    }
    out.push_str(name);
    out
}

/// Whether a word of the tag text is the tag `name`.
fn is_tag(word: &str, name: &str) -> bool {
    normalize_tag(word).is_ok_and(|w| w == name)
}

/// Replaces the tag `from` in a whitespace-separated tag text, without
/// repeating `to` if it is already there.
fn replace_tag(text: &str, from: &str, to: &str) -> String {
    let mut out: Vec<&str> = Vec::new();
    for word in text.split_whitespace() {
        let word = if is_tag(word, from) { to } else { word };
        if !out.contains(&word) {
            out.push(word);
        }
    }
    out.join(" ")
}

/// The words of a tag text that aren't on the post yet.
fn new_words(text: &str, saved: &[String]) -> String {
    text.split_whitespace()
        .filter(|word| !saved.iter().any(|s| s == word))
        .collect::<Vec<_>>()
        .join(" ")
}

/// "Did you mean" in the tag editor: a chip per unknown tag with a likely
/// spelling, which replaces it. It's only a hint, so a failed request shows nothing.
#[component]
fn DidYouMean(tag_text: Signal<String>, tag_check: Signal<String>, saved: Vec<String>) -> Element {
    let fixes = use_resource(move || {
        let saved = saved.clone();
        async move {
            let words = new_words(&tag_check(), &saved);
            let Some(client) = store::client().filter(|_| !words.is_empty()) else {
                return Vec::new();
            };
            client.tag_corrections(&words).await.unwrap_or_default()
        }
    });
    let current = tag_text();
    let shown: Vec<TagCorrection> = fixes
        .read()
        .iter()
        .flatten()
        .filter(|fix| current.split_whitespace().any(|w| is_tag(w, &fix.from)))
        .cloned()
        .collect();
    if shown.is_empty() {
        return rsx! {};
    }
    rsx! {
        div { class: "did-you-mean",
            span { class: "muted", {t!("post-did-you-mean")} }
            div { class: "chips",
                for fix in shown {
                    button {
                        key: "{fix.from}",
                        r#type: "button",
                        class: "chip",
                        title: t!("post-replace-tag", from = fix.from.as_str(), to = fix.to.as_str()),
                        onclick: move |_| {
                            let next = replace_tag(&tag_text.peek(), &fix.from, &fix.to);
                            tag_text.set(next.clone());
                            tag_check.set(next);
                        },
                        "{fix.from} → {fix.to}"
                    }
                }
            }
        }
    }
}

/// "Suggest tags" in the tag editor, shown only when the server has an AI
/// provider. Suggestions are chips: tap one to add it, or add them all.
#[component]
fn TagSuggester(id: i64, tag_text: Signal<String>, rating: Signal<Rating>) -> Element {
    let enabled = use_ai_status().tagging;
    let mut suggestions = use_signal(|| None::<TagSuggestions>);
    let mut busy = use_signal(|| false);
    let mut error = use_signal(|| None::<String>);
    if !enabled {
        return rsx! {};
    }
    let ask = move |_| {
        let Some(client) = store::client() else {
            return;
        };
        busy.set(true);
        error.set(None);
        spawn(async move {
            match client.suggest_tags(id).await {
                Ok(s) => suggestions.set(Some(s)),
                Err(e) => error.set(Some(e.to_string())),
            }
            busy.set(false);
        });
    };
    let current = tag_text();
    let shown: Vec<TagSuggestion> = suggestions
        .read()
        .as_ref()
        .map(|s| {
            s.tags
                .iter()
                .filter(|t| !current.split_whitespace().any(|c| c == t.name))
                .cloned()
                .collect()
        })
        .unwrap_or_default();
    let suggested_rating = suggestions.read().as_ref().and_then(|s| s.rating);
    rsx! {
        div { class: "suggest",
            button { class: "btn text", r#type: "button", disabled: busy(), onclick: ask,
                if busy() { {t!("ai-suggesting")} } else { {t!("ai-suggest")} }
            }
            if let Some(message) = error() {
                ErrorBanner { message }
            }
            if suggestions.read().is_some() {
                if shown.is_empty() && suggested_rating.is_none() {
                    p { class: "muted hint", {t!("ai-no-suggestions")} }
                }
                div { class: "chips",
                    for s in shown.clone() {
                        button {
                            key: "{s.name}",
                            r#type: "button",
                            class: if s.exists { "chip" } else { "chip suggest-new" },
                            onclick: move |_| {
                                let next = with_tag(&tag_text.peek(), &s.name);
                                tag_text.set(next);
                            },
                            "{s.name}"
                            if !s.exists {
                                span { class: "suggest-badge", {t!("ai-new-tag")} }
                            }
                        }
                    }
                }
                div { class: "row-end",
                    if let Some(r) = suggested_rating.filter(|r| *r != rating()) {
                        button {
                            class: "btn text",
                            r#type: "button",
                            onclick: move |_| rating.set(r),
                            {t!("ai-rating", rating = rating_label(r))}
                        }
                    }
                    if !shown.is_empty() {
                        button {
                            class: "btn text",
                            r#type: "button",
                            onclick: move |_| {
                                let mut next = tag_text.peek().clone();
                                for s in &shown {
                                    next = with_tag(&next, &s.name);
                                }
                                tag_text.set(next);
                            },
                            {t!("ai-add-all")}
                        }
                    }
                }
            }
        }
    }
}

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
        section { class: "related",
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

/// Whether the stage (when measured) shows more device pixels of this post
/// than its sample has: fullscreen on a big or dense screen.
fn needs_original(p: &Post, stage: Option<PixelsRect>, dpr: Option<f64>) -> bool {
    let (Some(stage), Some(dpr)) = (stage, dpr) else {
        return false;
    };
    p.kind != MediaKind::Video
        && p.sample_url.is_some()
        && !power::active()
        && p.width
            .zip(p.height)
            .is_some_and(|size| !sample_is_sharp(size, (stage.width(), stage.height()), dpr))
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

/// The entrance after previous/next: from the side the post came from, or none
/// at all when flipping through.
fn enter_class(next: Option<bool>, rushing: bool) -> &'static str {
    match next {
        Some(_) if rushing => "quick",
        Some(true) => "from-next",
        Some(false) => "from-prev",
        None => "",
    }
}

/// Whether the picture has yet to be asked for. One that came while flipping
/// through has only its thumbnail until the user stops, and one that's already
/// there keeps what it has when the flipping starts.
fn use_held_back(passing: bool) -> bool {
    let mut held = use_hook(|| CopyValue::new(passing));
    if !passing && *held.peek() {
        held.set(false);
    }
    *held.peek()
}

/// What the viewer's image shows. A picture held back shows its thumbnail: with
/// no source at all, a browser draws a frame around it as a broken image. The
/// thumbnail stays up until the file that replaces it has loaded.
fn stage_src(held: bool, src: &str, thumb: Option<&str>) -> Option<String> {
    if held {
        thumb.map(str::to_owned)
    } else {
        Some(src.to_owned())
    }
}

/// Sizes the image box to the fitted image before it loads, so the
/// thumbnail can stand in and nothing jumps when the image arrives.
/// `--fit-h` is the stage's height limit. A picture isn't enlarged past its own
/// size, except where `--fit-cap` lifts that: fullscreen fits the display.
fn fit_style(size: Option<(i32, i32)>) -> String {
    match size {
        Some((w, h)) if w > 0 && h > 0 => format!(
            "; aspect-ratio: {w} / {h}; width: min(100%, calc(var(--fit-h) * {w} / {h}), var(--fit-cap, {w}px))"
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

/// Waits until the viewer's image can paint: a big one has loaded some time
/// before it has decoded.
async fn image_decoded() {
    let _ = document::eval(
        "await document.getElementById('stage-img')?.decode().catch(() => {}); return true",
    )
    .join::<bool>()
    .await;
}

/// The viewer's image. Until it can paint, the thumbnail stretched over its
/// box stands in for it.
#[component]
fn StageImage(
    id: i64,
    src: String,
    thumb: Option<String>,
    alt: String,
    size: Option<(i32, i32)>,
    transform: String,
    enter: &'static str,
    passing: bool,
    onwheel: EventHandler<WheelEvent>,
) -> Element {
    let mut loaded = use_signal(|| false);
    let held = use_held_back(passing);
    // For this picture's whole time on the stage: another entrance starting on
    // it would be a second one, while the next post is on its way.
    let enter = use_hook(|| enter);
    let mut style = transform;
    style.push_str(&fit_style(size));
    if let Some(t) = thumb.as_deref() {
        style.push_str("; --thumb: ");
        style.push_str(&css_url(t));
    }
    let shown = stage_src(held, &src, thumb.as_deref());
    rsx! {
        img {
            id: "stage-img",
            class: if loaded() { "{enter}" } else { "{enter} loading" },
            "data-turn": (!enter.is_empty()).then_some("wait"),
            // With nothing to show, the browser would print this over the picture.
            alt: if shown.is_some() { "{alt}" },
            src: shown,
            draggable: "false",
            decoding: "async",
            style,
            onwheel: move |e| onwheel.call(e),
            onload: move |_| {
                spawn(async move {
                    image_decoded().await;
                    loaded.set(true);
                });
            },
            onmounted: move |_| {
                motion::arrive(id);
                if !enter.is_empty() {
                    motion::turn(thumb.as_deref());
                }
                // The file hasn't been asked for yet, and the thumbnail can sit behind.
                if held {
                    return;
                }
                // A cached image can finish before the listener exists.
                spawn(async move {
                    let complete = document::eval(
                        "return document.getElementById('stage-img')?.complete === true",
                    )
                    .join::<bool>()
                    .await;
                    if complete.unwrap_or(false) {
                        image_decoded().await;
                        loaded.set(true);
                    }
                });
            },
        }
    }
}

/// The viewer's video.
#[component]
fn StageVideo(
    id: i64,
    src: String,
    thumb: Option<String>,
    size: Option<(i32, i32)>,
    enter: &'static str,
    passing: bool,
) -> Element {
    // As for the image: one entrance.
    let enter = use_hook(|| enter);
    let held = use_held_back(passing);
    rsx! {
        video {
            class: enter,
            "data-turn": (!enter.is_empty()).then_some("wait"),
            src: if !held { "{src}" },
            poster: thumb.clone(),
            style: fit_style(size),
            controls: true,
            autoplay: !power::active(),
            r#loop: true,
            playsinline: true,
            onmounted: move |_| {
                motion::arrive(id);
                if !enter.is_empty() {
                    motion::turn(thumb.as_deref());
                }
            },
        }
    }
}

/// Narrowest block of translated text, in thousandths of the image width:
/// vertical Japanese balloons are much narrower than a translated line.
const MIN_BLOCK_W: u16 = 160;

/// Where a block of translated text goes, in percent of the image. Narrow
/// boxes widen around their centre and stay inside the image.
fn block_style(b: &TextBlock) -> String {
    let pc = |v: u16| f32::from(v) / 10.0;
    let w = b.w.clamp(MIN_BLOCK_W, 1000);
    let centre = b.x.saturating_add(b.w / 2);
    let x = centre.saturating_sub(w / 2).min(1000 - w);
    format!(
        "left: {}%; top: {}%; width: {}%; min-height: {}%",
        pc(x),
        pc(b.y),
        pc(w),
        pc(b.h)
    )
}

/// Which kept translation to draw: the one in the app's language (just
/// made, or kept), else, when the server can't make one, any kept one.
fn pick_translation(
    made: Option<PanelTranslation>,
    kept: &[PanelTranslation],
    lang: &str,
    can_make: bool,
) -> Option<PanelTranslation> {
    made.filter(|t| t.lang == lang)
        .or_else(|| kept.iter().find(|t| t.lang == lang).cloned())
        .or_else(|| (!can_make).then(|| kept.first().cloned()).flatten())
}

/// Translated text drawn over the image (comic and manga panels), sized and
/// zoomed with it, and the button that shows it or asks the server to make
/// it in the app's language.
#[component]
fn PanelOverlay(id: i64, size: (i32, i32), transform: String, ready: ReadSignal<bool>) -> Element {
    let can_make = use_ai_status().comics;
    let kept = use_resource(move || async move {
        // Flipped past, it isn't worth asking.
        if !ready() {
            return None;
        }
        let client = store::client()?;
        client.panel_translations(id).await.ok()
    });
    let mut made = use_signal(|| None::<PanelTranslation>);
    let mut shown = use_signal(|| false);
    let mut busy = use_signal(|| false);
    let mut note = use_signal(|| None::<String>);
    let lang = LANG.read().code();
    let kept = kept.read().clone().flatten().unwrap_or_default();
    let pick = pick_translation(made(), &kept, lang, can_make);
    if pick.is_none() && !can_make {
        return rsx! {};
    }
    let have = pick.is_some();
    let onclick = move |_| {
        note.set(None);
        if have {
            shown.toggle();
            return;
        }
        let Some(client) = store::client() else {
            return;
        };
        busy.set(true);
        spawn(async move {
            match client.translate_panels(id, lang).await {
                Ok(t) => {
                    if t.blocks.is_empty() {
                        note.set(Some(t!("post-panels-none")));
                    }
                    made.set(Some(t));
                    shown.set(true);
                }
                Err(e) => note.set(Some(e.to_string())),
            }
            busy.set(false);
        });
    };
    let (label, title) = if busy() {
        (t!("post-panels-making"), t!("post-panels-making"))
    } else if !have {
        (t!("post-panels-make"), t!("post-panels-make-title"))
    } else if shown() {
        (t!("post-panels-hide"), t!("post-panels-hide"))
    } else {
        (t!("post-panels-show"), t!("post-panels-show"))
    };
    let layer_style = format!("{transform}{}", fit_style(Some(size)));
    rsx! {
        if shown() && let Some(t) = pick.filter(|t| !t.blocks.is_empty()) {
            div { class: "panel-layer", style: layer_style, lang: "{t.lang}",
                for (i, b) in t.blocks.iter().enumerate() {
                    div { key: "{i}", class: "panel-block", style: block_style(b), title: "{b.text}",
                        "{b.translation}"
                    }
                }
            }
        }
        if let Some(message) = note() {
            p { class: "stage-note", role: "status", "{message}" }
        }
        button {
            class: if busy() { "stage-btn stage-translate glass float busy" } else { "stage-btn stage-translate glass float" },
            r#type: "button",
            title,
            aria_label: label,
            aria_pressed: if have { "{shown()}" },
            disabled: busy(),
            ondoubleclick: move |e| e.stop_propagation(),
            onclick,
            Ico { icon: Icon::Translate }
        }
    }
}

/// A little longer than `--fast`, the picture's way back from being zoomed.
const SHRINK_MS: u32 = 200;

/// The media, with zoom and pan for images, previous/next and fullscreen.
#[component]
fn Stage(
    id: i64,
    kind: MediaKind,
    src: String,
    thumb: Option<String>,
    size: Option<(i32, i32)>,
    enter: &'static str,
    passing: bool,
    ready: bool,
    alt: String,
    zoom: Signal<Zoom>,
    full: Signal<bool>,
    first: bool,
    last: bool,
    onstep: EventHandler<bool>,
    onfull: EventHandler<bool>,
    onback: EventHandler<()>,
    rect: Signal<Option<PixelsRect>>,
) -> Element {
    let mut mounted = use_signal(|| None::<std::rc::Rc<MountedData>>);
    // Pointers down on the stage. Not a signal: re-rendering the stage on
    // every pointer move made the desktop app lag. `dragging` follows
    // whether any pointer is down.
    let mut touches = use_hook(|| CopyValue::new(HashMap::<i32, Touch>::new()));
    let mut dragging = use_signal(|| false);
    let mut settle = move || {
        let now = !touches.peek().is_empty();
        if *dragging.peek() != now {
            dragging.set(now);
        }
    };
    let zoomable = kind != MediaKind::Video;

    // Going back to its size, the picture is bigger than the stage for a moment
    // yet, and stays inside its window until it's done.
    let mut shrinking = use_signal(|| false);
    let mut shrunk = use_hook(|| CopyValue::new(None::<Task>));
    let grown = use_memo(move || zoom().is_zoomed());
    use_effect(move || {
        if let Some(task) = shrunk.take() {
            task.cancel();
        }
        if grown() {
            shrinking.set(true);
        } else if *shrinking.peek() {
            shrunk.set(Some(spawn(async move {
                crate::activity::wait_ms(SHRINK_MS).await;
                shrinking.set(false);
            })));
        }
    });

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

    let wheel = move |e: WheelEvent| {
        e.prevent_default();
        e.stop_propagation();
        let factor = wheel_factor(e.delta());
        let at = e.client_coordinates();
        spawn(async move {
            let r = measure().await;
            zoom.set(zoom().zoom_at(factor, from_centre(at, r), size_of(r)));
        });
    };

    let z = zoom();
    let mut class = String::from("stage");
    for (on, name) in [
        (full(), " full"),
        (z.is_zoomed() || shrinking(), " zoomed"),
        (dragging(), " dragging"),
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
            // The wheel zooms on the picture (see `KEY_LISTENER` for when it scrolls
            // instead). Beside it, it scrolls on to what's below, unless there's
            // no page to scroll or the picture has grown.
            onwheel: move |e| {
                if zoomable && (full() || zoom().is_zoomed()) {
                    wheel(e);
                }
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
                settle();
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
                settle();
                let Some(t) = ended else {
                    return;
                };
                let (dx, dy) = (t.last.x - t.start.x, t.last.y - t.start.y);
                if touches.peek().is_empty() && !zoom().is_zoomed() && dx.abs() > SWIPE && dx.abs() > dy.abs() * 1.5 {
                    onstep.call(dx < 0.0);
                }
            },
            onpointercancel: move |e| {
                touches.write().remove(&e.pointer_id());
                settle();
            },
            onpointerleave: move |e| {
                touches.write().remove(&e.pointer_id());
                settle();
            },
            match kind {
                MediaKind::Video => rsx! {
                    StageVideo { key: "{id}", id, src, thumb, size, enter, passing }
                },
                _ => rsx! {
                    StageImage { key: "{id}", id, src, thumb, alt, size, transform: z.css(), enter, passing, onwheel: wheel }
                    if let Some(size) = size.filter(|_| kind == MediaKind::Image) {
                        PanelOverlay { key: "{id}", id, size, transform: z.css(), ready }
                    }
                },
            }
            button { class: "stage-btn stage-back glass float", title: t!("post-back"), aria_label: t!("post-back"),
                ondoubleclick: move |e| e.stop_propagation(),
                onclick: move |_| onback.call(()),
                Ico { icon: Icon::ArrowBack }
            }
            // Always there, greyed out at either end, so nothing comes and goes.
            button { class: "stage-btn stage-prev glass float", title: t!("post-prev-title"), aria_label: t!("post-prev"),
                disabled: first,
                ondoubleclick: move |e| e.stop_propagation(),
                onclick: move |_| onstep.call(false),
                Ico { icon: Icon::ChevronLeft, size: 32 }
            }
            button { class: "stage-btn stage-next glass float", title: t!("post-next-title"), aria_label: t!("post-next"),
                disabled: last,
                ondoubleclick: move |e| e.stop_propagation(),
                onclick: move |_| onstep.call(true),
                Ico { icon: Icon::ChevronRight, size: 32 }
            }
            // Only a double click on the media itself toggles the zoom. On the
            // bar, not its buttons, so one that has just disabled itself counts.
            div { class: "stage-tools glass float",
                ondoubleclick: move |e| e.stop_propagation(),
                if zoomable {
                    button { class: "stage-btn", title: t!("zoom-out-title"), aria_label: t!("zoom-out"),
                        disabled: !z.is_zoomed(),
                        onclick: move |_| zoom.set(zoom().zoom_at(1.0 / STEP, (0.0, 0.0), size_of(*rect.peek()))),
                        Ico { icon: Icon::ZoomOut }
                    }
                    button { class: "stage-zoom", title: t!("zoom-fit-title"), aria_label: t!("zoom-reset"),
                        onclick: move |_| zoom.set(Zoom::default()),
                        if z.is_zoomed() { "{z.percent()}%" } else { {t!("zoom-fit")} }
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
            "; aspect-ratio: 3600 / 2400; width: min(100%, calc(var(--fit-h) * 3600 / 2400), var(--fit-cap, 3600px))"
        );
        // Unknown or nonsense sizes leave the box to the image.
        assert_eq!(fit_style(None), "");
        assert_eq!(fit_style(Some((0, 10))), "");
        assert_eq!(fit_style(Some((10, -1))), "");
    }

    /// The rule for `selector`, up to its closing brace.
    fn rule(selector: &str) -> &'static str {
        let css = include_str!("../../assets/main.css");
        let (_, rest) = css
            .split_once(&format!("\n{selector} {{"))
            .unwrap_or_else(|| panic!("{selector}"));
        rest.split_once('}').map_or(rest, |(body, _)| body)
    }

    #[test]
    fn nothing_on_the_stage_moves_with_the_picture() {
        // The stage's size comes from the window, never from what's on it.
        let stage = rule(".stage");
        assert!(stage.contains("height: var(--fit-h)"));
        assert!(!stage.contains("min-height"));
        assert!(!stage.contains("background"));
        for edge in [
            ".stage-prev, .stage-next",
            ".stage-tools",
            ".stage-translate",
        ] {
            assert!(rule(edge).contains("position: absolute"), "{edge}");
        }
        // Holding the old height during a turn is what made it change.
        assert!(!include_str!("../motion.rs").contains("stage.style.height"));
        assert!(!include_str!("../motion.rs").contains("dataset.turning"));
    }

    #[test]
    fn the_viewers_buttons_stay_round() {
        // Regression: pressed, and on a vote, they turned into rounded squares.
        assert!(rule(".stage-btn:active:not(:disabled)").contains("scale(0.88)"));
        for button in [
            ".stage-btn",
            ".stage-btn:active:not(:disabled)",
            ".post-bar .icon-btn",
            ".post-bar .vote .icon-btn.on",
        ] {
            assert!(!rule(button).contains("border-radius: 14px"), "{button}");
            assert!(!rule(button).contains("border-radius var("), "{button}");
        }
        assert!(!include_str!("../../assets/main.css").contains(".post-bar .icon-btn:active"));
    }

    #[test]
    fn a_vote_leaves_the_rest_of_the_bar_alone() {
        // Regression: the bin and the vote buttons blinked grey for each request.
        let (source, _) = include_str!("post.rs").split_once("#[cfg(test)]").unwrap();
        let (_, bar) = source
            .split_once("class: \"post-bar glass float\"")
            .unwrap();
        let (bar, _) = bar.split_once("if let Some(message) = error()").unwrap();
        assert!(!bar.contains("disabled: busy()"));
        // What keeps a second press out instead.
        assert!(source.contains("if busy() || post.peek()"));
    }

    #[test]
    fn the_translate_button_stands_beside_the_tools() {
        let tools = rule(".stage-tools");
        assert!(tools.contains("right: 12px") && tools.contains("top: 12px"));
        assert!(tools.contains("padding: 2px") && tools.contains("gap: 2px"));
        let translate = rule(".stage-translate");
        assert!(translate.contains("right: calc(12px + var(--tools-w) + 6px)"));
        assert!(translate.contains("top: 12px"));
        assert!(
            rule(".app[data-hand=\"left\"] .stage-translate")
                .contains("left: calc(12px + var(--tools-w) + 6px)")
        );
        // The width it's placed by is the width the tools are built from.
        assert!(rule(".stage").contains("calc(3 * var(--tool-w) + var(--zoom-w) + 10px)"));
        assert!(rule(".stage-tools .stage-btn").contains("width: var(--tool-w)"));
        assert!(rule(".stage-zoom").contains("width: var(--zoom-w)"));
    }

    #[test]
    fn the_picture_keeps_its_corners_when_zoomed() {
        // The transform scales the radius too, so it's divided back down.
        assert!(rule(".stage.zoomed img").contains("calc(var(--r) / var(--z, 1))"));
        assert!(Zoom::default().css().contains("--z: 1.000"));
    }

    #[test]
    fn fullscreen_fits_the_display() {
        // Regression: a small picture stayed at its own size in the middle.
        assert!(fit_style(Some((200, 300))).contains("var(--fit-cap, 200px)"));
        assert!(rule(".stage.full").contains("--fit-cap: 100%"));
        // The page's scrollbar would take its width off the display.
        assert!(rule("html:has(.stage.full)").contains("overflow-y: hidden"));
    }

    #[test]
    fn fullscreen_controls_rest_and_come_back() {
        let hidden = rule(
            ".stage.full[data-rest] :is(.stage-prev, .stage-next, .stage-tools, .stage-translate:not(.busy))",
        );
        assert!(hidden.contains("opacity: 0") && hidden.contains("pointer-events: none"));
        assert!(KEY_LISTENER.contains("'pointerdown'") && KEY_LISTENER.contains("'keydown'"));
        assert!(KEY_LISTENER.contains("removeAttribute('data-rest')"));
        assert!(KEY_LISTENER.contains(":focus-visible"));
        // Regression: on a phone the button that opened fullscreen stayed `:hover`.
        assert!(KEY_LISTENER.contains("matchMedia('(hover: hover)')"));
        assert!(KEY_LISTENER_OFF.contains("__sbWake"));
    }

    #[test]
    fn the_wheel_can_leave_the_picture_for_whats_below() {
        // Regression: over a picture that fills the stage, the page wouldn't scroll.
        assert!(KEY_LISTENER.contains("e.deltaY > 0 || scrollY > 0"));
        assert!(KEY_LISTENER.contains("stage.matches('.full, .zoomed')"));
        assert!(KEY_LISTENER.contains("!e.ctrlKey"));
        assert!(KEY_LISTENER.contains("e.stopPropagation()"));
        assert!(KEY_LISTENER_OFF.contains("__sbWheel"));
    }

    #[test]
    fn the_glow_leaves_the_text_and_the_battery_alone() {
        assert!(rule(".post-glow").contains("pointer-events: none"));
        assert!(rule(r#".app[data-saver="on"] .post-glow"#).contains("display: none"));
    }

    #[test]
    fn the_glow_fades_rather_than_blurring_every_frame() {
        let layer = rule(".glow-layer");
        assert!(layer.contains("opacity: 0"));
        assert!(layer.contains("transition: opacity var(--fade)"));
        assert!(!layer.contains("filter"));
        assert!(rule(".glow-layer.on").contains("opacity: var(--glow-a)"));
    }

    #[test]
    fn a_zoomed_picture_stays_in_its_window_until_it_has_shrunk() {
        // Regression: the window went with the zoom, and the picture on its way
        // back to size spilled over everything around the stage for a moment.
        let css = include_str!("../../assets/main.css");
        let fast = css
            .split_once("--fast: ")
            .and_then(|(_, rest)| rest.split_once("ms"))
            .and_then(|(ms, _)| ms.parse::<u32>().ok())
            .unwrap();
        assert!(SHRINK_MS > fast);
        assert!(rule(".stage.zoomed").contains("overflow: hidden"));
    }

    #[test]
    fn no_press_is_lost_and_the_queue_stays_short() {
        assert_eq!(queued(0, true), 1);
        assert_eq!(queued(2, true), 3);
        assert_eq!(queued(0, false), -1);
        // The other way cancels one out.
        assert_eq!(queued(2, false), 1);
        // A key held down doesn't leave it walking on for long.
        assert_eq!(queued(MOST_QUEUED, true), MOST_QUEUED);
        assert_eq!(queued(-MOST_QUEUED, false), -MOST_QUEUED);
    }

    #[test]
    fn a_press_goes_the_way_it_was_meant() {
        let n = Neighbors {
            prev: Some(1),
            next: Some(3),
        };
        assert_eq!(step_target(2, n), Some(3));
        assert_eq!(step_target(-1, n), Some(1));
        assert_eq!(step_target(1, Neighbors::default()), None);
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
    fn adds_suggested_tags_once() {
        assert_eq!(replace_tag("blnde cat", "blnde", "blonde"), "blonde cat");
        assert_eq!(replace_tag("Blnde  cat ", "blnde", "blonde"), "blonde cat");
        assert_eq!(
            replace_tag("blnde blonde cat", "blnde", "blonde"),
            "blonde cat"
        );
        assert_eq!(replace_tag("cat", "blnde", "blonde"), "cat");
        let saved = ["cat".to_owned(), "sky".to_owned()];
        assert_eq!(new_words("cat  blnde sky dgo ", &saved), "blnde dgo");
        assert_eq!(new_words("cat sky", &saved), "");
        assert_eq!(with_tag("", "sky"), "sky");
        assert_eq!(with_tag("cat  ", "sky"), "cat sky");
        assert_eq!(with_tag("cat sky", "sky"), "cat sky");
    }

    #[test]
    fn places_and_picks_translations() {
        let b = TextBlock {
            x: 600,
            y: 45,
            w: 300,
            h: 125,
            text: String::new(),
            translation: "Ciao".into(),
        };
        assert_eq!(
            block_style(&b),
            "left: 60%; top: 4.5%; width: 30%; min-height: 12.5%"
        );
        // A narrow vertical balloon widens around its centre…
        let narrow = TextBlock {
            x: 500,
            w: 40,
            ..b.clone()
        };
        assert!(block_style(&narrow).starts_with("left: 44%; top: 4.5%; width: 16%;"));
        // …without leaving the image.
        let edge = TextBlock {
            x: 960,
            w: 40,
            ..b.clone()
        };
        assert!(block_style(&edge).starts_with("left: 84%;"));
        let edge = TextBlock {
            x: 0,
            w: 40,
            ..b.clone()
        };
        assert!(block_style(&edge).starts_with("left: 0%;"));
        let t = |lang: &str| PanelTranslation {
            lang: lang.into(),
            blocks: vec![b.clone()],
            model: "m".into(),
            created_at: chrono::Utc::now(),
        };
        let kept = [t("en"), t("it")];
        assert_eq!(
            pick_translation(None, &kept, "it", true).map(|t| t.lang),
            Some("it".into())
        );
        // None in the app's language: made on request when the server can.
        assert_eq!(pick_translation(None, &kept[..1], "it", true), None);
        assert_eq!(
            pick_translation(None, &kept[..1], "it", false).map(|t| t.lang),
            Some("en".into())
        );
        assert_eq!(
            pick_translation(Some(t("it")), &[], "it", true).map(|t| t.lang),
            Some("it".into())
        );
        assert_eq!(pick_translation(None, &[], "it", false), None);
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
        assert_eq!(enter_class(Some(true), false), "from-next");
        assert_eq!(enter_class(Some(false), false), "from-prev");
        assert_eq!(enter_class(None, false), "");
    }

    #[test]
    fn flipping_through_has_no_entrance() {
        assert_eq!(enter_class(Some(true), true), "quick");
        assert_eq!(enter_class(Some(false), true), "quick");
        // Opened from elsewhere, it isn't a turn at all.
        assert_eq!(enter_class(None, true), "");
    }

    #[test]
    fn a_picture_flipped_past_shows_its_thumbnail_and_never_nothing() {
        assert_eq!(
            stage_src(true, "file", Some("thumb")).as_deref(),
            Some("thumb")
        );
        assert_eq!(
            stage_src(false, "file", Some("thumb")).as_deref(),
            Some("file")
        );
        assert_eq!(stage_src(false, "file", None).as_deref(), Some("file"));
        // Nothing to stand in: no file is asked for either.
        assert_eq!(stage_src(true, "file", None), None);
    }
}
