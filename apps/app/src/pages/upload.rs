//! Upload: files and addresses wait here for their tags, then go to the
//! app-wide queue (`activity`), so they keep going on other pages.

use crate::link::Link;
use dioxus::html::{FileData, HasFileData};
use dioxus::prelude::*;
use starbooru_core::tag::TagError;
use starbooru_core::{ImportJob, JobState, Rating, parse_tag_list};

use super::post::RatingPicker;
use super::{TransferBar, notify};
use crate::Route;
use crate::activity::{self, Item, JOBS, QUEUE, Source, Status};
use crate::icons::{Ico, Icon};
use crate::motion;
use crate::store;
use crate::util::{drop_is_urls, drop_urls, folder_tag, human_size, media_files, parse_urls};

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

#[component]
pub fn Upload() -> Element {
    let mut dragging = use_signal(|| false);
    let folder_tags = use_signal(|| true);
    let mut url_text = use_signal(String::new);
    let mut notice = use_signal(|| None::<String>);
    let mut job_error = use_signal(|| None::<String>);
    // Hide URL import when the server has it off (shown while unknown).
    let url_import =
        use_resource(|| async move { store::server_info().await.is_none_or(|i| i.url_import) });
    use_hook(activity::refresh);

    // Only images and videos. Those from a folder get its name as a tag.
    let add = move |files: Vec<FileData>, from_folder: bool| {
        spawn(async move {
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
    let uploading = QUEUE
        .read()
        .iter()
        .any(|i| matches!(i.status, Status::Ready(..) | Status::Uploading));
    let any_finished = JOBS.read().iter().any(|j| !j.state.is_active());
    let show_url = url_import.read().unwrap_or(true);
    rsx! {
        div { class: "page narrow",
            h1 { class: "page-title", {t!("nav-upload")} }
            label {
                class: if dragging() { "dropzone over" } else { "dropzone" },
                ondragover: move |e| {
                    e.prevent_default();
                    dragging.set(true);
                },
                ondragleave: move |_| dragging.set(false),
                ondrop: move |e| {
                    e.prevent_default();
                    dragging.set(false);
                    let (files, urls) = dropped(&e);
                    for url in urls {
                        activity::add(Source::Url(url), String::new());
                    }
                    add(files, false);
                },
                Ico { icon: Icon::Upload, size: 40 }
                strong { {t!("upload-drop")} }
                span { class: "muted", {t!("upload-or-choose")} }
                input {
                    r#type: "file",
                    multiple: true,
                    accept: PICKER_ACCEPT,
                    class: "visually-hidden",
                    onchange: move |e| add(e.files(), false),
                }
            }
            div { class: "import-row",
                label { class: "btn tonal",
                    Ico { icon: Icon::Folder }
                    {t!("upload-folder")}
                    input {
                        r#type: "file",
                        directory: true,
                        multiple: true,
                        class: "visually-hidden",
                        onchange: move |e| add(e.files(), true),
                    }
                }
                label { class: "check",
                    input {
                        r#type: "checkbox",
                        checked: folder_tags(),
                        onchange: move |e| {
                            let mut folder_tags = folder_tags;
                            folder_tags.set(e.checked());
                        },
                    }
                    span { {t!("upload-folder-tags")} }
                }
                if show_url {
                    Link { class: "btn tonal", to: Route::Boorus { q: String::new(), s: String::new() },
                        Ico { icon: Icon::Explore }
                        {t!("boorus-open")}
                    }
                }
            }
            if show_url {
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
                        value: "{url_text}",
                        oninput: move |e| url_text.set(e.value()),
                    }
                    button { class: "btn tonal", r#type: "submit", disabled: url_text().trim().is_empty(),
                        Ico { icon: Icon::Link }
                        {t!("add")}
                    }
                }
            }
            if let Some(message) = notice() {
                div { class: "banner info", role: "status", "{message}" }
            }
            div { class: "card",
                SharedFields {}
                div { class: "row-end",
                    button {
                        class: "btn text",
                        disabled: QUEUE.read().iter().all(|i| i.status == Status::Uploading),
                        onclick: move |_| activity::clear(),
                        {t!("upload-clear")}
                    }
                    button {
                        class: "btn",
                        disabled: drafts == 0,
                        onclick: move |_| activity::start(&SHARED_TAGS.peek(), *RATING.peek()),
                        Ico { icon: Icon::Upload }
                        {t!("upload-start", count = drafts)}
                    }
                }
            }
            if uploading {
                p { class: "muted upload-hint", {t!("upload-device-hint")} }
            }
            ul { class: "queue",
                for item in QUEUE.read().iter() {
                    QueueRow { key: "{item.key}", item: item.clone() }
                }
            }
            if !JOBS.read().is_empty() {
                section { class: "upload-jobs",
                    div { class: "section-head",
                        Ico { icon: Icon::Download }
                        h2 { {t!("upload-jobs-title")} }
                        if any_finished {
                            button {
                                class: "btn text",
                                onclick: move |_| {
                                    spawn(async move {
                                        job_error.set(activity::clear_jobs().await.err());
                                    });
                                },
                                {t!("upload-jobs-clear")}
                            }
                        }
                    }
                    p { class: "muted upload-hint", {t!("upload-jobs-hint")} }
                    if let Some(message) = job_error() {
                        div { class: "banner error", role: "alert", "{message}" }
                    }
                    ul { class: "queue",
                        for job in JOBS.read().iter() {
                            JobRow { key: "{job.id}", job: job.clone(), onerror: move |m| job_error.set(Some(m)) }
                        }
                    }
                }
            }
        }
    }
}

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

/// One address the server is importing, or has.
#[component]
fn JobRow(job: ImportJob, onerror: EventHandler<String>) -> Element {
    let id = job.id;
    let name = job.source.clone().unwrap_or_else(|| job.url.clone());
    let tags = job.tags.join(" ");
    let when = job
        .created_at
        .with_timezone(&chrono::Local)
        .format("%H:%M")
        .to_string();
    rsx! {
        li { class: "queue-item",
            div { class: "queue-main",
                div { class: "queue-name",
                    strong { title: "{job.url}", "{name}" }
                    span { class: "muted", "{when}" }
                }
                match (job.state, job.post_id) {
                    (JobState::Queued, _) => rsx! { span { class: "pill", {t!("upload-queued")} } },
                    (JobState::Running, _) => rsx! { span { class: "pill busy", {t!("upload-job-running")} } },
                    (JobState::Done, Some(post)) => rsx! {
                        Link { class: "pill ok", to: Route::PostView { id: post, q: String::new() }, {t!("post-number", id = post.to_string())} }
                    },
                    (JobState::Done, None) => rsx! { span { class: "pill ok", {t!("upload-done")} } },
                    (JobState::Duplicate, _) => rsx! { span { class: "pill", {t!("boorus-known")} } },
                    (JobState::Failed, _) => rsx! { span { class: "pill bad", {t!("upload-failed")} } },
                }
                if job.state == JobState::Queued {
                    button {
                        class: "btn text small",
                        onclick: move |_| {
                            spawn(async move {
                                if let Err(m) = activity::cancel(id).await {
                                    onerror.call(m);
                                }
                            });
                        },
                        {t!("upload-job-cancel")}
                    }
                }
            }
            if job.state == JobState::Running {
                TransferBar { progress: job.progress }
            }
            if let Some(msg) = job.error.as_ref().filter(|_| job.state != JobState::Done) {
                p { class: if job.state == JobState::Failed { "queue-error" } else { "queue-note" }, "{msg}" }
            }
            if !tags.is_empty() {
                span { class: "queue-tags muted", Ico { icon: Icon::Label, size: 18 } "{tags}" }
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
    fn drags_from_the_webviews_count_as_files() {
        assert!(WATCH_DROP.contains("FILE_DRAG"));
        assert!(FILE_DRAG.contains(r#"types.includes("Files")"#));
        // A link or a picture dragged out of a browser comes as an address.
        assert!(FILE_DRAG.contains("text/uri-list"));
        // What is dragged from this page itself never counts.
        assert!(WATCH_DROP.contains("return !own && (FILE_DRAG)"));
    }
}
