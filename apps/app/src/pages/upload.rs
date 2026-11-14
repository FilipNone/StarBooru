//! Upload: files and addresses wait here for their tags, then go to the
//! app-wide queue (`activity`), so they keep going on other pages.

use crate::link::Link;
use dioxus::html::{FileData, HasFileData};
use dioxus::prelude::*;
use starbooru_core::tag::TagError;
use starbooru_core::{ImportJob, JobState, Rating, parse_tag_list};

use super::post::RatingPicker;
use super::{Choice, Select, TransferBar, notify};
use crate::Route;
use crate::activity::{self, Item, JOBS, QUEUE, Source, Status};
use crate::icons::{Ico, Icon};
use crate::motion;
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

/// How many tiles show a picture of their own file. A folder of thousands
/// would otherwise decode every picture in it at once.
const MAX_PREVIEWS: usize = 120;

#[component]
pub fn Upload() -> Element {
    let mut folder_tags = use_signal(|| true);
    let mut linking = use_signal(|| false);
    let mut url_text = use_signal(String::new);
    let mut notice = use_signal(|| None::<String>);
    // Hide URL import when the server has it off (shown while unknown).
    let url_import =
        use_resource(|| async move { store::server_info().await.is_none_or(|i| i.url_import) });
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
    let empty = QUEUE.read().is_empty() && JOBS.read().is_empty();
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
                button {
                    r#type: "button",
                    class: if folder_tags() { "icon-btn on" } else { "icon-btn" },
                    title: t!("upload-folder-tags"),
                    aria_label: t!("upload-folder-tags"),
                    aria_pressed: folder_tags(),
                    onclick: move |_| folder_tags.toggle(),
                    Ico { icon: Icon::Label }
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
                        class: "icon-btn",
                        title: t!("upload-clear"),
                        aria_label: t!("upload-clear"),
                        onclick: move |_| {
                            activity::clear();
                            spawn(async move {
                                if let Err(message) = activity::clear_jobs().await {
                                    notice.set(Some(message));
                                }
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
            div { class: if empty { "grid up-grid empty" } else { "grid up-grid" },
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
                for (index, item) in QUEUE.read().iter().enumerate() {
                    FileTile { key: "{item.key}", item: item.clone(), preview: index < MAX_PREVIEWS }
                }
                for (index, job) in JOBS.read().iter().enumerate() {
                    JobTile { key: "job-{job.id}", job: job.clone(), preview: index < MAX_PREVIEWS }
                }
            }
            if drafts > 0 {
                form {
                    class: "bulk-bar glass up-bar",
                    aria_label: t!("nav-upload"),
                    onsubmit: move |e| {
                        e.prevent_default();
                        activity::start(&SHARED_TAGS.peek(), *RATING.peek());
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
        activity::start_only(&chosen, &shared, *RATING.peek());
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

/// One file or address waiting to go, on its way, or done.
#[component]
fn FileTile(item: Item, preview: bool) -> Element {
    let key = item.key;
    let name = item.name();
    let source = item.source.clone();
    // Made once, and given back when the tile goes.
    let shown = use_hook(|| match (&source, preview) {
        (Source::File(file), true) => object_url(file).map(|url| (url, true)),
        (Source::Url(url), true) if is_picture_url(url) => Some((url.clone(), false)),
        _ => None,
    });
    let held = shown.clone();
    use_drop(move || {
        if let Some((url, true)) = &held {
            release_url(url);
        }
    });
    let mut tagging = use_signal(|| false);
    let (icon, caption) = match &item.source {
        Source::File(file) => (
            Icon::Gallery,
            format!("{name} · {}", human_size(file.size() as i64)),
        ),
        Source::Url(url) => (Icon::Link, host_of(url)),
    };
    let face = rsx! { TileFace { src: shown.map(|(url, _)| url), icon, caption } };
    if let Status::Done(id) = item.status {
        return rsx! {
            Link { class: "thumb up-tile done", title: "{name}", to: Route::PostView { id, q: String::new() },
                {face}
                span { class: "up-mark ok", aria_label: t!("upload-done"), Ico { icon: Icon::Check, size: 18 } }
            }
        };
    }
    let removable = matches!(item.status, Status::Draft | Status::Ready(..));
    let tagged = !item.tags.trim().is_empty();
    rsx! {
        div {
            class: match &item.status {
                Status::Failed(_) => "thumb up-tile failed",
                Status::Uploading => "thumb up-tile busy",
                _ => "thumb up-tile",
            },
            title: "{name}",
            {face}
            if removable {
                button {
                    r#type: "button",
                    class: "up-btn up-x",
                    aria_label: t!("upload-remove", name = name.clone()),
                    title: t!("upload-remove", name = name.clone()),
                    onclick: move |_| activity::remove(key),
                    Ico { icon: Icon::Close, size: 18 }
                }
            }
            if item.status == Status::Draft {
                button {
                    r#type: "button",
                    class: if tagged || tagging() { "up-btn up-tag on" } else { "up-btn up-tag" },
                    aria_label: t!("upload-file-tags-label", name = name.clone()),
                    title: if tagged { item.tags.clone() } else { t!("upload-file-tags-placeholder") },
                    aria_expanded: tagging(),
                    onclick: move |_| tagging.toggle(),
                    Ico { icon: Icon::Label, size: 18 }
                }
                if tagging() {
                    input {
                        class: "up-tags",
                        placeholder: t!("upload-file-tags-placeholder"),
                        aria_label: t!("upload-file-tags-label", name = name.clone()),
                        spellcheck: "false",
                        autocapitalize: "none",
                        autofocus: true,
                        value: "{item.tags}",
                        oninput: move |e| activity::set_tags(key, e.value()),
                        onkeydown: move |e: KeyboardEvent| {
                            if matches!(e.key(), Key::Enter | Key::Escape) {
                                e.prevent_default();
                                tagging.set(false);
                            }
                        },
                    }
                }
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
fn JobTile(job: ImportJob, preview: bool) -> Element {
    let id = job.id;
    let name = job.source.clone().unwrap_or_else(|| job.url.clone());
    let src = (preview && is_picture_url(&job.url)).then(|| job.url.clone());
    let face = rsx! { TileFace { src, icon: Icon::Link, caption: host_of(&name) } };
    if let (JobState::Done, Some(post)) = (job.state, job.post_id) {
        return rsx! {
            Link { class: "thumb up-tile done", title: "{name}", to: Route::PostView { id: post, q: String::new() },
                {face}
                span { class: "up-mark ok", aria_label: t!("upload-done"), Ico { icon: Icon::Check, size: 18 } }
            }
        };
    }
    let note = job.error.clone().unwrap_or_default();
    rsx! {
        div {
            class: match job.state {
                JobState::Failed => "thumb up-tile failed",
                JobState::Running => "thumb up-tile busy",
                _ => "thumb up-tile",
            },
            title: if note.is_empty() { name.clone() } else { format!("{name}\n{note}") },
            {face}
            match job.state {
                JobState::Queued => rsx! {
                    button {
                        r#type: "button",
                        class: "up-btn up-x",
                        aria_label: t!("upload-job-cancel"),
                        title: t!("upload-job-cancel"),
                        onclick: move |_| {
                            spawn(async move {
                                if let Err(message) = activity::cancel(id).await {
                                    notify(message);
                                }
                            });
                        },
                        Ico { icon: Icon::Close, size: 18 }
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
