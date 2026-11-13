//! What the page tells Rust about pointers (desktop and Android).
//!
//! In the native builds every event a Rust handler listens to reaches it as a
//! synchronous request from the page, and the page waits for the answer. On a
//! phone that is 5 to 10 ms each time, on the page's main thread. Pointer
//! moves come dozens of times a second, and the interpreter's listener for
//! them sits on the root, where it stays once any element has listened. After
//! the viewer had been open once, every finger moving anywhere in the app paid
//! for a request nobody answered, which is what made swipes stutter.
//!
//! `bridge.js` leaves out what no Rust handler wants: an element whose Rust
//! handlers need pointers carries `data-pointer` ([`ALL`], or [`ZOOM`] for
//! moves only while it's zoomed or two fingers are down), and pointer events
//! outside such an element stay on the page. A mouse still reaches `pointerenter`
//! and `pointerleave` everywhere, for the gallery's hover previews.
//!
//! The web build has no such requests: its handlers run in the page.

/// Every pointer event the element sees goes to Rust.
pub const ALL: &str = "all";
/// Moves go only while the element has the `zoomed` class or two pointers are down.
pub const ZOOM: &str = "zoom";

/// The page's half.
#[cfg_attr(not(any(feature = "desktop", feature = "mobile")), allow(dead_code))]
pub const SCRIPT: &str = include_str!("bridge.js");

/// Puts the page's half in place. Call once, from the root component.
pub fn install() {
    #[cfg(any(feature = "desktop", feature = "mobile"))]
    dioxus::prelude::use_hook(|| {
        dioxus::prelude::document::eval(SCRIPT);
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_script_knows_both_markers() {
        assert!(SCRIPT.contains("[data-pointer]"));
        assert!(SCRIPT.contains(&format!("=== \"{ALL}\"")));
        // ZOOM is any other value: moves wait for a zoom or a second finger.
        assert!(SCRIPT.contains(r#"classList.contains("zoomed")"#));
        assert!(SCRIPT.contains("down.size > 1"));
    }

    #[test]
    fn the_script_only_holds_back_pointer_events() {
        // Anything else, clicks and keys included, passes as before.
        let filter = SCRIPT
            .split_once("switch (name)")
            .map(|(_, rest)| rest)
            .unwrap_or("");
        assert!(filter.contains("default:\n        return true;"));
        for name in [
            "pointermove",
            "pointerdown",
            "pointerup",
            "pointercancel",
            "pointerenter",
            "pointerleave",
        ] {
            assert!(filter.contains(&format!("case \"{name}\"")), "{name}");
        }
        assert!(!filter.contains("case \"click\""));
    }

    #[test]
    fn the_script_installs_once_and_only_over_a_native_interpreter() {
        assert!(SCRIPT.contains("dx.__sbBridge) return;"));
        assert!(SCRIPT.contains(r#"typeof dx.handleEvent !== "function""#));
    }

    #[test]
    fn the_elements_that_need_pointers_are_marked() {
        let post = include_str!("pages/post.rs");
        assert!(post.contains(r#""data-pointer": zoomable.then_some(crate::bridge::ZOOM)"#));
        let editor = include_str!("pages/editor.rs");
        assert!(editor.contains(r#""data-pointer": crate::bridge::ALL"#));
    }
}
