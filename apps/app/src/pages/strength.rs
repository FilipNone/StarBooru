use dioxus::prelude::*;
use starbooru_core::{password_strength, strength_hue};

/// Wraps a new-password input and tints it by how hard `value` is to guess.
#[component]
pub fn StrengthField(value: String, username: String, children: Element) -> Element {
    let rated = !value.is_empty();
    let score = password_strength(&value, &username);
    let hue = strength_hue(score);
    let word = if score < 0.4 {
        t!("password-strength-weak")
    } else if score < 0.75 {
        t!("password-strength-fair")
    } else {
        t!("password-strength-strong")
    };
    rsx! {
        div {
            class: if rated { "pw-field rated" } else { "pw-field" },
            style: "--pw-hue: {hue:.1}",
            {children}
        }
        if rated {
            span { class: "visually-hidden", aria_live: "polite", {word} }
        }
    }
}
