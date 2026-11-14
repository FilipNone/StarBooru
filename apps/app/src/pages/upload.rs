//! Upload: files and addresses wait here for their tags, then go to the
//! app-wide queue (`activity`), so they keep going on other pages.

use crate::link::Link;
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::hash::Hash;
use std::rc::Rc;

use dioxus::html::{FileData, HasFileData};
use dioxus::prelude::*;
use starbooru_core::tag::TagError;
use starbooru_core::{ImportJob, JobState, Rating, parse_tag_list};

use super::post::RatingPicker;
use super::{Choice, Select, TransferBar, notify};
use crate::Route;
use crate::activity::{self, Item, JOBS, QUEUE, Remote, Source, Status};
use crate::icons::{Ico, Icon};
use crate::motion;
use crate::paste;
use crate::store;
use crate::util::{
    drop_is_urls, drop_urls, folder_tag, human_size, media_files, parse_urls, rating_label,
};

/// What the file picker lists. Android opens its gallery for this. Elsewhere it
/// would hide pictures with no extension and, on the desktop, every type but a
/// few, with no way to show them; `media_files` sorts out what gets chosen.
const PICKER_ACCEPT: Option<&str> = if cfg!(target_os = "android") {
    Some("image/*,video/*")
} else {
    None
};

/// Kept across pages, like the list they apply to.
static SHARED_TAGS: GlobalSignal<String> = Signal::global(String::new);
static RATING: GlobalSignal<Rating> = Signal::global(Rating::default);
/// Whether the AI tags what is sent. Until the toggle is pressed it is
/// `None`, and the server's setting decides.
static AUTOTAG: GlobalSignal<Option<bool>> = Signal::global(|| None);

/// What the bar puts on everything sent together.
fn batch() -> activity::Batch {
    activity::Batch {
        tags: SHARED_TAGS.peek().clone(),
        rating: *RATING.peek(),
        autotag: *AUTOTAG.peek(),
    }
}

/// Whether the AI tags what is sent, given the server's own choice.
fn autotag_on(server: bool) -> bool {
    AUTOTAG.read().unwrap_or(server)
}

/// How many tiles show a picture of their own file. A folder of thousands
/// would otherwise decode every picture in it at once.
const MAX_PREVIEWS: usize = 120;

