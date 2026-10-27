//! StarBooru client: one Dioxus codebase for web, desktop and Android.
//!
//! `dx serve --web | --desktop | --android` (see docs/development.md).

mod icons;
mod pages;
mod store;
mod util;

use dioxus::prelude::*;

use crate::pages::{Gallery, NotFound, PostView, Settings, Shell, TagPage, Tags, Trash, Upload};
use crate::store::{PREFS, SESSION};

const MAIN_CSS: Asset = asset!("/assets/main.css");

#[derive(Routable, Clone, PartialEq, Debug)]
#[rustfmt::skip]
pub enum Route {
    #[layout(Shell)]
        #[route("/?:q")]
        Gallery { q: String },
        #[route("/post/:id")]
        PostView { id: i64 },
        #[route("/tags")]
        Tags {},
        #[route("/tag?:name")]
        TagPage { name: String },
        #[route("/upload")]
        Upload {},
        #[route("/settings")]
        Settings {},
        #[route("/trash")]
        Trash {},
    #[end_layout]
    #[route("/:..segments")]
    NotFound { segments: Vec<String> },
}

fn main() {
    #[cfg(feature = "desktop")]
    launch_desktop();
    #[cfg(not(feature = "desktop"))]
    dioxus::launch(App);
}

#[cfg(feature = "desktop")]
fn launch_desktop() {
    use dioxus::desktop::{Config, LogicalSize, WindowBuilder};
    let window = WindowBuilder::new()
        .with_title("StarBooru")
        .with_inner_size(LogicalSize::new(1280.0, 820.0))
        .with_min_inner_size(LogicalSize::new(360.0, 480.0));
    dioxus::LaunchBuilder::desktop()
        .with_cfg(Config::new().with_window(window).with_menu(None))
        .launch(App);
}

#[component]
fn App() -> Element {
    // Persist whenever the session or preferences change.
    use_effect(|| {
        let _session = SESSION.read();
        let _prefs = PREFS.read();
        store::persist();
    });
    let prefs = PREFS.read();
    rsx! {
        document::Title { "StarBooru" }
        document::Meta {
            name: "viewport",
            content: "width=device-width, initial-scale=1, viewport-fit=cover",
        }
        document::Stylesheet { href: MAIN_CSS }
        div {
            class: "app",
            "data-theme": prefs.theme.as_str(),
            style: "--seed-hue: {prefs.accent_hue}",
            Router::<Route> {}
        }
    }
}
