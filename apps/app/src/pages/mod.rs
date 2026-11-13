mod admin;
mod admin_ai;
mod admin_backups;
mod api_keys;
mod booru_post;
mod boorus;
mod color_picker;
mod duplicates;
mod editor;
mod explore;
mod gallery;
mod help;
mod legal;
mod library_import;
mod login;
mod menu;
mod people;
mod post;
mod search_demo;
mod select;
mod settings;
mod share;
mod shell;
mod strength;
mod tags;
mod trash;
mod upload;

use crate::link::Link;
use dioxus::prelude::*;

pub use admin::Admin;
pub use booru_post::BooruPost;
pub use boorus::Boorus;
pub use color_picker::ColorPicker;
pub use duplicates::Duplicates;
pub use explore::Explore;
pub use gallery::Gallery;
pub use help::Help;
pub use legal::{LegalLinks, Privacy, Terms};
pub use post::PostView;
pub use select::{Choice, Select};
pub use settings::Settings;
pub use share::SharedPage;
pub use shell::{Shell, notify};
pub use tags::{TagPage, Tags};
pub use trash::Trash;
pub use upload::Upload;

use starbooru_core::TagCorrection;

use crate::Route;

#[component]
pub fn NotFound(segments: Vec<String>) -> Element {
    let path = segments.join("/");
    rsx! {
        div { class: "center-page",
            div { class: "card empty",
                h1 { {t!("notfound-title")} }
                p { class: "muted", {t!("notfound-body", path = format!("/{path}"))} }
                Link { class: "btn", to: Route::Gallery { q: String::new() }, {t!("notfound-back")} }
            }
        }
    }
}

/// Says which unknown tags a search corrected, with a link to the same search
/// as typed (`exact`).
#[component]
pub fn CorrectionBanner(corrected: Vec<TagCorrection>, exact: Route) -> Element {
    let tags = corrected
        .iter()
        .map(|c| format!("{} → {}", c.from, c.to))
        .collect::<Vec<_>>()
        .join(", ");
    rsx! {
        div { class: "banner info", role: "status",
            span { {t!("search-corrected", tags = tags)} }
            Link { class: "btn text", to: exact, {t!("search-exact")} }
        }
    }
}

/// Inline error banner with an optional retry action.
#[component]
pub fn ErrorBanner(message: String, onretry: Option<EventHandler<()>>) -> Element {
    rsx! {
        div { class: "banner error", role: "alert",
            span { "{message}" }
            if let Some(retry) = onretry {
                button { class: "btn text", onclick: move |_| retry.call(()), {t!("retry")} }
            }
        }
    }
}

/// A bar for a download or an upload under way, with how much is done and
/// how fast it goes. Without `progress`, or without a size to measure
/// against, the bar only shows that something is moving.
#[component]
pub fn TransferBar(progress: Option<starbooru_core::Transfer>) -> Element {
    let percent = progress.and_then(|p| p.percent());
    let label = progress
        .as_ref()
        .map(crate::util::transfer_label)
        .unwrap_or_default();
    rsx! {
        div { class: "transfer",
            span {
                class: if percent.is_some() { "meter" } else { "meter unknown" },
                role: "progressbar",
                aria_valuemin: "0",
                aria_valuemax: "100",
                aria_valuenow: percent.map(|p| p.to_string()),
                aria_valuetext: label.clone(),
                span { style: percent.map(|p| format!("width: {p}%")) }
            }
            if !label.is_empty() {
                span { class: "transfer-text", "{label}" }
            }
        }
    }
}

#[component]
pub fn Spinner() -> Element {
    rsx! { span { class: "spinner", role: "status", "aria-label": t!("loading") } }
}
