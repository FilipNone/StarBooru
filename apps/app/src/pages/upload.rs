//! Upload: files and addresses wait here for their tags, then go to the
//! app-wide queue (`activity`), so they keep going on other pages.

use dioxus::html::{FileData, HasFileData};
use dioxus::prelude::*;
use starbooru_core::{ImportJob, JobState, Rating};

use super::post::RatingPicker;
use crate::Route;
use crate::activity::{self, Item, JOBS, QUEUE, Source, Status};
use crate::icons::{Ico, Icon};
use crate::store;
use crate::util::{folder_tag, human_size, is_media_name, parse_urls};

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
    let url_import = use_resource(|| async move {
        match store::client() {
            Some(client) => client.info().await.map(|i| i.url_import).unwrap_or(true),
            None => true,
        }
    });
    use_hook(activity::refresh);

    let add = move |files: Vec<FileData>| {
        for file in files {
            activity::add(Source::File(file), String::new());
        }
    };
    // A folder: only images and videos, each tagged with its folder's name.
    let mut add_folder = move |files: Vec<FileData>| {
        let mut skipped = 0usize;
        for file in files {
            if !is_media_name(&file.name()) {
                skipped += 1;
                continue;
            }
            let tags = if folder_tags() {
                folder_tag(&file.path()).unwrap_or_default()
            } else {
                String::new()
            };
            activity::add(Source::File(file), tags);
        }
        notice.set((skipped > 0).then(|| t!("upload-skipped", count = skipped)));
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
                    add(e.files());
                },
                Ico { icon: Icon::Upload, size: 40 }
                strong { {t!("upload-drop")} }
                span { class: "muted", {t!("upload-or-choose")} }
                input {
                    r#type: "file",
                    multiple: true,
                    accept: "image/*,video/*",
                    class: "visually-hidden",
                    onchange: move |e| add(e.files()),
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
                        onchange: move |e| add_folder(e.files()),
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
                    Link { class: "btn tonal", to: Route::Boorus {},
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
                label { class: "field",
                    span { {t!("upload-shared-tags")} }
                    input {
                        placeholder: "artist_name landscape blue_sky",
                        spellcheck: "false",
                        value: "{SHARED_TAGS}",
                        oninput: move |e| *SHARED_TAGS.write() = e.value(),
                    }
                }
                div { class: "field",
                    span { {t!("rating")} }
                    RatingPicker { value: *RATING.read(), onchange: move |r| *RATING.write() = r }
                }
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
            if let Some(msg) = job.error.as_ref().filter(|_| job.state != JobState::Done) {
                p { class: if job.state == JobState::Failed { "queue-error" } else { "queue-note" }, "{msg}" }
            }
            if !tags.is_empty() {
                span { class: "queue-tags muted", Ico { icon: Icon::Label, size: 18 } "{tags}" }
            }
        }
    }
}
