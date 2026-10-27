use dioxus::prelude::*;

use crate::icons::{Ico, Icon};
use crate::store::{self, PREFS, SESSION, Theme};

/// Accent presets (OKLCH hues).
const SWATCHES: [(u16, &str); 8] = [
    (285, "Violet"),
    (330, "Pink"),
    (20, "Coral"),
    (65, "Amber"),
    (140, "Green"),
    (190, "Teal"),
    (245, "Blue"),
    (0, "Red"),
];

#[component]
pub fn Settings() -> Element {
    let info = use_resource(|| async move {
        let client = store::client()?;
        client.info().await.ok()
    });
    let prefs = PREFS.read().clone();
    let session = SESSION.read().clone();

    rsx! {
        div { class: "page narrow",
            h1 { class: "page-title", "Settings" }

            section { class: "card",
                h2 { class: "card-title", "Appearance" }
                div { class: "field",
                    span { "Theme" }
                    div { class: "segmented", role: "radiogroup", aria_label: "Theme",
                        for theme in Theme::ALL {
                            button {
                                key: "{theme.as_str()}",
                                role: "radio",
                                aria_checked: if prefs.theme == theme { "true" } else { "false" },
                                class: if prefs.theme == theme { "seg on" } else { "seg" },
                                onclick: move |_| PREFS.write().theme = theme,
                                "{theme.label()}"
                            }
                        }
                    }
                }
                div { class: "field",
                    span { "Accent colour" }
                    div { class: "swatches",
                        for (hue, name) in SWATCHES {
                            button {
                                key: "{hue}",
                                class: if prefs.accent_hue == hue { "swatch on" } else { "swatch" },
                                style: "--swatch-hue: {hue}",
                                aria_label: "{name}",
                                title: "{name}",
                                onclick: move |_| PREFS.write().accent_hue = hue,
                                if prefs.accent_hue == hue { Ico { icon: Icon::Check, size: 18 } }
                            }
                        }
                    }
                    input {
                        r#type: "range",
                        min: "0",
                        max: "359",
                        aria_label: "Custom accent hue",
                        value: "{prefs.accent_hue}",
                        oninput: move |e| {
                            if let Ok(h) = e.value().parse::<u16>() {
                                PREFS.write().accent_hue = h.min(359);
                            }
                        },
                    }
                }
            }

            if let Some(s) = session {
                section { class: "card",
                    h2 { class: "card-title", "Account" }
                    dl { class: "meta",
                        dt { "Signed in as" } dd { "{s.user.username}" }
                        dt { "Role" } dd { "{s.user.role.as_str()}" }
                        dt { "Server" } dd { class: "break", "{s.server}" }
                        if let Some(Some(i)) = info.read().as_ref() {
                            dt { "Server version" } dd { "{i.name} {i.version}" }
                        }
                    }
                    div { class: "row-end",
                        button { class: "btn tonal", onclick: move |_| store::sign_out(),
                            Ico { icon: Icon::Logout }
                            "Sign out"
                        }
                    }
                }
            }
        }
    }
}
