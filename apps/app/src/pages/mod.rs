mod explore;
mod gallery;
mod login;
mod post;
mod settings;
mod shell;
mod tags;
mod trash;
mod upload;

use dioxus::prelude::*;

pub use explore::Explore;
pub use gallery::Gallery;
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
                h1 { "Nothing here" }
                p { class: "muted", "There is no page at /{path}." }
                Link { class: "btn", to: Route::Gallery { q: String::new() }, "Back to the gallery" }
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
                button { class: "btn text", onclick: move |_| retry.call(()), "Retry" }
            }
        }
    }
}

#[component]
pub fn Spinner() -> Element {
    rsx! { span { class: "spinner", role: "status", "aria-label": "Loading" } }
}
