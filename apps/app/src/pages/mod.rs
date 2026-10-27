mod admin;
mod api_keys;
mod duplicates;
mod explore;
mod gallery;
mod help;
mod legal;
mod login;
mod post;
mod settings;
mod shell;
mod tags;
mod trash;
mod upload;

use dioxus::prelude::*;

pub use admin::Admin;
pub use duplicates::Duplicates;
pub use explore::Explore;
pub use gallery::Gallery;
pub use help::Help;
pub use legal::{LegalLinks, Privacy, Terms};
pub use post::PostView;
pub use settings::Settings;
pub use shell::Shell;
pub use tags::{TagPage, Tags};
pub use trash::Trash;
pub use upload::Upload;

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
