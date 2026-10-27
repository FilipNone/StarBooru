//! StarBooru client: one Dioxus codebase for web, desktop and Android.
//!
//! `dx serve --web | --desktop | --android` (see docs/development.md).

#[macro_use]
mod i18n;
mod cache;
mod download;
mod icons;
mod oidc;
mod pages;
mod store;
mod util;
mod zoom;

use dioxus::prelude::*;

use crate::oidc::OidcReturn;
use crate::pages::{
    Admin, Duplicates, Explore, Gallery, Help, NotFound, PostView, Privacy, Settings, SharedPage,
    Shell, TagPage, Tags, Terms, Trash, Upload,
};
use crate::store::{PREFS, SESSION};

const MAIN_CSS: Asset = asset!("/assets/main.css");

#[derive(Routable, Clone, PartialEq, Debug)]
#[rustfmt::skip]
pub enum Route {
    #[layout(Shell)]
        #[route("/?:q")]
        Gallery { q: String },
        #[route("/explore?:q")]
        Explore { q: String },
        #[route("/post/:id?:q")]
        PostView { id: i64, q: String },
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
        #[route("/duplicates")]
        Duplicates {},
        #[route("/help")]
        Help {},
        #[route("/admin")]
        Admin {},
    #[end_layout]
    #[route("/oidc?:oidc_code&:oidc_error")]
    OidcReturn { oidc_code: String, oidc_error: String },
    #[route("/s/:token")]
    SharedPage { token: String },
    #[route("/privacy")]
    Privacy {},
    #[route("/terms")]
    Terms {},
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
    cache::register();
    // Persist whenever the session or preferences change.
    use_effect(|| {
        let _session = SESSION.read();
        let _prefs = PREFS.read();
        store::persist();
    });
    // Follow a language change from Settings.
    use_effect(|| {
        let lang = i18n::resolve(PREFS.read().language);
        if *i18n::LANG.peek() != lang {
            *i18n::LANG.write() = lang;
        }
    });
    let prefs = PREFS.read();
    let lang = i18n::LANG.read().code();
    rsx! {
        document::Title { "StarBooru" }
        document::Meta {
            name: "viewport",
            content: "width=device-width, initial-scale=1, viewport-fit=cover",
        }
        document::Stylesheet { href: MAIN_CSS }
        div {
            class: "app",
            lang,
            "data-theme": prefs.theme.as_str(),
            "data-hand": prefs.hand.as_str(),
            style: "--seed-hue: {prefs.accent_hue}",
            Router::<Route> {}
        }
    }
}
