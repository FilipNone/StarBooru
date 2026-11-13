//! A link to another page of the app, in place of the router's `Link`.
//!
//! The router's `Link` listens for its own mounting even when nobody asked it
//! to. In the native builds each such event is a synchronous request from the
//! page to Rust (see bridge.rs), 5 to 10 ms on a phone, and the Tags page with
//! its 35 links spent close to 200 ms of its entrance on them. This one does
//! the same on a click and nothing on mounting.
//!
//! Files that use it `use crate::link::Link;`, which takes the place of the one
//! in `dioxus::prelude::*`.

use dioxus::prelude::*;

#[derive(Props, Clone, PartialEq)]
pub struct LinkProps {
    /// Where it goes.
    #[props(into)]
    pub to: NavigationTarget,
    #[props(into)]
    pub class: Option<String>,
    /// Added to `class` while `to` is the page on screen.
    #[props(into)]
    pub active_class: Option<String>,
    /// Runs on a click, after the page has been asked to change.
    pub onclick: Option<EventHandler<MouseEvent>>,
    #[props(extends = GlobalAttributes, extends = a)]
    pub attributes: Vec<Attribute>,
    pub children: Element,
}

/// The `class` attribute: `class`, plus `active_class` on the current page.
pub fn classes(class: Option<&str>, active_class: Option<&str>, current: bool) -> Option<String> {
    let parts: Vec<&str> = [class, active_class.filter(|_| current)]
        .into_iter()
        .flatten()
        .filter(|c| !c.is_empty())
        .collect();
    (!parts.is_empty()).then(|| parts.join(" "))
}

/// Whether a click is the plain one the app follows itself. A modifier or
/// another button is left to the webview, as the router's `Link` does.
fn plain(event: &MouseEvent) -> bool {
    event.modifiers().is_empty()
        && event.trigger_button() == Some(dioxus::html::input_data::MouseButton::Primary)
}

#[allow(non_snake_case)]
pub fn Link(props: LinkProps) -> Element {
    let LinkProps {
        to,
        class,
        active_class,
        onclick,
        attributes,
        children,
    } = props;
    let router = router();
    let here = router.full_route_string();
    let (href, external) = match &to {
        NavigationTarget::Internal(url) => (router.prefix().unwrap_or_default() + url, false),
        NavigationTarget::External(url) => (url.clone(), true),
    };
    let current = matches!(&to, NavigationTarget::Internal(url) if *url == here);
    let class = classes(class.as_deref(), active_class.as_deref(), current);
    rsx! {
        a {
            href,
            class,
            rel: external.then_some("noopener noreferrer"),
            aria_current: current.then_some("page"),
            onclick: move |event: MouseEvent| {
                if !plain(&event) || external {
                    return;
                }
                event.prevent_default();
                router.push(to.clone());
                if let Some(handler) = onclick {
                    handler.call(event);
                }
            },
            ..attributes,
            {children}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_active_class_only_on_the_current_page() {
        assert_eq!(
            classes(Some("nav-item"), Some("active"), true).as_deref(),
            Some("nav-item active")
        );
        assert_eq!(
            classes(Some("nav-item"), Some("active"), false).as_deref(),
            Some("nav-item")
        );
        assert_eq!(
            classes(None, Some("active"), true).as_deref(),
            Some("active")
        );
        assert_eq!(classes(None, None, true), None);
        assert_eq!(classes(Some(""), Some("active"), false), None);
    }

    #[test]
    fn no_link_listens_for_its_mounting() {
        // Every file that writes a `Link` uses this one, which has no `onmounted`.
        let sources = [
            ("pages/boorus.rs", include_str!("pages/boorus.rs")),
            ("pages/duplicates.rs", include_str!("pages/duplicates.rs")),
            ("pages/explore.rs", include_str!("pages/explore.rs")),
            ("pages/gallery.rs", include_str!("pages/gallery.rs")),
            ("pages/help.rs", include_str!("pages/help.rs")),
            ("pages/legal.rs", include_str!("pages/legal.rs")),
            ("pages/mod.rs", include_str!("pages/mod.rs")),
            ("pages/people.rs", include_str!("pages/people.rs")),
            ("pages/post.rs", include_str!("pages/post.rs")),
            ("pages/settings.rs", include_str!("pages/settings.rs")),
            ("pages/shell.rs", include_str!("pages/shell.rs")),
            ("pages/tags.rs", include_str!("pages/tags.rs")),
            ("pages/trash.rs", include_str!("pages/trash.rs")),
            ("pages/upload.rs", include_str!("pages/upload.rs")),
            ("oidc.rs", include_str!("oidc.rs")),
        ];
        for (name, src) in sources {
            assert!(
                src.contains("use crate::link::Link;"),
                "{name} uses the router's Link"
            );
        }
        assert!(
            !include_str!("link.rs")
                .split("mod tests")
                .next()
                .unwrap_or("")
                .contains("onmounted:")
        );
    }
}