#[component]
pub fn Upload() -> Element {
    let mut folder_tags = use_signal(|| true);
    let mut linking = use_signal(|| false);
    let mut url_text = use_signal(String::new);
    let mut notice = use_signal(|| None::<String>);
    // The item whose popup is open, and where its tile is.
    let mut opened = use_signal(|| None::<(u64, (f64, f64))>);
    let open_item = move |key: u64| {
        spawn(async move {
            let from = tile_offset(key).await;
            activity::look_up(key);
            opened.set(Some((key, from)));
        });
    };
    // Tiles taken off the list, shown while they go.
    let mut leaving = use_signal(|| None::<Leaving>);
    // Once they have gone, the grid follows the lists again.
    use_effect(move || {
        let Some(round) = leaving.read().as_ref().map(|l| l.round) else {
            return;
        };
        spawn(async move {
            tiles_gone().await;
            if leaving.peek().as_ref().is_some_and(|l| l.round == round) {
                leaving.set(None);
            }
        });
    });
    // Hide URL import when the server has it off (shown while unknown).
    let url_import =
        use_resource(|| async move { store::server_info().await.is_none_or(|i| i.url_import) });
    // The AI toggle shows only when the server can tag.
    let ai = use_resource(store::ai_status);
    use_hook(activity::refresh);

    // Only images and videos. Those from a folder get its name as a tag.
    let add = move |files: Vec<FileData>, from_folder: bool| async move {
        let (media, skipped) = media_files(files).await;
        for file in media {
            let tags = if from_folder && folder_tags() {
                folder_tag(&file.path()).unwrap_or_default()
            } else {
                String::new()
            };
            activity::add(Source::File(file), tags);
        }
        notice.set((skipped > 0).then(|| t!("upload-skipped", count = skipped)));
    };

    // Ctrl+V: files and pictures on the clipboard, and links. Only here.
    use_hook(move || {
        spawn(async move {
            let mut watch = paste::watch();
            while let Ok(pasted) = watch.recv::<paste::Pasted>().await {
                if !store::can_upload() {
                    continue;
                }
                let (files, too_big) = paste::files(&pasted).await;
                let urls = if url_import.peek().unwrap_or(true) {
                    parse_urls(&pasted.text)
                } else {
                    Vec::new()
                };
                let found = !files.is_empty() || !urls.is_empty();
                for url in urls {
                    activity::add(Source::Url(url), String::new());
                }
                add(files, false).await;
                if too_big > 0 {
                    notice.set(Some(t!("upload-paste-too-big", count = too_big)));
                } else if !found && pasted.files == 0 {
                    notice.set(Some(t!("upload-paste-nothing")));
                }
            }
        });
    });
    use_drop(paste::unwatch);

    // Files dragged over the window: the veil grows out of the drop box.
    let mut veil = use_signal(|| None::<BoxRect>);
    let mut veil_closing = use_signal(|| false);
    use_hook(move || {
        spawn(async move {
            let script = WATCH_PAGE_DROP.replace("FILE_DRAG", FILE_DRAG);
            let mut watch = document::eval(&script);
            while let Ok(shown) = watch.recv::<bool>().await {
                if !shown {
                    if veil.peek().is_some() && !*veil_closing.peek() {
                        veil.set(Some(drop_box().await));
                        veil_closing.set(true);
                    }
                } else if *veil_closing.peek() {
                    veil_closing.set(false);
                } else if veil.peek().is_none() && store::can_upload() {
                    veil.set(Some(drop_box().await));
                }
            }
        });
    });
    use_drop(|| {
        document::eval(WATCH_PAGE_DROP_OFF);
    });
    // Back into the box, then gone.
    use_effect(move || {
        if !veil_closing() {
            return;
        }
        spawn(async move {
            motion::settled(".up-veil").await;
            if *veil_closing.peek() {
                veil.set(None);
                veil_closing.set(false);
            }
        });
    });
    let take_drop = move |e: DragEvent| {
        e.prevent_default();
        let (files, urls) = dropped(&e);
        spawn(async move {
            for url in urls {
                activity::add(Source::Url(url), String::new());
            }
            add(files, false).await;
            // Where the box is now that the tiles are in.
            if veil.peek().is_some() {
                veil.set(Some(drop_box().await));
                veil_closing.set(true);
            }
        });
    };
    let mut add_urls = move || {
        let found = parse_urls(&url_text());
        if found.is_empty() {
            notice.set(Some(t!("upload-urls-only")));
            return;
        }
        notice.set(None);
        for url in found {
            activity::add(Source::Url(url), String::new());
        }
        url_text.set(String::new());
    };

    let drafts = QUEUE
        .read()
        .iter()
        .filter(|i| i.status == Status::Draft)
        .count();
    let clearable = QUEUE.read().iter().any(|i| i.status != Status::Uploading)
        || JOBS.read().iter().any(|j| !j.state.is_active());
    let show_url = url_import.read().unwrap_or(true);
    // The server's setting, or `None` when it can't tag.
    let ai_tagging = ai
        .read()
        .clone()
        .flatten()
        .filter(|s| s.tagging)
        .map(|s| s.autotag);
    let (items, jobs) = shown(&leaving.read(), &QUEUE.read(), &JOBS.read());
    let queued = items.len();
    let grid_class = if items.is_empty() && jobs.is_empty() {
        "grid up-grid empty"
    } else {
        "grid up-grid"
    };
    rsx! {
        div { class: if drafts > 0 { "page upload sending" } else { "page upload" },
            div { class: "page-head",
                h1 { class: "page-title", {t!("nav-upload")} }
                label { class: "icon-btn", title: t!("upload-files"), aria_label: t!("upload-files"),
                    Ico { icon: Icon::Add }
                    input {
                        r#type: "file",
                        multiple: true,
                        accept: PICKER_ACCEPT,
                        class: "visually-hidden",
                        onchange: move |e| {
                            spawn(add(e.files(), false));
                        },
                    }
                }
                label { class: "icon-btn", title: t!("upload-folder"), aria_label: t!("upload-folder"),
                    Ico { icon: Icon::Folder }
                    input {
                        r#type: "file",
                        directory: true,
                        multiple: true,
                        class: "visually-hidden",
                        onchange: move |e| {
                            spawn(add(e.files(), true));
                        },
                    }
                }
                span { class: "up-switches",
                    Switch {
                        icon: Icon::Label,
                        label: t!("upload-folder-tags"),
                        on: folder_tags(),
                        onchange: move |on| folder_tags.set(on),
                    }
                    if let Some(ai) = ai_tagging {
                        Switch {
                            icon: Icon::Sparkle,
                            label: t!("upload-autotag"),
                            on: autotag_on(ai),
                            onchange: move |on| *AUTOTAG.write() = Some(on),
                        }
                    }
                }
                if show_url {
                    button {
                        r#type: "button",
                        class: if linking() { "icon-btn on" } else { "icon-btn" },
                        title: t!("upload-link"),
                        aria_label: t!("upload-link"),
                        aria_expanded: linking(),
                        onclick: move |_| linking.toggle(),
                        Ico { icon: Icon::Link }
                    }
                }
                if clearable {
                    button {
                        r#type: "button",
                        class: "icon-btn danger",
                        title: t!("upload-clear"),
                        aria_label: t!("upload-clear"),
                        onclick: move |_| {
                            leave(leaving, || {
                                activity::clear();
                                activity::clear_jobs_now();
                            });
                        },
                        Ico { icon: Icon::Delete }
                    }
                }
            }
            if linking() && show_url {
                form {
                    class: "inline-add url-add",
                    onsubmit: move |e| {
                        e.prevent_default();
                        add_urls();
                    },
                    input {
                        r#type: "url",
                        placeholder: t!("upload-urls-placeholder"),
                        aria_label: t!("upload-urls-label"),
                        spellcheck: "false",
                        autofocus: true,
                        value: "{url_text}",
                        oninput: move |e| url_text.set(e.value()),
                    }
                    button { class: "btn tonal", r#type: "submit", disabled: url_text().trim().is_empty(),
                        Ico { icon: Icon::Add }
                        {t!("add")}
                    }
                }
            }
            if let Some(message) = notice() {
                div { class: "banner info", role: "status", "{message}" }
            }
            div { class: grid_class,
                label {
                    class: if veil().is_some() { "up-drop lifted" } else { "up-drop" },
                    title: t!("upload-files"),
                    Ico { icon: Icon::Upload, size: 40 }
                    span { {t!("upload-drop")} }
                    input {
                        r#type: "file",
                        multiple: true,
                        accept: PICKER_ACCEPT,
                        class: "visually-hidden",
                        onchange: move |e| {
                            spawn(add(e.files(), false));
                        },
                    }
                }
                for (index, (item, going)) in items.into_iter().enumerate() {
                    FileTile { key: "{item.key}", item, place: index, going, onopen: open_item }
                }
                for (index, (job, going)) in jobs.into_iter().enumerate() {
                    JobTile { key: "job-{job.id}", job, place: queued.saturating_add(index), going }
                }
            }
            if drafts > 0 {
                form {
                    class: "bulk-bar glass up-bar",
                    aria_label: t!("nav-upload"),
                    onsubmit: move |e| {
                        e.prevent_default();
                        activity::start(&batch());
                    },
                    input {
                        class: "bulk-tags",
                        r#type: "text",
                        placeholder: t!("upload-shared-tags"),
                        aria_label: t!("upload-shared-tags"),
                        autocapitalize: "none",
                        spellcheck: false,
                        value: "{SHARED_TAGS}",
                        oninput: move |e| *SHARED_TAGS.write() = e.value(),
                    }
                    Select {
                        class: "bulk-rating",
                        aria_label: t!("rating"),
                        value: RATING.read().as_str().to_string(),
                        options: Rating::ALL.into_iter().map(|r| Choice::new(r.as_str(), rating_label(r))).collect::<Vec<_>>(),
                        onchange: move |v: String| {
                            if let Some(rating) = Rating::parse(&v) {
                                *RATING.write() = rating;
                            }
                        },
                    }
                    button { class: "btn", r#type: "submit",
                        Ico { icon: Icon::Upload }
                        {t!("upload-start", count = drafts)}
                    }
                }
            }
            if let Some((key, from)) = opened() {
                ItemSheet {
                    key: "{key}",
                    item_key: key,
                    from,
                    onremove: move |key| leave(leaving, || activity::remove(key)),
                    onclose: move |_| opened.set(None),
                }
            }
            if let Some(rect) = veil() {
                div {
                    class: if veil_closing() { "drop-veil up-veil closing" } else { "drop-veil up-veil" },
                    style: rect.style(),
                    ondragover: move |e| e.prevent_default(),
                    ondrop: take_drop,
                    div { class: "drop-veil-card",
                        Ico { icon: Icon::Upload, size: 40 }
                        strong { {t!("upload-drop-now")} }
                    }
                }
            }
        }
    }
}

/// Where the drop box sits in the window, as the distance from each edge,
/// and how round its corners are.
#[derive(Clone, Copy, PartialEq, Default, serde::Deserialize)]
struct BoxRect {
    top: f64,
    right: f64,
    bottom: f64,
    left: f64,
    radius: f64,
}

impl BoxRect {
    /// The veil's starting and ending shape.
    fn style(&self) -> String {
        format!(
            "--box-t: {:.1}px; --box-r: {:.1}px; --box-b: {:.1}px; --box-l: {:.1}px; --box-radius: {:.1}px",
            self.top, self.right, self.bottom, self.left, self.radius
        )
    }
}

/// Measures the drop box. One scrolled out of sight stands in as a small
/// square in the middle of the window.
async fn drop_box() -> BoxRect {
    let mut eval = document::eval(MEASURE_DROP_BOX);
    eval.recv().await.unwrap_or_default()
}

const MEASURE_DROP_BOX: &str = r#"
const el = document.querySelector(".up-drop");
const r = el?.getBoundingClientRect();
const w = innerWidth, h = innerHeight;
if (!r || r.width === 0 || r.bottom <= 0 || r.top >= h) {
  const side = 80;
  dioxus.send({ top: h / 2 - side, right: w / 2 - side, bottom: h / 2 - side, left: w / 2 - side, radius: 24 });
} else {
  dioxus.send({
    top: Math.max(r.top, 0),
    right: Math.max(w - r.right, 0),
    bottom: Math.max(h - r.bottom, 0),
    left: Math.max(r.left, 0),
    radius: parseFloat(getComputedStyle(el).borderTopLeftRadius) || 0,
  });
}
"#;

