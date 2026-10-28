//! A themed drop-down. The native `<select>` opens a system popup that CSS can't
//! reach, so this draws its own list (ARIA "select-only combobox" pattern).
//!
//! The list is `position: fixed` and placed by script next to the button, so a
//! scrolling sheet can't clip it. It opens on the side with more room, clear of
//! the top and bottom bars, and closes when the page scrolls or resizes.

use std::sync::atomic::{AtomicUsize, Ordering};

use dioxus::core::Task;
use dioxus::prelude::*;

#[derive(Clone, Debug, PartialEq)]
pub struct Choice {
    pub value: String,
    pub label: String,
}

impl Choice {
    pub fn new(value: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            value: value.into(),
            label: label.into(),
        }
    }
}

/// `class` goes on the wrapper; `boxed` gives the full-width form-field look.
#[component]
pub fn Select(
    value: String,
    options: Vec<Choice>,
    onchange: EventHandler<String>,
    #[props(into, default)] class: String,
    #[props(into, default)] aria_label: String,
) -> Element {
    let id = use_hook(|| {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        NEXT.fetch_add(1, Ordering::Relaxed)
    });
    let mut open = use_signal(|| false);
    let mut watcher = use_signal(|| None::<Task>);
    let mut active = use_signal(|| 0usize);
    let count = options.len();
    let current = options.iter().position(|c| c.value == value);
    let label = current
        .and_then(|i| options.get(i))
        .map(|c| c.label.clone())
        .unwrap_or_default();
    let list_id = format!("select-{id}");
    let option_id = move |i: usize| format!("select-{id}-{i}");

    let mut show = move |i: usize| {
        active.set(i);
        open.set(true);
        // keep the highlighted option in view when the list scrolls
        let _ = document::eval(&format!(
            "document.getElementById('select-{id}-{i}')?.scrollIntoView({{block:'nearest'}})"
        ));
    };
    let values: Vec<String> = options.iter().map(|c| c.value.clone()).collect();
    let mut pick = move |i: usize| {
        open.set(false);
        if let Some(v) = values.get(i).filter(|_| Some(i) != current) {
            onchange.call(v.clone());
        }
    };
    let rows: Vec<(usize, String, String)> = options
        .iter()
        .enumerate()
        .map(|(i, c)| (i, c.value.clone(), c.label.clone()))
        .collect();

    rsx! {
        div { class: "select {class}",
            button {
                r#type: "button",
                id: "{list_id}-button",
                class: "select-button",
                role: "combobox",
                aria_label: if aria_label.is_empty() { None } else { Some(aria_label.clone()) },
                aria_haspopup: "listbox",
                aria_controls: "{list_id}",
                aria_expanded: if open() { "true" } else { "false" },
                aria_activedescendant: if open() { option_id(active()) } else { String::new() },
                onclick: move |_| {
                    if open() {
                        open.set(false);
                    } else {
                        show(current.unwrap_or(0));
                    }
                },
                onblur: move |_| open.set(false),
                onkeydown: move |e: KeyboardEvent| {
                    let key = e.key();
                    if key == Key::Tab {
                        open.set(false);
                        return;
                    }
                    let action = key_action(&key, open(), active(), current, count);
                    if action != Action::Nothing {
                        // the open list owns these keys: Escape mustn't also close a sheet
                        e.prevent_default();
                        e.stop_propagation();
                    }
                    match action {
                        Action::Show(i) => show(i),
                        Action::Pick(i) => pick(i),
                        Action::Close => open.set(false),
                        Action::Nothing => {}
                    }
                },
                // every label stacked in one grid cell, so the button is as wide as the longest
                span { class: "select-sizer", aria_hidden: "true",
                    for (i, c) in options.iter().enumerate() {
                        span { key: "{i}", "{c.label}" }
                    }
                }
                span { class: "select-value", "{label}" }
            }
            if open() {
                div { class: "select-backdrop", onclick: move |_| open.set(false) }
                ul {
                    class: "select-list",
                    id: "{list_id}",
                    role: "listbox",
                    aria_label: if aria_label.is_empty() { None } else { Some(aria_label.clone()) },
                    // mousedown would blur the button and close the list before the click lands
                    onmousedown: move |e| e.prevent_default(),
                    onmounted: move |_| {
                        if let Some(task) = watcher.take() {
                            task.cancel();
                        }
                        watcher.set(Some(spawn(async move {
                            let mut placed = document::eval(&place_script(id));
                            if placed.recv::<bool>().await.is_ok() {
                                open.set(false);
                            }
                        })));
                    },
                    for (i, v, l) in rows {
                        li {
                            key: "{i}",
                            id: option_id(i),
                            role: "option",
                            class: if active() == i { "active" } else { "" },
                            aria_selected: if current == Some(i) { "true" } else { "false" },
                            onmouseenter: move |_| active.set(i),
                            onclick: move |_| {
                                open.set(false);
                                if Some(i) != current {
                                    onchange.call(v.clone());
                                }
                            },
                            "{l}"
                        }
                    }
                }
            }
        }
    }
}

