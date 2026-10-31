mod admin;
mod admin_ai;
mod admin_backups;
mod api_keys;
mod boorus;
mod duplicates;
mod editor;
mod explore;
mod gallery;
mod help;
mod legal;
mod library_import;
mod login;
mod people;
mod post;
mod select;
mod settings;
mod share;
mod shell;
mod tags;
mod trash;
mod upload;

use dioxus::prelude::*;

pub use admin::Admin;
pub use boorus::Boorus;
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

#[component]
pub fn Spinner() -> Element {
    rsx! { span { class: "spinner", role: "status", "aria-label": t!("loading") } }
}