/// The Upload page's own watch for files dragged over the window. Unlike
/// the gallery's, the drop itself closes the veil, once the tiles are in.
const WATCH_PAGE_DROP: &str = r#"
window.__sbPageDrop?.stop();
let own = false;
const files = (e) => {
  const types = [...(e.dataTransfer?.types ?? [])];
  return !own && (FILE_DRAG);
};
const over = (e) => {
  if (!files(e)) return;
  e.preventDefault();
  if (!document.querySelector(".up-veil:not(.closing)")) dioxus.send(true);
};
const leave = (e) => {
  if (e.target?.classList?.contains("up-veil")) dioxus.send(false);
};
const drop = (e) => {
  if (files(e) && !e.target?.closest?.(".up-veil")) {
    e.preventDefault();
    dioxus.send(false);
  }
  own = false;
};
const start = () => { own = true; };
const end = () => { own = false; };
const hooks = { dragenter: over, dragover: over, dragleave: leave, drop, dragstart: start, dragend: end };
for (const [name, hook] of Object.entries(hooks)) addEventListener(name, hook);
window.__sbPageDrop = {
  stop() {
    for (const [name, hook] of Object.entries(hooks)) removeEventListener(name, hook);
  },
};
"#;
const WATCH_PAGE_DROP_OFF: &str = "window.__sbPageDrop?.stop(); window.__sbPageDrop = null;";

/// The tags and rating every file in a batch gets.
#[component]
fn SharedFields(#[props(default)] focus: bool) -> Element {
    rsx! {
        label { class: "field",
            span { {t!("upload-shared-tags")} }
            input {
                placeholder: "artist_name landscape blue_sky",
                spellcheck: "false",
                autofocus: focus,
                value: "{SHARED_TAGS}",
                oninput: move |e| *SHARED_TAGS.write() = e.value(),
            }
        }
        div { class: "field",
            span { {t!("rating")} }
            RatingPicker { value: *RATING.read(), onchange: move |r| *RATING.write() = r }
        }
    }
}

/// What a drop carries: files, and the addresses of a link or a picture
/// dragged out of a browser.
fn dropped(e: &DragEvent) -> (Vec<FileData>, Vec<String>) {
    let data = e.data_transfer();
    let get = |kind: &str| data.get_data(kind).unwrap_or_default();
    let urls = drop_urls(&get("text/uri-list"), &get("text/html"), &get("text/plain"));
    let files = e.files();
    if drop_is_urls(files.len(), &urls) {
        (Vec::new(), urls)
    } else {
        (files, Vec::new())
    }
}

/// Notices files dragged over the page. It's on the window, as the page doesn't
/// fill it, and a file dropped outside would make a browser open that file.
/// Pictures dragged off the page itself don't count.
const WATCH_DROP: &str = r#"
window.__sbDrop?.stop();
let own = false;
const files = (e) => {
  const types = [...(e.dataTransfer?.types ?? [])];
  return !own && (FILE_DRAG);
};
const over = (e) => {
  if (!files(e)) return;
  e.preventDefault();
  if (!document.querySelector(".drop-veil, .upload-sheet")) dioxus.send(true);
};
const leave = (e) => {
  if (e.target?.classList?.contains("drop-veil")) dioxus.send(false);
};
const drop = (e) => {
  if (files(e)) e.preventDefault();
  own = false;
  dioxus.send(false);
};
const start = () => { own = true; };
const end = () => { own = false; };
const hooks = { dragenter: over, dragover: over, dragleave: leave, drop, dragstart: start, dragend: end };
for (const [name, hook] of Object.entries(hooks)) addEventListener(name, hook);
window.__sbDrop = {
  stop() {
    for (const [name, hook] of Object.entries(hooks)) removeEventListener(name, hook);
  },
};
"#;
/// What a drag worth taking looks like: files, or a link or a picture out of
/// a browser, which come as an address. WebKitGTK lists files that way too,
/// and the stand-in events Dioxus makes on Windows list nothing. The
/// desktop's file paths come from the webview itself on the drop.
#[cfg(feature = "desktop")]
const FILE_DRAG: &str =
    r#"!types.length || types.includes("Files") || types.includes("text/uri-list")"#;
#[cfg(not(feature = "desktop"))]
const FILE_DRAG: &str = r#"types.includes("Files") || types.includes("text/uri-list")"#;
const WATCH_DROP_OFF: &str = "window.__sbDrop?.stop(); window.__sbDrop = null;";

/// What's wrong with the tags of a batch, if anything: each file gets the shared
/// ones and its own.
fn tag_problem<'a>(shared: &str, own: impl IntoIterator<Item = &'a str>) -> Option<TagError> {
    own.into_iter()
        .find_map(|tags| parse_tag_list(&format!("{shared} {tags}")).err())
}