/// Places list `id` by its button and sends once when the page scrolls or resizes.
fn place_script(id: usize) -> String {
    PLACE.replace("ID", &format!("select-{id}"))
}

const PLACE: &str = r#"
const b = document.getElementById("ID-button"), l = document.getElementById("ID");
if (!b || !l) return;
const gap = 6, edge = 8, r = b.getBoundingClientRect();
let floor = innerHeight - edge, ceil = edge;
const nav = document.querySelector(".nav-bottom"), bar = document.querySelector(".topbar");
if (nav) { const n = nav.getBoundingClientRect(); if (n.height && n.top >= r.bottom) floor = Math.min(floor, n.top - edge); }
if (bar) { const t = bar.getBoundingClientRect(); if (t.height && t.bottom <= r.top) ceil = Math.max(ceil, t.bottom + edge); }
l.style.minWidth = r.width + "px";
l.style.maxHeight = "none";
l.style.top = l.style.left = "0px";
// (0, 0) may not be the viewport's corner: a blurred bar is the containing block
const o = l.getBoundingClientRect();
const below = floor - r.bottom - gap, above = r.top - gap - ceil;
const up = l.scrollHeight > below && above > below;
const h = Math.max(0, Math.min(l.scrollHeight, up ? above : below, 440));
let left = r.left;
if (left + o.width > innerWidth - edge) left = Math.max(edge, r.right - o.width);
l.style.maxHeight = h + "px";
l.style.top = (up ? r.top - gap - h : r.bottom + gap) - o.top + "px";
l.style.left = left - o.left + "px";
l.dataset.placed = up ? "up" : "down";
l.querySelector('[aria-selected="true"]')?.scrollIntoView({ block: "nearest" });
const close = (e) => {
  if (e && e.target === l) return;
  removeEventListener("scroll", close, true);
  removeEventListener("resize", close);
  if (l.isConnected) dioxus.send(true);
};
addEventListener("scroll", close, true);
addEventListener("resize", close);
"#;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Action {
    Show(usize),
    Pick(usize),
    Close,
    Nothing,
}

/// What a key press on the button does, given whether the list is `open`, the
/// highlighted option `at`, the chosen one and how many there are.
fn key_action(key: &Key, open: bool, at: usize, current: Option<usize>, count: usize) -> Action {
    let Some(last) = count.checked_sub(1) else {
        return Action::Nothing;
    };
    let at = at.min(last);
    match key {
        Key::ArrowDown | Key::ArrowUp if !open => Action::Show(current.unwrap_or(0).min(last)),
        Key::ArrowDown => Action::Show(at.saturating_add(1).min(last)),
        Key::ArrowUp => Action::Show(at.saturating_sub(1)),
        Key::Home if open => Action::Show(0),
        Key::End if open => Action::Show(last),
        Key::Enter if open => Action::Pick(at),
        Key::Character(c) if c == " " && open => Action::Pick(at),
        Key::Escape if open => Action::Close,
        _ => Action::Nothing,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arrows_open_on_the_chosen_option() {
        assert_eq!(
            key_action(&Key::ArrowDown, false, 0, Some(3), 5),
            Action::Show(3)
        );
        assert_eq!(
            key_action(&Key::ArrowUp, false, 0, None, 5),
            Action::Show(0)
        );
    }

    #[test]
    fn arrows_stop_at_the_ends() {
        assert_eq!(
            key_action(&Key::ArrowDown, true, 4, None, 5),
            Action::Show(4)
        );
        assert_eq!(key_action(&Key::ArrowUp, true, 0, None, 5), Action::Show(0));
        assert_eq!(
            key_action(&Key::ArrowDown, true, 1, None, 5),
            Action::Show(2)
        );
        assert_eq!(key_action(&Key::End, true, 1, None, 5), Action::Show(4));
        assert_eq!(key_action(&Key::Home, true, 3, None, 5), Action::Show(0));
    }

    #[test]
    fn enter_space_and_escape_only_act_when_open() {
        assert_eq!(key_action(&Key::Enter, true, 2, None, 5), Action::Pick(2));
        assert_eq!(
            key_action(&Key::Character(" ".into()), true, 2, None, 5),
            Action::Pick(2)
        );
        assert_eq!(key_action(&Key::Escape, true, 2, None, 5), Action::Close);
        // closed: let the button's own click (Enter/Space) and the page (Escape) have them
        assert_eq!(key_action(&Key::Enter, false, 2, None, 5), Action::Nothing);
        assert_eq!(key_action(&Key::Escape, false, 2, None, 5), Action::Nothing);
    }

    #[test]
    fn empty_or_stale_lists_never_point_past_the_end() {
        assert_eq!(
            key_action(&Key::ArrowDown, true, 0, None, 0),
            Action::Nothing
        );
        assert_eq!(key_action(&Key::Enter, true, 9, None, 3), Action::Pick(2));
        assert_eq!(
            key_action(&Key::ArrowDown, false, 0, Some(7), 3),
            Action::Show(2)
        );
    }
}
