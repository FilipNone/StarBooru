//! StarBooru client: one Dioxus codebase for web, desktop and Android.
//!
//! `dx serve --web | --desktop | --android` (see docs/development.md).

#[macro_use]
mod i18n;
mod activity;
#[cfg(target_os = "android")]
mod back;
mod brand;
mod bridge;
#[rustfmt::skip]
mod brand_paths;
mod cache;
mod download;
mod gesture;
mod icons;
mod layout;
mod link;
mod motion;
mod oidc;
mod pace;
mod pages;
mod paste;
mod power;
mod route;
mod store;
mod system;
mod util;
mod zoom;

use dioxus::prelude::*;

pub use crate::route::Route;
use crate::store::{PREFS, SESSION};

/// On the web, `dx` writes the link to it into `index.html`, so the page has
/// its styles before the app has drawn anything.
const MAIN_CSS: Asset = asset!(
    "/assets/main.css",
    AssetOptions::css().with_static_head(true)
);
const FAVICON: Asset = asset!("/assets/brand/favicon.svg");
const FAVICON_PNG: Asset = asset!("/assets/brand/favicon-32.png");
const TOUCH_ICON: Asset = asset!("/assets/brand/apple-touch-icon.png");

fn main() {
    #[cfg(feature = "desktop")]
    launch_desktop();
    #[cfg(not(feature = "desktop"))]
    dioxus::launch(App);
}

#[cfg(feature = "desktop")]
fn launch_desktop() {
    use dioxus::desktop::tao::window::Icon;
    use dioxus::desktop::{Config, LogicalSize, WindowBuilder, icon_from_memory};
    let window = WindowBuilder::new()
        .with_title("StarBooru")
        .with_inner_size(LogicalSize::new(1280.0, 820.0))
        .with_min_inner_size(LogicalSize::new(360.0, 480.0));
    let mut config = Config::new()
        .with_window(window)
        .with_menu(None)
        .with_disable_dma_buf_on_wayland(system::dmabuf_off_here());
    // A bad icon is no reason not to start.
    if let Ok(icon) = icon_from_memory::<Icon>(include_bytes!("../assets/brand/window-icon.png")) {
        config = config.with_icon(icon);
    }
    dioxus::LaunchBuilder::desktop()
        .with_cfg(config)
        .launch(App);
}

#[component]
fn App() -> Element {
    cache::register();
    bridge::install();
    power::watch();
    pace::watch();
    system::watch();
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
        // The web page links it already (`index.html`). A second link would
        // have the browser match every rule twice.
        if cfg!(not(feature = "web")) {
            document::Stylesheet { href: MAIN_CSS }
        }
        document::Link { rel: "icon", r#type: "image/svg+xml", href: FAVICON }
        document::Link { rel: "icon", r#type: "image/png", sizes: "32x32", href: FAVICON_PNG }
        document::Link { rel: "apple-touch-icon", href: TOUCH_ICON }
        div {
            class: "app",
            lang,
            "data-theme": prefs.theme.as_str(),
            "data-hand": prefs.hand.as_str(),
            "data-tiles": if prefs.flush_tiles { "flush" } else { "spaced" },
            "data-saver": if power::active() { "on" } else { "off" },
            "data-effects": if pace::lite() { "lite" } else { "full" },
            style: "--seed-hue: {system::seed_hue(&prefs)}",
            Router::<Route> {}
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn pressing_something_draws_no_box_around_it() {
        let css = include_str!("../assets/main.css");
        assert!(css.contains("html { -webkit-tap-highlight-color: transparent; }"));
    }

    /// What `name: ` is set to in the first rule of `css` that starts with `rule`.
    fn token<'a>(css: &'a str, rule: &str, name: &str) -> &'a str {
        let (_, body) = css.split_once(rule).unwrap_or_else(|| panic!("{rule}"));
        let (body, _) = body.split_once('}').unwrap();
        let (_, value) = body
            .split_once(&format!("{name}: "))
            .unwrap_or_else(|| panic!("{name} in {rule}"));
        value.split_once(';').unwrap().0
    }

    #[test]
    fn the_loading_screen_is_in_the_apps_colours() {
        // It can't use main.css's tokens, as it paints before that file has come.
        let css = include_str!("../assets/main.css");
        let page = include_str!("../index.html");
        for name in ["--canvas", "--primary-container", "--primary"] {
            assert_eq!(
                token(page, "#boot {", name),
                token(css, "\n.app {", name),
                "{name}, light"
            );
            let dark = token(css, r#".app[data-theme="dark"] {"#, name);
            assert_eq!(
                token(page, r#"#boot[data-theme="dark"] {"#, name),
                dark,
                "{name}, dark"
            );
            assert_eq!(
                token(page, r#"#boot:not([data-theme="light"]) {"#, name),
                dark,
                "{name}, dark by the system"
            );
        }
        assert_eq!(token(page, "#boot {", "--h"), "285");
        assert!(css.contains("--h: var(--seed-hue, 285);"));
    }

    #[test]
    fn the_loading_screen_stays_until_the_app_is_styled_and_on_the_page() {
        // Regression: the page was white until the app started, and then showed
        // as bare text and blue links until its stylesheet had loaded.
        let page = include_str!("../index.html");
        // Empty, with not even a space in it, or `:empty` never matches.
        assert!(
            page.contains(
                "<div id=\"main\"></div>\n    <div id=\"boot\" aria-hidden=\"true\"></div>"
            )
        );
        let css = include_str!("../assets/main.css");
        assert!(css.contains("#main:not(:empty) + #boot { opacity: 0; visibility: hidden;"));
        // The stylesheet is linked by the page, after the loading screen.
        assert!(page.find("id=\"boot\"").unwrap() < page.find("{style_include}").unwrap());
        let source = include_str!("main.rs");
        assert!(source.contains("AssetOptions::css().with_static_head(true)"));
    }

    #[test]
    fn the_glass_blurs_in_a_release_build() {
        // Regression: with both spellings, the minifier in `dx bundle` kept only
        // `-webkit-backdrop-filter`, and Chromium showed no blur.
        let css = include_str!("../assets/main.css");
        assert!(css.contains("  backdrop-filter: blur("));
        assert!(!css.contains("-webkit-backdrop-filter"));
    }
}
