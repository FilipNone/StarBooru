use dioxus::html::FileData;
use dioxus::prelude::*;
use starbooru_client::Upload as UploadRequest;
use starbooru_core::{ImportRequest, Rating, parse_tag_list};

use super::post::RatingPicker;
use crate::Route;
use crate::icons::{Ico, Icon};
use crate::store;
use crate::util::{folder_tag, human_size, is_media_name, parse_urls};

#[derive(Clone, PartialEq)]
enum Status {
    Queued,
    Uploading,
    Done(i64),
    Failed(String),
}

#[derive(Clone, PartialEq)]
enum Source {
    File(FileData),
    Url(String),
}

#[derive(Clone, PartialEq)]
struct Item {
    key: u64,
    source: Source,
    /// Tags for this item only, on top of the shared ones.
    tags: String,
    status: Status,
}

impl Item {
    fn name(&self) -> String {
        match &self.source {
            Source::File(f) => f.name(),
            Source::Url(u) => u.clone(),
        }
    }
}

#[component]
pub fn Upload() -> Element {
    let mut items = use_signal(Vec::<Item>::new);
    let mut next_key = use_signal(|| 0u64);
    let mut tags = use_signal(String::new);
    let mut rating = use_signal(Rating::default);
    let mut running = use_signal(|| false);
    let mut dragging = use_signal(|| false);
    let folder_tags = use_signal(|| true);
    let mut url_text = use_signal(String::new);
    let mut notice = use_signal(|| None::<String>);
    // Hide URL import when the server has it off (shown while unknown).
    let url_import = use_resource(|| async move {
        match store::client() {
            Some(client) => client.info().await.map(|i| i.url_import).unwrap_or(true),
            None => true,
        }
    });

    let mut push = move |source: Source, tags: String| {
        let key = next_key();
        next_key += 1;
        items.write().push(Item {
            key,
            source,
            tags,
            status: Status::Queued,
        });
    };
    let mut add = move |files: Vec<FileData>| {
        for file in files {
            push(Source::File(file), String::new());
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
            push(Source::File(file), tags);
        }
        notice.set(match skipped {
            0 => None,
            1 => Some("Skipped 1 file that is not an image or video.".to_owned()),
            n => Some(format!("Skipped {n} files that are not images or videos.")),
        });
    };
    let mut add_urls = move || {
        let found = parse_urls(&url_text());
        if found.is_empty() {
            notice.set(Some(
                "Enter addresses that start with http:// or https://.".into(),
            ));
            return;
        }
        notice.set(None);
        for url in found {
            push(Source::Url(url), String::new());
        }
        url_text.set(String::new());
    };

    let start = move |_| {
        if running() {
            return;
        }
        let Some(client) = store::client() else {
            return;
        };
        running.set(true);
        let shared = tags();
        let rating = rating();
        spawn(async move {
            loop {
                let next = items.read().iter().position(|i| i.status == Status::Queued);
                let Some(index) = next else { break };
                let Some(item) = items.read().get(index).cloned() else {
                    break;
                };
                set_status(items, index, Status::Uploading);
                let status = match parse_tag_list(&format!("{shared} {}", item.tags)) {
                    Err(e) => Status::Failed(e.to_string()),
                    Ok(tag_list) => send(&client, item.source, tag_list, rating).await,
                };
                set_status(items, index, status);
            }
            running.set(false);
        });
    };

    let queued = items
        .read()
        .iter()
        .filter(|i| i.status == Status::Queued)
        .count();
    let show_url = url_import.read().unwrap_or(true);
    rsx! {
        div { class: "page narrow",
            h1 { class: "page-title", "Upload" }
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
                    add(e.data_transfer().files());
                },
                Ico { icon: Icon::Upload, size: 40 }
                strong { "Drop images or videos here" }
                span { class: "muted", "or click to choose files" }
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
                    "Choose a folder"
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
                    span { "Tag files with their folder name" }
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
                        placeholder: "Paste image or video addresses",
                        aria_label: "Image or video addresses",
                        spellcheck: "false",
                        value: "{url_text}",
                        oninput: move |e| url_text.set(e.value()),
                    }
                    button { class: "btn tonal", r#type: "submit", disabled: url_text().trim().is_empty(),
                        Ico { icon: Icon::Link }
                        "Add"
                    }
                }
            }
            if let Some(message) = notice() {
                div { class: "banner info", role: "status", "{message}" }
            }
            div { class: "card",
                label { class: "field",
                    span { "Tags for all files" }
                    input {
                        placeholder: "artist_name landscape blue_sky",
                        spellcheck: "false",
                        value: "{tags}",
                        oninput: move |e| tags.set(e.value()),
                    }
                }
                div { class: "field",
                    span { "Rating" }
                    RatingPicker { value: rating(), onchange: move |r| rating.set(r) }
                }
                div { class: "row-end",
                    button {
                        class: "btn text",
                        disabled: running() || items.read().is_empty(),
                        onclick: move |_| items.write().retain(|i| matches!(i.status, Status::Uploading)),
                        "Clear list"
                    }
                    button {
                        class: "btn",
                        disabled: running() || queued == 0,
                        onclick: start,
                        Ico { icon: Icon::Upload }
                        match (running(), queued) {
                            (true, _) => "Uploading…".to_owned(),
                            (false, 0) => "Upload".to_owned(),
                            (false, 1) => "Upload 1 file".to_owned(),
                            (false, n) => format!("Upload {n} files"),
                        }
                    }
                }
            }
            ul { class: "queue",
                for (index, item) in items.read().iter().enumerate() {
                    QueueRow { key: "{item.key}", items, index, item: item.clone() }
                }
            }
        }
    }
}