/// Files dropped on the page: asks for their tags in a sheet that comes out of
/// the Upload tab and goes back into it.
#[component]
pub fn DropToUpload() -> Element {
    let mut veil = use_signal(|| false);
    // The drafts in the sheet, which is open once it knows where it comes from.
    let mut keys = use_signal(Vec::<u64>::new);
    let mut from = use_signal(|| None::<(f64, f64)>);
    let mut closing = use_signal(|| false);
    // Whether closing the sheet also drops what's in it.
    let mut discard = use_signal(|| false);
    let mut error = use_signal(|| None::<String>);

    use_hook(move || {
        spawn(async move {
            let mut watch = document::eval(&WATCH_DROP.replace("FILE_DRAG", FILE_DRAG));
            while let Ok(shown) = watch.recv::<bool>().await {
                if *veil.peek() != shown {
                    veil.set(shown && store::can_upload());
                }
            }
        });
    });
    use_drop(|| {
        document::eval(WATCH_DROP_OFF);
    });

    let mut take = move |(files, urls): (Vec<FileData>, Vec<String>)| {
        veil.set(false);
        if *closing.peek() || !store::can_upload() {
            return;
        }
        spawn(async move {
            let (media, skipped) = media_files(files).await;
            if skipped > 0 {
                notify(t!("upload-skipped", count = skipped));
            }
            if (media.is_empty() && urls.is_empty()) || *closing.peek() {
                return;
            }
            for url in urls {
                let known = QUEUE.peek().iter().any(|i| {
                    keys.peek().contains(&i.key) && matches!(&i.source, Source::Url(u) if *u == url)
                });
                if !known {
                    let key = activity::add(Source::Url(url), String::new());
                    keys.write().push(key);
                }
            }
            for file in media {
                // The desktop keeps the last drag's paths, and hands them over again
                // for text dropped inside the sheet.
                let known = QUEUE.peek().iter().any(|i| {
                    keys.peek().contains(&i.key)
                        && matches!(&i.source, Source::File(f) if f.path() == file.path() && f.size() == file.size())
                });
                if !known {
                    let key = activity::add(Source::File(file), String::new());
                    keys.write().push(key);
                }
            }
            if from.peek().is_none() {
                from.set(Some(motion::upload_tab().await));
            }
        });
    };

    // Waits for the way back, in this scope so that nothing stops it halfway.
    use_effect(move || {
        if !closing() {
            return;
        }
        spawn(async move {
            motion::settled(".upload-sheet").await;
            if *discard.peek() {
                for key in keys.peek().iter() {
                    activity::remove(*key);
                }
            }
            keys.write().clear();
            error.set(None);
            discard.set(false);
            from.set(None);
            closing.set(false);
        });
    });
    let mut close = move |drop_files: bool| {
        if !*closing.peek() {
            discard.set(drop_files);
            closing.set(true);
        }
    };

    let items: Vec<Item> = QUEUE
        .read()
        .iter()
        .filter(|i| keys.read().contains(&i.key))
        .cloned()
        .collect();
    let drafts = items.iter().filter(|i| i.status == Status::Draft).count();
    // Each file removed from the list: nothing left to ask about.
    let emptied = from().is_some() && drafts == 0;
    use_effect(use_reactive!(|emptied| {
        if emptied {
            close(false);
        }
    }));
    let send = move |_| {
        let shared = SHARED_TAGS.peek().clone();
        let chosen = keys.peek().clone();
        let problem = tag_problem(
            &shared,
            QUEUE
                .peek()
                .iter()
                .filter(|i| chosen.contains(&i.key))
                .map(|i| i.tags.as_str()),
        );
        if let Some(problem) = problem {
            error.set(Some(problem.to_string()));
            return;
        }
        activity::start_only(&chosen, &batch());
        close(false);
    };

    rsx! {
        if veil() {
            div {
                class: "drop-veil",
                ondragover: move |e| e.prevent_default(),
                ondrop: move |e| {
                    e.prevent_default();
                    take(dropped(&e));
                },
                div { class: "drop-veil-card",
                    Ico { icon: Icon::Upload, size: 40 }
                    strong { {t!("upload-drop-now")} }
                }
            }
        }
        if let Some((x, y)) = from() {
            div {
                class: if closing() { "sheet-scrim closing" } else { "sheet-scrim" },
                onclick: move |_| close(true),
                onkeydown: move |e| {
                    if e.key() == Key::Escape {
                        close(true);
                    }
                },
                ondragover: move |e| e.prevent_default(),
                ondrop: move |e| {
                    e.prevent_default();
                    take(dropped(&e));
                },
                div {
                    class: "card sheet upload-sheet",
                    style: "--from-x: {x:.1}px; --from-y: {y:.1}px",
                    role: "dialog",
                    aria_modal: "true",
                    aria_labelledby: "upload-sheet-title",
                    onclick: move |e| e.stop_propagation(),
                    div { class: "sheet-head",
                        h2 { id: "upload-sheet-title", class: "card-title", {t!("nav-upload")} }
                        button { class: "icon-btn", r#type: "button", aria_label: t!("cancel"), title: t!("cancel"),
                            onclick: move |_| close(true),
                            Ico { icon: Icon::Close }
                        }
                    }
                    if let Some(message) = error() {
                        div { class: "banner error", role: "alert", "{message}" }
                    }
                    ul { class: "queue",
                        for item in items {
                            QueueRow { key: "{item.key}", item }
                        }
                    }
                    SharedFields { focus: true }
                    div { class: "row-end",
                        button { class: "btn text", r#type: "button", onclick: move |_| close(true), {t!("cancel")} }
                        button { class: "btn", r#type: "button", disabled: drafts == 0, onclick: send,
                            Ico { icon: Icon::Upload }
                            {t!("upload-start", count = drafts)}
                        }
                    }
                }
            }
        }
    }
}

/// One file or address in the queue, with its own tags while it waits.
#[component]
fn QueueRow(item: Item) -> Element {
    let detail = match &item.source {
        Source::File(f) => human_size(f.size() as i64),
        Source::Url(_) => t!("upload-from-web"),
    };
    let key = item.key;
    let name = item.name();
    let removable = matches!(item.status, Status::Draft | Status::Ready(..));
    rsx! {
        li { class: "queue-item",
            div { class: "queue-main",
                div { class: "queue-name",
                    strong { title: "{name}", "{name}" }
                    span { class: "muted", "{detail}" }
                }
                match &item.status {
                    Status::Draft => rsx! { span { class: "pill", {t!("upload-queued")} } },
                    Status::Ready(..) => rsx! { span { class: "pill", {t!("upload-ready")} } },
                    Status::Uploading => rsx! { span { class: "pill busy", {t!("upload-uploading")} } },
                    Status::Done(id) => rsx! {
                        Link { class: "pill ok", to: Route::PostView { id: *id, q: String::new() }, {t!("post-number", id = id.to_string())} }
                    },
                    Status::Failed(_) => rsx! { span { class: "pill bad", {t!("upload-failed")} } },
                }
                if removable {
                    button {
                        class: "icon-btn",
                        aria_label: t!("upload-remove", name = name.clone()),
                        title: t!("upload-remove", name = name.clone()),
                        onclick: move |_| activity::remove(key),
                        Ico { icon: Icon::Close, size: 20 }
                    }
                }
            }
            if let Status::Failed(msg) = &item.status {
                p { class: "queue-error", role: "alert", "{msg}" }
            }
            if item.status == Status::Uploading && matches!(item.source, Source::File(_)) {
                TransferBar { progress: item.progress }
            }
            if item.status == Status::Draft {
                label { class: "queue-tags",
                    Ico { icon: Icon::Label, size: 18 }
                    input {
                        placeholder: t!("upload-file-tags-placeholder"),
                        aria_label: t!("upload-file-tags-label", name = name.clone()),
                        spellcheck: "false",
                        value: "{item.tags}",
                        oninput: move |e| activity::set_tags(key, e.value()),
                    }
                }
            } else if !item.tags.trim().is_empty() {
                span { class: "queue-tags muted", Ico { icon: Icon::Label, size: 18 } "{item.tags}" }
            }
        }
    }
}

/// An address for showing a chosen file before it is uploaded. Only the web
/// app has one: a webview isn't given the file itself.
#[cfg(target_arch = "wasm32")]
fn object_url(file: &FileData) -> Option<String> {
    let file = file.inner().downcast_ref::<web_sys::File>()?;
    web_sys::Url::create_object_url_with_blob(file).ok()
}

#[cfg(not(target_arch = "wasm32"))]
fn object_url(_file: &FileData) -> Option<String> {
    None
}

/// Lets the browser forget a file it was shown by `object_url`.
fn release_url(url: &str) {
    #[cfg(target_arch = "wasm32")]
    let _ = web_sys::Url::revoke_object_url(url);
    #[cfg(not(target_arch = "wasm32"))]
    let _ = url;
}

/// Whether an address ends like a picture's, so it can stand as its own
/// preview. A link to a page can't.
fn is_picture_url(url: &str) -> bool {
    let path = url.split(['?', '#']).next().unwrap_or_default();
    let ext = path
        .rsplit('.')
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase();
    matches!(
        ext.as_str(),
        "jpg" | "jpeg" | "png" | "gif" | "webp" | "avif" | "bmp"
    )
}

/// The site an address is on, as a tile's caption: `gelbooru.com`.
fn host_of(url: &str) -> String {
    let rest = url.split_once("://").map_or(url, |(_, rest)| rest);
    let host = rest.split(['/', '?', '#']).next().unwrap_or_default();
    host.trim_start_matches("www.").to_owned()
}

/// What a tile shows of its file: the picture when there is one to show, or
/// an icon with a few words.
#[component]
fn TileFace(src: Option<String>, icon: Icon, caption: String) -> Element {
    // A file the browser can't draw (a video, an odd format) falls back.
    let mut broken = use_signal(|| false);
    rsx! {
        div { class: "thumb-media",
            if let Some(src) = src.filter(|_| !broken()) {
                img {
                    src,
                    alt: "",
                    loading: "lazy",
                    decoding: "async",
                    "referrerpolicy": "no-referrer",
                    onerror: move |_| broken.set(true),
                }
            } else {
                div { class: "thumb-placeholder up-face",
                    Ico { icon, size: 32 }
                    span { class: "up-name", "{caption}" }
                }
            }
        }
    }
}

/// What a tile or the item popup shows of a file or an address: the file
/// itself on the web, a picture's own address, or the post's preview from the
/// booru it was found on. The flag says whether it is an object URL to give
/// back.
fn face_src(item: &Item, preview: bool) -> Option<(String, bool)> {
    if !preview {
        return None;
    }
    match (&item.source, &item.remote) {
        (Source::Url(_), Remote::Found(post)) if !post.preview_url.is_empty() => {
            let shown = post.sample_url.as_deref().unwrap_or(&post.preview_url);
            let client = store::client()?;
            Some((super::boorus::picture_src(&client, shown), false))
        }
        (Source::File(file), _) => object_url(file).map(|url| (url, true)),
        (Source::Url(url), _) if is_picture_url(url) => Some((url.clone(), false)),
        _ => None,
    }
}

/// What to show of an item. An object URL is made once and given back when
/// its holder goes.
fn use_face(item: &Item, preview: bool) -> Option<String> {
    let first = item.clone();
    let made = use_hook(move || face_src(&first, preview));
    let held = made.clone();
    use_drop(move || {
        if let Some((url, true)) = &held {
            release_url(url);
        }
    });
    // A post found after the tile was made brings a picture of its own.
    if let (Remote::Found(_), Some((url, false))) = (&item.remote, face_src(item, preview)) {
        return Some(url);
    }
    made.map(|(url, _)| url)
}

/// One file or address waiting to go, on its way, or done.
#[component]
fn FileTile(item: Item, place: usize, going: Option<usize>, onopen: EventHandler<u64>) -> Element {
    let preview = place < MAX_PREVIEWS;
    let key = item.key;
    let name = item.name();
    let shown = use_face(&item, preview);
    let (icon, caption) = match &item.source {
        Source::File(file) => (
            Icon::Gallery,
            format!("{name} · {}", human_size(file.size() as i64)),
        ),
        Source::Url(url) => (Icon::Link, host_of(url)),
    };
    let face = rsx! { TileFace { src: shown, icon, caption } };
    if let Status::Done(id) = item.status {
        return rsx! {
            Link { class: tile_class("thumb up-tile done", going), style: going_style(going), title: "{name}", to: Route::PostView { id, q: String::new() },
                {face}
                span { class: "up-mark ok", aria_label: t!("upload-done"), Ico { icon: Icon::Check, size: 18 } }
            }
        };
    }
    let removable = matches!(item.status, Status::Draft | Status::Ready(..));
    let tag_count = item.tags.split_whitespace().count();
    rsx! {
        div {
            class: tile_class(
                match &item.status {
                    Status::Failed(_) => "thumb up-tile failed",
                    Status::Uploading => "thumb up-tile busy",
                    _ => "thumb up-tile",
                },
                going,
            ),
            style: going_style(going),
            "data-up-key": "{key}",
            title: "{name}",
            if item.status == Status::Draft {
                button {
                    r#type: "button",
                    class: "up-open",
                    aria_label: t!("upload-open", name = name.clone()),
                    onclick: move |_| onopen.call(key),
                    {face}
                }
                if tag_count > 0 {
                    span { class: "up-count", aria_hidden: "true",
                        Ico { icon: Icon::Label, size: 16 }
                        "{tag_count}"
                    }
                }
            } else {
                {face}
            }
            if removable {
                CrossButton { label: t!("upload-remove", name = name.clone()), onclick: move |_| activity::remove(key) }
            }
            if item.status == Status::Uploading {
                div { class: "up-progress",
                    TransferBar { progress: item.progress.filter(|_| matches!(item.source, Source::File(_))) }
                }
            }
            if let Status::Failed(message) = &item.status {
                FailMark { message: message.clone() }
            }
        }
    }
}

/// How far the middle of an item's tile is from the middle of the window,
/// for its popup to come out of it and go back into it.
async fn tile_offset(key: u64) -> (f64, f64) {
    let mut eval = document::eval(&format!(
        r#"
        const tile = document.querySelector('[data-up-key="{key}"]')?.getBoundingClientRect();
        dioxus.send(tile && tile.width > 0
          ? [tile.left + tile.width / 2 - innerWidth / 2, tile.top + tile.height / 2 - innerHeight / 2]
          : [0, 0]);
        "#
    ));
    eval.recv().await.unwrap_or_default()
}

/// The cross in a tile's corner that takes it off the list.
#[component]
fn CrossButton(label: String, onclick: EventHandler<MouseEvent>) -> Element {
    rsx! {
        button {
            r#type: "button",
            class: "up-btn up-x",
            aria_label: "{label}",
            title: "{label}",
            onclick: move |e| onclick.call(e),
            Ico { icon: Icon::Close, size: 18 }
        }
    }
}

/// An on/off switch with its icon on the knob, so it isn't taken for a
/// button that does something once.
#[component]
fn Switch(icon: Icon, label: String, on: bool, onchange: EventHandler<bool>) -> Element {
    rsx! {
        button {
            r#type: "button",
            class: if on { "switch on" } else { "switch" },
            role: "switch",
            aria_checked: on,
            aria_label: "{label}",
            title: "{label}",
            onclick: move |_| onchange.call(!on),
            span { class: "switch-knob", Ico { icon, size: 16 } }
        }
    }
}

/// A file or an address, opened from its tile: a bigger picture and its own
/// tags. An address on a booru the server knows comes with the post's tags
/// there, to change like any others.
#[component]
fn ItemSheet(
    item_key: u64,
    from: (f64, f64),
    onremove: EventHandler<u64>,
    onclose: EventHandler<()>,
) -> Element {
    let key = item_key;
    let mut closing = use_signal(|| false);
    // Removed from here: it fades, as its tile is going too.
    let mut fading = use_signal(|| false);
    let live = QUEUE.read().iter().find(|i| i.key == key).cloned();
    // The item as last seen, to show while the popup goes once it is off
    // the list.
    let kept = use_hook(|| Rc::new(RefCell::new(None::<Item>)));
    if let Some(item) = &live {
        kept.replace(Some(item.clone()));
    }
    let first = live.clone();
    let made = use_hook(move || first.and_then(|i| face_src(&i, true)));
    let held = made.clone();
    use_drop(move || {
        if let Some((url, true)) = &held {
            release_url(url);
        }
    });
    use_effect(move || {
        if !closing() {
            return;
        }
        spawn(async move {
            motion::settled(".item-sheet").await;
            onclose.call(());
        });
    });
    let mut close = move || {
        if !*closing.peek() {
            closing.set(true);
        }
    };
    // Sent, or gone from the list some other way: nothing left to show.
    let gone = live.as_ref().is_none_or(|i| i.status != Status::Draft);
    let item = live.or_else(|| kept.borrow().clone());
    use_effect(use_reactive!(|gone| {
        if gone {
            close();
        }
    }));
    let Some(item) = item else {
        return rsx! {};
    };
    let name = item.name();
    let src = match (&item.remote, face_src(&item, true)) {
        (Remote::Found(_), Some((url, false))) => Some(url),
        _ => made.map(|(url, _)| url),
    };
    // Its light around it, where the picture leaves the box empty. The item
    // stays shown once seen (`kept`), so this hook runs on every render.
    use_effect(use_reactive!(|src| {
        if let Some(url) = src {
            motion::glow_box(".item-glow", &url);
        }
    }));
    let (icon, detail) = match &item.source {
        Source::File(file) => (Icon::Gallery, human_size(file.size() as i64)),
        Source::Url(url) => (Icon::Link, host_of(url)),
    };
    let problem = tag_problem("", [item.tags.as_str()]);
    let usual = rating_label(item.usual_rating(*RATING.read()));
    let usual = match &item.remote {
        Remote::Found(post) => t!(
            "upload-rating-site",
            site = super::boorus::site_name(&post.site),
            rating = usual
        ),
        _ => t!("upload-rating-usual", rating = usual),
    };
    let (x, y) = from;
    rsx! {
        div {
            class: match (closing(), fading()) {
                (true, true) => "sheet-scrim closing fading",
                (true, false) => "sheet-scrim closing",
                _ => "sheet-scrim",
            },
            onclick: move |_| close(),
            onkeydown: move |e| {
                if e.key() == Key::Escape {
                    close();
                }
            },
            div {
                class: "card sheet item-sheet",
                style: "--from-x: {x:.1}px; --from-y: {y:.1}px",
                role: "dialog",
                aria_modal: "true",
                aria_labelledby: "item-sheet-title",
                onclick: move |e| e.stop_propagation(),
                div { class: "sheet-head item-head",
                    h2 { id: "item-sheet-title", class: "card-title item-title", title: "{name}", "{name}" }
                    button {
                        r#type: "button",
                        class: "icon-btn",
                        aria_label: t!("upload-close"),
                        title: t!("upload-close"),
                        autofocus: true,
                        onclick: move |_| close(),
                        Ico { icon: Icon::Close }
                    }
                }
                div { class: "item-body",
                    div { class: "item-preview",
                        canvas { class: "item-glow", aria_hidden: "true" }
                        TileFace { src, icon, caption: detail.clone() }
                    }
                    div { class: "item-fields",
                        p { class: "muted item-detail", "{detail}" }
                        match &item.remote {
                            Remote::Asking => rsx! {
                                p { class: "item-source muted", role: "status",
                                    span { class: "spinner small", aria_hidden: "true" }
                                    {t!("upload-looking-up")}
                                }
                            },
                            Remote::Found(post) => rsx! {
                                p { class: "item-source", role: "status",
                                    Ico { icon: Icon::Check, size: 18 }
                                    {t!("upload-tags-from", site = super::boorus::site_name(&post.site))}
                                }
                            },
                            _ => rsx! {},
                        }
                        // Not a `label`: a click on the list would also
                        // press the button inside it and open it again.
                        div { class: "field",
                            span { {t!("rating")} }
                            Select {
                                class: "boxed item-rating",
                                aria_label: t!("rating"),
                                value: item.rating.map(|r| r.as_str().to_string()).unwrap_or_default(),
                                options: rating_choices(usual),
                                onchange: move |v: String| activity::set_rating(key, Rating::parse(&v)),
                            }
                        }
                        label { class: "field",
                            span { {t!("post-tags")} }
                            textarea {
                                class: "item-tags",
                                rows: "8",
                                placeholder: t!("upload-file-tags-placeholder"),
                                spellcheck: "false",
                                autocapitalize: "none",
                                value: "{item.tags}",
                                oninput: move |e| activity::set_tags(key, e.value()),
                            }
                        }
                        if let Some(problem) = problem {
                            p { class: "queue-error", role: "alert", "{problem}" }
                        } else {
                            p { class: "muted hint", {t!("post-tags-hint")} }
                        }
                    }
                }
                div { class: "item-foot",
                    if let Source::Url(url) = &item.source {
                        a { class: "btn text", href: "{url}", target: "_blank", rel: "noopener noreferrer",
                            Ico { icon: Icon::Link }
                            {t!("upload-open-link")}
                        }
                    }
                    button {
                        r#type: "button",
                        class: "btn text bin",
                        onclick: move |_| {
                            fading.set(true);
                            close();
                            onremove.call(key);
                        },
                        Ico { icon: Icon::Delete }
                        {t!("upload-remove-item")}
                    }
                }
            }
        }
    }
}

/// The ratings an item can have: first going with the rest (`usual` says
/// which that is), then each one.
fn rating_choices(usual: String) -> Vec<Choice> {
    let mut choices = vec![Choice::new("", usual)];
    choices.extend(
        Rating::ALL
            .into_iter()
            .map(|r| Choice::new(r.as_str(), rating_label(r))),
    );
    choices
}

/// Tiles taken off the list, kept on the grid while they go: the lists as
/// they were shown when the last ones went, each gone one with its place in
/// the order of going.
struct Leaving {
    items: Vec<(Item, Option<usize>)>,
    jobs: Vec<(ImportJob, Option<usize>)>,
    /// Which removal it is, so that the end of an earlier one leaves a later
    /// one's tiles be.
    round: u32,
}

type Shown = (Vec<(Item, Option<usize>)>, Vec<(ImportJob, Option<usize>)>);

/// The tiles on the grid: the lists as they are, with any that are going
/// still in their places.
fn shown(leaving: &Option<Leaving>, queue: &[Item], jobs: &[ImportJob]) -> Shown {
    match leaving {
        None => (
            queue.iter().map(|i| (i.clone(), None)).collect(),
            jobs.iter().map(|j| (j.clone(), None)).collect(),
        ),
        Some(leaving) => {
            let (items, next) = with_leaving(&leaving.items, queue, |i| i.key, 0);
            let (jobs, _) = with_leaving(&leaving.jobs, jobs, |j| j.id, next);
            (items, jobs)
        }
    }
}

/// `before` with each entry still in `now` as it is now, and each one gone
/// as it was, with its place in the order of going. One that has just gone
/// gets the next place from `first`. Whatever is new in `now` comes after.
/// Also gives the next free place.
fn with_leaving<T: Clone, K: Eq + Hash>(
    before: &[(T, Option<usize>)],
    now: &[T],
    key: impl Fn(&T) -> K,
    first: usize,
) -> (Vec<(T, Option<usize>)>, usize) {
    let live: HashMap<K, &T> = now.iter().map(|t| (key(t), t)).collect();
    let mut seen = HashSet::with_capacity(before.len());
    let mut next = first;
    let mut out = Vec::with_capacity(before.len().max(now.len()));
    for (old, going) in before {
        let k = key(old);
        match live.get(&k) {
            Some(current) => out.push(((*current).clone(), None)),
            None => {
                let place = going.unwrap_or_else(|| {
                    let place = next;
                    next = next.saturating_add(1);
                    place
                });
                out.push((old.clone(), Some(place)));
            }
        }
        seen.insert(k);
    }
    out.extend(
        now.iter()
            .filter(|t| !seen.contains(&key(t)))
            .map(|t| (t.clone(), None)),
    );
    (out, next)
}

/// Does `act`, which takes tiles off the list, at once, so it holds whatever
/// page comes next, and keeps those tiles on the grid while they go.
fn leave(mut leaving: Signal<Option<Leaving>>, act: impl FnOnce()) {
    let (items, jobs) = shown(&leaving.peek(), &QUEUE.peek(), &JOBS.peek());
    let round = leaving
        .peek()
        .as_ref()
        .map_or(0, |l| l.round.wrapping_add(1));
    act();
    leaving.set(Some(Leaving { items, jobs, round }));
}

fn tile_class(base: &str, going: Option<usize>) -> String {
    match going {
        Some(place) if place >= MAX_STAGGER => format!("{base} leaving far"),
        Some(_) => format!("{base} leaving"),
        None => base.to_owned(),
    }
}

/// When a going tile starts and how long it takes, by its place in the
/// order of going: the first few one after another at the usual pace, the
/// rest quicker and closer together. Past a screenful they go at once
/// (`tile_class`).
fn going_style(going: Option<usize>) -> String {
    let Some(place) = going else {
        return String::new();
    };
    let place = place.min(MAX_STAGGER);
    let slow = place.min(SLOW_TILES);
    let quick = place.saturating_sub(SLOW_TILES);
    let delay = slow
        .saturating_mul(SLOW_GAP_MS)
        .saturating_add(quick.saturating_mul(QUICK_GAP_MS));
    let pace = if place < SLOW_TILES {
        "--med"
    } else {
        "--fast"
    };
    format!("--d: {delay}ms; --len: var({pace})")
}

/// The tiles that go one after another; about a screenful.
const MAX_STAGGER: usize = 40;
/// The first tiles to go, which go at the usual pace.
const SLOW_TILES: usize = 3;
const SLOW_GAP_MS: usize = 28;
const QUICK_GAP_MS: usize = 10;

/// Waits for the tiles to finish going, or at most a second.
async fn tiles_gone() {
    let mut eval = document::eval(
        r#"
        await new Promise(requestAnimationFrame);
        const grid = document.querySelector(".up-grid");
        const going = grid?.getAnimations({ subtree: true }) ?? [];
        await Promise.race([
          Promise.allSettled(going.map((a) => a.finished)),
          new Promise((done) => setTimeout(done, 1000)),
        ]);
        dioxus.send(true);
        "#,
    );
    let _ = eval.recv::<bool>().await;
}

/// The mark on a tile that failed. Its reason shows on hover, and in a notice
/// when pressed, as a finger has no hover.
#[component]
fn FailMark(message: String) -> Element {
    let said = message.clone();
    rsx! {
        button {
            r#type: "button",
            class: "up-mark bad",
            title: "{message}",
            aria_label: "{message}",
            onclick: move |_| notify(said.clone()),
            Ico { icon: Icon::Close, size: 18 }
        }
    }
}

/// One address the server is importing, or has.
#[component]
fn JobTile(job: ImportJob, place: usize, going: Option<usize>) -> Element {
    let preview = place < MAX_PREVIEWS;
    let id = job.id;
    let name = job.source.clone().unwrap_or_else(|| job.url.clone());
    let src = (preview && is_picture_url(&job.url)).then(|| job.url.clone());
    let face = rsx! { TileFace { src, icon: Icon::Link, caption: host_of(&name) } };
    if let (JobState::Done, Some(post)) = (job.state, job.post_id) {
        return rsx! {
            Link { class: tile_class("thumb up-tile done", going), style: going_style(going), title: "{name}", to: Route::PostView { id: post, q: String::new() },
                {face}
                span { class: "up-mark ok", aria_label: t!("upload-done"), Ico { icon: Icon::Check, size: 18 } }
            }
        };
    }
    let note = job.error.clone().unwrap_or_default();
    rsx! {
        div {
            class: tile_class(
                match job.state {
                    JobState::Failed => "thumb up-tile failed",
                    JobState::Running => "thumb up-tile busy",
                    _ => "thumb up-tile",
                },
                going,
            ),
            style: going_style(going),
            title: if note.is_empty() { name.clone() } else { format!("{name}\n{note}") },
            {face}
            match job.state {
                JobState::Queued => rsx! {
                    CrossButton {
                        label: t!("upload-job-cancel"),
                        onclick: move |_| {
                            spawn(async move {
                                if let Err(message) = activity::cancel(id).await {
                                    notify(message);
                                }
                            });
                        },
                    }
                },
                JobState::Running => rsx! {
                    div { class: "up-progress", TransferBar { progress: job.progress } }
                },
                JobState::Done => rsx! {
                    span { class: "up-mark ok", aria_label: t!("upload-done"), Ico { icon: Icon::Check, size: 18 } }
                },
                JobState::Duplicate => rsx! {
                    span { class: "up-mark", title: "{note}", aria_label: t!("boorus-known"), Ico { icon: Icon::History, size: 18 } }
                },
                JobState::Failed => rsx! { FailMark { message: note.clone() } },
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_long_queue_slides_in_only_its_first_screenful() {
        // Regression: 3000 queued rows each ran an entrance at once, most of
        // them far below the screen.
        let css = include_str!("../../assets/main.css");
        assert!(css.contains(".queue-item:nth-child(n + 30) { animation: none; }"));
    }

    #[test]
    fn a_long_list_of_tiles_draws_only_what_is_near() {
        let css = include_str!("../../assets/main.css");
        assert!(css.contains(".up-tile { content-visibility: auto;"));
        assert!(css.contains(".up-tile:nth-child(n + 40) { animation: none; }"));
        assert!(MAX_PREVIEWS <= 200);
    }

    #[test]
    fn only_a_pictures_address_stands_as_its_own_preview() {
        assert!(is_picture_url("https://cdn.example/a/b.JPG?x=1"));
        assert!(is_picture_url("https://cdn.example/a.webp#top"));
        for other in [
            "https://gelbooru.com/index.php?page=post&s=view&id=7",
            "https://cdn.example/a.mp4",
            "https://cdn.example/folder.png/",
            "",
        ] {
            assert!(!is_picture_url(other), "{other}");
        }
        assert_eq!(
            host_of("https://www.gelbooru.com/index.php?page=post"),
            "gelbooru.com"
        );
        assert_eq!(host_of("http://127.0.0.1:8192/a.png"), "127.0.0.1:8192");
        assert_eq!(host_of("not an address"), "not an address");
    }

    #[test]
    fn the_picker_hides_no_files_except_for_androids_gallery() {
        #[cfg(target_os = "android")]
        assert_eq!(PICKER_ACCEPT, Some("image/*,video/*"));
        #[cfg(not(target_os = "android"))]
        assert_eq!(PICKER_ACCEPT, None);
    }

    #[test]
    fn a_batch_needs_good_tags_on_every_file() {
        assert!(tag_problem("sky blue", ["", "cloud"]).is_none());
        let long = "x".repeat(400);
        assert!(tag_problem(&long, ["cloud"]).is_some());
        assert!(tag_problem("sky", ["cloud", long.as_str()]).is_some());
        // Nothing to send, nothing wrong.
        assert!(tag_problem(&long, []).is_none());
    }

    #[test]
    fn the_popup_has_a_red_bin_and_the_tiles_a_plain_cross() {
        let css = include_str!("../../assets/main.css");
        // The bin at the popup's foot turns red when pointed at.
        assert!(css.contains(
            ".btn.text.bin:hover:not(:disabled), .btn.text.bin:focus-visible { background: var(--danger);"
        ));
        // The cross on a tile stays a cross, and the picture stays zoomed
        // while it is pointed at.
        assert!(!css.contains(".bin-x"));
        assert!(css.contains(".up-tile:hover .up-open .thumb-media { transform: scale(1.04); }"));
        // The popup comes out of its tile and goes back into it.
        assert!(css.contains(".item-sheet { width: min(820px, 100%); animation: from-tab"));
        assert!(css.contains(".sheet-scrim.closing .item-sheet { animation: to-tab"));
        // The tiles' own little tag field is gone.
        assert!(!css.contains(".up-tags {"));
        assert!(!css.contains(".up-tag {"));
    }

    #[test]
    fn the_toggles_are_switches_that_slide() {
        let css = include_str!("../../assets/main.css");
        assert!(css.contains(".switch.on .switch-knob { translate: 20px 0;"));
        assert!(
            css.contains(
                ".switch.on { border-color: var(--primary); background: var(--primary); }"
            )
        );
    }

    #[test]
    fn an_item_can_go_with_the_rest_or_have_its_own_rating() {
        let choices = rating_choices("Same as the rest (Safe)".into());
        assert_eq!(choices.len(), Rating::ALL.len() + 1);
        // The first one gives no rating of its own.
        assert_eq!(choices.first().map(|c| c.value.as_str()), Some(""));
        assert_eq!(Rating::parse(""), None);
        for (choice, rating) in choices.iter().skip(1).zip(Rating::ALL) {
            assert_eq!(Rating::parse(&choice.value), Some(rating));
        }
    }

    #[test]
    fn the_first_three_go_at_the_usual_pace_and_the_rest_quicker() {
        let css = include_str!("../../assets/main.css");
        assert!(css.contains("animation: tile-out var(--len, var(--med)) var(--ease) both; animation-delay: var(--d, 0ms);"));
        assert_eq!(going_style(None), "");
        assert_eq!(going_style(Some(0)), "--d: 0ms; --len: var(--med)");
        assert_eq!(going_style(Some(2)), "--d: 56ms; --len: var(--med)");
        assert_eq!(going_style(Some(3)), "--d: 84ms; --len: var(--fast)");
        assert_eq!(going_style(Some(5)), "--d: 104ms; --len: var(--fast)");
        // A long list doesn't keep the last tile waiting.
        assert_eq!(going_style(Some(5000)), going_style(Some(MAX_STAGGER)));
        assert_eq!(tile_class("thumb", Some(MAX_STAGGER)), "thumb leaving far");
        assert_eq!(tile_class("thumb", Some(1)), "thumb leaving");
        assert_eq!(tile_class("thumb", None), "thumb");
    }

    #[test]
    fn gone_tiles_keep_their_places_while_they_go() {
        // 1 to 4 were shown; 2 and 4 were taken off and 5 came after.
        let before: Vec<(u32, Option<usize>)> = (1..=4).map(|n| (n, None)).collect();
        let (shown, next) = with_leaving(&before, &[1, 3, 5], |n| *n, 0);
        assert_eq!(
            shown,
            [(1, None), (2, Some(0)), (3, None), (4, Some(1)), (5, None)]
        );
        assert_eq!(next, 2);
        // Taking 3 off while those go: it goes first among the new ones, and
        // the others keep their places, so their delays don't jump.
        let (again, _) = with_leaving(&shown, &[1, 5], |n| *n, 0);
        assert_eq!(
            again,
            [
                (1, None),
                (2, Some(0)),
                (3, Some(0)),
                (4, Some(1)),
                (5, None)
            ]
        );
        // Places go on from `first`, so the jobs follow the files.
        let (jobs, next) = with_leaving(&[(7, None), (8, None)], &[], |n: &u32| *n, 3);
        assert_eq!(jobs, [(7, Some(3)), (8, Some(4))]);
        assert_eq!(next, 5);
    }

    #[test]
    fn the_popup_follows_the_hand_and_fades_when_its_item_goes() {
        let css = include_str!("../../assets/main.css");
        assert!(css.contains(".app[data-hand=\"left\"] .item-head, .app[data-hand=\"left\"] .item-foot { flex-direction: row-reverse; }"));
        assert!(css.contains(".sheet-scrim.closing.fading .item-sheet { animation: sheet-out"));
    }

    #[test]
    fn the_bar_has_as_much_room_on_the_left_as_on_the_right() {
        // Regression: it kept the gallery bar's wider left side, made for
        // a count, in front of a round field.
        let css = include_str!("../../assets/main.css");
        assert!(css.contains(".up-bar { padding: 10px; }"));
    }

    #[test]
    fn the_popup_picture_has_its_light_around_it_unless_effects_are_reduced() {
        let css = include_str!("../../assets/main.css");
        assert!(css.contains(".item-glow.on { opacity: 0.85; }"));
        assert!(css.contains(
            ".app:is([data-saver=\"on\"], [data-effects=\"lite\"]) .item-glow { display: none; }"
        ));
    }

    #[test]
    fn the_upload_page_comes_in_without_moving() {
        // Regression: the page's entrance moved it, and the floating bar,
        // which is fixed, moved with it under the tiles for a moment.
        let css = include_str!("../../assets/main.css");
        assert!(css.contains(".page.upload { animation: fade-in var(--med) var(--ease); }"));
        assert!(css.contains("@keyframes fade-in { from { opacity: 0; } }"));
    }

    #[test]
    fn the_veil_grows_out_of_the_drop_box_and_back_into_it() {
        let css = include_str!("../../assets/main.css");
        let style = BoxRect::default().style();
        for name in ["--box-t", "--box-r", "--box-b", "--box-l", "--box-radius"] {
            assert!(style.contains(name), "{name}");
            assert!(css.contains(&format!("var({name})")), "{name}");
        }
        assert!(WATCH_PAGE_DROP.contains("return !own && (FILE_DRAG)"));
        // Its own watch, so the gallery's going doesn't stop it.
        assert!(!WATCH_PAGE_DROP.contains("__sbDrop"));
        assert!(WATCH_PAGE_DROP_OFF.contains("__sbPageDrop"));
    }

    #[test]
    fn drags_from_the_webviews_count_as_files() {
        assert!(WATCH_DROP.contains("FILE_DRAG"));
        assert!(FILE_DRAG.contains(r#"types.includes("Files")"#));
        // A link or a picture dragged out of a browser comes as an address.
        assert!(FILE_DRAG.contains("text/uri-list"));
        // What is dragged from this page itself never counts.
        assert!(WATCH_DROP.contains("return !own && (FILE_DRAG)"));
    }
}
