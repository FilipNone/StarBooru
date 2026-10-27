use dioxus::html::FileData;
use dioxus::prelude::*;
use starbooru_client::Upload as UploadRequest;
use starbooru_core::Rating;

use super::post::RatingPicker;
use crate::Route;
use crate::icons::{Ico, Icon};
use crate::store;
use crate::util::human_size;

#[derive(Clone, PartialEq)]
enum Status {
    Queued,
    Uploading,
    Done(i64),
    Failed(String),
}

#[derive(Clone)]
struct Item {
    key: u64,
    file: FileData,
    status: Status,
}

#[component]
pub fn Upload() -> Element {
    let mut items = use_signal(Vec::<Item>::new);
    let mut next_key = use_signal(|| 0u64);
    let mut tags = use_signal(String::new);
    let mut rating = use_signal(Rating::default);
    let mut running = use_signal(|| false);
    let mut dragging = use_signal(|| false);

    let mut add = move |files: Vec<FileData>| {
        for file in files {
            let key = next_key();
            next_key += 1;
            items.write().push(Item {
                key,
                file,
                status: Status::Queued,
            });
        }
    };

    let start = move |_| {
        if running() {
            return;
        }
        let Some(client) = store::client() else {
            return;
        };
        running.set(true);
        let tag_list: Vec<String> = tags().split_whitespace().map(str::to_owned).collect();
        let rating = rating();
        spawn(async move {
            loop {
                let next = items.read().iter().position(|i| i.status == Status::Queued);
                let Some(index) = next else { break };
                let Some(file) = items.read().get(index).map(|i| i.file.clone()) else {
                    break;
                };
                set_status(items, index, Status::Uploading);
                let status = match file.read_bytes().await {
                    Err(e) => Status::Failed(format!("cannot read file: {e}")),
                    Ok(bytes) => {
                        let request = UploadRequest {
                            file_name: file.name(),
                            bytes: bytes.to_vec(),
                            tags: tag_list.clone(),
                            rating,
                            source: None,
                        };
                        match client.upload(request).await {
                            Ok(post) => Status::Done(post.id),
                            Err(e) => Status::Failed(e.to_string()),
                        }
                    }
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
            div { class: "card",
                label { class: "field",
                    span { "Tags for these files" }
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
                for item in items.read().iter() {
                    li { key: "{item.key}", class: "queue-item",
                        div { class: "queue-name",
                            strong { "{item.file.name()}" }
                            span { class: "muted", "{human_size(item.file.size() as i64)}" }
                        }
                        match &item.status {
                            Status::Queued => rsx! { span { class: "pill", "Queued" } },
                            Status::Uploading => rsx! { span { class: "pill busy", "Uploading" } },
                            Status::Done(id) => rsx! {
                                Link { class: "pill ok", to: Route::PostView { id: *id }, "Post #{id}" }
                            },
                            Status::Failed(msg) => rsx! { span { class: "pill bad", title: "{msg}", "{msg}" } },
                        }
                    }
                }
            }
        }
    }
}

fn set_status(mut items: Signal<Vec<Item>>, index: usize, status: Status) {
    if let Some(item) = items.write().get_mut(index) {
        item.status = status;
    }
}
