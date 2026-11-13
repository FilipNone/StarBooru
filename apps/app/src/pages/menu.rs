//! The menu a held or right-clicked thumbnail or tag opens (`gesture.rs`): what
//! can be done with it, growing out of the point it was opened at.

use chrono::{TimeDelta, Utc};
use dioxus::prelude::*;

use crate::gesture::MenuAt;
use crate::icons::{Ico, Icon};

/// How long after opening a right click on the backdrop still belongs to the
/// press that opened the menu.
const MENU_SETTLE_MS: i64 = 800;

/// Moves the focus along the menu's items, round from either end. From
/// outside the menu, down goes to the first item and up to the last.
const STEP_FOCUS: &str = r#"
const by = await dioxus.recv();
const items = [...document.querySelectorAll(".press-menu .menu-item:not(:disabled)")];
if (items.length) {
  const at = items.indexOf(document.activeElement);
  const n = items.length;
  const next = by === 0 ? 0 : at < 0 ? (by > 0 ? 0 : n - 1) : (at + by + n) % n;
  items[next].focus({ preventScroll: true });
}
"#;

/// The menu's keys, wherever the focus is: a finger lifted after a hold
/// leaves it on the page, not in the menu. Taken before the page's own keys,
/// so Escape closes the menu and not the post behind it.
const MENU_KEYS: &str = r#"
if (window.__sbMenuKeys) removeEventListener("keydown", window.__sbMenuKeys, true);
window.__sbMenuKeys = (e) => {
  if (!document.querySelector(".press-menu")) return;
  if (!["Escape", "Tab", "ArrowDown", "ArrowUp"].includes(e.key)) return;
  e.preventDefault();
  e.stopPropagation();
  dioxus.send(e.key);
};
addEventListener("keydown", window.__sbMenuKeys, true);
"#;

const MENU_KEYS_OFF: &str = r#"
if (window.__sbMenuKeys) removeEventListener("keydown", window.__sbMenuKeys, true);
window.__sbMenuKeys = null;
"#;

fn step_focus(by: i32) {
    let eval = document::eval(STEP_FOCUS);
    let _ = eval.send(by);
}

/// A menu of [`MenuItem`]s at `at`, over a backdrop that closes it.
#[component]
pub fn ActionMenu(
    at: MenuAt,
    title: String,
    onclose: EventHandler<()>,
    children: Element,
) -> Element {
    let opened = use_hook(Utc::now);
    use_future(move || async move {
        let mut keys = document::eval(MENU_KEYS);
        while let Ok(key) = keys.recv::<String>().await {
            match key.as_str() {
                "ArrowDown" => step_focus(1),
                "ArrowUp" => step_focus(-1),
                _ => onclose.call(()),
            }
        }
    });
    use_drop(|| {
        document::eval(MENU_KEYS_OFF);
    });
    rsx! {
        div {
            class: "press-backdrop",
            onclick: move |_| onclose.call(()),
            oncontextmenu: move |e| {
                e.prevent_default();
                // Android sends one for the long press that just opened it.
                if Utc::now() - opened > TimeDelta::milliseconds(MENU_SETTLE_MS) {
                    onclose.call(());
                }
            },
            onwheel: move |_| onclose.call(()),
        }
        div {
            class: "press-menu",
            role: "menu",
            aria_label: "{title}",
            style: at.style(),
            onmounted: move |_| step_focus(0),
            div { class: "press-menu-title", aria_hidden: "true", "{title}" }
            {children}
        }
    }
}

#[component]
pub fn MenuItem(
    icon: Icon,
    label: String,
    #[props(default)] danger: bool,
    onclick: EventHandler<()>,
) -> Element {
    rsx! {
        button {
            r#type: "button",
            role: "menuitem",
            class: if danger { "menu-item danger" } else { "menu-item" },
            onclick: move |_| onclick.call(()),
            Ico { icon, size: 20 }
            span { "{label}" }
        }
    }
}
