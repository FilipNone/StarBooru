//! The colour picker: a wheel for the pointer and a hex field for an exact
//! colour, which is also how the keyboard reaches it.

use dioxus::prelude::*;
use starbooru_core::color::{Rgb, parse_hex_color, to_hex, wheel_color};

use crate::icons::{Ico, Icon};

/// The wheel's width in pixels, as in `main.css`.
const WHEEL_SIZE: f64 = 176.0;

fn hex_text(value: Option<Rgb>) -> String {
    value
        .map(|rgb| format!("#{}", to_hex(rgb)))
        .unwrap_or_default()
}

/// Calls `on_pick` for a click on the wheel or a hex colour entered; `value`
/// fills the field.
#[component]
pub fn ColorPicker(value: Option<Rgb>, on_pick: EventHandler<Rgb>) -> Element {
    let mut typed = use_signal(|| hex_text(value));
    let mut invalid = use_signal(|| false);
    use_effect(use_reactive!(|value| {
        typed.set(hex_text(value));
        invalid.set(false);
    }));
    let preview = parse_hex_color(&typed()).map(to_hex);
    rsx! {
        div {
            class: "color-wheel",
            title: t!("color-wheel"),
            aria_hidden: "true",
            onclick: move |e: MouseEvent| {
                let at = e.element_coordinates();
                let radius = WHEEL_SIZE / 2.0;
                on_pick.call(wheel_color((at.x - radius) / radius, (at.y - radius) / radius));
            },
        }
        form {
            class: "field color-hex",
            onsubmit: move |e| {
                e.prevent_default();
                let rgb = parse_hex_color(&typed.peek());
                invalid.set(rgb.is_none());
                if let Some(rgb) = rgb {
                    on_pick.call(rgb);
                }
            },
            span {
                class: "color-preview",
                aria_hidden: "true",
                style: if let Some(hex) = &preview { "background-color: #{hex}" },
            }
            input {
                r#type: "text",
                maxlength: "7",
                spellcheck: "false",
                autocomplete: "off",
                placeholder: "#2060e0",
                aria_label: t!("color-hex"),
                aria_invalid: if invalid() { "true" } else { "false" },
                value: "{typed}",
                oninput: move |e| {
                    typed.set(e.value());
                    invalid.set(false);
                },
            }
            button {
                r#type: "submit",
                class: "icon-btn",
                title: t!("color-hex-apply"),
                aria_label: t!("color-hex-apply"),
                Ico { icon: Icon::Check, size: 20 }
            }
        }
        if invalid() {
            p { class: "field-error", role: "alert", {t!("color-hex-invalid")} }
        }
    }
}