/// One file or address in the queue, with its own tags while it waits.
#[component]
fn QueueRow(items: Signal<Vec<Item>>, index: usize, item: Item) -> Element {
    let detail = match &item.source {
        Source::File(f) => human_size(f.size() as i64),
        Source::Url(_) => "From the web".to_owned(),
    };
    let queued = item.status == Status::Queued;
    rsx! {
        li { class: "queue-item",
            div { class: "queue-main",
                div { class: "queue-name",
                    strong { title: "{item.name()}", "{item.name()}" }
                    span { class: "muted", "{detail}" }
                }
                match &item.status {
                    Status::Queued => rsx! { span { class: "pill", "Queued" } },
                    Status::Uploading => rsx! { span { class: "pill busy", "Uploading" } },
                    Status::Done(id) => rsx! {
                        Link { class: "pill ok", to: Route::PostView { id: *id, q: String::new() }, "Post #{id}" }
                    },
                    Status::Failed(_) => rsx! { span { class: "pill bad", "Failed" } },
                }
            }
            if let Status::Failed(msg) = &item.status {
                p { class: "queue-error", role: "alert", "{msg}" }
            }
            if queued {
                label { class: "queue-tags",
                    Ico { icon: Icon::Label, size: 18 }
                    input {
                        placeholder: "Tags for this file only",
                        aria_label: "Tags for {item.name()}",
                        spellcheck: "false",
                        value: "{item.tags}",
                        oninput: move |e| {
                            let mut items = items;
                            if let Some(i) = items.write().get_mut(index) {
                                i.tags = e.value();
                            }
                        },
                    }
                }
            } else if !item.tags.trim().is_empty() {
                span { class: "queue-tags muted", Ico { icon: Icon::Label, size: 18 } "{item.tags}" }
            }
        }
    }
}

/// Uploads a file or asks the server to import an address.
async fn send(
    client: &starbooru_client::Client,
    source: Source,
    tags: Vec<String>,
    rating: Rating,
) -> Status {
    let result = match source {
        Source::File(file) => match file.read_bytes().await {
            Err(e) => return Status::Failed(format!("cannot read file: {e}")),
            Ok(bytes) => {
                client
                    .upload(UploadRequest {
                        file_name: file.name(),
                        bytes: bytes.to_vec(),
                        tags,
                        rating,
                        source: None,
                    })
                    .await
            }
        },
        Source::Url(url) => {
            client
                .import_url(&ImportRequest {
                    url,
                    tags,
                    rating,
                    source: None,
                })
                .await
        }
    };
    match result {
        Ok(post) => Status::Done(post.id),
        Err(e) => Status::Failed(e.to_string()),
    }
}

fn set_status(mut items: Signal<Vec<Item>>, index: usize, status: Status) {
    if let Some(item) = items.write().get_mut(index) {
        item.status = status;
    }
}
